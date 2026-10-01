// The Frontier page's store and render scheduler (the pattern of v9's
// state.mjs, which is v9-shaped and stays untouched). Update code changes
// FS and calls invalidate('part', …); each part re-renders once, in the
// order registered, at the end of the current task. Renderers only read FS
// and write the DOM; one failing renderer never stops the others. A
// language switch re-renders every part.
import { onLangChange } from '../lang.mjs';

export const FS = {
  // ---- pins and the season (fchainio.setPin, herald.season)
  mode: 'play',          // 'play' | 'practice' | 'spectate'
  pin: null,             // {programId, cluster, seasonId, addresses}
  record: null,          // the herald's season record (JSON conveniences: display only)
  season: null,          // the Season account, decoded from its bytes
  beacon: null,          // checkBeacon() result: {ok, kind: 'quicknet'|'test'}
  // ---- time
  chain: null,           // ChainClock
  clock: null,           // seasonClock(season)
  // ---- the viewer
  wallet: null,          // connected wallet (wallet.mjs shape) or null
  session: null,         // fsession key or null
  citizen: null,         // decoded Citizen or null
  holdings: [],          // decoded Holdings
  book: [],              // marchbook entries
  // ---- the map
  overviews: new Map(),  // ring → decoded overview
  provinces: new Map(),  // "P,Q" → parsed envelope
  view: { lod: 'world', fog: true, showAll: false },
  selected: null,        // {kind: 'province'|'tile', p, q, idx?}
  // ---- status
  error: null,           // {code, text}
  stale: { behind: null, stale: false },
};

/** The holding the play screens act on: the one chosen in the rail (FS.activeHolding), else the first. */
export function activeHolding(fs = FS) {
  const hs = fs.holdings ?? [];
  const i = Number.isInteger(fs.activeHolding) && fs.activeHolding >= 0 && fs.activeHolding < hs.length ? fs.activeHolding : 0;
  return hs[i] ?? null;
}

const renderers = new Map();
const dirty = new Set();
let scheduled = false;

/** Register renderers as [name, fn] pairs; they run in this order. */
export function registerRenderers(list) { for (const [name, fn] of list) renderers.set(name, fn); }

/** Mark parts for re-render ('all' for every part). */
export function invalidate(...parts) {
  for (const p of parts) {
    if (p === 'all') for (const k of renderers.keys()) dirty.add(k);
    else dirty.add(p);
  }
  if (!scheduled) { scheduled = true; queueMicrotask(flush); }
}
export const invalidateAll = () => invalidate('all');
onLangChange(invalidateAll);

/** Run the renderers of the invalid parts now. */
export function flush() {
  scheduled = false;
  for (const [name, fn] of renderers) {
    if (!dirty.delete(name)) continue;
    try { fn(FS); } catch (e) { console.error(`render ${name}:`, e); }
  }
}
