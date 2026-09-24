// Shared loop for the reference agents: take a seat (x402), then every tick
// read the fogged view, decide, and submit a signed batch with a committed
// rationale. Keys, the seat and the commitments not yet revealed are kept in
// --dir, so an agent can be restarted mid-season and still reveal.
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { GameClient, loadOrCreateKeypair } from '../client/src/index.mjs';
import { officeOf } from '../client/src/game.mjs';

export function parseArgs(argv, defaults = {}) {
  const out = { ...defaults };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) continue;
    const k = a.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) out[k] = true; else { out[k] = next; i++; }
  }
  return out;
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const stamp = () => new Date().toISOString().slice(11, 19);

/**
 * Join a nation (x402), then every tick: read the nation's fogged view,
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
  const game = new GameClient({ server: args.server || 'http://127.0.0.1:4185', gateway: args.gateway || 'http://127.0.0.1:4191' });
  const wallet = await loadOrCreateKeypair(path.join(dir, 'wallet.json'));
  const session = await loadOrCreateKeypair(path.join(dir, 'session.json'));
  game.session = session;
  const stand = String(args.stand || 'Science,Diplomat').split(',').map(s => s.trim()).filter(Boolean);

  // ---- membership
  const info = await game.season();
  const seasonId = String(info.season.seasonId);
  const seatFile = path.join(dir, 'member.json');
  const saved = existsSync(seatFile) ? JSON.parse(readFileSync(seatFile, 'utf8')) : null;
  if (args.member !== undefined) Object.assign(game, { member: Number(args.member), civ: Number(args.civ) });
  else if (saved?.seasonId === seasonId) Object.assign(game, { member: saved.member, civ: saved.civ });
  else {
    if (info.season.status !== 'Registering') throw new Error(`season ${seasonId} is ${info.season.status}: registration is closed`);
    const f = await game.faucet(wallet.publicKey);
    log(`faucet: ${f.usdcAccount} holds 100 test USDC (localnet, no value)`);
    const civ = args.civ !== undefined ? Number(args.civ) : undefined;
    // Stand, and vote for ourselves in the first election for the offices we stand for.
    const me = info.season.memberCount;
    const votes = ['General', 'Steward', 'Science', 'Diplomat'].map(r => (stand.includes(r) ? me : undefined));
    const j = await game.joinViaX402({ wallet, session, civ, name, kind: 1, usdcAccount: f.usdcAccount, stand, votes });
    log(`paid ${Number(j.requirements.maxAmountRequired) / 1e6} USDC over x402 → member ${j.member} of ${j.nation} (${j.signature.slice(0, 16)}…)`);
    log(`X-PAYMENT-RESPONSE ${JSON.stringify(j.paymentResponse)}`);
    writeFileSync(seatFile, JSON.stringify({ seasonId, member: j.member, civ: j.civ, signature: j.signature, usdcAccount: f.usdcAccount }, null, 2));
  }
  const decisionsFile = path.join(dir, `decisions-${seasonId}.json`);
  if (existsSync(decisionsFile)) for (const d of JSON.parse(readFileSync(decisionsFile, 'utf8'))) game.decisions.set(`${d.tick}:${d.role}`, d);
  log(`member ${game.member} of nation ${game.civ}, season ${seasonId} (session key ${session.publicKey.toBase58().slice(0, 8)}…)`);

  // ---- ticks
  const map = await retry(() => game.map(), log, 'waiting for the game server (the season starts once registration closes)');
  let last = null, played = 0, over = false;
  const proposedAt = new Map();
  for (;;) {
    const v = await retry(() => game.state(), log, 'game server unreachable');
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
    // support, votes): once every office's batch is in, the tick's input is
    // frozen and later actions wait for the next tick.
    const notHeld = (decision.orders || []).filter(o => { const role = officeOf(o, v); return role && !held.includes(role); });
    await govern({ game, view: v, proposals: notHeld, stand, proposedAt, log });
    if (held.length) {
      const r = await game.submit({ orders: decision.orders, policy, rationale: decision.rationale, adopt: decision.adopt || {}, view: v }).catch(e => ({ ok: false, error: e.message }));
      if (r.ok) {
        log(`tick ${r.tick}: as ${held.join('+')}: ${r.offices.map(o => `${o.role} ${o.orders}${o.revealed.length ? ` (revealed ${o.revealed.join(',')})` : ''}`).join(', ') || 'nothing to send'}${r.warnings.length ? `, ${r.warnings.length} warnings` : ''} — ${decision.rationale}`);
        writeFileSync(decisionsFile, JSON.stringify([...game.decisions.values()], null, 1));
      } else log(`tick ${v.tick}: not submitted: ${r.error}`);
    }
    last = v.tick;
    if (args.ticks && ++played >= Number(args.ticks)) break;
    await game.waitForTick(v.tick).catch(e => log(e.message));
  }
  if (over) await claimPrize({ game, wallet, seatFile, log });
}

/** Once the season is finalized on base, claim the prize and treasury share into our own USDC account. */
async function claimPrize({ game, wallet, seatFile, log }) {
  const seat = existsSync(seatFile) ? JSON.parse(readFileSync(seatFile, 'utf8')) : {};
  const usdcAccount = seat.usdcAccount ?? (await game.faucet(wallet.publicKey)).usdcAccount; // localnet: the faucet returns our account
  for (let i = 0; i < 120; i++) {
    const r = await game.claim({ wallet, usdcAccount }).catch(e => ({ ok: false, error: e.message }));
    if (r.ok) { log(`claimed the prize into ${usdcAccount} (${r.signature.slice(0, 16)}…)`); return; }
    if (/NothingToClaim|AlreadyClaimed/.test(r.error)) { log(`nothing to claim (${r.error.match(/NothingToClaim|AlreadyClaimed/)[0]})`); return; }
    if (i === 0 || !/Finalized/.test(r.error)) log(`claim: ${r.error}`);
    await sleep(5000);
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

async function retry(fn, log, what) {
  for (let i = 0; ; i++) {
    try { return await fn(); } catch (e) {
      if (i % 10 === 0) log(`${what} (${e.message})`);
      await sleep(2000);
    }
  }
}
