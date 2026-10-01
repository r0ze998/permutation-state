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
  } else if (kind === 'carrier') {
    ctx.fillStyle = '#c9a66a'; ctx.strokeStyle = '#7a5a2a'; ctx.lineWidth = s * 0.03;
    ctx.beginPath(); ctx.ellipse(x - face * s * 0.1, top + s * 0.06, s * 0.16, s * 0.11, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  } else if (kind === 'scout') {
    ctx.fillStyle = dark; ctx.beginPath(); ctx.arc(x, top - s * 0.1, s * 0.13, Math.PI * 1.05, -0.05); ctx.fill();
  }
  ctx.globalAlpha = 1;
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
export function paintPeople(ctx, { tiles = [], zoom = 1, t = (globalThis.performance?.now?.() ?? Date.now()) / 1000, departures = [], explores = [], fogOf = () => 'clear', viewerFaction = null, columnLabel = null } = {}) {
  const r = RADIUS * zoom;
  if (r < PEOPLE_MIN_R) return 0;
  const s = RADIUS * 0.15;
  const full = r >= PEOPLE_FULL_R;
  const seen = f => f === 'clear' || f === 'sight' || f === undefined;
  const list = [];
  // residents and gatherers around the holdings
  for (const u of tiles) {
    if (u.state !== 1 || !(u.owner < 6) || !seen(u.fog ?? fogOf(u.p, u.pq))) continue;
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
  // departing columns: soldiers circle their own tile (no heading) and fade
  for (const dep of departures) {
    if (!seen(fogOf(dep.p, dep.q)) && dep.faction !== viewerFaction) continue;
    const c = hexCentre(dep.p, dep.q, dep.tile);
    const n = Math.max(4, Math.min(12, Math.ceil((dep.troops ?? 500) / 400)));
    for (let i = 0; i < n; i++) {
      const a = t * 0.35 + (i / n) * Math.PI * 2;
      list.push({ x: c.x + Math.cos(a) * RADIUS * 0.42, y: c.y + Math.sin(a) * RADIUS * 0.42 * FLATTEN + RADIUS * 0.1, kind: 'soldier', cloth: shade(FACTION_FILL[dep.faction] ?? '#8a8a80', -0.1),
        trim: FACTION_DARK[dep.faction], skin: SKIN[i % SKIN.length], step: (t * 2 + i * 0.37) % 1, face: Math.sin(a) >= 0 ? -1 : 1, alpha: 0.95 });
    }
    list.push({ banner: true, x: c.x, y: c.y + RADIUS * 0.12, faction: dep.faction });
  }
  // scouts on explored tiles
  for (const ex of explores) for (const idx of ex.tiles ?? []) {
    const c = hexCentre(ex.p, ex.q, idx);
    const a = t * 0.2 + hash(ex.p, idx) * 6.28;
    list.push({ x: c.x + Math.cos(a) * RADIUS * 0.25, y: c.y + Math.sin(a) * RADIUS * 0.25 * FLATTEN + RADIUS * 0.1, kind: 'scout', cloth: FACTION_FILL[ex.faction] ?? '#6a6a5e', skin: SKIN[3], step: (t * 1.2) % 1, face: Math.cos(a + 1.57) >= 0 ? 1 : -1 });
  }
  // back to front, within the budget (columns and scouts first, they matter most)
  const kept = list.length > PEOPLE_BUDGET ? [...list.filter(x => x.kind !== 'folk'), ...list.filter(x => x.kind === 'folk')].slice(0, PEOPLE_BUDGET) : list;
  kept.sort((a, b) => a.y - b.y);
  for (const f of kept) {
    if (f.banner) banner(ctx, f.x, f.y, s, f.faction, t);
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
