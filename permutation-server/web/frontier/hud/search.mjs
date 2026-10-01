// Map search (UI plan C5; Civ VI's map search, missed in Civ VII): one box
// that finds a province by its coordinates ("3,0", "州 3,0"), a faction's
// lands by its name, and a lord by name (the roster's derived names, in
// both languages). Results are buttons that move the map there.
import { html } from '../../util.mjs';
import { L, lang } from '../../lang.mjs';
import { factionName } from '../fi18n.mjs';
import { provincePixel } from '../map/layers.mjs';
import { identityOf, displayName } from '../people/identity.mjs';
import { majority } from './minimap.mjs';

export const SEARCH_MAX = 8;
const norm = s => String(s ?? '').normalize('NFKC').toLowerCase().replace(/\s+/g, ' ').trim();

/**
 * The results for a query: `[{kind, text, p, q, tile?}]`. `recs` the
 * overview records by "P,Q"; `roster` the roster store (entries()).
 */
export function searchMap(query, { recs = new Map(), roster = null, sitesOf = () => null, pins = [] } = {}) {
  const q = norm(query);
  if (!q) return [];
  const out = [];
  // "ピン" / "pin": the viewer's pins (hud/pins.mjs)
  if (/^(?:\u30d4\u30f3|pins?)$/u.test(q)) for (const x of pins) out.push({ kind: 'pin', text: x.tile === null ? L`州 ${x.p},${x.q}` : L`州 ${x.p},${x.q} · マス ${x.tile + 1}`, p: x.p, q: x.q, ...(x.tile === null ? {} : { tile: x.tile }) });
  const m = /^(?:\u5dde|province)?\s*(-?\d+)\s*[,\uff0c\u3001 ]\s*(-?\d+)$/u.exec(q);   // 州 3,0 / province 3,0 / 3、0
  if (m) {
    const p = Number(m[1]), qq = Number(m[2]);
    if (recs.has(`${p},${qq}`) || (p === 0 && qq === 0)) out.push({ kind: 'province', text: L`州 ${p},${qq}`, p, q: qq });
  }
  for (let f = 0; f < 6; f++) {
    if (!norm(factionName(f)).includes(q)) continue;
    // the heart of its lands: the weighted centre of the provinces it holds most of
    let x = 0, y = 0, w = 0, best = null;
    for (const r of recs.values()) if (majority(r) === f) { const c = provincePixel(r.p, r.q); x += c.x; y += c.y; w++; best = best ?? r; }
    if (!w) continue;
    let near = best, d0 = Infinity;
    for (const r of recs.values()) if (majority(r) === f) { const c = provincePixel(r.p, r.q); const d = (c.x - x / w) ** 2 + (c.y - y / w) ** 2; if (d < d0) { d0 = d; near = r; } }
    out.push({ kind: 'faction', text: L`${factionName(f)}の領土`, p: near.p, q: near.q });
  }
  if (roster && q.length >= 2) for (const e of roster.entries()) {
    if (out.length >= SEARCH_MAX) break;
    const id = identityOf(e.tag);
    const names = [displayName(id, { full: true, language: 'en' }), displayName(id, { full: true, language: 'ja' })].map(norm);
    if (!names.some(n => n.includes(q))) continue;
    const tile = sitesOf(e.p, e.q)?.[e.site];
    out.push({ kind: 'lord', text: L`${displayName(id, { full: true })}（州 ${e.p},${e.q}）`, p: e.p, q: e.q, tile: Number.isInteger(tile) ? tile : undefined });
  }
  return out.slice(0, SEARCH_MAX);
}

export function renderResults(list, query) {
  if (!norm(query)) return '';
  if (!list.length) return html`<p class="search-none">${L`見つかりません`}</p>`;
  return html`<ul class="search-list">${list.map((x, i) => html`<li><button type="button" class="search-hit search-${x.kind}" data-act="search-go" data-i="${i}">${x.text}</button></li>`)}</ul>`;
}
export { lang };
