// Wylls — playable client (Game Design V5).
// You are a member of one nation. Flow: select on the map → the inspector
// explains it → every option shows cost, time and, when blocked, the engine's
// own reason → add to this tick's orders. Orders of offices you hold are
// sealed and sent; orders of other offices become proposals (献策) to their
// officers. The nation plaza holds the government: offices, elections,
// proposals and recalls.
//
// This module boots the client, polls the server and applies each view;
// everything else lives in the modules imported below.
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import { $, toast } from './util.mjs';
import { S, civN, isWatching, invalidate, registerRenderers } from './state.mjs';
import { poll, setPollStep } from './sync.mjs';
import { map } from './world.mjs';
import { drawMinimap } from './map.mjs';
import { restoreTick, resetTick, afterView } from './orders.mjs';
import { refreshSelection } from './selection.mjs';
import { pickMember, chainLobby, describeSeason } from './lobby.mjs';
import * as chainio from './chainio.mjs';
import { renderTop, renderClock, renderPlate, renderRibbon } from './hud/top.mjs';
import { renderDock } from './hud/dock.mjs';
import { renderInspector } from './inspector/index.mjs';
import { renderDrawer, renderNavDots, toggleDrawer } from './drawers/index.mjs';
import { loadDecisions } from './drawers/decisions.mjs';
import { renderNextTurn, renderNotifs, renderSummary, tickChanges } from './next.mjs';
import { renderPool, renderChainBeat, renderChainDrawer, onChainView } from './chain.mjs';
import { bindInput } from './input.mjs';
import { adoptRules } from './rules.mjs';
import { L, onLangChange } from './lang.mjs';

function renderMinimap() {
  S.mini = drawMinimap($('#minimap'), S.map, S.view, map.viewport());
  $('#mini-left').textContent = L`${S.map.tiles.length}マス · 1文明 · ${S.view.civs.length}勢力`;
  $('#mini-right').textContent = L`領土 ${[...S.view.owners].filter(c => c === String(S.myCiv)).length}`;
}

// Render order: the top bar first, the map-side panels last.
registerRenderers([
  ['top', renderTop], ['plate', renderPlate], ['dock', renderDock], ['inspector', renderInspector],
  ['summary', renderSummary], ['minimap', renderMinimap], ['navDots', renderNavDots], ['ribbon', renderRibbon],
  ['nextTurn', renderNextTurn], ['notifs', renderNotifs], ['chainBeat', renderChainBeat], ['pool', renderPool],
  ['chainDrawer', renderChainDrawer], ['drawer', renderDrawer],
]);

// ================================================================== polling
async function fetchView() {
  try {
    const v = await api.get(`/api/state${isWatching() ? `?civ=${S.watch}` : ''}`);
    S.online = true;
    return v;
  } catch {
    S.online = false;
    renderClock();
    return null;
  }
}

// Views are applied in request order: a response older than one already
// applied is dropped (single-flight polling keeps them in order anyway).
let requested = 0, applied = 0;
async function pollOnce() {
  const seq = ++requested;
  const v = await fetchView();
  if (!v || seq < applied) return;
  applied = seq;
  applyView(v);
}
// poll() itself lives in sync.mjs (no import cycle back into this module).
setPollStep(pollOnce);

function applyView(v) {
  // Local mode: the server no longer knows this browser's member token (it
  // restarted): back to the lobby to join again, instead of rendering a view
  // that belongs to no nation. (Chain mode has no tokens: the member is
  // viewed by id and signs in the browser, so a restart changes nothing.)
  if (!v.chain && api.memberToken() && !isWatching() && v.viewer !== 'member') {
    api.tokenStore.set(null);
    toast(L`サーバーが再起動したため、メンバーとして入り直してください。`, 'error');
    setTimeout(() => location.reload(), 1200);
    return;
  }
  adoptRules(v);
  const prev = S.view;
  const newTick = S.lastTick !== null && v.tick !== S.lastTick;
  S.view = v; S.myCiv = v.me; S.memberId = v.member?.id ?? null;
  if (v.chain?.gateway) chainio.setGateway(v.chain.gateway);
  S.clockAt = performance.now();
  if (S.lastTick === null) restoreTick(v); // what was sealed earlier this tick, after a reload
  if (newTick) onNewTick(v, prev);
  S.lastTick = v.tick;
  map.setView(v, v.me);
  onChainView(v);
  // Chain play in step with the chain; auto-commit a dirty draft before the
  // deadline (never lose orders silently).
  afterView(v);
  invalidate('all');
}

/** The two views the tick report was drawn from (to say it again in another language). */
let reportViews = null;
// A language switch: the tick report is text, so draw it again from the same
// two views (state.mjs re-renders every part after this).
onLangChange(() => { if (reportViews) S.changes = tickChanges(...reportViews); });

/** A tick resolved: reset this tick's work and tell the player what happened. */
function onNewTick(v, prev) {
  resetTick();
  if (!v.gov?.voteOpen) S.myVotes = {};
  for (const k of v.skipped || []) {
    const role = T.ROLE_JA[k.role] || '', why = T.blockedText({ code: k.reason });
    toast(k.index === null ? L`見送られた命令：${role}の命令全体（${why}）` : L`見送られた命令：${role}の${k.index + 1}件目（${why}）`, 'error');
  }
  const me = civN(v.me);
  const events = (v.lastSummary || []).map(T.chronicleText);
  for (const [kind, text] of events) {
    if ((kind === 'era' || kind === 'gov' || kind === 'recall') && text.includes(me)) toast(text, kind === 'era' ? 'good' : '');
  }
  if (S.drawer === 'decisions') loadDecisions();
  reportViews = prev ? [prev, v] : null;
  S.changes = prev ? tickChanges(prev, v) : [];
  S.research = null; S.diplo = {};
  S.summaryOpen = true; S.summaryCollapsed = false; S.summaryPinned = false;
  for (const [kind, text] of events) if ((kind === 'war' || kind === 'capture') && text.includes(me)) toast(text, 'war');
  for (const p of v.proposals.filter(p => p.to === v.me && p.tick === v.tick - 1)) {
    toast(L`${civN(p.from)}から${T.PROPOSAL_KIND[p.kind]}の申し入れ（ティック${p.expires}まで有効）`, '', { label: L`外交を開く`, run: () => { if (S.drawer !== 'diplomacy') toggleDrawer('diplomacy'); } });
  }
  refreshSelection();
}

// ================================================================== timers
// One clock for everything periodic: the countdown every beat, the Next Turn
// ring every 2nd, the poll every 3rd (750 ms), the minimap every 4th.
const BEAT_MS = 250;
let beat = 0;
function onBeat() {
  beat++;
  if (!S.view) return;
  renderClock();
  if (beat % 2 === 0) invalidate('nextTurn');
  if (beat % 3 === 0) poll();
  if (beat % 4 === 0) invalidate('minimap');
}

// ================================================================== boot
/** What the loading screen says now (a function: said again after a language switch), or null. */
let loadingSays = null;
function showLoading(say) { loadingSays = say; say(); }
onLangChange(() => loadingSays?.());

/** Chain mode, watching: wait (with a message) while the season registers or starts. */
const waitForSeason = lobby => api.untilPlaying(lobby, phase => showLoading(() => {
  $('#loading h2').textContent = L`シーズンの開始を待っています`;
  $('#loading-text').textContent = phase === 'registering' ? L`登録を受け付けています。締切でシーズンが始まり、そのあと表示されます。` : L`シーズンを準備しています…`;
}));

async function boot() {
  const params = new URLSearchParams(location.search);
  if (params.has('spectate')) { location.replace('spectate.html'); return; }
  if (params.get('token')) { api.tokenStore.set(params.get('token')); history.replaceState(null, '', location.pathname); }
  api.setMemberToken(api.tokenStore.get());
  // ?watch=N: read-only view as nation N (for review; no membership needed). The
  // world is the same for everyone (perfect information); only relations and sight differ.
  if (params.has('watch')) { S.watch = +params.get('watch'); api.setMemberToken(null); }
  bindInput();
  try {
    const lobby = await api.get('/api/lobby');
    if (lobby.mode === 'chain') {
      // No member tokens on chain: the wallet registers, the browser keeps the
      // session key, and the game is this member's public view (?member=M).
      api.setMemberToken(null);
      if (isWatching()) await waitForSeason(lobby);
      else api.setViewerMember(await chainLobby(lobby));
    } else if (lobby.you === null && !isWatching()) {
      api.tokenStore.set(null); api.setMemberToken(null);
      api.setMemberToken(await pickMember(lobby));
    }
    S.map = await api.get('/api/map');
    map.setMap(S.map);
    await poll();
    if (!S.view) throw new Error('no view');
    describeSeason(S.view); // lobby.mjs says it again after a language switch
    loadingSays = null;
    $('#loading').hidden = true;
    if (S.view.phase === 'lobby' || (S.view.paused && S.view.tick === 0)) $('#help').showModal();
  } catch {
    const before = loadingSays; // a heading of the wait for the season stays
    showLoading(() => { before?.(); $('#loading-text').textContent = L`ゲームサーバーに接続できません。\`cargo run --release --bin play\` を起動してから再読み込みしてください。`; });
  }
  setInterval(onBeat, BEAT_MS);
}

boot();
