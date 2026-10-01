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
import { highlights, renderHighlights, provinceFactions } from '../people/ui.mjs';
import { factionName } from '../fi18n.mjs';
import { swatch } from './shell.mjs';

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

/**
 * The bells of the chronicle window that had events, newest first, with
 * their counts: `[{bell, events, clashes}]` (the spectator's timeline, UI plan G3).
 */
export function eventBells(chronicle, limit = 12) {
  const by = new Map();
  for (const x of chronicle ?? []) {
    const r = x.record ?? x;
    if (!['CLASH', 'DEPART', 'SETTLE', 'EXPLORE'].includes(r.name)) continue;
    const b = Number(r.bell);
    const e = by.get(b) ?? { bell: b, events: 0, clashes: 0 };
    e.events++; if (r.name === 'CLASH') e.clashes++;
    by.set(b, e);
  }
  return [...by.values()].sort((a, b) => b.bell - a.bell).slice(0, limit);
}

/** The spectator's controls: faction filter, the bell timeline, the camera following battles. */
function renderWatch(FS) {
  const w = FS.watch ?? {};
  const bells = eventBells(FS.chronicle);
  return html`<section class="watch" aria-labelledby="watch-title"><h3 id="watch-title">${L`観戦の設定`}</h3>
    <div class="choice-row" role="group" aria-label="${L`陣営で絞る`}"><span>${L`陣営で絞る`}</span>
      <button type="button" class="btn small" data-act="watch-faction" data-f="" aria-pressed="${w.faction === null || w.faction === undefined ? 'true' : 'false'}">${L`すべて`}</button>
      ${[0, 1, 2, 3, 4, 5].map(f => html`<button type="button" class="btn small" data-act="watch-faction" data-f="${f}" aria-pressed="${w.faction === f ? 'true' : 'false'}">${swatch(f)}${factionName(f)}</button>`)}</div>
    ${bells.length ? html`<div class="watch-bells" role="group" aria-label="${L`鐘の年表`}">
      <button type="button" class="btn small" data-act="watch-bell" data-bell="" aria-pressed="${w.bell === null || w.bell === undefined ? 'true' : 'false'}">${L`最新`}</button>
      ${bells.map(b => html`<button type="button" class="btn small wb${b.clashes ? ' wb-clash' : ''}" data-act="watch-bell" data-bell="${b.bell}" aria-pressed="${w.bell === b.bell ? 'true' : 'false'}">${L`第${fmtNum(b.bell)}鐘`}${b.clashes ? html` <span aria-hidden="true">⚔</span>` : ''} <span class="muted">${fmtNum(b.events)}</span></button>`)}</div>` : ''}
    <label class="choice"><input type="checkbox" data-act="watch-auto" ${w.auto === false ? '' : 'checked'}>${L`新しい戦いが起きたら、地図をそこへ動かして見せる`}</label></section>`;
}

/** The spectator's panel. */
export function render(FS, { ownerOf = null, standings = '' } = {}) {
  if (FS.report) return reportScreen.render(FS, () => false, { whatIf: false, ownerOf });
  const w = FS.watch ?? {};
  const f = Number.isInteger(w.faction) ? w.faction : null, b = Number.isInteger(w.bell) ? w.bell : null;
  const clashes = reportScreen.clashesFrom(FS.chronicle ?? [], 200)
    .filter(c => (b === null || c.bell === b) && (f === null || provinceFactions(FS.overviews, c.p, c.q).includes(f))).slice(0, SPECTATE_REPORTS);
  const items = highlights(FS.chronicle, FS.overviews, FS.roster, { limit: 12, faction: f, bell: b });
  const scope = [f !== null ? factionName(f) : null, b !== null ? L`第${fmtNum(b)}鐘` : null].filter(Boolean).join(' · ');
  return [
    html`<p class="muted">${L`ウォレットなしで地図と鐘の進み具合を見られます。`}</p>`,
    renderWatch(FS),
    standings ? html`<details class="watch-mini"><summary>${L`勢力の順位`}</summary>${standings}</details>` : '',
    html`<section aria-labelledby="watch-hl-title"><h3 id="watch-hl-title">${scope ? L`見どころ（${scope}）` : L`見どころ`}</h3>${renderHighlights(items, { go: true })}</section>`,
    FS.chain && FS.clock ? bellScreen.render(FS) : '',
    reportScreen.renderLinks(clashes, L`最近の衝突`) || html`<p class="muted">${L`まだ衝突はありません`}</p>`,
    chronicleScreen.render(FS),
    FS.record ? html`<p class="as-of">${L`スロット ${fmtNum(FS.record.latestSlot ?? 0)} 時点`}</p>` : '',
  ];
}
