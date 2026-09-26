// Diplomacy (the diplomat's office): proposals received, and one row per
// nation with what can be proposed and why not.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { $, html, attrJson, usdcFixed } from '../util.mjs';
import { addDraft } from '../orders.mjs';
import { S, civN, held, membersOf, invalidate } from '../state.mjs';
import { CASUS_BELLI, LEAVE_ALLIANCE_TICKS, NAP_BOND, NAP_TICKS, PROPOSAL_TTL, TRUCE_TICKS } from '../rules.mjs';
import { L, Lh, lang, lazyTable } from '../lang.mjs';

export async function loadDiplomacy() {
  const ids = S.view.civs.map(c => c.id).filter(id => id !== S.myCiv);
  const res = await Promise.all(ids.map(id => api.tryGet(`/api/preview/diplomacy?with=${id}`, [])));
  S.diplo = Object.fromEntries(ids.map((id, i) => [id, res[i]]));
  invalidate('drawer');
}

// Only kinds without their own case in termText are looked up here.
const TERM_JA = lazyTable({ Peace: () => L`講和する` });
const termText = t => (t.kind === 'LeaveAlliance' ? L`${civN(t.with)}との同盟を抜ける` : t.kind === 'KeepNap' ? L`不可侵を守る（${t.every}ティックごとに${t.installments}回に分けて）` : t.kind === 'Capture' ? L`${T.cityName(t.city)}を落とす（誰でも）` : TERM_JA[t.kind]);

/** Treasury contracts (V5 §18.6): what binds a promise. Offer to a nation from its row. */
export function offerContract(civ) {
  const kind = $(`#ct-kind-${civ}`)?.value || 'Peace';
  const usdc = Math.round(Number($(`#ct-usdc-${civ}`)?.value || 0) * 1e6);
  const ticks = Math.max(1, Number($(`#ct-ticks-${civ}`)?.value || 20));
  const deadline = Math.min(S.view.tick + ticks, S.view.ticks - 1);
  let dto;
  if (kind === 'Capture') {
    const city = Number($(`#ct-city-${civ}`)?.value);
    dto = { type: 'OfferContract', term: { kind: 'Capture', city }, usdc, deadline };
  } else if (kind === 'KeepNap') {
    dto = { type: 'OfferContract', to: civ, term: { kind: 'KeepNap', every: 5, installments: Math.max(1, Math.floor(ticks / 5)) }, usdc, deadline: Math.min(S.view.tick + 5, S.view.ticks - 1) };
  } else {
    dto = { type: 'OfferContract', to: civ, term: { kind: 'Peace' }, usdc, deadline };
  }
  if (usdc > 0 && addDraft(dto)) invalidate('drawer');
}

function contractsSection(v) {
  const list = v.contracts || [];
  const mine = list.filter(c => c.from === S.myCiv || c.to === S.myCiv || (c.to == null && c.from !== S.myCiv));
  const row = c => {
    const from = c.from === S.myCiv ? L`あなたの勢力` : civN(c.from);
    const to = c.to == null ? L`（落とした勢力）` : c.to === S.myCiv ? L`あなたの勢力` : civN(c.to);
    const act = c.to === S.myCiv && c.accepted == null && c.offered < v.tick
      ? html`<button class="btn primary" type="button" data-order="${attrJson({ type: 'AcceptContract', id: c.id })}">${L`受け入れる（枠1）`}</button>`
      : c.from === S.myCiv && c.accepted == null && c.term.kind !== 'Capture'
        ? html`<button class="btn" type="button" data-order="${attrJson({ type: 'CancelContract', id: c.id })}">${L`取り下げる`}</button>` : '';
    return html`<div class="proposal"><div>${Lh`<b>${from}</b> → <b>${to}</b>：${termText(c.term)}`}</div><div class="when">${L`預かり ${usdcFixed(c.escrow)} / ${usdcFixed(c.total)} USDC · 期限 ティック${c.deadline}${c.accepted != null ? L` · ティック${c.accepted}に成立` : L` · 未成立`}`}</div>${act ? html`<div class="row" style="margin-top:6px">${act}</div>` : ''}</div>`;
  };
  return html`<div class="section-title">${L`勢力の資金の契約 ${mine.length}`}</div><p class="desc">${L`約束を勢力の資金のUSDCで縛ります。条件がチェーンで確かめられたら相手の勢力の資金へ、期限までに満たされなければ戻ります。受け取ったUSDCは市場では使えず、最後に預けた人へ払い戻されます。`}</p>${mine.length ? mine.map(row) : html`<p class="desc">${L`ありません。`}</p>`}`;
}

const actionLabel = a => `${T.DIPLO_ACTION[a]}${a === 'ProposeNap' ? L`（保証金${NAP_BOND}金）` : ''}`;
const RELATION_TAG = { war: 'bad', alliance: 'positive', nap: 'warning' };

// One civ row of the drawer is a <details>: civHeader as its summary, then
// status lines, civActions and (market on) contractForm in its body. The
// helpers keep the row's exact markup, whitespace included.

/** The row's always-visible line: name, era, pending proposal, relation. */
function civHeader(c, hasInbox) {
  return html`<summary><span class="l1"><span class="swatch-s" style="background:${T.CIV_COLORS[c.id]}"></span><b>${civN(c.id)}</b><span class="badge">${L`第${c.era}時代`}</span>${hasInbox ? html`<span class="tag warning">${L`申し入れ`}</span>` : ''}<span class="grow"></span><span class="tag ${RELATION_TAG[c.relation] || ''}">${T.RELATION[c.relation]}</span></span><span class="l2 meta">${L`メンバー${membersOf(c.id).length}人 · 確認済み${c.cities}都市 · 目視の兵${c.troopsSeen}`}</span></summary>`;
}

/** What can be proposed to `c`: buttons, then the engine's reasons for the blocked ones. */
function civActions(c, mine) {
  const allowed = (S.diplo[c.id] || []).filter(o => !o.action.startsWith('Accept'));
  const blocked = allowed.filter(o => o.blocked);
  const action = o => {
    const dto = o.action === 'ProposeNap' ? { type: 'ProposeNap', civ: c.id, bond: NAP_BOND } : { type: o.action, civ: c.id };
    return o.blocked ? html`<button class="btn" type="button" disabled title="${T.blockedText(o.blocked)}">${actionLabel(o.action)}</button>`
      : html`<button class="btn ${o.action === 'DeclareWar' ? 'danger' : ''}" type="button" data-order="${attrJson(dto)}">${actionLabel(o.action)}</button>`;
  };
  const consent = (mine.includes('General') || mine.includes('Steward')) && c.relation === 'peace';
  return html`<div class="row" style="margin-top:6px">${allowed.map(action)}${consent ? html`<button class="btn" type="button" data-order="${attrJson({ type: 'ConsentWar', civ: c.id })}" title="${L`外交官の宣戦に、将軍・内政官として同意します（枠を使いません）`}">${L`⚖ 宣戦に同意`}</button>` : ''}${c.relation === 'nap' ? html`<button class="btn danger" type="button" data-order="${attrJson({ type: 'BreakNap', civ: c.id })}">${L`条約を破棄（保証金を失い宣戦）`}</button>` : ''}</div>
        ${blocked.length ? html`<div class="when" style="margin-top:4px">${blocked.map((o, i) => html`${i ? html`<br>` : ''}${L`${T.DIPLO_ACTION[o.action]}：${T.blockedText(o.blocked)}`}`)}</div>` : ''}`;
}

/** Treasury contract offer to `c` (read back by offerContract). */
function contractForm(c, v) {
  return html`<div class="row contract-form" style="margin-top:8px">
          <select id="ct-kind-${c.id}"><option value="Peace">${L`講和したら払う`}</option><option value="KeepNap">${L`不可侵を守る間、分けて払う`}</option><option value="Capture">${L`この勢力の都市を落とした勢力に払う`}</option></select>
          <select id="ct-city-${c.id}">${(v.cities || []).filter(x => x.owner === c.id).map(x => html`<option value="${x.id}">${T.cityName(x.id)}</option>`)}</select>
          <label class="field">USDC<input id="ct-usdc-${c.id}" type="number" min="0.1" step="0.1" value="2"></label>
          <label class="field">${Lh`期限<input id="ct-ticks-${c.id}" type="number" min="1" max="60" value="20">ティック`}</label>
          <button class="btn" type="button" data-offer-contract="${c.id}">${L`契約を出す`}</button></div>`;
}

export function drawerDiplomacy() {
  const v = S.view, mine = held();
  const inbox = v.proposals.filter(p => p.to === S.myCiv);
  const received = p => {
    const [text, type] = { Peace: [T.PROPOSAL_KIND.Peace, 'AcceptPeace'], Nap: [L`不可侵条約（保証金${p.bond}金ずつ・${NAP_TICKS}ティック）`, 'AcceptNap'], Alliance: [T.PROPOSAL_KIND.Alliance, 'AcceptAlliance'] }[p.kind];
    const dto = p.kind === 'Nap' ? { type, civ: p.from, bond: p.bond } : { type, civ: p.from };
    return html`<div class="proposal"><div><span class="swatch-s" style="background:${T.CIV_COLORS[p.from]}"></span>${Lh`<b>${civN(p.from)}</b>から${text}`}</div><div class="when">${L`ティック${p.tick}に提案 · ティック${p.expires}で失効${p.kind === 'Nap' ? L` · 破った側の保証金は相手のものに` : ''}`}</div><div class="row" style="margin-top:6px"><button class="btn primary" type="button" data-order="${attrJson(dto)}">${L`受け入れる（枠1）`}</button></div></div>`;
  };
  const row = c => {
    const hasInbox = inbox.some(p => p.from === c.id);
    const open = S.diploOpen.has(c.id) || hasInbox;
    return html`<details class="diplo-civ" data-civ-row="${c.id}" ${open ? 'open' : ''}>
      ${civHeader(c, hasInbox)}
      <div class="civ-body">
        <div class="when">${Lh`節目 ${c.tiers.map((t, i) => `${T.PATH_JA[i]}${lang() === 'en' ? ' ' : ''}${t}`).join(' · ')} · スターゲート${c.stages}/3${c.aggressor ? html` · <span style="color:var(--bad)">${L`侵略中`}</span>` : ''}${c.truceUntil > v.tick ? L` · 休戦 ティック${c.truceUntil}まで` : ''}`}</div>
        <div class="when">${L`あなたの不満 ${c.myGrievanceAgainst ?? 0} · 相手の不満 ${c.grievanceAgainstMe ?? 0}（${CASUS_BELLI}以上で正当な開戦理由）`}</div>
        ${civActions(c, mine)}
        ${v.market ?? true ? contractForm(c, v) : ''}
      </div></details>`;
  };
  return html`<div class="eyebrow">${L`DIPLOMACY · 外交官の担当`}</div><h2>${L`外交`}</h2><p class="drawer-intro">${Lh`申し入れは次のティック以降に相手が受ければ成立し、${PROPOSAL_TTL}ティックで失効します。講和すると${TRUCE_TICKS}ティックの休戦になります。<b>宣戦には、外交官とは別の人の将軍か内政官の同意（同じティック）が必要です。</b>${mine.includes('Diplomat') ? '' : L` あなたは外交官ではないので、ここでの命令は外交官への献策になります。`}`}</p>
    <div class="section-title">${L`届いた申し入れ ${inbox.length}`}</div>${inbox.length ? inbox.map(received) : html`<p class="desc">${L`ありません。`}</p>`}
    ${contractsSection(v)}
    <div class="section-title">${L`勢力 · 行をクリックで操作`}</div>${v.civs.filter(c => c.id !== S.myCiv).map(row)}
    ${v.civs.some(c => c.relation === 'alliance') ? html`<button class="btn wide" type="button" data-order="${attrJson({ type: 'LeaveAlliance' })}">${L`同盟から離脱する（${LEAVE_ALLIANCE_TICKS}ティック後）`}</button>` : ''}`;
}
