// Boot of the Frontier pages (index, practice, spectate): config → the
// herald's season record → pins (program, cluster, season, ruleset,
// beacon) → the chain clock → the shell (bell chip, staleness banner,
// language toggle) → the map. The screens of waves 3–4 (join, holding,
// host, march, tracker, bell sheet, report, practice, onboarding) plug into
// the render scheduler (fstate.mjs); W4-E routes them here.
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
  FS.stale = staleness({ latestUnix: FS.record?.latestUnix, chainNow: now });
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
  herald.pin(record.season);
  FS.beacon = checkBeacon({ drand: record.drand, seasonPkHash: season.quicknetPkHash, cluster: record.cluster });
  if (!FS.beacon.ok) FS.error = { code: FS.beacon.code, text: clientText(FS.beacon.code) };
  FS.record = record;
  FS.season = season;
  FS.clock = seasonClock(season);
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
  ]);
  if (canvas) {
    map = new FrontierMap(canvas, {
      source: () => ({ overviews: FS.overviews, ringsOpen: FS.record?.rings?.length ?? 1, own: [], known: new Set(), showAll: !FS.view.fog, selected: FS.selected }),
      onSelect: hit => {
        FS.selected = hit;
        setText('map-summary', L`州 ${hit.p},${hit.q} を選びました`);
        invalidate('map');
      },
      onView: (_, lod) => { FS.view.lod = lod; },
    });
  }
  invalidate('chip', 'status');
  if (await loadSeason(herald)) await loadOverviews(herald);
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
