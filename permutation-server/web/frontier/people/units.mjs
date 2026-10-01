// One token per host group (docs/frontier/ui-shell/UNITS-REDESIGN.md, after the Eternum
// benchmark): the hosts of one faction on one tile are drawn as a single soldier of
// their main unit type, standing on a disc in the faction's colour, with a pill label
// above (unit, troops, ×n) and a status mark (marching, exploring, fighting, resting,
// mustering, set out). A unit reads by its silhouette — spear and round shield, bow,
// horse, long pike, crossbow, armoured rider with a lance, hooded scout, a cart — in two
// colour blocks: the faction's cloth and steel. Pure canvas drawing; the map decides
// where tokens stand and what they are doing.
import { FACTION_FILL, FACTION_DARK, shade } from './avatar.mjs';
import { RADIUS, FLATTEN, project } from '../../map.mjs';
import { tileHex, DIRECTIONS } from '../fgeo.mjs';

export const UNIT_KINDS = Object.freeze(['spearman', 'archer', 'horseman', 'pikeman', 'crossbowman', 'knight', 'scout', 'settler']);
const INK = '#1b1712', STEEL = '#aeb4b9', STEEL_D = '#6c7378', WOOD = '#7a5634', LEATHER = '#6b4a2e', HORSE = '#7b5636', HORSE_D = '#4e3522', SKIN = '#e2b58e';

const ln = (ctx, w, c) => { ctx.lineWidth = w; ctx.strokeStyle = c; };

/** The disc a token stands on: shadow, the faction's colour, a dark rim and a light lip; `own` adds a gold ring. */
export function baseDisc(ctx, x, y, s, f, { own = false, alpha = 1, pulse = 0 } = {}) {
  const fill = FACTION_FILL[f] ?? '#8a8a80', dark = FACTION_DARK[f] ?? '#3a3a34';
  const rx = s * 0.46, ry = s * 0.16;
  ctx.save();
  ctx.globalAlpha = alpha * 0.35; ctx.fillStyle = '#0d0b08';
  ctx.beginPath(); ctx.ellipse(x + s * 0.04, y + s * 0.04, rx * 1.08, ry * 1.15, 0, 0, Math.PI * 2); ctx.fill();
  ctx.globalAlpha = alpha;
  ctx.fillStyle = dark; ctx.beginPath(); ctx.ellipse(x, y + s * 0.025, rx, ry, 0, 0, Math.PI * 2); ctx.fill();
  ctx.fillStyle = fill; ctx.beginPath(); ctx.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2); ctx.fill();
  ctx.fillStyle = 'rgba(255,255,255,.22)'; ctx.beginPath(); ctx.ellipse(x, y - ry * 0.25, rx * 0.8, ry * 0.5, 0, Math.PI, Math.PI * 2); ctx.fill();
  if (own) {
    ln(ctx, s * 0.035, `rgba(243,213,138,${0.85 + pulse * 0.15})`);
    ctx.beginPath(); ctx.ellipse(x, y, rx * 1.12, ry * 1.18, 0, 0, Math.PI * 2); ctx.stroke();
  }
  ctx.restore();
}

/** Legs that walk (`step` 0..1) or stand. */
function legs(ctx, x, hip, foot, s, cloth, step, walking) {
  const sw = walking ? Math.sin(step * Math.PI * 2) * s * 0.09 : 0;
  ln(ctx, s * 0.1, INK); ctx.lineCap = 'round';
  for (const d of [-1, 1]) { ctx.beginPath(); ctx.moveTo(x + d * s * 0.05, hip); ctx.lineTo(x + d * s * 0.06 + d * sw, foot); ctx.stroke(); }
  ln(ctx, s * 0.07, shade(cloth, -0.45));
  for (const d of [-1, 1]) { ctx.beginPath(); ctx.moveTo(x + d * s * 0.05, hip); ctx.lineTo(x + d * s * 0.06 + d * sw, foot); ctx.stroke(); }
}

/** A torso, head and helmet; returns the shoulder and head points. */
function body(ctx, x, top, s, cloth, trim, { hood = false, helm = true, face = 1 } = {}) {
  const sh = top + s * 0.16, hip = top + s * 0.46;
  ctx.fillStyle = cloth; ln(ctx, s * 0.03, INK);
  ctx.beginPath(); ctx.moveTo(x - s * 0.13, hip); ctx.quadraticCurveTo(x - s * 0.16, sh + s * 0.04, x - s * 0.08, sh); ctx.lineTo(x + s * 0.08, sh);
  ctx.quadraticCurveTo(x + s * 0.16, sh + s * 0.04, x + s * 0.13, hip); ctx.closePath(); ctx.fill(); ctx.stroke();
  ctx.fillStyle = trim; ctx.fillRect(x - s * 0.13, hip - s * 0.07, s * 0.26, s * 0.045);
  const hy = top + s * 0.07;
  ctx.fillStyle = SKIN; ctx.beginPath(); ctx.arc(x + face * s * 0.01, hy, s * 0.085, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  if (hood) { ctx.fillStyle = shade(cloth, -0.3); ctx.beginPath(); ctx.arc(x, hy - s * 0.005, s * 0.105, Math.PI * 0.95, Math.PI * 2.05); ctx.lineTo(x + face * s * 0.05, hy + s * 0.05); ctx.closePath(); ctx.fill(); ctx.stroke(); }
  else if (helm) { ctx.fillStyle = STEEL; ctx.beginPath(); ctx.arc(x, hy - s * 0.02, s * 0.095, Math.PI, Math.PI * 2); ctx.fill(); ctx.stroke(); ln(ctx, s * 0.025, STEEL_D); ctx.beginPath(); ctx.moveTo(x - s * 0.12, hy - s * 0.015); ctx.lineTo(x + s * 0.12, hy - s * 0.015); ctx.stroke(); }
  return { sh, hip, hy };
}

/** A horse in profile (legs trot with `step`), `barding` a cloth over its back. Returns the saddle point. */
function horse(ctx, x, y, s, face, step, walking, barding = null) {
  const sw = walking ? Math.sin(step * Math.PI * 2) : 0;
  const by = y - s * 0.32, f = face;
  const P = (dx, dy) => [x + f * dx * s, by + dy * s];
  // legs: four, the near pair darker in front; hooves
  ctx.lineCap = 'round';
  for (const [dx, ph, near] of [[-0.21, 1, 0], [0.17, -1, 0], [-0.15, -1, 1], [0.23, 1, 1]]) {
    const [hx, hy] = P(dx, 0.06), kx = hx + f * ph * sw * s * 0.05, fy = y - s * 0.015;
    ln(ctx, s * 0.065, INK); ctx.beginPath(); ctx.moveTo(hx, hy); ctx.lineTo(kx, hy + (fy - hy) * 0.55); ctx.lineTo(kx - f * ph * sw * s * 0.02, fy); ctx.stroke();
    ln(ctx, s * 0.045, near ? HORSE : HORSE_D); ctx.stroke();
    ctx.fillStyle = INK; ctx.beginPath(); ctx.ellipse(kx - f * ph * sw * s * 0.02, fy, s * 0.03, s * 0.017, 0, 0, Math.PI * 2); ctx.fill();
  }
  // tail
  ln(ctx, s * 0.05, HORSE_D); ctx.beginPath(); ctx.moveTo(...P(-0.29, -0.04)); ctx.quadraticCurveTo(...P(-0.42, 0.02), ...P(-0.38, 0.17)); ctx.stroke();
  // body, chest and rump in one shape
  ctx.fillStyle = HORSE; ln(ctx, s * 0.03, INK);
  ctx.beginPath(); ctx.moveTo(...P(-0.3, -0.03)); ctx.bezierCurveTo(...P(-0.31, -0.13), ...P(-0.12, -0.13), ...P(0.05, -0.11));
  ctx.bezierCurveTo(...P(0.2, -0.11), ...P(0.3, -0.08), ...P(0.31, 0.0)); ctx.bezierCurveTo(...P(0.31, 0.08), ...P(0.2, 0.1), ...P(0.1, 0.09));
  ctx.lineTo(...P(-0.18, 0.09)); ctx.bezierCurveTo(...P(-0.28, 0.09), ...P(-0.32, 0.05), ...P(-0.3, -0.03)); ctx.closePath(); ctx.fill(); ctx.stroke();
  if (barding) { ctx.fillStyle = barding; ctx.beginPath(); ctx.moveTo(...P(-0.2, -0.11)); ctx.lineTo(...P(0.16, -0.11)); ctx.lineTo(...P(0.2, 0.06)); ctx.lineTo(...P(-0.22, 0.06)); ctx.closePath(); ctx.fill(); ctx.stroke(); }
  // neck up and forward, then the head angled down
  ctx.fillStyle = HORSE;
  ctx.beginPath(); ctx.moveTo(...P(0.17, -0.09)); ctx.bezierCurveTo(...P(0.2, -0.2), ...P(0.26, -0.3), ...P(0.31, -0.33));
  ctx.lineTo(...P(0.36, -0.34)); ctx.bezierCurveTo(...P(0.42, -0.3), ...P(0.47, -0.22), ...P(0.49, -0.18));
  ctx.bezierCurveTo(...P(0.5, -0.15), ...P(0.47, -0.13), ...P(0.44, -0.15)); ctx.bezierCurveTo(...P(0.4, -0.18), ...P(0.36, -0.2), ...P(0.33, -0.2));
  ctx.bezierCurveTo(...P(0.31, -0.12), ...P(0.3, -0.04), ...P(0.29, 0.0)); ctx.closePath(); ctx.fill(); ctx.stroke();
  // ear, mane, eye
  ctx.beginPath(); ctx.moveTo(...P(0.32, -0.33)); ctx.lineTo(...P(0.33, -0.4)); ctx.lineTo(...P(0.36, -0.34)); ctx.closePath(); ctx.fill(); ctx.stroke();
  ln(ctx, s * 0.045, HORSE_D); ctx.beginPath(); ctx.moveTo(...P(0.3, -0.32)); ctx.bezierCurveTo(...P(0.24, -0.26), ...P(0.2, -0.18), ...P(0.17, -0.1)); ctx.stroke();
  ctx.fillStyle = INK; ctx.beginPath(); ctx.arc(...P(0.38, -0.27), s * 0.012, 0, Math.PI * 2); ctx.fill();
  return { x: x - f * s * 0.04, y: by - s * 0.1 };
}

/**
 * One soldier of a unit type, feet at (x, y), `s` the token's height (world px).
 * `{faction, face (±1), step (0..1), walking, alpha, lunge (0..1)}`.
 */
export function unitFigure(ctx, x, y, s, kind, { faction = 0, face = 1, step = 0, walking = false, alpha = 1, lunge = 0 } = {}) {
  const cloth = shade(FACTION_FILL[faction] ?? '#8a8a80', 0.05), trim = FACTION_DARK[faction] ?? '#3a3a34';
  const bob = walking ? Math.abs(Math.sin(step * Math.PI * 2)) * s * 0.03 : Math.sin(step * Math.PI * 2) * s * 0.006;
  const lx = x + face * lunge * s * 0.12;
  ctx.save();
  ctx.globalAlpha = alpha; ctx.lineJoin = 'round'; ctx.lineCap = 'round';
  if (kind === 'horseman' || kind === 'knight') {
    const saddle = horse(ctx, lx, y - bob, s, face, step, walking, kind === 'knight' ? shade(FACTION_FILL[faction] ?? '#8a8a80', -0.1) : null);
    if (kind === 'knight') { ctx.fillStyle = trim; ctx.fillRect(lx - s * 0.22, y - s * 0.34 - bob, s * 0.44, s * 0.05); }
    const top = saddle.y - s * 0.42;
    const p = body(ctx, saddle.x, top, s * 0.92, kind === 'knight' ? STEEL : cloth, trim, { face });
    // the rider's leg down the horse's flank
    ln(ctx, s * 0.075, INK); ctx.beginPath(); ctx.moveTo(saddle.x, p.hip - s * 0.02); ctx.lineTo(saddle.x + face * s * 0.05, saddle.y + s * 0.13); ctx.stroke();
    ln(ctx, s * 0.05, kind === 'knight' ? STEEL_D : shade(cloth, -0.4)); ctx.stroke();
    // the weapon: a lance with a pennant (knight), a sabre (horseman)
    if (kind === 'knight') {
      ln(ctx, s * 0.035, WOOD); ctx.beginPath(); ctx.moveTo(saddle.x - face * s * 0.12, p.sh + s * 0.2); ctx.lineTo(saddle.x + face * s * 0.55, p.sh - s * 0.12); ctx.stroke();
      ctx.fillStyle = FACTION_FILL[faction] ?? '#8a8a80'; ctx.beginPath(); ctx.moveTo(saddle.x + face * s * 0.48, p.sh - s * 0.08); ctx.lineTo(saddle.x + face * s * 0.34, p.sh - s * 0.12); ctx.lineTo(saddle.x + face * s * 0.36, p.sh - s * 0.02); ctx.closePath(); ctx.fill();
    } else {
      ln(ctx, s * 0.03, STEEL); ctx.beginPath(); ctx.moveTo(saddle.x + face * s * 0.08, p.sh + s * 0.1); ctx.quadraticCurveTo(saddle.x + face * s * 0.3, p.sh - s * 0.05, saddle.x + face * s * 0.34, p.sh - s * 0.22); ctx.stroke();
    }
  } else if (kind === 'settler') {
    // a hand cart with sacks, pushed by a settler
    const cx = lx + face * s * 0.16, cy = y - s * 0.16 - bob;
    ctx.fillStyle = WOOD; ln(ctx, s * 0.03, INK); ctx.fillRect(cx - s * 0.18, cy - s * 0.1, s * 0.36, s * 0.1); ctx.strokeRect(cx - s * 0.18, cy - s * 0.1, s * 0.36, s * 0.1);
    ctx.fillStyle = '#d8c39a'; for (const dx of [-0.1, 0.04]) { ctx.beginPath(); ctx.ellipse(cx + dx * s, cy - s * 0.16, s * 0.08, s * 0.07, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke(); }
    ctx.fillStyle = '#3b2b1e'; ctx.beginPath(); ctx.arc(cx, cy + s * 0.04, s * 0.07, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    const px = lx - face * s * 0.14, top = y - s * 0.78 - bob;
    legs(ctx, px, top + s * 0.46, y - s * 0.02, s, cloth, step, walking);
    const p = body(ctx, px, top, s, cloth, trim, { helm: false, face });
    ln(ctx, s * 0.05, cloth); ctx.beginPath(); ctx.moveTo(px, p.sh + s * 0.06); ctx.lineTo(cx - face * s * 0.18, cy - s * 0.06); ctx.stroke();
  } else {
    const top = y - s * 0.8 - bob;
    // behind the body: a quiver (archer, crossbowman), the shield arm
    if (kind === 'archer' || kind === 'crossbowman') { ctx.fillStyle = LEATHER; ln(ctx, s * 0.025, INK); ctx.save(); ctx.translate(lx - face * s * 0.09, top + s * 0.3); ctx.rotate(-face * 0.35); ctx.fillRect(-s * 0.04, -s * 0.14, s * 0.08, s * 0.24); ctx.strokeRect(-s * 0.04, -s * 0.14, s * 0.08, s * 0.24); ctx.restore(); }
    legs(ctx, lx, top + s * 0.46, y - s * 0.02, s, cloth, step, walking);
    const p = body(ctx, lx, top, s, cloth, trim, { hood: kind === 'scout' || kind === 'archer', helm: kind !== 'scout', face });
    const hand = { x: lx + face * s * 0.16, y: p.sh + s * 0.16 };
    ln(ctx, s * 0.065, INK); ctx.beginPath(); ctx.moveTo(lx + face * s * 0.07, p.sh + s * 0.03); ctx.lineTo(hand.x, hand.y); ctx.stroke();
    ln(ctx, s * 0.045, cloth); ctx.stroke();
    if (kind === 'spearman') {
      ln(ctx, s * 0.03, WOOD); ctx.beginPath(); ctx.moveTo(hand.x - face * s * 0.02, y - s * 0.06); ctx.lineTo(hand.x + face * s * 0.08, top - s * 0.18); ctx.stroke();
      ctx.fillStyle = STEEL; ln(ctx, s * 0.02, INK); ctx.beginPath(); const tx = hand.x + face * s * 0.08, ty = top - s * 0.18; ctx.moveTo(tx, ty - s * 0.08); ctx.lineTo(tx + s * 0.035, ty + s * 0.01); ctx.lineTo(tx - s * 0.035, ty + s * 0.01); ctx.closePath(); ctx.fill(); ctx.stroke();
      // a round shield in front
      ctx.fillStyle = trim; ln(ctx, s * 0.03, INK); ctx.beginPath(); ctx.arc(lx + face * s * 0.05, p.sh + s * 0.19, s * 0.13, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
      ctx.fillStyle = FACTION_FILL[faction] ?? '#8a8a80'; ctx.beginPath(); ctx.arc(lx + face * s * 0.05, p.sh + s * 0.19, s * 0.08, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = STEEL; ctx.beginPath(); ctx.arc(lx + face * s * 0.05, p.sh + s * 0.19, s * 0.03, 0, Math.PI * 2); ctx.fill();
    } else if (kind === 'pikeman') {
      ln(ctx, s * 0.03, WOOD); ctx.beginPath(); ctx.moveTo(lx - face * s * 0.22, p.sh + s * 0.36); ctx.lineTo(lx + face * s * 0.62, p.sh - s * 0.32); ctx.stroke();
      ctx.fillStyle = STEEL; ln(ctx, s * 0.02, INK); ctx.save(); ctx.translate(lx + face * s * 0.62, p.sh - s * 0.32); ctx.rotate(face * -0.68 + (face < 0 ? Math.PI : 0)); ctx.beginPath(); ctx.moveTo(s * 0.1, 0); ctx.lineTo(-s * 0.01, -s * 0.03); ctx.lineTo(-s * 0.01, s * 0.03); ctx.closePath(); ctx.fill(); ctx.stroke(); ctx.restore();
    } else if (kind === 'archer') {
      // a longbow held out in front, the string drawn to the cheek
      const bx = hand.x + face * s * 0.04, by = hand.y - s * 0.1, r = s * 0.26;
      const a0 = face > 0 ? -1.15 : Math.PI - 1.15, a1 = face > 0 ? 1.15 : Math.PI + 1.15;
      ln(ctx, s * 0.035, WOOD); ctx.beginPath(); ctx.arc(bx - face * r * 0.55, by, r, a0, a1); ctx.stroke();
      const ex = bx - face * r * 0.55 + Math.cos(a0) * r, ey0 = by + Math.sin(a0) * r, ey1 = by + Math.sin(a1) * r;
      ln(ctx, s * 0.012, '#efe6d0'); ctx.beginPath(); ctx.moveTo(ex, ey0); ctx.lineTo(lx + face * s * 0.02, p.sh + s * 0.04); ctx.lineTo(ex, ey1); ctx.stroke();
    } else if (kind === 'crossbowman') {
      ctx.fillStyle = WOOD; ln(ctx, s * 0.025, INK); ctx.fillRect(Math.min(hand.x, hand.x + face * s * 0.26), hand.y - s * 0.035, s * 0.26, s * 0.05); ctx.strokeRect(Math.min(hand.x, hand.x + face * s * 0.26), hand.y - s * 0.035, s * 0.26, s * 0.05);
      ln(ctx, s * 0.03, STEEL_D); ctx.beginPath(); ctx.moveTo(hand.x + face * s * 0.24, hand.y - s * 0.14); ctx.quadraticCurveTo(hand.x + face * s * 0.3, hand.y - s * 0.01, hand.x + face * s * 0.24, hand.y + s * 0.12); ctx.stroke();
    } else if (kind === 'scout') {
      ln(ctx, s * 0.03, WOOD); ctx.beginPath(); ctx.moveTo(hand.x, y - s * 0.04); ctx.lineTo(hand.x + face * s * 0.04, top - s * 0.02); ctx.stroke();
      ctx.fillStyle = shade(cloth, -0.25); ln(ctx, s * 0.025, INK); ctx.beginPath(); ctx.moveTo(lx - s * 0.1, p.sh); ctx.quadraticCurveTo(lx - face * s * 0.24, p.hip, lx - face * s * 0.2, y - s * 0.12); ctx.lineTo(lx + face * s * 0.02, p.hip); ctx.closePath(); ctx.fill(); ctx.stroke();
    }
  }
  ctx.restore();
}

// ------------------------------------------------------------------ the label (screen sized)
/** A small vector mark for a status, centred at (x, y), radius r. */
export function statusMark(ctx, x, y, r, status, t = 0) {
  const bg = { march: '#2f6b8f', explore: '#2f8f84', fight: '#a5312a', rest: '#5b5470', muster: '#7a6a3a', sealed: '#7a2a1f', arriving: '#a5312a' }[status];
  if (!bg) return;
  ctx.save();
  ctx.fillStyle = bg; ctx.strokeStyle = '#fffaf0'; ctx.lineWidth = r * 0.18;
  ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  ctx.strokeStyle = '#fffaf0'; ctx.fillStyle = '#fffaf0'; ctx.lineWidth = r * 0.22; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
  const k = r * 0.55;
  if (status === 'march') { ctx.beginPath(); ctx.moveTo(x - k, y); ctx.lineTo(x + k, y); ctx.moveTo(x + k * 0.3, y - k * 0.6); ctx.lineTo(x + k, y); ctx.lineTo(x + k * 0.3, y + k * 0.6); ctx.stroke(); }
  else if (status === 'explore') { ctx.beginPath(); ctx.arc(x, y, k * 0.9, 0, Math.PI * 2); ctx.stroke(); const a = t * 1.5; ctx.beginPath(); ctx.moveTo(x + Math.cos(a) * k * 0.8, y + Math.sin(a) * k * 0.8); ctx.lineTo(x - Math.cos(a) * k * 0.4, y - Math.sin(a) * k * 0.4); ctx.stroke(); }
  else if (status === 'fight' || status === 'arriving') { ctx.beginPath(); ctx.moveTo(x - k, y - k); ctx.lineTo(x + k, y + k); ctx.moveTo(x + k, y - k); ctx.lineTo(x - k, y + k); ctx.stroke(); }
  else if (status === 'rest') { ctx.beginPath(); ctx.arc(x, y, k * 0.85, Math.PI * 0.35, Math.PI * 1.65); ctx.stroke(); }
  else if (status === 'muster') { ctx.beginPath(); ctx.moveTo(x - k * 0.6, y - k); ctx.lineTo(x + k * 0.6, y - k); ctx.lineTo(x - k * 0.6, y + k); ctx.lineTo(x + k * 0.6, y + k); ctx.closePath(); ctx.stroke(); }
  else if (status === 'sealed') { ctx.beginPath(); ctx.arc(x, y - k * 0.25, k * 0.45, 0, Math.PI * 2); ctx.fill(); ctx.beginPath(); ctx.moveTo(x - k * 0.2, y); ctx.lineTo(x + k * 0.2, y); ctx.lineTo(x + k * 0.32, y + k * 0.8); ctx.lineTo(x - k * 0.32, y + k * 0.8); ctx.closePath(); ctx.fill(); }
  ctx.restore();
}

/** Troops as a short figure: 950, 1.2k, 12k. */
export const troopsText = n => (n === null || n === undefined || n <= 0 ? '?' : n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n));

/**
 * The pill over a token (screen sized: `k` = 1 / zoom): the faction's colour, troops and
 * "×n", a gold rim for the viewer's own, the status mark at its left. Returns its box.
 */
/** A pill's text and size (screen sized by `k`). */
export function pillSize(ctx, k, { troops, n = 1, status = null, text = null }) {
  const label = text ?? `${troopsText(troops)}${n > 1 ? ` \u00d7${n}` : ''}`;
  ctx.save(); ctx.font = `700 ${12 * k}px system-ui, -apple-system, sans-serif`;
  const w = ctx.measureText(label).width + 14 * k + (status ? 12 * k : 0);
  ctx.restore();
  return { label, w, h: 18 * k };
}

export function unitPill(ctx, x, y, k, { faction, troops, n = 1, own = false, status = null, text = null, t = 0, dashed = false }) {
  const label = text ?? `${troopsText(troops)}${n > 1 ? ` ×${n}` : ''}`;
  ctx.save();
  ctx.font = `700 ${12 * k}px system-ui, -apple-system, sans-serif`;
  const tw = ctx.measureText(label).width, h = 18 * k, w = tw + 14 * k + (status ? 12 * k : 0);
  const x0 = x - w / 2, y0 = y - h;
  ctx.fillStyle = 'rgba(16,14,10,.55)'; ctx.beginPath(); ctx.roundRect?.(x0 - 1 * k, y0 + 1.5 * k, w + 2 * k, h, 9 * k); ctx.fill();
  ctx.fillStyle = FACTION_DARK[faction] ?? '#3a3a34'; ctx.beginPath(); ctx.roundRect?.(x0, y0, w, h, 9 * k); ctx.fill();
  ctx.lineWidth = (own ? 2.2 : 1.2) * k; ctx.strokeStyle = own ? '#f3d58a' : 'rgba(255,250,240,.55)';
  if (dashed) ctx.setLineDash([4 * k, 3 * k]);
  ctx.stroke(); ctx.setLineDash([]);
  ctx.fillStyle = '#fffaf0'; ctx.textAlign = 'left'; ctx.textBaseline = 'middle';
  ctx.fillText(label, x0 + 7 * k + (status ? 12 * k : 0), y0 + h / 2 + 0.5 * k);
  if (status) statusMark(ctx, x0 + 2 * k, y0 + h / 2, 9 * k, status, t);
  ctx.restore();
  return { x: x0, y: y0, w, h };
}

/**
 * Place the pills of a frame without overlaps (a pill that would cover one already placed
 * moves up), then draw them: they are what a player reads, so they go on top of everything.
 * `items` = `[{tok, s}]`; `boxes` = boxes already taken (name tags), world px.
 */
export function placePills(ctx, items, k, t, boxes = []) {
  const taken = [...boxes];
  const hit = b => taken.some(o => b.x < o.x + o.w && b.x + b.w > o.x && b.y < o.y + o.h && b.y + b.h > o.y);
  for (const { tok, s } of items) {
    const { w, h } = pillSize(ctx, k, tok);
    let cy = tok.y - s * 1.08;
    for (let i = 0; i < 6; i++) { const b = { x: tok.x - w / 2, y: cy - h, w, h }; if (!hit(b)) break; cy -= h + 3 * k; }
    taken.push({ x: tok.x - w / 2, y: cy - h, w, h });
    unitPill(ctx, tok.x, cy, k, { faction: tok.faction, troops: tok.troops, n: tok.n ?? 1, own: !!tok.own, status: tok.status ?? null, text: tok.text ?? null, t, dashed: !!tok.dashed });
  }
  return taken;
}

/** A whole token: disc, soldier, and (when `label`) the pill above it. `s` is its height (world px). */
export function paintToken(ctx, tok, { s, k, t = 0, label = true }) {
  const { x, y, faction, kind, face = 1, alpha = 1, walking = false, own = false } = tok;
  const step = (t * (walking ? 1.6 : 0.5) + (tok.phase ?? 0)) % 1;
  baseDisc(ctx, x, y, s, faction, { own, alpha, pulse: 0.5 + 0.5 * Math.sin(t * 2.4) });
  unitFigure(ctx, x, y - s * 0.02, s, kind, { faction, face, step, walking, alpha, lunge: tok.lunge ?? 0 });
  if (label) unitPill(ctx, x, y - s * 1.08, k, { faction, troops: tok.troops, n: tok.n ?? 1, own, status: tok.status ?? null, text: tok.text ?? null, t, dashed: !!tok.dashed });
}

// ------------------------------------------------------------------ the tokens of a province (one per faction per tile)
const hash = (a, b, c = 0) => { let h = Math.imul(a | 0, 0x27d4eb2d) ^ Math.imul(b | 0, 0x165667b1) ^ Math.imul(c | 0, 0x9e3779b9); h ^= h >>> 15; h = Math.imul(h, 0x2c1b3c6d); h ^= h >>> 12; return (h >>> 0) / 4294967296; };
const centre = (p, q, idx) => { const h = tileHex(p, q, idx); return project(h.q, h.r); };
/** Where the groups of one tile stand (units of RADIUS): one in front, two face to face, three in a triangle. */
const SPOTS = [[[0, 0.16]], [[-0.3, 0.2], [0.3, 0.2]], [[-0.32, 0.26], [0.32, 0.26], [0, -0.02]]];
const SPOTS_HOLDING = [[[0.44, 0.4]], [[0.44, 0.4], [-0.1, 0.62]], [[0.44, 0.4], [-0.1, 0.62], [-0.5, 0.36]]];

/**
 * The tokens of one province: `[{x, y, faction, kind, troops, n, status, face, alpha, own, dashed, hosts}]`.
 * `hosts` = `[{id, faction, unit, troops (whole), stamina, state, arriving?, broken?}]` (already filtered by fog);
 * `holdingTiles` = Set of tile indices with a holding; `exploring` / `marching` = Sets of host ids;
 * `restBelow` the stamina a march needs.
 */
export function provinceTokens({ p, q, hosts, holdingTiles = new Set(), viewerFaction = null, exploring = new Set(), marching = new Set(), restBelow = 0, skipTiles = new Set() }) {
  const byTile = new Map();
  for (const h of hosts) {
    if (marching.has(String(h.id))) continue;   // the viewer's own column walks its road (movers)
    if (skipTiles.has(h.tile)) continue;
    // a host that has set out is its own token (sealed, at its origin) beside the ones still here
    const key = `${h.faction}${h.state === 3 ? ':out' : ''}`;
    const t = byTile.get(h.tile) ?? new Map();
    const g = t.get(key) ?? { faction: h.faction, out: h.state === 3, hosts: [] };
    g.hosts.push(h); t.set(key, g); byTile.set(h.tile, t);
  }
  const out = [];
  for (const [tile, groups] of byTile) {
    const list = [...groups.values()].sort((a, b) => (a.out - b.out) || (a.faction === viewerFaction ? -1 : b.faction === viewerFaction ? 1 : a.faction - b.faction));
    const c = centre(p, q, tile);
    const spots = (holdingTiles.has(tile) ? SPOTS_HOLDING : SPOTS)[Math.min(list.length, 3) - 1];
    const contested = new Set(list.filter(g => !g.out).map(g => g.faction)).size > 1;
    list.slice(0, 3).forEach((g, i) => {
      const main = [...g.hosts].sort((a, b) => b.troops - a.troops)[0];
      const kind = UNIT_KINDS[main.unit] ?? 'spearman';
      const troops = g.hosts.reduce((a, h) => a + (h.troops ?? 0), 0);
      const st = g.hosts.every(h => h.state === 3) ? 'sealed' : g.hosts.some(h => h.arriving) ? 'arriving' : contested ? 'fight'
        : g.hosts.some(h => exploring.has(String(h.id))) ? 'explore' : g.hosts.every(h => h.state === 2) ? 'muster'
        : g.hosts.some(h => h.state === 1 && (h.stamina ?? 120) < restBelow) ? 'rest' : null;
      const [dx, dy] = spots[i];
      const face = contested || list.length > 1 ? (dx <= 0 ? 1 : -1) : hash(p * 61 + tile, q, g.faction) < 0.5 ? 1 : -1;
      out.push({ x: c.x + dx * RADIUS, y: c.y + dy * RADIUS * FLATTEN * 1.6, faction: g.faction, kind, troops, n: g.hosts.length, status: st, face,
        alpha: st === 'sealed' ? 0.55 : st === 'muster' ? 0.75 : 1, own: g.faction === viewerFaction, dashed: st === 'arriving', phase: hash(tile, i, g.faction), tile, p, q, hosts: g.hosts.map(h => String(h.id)) });
    });
  }
  return out;
}

// ------------------------------------------------------------------ roads (the viewer's own marches)
/** The world points of a planned route `{p, q, tile, dirs}` (hex centres), or a straight pair. */
export function routePoints(route, dest) {
  if (route && Array.isArray(route.dirs)) {
    const h = tileHex(route.p, route.q, route.tile);
    if (h) {
      const pts = [project(h.q, h.r)];
      let hq = h.q, hr = h.r;
      for (const d of route.dirs) { const v = DIRECTIONS[d]; if (!v) break; hq += v[0]; hr += v[1]; pts.push(project(hq, hr)); }
      return pts;
    }
  }
  return dest ? [dest.from, dest.to] : [];
}

/** A point at share `k` (0..1) of a polyline smoothed as a Catmull-Rom curve, by arc length; and the heading. */
export function alongPath(pts, k) {
  if (!pts.length) return null;
  if (pts.length === 1) return { x: pts[0].x, y: pts[0].y, dx: 1, dy: 0 };
  const samples = [];
  const P = i => pts[Math.max(0, Math.min(pts.length - 1, i))];
  for (let i = 0; i < pts.length - 1; i++) for (let s = 0; s < 8; s++) {
    const t = s / 8, p0 = P(i - 1), p1 = P(i), p2 = P(i + 1), p3 = P(i + 2), t2 = t * t, t3 = t2 * t;
    const f = (a, b, c, d) => 0.5 * ((2 * b) + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3);
    samples.push({ x: f(p0.x, p1.x, p2.x, p3.x), y: f(p0.y, p1.y, p2.y, p3.y) });
  }
  samples.push(pts[pts.length - 1]);
  const len = [0];
  for (let i = 1; i < samples.length; i++) len.push(len[i - 1] + Math.hypot(samples[i].x - samples[i - 1].x, samples[i].y - samples[i - 1].y));
  const want = Math.max(0, Math.min(1, k)) * len[len.length - 1];
  let i = 1; while (i < len.length - 1 && len[i] < want) i++;
  const a = samples[i - 1], b = samples[i], u = len[i] > len[i - 1] ? (want - len[i - 1]) / (len[i] - len[i - 1]) : 0;
  return { x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u, dx: b.x - a.x, dy: b.y - a.y, samples };
}

/** The road of a march: dashed in the faction's colour ahead of the token, a ring at the destination. */
export function paintRoad(ctx, samples, from, faction, k, t) {
  if (!samples?.length) return;
  const fill = FACTION_FILL[faction] ?? '#8a8a80';
  ctx.save(); ctx.lineCap = 'round'; ctx.lineJoin = 'round';
  ctx.beginPath(); samples.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
  ctx.strokeStyle = 'rgba(16,14,10,.45)'; ctx.lineWidth = 6 * k; ctx.stroke();
  ctx.setLineDash([10 * k, 7 * k]); ctx.lineDashOffset = -t * 18 * k; ctx.strokeStyle = fill; ctx.lineWidth = 3.2 * k; ctx.stroke();
  ctx.setLineDash([]);
  const end = samples[samples.length - 1], pulse = 0.5 + 0.5 * Math.sin(t * 2.6);
  ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = 2.6 * k;
  ctx.beginPath(); ctx.ellipse(end.x, end.y + RADIUS * 0.12, RADIUS * (0.42 + pulse * 0.05), RADIUS * (0.42 + pulse * 0.05) * FLATTEN, 0, 0, Math.PI * 2); ctx.stroke();
  ctx.restore();
}

// ------------------------------------------------------------------ construction on a holding
/** A scaffold beside a town: poles, planks, a crane arm; the progress ring and its label are the caller's. */
export function scaffold(ctx, x, y, s, t) {
  ctx.save(); ctx.lineCap = 'round';
  ctx.fillStyle = 'rgba(0,0,0,.25)'; ctx.beginPath(); ctx.ellipse(x, y, s * 0.42, s * 0.12, 0, 0, Math.PI * 2); ctx.fill();
  // the rising walls (a stone base growing with time is the caller's `k`; here a fixed half)
  ctx.fillStyle = '#c9b48a'; ctx.strokeStyle = INK; ctx.lineWidth = s * 0.025;
  ctx.fillRect(x - s * 0.3, y - s * 0.3, s * 0.6, s * 0.3); ctx.strokeRect(x - s * 0.3, y - s * 0.3, s * 0.6, s * 0.3);
  ctx.strokeStyle = 'rgba(60,40,20,.5)'; ctx.lineWidth = s * 0.012;
  for (let i = 1; i < 3; i++) { ctx.beginPath(); ctx.moveTo(x - s * 0.3, y - s * 0.1 * i); ctx.lineTo(x + s * 0.3, y - s * 0.1 * i); ctx.stroke(); }
  // poles and planks
  ctx.strokeStyle = WOOD; ctx.lineWidth = s * 0.035;
  for (const dx of [-0.34, 0, 0.34]) { ctx.beginPath(); ctx.moveTo(x + dx * s, y + s * 0.02); ctx.lineTo(x + dx * s, y - s * 0.62); ctx.stroke(); }
  ctx.lineWidth = s * 0.045; for (const dy of [0.3, 0.55]) { ctx.beginPath(); ctx.moveTo(x - s * 0.38, y - dy * s); ctx.lineTo(x + s * 0.38, y - dy * s); ctx.stroke(); }
  // a crane arm that swings a load
  const sw = Math.sin(t * 0.9) * 0.15;
  ctx.lineWidth = s * 0.03; ctx.beginPath(); ctx.moveTo(x + s * 0.34, y - s * 0.62); ctx.lineTo(x + s * 0.34 + Math.cos(-0.5 + sw) * s * 0.4, y - s * 0.62 + Math.sin(-0.5 + sw) * s * 0.4); ctx.stroke();
  const ex = x + s * 0.34 + Math.cos(-0.5 + sw) * s * 0.4, ey = y - s * 0.62 + Math.sin(-0.5 + sw) * s * 0.4;
  ctx.strokeStyle = '#3b3128'; ctx.lineWidth = s * 0.01; ctx.beginPath(); ctx.moveTo(ex, ey); ctx.lineTo(ex, ey + s * 0.2); ctx.stroke();
  ctx.fillStyle = '#8f8a80'; ctx.fillRect(ex - s * 0.04, ey + s * 0.2, s * 0.08, s * 0.06);
  ctx.restore();
}

/** A progress ring with a hammer, screen sized, and a label beside it (`text`). */
export function progressBadge(ctx, x, y, k, share, text, { side = 'right' } = {}) {
  const r = 11 * k;
  ctx.save();
  ctx.fillStyle = 'rgba(16,14,10,.82)'; ctx.beginPath(); ctx.arc(x, y, r + 2 * k, 0, Math.PI * 2); ctx.fill();
  ctx.strokeStyle = 'rgba(255,255,255,.2)'; ctx.lineWidth = 3 * k; ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.stroke();
  ctx.strokeStyle = '#f3d58a'; ctx.beginPath(); ctx.arc(x, y, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.02, Math.min(1, share))); ctx.stroke();
  ctx.strokeStyle = '#fffaf0'; ctx.lineWidth = 2 * k; ctx.lineCap = 'round';
  ctx.beginPath(); ctx.moveTo(x - 4 * k, y + 4 * k); ctx.lineTo(x + 3 * k, y - 3 * k); ctx.stroke();
  ctx.fillStyle = '#fffaf0'; ctx.save(); ctx.translate(x + 3 * k, y - 3 * k); ctx.rotate(Math.PI / 4); ctx.fillRect(-4 * k, -2 * k, 8 * k, 4 * k); ctx.restore();
  if (text) {
    ctx.font = `700 ${11.5 * k}px system-ui, -apple-system, sans-serif`; ctx.textBaseline = 'middle'; ctx.textAlign = 'left';
    const w = ctx.measureText(text).width + 12 * k, h = 18 * k, x0 = side === 'left' ? x - r - 4 * k - w : x + r + 4 * k;
    ctx.fillStyle = 'rgba(16,14,10,.82)'; ctx.beginPath(); ctx.roundRect?.(x0, y - h / 2, w, h, 8 * k); ctx.fill();
    ctx.fillStyle = '#fffaf0'; ctx.fillText(text, x0 + 6 * k, y + 0.5 * k);
  }
  ctx.restore();
}
