// The web client's in-game chain actions (permutation-server/web/chainplay.mjs)
// against a fake gateway whose /seal is the gateway's own route and whose
// /relay applies the gateway's own shape check (src/routes/*.mjs): per held
// office the local copy is written first, the seal receipt goes out before
// the commitment and a refused seal sends none; the batch commits to a
// decision digest under policy officer@2 and carries the office's pending
// rationale reveals; governance goes out as SubmitGov × ≤ 8 per packet, and
// waits for the next tick when the commitments closed; talk is signed.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { createPublicKey, verify as nodeVerify, randomBytes } from 'node:crypto';
import * as chainplay from '../../permutation-server/web/chainplay.mjs';
import * as chainio from '../../permutation-server/web/chainio.mjs';
import * as book from '../../permutation-server/web/sealbook.mjs';
import { S } from '../../permutation-server/web/state.mjs';
import { keyFromSeed } from '../../permutation-server/web/session.mjs';
import { sealRoutes, SealCounter } from '../src/routes/seal.mjs';
import { relayShape } from '../src/routes/relay.mjs';
import { RouteError } from '../src/routes/errors.mjs';
import { pda } from '../client/src/player.mjs';
import { encode as base58 } from '../client/src/base58.mjs';
import { fromBase64, fromHex, toHex } from '../client/src/bytes.mjs';
import { decisionDigest } from '../client/src/decision.mjs';
import { IX_TAG, orderCommitment, ROLES } from '../client/src/codec.mjs';
import { talkBytes } from '../client/src/talk.mjs';
import { PACKET_BYTES, parseTransaction as decodeWire, pubkeyBytes } from '../client/src/solana-tx.mjs';

const SPKI = Buffer.from('302a300506032b6570032100', 'hex');
const verifies = (key, msg, sig) => nodeVerify(null, Buffer.from(msg), createPublicKey({ key: Buffer.concat([SPKI, Buffer.from(pubkeyBytes(key))]), format: 'der', type: 'spki' }), Buffer.from(sig));
const key = () => base58(randomBytes(32));

const PROGRAM = key(), CRANK = key(), BLOCKHASH = key();
const SEASON = '7', CIV = 2, ME = 3, TICK = 5;
const OBS = toHex(randomBytes(32));

let session;

/**
 * A fake gateway behind a stubbed fetch. /seal runs the gateway's route with
 * a snapshot where member ME holds `mine` offices of nation CIV; /relay
 * checks the shape with relayShape and the session signature, then answers
 * `relay(kind, tx)` (default ok).
 */
function fakeGateway({ mine = ['Science', 'Diplomat'], relay = () => ({ ok: true }), decisions = { open: TICK, records: [] }, tickPhase = 'commit' } = {}) {
  const calls = [], sealed = [], relayed = [], problems = [];
  const keys = ROLES.map(r => (mine.includes(r) ? pubkeyBytes(session.publicKey) : randomBytes(32)));
  const officers = ROLES.map(r => (mine.includes(r) ? ME : 99));
  const snap = { header: { meta: { finished: false, frozen: false, revealing: false } }, nations: [] };
  snap.nations[CIV] = { openTick: TICK, officers, keys };
  const ctx = {
    crank: { phase: 'playing', fresh: async () => snap, sealed: { put: (batch, salt, commitment) => sealed.push({ batch, salt, commitment }) } },
    store: { state: { seasonId: SEASON } }, limiter: { check() {} }, seals: new SealCounter(),
  };
  const nations = new Map([0, 1, 2, 3, 4, 5].map(c => [pda.nation(PROGRAM, SEASON, c), c]));
  const json = (status, body) => ({ ok: status < 300, status, json: async () => body });
  globalThis.fetch = async (url, init = {}) => {
    const method = init.method ?? 'GET';
    const path = String(url).replace(/^http:\/\/gw/, '');
    const body = init.body ? JSON.parse(init.body) : null;
    calls.push({ method, path, body, bookAtCall: structuredClone(chainplay.playBook()) });
    if (method === 'GET' && path === '/season') return json(200, { programId: PROGRAM, season: { seasonId: SEASON, crank: CRANK }, members: [] });
    if (method === 'GET' && path === '/relay') return json(200, { feePayer: CRANK, blockhash: BLOCKHASH, lastValidBlockHeight: 100 });
    if (method === 'GET' && path === '/tick') return json(200, { tick: TICK, phase: tickPhase, nations: [{ civ: CIV, committed: [false, false, true, false], revealed: [false, false, false, false] }] });
    if (method === 'GET' && path.startsWith('/api/decisions')) return json(200, decisions);
    if (method === 'POST' && path === '/seal') {
      try {
        return json(200, (await sealRoutes['POST /seal'](ctx, { json: async () => body })).body);
      } catch (e) {
        if (!(e instanceof RouteError)) throw e;
        return json(e.status, { error: e.message, code: e.code });
      }
    }
    if (method === 'POST' && path === '/relay') {
      const tx = decodeWire(fromBase64(body.tx));
      const shape = relayShape(tx, { programId: PROGRAM, crank: CRANK, nations });
      if (shape.problem) { problems.push(shape.problem); return json(400, { error: shape.problem, code: 'RelayRejected' }); }
      // (Assertions inside fetch would surface as network errors: collect them.)
      if (shape.signer !== session.publicKey) problems.push('signer');
      if (!verifies(session.publicKey, tx.message, tx.signatures[1])) problems.push('signature');
      if (tx.recentBlockhash !== BLOCKHASH) problems.push('blockhash');
      if (1 + 64 * 2 + tx.message.length > PACKET_BYTES) problems.push('size');
      if (problems.length) return json(400, { error: problems.join(), code: 'TestProblem' });
      relayed.push({ shape, tx });
      const r = relay(shape.kind, tx);
      return r.ok ? json(200, { ok: true, signature: `sig${relayed.length}` }) : json(r.status ?? 409, { error: r.code, code: r.code });
    }
    if (method === 'POST' && path === '/talk') return json(200, { ok: true, id: 0, tick: body.tick });
    return json(404, { error: `no route ${method} ${path}`, code: 'NoRoute' });
  };
  return { calls, sealed, relayed, keys, problems };
}

/** A view of nation CIV at TICK where member ME holds `mine`. */
function view(mine = ['Science', 'Diplomat'], phase = 'commit') {
  return {
    tick: TICK, tickSeconds: 30, chain: { seasonId: SEASON }, chainPhase: phase, decision: { tick: TICK, obsRoot: OBS },
    gov: { offices: ROLES.map(role => ({ role, holder: mine.includes(role) ? { id: ME } : { id: 99 } })) },
  };
}

beforeEach(async () => {
  const k = await keyFromSeed(randomBytes(32));
  session = { ...k, wallet: key() };
  chainio.setGateway('http://gw');
  chainio.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON, entryFee: 10n });
  Object.assign(S, { view: view(), session, memberId: ME, myCiv: CIV, watch: null, chainSeals: null, govQueue: [] });
  // Each test starts with an empty book.
  const b = chainplay.playBook();
  b.sealed = {};
  b.decisions = [];
});

const research = { type: 'SetResearch', techs: ['Writing'] };
const envoy = { type: 'SendEnvoy', cityState: 1, influence: 10 };

test('each office: the local copy first, the seal receipt, then the commitment, all offices at once', async () => {
  const g = fakeGateway();
  await chainio.season();
  const results = await chainplay.commitOffices([
    { role: 'Science', orders: [research], adopt: [4] },
    { role: 'Diplomat', orders: [envoy], adopt: [] },
  ], { rationale: 'why not' });
  assert.deepEqual(results.map(r => [r.role, r.ok]), [['Science', true], ['Diplomat', true]]);
  // Both offices sealed (the gateway kept both batches) and committed on chain.
  assert.equal(g.sealed.length, 2);
  assert.equal(g.relayed.length, 2);
  for (const role of ['Science', 'Diplomat']) {
    const i = ROLES.indexOf(role);
    const seal = g.calls.findIndex(c => c.path === '/seal' && c.body.role === role);
    const commit = g.relayed.findIndex(r => r.shape.commit.role === i);
    const posts = g.calls.filter(c => c.method === 'POST' && (c.path === '/seal' || c.path === '/relay'));
    assert.ok(seal >= 0 && commit >= 0);
    // The seal of an office goes out before its commitment.
    const sealAt = posts.findIndex(c => c.path === '/seal' && c.body.role === role);
    const commitAt = posts.findIndex(c => c.path === '/relay' && decodeWire(fromBase64(c.body.tx)).instructions[1].data[1] === i);
    assert.ok(sealAt < commitAt, `${role}: seal before commit`);
    // The book held the batch before the seal went out.
    const before = g.calls[seal].bookAtCall.sealed[`${TICK}:${role}`];
    assert.equal(before?.[0]?.state, 'sealing');
    const kept = g.sealed.find(s => s.batch.role === role);
    assert.equal(before[0].commitment, toHex(kept.commitment));
    // The commitment on chain is the one the gateway computed from the batch it keeps.
    const { shape, tx } = g.relayed[commit];
    assert.deepEqual(shape.commit, { civ: CIV, role: i, tick: TICK });
    assert.equal(tx.instructions[1].data[0], IX_TAG.commitOrders);
    assert.equal(toHex(tx.instructions[1].data.subarray(4, 36)), toHex(kept.commitment));
    assert.deepEqual(orderCommitment(kept.batch, kept.salt), kept.commitment);
    // The batch: this member, this tick, a decision digest under the neutral policy.
    assert.equal(kept.batch.member, ME);
    assert.equal(kept.batch.civ, CIV);
    const d = chainplay.playBook().decisions.find(x => x.role === role);
    assert.equal(d.policy, 'officer@2');
    assert.equal(d.text, 'why not');
    assert.equal(decisionDigest({ tick: TICK, obsRoot: OBS, policy: 'officer@2', salt: d.salt, text: 'why not' }), toHex(kept.batch.decisionDigest));
  }
  assert.deepEqual(g.sealed.find(s => s.batch.role === 'Science').batch.adopt, [4]);
  assert.deepEqual(g.sealed.find(s => s.batch.role === 'Science').batch.orders, [research]);
  // The dock: committed, and after a reload the drafts come back.
  const states = chainplay.officeStates();
  assert.ok(states.every(s => s.committed && s.digest));
  assert.equal(chainplay.turnEnded(), true);
  assert.deepEqual(chainplay.restore(), { orders: [research, envoy], adopt: { Science: [4] }, rationale: 'why not' });
});

test('a refused seal sends no commitment; the other office still goes', async () => {
  const g = fakeGateway({ mine: ['Science'] }); // the gateway does not see ME as the diplomat
  await chainio.season();
  S.view = view(['Science', 'Diplomat']);
  const results = await chainplay.commitOffices([{ role: 'Science', orders: [], adopt: [] }, { role: 'Diplomat', orders: [envoy], adopt: [] }]);
  const dip = results.find(r => r.role === 'Diplomat');
  assert.equal(dip.ok, false);
  assert.equal(dip.stage, 'seal');
  assert.equal(dip.code, 'NotOfficer');
  assert.equal(results.find(r => r.role === 'Science').ok, true);
  assert.deepEqual(g.relayed.map(r => r.shape.commit.role), [ROLES.indexOf('Science')]);
  const s = chainplay.officeStates().find(x => x.role === 'Diplomat');
  assert.equal(s.committed, false);
  assert.equal(s.failed.code, 'NotOfficer');
  assert.equal(chainplay.turnEnded(), false);
});

test('a refused commitment is kept as failed (the seal alone reveals nothing: its commitment is not on chain)', async () => {
  const g = fakeGateway({ relay: () => ({ ok: false, code: 'TickFrozen' }) });
  await chainio.season();
  const [r] = await chainplay.commitOffices([{ role: 'Science', orders: [research], adopt: [] }]);
  assert.equal(r.ok, false);
  assert.equal(r.stage, 'commit');
  assert.equal(r.code, 'TickFrozen');
  assert.equal(g.sealed.length, 1);
  assert.equal(book.candidates(chainplay.playBook(), TICK, 'Science')[0].state, 'failed');
});

test('earlier decisions that landed are revealed in the office\'s next batch, and dropped once revealed', async () => {
  const g = fakeGateway();
  await chainio.season();
  const b = chainplay.playBook();
  b.decisions.push({ tick: 3, role: 'Science', civ: CIV, policy: 'officer@2', salt: 'ab'.repeat(16), text: 'three', digest: 'd3', obsRoot: OBS, status: 'landed' });
  b.decisions.push({ tick: 4, role: 'Science', civ: CIV, policy: 'officer@2', salt: 'cd'.repeat(16), text: 'four', digest: 'd4', obsRoot: OBS, status: 'sealed' });
  await chainplay.commitOffices([{ role: 'Science', orders: [research], adopt: [] }]);
  const kept = g.sealed[0].batch;
  assert.deepEqual(kept.orders, [research, { type: 'RevealRationale', tick: 3, policy: 'officer@2', salt: 'ab'.repeat(16), text: 'three' }]);
  assert.equal(b.decisions.find(d => d.digest === 'd3').sentIn, TICK);
  // A second commit in the same tick carries it again (only one of them lands).
  await chainplay.commitOffices([{ role: 'Science', orders: [], adopt: [] }]);
  assert.equal(g.sealed[1].batch.orders[0].type, 'RevealRationale');
  // The next tick: the play server shows tick 3 revealed and tick 5's batch landed.
  const landed = toHex(g.sealed[1].batch.decisionDigest);
  fakeGateway({ decisions: { open: TICK + 1, records: [
    { tick: TICK, civ: CIV, role: 'Science', digest: landed, reveal: null },
    { tick: 4, civ: CIV, role: 'General', digest: 'x', reveal: null },
    { tick: 3, civ: CIV, role: 'Science', digest: 'd3', reveal: { at: TICK, text: 'three' } },
  ] } });
  await chainplay.reconcileDecisions();
  assert.deepEqual(b.decisions.map(d => [d.tick, d.status]), [[TICK, 'landed']], 'd3 revealed, d4 never landed, the other tick-5 candidate did not land');
});

test('nothing goes out when a batch does not fit, after 8 commitments of an office, outside the commit phase, or without the key', async () => {
  const g = fakeGateway();
  await chainio.season();
  const long = Array.from({ length: 40 }, (_, i) => ({ type: 'MoveUnit', unit: i, path: Array.from({ length: 4 }, (_, j) => [j, -j]) }));
  const [big] = await chainplay.commitOffices([{ role: 'Science', orders: long, adopt: [] }]);
  assert.equal(big.code, 'BatchTooLarge');
  for (let i = 0; i < book.MAX_COMMITS; i++) await chainplay.commitOffices([{ role: 'Science', orders: [], adopt: [] }]);
  const [capped] = await chainplay.commitOffices([{ role: 'Science', orders: [], adopt: [] }]);
  assert.equal(capped.code, 'TooManyCommits');
  assert.equal(g.sealed.length, book.MAX_COMMITS);
  S.view = view(undefined, 'reveal');
  const [late] = await chainplay.commitOffices([{ role: 'Diplomat', orders: [], adopt: [] }]);
  assert.equal(late.code, 'WrongPhase');
  S.view = view();
  S.view.decision = { tick: TICK - 1, obsRoot: OBS };
  const [noRoot] = await chainplay.commitOffices([{ role: 'Diplomat', orders: [], adopt: [] }]);
  assert.equal(noRoot.code, 'NoObservation');
  S.view = view();
  const [moved] = await chainplay.commitOffices([{ role: 'Diplomat', orders: [], adopt: [] }], { tick: TICK - 1 });
  assert.equal(moved.code, 'WrongTick', 'checked for a tick that has passed');
  S.session = null;
  const [keyless] = await chainplay.commitOffices([{ role: 'Diplomat', orders: [], adopt: [] }]);
  assert.equal(keyless.code, 'NoKey');
  assert.equal(chainplay.canSign(), false);
  assert.equal(g.sealed.length, book.MAX_COMMITS);
});

test('governance: SubmitGov × ≤ 8 per transaction in order, proposals split to fit; late ones wait for the next tick', async () => {
  const g = fakeGateway();
  await chainio.season();
  const votes = Array.from({ length: 10 }, (_, i) => ({ type: 'Vote', role: 'General', candidate: i }));
  const r = await chainplay.sendGov(votes);
  assert.equal(r.ok, true);
  assert.deepEqual(g.relayed.map(x => [x.shape.kind, x.tx.instructions.length - 1]), [['gov', 8], ['gov', 2]]);
  for (const { tx } of g.relayed) for (const ix of tx.instructions.slice(1)) assert.equal(ix.data[0], IX_TAG.submitGov);
  // member ME at data[1..5], then the Vote's candidate: in order.
  const candidates = g.relayed.flatMap(x => x.tx.instructions.slice(1).map(ix => new DataView(ix.data.buffer, ix.data.byteOffset).getUint32(7, true)));
  assert.deepEqual(candidates, votes.map(v => v.candidate));
  assert.ok(g.relayed.every(x => new DataView(x.tx.instructions[1].data.buffer, x.tx.instructions[1].data.byteOffset).getUint32(1, true) === ME));
  // Proposals of long orders: halved until each fits one packet.
  const move = u => ({ type: 'MoveUnit', unit: u, path: Array.from({ length: 20 }, (_, i) => [i, -i]) });
  const p = chainplay.proposalActions({ General: [1, 2, 3, 4, 5].map(move) });
  assert.equal(p.tooLarge.length, 0);
  assert.ok(p.actions.length >= 2);
  assert.deepEqual(p.actions.flatMap(a => a.orders).map(o => o.unit), [1, 2, 3, 4, 5]);
  assert.ok(p.actions.every(a => a.orders.length <= 4 && chainplay.govFits(a)));
  // The commitments just closed: the actions wait in the queue, and go with the next ones.
  const late = fakeGateway({ relay: () => ({ ok: false, code: 'TickFrozen' }) });
  const q = await chainplay.submitGov([votes[0]]);
  assert.deepEqual(q.requeued, [votes[0]]);
  assert.deepEqual(q.failed, []);
  assert.deepEqual(S.govQueue, [votes[0]]);
  assert.equal(late.relayed.length, 1);
  const next = fakeGateway();
  const n = await chainplay.submitGov([votes[1]]);
  assert.deepEqual(n.sent, [votes[0], votes[1]]);
  assert.deepEqual(S.govQueue, []);
  assert.equal(next.relayed.length, 1);
  // Another refusal is reported, not queued.
  fakeGateway({ relay: () => ({ ok: false, code: 'InboxFull' }) });
  const f = await chainplay.submitGov([votes[2]]);
  assert.equal(f.failed[0].code, 'InboxFull');
  assert.deepEqual(S.govQueue, []);
});

test('talk: the message bytes signed by the session key, for the gateway\'s open tick', async () => {
  const g = fakeGateway();
  await chainio.season();
  const r = await chainplay.sendTalk({ to: { civ: 4 }, text: '講和しませんか' });
  assert.equal(r.ok, true);
  const post = g.calls.find(c => c.path === '/talk');
  assert.deepEqual({ ...post.body, signature: undefined }, { member: ME, to: { civ: 4 }, text: '講和しませんか', tick: TICK, signature: undefined });
  const bytes = talkBytes({ season: BigInt(SEASON), tick: TICK, member: ME, to: { civ: 4 }, text: '講和しませんか' });
  assert.ok(verifies(session.publicKey, bytes, fromHex(post.body.signature)));
  // The /tick answer also refreshed the dock's flags.
  assert.deepEqual(S.chainSeals.committed, [false, false, true, false]);
  const long = await chainplay.sendTalk({ to: null, text: 'あ'.repeat(281) });
  assert.equal(long.ok, false);
  assert.equal(g.calls.filter(c => c.path === '/talk').length, 1);
});

test('the dock\'s flags come from /tick for my nation, re-read on a new phase', async () => {
  const g = fakeGateway();
  await chainio.season();
  chainplay.onView(S.view);
  await chainplay.refreshFlags();
  assert.equal(S.chainSeals.tick, TICK);
  const states = chainplay.officeStates();
  assert.equal(states.find(s => s.role === 'Science').committed, true, 'the chain shows the science officer committed');
  assert.equal(states.find(s => s.role === 'Diplomat').committed, false);
  const reads = () => g.calls.filter(c => c.path === '/tick').length;
  const n = reads();
  chainplay.onView(S.view); // same tick and phase, fresh flags: no new read
  assert.equal(reads(), n);
  chainplay.onView({ ...S.view, chainPhase: 'reveal' });
  await chainplay.refreshFlags();
  assert.ok(reads() > n);
});
