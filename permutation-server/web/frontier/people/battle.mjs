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
import { SKIN } from './identity.mjs';
import { actor, ACTOR_SCALE } from './crowds.mjs';

export const FATES = Object.freeze(['Stays', 'Withdrew', 'Bounced', 'Retreated', 'Destroyed']);
export const STANCE_POSE = Object.freeze(['hold', 'assault', 'flank', 'brace']);
/** The phases' start times (s) and the scene's length. */
export const PHASE = Object.freeze({ emerge: 0, deploy: 1.2, melee: 2.2, fates: 4.0, losses: 5.6, end: 7.0 });

const NEUTRAL = 6;

/**
 * The scene of a clash: `{p, q, bell, tiles: [{idx, attackers, defenders}]}`
 * with sides `[{id, faction, stance, before, after, fate, tag?, kind}]` in
 * whole troops. Only tiles where arrivals fought; null if none did.
 */
export function battleScene({ p, q, bell, inputs, before = null, after = null }) {
  const arrivals = (inputs?.arrivals ?? []).filter(a => a.present === 1 || a.present === true);
  if (!arrivals.length) return null;
  const byTile = new Map();
  const tileOf = idx => { if (!byTile.has(idx)) byTile.set(idx, { idx, attackers: [], defenders: [] }); return byTile.get(idx); };
  for (const a of arrivals) {
    tileOf(a.tile).attackers.push({ id: String(a.hostId), faction: a.faction, stance: a.stance ?? 0, before: troopsOf(a.troops), after: a.fate ? troopsOf(a.troopsAfter) : null,
      fate: FATES[a.fate - 1] ?? null, tag: a.citizenTag ?? null, kind: 'arrival' });
  }
  const alive = new Map((after?.entries ?? []).filter(e => e.state === 1).map(e => [String(e.id), e]));
  for (const e of before?.entries ?? []) {
    if (e.state !== 1 || !byTile.has(e.tile)) continue;
    const now = alive.get(String(e.id));
    byTile.get(e.tile).defenders.push({ id: String(e.id), faction: e.faction, stance: 0, before: troopsOf(e.troops), after: now ? troopsOf(now.troops) : (after ? 0 : null),
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

/** The total losses of a side on a tile (whole troops; null when unknown). */
export const losses = list => (list.some(x => x.after === null) ? null : list.reduce((s, x) => s + Math.max(0, x.before - x.after), 0));

// ------------------------------------------------------------------ playing
const ease = k => (k <= 0 ? 0 : k >= 1 ? 1 : k * k * (3 - 2 * k));
const clamp01 = k => Math.max(0, Math.min(1, k));
const hash = (a, b) => { let h = Math.imul(a | 0, 0x27d4eb2d) ^ Math.imul(b | 0, 0x165667b1); h ^= h >>> 15; return (h >>> 0) / 4294967296; };

/** Where a side's figures stand in a stance, relative to the tile centre (units of RADIUS); side −1 attackers, +1 defenders. */
function formation(stance, side, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const row = Math.floor(i / 3), col = (i % 3) - 1;
    let x = side * (0.34 + row * 0.16), y = col * 0.26;
    if (side < 0) {
      if (stance === 1) { x = side * (0.2 + Math.abs(col) * 0.14 + row * 0.12); }           // assault: a wedge, point forward
      else if (stance === 2) { y = (col === 0 ? (i % 2 ? 1 : -1) : col) * 0.42; x = side * (0.18 + row * 0.12); } // flank: the wings
      else if (stance === 3) { x = side * (0.42 + row * 0.08); y = col * 0.18; }               // brace: a tight line
    }
    out.push([x, y]);
  }
  return out;
}

/**
 * A playing scene: `{scene, t0}`; `paintBattle(ctx, play, {zoom, now, lossText})`
 * draws it at time `now` (s) and returns false once it is over.
 */
export function startBattle(scene, now = (globalThis.performance?.now?.() ?? Date.now()) / 1000) {
  return scene ? { scene, t0: now } : null;
}

export function paintBattle(ctx, play, { zoom = 1, now = (globalThis.performance?.now?.() ?? Date.now()) / 1000, lossText = n => `−${n}`, fateText = null } = {}) {
  if (!play) return false;
  const t = now - play.t0;
  if (t > PHASE.end) return false;
  const s = RADIUS * 0.15 * ACTOR_SCALE * 0.9;
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
    const figures = [];
    const side = (list, sgn) => {
      const n = Math.min(6, Math.max(2, list.length * 2));
      const stance = list[0]?.stance ?? 0;
      const fpos = formation(sgn < 0 ? stance : 0, sgn, n);
      fpos.forEach(([fx, fy], i) => {
        const who = list[i % list.length];
        const f = who.faction;
        const cloth = f === NEUTRAL ? '#6b5440' : shade(FACTION_FILL[f] ?? '#8a8a80', -0.05), trim = f === NEUTRAL ? '#3a2a1e' : FACTION_DARK[f] ?? '#3a3a34';
        // deploy: from a huddle at the tile's centre (arrivals) into the stance
        const dk = sgn < 0 ? ease(clamp01((t - PHASE.deploy) / 1.0)) : 1;
        let x = c.x + (sgn < 0 ? fx * dk + (1 - dk) * -0.3 : fx) * RADIUS, y = cy + fy * RADIUS * FLATTEN * (sgn < 0 ? dk : 1);
        // melee: the lines close in
        const mk = t >= PHASE.melee ? ease(clamp01((t - PHASE.melee) / 0.5)) * (t < PHASE.fates ? 1 : 1 - ease(clamp01((t - PHASE.fates) / 0.8))) : 0;
        x -= sgn * mk * RADIUS * 0.16;
        let alpha = sgn < 0 ? clamp01((t - 0.2) / 0.9) : 1, fallen = 0;
        // fates
        const fk = clamp01((t - PHASE.fates) / 1.4);
        const fate = who.fate;
        if (fk > 0 && fate) {
          const lost = who.before > 0 && who.after !== null ? 1 - who.after / who.before : 0;
          if (fate === 'Destroyed' || (i / n) < lost * 0.9) { fallen = ease(fk); }
          else if (fate === 'Retreated' || fate === 'Withdrew') { x += sgn * ease(fk) * RADIUS * 0.6; alpha *= 1 - ease(fk) * 0.8; }
          else if (fate === 'Bounced') { x += sgn * Math.sin(fk * Math.PI * 0.5) * RADIUS * 0.35; alpha *= 1 - ease(clamp01((fk - 0.5) * 2)) * 0.7; }
        }
        const fighting = t >= PHASE.melee && t < PHASE.fates + 0.4 && !fallen;
        figures.push({ x, y, alpha, fallen, cloth, trim, skin: SKIN[(i + tile.idx) % SKIN.length], face: sgn < 0 ? 1 : -1,
          pose: fighting ? 'sword' : sgn < 0 && t < PHASE.deploy + 0.8 ? 'march' : (who.stance === 3 || who.kind === 'garrison') ? 'spear' : 'spear',
          step: fighting ? (t * 2.2 + hash(i, sgn + 3)) % 1 : (t * 1.4 + i * 0.3) % 1, winner: fate === 'Stays' && t >= PHASE.fates + 0.6, faction: f });
      });
    };
    side(tile.attackers, -1);
    side(tile.defenders, 1);
    figures.sort((a, b) => a.y - b.y);
    for (const f of figures) {
      if (f.alpha <= 0.02) continue;
      if (f.fallen) {
        ctx.save(); ctx.translate(f.x, f.y); ctx.rotate(f.face * f.fallen * 1.35); ctx.globalAlpha = f.alpha * (1 - f.fallen * 0.35);
        actor(ctx, 0, 0, s, { ...f, pose: 'sword', step: 0.2 });
        ctx.restore();
      } else actor(ctx, f.x, f.y, s, f);
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
