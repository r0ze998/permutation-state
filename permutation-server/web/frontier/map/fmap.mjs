// FrontierMap: the ring/province map (web design §7.3). Levels of detail
// with hysteresis (world → province → tile), culling to the provinces in
// view, presentation-only fog with a "show everything" switch (I-35),
// selection by click/tap or keyboard, drag and wheel/pinch zoom, and a
// redraw only when something changed. Tile LOD draws terrain from the WASM
// kernel when the caller has it; every other level works without it. The
// pure parts (LOD, projection, culling, picking) are exported and tested
// (web-frontier-map.test.mjs); the class only wires them to a canvas.
import { inverseHex } from '../../map.mjs';
import { locate, ringOf, ringProvinces, hexDistance } from '../fgeo.mjs';
import { fogLevel, paintProvince, paintTiles, provincePixel, PROVINCE_CIRCUMRADIUS } from './layers.mjs';

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

export class FrontierMap {
  /**
   * `canvas`; `source()` → {overviews: Map ring→overview, ringsOpen,
   * own: [{p,q}], known: Set "p,q", showAll, selected, terrainOf(p,q) →
   * {terrain, sites, names} | null}; `onSelect(hit)`; `onView(view, lod)`.
   */
  constructor(canvas, { source, onSelect = () => {}, onView = () => {} }) {
    this.canvas = canvas;
    this.source = source;
    this.onSelect = onSelect;
    this.onView = onView;
    this.view = { x: 0, y: 0, zoom: 0.06 };
    this.lod = lodFor(this.view.zoom);
    this.dirty = true;
    this.drag = null;
    this.pointers = new Map();
    this.bind();
    this.frame = () => { if (this.dirty) this.draw(); this.raf = globalThis.requestAnimationFrame?.(this.frame); };
    this.raf = globalThis.requestAnimationFrame?.(this.frame);
  }

  size() { return { width: this.canvas.clientWidth, height: this.canvas.clientHeight }; }
  invalidate() { this.dirty = true; }

  setView(v) {
    this.view = { ...this.view, ...v };
    this.lod = lodFor(this.view.zoom, this.lod);
    this.dirty = true;
    this.onView(this.view, this.lod);
  }

  /** Centre on a province (and zoom to its LOD). */
  focus(p, q, zoom = 0.2) { const c = provincePixel(p, q); this.setView({ x: c.x, y: c.y, zoom }); }

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
      if (e.key === '+' || e.key === '=') this.setView(zoomAround(this.view, s, 1.25, s.width / 2, s.height / 2));
      else if (e.key === '-') this.setView(zoomAround(this.view, s, 0.8, s.width / 2, s.height / 2));
      else if (e.key === 'h' || e.key === 'H') this.setView({ x: 0, y: 0, zoom: 0.06 });
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
    const z = this.view.zoom;
    ctx.setTransform(dpr * z, 0, 0, dpr * z, dpr * (width / 2 - this.view.x * z), dpr * (height / 2 - this.view.y * z));
    const maxRing = Math.max(0, (src.ringsOpen ?? 1) - 1) + 1;
    const recs = new Map();
    for (const ov of src.overviews?.values?.() ?? []) for (const r of ov.provinces) recs.set(`${r.p},${r.q}`, r);
    for (const pr of visibleProvinces(this.view, { width, height }, maxRing)) {
      const key = `${pr.p},${pr.q}`;
      const fog = fogLevel({ ringOpen: ringOf(pr.p, pr.q) < (src.ringsOpen ?? 1), showAll: src.showAll, known: src.known?.has(key), sightDistance: sightDistance(pr.p, pr.q, src.own ?? []) });
      paintProvince(ctx, { ...pr, rec: recs.get(key), fog, selected: src.selected && src.selected.p === pr.p && src.selected.q === pr.q, scale: z });
      if (this.lod === 'tile' && fog !== 'unopened') {
        const t = src.terrainOf?.(pr.p, pr.q);
        if (t) paintTiles(ctx, { ...pr, ...t });
      }
    }
  }

  destroy() { if (this.raf) globalThis.cancelAnimationFrame?.(this.raf); }
}
