// The crank's tick index: its own parts are archived at once and never
// awaited behind a history read; gaps left by others' ResolveTick parts are
// filled in the background (single-flight, one attempt per gap, no
// duplicate lines); a tick someone else closed has commitSignature null.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { appendFileSync, mkdtempSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Crank, INDEX_PAGES } from '../src/crank.mjs';
import { appendTickLines, tickLine } from '../src/ticks.mjs';
import { fakeConnection, keyring, programId } from './gateway-fixtures.mjs';

const sha = b => createHash('sha256').update(b).digest();
const hex = b => Buffer.from(b).toString('hex');
const u16 = n => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };
const root = n => Buffer.alloc(32, n);
const data = (...fields) => `Program data: ${fields.map(f => Buffer.from(f).toString('base64')).join(' ')}`;
const tickLog = (tick, to, pre, post, hash) => data('PS_TICK', u16(tick), [to], pre, post, hash);

/** A tick's published input (as publishTickInput returns it). */
const published = tick => {
  const bytes = Buffer.from(`input of tick ${tick}`);
  return { tick, input: bytes.toString('hex'), hash: hex(sha(bytes)), signatures: [`in${tick}`], bytes };
};
/** A landed resolve part as send() returns it. */
const part = (signature, tick, to, pre, post) => ({
  signature, cu: 5, fetched: true,
  records: [{ tag: 'PS_TICK', tick, to, preRoot: pre, root: post, inputHash: sha(published(tick).bytes) }],
});
const line = (signature, tick, to, pre, post) => tickLine({ rec: part(signature, tick, to, pre, post).records[0], published: published(tick), signature, cu: 5 });

function crankWith({ er = {}, lines = [], open = root(0) } = {}) {
  const ticksDir = mkdtempSync(path.join(os.tmpdir(), 'crank-index-'));
  const logs = [];
  const conn = fakeConnection(er);
  const crank = new Crank({ base: fakeConnection(), er: conn, cfg: { programId }, store: { state: { seasonId: '42', members: [], open: { root: hex(open) } }, save() {} },
    keys: keyring(), ticksDir, log: m => logs.push(m) });
  appendTickLines(crank.tickFile, lines);
  crank.nations = 2;
  return { crank, er: conn, logs };
}

/** A snapshot at `openTick`: `step` 'publish' (frozen input), 'close' (commitments open, deadline passed) or 'reveal' (closed, the reveal window open). */
const snap = (openTick, step) => ({
  header: { meta: { finished: false, frozen: step === 'publish', revealing: step === 'reveal', deadline: Math.floor(Date.now() / 1000) - 60 } },
  nations: [0, 1].map(() => ({ openTick, committed: [0, 0, 0, 0].map(() => 65535), submitted: [0, 0, 0, 0].map(() => 65535), commits: [] })),
});

const tick4 = () => line('t4', 4, 12, root(3), root(4));

test('an outsider closed the tick: commitSignature null, and no history is read', async () => {
  let listed = 0;
  const p5 = published(5);
  const { crank } = crankWith({
    lines: [tick4()],
    er: {
      getSignaturesForAddress: async () => { listed++; return []; },
      getTransaction: async () => ({ slot: 3, meta: { err: null, computeUnitsConsumed: 5, logMessages: [`Program ${programId} invoke [1]`, tickLog(5, 12, root(4), root(5), sha(p5.bytes)), `Program ${programId} success`] } }),
    },
  });
  crank.closed = { tick: 4, signature: 'close4' };
  crank.publishInput = async () => p5;
  assert.equal(await crank.publishAndResolve(5, 2, { committed: 2, revealed: 1 }), true);
  const lines = crank.tickRecords();
  assert.deepEqual(lines.map(l => [l.tick, l.commitSignature]), [[4, null], [5, null]]);
  assert.equal(listed, 0, 'getSignaturesForAddress is never called while resolving');
  assert.equal(typeof crank.findClose, 'undefined');
  assert.equal(INDEX_PAGES, 1);
});

test('the critical path never waits for the indexer (a scan that hangs on spam)', async () => {
  let listed = 0;
  const { crank } = crankWith({ lines: [tick4()], er: { getSignaturesForAddress: () => { listed++; return new Promise(() => {}); } } });
  const calls = [];
  crank.revealHosted = async tick => calls.push(`reveal ${tick}`);
  crank.closeTick = async open => calls.push(`close ${open}`);
  crank.publishAndResolve = async open => { calls.push(`publish ${open}`); return false; };
  // Someone else resolved tick 5: the snapshot is at tick 6.
  await crank.play(snap(6, 'close'));
  await new Promise(r => setTimeout(r, 20));
  assert.deepEqual(calls, ['close 6']);
  assert.equal(listed, 1, 'the tail gap is being scanned');
  assert.ok(crank.indexing, 'still pending in the background');
  await crank.play({ ...snap(6, 'publish') });
  await new Promise(r => setTimeout(r, 20));
  assert.deepEqual(calls, ['close 6', 'publish 6']);
  // The reveal phase: the hosted batches are revealed while the scan still hangs.
  await crank.play(snap(6, 'reveal'));
  await new Promise(r => setTimeout(r, 20));
  assert.deepEqual(calls, ['close 6', 'publish 6', 'reveal 6', 'publish 6']);
  assert.equal(listed, 1, 'single-flight: no second scan');
});

test('a torn or unreadable index never stops the crank', async () => {
  const { crank, logs } = crankWith({ lines: [tick4()], er: { getSignaturesForAddress: () => new Promise(() => {}) } });
  const calls = [];
  crank.closeTick = async open => calls.push(`close ${open}`);
  // A crash mid-append left a torn last line: skipped.
  appendFileSync(crank.tickFile, '{"tick":5,"to":');
  await crank.play(snap(6, 'close'));
  assert.deepEqual(calls, ['close 6']);
  assert.equal(crank.head.signature, 't4');
  // An index that cannot be read at all (here a directory): logged, and the tick still closes.
  const broken = crankWith({ er: { getSignaturesForAddress: () => new Promise(() => {}) } });
  broken.crank.tickFile = path.dirname(broken.crank.tickFile);
  broken.crank.closeTick = async open => calls.push(`close ${open} (no index)`);
  await broken.crank.play(snap(6, 'close'));
  assert.deepEqual(calls, ['close 6', 'close 6 (no index)']);
  assert.match(broken.logs.join('\n'), /tick index: .*EISDIR/);
  assert.deepEqual(logs, []);
});

test('a part that cannot be archived is still resolved (the index is logged, not thrown)', async () => {
  const p5 = published(5);
  const { crank, logs } = crankWith({
    er: { getTransaction: async () => ({ slot: 3, meta: { err: null, computeUnitsConsumed: 5, logMessages: [`Program ${programId} invoke [1]`, tickLog(5, 12, root(4), root(5), sha(p5.bytes)), `Program ${programId} success`] } }) },
  });
  crank.tickFile = path.dirname(crank.tickFile);
  crank.publishInput = async () => p5;
  assert.equal(await crank.publishAndResolve(5, 2, { committed: 2, revealed: 1 }), true);
  assert.match(logs.join('\n'), /tick index: .*EISDIR/);
  assert.match(logs.join('\n'), /tick 5 resolved on ER/);
});

test('a gap whose scan failed is not tried again', async () => {
  let listed = 0;
  const { crank, logs } = crankWith({ lines: [tick4()], er: { getSignaturesForAddress: async () => { listed++; throw new Error('429'); } } });
  crank.closeTick = async () => {};
  await crank.play(snap(6, 'close'));
  await crank.kickIndexer();
  assert.equal(listed, 1);
  assert.match(logs.join('\n'), /tick index: gap after tick 4: 429/);
  await crank.play(snap(6, 'close'));
  await crank.kickIndexer();
  assert.equal(listed, 1, 'triedGaps: one attempt per gap');
});

/** The ER history of world chunk 0 as getTransaction results, newest first by slot. */
function history(crank, txs) {
  const keys = ['Payer', programId, crank.chain.worldChunks[0].toBase58()];
  const byId = new Map(txs.map(t => [t.signature, {
    slot: t.slot,
    meta: { err: null, computeUnitsConsumed: 9, innerInstructions: [], logMessages: t.ixs.flatMap(logs => [`Program ${programId} invoke [1]`, ...logs, `Program ${programId} success`]) },
    transaction: { message: { accountKeys: keys, instructions: t.ixs.map(() => ({ programIdIndex: 1, accounts: [2, 0], data: '' })) } },
  }]));
  return {
    getSignaturesForAddress: async () => [...txs].sort((a, b) => b.slot - a.slot).map(t => ({ signature: t.signature, slot: t.slot, err: null })),
    getTransaction: async sig => byId.get(sig) ?? null,
  };
}

test('a gap left by an outsider\'s resolve is filled in the background, in chain order, without duplicates', async () => {
  const p5 = published(5), p6 = published(6);
  const r5a = root(51), r5 = root(5), r6 = root(6);
  const txs = [
    { signature: 't4', slot: 40, ixs: [[tickLog(4, 12, root(3), root(4), sha(published(4).bytes))]] },
    { signature: 'in5', slot: 50, ixs: [[data('PS_INPUT', u16(5), u16(0), u16(1), sha(p5.bytes), p5.bytes)]] },
    // The outsider's transaction: [ResolveTick 2, ResolveTick 12] (and a no-op first).
    { signature: 'x5', slot: 51, ixs: [[tickLog(5, 0, root(4), root(4), sha(p5.bytes))], [tickLog(5, 2, root(4), r5a, sha(p5.bytes))], [tickLog(5, 12, r5a, r5, sha(p5.bytes))]] },
    { signature: 't6', slot: 60, ixs: [[tickLog(6, 12, r5, r6, sha(p6.bytes))]] },
  ];
  const { crank } = crankWith({ lines: [tick4()] });
  Object.assign(crank.er, history(crank, txs));
  // The crank resolves tick 6 itself: its part is archived at once (a gap queued).
  crank.publishAndResolve = async open => { crank.archivePart(part('t6', 6, 12, r5, r6), p6, {}); return true; };
  await crank.play(snap(6, 'publish'));
  await crank.kickIndexer();
  const lines = crank.tickRecords();
  assert.deepEqual(lines.map(l => [l.tick, l.to, l.signature]), [[4, 12, 't4'], [5, 2, 'x5'], [5, 12, 'x5'], [6, 12, 't6']]);
  assert.equal(lines[1].preRoot, hex(root(4)), 'chained from tick 4\'s root');
  assert.equal(lines[1].input, p5.input);
  // The other queued gap (tick 4 → the crank's tick 6) finds the same lines: none twice.
  await crank.kickIndexer();
  await crank.kickIndexer();
  assert.equal(crank.tickRecords().length, 4);
});

test('parts: a no-op is not archived; one after an outsider\'s part is, and queues a gap', () => {
  const { crank } = crankWith();
  const p0 = published(0);
  // The first tick-0 part (head null) from the first election's root: no gap.
  crank.archivePart(part('c0', 0, 2, root(0), root(1)), p0, {});
  assert.deepEqual(crank.gaps, []);
  // An outsider ran phases 2..7; the crank's next part starts there.
  crank.archivePart(part('c1', 0, 12, root(2), root(3)), p0, {});
  assert.deepEqual(crank.gaps.map(g => [g.after.signature, g.before.signature]), [['c0', 'c1']]);
  // A no-op part (`to` <= cursor) and a part archived twice: nothing appended.
  crank.archivePart(part('c2', 1, 5, root(3), root(3)), published(1), {});
  crank.archivePart(part('c1', 0, 12, root(2), root(3)), p0, {});
  assert.deepEqual(crank.tickRecords().map(l => l.signature), ['c0', 'c1']);
  assert.equal(crank.head.signature, 'c1');
});
