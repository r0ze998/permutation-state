// Painters for the Frontier map (web design §7.3): province shapes on the
// province lattice, rings and wedges, owners from the overview, fog, sites
// and tiles. Pure geometry is exported for the map and its tests; painters
// take a 2D context and draw only. Hex primitives come from v9's map.mjs
// (additive exports), so both maps share one projection and palette.
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
/** A province at world or province LOD: its cell, owner fill, fog, and a clash marker. */
export function paintProvince(ctx, { p, q, rec, fog = 'distant', selected = false, scale = 1 }) {
  const pts = provinceCorners(p, q, 2 / scale);
  polygon(ctx, pts, fog === 'unopened' ? '#e9e5d8' : provinceFill(rec), selected ? '#1b2e28' : 'rgba(40,52,46,.35)', selected ? 3 / scale : 1 / scale);
  const a = FOG[fog] ?? 0;
  if (a > 0 && fog !== 'unopened') polygon(ctx, pts, `rgba(233,229,216,${a})`, null);
  if (rec?.clash && fog !== 'unopened') {
    const c = provincePixel(p, q);
    ctx.beginPath();
    ctx.arc(c.x, c.y, 14 / scale, 0, Math.PI * 2);
    ctx.strokeStyle = '#b3402f'; ctx.lineWidth = 3 / scale; ctx.stroke();
  }
}

/** Tiles of a province at tile LOD, from the WASM kernel's generated terrain (TERRAIN names by index). */
export function paintTiles(ctx, { p, q, terrain, sites = [], names }) {
  for (let i = 0; i < PROVINCE_TILES; i++) {
    const h = tileHex(p, q, i);
    const { x, y } = project(h.q, h.r);
    const pal = COLORS[names[terrain[i]]] ?? COLORS.Plains;
    polygon(ctx, hexPoints(x, y, 1), pal[0], shade(pal[1], -0.1), 1);
  }
  for (const s of sites) {
    const h = tileHex(p, q, s);
    const { x, y } = project(h.q, h.r);
    ctx.beginPath(); ctx.arc(x, y, RADIUS * 0.28, 0, Math.PI * 2);
    ctx.fillStyle = '#f4efe0'; ctx.fill(); ctx.strokeStyle = '#3a3a33'; ctx.lineWidth = 2; ctx.stroke();
  }
}

/** A ring outline label at world LOD (the Concord is ring 0). */
export function paintRingLabel(ctx, { d, x, y, scale, text }) {
  ctx.font = `${12 / scale}px system-ui, sans-serif`;
  ctx.fillStyle = 'rgba(33,63,52,.7)';
  ctx.textAlign = 'center';
  ctx.fillText(text ?? String(d), x, y);
}
