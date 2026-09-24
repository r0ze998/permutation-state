// PERMUTATION STATE gateway: HTTP front for the chain.
//
//   node src/server.mjs --new-season      bootstrap a season on the local stack, then serve
//        [--open-seats N]                 leave the last N seats for outside agents (x402)
//        [--port P --state file.json]     run several seasons side by side
//   node src/server.mjs                   resume the season in .local/season.json
//
// Endpoints (JSON unless noted):
//   GET  /health
//   GET  /season                 season account + gateway info (hosted civs, keys)
//   GET  /world.bin              raw world account (octet-stream) from the layer it lives on
//   GET  /ticks?from=N           archived PS_TICK records (replay verifier input)
//   POST /submit                 {civ, tick, digest, orders}: sign with a hosted civ's session key
//   GET  /relay                  {feePayer, blockhash}: for agents that sign their own orders
//   POST /relay                  {tx}: an agent-signed SubmitOrders; the gateway only adds the fee payer
//   (x402 entry routes are mounted from x402.mjs)
import http from 'node:http';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeSeason } from '../client/src/codec.mjs';
import { loadConfig, namedKey, readState } from './config.mjs';
import { Crank } from './crank.mjs';
import { bootstrap, CIVS } from './season.mjs';
import { send, sendSigned } from './send.mjs';
import { mountX402 } from './x402.mjs';

const cfg = loadConfig();
const base = new Connection(cfg.baseRpc, 'confirmed');
const er = new Connection(cfg.erRpc, 'confirmed');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

let state = readState(cfg.stateFile);
if (process.argv.includes('--new-season') || !state) {
  // --open-seats N (or --open-seat for one): the last N civs are left for
  // outside agents paying their entry via x402.
  const i = process.argv.indexOf('--open-seats');
  const openSeats = i > 0 ? Number(process.argv[i + 1]) : process.argv.includes('--open-seat') ? 1 : 0;
  const open = openSeats > 0;
  const civs = CIVS.map((c, k) => (k >= CIVS.length - openSeats ? { ...c, hosted: 'open' } : c));
  state = await bootstrap({ base, er, cfg, log, civs, joinOpen: open });
}
// With an open seat, the crank starts the season once an outside entrant pays via x402.

const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = new Crank({ base, er, cfg, state, log });
setInterval(() => crank.step(), 400);

const json = (res, code, body) => { res.writeHead(code, { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' }); res.end(JSON.stringify(body, (_, v) => (typeof v === 'bigint' ? v.toString() : v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v))); };
const readBody = req => new Promise((ok, fail) => { let d = ''; req.on('data', c => { d += c; if (d.length > 1 << 20) req.destroy(); }); req.on('end', () => { try { ok(d ? JSON.parse(d) : {}); } catch (e) { fail(e); } }); });
const fromHex = h => Uint8Array.from(Buffer.from(h || '', 'hex'));

const routes = {
  'GET /health': async () => ({ ok: true, phase: crank.phase, season: state.seasonId }),
  'GET /season': async () => {
    const acc = await base.getAccountInfo(chain.season, 'confirmed');
    const s = decodeSeason(acc.data);
    return {
      season: { ...s, admin: new PublicKey(s.admin).toBase58(), crank: new PublicKey(s.crank).toBase58(), usdcMint: new PublicKey(s.usdcMint).toBase58(),
        civs: s.civs.map((c, i) => ({ ...c, civ: i, player: new PublicKey(c.player).toBase58(), session: new PublicKey(c.session).toBase58(), payout: new PublicKey(c.payout).toBase58(), hosted: state.civs[i]?.hosted ?? 'external' })) },
      programId: cfg.programId, cluster: cfg.cluster, phase: crank.phase,
      accounts: { season: chain.season.toBase58(), world: chain.world.toBase58(), vault: chain.vault.toBase58(), orders: chain.ordersList(s.civs.length).map(k => k.toBase58()) },
      genesis: state.genesis ?? null, endpoints: { base: cfg.baseRpc, er: cfg.erRpc },
    };
  },
  'GET /ticks': async (req, url) => ({ records: crank.tickRecords(Number(url.searchParams.get('from') || 0)) }),
  'POST /submit': async req => {
    const b = await readBody(req);
    const civ = state.civs.find(c => c.civ === b.civ);
    if (!civ || civ.hosted === 'external') return [403, { error: 'civ is not hosted by this gateway; sign it yourself and use /relay' }];
    const session = namedKey(`civ${b.civ}-session`);
    const ixs = chain.submitOrders({ signer: session.publicKey, civ: b.civ, tick: b.tick, decisionDigest: fromHex(b.digest).length === 32 ? fromHex(b.digest) : new Uint8Array(32), orders: b.orders || [] });
    const r = await send(er, ixs, [crank.crank, session], `submit civ ${b.civ}`);
    return { ok: true, signature: r.signature };
  },
  'GET /relay': async () => {
    const { blockhash } = await er.getLatestBlockhash('confirmed');
    return { feePayer: crank.crank.publicKey.toBase58(), blockhash, programId: cfg.programId, endpoint: cfg.erRpc };
  },
  'POST /relay': async req => {
    const b = await readBody(req);
    const tx = Transaction.from(Buffer.from(b.tx, 'base64'));
    // Only SubmitOrders to this program, paid by the gateway, nothing else.
    const only = tx.instructions.length === 1 && tx.instructions[0].programId.toBase58() === cfg.programId && tx.instructions[0].data[0] === 6;
    if (!only || !tx.feePayer?.equals(crank.crank.publicKey)) return [400, { error: 'relay accepts exactly one SubmitOrders instruction with the gateway as fee payer' }];
    tx.partialSign(crank.crank);
    const r = await sendSigned(er, tx, 'relay submit');
    return { ok: true, signature: r.signature };
  },
};

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  if (req.method === 'OPTIONS') { res.writeHead(204, { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': 'GET,POST' }); return res.end(); }
  try {
    if (req.method === 'GET' && url.pathname === '/world.bin') {
      const snap = Date.now() - (crank.snapshot?.at ?? 0) < 300 ? crank.snapshot : await crank.refresh();
      if (!snap) return json(res, 503, { error: 'world not available' });
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'X-Slot': String(snap.slot), 'X-Layer': snap.layer, 'X-Phase': crank.phase, 'Access-Control-Allow-Origin': '*' });
      return res.end(snap.world);
    }
    const h = routes[`${req.method} ${url.pathname}`];
    if (h) {
      const out = await h(req, url);
      return Array.isArray(out) ? json(res, out[0], out[1]) : json(res, 200, out);
    }
    if (await mountX402({ req, res, url, cfg, base, chain, state, json, readBody, log })) return;
    json(res, 404, { error: 'not found' });
  } catch (e) {
    log(`${req.method} ${url.pathname}: ${e.message}`);
    json(res, 500, { error: e.message, logs: e.logs?.slice(-6) });
  }
});
server.listen(cfg.port, '127.0.0.1', () => log(`gateway on http://127.0.0.1:${cfg.port} season ${state.seasonId} (${crank.phase})`));
