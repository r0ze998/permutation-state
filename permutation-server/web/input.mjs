// All input of the player client: one delegated click registry (by data-*
// attribute or element id), the keyboard, form changes and the lens bar.
import * as api from './api.mjs';
import { $, $$, html, setHtml } from './util.mjs';
import { S, civN, invalidate } from './state.mjs';
import { map } from './world.mjs';
import { YIELD_COLORS, THREAT_COLOR, CONCORD_NEUTRAL, rgba } from './map.mjs';
import * as T from './i18n.mjs';
import { addDraft, removeDraft, toggleAdopt, commit, commitOrEndTurn } from './orders.mjs';
import { hoverTile, selectTile, focusTile, focusUnit, clearSelection, quickMove, startPatrol, cancelPatrol, finishPatrol, removePatrolPoint } from './selection.mjs';
import { toggleDrawer, closeDrawer } from './drawers/index.mjs';
import { govAction, toggleStand } from './drawers/nation.mjs';
import { quoteAmm, reviewExchange, cancelExchange, confirmExchange } from './drawers/market.mjs';
import { proveTile, setProofCiv, setProofTick } from './drawers/decisions.mjs';
import { goNext, goToBlocker, nextTurnClick, toggleSummary, closeSummary } from './next.mjs';
import { toggleChain } from './chain.mjs';
import { pickCiv, pickStand, join, claim, startSeason } from './lobby.mjs';
import { offerContract } from './drawers/diplomacy.mjs';
import { sendTalk } from './drawers/talk.mjs';
import { poll } from './app.mjs';

// ================================================================== click registry
// [data attribute (dataset key), handler(value, element, event)] — the element
// closest to the click that carries one of these (or a registered id) wins.
const BY_DATA = {
  order: v => { if (addDraft(JSON.parse(v))) invalidate('inspector', 'drawer'); },
  remove: v => removeDraft(+v),
  gov: (v, el) => govAction(JSON.parse(v), el),
  stand: (v, el) => toggleStand(v, el),
  adopt: v => { const [role, id] = v.split(':'); toggleAdopt(role, +id); },
  chip: v => { const d = S.drafts[+v]; if (d?.focus) map.focusTile(d.focus); },
  drawer: v => toggleDrawer(v),
  close: () => clearSelection(),
  focus: v => focusTile(v),
  selectUnit: v => focusUnit(+v),
  move: v => quickMove(v),
  queueClear: v => { addDraft({ type: 'SetQueue', city: +v, items: [] }); },
  chron: v => { S.chronFilter = v; invalidate('drawer'); },
  dec: v => { S.decFilter = v; invalidate('drawer'); },
  patrolRemove: v => removePatrolPoint(+v),
  n: v => goToBlocker(+v),
  lens: v => setLens(v),
  closeHelp: () => $('#help').close(),
  start: () => { $('#help').close(); startSeason(); },
  closeChain: () => toggleChain(),
  pickCiv: v => pickCiv(+v),
  pickStand: v => pickStand(v),
  claimCiv: v => claim(+v),
  offerContract: v => offerContract(+v),
};
const BY_ID = {
  'talk-send': () => sendTalk(),
  'commit-btn': () => commitOrEndTurn(),
  'pause-btn': async () => { await api.post('/api/control', { paused: !S.view.paused }); poll(); },
  'help-btn': () => $('#help').showModal(),
  'zoom-in': () => map.zoomBy(1.2),
  'zoom-out': () => map.zoomBy(1 / 1.2),
  home: () => map.focusHome(),
  'summary-toggle': () => toggleSummary(),
  'summary-close': () => closeSummary(),
  'prove-tile': () => proveTile(),
  'patrol-start': (el) => startPatrol(+el.dataset.unit),
  'patrol-cancel': () => cancelPatrol(),
  'patrol-done': (el) => finishPatrol(JSON.parse(el.dataset.dto)),
  'amm-quote': () => quoteAmm(),
  'ex-review': () => reviewExchange(),
  'ex-cancel': () => cancelExchange(),
  'ex-confirm': () => confirmExchange(),
  'join-btn': () => join(),
  'chain-beat': () => toggleChain(),
  'next-turn': () => nextTurnClick(),
  tracker: () => toggleDrawer('era'),
  minimap: (el, e) => {
    if (!S.mini) return;
    const r = el.getBoundingClientRect(), w = S.mini.toWorld(e.clientX - r.left, e.clientY - r.top);
    map.centerOnWorld(w.x, w.y);
  },
};
const attr = k => `data-${k.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`;
const SELECTOR = [...Object.keys(BY_DATA).map(k => `[${attr(k)}]`), ...Object.keys(BY_ID).map(id => `#${id}`)].join(', ');

function onClick(e) {
  const el = e.target.closest(SELECTOR);
  if (!el) return;
  try {
    if (el.id && BY_ID[el.id]) return void BY_ID[el.id](el, e);
    for (const [k, fn] of Object.entries(BY_DATA)) if (el.dataset[k] !== undefined) return void fn(el.dataset[k], el, e);
  } catch (err) { console.error(err); }
}

// ================================================================== lenses
function setLens(lens) {
  S.lens = lens; map.setLens(lens);
  for (const b of $$('.lens-bar button')) b.setAttribute('aria-pressed', String(b.dataset.lens === lens));
  renderLensLegend();
}
const dot = c => html`<i style="background:${c}"></i>`;
function renderLensLegend() {
  const el = $('#lens-legend');
  const civs = S.view?.civs || [];
  const body = {
    political: html`${civs.map(c => html`<span>${dot(T.CIV_COLORS[c.id])}${civN(c.id)}${c.id === S.myCiv ? '（あなた）' : ''}</span>`)}<span class="hint">斜線＝戦争中の相手</span>`,
    yields: html`<span>${dot(YIELD_COLORS[0])}食料</span><span>${dot(YIELD_COLORS[1])}生産</span><span>${dot(YIELD_COLORS[2])}金</span><span class="hint">都市が使う土地の基本産出（川・資源込み）</span>`,
    military: html`<span>${dot(rgba(THREAT_COLOR, .6))}戦争中の敵軍・蛮族から2マス以内</span>`,
    concord: html`<span>${dot(CONCORD_NEUTRAL)}都市国家の周囲2マス（宗主の色）</span><span class="hint">協調の道：宗主・条約・交易</span>`,
  }[S.lens];
  el.hidden = !body;
  if (body) setHtml(el, body);
}

// ================================================================== keyboard and forms
function onKey(e) {
  if (e.target.closest?.('input, select, textarea') || document.querySelector('dialog[open]')) return;
  if (e.key === ' ') { e.preventDefault(); goNext(); }
  else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); commit(); }
  else if (e.key === 'Escape' && S.patrol) cancelPatrol();
  else if (e.key === 'Escape') { clearSelection(); closeDrawer(); }
  else if (e.key.toLowerCase() === 'h') map.focusHome();
  else if (e.key === '`') toggleChain();
}

function onChange(e) {
  if (e.target.id === 'proof-civ') setProofCiv(+e.target.value);
  if (e.target.id === 'proof-tick') setProofTick(+e.target.value);
  if (e.target.id === 'talk-to') S.talkTo = e.target.value;
}

/** Diplomacy rows remember whether they are open across re-renders. */
function onToggle(e) {
  const row = e.target.closest?.('[data-civ-row]'); if (!row) return;
  const id = +row.dataset.civRow;
  if (row.open) S.diploOpen.add(id); else S.diploOpen.delete(id);
}

export function bindInput() {
  document.addEventListener('click', onClick);
  document.addEventListener('keydown', onKey);
  document.addEventListener('change', onChange);
  document.addEventListener('toggle', onToggle, true);
  map.setCallbacks({ onSelect: selectTile, onHover: hoverTile, onMove: quickMove });
  const rationale = $('#rationale');
  rationale.addEventListener('input', () => invalidate('dock'));
  rationale.addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); commit(); } });
}
