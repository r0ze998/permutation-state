import { test } from 'node:test';
import assert from 'node:assert/strict';
import { encodedLen, MAX_REVEALS, packBatch } from '../client/src/batch.mjs';
import { BATCH_BYTES, isFree, MAX_BATCH_ORDERS, MAX_FREE_ORDERS } from '../client/src/codec.mjs';
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
  assert.equal(over.reason, 'bytes');
});

// The batch caps (WP04): at most MAX_FREE_ORDERS free orders (reveals are
// free), and MAX_BATCH_ORDERS orders counting the adopted proposals' ones.
const free = n => Array.from({ length: n }, (_, i) => ({ type: 'ConsentWar', civ: i % 6 }));

test('8 free orders fit with no room for a reveal; 6 free take 2 of 3 pending reveals; 9 free do not fit', () => {
  const eight = packBatch({ orders: free(MAX_FREE_ORDERS), pending: [pending(1)] });
  assert.deepEqual([eight.fits, eight.reveals.length, eight.reason], [true, 0, null]);
  const six = packBatch({ orders: free(6), pending: [pending(3), pending(1), pending(2)] });
  assert.deepEqual([six.fits, six.reveals.map(d => d.tick), six.reason], [true, [1, 2], null]);
  const nine = packBatch({ orders: free(MAX_FREE_ORDERS + 1) });
  assert.deepEqual([nine.fits, nine.reason], [false, 'free']);
  assert.ok(isFree({ type: 'RevealRationale' }) && isFree({ type: 'ExchangeOrder' }) && !isFree(research));
});

test('orders and the adopted proposals\' orders together stay within MAX_BATCH_ORDERS', () => {
  const costed = n => Array.from({ length: n }, (_, i) => ({ type: 'Purchase', city: i, gold: 1 }));
  const over = packBatch({ orders: costed(20), adopted: costed(5) });
  assert.deepEqual([over.fits, over.reason], [false, 'orders']);
  const full = packBatch({ orders: costed(20), adopted: costed(4), pending: [pending(1)] });
  assert.deepEqual([full.fits, full.reveals.length], [true, 0], 'no reveal past the cap');
  assert.equal(packBatch({ orders: costed(20), adopted: costed(3), pending: [pending(1), pending(2)] }).reveals.length, 1);
  // Free orders of adopted proposals count too.
  assert.equal(packBatch({ orders: free(4), adopted: free(5) }).reason, 'free');
  assert.equal(MAX_BATCH_ORDERS, 24);
});
