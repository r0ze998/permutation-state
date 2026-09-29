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
/** Holding tiers by the account's TIER byte (0..3). */
export const ART_TIERS = Object.freeze(['hamlet', 'town', 'city', 'stronghold']);
/** Host sprites (1x: 32 x 34, anchor 16,22 on the ground plane) and the six slots around a hex centre (hosts.py). */
const HOST_SIZES = { '@1x': { w: 32, h: 34, ax: 16, ay: 22 }, '@2x': { w: 64, h: 68, ax: 32, ay: 44 } };
const SLOT_ANG = [270, 210, 330, 150, 30, 90].map((d) => (d * Math.PI) / 180);
const SLOT_R = 0.46;
/** Below this on-screen tile radius hosts are chips, not figures (LOD.md, tile S). */
export const HOST_FIGURE_MIN_R = 35;

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

  /**
   * Tile LOD: the tiles of every entry {p, q, terrain, names, sites, rec, fog, selected} together.
   * An entry with fog 'unopened' (no terrain) is drawn as cloud sea.
   */
  paint(ctx, entries, { zoom, dpr = 1, terrainAt = () => null, fogAt = () => null, selected = null, viewerFaction = null }) {
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
        tiles.push(t);
        byHex.set(keyOf(h.q, h.r), t);
      }
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
    const draw = (img, t) => ctx.drawImage(img, t.x - s.ax * k, t.y - s.ay * k + TOP_LIFT, s.w * k, s.h * k);
    const siteGround = (t) => t.site !== undefined && SITE_LAND.has(t.name);
    // pass 1: ground and flat overlays
    for (const t of tiles) {
      if (t.cloud) continue;
      const g = this.image(siteGround(t) ? 'sites' : 'terrain', s.key, `${t.name}_${t.v}`);
      if (!g) { polygon(ctx, hexPoints(t.x, t.y, 0), FLAT[t.name][0], null); continue; }
      draw(g, t);
      EDGE_DIRS.forEach(([dq, dr], e) => {
        const nb = nameAt(t.q + dq, t.r + dr);
        if (!nb || nb === 'cloud') return;
        if (t.name === 'water' && nb !== 'water') { const o = this.image('overlays', s.key, `shore_${e}`); if (o) draw(o, t); }
        else if (t.name !== 'water' && nb === 'water') { const o = this.image('overlays', s.key, `beach_${e}`); if (o) draw(o, t); }
      });
      const f = ownerAt(t.q, t.r);
      if (f !== undefined) {
        const o = this.image('factions', s.key, `wash_${ART_FACTIONS[f]}`); if (o) draw(o, t);
        EDGE_DIRS.forEach(([dq, dr], e) => {
          if (ownerAt(t.q + dq, t.r + dr) === f) return;
          const b = this.image('factions', s.key, `border_${ART_FACTIONS[f]}_${e}_own`); if (b) draw(b, t);
        });
      }
    }
    // the hex grid, exactly on the game's hexes, under the props
    ctx.beginPath();
    for (const t of tiles) if (!t.cloud) hexPoints(t.x, t.y, 0).forEach(([px, py], j) => (j ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
    ctx.strokeStyle = GRID_INK; ctx.lineWidth = 1 / zoom; ctx.stroke();
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
      const pr = this.image(siteGround(t) ? 'sites_props' : 'props', s.key, `${t.name}_${t.v}`);
      if (pr) draw(pr, t);
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
    // pass 2b: hosts on their tiles, back to front (hidden under fog unless the viewer's own)
    const hosts = [];
    for (const e of entries) {
      if (!e.prov?.entries || e.fog === 'unopened') continue;
      const byTile = new Map();
      for (const h of e.prov.entries) {
        if (h.state !== 1 && h.state !== 2) continue;
        if ((e.fog === 'known' || e.fog === 'distant') && h.faction !== viewerFaction) continue;
        if (!byTile.has(h.tile)) byTile.set(h.tile, []);
        byTile.get(h.tile).push(h);
      }
      for (const [idx, list] of byTile) {
        const hx = tileHex(e.p, e.q, idx);
        const c = project(hx.q, hx.r);
        list.slice(0, 6).forEach((h, n) => {
          const x = c.x + SLOT_R * Math.cos(SLOT_ANG[n]) * RADIUS, y = c.y - SLOT_R * Math.sin(SLOT_ANG[n]) * FLATTEN * RADIUS;
          hosts.push({ x, y, h, cx: c.x, cy: c.y, n, count: list.length });
        });
      }
    }
    hosts.sort((a, b) => a.y - b.y || a.x - b.x);
    if (RADIUS * zoom >= HOST_FIGURE_MIN_R) {
      const hk = s.key === '@2x' ? '@2x' : '@1x';
      const hs = HOST_SIZES[hk];
      const kk = RADIUS / (hk === '@2x' ? 88 : 44);
      for (const o of hosts) {
        const img = this.image('hosts', hk, `${ART_FACTIONS[o.h.faction] ?? 'ember'}_hold`);
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
    // pass 3: fog over known / distant provinces (desaturate, haze, and mist when distant)
    for (const e of entries) {
      if (e.fog !== 'known' && e.fog !== 'distant') continue;
      const o = this.outline(e.p, e.q);
      if (!o.fill) continue;
      const far = e.fog === 'distant';
      ctx.save();
      ctx.globalCompositeOperation = 'saturation';
      ctx.globalAlpha = far ? 0.8 : 0.45;
      ctx.fillStyle = '#808080'; ctx.fill(o.fill);
      ctx.restore();
      ctx.fillStyle = far ? 'rgba(217,223,231,0.5)' : 'rgba(223,230,238,0.26)'; ctx.fill(o.fill);
      if (far) for (const t of tiles) if (t.p === e.p && t.pq === e.q) { const m = this.image('fog', s.key, `mist_${t.v}`); if (m) draw(m, t); }
    }
    for (const e of entries) if (e.fog !== 'unopened') this.frame(ctx, { p: e.p, q: e.q, fog: 'clear', selected: e.selected, zoom });
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
