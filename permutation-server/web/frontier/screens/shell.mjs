// The play shell (web design §7.1): the faction and quota chips beside the
// bell chip, the bottom tabs (Map, Holding, Hosts, Marches, More), the
// panel each tab shows, and the shared bits the screens use (lamports,
// times, the action notice). Renderers are pure: they read the store and
// return markup; app.mjs puts it in the page and routes `data-act` clicks.
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum, lang } from '../../lang.mjs';
import { factionName, failureText } from '../fi18n.mjs';
import { countdown } from '../clock.mjs';

export const TAB_IDS = Object.freeze(['map', 'holding', 'hosts', 'marches', 'more']);
const TAB_TEXT = { map: () => L`地図`, holding: () => L`拠点`, hosts: () => L`軍勢`, marches: () => L`進軍`, more: () => L`その他` };
export const tabLabel = id => TAB_TEXT[id]?.() ?? id;

/** Lamports with thousands separators ("14,441 lamports"). */
export const lamports = n => L`${fmtNum(Number(n))} ランポート`;
/** A chain time as the viewer's local hh:mm (UTC in the title attribute). */
export function clockTime(t) {
  const d = new Date(Number(t) * 1000);
  const loc = lang() === 'en' ? 'en-GB' : 'ja-JP';
  return { local: d.toLocaleTimeString(loc, { hour: '2-digit', minute: '2-digit' }), utc: d.toISOString().slice(11, 16) };
}
export const timeHtml = t => { const c = clockTime(t); return html`<time datetime="${new Date(Number(t) * 1000).toISOString()}" title="${c.utc} UTC">${c.local}</time>`; };
/** "in 6:12" / "6:12 ago". */
export const inTime = secs => (secs >= 0 ? L`あと ${countdown(secs)}` : L`${countdown(-secs)} 前`);

/** The faction chip's text, or null before joining. */
export const factionChip = citizen => (citizen ? factionName(citizen.faction) : null);
/** The quota chip: sponsored transactions left today (§8.3), or null. */
export const quotaChip = q => (q && Number.isFinite(Number(q.left)) ? L`中継 残り ${fmtNum(Number(q.left))} 回` : null);

/** The tab bar: `[{id, label, current}]`; Holding and Hosts only once the viewer holds land. */
export function tabs(FS) {
  const land = FS.land?.stage;
  const holds = land === 'provisional' || land === 'final';
  return TAB_IDS.filter(id => holds || !['holding', 'hosts'].includes(id)).map(id => ({ id, label: tabLabel(id), current: (FS.tab ?? 'map') === id }));
}

export function renderTabs(FS) {
  return tabs(FS).map(t => html`<button type="button" class="tab" data-act="tab" data-tab="${t.id}" ${raw(t.current ? 'aria-current="page"' : '')}>${t.label}</button>`);
}

/** The last action's outcome line: busy, done or the failure's text (§9.6). */
export function renderNotice(n) {
  if (!n) return '';
  const text = typeof n.text === 'function' ? n.text() : n.text;
  if (n.busy) return html`<p class="notice busy" role="status">${text ?? L`送信中…`}</p>`;
  if (n.ok) return html`<p class="notice ok" role="status">${text ?? L`完了しました`}</p>`;
  return html`<p class="notice error" role="alert">${text ?? failureText(n)}</p>`;
}

/**
 * A faction's colour swatch (colour is never the only signal: the name is
 * next to it). A class, not a style attribute: the CSP allows no inline
 * style (frontier.css `.f0`–`.f6` carry FACTION_COLORS).
 */
export const swatch = f => html`<span class="swatch f${Number.isInteger(f) && f >= 0 && f <= 6 ? f : 6}" aria-hidden="true"></span>`;

/** A labelled definition list row. */
export const row = (dt, dd) => html`<div class="row"><dt>${dt}</dt><dd>${dd}</dd></div>`;

/** The "as of" line every panel carries (web design §4.2). */
export const asOf = (slot, age) => (slot === null || slot === undefined ? '' : Lh`<p class="as-of">スロット ${fmtNum(slot)} 時点（${Math.round(age ?? 0)} 秒前）</p>`);
