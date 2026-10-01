// Boot of the Frontier pages (index, practice, spectate): config → the
// herald's season record → pins (program, cluster, season, ruleset,
// beacon) → the chain clock → the shell (bell chip, staleness banner,
// language toggle) → the map. On the game page (W3-F) the play screens
// follow: the faction and quota chips, the bottom tabs and the panel of
// each tab (join and sites, holding, hosts and explore, the march composer
// with the tracker and incoming warnings, the bell sheet and chronicle),
// driven by controller.mjs. Wave 4 (W4-E) routes the clash report with
// "verify in this browser", practice mode (its own page and, on the game
// page, the "what if" of a report), the onboarding card, and the spectator
// page; their logic is in screens/{report,practice,onboarding,spectate}.mjs
// and onboarding.mjs, the actions below only move state and call them.
import { config } from './config.mjs';
import { FS, invalidate, registerRenderers } from './fstate.mjs';
import { createHerald, nextPoll, staleness } from './herald.mjs';
import { setPin, setRelay } from './fchainio.mjs';
import { checkBeacon } from './seal.mjs';
import { ChainClock, bellChip, countdown, seasonClock } from './clock.mjs';
import { effectiveStatus } from './fcodec.mjs';
import { FrontierMap } from './map/fmap.mjs';
import { SEASON_STATUS_TEXT, clientText } from './fi18n.mjs';
import { L, fmtNum, mountLangToggle } from '../lang.mjs';
import { toHex } from '../sdk/bytes.mjs';
import { html, setHtml } from '../util.mjs';
import { ACTIONS, FORMS, bind, startPlay, wantProvince } from './controller.mjs';
import { renderTabs, renderNotice, factionChip, quotaChip, mountSheet } from './screens/shell.mjs';
import { createTerrain } from './map/terrain.mjs';
import { provincePixel } from './map/layers.mjs';

// Sprite art is on by default (?art=0 turns it off); ?art=1 adds the preview helpers below.
const ART_ON = new URLSearchParams(globalThis.location?.search ?? '').get('art') !== '0';
const ART_PREVIEW = new URLSearchParams(globalThis.location?.search ?? '').get('art') === '1';
// ?art=1&roads=1 adds sample roads between sites where the account has none (presentation only).
const ART_ROADS = ART_PREVIEW && new URLSearchParams(globalThis.location?.search ?? '').get('roads') === '1';
/**
 * Art mode: the ClashInputs of each province's last resolved clash (its /h/clash report), loaded once:
 * the resolve summary's bell, else the three bells before the envelope's.
 */
const artClash = new Map();
function artClashOf(p, q) {
  const env = FS.provinces.get(`${p},${q}`);
  if (!env || !heraldRef) return null;
  const key = `${p},${q}`;
  if (artClash.has(key)) return artClash.get(key);
  artClash.set(key, null);
  const rb = env.province?.resolveSummary?.bell;
  const bells = rb ? [rb] : ART_PREVIEW ? [env.bell - 1, env.bell - 2, env.bell - 3].filter(b => b > 0) : [];
  (async () => {
    for (const b of bells) {
      const r = await heraldRef.clash(p, q, b).catch(() => null);
      if (r?.ok && r.inputs) { artClash.set(key, r.inputs); mapRef?.invalidate(); return; }
    }
  })();
  return null;
}
// ?art=1&ringopen=1 replays the ring-open moment on the outermost open ring; &engine=N previews an Engine stage (presentation only).
const ART_Q = new URLSearchParams(globalThis.location?.search ?? '');
const ART_RINGOPEN = ART_PREVIEW && ART_Q.get('ringopen') === '1';
// ?art=1&relics=1 places sample Relic Sites and Waystones (M3 features, no accounts in M1; presentation only).
const ART_RELICS = ART_PREVIEW && ART_Q.get('relics') === '1';
// ?art=1&rivers=1 draws sample rivers (no river data in the Frontier; presentation only).
const ART_RIVERS = ART_PREVIEW && ART_Q.get('rivers') === '1';
// ?art=1&ally=0-1,2-4 shows those faction pairs as allied (presentation only; real relations come from the Province).
const ART_ALLY = ART_PREVIEW ? (ART_Q.get('ally') ?? '').split(',').map(x => x.split('-').map(Number)).filter(x => x.length === 2 && x.every(Number.isInteger)) : [];
const ART_ENGINE = ART_PREVIEW ? Math.max(0, Math.min(5, Number(ART_Q.get('engine') ?? 0) | 0)) : 0;
// ?art=1&fog=1 previews the fog as if the viewer held province (2,0) (presentation only).
const ART_FOG = ART_PREVIEW && new URLSearchParams(globalThis.location?.search ?? '').get('fog') === '1';
import * as joinScreen from './screens/join.mjs';
import * as holdingScreen from './screens/holding.mjs';
import * as hostScreen from './screens/host.mjs';
import * as exploreScreen from './screens/explore.mjs';
import * as marchScreen from './screens/march.mjs';
import * as trackerScreen from './screens/tracker.mjs';
import * as incomingScreen from './screens/incoming.mjs';
import * as bellScreen from './screens/bell.mjs';
import * as chronicleScreen from './screens/chronicle.mjs';
import * as reportScreen from './screens/report.mjs';
import * as practiceScreen from './screens/practice.mjs';
import * as onboardingCard from './screens/onboarding.mjs';
import * as spectateScreen from './screens/spectate.mjs';
import { FLAG, withFlag, restoreFlags } from './onboarding.mjs';
import { useHerald, refresh as refreshPlay } from './controller.mjs';
import { kernel as loadKernel } from './wasm.mjs';
import { scope } from './fchainio.mjs';
import { hostParts } from './faddr.mjs';
import { RETREAT_CHOICES, retreatBps } from './fmarch.mjs';
import { uiKey, uiStorage, loadUi, saveUi } from './fui.mjs';
import * as hud from './hud/hud.mjs';
import * as inspect from './hud/inspect.mjs';
import { createRoster } from './people/roster.mjs';
import * as scene from './people/scene.mjs';
import { activityText } from './people/activity.mjs';
import { battleScene, startBattle, PHASE } from './people/battle.mjs';
import { decode as decodeAccount } from './fcodec.mjs';
import { fromBase64 } from '../sdk/bytes.mjs';
import { tileHex } from './fgeo.mjs';
import { project } from '../map.mjs';
import { identityOf, displayName } from './people/identity.mjs';

const $ = id => globalThis.document?.getElementById(id);
const setText = (id, text) => { const el = $(id); if (el && el.textContent !== text) el.textContent = text; };

/** The chip's text: "鐘 1,034 · 残り 6:12" / "Bell 1,034 · 6:12 left". */
export function chipText(chip) {
  if (!chip || (chip.bell === null && !chip.beforeGenesis)) return L`鐘 —`;
  if (chip.beforeGenesis) return L`開始まで ${countdown(chip.secondsLeft)}`;
  if (chip.ended) return L`鐘 ${fmtNum(chip.bell)} · 終了`;
  return L`鐘 ${fmtNum(chip.bell)} · 残り ${countdown(chip.secondsLeft)}`;
}

function renderChip() {
  const now = FS.chain?.now() ?? null;
  setText('bell-chip', chipText(FS.clock ? bellChip(FS.clock, now) : null));
  renderHudTick(now);
  FS.stale = staleness({ latestUnix: FS.record?.latestUnix, chainNow: now, behind: FS.chain?.behind() ?? null });
  const banner = $('stale-banner');
  if (banner) {
    banner.hidden = !FS.stale.stale;
    if (FS.stale.stale) banner.textContent = L`表示が ${Math.round(FS.stale.behind)} 秒遅れています。最新の状態が必要な操作の前に再読み込みしてください`;
  }
}

function renderStatus() {
  const s = FS.season;
  const status = s ? SEASON_STATUS_TEXT[effectiveStatus(s, FS.chain?.now() ?? 0)] : L`読み込み中…`;
  const beacon = FS.beacon?.kind === 'test' ? L`（テスト用ビーコン）` : '';
  setText('season-status', `${status}${beacon}`);
  const err = $('error-line');
  if (err) { err.hidden = !FS.error; err.textContent = FS.error ? FS.error.text : ''; }
}

/** Whether a host or holding id is the viewer's (its province, site and generation are one of the viewer's holdings). */
export function mineOf(holdings) {
  return id => {
    const h = hostParts(id);
    return !!h && (holdings ?? []).some(o => o.p === h.p && o.q === h.q && o.site === h.site && o.gen === h.gen);
  };
}

/** The viewer's resolved clashes: marches whose arrival bell is resolved, and clashes in provinces of its holdings. */
export function myReports(FS) {
  const out = [];
  const add = x => { if (!out.some(y => y.p === x.p && y.q === x.q && y.bell === x.bell)) out.push({ ...x, mine: true }); };
  for (const m of FS.marches ?? []) {
    const bell = m.entry?.arriveBell ?? m.transit?.arriveBell;
    if (m.facts?.pipeline === 'resolved' && m.dest && Number.isInteger(bell)) add({ p: m.dest.p, q: m.dest.q, bell });
  }
  const own = FS.holdings ?? [];
  for (const c of reportScreen.clashesFrom(FS.chronicle ?? [], 24)) if (own.some(o => o.p === c.p && o.q === c.q)) add(c);
  return out.slice(0, 12);
}

/** The panel of the current tab (the game page). A report or a practice run open takes the panel. */
export function panelMarkup(FS) {
  const tab = FS.tab ?? 'map';
  const parts = [renderNotice(FS.notice)];
  if (FS.practice) return [...parts, practiceScreen.render(FS.practice, { kernelError: FS.practiceError ?? null, closable: true })];
  if (FS.report) return [...parts, reportScreen.render(FS, mineOf(FS.holdings))];
  // The selection first on the map tab (the player just chose it), then the guide.
  if (tab === 'map' && FS.selected) parts.push(inspect.render(FS, terrainRef, mapRef?.art?.activities ?? null));
  parts.push(onboardingCard.render(FS, { open: tab === 'map' }));
  // Another tab keeps one line of the selection (the inspector itself is on the map tab).
  if (tab !== 'map' && FS.selected) parts.push(html`<p class="sel-line">${Number.isInteger(FS.selected.idx) ? L`選択中：州 ${FS.selected.p},${FS.selected.q} · マス ${FS.selected.idx + 1}` : L`選択中：州 ${FS.selected.p},${FS.selected.q}`} <button type="button" class="btn small" data-act="tab" data-tab="map">${L`詳細`}</button></p>`);
  if (tab === 'map') {
    parts.push(joinScreen.render(FS));
  } else if (tab === 'holding') parts.push(holdingScreen.render(FS));
  else if (tab === 'hosts') parts.push(hostScreen.render(FS), exploreScreen.render(FS));
  else if (tab === 'marches') parts.push(marchScreen.render(FS), trackerScreen.render(FS), reportScreen.renderLinks(myReports(FS), L`あなたの衝突の報告`), incomingScreen.render(FS));
  else parts.push(bellScreen.render(FS), reportScreen.renderLinks(reportScreen.clashesFrom(FS.chronicle ?? []), L`最近の衝突`), chronicleScreen.render(FS), html`<section aria-labelledby="more-title"><h3 id="more-title">${L`設定`}</h3>
    <label class="choice"><input type="checkbox" data-act="fog" ${FS.view.fog ? '' : 'checked'}>${L`すべてを見せる（どの口座も公開されています）`}</label>
    <button type="button" class="btn" data-act="forget">${L`この端末からこのシーズンの鍵を消す`}</button>
    ${onboardingCard.renderRestore(FS)}
    <p><button type="button" class="btn" data-act="practice-open">${L`練習モードを開く`}</button></p>
    <p><a href="practice.html">${L`練習`}</a> · <a href="spectate.html">${L`観戦`}</a></p></section>`);
  return parts;
}

/** The panel of the practice and spectator pages. */
export function modePanel(FS) {
  if (FS.mode === 'practice') return [renderNotice(FS.notice), practiceScreen.render(FS.practice, { kernelError: FS.practiceError ?? null })];
  return [FS.selected ? inspect.render(FS, terrainRef, mapRef?.art?.activities ?? null) : '', spectateScreen.render(FS)];
}

function renderPlay() {
  const tabs = $('tabs');
  if (tabs) setHtml(tabs, renderTabs(FS));
  const body = $('panel-body');
  if (body) setHtml(body, panelMarkup(FS));
  const title = $('panel-title');
  const sub = FS.practice ? L`練習モード` : FS.report ? L`衝突の報告` : null;
  if (title) setText('panel-title', sub ?? { map: L`地図`, holding: L`拠点`, hosts: L`軍勢`, marches: L`進軍`, more: L`その他` }[FS.tab ?? 'map'] ?? L`シーズン`);
  renderRail();
}

function renderMode() {
  const body = $('panel-body');
  if (body) setHtml(body, modePanel(FS));
  renderRail();
}

// ------------------------------------------------------------------ the HUD (hud/hud.mjs)
let mapRef = null;
let terrainRef = null;
/** Who holds each site (people/roster.mjs): names and faces for tags, the rail, the inspector, reports. */
let rosterRef = null;
/** The people layer's inputs, rebuilt at most once a second (the chronicle changes on polls only). */
let peopleCache = { at: -1, value: null };
export function peopleSource() {
  const bell = FS.nowBell ?? (FS.clock ? Math.max(0, Math.floor(((FS.chain?.now() ?? 0) - FS.clock.genesisTs) / 600)) : 0);
  const sec = Math.floor(Date.now() / 1000);
  if (peopleCache.at === sec && peopleCache.value) { peopleCache.value.battles = (FS.battles ?? []).filter(b => (performance.now() / 1000 - b.t0) <= PHASE.end); return peopleCache.value; }
  if (rosterRef) {
    for (let d = 0; d < (FS.record?.rings?.length ?? 1); d++) rosterRef.ensure(d);
    scene.seedOwn(rosterRef, FS.holdings);
  }
  peopleCache = { at: sec, value: {
    departures: scene.departuresAt(FS.chronicle, FS.overviews, bell),
    explores: scene.exploresAt(FS.chronicle, FS.overviews, bell),
    nameOf: scene.namer(rosterRef),
    bell,
    own: FS.holdings ?? [],
    columnLabel: d => L`出陣 · 第${fmtNum(d.arriveBell)}鐘に到着`,
    demo: ART_PREVIEW && ART_Q.get('acts') === '1',
    battles: FS.battles ?? [],
    lossText: n => L`−${fmtNum(n)} 兵`,
    fateText: f => ({ Stays: L`持ちこたえた`, Withdrew: L`隣へ退いた`, Bounced: L`押し戻された`, Retreated: L`撤退した`, Destroyed: L`壊滅` })[f] ?? null,
  } };
  peopleCache.value.battles = (FS.battles ?? []).filter(b => (performance.now() / 1000 - b.t0) <= PHASE.end);
  autoBattles();
  return peopleCache.value;
}
const lastHtml = new Map();
/** Put markup in an element only when it changed (a hover title survives the one-second tick). */
function setHtmlIfChanged(el, markup) {
  const key = el.id;
  const text = [markup].flat(Infinity).map(String).join('');
  if (lastHtml.get(key) === text) return;
  lastHtml.set(key, text);
  setHtml(el, markup);
}

/** Every second: the bell pill's bar and urgency, the resource strip, the attention pill. */
function renderHudTick(now) {
  const m = hud.bellModel(FS.clock, now);
  const pill = $('bell-pill');
  if (pill) {
    if (pill.dataset.urgency !== m.urgency) pill.dataset.urgency = m.urgency;
    $('bell-fill')?.style.setProperty('--f', m.frac.toFixed(3));
  }
  // The screen edge warns only while a march is being composed (the bell closes its arrival choice).
  const body = globalThis.document?.body;
  const edge = FS.compose && !FS.compose.sending ? m.urgency : 'calm';
  if (body && body.dataset.urgency !== edge) body.dataset.urgency = edge;
  const strip = $('res-strip');
  if (strip) {
    const tokens = FS.mode === 'play' ? hud.resourceModel(hud.activeHolding(FS), now ?? 0) : [];
    strip.hidden = !tokens.length;
    setHtmlIfChanged(strip, hud.renderStrip(tokens));
  }
  const attn = $('attn-pill');
  if (attn) {
    const text = FS.mode === 'play' ? hud.attentionText(hud.attentionItems(FS)) : null;
    attn.hidden = !text;
    if (text && attn.textContent !== text) attn.textContent = text;
  }
}

function renderRail() {
  const el = $('rail');
  if (el) setHtmlIfChanged(el, hud.renderRail(FS));
}

/**
 * The hover tip (Civ's unit tooltip): who holds the tile and what is
 * happening on it, from the activities the last frame drew. Mouse only;
 * touch shows the same in the inspector.
 */
function showTip(hit, at) {
  const tip = $('map-tip');
  if (!tip) return;
  const acts = hit && Number.isInteger(hit.idx) ? mapRef?.art?.activities?.get(`${hit.p},${hit.q},${hit.idx}`) : null;
  const t = acts || hit ? mapRef?.art?.tiles?.find(u => u.p === hit?.p && u.pq === hit?.q && u.idx === hit?.idx) : null;
  const owner = t && t.state === 1 && t.site !== undefined ? rosterRef?.ownerOf(hit.p, hit.q, t.site) : null;
  if (!acts?.length && !owner) { tip.hidden = true; return; }
  const lines = [];
  if (owner) lines.push(html`<strong data-name>${displayName(identityOf(owner.tag), { full: true })}</strong>`);
  for (const a of (acts ?? []).filter((a, i, all) => all.findIndex(b => b.kind === a.kind) === i)) lines.push(html`<span class="tip-${a.kind}">${activityText(a)}</span>`);
  setHtml(tip, lines.map(l => html`<span class="tip-line">${l}</span>`));
  tip.hidden = false;
  tip.style.setProperty('--x', `${Math.round(at.x + 16)}px`);
  tip.style.setProperty('--y', `${Math.round(at.y + 12)}px`);
}

// ------------------------------------------------------------------ battle scenes (people/battle.mjs)
const battleSeen = new Map(); // "P,Q" → the last resolved bell a scene was considered for
/** Load a clash and play its scene on the map; `focus` moves the camera to it first. */
export async function playBattle(p, q, bell, { focus = false } = {}) {
  if (!heraldRef || ![p, q, bell].every(Number.isInteger)) return false;
  const r = await heraldRef.clash(p, q, bell).catch(() => null);
  if (!r?.ok || !r.inputs) return false;
  let before = null;
  try { before = r.report?.province_before_b64 ? decodeAccount('Province', fromBase64(r.report.province_before_b64)) : null; } catch { before = null; }
  const after = FS.provinces.get(`${p},${q}`)?.province ?? null;
  const scene = battleScene({ p, q, bell, inputs: r.inputs, before, after: after && after.resolvedNext > bell ? after : null });
  if (!scene) return false;
  if (focus && mapRef) { const h = tileHex(p, q, scene.tiles[0].idx), c = project(h.q, h.r); mapRef.setView({ x: c.x, y: c.y, zoom: 1.6 }); }
  FS.battles = [...(FS.battles ?? []).filter(b => !(b.scene.p === p && b.scene.q === q)), startBattle(scene, performance.now() / 1000)];
  mapRef?.invalidate();
  return true;
}
/** A clash that resolved in a province the page has loaded plays once, when it is new. */
function autoBattles() {
  for (const [key, env] of FS.provinces) {
    const b = env.province?.resolveSummary?.bell;
    if (!Number.isInteger(b) || b <= 0) continue;
    const prev = battleSeen.get(key);
    battleSeen.set(key, b);
    if (prev === undefined || prev >= b) continue;  // the first sight of a province is not news
    playBattle(env.province.p, env.province.q, b);
  }
}

/** Move the map to an attention item and open its tab. */
function goToItem(x) {
  if (!x) return;
  mapRef?.focus(x.p, x.q, 0.6);
  FS.selected = { kind: 'province', p: x.p, q: x.q };
  if (FS.mode === 'play' && x.tab) FS.tab = x.tab;
  invalidate('map', 'panel', 'tabs', 'rail');
}

/** The HUD's actions (data-act): cycle the attention items, jump to one, choose the active holding. */
export const HUD_ACTIONS = {
  'battle-play': d => playBattle(num(d.p), num(d.q), num(d.bell), { focus: true }),
  attn: () => {
    const items = hud.attentionItems(FS);
    if (!items.length) return;
    FS.attnIdx = ((FS.attnIdx ?? -1) + 1) % items.length;
    goToItem(items[FS.attnIdx]);
  },
  'attn-go': d => goToItem(hud.attentionItems(FS)[num(d.i)]),
  'holding-pick': d => {
    const i = num(d.i), h = FS.holdings?.[i];
    if (!h) return;
    FS.activeHolding = i;
    goToItem({ p: h.p, q: h.q, tab: 'holding' });
  },
};

function renderChips() {
  const f = $('faction-chip');
  const fc = factionChip(FS.citizen);
  if (f) { f.hidden = !fc; f.textContent = fc ?? ''; }
  const q = $('quota-chip');
  const qc = quotaChip(FS.quota);
  if (q) { q.hidden = !qc; q.textContent = qc ?? ''; }
}

// ------------------------------------------------------------------ wave 4: report, practice, onboarding (W4-E)
let heraldRef = null;
const num = x => Number.parseInt(x, 10);

/** Save the onboarding flags (ps-fui `dismissed`) of the pinned season; in memory only before a season is pinned. */
function setFlags(next) {
  const sc = scope();
  FS.ui = sc ? saveUi(uiStorage, uiKey(sc), { dismissed: next }) : { ...(FS.ui ?? loadUi(uiStorage, '')), dismissed: next };
  invalidate('panel');
}
const setFlag = (flag, add = true) => setFlags(withFlag(FS.ui?.dismissed ?? [], flag, add));

/** The page's kernel, with the season's ruleset required (practice and verify refuse another). */
async function seasonKernel() {
  const k = await loadKernel();
  if (FS.season && k.rulesetHash() !== toHex(FS.season.rulesetHash)) { const e = new Error('ruleset'); e.code = 'RulesetMismatch'; throw e; }
  return k;
}

/** Open the report of (P, Q, bell): the herald's /h/clash file, the ClashInputs decoded by this page. */
export async function openReport(p, q, bell) {
  if (!heraldRef || ![p, q, bell].every(Number.isInteger)) return;
  const rep = { p, q, bell, loading: true };
  FS.report = rep; FS.practice = null;
  invalidate('panel');
  const r = await heraldRef.clash(p, q, bell);
  if (FS.report !== rep) return;
  FS.report = r.ok ? { p, q, bell, clash: r } : { p, q, bell, error: r.code ?? 'Error' };
  if (r.ok && FS.mode === 'play') setFlag(FLAG.report);
  invalidate('panel');
}

async function verifyReport() {
  const rep = FS.report;
  if (!rep?.clash || rep.verifying || !FS.clock) return;
  rep.verifying = true;
  invalidate('panel');
  const v = await reportScreen.verifyClash({ p: rep.p, q: rep.q, bell: rep.bell, clash: rep.clash, herald: heraldRef, kernel: seasonKernel, clock: FS.clock, season: FS.season });
  rep.verifying = false;
  if (FS.report === rep) { rep.verify = v; invalidate('panel'); }
}

const retreatChoiceOf = bps => (RETREAT_CHOICES.find(c => c.bps === (bps ?? 0))?.id ?? 'never');

function openWhatIf() {
  const rep = FS.report, v = rep?.verify;
  if (!v?.args) return;
  const mine = mineOf(FS.holdings);
  const own = v.args.arrivals.filter(f => mine(f.id));
  const st = practiceScreen.practiceState({ faction: FS.citizen?.faction });
  if (own[0]) { st.stance = own[0].posture <= 3 ? own[0].posture : 0; st.retreat = retreatChoiceOf(own[0].retreatBps); }
  st.whatif = { p: rep.p, q: rep.q, bell: rep.bell, args: v.args, outcome: v.outcome, mine, mineCount: own.length, result: null };
  FS.practice = st;
  invalidate('panel');
}

async function runPractice({ reroll = false } = {}) {
  const st = FS.practice;
  if (!st) return;
  let k;
  try { k = await seasonKernel(); FS.practiceError = null; } catch (e) { FS.practiceError = e?.code ?? 'NoWasm'; invalidate('panel'); return; }
  if (st.whatif) {
    const retreat = st.retreat === 'never' ? null : retreatBps(st.retreat, st.ratio);
    st.whatif.result = practiceScreen.whatIf(k, st.whatif, st.whatif.mine, { stance: st.stance, retreat });
  } else {
    if (reroll || !st.seed) st.seed = practiceScreen.practiceSeed();
    st.result = practiceScreen.runScenario(k, st, st.seed);
    if (st.result.ok) st.history = [...st.history, st.stance].slice(-12);
  }
  if (FS.pin) setFlag(FLAG.practice); else invalidate('panel');
}

/** The wave-4 actions (data-act). */
export const W4_ACTIONS = {
  'report-open': d => openReport(num(d.p), num(d.q), num(d.bell)),
  'report-close': () => { FS.report = null; invalidate('panel'); },
  'report-verify': () => verifyReport(),
  'report-whatif': () => openWhatIf(),
  'practice-open': () => { FS.practice = practiceScreen.practiceState({ faction: FS.citizen?.faction }); invalidate('panel'); },
  'practice-close': () => { FS.practice = FS.mode === 'practice' ? practiceScreen.practiceState({ faction: FS.citizen?.faction }) : null; invalidate('panel'); },
  'practice-scenario': d => {
    const st = FS.practice;
    if (!st || !practiceScreen.SCENARIOS[d.scenario]) return;
    Object.assign(st, { scenario: d.scenario, troops: practiceScreen.SCENARIOS[d.scenario].troops, result: null, seed: null });
    invalidate('panel');
  },
  'practice-reroll': () => runPractice({ reroll: true }),
  'ob-seen': d => { if (FLAG[d.flag]) setFlag(FLAG[d.flag]); },
  'ob-skip': d => setFlag(FLAG.skip(String(d.step))),
  'ob-dismiss': () => setFlag(FLAG.dismissed),
  'ob-restore': () => setFlags(restoreFlags(FS.ui?.dismissed ?? [])),
};
/** The wave-4 forms (data-form). */
export const W4_FORMS = { 'practice-run': () => runPractice(), 'practice-whatif': () => runPractice() };
/** The wave-4 bound inputs (data-bind), all of the practice panel. */
export function w4Bind(name, value) {
  const st = FS.practice;
  if (!st) return false;
  if (name === 'pr-stance') st.stance = Math.max(0, Math.min(3, num(value) || 0));
  else if (name === 'pr-retreat') st.retreat = RETREAT_CHOICES.some(c => c.id === value && c.id !== 'custom') ? value : 'never';
  else if (name === 'pr-troops') st.troops = Math.max(100, Math.min(30_000, num(value) || 100));
  else return false;
  return true;
}

/** Route the screens' clicks, forms and bound inputs: the wave-4 screens here, the play screens to the controller (game page only). */
function delegate(doc) {
  const play = FS.mode === 'play';
  const run = p => Promise.resolve(p).catch(e => { FS.notice = { ok: false, code: e?.code ?? 'Error', text: String(e?.message ?? e) }; invalidate('panel'); });
  const action = name => HUD_ACTIONS[name] ?? W4_ACTIONS[name] ?? (play ? ACTIONS[name] : undefined);
  doc.addEventListener('click', e => {
    const el = e.target.closest?.('[data-act]');
    const fn = el && !el.disabled ? action(el.dataset.act) : undefined;
    if (!fn) return;
    if (el.tagName !== 'INPUT') e.preventDefault();
    run(fn(el.dataset));
  });
  doc.addEventListener('submit', e => {
    const f = e.target.closest?.('form[data-form]');
    const fn = f ? W4_FORMS[f.dataset.form] ?? (play ? FORMS[f.dataset.form] : undefined) : undefined;
    if (!fn) return;
    e.preventDefault();
    run(fn(f));
  });
  doc.addEventListener('change', e => {
    const el = e.target.closest?.('[data-bind]');
    if (!el) return;
    if (w4Bind(el.dataset.bind, el.value)) invalidate('panel');
    else if (play) bind(el.dataset.bind, el.value);
  });
}

/** The spectator's poll: the events and the bell sheet at load, then every 30 s (§4.2), paused while hidden. */
function startSpectate(herald) {
  useHerald(herald);
  let errors = 0, first = true;
  const tick = async () => {
    const wait = nextPoll({ kind: 'own', hidden: first ? false : globalThis.document?.hidden, errors });
    first = false;
    if (wait === null) { globalThis.setTimeout(tick, 5000); return; }
    try {
      await refreshPlay();
      FS.bellItems = spectateScreen.spectateBells(FS, FS.chain?.now() ?? 0);
      errors = 0;
    } catch (e) { errors++; console.error('frontier spectate:', e); }
    invalidate('panel');
    globalThis.setTimeout(tick, wait * 1000);
  };
  tick();
}

async function loadSeason(herald) {
  const r = await herald.season();
  if (!r.ok) { FS.error = { code: r.code, text: clientText(r.code) }; invalidate('status'); return false; }
  const { record, season } = r;
  try {
    FS.pin = setPin({ programId: record.programId, cluster: record.cluster, seasonId: record.season, seasonAddress: record.seasonAddress ?? null, rulesetHash: toHex(season.rulesetHash) });
  } catch (e) {
    FS.error = { code: e.code, text: e.message };
    invalidate('status');
    return false;
  }
  herald.pin(record.season, FS.pin.addresses);
  FS.beacon = checkBeacon({ drand: record.drand, seasonPkHash: season.quicknetPkHash, cluster: record.cluster, seasonNetwork: season.network });
  if (!FS.beacon.ok) FS.error = { code: FS.beacon.code, text: clientText(FS.beacon.code) };
  FS.record = record;
  FS.season = season;
  FS.clock = seasonClock(season);
  // Only a localnet Clock runs accelerated (§8.7); every other cluster's rate is 1.
  FS.chain.setAccelerated(record.cluster === 'localnet');
  FS.chain.observe(record.latestUnix, record.latestSlot);
  invalidate('chip', 'status', 'map');
  return true;
}

async function loadOverviews(herald) {
  const rings = Math.max(1, FS.record?.rings?.length ?? 1);
  for (let d = 0; d < rings; d++) {
    const r = await herald.overview(d);
    if (r.ok) FS.overviews.set(d, r);
  }
  invalidate('map');
}

/** Start the page. */
export async function boot() {
  const cfg = config();
  FS.mode = cfg.mode;
  setRelay(cfg.relay);
  FS.chain = new ChainClock();
  mountLangToggle($('lang-box'));
  // The phone bottom sheet (W5-E; mounted here since W6-D, R3).
  mountSheet();
  const herald = createHerald({ base: cfg.herald });
  rosterRef = createRoster({ base: cfg.herald ?? '', onChange: () => { mapRef?.invalidate(); invalidate('panel'); } });
  FS.roster = rosterRef;
  let map = null;
  const canvas = $('frontier-map');
  heraldRef = herald;
  if (FS.mode === 'practice') FS.practice = practiceScreen.practiceState();
  registerRenderers([
    ['chip', renderChip],
    ['status', renderStatus],
    ['map', () => map?.invalidate()],
    ...(FS.mode === 'play' ? [['chips', renderChips], ['tabs', renderPlay], ['panel', renderPlay]] : [['panel', renderMode]]),
    ['rail', renderRail],
  ]);
  delegate(globalThis.document);
  if (canvas) {
    // Tile-LOD terrain from the season record's ring seeds through the rules module (W5-E R3: passed by the app).
    const terrainOf = createTerrain({ onReady: () => { map?.invalidate(); invalidate('panel'); } });
    terrainRef = terrainOf;
    map = new FrontierMap(canvas, {
      source: () => {
        const own = ART_FOG ? [{ p: 2, q: 0 }] : (FS.holdings ?? []).map(h => ({ p: h.p, q: h.q }));
        return { overviews: FS.overviews, ringsOpen: FS.record?.rings?.length ?? 1, own, known: new Set([...own.map(o => `${o.p},${o.q}`), ...(ART_FOG ? ['-1,0', '-1,1', '0,-2', '-2,1'] : [])]), showAll: (ART_PREVIEW && !ART_FOG) || !FS.view.fog, selected: FS.selected, terrainOf,
          // art mode: the decoded Province (holdings' tiers, hosts on tiles, camp), loaded on demand
          viewerFaction: ART_FOG ? 0 : FS.citizen?.faction ?? null,
          demoRoads: ART_ROADS,
          engineStage: ART_ENGINE,
          demoSpecials: ART_RELICS,
          demoRivers: ART_RIVERS,
          alliedPairs: ART_ALLY,
          artReplayRing: ART_RINGOPEN ? Math.max(0, (FS.record?.rings?.length ?? 1) - 1) : null,
          clashOf: ART_ON ? artClashOf : undefined,
          pendingOf: ART_ON ? (p, q) => FS.provinces.get(`${p},${q}`)?.inputs ?? null : undefined,
          people: ART_ON && FS.mode !== 'practice' ? peopleSource : undefined,
          provinceOf: ART_ON ? (p, q) => { const env = FS.provinces.get(`${p},${q}`); if (!env) wantProvince(p, q, () => map?.invalidate()); return env?.province ?? null; } : undefined };
      },
      onSelect: hit => {
        FS.selected = hit;
        setText('map-summary', L`州 ${hit.p},${hit.q} を選びました`);
        // The inspector reads the province envelope (loaded once, on demand).
        if (FS.mode !== 'practice') wantProvince(hit.p, hit.q, () => { map?.invalidate(); invalidate('panel'); });
        invalidate('map', 'panel');
      },
      onView: (_, lod) => { FS.view.lod = lod; },
      onHover: (hit, at) => showTip(hit, at),
      // Sprite art at tile LOD, opt-in with ?art=1 (docs/frontier/art/tiles/LOD.md).
      art: ART_ON,
    });
    mapRef = map;
    // The art preview opens on the tiles with everything shown (presentation only).
    if (ART_RINGOPEN) setInterval(() => map?.invalidate(), 6000);
    // ?art=1&battle=P,Q,BELL plays that clash on a loop (presentation only; the demo's battle shot)
    const bq = ART_PREVIEW ? (ART_Q.get('battle') ?? '').split(',').map(Number) : [];
    if (bq.length === 3 && bq.every(Number.isInteger)) {
      const go = () => playBattle(bq[0], bq[1], bq[2], { focus: !FS.battleFocused }).then(ok => { if (ok) FS.battleFocused = true; });
      setTimeout(go, 4000); setInterval(go, (PHASE.end + 2.5) * 1000);
    }
    if (ART_PREVIEW) {
      const [ap, aq] = (ART_Q.get('at') ?? '2,0').split(',').map(Number);
      const c = provincePixel(Number.isInteger(ap) ? ap : 2, Number.isInteger(aq) ? aq : 0);
      map.setView({ x: c.x, y: c.y, zoom: 0.8 });
    }
  }
  invalidate('chip', 'status', 'panel');
  if (await loadSeason(herald)) {
    await loadOverviews(herald);
    // Practice runs without a season too (the kernel only needs its own ruleset); a season pins it and keeps its flags.
    if (FS.mode === 'practice') { FS.ui = loadUi(uiStorage, uiKey(scope())); invalidate('panel'); }
    if (FS.mode === 'spectate') startSpectate(herald);
    if (FS.mode === 'play') startPlay({ herald, cfg }).catch(e => console.error('frontier play:', e));
  }
  // The chip ticks every second (never announced: aria-live is off on it).
  globalThis.setInterval?.(() => invalidate('chip'), 1000);
  // The season record every 30 s (own data poll), paused while hidden.
  let errors = 0;
  const poll = async () => {
    const hidden = globalThis.document?.hidden;
    const wait = nextPoll({ kind: 'own', hidden, errors });
    if (wait === null) { globalThis.setTimeout(poll, 5000); return; }
    const ok = await loadSeason(herald);
    errors = ok ? 0 : errors + 1;
    globalThis.setTimeout(poll, wait * 1000);
  };
  globalThis.setTimeout(poll, 30_000);
}

if (globalThis.document && !globalThis.process?.versions?.node) boot().catch(e => console.error('frontier boot:', e));
