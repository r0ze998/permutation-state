// The march seal, main-thread side (contract §7 `seal`, §9.1; web design
// §8): the 37-byte plaintext (pack, unpack, validate), salt, commitment,
// body keystream, seal root — a transcription of permutation-rules
// `frontier::seal`, pinned byte for byte by web-frontier-seal.test.mjs
// against seal-vectors-v1.json — plus the beacon pin check and the call
// into the sealing worker (seal-worker.mjs), which does the pairing work
// and the self-audit. Sealing needs no network: the round comes from the
// bell clock, the public key from the season record (`/h/season`), checked
// here against the pinned quicknet key (or, on a localnet season only, the
// deterministic test key of I-53) and against the Season's
// `quicknet_pk_hash`.
import { sha256 } from '../sdk/sha256.mjs';
import { toHex } from '../sdk/bytes.mjs';
import { QUICKNET, TEST_BEACON } from './abi.mjs';

export const PLAIN_LEN = 37;
export const SEAL_LEN = 165;
export const PATH_BYTES = 12;
export const MAX_PATH_STEPS = 32;
export const STANCE_MAX = 3;
export const RETREAT_MAX_BPS = 60_000;
export const PROVINCE_TILES = 61;
export const DOMAIN_MARCH = 'PS-FRONTIER-MARCH-v1';
export const DOMAIN_SALT = 'PS-SALT';
export const DOMAIN_KS = 'PS-KS';
/** Stances in plaintext order. */
export const STANCES = Object.freeze(['Hold', 'Assault', 'Flank', 'Brace']);

const bytes = x => (typeof x === 'string' ? Uint8Array.from(x.match(/../g) ?? [], v => parseInt(v, 16)) : Uint8Array.from(x));

// ------------------------------------------------------------------ the plaintext (37 B)
/**
 * {version, hostId (BigInt), arriveBell, destP, destQ, destTile, stance,
 * retreatBps, pathLen, path (12 B), reserved (3 B)} → 37 bytes, little-endian.
 */
export function pack(p) {
  const b = new Uint8Array(PLAIN_LEN), dv = new DataView(b.buffer);
  b[0] = p.version;
  dv.setBigUint64(1, BigInt(p.hostId), true);
  dv.setUint32(9, p.arriveBell, true);
  dv.setInt16(13, p.destP, true);
  dv.setInt16(15, p.destQ, true);
  b[17] = p.destTile;
  b[18] = p.stance;
  dv.setUint16(19, p.retreatBps, true);
  b[21] = p.pathLen;
  b.set(bytes(p.path ?? new Uint8Array(PATH_BYTES)).subarray(0, PATH_BYTES), 22);
  b.set(bytes(p.reserved ?? new Uint8Array(3)).subarray(0, 3), 34);
  return b;
}

/** 37 bytes → the fields (total: every input has fields; validate decides). */
export function unpack(raw) {
  const b = bytes(raw);
  if (b.length !== PLAIN_LEN) throw new Error('a plaintext is 37 bytes');
  const dv = new DataView(b.buffer);
  return {
    version: b[0],
    hostId: dv.getBigUint64(1, true),
    arriveBell: dv.getUint32(9, true),
    destP: dv.getInt16(13, true),
    destQ: dv.getInt16(15, true),
    destTile: b[17],
    stance: b[18],
    retreatBps: dv.getUint16(19, true),
    pathLen: b[21],
    path: b.slice(22, 34),
    reserved: b.slice(34, 37),
  };
}

/** Direction of path step i (3-bit little-endian packing). */
export function pathStep(path, i) {
  const bit = 3 * i, lo = path[bit >> 3] ?? 0, hi = path[(bit >> 3) + 1] ?? 0;
  return ((((hi << 8) | lo) >> (bit % 8)) & 7);
}

/** Directions (0..5, ≤ 32) → {pathLen, path}; null if too long or a direction ≥ 6. */
export function encodePath(dirs) {
  if (dirs.length > MAX_PATH_STEPS || dirs.some(d => !(d >= 0 && d < 6))) return null;
  const path = new Uint8Array(PATH_BYTES);
  dirs.forEach((d, i) => {
    const bit = 3 * i, byte = bit >> 3, v = d << (bit % 8);
    path[byte] |= v & 0xff;
    if (byte + 1 < PATH_BYTES) path[byte + 1] |= v >> 8;
  });
  return { pathLen: dirs.length, path };
}

/**
 * Plain::validate (I-28), in the kernel's order: null when valid, else the
 * reason (Version, Reserved, HostMismatch, ArriveMismatch, PathTooLong,
 * PathBits, Direction, Tile, Stance, Retreat).
 */
export function validate(p, hostId, arriveBell) {
  if (p.version !== 1) return 'Version';
  if (bytes(p.reserved).some(x => x !== 0)) return 'Reserved';
  if (BigInt(p.hostId) !== BigInt(hostId)) return 'HostMismatch';
  if (p.arriveBell !== arriveBell) return 'ArriveMismatch';
  const n = p.pathLen;
  if (n > MAX_PATH_STEPS) return 'PathTooLong';
  const path = bytes(p.path), used = 3 * n;
  for (let i = 0; i < PATH_BYTES; i++) {
    const lo = 8 * i;
    const keep = used >= lo + 8 ? 0xff : used <= lo ? 0 : (1 << (used - lo)) - 1;
    if (path[i] & ~keep & 0xff) return 'PathBits';
  }
  for (let i = 0; i < n; i++) if (pathStep(path, i) >= 6) return 'Direction';
  if (p.destTile >= PROVINCE_TILES) return 'Tile';
  if (p.stance > STANCE_MAX) return 'Stance';
  if (p.retreatBps > RETREAT_MAX_BPS) return 'Retreat';
  return null;
}

// ------------------------------------------------------------------ hashes
/** salt = sha256("PS-SALT" ‖ k). */
export const saltOf = k => sha256(DOMAIN_SALT, bytes(k));
/** commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain37 ‖ salt). */
export const commit = (plain, salt) => sha256(DOMAIN_MARCH, bytes(plain), bytes(salt));
/** ct_hash = sha256(seal). */
export const ctHash = seal => sha256(bytes(seal));
/** seal_root = sha256(commit ‖ ct_hash) (the transit record's `seal_root`). */
export const sealRoot = (c, ct) => sha256(bytes(c), bytes(ct));
/** The 37-byte body XOR sha256("PS-KS" ‖ k ‖ [c]) for c = 0, 1 (seals and opens). */
export function bodyXor(k, plain) {
  const out = bytes(plain);
  for (let i = 0, c = 0; i < out.length; i += 32, c++) {
    const blk = sha256(DOMAIN_KS, bytes(k), Uint8Array.of(c));
    for (let j = 0; j < 32 && i + j < out.length; j++) out[i + j] ^= blk[j];
  }
  return out;
}

// ------------------------------------------------------------------ the beacon pin
/**
 * Check the season's beacon before sealing to it: the drand info from
 * `/h/season` (`{publicKey, chainHash?, period, genesis}`) must be quicknet
 * — or, only when the pinned cluster is `localnet`, the deterministic test
 * key (I-53) — and sha256(publicKey) must equal the Season account's
 * `quicknet_pk_hash`. `{ok: true, kind: 'quicknet'|'test'}` or
 * `{ok: false, code}`.
 */
export function checkBeacon({ drand, seasonPkHash, cluster }) {
  const pk = String(drand?.publicKey ?? '').toLowerCase();
  const period = Number(drand?.period), genesis = Number(drand?.genesis);
  let pin = null, kind = null;
  if (pk === QUICKNET.publicKey) { pin = QUICKNET; kind = 'quicknet'; }
  else if (pk === TEST_BEACON.publicKey) {
    if (cluster !== 'localnet') return { ok: false, code: 'TestBeaconOffLocalnet' };
    pin = TEST_BEACON; kind = 'test';
  } else return { ok: false, code: 'NotQuicknet' };
  if (period !== pin.period || genesis !== pin.genesis) return { ok: false, code: 'BeaconClockMismatch' };
  if (drand.chainHash && String(drand.chainHash).toLowerCase() !== pin.chainHash) return { ok: false, code: 'ChainHashMismatch' };
  const want = seasonPkHash instanceof Uint8Array ? toHex(seasonPkHash) : String(seasonPkHash ?? '').toLowerCase();
  if (want !== toHex(sha256(bytes(pk))) || want !== pin.pkHash) return { ok: false, code: 'PkHashMismatch' };
  return { ok: true, kind, publicKey: pk };
}

// ------------------------------------------------------------------ sealing (through the worker)
let worker = null, seq = 0;
const waiting = new Map();

function startWorker() {
  if (worker !== null) return worker;
  try {
    worker = new globalThis.Worker(new URL('./seal-worker.mjs', import.meta.url), { type: 'module' });
    worker.onmessage = e => { const w = waiting.get(e.data?.id); if (w) { waiting.delete(e.data.id); w(e.data); } };
    worker.onerror = () => { for (const w of waiting.values()) w({ ok: false, code: 'WorkerFailed', error: 'worker failed' }); waiting.clear(); worker = false; };
  } catch {
    worker = false; // no module workers: seal on this thread
  }
  return worker;
}

/**
 * Seal a march: `{plain (37 B), round, publicKey, hostId, arriveBell}` →
 * `{ok, seal, commit, salt, ctHash, sealRoot}` or `{ok: false, code, error}`.
 * The plaintext is validated here first; the worker seals and self-audits.
 * `inline: true` (or no Worker) runs the same code on this thread.
 */
export async function sealMarch(req, { inline = false } = {}) {
  const why = validate(unpack(req.plain), req.hostId, req.arriveBell);
  if (why) return { ok: false, code: 'BadPlaintext', error: why };
  const w = inline || typeof globalThis.Worker !== 'function' ? false : startWorker();
  if (!w) {
    const { sealRequest } = await import('./seal-worker.mjs');
    return sealRequest(req);
  }
  const id = ++seq;
  return new Promise(resolve => { waiting.set(id, resolve); w.postMessage({ id, ...req, plain: Uint8Array.from(req.plain) }); });
}
