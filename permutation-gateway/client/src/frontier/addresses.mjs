// Frontier account addresses (contract §4.1, kernel `frontier::addr`, SP-V2's
// grammar byte for byte):
//
//   addr(kind, key) = create_with_seed(season_pda, seed, program_id) = sha256(season_pda ‖ seed ‖ program_id)
//   seed            = tag (2 ASCII) ‖ lowercase hex of the key fields, little-endian, fixed width (≤ 15 raw bytes)
//
// The Season is the only PDA: `["season", le64(id)]`. Coordinates widen to
// i32 in seeds (accounts store i16), days and bells are u32. Also: citizen
// and keeper tags, the join shard of a wallet, province indices and host ids
// (`host_id = province_index << 44 | site << 40 | gen << 32 | seq`).
// Checked against frontier-abi/vectors/addresses.json and fclient's vectors.
import { LAYOUTS } from './abi-layouts.mjs';
import { concat, toHex, u32le, u64le, utf8 } from '../bytes.mjs';
import { decode as fromBase58, encode as toBase58 } from '../base58.mjs';
import { sha256 } from '../sha256.mjs';
import { findProgramAddress, pubkeyBytes } from '../solana-tx.mjs';

export const SYSTEM_PROGRAM = '11111111111111111111111111111111';
export const BPF_LOADER_UPGRADEABLE = 'BPFLoaderUpgradeab1e11111111111111111111111';
export const INSTRUCTIONS_SYSVAR = 'Sysvar1nstructions1111111111111111111111111';
export const CLOCK_SYSVAR = 'SysvarC1ock11111111111111111111111111111111';

/** Seed tags per account kind, from the ABI (`fr`, `ct`, `ho`…). */
export const SEED_TAGS = Object.freeze(Object.fromEntries(LAYOUTS.accounts.filter(a => a.seed_tag).map(a => [a.kind, a.seed_tag])));
/** Raw key lengths per account kind. */
export const RAW_KEY_LEN = Object.freeze(Object.fromEntries(LAYOUTS.accounts.filter(a => a.seed_tag).map(a => [a.kind, a.raw_key_len])));
/** Tags reserved for removed or later kinds (SealVerdict `sv`, PosturePDA `po`), with their raw lengths. */
export const RESERVED_SEEDS = Object.freeze({ SealVerdict: ['sv', 12], Posture: ['po', 13] });

const i32le = v => {
  const n = Number(v);
  if (!Number.isInteger(n) || n < -(2 ** 31) || n >= 2 ** 31) throw new RangeError(`${v} is not an i32`);
  return u32le(n < 0 ? n + 2 ** 32 : n);
};
const u8 = v => {
  const n = Number(v);
  if (!Number.isInteger(n) || n < 0 || n > 255) throw new RangeError(`${v} is not a u8`);
  return [n];
};
const u16le = v => {
  const n = Number(v);
  if (!Number.isInteger(n) || n < 0 || n > 0xffff) throw new RangeError(`${v} is not a u16`);
  return [n & 0xff, n >> 8];
};

/** A seed string from a 2-letter tag and raw key bytes. */
export function seed(tag, raw = []) {
  const r = Uint8Array.from(raw);
  if (tag.length !== 2) throw new Error(`seed tag ${tag}`);
  if (r.length > 15) throw new RangeError(`seed raw key: ${r.length} bytes (at most 15)`);
  return tag + toHex(r);
}

// ------------------------------------------------------------------ tags

/** The Citizen's raw key: `sha256("PSF-CIT" ‖ wallet)[0..15]`. */
export const citizenTag15 = wallet => sha256(utf8('PSF-CIT'), pubkeyBytes(wallet)).slice(0, 15);
/** The DefenceClaim keeper key: `sha256("PSF-KPR" ‖ beneficiary)[0..8]`. */
export const keeperTag8 = beneficiary => sha256(utf8('PSF-KPR'), pubkeyBytes(beneficiary)).slice(0, 8);
/** The quota's citizen id: the first 8 bytes of the Citizen address, little-endian (a bigint). */
export function citizenTag(citizenAddress) {
  const b = pubkeyBytes(citizenAddress);
  let v = 0n;
  for (let i = 7; i >= 0; i--) v = (v << 8n) | BigInt(b[i]);
  return v;
}
/** A wallet's JoinShard index within its faction: `sha256(wallet)[0] mod 8` (§5.9 Join). */
export const joinShardOf = wallet => sha256(pubkeyBytes(wallet))[0] % 8;

// ------------------------------------------------------------------ seeds per kind

export const seeds = Object.freeze({
  frontier: () => seed('fr'),
  defencePool: () => seed('dp'),
  ringSeed: d => seed('rs', u16le(d)),
  provinceFund: w => seed('pf', u8(w)),
  joinShard: (faction, shard) => seed('js', [...u8(faction), ...u8(shard)]),
  beaconLog: region => seed('bl', u8(region)),
  citizen: wallet => seed('ct', citizenTag15(wallet)),
  holding: (p, q, site) => seed('ho', concat(i32le(p), i32le(q), u8(site))),
  province: (p, q) => seed('pv', concat(i32le(p), i32le(q))),
  arrivalSlot: (p, q, bell, faction, i) => seed('ar', concat(i32le(p), i32le(q), u32le(bell), u8(faction), u8(i))),
  arrivalDay: (p, q, day) => seed('ad', concat(i32le(p), i32le(q), u32le(day))),
  clashInputs: (p, q, bell) => seed('ci', concat(i32le(p), i32le(q), u32le(bell))),
  posture: (p, q, bell, pos) => seed('po', concat(i32le(p), i32le(q), u32le(bell), u8(pos))),
  sealVerdictReserved: (host, bell) => seed('sv', concat(u64le(host), u32le(bell))),
  bellAnchor: (bell, region) => seed('an', concat(u32le(bell), u8(region))),
  seedCache: (bell, region, nonce) => seed('sd', concat(u32le(bell), u8(region), u8(nonce))),
  anchorArchive: (region, day) => seed('aa', concat(u8(region), u32le(day))),
  defenceClaim: (beneficiary, day) => seed('dc', concat(keeperTag8(beneficiary), u32le(day))),
});

// ------------------------------------------------------------------ addresses

/** `create_with_seed(base, seed, owner)`: sha256(base ‖ seed ‖ owner), base58 (no curve check, as the System program). */
export const withSeed = (base, seedStr, owner) => toBase58(sha256(pubkeyBytes(base), utf8(seedStr), pubkeyBytes(owner)));

/** `[address, bump]` of the Season PDA `["season", le64(id)]`. */
export const seasonPda = (programId, seasonId) => findProgramAddress([utf8('season'), u64le(seasonId)], programId);

/** A program's ProgramData address under the upgradeable loader (AnnounceSeason reads it, I-51). */
export const programDataAddress = programId => findProgramAddress([pubkeyBytes(programId)], BPF_LOADER_UPGRADEABLE)[0];

/**
 * Every account address of one season: `new FrontierAddresses({programId,
 * seasonId})`, then `a.citizen(wallet)`, `a.province(p, q)`… (base58).
 */
export class FrontierAddresses {
  constructor({ programId, seasonId }) {
    this.programId = toBase58(pubkeyBytes(programId));
    this.seasonId = BigInt(seasonId);
    [this.season, this.bump] = seasonPda(this.programId, this.seasonId);
  }

  /** The address of a seed string of this season. */
  of(seedStr) { return withSeed(this.season, seedStr, this.programId); }

  get frontier() { return this.of(seeds.frontier()); }
  get defencePool() { return this.of(seeds.defencePool()); }
  get programData() { return programDataAddress(this.programId); }
  ringSeed(d) { return this.of(seeds.ringSeed(d)); }
  provinceFund(w) { return this.of(seeds.provinceFund(w)); }
  joinShard(faction, shard) { return this.of(seeds.joinShard(faction, shard)); }
  /** The JoinShard a wallet joins `faction` through. */
  joinShardFor(faction, wallet) { return this.joinShard(faction, joinShardOf(wallet)); }
  beaconLog(region) { return this.of(seeds.beaconLog(region)); }
  citizen(wallet) { return this.of(seeds.citizen(wallet)); }
  holding(p, q, site) { return this.of(seeds.holding(p, q, site)); }
  province(p, q) { return this.of(seeds.province(p, q)); }
  arrivalSlot(p, q, bell, faction, i) { return this.of(seeds.arrivalSlot(p, q, bell, faction, i)); }
  arrivalDay(p, q, day) { return this.of(seeds.arrivalDay(p, q, day)); }
  clashInputs(p, q, bell) { return this.of(seeds.clashInputs(p, q, bell)); }
  bellAnchor(bell, region) { return this.of(seeds.bellAnchor(bell, region)); }
  seedCache(bell, region, nonce) { return this.of(seeds.seedCache(bell, region, nonce)); }
  anchorArchive(region, day) { return this.of(seeds.anchorArchive(region, day)); }
  defenceClaim(beneficiary, day) { return this.of(seeds.defenceClaim(beneficiary, day)); }
  /** The Holding that owns a host (from its id), or null for an id that is not one. */
  holdingOfHost(id) {
    const h = hostParts(id);
    return h ? this.holding(h.p, h.q, h.site) : null;
  }
}

// ------------------------------------------------------------------ provinces and hosts

/** Hard maximum ring of any season (§3.4 `R_MAX_HARD`). */
export const R_MAX_HARD = 128;
export const SITES_PER_PROVINCE = 12;
export const HOST_ID_INVALID = (1n << 64n) - 1n;

/** Ring of a province (hex distance of (P, Q) from the Concord). */
export const ringOf = (p, q) => (Math.abs(p) + Math.abs(q) + Math.abs(p + q)) / 2;
/** Provinces within ring d: `1 + 3d(d + 1)`. */
export const provincesWithin = d => 1 + 3 * d * (d + 1);

const rotate = ([q, r]) => [-r, q + r];
const unrotate = ([q, r]) => [q + r, -q];
const rotateBy = (h, k) => { for (let i = 0; i < k % 6; i++) h = rotate(h); return h; };
function sextant(h) {
  let w = h;
  for (let k = 0; k < 6; k++) {
    if (w[0] > 0 && w[1] >= 0) return k;
    w = unrotate(w);
  }
  return 0;
}

/**
 * Dense province index (kernel `ProvinceCoord::index`): rings in order and,
 * inside ring d, wedge by wedge; null beyond ring 128.
 */
export function provinceIndex(p, q) {
  const d = ringOf(p, q);
  if (d === 0) return 0;
  if (d > R_MAX_HARD) return null;
  const k = sextant([p, q]);
  const w = rotateBy([p, q], (6 - k) % 6); // q > 0, r ≥ 0, q + r = d
  return provincesWithin(d - 1) + k * d + w[1];
}

/** Inverse of provinceIndex: `{p, q}`, or null at or beyond ring 128's end. */
export function provinceFromIndex(i) {
  if (!Number.isInteger(i) || i < 0 || i >= provincesWithin(R_MAX_HARD)) return null;
  if (i === 0) return { p: 0, q: 0 };
  let d = 1;
  while (provincesWithin(d) <= i) d++;
  const pos = i - provincesWithin(d - 1);
  const k = Math.floor(pos / d);
  const r = pos % d;
  const [p, q] = rotateBy([d - r, r], k);
  return { p: p + 0, q: q + 0 };
}

/** `host_id(P, Q, site, gen, seq)` (bigint); HOST_ID_INVALID for coordinates beyond ring 128 or a site ≥ 12. */
export function hostId(p, q, site, gen, seq) {
  const idx = provinceIndex(p, q);
  if (idx === null || site < 0 || site >= SITES_PER_PROVINCE) return HOST_ID_INVALID;
  return (BigInt(idx) << 44n) | (BigInt(site) << 40n) | (BigInt(gen & 0xff) << 32n) | BigInt(seq >>> 0);
}

/** Inverse of hostId: `{provinceIndex, p, q, site, gen, seq}`, or null for an id that is none. */
export function hostParts(id) {
  const x = BigInt(id);
  if (x < 0n || x > HOST_ID_INVALID) return null;
  const provinceIndexValue = Number(x >> 44n);
  const site = Number((x >> 40n) & 0xfn);
  const gen = Number((x >> 32n) & 0xffn);
  const seq = Number(x & 0xffffffffn);
  if (site >= SITES_PER_PROVINCE) return null;
  const c = provinceFromIndex(provinceIndexValue);
  return c ? { provinceIndex: provinceIndexValue, p: c.p, q: c.q, site, gen, seq } : null;
}

/** A base58 key's 32 bytes (re-exported for SDK users). */
export const keyBytes = k => pubkeyBytes(k);
export const keyString = k => toBase58(pubkeyBytes(k));
export { fromBase58, toBase58 };
