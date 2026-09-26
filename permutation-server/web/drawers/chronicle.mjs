// The chronicle: public events of the season, filterable.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { html, usdcFixed } from '../util.mjs';
import { S, invalidate } from '../state.mjs';
import { L } from '../lang.mjs';

/** The seasons this one follows (the history layer, V5 §17.5). */
export async function loadHistory() {
  S.history = await api.tryGet('/api/history', { lineage: [] });
  invalidate('drawer');
}

function lineage() {
  const seasons = [...(S.history?.lineage || [])].reverse();
  if (!seasons.length) return html`<p class="desc">${L`このシーズンは、どのシーズンの続きでもありません。`}</p>`;
  return seasons.map(x => {
    const r = x.record || { nations: [], cities: [] };
    const rows = r.nations.map((n, c) => ({ c, ...n })).sort((a, b) => Number(b.points) - Number(a.points));
    const taken = r.cities.filter(c => c.capturedFrom != null).length;
    return html`<details class="diplo-civ"><summary><span class="l1"><b>${L`シーズン ${x.seasonId}`}</b><span class="grow"></span><span class="meta">${L`都市 ${r.cities.length} · 奪われた ${taken}`}</span></span></summary>
      <div class="civ-body">${rows.map((n, i) => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title" style="font-weight:400">${i + 1}. ${T.civName((x.nations || [])[n.c] || L`勢力${n.c}`)}</div><div class="meta">${L`第${n.era}時代 · ${n.points}点 · 都市${n.cities} · メンバー${n.members}`}</div></div><span class="meta">${usdcFixed(n.share)} USDC</span></div>`)}
      <p class="when">${L`歴史のルート ${String(x.historyRoot).slice(0, 16)}… · チェーンのシーズン口座と照らし合わせられます`}</p></div></details>`;
  });
}

const GROUPS = { all: null, war: ['war', 'capture', 'raze', 'revolt'], diplo: ['peace', 'ally', 'diplo'], growth: ['found', 'science', 'tech'] };
/** The filter buttons [group, label] (a function: the labels follow the language). */
const FILTERS = () => [['all', L`すべて`], ['war', L`戦い`], ['diplo', L`外交`], ['growth', L`発展`]];

export function drawerChronicle() {
  const f = S.chronFilter;
  const rows = (S.view.chronicle || []).map(e => ({ tick: e.tick, kt: T.chronicleText(e.text) })).filter(e => !GROUPS[f] || GROUPS[f].includes(e.kt[0]));
  return html`<div class="eyebrow">${L`CHRONICLE · 年代記`}</div><h2>${L`年代記`}</h2>
    <div class="row">${FILTERS().map(([k, n]) => html`<button class="btn ${f === k ? 'primary' : ''}" type="button" data-chron="${k}">${n}</button>`)}</div>
    <div style="margin-top:8px">${rows.length ? rows.map(e => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title" style="font-weight:400">${T.KIND_GLYPH[e.kt[0]] || '·'} ${e.kt[1]}</div></div><span class="meta">t${e.tick}</span></div>`) : html`<p class="desc">${L`まだ記録はありません。`}</p>`}</div>
    <div class="section-title">${L`これまでのシーズン（歴史の層）`}</div>${lineage()}`;
}
