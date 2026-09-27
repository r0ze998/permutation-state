// Packing one office's batch for a RevealOrders transaction: its own orders
// and as many reveals of the office's earlier decisions as fit (pure),
// within the program's batch caps (`permutation_rules::orders`).
import { Writer } from './borsh.mjs';
import { BATCH_BYTES, encodeOrder, isFree, MAX_BATCH_ORDERS, MAX_FREE_ORDERS } from './codec.mjs';
import { revealOrder } from './decision.mjs';

/** Encoded size of one order (borsh). */
export const encodedLen = o => encodeOrder(new Writer(), o).toBytes().length;

/** Reveals per batch at most. */
export const MAX_REVEALS = 3;

/**
 * @param {object} o
 * @param {object[]} o.orders    the office's own orders (always sent, never trimmed)
 * @param {object[]} [o.pending] commitments of that office not yet revealed ({tick, policy, salt, text})
 * @param {object[]} [o.adopted] the orders of the proposals the batch adopts, flattened: they run in the
 *                               batch and count towards its caps (an adoption over them is skipped)
 * @returns {{orders: object[], reveals: object[], used: number, fits: boolean, reason: null|'bytes'|'free'|'orders'}}
 *   `orders` = own orders then the reveal orders; `reveals` = the commitments
 *   revealed (oldest first); `fits` is false when the own orders alone
 *   exceed `maxBytes` (`reason: 'bytes'`), more than MAX_FREE_ORDERS are
 *   free (`'free'`), or they and the adopted ones exceed MAX_BATCH_ORDERS
 *   (`'orders'`). Reveals are free orders: they are only added within the caps.
 */
export function packBatch({ orders, pending = [], adopted = [], maxBytes = BATCH_BYTES, maxReveals = MAX_REVEALS }) {
  let used = orders.reduce((n, o) => n + encodedLen(o), 0);
  const free = [...orders, ...adopted].filter(isFree).length;
  const count = orders.length + adopted.length;
  const reveals = [];
  for (const d of [...pending].sort((a, b) => a.tick - b.tick).slice(0, maxReveals)) {
    const n = encodedLen(revealOrder(d));
    if (used + n > maxBytes || free + reveals.length >= MAX_FREE_ORDERS || count + reveals.length >= MAX_BATCH_ORDERS) break;
    used += n;
    reveals.push(d);
  }
  const reason = used > maxBytes ? 'bytes' : free + reveals.length > MAX_FREE_ORDERS ? 'free' : count + reveals.length > MAX_BATCH_ORDERS ? 'orders' : null;
  return { orders: [...orders, ...reveals.map(revealOrder)], reveals, used, fits: reason === null, reason };
}
