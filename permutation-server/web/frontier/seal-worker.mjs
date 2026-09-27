// The seal's pairing work, in a module Web Worker (web design §8): tlock's
// Boneh–Franklin IBE with the Fujisaki–Okamoto transform on G2 with the
// RFC 9380 G1 hash (tlock `encryptOnG2RFC9380`, quicknet), carrying a
// 16-byte key k; the 37-byte march body is XORed with the PS-KS keystream
// of k. Seal = U (96, compressed G2) ‖ V (16) ‖ W (16) ‖ body (37) = 165 B
// (the "compact-16" envelope of permutation-rules `frontier::seal`).
//
// Every seal is self-audited before it leaves the worker: the FO relation
// the program's opener checks (U = H3(σ, k)·G2, V = σ ⊕ H2(e(Q_id, pk)^r),
// W = k ⊕ H4(σ)), the body reopened with k, the plaintext re-packed and
// re-validated, the commitment and the seal root recomputed. A seal that
// fails is never returned. k and σ are drawn here and zeroed after use;
// they are never stored or posted (the salt and the plaintext are what a
// self-reveal needs).
//
// The module is also imported directly (tests, and the main-thread
// fallback where module workers are unavailable); the message handler is
// installed only inside a worker.
import { bls12_381 as bls } from '../sdk/vendor/noble/curves/bls12-381.mjs';
import { sha256 as nobleSha256 } from '../sdk/vendor/noble/hashes/sha2.mjs';
import { bodyXor, commit, ctHash, pack, PLAIN_LEN, saltOf, SEAL_LEN, sealRoot, sealRound, unpack, validate } from './seal.mjs';

export const DST_G1 = 'BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_';
export const K_LEN = 16;
const enc = new TextEncoder();
const G2 = bls.G2.ProjectivePoint;
const G1 = bls.G1.ProjectivePoint;
const Fr = bls.fields.Fr;

function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
const h = (...parts) => nobleSha256(concat(...parts.map(p => (typeof p === 'string' ? enc.encode(p) : p))));
const xor = (a, b) => { if (a.length !== b.length) throw new Error('xor: lengths'); return a.map((x, i) => x ^ b[i]); };
const hexToBytes = s => Uint8Array.from(s.match(/../g) ?? [], x => parseInt(x, 16));
const asBytes = x => (typeof x === 'string' ? hexToBytes(x) : Uint8Array.from(x));
const bigBE = b => b.reduce((n, x) => (n << 8n) | BigInt(x), 0n);

/** tlock's identity for a round: sha256(round as u64 big-endian). */
export function roundIdentity(round) {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setBigUint64(0, BigInt(round), false);
  return h(b);
}

/** H3(σ, msg): a scalar from sha256(i_le16 ‖ sha256("IBE-H3" ‖ σ ‖ msg)), top bit dropped, first i below r. */
export function h3(sigma, msg) {
  const base = h('IBE-H3', sigma, msg);
  for (let i = 1; i < 65535; i++) {
    const d = h(Uint8Array.of(i & 0xff, i >> 8), base);
    d[0] >>= 1;
    const n = bigBE(d);
    if (n < Fr.ORDER) return n;
  }
  throw new Error('h3: no scalar');
}
/** H4(σ): sha256("IBE-H4" ‖ σ), first `len` bytes. */
export const h4 = (sigma, len) => h('IBE-H4', sigma).slice(0, len);

// GT element bytes as tlock serialises them: Fp12 = (c1, c0) of Fp6, Fp6 =
// (c2, c1, c0) of Fp2, Fp2 = (c1, c0) of Fp, each Fp 48 bytes big-endian.
const fpBytes = x => hexToBytes(x.toString(16).padStart(96, '0'));
const fp2Bytes = x => concat(fpBytes(x.c1), fpBytes(x.c0));
const fp6Bytes = x => concat(fp2Bytes(x.c2), fp2Bytes(x.c1), fp2Bytes(x.c0));
const fp12Bytes = x => concat(fp6Bytes(x.c1), fp6Bytes(x.c0));
/** H2(g): sha256("IBE-H2" ‖ g), first `len` bytes. */
export const gtHash = (gt, len) => h('IBE-H2', fp12Bytes(gt)).slice(0, len);

/** The G2 public key (96-byte compressed hex or bytes) as a point; throws unless valid and in the subgroup. */
export function publicKeyPoint(pk) {
  const p = G2.fromHex(asBytes(pk));
  p.assertValidity();
  return p;
}

function maskFor(pk, round, r) {
  const qid = bls.G1.hashToCurve(roundIdentity(round), { DST: DST_G1 });
  const gid = bls.pairing(qid, publicKeyPoint(pk));
  return gtHash(bls.fields.Fp12.pow(gid, r), K_LEN);
}

/** IBE-encrypt the 16-byte key `k` to `round` under `pk` with the given σ: {U, V, W}. */
export function ibeEncrypt(pk, round, k, sigma) {
  if (k.length !== K_LEN || sigma.length !== K_LEN) throw new Error('k and σ are 16 bytes');
  const r = h3(sigma, k);
  const U = G2.BASE.multiply(r).toRawBytes(true);
  const V = xor(sigma, maskFor(pk, round, r));
  const W = xor(k, h4(sigma, K_LEN));
  return { U, V, W };
}

/**
 * Open a seal with the round's signature (48-byte compressed G1): the FO
 * check, then the body. Returns `{k, plain, salt, commit}`; throws
 * `{code: 'BadPoint'|'FoCheck'}` like the program's codes 2 and 1/3.
 */
export function openSeal(seal, signature) {
  const s = asBytes(seal);
  if (s.length !== SEAL_LEN) throw Object.assign(new Error('a seal is 165 bytes'), { code: 'BadData' });
  let U, sig;
  try {
    U = G2.fromHex(s.subarray(0, 96));
    U.assertValidity();
    sig = G1.fromHex(asBytes(signature));
    sig.assertValidity();
  } catch (e) {
    throw Object.assign(new Error(`bad point: ${e.message}`), { code: 'BadPoint' });
  }
  const sigma = xor(s.slice(96, 112), gtHash(bls.pairing(sig, U), K_LEN));
  const k = xor(s.slice(112, 128), h4(sigma, K_LEN));
  if (!G2.BASE.multiply(h3(sigma, k)).equals(U)) throw Object.assign(new Error('FO check failed'), { code: 'FoCheck' });
  const plain = bodyXor(k, s.slice(128));
  const salt = saltOf(k);
  return { k, plain, salt, commit: commit(plain, salt) };
}

/** The seal of a 37-byte plaintext for `round` with the given k and σ: {seal, commit, salt, ctHash, sealRoot}. */
export function sealWith({ plain, round, publicKey, k, sigma }) {
  const pt = asBytes(plain);
  if (pt.length !== PLAIN_LEN) throw new Error('a plaintext is 37 bytes');
  const { U, V, W } = ibeEncrypt(publicKey, round, k, sigma);
  const seal = concat(U, V, W, bodyXor(k, pt));
  const salt = saltOf(k);
  const c = commit(pt, salt);
  const ct = ctHash(seal);
  return { seal, commit: c, salt, ctHash: ct, sealRoot: sealRoot(c, ct) };
}

const same = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
function fail(what) { throw Object.assign(new Error(`seal self-audit failed: ${what}`), { code: 'SealAuditFailed' }); }

/**
 * The self-audit (web design §8.2 step 3): throws `SealAuditFailed` unless
 * the seal is exactly what the program's opener will accept for `plain`.
 * `hostId`/`arriveBell` are the transit's (Plain::validate, I-28).
 */
export function audit({ seal, plain, round, publicKey, k, sigma, commit: c, sealRoot: root, hostId, arriveBell, clock }) {
  const s = asBytes(seal), pt = asBytes(plain);
  if (s.length !== SEAL_LEN) fail('length');
  // The program opens with T(arrive_bell)'s signature: a seal to any other
  // round would be judged bad (integ-W2 review of W2-E).
  const t = sealRound(clock, arriveBell);
  if (t === null || Number(round) !== t) fail(`round ${round} is not T(${arriveBell}) = ${t}`);
  const r = h3(sigma, k);
  if (!same(G2.BASE.multiply(r).toRawBytes(true), s.subarray(0, 96))) fail('U ≠ H3(σ, k)·G2');
  if (!same(xor(sigma, maskFor(publicKey, round, r)), s.subarray(96, 112))) fail('V');
  if (!same(xor(k, h4(sigma, K_LEN)), s.subarray(112, 128))) fail('W');
  const opened = bodyXor(k, s.slice(128));
  if (!same(opened, pt)) fail('body');
  if (!same(pack(unpack(opened)), pt)) fail('re-pack');
  const why = validate(unpack(opened), hostId, arriveBell);
  if (why) fail(`plaintext (${why})`);
  const salt = saltOf(k);
  if (!same(commit(opened, salt), asBytes(c))) fail('commitment');
  if (!same(sealRoot(asBytes(c), ctHash(s)), asBytes(root))) fail('seal root');
  return true;
}

const random16 = () => globalThis.crypto.getRandomValues(new Uint8Array(K_LEN));

/**
 * One sealing request: draws k and σ, seals, audits, zeroes k and σ.
 * `{plain, round, publicKey, hostId, arriveBell, clock}` (the audit
 * refuses a round other than T(arriveBell) of `clock`) → `{ok: true, seal,
 * commit, salt, ctHash, sealRoot}` or `{ok: false, code, error}`.
 */
export function sealRequest({ plain, round, publicKey, hostId, arriveBell, clock }, draw = random16) {
  const k = draw(), sigma = draw();
  try {
    const out = sealWith({ plain, round, publicKey, k, sigma });
    audit({ ...out, plain, round, publicKey, k, sigma, hostId, arriveBell, clock });
    return { ok: true, ...out, round: Number(round) };
  } catch (e) {
    return { ok: false, code: e.code ?? 'SealFailed', error: String(e.message ?? e) };
  } finally {
    k.fill(0);
    sigma.fill(0);
  }
}

// Inside a module worker: one answer per request, tagged with its id.
if (typeof globalThis.WorkerGlobalScope !== 'undefined' && globalThis instanceof globalThis.WorkerGlobalScope) {
  globalThis.addEventListener('message', e => {
    const { id, ...req } = e.data ?? {};
    globalThis.postMessage({ id, ...sealRequest(req) });
  });
}
