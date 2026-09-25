import { test } from 'node:test';
import assert from 'node:assert/strict';
import { encodedLen, MAX_REVEALS, packBatch } from '../client/src/batch.mjs';
import { BATCH_BYTES } from '../client/src/codec.mjs';
import { revealOrder } from '../client/src/decision.mjs';

const pending = tick => ({ tick, role: 'General', policy: 'p/v1', salt: '00'.repeat(16), text: 'x'.repeat(100) });
const research = { type: 'SetResearch', techs: ['Writing'] };

test('own orders first, then the oldest reveals that fit, at most MAX_REVEALS', () => {
  const b = packBatch({ orders: [research], pending: [pending(5), pending(3), pending(4), pending(2)] });
  assert.equal(b.fits, true);
  assert.deepEqual(b.reveals.map(d => d.tick), [2, 3, 4].slice(0, MAX_REVEALS));
  assert.deepEqual(b.orders[0], research);
  assert.deepEqual(b.orders.slice(1), b.reveals.map(revealOrder));
  assert.equal(b.used, b.orders.reduce((n, o) => n + encodedLen(o), 0));
});

test('reveals stop at the byte budget; own orders over it do not fit', () => {
  const big = { tick: 1, role: 'General', policy: 'p', salt: '00'.repeat(16), text: 'y'.repeat(500) };
  const b = packBatch({ orders: [research], pending: [big, { ...big, tick: 2 }], maxBytes: 600 });
  assert.equal(b.reveals.length, 1);
  assert.ok(b.used <= 600);
  const many = Array.from({ length: 200 }, () => ({ type: 'Purchase', city: 1, gold: 9 }));
  const over = packBatch({ orders: many, pending: [pending(1)] });
  assert.equal(over.fits, false);
  assert.equal(over.reveals.length, 0);
  assert.ok(over.used > BATCH_BYTES);
});
