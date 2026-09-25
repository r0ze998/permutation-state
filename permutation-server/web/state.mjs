// The client's single store, its selectors, and the render scheduler.
//
// Update code changes S and calls invalidate('part', …); the parts are
// re-rendered once, in a fixed order, at the end of the current task
// (app.mjs registers the renderers). Renderers only read S and write the DOM.
import * as T from './i18n.mjs';

export const S = {
  // ---- server data
  map: null,             // /api/map (static terrain)
  view: null,            // latest /api/state
  lastTick: null,        // tick of the last applied view
  online: false,
  clockAt: 0,            // performance.now() when `view` arrived (the clock counts down from it)
  watch: null,           // ?watch=N: read-only view through nation N's fog
  myCiv: 0,              // the nation (civ id) this view belongs to
  memberId: null,        // my member id (null when watching)

  // ---- selection (map + inspector)
  tile: null,            // selected tile key "q,r"
  unit: null,            // selected unit id
  unitPreview: null,     // /api/preview/unit for `unit`
  cityPreview: null,     // /api/preview/city for the selected own city ({id, …})
  cityLoading: null,     // city id whose preview is being fetched
  moveWhy: null,         // {key, text}: why the selected unit cannot reach `tile`
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

/** Run the renderers of the invalid parts now. */
export function flush() {
  scheduled = false;
  if (!S.view) { dirty.clear(); return; }
  for (const [name, fn] of renderers) {
    if (!dirty.delete(name)) continue;
    try { fn(); } catch (e) { console.error(`render ${name}:`, e); } // one broken panel must not stop the others
  }
}
