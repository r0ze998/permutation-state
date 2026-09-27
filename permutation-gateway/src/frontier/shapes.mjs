// What the relay adds to the SDK's shape allowlist (sdk frontier/shapes.mjs)
// before it signs anything (contract §8.3, I-51):
//
// * the fee payer is one of the relay pool's keys, and the instruction is
//   for this relay's program and season;
// * Depart's tip is one of the three presets {tip_min, ⌈1.5 tip_min⌉, 2 tip_min}
//   (else 400 TipNotPreset);
// * the drain guard: simulated with signatures, the fee payer may lose at most
//   the fee plus the kind's allowance — Join: rent(Citizen); FileTicket: the
//   Holding-rent escrow shortfall `max(0, rent(1,280) − citizen.ticket_escrow)`;
//   Depart: `tip + march_fee + seal_bond`; every other kind: 0;
// * who is charged: a player shape its citizen, a settle shape its requester
//   (a session key that signed the request, else the client-address bucket),
//   never the citizen the settle names.
import { addressBucket } from '../guards.mjs';
import { baseFee, departEscrow, rent, seasonTipMin, tipPresets } from '../../client/src/frontier/fees.mjs';
import { layoutOf } from '../../client/src/frontier/codec.mjs';
import { DEPART_TAG, FILE_TICKET_TAG, JOIN_TAG } from '../../client/src/frontier/shapes.mjs';
import { RouteError } from '../routes/errors.mjs';

export const CITIZEN_RENT = rent(layoutOf('Citizen').size);
export const HOLDING_RENT = rent(layoutOf('Holding').size);

/** Refuse unless `shape` (classified) may be sponsored by this relay for this season. */
export function checkRelayShape(shape, { pool, addresses }) {
  if (!pool.has(shape.feePayer)) throw new RouteError(400, 'relay refused: the fee payer is not one of this relay\'s (GET /f/relay names one)', 'RelayRejected');
  if (shape.accounts.season !== addresses.season) throw new RouteError(400, 'relay refused: not this season\'s instruction', 'RelayRejected');
}

/** The Depart tip must be a preset; returns the tip (bigint). */
export function checkDepartTip(shape, season) {
  const tip = BigInt(shape.data.tip);
  const presets = tipPresets(seasonTipMin(season));
  if (!presets.includes(tip)) {
    throw new RouteError(400, `a sponsored Depart tips one of ${presets.join(', ')} lamports`, 'TipNotPreset', { presets: presets.map(String) });
  }
  return tip;
}

/**
 * Lamports (bigint) the fee payer may move for `shape` beyond the fee:
 * `{season, citizen}` are the decoded Season and (FileTicket) Citizen.
 */
export function allowanceFor(shape, { season, citizen = null }) {
  switch (shape.tag) {
    case JOIN_TAG: return CITIZEN_RENT;
    case FILE_TICKET_TAG: {
      const escrow = citizen ? BigInt(citizen.TICKET_ESCROW) : 0n;
      return escrow >= HOLDING_RENT ? 0n : HOLDING_RENT - escrow;
    }
    case DEPART_TAG: return departEscrow(season, checkDepartTip(shape, season));
    default: return 0n;
  }
}

/** The fee payer's cost of the transaction itself (base fee per signature; sponsored shapes pay no priority fee). */
export const feeOf = shape => baseFee(shape.signers.length);

/**
 * The drain guard: refuse unless `pre − post ≤ fee + allowance`. Returns
 * the lamports moved beyond the fee (what the lamport quota is charged).
 */
export function drainGuard({ pre, post, fee, allowance }) {
  if (post === null || post === undefined) throw new RouteError(502, 'the simulation did not report the fee payer\'s balance', 'SimulationIncomplete');
  const delta = BigInt(pre) - BigInt(post);
  if (delta > BigInt(fee) + BigInt(allowance)) {
    throw new RouteError(400, `relay refused: the fee payer would lose ${delta} lamports, more than the fee and the ${allowance} this kind may move`, 'RelayRejected',
      { drain: { delta: String(delta), allowed: String(BigInt(fee) + BigInt(allowance)) } });
  }
  return delta > BigInt(fee) ? delta - BigInt(fee) : 0n;
}

/** Sponsored lamports a key may move per game day: 24 Depart escrows at the top preset + the Citizen's and the Holding's rent. */
export function lamportsPerDay(season, departs = 24) {
  const top = tipPresets(seasonTipMin(season))[2];
  return BigInt(departs) * departEscrow(season, top) + CITIZEN_RENT + HOLDING_RENT;
}

/** The quota key a shape is charged to. */
export function quotaKeyOf(shape, { requester = null, ip }) {
  if (shape.kind === 'player') return `citizen:${shape.accounts.citizen}`;
  return requester ? `session:${requester}` : `addr:${addressBucket(ip)}`;
}
