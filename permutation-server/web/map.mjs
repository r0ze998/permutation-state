/** State-driven hex diorama for the multi-civilization game.
 *  Adapted from the /civilization/ prototype renderer (same geometry, painter
 *  and terrain art). Everything drawn comes from the server state; no
 *  decorative entities are invented. */
import { CIV_COLORS, CIV_DASH, cityName } from './i18n.mjs';

const SQRT3 = Math.sqrt(3);
const RADIUS = 44;
const FLATTEN = 0.76;
const COLORS = {
  Grassland: ['#c6cea0', '#a1ad80', '#899a71'],
  Plains: ['#d7cf9e', '#bcb07e', '#a0956c'],
  Forest: ['#99b08a', '#748e70', '#61765f'],
  Hills: ['#c5b897', '#a79b80', '#8b816b'],
  Mountain: ['#afa99c', '#928e83', '#7c7b71'],
  Water: ['#77a6a6', '#558a91', '#467880'],
};
// Edge i (between hex corners i and i+1) faces this axial neighbour.
const EDGE_NEIGHBOR = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
export const key = (q, r) => `${q},${r}`;
const project = (q, r) => ({ x: SQRT3 * RADIUS * (q + r / 2), y: RADIUS * 1.5 * r * FLATTEN });
const seed = (q, r, n = 0) => { const v = Math.sin(q * 127.1 + r * 311.7 + n * 74.7) * 43758.5453; return v - Math.floor(v); };
const hexDist = (a, b) => (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;
function shade(hex, f) {
  const n = parseInt(hex.slice(1), 16);
  const c = [n >> 16, (n >> 8) & 255, n & 255].map(v => clamp(Math.round(f < 0 ? v * (1 + f) : v + (255 - v) * f), 0, 255));
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}
function alpha(hex, a) { const n = parseInt(hex.slice(1), 16); return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`; }
function rounded(ctx, x, y, w, h, r = 6) {
  ctx.beginPath(); ctx.moveTo(x + r, y); ctx.lineTo(x + w - r, y); ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r); ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h); ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r); ctx.lineTo(x, y + r); ctx.quadraticCurveTo(x, y, x + r, y); ctx.closePath();
}
function polygon(ctx, points, fill, stroke, width = 1) {
  ctx.beginPath(); points.forEach(([x, y], i) => i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)); ctx.closePath();
  if (fill) { ctx.fillStyle = fill; ctx.fill(); }
  if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = width; ctx.stroke(); }
}
function hexPoints(x, y, inset = 0) {
  return Array.from({ length: 6 }, (_, i) => {
    const a = (i * 60 - 30) * Math.PI / 180;
    return [x + Math.cos(a) * (RADIUS - inset), y + Math.sin(a) * (RADIUS - inset) * FLATTEN];
  });
}
function inverseHex(x, y) {
  const r = (2 / 3 * (y / FLATTEN)) / RADIUS, q = (SQRT3 / 3 * x - (y / FLATTEN) / 3) / RADIUS;
  let aq = Math.round(q), ar = Math.round(r); const as = Math.round(-q - r);
  const dq = Math.abs(aq - q), dr = Math.abs(ar - r), ds = Math.abs(as + q + r);
  if (dq > dr && dq > ds) aq = -ar - as; else if (dr > ds) ar = -aq - as;
  return key(aq, ar);
}

export class WorldMap {
  constructor(canvas, { onSelect = () => {}, onHover = () => {}, onMove = () => {} } = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d', { alpha: false });
    this.callbacks = { onSelect, onHover, onMove };
    this.tiles = new Map(); this.sortedTiles = [];
    this.view = null; this.me = 0;
    this.selection = null; this.hover = null; this.lens = 'normal';
    this.overlay = { reach: new Map(), attacks: [], found: null, unit: null };
    this.drafts = [];
    this.zoom = 1.1; this.offset = { x: 0, y: 0 }; this.targetOffset = null;
    this.anim = new Map(); // unit id -> {from:{x,y}, to:{x,y}, since}
    this.cache = document.createElement('canvas'); this.cacheCtx = this.cache.getContext('2d', { alpha: false });
    this.dirty = true; this.destroyed = false; this.listeners = []; this.lastFrame = 0;
    this.width = 1; this.height = 1; this.dpr = 1;
    canvas.tabIndex = 0;
    canvas.setAttribute('aria-label', '世界地図。クリックで選択、ダブルクリックで選択中の部隊を移動、ドラッグで地図を移動、ホイールで拡大縮小');
    canvas.style.cursor = 'grab';
    this._bind();
    this.resizeObserver = new ResizeObserver(() => this._resize()); this.resizeObserver.observe(canvas);
    this._resize();
    this.frame = requestAnimationFrame(t => this._frame(t));
  }

  // ---------------------------------------------------------------- input
  _listen(t, type, h, o) { t.addEventListener(type, h, o); this.listeners.push(() => t.removeEventListener(type, h, o)); }
  _bind() {
    const point = e => { const r = this.canvas.getBoundingClientRect(); return { x: e.clientX - r.left, y: e.clientY - r.top }; };
    this._listen(this.canvas, 'pointerdown', e => {
      if (e.button !== 0) return;
      const p = point(e); this.pointer = { id: e.pointerId, start: p, last: p, dragged: false };
      this.targetOffset = null; this.canvas.setPointerCapture(e.pointerId); this.canvas.style.cursor = 'grabbing';
    });
    this._listen(this.canvas, 'pointermove', e => {
      const p = point(e);
      if (this.pointer?.id === e.pointerId) {
        if (Math.hypot(p.x - this.pointer.start.x, p.y - this.pointer.start.y) > 5) this.pointer.dragged = true;
        if (this.pointer.dragged) { this.offset.x += p.x - this.pointer.last.x; this.offset.y += p.y - this.pointer.last.y; this.dirty = true; }
        this.pointer.last = p;
      } else {
        const id = this._hit(p.x, p.y);
        if (id !== this.hover) { this.hover = id; this.callbacks.onHover(id); }
      }
    });
    this._listen(this.canvas, 'pointerup', e => {
      if (this.pointer?.id !== e.pointerId) return;
      if (!this.pointer.dragged) { const id = this._hit(point(e).x, point(e).y); if (id) this.callbacks.onSelect(id); }
      this.pointer = null; if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
      this.canvas.style.cursor = 'grab';
    });
    this._listen(this.canvas, 'pointercancel', () => { this.pointer = null; this.canvas.style.cursor = 'grab'; });
    this._listen(this.canvas, 'pointerleave', () => { if (!this.pointer) { this.hover = null; this.callbacks.onHover(null); } });
    this._listen(this.canvas, 'dblclick', e => { e.preventDefault(); const id = this._hit(point(e).x, point(e).y); if (id) this.callbacks.onMove(id); });
    this._listen(this.canvas, 'wheel', e => { e.preventDefault(); this._zoomAt(Math.exp(-clamp(e.deltaY, -140, 140) * .0015), point(e)); }, { passive: false });
    this._listen(this.canvas, 'keydown', e => {
      const s = 65, m = { ArrowLeft: [s, 0], ArrowRight: [-s, 0], ArrowUp: [0, s], ArrowDown: [0, -s] }[e.key];
      if (m) { e.preventDefault(); this.targetOffset = null; this.offset.x += m[0]; this.offset.y += m[1]; this.dirty = true; }
      else if (e.key === '+' || e.key === '=') this.zoomBy(1.15);
      else if (e.key === '-') this.zoomBy(1 / 1.15);
      else if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && this.selection) this.callbacks.onMove(this.selection); // Ctrl/⌘+Enter is commit
    });
  }
  _usableCenter(forSelection = false) {
    if (this.width <= 760) return { x: this.width * .5, y: this.height * (forSelection ? .32 : .45) };
    return { x: this.width > 1000 ? this.width / 2 - 20 : this.width / 2, y: this.height / 2 + (forSelection ? -10 : 10) };
  }
  _resize() {
    const rect = this.canvas.getBoundingClientRect();
    const oldC = this._usableCenter();
    this.width = Math.max(1, Math.round(rect.width)); this.height = Math.max(1, Math.round(rect.height));
    const newC = this._usableCenter();
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.canvas.width = this.cache.width = Math.round(this.width * this.dpr);
    this.canvas.height = this.cache.height = Math.round(this.height * this.dpr);
    if (this.initialized) { this.offset.x += newC.x - oldC.x; this.offset.y += newC.y - oldC.y; }
    else { this.offset = newC; this.zoom = this.width < 680 ? .7 : this.height < 700 ? .9 : 1.05; }
    this.dirty = true; this.renderNow();
  }

  // ---------------------------------------------------------------- data
  setMap(map) {
    this.tiles = new Map(map.tiles.map(([q, r, terrain, river, resource], i) => [key(q, r), { id: key(q, r), q, r, terrain, river, resource, index: i }]));
    this.sortedTiles = [...this.tiles.values()].sort((a, b) => a.r - b.r || a.q - b.q);
    this.hubs = new Set((map.hubs || []).map(([q, r]) => key(q, r)));
    this.dirty = true;
  }
  owner(tile) { const c = this.view?.owners?.[tile.index]; return c && c !== '.' ? parseInt(c, 36) : null; }
  /** '0' never seen · '1' remembered · '2' in sight (server fog, §7.4). */
  fogAt(tile) { return this.view?.fog?.[tile.index] ?? '2'; }
  setView(view, me) {
    const now = performance.now();
    const prevUnits = new Map((this.view?.units || []).map(u => [u.id, u]));
    for (const u of view.units) {
      const before = prevUnits.get(u.id);
      if (before && (before.q !== u.q || before.r !== u.r)) {
        const from = this._unitPos(before.id, now) || project(before.q, before.r);
        this.anim.set(u.id, { from, to: project(u.q, u.r), since: now });
      }
    }
    const firstView = !this.view;
    this.view = view; this.me = me;
    this.cityAt = new Map(view.cities.map(c => [key(c.q, c.r), c]));
    this.csAt = new Map(view.cityStates.map(c => [key(c.q, c.r), c]));
    if (firstView) this.focusHome(true);
    this.dirty = true; this.renderNow();
  }
  setOverlay(o) { this.overlay = { reach: new Map(), attacks: [], found: null, unit: null, ...o }; this.renderNow(); }
  setDrafts(d) { this.drafts = d || []; this.renderNow(); }
  setSelection(id) { this.selection = this.tiles.has(id) ? id : null; this.renderNow(); }
  setLens(l) { this.lens = ['normal', 'political', 'yields', 'military', 'concord'].includes(l) ? l : 'normal'; this.dirty = true; this.renderNow(); }
  focusTile(id, instant = false) {
    const t = this.tiles.get(id); if (!t) return;
    const p = project(t.q, t.r), c = this._usableCenter(true);
    const target = { x: c.x - p.x * this.zoom, y: c.y - p.y * this.zoom };
    if (instant) { this.offset = target; this.dirty = true; this.renderNow(); } else this.targetOffset = target;
  }
  focusHome(instant = false) {
    const civ = this.view?.civs?.[this.me]; const cap = this.view?.cities.find(c => c.id === civ?.capital);
    if (cap) this.focusTile(key(cap.q, cap.r), instant);
    this.initialized = true;
  }
  zoomBy(f) { this._zoomAt(f, this._usableCenter()); }
  _zoomAt(f, p) {
    const next = clamp(this.zoom * f, .42, 2.4), ratio = next / this.zoom;
    this.offset.x = p.x - (p.x - this.offset.x) * ratio; this.offset.y = p.y - (p.y - this.offset.y) * ratio;
    this.zoom = next; this.targetOffset = null; this.dirty = true;
  }
  screenPosition(id) {
    const t = this.tiles.get(id); if (!t) return null;
    const p = project(t.q, t.r); return { x: p.x * this.zoom + this.offset.x, y: p.y * this.zoom + this.offset.y };
  }
  viewport() { // world-space rectangle visible on screen, for the minimap
    return { x0: -this.offset.x / this.zoom, y0: -this.offset.y / this.zoom, x1: (this.width - this.offset.x) / this.zoom, y1: (this.height - this.offset.y) / this.zoom };
  }
  centerOnWorld(x, y) { const c = this._usableCenter(); this.targetOffset = { x: c.x - x * this.zoom, y: c.y - y * this.zoom }; }
  _hit(x, y) { const id = inverseHex((x - this.offset.x) / this.zoom, (y - this.offset.y) / this.zoom); return this.tiles.has(id) ? id : null; }

  _unitPos(id, now) {
    const a = this.anim.get(id); const u = this.view?.units.find(v => v.id === id);
    if (!u) return null;
    const to = project(u.q, u.r);
    if (!a) return to;
    const t = clamp((now - a.since) / 900, 0, 1);
    if (t >= 1) { this.anim.delete(id); return to; }
    const e = t < .5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
    return { x: a.from.x + (to.x - a.from.x) * e, y: a.from.y + (to.y - a.from.y) * e, moving: true };
  }

  // ---------------------------------------------------------------- frame
  renderNow() { if (!this.destroyed) this._draw(performance.now()); }
  _frame(now) {
    if (this.destroyed) return;
    this.frame = requestAnimationFrame(t => this._frame(t));
    if (document.hidden || now - this.lastFrame < 32) return;
    this.lastFrame = now;
    if (this.targetOffset) {
      this.offset.x += (this.targetOffset.x - this.offset.x) * .2; this.offset.y += (this.targetOffset.y - this.offset.y) * .2;
      if (Math.hypot(this.offset.x - this.targetOffset.x, this.offset.y - this.targetOffset.y) < .6) { this.offset = this.targetOffset; this.targetOffset = null; }
      this.dirty = true;
    }
    this._draw(now);
  }
  _draw(now) {
    if (this.dirty) { this._renderStatic(); this.dirty = false; }
    const ctx = this.ctx;
    ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.drawImage(this.cache, 0, 0);
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    if (!this.view) return;
    ctx.save(); ctx.translate(this.offset.x, this.offset.y); ctx.scale(this.zoom, this.zoom);
    this._drawOverlays(ctx, now);
    this._drawSites(ctx);
    this._drawUnits(ctx, now);
    this._drawDraftPaths(ctx, now);
    this._drawLabels(ctx);
    this._drawHoverEta(ctx);
    ctx.restore();
    this._drawCompass(ctx);
  }
  _visible(t, pad = 120) {
    const p = project(t.q, t.r); const x = p.x * this.zoom + this.offset.x, y = p.y * this.zoom + this.offset.y;
    return x > -pad && y > -pad && x < this.width + pad && y < this.height + pad;
  }
  _renderStatic() {
    const ctx = this.cacheCtx; ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    const bg = ctx.createRadialGradient(this.width * .45, this.height * .45, 40, this.width * .45, this.height * .45, Math.max(this.width, this.height) * .75);
    bg.addColorStop(0, '#eee9d9'); bg.addColorStop(.58, '#e6e4d5'); bg.addColorStop(1, '#cfd8cd');
    ctx.fillStyle = bg; ctx.fillRect(0, 0, this.width, this.height);
    ctx.fillStyle = 'rgba(90,111,96,.055)';
    for (let i = 0; i < 280; i++) ctx.fillRect(seed(i, 3) * this.width, seed(i, 7) * this.height, 1, 1);
    if (!this.sortedTiles.length) return;
    ctx.save(); ctx.translate(this.offset.x, this.offset.y); ctx.scale(this.zoom, this.zoom);
    const tiles = this.sortedTiles.filter(t => this._visible(t));
    for (const t of tiles) this._drawTile(ctx, t);
    // Lenses push the painted terrain back so the data reads first (Civ VI style).
    if (this.lens !== 'normal') for (const t of tiles) { const p = project(t.q, t.r); polygon(ctx, hexPoints(p.x, p.y, .65), 'rgba(238,234,218,.58)'); }
    for (const t of tiles) this._drawTerritory(ctx, t);
    ctx.globalAlpha = this.lens === 'normal' ? 1 : .32;
    for (const t of tiles) this._drawDecor(ctx, t);
    ctx.globalAlpha = 1;
    for (const t of tiles) this._drawLens(ctx, t);
    for (const t of tiles) this._drawBorders(ctx, t);
    ctx.restore();
    this._drawFog(ctx, tiles);
  }

  // ---------------------------------------------------------------- fog of war
  // Drawn as one soft layer: parchment where nothing was ever seen (terrain is
  // public, so it shows faintly), a cool veil where the map is only remembered.
  _drawFog(ctx, tiles) {
    if (!this.view?.fog) return;
    const fog = this.fogCanvas ||= document.createElement('canvas');
    if (fog.width !== this.cache.width || fog.height !== this.cache.height) { fog.width = this.cache.width; fog.height = this.cache.height; }
    const f = fog.getContext('2d');
    f.setTransform(1, 0, 0, 1, 0, 0); f.clearRect(0, 0, fog.width, fog.height);
    f.setTransform(this.dpr, 0, 0, this.dpr, 0, 0); f.translate(this.offset.x, this.offset.y); f.scale(this.zoom, this.zoom);
    for (const [code, color] of [['0', 'rgba(236,229,208,.86)'], ['1', 'rgba(196,203,194,.52)']]) {
      f.beginPath();
      for (const t of tiles) {
        if (this.fogAt(t) !== code) continue;
        const p = project(t.q, t.r); const pts = hexPoints(p.x, p.y, -1.5);
        pts.forEach(([x, y], i) => i ? f.lineTo(x, y) : f.moveTo(x, y)); f.closePath();
      }
      f.fillStyle = color; f.fill(); // one fill = union, no double-dark seams
    }
    // Faint survey hatching on uncharted land.
    f.save(); f.beginPath();
    for (const t of tiles) { if (this.fogAt(t) !== '0') continue; const p = project(t.q, t.r); hexPoints(p.x, p.y, -1.5).forEach(([x, y], i) => i ? f.lineTo(x, y) : f.moveTo(x, y)); f.closePath(); }
    f.clip(); f.strokeStyle = 'rgba(150,132,96,.10)'; f.lineWidth = 1 / this.zoom; f.beginPath();
    const vp = this.viewport();
    for (let x = vp.x0 - vp.y1; x < vp.x1; x += 14) { f.moveTo(x, vp.y0); f.lineTo(x + (vp.y1 - vp.y0), vp.y1); }
    f.stroke(); f.restore();
    ctx.save(); ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.filter = `blur(${Math.max(2, 5 * this.zoom * this.dpr)}px)`; ctx.drawImage(fog, 0, 0);
    ctx.filter = 'none'; ctx.restore();
  }

  // ---------------------------------------------------------------- terrain (prototype art)
  _drawTile(ctx, t) {
    const p = project(t.q, t.r), pts = hexPoints(p.x, p.y, .65);
    const pal = COLORS[t.terrain] || COLORS.Grassland;
    const depth = t.terrain === 'Water' ? 3 : 7;
    for (let i = 0; i < 3; i++) { const a = pts[i], b = pts[i + 1]; polygon(ctx, [a, b, [b[0], b[1] + depth], [a[0], a[1] + depth]], i === 0 ? pal[1] : pal[2]); }
    const top = ctx.createLinearGradient(p.x - 30, p.y - 25, p.x + 30, p.y + 35); top.addColorStop(0, pal[0]); top.addColorStop(1, pal[1]);
    polygon(ctx, pts, top, 'rgba(239,237,208,.48)', .8);
    ctx.save(); polygon(ctx, pts); ctx.clip();
    if (t.terrain === 'Water') {
      ctx.strokeStyle = 'rgba(204,229,208,.5)'; ctx.lineWidth = 1.3;
      for (let i = 0; i < 5; i++) { const x = p.x - 35 + seed(t.q, t.r, i) * 55, y = p.y - 24 + i * 11; ctx.beginPath(); ctx.moveTo(x, y); ctx.bezierCurveTo(x + 6, y - 3, x + 12, y + 3, x + 23, y); ctx.stroke(); }
    } else {
      for (let i = 0; i < 15; i++) {
        const x = p.x - 37 + seed(t.q, t.r, i + 1) * 74, y = p.y - 28 + seed(t.q, t.r, i + 27) * 56;
        ctx.fillStyle = i % 3 === 0 ? 'rgba(246,234,181,.25)' : 'rgba(63,89,61,.10)'; ctx.fillRect(x, y, 1.5 + seed(t.q, t.r, i + 50) * 3, 1.1);
      }
      if ((t.terrain === 'Grassland' || t.terrain === 'Plains') && !this.cityAt?.has(t.id)) {
        ctx.strokeStyle = t.terrain === 'Plains' ? 'rgba(150,130,70,.3)' : 'rgba(83,117,65,.3)'; ctx.lineWidth = 1;
        for (let i = 0; i < 5; i++) { const x = p.x - 29 + seed(t.q, t.r, i + 91) * 58, y = p.y - 19 + seed(t.q, t.r, i + 112) * 38; ctx.beginPath(); ctx.moveTo(x - 2, y - 3); ctx.lineTo(x, y); ctx.lineTo(x + 1, y - 4); ctx.stroke(); }
      }
      if (t.river) { // a river threads through the tile
        ctx.strokeStyle = '#7fb0b4'; ctx.lineWidth = 3.2; ctx.lineCap = 'round';
        const a = seed(t.q, t.r, 5) * Math.PI;
        ctx.beginPath(); ctx.moveTo(p.x + Math.cos(a) * 44, p.y + Math.sin(a) * 30);
        ctx.quadraticCurveTo(p.x + 8, p.y - 6, p.x - Math.cos(a) * 44, p.y - Math.sin(a) * 30); ctx.stroke();
        ctx.strokeStyle = 'rgba(225,242,236,.7)'; ctx.lineWidth = 1; ctx.stroke(); ctx.lineCap = 'butt';
      }
    }
    ctx.restore();
  }
  _drawTerritory(ctx, t) {
    const o = this.owner(t); if (o === null) return;
    const p = project(t.q, t.r);
    const strong = this.lens === 'political';
    polygon(ctx, hexPoints(p.x, p.y, 1.2), alpha(CIV_COLORS[o], strong ? .46 : this.lens === 'normal' ? .13 : .08));
    if (strong && o !== this.me && this.view.civs[o]?.relation === 'war') {
      ctx.save(); polygon(ctx, hexPoints(p.x, p.y, 1.2)); ctx.clip();
      ctx.strokeStyle = 'rgba(160,50,40,.32)'; ctx.lineWidth = 2;
      for (let i = -60; i < 60; i += 9) { ctx.beginPath(); ctx.moveTo(p.x + i, p.y - 40); ctx.lineTo(p.x + i + 40, p.y + 40); ctx.stroke(); }
      ctx.restore();
    }
  }
  _drawBorders(ctx, t) {
    const o = this.owner(t); if (o === null) return;
    const p = project(t.q, t.r), pts = hexPoints(p.x, p.y, 3.2);
    const width = (this.lens === 'political' ? 4 : 3) / Math.max(.8, this.zoom * .9);
    for (let i = 0; i < 6; i++) {
      const [dq, dr] = EDGE_NEIGHBOR[i]; const n = this.tiles.get(key(t.q + dq, t.r + dr));
      if (n && this.owner(n) === o) continue;
      const a = pts[i], b = pts[(i + 1) % 6];
      ctx.strokeStyle = shade(CIV_COLORS[o], -.08); ctx.lineWidth = width; ctx.lineCap = 'round';
      ctx.setLineDash(CIV_DASH[o % CIV_DASH.length].map(v => v * 1.6));
      ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke();
    }
    ctx.setLineDash([]); ctx.lineCap = 'butt';
  }
  _drawDecor(ctx, t) {
    const p = project(t.q, t.r); const site = this.cityAt?.has(t.id) || this.csAt?.has(t.id);
    if (t.terrain === 'Forest') {
      const pos = site ? [[-30, 4, .7], [27, -6, .68]] : [[-20, -5, .93], [5, -14, 1.03], [22, 2, .86], [-5, 14, .83]];
      pos.forEach(([x, y, s], i) => this._tree(ctx, p.x + x, p.y + y, s, seed(t.q, t.r, i)));
    }
    if (t.terrain === 'Mountain') { this._mountain(ctx, p.x - 17, p.y + 7, 42, .84); this._mountain(ctx, p.x + 9, p.y + 13, 67, 1.03); this._mountain(ctx, p.x + 26, p.y + 23, 28, .7); }
    else if (t.terrain === 'Hills' && !site) { this._rock(ctx, p.x - 20, p.y + 4, 19); this._rock(ctx, p.x + 15, p.y + 15, 13); }
    if (t.resource && !site) this._resource(ctx, p.x, p.y, t.resource);
    if (this.hubs?.has(t.id)) this._hub(ctx, p.x + 16, p.y + 10);
    const ruin = this.view?.ruins?.find(([q, r]) => q === t.q && r === t.r);
    if (ruin && !site) this._ruin(ctx, p.x, p.y);
  }
  _resource(ctx, x, y, res) {
    if (res === 'Wheat') {
      ctx.strokeStyle = '#c9a43f'; ctx.lineWidth = 1.4;
      for (let i = 0; i < 4; i++) { const bx = x - 14 + i * 6; ctx.beginPath(); ctx.moveTo(bx, y + 12); ctx.lineTo(bx + 2, y - 3); ctx.stroke(); ctx.fillStyle = '#e1bf57'; ctx.beginPath(); ctx.ellipse(bx + 2, y - 5, 2, 4, .2, 0, Math.PI * 2); ctx.fill(); }
    } else if (res === 'Iron') {
      for (let i = 0; i < 3; i++) polygon(ctx, [[x - 18 + i * 6, y + 2 + i * 2], [x - 14 + i * 6, y - 4 + i * 2], [x - 10 + i * 6, y + 2 + i * 2]], i === 1 ? '#8f6a4f' : '#b47c4f', '#6f5645', .5);
    } else if (res === 'Horses') {
      ctx.save(); ctx.translate(x - 10, y + 4); this._horse(ctx, '#a07a55', .75); ctx.restore();
    }
  }
  _hub(ctx, x, y) {
    polygon(ctx, [[x - 10, y], [x, y - 13], [x + 10, y]], '#c9a45f', '#8b6e45', .7);
    polygon(ctx, [[x - 10, y], [x, y - 13], [x - 2, y]], '#e2c07a');
    ctx.fillStyle = '#8b6e45'; ctx.fillRect(x - 9, y, 1.5, 6); ctx.fillRect(x + 7.5, y, 1.5, 6);
    rounded(ctx, x - 8, y + 2, 16, 4, 1); ctx.fillStyle = '#b98f5a'; ctx.fill();
  }
  _ruin(ctx, x, y) {
    ctx.fillStyle = 'rgba(60,60,50,.15)'; ctx.beginPath(); ctx.ellipse(x, y + 6, 22, 7, 0, 0, Math.PI * 2); ctx.fill();
    for (const [dx, h] of [[-12, 16], [-2, 9], [9, 20], [16, 6]]) { ctx.fillStyle = '#b9b29c'; ctx.fillRect(x + dx, y + 4 - h, 5, h); ctx.fillStyle = '#d6d0ba'; ctx.fillRect(x + dx, y + 4 - h, 2, h); }
    polygon(ctx, [[x - 14, y + 6], [x + 20, y + 4], [x + 18, y + 8], [x - 12, y + 9]], '#a39c86');
  }
  _tree(ctx, x, y, s, v) {
    ctx.save(); ctx.translate(x, y); ctx.scale(s, s);
    ctx.fillStyle = 'rgba(39,68,48,.18)'; ctx.beginPath(); ctx.ellipse(7, 3, 17, 6, -.1, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = '#725c41'; ctx.fillRect(-2, -16, 4, 20);
    if (v > .45) {
      polygon(ctx, [[-16, -10], [0, -46], [17, -10]], '#416b53'); polygon(ctx, [[-13, -23], [0, -48], [1, -15]], '#6d8d61');
      polygon(ctx, [[-13, -7], [0, -35], [15, -7]], '#517954'); polygon(ctx, [[-13, -7], [0, -35], [0, -5]], '#799760');
    } else {
      const g = v < .16 ? ['#a0a365', '#7d8f57', '#617c53'] : ['#86a379', '#638b67', '#476f58'];
      ctx.fillStyle = g[2]; ctx.beginPath(); ctx.ellipse(4, -23, 18, 17, .2, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = g[1]; ctx.beginPath(); ctx.ellipse(-5, -30, 17, 16, -.3, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = g[0]; ctx.beginPath(); ctx.ellipse(-9, -35, 10, 9, -.3, 0, Math.PI * 2); ctx.fill();
    }
    ctx.restore();
  }
  _mountain(ctx, x, y, h, s) {
    ctx.save(); ctx.translate(x, y); ctx.scale(s, s);
    ctx.fillStyle = 'rgba(59,71,67,.16)'; ctx.beginPath(); ctx.ellipse(10, 3, 27, 9, .05, 0, Math.PI * 2); ctx.fill();
    polygon(ctx, [[-28, 2], [-4, -h], [10, -h * .6], [27, 2], [0, 12]], '#a8aa9b');
    polygon(ctx, [[-4, -h], [10, -h * .6], [27, 2], [0, 12], [1, -h * .55]], '#7d8a82');
    polygon(ctx, [[-28, 2], [-4, -h], [-7, -h * .36], [-16, -2]], '#c8c6b0');
    polygon(ctx, [[-11, -h * .69], [-4, -h], [5, -h * .74], [0, -h * .78], [-5, -h * .7]], '#e9e6d2');
    ctx.restore();
  }
  _rock(ctx, x, y, s) {
    polygon(ctx, [[x - s, y], [x - s * .45, y - s * .78], [x + s * .25, y - s], [x + s, y - s * .2], [x + s * .55, y + 4], [x - 3, y + 7]], '#9b9e8d');
    polygon(ctx, [[x - s, y], [x - s * .45, y - s * .78], [x + s * .25, y - s], [x - 3, y + 1]], '#c3c0a8');
    polygon(ctx, [[x + s * .25, y - s], [x + s, y - s * .2], [x + s * .55, y + 4], [x - 3, y + 7], [x - 3, y + 1]], '#878e80');
  }
  _house(ctx, x, y, w, h, roof = '#b4694c', tower = false, roofLight = '#cd8a5e') {
    const depth = w * .42;
    ctx.fillStyle = 'rgba(41,63,54,.15)'; ctx.beginPath(); ctx.ellipse(x + 8, y + 5, w * .8, 7, 0, 0, Math.PI * 2); ctx.fill();
    polygon(ctx, [[x - w / 2, y - h], [x + w / 2, y - h + 6], [x + w / 2, y + 5], [x - w / 2, y - 1]], '#e9d7ad', '#927b5c', .5);
    polygon(ctx, [[x + w / 2, y - h + 6], [x + w / 2 + depth, y - h - 3], [x + w / 2 + depth, y - 4], [x + w / 2, y + 5]], '#c5b996', '#927b5c', .5);
    polygon(ctx, [[x - w / 2 - 4, y - h], [x - 3, y - h - 18], [x + w / 2 + depth + 4, y - h - 11], [x + w / 2 + 3, y - h + 8]], roof, '#795d48', .6);
    polygon(ctx, [[x - w / 2 - 4, y - h], [x - 3, y - h - 18], [x + w / 2 + 3, y - h + 8]], roofLight, '#795d48', .6);
    ctx.fillStyle = '#526a60'; ctx.fillRect(x - w / 2 + 5, y - h + 8, 5, 7);
    ctx.fillStyle = '#6a614a'; ctx.fillRect(x + 1, y - 9, 7, 12); ctx.fillStyle = '#d2b375'; ctx.fillRect(x + 2, y - 8, 1.5, 10);
    if (tower) {
      ctx.fillStyle = '#c9bc97'; ctx.fillRect(x - 7, y - h - 39, 11, 27);
      polygon(ctx, [[x - 11, y - h - 37], [x - 2, y - h - 48], [x + 8, y - h - 35]], roof, '#476555', .6);
      ctx.fillStyle = '#5f7569'; ctx.fillRect(x - 3, y - h - 31, 3, 7);
    }
  }
  _horse(ctx, color, s = 1) {
    ctx.save(); ctx.scale(s, s);
    ctx.fillStyle = color; ctx.beginPath(); ctx.ellipse(0, -6, 10, 5, 0, 0, Math.PI * 2); ctx.fill();
    polygon(ctx, [[7, -9], [13, -17], [16, -15], [11, -6]], color);
    ctx.strokeStyle = shade('#8a6a48', -.2); ctx.lineWidth = 1.6;
    for (const dx of [-7, -3, 4, 8]) { ctx.beginPath(); ctx.moveTo(dx, -3); ctx.lineTo(dx + (dx % 2 ? .5 : -.5), 5); ctx.stroke(); }
    ctx.restore();
  }

  // ---------------------------------------------------------------- lenses
  _drawLens(ctx, t) {
    if (this.lens === 'normal' || this.lens === 'political') return;
    const p = project(t.q, t.r);
    if (this.lens === 'yields' && t.terrain !== 'Mountain') {
      const base = { Grassland: [2, 0, 0], Plains: [1, 1, 0], Forest: [1, 2, 0], Hills: [0, 2, 0], Water: [1, 0, 1] }[t.terrain] || [0, 0, 0];
      const y = [...base]; if (t.river) y[2]++; if (t.resource === 'Wheat') y[0] += 2; if (t.resource === 'Iron') y[1]++; if (t.resource === 'Horses') y[0]++;
      const cols = ['#7f9a3e', '#9a6b3c', '#c9a43f']; let x = p.x - (y[0] + y[1] + y[2]) * 4.5;
      rounded(ctx, x - 4, p.y + 12, (y[0] + y[1] + y[2]) * 9 + 8, 12, 5); ctx.fillStyle = 'rgba(251,248,239,.86)'; ctx.fill();
      y.forEach((n, i) => { for (let k = 0; k < n; k++) { ctx.fillStyle = cols[i]; ctx.beginPath(); ctx.arc(x + 4, p.y + 18, 3.2, 0, Math.PI * 2); ctx.fill(); x += 9; } });
    }
    if (this.lens === 'military' && this.view) {
      const threat = this.view.units.some(u => !u.civilian && u.owner !== this.me && (this.view.civs[u.owner]?.relation === 'war' || u.owner === 'barbarian') && hexDist(u, t) <= 2);
      if (threat) polygon(ctx, hexPoints(p.x, p.y, 3), 'rgba(172,98,81,.2)', 'rgba(172,98,81,.45)', 1.2);
    }
    if (this.lens === 'concord' && this.view) {
      for (const cs of this.view.cityStates) {
        if (cs.capturedBy !== null) continue;
        const d = hexDist(cs, t); if (d > 2) continue;
        const c = cs.suzerain !== null ? CIV_COLORS[cs.suzerain] : '#8a9a8a';
        polygon(ctx, hexPoints(p.x, p.y, 3), alpha(c.startsWith('#') ? c : '#8a9a8a', d === 0 ? .32 : .14), null);
      }
    }
  }

  // ---------------------------------------------------------------- sites (cities, city-states)
  _drawSites(ctx) {
    const sites = [...this.view.cities.map(c => ({ c, cs: false })), ...this.view.cityStates.filter(c => c.capturedBy === null).map(c => ({ c, cs: true }))];
    sites.sort((a, b) => a.c.r - b.c.r);
    for (const { c, cs } of sites) {
      const t = this.tiles.get(key(c.q, c.r)); if (!t || !this._visible(t)) continue;
      const p = project(c.q, c.r);
      ctx.globalAlpha = c.seenTick !== null && c.seenTick !== undefined ? .62 : 1; // last known, not live
      if (cs) this._cityState(ctx, p.x, p.y, c); else this._city(ctx, p.x, p.y, c);
      ctx.globalAlpha = 1;
    }
  }
  _city(ctx, x, y, c) {
    const col = c.owner === null ? '#9a9a8c' : CIV_COLORS[c.owner];
    const roof = shade(col, -.15), light = shade(col, .15);
    if (c.walls) { ctx.strokeStyle = '#a79f86'; ctx.lineWidth = 5; ctx.beginPath(); ctx.ellipse(x + 2, y + 2, 38, 20, 0, 0, Math.PI * 2); ctx.stroke(); ctx.strokeStyle = '#cfc7ae'; ctx.lineWidth = 1.5; ctx.stroke(); }
    polygon(ctx, [[x - 30, y - 1], [x, y - 15], [x + 32, y - 1], [x + 3, y + 16]], '#d8cdac', '#b4ae93', .8);
    if (c.pop >= 6) this._house(ctx, x + 16, y - 6, 20, 17, roof, false, light);
    if (c.pop >= 3) this._house(ctx, x - 17, y - 4, 19, 16, roof, false, light);
    this._house(ctx, x - 1, y + 6, c.pop >= 3 ? 27 : 24, c.pop >= 3 ? 24 : 21, roof, c.capital, light);
    if (c.stages > 0) { // Star Gate: a ring that fills stage by stage
      ctx.save(); ctx.translate(x + 26, y - 30);
      ctx.strokeStyle = '#8fb8c8'; ctx.lineWidth = 3; ctx.beginPath(); ctx.arc(0, 0, 10, 0, Math.PI * 2); ctx.stroke();
      ctx.strokeStyle = '#e8d38a'; ctx.lineWidth = 3; ctx.beginPath(); ctx.arc(0, 0, 10, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * c.stages / 3); ctx.stroke();
      ctx.restore();
    }
    if (c.razing !== null && c.razing !== undefined) { ctx.fillStyle = 'rgba(90,70,60,.35)'; for (let i = 0; i < 3; i++) { ctx.beginPath(); ctx.arc(x - 6 + i * 7, y - 38 - i * 6, 6 + i * 2, 0, Math.PI * 2); ctx.fill(); } }
  }
  _cityState(ctx, x, y, cs) {
    polygon(ctx, [[x - 24, y], [x, y - 11], [x + 26, y], [x + 2, y + 12]], '#d9d3bd', '#b6af97', .8);
    ctx.fillStyle = '#d7ccaa'; ctx.fillRect(x - 8, y - 38, 16, 38);
    ctx.fillStyle = '#b8ad8c'; ctx.fillRect(x + 4, y - 38, 4, 38);
    polygon(ctx, [[x - 11, y - 38], [x, y - 52], [x + 11, y - 38]], '#8a9a8e', '#5e6e63', .7);
    ctx.fillStyle = '#6e6250'; ctx.fillRect(x - 2, y - 30, 4, 7);
    const flag = cs.suzerain !== null ? CIV_COLORS[cs.suzerain] : '#f4f0e0';
    ctx.fillStyle = '#6e6250'; ctx.fillRect(x - .7, y - 66, 1.4, 16);
    polygon(ctx, [[x + .7, y - 66], [x + 14, y - 62], [x + .7, y - 57]], flag, '#6e6250', .5);
  }

  // ---------------------------------------------------------------- units
  _drawUnits(ctx, now) {
    const units = [...this.view.units].sort((a, b) => a.r - b.r);
    for (const u of units) {
      const t = this.tiles.get(key(u.q, u.r)); if (!t || !this._visible(t)) continue;
      const pos = this._unitPos(u.id, now) || project(u.q, u.r);
      const inSite = this.cityAt.has(t.id) || this.csAt.has(t.id);
      const x = pos.x + (inSite ? (u.civilian ? -24 : 22) : 0), y = pos.y + (inSite ? 16 : 4);
      const col = u.owner === 'barbarian' ? '#5f5a52' : CIV_COLORS[u.owner];
      const mine = u.owner === this.me;
      const selected = this.overlay.unit === u.id;
      if (selected || mine) {
        ctx.strokeStyle = selected ? '#ffe6a1' : alpha(col, .6); ctx.lineWidth = selected ? 2.2 : 1.2;
        ctx.fillStyle = selected ? 'rgba(235,201,114,.2)' : 'rgba(255,255,255,.08)';
        ctx.beginPath(); ctx.ellipse(x, y + 3, 17 + (selected ? Math.sin(now / 500) : 0), 7, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
      }
      if (u.type === 'Settler') this._cart(ctx, x, y, col);
      else if (u.type === 'Scout') this._figure(ctx, x, y, col, now, pos.moving, 'cloak');
      else {
        const n = u.troops >= 120 ? 3 : u.troops >= 50 ? 2 : 1;
        const mounted = u.type === 'Horseman' || u.type === 'Knight';
        const ranged = u.type === 'Archer' || u.type === 'Crossbowman';
        const tier2 = ['Pikeman', 'Crossbowman', 'Knight'].includes(u.type);
        const slots = [[0, 0], [-11, -5], [11, -5]].slice(0, n);
        for (const [dx, dy] of slots.slice().reverse()) {
          if (mounted) { ctx.save(); ctx.translate(x + dx, y + dy + 2); this._horse(ctx, tier2 ? '#6e5a44' : '#a07a55', .9); ctx.restore(); }
          this._figure(ctx, x + dx, y + dy - (mounted ? 7 : 0), col, now, pos.moving, ranged ? 'bow' : 'spear', tier2);
        }
        // banner and troop badge
        ctx.fillStyle = '#5b4d3b'; ctx.fillRect(x + 15, y - 38, 1.5, 36);
        polygon(ctx, [[x + 16.5, y - 38], [x + 30, y - 34], [x + 16.5, y - 29]], col, shade(col, -.35), .6);
        const label = (u.troops / 10).toFixed(u.troops % 10 ? 1 : 0);
        ctx.font = '600 9px system-ui, sans-serif'; const w = ctx.measureText(label).width + 10;
        rounded(ctx, x - w / 2, y + 9, w, 13, 6); ctx.fillStyle = mine ? '#284d40' : 'rgba(250,247,236,.95)'; ctx.fill();
        ctx.strokeStyle = alpha(col, .9); ctx.lineWidth = 1; ctx.stroke();
        ctx.fillStyle = mine ? '#fff1cc' : '#3b4a43'; ctx.textAlign = 'center'; ctx.fillText(label, x, y + 19);
      }
    }
  }
  _figure(ctx, x, y, col, now, moving, kind = 'spear', tier2 = false) {
    const bob = moving ? Math.sin(now / 115) * 1.1 : 0, stride = moving ? Math.sin(now / 100) * 2.3 : 0;
    ctx.fillStyle = 'rgba(43,65,52,.22)'; ctx.beginPath(); ctx.ellipse(x + 2, y + 2, 6, 2.6, 0, 0, Math.PI * 2); ctx.fill();
    ctx.save(); ctx.translate(x, y + bob);
    ctx.strokeStyle = '#454d42'; ctx.lineWidth = 2.3; ctx.lineCap = 'round';
    ctx.beginPath(); ctx.moveTo(-2, -6); ctx.lineTo(-3 - stride, 1); ctx.moveTo(2, -6); ctx.lineTo(3 + stride, 1); ctx.stroke();
    polygon(ctx, [[-5, -17], [3, -18], [6, -6], [-5, -5]], kind === 'cloak' ? '#b9ab86' : col, shade(col, -.4), .6);
    if (tier2) { ctx.fillStyle = '#c8cdc9'; ctx.fillRect(-5, -17, 10, 3); }
    ctx.fillStyle = '#d9af80'; ctx.beginPath(); ctx.arc(0, -22, 4, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = tier2 ? '#9aa39f' : '#6c7257'; ctx.beginPath(); ctx.ellipse(-.5, -24, 4.5, 2.4, -.12, Math.PI, Math.PI * 2); ctx.fill();
    if (kind === 'spear') { ctx.strokeStyle = '#7a6448'; ctx.lineWidth = 1.2; ctx.beginPath(); ctx.moveTo(6, -2); ctx.lineTo(8, -32); ctx.stroke(); polygon(ctx, [[7, -32], [8.5, -37], [10, -32]], '#c9ccc4'); }
    if (kind === 'bow') { ctx.strokeStyle = '#7a6448'; ctx.lineWidth = 1.3; ctx.beginPath(); ctx.arc(6, -13, 8, -1.2, 1.2); ctx.stroke(); ctx.strokeStyle = '#e8e2cc'; ctx.lineWidth = .6; ctx.beginPath(); ctx.moveTo(6 + Math.cos(-1.2) * 8, -13 + Math.sin(-1.2) * 8); ctx.lineTo(6 + Math.cos(1.2) * 8, -13 + Math.sin(1.2) * 8); ctx.stroke(); }
    if (kind === 'cloak') { ctx.strokeStyle = '#6f6246'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(5, -2); ctx.lineTo(7, -26); ctx.stroke(); }
    ctx.lineCap = 'butt'; ctx.restore();
  }
  _cart(ctx, x, y, col) {
    ctx.fillStyle = 'rgba(39,69,51,.2)'; ctx.beginPath(); ctx.ellipse(x + 2, y + 4, 12, 4, 0, 0, Math.PI * 2); ctx.fill();
    for (const dx of [-7, 7]) { ctx.fillStyle = '#695d45'; ctx.beginPath(); ctx.arc(x + dx, y + 2, 3.2, 0, Math.PI * 2); ctx.fill(); }
    polygon(ctx, [[x - 10, y - 9], [x + 5, y - 12], [x + 11, y - 8], [x + 11, y], [x - 9, y + 1]], '#c6a568', '#8a734e', .7);
    polygon(ctx, [[x - 7, y - 10], [x + 4, y - 13], [x + 8, y - 9], [x - 3, y - 7]], '#ece0c0');
    ctx.fillStyle = '#5b4d3b'; ctx.fillRect(x - 1, y - 30, 1.3, 20);
    polygon(ctx, [[x + .3, y - 30], [x + 12, y - 26], [x + .3, y - 22]], col, shade(col, -.35), .5);
  }

  // ---------------------------------------------------------------- overlays
  _drawHoverEta(ctx) {
    const ticks = this.hover && this.overlay.reach.get(this.hover); if (!ticks) return;
    const t = this.tiles.get(this.hover); const p = project(t.q, t.r); const z = Math.max(this.zoom, .8);
    const label = ticks <= 1 ? '→ このティックで到着' : `→ ${ticks}ティックで到着`;
    ctx.font = `600 ${11 / z}px system-ui, sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    const w = ctx.measureText(label).width + 14 / z, h = 19 / z, y = p.y - 26 / z - h;
    rounded(ctx, p.x - w / 2, y, w, h, 5 / z); ctx.fillStyle = 'rgba(36,52,46,.9)'; ctx.fill();
    ctx.fillStyle = ticks <= 1 ? '#f6dc86' : '#e8e2cf'; ctx.fillText(label, p.x, y + h / 2); ctx.textBaseline = 'alphabetic';
  }
  _drawOverlays(ctx, now) {
    // Other capitals' protected zones: hatched, so "why can't I go there" is visible before asking.
    const radius = this.view.protectionRadius || 0;
    if (this.overlay.unit !== null && radius > 0) {
      for (const c of this.view.civs) {
        if (c.id === this.me || c.protectionLost) continue;
        const cap = this.view.cities.find(x => x.id === c.capital); if (!cap) continue;
        const col = CIV_COLORS[c.id];
        for (const t of this.sortedTiles) {
          if (hexDist(t, cap) > radius || !this._visible(t)) continue;
          const p = project(t.q, t.r), pts = hexPoints(p.x, p.y, 1.5);
          ctx.save(); polygon(ctx, pts); ctx.clip();
          ctx.strokeStyle = alpha(col, .38); ctx.lineWidth = 1.4 / this.zoom;
          ctx.beginPath(); for (let i = -60; i < 60; i += 8) { ctx.moveTo(p.x + i, p.y - 40); ctx.lineTo(p.x + i - 30, p.y + 40); } ctx.stroke();
          ctx.restore();
        }
      }
    }
    // Reach in tiers (Civ-style): this tick is bright with a hard outline, 2–3 ticks
    // a faint wash with a dashed edge; farther tiles only show their ETA on hover.
    const reach = this.overlay.reach;
    for (const [id, ticks] of reach) {
      if (ticks > 3) continue;
      const t = this.tiles.get(id); if (!t) continue; const p = project(t.q, t.r);
      polygon(ctx, hexPoints(p.x, p.y, 1.5), ticks <= 1 ? 'rgba(255,236,160,.34)' : 'rgba(255,244,205,.16)');
    }
    const within = (ticks, max) => ticks !== undefined && ticks <= max;
    for (const [max, stroke, width, dash] of [[3, 'rgba(120,98,48,.5)', 1.2, [5, 4]], [1, '#f6dc86', 2.6, []]]) {
      ctx.save(); ctx.setLineDash(dash.map(v => v / this.zoom)); ctx.strokeStyle = stroke; ctx.lineWidth = width / this.zoom; ctx.lineCap = 'round';
      if (max === 1) { ctx.shadowBlur = 6; ctx.shadowColor = 'rgba(90,70,20,.45)'; }
      ctx.beginPath();
      for (const [id, ticks] of reach) {
        if (!within(ticks, max)) continue;
        const t = this.tiles.get(id); if (!t) continue; const p = project(t.q, t.r); const pts = hexPoints(p.x, p.y, 1.5);
        for (let i = 0; i < 6; i++) {
          const [dq, dr] = EDGE_NEIGHBOR[i]; const nid = key(t.q + dq, t.r + dr);
          if (within(reach.get(nid), max) || nid === this.selection) continue;
          ctx.moveTo(pts[i][0], pts[i][1]); ctx.lineTo(pts[(i + 1) % 6][0], pts[(i + 1) % 6][1]);
        }
      }
      ctx.stroke(); ctx.restore();
    }
    for (const a of this.overlay.attacks) {
      const t = this.tiles.get(a.id); if (!t) continue; const p = project(t.q, t.r);
      const ok = !a.blocked;
      polygon(ctx, hexPoints(p.x, p.y, 3), ok ? 'rgba(190,80,60,.13)' : 'rgba(120,120,110,.08)', ok ? '#c0584a' : '#9a978a', (ok ? 2.4 : 1.2) / this.zoom);
    }
    if (this.overlay.found) {
      const t = this.tiles.get(this.overlay.found.id);
      if (t) { const p = project(t.q, t.r); ctx.setLineDash([4, 4]); polygon(ctx, hexPoints(p.x, p.y, -RADIUS * 0.95), null, this.overlay.found.ok ? '#6a8f5a' : '#b0705a', 2 / this.zoom); ctx.setLineDash([]); }
    }
    for (const [id, selected] of [[this.hover, false], [this.selection, true]]) {
      if (!id || (!selected && id === this.selection)) continue;
      const t = this.tiles.get(id); if (!t) continue; const p = project(t.q, t.r);
      // Dark casing first so the gold ring reads on parchment fog as well as on terrain.
      polygon(ctx, hexPoints(p.x, p.y, 2.5), null, 'rgba(40,52,46,.45)', (selected ? 4.6 : 3) / this.zoom);
      polygon(ctx, hexPoints(p.x, p.y, 2.5), selected ? 'rgba(249,220,139,.10)' : 'rgba(255,252,225,.10)', selected ? '#ffedb0' : 'rgba(255,248,217,.7)', (selected ? 2.4 : 1.4) / this.zoom);
      if (selected) { ctx.save(); ctx.shadowBlur = 8; ctx.shadowColor = 'rgba(221,190,112,.35)'; for (const [x, y] of hexPoints(p.x, p.y, 0)) { ctx.fillStyle = '#ffedb0'; ctx.beginPath(); ctx.arc(x, y, 2.3 / this.zoom, 0, Math.PI * 2); ctx.fill(); } ctx.restore(); }
    }
    // Engine paths still being walked (resolved state), thin and quiet.
    for (const u of this.view.units) {
      if (u.owner !== this.me || !u.path?.length) continue;
      const pos = this._unitPos(u.id, now) || project(u.q, u.r);
      ctx.beginPath(); ctx.moveTo(pos.x, pos.y); for (const [q, r] of u.path) { const n = project(q, r); ctx.lineTo(n.x, n.y); }
      ctx.strokeStyle = 'rgba(80,110,90,.55)'; ctx.lineWidth = 1.6 / this.zoom; ctx.setLineDash([2, 5]); ctx.stroke(); ctx.setLineDash([]);
    }
  }
  _drawDraftPaths(ctx, now) {
    for (const d of this.drafts) {
      if (!d.path?.length) continue;
      const start = project(d.from.q, d.from.r);
      ctx.beginPath(); ctx.moveTo(start.x, start.y);
      let last = start; for (const [q, r] of d.path) { last = project(q, r); ctx.lineTo(last.x, last.y); }
      ctx.lineJoin = ctx.lineCap = 'round';
      ctx.strokeStyle = 'rgba(46,58,50,.55)'; ctx.lineWidth = 5.5 / this.zoom; ctx.stroke(); // casing keeps it legible on any terrain
      ctx.strokeStyle = '#f6dc86'; ctx.lineWidth = 2.6 / this.zoom; ctx.setLineDash([7 / this.zoom, 6 / this.zoom]); ctx.lineDashOffset = -now / 60; ctx.stroke(); ctx.setLineDash([]);
      polygon(ctx, hexPoints(last.x, last.y, 7), 'rgba(246,220,134,.22)', '#f6dc86', 2 / this.zoom);
      ctx.fillStyle = '#2e3a32'; ctx.beginPath(); ctx.arc(last.x, last.y, 5.5 / this.zoom, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = '#f6dc86'; ctx.beginPath(); ctx.arc(last.x, last.y, 3.2 / this.zoom, 0, Math.PI * 2); ctx.fill();
    }
    for (const d of this.drafts) {
      if (!d.attackAt) continue;
      const a = project(d.from.q, d.from.r), b = project(d.attackAt.q, d.attackAt.r);
      ctx.strokeStyle = '#c0584a'; ctx.lineWidth = 3 / this.zoom; ctx.setLineDash([5, 4]); ctx.beginPath(); ctx.moveTo(a.x, a.y - 10); ctx.quadraticCurveTo((a.x + b.x) / 2, Math.min(a.y, b.y) - 40, b.x, b.y - 10); ctx.stroke(); ctx.setLineDash([]);
      ctx.font = 'bold 14px Georgia, serif'; ctx.fillStyle = '#a8483c'; ctx.textAlign = 'center'; ctx.fillText('⚔', b.x, b.y - 22);
    }
  }
  _drawLabels(ctx) {
    if (this.zoom < .55) return;
    const civs = this.view.civs;
    for (const c of this.view.cities) {
      const t = this.tiles.get(key(c.q, c.r)); if (!t || !this._visible(t)) continue;
      const p = project(c.q, c.r);
      const stale = c.seenTick !== null && c.seenTick !== undefined;
      const name = `${c.capital ? '★ ' : ''}${cityName(c.id)}${stale ? ` · T${c.seenTick}` : ''}`;
      ctx.globalAlpha = stale ? .75 : 1;
      ctx.font = '500 10px Georgia, "Yu Mincho", serif';
      const w = Math.max(64, ctx.measureText(name).width + 34), y = p.y + 22;
      rounded(ctx, p.x - w / 2, y, w, 19, 5); ctx.fillStyle = c.owner === this.me ? 'rgba(43,74,60,.95)' : 'rgba(248,242,220,.95)'; ctx.fill();
      ctx.strokeStyle = 'rgba(116,119,83,.25)'; ctx.lineWidth = .6; ctx.stroke();
      const col = c.owner === null ? '#9a9a8c' : CIV_COLORS[c.owner];
      rounded(ctx, p.x - w / 2 + 4, y + 4, 11, 11, 3); ctx.fillStyle = col; ctx.fill();
      ctx.font = 'bold 8px system-ui'; ctx.fillStyle = '#fff'; ctx.textAlign = 'center'; ctx.fillText(`${c.pop}`, p.x - w / 2 + 9.5, y + 12.5);
      ctx.font = '500 10px Georgia, "Yu Mincho", serif'; ctx.fillStyle = c.owner === this.me ? '#f4e5b8' : '#425747'; ctx.fillText(name, p.x + 7, y + 13.5);
      // defence bar under the banner
      const frac = c.defenseMax ? clamp(c.defense / c.defenseMax, 0, 1) : 0;
      rounded(ctx, p.x - w / 2 + 3, y + 21, w - 6, 3, 1.5); ctx.fillStyle = 'rgba(80,80,70,.25)'; ctx.fill();
      if (frac > 0) { rounded(ctx, p.x - w / 2 + 3, y + 21, (w - 6) * frac, 3, 1.5); ctx.fillStyle = frac < .35 ? '#c0584a' : '#7f9a6a'; ctx.fill(); }
      if (c.owner !== null && c.owner !== this.me && civs[c.owner]?.relation === 'war') { ctx.font = 'bold 11px Georgia'; ctx.fillStyle = '#a8483c'; ctx.fillText('⚔', p.x + w / 2 + 7, y + 13); }
      ctx.globalAlpha = 1;
    }
    for (const cs of this.view.cityStates) {
      if (cs.capturedBy !== null) continue;
      const t = this.tiles.get(key(cs.q, cs.r)); if (!t || !this._visible(t)) continue;
      const p = project(cs.q, cs.r), text = `都市国家 ${cs.id + 1}`;
      ctx.font = '500 9px system-ui, sans-serif'; const w = ctx.measureText(text).width + 14;
      rounded(ctx, p.x - w / 2, p.y + 17, w, 16, 5); ctx.fillStyle = 'rgba(244,240,224,.94)'; ctx.fill();
      ctx.strokeStyle = cs.suzerain !== null ? CIV_COLORS[cs.suzerain] : 'rgba(116,119,83,.3)'; ctx.lineWidth = 1; ctx.stroke();
      ctx.fillStyle = '#4c5a50'; ctx.textAlign = 'center'; ctx.fillText(text, p.x, p.y + 28.5);
    }
  }
  _drawCompass(ctx) {
    if (this.width < 900 || this.height < 640) return;
    const x = 47, y = this.height - 230;
    ctx.save(); ctx.translate(x, y); ctx.globalAlpha = .45;
    ctx.strokeStyle = '#576f60'; ctx.lineWidth = .8; ctx.beginPath(); ctx.arc(0, 0, 16, 0, Math.PI * 2); ctx.stroke();
    polygon(ctx, [[0, -21], [-4, 3], [0, -2], [4, 3]], '#476552'); polygon(ctx, [[0, 19], [-3, -1], [0, 3], [3, -1]], '#87977a');
    ctx.font = '500 8px Georgia, serif'; ctx.fillStyle = '#476552'; ctx.textAlign = 'center'; ctx.fillText('N', 0, -27);
    ctx.restore();
  }
  destroy() { this.destroyed = true; cancelAnimationFrame(this.frame); this.resizeObserver.disconnect(); this.listeners.forEach(f => f()); }
}

/** Minimap: ownership over terrain, plus the camera footprint. */
export function drawMinimap(canvas, map, view, footprint) {
  const ctx = canvas.getContext('2d'); const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const W = canvas.clientWidth, H = canvas.clientHeight; canvas.width = W * dpr; canvas.height = H * dpr; ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = '#e7e4d4'; ctx.fillRect(0, 0, W, H);
  const pts = map.tiles.map(([q, r]) => project(q, r));
  const xs = pts.map(p => p.x), ys = pts.map(p => p.y);
  const minX = Math.min(...xs) - RADIUS, maxX = Math.max(...xs) + RADIUS, minY = Math.min(...ys) - RADIUS, maxY = Math.max(...ys) + RADIUS;
  const s = Math.min(W / (maxX - minX), H / (maxY - minY)), ox = (W - (maxX - minX) * s) / 2 - minX * s, oy = (H - (maxY - minY) * s) / 2 - minY * s;
  const tcol = { Grassland: '#b8c294', Plains: '#cbc28f', Forest: '#86a079', Hills: '#b3a784', Mountain: '#9c978b', Water: '#6f9fa0' };
  map.tiles.forEach(([q, r, terrain], i) => {
    const p = project(q, r); const o = view?.owners?.[i]; const fog = view?.fog?.[i] ?? '2';
    ctx.fillStyle = fog === '0' ? '#ddd5bd' : o && o !== '.' ? CIV_COLORS[parseInt(o, 36)] : tcol[terrain] || '#bbb';
    ctx.globalAlpha = fog === '0' ? 1 : (o && o !== '.' ? .85 : 1) * (fog === '1' ? .55 : 1);
    ctx.beginPath(); ctx.arc(p.x * s + ox, p.y * s + oy, Math.max(1.6, RADIUS * s * .82), 0, Math.PI * 2); ctx.fill();
  });
  ctx.globalAlpha = 1;
  for (const c of view?.cities || []) { const p = project(c.q, c.r); ctx.fillStyle = '#fff'; ctx.fillRect(p.x * s + ox - 1.5, p.y * s + oy - 1.5, 3, 3); }
  if (footprint) { ctx.strokeStyle = '#213f34'; ctx.lineWidth = 1.2; ctx.strokeRect(footprint.x0 * s + ox, footprint.y0 * s + oy, (footprint.x1 - footprint.x0) * s, (footprint.y1 - footprint.y0) * s); }
  return { toWorld: (mx, my) => ({ x: (mx - ox) / s, y: (my - oy) / s }) };
}
