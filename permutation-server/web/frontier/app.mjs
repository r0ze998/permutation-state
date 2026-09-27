// Boot of the Frontier pages (index, practice, spectate): config → the
// herald's season record → pins (program, cluster, season, ruleset,
// beacon) → the chain clock → the shell (bell chip, staleness banner,
// language toggle) → the map. On the game page (W3-F) the play screens
// follow: the faction and quota chips, the bottom tabs and the panel of
// each tab (join and sites, holding, hosts and explore, the march composer
// with the tracker and incoming warnings, the bell sheet and chronicle),
// driven by controller.mjs. The screens of wave 4 (report, practice,
// onboarding, spectator) plug into the same scheduler; W4-E routes them.
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
import { renderTabs, renderNotice, factionChip, quotaChip } from './screens/shell.mjs';
import * as joinScreen from './screens/join.mjs';
import * as holdingScreen from './screens/holding.mjs';
import * as hostScreen from './screens/host.mjs';
import * as exploreScreen from './screens/explore.mjs';
import * as marchScreen from './screens/march.mjs';
import * as trackerScreen from './screens/tracker.mjs';
import * as incomingScreen from './screens/incoming.mjs';
import * as bellScreen from './screens/bell.mjs';
import * as chronicleScreen from './screens/chronicle.mjs';

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

/** The panel of the current tab (the game page). */
export function panelMarkup(FS) {
  const tab = FS.tab ?? 'map';
  const parts = [renderNotice(FS.notice)];
  if (tab === 'map') {
    if (FS.selected) parts.push(html`<p class="muted">${L`州 ${FS.selected.p},${FS.selected.q} を選びました`}</p>`);
    parts.push(joinScreen.render(FS));
  } else if (tab === 'holding') parts.push(holdingScreen.render(FS));
  else if (tab === 'hosts') parts.push(hostScreen.render(FS), exploreScreen.render(FS));
  else if (tab === 'marches') parts.push(marchScreen.render(FS), trackerScreen.render(FS), incomingScreen.render(FS));
  else parts.push(bellScreen.render(FS), chronicleScreen.render(FS), html`<section aria-labelledby="more-title"><h3 id="more-title">${L`設定`}</h3>
    <label class="choice"><input type="checkbox" data-act="fog" ${FS.view.fog ? '' : 'checked'}>${L`すべてを見せる（どの口座も公開されています）`}</label>
    <button type="button" class="btn" data-act="forget">${L`この端末からこのシーズンの鍵を消す`}</button>
    <p><a href="practice.html">${L`練習`}</a> · <a href="spectate.html">${L`観戦`}</a></p></section>`);
  return parts;
}

function renderPlay() {
  const tabs = $('tabs');
  if (tabs) setHtml(tabs, renderTabs(FS));
  const body = $('panel-body');
  if (body) setHtml(body, panelMarkup(FS));
  const title = $('panel-title');
  if (title) setText('panel-title', { map: L`地図`, holding: L`拠点`, hosts: L`軍勢`, marches: L`進軍`, more: L`その他` }[FS.tab ?? 'map'] ?? L`シーズン`);
}

function renderChips() {
  const f = $('faction-chip');
  const fc = factionChip(FS.citizen);
  if (f) { f.hidden = !fc; f.textContent = fc ?? ''; }
  const q = $('quota-chip');
  const qc = quotaChip(FS.quota);
  if (q) { q.hidden = !qc; q.textContent = qc ?? ''; }
}

/** Route the screens' clicks, forms and bound inputs to the controller. */
function delegate(doc) {
  const run = p => Promise.resolve(p).catch(e => { FS.notice = { ok: false, code: e?.code ?? 'Error', text: String(e?.message ?? e) }; invalidate('panel'); });
  doc.addEventListener('click', e => {
    const el = e.target.closest?.('[data-act]');
    if (!el || el.disabled || !ACTIONS[el.dataset.act]) return;
    if (el.tagName !== 'INPUT') e.preventDefault();
    run(ACTIONS[el.dataset.act](el.dataset));
  });
  doc.addEventListener('submit', e => {
    const f = e.target.closest?.('form[data-form]');
    if (!f || !FORMS[f.dataset.form]) return;
    e.preventDefault();
    run(FORMS[f.dataset.form](f));
  });
  doc.addEventListener('change', e => {
    const el = e.target.closest?.('[data-bind]');
    if (el) bind(el.dataset.bind, el.value);
  });
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
  const herald = createHerald({ base: cfg.herald });
  let map = null;
  const canvas = $('frontier-map');
  registerRenderers([
    ['chip', renderChip],
    ['status', renderStatus],
    ['map', () => map?.invalidate()],
    ...(FS.mode === 'play' ? [['chips', renderChips], ['tabs', renderPlay], ['panel', renderPlay]] : []),
  ]);
  if (canvas) {
    map = new FrontierMap(canvas, {
      source: () => {
        const own = (FS.holdings ?? []).map(h => ({ p: h.p, q: h.q }));
        return { overviews: FS.overviews, ringsOpen: FS.record?.rings?.length ?? 1, own, known: new Set(own.map(o => `${o.p},${o.q}`)), showAll: !FS.view.fog, selected: FS.selected };
      },
      onSelect: hit => {
        FS.selected = hit;
        setText('map-summary', L`州 ${hit.p},${hit.q} を選びました`);
        invalidate('map', 'panel');
      },
      onView: (_, lod) => { FS.view.lod = lod; },
    });
  }
  invalidate('chip', 'status');
  if (await loadSeason(herald)) {
    await loadOverviews(herald);
    if (FS.mode === 'play') {
      delegate(globalThis.document);
      startPlay({ herald, cfg }).catch(e => console.error('frontier play:', e));
    }
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
