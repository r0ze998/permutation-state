// March plaintexts, salts, commitments and the 165-B seal envelope (contract
// §4, §5.11, §7 `seal`; kernel `permutation_rules::frontier::seal`). The
// IBE part of a seal (tlock to quicknet round T(arrive)) runs in the web
// client's worker (W2-E) or in Rust (fclient); this module does the rest,
// byte for byte as the kernel:
//
//   plain  (37 B)  version 1 ‖ host_id u64 ‖ arrive_bell u32 ‖ dest_p i16 ‖ dest_q i16 ‖ dest_tile u8
//                  ‖ stance u8 ‖ retreat_bps u16 ‖ path_len u8 ‖ path [12] (3-bit steps, LE bits) ‖ reserved [3]
//   salt           = sha256("PS-SALT" ‖ k)                       (k: the 16-B IBE message)
//   commit         = sha256("PS-FRONTIER-MARCH-v1" ‖ plain ‖ salt)
//   seal  (165 B)  U [96] ‖ V [16] ‖ W [16] ‖ body [37],  body = plain XOR keystream(k)
//   keystream      block c = sha256("PS-KS" ‖ k ‖ c)
//   ct_hash        = sha256(seal);   seal_root = sha256(commit ‖ ct_hash)
//
// Reveal and SettleTransit take the salt, never k (I-06); the browser never
// stores k or sigma (§9.1). `validate` is the kernel's (I-28), in its order.
import { concat, equal, toBase64, u16le, u32le, u64le, utf8 } from '../bytes.mjs';
import { sha256 } from '../sha256.mjs';

export const DOMAIN_MARCH = 'PS-FRONTIER-MARCH-v1';
export const DOMAIN_POSTURE = 'PS-FRONTIER-POSTURE-v1';
export const DOMAIN_SALT = 'PS-SALT';
export const DOMAIN_KS = 'PS-KS';

export const PLAIN_VERSION = 1;
export const PLAIN_LEN = 37;
export const PATH_BYTES = 12;
export const K_LEN = 16;
export const SEAL_LEN = 165;
export const SEAL_U = 0;
export const SEAL_V = 96;
export const SEAL_W = 112;
export const SEAL_BODY = 128;
/** Steps a path holds (kernel `travel::MAX_PATH_STEPS`). */
export const MAX_PATH_STEPS = 32;
/** Hex directions (kernel `hex::DIRECTIONS`: 0 = E, then counter-clockwise). */
export const DIRECTIONS = 6;
/** Tiles per province (kernel `geometry::PROVINCE_TILES`). */
export const PROVINCE_TILES = 61;
/** Stances: Hold 0, Assault 1, Flank 2, Brace 3. */
export const STANCES = Object.freeze(['Hold', 'Assault', 'Flank', 'Brace']);
/** `retreat_bps` above this is invalid; 0 means never retreat (I-27). */
export const RETREAT_MAX_BPS = 60_000;
/** SettleTransit's seal codes (§5.11, I-44). */
export const SEAL_CODES = Object.freeze({ valid: 0, fo_failed: 1, bad_point: 2, wrong_round: 3, commit_mismatch: 4, plaintext_invalid: 5 });

const bytesOf = (b, n, what) => {
  const x = Uint8Array.from(b);
  if (x.length !== n) throw new RangeError(`${what}: ${x.length} bytes, not ${n}`);
  return x;
};
const le = (b, o, n) => { let v = 0n; for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(b[o + i]); return v; };
const i16 = v => {
  const n = Number(v);
  if (!Number.isInteger(n) || n < -32768 || n > 32767) throw new RangeError(`${v} is not an i16`);
  return u16le(n < 0 ? n + 65536 : n);
};

/** 3-bit step `i` of a 12-B path. */
export function pathStep(path, i) {
  const bit = 3 * i;
  const lo = path[bit >> 3] ?? 0;
  const hi = path[(bit >> 3) + 1] ?? 0;
  return (((hi << 8) | lo) >> (bit % 8)) & 7;
}

/** `{pathLen, path}` of a list of directions (each < 6, at most 32), or null. */
export function encodePath(dirs) {
  if (dirs.length > MAX_PATH_STEPS || dirs.some(d => !Number.isInteger(d) || d < 0 || d >= DIRECTIONS)) return null;
  const path = new Uint8Array(PATH_BYTES);
  dirs.forEach((d, i) => {
    const bit = 3 * i;
    const v = d << (bit % 8);
    path[bit >> 3] |= v & 0xff;
    if ((bit >> 3) + 1 < PATH_BYTES) path[(bit >> 3) + 1] |= v >> 8;
  });
  return { pathLen: dirs.length, path };
}

/** The directions of a plaintext's path. */
export const decodePath = pt => Array.from({ length: Math.min(pt.pathLen, MAX_PATH_STEPS) }, (_, i) => pathStep(pt.path, i));

/**
 * Pack a plaintext `{version = 1, hostId, arriveBell, destP, destQ,
 * destTile, stance, retreatBps, pathLen, path, reserved = [0, 0, 0]}` into
 * its 37 bytes.
 */
export function pack(pt) {
  return concat([pt.version ?? PLAIN_VERSION], u64le(pt.hostId), u32le(pt.arriveBell), i16(pt.destP), i16(pt.destQ), [pt.destTile], [pt.stance],
    u16le(pt.retreatBps), [pt.pathLen], bytesOf(pt.path, PATH_BYTES, 'path'), bytesOf(pt.reserved ?? [0, 0, 0], 3, 'reserved'));
}

/** Unpack 37 bytes (never throws on content: `validate` judges it). */
export function unpack(bytes) {
  const b = bytesOf(bytes, PLAIN_LEN, 'plaintext');
  const s16 = o => { const v = b[o] | (b[o + 1] << 8); return v >= 32768 ? v - 65536 : v; };
  return {
    version: b[0], hostId: le(b, 1, 8), arriveBell: Number(le(b, 9, 4)), destP: s16(13), destQ: s16(15), destTile: b[17], stance: b[18],
    retreatBps: b[19] | (b[20] << 8), pathLen: b[21], path: b.slice(22, 34), reserved: b.slice(34, 37),
  };
}

/**
 * The kernel's `Plain::validate` (I-28): the first failure's name
 * (`Version`, `Reserved`, `HostMismatch`, `ArriveMismatch`, `PathTooLong`,
 * `PathBits`, `Direction`, `Tile`, `Stance`, `Retreat`) or null when valid.
 * `hostId`/`arriveBell`: the transit's (omit either to skip that check).
 */
export function validate(pt, { hostId, arriveBell } = {}) {
  if (pt.version !== PLAIN_VERSION) return 'Version';
  if (Array.from(pt.reserved ?? []).some(x => x !== 0)) return 'Reserved';
  if (hostId !== undefined && BigInt(pt.hostId) !== BigInt(hostId)) return 'HostMismatch';
  if (arriveBell !== undefined && Number(pt.arriveBell) !== Number(arriveBell)) return 'ArriveMismatch';
  const n = pt.pathLen;
  if (n > MAX_PATH_STEPS) return 'PathTooLong';
  const used = 3 * n;
  for (let i = 0; i < PATH_BYTES; i++) {
    const lo = 8 * i;
    const keep = used >= lo + 8 ? 0xff : used <= lo ? 0 : (1 << (used - lo)) - 1;
    if (pt.path[i] & ~keep & 0xff) return 'PathBits';
  }
  for (let i = 0; i < n; i++) if (pathStep(pt.path, i) >= DIRECTIONS) return 'Direction';
  if (pt.destTile >= PROVINCE_TILES) return 'Tile';
  if (pt.stance > STANCES.length - 1) return 'Stance';
  if (pt.retreatBps > RETREAT_MAX_BPS) return 'Retreat';
  return null;
}

/** `salt = sha256("PS-SALT" ‖ k)`. */
export const saltOf = k => sha256(utf8(DOMAIN_SALT), bytesOf(k, K_LEN, 'k'));
/** `commit = sha256(domain ‖ plain ‖ salt)` (default domain: the march's). */
export const commit = (plain, salt, domain = DOMAIN_MARCH) => sha256(utf8(domain), bytesOf(plain, PLAIN_LEN, 'plaintext'), bytesOf(salt, 32, 'salt'));
/** `ct_hash = sha256(seal)`. */
export const ctHash = seal => sha256(bytesOf(seal, SEAL_LEN, 'seal'));
/** `seal_root = sha256(commit ‖ ct_hash)` (what Depart stores; Reveal and SettleTransit check it). */
export const sealRoot = (c, ct) => sha256(bytesOf(c, 32, 'commit'), bytesOf(ct, 32, 'ct_hash'));

/** The plaintext body XOR the PS-KS keystream of `k` (its own inverse). */
export function bodyXor(k, plain) {
  const key = bytesOf(k, K_LEN, 'k');
  const out = bytesOf(plain, PLAIN_LEN, 'body');
  for (let i = 0, c = 0; i < PLAIN_LEN; i += 32, c++) {
    const blk = sha256(utf8(DOMAIN_KS), key, [c]);
    for (let j = 0; j < Math.min(32, PLAIN_LEN - i); j++) out[i + j] ^= blk[j];
  }
  return out;
}

/** A seal's parts: `{u (96), v (16), w (16), body (37)}`. */
export function splitSeal(seal) {
  const s = bytesOf(seal, SEAL_LEN, 'seal');
  return { u: s.slice(SEAL_U, SEAL_V), v: s.slice(SEAL_V, SEAL_W), w: s.slice(SEAL_W, SEAL_BODY), body: s.slice(SEAL_BODY) };
}

/** The 165-B seal from the IBE ciphertext parts `u` (compressed G2, 96), `v`, `w` (16 each) and the 37-B body. */
export const assembleSeal = ({ u, v, w, body }) => concat(bytesOf(u, 96, 'U'), bytesOf(v, 16, 'V'), bytesOf(w, 16, 'W'), bytesOf(body, PLAIN_LEN, 'body'));

/**
 * Open a seal's body with `k` against `commitment` (kernel `open_body`): the
 * plaintext if `commit(body XOR ks(k), salt_of(k)) == commitment`, else null.
 */
export function openBody(k, seal, commitment) {
  const pt = bodyXor(k, splitSeal(seal).body);
  return equal(commit(pt, saltOf(k)), commitment) ? pt : null;
}

/**
 * Everything the chain needs of a march sealed with IBE message `k`:
 * `{plain, salt, commit, body}`; the caller encrypts `k` to the round
 * (IBE: U, V, W) and assembles the seal.
 */
export function sealParts(pt, k) {
  const plain = pack(pt);
  const salt = saltOf(k);
  return { plain, salt, commit: commit(plain, salt), body: bodyXor(k, plain) };
}

/**
 * The browser's self-audit before Depart is signed (§8): the seal's body
 * opens with `k` to exactly `plain`, the commitment is `commit(plain,
 * salt_of(k))`, the plaintext is valid for the host and arrival bell, and
 * the root is the one Depart will store. Returns null or the first problem.
 */
export function auditSeal({ k, seal, plain, commitment, hostId, arriveBell }) {
  const opened = openBody(k, seal, commitment);
  if (!opened) return 'CommitMismatch';
  if (!equal(opened, plain)) return 'BodyMismatch';
  const bad = validate(unpack(plain), { hostId, arriveBell });
  return bad ? `BadPlaintext:${bad}` : null;
}

/**
 * The body of `POST /gw/f/reveal` (§8.3, I-24): the owner's reveal
 * material, not a transaction.
 */
export const revealMaterial = ({ holding, transitSlot, plain, salt, seal }) => ({
  holding, transit_slot: transitSlot, plain_b64: toBase64(bytesOf(plain, PLAIN_LEN, 'plaintext')), salt_b64: toBase64(bytesOf(salt, 32, 'salt')),
  ct_hash_b64: toBase64(ctHash(seal)),
});
