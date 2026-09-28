// The play shell (web design §7.1): the faction and quota chips beside the
// bell chip, the bottom tabs (Map, Holding, Hosts, Marches, More), the
// panel each tab shows, and the shared bits the screens use (lamports,
// times, the action notice). Renderers are pure: they read the store and
// return markup; app.mjs puts it in the page and routes `data-act` clicks.
//
// W5-E (web design §7.1, §10): on phones (< 760 px) the panel is a bottom
// sheet with three heights — peek, half, full — changed by its handle
// (tap cycles, a drag up or down moves one step), collapsed to peek by
// Escape (focus returns to the handle), opened to half when a tab is
// chosen, and sized to the visual viewport so the on-screen keyboard
// never hides a field. The sheet mounts itself on the Frontier pages
// (every page imports this module); on desktop the handle is hidden and
// the panel is the right column.
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum, lang, onLangChange } from '../../lang.mjs';
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

// ------------------------------------------------------------------ the bottom sheet (phones)
export const SHEET_STATES = Object.freeze(['peek', 'half', 'full']);
/** The next height when the handle is tapped: peek → half → full → peek. */
export const nextSheet = s => SHEET_STATES[(SHEET_STATES.indexOf(s) + 1) % SHEET_STATES.length] ?? 'half';
/** One step up (-1) or down (+1) after a drag of `dy` px (a drag under 24 px is a tap). */
export function dragSheet(s, dy) {
  if (Math.abs(dy) < 24) return null;
  const i = Math.max(0, SHEET_STATES.indexOf(s));
  return SHEET_STATES[Math.min(SHEET_STATES.length - 1, Math.max(0, i + (dy < 0 ? 1 : -1)))];
}
/** The handle's accessible name for a height. */
export const sheetLabel = s => (s === 'full' ? L`パネルを小さくする` : L`パネルを広げる`);
/** Phones only: the sheet exists below the desktop breakpoint (frontier.css). */
export const PHONE_MAX = 759;

/**
 * Mount the sheet on `#panel` (idempotent): the handle first in the panel,
 * `data-sheet` on the panel, the keyboard, drag and tab rules, and the
 * visual-viewport height in `--vvh`. Returns `{set, state}` or null.
 */
export function mountSheet(doc = globalThis.document, win = globalThis.window) {
  const panel = doc?.getElementById?.('panel');
  if (!panel || panel.querySelector('[data-sheet-handle]')) return null;
  const phone = () => !win?.matchMedia || win.matchMedia(`(max-width: ${PHONE_MAX}px)`).matches;
  const handle = doc.createElement('button');
  handle.type = 'button';
  handle.className = 'sheet-handle';
  handle.dataset.sheetHandle = '';
  handle.setAttribute('aria-controls', 'panel-body');
  panel.prepend(handle);
  const set = s => {
    panel.dataset.sheet = s;
    handle.setAttribute('aria-expanded', s === 'peek' ? 'false' : 'true');
    handle.setAttribute('aria-label', sheetLabel(s));
  };
  set('half');
  onLangChange(() => set(panel.dataset.sheet));
  let drag = null;
  handle.addEventListener('pointerdown', e => { drag = { y: e.clientY }; });
  handle.addEventListener('pointerup', e => {
    const d = drag;
    drag = null;
    const to = d ? dragSheet(panel.dataset.sheet, e.clientY - d.y) : null;
    if (to) { set(to); handle.dataset.dragged = '1'; }
  });
  handle.addEventListener('click', () => {
    if (handle.dataset.dragged) { delete handle.dataset.dragged; return; }
    set(nextSheet(panel.dataset.sheet));
  });
  panel.addEventListener('keydown', e => {
    if (e.key !== 'Escape' || !phone() || panel.dataset.sheet === 'peek') return;
    // A field that handles Escape itself (an open select) keeps it.
    if (e.defaultPrevented) return;
    set('peek');
    handle.focus();
  });
  // Choosing a tab (or opening a report) brings the sheet up to half.
  doc.addEventListener('click', e => {
    if (panel.dataset.sheet === 'peek' && e.target?.closest?.('[data-act="tab"], [data-act="report-open"], [data-act="practice-open"]')) set('half');
  });
  const vv = win?.visualViewport;
  if (vv) {
    const fit = () => doc.documentElement.style.setProperty('--vvh', `${Math.round(vv.height)}px`);
    vv.addEventListener('resize', fit);
    fit();
  }
  return { set, state: () => panel.dataset.sheet };
}

// The Frontier pages load their modules as ES modules (deferred): the DOM is parsed when this runs.
if (globalThis.document && !globalThis.process?.versions?.node) mountSheet();
