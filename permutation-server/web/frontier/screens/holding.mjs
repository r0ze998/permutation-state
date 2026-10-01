// The holding panel (web design §7.4; contract §5.10, I-29, I-56): header
// (site, tier, provisional/final, shield, dormancy), the eight stores
// settled lazily as the kernel does, the build queue and the build list
// (first-copy costs), training (immediate, I-56), the trained reserve,
// muster into a host (joins the roster from the next bell), garrison,
// vigil hours, and the transit records. Every action says why it cannot
// run now, with a "catch up" nudge when the province lags (NotResident).
import { activeHolding } from '../fstate.mjs';
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { RESOURCES, RESOURCE_ORDER, UNITS, TIERS, BUILDINGS, HOLDING_STATES, errorText } from '../fi18n.mjs';
import { storesAt, holdingFacts, BUILD_ITEMS, UNIT_ORDER, SETTLER, actionBlocks, baseProduction } from '../fland.mjs';
import { holdingName } from '../people/ui.mjs';
import { span } from '../hud/hud.mjs';
import { inTime, row, timeHtml } from './shell.mjs';

const costText = cost => cost.map((c, i) => (c ? `${RESOURCES[RESOURCE_ORDER[i]]} ${fmtNum(c)}` : null)).filter(Boolean).join(' · ');

/** Why an action is blocked, as text, with the catch-up offer for a lagging province. */
export function blockedLine(blocks) {
  if (!blocks.length) return '';
  const lag = blocks.includes('NotResident');
  return html`<p class="blocked">${blocks.map(errorText).join(' / ')}${lag ? html` <button type="button" class="btn small" data-act="nudge">${L`追いつかせる`}</button>` : ''}</p>`;
}

/** Seconds the n-th copy of a building takes (catalog `build_secs`: 1 h × (1 + n/2)). */
export const buildSecs = n => 3_600 + 1_800 * n;

/**
 * The build cards (UI plan E2, Civ VII's yield preview): per item `{item,
 * resource, perHour, cost, copies, secs, short: [{resource, need}]}` —
 * `copies` the ones already built (production above the tier's base) or
 * queued, `short` what the stores lack for the first copy's cost (later
 * copies cost more: the program's duplicate cost; the card says so).
 */
export function buildCards(h, stores) {
  const base = baseProduction(Number(h.tier ?? 0));
  const have = Object.fromEntries(stores.map(s => [s.resource, s.value]));
  return BUILD_ITEMS.map(b => {
    const r = RESOURCE_ORDER.indexOf(b.resource);
    const above = r >= 0 && h.production?.[r] !== undefined ? Number(BigInt(h.production[r]) - (base[r] ?? 0n)) / 1000 : 0;
    const queued = (h.queue ?? []).filter(x => Number(x.doneAt) > 0 && x.kind === b.item).length;
    const copies = (b.perHour > 0 ? Math.max(0, Math.round(above / b.perHour)) : 0) + queued;
    const short = (b.cost ?? []).map((c, i) => ({ resource: RESOURCE_ORDER[i], need: c - (have[RESOURCE_ORDER[i]] ?? 0) })).filter(x => x.need > 0);
    return { item: b.item, resource: b.resource, perHour: b.perHour, cost: b.cost, copies, secs: buildSecs(copies), short };
  });
}

/** The panel's view model (numbers the tests read): stores, facts, the actions' blocks. */
export function holdingModel(FS) {
  const h = activeHolding(FS);
  if (!h) return null;
  const now = FS.chain?.now() ?? 0;
  const env = FS.provinces?.get(`${h.p},${h.q}`);
  const ctx = { holding: h, citizen: FS.citizen, province: env?.province ?? null, nowBell: FS.nowBell ?? 0, now };
  return {
    holding: h,
    stores: storesAt(h, now, RESOURCE_ORDER),
    facts: holdingFacts(h, FS.season, now),
    blocks: Object.fromEntries(['Harvest', 'Build', 'Train', 'Muster', 'Garrison'].map(n => [n, actionBlocks(n, ctx)])),
  };
}

const PANELS = [
  { id: 'hp-harvest', text: () => L`収穫`, glyph: '❦' }, { id: 'hp-build', text: () => L`建設`, glyph: '⚒' },
  { id: 'hp-train', text: () => L`訓練`, glyph: '⚔' }, { id: 'hp-muster', text: () => L`編成`, glyph: '⚑' }, { id: 'hp-garrison', text: () => L`守備`, glyph: '⛨' },
];

export function render(FS) {
  const m = holdingModel(FS);
  if (!m) return html`<p>${L`まだ拠点がありません。`}</p>`;
  const { holding: h, facts: f } = m;
  const state = HOLDING_STATES[f.state];
  const units = UNIT_ORDER.map((u, i) => ({ u, i })).filter(x => x.i !== SETTLER);
  const full = m.stores.filter(s => s.cap > 0 && s.value >= s.cap);
  const queueFull = f.queue.filter(q => q.doneIn > 0).length >= 4;
  const cards = buildCards(h, m.stores);
  return html`<section aria-labelledby="holding-title" class="holding"><h3 id="holding-title">${holdingName(h)} <span class="muted">${L`拠点 州 ${h.p},${h.q} 区画 ${h.site + 1}`}</span></h3>
    <dl class="facts">
      ${row(L`段階`, `${TIERS[h.tier] ?? h.tier} · ${state}`)}
      ${f.state === 'provisional' ? row(L`確定`, L`同じ鐘の入植希望がすべて決まってから`) : ''}
      ${f.shieldLeft > 0 ? row(L`保護`, inTime(f.shieldLeft)) : ''}
      ${row(L`休眠まで`, f.dormantIn > 0 ? inTime(f.dormantIn) : L`休眠中`)}
    </dl>
    <nav class="hp-nav" aria-label="${L`拠点の操作`}">${PANELS.map(x => html`<button type="button" class="hp-chip" data-act="hp-jump" data-id="${x.id}"><span aria-hidden="true">${x.glyph}</span>${x.text()}</button>`)}</nav>

    <section class="hpanel" id="hp-harvest" aria-labelledby="hp-harvest-h"><h4 id="hp-harvest-h">❦ ${L`資源`}</h4>
    <table class="stores"><thead><tr><th scope="col">${L`資源`}</th><th scope="col">${L`量`}</th><th scope="col">${L`毎時`}</th><th scope="col">${L`上限`}</th></tr></thead>
      <tbody>${m.stores.map(s => html`<tr class="${s.cap > 0 && s.value >= s.cap ? 'store-full' : ''}"><th scope="row"><span class="res-dot res-c-${s.resource}" aria-hidden="true"></span> ${RESOURCES[s.resource]}</th><td>${fmtNum(s.value)}${s.cap > 0 && s.value >= s.cap ? html` <span class="flag">${L`満杯`}</span>` : ''}</td><td>+${fmtNum(s.perHour)}</td><td>${fmtNum(s.cap)}</td></tr>`)}</tbody></table>
    <p><button type="button" class="btn primary" data-act="harvest" ${raw(m.blocks.Harvest.length ? 'disabled' : '')}>${L`収穫する`}</button>
      ${full.length ? html`<span class="muted">${L`${full.map(s => RESOURCES[s.resource]).join(' / ')}が満杯：これ以上は増えません`}</span>` : ''}</p>
    ${blockedLine(m.blocks.Harvest)}</section>

    <section class="hpanel" id="hp-build" aria-labelledby="hp-build-h"><h4 id="hp-build-h">⚒ ${L`建設（列は4つまで）`}</h4>
    ${f.queue.length ? html`<ul class="list hp-queue">${f.queue.map(q => html`<li>${BUILDINGS[BUILD_ITEMS[q.kind]?.resource] ?? `#${q.kind}`} · ${q.doneIn > 0 ? html`${L`あと ${span(q.doneIn)}`}` : L`完成`}</li>`)}</ul>` : html`<p class="muted">${L`建設の列は空です`}</p>`}
    <ul class="build-cards">${cards.map(c => {
      const blocked = m.blocks.Build.length || queueFull || c.short.length;
      return html`<li class="build-card ${c.short.length ? 'short' : ''}"><button type="button" class="btn" data-act="build" data-item="${c.item}" ${raw(blocked ? 'disabled' : '')}>${BUILDINGS[c.resource]}</button>
        <span class="bc-yield">${c.perHour ? html`<span class="res-dot res-c-${c.resource}" aria-hidden="true"></span> ${L`${RESOURCES[c.resource]} +${c.perHour}/時`}` : L`守りを固める`}</span>
        <span class="bc-time">⏱ ${span(c.secs)}${c.copies ? html` <span class="muted">${L`（${fmtNum(c.copies + 1)}つ目）`}</span>` : ''}</span>
        <span class="bc-cost">${c.cost ? (c.copies ? L`${costText(c.cost)}（1つ目の費用。2つ目からは高くなります）` : costText(c.cost)) : L`石材で守りを固める（鐘の始まりの守り手に数えられるのは完成した次の鐘から）`}</span>
        ${c.short.length ? html`<span class="bc-short">${L`足りない：${c.short.map(x => `${RESOURCES[x.resource]} ${fmtNum(x.need)}`).join(' · ')}`}</span>` : ''}</li>`;
    })}</ul>
    ${queueFull ? html`<p class="blocked">${L`建設の列がいっぱいです（4つまで）`}</p>` : ''}
    ${blockedLine(m.blocks.Build)}</section>

    <section class="hpanel" id="hp-train" aria-labelledby="hp-train-h"><h4 id="hp-train-h">⚔ ${L`訓練（すぐに終わります）`}</h4>
    <form class="inline" data-form="train"><label>${L`兵種`}<select name="unit">${units.map(x => html`<option value="${x.i}">${UNITS[x.u]}</option>`)}</select></label>
      <label>${L`人数`}<input name="n" type="number" min="1" step="1" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Train.length ? 'disabled' : '')}>${L`訓練する`}</button></form>
    ${blockedLine(m.blocks.Train)}
    <h4>${L`控えの兵`}</h4>
    ${f.reserve.length ? html`<ul class="list">${f.reserve.map(r => html`<li>${UNITS[UNIT_ORDER[r.unit]]} ${fmtNum(r.troops)}</li>`)}</ul>` : html`<p class="muted">${L`控えの兵はいません`}</p>`}</section>

    <section class="hpanel" id="hp-muster" aria-labelledby="hp-muster-h"><h4 id="hp-muster-h">⚑ ${L`軍勢を編成する`}</h4>
    <form class="inline" data-form="muster"><label>${L`兵種`}<select name="unit">${f.reserve.filter(r => r.unit !== SETTLER).map(r => html`<option value="${r.unit}">${UNITS[UNIT_ORDER[r.unit]]}</option>`)}</select></label>
      <label>${L`兵数（100〜30,000）`}<input name="troops" type="number" min="100" max="30000" step="1" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Muster.length || !f.reserve.length ? 'disabled' : '')}>${L`編成する`}</button></form>
    ${!f.reserve.length ? html`<p class="blocked">${L`控えの兵がいません：先に訓練しましょう`}</p>` : ''}
    <p class="muted">${L`新しい軍勢は次の鐘から顔ぶれに加わります。`}</p>
    ${blockedLine(m.blocks.Muster)}
    <h4>${L`進軍の記録（4つ）`}</h4>
    ${f.transits.length ? html`<ul class="list">${f.transits.map(t => html`<li>${L`枠 ${t.slot + 1}：第${fmtNum(t.arriveBell)}鐘に到着`}</li>`)}</ul>` : html`<p class="muted">${L`進軍中の軍勢はいません`}</p>`}</section>

    <section class="hpanel" id="hp-garrison" aria-labelledby="hp-garrison-h"><h4 id="hp-garrison-h">⛨ ${L`守備隊`}</h4>
    <form class="inline" data-form="garrison"><label>${L`増員（控えの兵から）`}<input name="delta" type="number" min="1" step="1" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Garrison.length ? 'disabled' : '')}>${L`守備隊を増やす`}</button></form>
    <p class="muted">${L`M1 の守備隊は増やすだけです（引き上げは次の段階で）。`}</p>
    ${blockedLine(m.blocks.Garrison)}
    <h4>${L`夜番の時間`}</h4>
    <p class="muted">${L`${String(Math.floor((FS.citizen?.vigilStartMin ?? 0) / 60)).padStart(2, '0')}:${String((FS.citizen?.vigilStartMin ?? 0) % 60).padStart(2, '0')} から（UTC）`}</p>
    <form class="inline" data-form="vigil"><label>${L`開始（UTC の時）`}<input name="hour" type="number" min="0" max="23" value="${Math.floor((FS.citizen?.vigilStartMin ?? 0) / 60)}" inputmode="numeric"></label>
      <button type="submit" class="btn">${L`変える`}</button></form>
    <p class="muted">${L`変更は24時間以上あとの最初の UTC 0時から有効で、週に1回までです。`}</p></section>
    ${f.finalTs && f.state === 'provisional' ? html`<p class="muted">${Lh`早くても ${timeHtml(f.finalTs)} 以降に確定します。`}</p>` : ''}
  </section>`;
}
