// Guided first bells (web design §7.11; DESIGN §2.3 with the restated times
// of O-M1-13: first holding about 11–21 minutes after the ticket, first clash
// report about 31–41 minutes after the departure). A pure state machine: its
// inputs are chain facts first — the Citizen and its land stage, the
// Holding's queue, reserve, explore record and transits, the Citizen's
// explore and arrival counters, the marches' pipeline states — and local
// flags only for what the chain cannot know ("seen" the welcome, a practice
// run, a report opened). So it survives a reload and a change of device (the
// seen flags then show again, which costs one tap). Steps never gate play:
// each one can be skipped and the whole card dismissed; the flags live in
// the season's UI preferences (`ps-fui:…`, field `dismissed`, contract §9.1).
//
// Also here: the adjacent-wedge ticket offer (contract §5.9 FileTicket: when
// the own wedge has no open site left, the program accepts sites in the
// adjacent wedges' outermost open ring; integ-W3 deferred item K11).
import { SCOUT, CITIZEN, freeByWedge, homeWedge, ticketTimes, baseProduction } from './fland.mjs';
import { ringOf, wedgeOf, provinceIndex } from './fgeo.mjs';

/** The steps, in order (design §7.11 table: 1–8). */
export const STEPS = Object.freeze(['welcome', 'join', 'build', 'scout', 'practice', 'march', 'report', 'done']);
/** Local flags (in ps-fui `dismissed`). */
export const FLAG = Object.freeze({
  dismissed: 'onboarding',
  welcome: 'ob:welcome',
  practice: 'ob:practice',
  report: 'ob:report',
  skip: id => `ob:skip:${id}`,
});
/** Minutes the waits take (O-M1-13, web design §6.3): [earliest, latest]. */
export const WAIT_MINUTES = Object.freeze({ ticket: [11, 21], explore: [11, 21], report: [31, 41] });

const NO_TICKET = 0xffffffff;

/**
 * Chain facts for the state machine from the page's store (`FS` shape of
 * controller.mjs): `{citizen, stage, holding, scoutHost, marches}`.
 */
export function factsOf(FS) {
  const holding = FS.holdings?.[0] ?? null;
  const home = holding ? FS.provinces?.get?.(`${holding.p},${holding.q}`)?.province ?? null : null;
  const scoutHost = !!home && (home.entries ?? []).some(e => e.state >= 1 && e.state <= 3 && e.unit === SCOUT && e.faction === FS.citizen?.faction);
  return { citizen: FS.citizen ?? null, stage: FS.land?.stage ?? (FS.citizen ? 'joined' : 'none'), holding, scoutHost, marches: FS.marches ?? [] };
}

/**
 * A Holding has queued or finished a building: its queue holds an item, or
 * it produces more than its tier's base (a founded Hamlet already produces
 * the base: W6-D found the step ticked before any build).
 */
export function hasBuilt(h) {
  if (!h) return false;
  if ((h.queue ?? []).some(x => Number(x.doneAt) > 0)) return true;
  const base = baseProduction(Number(h.tier ?? 0));
  return (h.production ?? []).some((x, r) => BigInt(x) > (base[r] ?? 0n));
}
/** A Holding has Scouts trained or mustered. */
export const hasScouts = (h, scoutHost = false) => scoutHost || (!!h && Number(h.reserve?.[SCOUT] ?? 0) > 0);
/** An exploration was sent (the Citizen counts it; a pending one sits in the Holding). */
export const hasExplored = (c, h) => (!!c && Number(c.explores) > 0) || (!!h && h.explore?.state > 0);
/** A march left (Depart counts it in the Citizen; its transit sits in the Holding). */
export const hasMarched = (c, h) => (!!c && Number(c.arrivals) > 0) || (!!h && (h.transit ?? []).some(t => t.state >= 1));

/**
 * The card's state: `{steps: [{id, status: 'done'|'skipped'|'current'|'todo',
 * wait?}], current (id or null), dismissed, complete}`.
 * `facts = {citizen, stage, holding, scoutHost, marches}`; `flags` = the
 * `dismissed` list of ps-fui (strings); `clock` (seasonClock) for the waits.
 */
export function onboardingState(facts, flags = [], clock = null) {
  const f = new Set(flags);
  const c = facts.citizen, h = facts.holding;
  const stage = facts.stage ?? 'none';
  const resolved = (facts.marches ?? []).some(m => m.facts?.pipeline === 'resolved');
  const done = {
    welcome: f.has(FLAG.welcome) || !!c,
    join: stage === 'provisional' || stage === 'final',
    build: hasBuilt(h),
    scout: hasExplored(c, h),
    practice: f.has(FLAG.practice),
    march: hasMarched(c, h),
    report: f.has(FLAG.report),
  };
  const steps = [];
  let current = null;
  for (const id of STEPS.slice(0, -1)) {
    const status = done[id] ? 'done' : f.has(FLAG.skip(id)) ? 'skipped' : current === null ? 'current' : 'todo';
    if (status === 'current') current = id;
    steps.push({ id, status, wait: waitOf(id, { stage, citizen: c, holding: h, marches: facts.marches ?? [], resolved, clock }) });
  }
  const complete = current === null;
  steps.push({ id: 'done', status: complete ? 'current' : 'todo', wait: null });
  return { steps, current: complete ? 'done' : current, dismissed: f.has(FLAG.dismissed), complete };
}

/** What a step is waiting for, if anything: `{kind, at?, minutes}` (the card shows the expected time). */
function waitOf(id, { stage, citizen, holding, marches, resolved, clock }) {
  if (id === 'join' && stage === 'ticket' && citizen && citizen.ticketBell !== NO_TICKET) {
    const t = clock ? ticketTimes(clock, citizen.ticketBell) : null;
    return { kind: 'ticket', at: t?.resultAbout ?? null, minutes: WAIT_MINUTES.ticket };
  }
  if (id === 'scout' && holding?.explore?.state > 0) return { kind: 'explore', at: null, minutes: WAIT_MINUTES.explore };
  if (id === 'report') {
    const m = marches.find(x => x.facts?.pipeline && x.facts.pipeline !== 'resolved');
    if (m && !resolved) return { kind: 'report', pipeline: m.facts.pipeline, at: null, minutes: WAIT_MINUTES.report };
  }
  return null;
}

/** The flags list after one change (`add` or remove), as ps-fui stores it (≤ 64 strings). */
export function withFlag(flags, flag, add = true) {
  const set = new Set(flags ?? []);
  if (add) set.add(flag); else set.delete(flag);
  return [...set].slice(-64);
}

/** Flags after "show the guide again": the card and every skipped step return (seen flags stay). */
export const restoreFlags = flags => (flags ?? []).filter(x => x !== FLAG.dismissed && !x.startsWith('ob:skip:'));

/** The report the card offers at step 7: the first march of the viewer whose clash is resolved (`{p, q, bell}`), or null. */
export function reportOffer(marches) {
  for (const m of marches ?? []) {
    if (m.facts?.pipeline !== 'resolved' || !m.dest) continue;
    const bell = m.entry?.arriveBell ?? m.transit?.arriveBell;
    if (Number.isInteger(bell)) return { p: m.dest.p, q: m.dest.q, bell };
  }
  return null;
}

// ------------------------------------------------------------------ adjacent-wedge tickets (§5.9)
/** The two wedges next to wedge w. */
export const adjacentWedges = w => [(w + 5) % 6, (w + 1) % 6];

/**
 * Where a ticket may go when the home wedge is full: `{full, ring,
 * provinces: [{p, q, ring, wedge, free}]}`. `full` when the ring overviews
 * show no free site left in the faction's wedge (an upper bound: the
 * program decides on its folded counters, and refuses otherwise); then the
 * provinces of the adjacent wedges in the outermost open ring (`ringsOpen
 * − 1`) with a free site, nearest to the home wedge first.
 */
export function overflowProvinces(overviews, faction, ringsOpen) {
  const free = freeByWedge(overviews);
  const w = homeWedge(faction);
  if (!Number.isInteger(w) || free[w] > 0) return { full: false, ring: null, provinces: [] };
  const ring = Math.max(0, Number(ringsOpen) - 1);
  const wedges = adjacentWedges(w);
  const out = [];
  for (const ov of overviews.values()) {
    if (!ov?.provinces || ov.ring !== ring) continue;
    for (const pr of ov.provinces) {
      const pw = wedgeOf(pr.p, pr.q);
      if (!wedges.includes(pw)) continue;
      const n = pr.sites.filter((s, i) => (s === 0 || s === 3) && pr.owners[i] === 7).length;
      if (n > 0 && ringOf(pr.p, pr.q) === ring) out.push({ p: pr.p, q: pr.q, ring, wedge: pw, free: n });
    }
  }
  out.sort((a, b) => b.free - a.free || provinceIndex(a.p, a.q) - provinceIndex(b.p, b.q));
  return { full: true, ring, provinces: out };
}

/** Whether the Citizen has a first holding the onboarding no longer needs to explain (final). */
export const settled = c => !!c && (c.flags & CITIZEN.FIRST_FINAL) !== 0;
