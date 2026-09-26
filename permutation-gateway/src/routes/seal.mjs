// Sealed batches handed to the gateway to reveal (commit–reveal, V5 D17).
//
//   POST /seal {civ, role, tick, member, digest, orders, adopt, salt, signature}
//     → {ok, commitment}
//
// An officer that signs its own CommitOrders (a browser, an agent) deposits
// the batch and its salt here first; after the tick's commitments close the
// crank reveals every kept batch whose commitment is the one on chain, as it
// does for the operator's AI members (crank.mjs `revealHosted`), so the
// reveal does not depend on the officer being online in the few seconds of
// the reveal window. The gateway then sees the orders before the deadline
// (disclosed: DESIGN.md, trust model).
//
// `signature` (hex) is the office key's (`nation.keys[role]`, the holder's
// session key) ed25519 signature over player.mjs `sealMessage({seasonId,
// tick, civ, role, commitment})` with commitment = orderCommitment(batch,
// salt), so nobody else can fill or probe an office's deposits. Every check
// is on public data or that signature, in one order, and answers the same
// for every office (AI or not):
//   409 WrongPhase (the season is not running, or the tick is not in its
//       commit phase), 409 WrongTick (not the open tick), 403 NotOfficer
//       (`member` does not hold the office), 400 InvalidSeal (malformed),
//   403 BadSignature, 400 MissingRationale (digest all zero), 400 WrongOffice
//       (an order the office cannot give), 400 BatchTooLarge (orders over
//       BATCH_BYTES encoded, or more adopted proposals than can be open),
//   429 TooManySeals (more than SEALS_PER_TICK by one office key in a tick).
import { Writer } from '../../client/src/borsh.mjs';
import { fromHex, toHex } from '../../client/src/bytes.mjs';
import { BATCH_BYTES, encodeOrder, NOBODY, orderCommitment, roleIndex, ROLES } from '../../client/src/codec.mjs';
import { allowedOffices } from '../../client/src/offices.mjs';
import { sealMessage } from '../../client/src/player.mjs';
import { verifyTalk as verifyEd25519 } from '../../client/src/talk-node.mjs';
import { SIGNER_LIMITS } from '../guards.mjs';
import { RouteError } from './errors.mjs';

/** Seals one office key may deposit per tick. */
export const SEALS_PER_TICK = 16;
/** Proposals open at once (permutation-rules params.rs `max_open_proposals`; the program refuses a batch adopting more). */
export const MAX_OPEN_PROPOSALS = 24;
/** How old the world snapshot the checks use may be (ms). */
export const SEAL_SNAPSHOT_MS = 300;

const hex = (v, len) => typeof v === 'string' && v.length === 2 * len && /^[0-9a-fA-F]*$/.test(v);
const uint = (v, max) => Number.isInteger(v) && v >= 0 && v <= max;

/** Deposits per office key and tick; ticks before the newest two are forgotten. */
export class SealCounter {
  constructor(max = SEALS_PER_TICK) {
    this.max = max;
    this.byTick = new Map(); // tick → Map(signer → n)
  }

  count(tick, signer) { return this.byTick.get(tick)?.get(signer) ?? 0; }
  add(tick, signer) {
    if (!this.byTick.has(tick)) this.byTick.set(tick, new Map());
    this.byTick.get(tick).set(signer, this.count(tick, signer) + 1);
    const newest = Math.max(...this.byTick.keys());
    for (const t of this.byTick.keys()) if (t < newest - 1) this.byTick.delete(t);
  }
}

const invalid = what => new RouteError(400, `seal: ${what}`, 'InvalidSeal');

export const sealRoutes = {
  'POST /seal': async (ctx, req) => {
    const { crank, store, limiter, seals } = ctx;
    const b = await req.json();
    if (!b || typeof b !== 'object') throw invalid('a JSON object');
    if (!uint(b.civ, 0xffff) || !ROLES.includes(b.role) || !uint(b.tick, 0xffff) || !uint(b.member, 0xffffffff)) throw invalid('civ, role (office name), tick and member');
    if (!hex(b.digest, 32) || !hex(b.salt, 32) || !hex(b.signature, 64)) throw invalid('digest and salt (32 bytes hex) and signature (64 bytes hex)');
    const orders = b.orders ?? [];
    const adopt = b.adopt ?? [];
    if (!Array.isArray(orders) || !Array.isArray(adopt) || !adopt.every(id => uint(id, 0xffffffff))) throw invalid('orders (order DTOs) and adopt (proposal ids)');

    const snap = crank.phase === 'playing' ? await crank.fresh(SEAL_SNAPSHOT_MS) : null;
    if (!snap) throw new RouteError(409, 'the season is not running', 'WrongPhase');
    const { meta } = snap.header;
    const h = snap.nations[b.civ];
    if (!h) throw new RouteError(403, 'no such office', 'NotOfficer');
    if (meta.finished || meta.frozen || meta.revealing) throw new RouteError(409, 'the open tick is not in its commit phase', 'WrongPhase');
    if (b.tick !== h.openTick) throw new RouteError(409, `tick ${b.tick} is not the open tick ${h.openTick}`, 'WrongTick');
    const i = roleIndex(b.role);
    if (h.officers[i] === NOBODY || h.officers[i] !== b.member) throw new RouteError(403, `member ${b.member} does not hold that office`, 'NotOfficer');

    const batch = { civ: b.civ, tick: b.tick, role: b.role, member: b.member, decisionDigest: fromHex(b.digest), orders, adopt };
    let used = 0;
    try {
      for (const o of orders) used += encodeOrder(new Writer(), o).toBytes().length;
    } catch (e) {
      throw invalid(`orders: ${e.message}`);
    }
    const salt = fromHex(b.salt);
    const commitment = orderCommitment(batch, salt);
    const seasonId = BigInt(store.state.seasonId);
    if (!verifyEd25519(sealMessage({ seasonId, tick: b.tick, civ: b.civ, role: b.role, commitment }), fromHex(b.signature), h.keys[i])) {
      throw new RouteError(403, 'the signature is not the office key\'s over this seal', 'BadSignature');
    }
    const signer = toHex(h.keys[i]);
    limiter.check(`seal:${signer}`, SIGNER_LIMITS.seal, 'seals for this key');
    if (batch.decisionDigest.every(x => x === 0)) throw new RouteError(400, 'digest: not all zero (officers seal a rationale)', 'MissingRationale');
    const wrong = orders.find(o => !allowedOffices(o).includes(b.role));
    if (wrong) throw new RouteError(400, `${wrong.type} is not an order of the ${b.role}`, 'WrongOffice');
    if (used > BATCH_BYTES) throw new RouteError(400, `orders take ${used} bytes encoded, at most ${BATCH_BYTES}`, 'BatchTooLarge');
    if (adopt.length > MAX_OPEN_PROPOSALS) throw new RouteError(400, `at most ${MAX_OPEN_PROPOSALS} adopted proposals`, 'BatchTooLarge');
    if (seals.count(b.tick, signer) >= seals.max) throw new RouteError(429, `at most ${seals.max} seals per office key and tick`, 'TooManySeals');
    seals.add(b.tick, signer);
    // Kept (on disk) for the crank's reveal after the tick's commitments close.
    crank.sealed.put(batch, salt, commitment);
    return { body: { ok: true, commitment: toHex(commitment) } };
  },
};
