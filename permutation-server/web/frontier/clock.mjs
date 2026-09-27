// The bell clock (contract §5.1; web design §6). Rule times come from the
// chain's Clock as the herald last saw it, extrapolated by the local clock
// — never from the local clock alone. Reveal windows are anchored: before
// THE anchor exists there is no countdown, only "waiting for the beacon".
//
// The round and bell arithmetic transcribes permutation-rules
// `frontier::beacon` (first round at or after x) and is pinned by
// web-frontier-clock.test.mjs against clock-vectors-v1.json.
import { windowAt } from './fcodec.mjs';

export const BELL_SECS = 600;
export const BELLS_PER_DAY = 144;

// ------------------------------------------------------------------ beacon arithmetic
/** Scheduled time of drand round r (r ≥ 1; round 0 reads as 1; a period of 0 as 1). */
export const roundTime = (genesis, period, r) => genesis + (Math.max(Number(r), 1) - 1) * Math.max(period, 1);
/** The first round scheduled at or after t (1 at or before genesis). */
export function firstRoundFrom(genesis, period, t) {
  if (t <= genesis) return 1;
  const p = Math.max(period, 1);
  return Math.ceil((t - genesis) / p) + 1;
}
export const bellStart = (genesisTs, b) => genesisTs + BELL_SECS * b;
export const bellEnd = (genesisTs, b) => genesisTs + BELL_SECS * (b + 1);
/** The bell containing t, or null before genesis. */
export const bellAt = (genesisTs, t) => (t < genesisTs ? null : Math.floor((t - genesisTs) / BELL_SECS));
export const dayOf = b => Math.floor(b / BELLS_PER_DAY);
/** The AnchorArchive part of a bell (v1.3: one archive per region and half day). */
export const archivePartOf = b => Math.floor(b / 72);
/** T(b): the round a march arriving at bell b is sealed to. */
export const tlockRound = (drand, genesisTs, b) => firstRoundFrom(drand.genesis, drand.period, bellEnd(genesisTs, b));
export const revealClose = (a, w) => a + w;
/** S(b, r) = first round at or after A + W + Δ. */
export const seedRound = (drand, close, margin) => firstRoundFrom(drand.genesis, drand.period, close + margin);

/** The clock parameters of a decoded Season (numbers, not BigInts). */
export function seasonClock(season) {
  return {
    genesisTs: Number(season.genesisTs),
    drand: { genesis: Number(season.drandGenesis), period: Number(season.drandPeriod) },
    margin: Number(season.seedMargin),
    window: b => Number(windowAt(season, b)),
    endBell: Number(season.endBell),
  };
}

// ------------------------------------------------------------------ the chain clock
/**
 * "Now" on the chain: the herald's latest (slot, unix) sample, advanced by
 * local elapsed time. The rate is **1** unless the clock is `accelerated`
 * (the pinned cluster is `localnet`, whose Clock runs at the stack's
 * scale): only then is the rate estimated, from two samples at least 2 s
 * apart in which the chain time advanced, and never below 1. A herald that
 * lags and catches up, or stops, can therefore neither run a real
 * cluster's clock fast nor freeze it (integ-W2 review of W2-E: revealStep
 * uses this `now`, and must never post before the arrival bell starts).
 * Monotone: it never goes back.
 */
export class ChainClock {
  constructor({ wall = () => Date.now(), accelerated = false } = {}) {
    this.wall = wall;
    this.accelerated = accelerated;
    this.last = null;
    this.prev = null;
    this.rate = 1;
    this.floor = -Infinity;
    /** min over samples of (wall − chain) seconds: the freshest sample's offset. */
    this.minOffset = Infinity;
    /** Wall ms at which the herald's chain time last moved forward. */
    this.advancedAt = null;
  }

  /** Mark the clock accelerated (localnet) or not; a real cluster resets the rate to 1. */
  setAccelerated(on) {
    this.accelerated = !!on;
    if (!this.accelerated) this.rate = 1;
  }

  /** A herald sample: the Clock sysvar's unix time at a slot. */
  observe(unix, slot) {
    const s = { unix: Number(unix), slot: Number(slot), at: this.wall() };
    if (this.last && s.slot < this.last.slot) return; // an older answer arriving late
    if (this.accelerated && this.last && s.at - this.last.at >= 2000 && s.unix > this.last.unix) {
      const r = (s.unix - this.last.unix) / ((s.at - this.last.at) / 1000);
      this.rate = Math.min(5000, Math.max(1, r));
    }
    if (!this.last || s.unix > this.last.unix) this.advancedAt = s.at;
    this.minOffset = Math.min(this.minOffset, s.at / 1000 - s.unix);
    this.prev = this.last;
    this.last = s;
  }

  /** Chain seconds now (fractional), or null before any sample. */
  now() {
    if (!this.last) return null;
    const t = this.last.unix + ((this.wall() - this.last.at) / 1000) * this.rate;
    this.floor = Math.max(this.floor, t);
    return this.floor;
  }

  /** Seconds since the last sample (the "as of … N s ago" line). */
  age() { return this.last ? (this.wall() - this.last.at) / 1000 : null; }

  /**
   * How far the herald's latest chain time is behind, in seconds, measured
   * independently of the herald-derived estimate: on a real cluster the
   * local wall clock minus the freshest sample's offset; on an accelerated
   * localnet the wall seconds since the herald's chain time last advanced.
   * null before any sample.
   */
  behind() {
    if (!this.last) return null;
    if (this.accelerated) return Math.max(0, (this.wall() - this.advancedAt) / 1000);
    return Math.max(0, this.wall() / 1000 - this.minOffset - this.last.unix);
  }

  /** Local clock minus chain clock, seconds (shown when |offset| > 2 s on a real cluster). */
  offset() {
    if (!this.last || this.rate !== 1) return 0;
    return this.wall() / 1000 - this.now();
  }
}

// ------------------------------------------------------------------ the bell chip
/** `{bell, secondsLeft, beforeGenesis, ended}` at chain time `now`. */
export function bellChip(clock, now) {
  if (now === null || now === undefined) return { bell: null, secondsLeft: null, beforeGenesis: false, ended: false };
  const b = bellAt(clock.genesisTs, now);
  if (b === null) return { bell: null, secondsLeft: Math.ceil(clock.genesisTs - now), beforeGenesis: true, ended: false };
  return { bell: b, secondsLeft: Math.ceil(bellEnd(clock.genesisTs, b) - now), beforeGenesis: false, ended: b >= clock.endBell };
}

// ------------------------------------------------------------------ the bell pipeline (§6.2)
export const PIPELINE = Object.freeze(['open', 'awaitingBeacon', 'revealing', 'awaitingSeed', 'resolving', 'resolved']);

/**
 * The state of bell `b` in one region (and one province, when
 * `resolvedNext` is given) from herald facts:
 *   anchor   {a} — THE BellAnchor's A, or null when absent
 *   archive  {tombstoned, archived, aOff?, seed?} for the region-day, or null
 *   seed     true once a SeedCache of THE anchor exists (or the archive has the seed)
 *   latestRound  the region's BeaconLog round (closes the window at S)
 * Answer: {state, until (chain seconds or null), close, seedRound, archived}.
 * No countdown before THE anchor exists.
 */
export function pipeline(clock, b, { now, anchor = null, archive = null, seed = false, latestRound = 0, resolvedNext = null } = {}) {
  const out = { state: 'open', until: bellEnd(clock.genesisTs, b), close: null, seedRound: null, archived: !!archive?.archived };
  if (resolvedNext !== null && resolvedNext > b) return { ...out, state: 'resolved', until: null };
  if (now < bellEnd(clock.genesisTs, b)) return out;
  let a = anchor?.a ?? null;
  if (a === null && archive?.archived && archive.aOff !== undefined) a = bellEnd(clock.genesisTs, b) + archive.aOff;
  if (a === null) return { ...out, state: archive?.tombstoned ? 'resolving' : 'awaitingBeacon', until: null };
  const close = revealClose(a, clock.window(b));
  const s = seedRound(clock.drand, close, clock.margin);
  const base = { ...out, close, seedRound: s };
  if (now < close && latestRound < s) return { ...base, state: 'revealing', until: close };
  if (!seed && !archive?.seed) return { ...base, state: 'awaitingSeed', until: roundTime(clock.drand.genesis, clock.drand.period, s) };
  return { ...base, state: 'resolving', until: null };
}

/** mm:ss (or h:mm:ss) for a countdown; '0:00' when past. */
export function countdown(secs) {
  const s = Math.max(0, Math.ceil(secs));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  return h ? `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}` : `${m}:${String(r).padStart(2, '0')}`;
}
