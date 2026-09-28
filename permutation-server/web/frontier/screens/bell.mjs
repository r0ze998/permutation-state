// The bell sheet (web design §6.2–§6.4; contract §5.1): the current bell,
// then the bells that matter to the viewer (arrivals of its marches, the
// ticket bell) with the pipeline state of each in its region — open,
// awaiting the beacon (no countdown before THE anchor exists), revealing
// until A + W, awaiting the seed, resolving, resolved — and the local
// clock's offset when it is more than 2 s off the chain's. The relay quota
// line too: on phones the quota chip is hidden and "More" (this sheet) is
// where it is read (wave-5 review of W5-E, D5).
import { html } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { PIPELINE_TEXT } from '../fi18n.mjs';
import { pipeline, countdown } from '../clock.mjs';
import { timeHtml, quotaChip } from './shell.mjs';

/**
 * The sheet's rows: `items = [{bell, region, why, facts: {anchor, archive,
 * seed, latestRound, resolvedNext}}]` → `[{bell, region, why, state, until,
 * close}]` at chain time `now`.
 */
export function sheetRows(clock, now, items) {
  return items.map(it => ({ bell: it.bell, region: it.region, why: it.why, ...pipeline(clock, it.bell, { now, ...(it.facts ?? {}) }) }));
}

const WHY = { current: () => L`いまの鐘`, arrival: () => L`あなたの到着`, ticket: () => L`入植希望`, explore: () => L`探索`, clash: () => L`衝突` };

export function render(FS) {
  if (!FS.clock || !FS.chain) return html`<p class="muted">${L`読み込み中…`}</p>`;
  const now = FS.chain.now() ?? 0;
  const rows = sheetRows(FS.clock, now, FS.bellItems ?? []);
  // The local clock's offset means something only on a real cluster (a localnet Clock runs scaled).
  const offset = FS.pin?.cluster === 'localnet' ? 0 : FS.chain.offset?.() ?? 0;
  const current = FS.clock ? Math.max(0, Math.floor((now - FS.clock.genesisTs) / 600)) : null;
  return html`<section aria-labelledby="bell-title"><h3 id="bell-title">${L`鐘の進み具合`}</h3>
    ${current !== null && now >= FS.clock.genesisTs ? html`<p>${L`いまは第${fmtNum(current)}鐘`}</p>` : ''}
    ${Math.abs(offset) > 2 ? html`<p class="muted">${L`この端末の時計はチェーンより ${Math.round(offset)} 秒ずれています`}</p>` : ''}
    ${quotaChip(FS.quota) ? html`<p class="quota-line" data-quota-line>${quotaChip(FS.quota)}</p>` : ''}
    <ul class="list bells">${rows.map(r => html`<li><strong>${L`第${fmtNum(r.bell)}鐘`}</strong>${WHY[r.why] ? html` · ${WHY[r.why]()}` : ''}${Number.isInteger(r.region) ? html` · ${L`地域 ${r.region}`}` : ''}
      <p>${PIPELINE_TEXT[r.state]}</p>
      ${r.until !== null && r.until !== undefined ? html`<p class="muted">${countdown(r.until - now)} · ${timeHtml(r.until)}</p>` : ''}
      ${r.archived ? html`<p class="muted">${L`記録庫から読んでいます`}</p>` : ''}</li>`)}</ul>
    <details><summary>${L`鐘のしくみ`}</summary><p>${L`到着は鐘の始まりまでに封をされ、守り手の顔ぶれは鐘の始まりで固まります。鐘の終わりの後の最初のビーコンが記録されると、誰でも封を開けられます。開封の受付は記録から決まった時間で閉じ、その後のビーコンの乱数で衝突が決まります。`}</p></details>
  </section>`;
}
