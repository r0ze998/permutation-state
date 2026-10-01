// The minimap and the lenses (UI plan C3, C4; Civ's minimap with its lens
// buttons): the whole open world in a corner of the map — each province a
// hex in the colour of the realm that holds most of it (land colour when
// nobody does), the Concord at the centre, clouds beyond the rim, clashes
// of this bell as red dots, the viewer's holdings ringed, and the part of
// the world the map shows as a frame. A press moves the map there.
//
// Lenses change what the main map stresses (one at a time, keys 1–4):
//   realm    the factions' lands (the default)
//   war      hosts and clashes: realm washes faint, host counts per province
//   land     the land itself: no washes
//   settle   free sites and crowding: where a new holding could go
// Drawn from the overviews only (no province loads): cheap for spectators.
import { L } from '../../lang.mjs';
import { provincePixel, PROVINCE_CIRCUMRADIUS } from '../map/layers.mjs';
import { ringProvinces } from '../fgeo.mjs';
import { FACTION_FILL, FACTION_DARK } from '../people/avatar.mjs';

export const LENSES = Object.freeze(['realm', 'war', 'land', 'settle']);
export const LENS_TEXT = { realm: () => L`領土`, war: () => L`軍事`, land: () => L`地形`, settle: () => L`入植` };
export const LENS_GLYPH = { realm: '⚑', war: '⚔', land: '⛰', settle: '⌂' };
export const MINIMAP_PX = 188;

const LAND = '#6f8f55', CLOUD = '#e8ecef';

/** The faction that holds most of a province's sites in an overview record, or null. */
export function majority(rec) {
  const n = Array(6).fill(0);
  rec.owners.forEach((f, j) => { if (rec.sites[j] === 1 && f < 6) n[f]++; });
  const m = Math.max(...n);
  return m > 0 ? n.indexOf(m) : null;
}

/** The minimap's world-to-pixel transform for `rings` open rings in a `px` square. */
export function frameOf(rings, px = MINIMAP_PX) {
  const d = Math.max(1, rings);
  const far = provincePixel(d, 0);
  const span = (Math.hypot(far.x, far.y) + PROVINCE_CIRCUMRADIUS * 1.2) * 2;
  const k = px / span;
  return { k, toPx: (x, y) => ({ x: px / 2 + x * k, y: px / 2 + y * k }), toWorld: (sx, sy) => ({ x: (sx - px / 2) / k, y: (sy - px / 2) / k }) };
}

/**
 * Paint the minimap into a 2D context of `px` × `px` (device pixels via
 * `dpr`): `{recs: Map "P,Q" → overview record, rings, own: [{p, q}], view,
 * size}` (`view` and `size` the main map's, for the frame).
 */
export function paintMinimap(ctx, { recs, rings, own = [], view = null, size = null, px = MINIMAP_PX, dpr = 1, lens = 'realm' }) {
  const fr = frameOf(rings + 1, px);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, px, px);
  const g = ctx.createRadialGradient(px / 2, px / 2, px * 0.1, px / 2, px / 2, px * 0.72);
  g.addColorStop(0, '#24403b'); g.addColorStop(1, '#0f1f1c');
  ctx.fillStyle = g; ctx.beginPath(); ctx.arc(px / 2, px / 2, px / 2 - 1, 0, Math.PI * 2); ctx.fill();
  const r = PROVINCE_CIRCUMRADIUS * fr.k * 1.02;
  const hex = (cx, cy, rr) => { ctx.beginPath(); for (let i = 0; i < 6; i++) { const a = Math.PI / 6 + (i * Math.PI) / 3; const x = cx + Math.cos(a) * rr, y = cy + Math.sin(a) * rr; i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); } ctx.closePath(); };
  const all = [{ p: 0, q: 0, d: 0 }];
  for (let d = 1; d <= rings; d++) for (const pr of ringProvinces(d)) all.push({ ...pr, d });
  for (const pr of all) {
    const c = (pp => fr.toPx(pp.x, pp.y))(provincePixel(pr.p, pr.q));
    const rec = recs.get(`${pr.p},${pr.q}`);
    const owner = rec ? majority(rec) : null;
    hex(c.x, c.y, r);
    ctx.fillStyle = !rec ? CLOUD : pr.d === 0 ? '#e6dcc2' : owner !== null && lens !== 'land' ? FACTION_FILL[owner] : LAND;
    ctx.globalAlpha = !rec ? 0.55 : lens === 'war' && owner !== null ? 0.55 : 1;
    ctx.fill();
    ctx.globalAlpha = 1;
    if (lens === 'settle' && rec) {
      const free = rec.sites.filter(s => s === 0).length;
      if (free) { ctx.fillStyle = '#f3d58a'; ctx.beginPath(); ctx.arc(c.x, c.y, Math.min(r * 0.6, 1 + free * 0.5), 0, Math.PI * 2); ctx.fill(); }
    }
    if (rec?.clash && (lens === 'war' || lens === 'realm')) { ctx.fillStyle = '#ff5a3c'; ctx.beginPath(); ctx.arc(c.x, c.y, Math.max(1.6, r * 0.32), 0, Math.PI * 2); ctx.fill(); }
  }
  for (const o of own) {
    const c = (pp => fr.toPx(pp.x, pp.y))(provincePixel(o.p, o.q));
    ctx.strokeStyle = '#fffaf0'; ctx.lineWidth = 2; hex(c.x, c.y, r * 1.05); ctx.stroke();
  }
  // the part of the world the map shows
  if (view && size) {
    const a = fr.toPx(view.x - size.width / 2 / view.zoom, view.y - size.height / 2 / view.zoom);
    const b = fr.toPx(view.x + size.width / 2 / view.zoom, view.y + size.height / 2 / view.zoom);
    ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = 1.6;
    ctx.strokeRect(Math.max(1, a.x), Math.max(1, a.y), Math.min(px - 2, b.x) - Math.max(1, a.x), Math.min(px - 2, b.y) - Math.max(1, a.y));
  }
  ctx.strokeStyle = 'rgba(217,180,74,.7)'; ctx.lineWidth = 1.2; ctx.beginPath(); ctx.arc(px / 2, px / 2, px / 2 - 1, 0, Math.PI * 2); ctx.stroke();
}

/** Faction colours for the dark border inks (used by the main map's war lens). */
export { FACTION_DARK };
