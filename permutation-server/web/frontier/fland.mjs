// Land and holdings as the page shows them (contract §5.3, §5.9, §5.10,
// I-29, I-47, I-56; web design §7.2, §7.4, §7.5): the faction picker's
// numbers, the site picker (free sites of the home wedge in rings ≥ 2), the
// ticket card (escrow, expected result, cohort finality, refile after a
// displacement), the holding panel (stores settled lazily as the kernel
// does, queue, reserve, transits, dormancy), the hosts of a holding, and
// what each resident action needs before it is sent. Pure functions over
// decoded accounts; the program decides every action.
import { rent } from '../sdk/frontier/fees.mjs';
import { ringOf, wedgeOf, provinceIndex, TILE_OFFSETS, DIRECTIONS } from './fgeo.mjs';
import { ACCOUNTS } from './abi.mjs';
import { hostParts } from './faddr.mjs';
import { bellStart, bellEnd } from './clock.mjs';
import { hostInTransit, staminaAt, troopsOf } from './fmarch.mjs';

export const NO_TICKET = 0xffffffff;
/** Citizen flags (§5.3). */
export const CITIZEN = Object.freeze({ JOINED: 1, FIRST_FINAL: 4, PROVISIONAL: 8, REFUGEE: 16 });
/** Holding states (§5.3). */
export const HOLDING_STATE = Object.freeze(['none', 'provisional', 'final', 'released']);
/** Site mirror states (§5.3): 0 free, 1 holding, 2 unused, 3 released-free, 4 reserved (rings 0–1). */
export const SITE = Object.freeze({ FREE: 0, HOLDING: 1, RELEASED: 3, RESERVED: 4 });
/** First ring with ticketable sites (I-30: rings 0–1 are reserved). */
export const FIRST_TICKET_RING = 2;
/** Units in kernel order; a Settler never trains nor forms a host, a Scout explores. */
export const UNIT_ORDER = Object.freeze(['Spearman', 'Archer', 'Horseman', 'Pikeman', 'Crossbowman', 'Knight', 'Scout', 'Settler']);
export const SCOUT = 6;
export const SETTLER = 7;
/**
 * Build items (catalog `BUILDINGS` then walls, item ids 0–6): the resource
 * each building produces, its per-hour rate and the first copy's cost (later
 * copies cost more: the program's `duplicate_cost`). Pinned against
 * permutation-rules `catalog.rs` by web-frontier-march.test.mjs.
 */
export const BUILD_ITEMS = Object.freeze([
  { item: 0, resource: 'Food', perHour: 12, cost: [0, 80, 40, 0, 0, 0, 0, 0] },
  { item: 1, resource: 'Wood', perHour: 10, cost: [40, 0, 40, 0, 0, 0, 0, 0] },
  { item: 2, resource: 'Stone', perHour: 8, cost: [40, 80, 0, 0, 0, 0, 0, 0] },
  { item: 3, resource: 'Ore', perHour: 6, cost: [40, 80, 20, 0, 0, 0, 0, 0] },
  { item: 4, resource: 'Gold', perHour: 6, cost: [40, 60, 40, 0, 0, 0, 0, 0] },
  { item: 5, resource: 'Science', perHour: 3, cost: [0, 60, 60, 0, 0, 20, 0, 0] },
  { item: 6, resource: 'Walls', perHour: 0, cost: null },
].map(Object.freeze));
/**
 * Base production per hour by resource (catalog `BASE_PROD`) and the tier
 * bonus in percent (catalog `tier_bonus_pct`, Hamlet … Stronghold): a
 * holding is founded with `production = base_production(Hamlet)`, so only
 * production above the tier's base comes from a building (W6-D: the
 * onboarding card counted the base as "built"). Pinned against catalog.rs
 * by web-frontier-playtest.test.mjs.
 */
export const BASE_PROD = Object.freeze([40, 30, 20, 15, 0, 15, 5, 0]);
export const TIER_BONUS_PCT = Object.freeze([0, 50, 100, 175]);
/** `catalog::base_production(tier)`, milli-units per hour (BigInt). */
export const baseProduction = tier => BASE_PROD.map(b => (BigInt(b) * BigInt(100 + (TIER_BONUS_PCT[tier] ?? 0)) / 100n) * 1000n);

// ------------------------------------------------------------------ factions and free land
/** A faction's home wedge (the simulator's and the program's rule: wedge = faction id). */
export const homeWedge = faction => faction;

/**
 * Free sites per wedge from the ring overviews (the herald's §9.3 records):
 * an upper bound (the overview cannot tell a missing site from a free one);
 * the site picker reads the Province itself before a ticket is filed.
 */
export function freeByWedge(overviews) {
  const out = [0, 0, 0, 0, 0, 0];
  for (const ov of overviews.values()) {
    if (!ov?.provinces || ov.ring < FIRST_TICKET_RING) continue;
    for (const pr of ov.provinces) {
      const w = wedgeOf(pr.p, pr.q);
      if (w === null) continue;
      out[w] += pr.sites.filter((s, i) => (s === 0 || s === 3) && pr.owners[i] === 7).length;
    }
  }
  return out;
}

/** Provinces of the home wedge worth opening in the site picker (open rings ≥ 2, some free site), nearest ring first. */
export function candidateProvinces(overviews, faction) {
  const w = homeWedge(faction), out = [];
  for (const ov of overviews.values()) {
    if (!ov?.provinces || ov.ring < FIRST_TICKET_RING) continue;
    for (const pr of ov.provinces) {
      if (wedgeOf(pr.p, pr.q) !== w) continue;
      const free = pr.sites.filter((s, i) => (s === 0 || s === 3) && pr.owners[i] === 7).length;
      if (free > 0) out.push({ p: pr.p, q: pr.q, ring: ringOf(pr.p, pr.q), free });
    }
  }
  return out.sort((a, b) => a.ring - b.ring || provinceIndex(a.p, a.q) - provinceIndex(b.p, b.q));
}

/** The free sites of one decoded Province: `[{p, q, site, tile}]` (sites beyond `siteCount` do not exist). */
export function freeSitesOf(province) {
  if (ringOf(province.p, province.q) < FIRST_TICKET_RING) return [];
  const out = [];
  for (let i = 0; i < province.siteCount; i++) {
    const st = province.siteMirror[i].state;
    if (st === SITE.FREE || st === SITE.RELEASED) out.push({ p: province.p, q: province.q, site: i, tile: province.sites[i] });
  }
  return out;
}

/** Whether a site may go on a ticket of `faction` (the program checks the overflow rule for a full wedge). */
export const siteAllowed = (faction, s) => ringOf(s.p, s.q) >= FIRST_TICKET_RING && wedgeOf(s.p, s.q) === homeWedge(faction) && s.site >= 0 && s.site < 12;

// ------------------------------------------------------------------ the citizen and its ticket
const isZeroRef = r => !r || (r.p === 0 && r.q === 0 && r.site === 0 && r.gen === 0);

/**
 * The land state of a Citizen: `{stage, ticket, holding, escrow, escrowNeeded}`:
 *   stage 'none' (no Citizen) | 'joined' (no ticket, no holding) | 'ticket'
 *   (open ticket) | 'provisional' | 'final' | 'refugee'
 * `escrowNeeded`: lamports the next FileTicket moves into the Citizen
 * (rent of a Holding minus the escrow already held; refunded to its funder).
 */
export function landState(citizen) {
  const holdingRent = rent(ACCOUNTS.Holding.size);
  if (!citizen) return { stage: 'none', ticket: null, holding: null, escrow: 0n, escrowNeeded: holdingRent };
  const escrow = BigInt(citizen.ticketEscrow);
  const escrowNeeded = escrow >= holdingRent ? 0n : holdingRent - escrow;
  const ref = citizen.holding?.[0];
  const holding = isZeroRef(ref) ? null : { p: ref.p, q: ref.q, site: ref.site, gen: ref.gen };
  const open = citizen.ticketBell !== NO_TICKET;
  const ticket = open ? { bell: citizen.ticketBell, next: citizen.ticketNext, sites: citizen.ticketSites.filter(s => !(s.p === 0 && s.q === 0 && s.site === 0)).map(({ p, q, site }) => ({ p, q, site })) } : null;
  let stage = 'joined';
  if (holding && (citizen.flags & CITIZEN.FIRST_FINAL)) stage = 'final';
  else if (holding && (citizen.flags & CITIZEN.PROVISIONAL)) stage = 'provisional';
  else if (open) stage = 'ticket';
  else if (citizen.flags & CITIZEN.REFUGEE) stage = 'refugee';
  return { stage, ticket, holding, escrow, escrowNeeded };
}

/**
 * The ticket card's times: the result is drawn from the seed of the ticket's
 * bell (S ≈ its end + W + Δ, rounded to a beacon round) — about 11–21 minutes
 * after filing (I-33) — and a provisional holding is final once its cohort
 * closes, at the latest `ticket_bell + 24` bells (I-47).
 */
export function ticketTimes(clock, ticketBell) {
  const genesis = clock.genesisTs;
  return {
    resultAbout: bellEnd(genesis, ticketBell) + clock.window(ticketBell) + clock.margin,
    cohortEndsBy: bellStart(genesis, ticketBell + 24),
  };
}

/**
 * Bells after the filing bell from which the last ticket counts as seen
 * without the page having watched it: the result is drawn at
 * `bellEnd(ticket_bell) + W + margin` ≈ 2.1 bells after the filing bell
 * (`ticketTimes`), and a ticket lasts 11–21 game minutes (O-M1-13).
 */
export const TICKET_SEEN_BELLS = 3;

// ------------------------------------------------------------------ the holding
const HOUR = 3600;
/** Accrual value at t (host `Accrual::value_at`: rate per hour, cap for positive rates, floored at 0). */
export function accrualAt(a, t) {
  const t0 = BigInt(a.t0), tt = BigInt(Math.floor(t));
  if (tt <= t0) return BigInt(a.value);
  const num = BigInt(a.frac) + BigInt(a.rate) * (tt - t0);
  const q = num / BigInt(HOUR), gain = num < 0n && num % BigInt(HOUR) !== 0n ? q - 1n : q;
  const v = BigInt(a.value) + gain;
  if (BigInt(a.rate) >= 0n) return BigInt(a.value) < BigInt(a.cap) ? (v < BigInt(a.cap) ? v : BigInt(a.cap)) : BigInt(a.value);
  return v < 0n ? 0n : v;
}

/** Stores in whole units at chain time `t`: `[{resource, value, perHour, cap}]` (stores hold milli-units). */
export function storesAt(holding, t, resources) {
  return holding.stores.map((a, i) => ({
    resource: resources[i], value: Number(accrualAt(a, t) / 1000n), perHour: Number(BigInt(a.rate)) / 1000, cap: Number(BigInt(a.cap) / 1000n),
  }));
}

/**
 * The holding panel's facts at chain time `t`: `{state, final, shieldLeft,
 * dormantIn, releaseIn, queue, reserve, transits}`.
 */
export function holdingFacts(holding, season, t) {
  const last = Number(holding.lastOwnerAction);
  return {
    state: HOLDING_STATE[holding.state] ?? 'none',
    final: holding.state === 2,
    finalTs: Number(holding.finalTs),
    shieldLeft: Math.max(0, Number(holding.shieldUntil) - t),
    dormantIn: last + Number(season.dormantAfterSecs) - t,
    releaseIn: last + Number(season.releaseAfterSecs) - t,
    queue: holding.queue.filter(x => Number(x.doneAt) > 0).map(x => ({ kind: x.kind, arg: x.arg, doneIn: Number(x.doneAt) - t })),
    reserve: holding.reserve.map((n, unit) => ({ unit, troops: n })).filter(x => x.troops > 0),
    transits: holding.transit.map((tr, slot) => ({ slot, ...tr })).filter(tr => tr.state !== 0),
  };
}

/** Whether a host id belongs to a holding (same province, site and generation: a newer `gen` makes it stranded). */
export function hostOfHolding(id, holding) {
  const h = hostParts(id);
  return !!h && h.p === holding.p && h.q === holding.q && h.site === holding.site && h.gen === holding.gen;
}

/**
 * The hosts of a holding in one decoded Province (its entries in states
 * 1–3): `[{id, unit, tile, state, troops, stamina, readyBell, pending,
 * inTransit}]` at bell `nowBell`.
 */
export function hostsIn(province, holding, nowBell) {
  return province.entries
    .filter(e => e.state >= 1 && e.state <= 3 && hostOfHolding(e.id, holding))
    .map(e => ({ id: e.id, unit: e.unit, tile: e.tile, state: e.state, troops: troopsOf(e.troops), stamina: staminaAt(e, nowBell), readyBell: e.readyBell,
      pending: e.pendOp, fromBell: e.fromBell, inTransit: hostInTransit(holding, e.id), entry: e }));
}

// ------------------------------------------------------------------ resident actions (I-29, I-44)
/**
 * What blocks an action now (`[]` = send it): program error names the
 * program would answer. `ctx = {holding, citizen, province (decoded or
 * null), nowBell, host (hostsIn row)?}`.
 */
export function actionBlocks(name, ctx) {
  const out = [];
  const resident = ['Muster', 'Dissolve', 'Garrison', 'Explore', 'Depart'].includes(name);
  if (!ctx.holding) return ['NoTicket'];
  if (resident && !isFinal(ctx.holding, ctx.province, ctx.now ?? 0, ctx.nowBell ?? 0)) out.push('NotFinal');
  if (resident && (!ctx.province || ctx.province.resolvedNext + 1 < ctx.nowBell)) out.push('NotResident');
  if (['Dissolve', 'Explore', 'Depart'].includes(name)) {
    const h = ctx.host;
    if (!h || h.state !== 1) out.push('HostBusy');
    else {
      if (h.pending !== 0) out.push('HostBusy');
      if (h.inTransit) out.push('HostInTransit');
    }
  }
  if (name === 'Explore' && ctx.host && ctx.host.unit !== SCOUT) out.push('NotScout');
  if (name === 'Explore' && ctx.holding.explore?.state) out.push('Explored');
  return out;
}

const TILE_AT = new Map(TILE_OFFSETS.map((o, i) => [`${o.q},${o.r}`, i]));
/** Tile indices one hex away from tile `idx` inside the same province. */
export const tileNeighbours = idx => {
  const o = TILE_OFFSETS[idx];
  return o ? DIRECTIONS.map(([dq, dr]) => TILE_AT.get(`${o.q + dq},${o.r + dr}`)).filter(i => i !== undefined) : [];
};

// ------------------------------------------------------------------ lazy finality (§5.6 step 5, I-29, I-47)
/** Bells after which a ticket cohort counts as closed whatever it holds (Province `COHORT_BELLS`). */
export const COHORT_BELLS = 24;
/** frontier-abi `prologue::cohort_closed`: no open record of `ticketBell` in the Province (or 24 bells passed). */
export function cohortClosed(province, ticketBell, nowBell) {
  if (nowBell >= ticketBell + COHORT_BELLS) return true;
  return (province?.ticketCohorts ?? []).every(c => c.bell !== ticketBell || c.filed === 0 || c.settled >= c.filed);
}
/**
 * frontier-abi `prologue::finality_due`: the provisional → final flip the
 * program applies inside the next resident action that carries the
 * holding's own Province (state 1, `now ≥ final_ts`, cohort closed). W6-D:
 * the page used to wait for state 2, which no instruction writes before
 * such an action, so Muster, Garrison, Explore and Depart stayed disabled
 * on a holding the program would take as final.
 */
export function finalityDue(holding, province, now, nowBell) {
  if (!holding || holding.state !== 1 || !province) return false;
  if (province.p !== holding.p || province.q !== holding.q) return false;
  return now >= Number(holding.finalTs) && cohortClosed(province, holding.ticketBell, nowBell);
}
/** Final for a resident action in `province`: state 2, or the lazy flip is due. */
export const isFinal = (holding, province, now, nowBell) => holding?.state === 2 || finalityDue(holding, province, now, nowBell);

/**
 * Explore targets of a Scout host: the passable tiles within one hex of its
 * tile, not yet explored in the Province (`explored_mask`). `neighbours(tile)`
 * gives the tile indices one hex away (fgeo-based, injected for tests).
 */
export function exploreTargets(province, host, neighbours = tileNeighbours) {
  const mask = BigInt(province.exploredMask), pass = BigInt(province.passableMask);
  return neighbours(host.tile).filter(i => ((pass >> BigInt(i)) & 1n) && !((mask >> BigInt(i)) & 1n));
}
