// The pure SHA-256 (client/src/sha256.mjs) against node:crypto: every size
// around the block and padding boundaries, random sizes, inputs split into
// parts at every offset, and the input forms it accepts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { sha256 } from '../client/src/sha256.mjs';

const node = (...parts) => { const h = createHash('sha256'); for (const p of parts) h.update(p); return h.digest('hex'); };
const hex = b => Buffer.from(b).toString('hex');

test('known answers (FIPS 180-2)', () => {
  assert.equal(hex(sha256()), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  assert.equal(hex(sha256(new TextEncoder().encode('abc'))), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  assert.equal(hex(sha256(new TextEncoder().encode('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq'))),
    '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1');
});

test('every length 0..300 and the padding edges equal node:crypto', () => {
  for (let n = 0; n <= 300; n++) {
    const b = randomBytes(n);
    assert.equal(hex(sha256(b)), node(b), `${n} bytes`);
  }
  for (const n of [55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1000, 4095, 4096, 65536, 1 << 20]) {
    const b = randomBytes(n);
    assert.equal(hex(sha256(new Uint8Array(b))), node(b), `${n} bytes`);
  }
});

test('parts hash as their concatenation, split anywhere', () => {
  for (let trial = 0; trial < 60; trial++) {
    const b = randomBytes(1 + Math.floor(Math.random() * 400));
    const cuts = Array.from({ length: Math.floor(Math.random() * 6) }, () => Math.floor(Math.random() * b.length)).sort((x, y) => x - y);
    const parts = [0, ...cuts, b.length].slice(1).map((end, i, ends) => b.subarray(i ? ends[i - 1] : 0, end));
    assert.equal(hex(sha256(...parts)), node(b));
  }
  const b = randomBytes(200);
  for (let i = 0; i <= b.length; i++) assert.equal(hex(sha256(b.subarray(0, i), b.subarray(i))), node(b), `split at ${i}`);
});

test('input forms: Buffer, subarray view, byte array, string (UTF-8), ArrayBuffer', () => {
  const b = randomBytes(100);
  const view = new Uint8Array(b.buffer, b.byteOffset + 10, 50);
  assert.equal(hex(sha256(view)), node(b.subarray(10, 60)));
  assert.equal(hex(sha256([...b])), node(b));
  assert.equal(hex(sha256('同盟 ok')), node('同盟 ok'));
  assert.equal(hex(sha256(new Uint8Array(b).buffer)), node(b));
  assert.equal(sha256().length, 32);
  assert.ok(sha256() instanceof Uint8Array && !Buffer.isBuffer(sha256()));
  assert.throws(() => sha256(42), TypeError);
});
