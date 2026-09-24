// PERMUTATION STATE gateway (Game Design V5): HTTP front for the chain.
//
//   node src/server.mjs --new-season      bootstrap a season on the local stack, then serve
//        [--humans 1 --ai 2]              hosted members: claimable humans, AI members per nation
//        [--wait-external N]              keep registration open until N outside members joined (x402)
//        [--registration-seconds S]       …or at most S seconds
//        [--market off]                   a season without the USDC market (V5 §7.5)
//        [--port P --state file.json]     run several seasons side by side
//   node src/server.mjs                   resume the season in .local/season.json
//
// Endpoints (JSON unless noted):
//   GET  /health
//   GET  /season                 season account, members (hosted or not), records for the verifier
//   GET  /world.bin              raw world account (octet-stream) from the layer it lives on
//   GET  /ticks?from=N           archived tick records: PS_TICK roots with the input its PS_INPUT
//                                chunks published (an index for the replay verifier, which re-reads
//                                both from the ER's logs)
//   POST /submit                 {civ, role, member|null, tick, digest, orders, adopt}: an office's
//                                batch, signed with a hosted member's session key (or the crank for
//                                a vacant office, the acting official)
//   POST /gov                    {member, action}: a hosted member's governance action
//   GET  /relay                  {feePayer, blockhash}: for members that sign their own transactions
//   POST /relay                  {tx}: a member-signed SubmitOrders or SubmitGov; the gateway only adds the fee payer
//   GET  /claim-relay            {feePayer, blockhash, mint, status}: for claims by members without SOL
//   POST /claim-relay            {tx}: a wallet-signed Claim on the base layer; the gateway only adds the fee payer
//   (x402 registration and the faucet are mounted from x402.mjs)
import http from 'node:http';
import { Connection, PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { chainError, decodeSeason, NOBODY } from '../client/src/codec.mjs';
import { loadConfig, namedKey, readState } from './config.mjs';
import { Crank } from './crank.mjs';
import { bootstrap, defaultRoster, NATIONS, seasonMembers } from './season.mjs';
import { send, sendSigned } from './send.mjs';
import { mountX402 } from './x402.mjs';

const cfg = loadConfig();
const base = new Connection(cfg.baseRpc, 'confirmed');
const er = new Connection(cfg.erRpc, 'confirmed');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

let state = readState(cfg.stateFile);
if (process.argv.includes('--new-season') || !state) {
  state = await bootstrap({ base, cfg, log, roster: defaultRoster({ humans: cfg.humans, ai: cfg.ai }) });
}

const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = new Crank({ base, er, cfg, state, log });
setInterval(() => crank.step(), 400);

const b58 = k => new PublicKey(k).toBase58();
const json = (res, code, body) => { res.writeHead(code, { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' }); res.end(JSON.stringify(body, (_, v) => (typeof v === 'bigint' ? v.toString() : v instanceof Uint8Array ? Buffer.from(v).toString('hex') : v))); };
const readBody = req => new Promise((ok, fail) => { let d = ''; req.on('data', c => { d += c; if (d.length > 1 << 20) req.destroy(); }); req.on('end', () => { try { ok(d ? JSON.parse(d) : {}); } catch (e) { fail(e); } }); });
const fromHex = h => Uint8Array.from(Buffer.from(h || '', 'hex'));

// Member registry: every Member PDA on chain, with who hosts it. Cached briefly.
let registry = { at: 0, members: [] };
async function members() {
  if (Date.now() - registry.at < 2000) return registry.members;
  const onChain = await seasonMembers(base, chain);
  registry = {
    at: Date.now(),
    members: onChain.map(m => {
      const hosted = state.members.find(x => x.index === m.index);
      return { index: m.index, civ: m.civ, name: m.name, kind: ['human', 'agent', 'undeclared'][m.kind] ?? 'undeclared',
        attested: m.attestation.some(b => b !== 0), hosted: hosted?.hosted ?? 'external', wallet: b58(m.wallet), session: b58(m.session),
        stand: m.stand, shares: m.shares, claimed: m.claimed };
    }),
  };
  return registry.members;
}
const hostedKey = m => {
  const h = state.members.find(x => x.index === m);
  return h && h.hosted !== 'external' ? namedKey(`member${h.key}-session`) : null;
};

const routes = {
  'GET /health': async () => ({ ok: true, phase: crank.phase, season: state.seasonId }),
  'GET /season': async () => {
    const acc = await base.getAccountInfo(chain.season, 'confirmed');
    const s = decodeSeason(acc.data);
    return {
      season: { ...s, admin: b58(s.admin), crank: b58(s.crank), usdcMint: b58(s.usdcMint) },
      nations: NATIONS.slice(0, s.nations), members: await members(),
      programId: cfg.programId, cluster: cfg.cluster, phase: crank.phase,
      accounts: { season: chain.season.toBase58(), world: chain.world.toBase58(), vault: chain.vault.toBase58(), nations: chain.nations(s.nations).map(k => k.toBase58()) },
      genesis: state.genesis ?? null, seating: state.seating ?? [], open: state.open ?? null, endpoints: { base: cfg.baseRpc, er: cfg.erRpc },
    };
  },
  'GET /ticks': async (req, url) => ({ records: crank.tickRecords(Number(url.searchParams.get('from') || 0)) }),
  'POST /submit': async req => {
    const b = await readBody(req);
    const member = b.member ?? NOBODY;
    // A vacant office is run by the acting official, which the crank signs for.
    const signer = member === NOBODY ? crank.crank : hostedKey(member);
    if (!signer) return [403, { error: 'this member is not hosted by this gateway; sign it yourself and use /relay' }];
    const digest = fromHex(b.digest);
    const ixs = chain.submitOrders({ signer: signer.publicKey, civ: b.civ, role: b.role, tick: b.tick, decisionDigest: digest.length === 32 ? digest : new Uint8Array(32), orders: b.orders || [], adopt: b.adopt || [] });
    const r = await send(er, ixs, signer === crank.crank ? [crank.crank] : [crank.crank, signer], `submit ${NATIONS[b.civ]} ${b.role}`);
    return { ok: true, signature: r.signature };
  },
  'POST /gov': async req => {
    const b = await readBody(req);
    const signer = hostedKey(b.member);
    if (!signer) return [403, { error: 'this member is not hosted by this gateway; sign it yourself and use /relay' }];
    const civ = (await members()).find(m => m.index === b.member)?.civ;
    if (civ === undefined) return [404, { error: 'no such member' }];
    const r = await send(er, chain.submitGov({ signer: signer.publicKey, civ, member: b.member, action: b.action }), [crank.crank, signer], `gov ${b.action?.type} by member ${b.member}`);
    return { ok: true, signature: r.signature };
  },
  'GET /relay': async () => {
    const { blockhash } = await er.getLatestBlockhash('confirmed');
    return { feePayer: crank.crank.publicKey.toBase58(), blockhash, programId: cfg.programId, endpoint: cfg.erRpc };
  },
  'POST /relay': async req => {
    const b = await readBody(req);
    const tx = Transaction.from(Buffer.from(b.tx, 'base64'));
    // Only SubmitOrders (6) or SubmitGov (17) to this program, paid by the gateway, nothing else.
    const program = tx.instructions.filter(i => i.programId.toBase58() === cfg.programId);
    const budget = tx.instructions.filter(i => i.programId.toBase58() === 'ComputeBudget111111111111111111111111111111');
    const only = program.length === 1 && program.length + budget.length === tx.instructions.length && [6, 17].includes(program[0].data[0]);
    if (!only || !tx.feePayer?.equals(crank.crank.publicKey)) return [400, { error: 'relay accepts exactly one SubmitOrders or SubmitGov instruction with the gateway as fee payer' }];
    tx.partialSign(crank.crank);
    const r = await sendSigned(er, tx, 'relay');
    return { ok: true, signature: r.signature };
  },
};

// Claims on the base layer, for members without SOL (agents): the member's
// wallet signs `Claim`, the gateway only pays the fee. The program pays only
// into a token account the wallet owns, so the fee payer cannot redirect it.
routes['GET /claim-relay'] = async () => {
  const { blockhash } = await base.getLatestBlockhash('confirmed');
  const s = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
  return { feePayer: crank.crank.publicKey.toBase58(), blockhash, programId: cfg.programId, mint: new PublicKey(s.usdcMint).toBase58(), status: s.status };
};
routes['POST /claim-relay'] = async req => {
  const b = await readBody(req);
  const tx = Transaction.from(Buffer.from(b.tx, 'base64'));
  const program = tx.instructions.filter(i => i.programId.toBase58() === cfg.programId);
  const only = program.length === 1 && tx.instructions.length === 1 && program[0].data[0] === 11 && program[0].keys[1]?.pubkey.equals(chain.season);
  if (!only || !tx.feePayer?.equals(crank.crank.publicKey)) return [400, { error: 'claim relay accepts exactly one Claim for this season with the gateway as fee payer' }];
  tx.partialSign(crank.crank);
  const r = await sendSigned(base, tx, 'claim relay');
  return { ok: true, signature: r.signature };
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
    if (await mountX402({ req, res, url, cfg, base, chain, state, json, readBody, log, onJoin: () => { registry.at = 0; } })) return;
    json(res, 404, { error: 'not found' });
  } catch (e) {
    // A submission that arrives after the tick's input froze (or after it
    // resolved) is too late for that tick, not a failure: 409, so clients
    // can retry on the next tick.
    const code = chainError(e.message);
    const late = code === 'TickFrozen' || code === 'WrongTick';
    if (!late) log(`${req.method} ${url.pathname}: ${e.message}`);
    json(res, late ? 409 : 500, { error: code ? `${code}: ${e.message}` : e.message, code, logs: e.logs?.slice(-6) });
  }
});
server.listen(cfg.port, '127.0.0.1', () => log(`gateway on http://127.0.0.1:${cfg.port} season ${state.seasonId} (${crank.phase})`));
