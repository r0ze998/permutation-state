// What each holding and host is doing now (design session: "like
// Civilization, you can tell what a character is doing"). Read only from
// public state — the Province (site mirror, entries, camp), the overview
// flags, the chronicle (DEPART, EXPLORE) — plus, for the viewer's own
// holdings, their Holding accounts (build queue). A hidden fact stays
// hidden: a sealed march has no destination here, a resident's stance is
// shown only once a clash revealed it.
//
//   tileActivities(entry, ctx) → Map(tile idx → [activity])
//   activity = {kind, faction, until?, n?, item?, host?}
//
// Kinds, in the order a badge shows them (the first is the badge):
//   battle     a clash this bell in the province, on a tile with hosts
//   depart     a host left this tile and is on the road (arrival bell only)
//   muster     a new host forms up here and joins the roster next bell
//   walls      walls are being raised (done at `until`)
//   build      the viewer's own build queue (item, done at `until`)
//   recruit    the garrison is being reinforced (from `until`)
//   explore    scouts on this tile (an Explore revealed it)
//   rest       a host recovering stamina (ready at `until`)
//   guard      a host holding the tile (the stance, when known, as `stance`)
//   garrison   the holding's garrison stands on the walls (n troops)
//   shield     the holding is protected (until bell `until`)
//   dormant    the holding is dormant (its lord has been away)
//   camp       a barbarian camp
import { L, fmtNum } from '../../lang.mjs';
import { BUILDINGS, factionName } from '../fi18n.mjs';
import { BUILD_ITEMS } from '../fland.mjs';
import { RULES, staminaAt, DEPART_STAMINA, troopsOf } from '../fmarch.mjs';

export const NO_BELL = 0xffffffff;
export const ACTIVITY_ORDER = Object.freeze(['battle', 'depart', 'muster', 'walls', 'build', 'recruit', 'explore', 'rest', 'guard', 'garrison', 'shield', 'dormant', 'camp']);
const rank = k => ACTIVITY_ORDER.indexOf(k);

/**
 * The activities of one drawn province. `e` is a sprites.mjs entry ({p, q,
 * prov, rec, sites, clash?}); `ctx`: {bell, departures, explores, own}
 * with `own` the viewer's Holding accounts.
 */
export function tileActivities(e, { bell = 0, departures = [], explores = [], own = [] } = {}) {
  const out = new Map();
  const add = (idx, a) => { if (!Number.isInteger(idx)) return; if (!out.has(idx)) out.set(idx, []); out.get(idx).push(a); };
  const prov = e.prov ?? null;
  const sites = Array.from(prov?.sites ?? e.sites ?? []);
  const fought = e.clash?.arrivals ? new Set(e.clash.arrivals.filter(a => a.present).map(a => a.tile)) : null;
  const stanceOf = new Map();
  for (const a of e.clash?.arrivals ?? []) if (a.present) stanceOf.set(String(a.hostId), a.stance);
  // sites: walls, garrison, recruits, shield, dormant
  (prov?.siteMirror ?? []).forEach((m, j) => {
    if (!m || m.state !== 1 || j >= sites.length) return;
    const idx = sites[j], f = m.faction;
    for (const [b, d] of [[m.wallItem0Bell, m.wallItem0Delta], [m.wallItem1Bell, m.wallItem1Delta]]) {
      if (b !== undefined && b !== NO_BELL && b > bell && Number(d) > 0) { add(idx, { kind: 'walls', faction: f, until: b }); break; }
    }
    const pend = [[m.pend0Bell, m.pend0Delta], [m.pend1Bell, m.pend1Delta]].find(([b, d]) => b !== undefined && b !== NO_BELL && b > bell && Number(d) > 0);
    if (pend) add(idx, { kind: 'recruit', faction: f, until: pend[0], n: troopsOf(pend[1]) });
    if (troopsOf(m.garrison) > 0) add(idx, { kind: 'garrison', faction: f, n: troopsOf(m.garrison) });
    if (m.shieldUntilBell > bell && m.shieldUntilBell !== NO_BELL) add(idx, { kind: 'shield', faction: f, until: m.shieldUntilBell });
  });
  if (e.rec?.dormant) for (const [j, idx] of sites.entries()) if (e.rec.sites?.[j] === 1 && e.rec.owners?.[j] < 6) add(idx, { kind: 'dormant', faction: e.rec.owners[j] });
  // hosts: muster, rest, guard; a clash this bell on their tile
  for (const h of prov?.entries ?? []) {
    if (h.state === 2) { add(h.tile, { kind: 'muster', faction: h.faction, until: bell + 1, host: String(h.id), n: troopsOf(h.troops) }); continue; }
    if (h.state !== 1) continue;
    const st = staminaAt(h, bell);
    if (h.readyBell > bell || st < DEPART_STAMINA) add(h.tile, { kind: 'rest', faction: h.faction, until: Math.max(h.readyBell, bell + Math.max(0, DEPART_STAMINA - st)), host: String(h.id), n: troopsOf(h.troops) });
    else add(h.tile, { kind: 'guard', faction: h.faction, host: String(h.id), n: troopsOf(h.troops), stance: stanceOf.get(String(h.id)) ?? null });
    // a clash this bell: on the tiles its arrivals fought on once the clash report is loaded, else on every host tile
    if (e.rec?.clash && (!fought || fought.has(h.tile))) add(h.tile, { kind: 'battle', faction: h.faction });
  }
  if (prov?.camp?.state === 1) add(prov.camp.tile, { kind: 'camp', faction: 6, n: troopsOf(prov.camp.troops) });
  for (const d of departures) if (d.p === e.p && d.q === e.q) add(d.tile, { kind: 'depart', faction: d.faction, until: d.arriveBell, host: d.host, n: d.troops });
  for (const x of explores) if (x.p === e.p && x.q === e.q) for (const idx of x.tiles ?? []) add(idx, { kind: 'explore', faction: x.faction });
  // the viewer's own build queue
  for (const h of own) {
    if (h.p !== e.p || h.q !== e.q) continue;
    for (const q of h.queue ?? []) {
      const done = Number(q.doneAt ?? 0);
      if (done > 0) add(h.tile, { kind: 'build', faction: h.faction, item: q.kind, untilTs: done });
    }
  }
  for (const list of out.values()) {
    list.sort((a, b) => rank(a.kind) - rank(b.kind));
    // one battle badge per tile
    const i = list.findIndex(a => a.kind === 'battle');
    if (i >= 0) for (let k = list.length - 1; k > i; k--) if (list[k].kind === 'battle') list.splice(k, 1);
  }
  return out;
}

/** Activities of every drawn province, keyed "P,Q,idx" (sprites.mjs keeps the last frame's for the hover tip). */
export function activitiesFor(entries, ctx) {
  const all = new Map();
  for (const e of entries) {
    if (e.fog === 'unopened') continue;
    for (const [idx, list] of tileActivities(e, ctx)) all.set(`${e.p},${e.q},${idx}`, list);
  }
  return all;
}

/** The badge's short label ("城壁 第1,040鐘まで"). */
export function activityText(a) {
  const until = Number.isInteger(a.until) ? fmtNum(a.until) : null;
  switch (a.kind) {
    case 'battle': return L`戦闘中`;
    case 'depart': return L`出陣中（第${until}鐘に到着、行き先は秘密）`;
    case 'muster': return L`編成中（第${until}鐘から加わる）`;
    case 'walls': return L`城壁を建設中（第${until}鐘に完成）`;
    case 'build': return L`${BUILDINGS[BUILD_ITEMS[a.item]?.resource] ?? L`建物`}を建設中`;
    case 'recruit': return L`守備隊を増員中（第${until}鐘から ${fmtNum(a.n ?? 0)} 兵）`;
    case 'explore': return L`探索中`;
    case 'rest': return L`休息中（第${until}鐘に回復）`;
    case 'guard': return L`駐留中（${fmtNum(a.n ?? 0)} 兵）`;
    case 'garrison': return L`守備隊 ${fmtNum(a.n ?? 0)} 兵`;
    case 'shield': return L`保護中（第${until}鐘まで攻撃されない）`;
    case 'dormant': return L`休眠中（領主が留守）`;
    case 'camp': return L`蛮族の野営地（${fmtNum(a.n ?? 0)} 兵）`;
    default: return '';
  }
}

/** One line for a tooltip: "<faction>: <activities>". */
export function tileSummary(list) {
  if (!list?.length) return '';
  const f = list.find(a => a.faction < 6)?.faction;
  const body = list.filter((a, i, all) => all.findIndex(b => b.kind === a.kind) === i).map(activityText).join(' · ');
  return f === undefined ? body : L`${factionName(f)}：${body}`;
}

// ------------------------------------------------------------------ the badge (canvas)
/** Badge colours by kind: the ring says how urgent it is (red fighting, amber moving or building, green calm). */
const RING = { battle: '#c8412f', depart: '#b07a12', muster: '#b07a12', walls: '#b07a12', build: '#b07a12', recruit: '#b07a12', explore: '#2e6f9e',
  rest: '#6a7b88', guard: '#2e7d4f', garrison: '#2e7d4f', shield: '#3f78c2', dormant: '#8a8a80', camp: '#5b3b2a' };

/** Draw the glyph of a kind in a unit box centred on (0, 0) (|x|, |y| ≤ 1). */
function glyph(ctx, kind) {
  ctx.lineCap = 'round'; ctx.lineJoin = 'round';
  const line = (pts, w = 0.18) => { ctx.lineWidth = w; ctx.beginPath(); pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y))); ctx.stroke(); };
  switch (kind) {
    case 'battle': line([[-0.6, -0.6], [0.6, 0.6]]); line([[0.6, -0.6], [-0.6, 0.6]]); line([[-0.75, -0.35], [-0.35, -0.75]], 0.14); line([[0.75, -0.35], [0.35, -0.75]], 0.14); break;
    case 'depart': line([[-0.45, 0.75], [-0.45, -0.75]], 0.14); ctx.beginPath(); ctx.moveTo(-0.45, -0.75); ctx.lineTo(0.6, -0.45); ctx.lineTo(-0.45, -0.1); ctx.closePath(); ctx.fill(); break;
    case 'muster': for (const x of [-0.5, 0, 0.5]) { ctx.beginPath(); ctx.arc(x, -0.35, 0.17, 0, 7); ctx.fill(); line([[x, -0.1], [x, 0.6]], 0.2); } break;
    case 'walls': case 'build': line([[-0.55, 0.65], [0.35, -0.25]], 0.2); ctx.beginPath(); ctx.moveTo(0.05, -0.6); ctx.lineTo(0.7, 0.05); ctx.lineTo(0.45, 0.3); ctx.lineTo(-0.2, -0.35); ctx.closePath(); ctx.fill(); break;
    case 'recruit': line([[0, -0.6], [0, 0.6]], 0.24); line([[-0.6, 0], [0.6, 0]], 0.24); break;
    case 'explore': ctx.beginPath(); ctx.arc(-0.1, -0.1, 0.42, 0, 7); ctx.lineWidth = 0.18; ctx.stroke(); line([[0.2, 0.2], [0.65, 0.65]], 0.22); break;
    case 'rest': ctx.font = '700 0.9px sans-serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText('z', -0.25, 0.2); ctx.font = '700 0.6px sans-serif'; ctx.fillText('z', 0.35, -0.3); break;
    case 'guard': case 'garrison': ctx.beginPath(); ctx.moveTo(0, -0.7); ctx.lineTo(0.6, -0.45); ctx.lineTo(0.5, 0.25); ctx.lineTo(0, 0.72); ctx.lineTo(-0.5, 0.25); ctx.lineTo(-0.6, -0.45); ctx.closePath(); ctx.fill(); break;
    case 'shield': ctx.beginPath(); ctx.arc(0, 0, 0.62, 0, 7); ctx.lineWidth = 0.16; ctx.stroke(); ctx.beginPath(); ctx.arc(0, 0, 0.3, 0, 7); ctx.fill(); break;
    case 'dormant': ctx.beginPath(); ctx.arc(0.1, 0, 0.55, Math.PI * 0.35, Math.PI * 1.65); ctx.arc(0.35, -0.05, 0.4, Math.PI * 1.45, Math.PI * 0.55, true); ctx.fill(); break;
    case 'camp': ctx.beginPath(); ctx.moveTo(-0.7, 0.6); ctx.lineTo(0, -0.7); ctx.lineTo(0.7, 0.6); ctx.closePath(); ctx.fill(); break;
    default: ctx.beginPath(); ctx.arc(0, 0, 0.3, 0, 7); ctx.fill();
  }
}

/**
 * A round status badge (Civ's unit flag): faction-coloured disc, the
 * kind's ring, the glyph, and a count dot when the tile has more. Screen
 * sized: `r` is in screen px, `zoom` converts to world px.
 */
export function paintBadge(ctx, x, y, kind, { faction = 6, more = 0, zoom = 1, r = 11, fill = '#8a8a80', dark = '#3a3a34', pulse = 0 } = {}) {
  const k = r / zoom;
  ctx.save();
  ctx.translate(x, y);
  if (pulse) { ctx.globalAlpha = 0.35 * (1 - pulse); ctx.fillStyle = RING[kind] ?? dark; ctx.beginPath(); ctx.arc(0, 0, k * (1.2 + pulse), 0, 7); ctx.fill(); ctx.globalAlpha = 1; }
  ctx.fillStyle = '#fffaf0'; ctx.beginPath(); ctx.arc(0, 0, k * 1.08, 0, 7); ctx.fill();
  ctx.fillStyle = fill; ctx.beginPath(); ctx.arc(0, 0, k * 0.92, 0, 7); ctx.fill();
  ctx.strokeStyle = RING[kind] ?? dark; ctx.lineWidth = k * 0.22; ctx.beginPath(); ctx.arc(0, 0, k * 0.98, 0, 7); ctx.stroke();
  ctx.scale(k * 0.62, k * 0.62);
  ctx.fillStyle = '#fffaf0'; ctx.strokeStyle = '#fffaf0';
  glyph(ctx, kind);
  ctx.restore();
  if (more > 0) {
    ctx.save();
    ctx.fillStyle = dark; ctx.beginPath(); ctx.arc(x + k * 0.85, y - k * 0.85, k * 0.5, 0, 7); ctx.fill();
    ctx.fillStyle = '#fff'; ctx.font = `700 ${k * 0.62}px sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText(`+${more}`, x + k * 0.85, y - k * 0.82);
    ctx.restore();
  }
}

export { RULES };
