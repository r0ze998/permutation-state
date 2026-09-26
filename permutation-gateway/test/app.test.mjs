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
import { SendError } from '../src/send.mjs';
import { fromHex, vectors } from './vectors.mjs';
import { SealedStore } from '../src/sealed.mjs';

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

/** The operator token header (the game server's). */
const OP = { authorization: 'Bearer op-token' };

function app({ base = connection(), er = connection(), cluster = 'localnet', members = [], registryMembers = [], roster, talk } = {}) {
  const logs = [];
  const store = { state: { seasonId: seasonId.toString(), members, mint: Keypair.generate().publicKey.toBase58(), ...(roster ? { roster } : {}), ...(talk ? { talk } : {}) }, save() {} };
  const crank = { crank: crankKey, phase: 'registering', snapshot: null, refresh: async () => null, tickRecords: from => [{ tick: from }], sealed: new SealedStore() };
  const cfg = { programId: DEFAULTS.programId, cluster, port: 4191, baseRpc: 'b', erRpc: 'e', operatorToken: 'op-token' };
  const handle = createApp({ cfg, base, er, store, crank, keys, log: m => logs.push(m), registry: { list: async () => registryMembers, invalidate() {} } });
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
    const r = await call(handle, 'POST', path, { body: '{not json', headers: OP });
    assert.deepEqual([r.status, r.json.code], [400, 'InvalidJson'], path);
  }
});

test('/submit and /gov: only the operator, and only for hosted members', async () => {
  const { handle } = app({ members: [{ index: 4, hosted: 'external' }, { index: 5, hosted: 'ai', key: 5 }] });
  // Without the operator token nobody learns anything about a member.
  for (const member of [4, 5, 9]) {
    const r = await call(handle, 'POST', '/submit', { body: { member, civ: 0, role: 'General', tick: 1 } });
    assert.deepEqual([r.status, r.json.code], [403, 'OperatorOnly']);
    assert.deepEqual((await call(handle, 'POST', '/gov', { body: { member } })).json.code, 'OperatorOnly');
  }
  const r = await call(handle, 'POST', '/submit', { body: { member: 4, civ: 0, role: 'General', tick: 1 }, headers: OP });
  assert.deepEqual([r.status, r.json.code], [403, 'NotHosted']);
  assert.equal((await call(handle, 'POST', '/gov', { body: { member: 9 }, headers: OP })).status, 403);
});

test('/roster shows only announced AI members until the roster is revealed; /operator/roster needs the token', async () => {
  const members = [{ index: 0, civ: 0, hosted: 'external' }, { index: 1, civ: 0, hosted: 'ai', salt: 'aa'.repeat(32) }, { index: 2, civ: 1, hosted: 'ai', salt: 'bb'.repeat(32) }];
  const { handle } = app({ members, roster: { aiCount: 2, bountyEach: '5000000', bond: '40000000', revealed: false } });
  const pub = await call(handle, 'GET', '/roster');
  assert.deepEqual([pub.json.aiCount, pub.json.ai], [2, []]);
  assert.equal((await call(handle, 'GET', '/operator/roster')).status, 403);
  const op = await call(handle, 'GET', '/operator/roster', { headers: OP });
  assert.deepEqual(op.json.members.map(m => m.hosted), ['external', 'ai', 'ai']);
  assert.deepEqual(op.json.ai.map(m => m.member), [1, 2]);
  assert.equal((await call(handle, 'POST', '/roster/announce', { body: { member: 0 }, headers: OP })).status, 404, 'not an AI');
  assert.equal((await call(handle, 'POST', '/roster/announce', { body: { member: 2 } })).status, 403);
  assert.equal((await call(handle, 'POST', '/roster/announce', { body: { member: 2 }, headers: OP })).status, 200);
  assert.deepEqual((await call(handle, 'GET', '/roster')).json.ai, [{ member: 2, civ: 1, salt: 'bb'.repeat(32) }]);
});

test('/talk: a member-signed message is kept and public; a forged one is refused; hosted members sign through the operator', async () => {
  const { talkBytes, signTalk } = await import('../src/talk.mjs');
  const session = Keypair.generate();
  const hostedSession = keys('member7-session');
  const registryMembers = [{ index: 3, civ: 1, session: session.publicKey.toBase58() }, { index: 7, civ: 2, session: hostedSession.publicKey.toBase58() }];
  const { handle } = app({ registryMembers, members: [{ index: 7, civ: 2, hosted: 'ai', key: 7 }] });
  const text = 'Peace for 4 USDC?';
  const sig = Buffer.from(signTalk(talkBytes({ season: seasonId, tick: 0, member: 3, to: { civ: 2 }, text }), session)).toString('hex');
  const ok = await call(handle, 'POST', '/talk', { body: { member: 3, to: { civ: 2 }, text, signature: sig } });
  assert.deepEqual([ok.status, ok.json.id], [200, 0]);
  const forged = await call(handle, 'POST', '/talk', { body: { member: 3, to: { civ: 2 }, text: 'something else', signature: sig } });
  assert.deepEqual([forged.status, forged.json.code], [400, 'TalkRefused']);
  assert.equal((await call(handle, 'POST', '/talk', { body: { member: 7, text: 'hi' } })).status, 403, 'unsigned needs the operator');
  assert.equal((await call(handle, 'POST', '/talk', { body: { member: 7, text: 'Deal.' }, headers: OP })).status, 200);
  const all = await call(handle, 'GET', '/talk');
  assert.deepEqual(all.json.messages.map(m => [m.member, m.text]), [[3, text], [7, 'Deal.']]);
});

test('/talk refuses a message for a tick whose messages are already anchored (one PS_TALK root per tick)', async () => {
  const { talkBytes, signTalk } = await import('../src/talk.mjs');
  const session = Keypair.generate();
  const talk = [{ id: 0, tick: 0, member: 3, to: null, text: 'earlier', bytes: '00', signature: '00', anchored: { signature: 'x', root: '00'.repeat(32) } }];
  const { handle } = app({ registryMembers: [{ index: 3, civ: 1, session: session.publicKey.toBase58() }], talk });
  const text = 'too late';
  const signature = Buffer.from(signTalk(talkBytes({ season: seasonId, tick: 0, member: 3, to: null, text }), session)).toString('hex');
  const r = await call(handle, 'POST', '/talk', { body: { member: 3, text, tick: 0, signature } });
  assert.deepEqual([r.status, r.json.code, r.json.error], [400, 'TalkRefused', 'tick 0 is already anchored']);
});

test('/roster/announce works before the roster record exists', async () => {
  const { handle } = app({ members: [{ index: 1, civ: 0, hosted: 'ai', salt: 'aa'.repeat(32) }] });
  assert.equal((await call(handle, 'POST', '/roster/announce', { body: { member: 1 }, headers: OP })).status, 200);
});

test('a missing season account is a 503, not a crash', async () => {
  const { handle } = app({ base: connection({ getAccountInfo: async () => null }) });
  for (const url of ['/season', '/history', '/claim-relay']) {
    const r = await call(handle, 'GET', url);
    assert.deepEqual([r.status, r.json.code, r.json.error], [503, 'WorldUnavailable', 'season account not found'], url);
  }
});

test('program errors map to 4xx by class; late ones are 409 and not logged', async () => {
  const refuse = code => connection({ sendRawTransaction: async () => { throw new Error(`{"InstructionError":[1,{"Custom":${code}}]}`); } });
  for (const [code, status, name] of [[7, 403, 'Unauthorized'], [27, 409, 'TickFrozen'], [14, 409, 'WrongTick'], [24, 400, 'InvalidParams'], [21, 409, 'NothingToClaim']]) {
    const { handle, logs } = app({ er: refuse(code), members: [{ index: 0, hosted: 'ai', key: 0 }] });
    const r = await call(handle, 'POST', '/submit', { body: { member: 0, civ: 0, role: 'General', tick: 1, orders: [], digest: '09'.repeat(32) }, headers: OP });
    assert.deepEqual([r.status, r.json.code], [status, name], name);
    assert.equal(logs.length, name === 'TickFrozen' || name === 'WrongTick' ? 0 : 1, name);
  }
  // Checked before anything is sealed: a rationale, and orders of the office.
  const { handle } = app({ members: [{ index: 0, hosted: 'ai', key: 0 }] });
  const noDigest = await call(handle, 'POST', '/submit', { body: { member: 0, civ: 0, role: 'General', tick: 1, orders: [] }, headers: OP });
  assert.deepEqual([noDigest.status, noDigest.json.code], [400, 'MissingRationale']);
  const research = await call(handle, 'POST', '/submit', { body: { member: 0, civ: 0, role: 'General', tick: 1, digest: '09'.repeat(32), orders: [{ type: 'SetResearch', techs: ['Writing'] }] }, headers: OP });
  assert.deepEqual([research.status, research.json.code], [400, 'WrongOffice']);
  assert.equal(errorResponse(new SendError('x', 'confirmation timed out')).status, 500);
  assert.equal(errorResponse(new Error('custom program error: 0x1b')).status, 409);
  assert.equal(errorResponse(new RouteError(429, 'slow down', 'FaucetBusy')).body.code, 'FaucetBusy');
});

test('/relay: hands out the blockhash expiry; accepts only one CommitOrders/RevealOrders/SubmitGov paid by the gateway', async () => {
  const { handle } = app();
  const g = await call(handle, 'GET', '/relay');
  assert.deepEqual([g.json.blockhash, g.json.lastValidBlockHeight, g.json.feePayer], [blockhash, 1234, crankKey.publicKey.toBase58()]);
  const session = Keypair.generate();
  const submit = chain.commitOrders({ signer: session.publicKey, civ: 0, role: 'General', tick: 1, commitment: new Uint8Array(32).fill(1) });
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
  const closed = await call(app().handle, 'POST', '/x402/join', { body: {} });
  assert.deepEqual([closed.status, closed.json.code], [409, 'RegistrationClosed']);
  // The rest with the season still registering.
  const registering = Buffer.from(seasonData);
  registering[130] = 0; // SeasonStatus::Registering (the status byte of the sample season)
  const { handle } = app({ base: connection({ getAccountInfo: async () => ({ data: registering }) }) });
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

test('/gov sends a member\'s actions in as few packet-sized transactions as fit', async () => {
  const sent = [];
  const er = connection({
    sendRawTransaction: async bytes => { sent.push(Transaction.from(bytes)); return 'sig'.padEnd(64, '1'); },
    getSignatureStatuses: async () => ({ value: [{ confirmationStatus: 'confirmed', err: null }] }),
    getTransaction: async () => ({ meta: { logMessages: [], computeUnitsConsumed: 1 } }),
    getBlockHeight: async () => 1,
  });
  const store = { state: { seasonId: seasonId.toString(), members: [{ index: 0, civ: 2, hosted: 'ai', key: 0 }], mint: '' }, save() {} };
  const crank = { crank: crankKey, phase: 'playing', snapshot: null, refresh: async () => null, tickRecords: () => [] };
  const handle = createApp({ cfg: { programId: DEFAULTS.programId, cluster: 'localnet', port: 4191, operatorToken: 'op-token' }, base: connection(), er, store, crank, keys, log: () => {},
    registry: { list: async () => [{ index: 0, civ: 2 }], invalidate() {} } });
  const program = t => t.instructions.filter(i => i.programId.toBase58() === DEFAULTS.programId).length;

  // A vote window: four votes, one transaction.
  const votes = ['General', 'Steward', 'Science', 'Diplomat'].map(role => ({ type: 'Vote', role, candidate: 0 }));
  const r = await call(handle, 'POST', '/gov', { body: { member: 0, actions: votes }, headers: OP });
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.deepEqual([sent.length, program(sent[0]), r.json.signatures.length], [1, 4, 1]);

  // Large proposals do not fit together: several transactions, each within a packet.
  sent.length = 0;
  const orders = Array.from({ length: 6 }, () => ({ type: 'MoveUnit', unit: 1, path: Array.from({ length: 8 }, (_, i) => [i, -i]) }));
  const proposals = Array.from({ length: 4 }, () => ({ type: 'Propose', role: 'General', orders }));
  const p = await call(handle, 'POST', '/gov', { body: { member: 0, actions: proposals }, headers: OP });
  assert.equal(p.status, 200, JSON.stringify(p.json));
  assert.ok(sent.length > 1, `${sent.length} transactions`);
  assert.equal(sent.reduce((n, t) => n + program(t), 0), 4);
  for (const t of sent) assert.ok(t.serialize({ requireAllSignatures: false }).length <= 1232);

  const tooMany = Array.from({ length: 9 }, () => votes[0]);
  const x = await call(handle, 'POST', '/gov', { body: { member: 0, actions: tooMany }, headers: OP });
  assert.deepEqual([x.status, x.json.code], [400, 'TooManyActions']);
});
