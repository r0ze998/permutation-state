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
import { ChainClock, bellChip, countdown, seasonClock, bellStart } from './clock.mjs';
import { effectiveStatus } from './fcodec.mjs';
import { FrontierMap } from './map/fmap.mjs';
import { SEASON_STATUS_TEXT, clientText, factionName, TIERS, BUILDINGS } from './fi18n.mjs';
import { L, fmtNum, mountLangToggle, onLangChange } from '../lang.mjs';
import { toHex } from '../sdk/bytes.mjs';
import { html, raw, setHtml } from '../util.mjs';
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
import { RETREAT_CHOICES, retreatBps, DEPART_STAMINA } from './fmarch.mjs';
import { uiKey, uiStorage, loadUi, saveUi, UI_PREFIX } from './fui.mjs';
import * as hud from './hud/hud.mjs';
import * as inspect from './hud/inspect.mjs';
import * as feed from './hud/feed.mjs';
import * as title from './intro/title.mjs';
import * as marchCard from './hud/marchcard.mjs';
import * as minimap from './hud/minimap.mjs';
import * as search from './hud/search.mjs';
import { createRoster } from './people/roster.mjs';
import * as scene from './people/scene.mjs';
import { activityText } from './people/activity.mjs';
import { hostOwner, provinceFactions, renderNameForm, ownTag } from './people/ui.mjs';
import * as profile from './people/profile.mjs';
import { leaderSvg } from './people/leaders.mjs';
import * as glossary from './hud/glossary.mjs';
import * as milestones from './hud/milestones.mjs';
import * as guide from './hud/guide.mjs';
import { forecast, forecastKey } from './hud/forecast.mjs';
import { BUILD_ITEMS } from './fland.mjs';
import { UNIT_KINDS } from './people/units.mjs';
import { reachTiles, reachSteps } from './hud/reach.mjs';
import { momentSnapshot, detectMoments, liveMoments } from './people/moments.mjs';
import * as pins from './hud/pins.mjs';
import { battleScene, startBattle, battleLive, BATTLE_SPEEDS, PHASE } from './people/battle.mjs';
import { decode as decodeAccount } from './fcodec.mjs';
import { fromBase64 } from '../sdk/bytes.mjs';
import { tileHex } from './fgeo.mjs';
import { project } from '../map.mjs';
import { identityOf, displayName, withProfile, tagKey } from './people/identity.mjs';
import { lifeAt, lordLine } from './people/life.mjs';

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
  if (FS.report) return [...parts, reportScreen.render(FS, mineOf(FS.holdings), { ownerOf: reportOwner })];
  // A march being composed on the map: its card first (hud/marchcard.mjs).
  if (tab === 'map' && FS.compose) parts.push(marchCard.render(FS));
  // Phones and tablets (no left rail below 1100 px): the rail's holdings and to-do list, folded (closed while composing).
  if (tab === 'map' && (FS.holdings ?? []).length) {
    const n = hud.attentionItems(FS).length;
    parts.push(html`<details class="rail-mini" ${raw(n && !FS.compose ? 'open' : '')}><summary>${n ? L`拠点と次の鐘までにやること（${fmtNum(n)}）` : L`拠点と次の鐘までにやること`}</summary>${hud.renderRail(FS)}</details>`);
  }
  // The selection first on the map tab (the player just chose it), then the guide.
  if (tab === 'map' && FS.selected) parts.push(inspect.render(FS, terrainRef, mapRef?.art?.activities ?? null));
  else if (FS.selected) parts.push(inspect.renderBrief(FS, terrainRef));
  parts.push(onboardingCard.render(FS, { open: tab === 'map' }));
  // Another tab keeps one line of the selection (the inspector itself is on the map tab).
  if (tab === 'map') {
    parts.push(joinScreen.render(FS));
  } else if (tab === 'holding') parts.push(holdingScreen.render(FS));
  else if (tab === 'hosts') parts.push(hostScreen.render(FS), exploreScreen.render(FS));
  else if (tab === 'marches') parts.push(marchScreen.render(FS), trackerScreen.render(FS), reportScreen.renderLinks(myReports(FS), L`あなたの衝突の報告`), incomingScreen.render(FS));
  else parts.push(feed.renderCentre(FS.feed ?? [], FS.feedFilter ?? 'all'), bellScreen.render(FS), reportScreen.renderLinks(reportScreen.clashesFrom(FS.chronicle ?? []), L`最近の衝突`), chronicleScreen.render(FS), html`<section aria-labelledby="more-title"><h3 id="more-title">${L`設定`}</h3>
    <label class="choice"><input type="checkbox" data-act="fog" ${FS.view.fog ? '' : 'checked'}>${L`すべてを見せる（どの口座も公開されています）`}</label>
    <label class="choice"><input type="checkbox" data-act="autopan" ${FS.ui?.autoPan ? 'checked' : ''}>${L`自分に関わる戦いが決着したら、地図をそこへ動かして見せる`}</label>
    <div class="choice-row" role="group" aria-label="${L`ガイドの強さ`}"><span>${L`ガイドの強さ`}</span>${guide.GUIDE_LEVELS.map(v => html`<button type="button" class="btn small" data-act="guide-level" data-v="${v}" aria-pressed="${guide.guideLevel(FS) === v ? 'true' : 'false'}">${guide.GUIDE_TEXT[v]()}</button>`)}</div>
    <div class="choice-row" role="group" aria-label="${L`戦いの演出`}"><span>${L`戦いの演出`}</span>${['normal', 'fast', 'off'].map(v => html`<button type="button" class="btn small" data-act="battle-fx" data-v="${v}" aria-pressed="${(FS.ui?.battleFx ?? 'normal') === v ? 'true' : 'false'}">${BATTLE_FX_TEXT[v]()}</button>`)}</div>
    <button type="button" class="btn" data-act="forget">${L`この端末からこのシーズンの鍵を消す`}</button>
    ${onboardingCard.renderRestore(FS)}
    <p><button type="button" class="btn" data-act="practice-open">${L`練習モードを開く`}</button> <button type="button" class="btn" data-act="intro-open">${L`タイトルを見る`}</button></p>
    <p><a href="practice.html">${L`練習`}</a> · <a href="spectate.html">${L`観戦`}</a></p></section>`, pins.renderPins(FS.pins ?? []), renderNameForm(FS), milestones.renderTimeline(FS.mileRecord), glossary.renderGlossary());
  return parts;
}

/** The panel of the practice and spectator pages. */
export function modePanel(FS) {
  if (FS.mode === 'practice') return [renderNotice(FS.notice), practiceScreen.render(FS.practice, { kernelError: FS.practiceError ?? null })];
  return [FS.selected ? inspect.render(FS, terrainRef, mapRef?.art?.activities ?? null) : '', spectateScreen.render(FS, { ownerOf: reportOwner, standings: hud.renderStandingsList(FS) }), FS.report ? '' : pins.renderPins(FS.pins ?? [])];
}

/**
 * Re-render a panel without losing what the player was doing: the values
 * typed into its forms (inputs whose value differs from the markup's), the
 * focused field and the scroll position come back after the new markup.
 */
export function keepState(el, render, scroller = el) {
  if (!el?.querySelectorAll) { render(); return; }
  const keyOf = x => { const f = x.closest('form[data-form]')?.dataset.form ?? x.closest('[data-bind]')?.dataset.bind ?? ''; return `${f}|${x.name || x.dataset.bind || x.id}`; };
  const typed = new Map();
  for (const x of el.querySelectorAll('input, select, textarea')) {
    if (x.type === 'checkbox' || x.type === 'radio') continue;
    if (x.value !== (x.defaultValue ?? x.value)) typed.set(keyOf(x), x.value);
  }
  const active = globalThis.document?.activeElement;
  const focusKey = active && el.contains(active) && active.matches?.('input, select, textarea') ? keyOf(active) : null;
  const top = scroller?.scrollTop ?? 0;
  render();
  if (typed.size || focusKey) for (const x of el.querySelectorAll('input, select, textarea')) {
    const k = keyOf(x);
    if (typed.has(k) && x.type !== 'checkbox' && x.type !== 'radio') x.value = typed.get(k);
    if (k === focusKey) x.focus({ preventScroll: true });
  }
  if (scroller && scroller.scrollTop !== top) scroller.scrollTop = top;
}

let lastPanelView = null;
function renderPlay() {
  const tabs = $('tabs');
  if (tabs) setHtml(tabs, renderTabs(FS));
  const body = $('panel-body');
  // the same view keeps its scroll; a new tab, report or practice run starts at the top
  const view = `${FS.tab ?? 'map'}|${FS.report ? 'r' : ''}|${FS.practice ? 'p' : ''}`;
  const same = view === lastPanelView;
  lastPanelView = view;
  if (body) keepState(body, () => setHtml(body, panelMarkup(FS)), same ? $('panel') : null);
  const title = $('panel-title');
  const sub = FS.practice ? L`練習モード` : FS.report ? L`衝突の報告` : null;
  if (title) setText('panel-title', sub ?? { map: L`地図`, holding: L`拠点`, hosts: L`軍勢`, marches: L`進軍`, more: L`その他` }[FS.tab ?? 'map'] ?? L`シーズン`);
  renderRail();
  renderMinimap();
  // the header follows new data at once, not only on the next one-second tick
  renderHudTick(FS.chain?.now() ?? null);
}

function renderMode() {
  const body = $('panel-body');
  if (body) setHtml(body, modePanel(FS));
  renderRail();
  renderMinimap();
}

// ------------------------------------------------------------------ the HUD (hud/hud.mjs)
let mapRef = null;
let terrainRef = null;
/** Who holds each site (people/roster.mjs): names and faces for tags, the rail, the inspector, reports. */
let rosterRef = null;
/** The face and name of a report row's owner (a host id; holdings' ids give none). */
const reportOwner = id => { try { return hostOwner(rosterRef, id); } catch { return null; } };
/** The people layer's inputs, rebuilt at most once a second (the chronicle changes on polls only). */
let peopleCache = { at: -1, value: null };
export function peopleSource() {
  const bell = FS.nowBell ?? (FS.clock ? Math.max(0, Math.floor(((FS.chain?.now() ?? 0) - FS.clock.genesisTs) / 600)) : 0);
  const sec = Math.floor(Date.now() / 1000);
  if (peopleCache.at === sec && peopleCache.value) { peopleCache.value.battles = (FS.battles ?? []).filter(b => battleLive(b, performance.now() / 1000)); return peopleCache.value; }
  if (rosterRef) {
    for (let d = 0; d < (FS.record?.rings?.length ?? 1); d++) rosterRef.ensure(d);
    scene.seedOwn(rosterRef, FS.holdings);
  }
  peopleCache = { at: sec, value: {
    departures: scene.departuresAt(FS.chronicle, FS.overviews, bell),
    explores: scene.exploresAt(FS.chronicle, FS.overviews, bell),
    marches: ownColumns(),
    // the token layer (people/units.mjs): who explores, whose column walks its road, the stamina a march needs
    exploringHosts: new Set(scene.exploresAt(FS.chronicle, FS.overviews, bell).map(x => x.host).filter(Boolean)),
    marchingHosts: new Set(ownColumns().map(m => m.host)),
    restBelow: DEPART_STAMINA,
    constructions: constructionsNow(),
    moments: liveMoments(FS.moments ?? [], performance.now() / 1000),
    nameOf: ownNamer(scene.namer(rosterRef)),
    life: FS.life ?? null,
    tierName: t => TIERS[t] ?? '',
    now: FS.chain?.now() ?? 0,
    lordOf: (p, q, site) => { const o = rosterRef?.ownerOf(p, q, site); return o ? identityOf(o.tag) : null; },
    bell,
    own: FS.holdings ?? [],
    columnLabel: d => L`出陣 · 第${fmtNum(d.arriveBell)}鐘に到着`,
    demo: ART_PREVIEW && ART_Q.get('acts') === '1',
    battles: FS.battles ?? [],
    lossText: n => L`−${fmtNum(n)} 兵`,
    fateText: f => ({ Stays: L`持ちこたえた`, Withdrew: L`隣へ退いた`, Bounced: L`押し戻された`, Retreated: L`撤退した`, Destroyed: L`壊滅` })[f] ?? null,
  } };
  peopleCache.value.battles = (FS.battles ?? []).filter(b => battleLive(b, performance.now() / 1000));
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
let routeShown = null;
/** When a composed march gets its route, the camera frames origin and destination at tile detail. */
function frameRoute() {
  const c = FS.compose;
  const key = c?.route && c.dest ? `${c.host.id}:${c.dest.p},${c.dest.q},${c.dest.tile}` : null;
  if (!key || key === routeShown || !mapRef) { routeShown = key ?? routeShown; return; }
  routeShown = key;
  const hx = marchCard.routeHexes(c).map(h => project(h.q, h.r));
  if (!hx.length) return;
  const xs = hx.map(p => p.x), ys = hx.map(p => p.y);
  const size = mapRef.size(), span = Math.max(Math.max(...xs) - Math.min(...xs), (Math.max(...ys) - Math.min(...ys)) * 1.4, 1);
  const zoom = Math.max(0.55, Math.min(1.4, (Math.min(size.width, size.height) * 0.6) / span));
  const phone = globalThis.matchMedia?.('(max-width: 759px)').matches;
  mapRef.setView({ x: (Math.min(...xs) + Math.max(...xs)) / 2, y: (Math.min(...ys) + Math.max(...ys)) / 2 + (phone ? size.height * 0.22 / zoom : 0), zoom });
}

/** The resource breakdown under the header (hud.renderBreakdown): every holding's store of one resource. */
function renderResPop(now) {
  let el = $('res-pop');
  if (!FS.resOpen || FS.mode !== 'play') { if (el) el.hidden = true; return; }
  const doc = globalThis.document;
  if (!el && doc) {
    el = doc.createElement('div');
    el.id = 'res-pop'; el.className = 'res-pop'; el.setAttribute('role', 'dialog');
    doc.body.appendChild(el);
  }
  if (!el) return;
  el.hidden = false;
  if (FS.resOpen === '*') { el.setAttribute('aria-label', L`資源`); setHtmlIfChanged(el, hud.renderAllBreakdowns(hud.allBreakdowns(FS.holdings ?? [], now))); return; }
  const b = hud.resourceBreakdown(FS.holdings ?? [], FS.resOpen, now);
  el.setAttribute('aria-label', b.name);
  setHtmlIfChanged(el, hud.renderBreakdown(b));
}
/** The term popover (hud/glossary.mjs): one definition, a link to the glossary. */
function renderTermPop() {
  let el = $('term-pop');
  if (!FS.term) { if (el) el.hidden = true; return; }
  const doc = globalThis.document;
  if (!el && doc) { el = doc.createElement('div'); el.id = 'term-pop'; el.className = 'res-pop term-pop'; el.setAttribute('role', 'dialog'); doc.body.appendChild(el); }
  if (!el) return;
  el.hidden = false;
  el.setAttribute('aria-label', glossary.TERMS[FS.term]?.name() ?? '');
  setHtmlIfChanged(el, glossary.renderTerm(FS.term));
}
function closeTermPop() { if (FS.term) { FS.term = null; renderTermPop(); } }
// ------------------------------------------------------------------ milestones (hud/milestones.mjs)
const BOOT_AT = Date.now();
const MILE_QUIET_MS = 20_000;   // what loads in the first seconds is old news, recorded without a banner
let mileTimer = null;
function checkMilestones() {
  if (FS.mode !== 'play' || !Number.isInteger(FS.citizen?.faction)) return;
  const sc = scope();
  if (!sc) return;
  const key = milestones.MILESTONE_KEY + uiKey(sc).slice(UI_PREFIX.length);
  if (FS.mileRecord === undefined) FS.mileRecord = milestones.loadSeen(uiStorage, key);
  const quiet = Date.now() - BOOT_AT < MILE_QUIET_MS;
  const { fresh, record } = milestones.newMilestones(quiet && !FS.mileRecord ? null : FS.mileRecord ?? { v: 1, seen: {} }, milestones.reachedMilestones(FS), FS.nowBell ?? 0);
  if (Object.keys(record.seen).length !== Object.keys(FS.mileRecord?.seen ?? {}).length) { FS.mileRecord = record; uiStorage.set(key, JSON.stringify(record)); }
  if (!quiet && fresh.length) { FS.mileQueue = [...(FS.mileQueue ?? []), ...fresh]; showMilestone(); }
}
function showMilestone() {
  const doc = globalThis.document;
  if (!doc || mileTimer || !(FS.mileQueue ?? []).length) return;
  const m = FS.mileQueue.shift();
  let el = $('mile-banner');
  if (!el) { el = doc.createElement('div'); el.id = 'mile-banner'; el.className = 'mile-banner'; el.setAttribute('role', 'status'); doc.body.appendChild(el); }
  setHtml(el, milestones.renderBanner(m, FS.citizen?.faction));
  el.hidden = false;
  mileTimer = setTimeout(closeMilestone, milestones.BANNER_MS);
}
function closeMilestone() {
  clearTimeout(mileTimer); mileTimer = null;
  const el = $('mile-banner');
  if (el) el.hidden = true;
  if ((FS.mileQueue ?? []).length) setTimeout(showMilestone, 600);
}

// ------------------------------------------------------------------ the viewer's own name (people/profile.mjs, UI plan F4)
/**
 * The viewer's own marches on the road, for the people layer: from the
 * holding's tile toward the destination this browser sealed, `k` the share
 * of the way from the departure bell to the arrival bell. Never anyone
 * else's (their destinations are sealed).
 */
function ownColumns() {
  const out = [];
  const now = FS.chain?.now?.() ?? 0;
  if (!FS.clock) return out;
  for (const m of FS.marches ?? []) {
    const e = m.entry, d = m.dest;
    if (!e || !d || !Number.isInteger(e.departBell) || !Number.isInteger(e.arriveBell)) continue;
    if (['revealed', 'settled', 'failed'].includes(e.state)) continue;
    const hp = hostParts(e.host);
    const h = hp && (FS.holdings ?? []).find(x => x.p === hp.p && x.q === hp.q && x.site === hp.site);
    if (!h) continue;
    const t0 = bellStart(FS.clock.genesisTs, e.departBell), t1 = bellStart(FS.clock.genesisTs, e.arriveBell);
    const k = t1 > t0 ? (now - t0) / (t1 - t0) : 1;
    if (k >= 1) continue;
    const host = (FS.provinces.get(`${hp.p},${hp.q}`)?.province?.entries ?? []).find(x => String(x.id) === String(e.host));
    out.push({ host: String(e.host), from: e.route ? { p: e.route.p, q: e.route.q, tile: e.route.tile } : { p: h.p, q: h.q, tile: h.tile }, to: { p: d.p, q: d.q, tile: d.tile }, route: e.route ?? null,
      k: Math.max(0, k), faction: FS.citizen?.faction ?? 0, kind: UNIT_KINDS[host?.unit ?? 0] ?? 'spearman', troops: host ? Math.floor(Number(host.troops) / 1000) : null });
  }
  return out;
}

/**
 * Buildings going up (public: the BUILD records of the chronicle, folded into FS.life; and the
 * viewer's own queue): `[{p, q, site, share, label}]` — the share of the time gone, the
 * building's name and the time left.
 */
function constructionsNow() {
  const now = FS.chain?.now?.() ?? 0, out = [], seen = new Set();
  const add = (p, q, site, item, doneAt, startTs) => {
    const key = `${p},${q},${site}`;
    if (seen.has(key) || !(doneAt > now)) return;
    seen.add(key);
    const total = Math.max(60, doneAt - startTs);
    const name = BUILDINGS[BUILD_ITEMS[item]?.resource] ?? '';
    out.push({ p, q, site, name, share: Math.max(0, Math.min(1, 1 - (doneAt - now) / total)), label: `${name} ${hud.span(doneAt - now)}`.trim() });
  };
  for (const h of FS.holdings ?? []) for (const q of h.queue ?? []) {
    const done = Number(q.doneAt ?? 0);
    if (done > now) add(h.p, h.q, h.site, q.kind, done, done - buildSecsOf(q));
  }
  if (FS.clock) for (const [k, rec] of FS.life ?? []) {
    if (!rec.build || !(rec.build.doneAt > now)) continue;
    const [p, q, site] = k.split(',').map(Number);
    add(p, q, site, rec.build.item, rec.build.doneAt, bellStart(FS.clock.genesisTs, rec.build.bell));
  }
  return out;
}
/** A queued build's length (the first copy's hour when the queue does not say). */
const buildSecsOf = q => (Number(q.secs) > 0 ? Number(q.secs) : 3_600);

let reachCache = { key: null, tiles: [], t0: 0 };
/** The tiles the composed march's host could reach (cached per host and loaded provinces). */
function reachNow() {
  const c = FS.compose;
  if (!c?.host || c.dest || c.sending) { reachCache.key = null; return null; }
  const key = `${c.host.id}|${FS.provinces.size}`;
  if (reachCache.key !== key) {
    const tiles = reachTiles({ start: { p: c.origin.p, q: c.origin.q, tile: c.host.tile }, steps: reachSteps(c.host.staminaValue ?? c.host.stamina ?? 120),
      provinceOf: (p, q) => FS.provinces.get(`${p},${q}`)?.province ?? null, faction: FS.citizen?.faction ?? null });
    reachCache = { key, tiles, t0: reachCache.key && reachCache.key.split('|')[0] === String(c.host.id) ? reachCache.t0 : performance.now() / 1000 };
  }
  return { tiles: reachCache.tiles, t0: reachCache.t0 };
}

/** The map's name tags use the viewer's verified profile on their own holdings. */
function ownNamer(base) {
  const p = FS.ownProfile, tag = p ? ownTag(FS) : null;
  if (!p || tag === null) return base;
  const key = tagKey(tag);
  return (...a) => { const r = base(...a); if (!r || r.identity.tag !== key) return r; const id = withProfile(r.identity, p); return { ...r, identity: id, name: displayName(id) }; };
}
let profileChecked = null;
/** Load and verify the stored profile once per (season, wallet). */
async function checkOwnProfile() {
  const sc = scope(), w = FS.wallet?.address;
  const k = sc && w ? `${sc.seasonId}:${w}` : null;
  if (!k || k === profileChecked) return;
  profileChecked = k;
  const p = profile.loadOwnProfile(uiStorage, sc.seasonId, w);
  FS.ownProfile = p && (await profile.verifyProfile(p)) ? p : null;
  if (FS.ownProfile) { peopleCache.at = -1; invalidate('panel', 'rail', 'map'); }
}
async function saveOwnName(raw) {
  const sc = scope(), w = FS.wallet;
  const name = profile.validName(raw);
  if (!name) { FS.notice = { ok: false, code: 'BadName', text: L`名前は1〜24文字の文字・数字・空白・「-」「_」「.」で付けてください` }; invalidate('panel'); return; }
  if (!sc || !w?.signMessage) return;
  FS.nameBusy = true; invalidate('panel');
  try {
    const p = await profile.makeProfile({ season: sc.seasonId, wallet: w.address, name }, async bytes => (await w.signMessage(bytes)).signature);
    if (!(await profile.verifyProfile(p))) throw Object.assign(new Error('verify'), { code: 'WalletBadSignature' });
    profile.saveOwnProfile(uiStorage, p);
    FS.ownProfile = p;
    FS.notice = { ok: true, text: L`名前を「${name}」にしました` };
  } catch (e) {
    FS.notice = { ok: false, code: e?.code ?? 'Error', text: L`名前を保存できませんでした（${e?.code ?? e?.message ?? 'error'}）` };
  } finally { FS.nameBusy = false; peopleCache.at = -1; invalidate('panel', 'rail', 'map'); }
}

// ------------------------------------------------------------------ the guide (hud/guide.mjs)
/** The to-do pill's items at the guide's level: all, warnings only, or none. */
function pillItems() {
  const lv = guide.guideLevel(FS);
  if (lv === 'off') return [];
  const items = hud.attentionItems(FS);
  return lv === 'warn' ? items.filter(x => guide.WARN_KINDS.has(x.kind)) : items;
}
let guideCache = { at: 0, value: null };
function guideNow() {
  const t = Date.now();
  if (t - guideCache.at > 1000) guideCache = { at: t, value: guide.guideTarget(FS) };
  return guideCache.value;
}
/** The guide card's "show me": the target in view, and for the first march the card with host and destination filled in. */
async function guideGo() {
  const g = guide.guideTarget(FS);
  if (!g) return;
  if (g.kind === 'settle') { setLens('settle'); FS.tab = 'map'; invalidate('panel', 'tabs', 'map'); return; }
  if (g.kind === 'build') { FS.tab = 'holding'; invalidate('panel', 'tabs', 'rail'); requestAnimationFrame(() => HUD_ACTIONS['hp-jump']({ id: 'hp-build' })); return; }
  if (g.kind === 'scout') { goToItem({ p: g.p, q: g.q, tile: g.tile, tab: 'hosts' }); return; }
  if (g.kind === 'march') {
    goToItem({ p: g.p, q: g.q, tile: g.tile, tab: 'map' });
    if (g.host && FS.mode === 'play' && !FS.compose) {
      ACTIONS.compose({ host: g.host, stay: 'map' });
      setLens('war');
      await ACTIONS['dest-quick']({ p: String(g.p), q: String(g.q), tile: String(g.tile) });
    }
  }
}

function nudgeField(el, name, f) {
  const i = el?.closest?.('form')?.querySelector?.(`input[name="${String(name).replace(/[^\w-]/g, '')}"]`);
  if (!i) return;
  const min = i.min === '' ? -Infinity : Number(i.min), max = i.max === '' ? Infinity : Number(i.max);
  i.value = String(Math.max(min, Math.min(max, Math.round(f(Number(i.value) || 0)))));
  i.dispatchEvent(new Event('input', { bubbles: true }));
}

function closeResPop() { if (FS.resOpen) { FS.resOpen = null; renderHudTick(FS.chain?.now?.() ?? 0); } }

/** The march card's forecast: recomputed with the rules when the composed march changes (hud/forecast.mjs). */
let fcBusy = null;
function updateForecast() {
  const c = FS.compose;
  const key = forecastKey(c);
  if (!key || !c.route || c.sending) return;
  if (c.forecast?.key === key || fcBusy === key) return;
  fcBusy = key;
  c.forecast = { key, pending: true };
  invalidate('panel');
  seasonKernel().then(k => {
    if (FS.compose !== c || forecastKey(c) !== key) return;
    const prov = FS.provinces.get(`${c.dest.p},${c.dest.q}`)?.province ?? null;
    let fc;
    try { fc = forecast(k, c, prov, FS.citizen?.faction ?? 0); } catch (e) { fc = { ok: false, why: e?.code ?? 'Error' }; }
    c.forecast = { key, ...fc };
  }).catch(() => { if (FS.compose === c) c.forecast = { key, ok: false, why: 'NoWasm' }; })
    .finally(() => { fcBusy = null; invalidate('panel'); });
}

/** Once a second: compare the page's state with the last look and start the moments it shows (people/moments.mjs). */
let momentPrev = null;
function checkMoments() {
  if (!FS.provinces) return;
  const next = momentSnapshot({ life: FS.life ?? new Map(), constructions: constructionsNow(), provinces: FS.provinces });
  const t = performance.now() / 1000;
  const fresh = detectMoments(momentPrev, next, t);
  momentPrev = next;
  if (fresh.length) { FS.moments = liveMoments([...(FS.moments ?? []), ...fresh], t); peopleCache.at = -1; mapRef?.invalidate(); }
}

function renderHudTick(now) {
  const m = hud.bellModel(FS.clock, now);
  checkMoments();
  frameRoute();
  updateForecast();
  ringToll(m.bell);
  renderIntro();
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
    const w = globalThis.innerWidth ?? 1440;
    setHtmlIfChanged(strip, hud.renderStrip(tokens, FS.resOpen ?? null, w < 760 ? 1 : w < 1100 ? 3 : w < 1440 ? 3 : w < 1700 ? 5 : 8));
  }
  renderResPop(now ?? 0);
  checkMilestones();
  checkOwnProfile();
  if (FS.pins === undefined) { const sc = scope(); if (sc) { FS.pins = pins.loadPins(uiStorage, sc.seasonId); if (FS.pins.length) { invalidate('map'); renderMinimap(); } } }
  tickFeed();
  renderFeed();
  const attn = $('attn-pill');
  if (attn) {
    const items = FS.mode === 'play' ? pillItems() : [];
    const text = hud.attentionText(items);
    attn.hidden = !text;
    // phones show only the count (the header keeps one row: no layout shift when it appears); the full text is its name
    if (text && attn.getAttribute('aria-label') !== text) {
      attn.setAttribute('aria-label', text);
      setHtml(attn, html`<span class="attn-long" aria-hidden="true">${text}</span><span class="attn-short" aria-hidden="true">${fmtNum(items.length)}</span>`);
    }
  }
}

// ------------------------------------------------------------------ notifications (hud/feed.mjs)
let feedPrev = null;
let feedBell = null;
/** Compare what the page knows with the last tick's; new changes become notifications; a new bell folds the last one into a summary. */
function tickFeed() {
  if (FS.mode !== 'play' || !FS.citizen || !FS.chronicle) return;
  const snap = feed.snapshot(FS);
  const fresh = feed.diffFeed(feedPrev, snap);
  feedPrev = snap;
  let items = fresh;
  if (feedBell !== null && snap.bell !== null && snap.bell > feedBell) {
    const sum = feed.bellSummary(FS.feed ?? [], feedBell);
    if (sum) items = [sum, ...items];
  }
  if (snap.bell !== null) feedBell = snap.bell;
  if (!items.length) return;
  FS.feed = feed.pushFeed(FS.feed ?? [], items);
  // Civ VII's "pan to combat", opt-in: the newest battle of the viewer's plays where it happened
  const b = fresh.find(x => x.battle);
  if (b && FS.ui?.autoPan) playBattle(b.battle.p, b.battle.q, b.battle.bell, { focus: true });
  invalidate('panel');
}

function renderFeed() {
  const el = $('feed');
  if (!el) return;
  setHtmlIfChanged(el, feed.renderToasts(FS.feed ?? [], { dismissed: FS.feedDismissed ?? new Set() }));
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
  // the lord out on their land (people/life.mjs): what they are doing
  const lf = owner && FS.life ? lifeAt(FS.life.get(`${hit.p},${hit.q},${t.site}`), FS.nowBell ?? 0, FS.chain?.now() ?? 0) : null;
  if (lf?.lord) lines.push(html`<span class="tip-lord">${lordLine(displayName(identityOf(owner.tag)), lf.doing, L)}</span>`);
  for (const a of (acts ?? []).filter((a, i, all) => all.findIndex(b => b.kind === a.kind) === i)) lines.push(html`<span class="tip-${a.kind}">${activityText(a)}</span>`);
  setHtml(tip, lines.map(l => html`<span class="tip-line">${l}</span>`));
  tip.hidden = false;
  tip.style.setProperty('--x', `${Math.round(at.x + 16)}px`);
  tip.style.setProperty('--y', `${Math.round(at.y + 12)}px`);
}

// ------------------------------------------------------------------ battle scenes (people/battle.mjs)
const battleSeen = new Map(); // "P,Q" → the last resolved bell a scene was considered for
/** Load a clash and play its scene on the map; `focus` moves the camera to it first. */
export async function playBattle(p, q, bell, { focus = false, auto = false } = {}) {
  const fx = FS.ui?.battleFx ?? 'normal';
  if (auto && fx === 'off') return false;
  if (!heraldRef || ![p, q, bell].every(Number.isInteger)) return false;
  const r = await heraldRef.clash(p, q, bell).catch(() => null);
  if (!r?.ok || !r.inputs) return false;
  let before = null;
  try { before = r.report?.province_before_b64 ? decodeAccount('Province', fromBase64(r.report.province_before_b64)) : null; } catch { before = null; }
  const after = FS.provinces.get(`${p},${q}`)?.province ?? null;
  const scene = battleScene({ p, q, bell, inputs: r.inputs, before, after: after && after.resolvedNext > bell ? after : null });
  if (!scene) return false;
  if (focus && mapRef) { const h = tileHex(p, q, scene.tiles[0].idx), c = project(h.q, h.r); mapRef.setView({ x: c.x, y: c.y, zoom: 2.1 }); }
  FS.battles = [...(FS.battles ?? []).filter(b => !(b.scene.p === p && b.scene.q === q)), startBattle(scene, performance.now() / 1000, BATTLE_SPEEDS[fx] || 1)];
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
    playBattle(env.province.p, env.province.q, b, { auto: true });
  }
}

// ------------------------------------------------------------------ the minimap and the lenses (hud/minimap.mjs)
let miniQueued = false;
function renderMinimap() {
  if (miniQueued) return;
  miniQueued = true;
  requestAnimationFrame(() => {
    miniQueued = false;
    const cv = $('minimap-canvas');
    if (!cv || !mapRef || !cv.getContext) return;
    const px = minimap.MINIMAP_PX, dpr = Math.min(globalThis.devicePixelRatio || 1, 2);
    if (cv.width !== px * dpr) { cv.width = px * dpr; cv.height = px * dpr; }
    const recs = new Map();
    for (const ov of FS.overviews.values()) for (const r of ov.provinces) recs.set(`${r.p},${r.q}`, r);
    minimap.paintMinimap(cv.getContext('2d'), { recs, rings: Math.max(1, (FS.record?.rings?.length ?? 1)), own: (FS.holdings ?? []).map(h => ({ p: h.p, q: h.q })),
      view: mapRef.view, size: mapRef.size(), px, dpr, lens: FS.view.lens ?? 'realm', pins: FS.pins ?? [] });
  });
}
function renderLenses() {
  const el = $('lenses');
  if (!el) return;
  const cur = FS.view.lens ?? 'realm';
  setHtmlIfChanged(el, minimap.LENSES.map((l, i) => html`<button type="button" class="lens" data-act="lens" data-lens="${l}" aria-pressed="${l === cur ? 'true' : 'false'}" title="${minimap.LENS_TEXT[l]()} (${i + 1})"><span aria-hidden="true">${minimap.LENS_GLYPH[l]}</span><span class="lens-text">${minimap.LENS_TEXT[l]()}</span></button>`));
}
// ------------------------------------------------------------------ map search (hud/search.mjs)
let searchHits = [];
function runSearch(q) {
  const recs = new Map();
  for (const ov of FS.overviews.values()) for (const r of ov.provinces) recs.set(`${r.p},${r.q}`, r);
  searchHits = search.searchMap(q, { recs, roster: rosterRef, sitesOf: (p, q2) => terrainRef?.(p, q2)?.sites ?? null, pins: FS.pins ?? [] });
  const el = $('map-search-results');
  if (el) setHtml(el, search.renderResults(searchHits, q));
}

function setLens(l) {
  if (!minimap.LENSES.includes(l)) return;
  FS.view.lens = l;
  renderLenses(); renderMinimap(); mapRef?.invalidate();
}

// ------------------------------------------------------------------ the title and the bell toll (intro/title.mjs)
let introDolly = null;
function introLive() {
  const total = hud.standings(FS.overviews).reduce((a, r) => a + r.holdings, 0);
  const bell = FS.clock ? bellChip(FS.clock, FS.chain?.now() ?? null).bell : null;
  return title.liveLine({ seasonId: FS.record?.season ?? null, bell, holdings: total });
}
function renderIntro() {
  const el = $('intro');
  if (!el || el.hidden) return;
  setHtmlIfChanged(el, title.render({ mode: FS.mode, live: introLive() }));
}
export function openIntro() {
  const el = $('intro');
  if (!el) return;
  el.hidden = false;
  lastHtml.delete('intro');
  renderIntro();
  el.querySelector('.intro-go')?.focus({ preventScroll: true });
  // the camera drifts in from the mist toward the fitted view
  if (mapRef) {
    let target = null, t0 = 0;
    const dur = 7000;
    introDolly = () => {
      if (el.hidden) { introDolly = null; return; }
      // wait for the map's first fit (the season's rings), then drift from far above the Concord to it
      if (!target) {
        if (!FS.record) { requestAnimationFrame(introDolly); return; }
        mapRef.fit(mapRef.source(), mapRef.size());   // every open ring, now that the season says how many
        target = { ...mapRef.view }; t0 = performance.now();
      }
      const k = (performance.now() - t0) / dur;
      mapRef.setView(title.dolly(target, k));
      if (k < 1) requestAnimationFrame(introDolly); else introDolly = null;
    };
    requestAnimationFrame(introDolly);
  }
}
export function closeIntro() {
  const el = $('intro');
  if (!el || el.hidden) return;
  el.hidden = true;
  title.markSeen(globalThis.localStorage);
  $('frontier-map')?.focus({ preventScroll: true });
}
let tollBell = null;
/** The bell toll: the new bell's number rings over the map for a moment. */
function ringToll(bell) {
  if (!Number.isInteger(bell)) return;
  if (tollBell !== null && bell > tollBell) {
    const el = $('bell-toll');
    if (el) { el.textContent = title.tollText(bell); el.classList.remove('ring'); void el.offsetWidth; el.classList.add('ring'); }
  }
  tollBell = bell;
}

/** Move the map to an attention item and open its tab. */
/** The report's map buttons close it, so the map (and on phones the sheet's map) is in view. */
function leaveReport() { if (FS.report) { FS.report = null; invalidate('panel'); } }
const BATTLE_FX_TEXT = { normal: () => L`ふつう`, fast: () => L`早送り`, off: () => L`自動では見せない` };

function goToItem(x) {
  if (!x) return;
  // the item's holding becomes the active one (its tab then shows that holding)
  if (x.holding && FS.holdings?.includes(x.holding)) FS.activeHolding = FS.holdings.indexOf(x.holding);
  if (Number.isInteger(x.tile)) {
    const h = tileHex(x.p, x.q, x.tile), c = project(h.q, h.r);
    mapRef?.setView({ x: c.x, y: c.y, zoom: 1.0 });
    FS.selected = { kind: 'tile', p: x.p, q: x.q, idx: x.tile };
  } else {
    mapRef?.focus(x.p, x.q, 0.6);
    FS.selected = { kind: 'province', p: x.p, q: x.q };
  }
  if (FS.mode === 'play' && x.tab) FS.tab = x.tab;
  invalidate('map', 'panel', 'tabs', 'rail');
}

/** The HUD's actions (data-act): cycle the attention items, jump to one, choose the active holding. */
/** The next (dir +1) or previous (−1) host ready to march, selected on the map (Civ's "next unit"). */
function cycleHost(dir) {
  const ready = FS.holdings?.length ? hostScreen.hostRows(FS).filter(r => r.state === 1 && !r.inTransit && !r.pending) : [];
  if (!ready.length) return;
  FS.hostCursor = (((FS.hostCursor ?? -1) + dir) % ready.length + ready.length) % ready.length;
  const r = ready[FS.hostCursor];
  goToItem({ p: r.p, q: r.q, tile: r.tile, tab: 'map' });
}
/** The next or previous holding (Civ's "next city"). */
function cycleHolding(dir) {
  const hs = FS.holdings ?? [];
  if (!hs.length) return;
  const i = (((FS.activeHolding ?? 0) + dir) % hs.length + hs.length) % hs.length;
  FS.activeHolding = i;
  goToItem({ p: hs[i].p, q: hs[i].q, tile: hs[i].tile, tab: 'holding', holding: hs[i] });
}

export const HUD_ACTIONS = {
  'search-go': d => { const x = searchHits[num(d.i)]; if (!x) return; goToItem({ ...x, tab: FS.mode === 'play' ? 'map' : undefined }); const el = $('map-search-results'); if (el) setHtml(el, ''); },
  lens: d => setLens(d.lens),
  goto: d => { leaveReport(); closeResPop(); goToItem({ p: num(d.p), q: num(d.q), tab: 'map' }); },
  'intro-close': () => closeIntro(),
  'intro-open': () => openIntro(),
  'feed-go': d => goToItem((FS.feed ?? []).find(x => x.id === d.id)),
  'feed-dismiss': d => { FS.feedDismissed = new Set([...(FS.feedDismissed ?? []), d.id]); renderFeed(); },
  'feed-filter': d => { FS.feedFilter = feed.FEED_FILTERS.includes(d.f) ? d.f : 'all'; invalidate('panel'); },
  autopan: () => { const sc = scope(); const v = !FS.ui?.autoPan; FS.ui = sc ? saveUi(uiStorage, uiKey(sc), { autoPan: v }) : { ...(FS.ui ?? {}), autoPan: v }; invalidate('panel'); },
  'battle-play': d => { leaveReport(); playBattle(num(d.p), num(d.q), num(d.bell), { focus: true }); },
  'res-open': d => { FS.resOpen = FS.resOpen === d.r ? null : d.r; renderHudTick(FS.chain?.now?.() ?? 0); },
  'res-close': () => closeResPop(),
  // pins (hud/pins.mjs): this device, this season
  'pin-toggle': d => {
    const sc = scope(); if (!sc) return;
    const place = { p: num(d.p), q: num(d.q), tile: d.tile === '' || d.tile === undefined ? null : num(d.tile) };
    if (!Number.isInteger(place.p) || !Number.isInteger(place.q)) return;
    FS.pins = pins.togglePin(FS.pins ?? pins.loadPins(uiStorage, sc.seasonId), place);
    pins.savePins(uiStorage, sc.seasonId, FS.pins);
    invalidate('panel', 'map'); renderMinimap();
  },
  'goto-pin': d => { leaveReport(); goToItem({ p: num(d.p), q: num(d.q), tile: d.tile === '' ? undefined : num(d.tile), tab: 'map' }); },
  // the count steppers (screens/holding.mjs): change the number field of the same form, within its min and max
  step: (d, el) => nudgeField(el, d.name, v => v + (num(d.d) || 0)),
  'step-set': (d, el) => nudgeField(el, d.name, () => num(d.v) || 0),
  'profile-clear': () => { const sc = scope(); if (sc) profile.clearOwnProfile(uiStorage, sc.seasonId); FS.ownProfile = null; peopleCache.at = -1; invalidate('panel', 'rail', 'map'); },
  // the spectator (screens/spectate.mjs, UI plan G3): faction filter, bell timeline, the camera following battles
  'watch-faction': d => { FS.watch = { ...(FS.watch ?? {}), faction: d.f === '' ? null : num(d.f) }; invalidate('panel'); },
  'watch-bell': d => { FS.watch = { ...(FS.watch ?? {}), bell: d.bell === '' ? null : num(d.bell) }; invalidate('panel'); },
  'watch-auto': () => { const v = (FS.watch?.auto ?? true) === false; FS.watch = { ...(FS.watch ?? {}), auto: v }; try { globalThis.localStorage?.setItem('ps-fwatch:v1', v ? '1' : '0'); } catch { /* none */ } invalidate('panel'); },
  'ob-go': () => guideGo(),
  'guide-level': d => { if (!guide.GUIDE_LEVELS.includes(d.v)) return; const sc = scope(); FS.ui = sc ? saveUi(uiStorage, uiKey(sc), { guide: d.v }) : { ...(FS.ui ?? {}), guide: d.v }; guideCache.at = 0; invalidate('panel', 'map', 'rail'); renderHudTick(FS.chain?.now?.() ?? 0); },
  'mile-close': () => closeMilestone(),
  'sel-clear': () => { FS.selected = null; invalidate('map', 'panel'); },
  term: d => { FS.term = FS.term === d.term || !glossary.TERMS[d.term] ? null : d.term; renderTermPop(); $('term-pop')?.querySelector('button')?.focus?.(); },
  'term-close': () => closeTermPop(),
  'glossary-open': () => { closeTermPop(); leaveReport(); if (FS.mode === 'play') FS.tab = 'more'; invalidate('panel', 'tabs'); requestAnimationFrame(() => $('glossary')?.scrollIntoView?.({ block: 'start' })); },
  'hp-jump': d => { const el = /^hp-[a-z]+$/.test(d.id ?? '') ? $(d.id) : null; el?.scrollIntoView?.({ block: 'start', behavior: 'smooth' }); el?.querySelector?.('h4')?.focus?.(); },
  'battle-fx': d => { if (!['normal', 'fast', 'off'].includes(d.v)) return; const sc = scope(); FS.ui = sc ? saveUi(uiStorage, uiKey(sc), { battleFx: d.v }) : { ...(FS.ui ?? {}), battleFx: d.v }; invalidate('panel'); },
  attn: () => {
    const items = pillItems();
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
  // the faction chip carries its leader's portrait (UI plan F1); on phones the portrait alone
  if (f) { f.hidden = !fc; setHtmlIfChanged(f, fc ? html`${raw(leaderSvg(FS.citizen.faction, { size: 26 }))}<span class="chip-text">${fc}</span>` : ''); }
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
export const W4_FORMS = { 'practice-run': () => runPractice(), 'practice-whatif': () => runPractice(),
  'profile-name': f => saveOwnName(f.querySelector('input[name="name"]')?.value ?? ''),
  'map-search': f => { runSearch(f.querySelector('input')?.value ?? ''); if (searchHits[0]) HUD_ACTIONS['search-go']({ i: '0' }); } };
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
  doc.addEventListener('keydown', e => {
    const intro = $('intro');
    if (intro && !intro.hidden && (e.key === 'Escape' || e.key === 'Enter')) { e.preventDefault(); closeIntro(); return; }
    if (e.key === 'Escape' && FS.resOpen) { e.preventDefault(); closeResPop(); return; }
    if (e.key === 'Escape' && FS.term) { e.preventDefault(); closeTermPop(); return; }
    // . , next / previous ready host; ] [ next / previous holding; 1–4 the lenses (never while typing)
    if ((FS.mode !== 'play' && !/^[1-4]$/.test(e.key)) || e.target?.closest?.('input, select, textarea') || e.metaKey || e.ctrlKey || e.altKey) return;
    const k = { '.': () => cycleHost(1), ',': () => cycleHost(-1), ']': () => cycleHolding(1), '[': () => cycleHolding(-1),
      1: () => setLens('realm'), 2: () => setLens('war'), 3: () => setLens('land'), 4: () => setLens('settle') }[e.key];
    if (k) { e.preventDefault(); k(); }
  });
  const play = FS.mode === 'play';
  const run = p => Promise.resolve(p).catch(e => { FS.notice = { ok: false, code: e?.code ?? 'Error', text: String(e?.message ?? e) }; invalidate('panel'); });
  const action = name => HUD_ACTIONS[name] ?? W4_ACTIONS[name] ?? (play ? ACTIONS[name] : undefined);
  doc.addEventListener('click', e => {
    const el = e.target.closest?.('[data-act]');
    const fn = el && !el.disabled ? action(el.dataset.act) : undefined;
    if (!fn) return;
    if (el.tagName !== 'INPUT') e.preventDefault();
    run(fn(el.dataset, el));
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
/** A clash newer than the last one seen: the spectator's camera goes there and plays it (unless turned off). */
let watchSeen = null;
function followBattles() {
  const c = reportScreen.clashesFrom(FS.chronicle ?? [], 1)[0];
  if (!c) return;
  const key = `${c.p},${c.q},${c.bell}`;
  if (watchSeen === null) { watchSeen = key; return; }   // the first look is not news
  if (key === watchSeen) return;
  watchSeen = key;
  const f = FS.watch?.faction;
  if (FS.watch?.auto === false || FS.report || (Number.isInteger(f) && !provinceFactions(FS.overviews, c.p, c.q).includes(f))) return;
  playBattle(c.p, c.q, c.bell, { focus: true }).then(ok => { if (!ok) mapRef?.focus(c.p, c.q, 0.6); });
}

function startSpectate(herald) {
  useHerald(herald);
  try { FS.watch = { ...(FS.watch ?? {}), auto: globalThis.localStorage?.getItem('ps-fwatch:v1') !== '0' }; } catch { /* none */ }
  let errors = 0, first = true;
  const tick = async () => {
    const wait = nextPoll({ kind: 'own', hidden: first ? false : globalThis.document?.hidden, errors });
    first = false;
    if (wait === null) { globalThis.setTimeout(tick, 5000); return; }
    try {
      await refreshPlay();
      FS.bellItems = spectateScreen.spectateBells(FS, FS.chain?.now() ?? 0);
      followBattles();
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
        if (rosterRef) for (let d = 0; d < (FS.record?.rings?.length ?? 1); d++) rosterRef.ensure(d);
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
          // the world view's labels: the faction names and the Concord, in the page's language
          realmName: f => (f === 'concord' ? L`大協約` : factionName(f)),
          lens: FS.view.lens ?? 'realm',
          // incoming risk around the viewer's holdings (hud/hud.mjs attention, controller incoming warnings)
          threats: (FS.incoming ?? []).map(w => ({ p: w.holding.p, q: w.holding.q, tile: w.holding.tile, bell: w.bell })),
          threatLabel: w => L`来襲 第${fmtNum(w.bell)}鐘`,
          // the move preview while a march has a host and no destination yet (hud/reach.mjs)
          reach: reachNow(),
          // the viewer's pins (hud/pins.mjs)
          pins: FS.pins ?? [],
          // the guide's target of the current step (hud/guide.mjs; "all" only)
          guide: guideNow(),
          guideLabel: guide.guideLabel,
          // holdings' tiers for the far view from the roster (no province loads for a spectator's world map)
          tierOf: (p, q, site) => rosterRef?.tierOf(p, q, site) ?? null,
          // the march being composed: its route, drawn for this browser only (the destination is sealed)
          route: FS.compose ? { hexes: marchCard.routeHexes(FS.compose), dest: FS.compose.dest ? tileHex(FS.compose.dest.p, FS.compose.dest.q, FS.compose.dest.tile) : null } : null,
          provinceOf: ART_ON ? (p, q, { far = false } = {}) => { if (!far) wantProvince(p, q, () => map?.invalidate()); return FS.provinces.get(`${p},${q}`)?.province ?? null; } : undefined };
      },
      onSelect: hit0 => {
        // a tap selects what the zoom shows: a province from afar, a tile up close
        const hit = FS.view.lod === 'tile' ? hit0 : { kind: 'province', p: hit0.p, q: hit0.q };
        FS.selected = hit;
        setText('map-summary', L`州 ${hit.p},${hit.q} を選びました`);
        // The inspector reads the province envelope (loaded once, on demand).
        if (FS.mode !== 'practice') wantProvince(hit.p, hit.q, () => { map?.invalidate(); invalidate('panel'); });
        // composing a march: a tap on another tile makes it the destination (Civ: select the unit, click where)
        const c = FS.compose;
        if (FS.mode === 'play' && c && !c.sending && Number.isInteger(hit.idx) && !(hit.p === c.origin.p && hit.q === c.origin.q && hit.idx === c.host.tile)) ACTIONS['dest-from-map']()?.catch?.(() => {});
        invalidate('map', 'panel');
      },
      onView: (_, lod) => { FS.view.lod = lod; renderMinimap(); },
      onHover: (hit, at) => showTip(hit, at),
      // Sprite art at tile LOD, opt-in with ?art=1 (docs/frontier/art/tiles/LOD.md).
      art: ART_ON,
    });
    mapRef = map;
    renderLenses();
    const mini = $('minimap-canvas');
    mini?.addEventListener('pointerup', e => {
      const r = mini.getBoundingClientRect();
      const fr = minimap.frameOf(Math.max(1, FS.record?.rings?.length ?? 1) + 1, r.width);
      const w = fr.toWorld(e.clientX - r.left, e.clientY - r.top);
      map.setView({ x: w.x, y: w.y, zoom: map.view.zoom });
    });
    onLangChange(renderLenses);
    $('map-search-q')?.addEventListener('input', e => runSearch(e.target.value));
    // The art preview opens on the tiles with everything shown (presentation only).
    if (ART_RINGOPEN) setInterval(() => map?.invalidate(), 6000);
    // ?art=1&battle=P,Q,BELL plays that clash on a loop (presentation only; the demo's battle shot)
    const bq = ART_PREVIEW ? (ART_Q.get('battle') ?? '').split(',').map(Number) : [];
    if (bq.length === 3 && bq.every(Number.isInteger)) {
      const go = () => playBattle(bq[0], bq[1], bq[2], { focus: !FS.battleFocused }).then(ok => { if (ok) FS.battleFocused = true; });
      setTimeout(go, 4000); setInterval(go, (PHASE.end + 2.5) * 1000);
    }
    if (ART_PREVIEW || ART_Q.has('at')) {
      // ?at=P,Q[,TILE[,ZOOM]]: a link that opens the map on a province, or on one tile at a zoom (with ?art=1 the preview's default)
      const [ap, aq, at, az] = (ART_Q.get('at') ?? '2,0').split(',').map(Number);
      const P = Number.isInteger(ap) ? ap : 2, Q = Number.isInteger(aq) ? aq : 0;
      const c = Number.isInteger(at) ? (h => project(h.q, h.r))(tileHex(P, Q, at)) : provincePixel(P, Q);
      map.setView({ x: c.x, y: c.y, zoom: Number.isFinite(az) && az > 0 ? Math.min(az, 2.5) : 0.8 });
    }
  }
  invalidate('chip', 'status', 'panel');
  if (title.shouldOpen({ storage: globalThis.localStorage, search: globalThis.location?.search ?? '' })) openIntro();
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
