// What is selected on the map (tile, unit, patrol being drawn) and the
// previews the inspector shows for it.
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import { $, toast } from './util.mjs';
import { S, civN, unitById, invalidate } from './state.mjs';
import { map, key, keyOf, ownerOf, unitsAt } from './world.mjs';
import { addDraft } from './orders.mjs';
import { loadResearch } from './drawers/research.mjs';
import { loadDiplomacy } from './drawers/diplomacy.mjs';
import { PATROL_MAX } from './rules.mjs';
import { L } from './lang.mjs';

export function hoverTile(id) {
  const el = $('#hover-label');
  if (!id || !S.view) { el.hidden = true; return; }
  const t = map.tiles.get(id); const o = ownerOf(id);
  el.hidden = false;
  el.textContent = `${T.TERRAIN[t.terrain]}${t.resource ? L`・${T.RESOURCE[t.resource]}` : ''}${t.river ? L`・川` : ''} · ${t.q}, ${t.r}${o !== null ? L` · ${civN(o)}の領土` : ''}`;
}

function clearUnit() { S.unit = null; S.unitPreview = null; map.setOverlay({}); }

export function clearSelection() {
  S.tile = null; S.unit = null;
  map.setSelection(null); map.setOverlay({});
  invalidate('inspector', 'summary');
}

export function selectTile(id) {
  if (S.patrol) { addPatrolPoint(id); return; }
  if (!S.summaryPinned && !S.summaryCollapsed) { S.summaryCollapsed = true; invalidate('summary'); }
  if (!S.view) return;
  invalidate('inspector', 'summary');
  // With one of my units selected, clicking a reachable tile or target acts on it.
  if (S.unit !== null) {
    const u = unitById(S.unit);
    const mineHere = unitsAt(id).find(x => x.owner === S.myCiv && x.id !== S.unit);
    if (u && !mineHere && id !== keyOf(u)) { S.tile = id; map.setSelection(id); ensureMoveWhy(); return; }
  }
  const mine = unitsAt(id).filter(u => u.owner === S.myCiv);
  const myCity = S.view.cities.find(c => keyOf(c) === id && c.owner === S.myCiv);
  const reclick = S.tile === id;
  S.tile = id; map.setSelection(id);
  // Own city: the first click opens the city (production lives there); further clicks cycle its units.
  if (myCity && (!reclick || (S.unit === null && !mine.length))) { clearUnit(); loadCity(myCity.id); return; }
  if (myCity && S.unit !== null && mine.findIndex(u => u.id === S.unit) === mine.length - 1) { clearUnit(); loadCity(myCity.id); return; }
  if (mine.length) {
    const cur = mine.findIndex(u => u.id === S.unit);
    selectUnit(mine[(cur + 1) % mine.length].id);
  } else clearUnit();
}

export async function selectUnit(id) {
  S.unit = id; S.unitPreview = null; invalidate('inspector', 'summary');
  const u = unitById(id);
  if (u) { S.tile = keyOf(u); map.setSelection(S.tile); }
  const p = await api.tryGet(`/api/preview/unit?id=${id}`);
  if (!p || S.unit !== id) return;
  S.unitPreview = p;
  map.setOverlay({
    unit: id,
    reach: new Map(p.reach.map(([q, r, ticks]) => [key(q, r), ticks])),
    attacks: p.attacks.map(a => ({ id: key(a.q, a.r), blocked: a.blocked, forecast: a.forecast })),
    found: p.found !== null && p.found !== undefined && u ? { id: keyOf(u), ok: !p.found } : null,
  });
  ensureMoveWhy();
  invalidate('inspector');
}

/** Select a map tile and bring it into view. */
export function focusTile(id) { map.focusTile(id); selectTile(id); }
/** Select a unit and bring it into view. */
export function focusUnit(id) {
  const u = unitById(id);
  if (u) { map.focusTile(keyOf(u)); selectUnit(u.id); }
}

export async function loadCity(id) {
  S.cityPreview = null; S.cityLoading = id; invalidate('inspector');
  const p = await api.tryGet(`/api/preview/city?id=${id}`);
  if (!p || S.cityLoading !== id) return;
  S.cityPreview = { id, ...p }; invalidate('inspector');
}

/** A new tick: previews are stale, and the selected unit may be gone. */
export function refreshSelection() {
  S.unitPreview = null; S.cityPreview = null;
  if (S.unit !== null) {
    if (S.view.units.some(u => u.id === S.unit && u.owner === S.myCiv)) selectUnit(S.unit);
    else { S.unit = null; map.setOverlay({}); }
  }
  const c = S.tile && S.view.cities.find(c => keyOf(c) === S.tile && c.owner === S.myCiv);
  if (c && S.unit === null) loadCity(c.id);
  if (S.drawer === 'research') loadResearch();
  if (S.drawer === 'diplomacy') loadDiplomacy();
  invalidate('inspector');
}

// ================================================================== moving
export function moveBlockedText(b) {
  if (b?.code === 'ProtectedCapital') return L`${civN(b.civ)}の首都の保護区域です${b.until !== null ? L`（ティック${b.until}から入れます）` : L`（保護が続く間は入れません）`}`;
  if (b?.code === 'Unreachable') return L`途中の道がふさがっているか、12マスより遠い場所です`;
  return T.blockedText(b);
}

/** Double-click / Enter: move the selected unit to `id` (or select, with no unit). */
export async function quickMove(id) {
  if (S.unit === null) return selectTile(id);
  const u = unitById(S.unit), t = map.tiles.get(id);
  if (!u || !t) return;
  const res = await api.tryGet(`/api/preview/path?unit=${u.id}&q=${t.q}&r=${t.r}`);
  if (!res) return;
  if (!res.path) return toast(L`そこへは移動できません：${moveBlockedText(res.blocked)}`, 'error');
  addDraft({ type: 'MoveUnit', unit: u.id, path: res.path });
}

/**
 * The target panel says why the selected unit cannot go to the selected tile
 * when it is neither reachable nor attackable: ask the server once per
 * unit, tile and tick.
 */
function ensureMoveWhy() {
  const u = unitById(S.unit), p = S.unitPreview, id = S.tile;
  if (!u || !p || !id || id === keyOf(u)) return;
  const t = map.tiles.get(id); if (!t) return;
  if (p.reach.some(([q, r]) => q === t.q && r === t.r) || p.attacks.some(a => key(a.q, a.r) === id)) return;
  const k = moveWhyKey(u.id, id);
  if (S.moveWhy?.key === k) return;
  S.moveWhy = { key: k, text: null, blocked: null }; // the inspector re-reads the text from `blocked` (language switches)
  api.tryGet(`/api/preview/path?unit=${u.id}&q=${t.q}&r=${t.r}`).then(res => {
    if (!res || S.moveWhy?.key !== k) return;
    S.moveWhy.text = res.path ? null : moveBlockedText(res.blocked);
    S.moveWhy.blocked = res.path ? null : res.blocked;
    invalidate('inspector');
  });
}
export const moveWhyKey = (unit, id) => `${unit}:${id}:${S.view.tick}`;

// ================================================================== patrol (standing rule)
export function startPatrol(unitId) {
  const u = unitById(unitId); if (!u) return;
  S.patrol = { unit: u.id, from: [u.q, u.r], route: [] };
  map.setPatrol(S.patrol); invalidate('inspector');
  toast(L`地図で巡回する地点を順にクリックしてください（最大${PATROL_MAX}）。`);
}
export function cancelPatrol() { S.patrol = null; map.setPatrol(null); invalidate('inspector'); }
export function finishPatrol(dto) { cancelPatrol(); addDraft(dto); }
export function removePatrolPoint(i) {
  if (!S.patrol) return;
  S.patrol.route.splice(i, 1); map.setPatrol(S.patrol); invalidate('inspector');
}
function addPatrolPoint(id) { // editing a patrol: clicks add waypoints
  const t = map.tiles.get(id);
  if (t && t.terrain !== 'Water' && t.terrain !== 'Mountain' && S.patrol.route.length < PATROL_MAX) { S.patrol.route.push([t.q, t.r]); map.setPatrol(S.patrol); invalidate('inspector'); }
  else if (S.patrol.route.length >= PATROL_MAX) toast(L`巡回の地点は${PATROL_MAX}つまでです。`, 'error');
  else toast(L`水域や山岳は巡回の地点にできません。`, 'error');
}
