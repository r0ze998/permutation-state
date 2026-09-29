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
import { PROVINCE_TILES, locate, tileHex } from '../fgeo.mjs';
import { FACTION_COLORS } from '../fi18n.mjs';
import { majorityOwner } from '../herald.mjs';
import { BOUNDARY_HALO, BOUNDARY_INK, FOG, UNOPENED_FILL, paintSigil, provincePixel, PROVINCE_CIRCUMRADIUS } from './layers.mjs';

const BASE = new URL('../art/', import.meta.url);
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
  }

  image(set, size, name) {
    const key = `${set}/${size}/${name}`;
    const have = this.images.get(key);
    if (have) return have.ok ? have.img : null;
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

  /** Tile LOD: the tiles of every entry {p, q, terrain, names, sites, rec, fog, selected} together. */
  paint(ctx, entries, { zoom, dpr = 1, terrainAt = () => null, selected = null }) {
    const s = artSize(RADIUS * zoom * dpr);
    const k = RADIUS / s.r;
    const tiles = [];
    const byHex = new Map();
    for (const e of entries) {
      const siteOf = new Map();
      (e.sites ?? []).forEach((idx, j) => siteOf.set(idx, j));
      for (let i = 0; i < PROVINCE_TILES; i++) {
        const h = tileHex(e.p, e.q, i);
        const { x, y } = project(h.q, h.r);
        const name = TERRAIN_KEY[e.names?.[e.terrain[i]]] ?? 'plains';
        const j = siteOf.get(i);
        const t = { q: h.q, r: h.r, x, y, name, v: artVariant(h.q, h.r), p: e.p, pq: e.q, idx: i, site: j,
          state: j === undefined ? undefined : e.rec?.sites?.[j], owner: j === undefined ? undefined : e.rec?.owners?.[j] };
        tiles.push(t);
        byHex.set(keyOf(h.q, h.r), t);
      }
    }
    tiles.sort((a, b) => a.y - b.y || a.x - b.x);
    const nameAt = (q, r) => byHex.get(keyOf(q, r))?.name ?? terrainAt(q, r);
    const wash = new Map();
    for (const t of tiles) {
      if (t.state !== 1 || !(t.owner < 6)) continue;
      wash.set(keyOf(t.q, t.r), t.owner);
      for (const [dq, dr] of EDGE_DIRS) if (!wash.has(keyOf(t.q + dq, t.r + dr))) wash.set(keyOf(t.q + dq, t.r + dr), t.owner);
    }
    const draw = (img, t) => ctx.drawImage(img, t.x - s.ax * k, t.y - s.ay * k + TOP_LIFT, s.w * k, s.h * k);
    const siteGround = (t) => t.site !== undefined && SITE_LAND.has(t.name);
    // pass 1: ground and flat overlays
    for (const t of tiles) {
      const g = this.image(siteGround(t) ? 'sites' : 'terrain', s.key, `${t.name}_${t.v}`);
      if (!g) { polygon(ctx, hexPoints(t.x, t.y, 0), FLAT[t.name][0], null); continue; }
      draw(g, t);
      EDGE_DIRS.forEach(([dq, dr], e) => {
        const nb = nameAt(t.q + dq, t.r + dr);
        if (!nb) return;
        if (t.name === 'water' && nb !== 'water') { const o = this.image('overlays', s.key, `shore_${e}`); if (o) draw(o, t); }
        else if (t.name !== 'water' && nb === 'water') { const o = this.image('overlays', s.key, `beach_${e}`); if (o) draw(o, t); }
      });
      const f = wash.get(keyOf(t.q, t.r));
      if (f !== undefined && t.name !== 'water') { const o = this.image('factions', s.key, `wash_${ART_FACTIONS[f]}`); if (o) draw(o, t); }
    }
    // the hex grid, exactly on the game's hexes, under the props
    ctx.beginPath();
    for (const t of tiles) hexPoints(t.x, t.y, 0).forEach(([px, py], j) => (j ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
    ctx.strokeStyle = GRID_INK; ctx.lineWidth = 1 / zoom; ctx.stroke();
    // pass 2: props and holdings
    for (const t of tiles) {
      const pr = this.image(siteGround(t) ? 'sites_props' : 'props', s.key, `${t.name}_${t.v}`);
      if (pr) draw(pr, t);
      if (t.site === undefined) continue;
      let img = null;
      if (t.state === 1 && t.owner < 6) img = this.image('holdings', s.key, `hamlet_o_${ART_FACTIONS[t.owner]}`);
      else if (t.state === 2) img = this.image('specials', s.key, 'barbarian_1');
      else if (SITE_LAND.has(t.name)) img = this.image('holdings', s.key, 'site');
      if (img) draw(img, t);
    }
    for (const e of entries) this.frame(ctx, { p: e.p, q: e.q, fog: e.fog, selected: e.selected, zoom });
    if (selected) {
      const t = tiles.find((u) => u.p === selected.p && u.pq === selected.q && u.idx === selected.idx);
      if (t) polygon(ctx, hexPoints(t.x, t.y, 2), null, '#1b2e28', 3 / zoom);
    }
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
