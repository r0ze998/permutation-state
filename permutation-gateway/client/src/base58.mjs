// Base58 (the Bitcoin alphabet, as Solana writes keys, blockhashes and
// signatures), dependency-free. Same output as the `bs58` package
// (test/base58.test.mjs).

const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const DIGIT = new Int8Array(128).fill(-1);
for (let i = 0; i < ALPHABET.length; i++) DIGIT[ALPHABET.charCodeAt(i)] = i;

/** Bytes (a Uint8Array or a byte array) as base58; each leading zero byte is a '1'. */
export function encode(bytes) {
  const src = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  let zeros = 0;
  while (zeros < src.length && src[zeros] === 0) zeros++;
  // log(256) / log(58) ≈ 1.37 base58 digits per byte, rounded up.
  const size = (((src.length - zeros) * 138) / 100 + 1) >>> 0;
  const digits = new Uint8Array(size);
  let length = 0;
  for (let i = zeros; i < src.length; i++) {
    let carry = src[i];
    let j = 0;
    for (let k = size - 1; (carry !== 0 || j < length) && k >= 0; k--, j++) {
      carry += 256 * digits[k];
      digits[k] = carry % 58;
      carry = (carry / 58) | 0;
    }
    length = j;
  }
  let it = size - length;
  while (it < size && digits[it] === 0) it++;
  let out = '1'.repeat(zeros);
  for (; it < size; it++) out += ALPHABET[digits[it]];
  return out;
}

/** A base58 string as bytes; throws on any character outside the alphabet. */
export function decode(str) {
  if (typeof str !== 'string') throw new TypeError('base58: expected a string');
  let zeros = 0;
  while (zeros < str.length && str[zeros] === '1') zeros++;
  // log(58) / log(256) ≈ 0.733 bytes per base58 digit, rounded up.
  const size = (((str.length - zeros) * 733) / 1000 + 1) >>> 0;
  const bytes = new Uint8Array(size);
  let length = 0;
  for (let i = zeros; i < str.length; i++) {
    const c = str.charCodeAt(i);
    let carry = c < 128 ? DIGIT[c] : -1;
    if (carry < 0) throw new Error(`base58: invalid character ${JSON.stringify(str[i])}`);
    let j = 0;
    for (let k = size - 1; (carry !== 0 || j < length) && k >= 0; k--, j++) {
      carry += 58 * bytes[k];
      bytes[k] = carry & 0xff;
      carry >>= 8;
    }
    length = j;
  }
  let it = size - length;
  while (it < size && bytes[it] === 0) it++;
  const out = new Uint8Array(zeros + size - it);
  out.set(bytes.subarray(it), zeros);
  return out;
}
