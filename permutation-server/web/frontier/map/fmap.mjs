// FrontierMap: the ring/province map (web design §7.3). Levels of detail
// with hysteresis (world → province → tile), culling to the provinces in
// view, presentation-only fog with a "show everything" switch (I-35),
// selection by click/tap or keyboard, drag and wheel/pinch zoom, and a
// redraw only when something changed. Tile LOD draws terrain from the WASM
// kernel when the caller has it; every other level works without it. The
// pure parts (LOD, projection, culling, picking) are exported and tested
// (web-frontier-map.test.mjs); the class only wires them to a canvas.
//
// W5-E (web design §10): zoom in, zoom out and "my holding" buttons stay
// visible at every width (a group after the canvas, 44-px targets, names in
// the current language); the canvas carries `data-lod` and, at tile LOD,
// `data-terrain` (ready | pending) for the smoke tests; the terrain comes
// from the rules module lazily (terrain.mjs) when the caller gives none;
// only provinces the viewer knows or sees are drawn as tiles, with the fog
// veil over them; H goes to the viewer's first holding.
import { inverseHex } from '../../map.mjs';
import { L, onLangChange } from '../../lang.mjs';
import { locate, ringOf, ringProvinces, hexDistance } from '../fgeo.mjs';
import { fogLevel, paintProvince, paintTiles, paintVeil, provincePixel, PROVINCE_CIRCUMRADIUS } from './layers.mjs';
import { createTerrain } from './terrain.mjs';
import { SpriteArt, terrainLookup } from './sprites.mjs';

/** Fog levels drawn as tiles at tile LOD (a distant province stays a muted cell). */
export const TILE_FOGS = Object.freeze(['sight', 'known', 'clear']);
/** The map buttons: `[{id, glyph, label()}]` (glyphs are ASCII or symbols, never text to translate). */
export const MAP_TOOLS = Object.freeze([
  { id: 'in', glyph: '+', label: () => L`地図を拡大` },
  { id: 'out', glyph: '\u2212', label: () => L`地図を縮小` },
  { id: 'home', glyph: '\u2302', label: () => L`自分の拠点へ移動` },
]);

/** Zoom thresholds (screen px per world px) with hysteresis between levels. */
export const LOD_EDGES = Object.freeze({ provinceIn: 0.14, provinceOut: 0.12, tileIn: 0.5, tileOut: 0.45 });
export const ZOOM_MIN = 0.02;
export const ZOOM_MAX = 2.5;

/** The level of detail at `zoom`, given the current one (no flicker at an edge). */
export function lodFor(zoom, current = 'world') {
  const e = LOD_EDGES;
  if (current === 'world') return zoom >= e.tileIn ? 'tile' : zoom >= e.provinceIn ? 'province' : 'world';
  if (current === 'province') return zoom >= e.tileIn ? 'tile' : zoom < e.provinceOut ? 'world' : 'province';
  return zoom < e.provinceOut ? 'world' : zoom < e.tileOut ? 'province' : 'tile';
}

/** view = {x, y (world px at the screen centre), zoom}. */
export const worldToScreen = (view, size, x, y) => ({ x: (x - view.x) * view.zoom + size.width / 2, y: (y - view.y) * view.zoom + size.height / 2 });
export const screenToWorld = (view, size, sx, sy) => ({ x: (sx - size.width / 2) / view.zoom + view.x, y: (sy - size.height / 2) / view.zoom + view.y });

/** The provinces of rings 0..maxRing whose cell intersects the viewport, nearest the centre first. */
export function visibleProvinces(view, size, maxRing) {
  const a = screenToWorld(view, size, 0, 0), b = screenToWorld(view, size, size.width, size.height);
  const m = PROVINCE_CIRCUMRADIUS;
  const out = [];
  for (let d = 0; d <= maxRing; d++) {
    for (const pr of ringProvinces(d)) {
      const c = provincePixel(pr.p, pr.q);
      if (c.x >= a.x - m && c.x <= b.x + m && c.y >= a.y - m && c.y <= b.y + m) out.push({ ...pr, dist: Math.hypot(c.x - view.x, c.y - view.y) });
    }
  }
  return out.sort((x, y) => x.dist - y.dist).map(({ p, q }) => ({ p, q }));
}

/** The tile and province under a screen point: {tileQ, tileR, p, q, idx}. */
export function pick(view, size, sx, sy) {
  const w = screenToWorld(view, size, sx, sy);
  const [tq, tr] = inverseHex(w.x, w.y).split(',').map(Number);
  const l = locate(tq, tr);
  return { tileQ: tq, tileR: tr, p: l.p, q: l.q, idx: l.idx };
}

/** Clamp a zoom and keep the world point under (sx, sy) fixed. */
export function zoomAround(view, size, factor, sx, sy) {
  const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, view.zoom * factor));
  const before = screenToWorld(view, size, sx, sy);
  const next = { ...view, zoom };
  const after = screenToWorld(next, size, sx, sy);
  return { ...next, x: view.x + before.x - after.x, y: view.y + before.y - after.y };
}

/** The distance in provinces from (p, q) to the nearest of `anchors` (the viewer's holdings and hosts). */
export const sightDistance = (p, q, anchors) => anchors.reduce((m, a) => Math.min(m, hexDistance(p, q, a.p, a.q)), Infinity);

/** Height (CSS px) of the canvas covered from below by the page's bottom sheet (`#panel` laid over the map), else 0. */
export function coveredBelow(canvas) {
  const panel = canvas.ownerDocument?.getElementById?.('panel');
  const view = canvas.ownerDocument?.defaultView;
  if (!panel || !view || view.getComputedStyle(panel).position !== 'absolute' || !canvas.getBoundingClientRect) return 0;
  const c = canvas.getBoundingClientRect(), p = panel.getBoundingClientRect();
  return Math.max(0, Math.min(c.bottom, p.bottom) - Math.max(c.top, p.top));
}

export class FrontierMap {
  /**
   * `canvas`; `source()` → {overviews: Map ring→overview, ringsOpen,
   * own: [{p,q}], known: Set "p,q", showAll, selected, terrainOf(p,q) →
   * {terrain, sites, names} | null}; `onSelect(hit)`; `onView(view, lod)`.
   */
  constructor(canvas, { source, onSelect = () => {}, onView = () => {}, art = false }) {
    this.canvas = canvas;
    this.source = source;
    this.onSelect = onSelect;
    this.onView = onView;
    this.view = { x: 0, y: 0, zoom: 0.06 };
    this.lod = lodFor(this.view.zoom);
    this.dirty = true;
    this.drag = null;
    this.pointers = new Map();
    this.terrainOf = createTerrain({ onReady: () => this.invalidate() });
    // Opt-in sprite art at tile LOD (map/sprites.mjs); the vector tiles stay the default.
    this.art = art ? new SpriteArt({ onLoad: () => this.invalidate() }) : null;
    this.mark();
    this.bind();
    this.mountTools();
    this.frame = () => { if (this.dirty) this.draw(); this.raf = globalThis.requestAnimationFrame?.(this.frame); };
    this.raf = globalThis.requestAnimationFrame?.(this.frame);
  }

  size() { return { width: this.canvas.clientWidth, height: this.canvas.clientHeight }; }
  invalidate() { this.dirty = true; }

  setView(v) {
    this.view = { ...this.view, ...v };
    this.lod = lodFor(this.view.zoom, this.lod);
    this.dirty = true;
    this.mark();
    this.onView(this.view, this.lod);
  }

  /** The canvas's `data-lod` (and `data-terrain` once drawn at tile LOD). */
  mark(terrain) {
    const d = this.canvas.dataset;
    if (!d) return;
    if (d.lod !== this.lod) d.lod = this.lod;
    const t = this.lod === 'tile' ? terrain ?? d.terrain ?? 'pending' : 'none';
    if (d.terrain !== t) d.terrain = t;
  }

  /** Centre on a province (and zoom to its LOD). */
  focus(p, q, zoom = 0.2) { const c = provincePixel(p, q); this.setView({ x: c.x, y: c.y, zoom }); }

  /** The viewer's first holding at province LOD, or the Concord zoomed out. */
  home() {
    const own = this.source()?.own?.[0];
    if (own) this.focus(own.p, own.q, 0.2);
    else this.setView({ x: 0, y: 0, zoom: 0.06 });
  }

  /**
   * The first view: the open rings fill the smaller side of the canvas
   * (world LOD at most), centred on the Concord. Once; the viewer's pan and
   * zoom are never overridden.
   */
  fit(src, size) {
    this.fitted = true;
    if (this.view.x !== 0 || this.view.y !== 0 || this.view.zoom !== 0.06) return;
    const rings = Math.max(1, src?.ringsOpen ?? 1);
    const edge = provincePixel(rings, 0);
    const radius = Math.hypot(edge.x, edge.y) + PROVINCE_CIRCUMRADIUS;
    // On a phone the bottom sheet covers the lower part of the canvas: fit the part above it.
    const covered = coveredBelow(this.canvas);
    const free = Math.max(size.height * 0.4, size.height - covered);
    const zoom = Math.max(ZOOM_MIN, Math.min(LOD_EDGES.provinceOut * 0.95, (0.9 * Math.min(size.width, free)) / (2 * radius)));
    this.setView({ x: 0, y: (size.height - free) / 2 / zoom, zoom });
  }

  /** Zoom around the canvas centre by `factor`. */
  zoomBy(factor) { const s = this.size(); this.setView(zoomAround(this.view, s, factor, s.width / 2, s.height / 2)); }

  /** The map buttons after the canvas (a DOM page only). */
  mountTools() {
    const c = this.canvas, doc = c.ownerDocument;
    if (!doc || !c.parentElement || c.parentElement.querySelector?.('.map-tools')) return;
    const box = doc.createElement('div');
    box.className = 'map-tools';
    box.setAttribute('role', 'group');
    const buttons = MAP_TOOLS.map(t => {
      const b = doc.createElement('button');
      b.type = 'button';
      b.className = 'map-btn';
      b.dataset.map = t.id;
      const g = doc.createElement('span');
      g.setAttribute('aria-hidden', 'true');
      g.textContent = t.glyph;
      b.append(g);
      b.addEventListener('click', () => (t.id === 'in' ? this.zoomBy(1.25) : t.id === 'out' ? this.zoomBy(0.8) : this.home()));
      box.append(b);
      return [b, t];
    });
    const label = () => {
      box.setAttribute('aria-label', L`地図の操作`);
      for (const [b, t] of buttons) { const x = t.label(); b.setAttribute('aria-label', x); b.title = x; }
    };
    label();
    this.unlang = onLangChange(label);
    c.after(box);
    this.tools = box;
  }

  bind() {
    const c = this.canvas;
    if (!c.addEventListener) return;
    c.addEventListener('pointerdown', e => { c.setPointerCapture?.(e.pointerId); this.pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY }); this.drag = { x: e.offsetX, y: e.offsetY, moved: false }; });
    c.addEventListener('pointermove', e => {
      const prev = this.pointers.get(e.pointerId);
      if (!prev) return;
      if (this.pointers.size === 2) {
        const [a, b] = [...this.pointers.values()];
        const before = Math.hypot(a.x - b.x, a.y - b.y);
        this.pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
        const [a2, b2] = [...this.pointers.values()];
        const after = Math.hypot(a2.x - b2.x, a2.y - b2.y);
        if (before > 0) this.setView(zoomAround(this.view, this.size(), after / before, (a2.x + b2.x) / 2, (a2.y + b2.y) / 2));
        if (this.drag) this.drag.moved = true;
        return;
      }
      this.pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
      const dx = e.offsetX - prev.x, dy = e.offsetY - prev.y;
      if (this.drag && Math.hypot(e.offsetX - this.drag.x, e.offsetY - this.drag.y) > 4) this.drag.moved = true;
      if (this.drag?.moved) this.setView({ x: this.view.x - dx / this.view.zoom, y: this.view.y - dy / this.view.zoom });
    });
    const up = e => {
      const d = this.drag;
      this.pointers.delete(e.pointerId);
      if (this.pointers.size === 0) this.drag = null;
      if (d && !d.moved && e.type === 'pointerup') this.onSelect(pick(this.view, this.size(), e.offsetX, e.offsetY));
    };
    c.addEventListener('pointerup', up);
    c.addEventListener('pointercancel', up);
    c.addEventListener('wheel', e => { e.preventDefault(); this.setView(zoomAround(this.view, this.size(), Math.exp(-e.deltaY * 0.0015), e.offsetX, e.offsetY)); }, { passive: false });
    c.addEventListener('keydown', e => {
      const step = 80 / this.view.zoom;
      const k = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
      if (k) { e.preventDefault(); this.setView({ x: this.view.x + k[0], y: this.view.y + k[1] }); return; }
      const s = this.size();
      if (e.key === '+' || e.key === '=') this.zoomBy(1.25);
      else if (e.key === '-') this.zoomBy(0.8);
      else if (e.key === 'h' || e.key === 'H') this.home();
      else if (e.key === 'Enter') this.onSelect(pick(this.view, s, s.width / 2, s.height / 2));
    });
  }

  draw() {
    this.dirty = false;
    const ctx = this.canvas.getContext?.('2d');
    if (!ctx) return;
    const dpr = Math.min(globalThis.devicePixelRatio || 1, 2);
    const { width, height } = this.size();
    if (this.canvas.width !== Math.round(width * dpr)) { this.canvas.width = Math.round(width * dpr); this.canvas.height = Math.round(height * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = '#e9e5d8';
    ctx.fillRect(0, 0, width, height);
    const src = this.source();
    if (!this.fitted && width > 0 && height > 0) this.fit(src, { width, height });
    const z = this.view.zoom;
    ctx.setTransform(dpr * z, 0, 0, dpr * z, dpr * (width / 2 - this.view.x * z), dpr * (height / 2 - this.view.y * z));
    const maxRing = Math.max(0, (src.ringsOpen ?? 1) - 1) + 1;
    const recs = new Map();
    for (const ov of src.overviews?.values?.() ?? []) for (const r of ov.provinces) recs.set(`${r.p},${r.q}`, r);
    const terrainOf = src.terrainOf ?? this.terrainOf;
    let wanted = 0, drawn = 0;
    const artTiles = [], artCells = [], artVoid = [];
    for (const pr of visibleProvinces(this.view, { width, height }, maxRing)) {
      const key = `${pr.p},${pr.q}`;
      const fog = fogLevel({ ringOpen: ringOf(pr.p, pr.q) < (src.ringsOpen ?? 1), showAll: src.showAll, known: src.known?.has(key), sightDistance: sightDistance(pr.p, pr.q, src.own ?? []) });
      const selected = !!src.selected && src.selected.p === pr.p && src.selected.q === pr.q;
      const rec = recs.get(key);
      if (this.lod === 'tile' && this.art) {
        // Art: every fog level is drawn as tiles (distant muted, unopened as cloud sea; LOD.md).
        if (fog === 'unopened') { artTiles.push({ ...pr, fog, selected }); continue; }
        const t = terrainOf?.(pr.p, pr.q);
        if (TILE_FOGS.includes(fog)) wanted++;
        if (t) { artTiles.push({ ...pr, ...t, rec, fog, selected, prov: src.provinceOf?.(pr.p, pr.q) ?? null, clash: src.clashOf?.(pr.p, pr.q) ?? null, pending: src.pendingOf?.(pr.p, pr.q) ?? null }); if (TILE_FOGS.includes(fog)) drawn++; continue; }
        paintProvince(ctx, { ...pr, rec, fog, selected, scale: z });
        continue;
      }
      if (this.lod === 'tile' && TILE_FOGS.includes(fog)) {
        wanted++;
        const t = terrainOf?.(pr.p, pr.q);
        if (t) {
          paintTiles(ctx, { ...pr, ...t, rec, selectedTile: selected ? src.selected.idx : null });
          paintVeil(ctx, { ...pr, fog, scale: z, selected });
          drawn++;
          continue;
        }
      }
      if (this.art && fog === 'unopened') { artVoid.push(pr); continue; }
      if (this.art && fog !== 'unopened') {
        const t = terrainOf?.(pr.p, pr.q);
        if (t) { artCells.push({ ...pr, t, rec, fog, selected }); continue; }
      }
      paintProvince(ctx, { ...pr, rec, fog, selected, scale: z });
    }
    if (artVoid.length) this.art.paintUnopened(ctx, artVoid, { zoom: z });
    if (artCells.length) this.art.paintStrategic(ctx, artCells, { zoom: z, dpr });
    if (artTiles.length) {
      const fogAt = (q, r) => { const at = locate(q, r); return fogLevel({ ringOpen: ringOf(at.p, at.q) < (src.ringsOpen ?? 1), showAll: src.showAll, known: src.known?.has(`${at.p},${at.q}`), sightDistance: sightDistance(at.p, at.q, src.own ?? []) }); };
      this.art.paint(ctx, artTiles, { zoom: z, dpr, terrainAt: terrainLookup(terrainOf), fogAt, selected: src.selected, viewerFaction: src.viewerFaction ?? null, demoRoads: !!src.demoRoads, ringsOpen: src.ringsOpen ?? null, replayRing: src.artReplayRing ?? null, engineStage: src.engineStage ?? 0, relics: src.relics ?? [], waystones: src.waystones ?? [], demoSpecials: !!src.demoSpecials, rivers: src.rivers ?? [], demoRivers: !!src.demoRivers, alliedPairs: src.alliedPairs ?? [] });
    }
    this.mark(wanted > 0 && drawn === wanted ? 'ready' : 'pending');
  }

  destroy() { if (this.raf) globalThis.cancelAnimationFrame?.(this.raf); this.unlang?.(); this.tools?.remove(); }
}
