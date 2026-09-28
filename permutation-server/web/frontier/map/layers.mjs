// Painters for the Frontier map (web design §7.3): province shapes on the
// province lattice, rings and wedges, owners from the overview, fog, sites
// and tiles. Pure geometry is exported for the map and its tests; painters
// take a 2D context and draw only. Hex primitives come from v9's map.mjs
// (additive exports), so both maps share one projection and palette.
// Accessibility (web design §10, W5-E): colour is never the only signal —
// each faction also has a sigil (a shape) drawn on its provinces and sites;
// cell strokes keep 3:1 against the land; fog veils never cover text.
import { COLORS, FLATTEN, RADIUS, SQRT3, hexPoints, polygon, project, shade } from '../../map.mjs';
import { provinceCentre, tileHex, PROVINCE_TILES } from '../fgeo.mjs';
import { FACTION_COLORS } from '../fi18n.mjs';
import { majorityOwner } from '../herald.mjs';

export { RADIUS, FLATTEN };
/** World pixels of a tile hex (v9's projection). */
export const tilePixel = (q, r) => project(q, r);
/** World pixels of a province's centre tile. */
export const provincePixel = (p, q) => { const c = provinceCentre(p, q); return project(c.q, c.r); };

// Neighbouring province centres are 9 tiles apart along the lattice basis
// (2n+1, −n) = (9, −4); a province is drawn as the lattice's hexagonal cell
// (circumradius = spacing / √3), then flattened like the tiles.
const B = (() => { const x = SQRT3 * RADIUS * (9 + -4 / 2), y = RADIUS * 1.5 * -4; return { angle: Math.atan2(y, x), spacing: Math.hypot(x, y) }; })();
export const PROVINCE_CIRCUMRADIUS = B.spacing / Math.sqrt(3);

/** The six corners of province (p, q) in world pixels. */
export function provinceCorners(p, q, inset = 0) {
  const c = provinceCentre(p, q);
  const x0 = SQRT3 * RADIUS * (c.q + c.r / 2), y0 = RADIUS * 1.5 * c.r;
  const r = PROVINCE_CIRCUMRADIUS - inset;
  return Array.from({ length: 6 }, (_, k) => {
    const a = B.angle + Math.PI / 6 + (k * Math.PI) / 3;
    return [x0 + Math.cos(a) * r, (y0 + Math.sin(a) * r) * FLATTEN];
  });
}

/** Fog levels (presentation only: every account is public; §7.3). */
export const FOG = Object.freeze({ unopened: 1, distant: 0.55, known: 0.18, sight: 0, clear: 0 });

/**
 * The fog of one province: `unopened` (its ring is not open), `sight`
 * (within 2 provinces of the viewer's holdings and hosts), `known`
 * (holdings, hosts, past arrivals, explored hexes), else `distant`; the
 * "show everything" switch clears it (`clear`).
 */
export function fogLevel({ ringOpen, showAll = false, known = false, sightDistance = Infinity }) {
  if (!ringOpen) return 'unopened';
  if (showAll) return 'clear';
  if (sightDistance <= 2) return 'sight';
  return known ? 'known' : 'distant';
}

const NEUTRAL_FILL = '#8a8f86';
const EMPTY_FILL = '#cfc9b4';
/** The fill of a province at world LOD: its majority owner's colour, else neutral land. */
export function provinceFill(rec) {
  if (!rec) return EMPTY_FILL;
  const f = majorityOwner(rec);
  return f === null ? (rec.sites.some(s => s === 2) ? NEUTRAL_FILL : EMPTY_FILL) : FACTION_COLORS[f];
}

// ------------------------------------------------------------------ painters
/** The shape of each faction's sigil (colour is never the only signal, web design §10): ids 0–5, 6 neutral. */
export const SIGILS = Object.freeze(['circle', 'triangle', 'square', 'diamond', 'cross', 'hexagon', 'ring']);

/** A faction's sigil of radius r (world px) at (x, y): its shape filled with its colour, outlined in ink. */
export function paintSigil(ctx, { x, y, r, faction, scale = 1 }) {
  const shape = SIGILS[faction] ?? 'ring';
  const pts = n => Array.from({ length: n }, (_, k) => { const a = -Math.PI / 2 + (k * 2 * Math.PI) / n; return [x + Math.cos(a) * r, y + Math.sin(a) * r]; });
  ctx.beginPath();
  if (shape === 'circle' || shape === 'ring') ctx.arc(x, y, r * 0.9, 0, Math.PI * 2);
  else if (shape === 'cross') {
    const w = r * 0.38;
    for (const [px, py] of [[-w, -r], [w, -r], [w, -w], [r, -w], [r, w], [w, w], [w, r], [-w, r], [-w, w], [-r, w], [-r, -w], [-w, -w]]) ctx.lineTo(x + px, y + py);
    ctx.closePath();
  } else {
    const corners = shape === 'triangle' ? pts(3) : shape === 'square' ? pts(4).map(([px, py]) => [x + ((px - x) - (py - y)) * 0.62, y + ((py - y) + (px - x)) * 0.62]) : shape === 'diamond' ? pts(4) : pts(6);
    corners.forEach(([px, py], k) => (k ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
    ctx.closePath();
  }
  ctx.fillStyle = shape === 'ring' ? 'rgba(0,0,0,0)' : FACTION_COLORS[faction] ?? NEUTRAL_FILL;
  ctx.fill();
  ctx.strokeStyle = '#1b2e28'; ctx.lineWidth = (shape === 'ring' ? 2.5 : 1.5) / scale; ctx.stroke();
}

/** A province at world or province LOD: its cell, owner fill and sigil, fog, and a clash marker. */
export function paintProvince(ctx, { p, q, rec, fog = 'distant', selected = false, scale = 1 }) {
  const pts = provinceCorners(p, q, 2 / scale);
  // Strokes: ink at 55% on land keeps ≥ 3:1 (web design §10); the selection is ink, thicker.
  polygon(ctx, pts, fog === 'unopened' ? '#e9e5d8' : provinceFill(rec), selected ? '#1b2e28' : 'rgba(27,46,40,.55)', selected ? 4 / scale : 1 / scale);
  const a = FOG[fog] ?? 0;
  if (a > 0 && fog !== 'unopened') polygon(ctx, pts, `rgba(233,229,216,${a})`, null);
  if (fog === 'unopened') return;
  const c = provincePixel(p, q);
  const owner = rec ? majorityOwner(rec) : null;
  // The sigil stays legible at every zoom: about 7 screen px, never larger than a fifth of the cell.
  if (owner !== null) paintSigil(ctx, { x: c.x, y: c.y, r: Math.min(7 / scale, PROVINCE_CIRCUMRADIUS / 5), faction: owner, scale });
  if (rec?.clash) {
    ctx.beginPath();
    ctx.arc(c.x, c.y, 14 / scale, 0, Math.PI * 2);
    ctx.strokeStyle = '#b3402f'; ctx.lineWidth = 3 / scale; ctx.stroke();
  }
}

/** Over a province's tiles at tile LOD: its fog veil (`known`: the tiles show through, muted) and the selection outline. */
export function paintVeil(ctx, { p, q, fog, scale = 1, selected = false }) {
  const pts = provinceCorners(p, q, 2 / scale);
  const a = FOG[fog] ?? 0;
  if (a > 0 && fog !== 'unopened') polygon(ctx, pts, `rgba(233,229,216,${a})`, null);
  polygon(ctx, pts, null, selected ? '#1b2e28' : 'rgba(27,46,40,.55)', (selected ? 4 : 1.5) / scale);
}

/**
 * Tiles of a province at tile LOD, from the WASM kernel's generated terrain
 * (TERRAIN names by index); its sites as paper discs, a held site with its
 * owner's sigil (the overview record `rec`: owners and site states by site).
 */
export function paintTiles(ctx, { p, q, terrain, sites = [], names, rec = null, selectedTile = null }) {
  for (let i = 0; i < PROVINCE_TILES; i++) {
    const h = tileHex(p, q, i);
    const { x, y } = project(h.q, h.r);
    const pal = COLORS[names[terrain[i]]] ?? COLORS.Plains;
    polygon(ctx, hexPoints(x, y, 1), pal[0], shade(pal[1], -0.1), 1);
  }
  sites.forEach((s, j) => {
    const h = tileHex(p, q, s);
    const { x, y } = project(h.q, h.r);
    ctx.beginPath(); ctx.arc(x, y, RADIUS * 0.34, 0, Math.PI * 2);
    ctx.fillStyle = '#f4efe0'; ctx.fill(); ctx.strokeStyle = '#3a3a33'; ctx.lineWidth = 2; ctx.stroke();
    if (rec && rec.sites?.[j] === 1 && rec.owners?.[j] < 6) paintSigil(ctx, { x, y, r: RADIUS * 0.22, faction: rec.owners[j] });
  });
  if (selectedTile !== null && selectedTile !== undefined && selectedTile >= 0 && selectedTile < PROVINCE_TILES) {
    const h = tileHex(p, q, selectedTile);
    const { x, y } = project(h.q, h.r);
    polygon(ctx, hexPoints(x, y, 3), null, '#1b2e28', 4);
  }
}

/** A ring outline label at world LOD (the Concord is ring 0). */
export function paintRingLabel(ctx, { d, x, y, scale, text }) {
  ctx.font = `${12 / scale}px system-ui, sans-serif`;
  ctx.fillStyle = 'rgba(33,63,52,.7)';
  ctx.textAlign = 'center';
  ctx.fillText(text ?? String(d), x, y);
}
