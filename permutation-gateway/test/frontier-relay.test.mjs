// The Frontier relay (src/frontier) end to end over a scripted chain: shapes
// and the pool, the drain guard, tip presets, quotas (nothing charged on a
// failed simulation; settle shapes charged to the requester), replays, the
// join gate with one-time invites, UseRevealRoute, the keeper link, the
// X-Forwarded-For rule and the read routes (contract §8.3, I-24, I-51).
// No network except 127.0.0.1:0 fixture servers.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { Keypair, PublicKey } from '@solana/web3.js';
import { toBase64 } from '../client/src/bytes.mjs';
import { encode as base58 } from '../client/src/base58.mjs';
import { CLOCK_SYSVAR, FrontierAddresses, hostId } from '../client/src/frontier/addresses.mjs';
import { encodeAccount } from '../client/src/frontier/codec.mjs';
import { minTipLamports, rent, tipPresets } from '../client/src/frontier/fees.mjs';
import { commit, ctHash, encodePath, pack, revealMaterial, saltOf, sealRoot } from '../client/src/frontier/seal.mjs';
import { shapeMessage } from '../client/src/frontier/shapes.mjs';
import { parseTransaction, wireTransaction } from '../client/src/solana-tx.mjs';
import { signTalk, verifyTalk } from '../client/src/talk-node.mjs';
import { createFrontierApps, clientAddress } from '../src/frontier/app.mjs';
import { InviteBook } from '../src/frontier/invites.mjs';
import { KeeperLink } from '../src/frontier/keeperlink.mjs';
import { PayerPool } from '../src/frontier/payers.mjs';
import { QUOTA } from '../src/frontier/quota.mjs';
import { call } from './gateway-fixtures.mjs';

const PROGRAM = Keypair.generate().publicKey.toBase58();
const SEASON_ID = 5n;
const A = new FrontierAddresses({ programId: PROGRAM, seasonId: SEASON_ID });
const GENESIS = 1_800_000_000;
const MARCH_FEE = 10_000n;
const SEAL_BOND = 20_000n;
const TIP_MIN = minTipLamports(433, 26_000, 1_048_576);
const BLOCKHASH = Keypair.generate().publicKey.toBase58();
const OPERATOR = 'op-token';

function seasonAccount(over = {}) {
  return encodeAccount('Season', { SEASON_ID, STATUS: 2, GENESIS_TS: BigInt(GENESIS), MARCH_FEE, SEAL_BOND, MIN_REVEAL_PRIORITY_MILLI: 433, REVEAL_CU_LIMIT: 26_000,
    REVEAL_LOADED_LIMIT: 1_048_576, JOIN_GATE: new Uint8Array(32), ...over });
}

function clockAccount(unix) {
  const b = Buffer.alloc(40);
  b.writeBigUInt64LE(77n, 0);
  b.writeBigInt64LE(BigInt(unix), 32);
  return b;
}

/**
 * A scripted chain: accounts (base58 → {lamports, data}); simulate checks
 * every signature (as sigVerify), charges the fee payer 5,000 per signature
 * plus `debit(tx)` and reports its post-balance; `fail(tx)` makes a
 * simulation fail with that error.
 */
function fakeChain({ debit = () => 0n, fail = () => null } = {}) {
  const c = {
    accounts: new Map(), sent: [], simulated: 0, statuses: new Map(), height: 100,
    set(key, v) { c.accounts.set(String(key), { lamports: v.lamports ?? 1_000_000_000, data: Buffer.from(v.data ?? []) }); },
    getAccountInfo: async key => c.accounts.get(new PublicKey(key).toBase58()) ?? null,
    getMultipleAccountsInfo: async keys => keys.map(k => c.accounts.get(new PublicKey(k).toBase58()) ?? null),
    getBalance: async key => c.accounts.get(new PublicKey(key).toBase58())?.lamports ?? 0,
    getLatestBlockhash: async () => ({ blockhash: BLOCKHASH, lastValidBlockHeight: 150 }),
    getBlockHeight: async () => c.height,
    getSlot: async () => 77,
    simulateTransaction: async (vtx, cfg) => {
      c.simulated++;
      const wire = vtx.serialize();
      const tx = parseTransaction(wire);
      assert.equal(cfg.sigVerify, true, 'simulated with signatures');
      assert.equal(cfg.replaceRecentBlockhash, false);
      for (const [i, s] of tx.signers.entries()) {
        if (!verifyTalk(tx.message, tx.signatures[i], new PublicKey(s).toBytes())) return { value: { err: 'SignatureFailure', logs: [] } };
      }
      const err = fail(tx);
      if (err) return { value: { err, logs: ['Program log: refused'] } };
      const payer = tx.signers[0];
      const pre = BigInt(c.accounts.get(payer)?.lamports ?? 0);
      const post = pre - 5_000n * BigInt(tx.signers.length) - BigInt(debit(tx));
      return { value: { err: null, logs: [], unitsConsumed: 9_000, accounts: cfg.accounts.addresses.map(a => (a === payer ? { lamports: Number(post), data: ['', 'base64'] } : null)) } };
    },
    sendRawTransaction: async raw => {
      const tx = parseTransaction(new Uint8Array(raw));
      c.sent.push(tx);
      return base58(tx.signatures[0]);
    },
    getSignatureStatuses: async sigs => ({ value: sigs.map(s => c.statuses.get(s) ?? null) }),
  };
  return c;
}

const MASTER = Buffer.alloc(32, 7);

function relay({ chain = fakeChain(), season = {}, clock = GENESIS + 3_600, poolSize = 150, keeper = null, gateKey = null, heraldPeer = '127.0.0.1', log = () => {}, now = Date.now } = {}) {
  const pool = new PayerPool({ masterSeed: MASTER, n: poolSize, dev: poolSize < 150 });
  for (const k of pool.publicKeys()) chain.set(k, { lamports: 2_000_000_000 });
  chain.set(A.season, { data: seasonAccount(season) });
  chain.set(CLOCK_SYSVAR, { data: clockAccount(clock) });
  const store = { state: {}, save() { store.saves = (store.saves ?? 0) + 1; } };
  const cfg = { cluster: 'localnet', programId: PROGRAM, seasonId: String(SEASON_ID), heraldPeer, heraldUrl: 'http://127.0.0.1:41040', minPoolSol: 1, operatorToken: OPERATOR };
  const invites = new InviteBook({ secret: Buffer.alloc(32, 3), seasonId: SEASON_ID, store });
  const apps = createFrontierApps({ cfg, connection: chain, pool, keeper, invites, gateKey, store, log, now });
  return { ...apps, chain, pool, store, invites };
}

// ------------------------------------------------------------------ building player transactions

const wallet = Keypair.generate();
const session = Keypair.generate();
const HOLD = [-3, 7, 11];

function playerTx({ name, fields = {}, feePayer, signer = session, accounts = {}, extraSign = [], recentBlockhash = BLOCKHASH }) {
  const base = { actor: signer.publicKey.toBase58(), season: A.season, citizen: A.citizen(wallet.publicKey.toBase58()), holding: A.holding(...HOLD),
    province: A.province(HOLD[0], HOLD[1]), frontier: A.frontier };
  const message = shapeMessage({ programId: PROGRAM, name, accounts: { ...base, ...accounts }, fields, feePayer, recentBlockhash });
  const sigs = {};
  for (const k of [signer, ...extraSign]) sigs[k.publicKey.toBase58()] = signTalk(message, k);
  return toBase64(wireTransaction(message, sigs));
}

const departFields = tip => {
  const p = encodePath([0, 1]);
  const plain = pack({ hostId: hostId(...HOLD, 0, 1), arriveBell: 147, destP: -2, destQ: 7, destTile: 30, stance: 1, retreatBps: 0, pathLen: p.pathLen, path: p.path });
  const k = new Uint8Array(16).fill(1);
  return { host_id: hostId(...HOLD, 0, 1), commit: commit(plain, saltOf(k)), seal: new Uint8Array(165).fill(0xc0), arrive_bell: 147, tip, transit_slot: 1 };
};

async function feePayerOf(r, ip = '127.0.0.1') {
  const g = await call(r.public, 'GET', '/f/relay', { ip });
  assert.equal(g.status, 200, JSON.stringify(g.json));
  return g.json.feePayer;
}

// ------------------------------------------------------------------ tests

test('GET /f/relay draws a pool payer and reports the blockhash and quota; GET /f/season publishes the terms', async () => {
  const r = relay();
  const g = await call(r.public, 'GET', `/f/relay?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' });
  assert.equal(g.status, 200);
  assert.ok(r.pool.has(g.json.feePayer));
  assert.equal(g.json.blockhash, BLOCKHASH);
  assert.equal(g.json.programId, PROGRAM);
  assert.deepEqual(g.json.quota, { left: 40, resetsAt: GENESIS + 86_400 });
  const s = await call(r.public, 'GET', '/f/season', { ip: '203.0.113.9' });
  assert.equal(s.status, 200);
  assert.equal(s.json.relayPool, 150);
  assert.equal(s.json.season, A.season);
  assert.equal(s.json.joinGate, null);
  assert.equal(s.json.inviteRequired, false);
  assert.deepEqual(s.json.tipPresets, tipPresets(TIP_MIN).map(String));
  assert.equal(s.json.tipMin, '14441');
  assert.equal(s.json.quotas.txsPerDay, 40);
  assert.equal(s.json.quotas.txsPerDayAfter, 20);
  // Without the season account: 503, not a crash.
  const empty = relay();
  empty.chain.accounts.delete(A.season);
  assert.equal((await call(empty.public, 'GET', '/f/season', { ip: '203.0.113.9' })).json.code, 'WorldUnavailable');
});

test('POST /f/relay co-signs a player shape, simulates it with signatures and sends it; the quota is charged once', async () => {
  const r = relay();
  const feePayer = await feePayerOf(r);
  const tx = playerTx({ name: 'Harvest', feePayer });
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx }, ip: '203.0.113.9' });
  assert.equal(res.status, 200, JSON.stringify(res.json));
  assert.equal(res.json.ok, true);
  assert.equal(r.chain.sent.length, 1);
  const sent = r.chain.sent[0];
  assert.equal(sent.signers[0], feePayer);
  assert.ok(verifyTalk(sent.message, sent.signatures[0], new PublicKey(feePayer).toBytes()), 'the relay signed as the fee payer');
  assert.equal(res.json.signature, base58(sent.signatures[0]));
  const q = await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' });
  assert.equal(q.json.left, 39);
  // The same transaction again: 409 Duplicate, nothing sent or charged.
  const again = await call(r.public, 'POST', '/f/relay', { body: { tx }, ip: '203.0.113.9' });
  assert.equal(again.json.code, 'Duplicate');
  assert.equal(r.chain.sent.length, 1);
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' })).json.left, 39);
});

test('POST /f/relay refuses: a Reveal (UseRevealRoute), a Join (its own route), a payer outside the pool, another season, a bad signature', async () => {
  const r = relay();
  const feePayer = await feePayerOf(r);
  const reveal = playerTx({ name: 'Harvest', feePayer });
  // Rewrite the program instruction's tag to Reveal's: the classifier stops at the tag.
  const t = parseTransaction(Buffer.from(reveal, 'base64'));
  const ixs = t.instructions.map(ix => ({ programId: ix.programId, keys: ix.keys, data: ix.data }));
  ixs[3] = { ...ixs[3], data: Uint8Array.from([0x51]) };
  const { compileMessage } = await import('../client/src/solana-tx.mjs');
  const rv = toBase64(wireTransaction(compileMessage({ feePayer, recentBlockhash: BLOCKHASH, instructions: ixs })));
  const a = await call(r.public, 'POST', '/f/relay', { body: { tx: rv }, ip: '203.0.113.9' });
  assert.equal(a.status, 400);
  assert.equal(a.json.code, 'UseRevealRoute');
  const join = playerTx({ name: 'Join', signer: wallet, feePayer, accounts: { wallet: wallet.publicKey.toBase58(), joinshard: A.joinShardFor(2, wallet.publicKey.toBase58()) },
    fields: { faction: 2, session: session.publicKey.toBytes(), session_expiry: BigInt(GENESIS + 86_400) } });
  assert.equal((await call(r.public, 'POST', '/f/relay', { body: { tx: join }, ip: '203.0.113.9' })).json.code, 'RelayRejected');
  const stranger = Keypair.generate().publicKey.toBase58();
  const out = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer: stranger }) }, ip: '203.0.113.9' });
  assert.equal(out.json.code, 'RelayRejected');
  assert.match(out.json.error, /fee payer/);
  const other = new FrontierAddresses({ programId: PROGRAM, seasonId: 6n });
  const wrongSeason = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer, accounts: { season: other.season } }) }, ip: '203.0.113.9' });
  assert.equal(wrongSeason.json.code, 'RelayRejected');
  const forged = parseTransaction(Buffer.from(playerTx({ name: 'Harvest', feePayer }), 'base64'));
  const sigs = [...forged.signatures];
  sigs[1] = new Uint8Array(64).fill(1);
  const bad = await call(r.public, 'POST', '/f/relay', { body: { tx: toBase64(wireTransaction(forged.message, sigs)) }, ip: '203.0.113.9' });
  assert.equal(bad.json.code, 'BadSignature');
  const junk = await call(r.public, 'POST', '/f/relay', { body: { tx: 'AAAA' }, ip: '203.0.113.9' });
  assert.equal(junk.json.code, 'InvalidTransaction');
  assert.equal(r.chain.sent.length, 0);
  assert.equal(r.chain.simulated, 0, 'nothing refused above reached a simulation');
});

test('a failed simulation charges nothing and sends nothing; the program error is named', async () => {
  const chain = fakeChain({ fail: () => ({ InstructionError: [3, { Custom: 24 }] }) });
  const r = relay({ chain });
  const feePayer = await feePayerOf(r);
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer }) }, ip: '203.0.113.9' });
  assert.equal(res.status, 409);
  assert.equal(res.json.code, 'NotFinal');
  assert.equal(res.json.programCode, 24);
  assert.equal(chain.sent.length, 0);
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' })).json.left, 40, 'not charged');
});

test('the drain guard: the fee payer loses at most the fee plus the kind\'s allowance', async () => {
  // Harvest may move nothing beyond the fee.
  const chain = fakeChain({ debit: () => 1n });
  const r = relay({ chain });
  const feePayer = await feePayerOf(r);
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer }) }, ip: '203.0.113.9' });
  assert.equal(res.json.code, 'RelayRejected');
  assert.match(res.json.error, /would lose 10001 lamports/);
  assert.equal(chain.sent.length, 0);
  // Depart may move tip + march fee + seal bond, not a lamport more.
  for (const [extra, ok] of [[0n, true], [1n, false]]) {
    const tip = TIP_MIN;
    const c = fakeChain({ debit: () => tip + MARCH_FEE + SEAL_BOND + extra });
    const rr = relay({ chain: c });
    const fp = await feePayerOf(rr);
    const d = await call(rr.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Depart', feePayer: fp, fields: departFields(tip) }) }, ip: '203.0.113.9' });
    assert.equal(d.json.ok === true, ok, JSON.stringify(d.json));
    assert.equal(c.sent.length, ok ? 1 : 0);
    // The lamport quota keeps what the simulation moved beyond the fee (nothing for a refusal).
    const q = rr.store.state.quota[`citizen:${A.citizen(wallet.publicKey.toBase58())}`];
    assert.deepEqual(q && [q.left, q.lamports], ok ? [39, Number(tip + MARCH_FEE + SEAL_BOND)] : [40, 0]);
  }
});

test('Depart tips are the three presets only (TipNotPreset); FileTicket may move the escrow shortfall', async () => {
  const tipOf = d => Buffer.from(d).readBigUInt64LE(210);
  const chain = fakeChain({ debit: tx => (tx.instructions[3].data[0] === 0x50 ? tipOf(tx.instructions[3].data) + MARCH_FEE + SEAL_BOND : 0n) });
  const r = relay({ chain });
  for (const tip of [0n, TIP_MIN - 1n, TIP_MIN + 1n, TIP_MIN * 3n]) {
    const fp = await feePayerOf(r);
    const d = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Depart', feePayer: fp, fields: departFields(tip) }) }, ip: '203.0.113.9' });
    assert.equal(d.json.code, 'TipNotPreset', `tip ${tip}`);
    assert.deepEqual(d.json.presets, tipPresets(TIP_MIN).map(String));
  }
  for (const tip of tipPresets(TIP_MIN)) {
    const fp = await feePayerOf(r);
    const d = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Depart', feePayer: fp, fields: departFields(tip) }) }, ip: '203.0.113.9' });
    assert.equal(d.json.ok, true, `tip ${tip}: ${JSON.stringify(d.json)}`);
  }
  // FileTicket: the Holding-rent escrow shortfall (rent(1,280) − ticket_escrow).
  const holdingRent = rent(1280);
  for (const [escrow, moved, ok] of [[0n, holdingRent, true], [0n, holdingRent + 1n, false], [holdingRent - 5n, 5n, true], [holdingRent - 5n, 6n, false], [holdingRent, 0n, true]]) {
    const c = fakeChain({ debit: () => moved });
    const rr = relay({ chain: c });
    c.set(A.citizen(wallet.publicKey.toBase58()), { data: encodeAccount('Citizen', { SEASON_ID, TICKET_ESCROW: escrow }) });
    const fp = await feePayerOf(rr);
    const t = playerTx({ name: 'FileTicket', feePayer: fp, fields: { sites: [{ p: -3, q: 7, site: 11 }] } });
    const d = await call(rr.public, 'POST', '/f/relay', { body: { tx: t }, ip: '203.0.113.9' });
    assert.equal(d.json.ok === true, ok, `escrow ${escrow}, moved ${moved}: ${JSON.stringify(d.json)}`);
  }
});

test('quotas: 40 a game day for days 0–6 with carry-over to 60, then 20; lamports capped; resets at the game midnight', async () => {
  // The per-signer bucket (burst 24, 1/s) refills between sends: 2 s of the relay's clock each.
  let t = 1_000_000;
  const r = relay({ poolSize: 4, now: () => t });
  const citizen = A.citizen(wallet.publicKey.toBase58());
  const send = async () => {
    t += 2_000;
    const fp = await feePayerOf(r);
    // A fresh blockhash each time: distinct transactions (the replay cache keys on the session's signature).
    return call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer: fp, recentBlockhash: Keypair.generate().publicKey.toBase58() }) }, ip: '127.0.0.1' });
  };
  let ok = 0;
  for (let i = 0; i < 45; i++) if ((await send()).json.ok) ok++;
  assert.equal(ok, 40, 'day 0: 40');
  const over = await send();
  assert.equal(over.status, 429);
  assert.equal(over.json.code, 'QuotaExceeded');
  assert.equal(over.json.retryAt, GENESIS + 86_400);
  // Next game day: the Clock moved (the wall clock did not); nothing carried over from a spent day.
  r.chain.set(CLOCK_SYSVAR, { data: clockAccount(GENESIS + 86_400 + 5) });
  r.ctx.chain.cache.clear();
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${citizen}`, { ip: '127.0.0.1' })).json.left, 40);
  assert.equal((await send()).json.ok, true);
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${citizen}`, { ip: '127.0.0.1' })).json.left, 39);
  // The per-signer bucket still applies: 30 sends within one second of the relay's clock.
  const burst = [];
  for (let i = 0; i < 30; i++) {
    const fp = await feePayerOf(r);
    burst.push((await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer: fp, recentBlockhash: Keypair.generate().publicKey.toBase58() }) }, ip: '127.0.0.1' })).json.code);
  }
  assert.ok(burst.includes('RateLimited'), 'the signer bucket (burst 24) stops a rush');
  // Idle days carry over, capped at the burst (60).
  r.chain.set(CLOCK_SYSVAR, { data: clockAccount(GENESIS + 4 * 86_400 + 5) });
  r.ctx.chain.cache.clear();
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${citizen}`, { ip: '127.0.0.1' })).json.left, QUOTA.burst);
  // A fresh citizen on day 7 gets 20.
  r.chain.set(CLOCK_SYSVAR, { data: clockAccount(GENESIS + 7 * 86_400 + 5) });
  r.ctx.chain.cache.clear();
  const fresh = await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(Keypair.generate().publicKey.toBase58())}`, { ip: '127.0.0.1' });
  assert.equal(fresh.json.left, 20);
  assert.equal(fresh.json.resetsAt, GENESIS + 8 * 86_400);
});

test('settle shapes are charged to the requester\'s verified citizen (else the address), never to the citizen they name', async () => {
  const r = relay();
  const buildSettle = feePayer => {
    const pl = pack({ hostId: 1n, arriveBell: 147, destP: 0, destQ: 0, destTile: 0, stance: 0, retreatBps: 0, pathLen: 0, path: new Uint8Array(12) });
    const accounts = { payer: feePayer, season: A.season, holding: A.holding(...HOLD), dest_province: A.province(-2, 7), inputs: A.clashInputs(-2, 7, 147),
      slot: A.arrivalSlot(-2, 7, 147, 3, 0), home_province: A.province(-3, 7), anchor_or_archive: A.bellAnchor(147, 4), slot_beneficiary: feePayer, resolver: feePayer,
      holding_rent_payer: Keypair.generate().publicKey.toBase58(), settle_beneficiary: feePayer };
    const message = shapeMessage({ programId: PROGRAM, name: 'SettleTransit', accounts, fields: { transit_slot: 1, commit: commit(pl, new Uint8Array(32)), seal: new Uint8Array(165), beneficiary: new PublicKey(feePayer).toBytes() },
      feePayer, recentBlockhash: Keypair.generate().publicKey.toBase58() });
    return { message, tx: toBase64(wireTransaction(message)) };
  };
  // Another player (not the one the settle names) with a live session key.
  const w2 = Keypair.generate();
  const s2 = Keypair.generate();
  const c2 = A.citizen(w2.publicKey.toBase58());
  r.chain.set(c2, { data: encodeAccount('Citizen', { SEASON_ID, WALLET: w2.publicKey.toBytes(), SESSION: s2.publicKey.toBytes(), SESSION_EXPIRY: BigInt(GENESIS + 7_200) }) });
  const settleAs = async (key, citizen, ip = '203.0.113.9') => {
    const s1 = buildSettle(await feePayerOf(r));
    return call(r.public, 'POST', '/f/relay', { body: { tx: s1.tx, requester: key.publicKey.toBase58(), requesterSig: toBase64(signTalk(s1.message, key)), citizen }, ip });
  };
  // The session key and the wallet both charge that citizen.
  assert.equal((await settleAs(s2, c2)).json.ok, true);
  assert.equal((await settleAs(w2, c2)).json.ok, true);
  assert.equal(r.store.state.quota[`citizen:${c2}`].left, 38);
  assert.equal(r.store.state.quota[`citizen:${A.citizen(wallet.publicKey.toBase58())}`], undefined, 'the named citizen is not charged');
  // Fresh throwaway keys (valid signatures, no citizen) open no bucket of
  // their own: they all land in the client-address bucket.
  for (let i = 0; i < 3; i++) {
    const res = await settleAs(Keypair.generate(), c2, '198.51.100.7');
    assert.equal(res.json.ok, true, JSON.stringify(res.json));
  }
  assert.equal(r.store.state.quota['addr:198.51.100.7'].left, 37);
  assert.ok(!Object.keys(r.store.state.quota).some(k => k.startsWith('session:')), 'no per-key buckets');
  assert.equal(r.store.state.quota[`citizen:${c2}`].left, 38, 'a stranger naming c2 does not charge (or use) c2');
  // An expired session key is anonymous too.
  r.chain.set(c2, { data: encodeAccount('Citizen', { SEASON_ID, WALLET: w2.publicKey.toBytes(), SESSION: s2.publicKey.toBytes(), SESSION_EXPIRY: BigInt(GENESIS) }) });
  assert.equal((await settleAs(s2, c2, '198.51.100.8')).json.ok, true);
  assert.equal(r.store.state.quota['addr:198.51.100.8'].left, 39);
  // A Citizen at a non-canonical address (its wallet's is elsewhere) is not trusted.
  const fake = Keypair.generate().publicKey.toBase58();
  r.chain.set(fake, { data: encodeAccount('Citizen', { SEASON_ID, WALLET: w2.publicKey.toBytes() }) });
  assert.equal((await settleAs(w2, fake, '198.51.100.9')).json.ok, true);
  assert.equal(r.store.state.quota['addr:198.51.100.9'].left, 39);
  // Anonymous: the client-address bucket.
  const anon = await call(r.public, 'POST', '/f/relay', { body: { tx: buildSettle(await feePayerOf(r)).tx }, ip: '203.0.113.9' });
  assert.equal(anon.json.ok, true);
  assert.equal(r.store.state.quota['addr:203.0.113.9'].left, 39);
  // A requester signature over another message is refused.
  const s1 = buildSettle(await feePayerOf(r));
  const s3 = buildSettle(await feePayerOf(r));
  const bad = await call(r.public, 'POST', '/f/relay', { body: { tx: s3.tx, requester: s2.publicKey.toBase58(), requesterSig: toBase64(signTalk(s1.message, s2)), citizen: c2 }, ip: '203.0.113.9' });
  assert.equal(bad.json.code, 'BadSignature');
});

test('identical concurrent submissions: one is sent and charged, the rest are Duplicate', async () => {
  const chain = fakeChain();
  // A slow chain: every read waits, so the requests interleave at each await.
  const slow = f => async (...a) => { await new Promise(res => setTimeout(res, 5)); return f(...a); };
  chain.getAccountInfo = slow(chain.getAccountInfo);
  chain.getBalance = slow(chain.getBalance);
  const r = relay({ chain });
  const feePayer = await feePayerOf(r);
  const tx = playerTx({ name: 'Harvest', feePayer });
  const res = await Promise.all([0, 1, 2].map(() => call(r.public, 'POST', '/f/relay', { body: { tx }, ip: '203.0.113.9' })));
  assert.deepEqual(res.map(x => x.json.ok === true).sort(), [false, false, true], JSON.stringify(res.map(x => x.json)));
  assert.equal(res.filter(x => x.json.code === 'Duplicate').length, 2);
  assert.equal(chain.sent.length, 1);
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' })).json.left, 39);
});

test('a send that throws charges nothing and the same bytes may be sent again', async () => {
  const chain = fakeChain();
  const send = chain.sendRawTransaction;
  chain.sendRawTransaction = async () => { throw new Error('node down'); };
  const r = relay({ chain });
  const feePayer = await feePayerOf(r);
  const tx = playerTx({ name: 'Harvest', feePayer });
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx }, ip: '203.0.113.9' });
  assert.notEqual(res.json.ok, true);
  assert.equal((await call(r.public, 'GET', `/f/quota?citizen=${A.citizen(wallet.publicKey.toBase58())}`, { ip: '203.0.113.9' })).json.left, 40, 'not charged');
  chain.sendRawTransaction = send;
  assert.equal((await call(r.public, 'POST', '/f/relay', { body: { tx }, ip: '203.0.113.9' })).json.ok, true, 'not a Duplicate');
});

test('POST /f/join: open seasons sponsor the Join; gated seasons need a one-time invite and the relay co-signs with the gate key', async () => {
  const joinTx = (feePayer, gate, w = wallet) => playerTx({ name: 'Join', signer: w, feePayer,
    accounts: { wallet: w.publicKey.toBase58(), citizen: A.citizen(w.publicKey.toBase58()), joinshard: A.joinShardFor(2, w.publicKey.toBase58()), ...(gate ? { join_gate: gate } : {}) },
    fields: { faction: 2, session: session.publicKey.toBytes(), session_expiry: BigInt(GENESIS + 86_400) } });
  // Open: the Citizen's rent is the allowance.
  const open = relay({ chain: fakeChain({ debit: () => rent(384) }) });
  const fp = await feePayerOf(open);
  const ok = await call(open.public, 'POST', '/f/join', { body: { tx: joinTx(fp) }, ip: '203.0.113.9' });
  assert.equal(ok.json.ok, true, JSON.stringify(ok.json));
  assert.equal((await call(open.public, 'POST', '/f/join', { body: { tx: playerTx({ name: 'Harvest', feePayer: fp }) }, ip: '203.0.113.9' })).json.code, 'RelayRejected');
  // Gated.
  const gate = Keypair.generate();
  const chain = fakeChain({ debit: () => rent(384) });
  const r = relay({ chain, gateKey: gate, season: { JOIN_GATE: gate.publicKey.toBytes() } });
  const s = await call(r.public, 'GET', '/f/season', { ip: '203.0.113.9' });
  assert.equal(s.json.joinGate, gate.publicKey.toBase58());
  assert.equal(s.json.inviteRequired, true);
  const fp2 = await feePayerOf(r);
  assert.equal((await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp2) }, ip: '203.0.113.9' })).json.code, 'RelayRejected', 'the gate account is required');
  assert.equal((await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp2, gate.publicKey.toBase58()) }, ip: '203.0.113.9' })).json.code, 'InviteRequired');
  assert.equal((await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp2, gate.publicKey.toBase58()), invite: 'forged' }, ip: '203.0.113.9' })).json.code, 'InviteRequired');
  const inv = await call(r.operator, 'POST', '/f/operator/invites', { body: { count: 2 }, headers: { authorization: `Bearer ${OPERATOR}` } });
  assert.equal(inv.json.invites.length, 2);
  // A failed simulation does not spend the invite.
  const failing = fakeChain({ fail: () => ({ InstructionError: [3, { Custom: 10 }] }) });
  const simulate = chain.simulateTransaction;
  r.ctx.connection.simulateTransaction = failing.simulateTransaction;
  const refused = await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp2, gate.publicKey.toBase58()), invite: inv.json.invites[0] }, ip: '203.0.113.9' });
  assert.equal(refused.json.code, 'Capacity');
  r.ctx.connection.simulateTransaction = simulate;
  const joined = await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp2, gate.publicKey.toBase58()), invite: inv.json.invites[0] }, ip: '203.0.113.9' });
  assert.equal(joined.json.ok, true, JSON.stringify(joined.json));
  const sent = chain.sent.at(-1);
  const gi = sent.signers.indexOf(gate.publicKey.toBase58());
  assert.ok(gi > 0 && verifyTalk(sent.message, sent.signatures[gi], gate.publicKey.toBytes()), 'the gate co-signed');
  // One-time: the same invite for another wallet is refused.
  const w2 = Keypair.generate();
  const fp3 = await feePayerOf(r);
  assert.equal((await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp3, gate.publicKey.toBase58(), w2), invite: inv.json.invites[0] }, ip: '203.0.113.9' })).json.code, 'InviteRequired');
  assert.equal((await call(r.public, 'POST', '/f/join', { body: { tx: joinTx(fp3, gate.publicKey.toBase58(), w2), invite: inv.json.invites[1] }, ip: '203.0.113.9' })).json.ok, true);
  // Invites are operator-only.
  assert.equal((await call(r.public, 'POST', '/f/operator/invites', { body: { count: 1 }, headers: { authorization: `Bearer ${OPERATOR}` } })).status, 404);
  assert.equal((await call(r.operator, 'POST', '/f/operator/invites', { body: { count: 1 }, headers: { authorization: 'Bearer nope' } })).json.code, 'OperatorOnly');
  // A relay without the season's gate key cannot co-sign.
  const keyless = relay({ season: { JOIN_GATE: gate.publicKey.toBytes() } });
  const fp4 = await feePayerOf(keyless);
  assert.equal((await call(keyless.public, 'POST', '/f/join', { body: { tx: joinTx(fp4, gate.publicKey.toBase58()), invite: inv.json.invites[1] }, ip: '203.0.113.9' })).json.code, 'GateUnavailable');
});

test('POST /f/reveal and /f/nudge go to the keeper\'s loopback API with its token; answers pass through; a silent keeper is 502', async () => {
  const seen = [];
  const keeper = http.createServer((req, res) => {
    let body = '';
    req.on('data', c => { body += c; });
    req.on('end', () => {
      seen.push({ url: req.url, auth: req.headers.authorization, body: JSON.parse(body) });
      const b = JSON.parse(body);
      res.writeHead(req.url === '/v1/reveal' ? (b.transit_slot === 3 ? 410 : 202) : 200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(req.url === '/v1/reveal' ? (b.transit_slot === 3 ? { code: 'WindowClosed' } : { accepted: true, track: 't1' }) : { queued: true, blocking: [] }));
    });
  });
  await new Promise(r => keeper.listen(0, '127.0.0.1', r));
  try {
    const link = new KeeperLink({ url: `http://127.0.0.1:${keeper.address().port}`, token: 'kpr' });
    const r = relay({ keeper: link });
    const pl = pack({ hostId: 1n, arriveBell: 147, destP: 0, destQ: 0, destTile: 0, stance: 0, retreatBps: 0, pathLen: 0, path: new Uint8Array(12) });
    const seal = new Uint8Array(165).fill(3);
    const body = revealMaterial({ holding: A.holding(...HOLD), transitSlot: 1, plain: pl, salt: new Uint8Array(32).fill(2), seal });
    const ok = await call(r.public, 'POST', '/f/reveal', { body, ip: '203.0.113.9' });
    assert.equal(ok.status, 202);
    assert.deepEqual(ok.json, { accepted: true, track: 't1' });
    assert.equal(seen[0].url, '/v1/reveal');
    assert.equal(seen[0].auth, 'Bearer kpr');
    assert.deepEqual(seen[0].body, body);
    const closed = await call(r.public, 'POST', '/f/reveal', { body: { ...body, transit_slot: 3 }, ip: '203.0.113.9' });
    assert.equal(closed.status, 410);
    assert.equal(closed.json.code, 'WindowClosed');
    for (const b of [{ ...body, transit_slot: 4 }, { ...body, plain_b64: toBase64(new Uint8Array(36)) }, { ...body, holding: 'x' }, { ...body, salt_b64: 'not base64!' }]) {
      assert.equal((await call(r.public, 'POST', '/f/reveal', { body: b, ip: '203.0.113.9' })).status, 400);
    }
    assert.equal(seen.length, 2, 'malformed material never reaches the keeper');
    const n = await call(r.public, 'POST', '/f/nudge', { body: { province: [-3, 7], bell: 146 }, ip: '203.0.113.9' });
    assert.deepEqual(n.json, { queued: true, blocking: [] });
    assert.deepEqual(seen.at(-1).body, { province: [-3, 7], bell: 146 });
    assert.equal((await call(r.public, 'POST', '/f/nudge', { body: { province: [1], bell: 1 }, ip: '203.0.113.9' })).status, 400);
    // Material is what the reveal needs: its ct_hash is sha256(seal), the root the Depart stored.
    assert.equal(Buffer.from(body.ct_hash_b64, 'base64').toString('hex'), Buffer.from(ctHash(seal)).toString('hex'));
    assert.equal(sealRoot(commit(pl, new Uint8Array(32).fill(2)), ctHash(seal)).length, 32);
  } finally {
    await new Promise(r => keeper.close(r));
  }
  const down = relay({ keeper: new KeeperLink({ url: 'http://127.0.0.1:9', token: 'x', timeoutMs: 2_000 }) });
  const res = await call(down.public, 'POST', '/f/nudge', { body: { province: [0, 0], bell: 1 }, ip: '203.0.113.9' });
  assert.equal(res.status, 502);
  assert.equal(res.json.code, 'KeeperUnavailable');
});

test('X-Forwarded-For is trusted only from the herald\'s peer; loopback clients skip the address limits, not the quotas', async () => {
  const req = (peer, xff) => ({ socket: { remoteAddress: peer }, headers: xff === undefined ? {} : { 'x-forwarded-for': xff } });
  assert.equal(clientAddress(req('127.0.0.1', '198.51.100.7'), { heraldPeer: '127.0.0.1' }), '198.51.100.7');
  assert.equal(clientAddress(req('::ffff:127.0.0.1', '10.0.0.1, 198.51.100.7'), { heraldPeer: '127.0.0.1' }), '198.51.100.7', 'the last hop is the herald\'s');
  assert.equal(clientAddress(req('127.0.0.2', '198.51.100.7'), { heraldPeer: '127.0.0.1' }), '127.0.0.2', 'another loopback peer is not the herald');
  assert.equal(clientAddress(req('203.0.113.9', '198.51.100.7'), { heraldPeer: '127.0.0.1' }), '203.0.113.9');
  assert.equal(clientAddress(req('127.0.0.1', 'garbage'), { heraldPeer: '127.0.0.1' }), '127.0.0.1');
  // The limit on POST /f/join (burst 10) applies per forwarded client; the herald's own peer address is not the bucket.
  const r = relay({ heraldPeer: '127.0.0.9' });
  const hit = ip => call(r.public, 'POST', '/f/join', { body: {}, ip: '127.0.0.9', headers: { 'x-forwarded-for': ip } });
  const codes = [];
  for (let i = 0; i < 12; i++) codes.push((await hit('198.51.100.7')).json.code);
  assert.equal(codes.filter(c => c === 'RateLimited').length, 2);
  assert.notEqual((await hit('198.51.100.8')).json.code, 'RateLimited', 'another client has its own bucket');
  // A spoofed header from a non-herald peer does not move the bucket.
  const spoof = [];
  for (let i = 0; i < 12; i++) spoof.push((await call(r.public, 'POST', '/f/join', { body: {}, ip: '203.0.113.50', headers: { 'x-forwarded-for': `198.51.100.${i + 100}` } })).json.code);
  assert.equal(spoof.filter(c => c === 'RateLimited').length, 2);
  // Loopback (bots): no address limit.
  const bots = [];
  for (let i = 0; i < 15; i++) bots.push((await call(r.public, 'POST', '/f/join', { body: {}, ip: '127.0.0.1' })).json.code);
  assert.equal(bots.filter(c => c === 'RateLimited').length, 0);
});

test('GET /f/tx/{signature}: landed, failed with the program error, expired, unknown', async () => {
  const r = relay();
  const fp = await feePayerOf(r);
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer: fp }) }, ip: '203.0.113.9' });
  const sig = res.json.signature;
  assert.deepEqual((await call(r.public, 'GET', `/f/tx/${sig}`, { ip: '203.0.113.9' })).json, { state: 'unknown' });
  r.chain.height = 151;
  assert.deepEqual((await call(r.public, 'GET', `/f/tx/${sig}`, { ip: '203.0.113.9' })).json, { state: 'expired' });
  r.chain.statuses.set(sig, { slot: 9, confirmationStatus: 'confirmed', err: null });
  assert.deepEqual((await call(r.public, 'GET', `/f/tx/${sig}`, { ip: '203.0.113.9' })).json, { state: 'landed', slot: 9 });
  r.chain.statuses.set(sig, { slot: 9, confirmationStatus: 'confirmed', err: { InstructionError: [3, { Custom: 58 }] } });
  assert.deepEqual((await call(r.public, 'GET', `/f/tx/${sig}`, { ip: '203.0.113.9' })).json, { state: 'failed', slot: 9, code: 'HostInTransit', programCode: 58 });
  assert.equal((await call(r.public, 'GET', '/f/tx/nope', { ip: '203.0.113.9' })).status, 400);
  assert.equal((await call(r.public, 'GET', '/f/operator/pool', { ip: '203.0.113.9' })).status, 404);
  const pool = await call(r.operator, 'GET', '/f/operator/pool', { headers: { authorization: `Bearer ${OPERATOR}` } });
  assert.equal(pool.json.size, 150);
  assert.equal(pool.json.payers.length, 150);
});

test('the funds breaker: sponsored routes pause while the pool is below its minimum', async () => {
  const r = relay({ poolSize: 2 });
  for (const k of r.pool.publicKeys()) r.chain.set(k, { lamports: 100 });
  const fp = await feePayerOf(r);
  const res = await call(r.public, 'POST', '/f/relay', { body: { tx: playerTx({ name: 'Harvest', feePayer: fp }) }, ip: '203.0.113.9' });
  assert.equal(res.status, 503);
  assert.equal(res.json.code, 'OperatorLowFunds');
  assert.equal(r.chain.sent.length, 0);
  // Reads go on.
  assert.equal((await call(r.public, 'GET', '/f/season', { ip: '203.0.113.9' })).status, 200);
});
