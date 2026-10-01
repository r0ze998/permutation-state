// Sprite art for the Frontier map (art: docs/frontier/art/tiles, rules: its LOD.md). Opt-in: the
// page passes `art` to FrontierMap when the URL has ?art=1; the vector painters in layers.mjs stay
// the default.
//
// Tile LOD: every visible province's tiles are drawn together, back to front — pass 1 ground,
// shore/beach, territory wash, then a thin vector hex grid; pass 2 props and holdings. Sprites are
// shifted so each tile's top face sits exactly on the game's hex (the art's anchor is the ground
// plane; its top face is H above it). Province boundaries follow the tile edges (not the province
// cell), and fog veils the same outline.
//
// Province / world LOD: a Civ-style strategic view — each province is a cached bitmap of flat hexes
// in muted terrain colours with small marks, under a translucent owner tint, the tile-edge
// boundary, the sigil and the clash ring. A province without terrain yet uses the vector cell.
import { COLORS, FLATTEN, RADIUS, hexPoints, polygon, project, shade } from '../../map.mjs';
import { PROVINCE_TILES, locate, provinceCentre, ringOf, ringProvinces, tileHex, wedgeOf } from '../fgeo.mjs';
import { FACTION_COLORS } from '../fi18n.mjs';
import { majorityOwner } from '../herald.mjs';
import { paintPeople, paintNameTags, paintBadges, PEOPLE_FRAME_MS } from '../people/crowds.mjs';
import { activitiesFor } from '../people/activity.mjs';
import { paintBattle } from '../people/battle.mjs';
import { BOUNDARY_HALO, BOUNDARY_INK, FOG, UNOPENED_FILL, paintSigil, provincePixel, PROVINCE_CIRCUMRADIUS } from './layers.mjs';

const BASE = new URL('../art/', import.meta.url);
/** The far bitmaps' resolutions (device px per world px) and their cache budget in pixels. */
export const FAR_RES = Object.freeze([0.1, 0.2, 0.36]);
export const FAR_PIXELS = 24_000_000;
/** Dark inks of the six factions (borders), after the portraits' palette. */
const FACTION_DARK_INK = Object.freeze(['#7d2c27', '#1b5a53', '#7d5a14', '#4b3874', '#24497b', '#6f2d4c']);

/** The realms of a set of far entries: per faction a fill path and a border path, and the holdings' marks. */
export function buildRealms(entries, { grow = 0 } = {}) {
  if (typeof Path2D === 'undefined') return null;
  const owner = new Map(), holdings = [];
  const hexOf = new Map(), water = new Set();
  for (const e of entries) {
    if (!e.terrain) continue;
    for (let i = 0; i < PROVINCE_TILES; i++) if (e.names?.[e.terrain[i]] === 'Water') { const h = tileHex(e.p, e.q, i); water.add(keyOf(h.q, h.r)); }
  }
  for (const e of entries) {
    if (!e.rec || !e.sites) continue;
    e.sites.forEach((idx, j) => {
      if (e.rec.sites[j] !== 1 || !(e.rec.owners[j] < 6)) return;
      const f = e.rec.owners[j];
      const tier = e.prov?.siteMirror?.[j]?.state === 1 ? e.prov.siteMirror[j].tier : 0;
      const h = tileHex(e.p, e.q, idx);
      const c = project(h.q, h.r);
      holdings.push({ x: c.x, y: c.y, f, tier });
      const rad = ([1, 2, 2, 3][tier] ?? 1) + grow;
      for (let dq = -rad; dq <= rad; dq++) for (let dr = Math.max(-rad, -dq - rad); dr <= Math.min(rad, -dq + rad); dr++) {
        const d = Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr));
        const k = keyOf(h.q + dq, h.r + dr);
        if (d > 0 && water.has(k)) continue;
        const cur = owner.get(k);
        if (!cur || d < cur.d) { owner.set(k, { f, d }); hexOf.set(k, { q: h.q + dq, r: h.r + dr }); }
      }
    });
  }
  // close the small pockets a realm leaves (land with at least 4 of its 6 neighbours of one faction), twice
  const land = new Set();
  for (const e of entries) if (e.terrain) for (let i = 0; i < PROVINCE_TILES; i++) { const h = tileHex(e.p, e.q, i); const k = keyOf(h.q, h.r); if (!water.has(k)) { land.add(k); if (!hexOf.has(k)) hexOf.set(k, { q: h.q, r: h.r }); } }
  for (let pass = 0; pass < 2 && grow > 0; pass++) {
    const add = [];
    for (const k of land) {
      if (owner.has(k)) continue;
      const h = hexOf.get(k);
      const n = Array(6).fill(0);
      for (const [dq, dr] of EDGE_DIRS) { const o = owner.get(keyOf(h.q + dq, h.r + dr)); if (o) n[o.f]++; }
      const best = n.indexOf(Math.max(...n));
      if (n[best] >= 4) add.push([k, best]);
    }
    for (const [k, f] of add) owner.set(k, { f, d: 9 });
  }
  const fill = [], edge = [];
  for (const [k, o] of owner) {
    const h = hexOf.get(k);
    const c = project(h.q, h.r);
    if (!fill[o.f]) fill[o.f] = new Path2D();
    hexPoints(c.x, c.y, 0).forEach(([x, y], i) => (i ? fill[o.f].lineTo(x, y) : fill[o.f].moveTo(x, y)));
    fill[o.f].closePath();
    for (const [dq, dr] of EDGE_DIRS) {
      const nk = keyOf(h.q + dq, h.r + dr);
      const n = owner.get(nk);
      if (n && n.f === o.f) continue;
      if (!n && water.has(nk)) continue;   // a coast is not a frontier
      const e2 = sharedEdge(c, project(h.q + dq, h.r + dr));
      if (!e2) continue;
      if (!edge[o.f]) edge[o.f] = new Path2D();
      edge[o.f].moveTo(e2[0][0], e2[0][1]); edge[o.f].lineTo(e2[1][0], e2[1][1]);
    }
  }
  return { fill, edge, holdings };
}

export const farRes = pxPerWorld => FAR_RES.find(r => r >= pxPerWorld * 0.85) ?? FAR_RES[FAR_RES.length - 1];
/** Sprite sets by tile radius (px); anchor = the tile centre on the ground plane. */
export const ART_SIZES = Object.freeze([
  { key: '@0.5x', r: 22, w: 44, h: 52, ax: 22, ay: 31 },
  { key: '@1x', r: 44, w: 88, h: 104, ax: 44, ay: 62 },
  { key: '@2x', r: 88, w: 176, h: 208, ax: 88, ay: 124 },
]);
/** World px from the art's ground-plane anchor down to its top face (H 0.24 × cos(asin 0.76) × R). */
export const TOP_LIFT = 0.24 * Math.sqrt(1 - FLATTEN * FLATTEN) * RADIUS;
const TERRAIN_KEY = { Grassland: 'grassland', Plains: 'plains', Forest: 'forest', Hills: 'hills', Mountain: 'mountain', Water: 'water' };
const SITE_LAND = new Set(['grassland', 'plains', 'forest', 'hills']);
/** Faction index 0..5 (CIV_COLORS order) -> sprite name. */
export const ART_FACTIONS = Object.freeze(['ember', 'tide', 'lumen', 'iron', 'stone', 'verdant']);
/** Neighbour across edge k (axial, r grows downward); edge 0 is the upper-left one. */
const EDGE_DIRS = [[0, -1], [-1, 0], [-1, 1], [0, 1], [1, 0], [1, -1]];
/** Strategic-view colours (muted, after Eternum's biome palette and Civ VI's strategic view). */
export const FLAT = Object.freeze({
  grassland: ['#7d9651', '#6d8546'], plains: ['#bfae6c', '#a8985c'], forest: ['#4d6a3b', '#3c5530'],
  hills: ['#8e9a5c', '#737d49'], mountain: ['#8a847a', '#6c675f'], water: ['#467f8c', '#3a6c78'],
});
const GRID_INK = 'rgba(24,34,26,0.32)';
/** Holding tiers by the account's TIER byte (0..3). */
export const ART_TIERS = Object.freeze(['hamlet', 'town', 'city', 'stronghold']);
/** Host sprites (1x: 32 x 34, anchor 16,22 on the ground plane) and the six slots around a hex centre (hosts.py). */
const HOST_SIZES = { '@1x': { w: 32, h: 34, ax: 16, ay: 22 }, '@2x': { w: 64, h: 68, ax: 32, ay: 44 } };
const SLOT_ANG = [270, 210, 330, 150, 30, 90].map((d) => (d * Math.PI) / 180);
const SLOT_R = 0.46;
/** Below this on-screen tile radius hosts are chips, not figures (LOD.md, tile S). */
export const HOST_FIGURE_MIN_R = 35;
/** Stance by its plaintext byte (0..3, rules stance.rs); fates 3 bounced, 4 retreated, 5 destroyed read as disarray. */
export const ART_STANCES = Object.freeze(['hold', 'assault', 'flank', 'brace']);
const BROKEN_FATES = new Set([3, 4, 5]);
/** Ring-open moment: 8 dissolve frames of 90 ms, the gold edge glow levels per frame (fog.py). */
const OPEN_FRAME_MS = 90;
const OPEN_GLOW = [null, 0, 1, 2, 2, 3, 3, 3, 2, 1, 0];
const now = () => (globalThis.performance?.now?.() ?? Date.now());
/** Relations (clash.rs): bit a*8+b set = a and b are not hostile (peace, NAP or alliance). */
const peaceful = (rel, a, b) => { try { return ((BigInt(rel ?? 0) >> BigInt((a % 8) * 8 + (b % 8))) & 1n) === 1n; } catch { return false; } };
const roadBit = (mask, i) => { try { return mask !== undefined && mask !== null && ((BigInt(mask) >> BigInt(i)) & 1n) === 1n; } catch { return false; } };

/** Axial hexes from a to b inclusive (cube lerp). */
function hexLine(a, b) {
  const n = Math.max(Math.abs(a.q - b.q), Math.abs(a.r - b.r), Math.abs(a.q + a.r - b.q - b.r));
  const out = [];
  for (let i = 0; i <= n; i++) {
    const t = n ? i / n : 0;
    const x = a.q + (b.q - a.q) * t + 1e-6, z = a.r + (b.r - a.r) * t + 1e-6, y = -x - z;
    let rx = Math.round(x), rz = Math.round(z), ry = Math.round(y);
    const dx = Math.abs(rx - x), dz = Math.abs(rz - z), dy = Math.abs(ry - y);
    if (dx > dz && dx > dy) rx = -rz - ry; else if (dz > dy) rz = -rx - ry;
    out.push({ q: rx, r: rz });
  }
  return out;
}

/** The edge (0..5) whose direction from (x, y) points closest to (tx, ty). */
function edgeToward(x, y, tx, ty) {
  let best = 3, dot = -Infinity;
  EDGE_DIRS.forEach(([dq, dr], k) => {
    const o = project(dq, dr);
    const d = (o.x * (tx - x) + o.y * (ty - y)) / Math.hypot(o.x, o.y);
    if (d > dot) { dot = d; best = k; }
  });
  return best;
}

export function artSize(r) {
  return ART_SIZES.find((s) => s.r >= r) ?? ART_SIZES[ART_SIZES.length - 1];
}
export const artVariant = (q, r) => 1 + ((((q * 7 + r * 13) % 3) + 3) % 3);
const keyOf = (q, r) => `${q},${r}`;

/** The two corners two neighbouring hexes share (world px), or null. */
function sharedEdge(a, b) {
  const pa = hexPoints(a.x, a.y, 0), pb = hexPoints(b.x, b.y, 0);
  const out = pa.filter(([x, y]) => pb.some(([u, v]) => Math.abs(u - x) < 0.5 && Math.abs(v - y) < 0.5));
  return out.length === 2 ? out : null;
}

export class SpriteArt {
  constructor({ onLoad = () => {}, base = BASE } = {}) {
    this.onLoad = onLoad;
    this.base = base;
    this.images = new Map();
    this.outlines = new Map();
    this.thumbs = new Map();
    this.misses = 0;
    this.farCache = new Map();   // "P,Q@res|state" → {cv, x, y, w, h, px}
    this.farPixels = 0;
  }

  /**
   * The far view of one province (world and province LOD): the real tile
   * art — terrain, shores, rivers, roads, forests and peaks, holdings and the
   * factions' territory with its borders — painted once into a bitmap at
   * `res` device px per world px and kept until its holdings change. A
   * bitmap painted while some art was still loading is used but not kept.
   * The cache holds at most FAR_PIXELS pixels (oldest dropped first).
   */
  farBitmap(e, { res, terrainAt, fogAt, alliedPairs = [], stateKey = '' }) {
    const k = `${e.p},${e.q}@${res}|${e.fog}|${stateKey}`;
    const hit = this.farCache.get(k);
    if (hit) { this.farCache.delete(k); this.farCache.set(k, hit); return hit; }
    if (typeof document === 'undefined' && typeof OffscreenCanvas === 'undefined') return null;
    const c = provincePixel(e.p, e.q);
    const half = PROVINCE_CIRCUMRADIUS * 1.08 + RADIUS * 1.6;
    const size = Math.max(8, Math.ceil(2 * half * res));
    const cv = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(size, size) : Object.assign(document.createElement('canvas'), { width: size, height: size });
    const g = cv.getContext('2d');
    g.setTransform(res, 0, 0, res, (half - c.x) * res, (half - c.y) * res);
    this.misses = 0;
    this.paint(g, [e], { zoom: res, dpr: 1, terrainAt, fogAt, alliedPairs, far: true });
    const v = { cv, x: c.x - half, y: c.y - half, w: 2 * half, h: 2 * half, px: size * size };
    if (this.misses === 0) {
      this.farCache.set(k, v);
      this.farPixels += v.px;
      while (this.farPixels > FAR_PIXELS && this.farCache.size > 1) { const [ok, ov] = this.farCache.entries().next().value; this.farCache.delete(ok); this.farPixels -= ov.px; }
    }
    return v;
  }

  image(set, size, name) {
    const key = `${set}/${size}/${name}`;
    const have = this.images.get(key);
    if (have) { if (!have.ok) this.misses++; return have.ok ? have.img : null; }
    this.misses++;
    if (typeof Image === 'undefined') return null;
    const img = new Image();
    const rec = { img, ok: false };
    this.images.set(key, rec);
    img.onload = () => { rec.ok = true; this.onLoad(); };
    img.src = new URL(`${key}.webp`, this.base).href;
    return null;
  }

  /** The province's outline along its tile edges, as segments (world px); cached. */
  outline(p, q) {
    const k = keyOf(p, q);
    if (this.outlines.has(k)) return this.outlines.get(k);
    const segs = [];
    for (let i = 0; i < PROVINCE_TILES; i++) {
      const h = tileHex(p, q, i);
      const a = project(h.q, h.r);
      for (const [dq, dr] of EDGE_DIRS) {
        const n = { q: h.q + dq, r: h.r + dr };
        const at = locate(n.q, n.r);
        if (at.p === p && at.q === q) continue;
        const e = sharedEdge(a, project(n.q, n.r));
        if (e) segs.push(e);
      }
    }
    const path = typeof Path2D === 'undefined' ? null : new Path2D();
    const fill = typeof Path2D === 'undefined' ? null : new Path2D();
    if (path) for (const [[x0, y0], [x1, y1]] of segs) { path.moveTo(x0, y0); path.lineTo(x1, y1); }
    if (fill) for (let i = 0; i < PROVINCE_TILES; i++) {
      const h = tileHex(p, q, i);
      const { x, y } = project(h.q, h.r);
      hexPoints(x, y, -0.3).forEach(([px, py], j) => (j ? fill.lineTo(px, py) : fill.moveTo(px, py)));
      fill.closePath();
    }
    const v = { path, fill };
    this.outlines.set(k, v);
    return v;
  }

  /** Boundary (halo, then ink) along the tile edges, plus the fog veil over the tiles. */
  frame(ctx, { p, q, fog, selected, zoom }) {
    const o = this.outline(p, q);
    if (!o.path) return;
    const a = FOG[fog] ?? 0;
    if (a > 0 && fog !== 'unopened') { ctx.fillStyle = `rgba(233,229,216,${a})`; ctx.fill(o.fill); }
    const ink = selected ? 4 : 1.25;
    ctx.lineCap = 'round';
    ctx.strokeStyle = BOUNDARY_HALO; ctx.lineWidth = (ink + 2) / zoom; ctx.stroke(o.path);
    ctx.strokeStyle = BOUNDARY_INK; ctx.lineWidth = ink / zoom; ctx.stroke(o.path);
    ctx.lineCap = 'butt';
  }

  /** A cached strategic bitmap of a province: flat hexes with small terrain marks. */
  thumb(p, q, t, pxPerWorld) {
    const res = pxPerWorld > 0.3 ? 0.6 : 0.22;
    const k = `${p},${q}@${res}`;
    if (this.thumbs.has(k)) return this.thumbs.get(k);
    if (typeof document === 'undefined' && typeof OffscreenCanvas === 'undefined') return null;
    const c = provincePixel(p, q);
    const half = PROVINCE_CIRCUMRADIUS * 1.12;
    const size = Math.ceil(2 * half * res);
    const cv = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(size, size) : Object.assign(document.createElement('canvas'), { width: size, height: size });
    const g = cv.getContext('2d');
    g.setTransform(res, 0, 0, res, (half - c.x) * res, (half - c.y) * res);
    for (let i = 0; i < PROVINCE_TILES; i++) {
      const h = tileHex(p, q, i);
      const { x, y } = project(h.q, h.r);
      const name = TERRAIN_KEY[t.names?.[t.terrain[i]]] ?? 'plains';
      const [fill, dark] = FLAT[name];
      polygon(g, hexPoints(x, y, -0.4), fill, null);
      g.fillStyle = dark; g.strokeStyle = dark; g.lineWidth = 3;
      if (name === 'forest') for (const [dx, dy] of [[-12, 4], [0, -6], [12, 4]]) { g.beginPath(); g.moveTo(x + dx, y + dy - 9); g.lineTo(x + dx + 7, y + dy + 5); g.lineTo(x + dx - 7, y + dy + 5); g.closePath(); g.fill(); }
      else if (name === 'mountain') { g.beginPath(); g.moveTo(x - 16, y + 9); g.lineTo(x - 2, y - 14); g.lineTo(x + 6, y - 2); g.lineTo(x + 10, y - 8); g.lineTo(x + 20, y + 9); g.closePath(); g.fill(); g.fillStyle = '#e9e6de'; g.beginPath(); g.moveTo(x - 6, y - 7); g.lineTo(x - 2, y - 14); g.lineTo(x + 2, y - 7); g.closePath(); g.fill(); }
      else if (name === 'hills') { g.beginPath(); g.arc(x - 7, y + 6, 9, Math.PI, 0); g.arc(x + 9, y + 7, 7, Math.PI, 0); g.stroke(); }
      else if (name === 'water') { g.beginPath(); g.moveTo(x - 12, y); g.quadraticCurveTo(x - 6, y - 5, x, y); g.quadraticCurveTo(x + 6, y + 5, x + 12, y); g.stroke(); }
      polygon(g, hexPoints(x, y, 0), null, GRID_INK, 1.2 / res);
    }
    const v = { cv, x: c.x - half, y: c.y - half, w: 2 * half, h: 2 * half };
    this.thumbs.set(k, v);
    if (this.thumbs.size > 400) this.thumbs.delete(this.thumbs.keys().next().value);
    return v;
  }

  /** Province/world LOD: a far bitmap per province (the land itself), then the labels a map has. */
  paintFar(ctx, entries, { zoom, dpr = 1, terrainAt, fogAt, alliedPairs = [], recs = new Map(), lod = 'world' }) {
    const res = farRes(zoom * dpr);
    for (const e of entries) {
      const state = e.rec ? `${e.rec.owners.join('')}${e.rec.sites.join('')}` + (e.prov ? (e.prov.siteMirror ?? []).map(m => `${m.state}${m.tier}`).join('') : '') : '';
      const b = this.farBitmap(e, { res, terrainAt, fogAt, alliedPairs, stateKey: state });
      if (!b) continue;
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(b.cv, b.x, b.y, b.w, b.h);
    }
    // province edges only where provinces are the unit of play (faint, never a board grid)
    if (lod === 'province') for (const e of entries) {
      if (e.fog === 'unopened') continue;
      const o = this.outline(e.p, e.q);
      if (!o.path) continue;
      ctx.strokeStyle = 'rgba(24,30,26,0.22)'; ctx.lineWidth = 1.4 / zoom; ctx.stroke(o.path);
      if (e.selected) { ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = 3 / zoom; ctx.stroke(o.path); }
    }
    this.paintRealms(ctx, entries, { zoom, lod });
    // a clash this bell: a small crossed-swords mark, at province detail only (the world view stays a map)
    if (lod === 'province') for (const e of entries) {
      if (!e.rec?.clash) continue;
      const c = provincePixel(e.p, e.q), r = 7 / zoom;
      ctx.save(); ctx.translate(c.x, c.y);
      ctx.fillStyle = 'rgba(120,28,18,.85)'; ctx.beginPath(); ctx.arc(0, 0, r, 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = '#ffe2d8'; ctx.lineWidth = 1.6 / zoom; ctx.lineCap = 'round';
      ctx.beginPath(); ctx.moveTo(-r * 0.5, -r * 0.5); ctx.lineTo(r * 0.5, r * 0.5); ctx.moveTo(r * 0.5, -r * 0.5); ctx.lineTo(-r * 0.5, r * 0.5); ctx.stroke();
      ctx.restore();
    }
  }

  /**
   * The factions' lands over the far view: every held site claims its tile
   * and a radius by tier (as the tile view's territory), computed over all
   * drawn provinces together so a realm reads as one region; a translucent
   * wash, a crisp two-tone border where it meets another realm or the wild,
   * and each holding as a mark in its faction's colour (larger for a city or
   * a stronghold). Cached until the drawn provinces' holdings change.
   */
  paintRealms(ctx, entries, { zoom, lod }) {
    const key = entries.map(e => (e.rec ? `${e.p},${e.q}:${e.rec.owners.join('')}${e.rec.sites.join('')}${e.prov ? (e.prov.siteMirror ?? []).map(m => m.tier).join('') : ''}` : '')).join('|');
    const rk = `${lod}|${key}`;
    if (this.realmKey !== rk) { this.realm = buildRealms(entries, { grow: 1 }); this.realmKey = rk; }
    const R = this.realm;
    if (!R) return;
    ctx.save();
    for (let f = 0; f < 6; f++) {
      if (!R.fill[f]) continue;
      ctx.globalAlpha = lod === 'world' ? 0.34 : 0.24; ctx.fillStyle = FACTION_COLORS[f]; ctx.fill(R.fill[f]);
    }
    ctx.globalAlpha = 1; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
    for (let f = 0; f < 6; f++) {
      if (!R.edge[f]) continue;
      // a soft glow of the realm's colour inside, then the border: light rim and dark ink (a painted map's frontier)
      ctx.globalAlpha = 0.45; ctx.strokeStyle = FACTION_COLORS[f]; ctx.lineWidth = 9 / zoom; ctx.stroke(R.edge[f]);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = 'rgba(255,250,236,.8)'; ctx.lineWidth = 3.4 / zoom; ctx.stroke(R.edge[f]);
      ctx.strokeStyle = FACTION_DARK_INK[f]; ctx.lineWidth = 1.8 / zoom; ctx.stroke(R.edge[f]);
    }
    // holdings: a mark per site, white-rimmed (a town is read from afar by its mark, Civ's city banner in miniature)
    const unit = 1 / zoom;
    // province detail: each holding's own art, drawn larger than its tile so a settled land reads as settled
    if (lod === 'province') {
      const sz = ART_SIZES[0], k = (RADIUS / sz.r) * 1.7;
      const list = R.holdings.slice().sort((a, b) => a.y - b.y);
      for (const h of list) {
        const tier = ART_TIERS[h.tier] ?? 'hamlet';
        const img = this.image('holdings', sz.key, `${tier}_${tier === 'stronghold' ? 'w' : 'o'}_${ART_FACTIONS[h.f]}`);
        if (img) ctx.drawImage(img, h.x - sz.ax * k, h.y - sz.ay * k + TOP_LIFT * 1.7, sz.w * k, sz.h * k);
      }
    }
    for (const h of R.holdings) {
      if (lod !== 'world' || h.tier < 1) continue;   // the world view marks towns and up; nearer, the holdings' own art shows
      const r = (h.tier >= 2 ? 4.2 : h.tier === 1 ? 3.2 : 2.4) * unit * (lod === 'world' ? 1 : 1.3);
      ctx.beginPath(); ctx.arc(h.x, h.y, r + 1.2 * unit, 0, Math.PI * 2); ctx.fillStyle = '#fffaf0'; ctx.fill();
      ctx.beginPath(); ctx.arc(h.x, h.y, r, 0, Math.PI * 2); ctx.fillStyle = FACTION_COLORS[h.f]; ctx.fill();
      if (h.tier >= 3) { ctx.lineWidth = 1.2 * unit; ctx.strokeStyle = FACTION_DARK_INK[h.f]; ctx.stroke(); }
    }
    ctx.restore();
  }

  /** Province/world LOD: strategic bitmap, owner tint, boundary, sigil, clash. */
  paintStrategic(ctx, entries, { zoom, dpr = 1 }) {
    for (const e of entries) {
      const th = this.thumb(e.p, e.q, e.t, zoom * dpr);
      if (!th) continue;
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(th.cv, th.x, th.y, th.w, th.h);
      const o = this.outline(e.p, e.q);
      const owner = e.rec ? majorityOwner(e.rec) : null;
      if (owner !== null && o.fill) { ctx.globalAlpha = 0.3; ctx.fillStyle = FACTION_COLORS[owner]; ctx.fill(o.fill); ctx.globalAlpha = 1; }
      this.frame(ctx, { p: e.p, q: e.q, fog: e.fog, selected: e.selected, zoom });
      const c = provincePixel(e.p, e.q);
      if (owner !== null) paintSigil(ctx, { x: c.x, y: c.y, r: Math.min(8 / zoom, PROVINCE_CIRCUMRADIUS / 5), faction: owner, scale: zoom });
      if (e.rec?.clash) { ctx.beginPath(); ctx.arc(c.x, c.y, 14 / zoom, 0, Math.PI * 2); ctx.strokeStyle = '#b3402f'; ctx.lineWidth = 3 / zoom; ctx.stroke(); }
    }
  }

  /** An unopened province: parchment with its hex grid only (after Eternum's unexplored hexes). */
  paintUnopened(ctx, entries, { zoom }) {
    for (const e of entries) {
      const o = this.outline(e.p, e.q);
      if (!o.fill) continue;
      ctx.fillStyle = '#e3ddcb'; ctx.fill(o.fill);
      ctx.strokeStyle = 'rgba(90,82,60,0.18)'; ctx.lineWidth = 1 / zoom; ctx.stroke(o.fill);
      ctx.strokeStyle = 'rgba(90,82,60,0.45)'; ctx.lineWidth = 1.25 / zoom; ctx.setLineDash([4 / zoom, 3 / zoom]); ctx.stroke(o.path); ctx.setLineDash([]);
    }
  }

  /**
   * Tile LOD: the tiles of every entry {p, q, terrain, names, sites, rec, fog, selected} together.
   * An entry with fog 'unopened' (no terrain) is drawn as cloud sea.
   */
  paint(ctx, entries, { zoom, dpr = 1, terrainAt = () => null, fogAt = () => null, selected = null, viewerFaction = null, demoRoads = false,
    ringsOpen = null, replayRing = null, replayEvery = 6000, engineStage = 0,
    relics = [], waystones = [], demoSpecials = false, rivers = [], demoRivers = false, alliedPairs = [], people = null, far = false }) {
    const allied = new Set(alliedPairs.map(([a, b]) => `${Math.min(a, b)}-${Math.max(a, b)}`));
    const calm = (rel, a, b) => peaceful(rel, a, b) || allied.has(`${Math.min(a, b)}-${Math.max(a, b)}`);
    const s = artSize(RADIUS * zoom * dpr);
    // the ring-open moment starts when the open ring count grows (or, in the preview, on a timer)
    const t0 = now();
    if (ringsOpen !== null && this.lastRings !== undefined && ringsOpen > this.lastRings) this.opening = { ring: ringsOpen - 1, at: t0 };
    if (ringsOpen !== null) this.lastRings = ringsOpen;
    if (replayRing !== null && (!this.opening || t0 - this.opening.at > replayEvery)) this.opening = { ring: replayRing, at: t0 };
    // preload every frame of the moment for this size so none is skipped on the first play
    if (this.opening && this.opening.preloaded !== s.key) {
      for (let f = 0; f < 8; f++) this.image('fog', s.key, `cloud_1_open_${f}`);
      for (let k = 0; k < 6; k++) for (let l = 0; l < 4; l++) this.image('fog', s.key, `ringglow_${k}_${l}`);
      this.opening.preloaded = s.key;
    }
    const openFrame = this.opening ? Math.floor((t0 - this.opening.at) / OPEN_FRAME_MS) : Infinity;
    const opening = this.opening && openFrame < OPEN_GLOW.length ? this.opening.ring : null;
    const k = RADIUS / s.r;
    const tiles = [];
    const byHex = new Map();
    for (const e of entries) {
      const siteOf = new Map();
      (e.sites ?? []).forEach((idx, j) => siteOf.set(idx, j));
      for (let i = 0; i < PROVINCE_TILES; i++) {
        const h = tileHex(e.p, e.q, i);
        const { x, y } = project(h.q, h.r);
        const cloud = e.fog === 'unopened';
        const name = cloud ? 'cloud' : TERRAIN_KEY[e.names?.[e.terrain[i]]] ?? 'plains';
        const j = siteOf.get(i);
        const m = j === undefined ? null : e.prov?.siteMirror?.[j] ?? null;
        const t = { q: h.q, r: h.r, x, y, name, cloud, fog: e.fog, v: artVariant(h.q, h.r), p: e.p, pq: e.q, idx: i, site: j,
          state: j === undefined ? undefined : m ? (m.state === 3 ? 5 : m.state === 4 ? 0 : m.state) : e.rec?.sites?.[j],
          owner: j === undefined ? undefined : m ? m.faction : e.rec?.owners?.[j],
          tier: m && m.state === 1 ? m.tier : 0, walls: !!(m && m.wallsCommitted > 0),
          camp: !!(e.prov?.camp?.state === 1 && e.prov.camp.tile === i && j === undefined),
          shield: !!(m && m.shieldUntilBell > 0 && m.shieldUntilBell !== 0xffffffff && m.shieldUntilBell >= (e.prov?.resolvedNext ?? 0)) };
        t.road = !cloud && SITE_LAND.has(name) && roadBit(e.prov?.roadMask, i);
        t.ring = ringOf(e.p, e.q);
        t.rel = e.prov?.relations ?? 0;
        const pc = provinceCentre(e.p, e.q);
        t.centre = h.q === pc.q && h.r === pc.r;
        tiles.push(t);
        byHex.set(keyOf(h.q, h.r), t);
      }
    }
    // preview only (?roads=1): a sample road from every held site to its nearest other site, when the account has none
    if (demoRoads) for (const e of entries) {
      if (!e.prov || BigInt(e.prov.roadMask ?? 0) !== 0n) continue;
      const mine = tiles.filter((t) => t.p === e.p && t.pq === e.q && t.site !== undefined);
      for (const a of mine.filter((t) => t.state === 1)) {
        const b = mine.filter((t) => t !== a).sort((u, v) => Math.hypot(u.x - a.x, u.y - a.y) - Math.hypot(v.x - a.x, v.y - a.y))[0];
        if (b) for (const h of hexLine(a, b)) { const t = byHex.get(keyOf(h.q, h.r)); if (t && SITE_LAND.has(t.name)) t.road = true; }
      }
    }
    // Relic Sites and Waystones: from the page ({p, q, tile, holder|faction}); none in M1, so the
    // preview (?relics=1) places samples: one Relic Site per wedge on the outermost open ring, and a
    // Waystone next to every town-or-larger holding.
    const at = (p, q, idx) => tiles.find((u) => u.p === p && u.pq === q && u.idx === idx);
    for (const r of relics) { const t = at(r.p, r.q, r.tile); if (t) t.relic = { holder: r.holder ?? null }; }
    for (const w of waystones) { const t = at(w.p, w.q, w.tile); if (t) t.waystone = { faction: w.faction }; }
    if (demoSpecials) {
      // the same province each time: per wedge, the first province of the outermost open ring
      const outer = Math.max(1, (ringsOpen ?? 2) - 1);
      const pick = new Map();
      for (const pr of ringProvinces(outer)) { const w = wedgeOf(pr.p, pr.q); if (w !== null && !pick.has(w)) pick.set(w, `${pr.p},${pr.q}`); }
      for (const e of entries) {
        const w = wedgeOf(e.p, e.q);
        if (e.fog === 'unopened' || w === null || pick.get(w) !== `${e.p},${e.q}`) continue;
        const pc = provinceCentre(e.p, e.q);
        const cand = tiles.filter((u) => u.p === e.p && u.pq === e.q && u.site === undefined && SITE_LAND.has(u.name) && !u.centre)
          .sort((a, b) => Math.hypot(a.q - pc.q - 2, a.r - pc.r + 1) - Math.hypot(b.q - pc.q - 2, b.r - pc.r + 1))[0];
        if (!cand) continue;
        cand.relic = { holder: w % 2 ? w : null };
      }
      for (const t of tiles) {
        if (t.state !== 1 || !(t.owner < 6) || (t.tier ?? 0) < 1) continue;
        for (const [dq, dr] of [EDGE_DIRS[3], EDGE_DIRS[4], EDGE_DIRS[2], EDGE_DIRS[5], EDGE_DIRS[1], EDGE_DIRS[0]]) {
          const n = byHex.get(keyOf(t.q + dq, t.r + dr));
          if (n && n.site === undefined && SITE_LAND.has(n.name) && !n.relic) { n.waystone = { faction: t.owner }; break; }
        }
      }
    }
    // Rivers: tiles from the page ({p, q, tile}; none in M1), or preview samples (?rivers=1): in each
    // province with water, from its first hills/mountain tile to the nearest water, avoiding sites.
    const river = new Set();
    for (const r of rivers) { const t = at(r.p, r.q, r.tile); if (t && SITE_LAND.has(t.name) && t.site === undefined) river.add(t); }
    if (demoRivers) for (const e of entries) {
      if (e.fog === 'unopened') continue;
      const mine = tiles.filter((u) => u.p === e.p && u.pq === e.q);
      const seas = mine.filter((u) => u.name === 'water');
      if (!seas.length) continue;
      const rank = { mountain: 0, hills: 1, forest: 2, grassland: 3, plains: 3 };
      const starts = mine.filter((u) => u.name !== 'water').sort((a, b) => (rank[a.name] - rank[b.name]) || (a.idx - b.idx));
      for (const src of starts.slice(0, 12)) {
        const sea = seas.slice().sort((a, b) => Math.hypot(a.x - src.x, a.y - src.y) - Math.hypot(b.x - src.x, b.y - src.y))[0];
        const path = hexLine(src, sea).map((h) => byHex.get(keyOf(h.q, h.r))).filter(Boolean);
        const land = path.filter((u) => SITE_LAND.has(u.name));
        if (land.length < 3 || path.some((u) => u.site !== undefined || u.camp || u.centre)) continue;
        for (const u of land) river.add(u);
        break;
      }
    }
    for (const t of river) {
      let mask = 0;
      EDGE_DIRS.forEach(([dq, dr], k) => { const n = byHex.get(keyOf(t.q + dq, t.r + dr)); if (n && river.has(n)) mask |= 1 << k; });
      // a river end next to water flows into it (one mouth)
      if ([0, 1, 2, 3, 4, 5].filter((k) => mask >> k & 1).length <= 1) {
        const k = EDGE_DIRS.findIndex(([dq, dr]) => byHex.get(keyOf(t.q + dq, t.r + dr))?.name === 'water');
        if (k >= 0) { mask |= 1 << k; t.mouth = k; }
      }
      if (mask) t.river = mask;
    }
    tiles.sort((a, b) => a.y - b.y || a.x - b.x);
    const nameAt = (q, r) => byHex.get(keyOf(q, r))?.name ?? terrainAt(q, r);
    // territory: a held site owns itself and its ring-1 tiles (a hamlet's worked radius); nearer site wins
    const owner = new Map();
    for (const t of tiles) {
      if (t.cloud || t.state !== 1 || !(t.owner < 6)) continue;
      const rad = [1, 2, 2, 3][t.tier] ?? 1;
      for (let dq = -rad; dq <= rad; dq++) for (let dr = Math.max(-rad, -dq - rad); dr <= Math.min(rad, -dq + rad); dr++) {
        const d = Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr));
        const key = keyOf(t.q + dq, t.r + dr);
        const nb = byHex.get(key);
        if (!nb || nb.cloud || (d > 0 && nb.name === 'water')) continue;
        const cur = owner.get(key);
        if (!cur || d < cur.d) owner.set(key, { f: t.owner, d });
      }
    }
    const ownerAt = (q, r) => owner.get(keyOf(q, r))?.f;
    const war = new Set();
    for (const e of entries) {
      const fs = new Set((e.clash?.arrivals ?? []).filter((a) => a.present && a.faction < 6).map((a) => a.faction));
      if (!fs.size) continue;
      for (const t of tiles) if (t.p === e.p && t.pq === e.q && t.state === 1 && t.owner < 6) fs.add(t.owner);
      const list = [...fs];
      for (const a of list) for (const b of list) if (a < b && !calm(e.prov?.relations, a, b)) war.add(`${e.p},${e.q}:${a}-${b}`);
    }
    const draw = (img, t) => ctx.drawImage(img, t.x - s.ax * k, t.y - s.ay * k + TOP_LIFT, s.w * k, s.h * k);
    const nearCentre = (t) => { const pc = provinceCentre(t.p, t.pq); return Math.max(Math.abs(t.q - pc.q), Math.abs(t.r - pc.r), Math.abs(t.q + t.r - pc.q - pc.r)) <= 1; };
    const siteGround = (t) => t.site !== undefined && SITE_LAND.has(t.name);
    // pass 1: ground and flat overlays
    for (const t of tiles) {
      if (t.cloud) continue;
      const g = t.river ? this.image('rivers', s.key, `${t.name}_${String(t.river).padStart(2, '0')}`)
        : this.image(siteGround(t) ? 'sites' : 'terrain', s.key, `${t.name}_${t.v}`);
      if (!g) { polygon(ctx, hexPoints(t.x, t.y, 0), FLAT[t.name][0], null); continue; }
      draw(g, t);
      if (t.ring === 0 && t.name !== 'water' && t.name !== 'mountain') { const pv = this.image('specials', s.key, 'concord_paving'); if (pv) draw(pv, t); }
      EDGE_DIRS.forEach(([dq, dr], e) => {
        const nb = nameAt(t.q + dq, t.r + dr);
        if (!nb || nb === 'cloud') return;
        if (t.name === 'water' && nb !== 'water') {
          const o = this.image('overlays', s.key, `shore_${e}`); if (o) draw(o, t);
          const rt = byHex.get(keyOf(t.q + dq, t.r + dr));
          if (rt?.river && (rt.river >> ((e + 3) % 6) & 1)) { const m = this.image('overlays', s.key, `mouth_${e}`); if (m) draw(m, t); }
        }
        else if (t.name !== 'water' && nb === 'water') { const o = this.image('overlays', s.key, `beach_${e}`); if (o) draw(o, t); }
      });
      if (t.road && !t.river) {
        const pre = `${t.name}_${t.v}`;
        EDGE_DIRS.forEach(([dq, dr], e) => {
          const nb = byHex.get(keyOf(t.q + dq, t.r + dr));
          if (nb?.road) { const o = this.image('roads', s.key, `${pre}_${e}`); if (o) draw(o, t); }
        });
        const c = this.image('roads', s.key, `${pre}_c`); if (c) draw(c, t);
      }
      const f = ownerAt(t.q, t.r);
      if (f !== undefined) {
        const o = this.image('factions', s.key, `wash_${ART_FACTIONS[f]}`); if (o) draw(o, t);
        EDGE_DIRS.forEach(([dq, dr], e) => {
          const g2 = ownerAt(t.q + dq, t.r + dr);
          if (g2 === f) return;
          const style = g2 === undefined ? 'own' : calm(t.rel, f, g2) ? 'ally' : war.has(`${t.p},${t.pq}:${Math.min(f, g2)}-${Math.max(f, g2)}`) ? 'war' : 'own';
          const b = this.image('factions', s.key, `border_${ART_FACTIONS[f]}_${e}_${style}`); if (b) draw(b, t);
        });
      }
    }
    // the hex grid, exactly on the game's hexes, under the props (not in the far view: land, not a board)
    if (!far) {
      ctx.beginPath();
      for (const t of tiles) if (!t.cloud) hexPoints(t.x, t.y, 0).forEach(([px, py], j) => (j ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
      ctx.strokeStyle = GRID_INK; ctx.lineWidth = 1 / zoom; ctx.stroke();
    }
    // pass 2: props, holdings, cloud sea
    for (const t of tiles) {
      if (t.cloud) {
        const c = this.image('fog', s.key, `cloud_${t.v}`);
        if (c) draw(c, t);
        EDGE_DIRS.forEach(([dq, dr], e) => {
          const nb = byHex.get(keyOf(t.q + dq, t.r + dr));
          const open = nb ? !nb.cloud : fogAt(t.q + dq, t.r + dr) !== 'unopened';
          if (!open) return;
          const b = this.image('fog', s.key, `bank_${e}`); if (b) draw(b, t);
        });
        continue;
      }
      const paved = t.ring === 0 && t.name !== 'water' && t.name !== 'mountain';
      // the Permutation Engine stands over the Concord's centre and its six neighbours
      if (t.centre && t.ring === 0) {
        const es = ART_SIZES[Math.min(ART_SIZES.length - 1, ART_SIZES.indexOf(s) + 1)], kk = RADIUS / es.r, m = 2.4;
        const en = this.image('specials', es.key, `engine_${Math.max(0, Math.min(5, engineStage | 0))}`);
        if (en) ctx.drawImage(en, t.x - es.ax * kk * m, t.y - es.ay * kk * m + TOP_LIFT * m, es.w * kk * m, es.h * kk * m);
        continue;
      }
      if (t.ring === 0 && nearCentre(t)) continue;
      // a Seat's camp spreads over its province's centre and six neighbours
      if (t.centre && t.ring === 1) {
        const es = ART_SIZES[Math.min(ART_SIZES.length - 1, ART_SIZES.indexOf(s) + 1)], kk = RADIUS / es.r, m = 1.8;
        const st = this.image('specials', es.key, `seat_${ART_FACTIONS[wedgeOf(t.p, t.pq)] ?? 'ember'}`);
        if (st) ctx.drawImage(st, t.x - es.ax * kk * m, t.y - es.ay * kk * m + TOP_LIFT * m, es.w * kk * m, es.h * kk * m);
        continue;
      }
      if (t.ring === 1 && nearCentre(t)) continue;
      if (t.relic) {
        const f = t.relic.holder;
        const img = this.image('specials', s.key, f === null || f === undefined ? 'relic_open' : `relic_${ART_FACTIONS[f]}`);
        if (img) draw(img, t);
        continue;
      }
      if (t.waystone) { const img = this.image('specials', s.key, `waystone_${ART_FACTIONS[t.waystone.faction] ?? 'ember'}`); if (img) draw(img, t); continue; }
      const decor = ((t.q * 5 + t.r * 11) % 3 + 3) % 3 === 0;
      const pr = paved ? (t.site === undefined && decor ? this.image('specials', s.key, `concord_plaza_${1 + (t.v % 2)}`) : null)
        : t.river ? this.image('rivers_props', s.key, `${t.name}_${String(t.river).padStart(2, '0')}`)
        : this.image(siteGround(t) ? 'sites_props' : 'props', s.key, `${t.name}_${t.v}`);
      if (pr) draw(pr, t);
      if (opening !== null && t.ring === opening && openFrame < 8) { const c = this.image('fog', s.key, `cloud_1_open_${openFrame}`); if (c) draw(c, t); }
      if (t.camp) { const c = this.image('specials', s.key, 'barbarian_1'); if (c) draw(c, t); }
      if (t.site === undefined) continue;
      let img = null;
      if (t.state === 1 && t.owner < 6) {
        const tier = ART_TIERS[t.tier] ?? 'hamlet';
        img = this.image('holdings', s.key, `${tier}_${tier === 'stronghold' || t.walls ? 'w' : 'o'}_${ART_FACTIONS[t.owner]}`);
      } else if (t.state === 2) img = this.image('specials', s.key, 'barbarian_1');
      else if (t.state === 5) img = this.image('specials', s.key, 'freecity_town');
      else if (SITE_LAND.has(t.name)) img = this.image('holdings', s.key, 'site');
      if (img) draw(img, t);
      if (t.shield) { const d = this.image('holdings', s.key, 'shield'); if (d) draw(d, t); }
    }
    // the far view's bitmap stops here: land, props, holdings, territory (map/sprites.mjs farBitmap)
    if (far) return tiles.length;
    // pass 2b: hosts on their tiles, back to front (hidden under fog unless the viewer's own)
    const hosts = [];
    for (const e of entries) {
      if (!e.prov?.entries || e.fog === 'unopened') continue;
      const byTile = new Map();
      const hidden = (f) => (e.fog === 'known' || e.fog === 'distant') && f !== viewerFaction;
      const push = (tile, h) => { if (!byTile.has(tile)) byTile.set(tile, []); byTile.get(tile).push(h); };
      // the last resolved clash: survivors keep the stance they fought in, broken arrivals show as disarray
      const last = new Map();
      for (const a of e.clash?.arrivals ?? []) if (a.present) last.set(String(a.hostId), a);
      const resident = new Set();
      for (const h of e.prov.entries) {
        if (h.state !== 1 && h.state !== 2) continue;
        if (hidden(h.faction)) continue;
        const a = last.get(String(h.id));
        resident.add(String(h.id));
        push(h.tile, { faction: h.faction, stance: a ? ART_STANCES[a.stance] ?? 'hold' : 'hold' });
      }
      for (const a of last.values()) {
        if (resident.has(String(a.hostId)) || !BROKEN_FATES.has(a.fate) || hidden(a.faction)) continue;
        push(a.tile, { faction: a.faction, stance: 'disarray' });
      }
      // arrivals of the latest bell that are not resolved yet: their revealed stance
      const pend = e.pending;
      if (pend && !pend.resolvedTs) for (const a of pend.arrivals ?? []) {
        if (!a.present || hidden(a.faction) || resident.has(String(a.hostId))) continue;
        push(a.tile, { faction: a.faction, stance: ART_STANCES[a.stance] ?? 'hold', arriving: true });
      }
      for (const [idx, list] of byTile) {
        const hx = tileHex(e.p, e.q, idx);
        const c = project(hx.q, hx.r);
        // assault and flank face the nearest hex held by another faction (else the province centre)
        const foes = tiles.filter((u) => u.p === e.p && u.pq === e.q && u.state === 1 && u.owner < 6);
        list.slice(0, 6).forEach((h, n) => {
          const x = c.x + SLOT_R * Math.cos(SLOT_ANG[n]) * RADIUS, y = c.y - SLOT_R * Math.sin(SLOT_ANG[n]) * FLATTEN * RADIUS;
          let facing = null;
          if (h.stance === 'assault' || h.stance === 'flank') {
            const f = foes.filter((u) => u.owner !== h.faction).sort((u, v) => Math.hypot(u.x - c.x, u.y - c.y) - Math.hypot(v.x - c.x, v.y - c.y))[0];
            const pc = provincePixel(e.p, e.q);
            facing = f && (f.x !== c.x || f.y !== c.y) ? edgeToward(c.x, c.y, f.x, f.y) : edgeToward(c.x, c.y, pc.x, pc.y);
          }
          hosts.push({ x, y, h, cx: c.x, cy: c.y, n, count: list.length, facing });
        });
      }
    }
    hosts.sort((a, b) => a.y - b.y || a.x - b.x);
    if (RADIUS * zoom >= HOST_FIGURE_MIN_R) {
      const hk = s.key === '@2x' ? '@2x' : '@1x';
      const hs = HOST_SIZES[hk];
      const kk = RADIUS / (hk === '@2x' ? 88 : 44);
      for (const o of hosts) {
        const st = o.h.stance ?? 'hold';
        const img = this.image('hosts', hk, `${ART_FACTIONS[o.h.faction] ?? 'ember'}_${st}${o.facing === null || o.facing === undefined ? '' : `_${o.facing}`}`);
        if (img) ctx.drawImage(img, o.x - hs.ax * kk, o.y - hs.ay * kk + TOP_LIFT, hs.w * kk, hs.h * kk);
      }
    } else {
      // chips: one per faction per hex, with the host count (screen-sized)
      const seen = new Set();
      for (const o of hosts) {
        const key = `${o.cx},${o.cy},${o.h.faction}`;
        if (seen.has(key)) continue;
        seen.add(key);
        const n = hosts.filter((u) => u.cx === o.cx && u.cy === o.cy && u.h.faction === o.h.faction).length;
        const f = [...seen].filter((k2) => k2.startsWith(`${o.cx},${o.cy},`)).length - 1;
        const w = 30 / zoom, hgt = 16 / zoom, x = o.cx - w / 2 + f * (w + 3 / zoom), y = o.cy + 4 / zoom;
        ctx.beginPath(); ctx.roundRect?.(x - 1.5 / zoom, y - 1.5 / zoom, w + 3 / zoom, hgt + 3 / zoom, 9.5 / zoom); ctx.fillStyle = '#1a1d22'; ctx.fill();
        ctx.beginPath(); ctx.roundRect?.(x, y, w, hgt, 8 / zoom); ctx.fillStyle = FACTION_COLORS[o.h.faction] ?? '#8a8f86'; ctx.fill();
        ctx.fillStyle = '#f6f1e2'; ctx.font = `700 ${12 / zoom}px sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
        ctx.fillText(`⚔${n}`, x + w / 2, y + hgt / 2 + 0.5 / zoom);
      }
      ctx.textAlign = 'start'; ctx.textBaseline = 'alphabetic';
    }
    // pass 2c: people (people/crowds.mjs): townsfolk, carriers, departing columns at their origin, scouts
    // what every tile is doing (people/activity.mjs); kept for the page's hover tip
    const activities = people ? activitiesFor(entries, { bell: people.bell ?? 0, departures: people.departures ?? [], explores: people.explores ?? [], own: people.own ?? [] }) : null;
    // preview only (?art=1&acts=1): sample activities on the drawn holdings
    if (activities && people.demo) for (const t of tiles) {
      if (t.state !== 1 || !(t.owner < 6) || t.fog === 'unopened') continue;
      const k = ['walls', 'muster', 'rest', 'battle', 'garrison', 'depart', 'explore', null][((t.q * 7 + t.r * 3) % 8 + 8) % 8];
      if (!k) continue;
      const key = `${t.p},${t.pq},${t.idx}`;
      if (k === 'depart') { people.departures = [...(people.departures ?? []), { p: t.p, q: t.pq, tile: t.idx, faction: t.owner, troops: 2000, arriveBell: (people.bell ?? 0) + 3 }]; }
      if (k === 'explore') { people.explores = [...(people.explores ?? []), { p: t.p, q: t.pq, tiles: [t.idx], faction: t.owner }]; }
      activities.set(key, [{ kind: k, faction: t.owner, until: (people.bell ?? 0) + 2, n: 400 }, ...(activities.get(key) ?? [])]);
    }
    this.activities = activities;
    this.tiles = tiles;
    const moving = people ? paintPeople(ctx, { tiles, zoom, departures: people.departures ?? [], explores: people.explores ?? [], viewerFaction, columnLabel: people.columnLabel ?? null, activities }) : 0;
    // pass 2d: battle scenes playing (people/battle.mjs)
    let fighting = 0;
    for (const b of people?.battles ?? []) if (paintBattle(ctx, b, { zoom, lossText: people.lossText, fateText: people.fateText })) fighting++;
    // pass 3: fog over known / distant provinces (desaturate, haze, and mist when distant)
    for (const e of entries) {
      if (e.fog !== 'known' && e.fog !== 'distant') continue;
      const o = this.outline(e.p, e.q);
      if (!o.fill) continue;
      const far = e.fog === 'distant';
      ctx.save();
      ctx.globalCompositeOperation = 'saturation';
      ctx.globalAlpha = far ? 0.55 : 0.3;
      ctx.fillStyle = '#808080'; ctx.fill(o.fill);
      ctx.restore();
      ctx.fillStyle = far ? 'rgba(217,223,231,0.32)' : 'rgba(223,230,238,0.16)'; ctx.fill(o.fill);
      if (far) for (const t of tiles) if (t.p === e.p && t.pq === e.q) { const m = this.image('fog', s.key, `mist_${t.v}`); if (m) draw(m, t); }
    }
    for (const e of entries) if (e.fog !== 'unopened') this.frame(ctx, { p: e.p, q: e.q, fog: 'clear', selected: e.selected, zoom });
    if (opening !== null) {
      const lvl = OPEN_GLOW[openFrame];
      if (lvl !== null && lvl !== undefined) for (const t of tiles) {
        if (t.ring !== opening || t.cloud) continue;
        EDGE_DIRS.forEach(([dq, dr], e) => {
          const at = locate(t.q + dq, t.r + dr);
          if (ringOf(at.p, at.q) >= opening) return;
          const gl = this.image('fog', s.key, `ringglow_${e}_${lvl}`); if (gl) draw(gl, t);
        });
      }
      this.onLoad();   // keep frames coming until the moment is over
    }
    if (selected) {
      const t = tiles.find((u) => u.p === selected.p && u.pq === selected.q && u.idx === selected.idx);
      if (t) polygon(ctx, hexPoints(t.x, t.y, 2), null, '#1b2e28', 3 / zoom);
    }
    // name tags over the fog (the holder's face and name), then the next animation frame while figures move
    if (people?.nameOf) paintNameTags(ctx, { tiles, zoom, nameOf: people.nameOf, centre: people.centre ?? null, onImage: this.onLoad });
    if (activities) paintBadges(ctx, { tiles, zoom, activities });
    if ((moving || fighting) && !this.peopleTimer) this.peopleTimer = setTimeout(() => { this.peopleTimer = null; this.onLoad(); }, fighting ? 33 : PEOPLE_FRAME_MS);
    return tiles.length;
  }
}

/** A `terrainAt(q, r)` for tiles outside the drawn entries, from the page's terrainOf(p, q). */
export function terrainLookup(terrainOf) {
  return (q, r) => {
    const at = locate(q, r);
    const t = terrainOf?.(at.p, at.q);
    return t ? TERRAIN_KEY[t.names?.[t.terrain[at.idx]]] ?? null : null;
  };
}

export { UNOPENED_FILL, COLORS, shade };
