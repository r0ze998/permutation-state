// The marchbook (permutation-server/web/frontier/marchbook.mjs): written
// before Depart is signed, never holding k or σ, the self-reveal timing of
// contract §9.1 (never before the arrival bell starts, at most two
// attempts, the second only when the slot is absent 60 s later and the
// window is open), reconciliation with the Holding's transit records
// (landed, a swapped seal root, settled, revealed), the reveal material of
// I-24, and a lost book.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as mb from '../../permutation-server/web/frontier/marchbook.mjs';

const memory = () => { const m = new Map(); return { get: k => m.get(k) ?? null, set: (k, v) => (m.set(k, v), true), m }; };
const bytes = (n, fill) => new Uint8Array(n).fill(fill);
const sealed = (over = {}) => ({
  host: 321057395310599n, transitSlot: 1, departBell: 100, arriveBell: 104, holding: 'AfZ7beD99Pttongtq1EKEiM34yUUcjJwwpAt9PJnT51F',
  plain: bytes(37, 1), salt: bytes(32, 2), commit: bytes(32, 3), sealRoot: bytes(32, 4), ctHash: bytes(32, 5), round: 32556137, tip: 14441, ...over,
});
const scope = { cluster: 'localnet', programId: 'Prog', seasonId: '1' };

test('entries: season- and wallet-scoped key, v1 fields, never k or σ, written before sending', () => {
  assert.equal(mb.bookKey(scope, 'W1'), 'ps-fmarch:localnet:Prog:1:W1');
  const e = mb.entryOf(sealed(), () => 0.5);
  assert.deepEqual(Object.keys(e).sort(), ['arriveBell', 'attempts', 'commit_hex', 'ctHash_hex', 'departBell', 'holding', 'host', 'plain_b64', 'revealDelay', 'round', 'salt_b64', 'sealRoot_hex', 'state', 'tip', 'transitSlot', 'v'].sort());
  assert.equal(e.v, 1);
  assert.equal(e.host, '321057395310599');
  assert.equal(e.state, 'sealed');
  assert.equal(e.revealDelay, 10);
  assert.ok(e.revealDelay >= 0 && e.revealDelay <= 20);
  assert.throws(() => mb.entryOf({ ...sealed(), k: bytes(16, 9) }), /never stores k/);
  assert.throws(() => mb.entryOf({ ...sealed(), sigma: bytes(16, 9) }), /never stores sigma/);
  const st = memory(), key = mb.bookKey(scope, 'W1');
  const book = mb.addSealed(mb.loadBook(st, key), e);
  assert.equal(mb.saveBook(st, key, book), true);
  assert.deepEqual(mb.loadBook(st, key), book);
  // A tampered store with a key in it: the key is never read back.
  st.set(key, JSON.stringify([{ ...e, k: 'AAAA', sigma: 'BBBB' }, { junk: 1 }]));
  const back = mb.loadBook(st, key);
  assert.equal(back.length, 1);
  assert.ok(!('k' in back[0]) && !('sigma' in back[0]));
  assert.deepEqual(mb.loadBook({ get: () => '{not json' }, key), []);
});

test('a re-seal replaces an unsent entry; a sent march cannot be sealed again', () => {
  let book = mb.addSealed([], mb.entryOf(sealed()));
  book = mb.addSealed(book, mb.entryOf(sealed({ commit: bytes(32, 7) })));
  assert.equal(book.length, 1);
  assert.equal(book[0].commit_hex, '07'.repeat(32));
  book = mb.mark(book, book[0], { state: 'sent' });
  assert.throws(() => mb.addSealed(book, mb.entryOf(sealed())), /already sent/);
  book = mb.addSealed(book, mb.entryOf(sealed({ transitSlot: 2 })));
  assert.equal(book.length, 2);
});

test('self-reveal timing: never before the bell starts, delay 0–20 s, a second try only after 60 s with the slot absent and the window open', () => {
  const start = 1_800_060_000;
  let e = { ...mb.entryOf(sealed(), () => 0.99), state: 'landed' };
  assert.equal(e.revealDelay, 20);
  assert.deepEqual(mb.revealStep({ ...e, state: 'sealed' }, { now: start + 100, bellStart: start }), { action: 'none', reason: 'notLanded' });
  assert.deepEqual(mb.revealStep(e, { now: start - 30, bellStart: start }), { action: 'wait', at: start + 20 });
  assert.deepEqual(mb.revealStep(e, { now: start + 19, bellStart: start }), { action: 'wait', at: start + 20 });
  assert.deepEqual(mb.revealStep(e, { now: start + 20, bellStart: start }), { action: 'send' });
  let book = [e];
  book = mb.recordAttempt(book, e, { t: start + 20, route: 'self', result: 'accepted' });
  e = book[0];
  assert.equal(e.state, 'revealing');
  assert.deepEqual(mb.revealStep(e, { now: start + 79, bellStart: start }), { action: 'wait', at: start + 80 });
  assert.deepEqual(mb.revealStep(e, { now: start + 80, bellStart: start, slotPresent: true }), { action: 'none', reason: 'revealed' });
  assert.deepEqual(mb.revealStep(e, { now: start + 80, bellStart: start, windowOpen: false }), { action: 'none', reason: 'windowClosed' });
  assert.deepEqual(mb.revealStep(e, { now: start + 80, bellStart: start }), { action: 'send' });
  book = mb.recordAttempt(book, e, { t: start + 80, route: 'self', result: 'accepted' });
  assert.deepEqual(mb.revealStep(book[0], { now: start + 500, bellStart: start }), { action: 'none', reason: 'attempted' });
  // A keeper's reveal does not count against the two self attempts.
  const k = mb.recordAttempt([e], e, { t: start + 30, route: 'keeper', result: 'landed' });
  assert.equal(k[0].attempts.filter(a => a.route === 'self').length, 1);
  // Whatever the delay, never before the bell start.
  for (const d of [0, 7, 20]) {
    const x = { ...e, attempts: [], revealDelay: d };
    for (let now = start - 5; now < start + d; now++) assert.equal(mb.revealStep(x, { now, bellStart: start }).action, 'wait');
  }
});

test('reveal material is the /gw/f/reveal body: holding, slot, plaintext, salt, ct_hash — base64, nothing else', () => {
  const e = mb.entryOf(sealed());
  const m = mb.revealMaterial(e);
  assert.deepEqual(Object.keys(m).sort(), ['ct_hash_b64', 'holding', 'plain_b64', 'salt_b64', 'transit_slot']);
  assert.equal(Buffer.from(m.plain_b64, 'base64').length, 37);
  assert.equal(Buffer.from(m.salt_b64, 'base64').length, 32);
  assert.deepEqual(Buffer.from(m.ct_hash_b64, 'base64'), Buffer.alloc(32, 5));
  assert.deepEqual(mb.materialBytes(e).plain, bytes(37, 1));
});

test('reconcile: landed, a swapped seal root, revealed, settled', () => {
  const e = mb.entryOf(sealed());
  const root = e.sealRoot_hex;
  let book = [{ ...e, state: 'sent' }];
  const rec = (over = {}) => ({ slot: 1, state: 1, hostId: 321057395310599n, arriveBell: 104, sealRoot: root, ...over });
  book = mb.reconcile(book, { transits: [rec()] });
  assert.equal(book[0].state, 'landed');
  assert.equal(mb.reconcile(book, { transits: [rec()], revealedHosts: ['321057395310599'] })[0].state, 'revealed');
  const swapped = mb.reconcile([{ ...e, state: 'sent' }], { transits: [rec({ sealRoot: 'ab'.repeat(32) })] });
  assert.equal(swapped[0].state, 'failed');
  assert.equal(mb.trackerHint(swapped[0]), 'sealMismatch');
  assert.equal(mb.revealStep(swapped[0], { now: 1e12, bellStart: 0 }).action, 'none', 'a swapped seal is never revealed');
  assert.equal(mb.reconcile(book, { transits: [rec({ state: 0 })] })[0].state, 'settled');
  assert.equal(mb.reconcile(book, { transits: [], complete: true })[0].state, 'settled');
  assert.equal(mb.reconcile(book, { transits: [] })[0].state, 'landed', 'a partial list settles nothing');
  // A march that never landed stays as it was.
  assert.equal(mb.reconcile([{ ...e, state: 'sent' }], { transits: [], complete: true })[0].state, 'sent');
});

test('a lost book: keepers reveal; the book stays bounded', () => {
  assert.equal(mb.trackerHint(null), 'keepersWillReveal');
  assert.equal(mb.revealStep(null, { now: 0, bellStart: 0 }).action, 'none');
  let book = [];
  for (let i = 0; i < mb.MAX_ENTRIES; i++) book = mb.addSealed(book, { ...mb.entryOf(sealed({ transitSlot: i % 4, arriveBell: 200 + i })), state: 'settled' });
  book = mb.addSealed(book, mb.entryOf(sealed({ arriveBell: 999 })));
  assert.equal(book.length, mb.MAX_ENTRIES);
  assert.equal(book.at(-1).arriveBell, 999);
});
