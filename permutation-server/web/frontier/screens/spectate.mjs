// The spectator page (web design §7.13; contract §13.6 E7): the map, the
// bell sheet, the latest clash reports and the chronicle, with no wallet.
// It reads the herald exactly like the game page (the same §4.2 rules: the
// events poll every 30 s, paused while the page is hidden; the immutable
// files come from the LRU), and it keeps only bounded state — the chronicle
// window (400 records), the last reports (at most 12 links, one report
// open) — so a spectator left open for a game day stays within the memory
// budget of E7 (≤ 200 MB growth).
import { html } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { regionOf } from '../fgeo.mjs';
import { bellAt } from '../clock.mjs';
import * as bellScreen from './bell.mjs';
import * as chronicleScreen from './chronicle.mjs';
import * as reportScreen from './report.mjs';

/** Recent clash links shown (newest first). */
export const SPECTATE_REPORTS = 12;

/** The bell sheet's rows for a spectator: the current bell and the bells of the latest clashes, by region. */
export function spectateBells(FS, now) {
  const items = [];
  const cur = FS.clock ? bellAt(FS.clock.genesisTs, now) : null;
  const clashes = reportScreen.clashesFrom(FS.chronicle ?? [], 4);
  if (cur !== null) {
    const regions = [...new Set(clashes.map(c => regionOf(c.p, c.q)))];
    for (const r of regions.length ? regions : [0]) items.push({ bell: cur, region: r, why: 'current', facts: {} });
  }
  for (const c of clashes) items.push({ bell: c.bell, region: regionOf(c.p, c.q), why: 'clash', facts: { resolvedNext: c.bell + 1 } });
  return items;
}

/** The spectator's panel. */
export function render(FS) {
  if (FS.report) return reportScreen.render(FS, () => false, { whatIf: false });
  const clashes = reportScreen.clashesFrom(FS.chronicle ?? [], SPECTATE_REPORTS);
  return [
    html`<p class="muted">${L`ウォレットなしで地図と鐘の進み具合を見られます。`}</p>`,
    FS.chain && FS.clock ? bellScreen.render(FS) : '',
    reportScreen.renderLinks(clashes, L`最近の衝突`) || html`<p class="muted">${L`まだ衝突はありません`}</p>`,
    chronicleScreen.render(FS),
    FS.record ? html`<p class="as-of">${L`スロット ${fmtNum(FS.record.latestSlot ?? 0)} 時点`}</p>` : '',
  ];
}
