// People on the map (design session, "people" request): the life of a
// holding and the movement of armies, drawn as small figures in world
// coordinates over the sprite art (map/sprites.mjs calls `paintPeople`
// after the hosts and `paintNameTags` after the fog; the replay page can
// call both with its own scene). Everything is presentation.
//
//   residents   a few townsfolk around each holding, more with the tier
//   gatherers   carriers walking between a holding and a neighbouring tile
//   columns     a host that departed and has not arrived: soldiers forming
//               up and marching out *at the origin*, with the arrival bell
//               only. A sealed march shows that it left, never where it
//               goes: no line, no arrow, no heading toward a destination
//               (the column circles its own tile and fades).
//   scouts      a cloaked figure on each tile an Explore revealed
//   name tags   the holder's face and name over a holding (identity.mjs)
//
// Level of detail: nothing below `PEOPLE_MIN_R` screen px per hex radius;
// residents thin out first; a frame never draws more than `PEOPLE_BUDGET`
// figures, so thousands of holdings on screen cost a bounded amount.
import { RADIUS, FLATTEN } from '../../map.mjs';
import { tileHex } from '../fgeo.mjs';
import { project } from '../../map.mjs';
import { FACTION_FILL, FACTION_DARK, shade, avatarImage } from './avatar.mjs';
import { SKIN } from './identity.mjs';
import { paintBadge } from './activity.mjs';

/** Screen px per hex radius from which figures are drawn, and from which every resident is. */
export const PEOPLE_MIN_R = 22;
export const PEOPLE_FULL_R = 34;
/** Name tags: cities and strongholds from NAME_TAG_MIN_R screen px per hex radius, every holding from NAME_TAG_ALL_R; at most NAME_TAG_MAX a frame. */
export const NAME_TAG_MIN_R = 30;
export const NAME_TAG_ALL_R = 52;
export const NAME_TAG_MAX = 60;
/** The most figures one frame draws. */
export const PEOPLE_BUDGET = 900;
/** Residents by tier (hamlet, town, city, stronghold). */
export const RESIDENTS = Object.freeze([3, 5, 8, 9]);
/** Animation frame interval while figures are on screen (ms). */
export const PEOPLE_FRAME_MS = 70;

const CLOTH = ['#8a6a46', '#6f7a52', '#9a7a5a', '#5f6a74', '#a08a62', '#7a5a4a'];
const hash = (a, b, c = 0) => { let h = Math.imul(a | 0, 0x27d4eb2d) ^ Math.imul(b | 0, 0x165667b1) ^ Math.imul(c | 0, 0x9e3779b9); h ^= h >>> 15; h = Math.imul(h, 0x2c1b3c6d); h ^= h >>> 12; return (h >>> 0) / 4294967296; };
const hexCentre = (p, q, idx) => { const h = tileHex(p, q, idx); return project(h.q, h.r); };
/** The six neighbour directions of a pointy-top hex in world px (flattened). */
const DIRS = [0, 60, 120, 180, 240, 300].map(d => { const a = (d * Math.PI) / 180; return [Math.cos(a) * RADIUS * 1.73, Math.sin(a) * RADIUS * 1.73 * FLATTEN]; });

/**
 * One figure, feet at (x, y), `s` tall (world px). kind: 'folk' | 'carrier'
 * | 'soldier' | 'scout'; `step` 0..1 the walk phase; `face` ±1 the side it
 * walks toward; `alpha` for fading columns.
 */
export function figure(ctx, x, y, s, { kind = 'folk', cloth = '#8a6a46', trim = null, skin = SKIN[1], step = 0, face = 1, alpha = 1 } = {}) {
  const bob = Math.abs(Math.sin(step * Math.PI * 2)) * s * 0.05;
  const leg = Math.sin(step * Math.PI * 2) * s * 0.12;
  ctx.globalAlpha = alpha * 0.28;
  ctx.fillStyle = '#1a1612';
  ctx.beginPath(); ctx.ellipse(x, y, s * 0.24, s * 0.08, 0, 0, Math.PI * 2); ctx.fill();
  ctx.globalAlpha = alpha;
  const dark = shade(cloth, -0.35);
  // legs
  ctx.strokeStyle = shade(cloth, -0.5); ctx.lineWidth = s * 0.1; ctx.lineCap = 'round';
  ctx.beginPath(); ctx.moveTo(x - s * 0.06, y - s * 0.34 - bob); ctx.lineTo(x - s * 0.06 + leg, y - s * 0.02);
  ctx.moveTo(x + s * 0.06, y - s * 0.34 - bob); ctx.lineTo(x + s * 0.06 - leg, y - s * 0.02); ctx.stroke();
  // body (a rounded tunic), cloak for scouts
  const top = y - s * 0.82 - bob, mid = y - s * 0.3 - bob;
  if (kind === 'scout') {
    ctx.fillStyle = dark;
    ctx.beginPath(); ctx.moveTo(x, top - s * 0.04); ctx.quadraticCurveTo(x - s * 0.32, mid, x - s * 0.24 * face, y - s * 0.12 - bob); ctx.lineTo(x + s * 0.2, mid); ctx.closePath(); ctx.fill();
  }
  ctx.fillStyle = cloth; ctx.strokeStyle = dark; ctx.lineWidth = s * 0.04;
  ctx.beginPath();
  ctx.moveTo(x - s * 0.16, mid); ctx.quadraticCurveTo(x - s * 0.2, top + s * 0.1, x - s * 0.08, top + s * 0.02);
  ctx.lineTo(x + s * 0.08, top + s * 0.02); ctx.quadraticCurveTo(x + s * 0.2, top + s * 0.1, x + s * 0.16, mid); ctx.closePath();
  ctx.fill(); ctx.stroke();
  if (trim) { ctx.fillStyle = trim; ctx.fillRect(x - s * 0.15, mid - s * 0.22, s * 0.3, s * 0.07); }
  // head
  ctx.fillStyle = skin; ctx.beginPath(); ctx.arc(x + face * s * 0.01, top - s * 0.08, s * 0.11, 0, Math.PI * 2); ctx.fill();
  ctx.strokeStyle = shade(skin, -0.35); ctx.lineWidth = s * 0.025; ctx.stroke();
  if (kind === 'soldier') {
    // helmet, spear, a small shield
    ctx.fillStyle = '#9aa0a6'; ctx.beginPath(); ctx.arc(x, top - s * 0.11, s * 0.115, Math.PI, 0); ctx.fill();
    ctx.strokeStyle = '#5b4632'; ctx.lineWidth = s * 0.045;
    ctx.beginPath(); ctx.moveTo(x + face * s * 0.2, y - s * 0.12 - bob); ctx.lineTo(x + face * s * 0.24, top - s * 0.42 - bob); ctx.stroke();
    ctx.fillStyle = '#c9ced2'; ctx.beginPath(); ctx.moveTo(x + face * s * 0.24, top - s * 0.52 - bob); ctx.lineTo(x + face * s * 0.2, top - s * 0.38 - bob); ctx.lineTo(x + face * s * 0.28, top - s * 0.38 - bob); ctx.closePath(); ctx.fill();
    ctx.fillStyle = trim ?? cloth; ctx.strokeStyle = dark; ctx.lineWidth = s * 0.03;
    ctx.beginPath(); ctx.ellipse(x - face * s * 0.16, mid - s * 0.18, s * 0.1, s * 0.15, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  } else if (kind === 'builder' || kind === 'fighter') {
    // an arm with a hammer (builder) or a sword (fighter), swinging with the step
    const sw = Math.sin(step * Math.PI * 2) * (kind === 'fighter' ? 1.1 : 0.9);
    const ax = x + face * s * 0.12, ay = top + s * 0.14;
    const hx = ax + face * Math.cos(sw - 0.6) * s * 0.34, hy = ay - Math.sin(sw - 0.6) * s * 0.34;
    ctx.strokeStyle = kind === 'fighter' ? '#d8dde2' : '#6b5032'; ctx.lineWidth = s * (kind === 'fighter' ? 0.05 : 0.06);
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(hx, hy); ctx.stroke();
    if (kind === 'builder') { ctx.fillStyle = '#7d848a'; ctx.fillRect(hx - s * 0.07, hy - s * 0.05, s * 0.14, s * 0.1); }
    else { ctx.fillStyle = '#9aa0a6'; ctx.beginPath(); ctx.arc(x, top - s * 0.11, s * 0.115, Math.PI, 0); ctx.fill(); }
  } else if (kind === 'carrier') {
    ctx.fillStyle = '#c9a66a'; ctx.strokeStyle = '#7a5a2a'; ctx.lineWidth = s * 0.03;
    ctx.beginPath(); ctx.ellipse(x - face * s * 0.1, top + s * 0.06, s * 0.16, s * 0.11, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  } else if (kind === 'scout') {
    ctx.fillStyle = dark; ctx.beginPath(); ctx.arc(x, top - s * 0.1, s * 0.13, Math.PI * 1.05, -0.05); ctx.fill();
  }
  ctx.globalAlpha = 1;
}

/** Actors (what a tile is doing) are drawn this many times a townsperson's height (Civ: units read bigger than buildings). */
export const ACTOR_SCALE = 3.6;
const INK = '#1d1a16';

/**
 * A character doing something, readable at a glance: outlined body,
 * arms, the tool of its work and an exaggerated motion. `pose`:
 * 'hammer' | 'spear' | 'drill' | 'sword' | 'scout' | 'sit' | 'march'.
 * Feet at (x, y), `s` tall (world px); `face` ±1; `step` 0..1.
 */
export function actor(ctx, x, y, s, { pose = 'spear', cloth = '#8a6a46', trim = '#3a3a34', skin = SKIN[1], step = 0, face = 1, alpha = 1, hair = '#3b2a20' } = {}) {
  const w = s * 0.035;
  const sw = Math.sin(step * Math.PI * 2);
  const walking = pose === 'march' || pose === 'scout';
  const bob = walking ? Math.abs(sw) * s * 0.04 : pose === 'drill' ? (step < 0.5 ? s * 0.03 : 0) : 0;
  const leg = walking ? sw * s * 0.13 : pose === 'drill' ? (step < 0.5 ? s * 0.1 : 0) : 0;
  const seated = pose === 'sit';
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.lineJoin = 'round'; ctx.lineCap = 'round';
  // shadow
  ctx.globalAlpha = alpha * 0.3; ctx.fillStyle = '#120f0c';
  ctx.beginPath(); ctx.ellipse(x, y, s * 0.26, s * 0.075, 0, 0, Math.PI * 2); ctx.fill();
  ctx.globalAlpha = alpha;
  const hip = y - (seated ? s * 0.16 : s * 0.42) - bob, shoulder = y - (seated ? s * 0.5 : s * 0.78) - bob, head = shoulder - s * 0.13;
  const body = shade(cloth, -0.0), dark = shade(cloth, -0.4), boot = '#3b2b1e';
  // legs (seated: folded in front)
  ctx.strokeStyle = INK; ctx.lineWidth = s * 0.11 + w * 2;
  const legs = seated ? [[x - s * 0.05, hip, x + face * s * 0.22, y - s * 0.06], [x + s * 0.05, hip, x + face * s * 0.26, y - s * 0.02]]
    : [[x - s * 0.06, hip, x - s * 0.06 + leg, y - s * 0.03], [x + s * 0.06, hip, x + s * 0.06 - leg, y - s * 0.03]];
  for (const [a, b, c, d] of legs) { ctx.beginPath(); ctx.moveTo(a, b); ctx.lineTo(c, d); ctx.stroke(); }
  ctx.strokeStyle = shade(cloth, -0.55); ctx.lineWidth = s * 0.11;
  for (const [a, b, c, d] of legs) { ctx.beginPath(); ctx.moveTo(a, b); ctx.lineTo(c, d); ctx.stroke(); }
  ctx.fillStyle = boot; for (const [, , c, d] of legs) { ctx.beginPath(); ctx.ellipse(c + face * s * 0.02, d, s * 0.07, s * 0.04, 0, 0, Math.PI * 2); ctx.fill(); }
  // back arm (shield arm for soldiers)
  const armed = pose === 'spear' || pose === 'drill' || pose === 'sword' || pose === 'march';
  const backHand = { x: x - face * s * 0.2, y: shoulder + s * 0.26 };
  ctx.strokeStyle = INK; ctx.lineWidth = s * 0.09 + w * 2;
  ctx.beginPath(); ctx.moveTo(x - face * s * 0.1, shoulder + s * 0.04); ctx.lineTo(backHand.x, backHand.y); ctx.stroke();
  ctx.strokeStyle = dark; ctx.lineWidth = s * 0.09; ctx.stroke();
  if (armed) {
    ctx.fillStyle = trim; ctx.strokeStyle = INK; ctx.lineWidth = w * 1.4;
    ctx.beginPath(); ctx.ellipse(backHand.x - face * s * 0.02, backHand.y - s * 0.06, s * 0.13, s * 0.17, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    ctx.fillStyle = cloth; ctx.beginPath(); ctx.arc(backHand.x - face * s * 0.02, backHand.y - s * 0.06, s * 0.05, 0, Math.PI * 2); ctx.fill();
  }
  // torso: a tunic with a belt, cloak for scouts, apron for builders
  ctx.fillStyle = body; ctx.strokeStyle = INK; ctx.lineWidth = w * 1.4;
  ctx.beginPath();
  ctx.moveTo(x - s * 0.17, hip + s * 0.06);
  ctx.quadraticCurveTo(x - s * 0.2, shoulder + s * 0.08, x - s * 0.1, shoulder);
  ctx.lineTo(x + s * 0.1, shoulder);
  ctx.quadraticCurveTo(x + s * 0.2, shoulder + s * 0.08, x + s * 0.17, hip + s * 0.06);
  ctx.closePath(); ctx.fill(); ctx.stroke();
  ctx.fillStyle = trim; ctx.fillRect(x - s * 0.17, hip - s * 0.04, s * 0.34, s * 0.06);
  if (pose === 'hammer') { ctx.fillStyle = '#a07a4a'; ctx.beginPath(); ctx.moveTo(x - s * 0.12, shoulder + s * 0.12); ctx.lineTo(x + s * 0.12, shoulder + s * 0.12); ctx.lineTo(x + s * 0.14, hip + s * 0.06); ctx.lineTo(x - s * 0.14, hip + s * 0.06); ctx.closePath(); ctx.fill(); ctx.stroke(); }
  if (pose === 'scout') { ctx.fillStyle = dark; ctx.beginPath(); ctx.moveTo(x - s * 0.12, shoulder - s * 0.02); ctx.quadraticCurveTo(x - face * s * 0.34, hip, x - face * s * 0.26, y - s * 0.12); ctx.lineTo(x + face * s * 0.05, hip + s * 0.04); ctx.closePath(); ctx.fill(); ctx.stroke(); }
  // head
  ctx.fillStyle = skin; ctx.strokeStyle = INK; ctx.lineWidth = w * 1.4;
  ctx.beginPath(); ctx.arc(x + face * s * 0.01, head, s * 0.12, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  ctx.fillStyle = INK; ctx.beginPath(); ctx.arc(x + face * s * 0.06, head - s * 0.01, s * 0.018, 0, Math.PI * 2); ctx.fill();
  if (armed) { // a kettle helmet with a rim
    ctx.fillStyle = '#9aa0a6'; ctx.beginPath(); ctx.arc(x, head - s * 0.02, s * 0.13, Math.PI * 1.02, Math.PI * 1.98); ctx.fill(); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(x - s * 0.18, head - s * 0.01); ctx.lineTo(x + s * 0.18, head - s * 0.01); ctx.lineWidth = s * 0.035; ctx.strokeStyle = '#6d7277'; ctx.stroke();
  } else if (pose === 'scout') {
    ctx.fillStyle = dark; ctx.strokeStyle = INK; ctx.lineWidth = w * 1.4; ctx.beginPath(); ctx.arc(x, head, s * 0.15, Math.PI * 1.05, Math.PI * 1.95 + 0.4); ctx.fill(); ctx.stroke();
  } else {
    ctx.fillStyle = hair; ctx.beginPath(); ctx.arc(x - face * s * 0.01, head - s * 0.03, s * 0.12, Math.PI * 1.05, Math.PI * 1.95); ctx.fill();
  }
  // front arm and the tool of the work
  const sh = { x: x + face * s * 0.1, y: shoulder + s * 0.04 };
  let hand, tool = null;
  if (pose === 'hammer') { const a = -0.3 + (sw > 0 ? sw : sw * 0.2) * 1.6; hand = { x: sh.x + face * Math.cos(a) * s * 0.26, y: sh.y - Math.sin(a) * s * 0.26 }; tool = 'hammer'; }
  else if (pose === 'sword') { const a = 0.2 + sw * 1.2; hand = { x: sh.x + face * Math.cos(a) * s * 0.24, y: sh.y - Math.sin(a) * s * 0.24 }; tool = 'sword'; }
  else if (pose === 'scout') { hand = { x: sh.x + face * s * 0.12, y: sh.y + s * 0.2 }; tool = 'staff'; }
  else if (pose === 'sit') { hand = { x: sh.x + face * s * 0.18, y: sh.y + s * 0.16 }; tool = 'bowl'; }
  else { hand = { x: sh.x + face * s * 0.12, y: sh.y + s * 0.18 }; tool = 'spear'; }
  ctx.strokeStyle = INK; ctx.lineWidth = s * 0.09 + w * 2; ctx.beginPath(); ctx.moveTo(sh.x, sh.y); ctx.lineTo(hand.x, hand.y); ctx.stroke();
  ctx.strokeStyle = body; ctx.lineWidth = s * 0.09; ctx.stroke();
  ctx.fillStyle = skin; ctx.beginPath(); ctx.arc(hand.x, hand.y, s * 0.045, 0, Math.PI * 2); ctx.fill();
  ctx.lineWidth = s * 0.045;
  if (tool === 'hammer') {
    const a = Math.atan2(hand.y - sh.y, hand.x - sh.x), hx = hand.x + Math.cos(a) * s * 0.2, hy = hand.y + Math.sin(a) * s * 0.2;
    ctx.strokeStyle = '#6b5032'; ctx.beginPath(); ctx.moveTo(hand.x, hand.y); ctx.lineTo(hx, hy); ctx.stroke();
    ctx.save(); ctx.translate(hx, hy); ctx.rotate(a); ctx.fillStyle = '#7d848a'; ctx.strokeStyle = INK; ctx.lineWidth = w; ctx.fillRect(-s * 0.04, -s * 0.09, s * 0.08, s * 0.18); ctx.strokeRect(-s * 0.04, -s * 0.09, s * 0.08, s * 0.18); ctx.restore();
  } else if (tool === 'sword') {
    const a = Math.atan2(hand.y - sh.y, hand.x - sh.x) - face * 0.6;
    ctx.strokeStyle = INK; ctx.lineWidth = s * 0.06; ctx.beginPath(); ctx.moveTo(hand.x, hand.y); ctx.lineTo(hand.x + Math.cos(a) * s * 0.42, hand.y + Math.sin(a) * s * 0.42); ctx.stroke();
    ctx.strokeStyle = '#e4e8ec'; ctx.lineWidth = s * 0.035; ctx.stroke();
  } else if (tool === 'spear') {
    ctx.strokeStyle = '#5b4632'; ctx.lineWidth = s * 0.045; ctx.beginPath(); ctx.moveTo(hand.x, y - s * 0.02); ctx.lineTo(hand.x, y - s * 1.22 - bob); ctx.stroke();
    ctx.fillStyle = '#c9ced2'; ctx.strokeStyle = INK; ctx.lineWidth = w; ctx.beginPath(); ctx.moveTo(hand.x, y - s * 1.36 - bob); ctx.lineTo(hand.x - s * 0.05, y - s * 1.2 - bob); ctx.lineTo(hand.x + s * 0.05, y - s * 1.2 - bob); ctx.closePath(); ctx.fill(); ctx.stroke();
  } else if (tool === 'staff') {
    ctx.strokeStyle = '#6b5032'; ctx.lineWidth = s * 0.04; ctx.beginPath(); ctx.moveTo(hand.x + face * s * 0.04, y - s * 0.02); ctx.lineTo(hand.x - face * s * 0.02, shoulder - s * 0.2); ctx.stroke();
  } else if (tool === 'bowl') {
    ctx.fillStyle = '#a07a4a'; ctx.strokeStyle = INK; ctx.lineWidth = w; ctx.beginPath(); ctx.arc(hand.x + face * s * 0.03, hand.y, s * 0.06, 0, Math.PI); ctx.fill(); ctx.stroke();
  }
  ctx.restore();
}

/** A seated figure (a resting host). */
function sitter(ctx, x, y, s, { cloth, skin = SKIN[1], trim = null, face = 1 }) {
  ctx.globalAlpha = 0.25; ctx.fillStyle = '#1a1612'; ctx.beginPath(); ctx.ellipse(x, y, s * 0.26, s * 0.08, 0, 0, 7); ctx.fill(); ctx.globalAlpha = 1;
  ctx.fillStyle = cloth; ctx.strokeStyle = shade(cloth, -0.35); ctx.lineWidth = s * 0.04;
  ctx.beginPath(); ctx.ellipse(x, y - s * 0.24, s * 0.17, s * 0.22, 0, 0, 7); ctx.fill(); ctx.stroke();
  if (trim) { ctx.fillStyle = trim; ctx.fillRect(x - s * 0.15, y - s * 0.26, s * 0.3, s * 0.06); }
  ctx.fillStyle = skin; ctx.beginPath(); ctx.arc(x + face * s * 0.02, y - s * 0.55, s * 0.11, 0, 7); ctx.fill();
}
/** A small campfire with rising smoke. */
function campfire(ctx, x, y, s, t) {
  ctx.fillStyle = '#5b4632'; ctx.fillRect(x - s * 0.22, y - s * 0.05, s * 0.44, s * 0.08);
  const fl = 0.8 + Math.sin(t * 9) * 0.15;
  ctx.fillStyle = '#f2a33a'; ctx.beginPath(); ctx.moveTo(x - s * 0.14, y - s * 0.04); ctx.quadraticCurveTo(x, y - s * 0.5 * fl, x + s * 0.14, y - s * 0.04); ctx.fill();
  ctx.fillStyle = '#ffe08a'; ctx.beginPath(); ctx.moveTo(x - s * 0.06, y - s * 0.04); ctx.quadraticCurveTo(x, y - s * 0.28 * fl, x + s * 0.06, y - s * 0.04); ctx.fill();
  for (let i = 0; i < 3; i++) { const k = (t * 0.5 + i / 3) % 1; ctx.globalAlpha = 0.35 * (1 - k); ctx.fillStyle = '#d8d4cc'; ctx.beginPath(); ctx.arc(x + Math.sin(k * 6 + i) * s * 0.12, y - s * (0.5 + k * 1.1), s * (0.08 + k * 0.12), 0, 7); ctx.fill(); }
  ctx.globalAlpha = 1;
}
/** A clash spark. */
function spark(ctx, x, y, s) {
  ctx.strokeStyle = '#ffe08a'; ctx.lineWidth = s * 0.05;
  ctx.beginPath(); for (let i = 0; i < 6; i++) { const a = i * Math.PI / 3; ctx.moveTo(x + Math.cos(a) * s * 0.06, y + Math.sin(a) * s * 0.06); ctx.lineTo(x + Math.cos(a) * s * 0.2, y + Math.sin(a) * s * 0.2); } ctx.stroke();
}

/** A banner on a pole (a departing column's standard). */
function banner(ctx, x, y, s, faction, t) {
  ctx.strokeStyle = '#5b4632'; ctx.lineWidth = s * 0.06;
  ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x, y - s * 1.7); ctx.stroke();
  const wave = Math.sin(t * 4) * s * 0.06;
  ctx.fillStyle = FACTION_FILL[faction] ?? '#8a8a80'; ctx.strokeStyle = FACTION_DARK[faction] ?? '#55554e'; ctx.lineWidth = s * 0.04;
  ctx.beginPath(); ctx.moveTo(x, y - s * 1.68); ctx.lineTo(x + s * 0.7, y - s * 1.55 + wave); ctx.lineTo(x + s * 0.62, y - s * 1.32 + wave); ctx.lineTo(x, y - s * 1.2); ctx.closePath(); ctx.fill(); ctx.stroke();
}

/**
 * The people of one frame. `tiles` are sprites.mjs's drawn tiles ({x, y,
 * p, pq, idx, state, owner, tier, fog?}); `departures` [{p, q, tile,
 * faction, troops, arriveBell}] hosts in transit (origin only);
 * `explores` [{p, q, tiles: [idx], faction}]; `fogOf(p, q)` the province's
 * fog. Returns the number of figures drawn (0: nothing moves, no new frame).
 */
export function paintPeople(ctx, { tiles = [], zoom = 1, t = (globalThis.performance?.now?.() ?? Date.now()) / 1000, departures = [], explores = [], fogOf = () => 'clear', viewerFaction = null, columnLabel = null, activities = null } = {}) {
  const r = RADIUS * zoom;
  if (r < PEOPLE_MIN_R) return 0;
  const s = RADIUS * 0.15;
  const full = r >= PEOPLE_FULL_R;
  const seen = f => f === 'clear' || f === 'sight' || f === undefined;
  const list = [];
  // residents and gatherers around the holdings
  for (const u of tiles) {
    if (u.state !== 1 || !(u.owner < 6) || !seen(u.fog ?? fogOf(u.p, u.pq))) continue;
    const acts = activities?.get(`${u.p},${u.pq},${u.idx}`) ?? [];
    if (acts.some(a => a.kind === 'dormant')) continue;  // its lord is away: a quiet holding
    const n = full ? RESIDENTS[u.tier] ?? 2 : 1;
    for (let i = 0; i < n; i++) {
      const h = hash(u.q, u.r ?? u.idx, i);
      const speed = 0.05 + h * 0.06, a = (t * speed + h) * Math.PI * 2 * (i % 2 ? 1 : -1);
      const rad = RADIUS * (0.2 + 0.34 * hash(u.idx, i, 7));
      const x = u.x + Math.cos(a) * rad, y = u.y + Math.sin(a) * rad * FLATTEN + RADIUS * 0.12;
      const idle = hash(i, u.idx, Math.floor(t / 4)) < 0.3;
      list.push({ x, y, kind: 'folk', cloth: CLOTH[Math.floor(h * CLOTH.length)], trim: i === 0 ? FACTION_FILL[u.owner] : null, skin: SKIN[Math.floor(hash(i, u.q, 3) * SKIN.length)],
        step: idle ? 0 : (t * 1.6 + h) % 1, face: Math.sin(a) * (i % 2 ? 1 : -1) >= 0 ? -1 : 1 });
    }
    if (full) {
      // one carrier to a neighbouring tile and back (a fixed neighbour per holding)
      const d = DIRS[Math.floor(hash(u.idx, u.q, 11) * 6)];
      const k = (t * 0.07 + hash(u.q, u.idx)) % 1, there = k < 0.5 ? k * 2 : 2 - k * 2;
      list.push({ x: u.x + d[0] * 0.75 * there, y: u.y + d[1] * 0.75 * there + RADIUS * 0.1, kind: 'carrier', cloth: '#7a6a4a', skin: SKIN[2], step: (t * 1.8) % 1, face: (k < 0.5 ? d[0] : -d[0]) >= 0 ? 1 : -1 });
    }
  }
  // what the tile is doing (people/activity.mjs): one or two big actors per activity, in front of the holding
  if (activities) for (const u of tiles) {
    const acts = activities.get(`${u.p},${u.pq},${u.idx}`);
    if (!acts || !seen(u.fog ?? fogOf(u.p, u.pq))) continue;
    const kinds = new Set(acts.map(a => a.kind));
    const f = acts.find(a => a.faction < 6)?.faction ?? u.owner;
    const cloth = shade(FACTION_FILL[f] ?? '#8a8a80', -0.05), trim = FACTION_DARK[f] ?? '#3a3a34';
    const at = (dx, dy) => ({ x: u.x + dx * RADIUS, y: u.y + dy * RADIUS * FLATTEN + RADIUS * 0.12 });
    const ph = hash(u.q, u.r ?? u.idx, 5);
    const big = (p, o) => list.push({ ...p, big: true, cloth, trim, skin: SKIN[Math.floor(hash(p.x | 0, p.y | 0) * SKIN.length)], ...o });
    if (kinds.has('battle')) {
      const p = at(-0.05, 0.42);
      big({ x: p.x - RADIUS * 0.16, y: p.y }, { pose: 'sword', step: (t * 1.6 + ph) % 1, face: 1 });
      big({ x: p.x + RADIUS * 0.16, y: p.y }, { pose: 'sword', step: (t * 1.6 + ph + 0.5) % 1, face: -1, cloth: '#5f6a74', trim: '#2a2f34' });
      if (((t * 1.6 + ph) % 1) < 0.22) list.push({ x: p.x, y: p.y - RADIUS * 0.5, spark: true, big: true });
      continue;
    }
    if (kinds.has('walls') || kinds.has('build')) {
      big(at(-0.3, 0.44), { pose: 'hammer', step: (t * 1.1 + ph) % 1, face: 1, cloth: '#8a6a46', trim });
      big(at(0.32, 0.36), { pose: 'hammer', step: (t * 1.1 + ph + 0.45) % 1, face: -1, cloth: '#7a6a52', trim });
    } else if (kinds.has('muster') || kinds.has('recruit')) {
      for (let i = 0; i < 3; i++) big(at(-0.3 + i * 0.3, 0.44), { pose: 'drill', step: (t * 1.2) % 1, face: 1 });
    } else if (kinds.has('rest')) {
      big(at(-0.22, 0.46), { pose: 'sit', face: 1 }); big(at(0.26, 0.4), { pose: 'sit', face: -1 });
      list.push({ ...at(0.02, 0.5), fire: true, big: true });
    } else if (kinds.has('garrison')) {
      big(at(0.36, 0.38), { pose: 'spear', step: 0, face: -1 });
    }
  }
  // departing columns: soldiers circle their own tile (no heading) and fade
  for (const dep of departures) {
    if (!seen(fogOf(dep.p, dep.q)) && dep.faction !== viewerFaction) continue;
    const c = hexCentre(dep.p, dep.q, dep.tile);
    const n = 5;
    for (let i = 0; i < n; i++) {
      const a = t * 0.35 + (i / n) * Math.PI * 2;
      list.push({ x: c.x + Math.cos(a) * RADIUS * 0.5, y: c.y + Math.sin(a) * RADIUS * 0.5 * FLATTEN + RADIUS * 0.1, big: true, pose: 'march', cloth: shade(FACTION_FILL[dep.faction] ?? '#8a8a80', -0.05),
        trim: FACTION_DARK[dep.faction], skin: SKIN[i % SKIN.length], step: (t * 1.4 + i * 0.37) % 1, face: Math.sin(a) >= 0 ? -1 : 1 });
    }
    list.push({ banner: true, big: true, x: c.x, y: c.y + RADIUS * 0.12, faction: dep.faction });
  }
  // scouts on explored tiles
  for (const ex of explores) for (const idx of ex.tiles ?? []) {
    const c = hexCentre(ex.p, ex.q, idx);
    const a = t * 0.2 + hash(ex.p, idx) * 6.28;
    list.push({ x: c.x + Math.cos(a) * RADIUS * 0.25, y: c.y + Math.sin(a) * RADIUS * 0.25 * FLATTEN + RADIUS * 0.15, big: true, pose: 'scout', cloth: FACTION_FILL[ex.faction] ?? '#6a6a5e', trim: FACTION_DARK[ex.faction] ?? '#3a3a34', skin: SKIN[3], step: (t * 1.0) % 1, face: Math.cos(a + 1.57) >= 0 ? 1 : -1 });
  }
  // back to front, within the budget (columns and scouts first, they matter most)
  const kept = list.length > PEOPLE_BUDGET ? [...list.filter(x => x.big || x.kind !== 'folk'), ...list.filter(x => !x.big && x.kind === 'folk')].slice(0, PEOPLE_BUDGET) : list;
  kept.sort((a, b) => a.y - b.y);
  const S = s * ACTOR_SCALE;
  for (const f of kept) {
    if (f.banner) banner(ctx, f.x, f.y, f.big ? S * 0.8 : s, f.faction, t);
    else if (f.fire) campfire(ctx, f.x, f.y, f.big ? S * 0.6 : s, t);
    else if (f.spark) spark(ctx, f.x, f.y, f.big ? S : s);
    else if (f.big) actor(ctx, f.x, f.y, S, f);
    else if (f.kind === 'sitter') sitter(ctx, f.x, f.y, s, f);
    else figure(ctx, f.x, f.y, s, f);
  }
  // the column's only public fact besides its origin: the arrival bell (`columnLabel(dep)` gives the text)
  if (columnLabel && full) {
    const k = 1 / zoom;
    ctx.save();
    ctx.font = `600 ${11 * k}px system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif`;
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    for (const dep of departures) {
      const text = columnLabel(dep);
      if (!text) continue;
      const c = hexCentre(dep.p, dep.q, dep.tile);
      const w = ctx.measureText(text).width + 14 * k, h = 17 * k, x = c.x - w / 2, y = c.y + RADIUS * 0.5;
      ctx.fillStyle = FACTION_DARK[dep.faction] ?? '#55554e'; ctx.globalAlpha = 0.92;
      ctx.beginPath(); ctx.roundRect?.(x, y, w, h, 8 * k); ctx.fill();
      ctx.globalAlpha = 1; ctx.fillStyle = '#fffaf0'; ctx.fillText(text, c.x, y + h / 2 + 0.5 * k);
    }
    ctx.restore();
  }
  return kept.length;
}

/**
 * Name tags over holdings: the holder's portrait and name. `nameOf(p, q,
 * site)` → {name, identity, faction} | null (the roster through
 * identity.mjs). Drawn screen-sized; at most NAME_TAG_MAX, the nearest to
 * `centre` (world px) first. `onImage` redraws when a portrait decodes.
 */
export function paintNameTags(ctx, { tiles = [], zoom = 1, nameOf = () => null, centre = null, onImage = () => {}, fogOf = () => 'clear' } = {}) {
  if (RADIUS * zoom < NAME_TAG_MIN_R) return 0;
  const all = RADIUS * zoom >= NAME_TAG_ALL_R;
  let list = tiles.filter(u => u.state === 1 && u.owner < 6 && u.site !== undefined && (all || u.tier >= 2) && (u.fog ?? fogOf(u.p, u.pq)) !== 'unopened');
  if (centre) list = list.map(u => ({ u, d: (u.x - centre.x) ** 2 + (u.y - centre.y) ** 2 })).sort((a, b) => a.d - b.d).map(x => x.u);
  let n = 0;
  const k = 1 / zoom;
  ctx.save();
  ctx.font = `600 ${12 * k}px system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif`;
  ctx.textBaseline = 'middle';
  for (const u of list) {
    if (n >= NAME_TAG_MAX) break;
    const who = nameOf(u.p, u.pq, u.site, u.owner);
    if (!who) continue;
    const label = who.name;
    const w = ctx.measureText(label).width + 30 * k, h = 20 * k;
    const x = u.x - w / 2, y = u.y - RADIUS * 0.62 - h;
    ctx.globalAlpha = 0.94;
    ctx.fillStyle = '#fffaf0'; ctx.strokeStyle = FACTION_DARK[who.faction] ?? '#55554e'; ctx.lineWidth = 1.5 * k;
    ctx.beginPath(); ctx.roundRect?.(x, y, w, h, 10 * k); ctx.fill(); ctx.stroke();
    ctx.globalAlpha = 1;
    const img = avatarImage(who.identity, who.faction, onImage);
    if (img) ctx.drawImage(img, x + 2 * k, y + 2 * k, h - 4 * k, h - 4 * k);
    else { ctx.fillStyle = FACTION_FILL[who.faction] ?? '#8a8a80'; ctx.beginPath(); ctx.arc(x + h / 2, y + h / 2, h / 2 - 3 * k, 0, Math.PI * 2); ctx.fill(); }
    ctx.fillStyle = '#1b2e28'; ctx.textAlign = 'left';
    ctx.fillText(label, x + h + 4 * k, y + h / 2 + 0.5 * k);
    n++;
  }
  ctx.restore();
  return n;
}

/** Status badges from this many screen px of hex radius (Civ's unit flags). */
export const BADGE_MIN_R = 26;

/**
 * One badge per tile with an activity (the first kind is the badge; a
 * "+n" dot counts the rest), at the tile's upper right, screen sized.
 * Battles pulse. Returns the badges drawn.
 */
export function paintBadges(ctx, { tiles = [], zoom = 1, activities = null, t = (globalThis.performance?.now?.() ?? Date.now()) / 1000 } = {}) {
  if (!activities || RADIUS * zoom < BADGE_MIN_R) return 0;
  let n = 0;
  for (const u of tiles) {
    const acts = activities.get(`${u.p},${u.pq},${u.idx}`);
    if (!acts?.length || u.fog === 'unopened' || u.fog === 'distant') continue;
    const top = acts[0];
    const kinds = new Set(acts.map(a => a.kind));
    const f = top.faction < 6 ? top.faction : null;
    paintBadge(ctx, u.x + RADIUS * 0.5, u.y - RADIUS * 0.42, top.kind, {
      zoom, r: 10, more: kinds.size - 1, fill: f === null ? '#5b3b2a' : FACTION_FILL[f], dark: f === null ? '#2a1a12' : FACTION_DARK[f],
      pulse: top.kind === 'battle' ? (t * 1.2) % 1 : 0,
    });
    n++;
  }
  return n;
}
