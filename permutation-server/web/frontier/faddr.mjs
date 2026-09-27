// Account addresses, recomputed in the browser (contract §4.1): every
// account but the Season is `create_with_seed(season_pda, seed, program)` =
// sha256(season_pda ‖ seed ‖ program_id), with seed = tag(2) ‖ lowercase
// hex of the little-endian key fields (coordinates i32, bells and days u32).
// The Season is the program address ["season", le64(id)]; its bump is found
// here, never taken from anyone (§3.3). Tags come from the generated layout
// table (abi.mjs); the grammar is pinned by web-frontier-codec.test.mjs
// against frontier-abi/vectors/addresses.json and the kernel's
// addr-vectors-v1.json.
import { sha256 } from '../sdk/sha256.mjs';
import { concat, toHex, utf8 } from '../sdk/bytes.mjs';
import { encode as toBase58 } from '../sdk/base58.mjs';
import { findProgramAddress, pubkeyBytes, pubkeyString } from '../sdk/solana-tx.mjs';
import { ACCOUNTS } from './abi.mjs';
import { provinceFromIndex, provinceIndex, SITES_PER_PROVINCE } from './fgeo.mjs';

const TAG = Object.fromEntries(Object.entries(ACCOUNTS).filter(([, a]) => a.tag).map(([k, a]) => [k, a.tag]));

function le(bits, signed, v) {
  const b = new Uint8Array(bits / 8), dv = new DataView(b.buffer);
  if (bits === 8) dv.setUint8(0, Number(v));
  else if (bits === 16) dv.setUint16(0, Number(v), true);
  else if (bits === 32) (signed ? dv.setInt32(0, Number(v), true) : dv.setUint32(0, Number(v), true));
  else dv.setBigUint64(0, BigInt(v), true);
  return b;
}
const u8 = v => le(8, false, v), u16 = v => le(16, false, v), u32 = v => le(32, false, v), i32 = v => le(32, true, v), u64 = v => le(64, false, v);

/** The raw key bytes of each account kind (§4.1), from named fields. */
export const RAW = Object.freeze({
  Frontier: () => new Uint8Array(0),
  DefencePool: () => new Uint8Array(0),
  RingSeed: ({ d }) => u16(d),
  ProvinceFund: ({ w, wedge }) => u8(w ?? wedge),
  JoinShard: ({ faction, shard }) => concat(u8(faction), u8(shard)),
  BeaconLog: ({ region }) => u8(region),
  Citizen: ({ wallet }) => citizenTag15(wallet),
  Holding: ({ p, q, site }) => concat(i32(p), i32(q), u8(site)),
  Province: ({ p, q }) => concat(i32(p), i32(q)),
  ArrivalSlot: ({ p, q, bell, faction, i }) => concat(i32(p), i32(q), u32(bell), u8(faction), u8(i)),
  ArrivalDay: ({ p, q, day }) => concat(i32(p), i32(q), u32(day)),
  ClashInputs: ({ p, q, bell }) => concat(i32(p), i32(q), u32(bell)),
  BellAnchor: ({ bell, region }) => concat(u32(bell), u8(region)),
  SeedCache: ({ bell, region, nonce }) => concat(u32(bell), u8(region), u8(nonce)),
  AnchorArchive: ({ region, day }) => concat(u8(region), u32(day)),
  DefenceClaim: ({ beneficiary, day }) => concat(keeperTag8(beneficiary), u32(day)),
});

/** The seed string of an account kind's key: tag ‖ lowercase hex (≤ 32 characters). */
export function seedOf(kind, key = {}) {
  const tag = TAG[kind], raw = RAW[kind];
  if (!tag || !raw) throw new Error(`no with-seed address for ${kind}`);
  const s = tag + toHex(raw(key));
  if (s.length > 32) throw new Error(`${kind}: seed longer than 32 bytes`);
  return s;
}

/** create_with_seed: base58 of sha256(base ‖ seed ‖ owner). */
export const withSeed = (base, seed, owner) => toBase58(sha256(pubkeyBytes(base), utf8(seed), pubkeyBytes(owner)));

/** The Season program address and its bump, found here: [address, bump]. */
export const seasonPda = (programId, seasonId) => findProgramAddress([utf8('season'), u64(BigInt(String(seasonId)))], pubkeyString(programId));

/** sha256("PSF-CIT" ‖ wallet)[0..15]: the Citizen's key. */
export const citizenTag15 = wallet => sha256(utf8('PSF-CIT'), pubkeyBytes(wallet)).slice(0, 15);
/** sha256("PSF-KPR" ‖ beneficiary)[0..8]: the DefenceClaim's keeper tag. */
export const keeperTag8 = beneficiary => sha256(utf8('PSF-KPR'), pubkeyBytes(beneficiary)).slice(0, 8);
/** The quota's citizen id: the first 8 bytes of the Citizen address, little-endian (BigInt). */
export const citizenTag = citizenAddress => new DataView(pubkeyBytes(citizenAddress).buffer).getBigUint64(0, true);
/** The JoinShard a wallet joins: sha256(wallet)[0] mod 8. */
export const joinShardOf = wallet => sha256(pubkeyBytes(wallet))[0] % 8;

// ------------------------------------------------------------------ host ids (§4.1, pinned)
export const HOST_ID_INVALID = 0xffffffffffffffffn;
/** host_id = index(P,Q) << 44 | site << 40 | gen << 32 | seq (BigInt), or HOST_ID_INVALID. */
export function hostId({ p, q, site, gen, seq }) {
  if (!(site >= 0 && site < SITES_PER_PROVINCE)) return HOST_ID_INVALID;
  const idx = provinceIndex(p, q);
  if (idx === 0xffffffff) return HOST_ID_INVALID;
  return (BigInt(idx) << 44n) | (BigInt(site) << 40n) | (BigInt(gen & 0xff) << 32n) | BigInt(seq >>> 0);
}
/** The parts of a host id, or null (site ≥ 12, index beyond ring 128). */
export function hostParts(id) {
  const x = BigInt(id);
  if (x < 0n || x > HOST_ID_INVALID) return null;
  const idx = Number(x >> 44n), site = Number((x >> 40n) & 0xfn), gen = Number((x >> 32n) & 0xffn), seq = Number(x & 0xffffffffn);
  if (site >= SITES_PER_PROVINCE) return null;
  const c = provinceFromIndex(idx);
  return c && { provinceIndex: idx, p: c.p, q: c.q, site, gen, seq };
}

/**
 * The addresses of one season: `{programId, seasonId, season, bump,
 * of(kind, key), holdingOfHost(id)}`. Everything is recomputed locally.
 */
export function seasonAddresses(programId, seasonId) {
  const program = pubkeyString(programId);
  const [season, bump] = seasonPda(program, seasonId);
  const cache = new Map();
  const of = (kind, key = {}) => {
    const seed = seedOf(kind, key);
    let a = cache.get(seed);
    if (!a) {
      if (cache.size > 4096) cache.clear();
      a = withSeed(season, seed, program);
      cache.set(seed, a);
    }
    return a;
  };
  return Object.freeze({
    programId: program,
    seasonId: String(BigInt(String(seasonId))),
    season,
    bump,
    of,
    holdingOfHost: id => { const h = hostParts(id); return h && of('Holding', h); },
  });
}
