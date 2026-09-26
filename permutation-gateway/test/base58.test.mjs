// The pure base58 (client/src/base58.mjs) against the bs58 package web3.js
// uses: random bytes with and without leading zeros, keys, round trips, and
// refusal of characters outside the alphabet.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import bs58 from 'bs58';
import { Keypair } from '@solana/web3.js';
import { decode, encode } from '../client/src/base58.mjs';

test('encode and decode equal bs58 for random bytes, leading zeros included', () => {
  for (let trial = 0; trial < 2000; trial++) {
    const n = Math.floor(Math.random() * 80);
    const b = randomBytes(n);
    for (let i = 0; i < Math.floor(Math.random() * 4) && i < n; i++) b[i] = 0;
    const s = bs58.encode(b);
    assert.equal(encode(b), s, b.toString('hex'));
    assert.equal(encode([...b]), s);
    assert.deepEqual(decode(s), new Uint8Array(b));
    assert.deepEqual(Buffer.from(decode(s)), bs58.decode(s));
  }
});

test('edge cases: empty, all zeros, 0xff runs, public keys', () => {
  assert.equal(encode(new Uint8Array(0)), '');
  assert.deepEqual(decode(''), new Uint8Array(0));
  for (const n of [1, 2, 32, 64]) {
    assert.equal(encode(new Uint8Array(n)), '1'.repeat(n));
    assert.deepEqual(decode('1'.repeat(n)), new Uint8Array(n));
    const ff = new Uint8Array(n).fill(0xff);
    assert.equal(encode(ff), bs58.encode(Buffer.from(ff)));
  }
  for (let i = 0; i < 100; i++) {
    const k = Keypair.generate().publicKey;
    assert.equal(encode(k.toBytes()), k.toBase58());
    assert.deepEqual(decode(k.toBase58()), k.toBytes());
  }
  assert.equal(encode(new Uint8Array(32)), '11111111111111111111111111111111'); // the system program
});

test('characters outside the alphabet throw (0 O I l, punctuation, space, non-ASCII)', () => {
  for (const s of ['0', 'O', 'I', 'l', 'abc0', '+', '/', ' 1', '1 ', 'ä', '１']) {
    assert.throws(() => decode(s), /base58: invalid character/, JSON.stringify(s));
    assert.throws(() => bs58.decode(s), undefined, `bs58 also refuses ${JSON.stringify(s)}`);
  }
  assert.throws(() => decode(42), TypeError);
});
