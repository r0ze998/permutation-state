// The Buffer-free byte helpers (client/src/bytes.mjs) against Node's Buffer,
// which they replace: hex (fromHex exactly as lenient: it stops at the first
// bad pair and never throws), base64, integers, and the JSON replacer.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes as nodeRandom } from 'node:crypto';
import { concat, equal, fromBase64, fromHex, jsonSafe, randomBytes, toBase64, toHex, toJson, u16le, u32le, u64le, utf8 } from '../client/src/bytes.mjs';

const plain = b => { assert.ok(b instanceof Uint8Array && !Buffer.isBuffer(b), 'a plain Uint8Array'); return b; };

test('toHex equals Buffer for bytes, views, arrays (each & 0xff) and strings (UTF-8)', () => {
  for (let n = 0; n < 70; n++) {
    const b = nodeRandom(n);
    assert.equal(toHex(b), b.toString('hex'));
    assert.equal(toHex(new Uint8Array(b)), b.toString('hex'));
    assert.equal(toHex([...b]), b.toString('hex'));
  }
  const odd = [256, -1, 1.5, 'a', NaN, '7', 511];
  assert.equal(toHex(odd), Buffer.from(odd).toString('hex'));
  assert.equal(toHex('同盟 ok'), Buffer.from('同盟 ok').toString('hex'));
  const base = nodeRandom(40);
  assert.equal(toHex(base.subarray(5, 17)), base.subarray(5, 17).toString('hex'));
  assert.equal(toHex(new Uint8Array(base).buffer), base.toString('hex'));
  assert.equal(toHex(new Uint16Array([0x1234, 0xff])), Buffer.from(new Uint16Array([0x1234, 0xff])).toString('hex'));
  assert.throws(() => toHex(5), TypeError);
  assert.throws(() => toHex(null), TypeError);
});

test('fromHex is exactly as lenient as Buffer.from(h, "hex") and never throws', () => {
  const cases = ['', 'ab', 'AB', 'aBcD', 'abc', 'zz12', '12zz34', '1z', '0x12', ' 12', '12 34', 'šŢ', 'İ1', '12Ā', 'g0', '0g', 'ff'.repeat(40) + 'f'];
  const alphabet = '0123456789abcdefABCDEFgxz -+Gššİあ';
  for (let i = 0; i < 3000; i++) {
    const len = Math.floor(Math.random() * 12);
    cases.push(Array.from({ length: len }, () => (Math.random() < 0.8 ? '0123456789abcdefABCDEF' : alphabet)[Math.floor(Math.random() * (Math.random() < 0.8 ? 22 : alphabet.length))]).join(''));
  }
  for (const h of cases) assert.deepEqual(plain(fromHex(h)), new Uint8Array(Buffer.from(h, 'hex')), JSON.stringify(h));
  // Non-strings: bytes are copied as Buffer.from copies them; anything else is no bytes (Buffer would throw).
  const b = nodeRandom(8);
  assert.deepEqual(plain(fromHex(b)), new Uint8Array(b));
  assert.deepEqual(fromHex([1, 2, 300]), Uint8Array.of(1, 2, 44));
  for (const x of [undefined, null, 5, {}, true]) assert.deepEqual(fromHex(x), new Uint8Array(0), String(x));
  // A 32-byte digest check stays a length check (relay.mjs /submit: 400, not 500).
  assert.equal(fromHex('zz'.repeat(32)).length, 0);
  assert.equal(fromHex('ab'.repeat(32)).length, 32);
});

test('base64: toBase64 equals Buffer; fromBase64 reads standard, URL-safe, unpadded and spaced input', () => {
  for (let n = 0; n < 100; n++) {
    const b = nodeRandom(n);
    const s = b.toString('base64');
    assert.equal(toBase64(b), s);
    assert.deepEqual(plain(fromBase64(s)), new Uint8Array(b));
    assert.deepEqual(fromBase64(b.toString('base64url')), new Uint8Array(b));
    assert.deepEqual(fromBase64(s.replace(/=+$/, '')), new Uint8Array(b));
    assert.deepEqual(fromBase64(s.replace(/(.{4})/g, '$1\n ')), new Uint8Array(b));
  }
  assert.equal(toBase64([]), '');
  assert.deepEqual(fromBase64(''), new Uint8Array(0));
  assert.throws(() => fromBase64('ab!d'), /base64: invalid character/);
  assert.throws(() => fromBase64(7), TypeError);
});

test('integers: u16le/u32le/u64le as Buffer writes them; out of range throws', () => {
  for (const v of [0, 1, 255, 256, 0x1234, 0xffff]) {
    const b = Buffer.alloc(2); b.writeUInt16LE(v);
    assert.deepEqual(plain(u16le(v)), new Uint8Array(b));
  }
  for (const v of [0, 1, 0x12345678, 0xfffffffe, 0xffffffff]) {
    const b = Buffer.alloc(4); b.writeUInt32LE(v);
    assert.deepEqual(plain(u32le(v)), new Uint8Array(b));
  }
  for (const v of [0n, 1n, 1_790_000_000_123n, 0xffffffffffffffffn, 42]) {
    const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(v));
    assert.deepEqual(plain(u64le(v)), new Uint8Array(b));
  }
  assert.deepEqual(u16le('7'), Uint8Array.of(7, 0));
  for (const v of [-1, 0x10000, 1.5, NaN, undefined]) assert.throws(() => u16le(v), RangeError, String(v));
  for (const v of [-1, 2 ** 32, 0.5]) assert.throws(() => u32le(v), RangeError, String(v));
});

test('concat, equal, utf8, randomBytes', () => {
  assert.deepEqual(plain(concat(Uint8Array.of(1), [2, 3], Buffer.from([4]), 'A')), Uint8Array.of(1, 2, 3, 4, 65));
  assert.deepEqual(concat(), new Uint8Array(0));
  assert.ok(equal(Uint8Array.of(1, 2), [1, 2]));
  assert.ok(equal(Buffer.from('ab'), 'ab'));
  assert.ok(!equal(Uint8Array.of(1, 2), Uint8Array.of(1, 2, 3)));
  assert.ok(!equal(Uint8Array.of(1, 2), Uint8Array.of(1, 3)));
  assert.deepEqual(plain(utf8('é')), Uint8Array.of(0xc3, 0xa9));
  const r = plain(randomBytes(32));
  assert.equal(r.length, 32);
  assert.notDeepEqual(r, randomBytes(32));
  assert.equal(randomBytes(0).length, 0);
  const big = randomBytes(70_000); // more than one getRandomValues call
  assert.equal(big.length, 70_000);
  assert.ok(big.subarray(65_536).some(x => x !== 0));
});

test('JSON: bigints as decimal strings, plain bytes as hex, Buffers as before', () => {
  assert.equal(toJson({ a: 1n, b: Uint8Array.of(1, 255), c: 'x' }), '{"a":"1","b":"01ff","c":"x"}');
  assert.equal(JSON.stringify({ b: Buffer.from([1]) }, jsonSafe), '{"b":{"type":"Buffer","data":[1]}}');
});
