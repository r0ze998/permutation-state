// Inspector: one of my units, a target tile for it, and patrol drawing.
import * as T from '../i18n.mjs';
import { html, attrJson, troops } from '../util.mjs';
import { S, civN, unitById } from '../state.mjs';
import { map, key } from '../world.mjs';
import { slotOf } from '../orders.mjs';
import { moveWhyKey, moveBlockedText } from '../selection.mjs';
import { PATROL_MAX, ruledUnitsCap } from '../rules.mjs';
import { head } from './head.mjs';
import { L } from '../lang.mjs';

export function unitPanel(u) {
  const p = S.unitPreview;
  const draft = S.drafts.find(d => slotOf(d.dto) === `u${u.id}`);
  const idle = !u.path?.length && !draft && !u.standing;
  const title = html`${T.UNIT[u.type]}${u.civilian ? '' : html` <span style="font-size:15px;color:var(--muted)">${L`兵 ${troops(u.troops)}`}</span>`}`;
  const desc = u.type === 'Settler' ? L`都市を建てられる場所（都市から3マス以上、他勢力の領土・保護区域の外）へ移動して建設します。`
    : u.type === 'Scout' ? L`森や丘陵でも1ずつ進める偵察役です。戦闘はできません。` : L`移動先の土地をクリックかダブルクリック。攻撃できる相手は赤い枠で示されます。`;
  const top = html`${head(`${u.civilian ? 'CIVILIAN' : 'ARMY'} · ${u.q}, ${u.r}`, title, desc)}<div class="tags">${idle ? html`<span class="tag warning">${L`待機中`}</span>` : u.path?.length ? html`<span class="tag">${L`移動中 · 残り${u.path.length}マス`}</span>` : ''}${draft ? html`<span class="tag positive">${L`命令あり：${draft.label}`}</span>` : ''}</div>`;
  if (!p) return html`${top}<p class="desc">${L`行動を計算しています…`}</p>`;
  const settler = u.type !== 'Settler' ? ''
    : p.found ? html`<button class="btn wide" type="button" aria-disabled="true" disabled>⌂ ${L`ここに都市を建てる`}</button><p class="option why" style="margin:4px 0">${T.blockedText(p.found)}</p>`
    : html`<button class="btn primary wide" type="button" data-order="${attrJson({ type: 'FoundCity', settler: u.id })}">⌂ ${L`ここに都市を建てる（枠1）`}</button><p class="desc">${L`建設すると開拓者は消え、半径1の土地が領土になります。`}</p>`;
  const attacks = u.civilian ? '' : html`<div class="section-title">${L`攻撃できる相手`}</div>${p.attacks.length ? p.attacks.map(a => attackOption(u, a)) : html`<p class="desc">${L`射程内に相手はいません。戦争中の相手・蛮族・都市国家を攻撃できます（弓兵・弩兵は2マス、ほかは隣接）。`}</p>`}`;
  const now1 = p.reach.filter(r => r[2] <= 1).length, soon = p.reach.filter(r => r[2] > 1 && r[2] <= 3).length;
  return html`${top}${settler}${attacks}${standingSection(u)}<div class="section-title">${L`移動`}</div><div class="legend-row"><span><span class="sw sw-now"></span>${L`このティック ${now1}マス`}</span><span><span class="sw sw-soon"></span>${L`2〜3ティック ${soon}マス`}</span>${S.view.protectionRadius ? html`<span><span class="sw sw-prot"></span>${L`他勢力の首都の保護区域（半径${S.view.protectionRadius}）`}</span>` : ''}</div>
    <p class="desc">${L`行き先をダブルクリック（または選んで Enter）で下書きに入ります。遠い土地はカーソルを当てると到着ティックが出ます。移動は締切で全員同時に解決されます。`}</p>`;
}

/** Standing rules for one of my units (§13): set once for 1 slot, then free every tick. */
function standingSection(u) {
  const cur = u.standing;
  const draft = S.drafts.find(d => d.dto.type === 'SetStanding' && d.dto.target.kind === 'Unit' && d.dto.target.id === u.id);
  const order = rule => attrJson({ type: 'SetStanding', target: { kind: 'Unit', id: u.id }, rule });
  const btn = (rule, text, on) => html`<button class="btn ${on ? 'primary' : ''}" type="button" data-order="${order(rule)}" ${on ? 'disabled' : ''}>${text}</button>`;
  let rows = '';
  if (!u.civilian) {
    const eff = draft ? (draft.dto.rule.kind === 'Clear' ? null : draft.dto.rule) : cur; // show what will be in force
    const r = eff?.kind === 'AutoDefend' ? eff.radius : 0;
    const q = eff?.kind === 'Retreat' ? eff.ratioBps : 0;
    rows = html`<div class="standing-row"><span class="sl">${T.STANDING_GLYPH.AutoDefend} ${L`自動防衛`}</span>${[1, 2, 3].map(n => btn({ kind: 'AutoDefend', radius: n }, L`半径${n}`, r === n))}</div>
      <div class="standing-row"><span class="sl">${T.STANDING_GLYPH.Retreat} ${L`撤退`}</span>${[[10000, L`1倍`], [15000, L`1.5倍`], [20000, L`2倍`]].map(([b, t]) => btn({ kind: 'Retreat', ratioBps: b }, t, q === b))}</div>`;
  }
  return html`<div class="section-title">${L`継続命令 · 毎ティック自動（設定に枠1・実行は無料）`}</div>
    <div class="standing-now">${cur ? html`<span class="tag positive">${T.STANDING_GLYPH[cur.kind]} ${T.standingText(cur)}</span>` : html`<span class="tag">${L`なし`}</span>`}${draft ? html`<span class="tag warning">${L`下書き：${T.standingText(draft.dto.rule)}`}</span>` : ''}</div>
    ${rows}<div class="standing-row"><span class="sl">${T.STANDING_GLYPH.Patrol} ${L`巡回`}</span><button class="btn" type="button" id="patrol-start" data-unit="${u.id}">${cur?.kind === 'Patrol' ? L`道筋を引き直す` : L`地図で地点を選ぶ`}</button>${cur ? btn({ kind: 'Clear' }, L`解除`, false) : ''}</div>
    <p class="desc">${L`${u.civilian ? '' : L`自動防衛：基点から半径内に入った敵軍のうち最も弱いものを攻撃。撤退：隣の敵の強さが自軍の指定倍を超えたら自分の都市へ1マス下がる。`}巡回：最大${PATROL_MAX}地点を順に回り続けます。各地点へはまっすぐ進みます（水や山を回り込むには地点を足してください）。進めない地点は飛ばします。手動の命令を出したティックはそちらが優先されます。`}${L`継続命令が動くのは国ごとに${ruledUnitsCap(S.view.civs.length)}部隊まで（番号の小さい順）。`}</p>`;
}

export function patrolPanel() {
  const pt = S.patrol; const u = unitById(pt.unit);
  const dto = { type: 'SetStanding', target: { kind: 'Unit', id: pt.unit }, rule: { kind: 'Patrol', route: pt.route } };
  return html`${head(`PATROL · ${T.UNIT[u?.type] || ''}`, L`巡回の道筋`, L`地図で回る地点を順にクリックしてください（最大${PATROL_MAX}）。最後の地点のあとは最初に戻ります。`)}
    <ol class="patrol-list">${pt.route.length ? pt.route.map(([q, r], i) => html`<li>${i + 1}. (${q}, ${r}) <button class="close-x" type="button" data-patrol-remove="${i}" aria-label="${L`削除`}">×</button></li>`) : html`<li class="meta">${L`まだ地点がありません`}</li>`}</ol>
    <div class="row"><button class="btn primary" type="button" id="patrol-done" ${pt.route.length ? '' : 'disabled'} data-dto="${attrJson(dto)}">${L`この道筋で巡回（枠1）`}</button><button class="btn" type="button" id="patrol-cancel">${L`やめる`}</button></div>`;
}

function attackOption(u, a) {
  const t = a.target;
  let name = '';
  if (t.kind === 'Unit') { const y = unitById(t.id); name = L`${civN(y?.owner)}の${T.UNIT[y?.type]}`; }
  if (t.kind === 'City') name = L`${T.cityName(t.id)}（都市）`;
  if (t.kind === 'CityState') name = L`都市国家${t.id + 1}`;
  if (a.blocked) return html`<div class="option" aria-disabled="true"><span class="ic">⚔</span><span><span class="name">${name}</span><div class="why">${T.blockedText(a.blocked)}</div></span><span></span></div>`;
  const f = a.forecast;
  const detail = f.captureCivilian ? L`非戦闘ユニットを捕獲します` : L`予測：相手 −${(f.toDefender / 1000).toFixed(1)}（残り${((f.defenderTroops - f.toDefender) / 1000).toFixed(1)}）・自軍 −${(f.toAttacker / 1000).toFixed(1)}`;
  const warn = f.neutral ? html`<div class="why">${L`中立への攻撃は侵略扱い：都市国家への影響力をすべて失います`}</div>` : '';
  return html`<button class="option" type="button" data-order="${attrJson({ type: 'Attack', army: u.id, target: t })}"><span class="ic">⚔</span><span><span class="name">${name}</span><div class="meta">${detail}</div>${warn}<div class="meta">${L`乱数±10%・他の勢力も同時に動くため目安です`}</div></span><span class="cost">${L`枠1`}</span></button>`;
}

/** The selected unit and another tile: move there, attack there, or why not. */
export function targetPanel(u, id) {
  const p = S.unitPreview; const t = map.tiles.get(id);
  const reach = p?.reach.find(([q, r]) => q === t.q && r === t.r);
  const atk = p?.attacks.filter(a => key(a.q, a.r) === id) || [];
  const di = S.drafts.findIndex(d => d.dto.type === 'MoveUnit' && d.dto.unit === u.id);
  const here = di >= 0 && S.drafts[di].focus === id;
  let move = '';
  if (reach && here) move = html`<div class="explanation ok">✓ ${L`この土地への移動は下書き済みです（約${reach[2]}ティック）。`}</div><button class="btn wide" type="button" data-remove="${di}">${L`下書きを取り消す`}</button>`;
  else if (reach) move = html`<button class="btn primary wide" type="button" data-move="${id}">→ ${L`ここへ移動（約${reach[2]}ティック・${di >= 0 ? L`今の移動命令と差し替え` : L`枠1`}）`}</button>`;
  else if (!atk.length) {
    const mw = S.moveWhy?.key === moveWhyKey(u.id, id) ? S.moveWhy : null;
    const why = mw?.text ? moveBlockedText(mw.blocked) : null; // re-read: the language may have changed since
    move = html`<div class="explanation">${why ? L`この部隊はここへ移動できません：${why}` : L`この部隊はここへ移動できません。`}</div>`;
  }
  return html`${head(`${T.UNIT[u.type]} → ${t.q}, ${t.r}`, T.TERRAIN[t.terrain], L`選択中の部隊で、この土地に対してできること。`)}${atk.map(a => attackOption(u, a))}${move}
    <button class="btn wide" type="button" data-select-unit="${u.id}" style="margin-top:6px">← ${L`${T.UNIT[u.type]}に戻る`}</button>`;
}
