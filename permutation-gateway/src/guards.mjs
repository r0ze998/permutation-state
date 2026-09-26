// What keeps a public gateway up (it pays every fee): rate limits per client
// address and per signer, a circuit breaker on the crank's SOL, small caches,
// and the client address itself (behind a reverse proxy on this machine).
import { isIP } from 'node:net';
import { LAMPORTS_PER_SOL } from '@solana/web3.js';
import { RouteError } from './routes/errors.mjs';

/**
 * Token buckets in memory: `take(key, {burst, perSecond})` spends one token
 * of `key`'s bucket (full: `burst`, refilled at `perSecond`) and says
 * whether there was one. Buckets that would be full again are forgotten.
 */
export class RateLimiter {
  constructor({ now = Date.now, maxBuckets = 100_000 } = {}) {
    Object.assign(this, { now, maxBuckets });
    this.buckets = new Map(); // key → {tokens, at, full: ms until full again}
  }

  take(key, { burst, perSecond }) {
    const t = this.now();
    let b = this.buckets.get(key);
    if (b) b.tokens = Math.min(burst, b.tokens + ((t - b.at) / 1000) * perSecond);
    else b = { tokens: burst };
    b.at = t;
    b.full = (burst / perSecond) * 1000;
    if (b.tokens < 1) {
      this.buckets.set(key, b);
      return false;
    }
    b.tokens -= 1;
    this.buckets.set(key, b);
    if (this.buckets.size > this.maxBuckets) this.prune(t);
    return true;
  }

  /** Throw 429 RateLimited unless `key` has a token. */
  check(key, limit, what = 'requests') {
    if (!this.take(key, limit)) throw new RouteError(429, `too many ${what}; slow down`, 'RateLimited');
  }

  prune(t = this.now()) {
    for (const [k, b] of this.buckets) if (t - b.at >= b.full) this.buckets.delete(k);
  }
}

/**
 * Limits per client address on the public listener, by route (a burst,
 * then a steady rate). Browsers poll GET /tick and /usdc and send a few
 * relays per tick; the POSTs that cost the crank SOL are the tightest.
 */
export const IP_LIMITS = Object.freeze({
  'POST /x402/join': { burst: 10, perSecond: 0.2 },
  'POST /relay': { burst: 40, perSecond: 2 },
  'POST /claim-relay': { burst: 6, perSecond: 0.1 },
  'POST /seal': { burst: 40, perSecond: 2 },
  'POST /talk': { burst: 10, perSecond: 0.5 },
  'POST /faucet': { burst: 3, perSecond: 1 / 60 },
  'GET /usdc': { burst: 20, perSecond: 2 },
  'GET /tick': { burst: 30, perSecond: 5 },
  'GET /relay': { burst: 40, perSecond: 3 },
  'GET /claim-relay': { burst: 10, perSecond: 0.5 },
  // Reads that cost an RPC call or a large answer.
  'GET /season': { burst: 20, perSecond: 2 },
  'GET /history': { burst: 10, perSecond: 1 },
  'GET /ticks': { burst: 10, perSecond: 1 },
  'GET /world.bin': { burst: 10, perSecond: 2 },
  'GET /talk': { burst: 20, perSecond: 2 },
  'GET /roster': { burst: 10, perSecond: 1 },
  // A wallet's members of this season and its lineage: one RPC read per wallet (cached 5 s).
  'GET /claims': { burst: 10, perSecond: 1 },
});

/** Limits per signer (session key or wallet) of the co-signing and deposit routes, on every listener. */
export const SIGNER_LIMITS = Object.freeze({
  join: { burst: 4, perSecond: 0.05 },
  relay: { burst: 24, perSecond: 1 },
  claim: { burst: 4, perSecond: 0.05 },
  seal: { burst: 24, perSecond: 1 },
});

/**
 * The crank's SOL, checked at most every `intervalMs` (one read at a time):
 * below `minLamports` everything that makes the crank pay for a member
 * pauses (503 OperatorLowFunds): on either listener the co-signing routes,
 * the faucet and the operator's /submit and /gov, and the crank's own AI
 * registrations (one guard shared by the routes and the crank, so people
 * and the operator's AI members pause together; app.mjs FUNDED_ROUTES). The
 * crank's upkeep of the season goes on. A failed read keeps the last
 * balance (none yet: allowed).
 */
export class FundsGuard {
  constructor({ read, minLamports, intervalMs = 30_000, now = Date.now, log = () => {} }) {
    Object.assign(this, { read, minLamports, intervalMs, now, log });
    this.balance = null;
    this.at = -Infinity;
    this.pending = null;
    this.low = false;
  }

  static forSol({ read, minSol, ...rest }) {
    return new FundsGuard({ read, minLamports: Math.round(minSol * LAMPORTS_PER_SOL), ...rest });
  }

  async refresh() {
    this.pending ??= Promise.resolve().then(this.read).then(b => { this.balance = Number(b); }, () => {}).finally(() => {
      this.at = this.now();
      this.pending = null;
    });
    return this.pending;
  }

  /** Throws 503 OperatorLowFunds while the balance is below the minimum. */
  async check() {
    if (this.now() - this.at >= this.intervalMs) await this.refresh();
    const low = this.balance !== null && this.balance < this.minLamports;
    if (low !== this.low) {
      this.low = low;
      this.log(low ? `WARNING: the crank holds ${this.balance / LAMPORTS_PER_SOL} SOL (< ${this.minLamports / LAMPORTS_PER_SOL}): co-signing, the faucet and AI registrations paused until it is funded`
        : 'the crank is funded again: co-signing, the faucet and AI registrations resumed');
    }
    if (low) throw new RouteError(503, 'the operator is out of funds for fees; try again later', 'OperatorLowFunds');
  }
}

/** Values kept `ttlMs` per key; `get(key, load)` loads a missing or stale one (concurrent loads of a key share one). */
export class TtlCache {
  constructor({ ttlMs, now = Date.now, max = 10_000 }) {
    Object.assign(this, { ttlMs, now, max });
    this.map = new Map();
  }

  async get(key, load) {
    const t = this.now();
    const hit = this.map.get(key);
    if (hit && (hit.pending || t - hit.at < this.ttlMs)) return hit.pending ?? hit.value;
    if (this.map.size >= this.max) for (const [k, v] of this.map) if (!v.pending && t - v.at >= this.ttlMs) this.map.delete(k);
    const entry = { at: t, pending: null, value: undefined };
    entry.pending = Promise.resolve().then(load).then(v => {
      Object.assign(entry, { value: v, at: this.now(), pending: null });
      return v;
    }, e => {
      this.map.delete(key);
      throw e;
    });
    this.map.set(key, entry);
    return entry.pending;
  }

  /** Forget `key` (the next `get` loads it again). */
  delete(key) { this.map.delete(key); }
}

/**
 * How long a member signature that was charged to its signer's bucket is
 * remembered (ms). Far longer than any blockhash lives (about a minute), so
 * a transaction replayed from the chain is answered 409 Duplicate before it
 * can spend the signer's token again; after the window a stale one spends at
 * most one token per window (its simulation then fails on the expired
 * blockhash). Draining a relay bucket (refill 1/s) that way would take more
 * than 1800 of the member's own old transactions per window.
 */
export const REPLAY_TTL_MS = 30 * 60_000;

/**
 * Member signatures already charged (base58/hex strings), each kept
 * `ttlMs`, at most `max` (the oldest forgotten first).
 */
export class ReplayCache {
  constructor({ ttlMs = REPLAY_TTL_MS, max = 200_000, now = Date.now } = {}) {
    Object.assign(this, { ttlMs, max, now });
    this.seen = new Map(); // key → time added (insertion order = age order)
  }

  has(key) {
    const at = this.seen.get(key);
    if (at === undefined) return false;
    if (this.now() - at < this.ttlMs) return true;
    this.seen.delete(key);
    return false;
  }

  add(key) {
    const t = this.now();
    this.seen.delete(key);
    this.seen.set(key, t);
    if (this.seen.size > this.max) {
      for (const [k, at] of this.seen) {
        if (this.seen.size <= this.max && t - at < this.ttlMs) break;
        this.seen.delete(k);
      }
    }
  }

  /** Throw 409 Duplicate if `key` was charged already (within the window). */
  refuseRepeat(key, what = 'transaction') {
    if (this.has(key)) throw new RouteError(409, `this exact ${what} was relayed already; sign a new one (with a fresh blockhash)`, 'Duplicate');
  }
}

const loopback = a => a === '::1' || a.startsWith('127.') || a.startsWith('::ffff:127.');

/**
 * The rate-limit bucket of a client address: an IPv4 address itself (also
 * written IPv4-mapped), an IPv6 address by its /64 (one site or device gets a
 * whole /64, so one bucket per address would be one per request).
 */
export function addressBucket(ip) {
  const a = String(ip).toLowerCase();
  if (isIP(a) !== 6) return a;
  if (a.startsWith('::ffff:') && isIP(a.slice(7)) === 4) return a.slice(7);
  const [head, tail] = a.split('::');
  const groups = s => (s ? s.split(':').flatMap(g => (g.includes('.') ? ['0', '0'] : [g])) : []);
  const h = groups(head);
  const all = tail === undefined ? h : [...h, ...Array(8 - h.length - groups(tail).length).fill('0'), ...groups(tail)];
  return `${all.slice(0, 4).map(g => g.replace(/^0+(?=.)/, '')).join(':')}::/64`;
}

/**
 * The client's address: the socket's peer, or with `trustProxy` and a
 * loopback peer (a reverse proxy on this machine, e.g. the play server's
 * /gw), the last address in X-Forwarded-For (the one that proxy added;
 * earlier ones are the client's own say).
 */
export function clientIp(req, { trustProxy = false } = {}) {
  const peer = req.socket?.remoteAddress ?? 'unknown';
  if (!trustProxy || !loopback(peer)) return peer;
  const header = req.headers?.['x-forwarded-for'];
  const last = String(Array.isArray(header) ? header.at(-1) : header ?? '').split(',').at(-1).trim();
  return isIP(last) ? last : peer;
}
