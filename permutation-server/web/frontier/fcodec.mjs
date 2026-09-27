// Account decoding from raw bytes, driven by the generated layout tables
// (abi.mjs, from frontier-abi's vectors). The client decodes the bytes the
// herald serves itself and never trusts the herald's decoded JSON for
// anything it signs or verifies (contract §9.2).
//
// decode('Season', bytes) → {kind, …fields} with camelCase names; u64/i64
// as BigInt, smaller integers as numbers, byte arrays as Uint8Array,
// sub-records as objects (arrays of them for ×N). Reserved bytes are left
// out. The magic, the size and (when given) the season id are checked.
import { ACCOUNTS, RECORDS } from './abi.mjs';

/** 'GENESIS_TS' → 'genesisTs'. */
export const camel = name => name.toLowerCase().replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

export class CodecError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}

const SCALAR = {
  u8: [1, (dv, o) => dv.getUint8(o)],
  u16: [2, (dv, o) => dv.getUint16(o, true)],
  u32: [4, (dv, o) => dv.getUint32(o, true)],
  u64: [8, (dv, o) => dv.getBigUint64(o, true)],
  i16: [2, (dv, o) => dv.getInt16(o, true)],
  i32: [4, (dv, o) => dv.getInt32(o, true)],
  i64: [8, (dv, o) => dv.getBigInt64(o, true)],
};

function readField(bytes, dv, base, [, off, len, ty]) {
  const o = base + off;
  if (SCALAR[ty]) return SCALAR[ty][1](dv, o);
  let m = /^\[u8;(\d+)\]$/.exec(ty);
  if (m) return bytes.slice(o, o + +m[1]);
  m = /^\[(u16|u32|u64|i64);(\d+)\]$/.exec(ty);
  if (m) {
    const [w, rd] = SCALAR[m[1]];
    return Array.from({ length: +m[2] }, (_, i) => rd(dv, o + i * w));
  }
  m = /^rec:(\w+) x(\d+)$/.exec(ty);
  if (m) {
    const rec = RECORDS[m[1]];
    if (!rec) throw new CodecError('BadLayout', `no record ${m[1]}`);
    const n = +m[2];
    if (rec.size * n !== len) throw new CodecError('BadLayout', `${m[1]} x${n} is not ${len} B`);
    const one = i => readRecord(bytes, dv, o + i * rec.size, rec);
    return n === 1 ? one(0) : Array.from({ length: n }, (_, i) => one(i));
  }
  throw new CodecError('BadLayout', `unknown type ${ty}`);
}

function readRecord(bytes, dv, base, layout) {
  const out = {};
  for (const f of layout.fields) if (f[3] !== 'rsv') out[camel(f[0])] = readField(bytes, dv, base, f);
  return out;
}

const text = b => String.fromCharCode(...b);

/**
 * Decode one account. `seasonId` (string, number or BigInt), when given,
 * must equal the header's. Throws CodecError (`WrongKind`, `WrongSize`,
 * `WrongMagic`, `WrongSeason`).
 */
export function decode(kind, bytes, { seasonId } = {}) {
  const layout = ACCOUNTS[kind];
  if (!layout) throw new CodecError('WrongKind', `unknown account kind ${kind}`);
  const b = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  // A closed Season is a 128-byte tombstone (contract §5.2).
  if (b.length !== layout.size && !(kind === 'Season' && b.length === 128)) throw new CodecError('WrongSize', `${kind}: ${b.length} B, expected ${layout.size}`);
  if (text(b.subarray(0, 8)) !== layout.magic) throw new CodecError('WrongMagic', `${kind}: magic ${JSON.stringify(text(b.subarray(0, 8)))}`);
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const out = { kind };
  for (const f of layout.fields) {
    if (f[3] === 'rsv' || f[1] + f[2] > b.length) continue;
    out[camel(f[0])] = readField(b, dv, 0, f);
  }
  if (seasonId !== undefined && seasonId !== null && out.seasonId !== BigInt(String(seasonId))) {
    throw new CodecError('WrongSeason', `${kind}: season ${out.seasonId}, expected ${seasonId}`);
  }
  return out;
}

/** The byte range of a field: [offset, length] (for checks that compare raw bytes). */
export function fieldRange(kind, name) {
  const f = ACCOUNTS[kind]?.fields.find(x => x[0] === name);
  if (!f) throw new CodecError('BadLayout', `${kind}.${name}`);
  return [f[1], f[2]];
}

// ------------------------------------------------------------------ season helpers (§5.3, §5.1)
export const SEASON_STATUS = Object.freeze(['Announced', 'Created', 'Seeded', 'Running', 'Ended', 'Closed', 'Aborted']);
export const NO_WINDOW_CHANGE = 0xffffffff;

/** The effective status at chain time `now` (s): Running is never stored (Seeded ∧ now ≥ genesis_ts). */
export function effectiveStatus(season, now) {
  const s = SEASON_STATUS[season.status] ?? 'Unknown';
  if (s === 'Seeded' && season.genesisTs !== undefined && BigInt(Math.floor(now)) >= season.genesisTs) return 'Running';
  return s;
}

/** W(b): `window_next` once a scheduled change applies, else `reveal_window`. */
export const windowAt = (season, b) => (season.windowFromBell !== NO_WINDOW_CHANGE && b >= season.windowFromBell ? season.windowNext : season.revealWindow);
