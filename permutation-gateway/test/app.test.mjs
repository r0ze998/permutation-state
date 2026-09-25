// The gateway's HTTP application with stubbed connections: routing, body
// parsing, relay filters, x402 payment checks, error statuses. No network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Readable } from 'node:stream';
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { createApp } from '../src/app.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { errorResponse, RouteError } from '../src/routes/errors.mjs';
import { FaucetLimiter } from '../src/routes/faucet.mjs';
import { SendError } from '../src/send.mjs';
import { fromHex, vectors } from './vectors.mjs';

const seasonData = Buffer.from(fromHex(vectors.accounts.season.hex)); // status Finalized
const seasonId = 42n;
const chain = new ChainClient(DEFAULTS.programId, seasonId);
const crankKey = Keypair.generate();
const blockhash = Keypair.generate().publicKey.toBase58();
const keyring = new Map();
const keys = name => { if (!keyring.has(name)) keyring.set(name, Keypair.generate()); return keyring.get(name); };

function connection(overrides = {}) {
  return {
    getLatestBlockhash: async () => ({ blockhash, lastValidBlockHeight: 1234 }),
    getAccountInfo: async () => ({ data: seasonData }),
    sendRawTransaction: async () => { throw new Error('not in this test'); },
    getTransaction: async () => null,
    ...overrides,
  };
}

function app({ base = connection(), er = connection(), cluster = 'localnet', members = [] } = {}) {
  const logs = [];
  const store = { state: { seasonId: seasonId.toString(), members, mint: Keypair.generate().publicKey.toBase58() }, save() {} };
  const crank = { crank: crankKey, phase: 'registering', snapshot: null, refresh: async () => null, tickRecords: from => [{ tick: from }] };
  const cfg = { programId: DEFAULTS.programId, cluster, port: 4191, baseRpc: 'b', erRpc: 'e' };
  const handle = createApp({ cfg, base, er, store, crank, keys, log: m => logs.push(m), registry: { list: async () => [], invalidate() {} } });
  return { handle, logs };
}

async function call(handle, method, url, { body, headers = {} } = {}) {
  const req = Readable.from(body === undefined ? [] : [Buffer.from(typeof body === 'string' ? body : JSON.stringify(body))]);
  Object.assign(req, { method, url, headers });
  const res = { headers: {} };
  await new Promise(resolve => {
    res.writeHead = (status, h) => { res.status = status; Object.assign(res.headers, h); };
    res.end = data => { res.body = data; resolve(); };
    handle(req, res);
  });
  const text = res.body === undefined ? '' : Buffer.from(res.body).toString();
  return { status: res.status, headers: res.headers, json: text && res.headers['Content-Type'] === 'application/json' ? JSON.parse(text) : null };
}

const b64 = tx => tx.serialize({ requireAllSignatures: false, verifySignatures: false }).toString('base64');
function tx(ixs, feePayer = crankKey.publicKey) {
  const t = new Transaction().add(...ixs);
  t.feePayer = feePayer;
  t.recentBlockhash = blockhash;
  return t;
}

test('routing: health, CORS preflight, 404, ticks', async () => {
  const { handle } = app();
  const h = await call(handle, 'GET', '/health');
  assert.equal(h.status, 200);
  assert.deepEqual(h.json, { ok: true, phase: 'registering', season: '42' });
  assert.equal(h.headers['Access-Control-Allow-Origin'], '*');
  assert.equal((await call(handle, 'OPTIONS', '/relay')).status, 204);
  const nf = await call(handle, 'GET', '/nope');
  assert.deepEqual([nf.status, nf.json.code], [404, 'NotFound']);
  assert.deepEqual((await call(handle, 'GET', '/ticks?from=7')).json, { records: [{ tick: 7 }] });
  const w = await call(handle, 'GET', '/world.bin');
  assert.deepEqual([w.status, w.json.code], [503, 'WorldUnavailable']);
});

test('a body that is not JSON is a 400, not a 500', async () => {
  const { handle } = app();
  for (const path of ['/submit', '/gov', '/relay', '/claim-relay', '/faucet']) {
    const r = await call(handle, 'POST', path, { body: '{not json' });
    assert.deepEqual([r.status, r.json.code], [400, 'InvalidJson'], path);
  }
});

test('/submit and /gov only for hosted members', async () => {
  const { handle } = app({ members: [{ index: 4, hosted: 'external' }] });
  const r = await call(handle, 'POST', '/submit', { body: { member: 4, civ: 0, role: 'General', tick: 1 } });
  assert.deepEqual([r.status, r.json.code], [403, 'NotHosted']);
  assert.equal((await call(handle, 'POST', '/gov', { body: { member: 9 } })).status, 403);
});

test('program errors map to 4xx by class; late ones are 409 and not logged', async () => {
  const refuse = code => connection({ sendRawTransaction: async () => { throw new Error(`{"InstructionError":[1,{"Custom":${code}}]}`); } });
  for (const [code, status, name] of [[7, 403, 'Unauthorized'], [27, 409, 'TickFrozen'], [14, 409, 'WrongTick'], [24, 400, 'InvalidParams'], [21, 409, 'NothingToClaim']]) {
    const { handle, logs } = app({ er: refuse(code), members: [{ index: 0, hosted: 'ai', key: 0 }] });
    const r = await call(handle, 'POST', '/submit', { body: { member: 0, civ: 0, role: 'General', tick: 1, orders: [] } });
    assert.deepEqual([r.status, r.json.code], [status, name], name);
    assert.equal(logs.length, name === 'TickFrozen' || name === 'WrongTick' ? 0 : 1, name);
  }
  assert.equal(errorResponse(new SendError('x', 'confirmation timed out')).status, 500);
  assert.equal(errorResponse(new Error('custom program error: 0x1b')).status, 409);
  assert.equal(errorResponse(new RouteError(429, 'slow down', 'FaucetBusy')).body.code, 'FaucetBusy');
});

test('/relay: hands out the blockhash expiry; accepts only one SubmitOrders/SubmitGov paid by the gateway', async () => {
  const { handle } = app();
  const g = await call(handle, 'GET', '/relay');
  assert.deepEqual([g.json.blockhash, g.json.lastValidBlockHeight, g.json.feePayer], [blockhash, 1234, crankKey.publicKey.toBase58()]);
  const session = Keypair.generate();
  const submit = chain.submitOrders({ signer: session.publicKey, civ: 0, role: 'General', tick: 1, decisionDigest: new Uint8Array(32), orders: [] });
  const drain = SystemProgram.transfer({ fromPubkey: crankKey.publicKey, toPubkey: session.publicKey, lamports: 1 });
  const cases = {
    'not a transaction': 'AAAA',
    'extra instruction': b64(tx([...submit, drain])),
    'someone else pays': b64(tx(submit, session.publicKey)),
    'another instruction': b64(tx(chain.claim({ wallet: session.publicKey, dest: session.publicKey, mint: session.publicKey }))),
  };
  for (const [name, t] of Object.entries(cases)) {
    const r = await call(handle, 'POST', '/relay', { body: { tx: t } });
    assert.equal(r.status, 400, name);
    assert.equal(r.json.code, name === 'not a transaction' ? 'InvalidTransaction' : 'RelayRejected', name);
  }
  // A claim for another season is refused by the claim relay.
  const other = new ChainClient(DEFAULTS.programId, seasonId + 1n);
  const w = Keypair.generate().publicKey;
  const r = await call(handle, 'POST', '/claim-relay', { body: { tx: b64(tx(other.claim({ wallet: w, dest: w, mint: w }))) } });
  assert.deepEqual([r.status, r.json.code], [400, 'RelayRejected']);
});

test('/x402/join: closed season, malformed payments are 400, invalid ones 402', async () => {
  const { handle } = app();
  const closed = await call(handle, 'POST', '/x402/join', { body: {} });
  assert.deepEqual([closed.status, closed.json.code], [409, 'RegistrationClosed']);
  const header = v => ({ 'x-payment': Buffer.from(typeof v === 'string' ? v : JSON.stringify(v)).toString('base64') });
  const notJson = await call(handle, 'POST', '/x402/join', { body: {}, headers: { 'x-payment': '%%%' } });
  assert.equal(notJson.status, 400);
  const notTx = await call(handle, 'POST', '/x402/join', { body: {}, headers: header({ scheme: 'exact', payload: { transaction: 'AAAA' } }) });
  assert.deepEqual([notTx.status, notTx.json.code], [400, 'InvalidPayment']);
  // A well-formed payment into another season's vault.
  const wallet = Keypair.generate();
  const other = new ChainClient(DEFAULTS.programId, seasonId + 1n);
  const t = tx(other.register({ wallet: wallet.publicKey, feePayer: crankKey.publicKey, civ: 1, walletToken: wallet.publicKey, mint: wallet.publicKey, name: 'M', session: wallet.publicKey }));
  t.partialSign(wallet);
  const bad = await call(handle, 'POST', '/x402/join', { body: { civ: 2 }, headers: header({ scheme: 'exact', network: 'solana-localnet', payload: { transaction: b64(t) } }) });
  assert.equal(bad.status, 402);
  assert.match(bad.json.error, /wrong season or vault/);
  assert.match(bad.json.error, /another nation/);
  assert.equal(bad.json.accepts[0].payTo, chain.vault.toBase58());
});

test('/faucet: devnet and localnet only, owner must be a key', async () => {
  const main = await call(app({ cluster: 'mainnet' }).handle, 'POST', '/faucet', { body: { owner: Keypair.generate().publicKey.toBase58() } });
  assert.deepEqual([main.status, main.json.code], [403, 'FaucetDisabled']);
  const bad = await call(app().handle, 'POST', '/faucet', { body: { owner: 'nope' } });
  assert.deepEqual([bad.status, bad.json.code], [400, 'InvalidOwner']);
});

test('FaucetLimiter: cooldown per owner, a cap per hour of requests, failed mints give the slot back', () => {
  let now = 0;
  const f = new FaucetLimiter({ cooldownMs: 10 * 60_000, perHour: 3, now: () => now });
  f.acquire('a');
  assert.throws(() => f.acquire('a'), e => e.status === 429 && e.code === 'FaucetCooldown');
  const release = f.acquire('b');
  release(); // the mint failed: b may retry at once, and it does not count
  release();
  f.acquire('b');
  f.acquire('c');
  assert.throws(() => f.acquire('d'), e => e.code === 'FaucetBusy', 'three grants this hour');
  now = 60 * 60_000; // the first grants are an hour old: they no longer count
  f.acquire('a');
  now = 71 * 60_000; // a's cooldown is over
  f.acquire('a');
  f.acquire('x');
  assert.throws(() => f.acquire('y'), e => e.code === 'FaucetBusy', 'the hourly cap counts requests (a twice), not distinct owners');
});

test('/x402/join: a valid payment settles, is saved at once and answered with X-PAYMENT-RESPONSE', async () => {
  const wallet = Keypair.generate();
  const memberPda = chain.member(wallet.publicKey);
  const memberData = Buffer.from(fromHex(vectors.accounts.member.hex)); // member 2 of civ 1
  const registering = Buffer.from(seasonData);
  registering[130] = 0; // SeasonStatus::Registering (the status byte of the sample season)
  const sent = [];
  const base = connection({
    getAccountInfo: async key => (key.equals(memberPda) ? { data: memberData } : { data: registering }),
    sendRawTransaction: async bytes => { sent.push(Transaction.from(bytes)); return 'sig'.padEnd(64, '1'); },
    getSignatureStatuses: async () => ({ value: [{ confirmationStatus: 'confirmed', err: null }] }),
    getTransaction: async () => ({ meta: { logMessages: [], computeUnitsConsumed: 1 } }),
    getBlockHeight: async () => 1,
  });
  const saves = [];
  const store = { state: { seasonId: seasonId.toString(), members: [], mint: '' }, save() { saves.push(JSON.parse(JSON.stringify(this.state))); } };
  const crank = { crank: crankKey, phase: 'registering' };
  const invalidated = [];
  const handle = createApp({ cfg: { programId: DEFAULTS.programId, cluster: 'localnet', port: 4191 }, base, er: connection(), store, crank, keys, log: () => {},
    registry: { list: async () => [], invalidate: () => invalidated.push(1) } });
  const first = await call(handle, 'POST', '/x402/join', { body: { civ: 1, name: 'M' } });
  assert.equal(first.status, 402);
  const req = first.json.accepts[0];
  assert.deepEqual([req.scheme, req.network, req.maxAmountRequired, req.payTo], ['exact', 'solana-localnet', '10000000', chain.vault.toBase58()]);
  const t = tx(chain.register({ wallet: wallet.publicKey, feePayer: crankKey.publicKey, civ: 1, walletToken: Keypair.generate().publicKey, mint: Keypair.generate().publicKey,
    name: 'M', session: Keypair.generate().publicKey }));
  t.partialSign(wallet);
  const payment = Buffer.from(JSON.stringify({ x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: b64(t) } })).toString('base64');
  const r = await call(handle, 'POST', '/x402/join', { body: { civ: 1, name: 'M' }, headers: { 'x-payment': payment } });
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.deepEqual([r.json.member, r.json.civ, r.json.nation], [2, 1, 'Borealis']);
  assert.equal(sent.length, 1);
  assert.ok(sent[0].signatures.every(s => s.signature), 'the facilitator co-signed');
  assert.equal(saves.length, 1, 'the new member is written to the state file immediately');
  assert.deepEqual(saves[0].members, [{ index: 2, civ: 1, name: 'Hypatia-アステル', kind: 1, hosted: 'external', wallet: wallet.publicKey.toBase58(), session: new PublicKey(Buffer.alloc(32, 8)).toBase58() }]);
  assert.equal(invalidated.length, 1);
  const receipt = JSON.parse(Buffer.from(r.headers['X-PAYMENT-RESPONSE'], 'base64').toString());
  assert.deepEqual([receipt.success, receipt.payer], [true, wallet.publicKey.toBase58()]);
});
