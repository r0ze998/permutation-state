// Explore (web design §7.5; contract §5.10 Explore/SettleExplore, I-56): a
// Scout host picks one or two passable tiles next to it that nobody has
// explored; the finds are rolled from the bell's seed (≈ 11–21 minutes
// later). The first three explorations of a citizen always give Works; the
// rest do with chance ½ (M1 has no goods). Keepers settle the result; the
// page offers the settle button once the seed exists.
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { exploreTargets, ticketTimes } from '../fland.mjs';
import { timeHtml } from './shell.mjs';

/** Up to two chosen tiles. */
export const toggleTile = (chosen, tile) => (chosen.includes(tile) ? chosen.filter(t => t !== tile) : chosen.length < 2 ? [...chosen, tile] : chosen);

export function render(FS) {
  const h = FS.holdings?.[0];
  const rec = h?.explore;
  const floor = FS.citizen?.exploresFloorLeft ?? 0;
  const pending = rec && rec.state !== 0;
  const draft = FS.explore;
  let targets = [];
  if (draft?.host) targets = exploreTargets(draft.host.province, draft.host);
  const times = pending && FS.clock ? ticketTimes(FS.clock, rec.bell) : null;
  return html`<section aria-labelledby="explore-title"><h3 id="explore-title">${L`探索`}</h3>
    <p>${floor > 0 ? L`はじめの探索はあと ${floor} 回、必ず功績 4 を得ます。` : L`探索のたびに半々の確率で功績 4 を得ます。`}</p>
    ${pending ? html`<p>${L`州 ${rec.p},${rec.q} の探索の結果を待っています。`}${times ? html` ${Lh`結果は ${timeHtml(times.resultAbout)} ごろに出ます。`}` : ''}</p>
      <button type="button" class="btn" data-act="settle-explore" ${raw(FS.exploreSeedReady ? '' : 'disabled')}>${L`結果を確定する`}</button>
      <p class="muted">${L`ふつうはキーパーが確定します。`}</p>` : ''}
    ${draft?.host && !pending ? html`<p>${L`隣のマスを2つまで選んでください`}</p>
      <ul class="list">${targets.map(t => html`<li><button type="button" class="btn" data-act="explore-tile" data-tile="${t}" aria-pressed="${draft.tiles.includes(t) ? 'true' : 'false'}">${L`マス ${fmtNum(t)}`}</button></li>`)}</ul>
      ${targets.length ? '' : html`<p class="muted">${L`隣に探索できるマスがありません`}</p>`}
      <button type="button" class="btn primary" data-act="explore-send" ${raw(draft.tiles.length ? '' : 'disabled')}>${L`探索に出す`}</button>` : ''}
    ${!draft?.host && !pending ? html`<p class="muted">${L`「軍勢」で斥候の軍勢の「探索」を押してください。`}</p>` : ''}
  </section>`;
}
