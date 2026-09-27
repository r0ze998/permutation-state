// Province geometry the page needs before (or without) the WASM kernel:
// the province of a tile, province centres, rings, wedges, regions and the
// dense province index behind host ids. A transcription of
// permutation-rules `frontier::geometry` and `hex`, pinned by
// web-frontier-codec.test.mjs (host-id vectors) and web-frontier-wasm.test.mjs
// (the kernel's own answers recorded in frontier-wasm/vectors/wasm-vectors.json).
// Everything else (terrain, paths, the clash) is the WASM kernel's.
import { sha256 } from '../sdk/sha256.mjs';
import { utf8 } from '../sdk/bytes.mjs';

export const PROVINCE_RADIUS = 4;
export const PROVINCE_TILES = 61;
export const SITES_PER_PROVINCE = 12;
export const R_MAX_HARD = 128;
export const REGIONS = 16;
/** Axial neighbour order E, NE, NW, W, SW, SE (the path direction codes 0..5). */
export const DIRECTIONS = Object.freeze([[1, 0], [1, -1], [0, -1], [-1, 0], [-1, 1], [0, 1]]);

export const hexDistance = (aq, ar, bq, br) => (Math.abs(aq - bq) + Math.abs(ar - br) + Math.abs(aq + ar - bq - br)) / 2;
/** Province-grid distance from the Concord. */
export const ringOf = (p, q) => (Math.abs(p) + Math.abs(q) + Math.abs(p + q)) / 2;
const rotate = ([q, r]) => [0 - r, q + r];
const unrotate = ([q, r]) => [q + r, 0 - q];
const rotateBy = (h, k) => { let x = h; for (let i = 0; i < ((k % 6) + 6) % 6; i++) x = rotate(x); return x; };
/** The k with h == w.rotate_by(k) for a w with q > 0, r ≥ 0 (0 for the origin). */
export function sextant(q, r) {
  let w = [q, r];
  for (let k = 0; k < 6; k++) {
    if (w[0] > 0 && w[1] >= 0) return k;
    w = unrotate(w);
  }
  return 0;
}
const turned = (h, k) => rotateBy(h, (6 - (k % 6)) % 6);

/** Provinces in rings 0..=d. */
export const provincesWithin = d => 1 + 3 * d * (d + 1);
/** Provinces in ring d. */
export const ringSize = d => (d === 0 ? 1 : 6 * d);

/** Dense index (host ids): the Concord is 0; ring d ≥ 1 wedge by wedge. 0xffffffff beyond ring 128. */
export function provinceIndex(p, q) {
  const d = ringOf(p, q);
  if (d === 0) return 0;
  if (d > R_MAX_HARD) return 0xffffffff;
  const k = sextant(p, q);
  const w = turned([p, q], k);
  return provincesWithin(d - 1) + k * d + w[1];
}

/** Inverse of provinceIndex, or null for an index beyond ring 128. */
export function provinceFromIndex(i) {
  if (!Number.isInteger(i) || i < 0 || i >= provincesWithin(R_MAX_HARD)) return null;
  if (i === 0) return { p: 0, q: 0 };
  let d = 1;
  while (provincesWithin(d) <= i) d++;
  const pos = i - provincesWithin(d - 1);
  const k = Math.floor(pos / d), r = pos % d;
  const [p, q] = rotateBy([d - r, r], k);
  return { p, q };
}

/** Wedge (home faction) of a province, null for the Concord. */
export const wedgeOf = (p, q) => (p === 0 && q === 0 ? null : sextant(p, q));

const floorDiv = (a, b) => Math.floor(a / b);
/** The lattice cell (P, Q) of tile (q, r) for hexagons of radius n. */
export function cellOf(q, r, n = PROVINCE_RADIUS) {
  const det = 3 * n * n + 3 * n + 1;
  const p0 = floorDiv((n + 1) * q - n * r, det);
  const q0 = floorDiv(n * q + (2 * n + 1) * r, det);
  let best = [Infinity, 0, 0];
  for (let dp = -1; dp <= 1; dp++) {
    for (let dq = -1; dq <= 1; dq++) {
      const pp = p0 + dp, qq = q0 + dq;
      const cq = (2 * n + 1) * pp + n * qq, cr = -n * pp + (n + 1) * qq;
      const d = hexDistance(q, r, cq, cr);
      if (d < best[0]) best = [d, pp, qq];
    }
  }
  return { p: best[1], q: best[2] };
}

/** Centre tile of province (p, q). */
export const provinceCentre = (p, q, n = PROVINCE_RADIUS) => ({ q: (2 * n + 1) * p + n * q, r: -n * p + (n + 1) * q });

/** Tile offsets from a province centre in tile-index order (hexes within 4, sorted by q then r). */
export const TILE_OFFSETS = Object.freeze((() => {
  const out = [], R = PROVINCE_RADIUS;
  for (let q = -R; q <= R; q++) for (let r = Math.max(-R, -q - R); r <= Math.min(R, -q + R); r++) out.push(Object.freeze({ q, r }));
  return out;
})());
const TILE_INDEX = new Map(TILE_OFFSETS.map((o, i) => [`${o.q},${o.r}`, i]));

/** Province and tile index of tile (q, r). */
export function locate(q, r) {
  const p = cellOf(q, r);
  const c = provinceCentre(p.p, p.q);
  return { p: p.p, q: p.q, idx: TILE_INDEX.get(`${q - c.q},${r - c.r}`) ?? 0 };
}

/** Absolute hex of tile `idx` of province (p, q). */
export function tileHex(p, q, idx) {
  const o = TILE_OFFSETS[idx];
  if (!o) return null;
  const c = provinceCentre(p, q);
  return { q: c.q + o.q, r: c.r + o.r };
}

const le32 = v => { const b = new Uint8Array(4); new DataView(b.buffer).setInt32(0, v, true); return b; };
const REGION_DOMAIN = utf8('frontier/region');
/** Beacon region of a province: sha256("frontier/region" ‖ le32 P ‖ le32 Q)[0] mod 16. */
export const regionOf = (p, q) => sha256(REGION_DOMAIN, le32(p), le32(q))[0] % REGIONS;

/** The provinces of ring d in index order. */
export function ringProvinces(d) {
  const first = d === 0 ? 0 : provincesWithin(d - 1);
  return Array.from({ length: ringSize(d) }, (_, j) => provinceFromIndex(first + j));
}
