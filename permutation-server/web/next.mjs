// What needs a decision next (the Next Turn button, the notification stack,
// Space), and the report of what the last tick changed.
import * as T from './i18n.mjs';
import { $, html, setHtml, toast, troops } from './util.mjs';
import { S, civN, held, spendable, myUnits, myCities, unitById, secondsLeft, invalidate } from './state.mjs';
import { map, key, keyOf } from './world.mjs';
import { used, draftedUnits, endTurn, turnEnded } from './orders.mjs';
import { chainPhase, NO_KEY } from './chainplay.mjs';
import { selectUnit, selectTile } from './selection.mjs';
import { toggleDrawer } from './drawers/index.mjs';
import { startSeason } from './lobby.mjs';
import { BANK_TICKS } from './rules.mjs';
import { L } from './lang.mjs';

// ================================================================== next decision
/** Things that could use a decision now, most urgent first; `none` items are only advice. */
export function nextItems() {
  const v = S.view; const items = [];
  if (!v.economy) return [{ pri: 0, title: L`観戦中`, detail: '', none: true }];
  const mine = held(), g = v.gov || {};
  if (g.voteOpen && T.ROLES.some(r => g.candidates?.find(c => c.role === r)?.candidates.length && S.myVotes[r] === undefined)) items.push({ pri: 95, title: L`選挙の投票受付中（ティック${g.nextElection}の任期）`, detail: L`勢力の広場で各役職に投票できます。`, drawer: 'nation' });
  for (const r of g.recalls || []) items.push({ pri: 92, title: L`${T.ROLE_JA[r.role]}のリコール投票`, detail: L`賛成${r.yes}件 · ティック${r.closes}まで`, drawer: 'nation' });
  const adoptable = (g.proposals || []).filter(p => mine.includes(p.role) && p.proposer?.id !== S.memberId && !(S.adopt[p.role] || []).includes(p.id));
  if (adoptable.length) items.push({ pri: 88, title: L`あなた宛ての献策 ${adoptable.length}件`, detail: L`採用すると功績を献策者と半分ずつ分けます。`, drawer: 'nation' });
  const drafted = draftedUnits();
  for (const p of v.proposals.filter(p => p.to === S.myCiv && mine.includes('Diplomat'))) items.push({ pri: 100, title: L`${civN(p.from)}の${T.PROPOSAL_KIND[p.kind]}の申し入れ`, detail: L`ティック${p.expires}まで有効。外交パネルで受けるか決めてください。`, drawer: 'diplomacy' });
  const atWar = Object.values(v.civs).some(c => c.relation === 'war');
  for (const u of myUnits()) {
    if (u.path?.length || drafted.has(u.id)) continue;
    if (!mine.includes(u.type === 'Settler' ? 'Steward' : 'General')) continue;
    if (u.standing && u.type !== 'Settler') continue; // a standing rule is looking after it
    if (u.type === 'Settler') items.push({ pri: 90, title: L`開拓者が待機中`, detail: L`都市を建てられる土地へ移動するか、この場で建設します。`, unit: u.id });
    else if (!u.civilian && atWar) items.push({ pri: 70, title: L`${T.UNIT[u.type]}が待機中（戦争中）`, detail: L`攻撃できる相手がいないか確認するか、前線へ移動させます。`, unit: u.id });
  }
  for (const c of mine.includes('Steward') ? myCities() : []) {
    const qd = S.drafts.some(d => d.dto.type === 'SetQueue' && d.dto.city === c.id);
    if (!qd && !(c.queue || []).length) items.push({ pri: 80, title: L`${T.cityName(c.id)}の生産予定が空です`, detail: L`建物・部隊・開拓者から選びます。生産は蓄積されるので無駄にはなりません。`, tile: keyOf(c) });
  }
  if (mine.includes('Science') && !v.economy.researchQueue.length && !S.drafts.some(d => d.dto.type === 'SetResearch')) items.push({ pri: 85, title: L`研究が選ばれていません`, detail: L`科学は蓄積されますが、研究を選ぶと技術が解放されます。`, drawer: 'research' });
  const left = spendable() - used();
  if (mine.length && left > 0 && !v.paused && v.secondsLeft < v.tickSeconds * .25) items.push({ pri: 60, title: L`命令の枠が${left}残っています`, detail: L`使わない枠は役職ごとに繰り越されます（最大${BANK_TICKS}ティック分）。`, none: true });
  if (!items.length) items.push({ pri: 0, title: L`急ぎの判断はありません`, detail: mine.length ? L`地図を眺めて、拡大・研究・外交の次の一手を考えましょう。` : L`役職外の命令は献策として出せます。勢力の広場で他のメンバーの献策を支持しましょう。`, none: true });
  return items.sort((a, b) => b.pri - a.pri);
}
const blockers = () => nextItems().filter(i => !i.none);

/** Take the player to an item: its unit, its tile or its drawer. */
function goTo(it) {
  if (!it) return;
  if (it.unit !== undefined) {
    const u = unitById(it.unit);
    selectUnit(it.unit);
    if (u) map.focusTile(key(u.q, u.r));
  } else if (it.tile) { selectTile(it.tile); map.focusTile(it.tile); }
  else if (it.drawer && S.drawer !== it.drawer) toggleDrawer(it.drawer); // open, never close, the drawer it points to
}
/** Space: go to the next item, cycling. */
export function goNext() {
  const items = nextItems();
  goTo(items[S.nextIndex % items.length]);
  S.nextIndex++;
}
/** A notification: go to the n-th item that needs a decision. */
export function goToBlocker(n) { S.nextIndex = n; goTo(blockers()[n]); }

/** Civ-style blocker labels: a verb, not a sentence. */
function shortBlocker(it) {
  if (it.drawer === 'research') return L`研究を選ぶ`;
  if (it.drawer === 'diplomacy') return L`外交に返答`;
  if (it.tile) return L`生産を選ぶ`;
  if (it.unit !== undefined) return unitById(it.unit)?.type === 'Settler' ? L`開拓者に命令` : L`部隊に命令`;
  return it.title;
}

/** Civ's Next Turn button: names what still needs a decision, or ends the turn. */
export function renderNextTurn() {
  const v = S.view, el = $('#next-turn');
  const items = blockers();
  const secs = secondsLeft();
  const frac = v.paused ? 1 : v.tickSeconds ? secs / v.tickSeconds : 1;
  const C = 2 * Math.PI * 62;
  const ring = html`<svg viewBox="0 0 132 132" aria-hidden="true"><circle cx="66" cy="66" r="62" fill="none" stroke="#ffffff22" stroke-width="5"/><circle cx="66" cy="66" r="62" fill="none" stroke="${frac < .15 ? '#e0674f' : '#f0cf7a'}" stroke-width="5" stroke-linecap="round" stroke-dasharray="${C}" stroke-dashoffset="${C * (1 - frac)}"/></svg>`;
  let cls = '', title, sub;
  const closed = v.chain && chainPhase(v) !== 'commit';
  if (v.over) { title = L`シーズン終了`; sub = L`結果を見る`; }
  else if (v.phase === 'lobby') { title = L`開幕する`; sub = L`第1回選挙を行い、ティックを始める`; }
  // Chain mode after the deadline: the gateway reveals the sealed batches, then the tick resolves.
  else if (closed) { cls = 'done'; title = L`解決中`; sub = chainPhase(v) === 'reveal' ? L`封印した命令を公開しています` : L`全ての勢力の命令を解決しています`; }
  else if (turnEnded()) { cls = 'done'; const waiting = v.waiting ?? 0; title = L`手番を終えた`; sub = v.chain ? L`締切（全員同時）を待っています` : waiting ? L`ほか${waiting}人を待っています` : L`解決を待っています`; }
  else if (items.length) { cls = 'blocker'; title = shortBlocker(items[0]); sub = items.length > 1 ? L`ほか${items.length - 1}件 · クリックで移動` : L`クリックで移動`; }
  else { title = L`手番を終える`; sub = v.chain ? L`署名してチェーンへ送信` : L`${Math.ceil(secs)}秒後に自動で解決`; }
  el.className = `next-turn ${cls}`;
  setHtml(el, html`${ring}<span class="lbl"><b>${title}</b><small>${sub}</small></span>`);
}

export function nextTurnClick() {
  const v = S.view;
  if (v.over) { toggleDrawer('era'); return; }
  if (v.phase === 'lobby') { startSeason(); return; }
  if (v.chain && chainPhase(v) !== 'commit') { toast(L`締切を過ぎ、解決しています。次のティックで操作できます。`); return; }
  const items = blockers();
  if (items.length && !turnEnded()) { goToBlocker(0); return; }
  if (v.chain && !S.session) { toast(NO_KEY(), 'error'); return; }
  endTurn({ commitFirst: true });
}

/** Civ's notification stack: grouped, click to jump. */
export function renderNotifs() {
  const groups = new Map();
  blockers().forEach((it, i) => {
    const g = it.drawer === 'diplomacy' ? '✉' : it.drawer === 'research' ? '✧' : it.unit !== undefined ? '⚑' : it.tile ? '⌂' : '•';
    if (!groups.has(g)) groups.set(g, { n: i, count: 0, title: it.title });
    groups.get(g).count++;
  });
  setHtml($('#notif-stack'), [...groups].map(([g, x]) => html`<button class="notif" type="button" data-n="${x.n}">${g}${x.count > 1 ? html`<span class="count">${x.count}</span>` : ''}<span class="tipx">${x.title}</span></button>`));
}

// ================================================================== tick report
/** What changed for my nation this tick, diffed client-side from the previous view. */
export function tickChanges(a, b) {
  const me = b.me, out = [];
  const ea = a.economy, eb = b.economy;
  if (!ea || !eb) return out;
  const dg = eb.gold - ea.gold;
  if (dg) out.push(['◆', L`金 ${dg > 0 ? '+' : ''}${dg}（${eb.gold}）`]);
  if (ea.research && !eb.techs.includes(ea.research.tech) && eb.research?.tech === ea.research.tech) out.push(['✧', L`研究「${T.TECH[eb.research.tech]}」${eb.research.store}/${eb.research.cost}`]);
  else if (eb.research && !ea.research) out.push(['✧', L`研究「${T.TECH[eb.research.tech]}」を開始（${eb.research.store}/${eb.research.cost}）`]);
  for (const t of eb.techs.filter(t => !ea.techs.includes(t))) out.push(['✦', L`「${T.TECH[t]}」を発見：${T.TECH_UNLOCK[t]}`]);
  const ca = new Map(a.cities.filter(c => c.owner === me).map(c => [c.id, c]));
  for (const c of b.cities.filter(c => c.owner === me)) {
    const o = ca.get(c.id);
    if (!o) { out.push(['⌂', L`${T.cityName(c.id)}があなたの都市に`]); continue; }
    if (c.pop > o.pop) out.push(['❧', L`${T.cityName(c.id)}の人口が${c.pop}に`]);
    for (const bld of c.buildings.filter(x => !o.buildings.includes(x))) out.push(['▤', L`${T.cityName(c.id)}で${T.BUILDING[bld] || bld}が完成`]);
    if (c.defense < o.defense) out.push(['!', L`${T.cityName(c.id)}が攻撃を受けた（防御 ${(c.defense / 10).toFixed(0)}/${(c.defenseMax / 10).toFixed(0)}）`]);
  }
  for (const id of ca.keys()) if (!b.cities.some(c => c.id === id && c.owner === me)) out.push(['!', L`${T.cityName(id)}を失った`]);
  const ua = new Map(a.units.filter(u => u.owner === me).map(u => [u.id, u]));
  for (const u of b.units.filter(u => u.owner === me)) {
    const o = ua.get(u.id);
    if (!o) { out.push([L`＋`, L`${T.UNIT[u.type]}が誕生`]); continue; }
    if (o.q !== u.q || o.r !== u.r) out.push(['→', L`${T.UNIT[u.type]}が移動（${u.q}, ${u.r}）${u.path?.length ? L`・残り${u.path.length}マス` : ''}`, key(u.q, u.r)]);
    if (u.troops < o.troops) out.push(['⚔', L`${T.UNIT[u.type]}が損害 −${troops(o.troops - u.troops)}（残り${troops(u.troops)}）`, key(u.q, u.r)]);
  }
  for (const id of ua.keys()) if (!b.units.some(u => u.id === id && u.owner === me)) out.push(['✕', L`${T.UNIT[ua.get(id).type]}を失った`]);
  return out;
}

export function renderSummary() {
  const v = S.view, el = $('#summary');
  if (v.resolvedTick === null || !S.summaryOpen) { el.hidden = true; return; }
  el.hidden = false;
  const lines = (v.lastSummary || []).map(T.chronicleText);
  const mine = S.changes;
  // Once the player starts working the map, the report folds to one line (still one click away).
  const collapsed = S.summaryCollapsed && (S.tile !== null || S.unit !== null);
  el.classList.toggle('collapsed', collapsed);
  setHtml(el, html`<div class="summary-head"><button class="summary-toggle" type="button" id="summary-toggle" aria-expanded="${String(!collapsed)}"><span class="eyebrow">${collapsed ? L`ティック ${v.resolvedTick} の結果 · ${mine.length + lines.length}件` : L`ティック ${v.resolvedTick} の結果`}</span><span class="chev">${collapsed ? '▾' : '▴'}</span></button><button class="close-x" type="button" id="summary-close" aria-label="${L`閉じる`}">×</button></div>
    ${mine.length ? html`<div class="section-title">${L`あなたの勢力`}</div><ul>${mine.map(([g, t, at]) => (at ? html`<li class="go" data-focus="${at}" title="${L`地図で見る`}"><span>${g}</span><span>${t}</span></li>` : html`<li><span>${g}</span><span>${t}</span></li>`))}</ul>` : ''}
    <div class="section-title">${L`世界`}</div>
    ${lines.length ? html`<ul>${lines.map(([k, t]) => html`<li><span>${T.KIND_GLYPH[k] || '·'}</span><span>${t}</span></li>`)}</ul>` : html`<p class="desc" style="margin:4px 0 0">${L`世界に大きな出来事はありませんでした。`}</p>`}`);
}
export function toggleSummary() {
  const wasCollapsed = S.summaryCollapsed && (S.tile !== null || S.unit !== null);
  S.summaryCollapsed = !wasCollapsed; S.summaryPinned = wasCollapsed;
  invalidate('summary');
}
export function closeSummary() { S.summaryOpen = false; invalidate('summary'); }
