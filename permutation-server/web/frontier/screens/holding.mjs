// The holding panel (web design §7.4; contract §5.10, I-29, I-56): header
// (site, tier, provisional/final, shield, dormancy), the eight stores
// settled lazily as the kernel does, the build queue and the build list
// (first-copy costs), training (immediate, I-56), the trained reserve,
// muster into a host (joins the roster from the next bell), garrison,
// vigil hours, and the transit records. Every action says why it cannot
// run now, with a "catch up" nudge when the province lags (NotResident).
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { RESOURCES, RESOURCE_ORDER, UNITS, TIERS, BUILDINGS, HOLDING_STATES, errorText } from '../fi18n.mjs';
import { storesAt, holdingFacts, BUILD_ITEMS, UNIT_ORDER, SETTLER, actionBlocks } from '../fland.mjs';
import { inTime, row, timeHtml } from './shell.mjs';

const costText = cost => cost.map((c, i) => (c ? `${RESOURCES[RESOURCE_ORDER[i]]} ${fmtNum(c)}` : null)).filter(Boolean).join(' · ');

/** Why an action is blocked, as text, with the catch-up offer for a lagging province. */
export function blockedLine(blocks) {
  if (!blocks.length) return '';
  const lag = blocks.includes('NotResident');
  return html`<p class="blocked">${blocks.map(errorText).join(' / ')}${lag ? html` <button type="button" class="btn small" data-act="nudge">${L`追いつかせる`}</button>` : ''}</p>`;
}

/** The panel's view model (numbers the tests read): stores, facts, the actions' blocks. */
export function holdingModel(FS) {
  const h = FS.holdings?.[0];
  if (!h) return null;
  const now = FS.chain?.now() ?? 0;
  const env = FS.provinces?.get(`${h.p},${h.q}`);
  const ctx = { holding: h, citizen: FS.citizen, province: env?.province ?? null, nowBell: FS.nowBell ?? 0 };
  return {
    holding: h,
    stores: storesAt(h, now, RESOURCE_ORDER),
    facts: holdingFacts(h, FS.season, now),
    blocks: Object.fromEntries(['Harvest', 'Build', 'Train', 'Muster', 'Garrison'].map(n => [n, actionBlocks(n, ctx)])),
  };
}

export function render(FS) {
  const m = holdingModel(FS);
  if (!m) return html`<p>${L`まだ拠点がありません。`}</p>`;
  const { holding: h, facts: f } = m;
  const state = HOLDING_STATES[f.state];
  const units = UNIT_ORDER.map((u, i) => ({ u, i })).filter(x => x.i !== SETTLER);
  return html`<section aria-labelledby="holding-title"><h3 id="holding-title">${L`拠点 州 ${h.p},${h.q} 区画 ${h.site + 1}`}</h3>
    <dl class="facts">
      ${row(L`段階`, `${TIERS[h.tier] ?? h.tier} · ${state}`)}
      ${f.state === 'provisional' ? row(L`確定`, L`同じ鐘の入植希望がすべて決まってから`) : ''}
      ${f.shieldLeft > 0 ? row(L`保護`, inTime(f.shieldLeft)) : ''}
      ${row(L`休眠まで`, f.dormantIn > 0 ? inTime(f.dormantIn) : L`休眠中`)}
      ${row(L`夜番の時間`, L`${String(Math.floor((FS.citizen?.vigilStartMin ?? 0) / 60)).padStart(2, '0')}:${String((FS.citizen?.vigilStartMin ?? 0) % 60).padStart(2, '0')} から（UTC）`)}
    </dl>
    <h4>${L`資源`}</h4>
    <table class="stores"><thead><tr><th scope="col">${L`資源`}</th><th scope="col">${L`量`}</th><th scope="col">${L`毎時`}</th><th scope="col">${L`上限`}</th></tr></thead>
      <tbody>${m.stores.map(s => html`<tr><th scope="row">${RESOURCES[s.resource]}</th><td>${fmtNum(s.value)}</td><td>${fmtNum(s.perHour)}</td><td>${fmtNum(s.cap)}</td></tr>`)}</tbody></table>
    <button type="button" class="btn" data-act="harvest" ${raw(m.blocks.Harvest.length ? 'disabled' : '')}>${L`収穫する`}</button>
    <h4>${L`建設（列は4つまで）`}</h4>
    ${f.queue.length ? html`<ul class="list">${f.queue.map(q => html`<li>${BUILDINGS[BUILD_ITEMS[q.kind]?.resource] ?? `#${q.kind}`} · ${q.doneIn > 0 ? inTime(q.doneIn) : L`完成`}</li>`)}</ul>` : ''}
    <ul class="list">${BUILD_ITEMS.map(b => html`<li><button type="button" class="btn" data-act="build" data-item="${b.item}" ${raw(m.blocks.Build.length || f.queue.length >= 4 ? 'disabled' : '')}>${BUILDINGS[b.resource]}</button>
      <span class="muted">${b.cost ? L`${costText(b.cost)}（1つ目）· ${RESOURCES[b.resource]} +${b.perHour}/時` : L`石材で守りを固める（鐘の始まりの守り手に数えられるのは完成した次の鐘から）`}</span></li>`)}</ul>
    ${blockedLine(m.blocks.Build)}
    <h4>${L`訓練（すぐに終わります）`}</h4>
    <form class="inline" data-form="train"><label>${L`兵種`}<select name="unit">${units.map(x => html`<option value="${x.i}">${UNITS[x.u]}</option>`)}</select></label>
      <label>${L`人数`}<input name="n" type="number" min="1" step="100" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Train.length ? 'disabled' : '')}>${L`訓練する`}</button></form>
    <h4>${L`控えの兵`}</h4>
    ${f.reserve.length ? html`<ul class="list">${f.reserve.map(r => html`<li>${UNITS[UNIT_ORDER[r.unit]]} ${fmtNum(r.troops)}</li>`)}</ul>` : html`<p class="muted">${L`控えの兵はいません`}</p>`}
    <h4>${L`軍勢を編成する`}</h4>
    <form class="inline" data-form="muster"><label>${L`兵種`}<select name="unit">${f.reserve.filter(r => r.unit !== SETTLER).map(r => html`<option value="${r.unit}">${UNITS[UNIT_ORDER[r.unit]]}</option>`)}</select></label>
      <label>${L`兵数（100〜30,000）`}<input name="troops" type="number" min="100" max="30000" step="100" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Muster.length || !f.reserve.length ? 'disabled' : '')}>${L`編成する`}</button></form>
    <p class="muted">${L`新しい軍勢は次の鐘から顔ぶれに加わります。`}</p>
    ${blockedLine(m.blocks.Muster)}
    <h4>${L`守備隊`}</h4>
    <form class="inline" data-form="garrison"><label>${L`増員（控えの兵から）`}<input name="delta" type="number" min="1" step="100" value="100" inputmode="numeric"></label>
      <button type="submit" class="btn" ${raw(m.blocks.Garrison.length ? 'disabled' : '')}>${L`守備隊を増やす`}</button></form>
    <p class="muted">${L`M1 の守備隊は増やすだけです（引き上げは次の段階で）。`}</p>
    ${blockedLine(m.blocks.Garrison)}
    <h4>${L`夜番の時間`}</h4>
    <form class="inline" data-form="vigil"><label>${L`開始（UTC の時）`}<input name="hour" type="number" min="0" max="23" value="${Math.floor((FS.citizen?.vigilStartMin ?? 0) / 60)}" inputmode="numeric"></label>
      <button type="submit" class="btn">${L`変える`}</button></form>
    <p class="muted">${L`変更は24時間以上あとの最初の UTC 0時から有効で、週に1回までです。`}</p>
    <h4>${L`進軍の記録（4つ）`}</h4>
    ${f.transits.length ? html`<ul class="list">${f.transits.map(t => html`<li>${L`枠 ${t.slot + 1}：第${fmtNum(t.arriveBell)}鐘に到着`}</li>`)}</ul>` : html`<p class="muted">${L`進軍中の軍勢はいません`}</p>`}
    ${f.finalTs && f.state === 'provisional' ? html`<p class="muted">${Lh`早くても ${timeHtml(f.finalTs)} 以降に確定します。`}</p>` : ''}
  </section>`;
}
