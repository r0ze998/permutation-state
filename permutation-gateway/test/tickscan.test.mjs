// tickscan.mjs: the program's records with the instruction that logged
// them, tick lines that follow the chain of roots (several records per
// transaction, no-op records, other seasons' closes), and scans of an
// address's history in a slot window.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { Keypair } from '@solana/web3.js';
import { records } from '../src/send.mjs';
import { buildTickLines, chainLines, programLog, scanRecords, txRecords } from '../src/tickscan.mjs';

const OURS = Keypair.generate().publicKey.toBase58();
const OTHER = Keypair.generate().publicKey.toBase58();
const WRAP = Keypair.generate().publicKey.toBase58();
const CHUNK0 = 'Chunk0OfThisSeason1111111111111111111111111';
const FOREIGN0 = 'Chunk0OfAnotherSeason11111111111111111111';

const sha = b => createHash('sha256').update(b).digest();
const u16 = n => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };
const u32 = n => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const root = n => Buffer.alloc(32, n);
const data = (...fields) => `Program data: ${fields.map(f => Buffer.from(f).toString('base64')).join(' ')}`;
const invoke = (id, d) => `Program ${id} invoke [${d}]`;
const ok = id => `Program ${id} success`;

const tickLog = (tick, to, pre, post, hash) => data('PS_TICK', u16(tick), [to], pre, post, hash);
const inputLog = (tick, bytes) => data('PS_INPUT', u16(tick), u16(0), u16(1), sha(bytes), bytes);
const commitsLog = (tick, n) => data('PS_COMMITS', u16(tick), Buffer.concat([u32(n), ...Array.from({ length: n }, (_, i) => Buffer.concat([u16(0), Buffer.from([i]), u32(i), root(9)]))]));
const saltsLog = (tick, pre, n) => data('PS_SALTS', u16(tick), pre, Buffer.concat([u32(n), ...Array.from({ length: n }, (_, i) => Buffer.concat([u16(0), Buffer.from([i]), root(8)]))]));

/** A getTransaction result: one top-level instruction of ours per entry of `ixs` ({accounts, logs}). */
function tx(slot, ixs, { inner } = {}) {
  const keys = ['Payer', OURS, WRAP, CHUNK0, FOREIGN0];
  return {
    slot,
    meta: { err: null, computeUnitsConsumed: 7, logMessages: ixs.flatMap(ix => [invoke(OURS, 1), ...ix.logs, ok(OURS)]), innerInstructions: inner ?? [] },
    transaction: { message: { accountKeys: keys, instructions: ixs.map(ix => ({ programIdIndex: 1, accounts: ix.accounts.map(a => keys.indexOf(a)), data: '' })) } },
  };
}

test('records(logs, program): only the program\'s own frames; without a program, as before', () => {
  const forged = tickLog(1, 12, root(1), root(2), root(3));
  const logs = [invoke(OTHER, 1), forged, ok(OTHER), invoke(OURS, 1), data('PS_OPEN', root(4)), ok(OURS)];
  assert.deepEqual(records(logs, OURS).map(r => r.tag), ['PS_OPEN']);
  assert.deepEqual(records(logs).map(r => r.tag), ['PS_TICK', 'PS_OPEN']);
  // A program cannot print a frame line: `Program data: invoke [1]` is data.
  const tricked = [invoke(OURS, 1), 'Program log: Program Other invoke [2]', data('invoke'), data('PS_OPEN', root(4)), ok(OURS)];
  assert.deepEqual(records(tricked, OURS).map(r => r.tag), ['PS_OPEN']);
});

test('txRecords: records with the accounts of the instruction that logged them', () => {
  const t = tx(5, [{ accounts: [CHUNK0], logs: [] }, { accounts: [FOREIGN0, 'Payer'], logs: [data('PS_OPEN', root(4))] }]);
  assert.deepEqual(txRecords(t, OURS).map(e => e.accounts), [[FOREIGN0, 'Payer']]);
  // A CPI from a wrapper into our program, through innerInstructions.
  const keys = ['Payer', OURS, WRAP, CHUNK0];
  const cpi = {
    slot: 6,
    meta: { err: null, logMessages: [invoke(WRAP, 1), invoke(OURS, 2), data('PS_OPEN', root(4)), ok(OURS), ok(WRAP)],
      innerInstructions: [{ index: 0, instructions: [{ programIdIndex: 1, accounts: [3, 0], data: '' }] }] },
    transaction: { message: { accountKeys: keys, instructions: [{ programIdIndex: 2, accounts: [1, 3], data: '' }] } },
  };
  assert.deepEqual(txRecords(cpi, OURS)[0].accounts, [CHUNK0, 'Payer']);
  assert.equal(txRecords({ ...cpi, meta: { ...cpi.meta, innerInstructions: null } }, OURS)[0].accounts, null);
  // Cut logs: not attributable. A failed transaction: nothing.
  const cut = { ...t, meta: { ...t.meta, logMessages: [invoke(OURS, 1), 'Log truncated', data('PS_OPEN', root(4))] } };
  assert.equal(txRecords(cut, OURS)[0].accounts, null);
  assert.deepEqual(txRecords({ ...t, meta: { ...t.meta, err: { InstructionError: [0, 'x'] } } }, OURS), []);
});

/** An outsider's tick: close, input, then one transaction `[ResolveTick 2, ResolveTick 12]` with a no-op first. */
function outsiderTick({ tick = 5, from = root(1), foreignClose = false, boundClose = true } = {}) {
  const input = Buffer.from(`input of tick ${tick}`);
  const txs = [];
  if (foreignClose) txs.push(tx(9, [{ accounts: [FOREIGN0, CHUNK0], logs: [commitsLog(tick, 1)] }]));
  if (boundClose) txs.push(tx(10, [{ accounts: [CHUNK0], logs: [commitsLog(tick, 2)] }]));
  txs.push(tx(11, [{ accounts: [CHUNK0], logs: [saltsLog(tick, from, 2), inputLog(tick, input)] }]));
  txs.push(tx(12, [
    { accounts: [CHUNK0], logs: [tickLog(tick, 0, from, from, sha(input))] },
    { accounts: [CHUNK0], logs: [tickLog(tick, 2, from, root(2), sha(input))] },
    { accounts: [CHUNK0], logs: [tickLog(tick, 255, root(2), root(3), sha(input))] },
  ]));
  return txs.map((t, i) => ({ signature: `sig${t.slot}-${i}`, slot: t.slot, cu: 7, emitted: txRecords(t, OURS) }));
}

test('buildTickLines: one line per advancing record, `to` normalized, this season\'s close only', () => {
  const txs = outsiderTick();
  const lines = buildTickLines(txs, { anchor: CHUNK0 });
  assert.deepEqual(lines.map(l => [l.tick, l.to, l.signature]), [[5, 2, 'sig12-2'], [5, 12, 'sig12-2']]);
  assert.equal(lines[0].commitSignature, 'sig10-0');
  assert.deepEqual([lines[0].committed, lines[0].revealed], [2, 2]);
  assert.equal(lines[0].input, Buffer.from('input of tick 5').toString('hex'));
  // Another season's close listing this chunk 0: never this season's.
  const both = buildTickLines(outsiderTick({ foreignClose: true }), { anchor: CHUNK0 });
  assert.equal(both[0].commitSignature, 'sig10-1');
  const onlyForeign = buildTickLines(outsiderTick({ foreignClose: true, boundClose: false }), { anchor: CHUNK0 });
  assert.equal(onlyForeign[0].commitSignature, null);
  // No input published, none known: no line; a known line's input serves.
  const noInput = outsiderTick().filter(t => t.slot !== 11);
  assert.deepEqual(buildTickLines(noInput, { anchor: CHUNK0 }), []);
  assert.equal(buildTickLines(noInput, { anchor: CHUNK0, knownLines: [lines[0]] }).length, 2);
});

test('chainLines: from the first election\'s root, past no-op lines, up to a gap', () => {
  const l = (tick, to, pre, post) => ({ tick, to, preRoot: pre, root: post });
  const lines = [l(0, 0, 'a', 'a'), l(0, 12, 'a', 'b'), l(1, 5, 'b', 'c'), l(1, 12, 'c', 'd'), l(2, 12, 'x', 'y'), l(1, 12, 'b', 'z')];
  assert.deepEqual(chainLines(lines, 'a').map(x => x.root), ['b', 'c', 'd']);
  assert.deepEqual(chainLines(lines, 'q'), []);
  assert.deepEqual(chainLines([l(0, 12, 'a', 'b'), l(0, 12, 'b', 'a')], 'a').length, 2, 'a cycle ends');
});

test('scanRecords: pages backwards from `start`, stops below `lo`, `maxPages` bounds it, `concurrency` holds', async () => {
  // 2500 signatures, slot = 3000 - i (newest first), every 7th failed.
  const all = Array.from({ length: 2500 }, (_, i) => ({ signature: `s${i}`, slot: 3000 - i, err: i % 7 === 0 ? { x: 1 } : null }));
  const calls = [];
  let running = 0, peak = 0;
  const connection = {
    getSignaturesForAddress: async (_addr, { before, limit }) => {
      calls.push(before ?? null);
      const from = before ? all.findIndex(s => s.signature === before) + 1 : 0;
      return all.slice(from, from + limit);
    },
    getTransaction: async sig => {
      running++; peak = Math.max(peak, running);
      await new Promise(r => setTimeout(r, 1));
      running--;
      const slot = all.find(s => s.signature === sig).slot;
      return tx(slot, [{ accounts: [CHUNK0], logs: [data('PS_OPEN', root(1))] }]);
    },
  };
  const address = Keypair.generate().publicKey;
  const r = await scanRecords({ connection, address, program: OURS, lo: 1200, hi: 2800, start: 's100', concurrency: 3 });
  assert.deepEqual(calls, ['s100', 's1100']);
  assert.ok(r.complete);
  assert.ok(r.txs.every(t => t.slot >= 1200 && t.slot <= 2800));
  assert.equal(r.txs.length, all.filter((s, i) => i > 100 && s.slot >= 1200 && s.slot <= 2800 && !s.err).length);
  assert.ok(r.txs.every((t, i) => i === 0 || r.txs[i - 1].slot <= t.slot), 'oldest first');
  assert.equal(r.txs[0].emitted[0].accounts[0], CHUNK0);
  assert.equal(peak, 3);
  calls.length = 0;
  const one = await scanRecords({ connection, address, program: OURS, maxPages: 1, concurrency: 50 });
  assert.deepEqual([calls.length, one.complete], [1, false]);
});

test('programLog: frames and CPI indexes', () => {
  const logs = [invoke(WRAP, 1), invoke(OTHER, 2), ok(OTHER), invoke(OURS, 2), data('PS_OPEN', root(1)), ok(OURS), ok(WRAP), invoke(OURS, 1), data('PS_OPEN', root(1)), ok(OURS)];
  assert.deepEqual(programLog(logs, OURS).map(x => [x.top, x.inner]), [[0, 1], [1, null]]);
});
