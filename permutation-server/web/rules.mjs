// Rule numbers the UI shows or uses. Prefer what the server sends in the view
// (season.*, gov.*, member.*); the constants below are the ones the view does
// not carry yet and must be kept in step with permutation-rules by hand.
// Each is a candidate for a server field (see the refactor notes).
import { TECH } from './i18n.mjs';

// ---------------------------------------------------------------- from the server
// Live bindings: `adoptRules` replaces the defaults with the numbers the
// server sends (`season.rules` in /api/state, and the lobby's fields), so
// every module that imports them shows the engine's own values. The
// defaults only cover the first render before any answer.
export let NAP_BOND = 30;                   // gold each side stakes on a non-aggression pact (we offer the minimum)
export let NAP_TICKS = 30;                  // length of a non-aggression pact
export let SUZERAIN_MIN = 60;               // influence needed to become suzerain
export let SUZERAIN_REVIEW = 45;            // ticks between suzerain reviews (influence halves)
export let EXCHANGE_FEE = 0.05;             // buyer's fee on the USDC exchange
export let AMM_FEE_TEXT = '3%：交易拠点の保有者に1%、2%は消滅';
export let MAX_OFFICES = 2;                 // offices one member may hold
export let BANK_TICKS = 4;                  // unused office budget carried, per office
export let BUDGET_TEXT = '3＋都市数（最大8）'; // national order budget
export let CASUS_BELLI = 30;                // grievance that justifies a war
export let PROPOSAL_TTL = 6, TRUCE_TICKS = 12, LEAVE_ALLIANCE_TICKS = 6;
export let RECALL_ACTIVE_TICKS = 10, RECALL_TICKS = 5;
/** Number of techs in the tree. */
export let TECH_COUNT = Object.keys(TECH).length;

/** Operations share from the lobby, before a season view exists. */
let lobbyOpsBps = 2000;

const pct = bps => `${+(bps / 100).toFixed(2)}%`;

/** Take the rule numbers from a view (`/api/state`) or the lobby (`/api/lobby`). */
export function adoptRules(v) {
  const r = v?.season?.rules;
  if (r) {
    NAP_BOND = r.napMinBond; NAP_TICKS = r.napTicks;
    SUZERAIN_MIN = r.suzerainThreshold; SUZERAIN_REVIEW = r.suzerainLockTicks;
    EXCHANGE_FEE = r.exchangeFeeBps / 10000;
    AMM_FEE_TEXT = `${pct(r.ammFeeBps)}：交易拠点の保有者に${pct(r.hubFeeBps)}、${pct(r.ammFeeBps - r.hubFeeBps)}は消滅`;
    MAX_OFFICES = r.maxOfficesPerMember; BANK_TICKS = r.bankTicks;
    BUDGET_TEXT = `${r.budgetBase}＋都市数（最大${r.budgetCap}）`;
    CASUS_BELLI = r.casusBelliThreshold;
    PROPOSAL_TTL = r.proposalTtl; TRUCE_TICKS = r.truceTicks; LEAVE_ALLIANCE_TICKS = r.allianceLeaveDelay;
    RECALL_ACTIVE_TICKS = r.recallElectorateTicks; RECALL_TICKS = r.recallTicks;
    TECH_COUNT = r.techCount;
  }
  if (v?.maxOfficesPerMember) MAX_OFFICES = v.maxOfficesPerMember;
  if (v?.opsShareBps != null) lobbyOpsBps = v.opsShareBps;
}

// ---------------------------------------------------------------- not sent by the server
// These are inline constants in permutation-rules (or client choices), so
// they must be kept in step by hand.
export const PURCHASE_GOLD = 60;            // gold of a one-click production purchase
export const ENVOY_AMOUNTS = [10, 20, 40];  // influence per envoy button
export const AUTO_PURCHASE_STEPS = [0, 20, 40, 80];
export const QUEUE_MAX = 3;                 // production queue / research plan length
export const PATROL_MAX = 6;
export const AUTO_COMMIT_SECONDS = 3;       // a dirty draft is committed this close to the deadline
/** Paths that must reach a tier to enter the next era (the last era needs three). */
export const eraPathsNeeded = era => (era === 5 ? 3 : 2);
/** Yes-votes that recall an officer: a majority of the active electorate. */
export const recallNeeded = electorate => Math.floor(electorate / 2) + 1;

// ---------------------------------------------------------------- from the view
const season = v => v?.season || {};
/** Tick from which the USDC exchange is frozen. */
export const marketFreeze = v => season(v).marketFreeze ?? 120;
/** Share of fees and entry fees that goes to the prize pool, in percent. */
export const poolSharePct = v => 100 - (season(v).opsShareBps ?? lobbyOpsBps) / 100;
export const opsSharePct = v => (season(v).opsShareBps ?? lobbyOpsBps) / 100;
/** Share of a nation's payout split equally among active members, in percent. */
export const equalSharePct = v => (season(v).equalShareBps ?? 2000) / 100;
/** Activity windows in the season (member.windows), for "n / N 区間". */
export const activityWindows = v => v?.member?.windows ?? (season(v).activityWindow ? Math.ceil((v.ticks ?? 0) / season(v).activityWindow) : 18);
