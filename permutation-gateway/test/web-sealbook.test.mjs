// The web client's own record of its chain play and the pure rules of its
// in-game chain actions (permutation-server/web/sealbook.mjs): the book in
// storage, pending rationale reveals and their reconciliation with the play
// server's decision log, each office's seal state, restoring after a
// reload, the check of the play server's /api/validate split, and how
// governance actions are packed into transactions.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as book from '../../permutation-server/web/sealbook.mjs';
import { submitGovIxs } from '../client/src/player.mjs';
import { compileMessage, PACKET_BYTES } from '../client/src/solana-tx.mjs';
import { encode as base58 } from '../client/src/base58.mjs';

const mapStorage = () => {
  const m = new Map();
  return { m, get: k => m.get(k) ?? null, set: (k, v) => { m.set(k, v); return true; } };
};
const hex = n => n.toString(16).padStart(64, '0');
const decision = (tick, role, digest, extra = {}) => ({ tick, role, civ: 2, policy: 'officer@2', salt: '00'.repeat(16), text: `why ${tick}`, digest, obsRoot: hex(1), status: 'sealed', ...extra });

test('auto-commit lead time: max(8 s, a third of the tick)', () => {
  assert.equal(book.autoCommitSeconds(30), 10);
  assert.equal(book.autoCommitSeconds(60), 20);
  assert.equal(book.autoCommitSeconds(12), 8);
  assert.equal(book.autoCommitSeconds(4), 8);
  assert.equal(book.autoCommitSeconds(undefined), 8);
});

test('the book survives storage (per season and member) and starts empty when damaged', () => {
  const st = mapStorage();
  const key = book.bookKey({ cluster: 'localnet', programId: 'P', seasonId: '7' }, 3);
  assert.equal(key, 'ps-play:localnet:P:7:3');
  assert.deepEqual(book.loadBook(st, key), book.emptyBook());
  const b = book.emptyBook();
  book.addCandidate(b, { tick: 5, role: 'Science', commitment: 'c1', drafts: [] }, decision(5, 'Science', 'd1'));
  assert.equal(book.saveBook(st, key, b), true);
  assert.deepEqual(book.loadBook(st, key), b);
  for (const bad of ['{', '[]', '{"v":2,"sealed":{},"decisions":[]}', '{"v":1,"sealed":[],"decisions":[]}', 'null']) {
    st.m.set(key, bad);
    assert.deepEqual(book.loadBook(st, key), book.emptyBook(), bad);
  }
  // The browser storage falls back to memory without localStorage (node).
  assert.equal(book.browserStorage.set('ps-test', 'x'), false);
  assert.equal(book.browserStorage.get('ps-test'), 'x');
});

test('sealed batches: newest first per office and tick, marked as they go out, pruned once the tick resolved', () => {
  const b = book.emptyBook();
  const c1 = book.addCandidate(b, { tick: 5, role: 'Science', commitment: 'a' }, decision(5, 'Science', 'da'));
  assert.equal(c1.state, 'sealing');
  book.addCandidate(b, { tick: 5, role: 'Science', commitment: 'b' }, decision(5, 'Science', 'db'));
  book.addCandidate(b, { tick: 4, role: 'General', commitment: 'z' }, decision(4, 'General', 'dz'));
  assert.deepEqual(book.candidates(b, 5, 'Science').map(c => c.commitment), ['b', 'a']);
  assert.equal(book.commitCount(b, 5, 'Science'), 2);
  assert.equal(book.lastCommitted(b, 5, 'Science'), null);
  book.markCandidate(b, 5, 'Science', 'a', { state: 'committed', signature: 's' });
  book.markCandidate(b, 5, 'Science', 'b', { state: 'failed', stage: 'seal', code: 'NotOfficer' });
  assert.equal(book.lastCommitted(b, 5, 'Science').commitment, 'a');
  assert.equal(book.markCandidate(b, 5, 'Science', 'nope', {}), null);
  assert.equal(b.decisions.length, 3);
  assert.ok(b.decisions.every(d => d.status === 'sealed'));
  book.pruneSealed(b, 5);
  assert.deepEqual(Object.keys(b.sealed), ['5:Science']);
  book.pruneSealed(b, 6);
  assert.deepEqual(b.sealed, {});
});

test('pending reveals: landed, earlier, oldest first; not while a batch of an earlier tick may carry them', () => {
  const b = book.emptyBook();
  b.decisions = [
    { ...decision(4, 'Science', 'd4'), status: 'landed' },
    { ...decision(2, 'Science', 'd2'), status: 'landed' },
    { ...decision(3, 'Science', 'd3'), status: 'sealed' },   // not known to have landed
    { ...decision(3, 'General', 'g3'), status: 'landed' },   // another office
    { ...decision(6, 'Science', 'd6'), status: 'landed' },   // not before the tick
  ];
  assert.deepEqual(book.pendingReveals(b, 'Science', 6).map(d => d.tick), [2, 4]);
  assert.deepEqual(Object.keys(book.pendingReveals(b, 'Science', 6)[0]).sort(), ['policy', 'salt', 'text', 'tick']);
  book.markSent(b, 'Science', 6, [2]);
  // Sent this tick: every re-commit of the tick carries it again (only one lands) …
  assert.deepEqual(book.pendingReveals(b, 'Science', 6).map(d => d.tick), [2, 4]);
  // … but a batch of an earlier tick whose fate is unknown holds it back.
  assert.deepEqual(book.pendingReveals(b, 'Science', 7).map(d => d.tick), [4, 6]);
});

test('reconcile against the play server\'s decision log', () => {
  const b = book.emptyBook();
  b.decisions = [
    decision(8, 'Science', 'open'),                                   // the open tick: untouched
    decision(7, 'Science', 'landed'),                                 // landed, not revealed → landed
    decision(7, 'Science', 'twin'),                                   // another candidate of that office → gone
    decision(6, 'Science', 'revealed'),                               // revealed → gone
    { ...decision(5, 'Science', 'carried'), status: 'landed', sentIn: 7 }, // its reveal rode in tick 7's batch, which did not reveal it
    decision(6, 'General', 'none'),                                   // no batch of the General landed at 6 → gone
    decision(2, 'Diplomat', 'old'),                                   // before the list: kept (unknown)
    decision(-40, 'Diplomat', 'ancient'),                             // unknown and too old → gone
  ];
  const records = [
    { tick: 7, civ: 2, role: 'Science', digest: 'landed', reveal: null },
    { tick: 7, civ: 1, role: 'General', digest: 'x', reveal: null },
    { tick: 6, civ: 2, role: 'Science', digest: 'revealed', reveal: { at: 7, text: 'why' } },
    { tick: 5, civ: 2, role: 'Science', digest: 'carried', reveal: null },
    { tick: 5, civ: 2, role: 'Diplomat', digest: 'y', reveal: null },
  ];
  book.reconcile(b, { records, open: 8, civ: 2, complete: false });
  const left = Object.fromEntries(b.decisions.map(d => [d.digest, d]));
  assert.deepEqual(Object.keys(left).sort(), ['carried', 'landed', 'old', 'open']);
  assert.equal(left.landed.status, 'landed');
  assert.equal(left.open.status, 'sealed');
  assert.equal(left.old.status, 'sealed');
  assert.equal(left.carried.status, 'landed');
  assert.equal(left.carried.sentIn, undefined, 'pending again');
  // The oldest listed tick counts only when the list is complete.
  const c = book.emptyBook();
  c.decisions = [decision(5, 'General', 'g')];
  book.reconcile(c, { records, open: 8, civ: 2, complete: false });
  assert.equal(c.decisions.length, 1);
  book.reconcile(c, { records, open: 8, civ: 2, complete: true });
  assert.equal(c.decisions.length, 0);
  // An empty log says nothing (e.g. a play server that just restarted).
  const d = book.emptyBook();
  d.decisions = [decision(5, 'General', 'g')];
  book.reconcile(d, { records: [], open: 8, civ: 2, complete: true });
  assert.equal(d.decisions.length, 1);
});

test('an office\'s seal state from the local batches and the gateway\'s flags', () => {
  const b = book.emptyBook();
  const none = book.officeState(b, { tick: 5, role: 'Science', flags: null });
  assert.deepEqual(none, { role: 'Science', sending: false, committed: false, onChain: null, revealed: null, digest: null, failed: null, count: 0 });
  // Committed from another tab or device: the chain says so.
  const flags = { tick: 5, committed: [false, false, true, false], revealed: [false, false, false, false] };
  assert.equal(book.officeState(b, { tick: 5, role: 'Science', flags }).committed, true);
  assert.equal(book.officeState(b, { tick: 5, role: 'Science', flags: { ...flags, tick: 4 } }).onChain, null, 'flags of another tick are unknown');
  book.addCandidate(b, { tick: 5, role: 'General', commitment: 'a', digest: 'da' }, decision(5, 'General', 'da'));
  assert.equal(book.officeState(b, { tick: 5, role: 'General', flags }).sending, true);
  book.markCandidate(b, 5, 'General', 'a', { state: 'failed', stage: 'seal', code: 'NotOfficer', error: 'no' });
  assert.deepEqual(book.officeState(b, { tick: 5, role: 'General', flags }).failed, { stage: 'seal', code: 'NotOfficer', error: 'no' });
  book.addCandidate(b, { tick: 5, role: 'General', commitment: 'b', digest: 'db' }, decision(5, 'General', 'db'));
  book.markCandidate(b, 5, 'General', 'b', { state: 'committed' });
  const s = book.officeState(b, { tick: 5, role: 'General', flags: { ...flags, revealed: [true, false, false, false] } });
  assert.equal(s.committed, true);
  assert.equal(s.onChain, false);
  assert.equal(s.revealed, true);
  assert.equal(s.digest, 'db');
  assert.equal(s.failed, null);
  assert.equal(s.count, 2);
  // A newer attempt that failed does not undo the commitment already out.
  book.addCandidate(b, { tick: 5, role: 'General', commitment: 'c', digest: 'dc' }, decision(5, 'General', 'dc'));
  book.markCandidate(b, 5, 'General', 'c', { state: 'failed' });
  assert.equal(book.officeState(b, { tick: 5, role: 'General', flags }).committed, true);
  assert.equal(book.officeState(b, { tick: 5, role: 'General', flags }).failed, null);
});

test('after a reload: the orders, adoptions and rationale this browser sealed this tick', () => {
  const b = book.emptyBook();
  assert.equal(book.restoreFrom(b, 5, ['Science']), null);
  const research = { type: 'SetResearch', techs: ['Writing'] };
  const envoy = { type: 'SendEnvoy', cityState: 1, influence: 10 };
  book.addCandidate(b, { tick: 5, role: 'Science', commitment: 'old', drafts: [{ type: 'SetResearch', techs: ['Agriculture'] }], adopt: [], rationale: 'first' }, decision(5, 'Science', 'x'));
  book.markCandidate(b, 5, 'Science', 'old', { state: 'committed' });
  book.addCandidate(b, { tick: 5, role: 'Science', commitment: 'new', drafts: [research], adopt: [4], rationale: 'second' }, decision(5, 'Science', 'y'));
  book.markCandidate(b, 5, 'Science', 'new', { state: 'committed' });
  book.addCandidate(b, { tick: 5, role: 'Diplomat', commitment: 'd', drafts: [envoy], adopt: [], rationale: 'second' }, decision(5, 'Diplomat', 'z'));
  book.markCandidate(b, 5, 'Diplomat', 'd', { state: 'committed' });
  book.addCandidate(b, { tick: 5, role: 'Diplomat', commitment: 'e', drafts: [], adopt: [], rationale: 'lost' }, decision(5, 'Diplomat', 'w'));
  assert.deepEqual(book.restoreFrom(b, 5, ['Science', 'Diplomat']), { orders: [research, envoy], adopt: { Science: [4] }, rationale: 'second' });
  assert.equal(book.restoreFrom(b, 6, ['Science', 'Diplomat']), null);
});

test('the play server\'s split of my drafts is checked before anything is sealed', () => {
  const a = { type: 'SetResearch', techs: ['Writing'] };
  const m = { type: 'MoveUnit', unit: 4, path: [[1, 2]] };
  const p = { type: 'Purchase', city: 1, gold: 60 };
  const echo = o => JSON.parse(JSON.stringify(Object.fromEntries(Object.entries(o).reverse()))); // keys in another order
  assert.equal(book.canonical(echo(m)), book.canonical(m));
  assert.notEqual(book.canonical({ a: [1, 2] }), book.canonical({ a: [2, 1] }));
  const answer = {
    ok: true, tick: 5, warnings: [],
    offices: [
      { role: 'General', orders: [echo(m)], adopt: [], cost: 1, spendable: 2, error: null },
      { role: 'Science', orders: [echo(a)], adopt: [9, 3], cost: 1, spendable: 1, error: null },
    ],
    refused: [{ order: echo(p), error: 'the Steward gives it: propose it instead' }],
  };
  const sent = { orders: [a, m, p], adopt: { Science: [3, 9] }, tick: 5 };
  const r = book.checkValidation(answer, sent);
  assert.equal(r.ok, true);
  assert.equal(r.offices[0].orders[0], m, 'the objects sent, not the echo');
  assert.equal(r.offices[1].orders[0], a);
  assert.deepEqual(r.offices[1].adopt, [3, 9]);
  assert.equal(r.refused[0].order, p);
  // What the play server may not do.
  const injected = structuredClone(answer);
  injected.offices[0].orders.push({ type: 'DeclareWar', civ: 1 });
  assert.equal(book.checkValidation(injected, sent).problem, 'orders');
  const doubled = structuredClone(answer);
  doubled.offices[1].orders.push(echo(a));
  assert.equal(book.checkValidation(doubled, sent).problem, 'orders');
  const dropped = structuredClone(answer);
  dropped.refused = [];
  assert.equal(book.checkValidation(dropped, sent).problem, 'missing');
  const adopted = structuredClone(answer);
  adopted.offices[0].adopt = [7];
  assert.equal(book.checkValidation(adopted, sent).problem, 'adopt');
  assert.equal(book.checkValidation({ ...answer, tick: 6 }, sent).problem, 'tick');
  assert.equal(book.checkValidation({ ok: false, error: 'no such member' }, sent).problem, 'answer');
  const office = structuredClone(answer);
  office.offices[0].role = 'King';
  assert.equal(book.checkValidation(office, sent).problem, 'office');
  // An office's batch that fails makes the whole answer not ok.
  const bad = structuredClone(answer);
  bad.ok = false;
  bad.offices[1].error = 'orders cost 2, only 1 spendable';
  const rb = book.checkValidation(bad, sent);
  assert.equal(rb.ok, false);
  assert.equal(rb.offices[1].error, 'orders cost 2, only 1 spendable');
});

test('proposals: four orders each, halved until each fits a transaction', () => {
  const o = n => ({ type: 'SetResearch', techs: [`T${n}`] });
  const orders = [1, 2, 3, 4, 5, 6].map(o);
  const all = book.proposals('Science', orders, () => true);
  assert.deepEqual(all.actions.map(a => a.orders.length), [4, 2]);
  assert.deepEqual(all.actions[0], { type: 'Propose', role: 'Science', orders: orders.slice(0, 4) });
  // Only two orders fit one transaction: 4 → 2 + 2, then 2.
  const two = book.proposals('Science', orders, a => a.orders.length <= 2);
  assert.deepEqual(two.actions.map(a => a.orders.length), [2, 2, 2]);
  assert.deepEqual(two.actions.flatMap(a => a.orders), orders, 'in order');
  // An order that does not fit alone.
  const [x1, x2] = [o(1), o(2)];
  const none = book.proposals('Science', [x1, x2], a => a.orders.length === 1 && a.orders[0] !== x1);
  assert.deepEqual(none.actions, [{ type: 'Propose', role: 'Science', orders: [x2] }]);
  assert.deepEqual(none.tooLarge, [{ type: 'Propose', role: 'Science', orders: [x1] }]);
});

test('governance transactions: up to 8 actions each, each within one packet, in order', () => {
  const size = list => 100 + list.length * 100;
  const votes = Array.from({ length: 19 }, (_, i) => ({ type: 'Vote', role: 'General', candidate: i }));
  const r = book.packGov(votes, { sizeOf: list => list.length });
  assert.deepEqual(r.chunks.map(c => c.length), [8, 8, 3]);
  assert.deepEqual(r.chunks.flat(), votes);
  const s = book.packGov(votes.slice(0, 5), { sizeOf: size, limit: 350 });
  assert.deepEqual(s.chunks.map(c => c.length), [2, 2, 1]);
  const big = { type: 'Propose', role: 'General', orders: [] };
  const t = book.packGov([votes[0], big, votes[1]], { sizeOf: list => (list.includes(big) ? 10_000 : list.length) });
  assert.deepEqual(t.chunks, [[votes[0], votes[1]]]);
  assert.deepEqual(t.tooLarge, [big]);
});

test('eight governance actions of one member fit one real transaction (the AI members\' shape)', () => {
  const key = n => base58(Uint8Array.from({ length: 32 }, (_, i) => (i * 7 + n) & 0xff));
  const [program, signer, payer] = [key(1), key(2), key(3)];
  const sizeOf = actions => 1 + 64 * 2 + compileMessage({ feePayer: payer, recentBlockhash: payer,
    instructions: submitGovIxs({ programId: program, seasonId: 7n, signer, civ: 2, member: 3, actions }) }).length;
  const votes = Array.from({ length: 8 }, (_, i) => ({ type: 'Vote', role: 'Science', candidate: i }));
  assert.ok(sizeOf(votes) <= PACKET_BYTES, `${sizeOf(votes)} bytes`);
  const move = u => ({ type: 'MoveUnit', unit: u, path: Array.from({ length: 12 }, (_, i) => [i, -i]) });
  const props = [1, 2, 3].map(u => ({ type: 'Propose', role: 'General', orders: [move(u), move(u + 10), move(u + 20), move(u + 30)] }));
  const r = book.packGov([...votes.slice(0, 3), ...props], { sizeOf });
  assert.ok(r.chunks.length > 1, 'large proposals need their own transactions');
  for (const c of r.chunks) assert.ok(sizeOf(c) <= PACKET_BYTES && c.length <= 8);
  assert.deepEqual(r.chunks.flat(), [...votes.slice(0, 3), ...props]);
});

test('an open Capture offer passes the check although the server echoes "to": null (and the other way round)', () => {
  // diplomacy.mjs builds an open offer without `to`; the play server echoes
  // its parsed OrderDto, which writes the missing field as null.
  const capture = { type: 'OfferContract', term: { kind: 'Capture', city: 11 }, usdc: 3000000, deadline: 90 };
  const science = { type: 'SetResearch', techs: ['Writing'] };
  const echo = { deadline: 90, term: { city: 11, kind: 'Capture' }, to: null, type: 'OfferContract', usdc: 3000000 }; // the server's literal echo
  assert.equal(book.canonical(echo), book.canonical(capture));
  assert.equal(book.canonical({ a: 1, b: undefined, c: { d: null } }), book.canonical({ a: 1, c: {} }));
  assert.notEqual(book.canonical({ to: 2 }), book.canonical({}), 'a value that is there still counts');
  assert.equal(book.canonical([null, 1]), '[null,1]', 'array items are kept');
  const sent = { orders: [capture, science], adopt: {}, tick: 5 };
  const answer = {
    ok: true, tick: 5, warnings: [], refused: [],
    offices: [
      { role: 'Diplomat', orders: [echo], adopt: [], cost: 1, spendable: 2, error: null },
      { role: 'Science', orders: [science], adopt: [], cost: 1, spendable: 1, error: null },
    ],
  };
  const r = book.checkValidation(answer, sent);
  assert.equal(r.problem, undefined);
  assert.equal(r.ok, true);
  assert.equal(r.offices[0].orders[0], capture, 'the member\'s own draft is sealed (no "to" key)');
  assert.equal('to' in r.offices[0].orders[0], false);
  // The same echo when the member does not hold Diplomat: refused, not a mismatch.
  const refused = { ok: true, tick: 5, warnings: [], offices: [answer.offices[1]], refused: [{ order: echo, error: 'the Diplomat gives it: propose it instead' }] };
  const rr = book.checkValidation(refused, sent);
  assert.equal(rr.problem, undefined);
  assert.equal(rr.refused[0].order, capture);
  // A server that leaves out a field the member sent as null matches too.
  const withNull = { ...capture, to: null };
  const back = book.checkValidation({ ...answer, offices: [{ ...answer.offices[0], orders: [capture] }, answer.offices[1]] }, { ...sent, orders: [withNull, science] });
  assert.equal(back.ok, true);
  assert.equal(back.offices[0].orders[0], withNull);
  // But an echo naming a counterparty the member did not name is still refused.
  const named = { ...echo, to: 3 };
  assert.equal(book.checkValidation({ ...answer, offices: [{ ...answer.offices[0], orders: [named] }, answer.offices[1]] }, sent).problem, 'orders');
});

test('retryable: only failures that may pass if sent again', () => {
  for (const f of [{ code: 'network' }, { ok: false, error: 'network' }, { code: 'RateLimited', httpStatus: 429 }, { code: 'BlockhashExpired', httpStatus: 409 },
    { code: 'Unavailable', httpStatus: 503 }, { code: 'HTTP502', httpStatus: 502 }, { code: 'SimulationFailed', httpStatus: 500 }, { code: 'NoObservation' }]) {
    assert.equal(book.retryable(f), true, JSON.stringify(f));
  }
  for (const f of [null, undefined, {}, { code: 'TooManyCommits', httpStatus: 429 }, { code: 'TooManySeals', httpStatus: 429 }, { code: 'NotOfficer', httpStatus: 403 },
    { code: 'WrongTick' }, { code: 'WrongPhase' }, { code: 'BatchTooLarge' }, { code: 'RelayRejected', httpStatus: 400 }, { ok: false, error: 'orders cost 2, only 1 spendable' }]) {
    assert.equal(book.retryable(f), false, JSON.stringify(f));
  }
});

test('auto-commit: waits for the drafts to settle, ignores the rationale, fires once per draft state', () => {
  const a = {};
  const base = { dirty: true, phase: 'commit', secondsLeft: 9, window: 10, counts: [0] };
  const step = (key, now, extra = {}) => book.autoCommitStep(a, { ...base, key, now, ...extra });
  // Outside the window nothing fires, but the draft's age is tracked.
  assert.equal(step('5|A', 0, { secondsLeft: 20 }).fire, false);
  assert.equal(step('5|A', 1000).fire, false, 'less than AUTO_QUIET_MS unchanged');
  assert.equal(step('5|A', book.AUTO_QUIET_MS).fire, true);
  book.autoCommitStarted(a);
  assert.equal(step('5|A', book.AUTO_QUIET_MS + 100).fire, false, 'in flight');
  assert.equal(book.autoCommitSettled(a, '5|A', { ok: true, errors: [] }, 2500), 'done');
  // Typing a rationale keeps the key (it is not part of it) and the draft dirty: no second commit.
  for (let t = 3000; t < 9000; t += 750) assert.equal(step('5|A', t).fire, false);
  // A new draft is a new state: after it settles, it goes once.
  assert.equal(step('5|AB', 9000).fire, false);
  assert.equal(step('5|ABC', 9500).fire, false, 'still changing');
  assert.equal(step('5|ABC', 11_000).fire, false);
  assert.equal(step('5|ABC', 11_500).fire, true);
  // Not dirty, not the commit phase, busy, or out of the window: nothing.
  assert.equal(step('5|ABC', 12_000, { dirty: false }).fire, false);
  assert.equal(step('5|ABC', 12_000, { phase: 'reveal' }).fire, false);
  assert.equal(step('5|ABC', 12_000, { busy: true }).fire, false);
  assert.equal(step('5|ABC', 12_000, { secondsLeft: 10 }).fire, false);
  // Too close to the deadline to wait for the drafts to settle: it goes at once.
  const b = {};
  assert.equal(book.autoCommitStep(b, { ...base, key: '5|Z', now: 0, secondsLeft: 3 }).fire, true);
});

test('auto-commit: never past an office\'s commit cap', () => {
  const a = {};
  const input = { key: '5|A', now: 0, dirty: true, phase: 'commit', secondsLeft: 3, window: 10 };
  assert.equal(book.autoCommitStep(a, { ...input, counts: [book.MAX_COMMITS - 1, 0] }).fire, true);
  assert.equal(book.autoCommitStep({}, { ...input, counts: [0, book.MAX_COMMITS] }).fire, false);
});

test('auto-commit: a failure that may pass is tried again after a pause, a refusal is not', () => {
  const a = {};
  const base = { key: '5|A', dirty: true, phase: 'commit', window: 10, counts: [1] };
  const step = (now, secondsLeft, extra = {}) => book.autoCommitStep(a, { ...base, now, secondsLeft, ...extra });
  step(0, 9);
  assert.equal(step(2000, 8).fire, true);
  book.autoCommitStarted(a);
  assert.equal(book.autoCommitSettled(a, '5|A', { ok: false, retry: true, errors: ['確定できませんでした：混み合っています'] }, 3000), 'retry');
  assert.equal(step(4000, 6).fire, false, 'backing off');
  assert.equal(step(3000 + book.AUTO_RETRY_MS, 5).fire, true);
  book.autoCommitStarted(a);
  assert.equal(book.autoCommitSettled(a, '5|A', { ok: false, retry: true, errors: ['again'] }, 6000), 'retry');
  assert.equal(step(6000 + book.AUTO_RETRY_MS, 4).fire, true);
  book.autoCommitStarted(a);
  // The last attempt: settled, whatever the failure.
  assert.equal(book.autoCommitSettled(a, '5|A', { ok: false, retry: true, errors: ['third'] }, 9000), 'failed');
  assert.equal(a.attempts, book.AUTO_ATTEMPTS);
  assert.equal(step(20_000, 3).fire, false);
  // A refusal (not retryable) settles at once: one attempt per draft state.
  const c = {};
  book.autoCommitStep(c, { ...base, key: '5|B', now: 0, secondsLeft: 3 });
  book.autoCommitStarted(c);
  assert.equal(book.autoCommitSettled(c, '5|B', { ok: false, retry: false, errors: ['no'] }, 100), 'failed');
  for (let t = 500; t < 5000; t += 500) assert.equal(book.autoCommitStep(c, { ...base, key: '5|B', now: t, secondsLeft: 3 }).fire, false);
  // A retry that can no longer happen (2 s or less left, or the phase closed) reports the errors it held back, once.
  const d = {};
  book.autoCommitStep(d, { ...base, key: '5|C', now: 0, secondsLeft: 3 });
  book.autoCommitStarted(d);
  book.autoCommitSettled(d, '5|C', { ok: false, retry: true, errors: ['held back'] }, 0);
  assert.deepEqual(book.autoCommitStep(d, { ...base, key: '5|C', now: 5000, secondsLeft: 2 }), { fire: false, report: ['held back'] });
  assert.deepEqual(book.autoCommitStep(d, { ...base, key: '5|C', now: 6000, secondsLeft: 1 }), { fire: false, report: null });
  const e = {};
  book.autoCommitStep(e, { ...base, key: '5|D', now: 0, secondsLeft: 9 });
  book.autoCommitStep(e, { ...base, key: '5|D', now: 2000, secondsLeft: 8 });
  book.autoCommitStarted(e);
  book.autoCommitSettled(e, '5|D', { ok: false, retry: true, errors: ['held'] }, 2000);
  assert.deepEqual(book.autoCommitStep(e, { ...base, key: '5|D', now: 3000, secondsLeft: 7, phase: 'reveal' }).report, ['held']);
  // Sent by hand meanwhile (no longer dirty): the retry is dropped quietly.
  const f = {};
  book.autoCommitStep(f, { ...base, key: '5|E', now: 0, secondsLeft: 3 });
  book.autoCommitStarted(f);
  book.autoCommitSettled(f, '5|E', { ok: false, retry: true, errors: ['x'] }, 0);
  assert.deepEqual(book.autoCommitStep(f, { ...base, key: '5|E', now: 5000, secondsLeft: 5, dirty: false }), { fire: false, report: null });
  // The draft changed while an attempt was out: its answer no longer counts.
  const g = {};
  book.autoCommitStep(g, { ...base, key: '5|F', now: 0, secondsLeft: 3 });
  book.autoCommitStarted(g);
  book.autoCommitStep(g, { ...base, key: '5|G', now: 100, secondsLeft: 3, busy: true });
  assert.equal(book.autoCommitSettled(g, '5|F', { ok: true }, 200), 'stale');
  assert.equal(book.autoCommitStep(g, { ...base, key: '5|G', now: 300, secondsLeft: 3 }).fire, true);
});
