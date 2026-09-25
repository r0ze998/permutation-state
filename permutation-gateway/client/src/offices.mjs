// Which office gives which order (V5 §5.1), mirroring permutation-rules
// `role_allows_static` / `role_allows` (checked against the Rust vectors in
// test/offices.test.mjs).
import { ROLES } from './codec.mjs';

const [GENERAL, STEWARD, SCIENCE, DIPLOMAT] = ROLES;
const STEWARD_ORDERS = new Set(['FoundCity', 'SetQueue', 'SetFocus', 'Purchase']);
const DIPLOMAT_ORDERS = new Set(['DeclareWar', 'ProposePeace', 'AcceptPeace', 'ProposeNap', 'AcceptNap', 'BreakNap', 'ProposeAlliance',
  'AcceptAlliance', 'LeaveAlliance', 'SendEnvoy', 'Transfer', 'MarketTrade', 'ExchangeOrder', 'OfferContract', 'AcceptContract', 'CancelContract']);

/** Orders whose office depends on the unit they move or rule (settlers: steward, others: general). */
const unitOf = o => (o.type === 'MoveUnit' ? o.unit : o.type === 'SetStanding' && o.target?.kind === 'Unit' ? o.target.id : undefined);

/**
 * Every office that may give `order` whatever the world looks like
 * (`role_allows_static`). Unit orders list both the general and the steward:
 * which one depends on the unit (see `officeOf`).
 */
export function allowedOffices(order) {
  if (unitOf(order) !== undefined) return [GENERAL, STEWARD];
  switch (order.type) {
    case 'Attack': case 'Raze': return [GENERAL];
    case 'SetStanding': return [STEWARD]; // a city's standing rule
    case 'SetResearch': return [SCIENCE];
    case 'ConsentWar': return [GENERAL, STEWARD];
    case 'ConsentSpend': return [GENERAL, STEWARD, SCIENCE];
    case 'RevealRationale': return [...ROLES];
    default:
      if (STEWARD_ORDERS.has(order.type)) return [STEWARD];
      if (DIPLOMAT_ORDERS.has(order.type)) return [DIPLOMAT];
      return [];
  }
}

/**
 * The office an order belongs to (V5 §5.1). Unit orders depend on the unit,
 * looked up in `view`: settlers are the steward's, armies and scouts the
 * general's. `ConsentWar`/`ConsentSpend` are listed under the general (see
 * `submitOffice` for the other offices that may give them).
 * `RevealRationale` belongs to no single office (null): the client adds
 * reveals to the batch of the office that committed.
 */
export function officeOf(order, view) {
  const unit = unitOf(order);
  if (unit !== undefined) return view?.units?.find(u => u.id === unit)?.type === 'Settler' ? STEWARD : GENERAL;
  if (order.type === 'RevealRationale') return null;
  return allowedOffices(order)[0] ?? null;
}

/**
 * The office among `held` whose batch should carry `order`, or null when
 * none may give it: its own office if held, otherwise (consents) another
 * held office the program accepts it from.
 */
export function submitOffice(order, held, view) {
  const own = officeOf(order, view);
  if (own === null) return null;
  if (held.includes(own)) return own;
  if (unitOf(order) !== undefined) return null;
  return allowedOffices(order).find(r => held.includes(r)) ?? null;
}

/**
 * Split orders into one list per office held, and the rest (`notHeld`:
 * orders to propose to their office instead, or unknown ones).
 */
export function splitByOffice(orders, held, view) {
  const byOffice = Object.fromEntries(ROLES.map(r => [r, []]));
  const notHeld = [];
  for (const o of orders) {
    const r = submitOffice(o, held, view);
    if (r) byOffice[r].push(o);
    else notHeld.push(o);
  }
  return { byOffice, notHeld };
}
