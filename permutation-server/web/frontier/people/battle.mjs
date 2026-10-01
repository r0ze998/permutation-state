// The battle scene (design session: "make battles move"): a clash of one
// province and bell, played as a few seconds on the map when the bell
// resolves it (or on demand: the inspector, the report, the replay page).
//
//   1  0.0–1.2 s  the revealed arrivals come out of the mist on the tile
//                 they arrived at — never from a direction: a sealed march
//                 shows no route, only where it was revealed
//   2  1.2–2.2 s  each side takes its stance: Assault a wedge forward,
//                 Flank split to the wings, Brace a tight spear line, Hold
//                 a block; defenders stand on their tile
//   3  2.2–4.0 s  the melee: swords, sparks, dust
//   4  4.0–5.6 s  the fates (ClashInputs): Stays raises the banner,
//                 Retreated walks off unhurt, Bounced is pushed back,
//                 Withdrew steps aside to a friendly tile, Destroyed falls
//   5  5.6–7.0 s  the losses rise and fade ("−1,234")
//
// Everything comes from bytes the page decodes itself: the ClashInputs
// (arrivals: tile, stance, troops before and after, fate, owner tag), the
// Province before the bell (the report's province_before_b64: residents,
// garrisons, camp) and the Province after (the current envelope).
import { RADIUS, FLATTEN, project } from '../../map.mjs';
import { tileHex } from '../fgeo.mjs';
import { troopsOf } from '../fmarch.mjs';
import { FACTION_FILL, FACTION_DARK, shade } from './avatar.mjs';
import { zoomBoost } from './crowds.mjs';
import { baseDisc, unitFigure, UNIT_KINDS } from './units.mjs';

export const FATES = Object.freeze(['Stays', 'Withdrew', 'Bounced', 'Retreated', 'Destroyed']);
export const STANCE_POSE = Object.freeze(['hold', 'assault', 'flank', 'brace']);
/** The phases' start times (s) and the scene's length. */
export const PHASE = Object.freeze({ emerge: 0, deploy: 1.2, melee: 2.2, fates: 4.0, losses: 5.6, end: 7.0 });

const NEUTRAL = 6;

/**
 * The scene of a clash: `{p, q, bell, tiles: [{idx, attackers, defenders}]}`
 * with sides `[{id, faction, stance, before, after, fate, tag?, kind}]` in
 * whole troops. Tiles where arrivals fought; with no arrivals (hosts already
 * in the province fighting each other, a garrison or a camp — most clashes
 * of a long season), the tiles where sides met and someone lost troops
 * between the province before and after (`residentScene`); null if none.
 */
export function battleScene({ p, q, bell, inputs, before = null, after = null }) {
  const arrivals = (inputs?.arrivals ?? []).filter(a => a.present === 1 || a.present === true);
  if (!arrivals.length) return residentScene({ p, q, bell, before, after });
  const byTile = new Map();
  const tileOf = idx => { if (!byTile.has(idx)) byTile.set(idx, { idx, attackers: [], defenders: [] }); return byTile.get(idx); };
  for (const a of arrivals) {
    tileOf(a.tile).attackers.push({ id: String(a.hostId), faction: a.faction, unit: a.unit ?? 0, stance: a.stance ?? 0, before: troopsOf(a.troops), after: a.fate ? troopsOf(a.troopsAfter) : null,
      fate: FATES[a.fate - 1] ?? null, tag: a.citizenTag ?? null, kind: 'arrival' });
  }
  const alive = new Map((after?.entries ?? []).filter(e => e.state === 1).map(e => [String(e.id), e]));
  // without the province before the bell (an older report), the defenders are the ones there now: who
  // stood is known, their losses are not
  const standIn = !before && after;
  if (standIn) {
    for (const e of after.entries ?? []) {
      if (e.state !== 1 || !byTile.has(e.tile) || arrivals.some(a => String(a.hostId) === String(e.id))) continue;
      byTile.get(e.tile).defenders.push({ id: String(e.id), faction: e.faction, unit: e.unit ?? 0, stance: 0, before: troopsOf(e.troops), after: null, fate: 'Stays', kind: 'resident' });
    }
    const sitesNow = Array.from(after.sites ?? []);
    (after.siteMirror ?? []).forEach((m, j) => {
      if (!m || m.state !== 1 || !byTile.has(sitesNow[j]) || troopsOf(m.garrison) <= 0) return;
      byTile.get(sitesNow[j]).defenders.push({ id: `g${j}`, faction: m.faction, stance: 3, before: troopsOf(m.garrison), after: null, fate: 'Stays', kind: 'garrison' });
    });
  }
  for (const e of before?.entries ?? []) {
    if (e.state !== 1 || !byTile.has(e.tile)) continue;
    const now = alive.get(String(e.id));
    byTile.get(e.tile).defenders.push({ id: String(e.id), faction: e.faction, unit: e.unit ?? 0, stance: 0, before: troopsOf(e.troops), after: now ? troopsOf(now.troops) : (after ? 0 : null),
      fate: now ? 'Stays' : after ? 'Destroyed' : null, kind: 'resident' });
  }
  const sites = Array.from(before?.sites ?? []);
  (before?.siteMirror ?? []).forEach((m, j) => {
    if (!m || m.state !== 1 || !byTile.has(sites[j]) || troopsOf(m.garrison) <= 0) return;
    const a = after?.siteMirror?.[j];
    const left = a ? troopsOf(a.garrison) : null;
    byTile.get(sites[j]).defenders.push({ id: `g${j}`, faction: m.faction, stance: 3, before: troopsOf(m.garrison), after: left, fate: left === 0 ? 'Destroyed' : left === null ? null : 'Stays', kind: 'garrison' });
  });
  if (before?.camp?.state === 1 && byTile.has(before.camp.tile)) {
    const left = after ? (after.camp?.state === 1 ? Number(after.camp.troops) : 0) : null;
    byTile.get(before.camp.tile).defenders.push({ id: 'camp', faction: NEUTRAL, stance: 0, before: Number(before.camp.troops), after: left, fate: left === 0 ? 'Destroyed' : left === null ? null : 'Stays', kind: 'camp' });
  }
  return { p, q, bell, tiles: [...byTile.values()] };
}

/**
 * A clash of residents (no arrivals): needs both provinces. On each tile the
 * hosts, the garrison and the camp before the bell are the sides; a tile is
 * a battle when two sides met and troops were lost. The defenders are the
 * garrison's (or the camp's) side, else the faction holding the tile's
 * site, else the side that lost least; the rest attack.
 */
export function residentScene({ p, q, bell, before, after }) {
  if (!before || !after) return null;
  const alive = new Map((after.entries ?? []).filter(e => e.state === 1).map(e => [String(e.id), e]));
  const byTile = new Map();
  const at = idx => { if (!byTile.has(idx)) byTile.set(idx, []); return byTile.get(idx); };
  for (const e of before.entries ?? []) {
    if (e.state !== 1) continue;
    const now = alive.get(String(e.id));
    at(e.tile).push({ id: String(e.id), faction: e.faction, unit: e.unit ?? 0, stance: 0, before: troopsOf(e.troops), after: now ? troopsOf(now.troops) : 0, fate: now ? 'Stays' : 'Destroyed', kind: 'resident' });
  }
  const sites = Array.from(before.sites ?? []);
  const holderOf = new Map();
  (before.siteMirror ?? []).forEach((m, j) => {
    if (!m || m.state !== 1) return;
    holderOf.set(sites[j], m.faction);
    if (troopsOf(m.garrison) <= 0) return;
    const left = troopsOf(after.siteMirror?.[j]?.garrison ?? 0);
    at(sites[j]).push({ id: `g${j}`, faction: m.faction, stance: 3, before: troopsOf(m.garrison), after: left, fate: left === 0 ? 'Destroyed' : 'Stays', kind: 'garrison' });
  });
  if (before.camp?.state === 1) {
    const left = after.camp?.state === 1 ? Number(after.camp.troops) : 0;
    at(before.camp.tile).push({ id: 'camp', faction: NEUTRAL, stance: 0, before: Number(before.camp.troops), after: left, fate: left === 0 ? 'Destroyed' : 'Stays', kind: 'camp' });
  }
  const tiles = [];
  for (const [idx, list] of byTile) {
    const sides = new Set(list.map(x => x.faction));
    if (sides.size < 2 || !list.some(x => x.after < x.before)) continue;
    const held = list.find(x => x.kind === 'garrison' || x.kind === 'camp');
    let def = held ? held.faction : holderOf.has(idx) && sides.has(holderOf.get(idx)) ? holderOf.get(idx) : null;
    if (def === null) {
      const lost = new Map();
      for (const x of list) lost.set(x.faction, (lost.get(x.faction) ?? 0) + (x.before - x.after));
      def = [...lost.entries()].sort((a, b) => a[1] - b[1] || a[0] - b[0])[0][0];
    }
    tiles.push({ idx, attackers: list.filter(x => x.faction !== def), defenders: list.filter(x => x.faction === def) });
  }
  return tiles.length ? { p, q, bell, tiles, residents: true } : null;
}

/** The total losses of a side on a tile (whole troops; null when unknown). */
export const losses = list => (list.some(x => x.after === null) ? null : list.reduce((s, x) => s + Math.max(0, x.before - x.after), 0));

// ------------------------------------------------------------------ playing
const ease = k => (k <= 0 ? 0 : k >= 1 ? 1 : k * k * (3 - 2 * k));
const clamp01 = k => Math.max(0, Math.min(1, k));
const hash = (a, b) => { let h = Math.imul(a | 0, 0x27d4eb2d) ^ Math.imul(b | 0, 0x165667b1); h ^= h >>> 15; return (h >>> 0) / 4294967296; };

/** Playback speeds (UI plan D4, Civ's quick combat): the setting's values and their rates. */
export const BATTLE_SPEEDS = Object.freeze({ normal: 1, fast: 2.5, off: 0 });

/**
 * A playing scene: `{scene, t0, speed}`; `paintBattle(ctx, play, {zoom, now, lossText})`
 * draws it at time `now` (s) and returns false once it is over. `speed` > 1 plays it faster.
 */
export function startBattle(scene, now = (globalThis.performance?.now?.() ?? Date.now()) / 1000, speed = 1) {
  return scene ? { scene, t0: now, speed: speed > 0 ? speed : 1 } : null;
}

/** The scene's own time (s) at `now`. */
export const battleTime = (play, now) => (now - play.t0) * (play.speed ?? 1) + (play.scene?.residents ? PHASE.deploy : 0);   // residents were there all along: no emerging from the mist
/** Whether a scene is still playing at `now`. */
export const battleLive = (play, now) => !!play && battleTime(play, now) <= PHASE.end;
/** "P,Q,tile" of every tile a playing scene covers (the map hides the host sprites there). */
export function battleTiles(plays, now) {
  const out = new Set();
  for (const b of plays ?? []) if (battleLive(b, now)) for (const t of b.scene.tiles) out.add(`${b.scene.p},${b.scene.q},${t.idx}`);
  return out;
}

export function paintBattle(ctx, play, { zoom = 1, now = (globalThis.performance?.now?.() ?? Date.now()) / 1000, lossText = n => `−${n}`, fateText = null } = {}) {
  if (!play) return false;
  const t = battleTime(play, now);
  if (t > PHASE.end) return false;
  const s = RADIUS * 0.6 * zoomBoost(RADIUS * zoom);
  for (const tile of play.scene.tiles) {
    const h = tileHex(play.scene.p, play.scene.q, tile.idx);
    const c = project(h.q, h.r);
    const cy = c.y + RADIUS * 0.15;
    // mist the arrivals come out of (phase 1), dust over the melee (phase 3)
    const mist = 1 - clamp01((t - 0.6) / 0.8);
    if (mist > 0) for (let i = 0; i < 7; i++) {
      const a = (i / 7) * Math.PI * 2 + t * 0.4;
      ctx.globalAlpha = 0.45 * mist; ctx.fillStyle = '#eef1f4';
      ctx.beginPath(); ctx.ellipse(c.x - RADIUS * 0.4 + Math.cos(a) * RADIUS * 0.22, cy + Math.sin(a) * RADIUS * 0.12, RADIUS * 0.28, RADIUS * 0.14, 0, 0, Math.PI * 2); ctx.fill();
    }
    const dust = t >= PHASE.melee && t < PHASE.fates + 0.6 ? Math.sin(Math.PI * clamp01((t - PHASE.melee) / (PHASE.fates + 0.6 - PHASE.melee))) : 0;
    if (dust > 0) for (let i = 0; i < 6; i++) {
      ctx.globalAlpha = 0.28 * dust; ctx.fillStyle = '#c9b68c';
      ctx.beginPath(); ctx.arc(c.x + (hash(i, tile.idx) - 0.5) * RADIUS * 0.7, cy - RADIUS * 0.1 - hash(tile.idx, i) * RADIUS * 0.25, RADIUS * (0.12 + 0.1 * hash(i, 9)) * (1 + (t - PHASE.melee) * 0.15), 0, Math.PI * 2); ctx.fill();
    }
    ctx.globalAlpha = 1;
    // the sides as tokens (people/units.mjs): one per faction on each side, the garrison and the camp their own
    const groups = (list, sgn) => {
      const by = new Map();
      for (const w of list) {
        const key = `${w.kind === 'garrison' ? 'g' : w.kind === 'camp' ? 'c' : 'h'}${w.faction}`;
        const g = by.get(key) ?? { faction: w.faction, kind: w.kind, members: [], sgn };
        g.members.push(w); by.set(key, g);
      }
      return [...by.values()];
    };
    const left = groups(tile.attackers, -1), right = groups(tile.defenders, 1);
    const toks = [];
    const place = (list, sgn) => list.forEach((g, i) => {
      const main = [...g.members].sort((a, b) => b.before - a.before)[0];
      const kind = g.kind === 'camp' ? 'spearman' : g.kind === 'garrison' ? 'pikeman' : UNIT_KINDS[main.unit ?? 0] ?? 'spearman';
      const fate = g.members.find(m => m.fate && m.fate !== 'Stays')?.fate ?? (g.members.every(m => m.fate === 'Stays') ? 'Stays' : null);
      // deploy: arrivals come in from the side they were revealed on; defenders stand
      const dk = sgn < 0 ? ease(clamp01((t - PHASE.deploy) / 1.0)) : 1;
      let x = c.x + sgn * (0.36 + i * 0.16) * RADIUS + (sgn < 0 ? (1 - dk) * -0.35 * RADIUS : 0), y = cy + (i % 2 ? 0.16 : -0.02) * RADIUS * 1.4;
      let alpha = sgn < 0 ? clamp01((t - 0.2) / 0.9) : 1, fallen = 0, walking = sgn < 0 && t < PHASE.deploy + 1, face = -sgn;
      // the melee: the sides close in and strike
      const mk = t >= PHASE.melee ? ease(clamp01((t - PHASE.melee) / 0.5)) * (t < PHASE.fates ? 1 : 1 - ease(clamp01((t - PHASE.fates) / 0.8))) : 0;
      x -= sgn * mk * RADIUS * 0.2;
      const lunge = t >= PHASE.melee && t < PHASE.fates ? Math.max(0, Math.sin((t - PHASE.melee) * 9 + i + (sgn < 0 ? 0 : 1.7))) : 0;
      // the fates
      const fk = clamp01((t - PHASE.fates) / 1.4);
      if (fk > 0 && fate) {
        if (fate === 'Destroyed') fallen = ease(fk);
        else if (fate === 'Retreated' || fate === 'Withdrew') { x += sgn * ease(fk) * RADIUS * 0.7; alpha *= 1 - ease(fk) * 0.85; walking = true; face = sgn; }
        else if (fate === 'Bounced') { x += sgn * Math.sin(fk * Math.PI * 0.5) * RADIUS * 0.45; alpha *= 1 - ease(clamp01((fk - 0.5) * 2)) * 0.7; }
      }
      toks.push({ x, y, alpha, fallen, walking, face, lunge, kind, faction: g.faction, fate, ranged: kind === 'archer' || kind === 'crossbowman', sgn });
    });
    place(left, -1); place(right, 1);
    toks.sort((a, b) => a.y - b.y);
    for (const k of toks) {
      if (k.alpha <= 0.02) continue;
      const step = (t * (k.walking ? 1.6 : 0.6)) % 1;
      if (k.fallen) {
        ctx.save(); ctx.globalAlpha = k.alpha * (1 - k.fallen * 0.5);
        baseDisc(ctx, k.x, k.y, s, k.faction, { alpha: k.alpha * (1 - k.fallen * 0.6) });
        ctx.translate(k.x, k.y); ctx.rotate(-k.face * k.fallen * 1.4);
        unitFigure(ctx, 0, 0, s, k.kind, { faction: k.faction, face: k.face, step: 0.2, alpha: 1 });
        ctx.restore();
        // a skull over the fallen
        if (k.fallen > 0.6) { ctx.save(); ctx.globalAlpha = (k.fallen - 0.6) * 2.5; ctx.fillStyle = '#f4efe4'; ctx.strokeStyle = '#2a2018'; ctx.lineWidth = s * 0.03;
          const sx = k.x, sy = k.y - s * 0.9; ctx.beginPath(); ctx.arc(sx, sy, s * 0.13, 0, Math.PI * 2); ctx.fill(); ctx.stroke(); ctx.fillRect(sx - s * 0.07, sy + s * 0.08, s * 0.14, s * 0.08);
          ctx.fillStyle = '#2a2018'; ctx.beginPath(); ctx.arc(sx - s * 0.05, sy, s * 0.03, 0, Math.PI * 2); ctx.arc(sx + s * 0.05, sy, s * 0.03, 0, Math.PI * 2); ctx.fill(); ctx.restore(); }
        continue;
      }
      baseDisc(ctx, k.x, k.y, s, k.faction, { alpha: k.alpha });
      unitFigure(ctx, k.x, k.y - s * 0.02, s, k.kind, { faction: k.faction, face: k.face, step, walking: k.walking, alpha: k.alpha, lunge: k.ranged ? 0 : k.lunge });
      // archers and crossbowmen loose: arrows arc to the other side
      if (k.ranged && t >= PHASE.melee && t < PHASE.fates) for (let j = 0; j < 3; j++) {
        const u = ((t - PHASE.melee) * 1.4 + j / 3) % 1;
        const tx = c.x - k.sgn * 0.3 * RADIUS, ax = k.x + (tx - k.x) * u, ay = k.y - s * 0.6 - Math.sin(u * Math.PI) * s * 0.5;
        const dx = (tx - k.x), dy = -Math.cos(u * Math.PI) * s * 0.5 * Math.PI, len = Math.hypot(dx, dy) || 1;
        ctx.save(); ctx.globalAlpha = k.alpha; ctx.strokeStyle = '#3a2a1a'; ctx.lineWidth = s * 0.02;
        ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(ax - dx / len * s * 0.16, ay - dy / len * s * 0.16); ctx.stroke(); ctx.restore();
      }
    }
    // sparks in the melee
    if (t >= PHASE.melee + 0.3 && t < PHASE.fates) for (let i = 0; i < 3; i++) {
      const k = (t * 3 + i * 0.33) % 1;
      if (k > 0.25) continue;
      const x = c.x + (hash(i, Math.floor(t * 3)) - 0.5) * RADIUS * 0.5, y = cy - s * 0.7 + (hash(Math.floor(t * 3), i) - 0.5) * RADIUS * 0.3;
      ctx.strokeStyle = '#ffe08a'; ctx.lineWidth = s * 0.05;
      ctx.beginPath(); for (let r = 0; r < 6; r++) { const a = r * Math.PI / 3; ctx.moveTo(x + Math.cos(a) * s * 0.05, y + Math.sin(a) * s * 0.05); ctx.lineTo(x + Math.cos(a) * s * (0.12 + k * 0.5), y + Math.sin(a) * s * (0.12 + k * 0.5)); } ctx.stroke();
    }
    // the winner's banner
    const won = [...tile.attackers, ...tile.defenders].find(x => x.fate === 'Stays' && x.faction !== NEUTRAL);
    if (won && t >= PHASE.fates + 0.6) {
      const k = ease(clamp01((t - PHASE.fates - 0.6) / 0.6));
      const bx = c.x, by = cy - RADIUS * 0.05;
      ctx.strokeStyle = '#5b4632'; ctx.lineWidth = s * 0.06;
      ctx.beginPath(); ctx.moveTo(bx, by); ctx.lineTo(bx, by - s * 1.6 * k); ctx.stroke();
      ctx.fillStyle = FACTION_FILL[won.faction]; ctx.strokeStyle = FACTION_DARK[won.faction]; ctx.lineWidth = s * 0.04;
      const wv = Math.sin(t * 5) * s * 0.06;
      ctx.beginPath(); ctx.moveTo(bx, by - s * 1.6 * k); ctx.lineTo(bx + s * 0.75 * k, by - s * 1.45 * k + wv); ctx.lineTo(bx + s * 0.66 * k, by - s * 1.18 * k + wv); ctx.lineTo(bx, by - s * 1.08 * k); ctx.closePath(); ctx.fill(); ctx.stroke();
    }
    // losses rise and fade
    if (t >= PHASE.losses - 0.4) {
      const k = clamp01((t - PHASE.losses + 0.4) / 1.6);
      ctx.save();
      ctx.globalAlpha = 1 - ease(clamp01((k - 0.6) / 0.4));
      ctx.font = `800 ${RADIUS * 0.2}px system-ui, sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      for (const [list, dx, dy] of [[tile.attackers, -0.62, 0], [tile.defenders, 0.62, 0.18]]) {
        const n = losses(list);
        if (!n) continue;
        const f = list[0]?.faction;
        const text = lossText(n), w = ctx.measureText(text).width + RADIUS * 0.2, h = RADIUS * 0.3;
        const x = c.x + dx * RADIUS, y = cy - RADIUS * (0.6 + dy + k * 0.3);
        ctx.fillStyle = f === NEUTRAL ? '#4a3626' : FACTION_DARK[f] ?? '#3a3a34';
        ctx.beginPath(); ctx.roundRect?.(x - w / 2, y - h / 2, w, h, h / 2); ctx.fill();
        ctx.fillStyle = '#fff4ee'; ctx.fillText(text, x, y + RADIUS * 0.01);
        // the side's fate in a word (Bounced, Destroyed…), under its losses
        const fate = list.find(z => z.fate && z.fate !== 'Stays')?.fate ?? (list.every(z => z.fate === 'Stays') ? 'Stays' : null);
        const ft = fate && fateText ? fateText(fate) : null;
        if (ft) {
          ctx.save(); ctx.font = `700 ${RADIUS * 0.14}px system-ui, sans-serif`;
          ctx.lineWidth = RADIUS * 0.04; ctx.strokeStyle = 'rgba(20,16,12,.85)'; ctx.strokeText(ft, x, y + h * 0.95);
          ctx.fillStyle = '#fff4ee'; ctx.fillText(ft, x, y + h * 0.95); ctx.restore();
        }
      }
      ctx.restore();
    }
  }
  ctx.globalAlpha = 1;
  return true;
}
