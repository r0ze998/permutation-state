// The chronicle: public events of the season, filterable.
import * as T from '../i18n.mjs';
import { html } from '../util.mjs';
import { S } from '../state.mjs';

const GROUPS = { all: null, war: ['war', 'capture', 'raze', 'revolt'], diplo: ['peace', 'ally', 'diplo'], growth: ['found', 'science', 'tech'] };
const FILTERS = [['all', 'すべて'], ['war', '戦い'], ['diplo', '外交'], ['growth', '発展']];

export function drawerChronicle() {
  const f = S.chronFilter;
  const rows = (S.view.chronicle || []).map(e => ({ tick: e.tick, kt: T.chronicleText(e.text) })).filter(e => !GROUPS[f] || GROUPS[f].includes(e.kt[0]));
  return html`<div class="eyebrow">CHRONICLE · 年代記</div><h2>年代記</h2>
    <div class="row">${FILTERS.map(([k, n]) => html`<button class="btn ${f === k ? 'primary' : ''}" type="button" data-chron="${k}">${n}</button>`)}</div>
    <div style="margin-top:8px">${rows.length ? rows.map(e => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title" style="font-weight:400">${T.KIND_GLYPH[e.kt[0]] || '·'} ${e.kt[1]}</div></div><span class="meta">t${e.tick}</span></div>`) : html`<p class="desc">まだ記録はありません。</p>`}</div>`;
}
