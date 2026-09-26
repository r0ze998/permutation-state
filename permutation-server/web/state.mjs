// The client's single store, its selectors, and the render scheduler.
//
// Update code changes S and calls invalidate('part', …); the parts are
// re-rendered once, in a fixed order, at the end of the current task
// (app.mjs registers the renderers). Renderers only read S and write the DOM.
import * as T from './i18n.mjs';
import { onLangChange } from './lang.mjs';

export const S = {
  // ---- server data
  map: null,             // /api/map (static terrain)
  view: null,            // latest /api/state
  lastTick: null,        // tick of the last applied view
  online: false,
  clockAt: 0,            // performance.now() when `view` arrived (the clock counts down from it)
  watch: null,           // ?watch=N: read-only view as nation N (perfect information: same world)
  myCiv: 0,              // the nation (civ id) this view belongs to
  memberId: null,        // my member id (null when watching)

  // ---- selection (map + inspector)
  tile: null,            // selected tile key "q,r"
  unit: null,            // selected unit id
  unitPreview: null,     // /api/preview/unit for `unit`
  cityPreview: null,     // /api/preview/city for the selected own city ({id, …})
  cityLoading: null,     // city id whose preview is being fetched
  moveWhy: null,         // {key, text, blocked}: why the selected unit cannot reach `tile` (text rebuilt from blocked on a language switch)
  patrol: null,          // patrol being drawn: {unit, from, route}

  // ---- this tick's orders
  drafts: [],            // described orders (orders.mjs describe())
  committedJson: '[]',   // drafts as last sealed
  committedRationale: '',
  adopt: {},             // role → proposal ids to adopt
  committedAdopt: '[]',
  committing: false,
  sealed: null,          // offices sealed by the last commit (POST /api/orders `offices`)
  myVotes: {},           // role → candidate voted for this election
  hintedPropose: false, hintedDock: false,
  pulseChip: null,       // index of a just-added chip to highlight

  // ---- panels
  drawer: null,          // open drawer name
  lens: 'normal',
  research: null,        // /api/preview/research
  diplo: {},             // civ id → /api/preview/diplomacy
  diploOpen: new Set(),  // civ rows expanded in the diplomacy drawer
  marketQuote: null,
  exchangeStep: 0, exDraft: null,
  chronFilter: 'all',
  decFilter: 'all',
  decisions: null, verified: {}, proof: null, proofCiv: null, proofTick: null,

  // ---- tick report and next decision
  changes: [],           // what changed for my nation last tick (next.mjs tickChanges)
  summaryOpen: true, summaryCollapsed: false, summaryPinned: false,
  nextIndex: 0,          // Space cycles through the next-decision items

  // ---- chain lens
  chainOpen: false,
  season: null,          // gateway /season
  ticks: null,           // gateway /ticks, newest first
  lastSealed: null,      // last sealed tick seen (drives the chain beat)
  mini: null,            // minimap transform

  // ---- lobby
  lobbyPick: { civ: null, stand: ['General', 'Steward'] },
  joinName: '',

  // ---- chain membership (chain mode: the browser holds the keys)
  wallet: null,          // connected wallet (wallet.mjs connected()): {name, address, signMessage, signTransaction}, or null
  session: null,         // this member's session key (session.mjs): {publicKey, sign(bytes), …}, or null (view only)
  chainMember: null,     // my member in the gateway's /season: {index, civ, name, wallet, session}, or null
  claim: null,           // the season-end claim card (claim.mjs)
  // ---- chain play (chainplay.mjs; the sealed batches themselves are in localStorage, sealbook.mjs)
  chainSeals: null,      // gateway /tick for my nation: {tick, phase, committed[4], revealed[4], at}
  govQueue: [],          // governance actions waiting for the next commit phase
  autoCommit: null,      // chain auto-commit's memory (sealbook.mjs autoCommitStep): the draft state, its attempts, a retry
};

// ================================================================== selectors
export const isWatching = () => S.watch !== null;
export const civ = id => S.view?.civs?.[id];
export const civN = id => T.civName(civ(id)?.name ?? `#${id}`);
export const myNation = () => civ(S.myCiv);
export const unitById = id => S.view?.units.find(u => u.id === id);
export const cityById = id => S.view?.cities.find(c => c.id === id);
export const myUnits = () => S.view.units.filter(u => u.owner === S.myCiv);
export const myCities = () => S.view.cities.filter(c => c.owner === S.myCiv);
export const membersOf = civId => (S.view?.members || []).filter(m => m.civ === civId);
/** Offices this member holds (role names). */
export const held = () => (S.view?.gov?.offices || []).filter(o => o.holder && o.holder.id === S.memberId).map(o => o.role);
export const officeInfo = role => (S.view?.economy?.offices || []).find(o => o.role === role) || { budget: 0, bank: 0, spendable: 0 };
/** Order slots available to one office, or to all offices held. */
export const spendable = role => (role ? officeInfo(role).spendable : held().reduce((a, r) => a + officeInfo(r).spendable, 0));
/**
 * The operator's AI members as far as they are public (/api/state
 * `aiRoster`: {aiCount, bountyEach, homeTick, revealed, fallen}). A play
 * server from before `aiRoster` sent it as `roster` (an object, where the
 * nation's member list is an array).
 */
export function aiRoster(v = S.view) {
  const obj = x => (x && typeof x === 'object' && !Array.isArray(x) ? x : null);
  const r = obj(v?.aiRoster) ?? obj(v?.roster) ?? {};
  return { aiCount: 0, bountyEach: '0', homeTick: null, revealed: false, ...r, fallen: Array.isArray(r.fallen) ? r.fallen : [] };
}
/** My nation's members (/api/state `roster`: {id, name, kind, merit, active, activeWindows}); [] when the view has none. */
export const nationRoster = (v = S.view) => (Array.isArray(v?.roster) ? v.roster : []);
/** Seconds to the deadline, counted down locally between polls. */
export function secondsLeft() {
  const v = S.view;
  if (!v) return 0;
  return v.paused ? v.secondsLeft : Math.max(0, v.secondsLeft - (performance.now() - S.clockAt) / 1000);
}

// ================================================================== render scheduler
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

/** Re-render every part (the display language changed, or anything else they all show). */
export const invalidateAll = () => invalidate('all');
// A language switch re-renders every part in place: the HUD, the minimap
// labels, whichever drawer and inspector are open (lang.mjs setLang). The
// map canvas redraws its banners and flags every frame by itself.
onLangChange(invalidateAll);

/** Run the renderers of the invalid parts now. */
export function flush() {
  scheduled = false;
  if (!S.view) { dirty.clear(); return; }
  for (const [name, fn] of renderers) {
    if (!dirty.delete(name)) continue;
    try { fn(); } catch (e) { console.error(`render ${name}:`, e); } // one broken panel must not stop the others
  }
}
