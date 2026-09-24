// Shared loop for the reference agents: take a seat (x402), then every tick
// read the fogged view, decide, and submit a signed batch with a committed
// rationale. Keys, the seat and the commitments not yet revealed are kept in
// --dir, so an agent can be restarted mid-season and still reveal.
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { GameClient, loadOrCreateKeypair } from '../client/src/index.mjs';

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
 * @param {object} o
 * @param {string} o.name     civilization name to register
 * @param {string} o.policy   policy name committed with every decision
 * @param {(ctx) => Promise<{orders, rationale}>} o.decide
 * @param {object} o.args     parsed CLI args: server, gateway, dir, civ, ticks
 */
export async function runAgent({ name, policy, decide, args }) {
  const log = (...a) => console.log(stamp(), `[${name}]`, ...a);
  const dir = path.resolve(args.dir || `.local/agents/${name.toLowerCase()}`);
  mkdirSync(dir, { recursive: true });
  const game = new GameClient({ server: args.server || 'http://127.0.0.1:4185', gateway: args.gateway || 'http://127.0.0.1:4190' });
  const wallet = await loadOrCreateKeypair(path.join(dir, 'wallet.json'));
  const session = await loadOrCreateKeypair(path.join(dir, 'session.json'));
  game.session = session;

  // ---- seat
  const info = await game.season();
  const seasonId = String(info.season.seasonId);
  const seatFile = path.join(dir, 'seat.json');
  const saved = existsSync(seatFile) ? JSON.parse(readFileSync(seatFile, 'utf8')) : null;
  if (args.civ !== undefined) game.civ = Number(args.civ);
  else if (saved?.seasonId === seasonId) game.civ = saved.civ;
  else {
    if (info.season.status !== 'Registering') throw new Error(`season ${seasonId} is ${info.season.status}: entry is closed`);
    const f = await game.faucet(wallet.publicKey);
    log(`faucet: ${f.usdcAccount} holds 100 test USDC (localnet, no value)`);
    const j = await game.joinViaX402({ wallet, session, name, kind: 1, usdcAccount: f.usdcAccount });
    log(`paid ${Number(j.requirements.maxAmountRequired) / 1e6} USDC over x402 → civ ${j.civ} (${j.signature.slice(0, 16)}…)`);
    log(`X-PAYMENT-RESPONSE ${JSON.stringify(j.paymentResponse)}`);
    writeFileSync(seatFile, JSON.stringify({ seasonId, civ: j.civ, signature: j.signature }, null, 2));
  }
  const decisionsFile = path.join(dir, `decisions-${seasonId}.json`);
  if (existsSync(decisionsFile)) for (const d of JSON.parse(readFileSync(decisionsFile, 'utf8'))) game.decisions.set(d.tick, d);
  log(`playing civ ${game.civ} of season ${seasonId} (session key ${session.publicKey.toBase58().slice(0, 8)}…)`);

  // ---- ticks
  const map = await retry(() => game.map(), log, 'waiting for the game server (the season starts once every seat is taken)');
  let last = null, played = 0;
  for (;;) {
    const v = await retry(() => game.state(), log, 'game server unreachable');
    if (v.over || v.chain?.finished) { log('season over'); break; }
    if (v.me !== game.civ || !v.decision?.obsRoot) { await sleep(1000); continue; }
    if (last === v.tick) { await game.waitForTick(v.tick).catch(() => {}); continue; }
    let decision;
    try {
      decision = await decide({ game, view: v, map, log });
    } catch (e) {
      log(`decide failed (${e.message}); submitting an empty batch to keep the commitment chain`);
      decision = { orders: [], rationale: `判断に失敗: ${e.message}`.slice(0, 200) };
    }
    const r = await game.submit({ orders: decision.orders, policy, rationale: decision.rationale }).catch(e => ({ ok: false, error: e.message }));
    if (r.ok) {
      log(`tick ${r.tick}: ${decision.orders.length} orders (cost ${r.cost})${r.revealed.length ? `, revealed ${r.revealed.join(',')}` : ''}${r.warnings.length ? `, ${r.warnings.length} warnings` : ''} — ${decision.rationale}`);
      writeFileSync(decisionsFile, JSON.stringify([...game.decisions.values()], null, 1));
    } else {
      log(`tick ${v.tick}: not submitted: ${r.error}`);
    }
    last = v.tick;
    if (args.ticks && ++played >= Number(args.ticks)) break;
    await game.waitForTick(v.tick).catch(e => log(e.message));
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
