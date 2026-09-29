// Sprite renderer for the tile level of detail (art: docs/frontier/art/tiles, rules: its LOD.md).
// Opt-in (the page passes `art` to FrontierMap when the URL has ?art=1); the vector painter in
// layers.mjs stays the default. Every visible province's tiles are drawn together, back to front:
// pass 1 ground + shore/beach + territory wash + grid rim, pass 2 props + holdings.
// A tile whose ground image has not loaded yet is drawn as the vector hex, so nothing flickers.
import { COLORS, RADIUS, hexPoints, polygon, project, shade } from '../../map.mjs';
import { PROVINCE_TILES, locate, tileHex } from '../fgeo.mjs';

const BASE = new URL('../art/', import.meta.url);
/** Sprite sets by tile radius (px); anchor = the tile centre on the ground plane. */
export const ART_SIZES = Object.freeze([
  { key: '@0.5x', r: 22, w: 44, h: 52, ax: 22, ay: 31 },
  { key: '@1x', r: 44, w: 88, h: 104, ax: 44, ay: 62 },
  { key: '@2x', r: 88, w: 176, h: 208, ax: 88, ay: 124 },
]);
const TERRAIN_KEY = { Grassland: 'grassland', Plains: 'plains', Forest: 'forest', Hills: 'hills', Mountain: 'mountain', Water: 'water' };
const SITE_LAND = new Set(['grassland', 'plains', 'forest', 'hills']);
/** Faction index 0..5 (CIV_COLORS order) -> sprite name. */
export const ART_FACTIONS = Object.freeze(['ember', 'tide', 'lumen', 'iron', 'stone', 'verdant']);
/** Neighbour across edge k (axial, r grows downward); edge 0 is the upper-left one. */
const EDGE_DIRS = [[0, -1], [-1, 0], [-1, 1], [0, 1], [1, 0], [1, -1]];

/** The sprite set for a tile drawn `r` device px wide: the smallest at least as large. */
export function artSize(r) {
  return ART_SIZES.find((s) => s.r >= r) ?? ART_SIZES[ART_SIZES.length - 1];
}

/** Deterministic variant 1..3 of a tile. */
export const artVariant = (q, r) => 1 + ((((q * 7 + r * 13) % 3) + 3) % 3);

export class SpriteArt {
  constructor({ onLoad = () => {}, base = BASE } = {}) {
    this.onLoad = onLoad;
    this.base = base;
    this.images = new Map();
  }

  /** The loaded image for set/size/name, or null (and it starts loading). */
  image(set, size, name) {
    const key = `${set}/${size}/${name}`;
    const have = this.images.get(key);
    if (have) return have.ok ? have.img : null;
    if (typeof Image === 'undefined') return null;
    const img = new Image();
    const rec = { img, ok: false };
    this.images.set(key, rec);
    img.onload = () => { rec.ok = true; this.onLoad(); };
    img.onerror = () => { rec.failed = true; };
    img.src = new URL(`${key}.webp`, this.base).href;
    return null;
  }

  /**
   * Draw the tiles of every entry {p, q, terrain, names, sites, rec} together.
   * `zoom` and `dpr` pick the sprite set; `terrainAt(q, r)` names terrain outside the entries.
   */
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
        byHex.set(`${h.q},${h.r}`, t);
      }
    }
    tiles.sort((a, b) => a.y - b.y || a.x - b.x);
    const nameAt = (q, r) => byHex.get(`${q},${r}`)?.name ?? terrainAt(q, r);
    // territory: a held site washes itself and its six neighbours with its owner's colour
    const wash = new Map();
    for (const t of tiles) {
      if (t.state !== 1 || !(t.owner < 6)) continue;
      wash.set(`${t.q},${t.r}`, t.owner);
      for (const [dq, dr] of EDGE_DIRS) if (!wash.has(`${t.q + dq},${t.r + dr}`)) wash.set(`${t.q + dq},${t.r + dr}`, t.owner);
    }
    const draw = (img, t) => ctx.drawImage(img, t.x - s.ax * k, t.y - s.ay * k, s.w * k, s.h * k);
    const siteGround = (t) => t.site !== undefined && SITE_LAND.has(t.name);
    // pass 1
    for (const t of tiles) {
      const g = this.image(siteGround(t) ? 'sites' : 'terrain', s.key, `${t.name}_${t.v}`);
      if (!g) {
        const pal = COLORS[t.name[0].toUpperCase() + t.name.slice(1)] ?? COLORS.Plains;
        polygon(ctx, hexPoints(t.x, t.y, 1), pal[0], shade(pal[1], -0.1), 1);
        continue;
      }
      draw(g, t);
      EDGE_DIRS.forEach(([dq, dr], e) => {
        const nb = nameAt(t.q + dq, t.r + dr);
        if (!nb) return;
        if (t.name === 'water' && nb !== 'water') { const o = this.image('overlays', s.key, `shore_${e}`); if (o) draw(o, t); }
        else if (t.name !== 'water' && nb === 'water') { const o = this.image('overlays', s.key, `beach_${e}`); if (o) draw(o, t); }
      });
      const f = wash.get(`${t.q},${t.r}`);
      if (f !== undefined && t.name !== 'water') { const o = this.image('factions', s.key, `wash_${ART_FACTIONS[f]}`); if (o) draw(o, t); }
      const grid = this.image('overlays', s.key, t.name === 'water' ? 'grid_water' : 'grid');
      if (grid && s.r >= 44) draw(grid, t);
    }
    // pass 2
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
    if (selected) {
      const t = tiles.find((u) => u.p === selected.p && u.pq === selected.q && u.idx === selected.idx);
      if (t) polygon(ctx, hexPoints(t.x, t.y, 3), null, '#1b2e28', 4 / zoom);
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
