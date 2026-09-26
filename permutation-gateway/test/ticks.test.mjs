import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { appendFileSync, mkdtempSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { appendTickLines, assembleInput, LAST_PHASE, nextStops, publishTickInput, readTickLines, resolveInParts, STOPS, tickLine, tickLinesOf } from '../src/ticks.mjs';

const sha = b => createHash('sha256').update(b).digest();
const heavy = () => Object.assign(new Error('resolve: failed'), { logs: ['exceeded CUs meter at BPF instruction'] });

test('nextStops: past the cursor, up to what reached before, furthest first', () => {
  assert.deepEqual(nextStops(0), [...STOPS].reverse());
  assert.deepEqual(nextStops(0, { 0: 5 }), [5, 4, 3, 2, 1]);
  assert.deepEqual(nextStops(5, { 0: 5 }), [12, 11, 10, 9, 8, 7, 6]);
  assert.deepEqual(nextStops(9, { 9: 12 }), [12, 11, 10]);
  assert.deepEqual(nextStops(2, { 2: 3 }), [3], 'any single phase can be a part of its own');
  assert.deepEqual(nextStops(LAST_PHASE), []);
});

test('resolveInParts: shorter parts on heavy failures, remembers the reach, other errors stop it', async () => {
  // Parts cost their phase span; at most 5 phases fit a transaction.
  const tried = [];
  const resolve = cursor => async to => { tried.push(to); if (to - cursor.at > 5) throw heavy(); const r = { cu: to - cursor.at, to }; cursor.at = to; return r; };
  const cursor = { at: 0 };
  const reach = {};
  const archived = [];
  const parts = await resolveInParts({ resolve: resolve(cursor), reach, onPart: (r, to) => archived.push(to) });
  assert.deepEqual(parts.map(p => p.to), [5, 10, 12]);
  assert.deepEqual(archived, [5, 10, 12]);
  assert.deepEqual(reach, { 0: 5, 5: 10, 10: 12 });
  // The next tick starts from the reach: no heavy failure at all.
  tried.length = 0;
  cursor.at = 0;
  await resolveInParts({ resolve: resolve(cursor), reach });
  assert.deepEqual(tried, [5, 10, 12]);
  await assert.rejects(resolveInParts({ resolve: async () => { throw new Error('{"Custom":28}'); } }), /Custom/);
  await assert.rejects(resolveInParts({ resolve: async () => { throw heavy(); } }), /exceeded CUs|failed/);
});

const input = Buffer.from('tick input bytes, published in chunks');
const recs = (tick = 3) => {
  const parts = [input.subarray(0, 10), input.subarray(10, 25), input.subarray(25)];
  return parts.map((bytes, chunk) => ({ tag: 'PS_INPUT', tick, chunk, total: parts.length, hash: sha(input), bytes }));
};

test('assembleInput: any order, first copy of a chunk wins, hash checked, gaps refused', () => {
  const [a, b, c] = recs();
  const out = assembleInput([c, a, b, { ...b, bytes: Buffer.from('ignored') }], ['sc', 'sa', 'sb', 'sb2']);
  assert.equal(out.input, input.toString('hex'));
  assert.equal(out.hash, sha(input).toString('hex'));
  assert.deepEqual(out.signatures, ['sa', 'sb', 'sc']);
  assert.throws(() => assembleInput([a, c]), /chunks 1 of 3 not published/);
  assert.throws(() => assembleInput([a, { ...b, bytes: Buffer.from('tampered.......') }, c]), /does not match its hash/);
  assert.throws(() => assembleInput([a, { ...b, tick: 4 }, c]), /different inputs/);
});

test('publishTickInput: publishes every chunk the first record announces', async () => {
  const all = recs(7);
  const sent = [];
  const out = await publishTickInput({ tick: 7, publishChunk: async chunk => { sent.push(chunk); return { signature: `s${chunk}`, records: [all[chunk]] }; } });
  assert.deepEqual(sent, [0, 1, 2]);
  assert.equal(out.input, input.toString('hex'));
  await assert.rejects(publishTickInput({ tick: 8, publishChunk: async chunk => ({ signature: 'x', records: [all[chunk]] }) }), /unexpected PS_INPUT/);
  await assert.rejects(publishTickInput({ tick: 7, publishChunk: async () => ({ signature: 'x', records: [] }) }), /could not be read/);
});

test('tick index lines: one format for the crank and the reindexer', () => {
  const published = assembleInput(recs(), ['a', 'b', 'c']);
  const rec = { tag: 'PS_TICK', tick: 3, to: 12, preRoot: Buffer.alloc(32, 1), root: Buffer.alloc(32, 2), inputHash: sha(input) };
  const seals = { committed: 22, revealed: 20, commitSignature: 'close' };
  const line = tickLine({ rec, published, signature: 'sig', cu: 1000, seals });
  assert.deepEqual(Object.keys(line), ['tick', 'to', 'preRoot', 'root', 'input', 'inputHash', 'inputSignatures', 'signature', 'cu', 'submitted', 'committed', 'revealed', 'commitSignature']);
  assert.equal(line.submitted, 20, 'submitted = revealed, for older readers');
  const unknown = tickLine({ rec, published, signature: 'sig', cu: 1 });
  assert.deepEqual([unknown.submitted, unknown.committed, unknown.commitSignature], [null, null, null]);
  assert.deepEqual(tickLinesOf({ signature: 'sig', cu: 1000, records: [rec] }, published, seals), [line]);
  assert.throws(() => tickLinesOf({ signature: 'sig', records: [{ ...rec, inputHash: Buffer.alloc(32) }] }, published, {}), /another input/);
  assert.throws(() => tickLinesOf({ signature: 'sig', records: [], fetched: false }, published, {}), /not fetchable/);
  const file = path.join(mkdtempSync(path.join(os.tmpdir(), 'ticks-')), 't.jsonl');
  appendTickLines(file, [line, { ...line, tick: 4 }]);
  assert.deepEqual(readTickLines(file, 4), [{ ...line, tick: 4 }]);
  assert.deepEqual(readTickLines(`${file}.missing`), []);
  // A torn last line (a crash mid-append) is skipped, and the next append
  // starts on a line of its own.
  appendFileSync(file, '{"tick":5,"to":');
  assert.deepEqual(readTickLines(file).map(l => l.tick), [3, 4]);
  appendTickLines(file, [{ ...line, tick: 6 }]);
  assert.deepEqual(readTickLines(file).map(l => l.tick), [3, 4, 6]);
});

test('tickLine: `to` is the stop that ran (an outsider may log a raw to above 12)', () => {
  const published = assembleInput(recs(), ['a', 'b', 'c']);
  const rec = { tag: 'PS_TICK', tick: 3, to: 255, preRoot: Buffer.alloc(32, 1), root: Buffer.alloc(32, 2), inputHash: sha(input) };
  assert.equal(tickLine({ rec, published, signature: 's', cu: 1 }).to, 12);
  assert.equal(tickLine({ rec: { ...rec, to: 7 }, published, signature: 's', cu: 1 }).to, 7);
});
