// Shared loop for the reference agents: take a seat (x402), then every tick
// read the view (the whole world), decide, and submit a signed batch with a committed
// rationale. Keys, the seat and the commitments not yet revealed are kept in
// --dir, so an agent can be restarted mid-season and still reveal.
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { GameClient, loadOrCreateKeypair } from '../client/src/index.mjs';
import { ROLES } from '../client/src/codec.mjs';
import { errorCode } from '../client/src/http.mjs';
import { officeOf, splitByOffice } from '../client/src/offices.mjs';
import { retry, sleep } from '../client/src/retry.mjs';
import { randomOffices } from '../client/src/x402-client.mjs';
import { DEFAULTS, parseArgs } from '../src/config.mjs';

export { parseArgs };

const stamp = () => new Date().toISOString().slice(11, 19);
/** Program errors that mean "too late for this tick": no point retrying within it. */
const LATE = new Set(['TickFrozen', 'WrongTick']);

/**
 * Join a nation (x402), then every tick: read the nation's view,
 * decide, submit the orders of the offices held (one sealed batch per
 * office), propose the rest to their offices, and take part in governance:
 * vote in the vote window, support workable proposals, stand for office.
 * @param {object} o
 * @param {string} o.name     member name to register
 * @param {string} o.policy   policy name committed with every decision
 * @param {(ctx) => Promise<{orders, rationale}>} o.decide
 * @param {object} o.args     parsed CLI args: server, gateway, dir, civ, stand, ticks
 */
export async function runAgent({ name, policy, decide, args }) {
  const log = (...a) => console.log(stamp(), `[${name}]`, ...a);
  const dir = path.resolve(args.dir || `.local/agents/${name.toLowerCase()}`);
  mkdirSync(dir, { recursive: true });
  const game = new GameClient({ server: args.server || DEFAULTS.serverUrl, gateway: args.gateway || DEFAULTS.gatewayUrl });
  const wallet = await loadOrCreateKeypair(path.join(dir, 'wallet.json'));
  const session = await loadOrCreateKeypair(path.join(dir, 'session.json'));
  game.session = session;
  // 1–2 offices, as everyone stands (the gateway refuses other candidacies
  // in a season with AI members); default: drawn at random, like theirs.
  const stand = args.stand ? String(args.stand).split(',').map(s => s.trim()).filter(Boolean) : randomOffices();
  const unknown = stand.filter(r => !ROLES.includes(r));
  if (unknown.length) throw new Error(`--stand: unknown office ${unknown.join(', ')} (offices: ${ROLES.join(', ')})`);

  // ---- membership
  const info = await game.season();
  const seasonId = String(info.season.seasonId);
  const seatFile = path.join(dir, 'member.json');
  const saved = existsSync(seatFile) ? JSON.parse(readFileSync(seatFile, 'utf8')) : null;
  // A registration that settled although we never heard back (e.g. the
  // gateway lost the confirmation) is recovered from the chain by wallet.
  const onChain = (info.members || []).find(m => m.wallet === wallet.publicKey.toBase58());
  if (args.member !== undefined) Object.assign(game, { member: Number(args.member), civ: Number(args.civ) });
  else if (saved?.seasonId === seasonId) Object.assign(game, { member: saved.member, civ: saved.civ });
  else if (onChain) {
    Object.assign(game, { member: onChain.index, civ: onChain.civ });
    writeFileSync(seatFile, JSON.stringify({ seasonId, member: onChain.index, civ: onChain.civ, recovered: true }, null, 2));
    log(`already a member on chain (member ${onChain.index}); continuing`);
  } else {
    if (info.season.status !== 'Registering') throw new Error(`season ${seasonId} is ${info.season.status}: registration is closed`);
    const civ = args.civ !== undefined ? Number(args.civ) : undefined;
    const j = await joinSeason({ game, wallet, session, civ, name, stand, log });
    writeFileSync(seatFile, JSON.stringify({ seasonId, member: j.member, civ: j.civ, signature: j.signature, usdcAccount: j.usdcAccount }, null, 2));
  }
  const decisionsFile = path.join(dir, `decisions-${seasonId}.json`);
  if (existsSync(decisionsFile)) for (const d of JSON.parse(readFileSync(decisionsFile, 'utf8'))) game.decisions.set(`${d.tick}:${d.role}`, d);
  // Sealed orders not yet revealed survive a restart too.
  const sealedFile = path.join(dir, `sealed-${seasonId}.json`);
  if (existsSync(sealedFile)) game.importSealed(JSON.parse(readFileSync(sealedFile, 'utf8')));
  const saveSealed = () => writeFileSync(sealedFile, JSON.stringify(game.exportSealed()));
  log(`member ${game.member} of nation ${game.civ}, season ${seasonId} (session key ${session.publicKey.toBase58().slice(0, 8)}…)`);

  // ---- ticks
  const map = await untilReachable(() => game.map(), log, 'waiting for the game server (the season starts once registration closes)');
  let last = null, played = 0, over = false;
  const proposedAt = new Map();
  for (;;) {
    const v = await untilReachable(() => game.state(), log, 'game server unreachable');
    if (v.over || v.chain?.finished) { log('season over'); over = true; break; }
    if (v.me !== game.civ || !v.decision?.obsRoot || v.phase !== 'playing') { await sleep(1000); continue; }
    if (last === v.tick) { await game.waitForTick(v.tick).catch(() => {}); continue; }
    const held = game.myOffices(v);
    let decision;
    try {
      decision = await decide({ game, view: v, map, log, held });
    } catch (e) {
      log(`decide failed (${e.message}); keeping the commitment chain with an empty batch`);
      decision = { orders: [], rationale: `判断に失敗: ${e.message}`.slice(0, 200) };
    }
    // Governance first (proposals of the orders for offices we do not hold,
    // support, votes): it closes with the commitments at the tick's deadline,
    // and later actions wait for the next tick.
    const notHeld = splitByOffice(decision.orders || [], held, v).notHeld.filter(o => officeOf(o, v));
    await govern({ game, view: v, proposals: notHeld, stand, proposedAt, log });
    if (held.length) {
      // Retry a failed submission a few times within the tick (e.g. the ER
      // is still taking the accounts over), but not one that came too late
      // or cannot fit.
      const attempt = async () => {
        const r = await game.submit({ orders: decision.orders, policy, rationale: decision.rationale, adopt: decision.adopt || {}, view: v })
          .catch(e => ({ ok: false, error: e.message, code: errorCode(e), status: e.status }));
        if (!r.ok) throw Object.assign(new Error(r.error), { result: r });
        return r;
      };
      const r = await retry(attempt, { attempts: 4, delayMs: 2000, retryIf: e => !LATE.has(e.result.code) && e.result.status !== 409 && e.result.code !== 'BatchTooLarge' })
        .catch(e => e.result);
      if (r.ok) log(`tick ${r.tick}: as ${held.join('+')}: ${r.offices.map(o => `${o.role} ${o.orders}${o.revealed.length ? ` (revealed ${o.revealed.join(',')})` : ''}`).join(', ') || 'nothing to send'}${r.warnings.length ? `, ${r.warnings.length} warnings` : ''} — ${decision.rationale}`);
      else log(`tick ${v.tick}: not submitted: ${r.error}`);
      // Orders are sealed: reveal them once the tick's commitments close
      // (orders left unrevealed when the reveal window ends do not run).
      saveSealed();
      if (r.ok && r.offices.some(o => o.signature)) {
        const revealed = await game.revealWhenOpen({ tick: r.tick }).catch(e => [{ error: e.message }]);
        saveSealed();
        const bad = revealed.filter(x => x.error);
        if (bad.length) log(`tick ${r.tick}: reveal failed: ${bad.map(x => `${x.role ?? ''} ${x.code ?? x.error}`).join(', ')}`);
        else log(`tick ${r.tick}: revealed ${revealed.map(x => x.role).join('+') || 'nothing (window missed)'}`);
      }
      // Offices that did go through (even if another failed) must reveal later, also after a restart.
      writeFileSync(decisionsFile, JSON.stringify([...game.decisions.values()], null, 1));
    }
    last = v.tick;
    if (args.ticks && ++played >= Number(args.ticks)) break;
    await game.waitForTick(v.tick).catch(e => log(e.message));
  }
  if (over) await claimPrize({ game, wallet, seatFile, log });
}

/**
 * Register over x402 like everyone else: test USDC from the faucet, then the
 * x402 client's defaults (kind 2 in a season with AI members, the public
 * deposit, no pre-season votes: our member index is only known once the
 * registration lands, so votes are cast in the vote windows). Returns the
 * join result with `usdcAccount`.
 */
export async function joinSeason({ game, wallet, session, civ, name, stand, log = () => {} }) {
  const f = await game.faucet(wallet.publicKey);
  log(`faucet: ${f.usdcAccount} ${f.amount === '0' ? 'already funded' : `received ${Number(f.amount) / 1e6} test USDC`} (test token, no value)`);
  const j = await game.joinViaX402({ wallet, session, civ, name, usdcAccount: f.usdcAccount, stand });
  log(`paid ${Number(j.requirements.maxAmountRequired) / 1e6} USDC over x402 → member ${j.member} of ${j.nation} (${j.signature.slice(0, 16)}…)`);
  log(`X-PAYMENT-RESPONSE ${JSON.stringify(j.paymentResponse)}`);
  return { ...j, usdcAccount: f.usdcAccount };
}

/**
 * Once the season is finalized on base, claim the prize and treasury share
 * into our own USDC account: the one we registered from, else what
 * GameClient.claim picks (never the faucet, which closes with registration).
 */
export async function claimPrize({ game, wallet, seatFile, log, retryOpts = { attempts: 120, delayMs: 5000 } }) {
  const seat = existsSync(seatFile) ? JSON.parse(readFileSync(seatFile, 'utf8')) : {};
  try {
    // Every 5 s for up to 10 minutes while the season is not finalized yet.
    const r = await retry(() => game.claim({ wallet, usdcAccount: seat.usdcAccount }), {
      ...retryOpts,
      retryIf: (e, i) => {
        if (i === 0 || errorCode(e) !== 'NotFinalized') log(`claim: ${e.message}`);
        return !['NothingToClaim', 'AlreadyClaimed'].includes(errorCode(e));
      },
    });
    log(`claimed the prize (${r.signature.slice(0, 16)}…)`);
    return r;
  } catch (e) {
    const code = errorCode(e);
    if (code === 'NothingToClaim' || code === 'AlreadyClaimed') log(`nothing to claim (${code})`);
    else log('gave up waiting for the season to be finalized; run the agent again later to claim');
  }
}

/**
 * Governance as a member: propose orders for offices we do not hold (each
 * office at most every 5 ticks), support nation-mates' proposals that would
 * still work, vote for ourselves (or the incumbent) in the vote window.
 */
async function govern({ game, view: v, proposals, stand, proposedAt, log }) {
  const gov = v.gov || {};
  const byRole = {};
  for (const o of proposals) (byRole[officeOf(o, v)] ||= []).push(o);
  for (const [role, orders] of Object.entries(byRole)) {
    if ((proposedAt.get(role) ?? -99) > v.tick - 5) continue;
    const r = await game.propose(role, orders.slice(0, 2)).catch(e => ({ ok: false, error: e.message }));
    if (r.ok) { proposedAt.set(role, v.tick); log(`proposed to the ${role}: ${orders.slice(0, 2).map(o => o.type).join(', ')}`); }
  }
  for (const p of (gov.proposals || []).filter(p => p.proposer?.id !== game.member && !(p.supportedBy || []).includes(game.member)).slice(0, 2)) {
    const check = await game.validate(p.orders).catch(() => null);
    if (check && !(check.warnings || []).length) {
      const r = await game.support(p.id).catch(e => ({ ok: false, error: e.message }));
      if (r.ok) log(`supported proposal ${p.id} (${p.role}) by ${p.proposer?.name}`);
    }
  }
  if (gov.voteOpen) {
    for (const c of gov.candidates || []) {
      const holder = gov.offices?.find(o => o.role === c.role);
      const pick = stand.includes(c.role) && c.candidates.some(x => x.member?.id === game.member) ? game.member
        : holder?.active && c.candidates.some(x => x.member?.id === holder.holder?.id) ? holder.holder.id
        : c.candidates.sort((a, b) => b.votes - a.votes)[0]?.member?.id;
      if (pick === undefined) continue;
      const r = await game.vote(c.role, pick).catch(e => ({ ok: false, error: e.message }));
      if (r.ok) log(`voted ${c.role}: member ${pick}`);
    }
  }
}

/** Retry `fn` every 2 s until it answers, logging every 10th failure. */
function untilReachable(fn, log, what) {
  return retry(fn, { attempts: Infinity, delayMs: 2000, onRetry: (e, i) => { if (i % 10 === 0) log(`${what} (${e.message})`); } });
}
