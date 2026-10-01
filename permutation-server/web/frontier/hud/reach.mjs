// The move preview (after the Eternum benchmark: a selected army lights the hexes it can
// act on, coloured by the action — move green, attack orange — and they roll out from the
// army). While a march is composed and has no destination yet, the passable tiles around
// the host within its stamina light up: green to move, orange where a camp, an enemy
// holding or an enemy host stands, gold for the viewer's own holdings. A guide only: the
// route the planner finds for the tile chosen is what the march takes.
import { DIRECTIONS, locate, tileHex } from '../fgeo.mjs';
import { project, hexPoints, RADIUS } from '../../map.mjs';
import { RULES } from '../fmarch.mjs';

/** Steps a host can walk with this stamina (the march costs base + per hex), capped for the preview. */
export const reachSteps = (stamina, cap = 6) => Math.max(0, Math.min(cap, Math.floor((Number(stamina ?? 0) - RULES.MARCH_STAMINA_BASE) / RULES.MARCH_STAMINA_PER_HEX)));

const passable = (prov, idx) => (BigInt(prov.passableMask ?? 0n) >> BigInt(idx)) & 1n;

/**
 * The tiles a host could reach: `[{p, q, tile, d, kind: 'move'|'attack'|'home'}]` by a
 * breadth-first walk over passable tiles of loaded provinces (`provinceOf(p, q)` →
 * Province or null: an unloaded province stops the walk).
 */
export function reachTiles({ start, steps, provinceOf, faction, maxTiles = 900 }) {
  const h0 = tileHex(start.p, start.q, start.tile);
  if (!h0 || steps <= 0) return [];
  const seen = new Set([`${h0.q},${h0.r}`]);
  const out = [];
  let front = [h0];
  for (let d = 1; d <= steps && front.length && out.length < maxTiles; d++) {
    const next = [];
    for (const h of front) for (const [dq, dr] of DIRECTIONS) {
      const q = h.q + dq, r = h.r + dr, k = `${q},${r}`;
      if (seen.has(k)) continue;
      seen.add(k);
      const at = locate(q, r), prov = provinceOf(at.p, at.q);
      if (!prov || !passable(prov, at.idx)) continue;
      out.push({ p: at.p, q: at.q, tile: at.idx, d, kind: kindAt(prov, at.idx, faction) });
      next.push({ q, r });
    }
    front = next;
  }
  return out;
}

function kindAt(prov, idx, faction) {
  if (prov.camp?.state === 1 && prov.camp.tile === idx) return 'attack';
  const j = Array.from(prov.sites ?? []).indexOf(idx);
  const m = j >= 0 ? prov.siteMirror?.[j] : null;
  if (m && m.state === 1) return m.faction === faction ? 'home' : 'attack';
  if ((prov.entries ?? []).some(e => e.state === 1 && e.tile === idx && e.faction !== faction)) return 'attack';
  return 'move';
}

const COLOUR = { move: [79, 207, 107], attack: [255, 107, 61], home: [243, 213, 138] };

/** Draw the preview; `t0` is when it opened (the hexes roll out from the host, 30 ms a step). */
export function paintReach(ctx, tiles, zoom, t, t0 = 0) {
  const k = 1 / zoom, pulse = 0.5 + 0.5 * Math.sin(t * 2.2);
  ctx.save();
  for (const x of tiles) {
    const show = Math.max(0, Math.min(1, (t - t0 - x.d * 0.03) / 0.2));
    if (show <= 0) continue;
    const h = tileHex(x.p, x.q, x.tile), c = project(h.q, h.r);
    const [r, g, b] = COLOUR[x.kind];
    const pts = hexPoints(c.x, c.y, RADIUS * (x.kind === 'move' ? 0.12 : 0.06));
    ctx.beginPath(); pts.forEach(([px, py], i) => (i ? ctx.lineTo(px, py) : ctx.moveTo(px, py))); ctx.closePath();
    const near = Math.max(0.35, 1 - (x.d - 1) * 0.12);   // the move area fades with distance; targets stay bright
    ctx.globalAlpha = show * (x.kind === 'move' ? 0.18 * near : 0.26 + pulse * 0.12); ctx.fillStyle = `rgb(${r},${g},${b})`; ctx.fill();
    ctx.globalAlpha = show * (x.kind === 'move' ? 0.55 * near : 0.95); ctx.strokeStyle = `rgb(${r},${g},${b})`; ctx.lineWidth = (x.kind === 'move' ? 1.4 : 2.4) * k; ctx.stroke();
  }
  ctx.restore();
}
