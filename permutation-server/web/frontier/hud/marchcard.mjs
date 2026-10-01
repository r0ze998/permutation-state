// The march card (UI plan B1–B4; Civ's move-and-attack preview): a march
// composed on the map without leaving it. Choose a host (the inspector's
// "march with this host"), tap a tile — the destination is set at once —
// and the card shows everything the decision needs in one place:
//   the route and the arrival bell with the local time (a −/+ stepper over
//   the window), the four stances as big buttons with the triangle, retreat
//   presets, the defenders frozen at the destination as the bell starts
//   (from the province the page holds), a rough odds word from the troop
//   ratio (honestly labelled a guide), and "seal and depart".
// The full form stays on the Marches tab (tip, costs, custom ratio, the
// coordinate list for keyboard and screen-reader use).
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { STANCES as STANCE_TEXT, UNITS, clientText, failureText, factionName } from '../fi18n.mjs';
import { STANCES } from '../seal.mjs';
import { RETREAT_CHOICES, arrivalWindow, checkMarch, troopsOf } from '../fmarch.mjs';
import { UNIT_ORDER } from '../fland.mjs';
import { bellStart } from '../clock.mjs';
import { composerMarch, routeLine } from '../screens/march.mjs';
import { clockTime, swatch } from '../screens/shell.mjs';
import { DIRECTIONS, tileHex } from '../fgeo.mjs';

/** Stance glyphs (the triangle: Assault beats Flank, Flank beats Brace, Brace beats Assault). */
const STANCE_GLYPH = { Hold: '■', Assault: '▲', Flank: '⇄', Brace: '❙' };
const BEATS = { Assault: 'Flank', Flank: 'Brace', Brace: 'Assault' };
const RETREAT_SHORT = { never: () => L`撤退しない`, x2: () => L`2倍で撤退`, 'x1.5': () => L`1.5倍で撤退`, x1: () => L`同数で撤退`, 'x0.5': () => L`半分で撤退` };

/** The route as hexes (axial q, r) from the origin tile, following the planner's directions. */
export function routeHexes(c) {
  if (!c?.route?.dirs || !c.origin || !c.host) return [];
  const s = tileHex(c.origin.p, c.origin.q, c.host.tile);
  if (!s) return [];
  const out = [{ q: s.q, r: s.r }];
  let q = s.q, r = s.r;
  for (const d of c.route.dirs) { const v = DIRECTIONS[d]; if (!v) break; q += v[0]; r += v[1]; out.push({ q, r }); }
  return out;
}

/**
 * The defenders frozen at the destination (the province the page holds):
 * `{residents: [{faction, troops}], garrison, camp, total}` in whole troops.
 * A guide only: the roster freezes at the start of the arrival bell.
 */
export function defendersAt(FS, dest) {
  const prov = dest ? FS.provinces?.get(`${dest.p},${dest.q}`)?.province : null;
  if (!prov) return null;
  const residents = (prov.entries ?? []).filter(e => e.state === 1 && e.tile === dest.tile && e.faction !== FS.citizen?.faction).map(e => ({ faction: e.faction, troops: troopsOf(e.troops) }));
  const j = Array.from(prov.sites ?? []).indexOf(dest.tile);
  const m = j >= 0 ? prov.siteMirror?.[j] : null;
  const garrison = m && m.state === 1 && m.faction !== FS.citizen?.faction ? troopsOf(m.garrison) : 0;
  const camp = prov.camp?.state === 1 && prov.camp.tile === dest.tile ? Number(prov.camp.troops) : 0;
  return { residents, garrison, camp, total: residents.reduce((a, r) => a + r.troops, 0) + garrison + camp };
}

/** A guide word from the troop ratio (attacker ÷ defenders): no defenders → "unopposed". */
export function oddsWord(mine, theirs) {
  if (!theirs) return { id: 'free', text: L`守り手なし` };
  const k = mine / theirs;
  if (k >= 2) return { id: 'strong', text: L`優勢` };
  if (k >= 1.25) return { id: 'edge', text: L`やや優勢` };
  if (k >= 0.8) return { id: 'even', text: L`互角` };
  if (k >= 0.5) return { id: 'weak', text: L`やや不利` };
  return { id: 'bad', text: L`不利` };
}

/** The arrival options the stepper walks: the window from the earliest bell. */
export function bellWindow(FS) {
  const c = FS.compose;
  if (!c || !FS.season) return null;
  const w = arrivalWindow(FS.season, FS.nowBell ?? 0, c.earliest);
  return { ...w, value: Math.min(w.max, Math.max(w.min, c.arriveBell ?? w.min)) };
}

/** The card (inside the inspector while a march is composed). */
export function render(FS) {
  const c = FS.compose;
  if (!c?.host) return '';
  const m = composerMarch(FS);
  const unit = UNITS[UNIT_ORDER[c.host.unit]] ?? '';
  const w = bellWindow(FS);
  const at = w && FS.clock ? clockTime(bellStart(FS.clock.genesisTs, w.value)) : null;
  const def = c.dest ? defendersAt(FS, c.dest) : null;
  const odds = c.dest && def ? oddsWord(c.host.troops, def.total) : null;
  const problems = c.dest ? checkMarch(m).filter(p => p !== 'NoDestination') : [];
  const busy = !!c.sending;
  const stance = STANCES[m.stance] ?? 'Hold';
  return html`<section class="marchcard" aria-labelledby="mc-title">
    <h3 id="mc-title">${L`進軍：${unit} ${fmtNum(c.host.troops)}`}</h3>
    ${c.dest ? html`
      <p class="mc-dest"><strong>${L`行き先 州 ${c.dest.p},${c.dest.q} · マス ${c.dest.tile + 1}`}</strong>
        <span class="mc-sealed">${L`封の中（あなたにしか見えません）`}</span></p>
      <p class="mc-route">${c.route ? routeLine(c.route) : c.routeError ? clientText(c.routeError) : L`道のりを探しています…`}</p>
      ${w ? html`<div class="mc-bell" role="group" aria-label="${L`到着の鐘`}">
        <button type="button" class="btn small" data-act="mc-bell" data-d="-1" aria-label="${L`1つ早い鐘`}" ${raw(w.value <= w.min ? 'disabled' : '')}>−</button>
        <span class="mc-bell-v"><strong>${L`第${fmtNum(w.value)}鐘に到着`}</strong>${at ? html` <span class="muted">${L`${at.local} ごろ`}</span>` : ''}</span>
        <button type="button" class="btn small" data-act="mc-bell" data-d="1" aria-label="${L`1つ遅い鐘`}" ${raw(w.value >= w.max ? 'disabled' : '')}>+</button></div>` : ''}
      <div class="mc-stances" role="radiogroup" aria-label="${L`構え`}">${STANCES.map((s, i) => html`<button type="button" class="mc-stance" role="radio" aria-checked="${i === m.stance ? 'true' : 'false'}" data-act="mc-stance" data-v="${i}">
        <span class="mc-glyph" aria-hidden="true">${STANCE_GLYPH[s]}</span><span>${STANCE_TEXT[s]}</span>${BEATS[s] ? html`<span class="mc-beats">${L`${STANCE_TEXT[BEATS[s]]}に強い`}</span>` : html`<span class="mc-beats">${L`相性なし`}</span>`}</button>`)}</div>
      <div class="mc-retreat" role="radiogroup" aria-label="${L`撤退`}">${RETREAT_CHOICES.filter(r => r.id !== 'custom').map(r => html`<button type="button" class="btn small" role="radio" aria-checked="${r.id === m.retreat.choice ? 'true' : 'false'}" data-act="mc-retreat" data-v="${r.id}">${RETREAT_SHORT[r.id]()}</button>`)}</div>
      ${def ? html`<div class="mc-odds mc-odds-${odds.id}">
        <p><strong>${odds.text}</strong> <span class="muted">${L`目安（兵数の比べ。実際は鐘の始まりの顔ぶれと乱数で決まります）`}</span></p>
        <p>${L`あなた ${fmtNum(c.host.troops)}（${STANCE_TEXT[stance]}）対 守り ${fmtNum(def.total)}`}</p>
        ${def.residents.length || def.garrison || def.camp ? html`<ul class="list mc-def">${def.residents.map(r => html`<li>${swatch(r.faction)}${factionName(r.faction)} ${fmtNum(r.troops)}</li>`)}${def.garrison ? html`<li>${L`守備隊 ${fmtNum(def.garrison)}`}</li>` : ''}${def.camp ? html`<li>${L`蛮族 ${fmtNum(def.camp)}`}</li>` : ''}</ul>` : ''}
      </div>` : html`<p class="muted">${L`行き先の州の中身を読み込むと、守り手が出ます。`}</p>`}
      ${problems.length ? html`<ul class="problems">${problems.map(p => html`<li>${failureText({ code: p })}</li>`)}</ul>` : ''}
      <div class="actions"><button type="button" class="btn primary" data-act="march-send" ${raw(problems.length || busy || !c.route ? 'disabled' : '')}>${L`封をして出発する`}</button>
        <button type="button" class="btn" data-act="tab" data-tab="marches">${L`詳しい設定`}</button>
        <button type="button" class="btn" data-act="compose-close">${L`やめる`}</button></div>`
    : html`<p class="mc-hint">${L`地図で行き先のマスをタップしてください。`}</p>
      <div class="actions"><button type="button" class="btn" data-act="compose-close">${L`やめる`}</button></div>`}
  </section>`;
}
