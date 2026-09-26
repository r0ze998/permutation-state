// SHA-256 (FIPS 180-4), synchronous and dependency-free, so the same code
// hashes commitments, digests and PDAs in Node and in the browser (WebCrypto's
// digest is async). Checked against node:crypto in test/sha256.test.mjs.

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
const IV = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

const utf8 = new TextEncoder();
/** One input part as bytes: bytes as they are, a string as UTF-8, a byte array by value. */
function asBytes(p) {
  if (p instanceof Uint8Array) return p;
  if (typeof p === 'string') return utf8.encode(p);
  if (p instanceof ArrayBuffer) return new Uint8Array(p);
  if (ArrayBuffer.isView(p)) return new Uint8Array(p.buffer, p.byteOffset, p.byteLength);
  if (Array.isArray(p)) return Uint8Array.from(p);
  throw new TypeError('sha256: parts must be bytes or strings');
}

/** Process one 64-byte block of `b` at `o` into the state `H` (W: scratch). */
function compress(H, W, b, o) {
  for (let i = 0; i < 16; i++, o += 4) W[i] = (b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3];
  for (let i = 16; i < 64; i++) {
    const w15 = W[i - 15], w2 = W[i - 2];
    const s0 = ((w15 >>> 7) | (w15 << 25)) ^ ((w15 >>> 18) | (w15 << 14)) ^ (w15 >>> 3);
    const s1 = ((w2 >>> 17) | (w2 << 15)) ^ ((w2 >>> 19) | (w2 << 13)) ^ (w2 >>> 10);
    W[i] = (W[i - 16] + s0 + W[i - 7] + s1) | 0;
  }
  let a = H[0], bb = H[1], c = H[2], d = H[3], e = H[4], f = H[5], g = H[6], h = H[7];
  for (let i = 0; i < 64; i++) {
    const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
    const t1 = (h + S1 + ((e & f) ^ (~e & g)) + K[i] + W[i]) | 0;
    const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
    const t2 = (S0 + ((a & bb) ^ (a & c) ^ (bb & c))) | 0;
    h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = bb; bb = a; a = (t1 + t2) | 0;
  }
  H[0] = (H[0] + a) | 0; H[1] = (H[1] + bb) | 0; H[2] = (H[2] + c) | 0; H[3] = (H[3] + d) | 0;
  H[4] = (H[4] + e) | 0; H[5] = (H[5] + f) | 0; H[6] = (H[6] + g) | 0; H[7] = (H[7] + h) | 0;
}

/**
 * SHA-256 of the concatenation of `parts` (Uint8Arrays, other byte views,
 * byte arrays or strings as UTF-8), as a 32-byte Uint8Array.
 */
export function sha256(...parts) {
  const H = new Int32Array(IV);
  const W = new Int32Array(64);
  const block = new Uint8Array(64);
  let fill = 0;
  let total = 0;
  for (const part of parts) {
    const p = asBytes(part);
    total += p.length;
    let i = 0;
    if (fill) {
      i = Math.min(64 - fill, p.length);
      block.set(p.subarray(0, i), fill);
      fill += i;
      if (fill < 64) continue;
      compress(H, W, block, 0);
      fill = 0;
    }
    for (; i + 64 <= p.length; i += 64) compress(H, W, p, i);
    if (i < p.length) { block.set(p.subarray(i), 0); fill = p.length - i; }
  }
  // Padding: 0x80, zeros, then the length in bits as a big-endian u64.
  block[fill++] = 0x80;
  if (fill > 56) { block.fill(0, fill); compress(H, W, block, 0); fill = 0; }
  block.fill(0, fill, 56);
  const hi = Math.floor(total / 0x20000000);
  const lo = (total * 8) >>> 0;
  block[56] = hi >>> 24; block[57] = hi >>> 16; block[58] = hi >>> 8; block[59] = hi;
  block[60] = lo >>> 24; block[61] = lo >>> 16; block[62] = lo >>> 8; block[63] = lo;
  compress(H, W, block, 0);
  const out = new Uint8Array(32);
  for (let i = 0; i < 8; i++) {
    out[4 * i] = H[i] >>> 24; out[4 * i + 1] = H[i] >>> 16; out[4 * i + 2] = H[i] >>> 8; out[4 * i + 3] = H[i];
  }
  return out;
}
