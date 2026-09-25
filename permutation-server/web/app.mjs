// PERMUTATION STATE — playable client (Game Design V5).
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
import { $, toast, singleFlight } from './util.mjs';
import { S, civN, isWatching, invalidate, registerRenderers } from './state.mjs';
import { map } from './world.mjs';
import { drawMinimap } from './map.mjs';
import { isDirty, commit, restoreCommitted, resetTick } from './orders.mjs';
import { refreshSelection } from './selection.mjs';
import { pickMember, describeSeason } from './lobby.mjs';
import { renderTop, renderClock, renderPlate, renderRibbon } from './hud/top.mjs';
import { renderDock } from './hud/dock.mjs';
import { renderInspector } from './inspector/index.mjs';
import { renderDrawer, renderNavDots, toggleDrawer } from './drawers/index.mjs';
import { loadDecisions } from './drawers/decisions.mjs';
import { renderNextTurn, renderNotifs, renderSummary, tickChanges } from './next.mjs';
import { renderPool, renderChainBeat, renderChainDrawer, onChainView } from './chain.mjs';
import { bindInput } from './input.mjs';
import { AUTO_COMMIT_SECONDS, adoptRules } from './rules.mjs';

function renderMinimap() {
  S.mini = drawMinimap($('#minimap'), S.map, S.view, map.viewport());
  $('#mini-left').textContent = `${S.map.tiles.length}マス · ${S.view.civs.length}文明`;
  $('#mini-right').textContent = `領土 ${[...S.view.owners].filter(c => c === String(S.myCiv)).length}`;
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
/** Fetch and apply the latest view; overlapping calls are coalesced (one request in flight). */
export const poll = singleFlight(pollOnce);

function applyView(v) {
  // The server no longer knows this browser's member token (it restarted):
  // back to the lobby to claim or join again, instead of rendering a view
  // that belongs to no nation.
  if (api.memberToken() && !isWatching() && v.viewer !== 'member') {
    api.tokenStore.set(null);
    toast('サーバーが再起動したため、国民として入り直してください。', 'error');
    setTimeout(() => location.reload(), 1200);
    return;
  }
  adoptRules(v);
  const prev = S.view;
  const newTick = S.lastTick !== null && v.tick !== S.lastTick;
  S.view = v; S.myCiv = v.me; S.memberId = v.member?.id ?? null;
  S.clockAt = performance.now();
  if (S.lastTick === null) {
    const orders = (v.member?.committed || []).flatMap(c => c.orders);
    if (orders.length) restoreCommitted(orders);
  }
  if (newTick) onNewTick(v, prev);
  S.lastTick = v.tick;
  map.setView(v, v.me);
  onChainView(v);
  // Auto-commit a dirty draft just before the deadline (never lose orders silently).
  if (!v.paused && isDirty() && v.secondsLeft < AUTO_COMMIT_SECONDS && !S.committing) commit(true);
  invalidate('all');
}

/** A tick resolved: reset this tick's work and tell the player what happened. */
function onNewTick(v, prev) {
  resetTick();
  if (!v.gov?.voteOpen) S.myVotes = {};
  for (const k of v.skipped || []) toast(`見送られた命令：${T.ROLE_JA[k.role] || ''}${k.index === null ? 'の命令全体' : `の${k.index + 1}件目`}（${T.blockedText({ code: k.reason })}）`, 'error');
  const me = civN(v.me);
  const events = (v.lastSummary || []).map(T.chronicleText);
  for (const [kind, text] of events) {
    if ((kind === 'era' || kind === 'gov' || kind === 'recall') && text.includes(me)) toast(text, kind === 'era' ? 'good' : '');
  }
  if (S.drawer === 'decisions') loadDecisions();
  S.changes = prev ? tickChanges(prev, v) : [];
  S.research = null; S.diplo = {};
  S.summaryOpen = true; S.summaryCollapsed = false; S.summaryPinned = false;
  for (const [kind, text] of events) if ((kind === 'war' || kind === 'capture') && text.includes(me)) toast(text, 'war');
  for (const p of v.proposals.filter(p => p.to === v.me && p.tick === v.tick - 1)) {
    toast(`${civN(p.from)}から${T.PROPOSAL_KIND[p.kind]}の申し入れ（ティック${p.expires}まで有効）`, '', { label: '外交を開く', run: () => { if (S.drawer !== 'diplomacy') toggleDrawer('diplomacy'); } });
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
async function boot() {
  const params = new URLSearchParams(location.search);
  if (params.has('spectate')) { location.replace('spectate.html'); return; }
  if (params.get('token')) { api.tokenStore.set(params.get('token')); history.replaceState(null, '', location.pathname); }
  api.setMemberToken(api.tokenStore.get());
  // ?watch=N: read-only view through nation N's fog (for review; no membership needed).
  if (params.has('watch')) { S.watch = +params.get('watch'); api.setMemberToken(null); }
  bindInput();
  try {
    const lobby = await api.get('/api/lobby');
    if (lobby.you === null && !isWatching()) {
      api.tokenStore.set(null); api.setMemberToken(null);
      api.setMemberToken(await pickMember(lobby));
    }
    S.map = await api.get('/api/map');
    map.setMap(S.map);
    await poll();
    if (!S.view) throw new Error('no view');
    describeSeason(S.view);
    $('#loading').hidden = true;
    if (S.view.phase === 'lobby' || (S.view.paused && S.view.tick === 0)) $('#help').showModal();
  } catch {
    $('#loading-text').textContent = 'ゲームサーバーに接続できません。`cargo run --release --bin play` を起動してから再読み込みしてください。';
  }
  setInterval(onBeat, BEAT_MS);
}

boot();
