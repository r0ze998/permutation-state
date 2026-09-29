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
import { ACTIONS, FORMS, bind, startPlay } from './controller.mjs';
import { renderTabs, renderNotice, factionChip, quotaChip, mountSheet } from './screens/shell.mjs';
import { createTerrain } from './map/terrain.mjs';
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
  parts.push(onboardingCard.render(FS, { open: tab === 'map' }));
  if (tab === 'map') {
    if (FS.selected) parts.push(html`<p class="muted">${L`州 ${FS.selected.p},${FS.selected.q} を選びました`}</p>`);
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
  return spectateScreen.render(FS);
}

function renderPlay() {
  const tabs = $('tabs');
  if (tabs) setHtml(tabs, renderTabs(FS));
  const body = $('panel-body');
  if (body) setHtml(body, panelMarkup(FS));
  const title = $('panel-title');
  const sub = FS.practice ? L`練習モード` : FS.report ? L`衝突の報告` : null;
  if (title) setText('panel-title', sub ?? { map: L`地図`, holding: L`拠点`, hosts: L`軍勢`, marches: L`進軍`, more: L`その他` }[FS.tab ?? 'map'] ?? L`シーズン`);
}

function renderMode() {
  const body = $('panel-body');
  if (body) setHtml(body, modePanel(FS));
}

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
  const action = name => W4_ACTIONS[name] ?? (play ? ACTIONS[name] : undefined);
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
  let map = null;
  const canvas = $('frontier-map');
  heraldRef = herald;
  if (FS.mode === 'practice') FS.practice = practiceScreen.practiceState();
  registerRenderers([
    ['chip', renderChip],
    ['status', renderStatus],
    ['map', () => map?.invalidate()],
    ...(FS.mode === 'play' ? [['chips', renderChips], ['tabs', renderPlay], ['panel', renderPlay]] : [['panel', renderMode]]),
  ]);
  delegate(globalThis.document);
  if (canvas) {
    // Tile-LOD terrain from the season record's ring seeds through the rules module (W5-E R3: passed by the app).
    const terrainOf = createTerrain({ onReady: () => map?.invalidate() });
    map = new FrontierMap(canvas, {
      source: () => {
        const own = (FS.holdings ?? []).map(h => ({ p: h.p, q: h.q }));
        return { overviews: FS.overviews, ringsOpen: FS.record?.rings?.length ?? 1, own, known: new Set(own.map(o => `${o.p},${o.q}`)), showAll: !FS.view.fog, selected: FS.selected, terrainOf };
      },
      onSelect: hit => {
        FS.selected = hit;
        setText('map-summary', L`州 ${hit.p},${hit.q} を選びました`);
        invalidate('map', 'panel');
      },
      onView: (_, lod) => { FS.view.lod = lod; },
      // Sprite art at tile LOD, opt-in with ?art=1 (docs/frontier/art/tiles/LOD.md).
      art: new URLSearchParams(globalThis.location?.search ?? '').get('art') === '1',
    });
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
