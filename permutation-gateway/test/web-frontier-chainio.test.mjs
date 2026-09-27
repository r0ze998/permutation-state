// The Frontier write path's foundation (permutation-server/web/frontier/
// fchainio.mjs): pins re-derived locally (the Season PDA and bump found
// here, the ruleset hash), requests that never reject, the relay's answer
// checked against the pin, and the message checks every key runs before it
// signs (contract §9.4): the announced fee payer, the exact signers, the
// compute-budget prefix with a zero CU price and the budgets' limits, one
// Frontier instruction of a player or settle tag, never a Reveal
// (UseRevealRoute); after co-signing the message must come back unchanged.
// The relay routes are exercised against a fake relay on 127.0.0.1:0.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync } from 'node:fs';
import { generateKeyPairSync } from 'node:crypto';
import bs58 from 'bs58';
import * as io from '../../permutation-server/web/frontier/fchainio.mjs';
import { RULESET_HASH } from '../../permutation-server/web/frontier/abi.mjs';
import { COMPUTE_BUDGET_PROGRAM, compileMessage, wireTransaction } from '../../permutation-server/web/sdk/solana-tx.mjs';
import { frontierIx } from '../../permutation-server/web/sdk/frontier/shapes.mjs';

const F = JSON.parse(readFileSync(new URL('./frontier-vectors.json', import.meta.url), 'utf8')).addresses;
const key = () => bs58.encode(generateKeyPairSync('ed25519').publicKey.export({ format: 'der', type: 'spki' }).subarray(-32));
const feePayer = key(), session = key(), blockhash = key();
const u32 = v => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); return b; };
const u64 = v => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); return b; };
const cb = (tag, bytes) => ({ programId: COMPUTE_BUDGET_PROGRAM, keys: [], data: Uint8Array.of(tag, ...bytes) });

/** The Harvest this page would build (the expected instruction, from recomputed addresses). */
function expectedHarvest({ holding = { p: 2, q: 0, site: 3 } } = {}) {
  const A = io.pinned().addresses;
  return frontierIx(io.pinned().programId, 'Harvest', { actor: session, payer: feePayer, season: A.season,
    citizen: A.of('Citizen', { wallet: F.citizen.wallet }), holding: A.of('Holding', holding) }, {});
}

function harvest({ price = 0, tag = 0x40, extra = [], prefix = null, payer = feePayer, signers = [session], swap = null, keys = null } = {}) {
  const A = io.pinned().addresses;
  const frontier = {
    programId: io.pinned().programId,
    keys: keys ?? [
      ...signers.map(s => ({ pubkey: s, isSigner: true, isWritable: false })),
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: A.season, isSigner: false, isWritable: false },
      { pubkey: A.of('Citizen', { wallet: F.citizen.wallet }), isSigner: false, isWritable: true },
      { pubkey: A.of('Holding', { p: 2, q: 0, site: 3 }), isSigner: false, isWritable: true },
    ],
    data: Uint8Array.of(tag),
  };
  if (swap) [frontier.keys[swap[0]], frontier.keys[swap[1]]] = [frontier.keys[swap[1]], frontier.keys[swap[0]]];
  const instructions = prefix ?? [cb(2, u32(17_500)), cb(3, u64(price)), cb(4, u32(1_048_576))];
  return compileMessage({ feePayer: payer, recentBlockhash: blockhash, instructions: [...instructions, frontier, ...extra] });
}

test('pins: the Season PDA and bump are found here; another season address or ruleset is refused', () => {
  assert.throws(() => io.setPin({ programId: F.program, cluster: 'localnet', seasonId: 1, seasonAddress: F.citizen.address }), e => e.code === 'PinMismatch');
  assert.throws(() => io.setPin({ programId: F.program, cluster: 'localnet', seasonId: 1, rulesetHash: '00'.repeat(32) }), e => e.code === 'RulesetMismatch');
  const p = io.setPin({ programId: F.program, cluster: 'localnet', seasonId: 1, seasonAddress: F.season, rulesetHash: RULESET_HASH });
  assert.equal(p.addresses.season, F.season);
  assert.equal(p.addresses.bump, F.bump);
  assert.deepEqual(io.scope(), { cluster: 'localnet', programId: F.program, seasonId: '1' });
});

test('message checks: a good player shape passes; every deviation is named', () => {
  io.setPin({ programId: F.program, cluster: 'localnet', seasonId: 1 });
  const ok = { feePayer, blockhash, tag: 0x40, signers: [session], cuLimit: 17_500, loadedLimit: 1_048_576, expected: expectedHarvest() };
  assert.deepEqual(io.messageProblems(harvest(), ok), []);
  const has = (msg, opts, re) => assert.ok(io.messageProblems(msg, opts).some(p => re.test(p)), `${re}: ${io.messageProblems(msg, opts)}`);
  has(harvest({ price: 1 }), ok, /CU price is not 0/);
  has(harvest({ tag: 0x51 }), { ...ok, tag: undefined }, /UseRevealRoute/);
  has(harvest({ tag: 0x61 }), { ...ok, tag: undefined }, /not a player or settle shape/);
  has(harvest({ tag: 0x41 }), ok, /not the expected 0x40/);
  has(harvest(), { ...ok, feePayer: key() }, /fee payer/);
  has(harvest(), { ...ok, blockhash: key() }, /blockhash/);
  has(harvest(), { ...ok, cuLimit: 99 }, /CU limit/);
  has(harvest(), { ...ok, loadedLimit: 65_536 }, /loaded-data limit/);
  has(harvest({ signers: [session, key()] }), ok, /signers/);
  has(harvest({ prefix: [cb(2, u32(17_500)), cb(4, u32(1_048_576)), cb(3, u64(0))] }), ok, /compute-budget prefix/);
  has(harvest({ extra: [{ programId: key(), keys: [], data: Uint8Array.of(1) }] }), ok, /instructions|unexpected program/);
  has(Uint8Array.of(1, 2, 3), ok, /unparseable/);
  // §9.4 "accounts recomputed" (integ-W2 review of W2-E): the blockhash and
  // the expected instruction are required; a swapped account list, another
  // holding, a payer account that is not the fee payer, a read-only account
  // marked writable are all named.
  has(harvest(), { ...ok, blockhash: undefined }, /no recent blockhash/);
  has(harvest(), { ...ok, expected: null }, /no expected instruction/);
  has(harvest({ swap: [3, 4] }), ok, /account 3 is .*expected/);
  has(harvest(), { ...ok, expected: expectedHarvest({ holding: { p: 2, q: 0, site: 4 } }) }, /account 4 is .*expected/);
  const A = io.pinned().addresses;
  const other = key();
  has(harvest({ keys: [
    { pubkey: session, isSigner: true, isWritable: false },
    { pubkey: other, isSigner: true, isWritable: true },
    { pubkey: A.season, isSigner: false, isWritable: false },
    { pubkey: A.of('Citizen', { wallet: F.citizen.wallet }), isSigner: false, isWritable: true },
    { pubkey: A.of('Holding', { p: 2, q: 0, site: 3 }), isSigner: false, isWritable: true },
  ] }), { ...ok, signers: [session, other] }, /payer|not a relay shape/);
  has(harvest({ keys: [
    { pubkey: session, isSigner: true, isWritable: false },
    { pubkey: feePayer, isSigner: true, isWritable: true },
    { pubkey: A.season, isSigner: false, isWritable: true },
    { pubkey: A.of('Citizen', { wallet: F.citizen.wallet }), isSigner: false, isWritable: true },
    { pubkey: A.of('Holding', { p: 2, q: 0, site: 3 }), isSigner: false, isWritable: true },
  ] }), ok, /not a relay shape: .*read-only/);
  // A settle shape: no authority signer, only the relay's.
  const settleAccounts = { payer: feePayer, season: A.season, holding: A.of('Holding', { p: 2, q: 0, site: 3 }), dest_province: key(), inputs: key(),
    slot: key(), home_province: key(), anchor_or_archive: key(), slot_beneficiary: feePayer, resolver: feePayer, holding_rent_payer: key(), settle_beneficiary: feePayer };
  const settle = frontierIx(io.pinned().programId, 'SettleTransit', settleAccounts,
    { transit_slot: 1, commit: new Uint8Array(32), seal: new Uint8Array(165), beneficiary: bs58.decode(feePayer) });
  const settleMsg = compileMessage({ feePayer, recentBlockhash: blockhash, instructions: [cb(2, u32(85_000)), cb(3, u64(0)), cb(4, u32(1_048_576)), settle] });
  assert.deepEqual(io.messageProblems(settleMsg, { feePayer, blockhash, tag: 0x54, signers: [], expected: settle }), []);
});

test('after co-signing, the message must come back byte for byte', () => {
  const msg = harvest();
  const wire = wireTransaction(msg, [new Uint8Array(64).fill(1), new Uint8Array(64).fill(2)]);
  assert.equal(io.sameMessage(msg, wire), true);
  const other = wireTransaction(harvest({ tag: 0x41 }), [new Uint8Array(64), new Uint8Array(64)]);
  assert.equal(io.sameMessage(msg, other), false);
  assert.equal(io.sameMessage(msg, Uint8Array.of(9)), false);
});

// ------------------------------------------------------------------ a fake relay on port 0
const seen = [];
const relay = http.createServer((req, res) => {
  let body = '';
  req.on('data', c => { body += c; });
  req.on('end', () => {
    seen.push({ method: req.method, url: req.url, body: body ? JSON.parse(body) : null });
    const send = (status, v) => { res.writeHead(status, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(v)); };
    if (req.url === '/gw/f/relay' && req.method === 'GET') return send(200, { feePayer, blockhash, lastValidBlockHeight: 99, programId: F.program, quota: { left: 38 } });
    if (req.url === '/gw/f/relay') return send(200, { ok: true, signature: 'sig1' });
    if (req.url === '/gw/f/reveal') return send(202, { accepted: true, track: 't1' });
    if (req.url === '/gw/f/nudge') return send(200, { queued: true, blocking: [] });
    if (req.url.startsWith('/gw/f/tx/')) return send(200, { state: 'landed', slot: 5 });
    if (req.url.startsWith('/gw/f/quota')) return send(429, { ok: false, code: 'QuotaExceeded', retryAt: 10 });
    send(404, { ok: false, code: 'NotFound' });
  });
});
await new Promise(r => relay.listen(0, '127.0.0.1', r));
after(() => relay.close());

test('relay routes: answers never reject, codes pass through, the relay\'s program is checked', async () => {
  io.setRelay(null);
  assert.equal((await io.relayInfo()).code, 'NoRelay');
  io.setRelay(`http://127.0.0.1:${relay.address().port}/gw/`);
  io.setPin({ programId: F.program, cluster: 'localnet', seasonId: 1 });
  const info = await io.relayInfo();
  assert.equal(info.ok, true);
  assert.equal(info.feePayer, feePayer);
  const sent = await io.sendTx(Uint8Array.of(1, 2, 3));
  assert.equal(sent.signature, 'sig1');
  assert.equal(seen.at(-1).body.tx, 'AQID');
  const rv = await io.reveal({ holding: 'h', transit_slot: 1, plain_b64: 'p', salt_b64: 's', ct_hash_b64: 'c' });
  assert.equal(rv.ok, true);
  assert.equal(rv.httpStatus, 202);
  assert.deepEqual(Object.keys(seen.at(-1).body).sort(), ['ct_hash_b64', 'holding', 'plain_b64', 'salt_b64', 'transit_slot']);
  assert.deepEqual(seen.at(-1).body.transit_slot, 1);
  assert.deepEqual((await io.nudge(2, 0, 40)).queued, true);
  assert.deepEqual(seen.at(-1).body, { province: [2, 0], bell: 40 });
  assert.equal((await io.txStatus('abc')).state, 'landed');
  const q = await io.quota('C1');
  assert.deepEqual([q.ok, q.code, q.httpStatus], [false, 'QuotaExceeded', 429]);
  // Another program announced by the relay is refused before anything is signed.
  io.setPin({ programId: F.season, cluster: 'localnet', seasonId: 1 });
  assert.equal((await io.relayInfo()).code, 'PinMismatch');
  io.setRelay('http://127.0.0.1:9');
  assert.equal((await io.txStatus('x')).code, 'network');
});
