// The Frontier relay's HTTP application (contract §8.3): routes over one
// context, served on two loopback listeners, as the v9 gateway's app.mjs.
//
// * Operator listener (41030): every route; the operator routes need the
//   operator token.
// * Public listener (41033, 127.0.0.1 only): FRONTIER_PUBLIC_ROUTES; the
//   herald proxies them as `/gw/*`. Per-address rate limits (FRONTIER_IP_LIMITS);
//   loopback clients (bots) are exempt from those, never from the quotas.
//
// The client address (I-51): the TCP peer, unless the peer is the herald's
// loopback address (`heraldPeer`), whose request carries the client in the
// last X-Forwarded-For entry (the one the herald added). Nobody else's
// X-Forwarded-For is read.
//
// A route is `async (ctx, req) => ({status = 200, body})` where `req` is
// `{method, url, headers, surface, ip, json()}`; it throws a RouteError to
// fail. `createFrontierHandler` returns a plain Node request handler, so it
// is testable without a socket.
import { isIP } from 'node:net';
import { toJson } from '../../client/src/bytes.mjs';
import { FrontierAddresses } from '../../client/src/frontier/addresses.mjs';
import { addressBucket, FundsGuard, RateLimiter, ReplayCache } from '../guards.mjs';
import { errorResponse, RouteError } from '../routes/errors.mjs';
import { BlockhashBook } from '../send.mjs';
import { ChainView } from './chain.mjs';
import { QuotaBook } from './quota.mjs';
import { keeperRoutes } from './routes/keeper.mjs';
import { operatorRoutes } from './routes/operator.mjs';
import { relayRoutes } from './routes/relay.mjs';
import { seasonRoutes } from './routes/season.mjs';

export const FRONTIER_ROUTES = Object.freeze({ ...seasonRoutes, ...relayRoutes, ...keeperRoutes, ...operatorRoutes });

/** What the public listener serves ('GET /f/tx/' is a prefix: /f/tx/{signature}). */
export const FRONTIER_PUBLIC_ROUTES = Object.freeze([
  'GET /f/season', 'GET /f/relay', 'GET /f/quota', 'GET /f/tx/',
  'POST /f/relay', 'POST /f/join', 'POST /f/reveal', 'POST /f/nudge',
]);

/** Routes that make the relay pool pay: paused (503 OperatorLowFunds) while the pool is below its minimum. */
export const FRONTIER_FUNDED_ROUTES = Object.freeze(['POST /f/relay', 'POST /f/join']);

/** Per client address on the public listener (burst, then a steady rate; §8.3). */
export const FRONTIER_IP_LIMITS = Object.freeze({
  'POST /f/relay': { burst: 40, perSecond: 2 },
  'POST /f/join': { burst: 10, perSecond: 0.2 },
  'POST /f/reveal': { burst: 40, perSecond: 2 },
  'POST /f/nudge': { burst: 10, perSecond: 1 },
  'GET /f/relay': { burst: 40, perSecond: 3 },
  'GET /f/season': { burst: 20, perSecond: 2 },
  'GET /f/quota': { burst: 20, perSecond: 2 },
  'GET /f/tx/': { burst: 40, perSecond: 4 },
});

export const FRONTIER_MAX_BODY_BYTES = 64 << 10;

const unmap = a => (typeof a === 'string' && a.startsWith('::ffff:') ? a.slice(7) : a);
export const isLoopback = a => { const x = unmap(String(a)); return x === '::1' || x.startsWith('127.'); };

/**
 * The client's address (I-51): the TCP peer, or — only when the peer is the
 * herald (`heraldPeer`) — the last X-Forwarded-For entry.
 */
export function clientAddress(req, { heraldPeer }) {
  const peer = req.socket?.remoteAddress ?? 'unknown';
  if (unmap(peer) !== unmap(heraldPeer)) return peer;
  const header = req.headers?.['x-forwarded-for'];
  if (header === undefined) return peer;
  const last = String(Array.isArray(header) ? header.at(-1) : header).split(',').at(-1).trim();
  return isIP(last) ? last : peer;
}

function readJson(req, limit) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', c => {
      if (size > limit) return;
      size += c.length;
      if (size > limit) { reject(new RouteError(413, 'request body too large', 'BodyTooLarge')); return; }
      chunks.push(c);
    });
    req.on('error', reject);
    req.on('end', () => {
      const text = Buffer.concat(chunks).toString('utf8');
      try {
        const v = text ? JSON.parse(text) : {};
        if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('not an object');
        resolve(v);
      } catch { reject(new RouteError(400, 'the request body is not a JSON object', 'InvalidJson')); }
    });
  });
}

/**
 * The routes' shared context (both listeners share its limits, caches,
 * quota book and pool).
 * @param {object} o
 * @param {object} o.cfg         loadFrontierConfig()
 * @param {object} o.connection  web3.js Connection to the chain (localnet)
 * @param {object} o.pool        PayerPool (relay)
 * @param {object} [o.keeper]    KeeperLink
 * @param {object} [o.invites]   InviteBook (gated seasons)
 * @param {object} [o.gateKey]   the join-gate Keypair (gated seasons)
 * @param {object} [o.store]     `{state, save()}` for quotas and used invites
 */
export function createFrontierContext({ cfg, connection, pool, keeper = null, invites = null, gateKey = null, store = { state: {}, save() {} }, now = Date.now,
  log = console.log, limiter, funds, blockhashes }) {
  const addresses = new FrontierAddresses({ programId: cfg.programId, seasonId: BigInt(cfg.seasonId) });
  return {
    cfg, connection, pool, keeper, invites, gateKey, log, now, addresses,
    programId: addresses.programId,
    chain: new ChainView({ connection, addresses, now }),
    quota: new QuotaBook({ store }),
    limiter: limiter ?? new RateLimiter({ now }),
    relayed: new ReplayCache({ now }),
    blockhashes: blockhashes ?? new BlockhashBook(connection, { now }),
    funds: funds ?? FundsGuard.forSol({ read: () => pool.refresh(connection), minSol: cfg.minPoolSol, now, log }),
    /** Signatures this relay sent → {lastValidBlockHeight, kind} (GET /f/tx expiry); the oldest forgotten past 10,000. */
    sent: boundedMap(10_000),
  };
}

function boundedMap(max) {
  const m = new Map();
  const set = m.set.bind(m);
  m.set = (k, v) => { set(k, v); if (m.size > max) m.delete(m.keys().next().value); return m; };
  return m;
}

/** The route for `method path` (exact, or a prefix route such as 'GET /f/tx/'), with its table key. */
function findRoute(routes, method, pathname) {
  const exact = `${method} ${pathname}`;
  if (routes[exact]) return [exact, routes[exact]];
  for (const k of Object.keys(routes)) if (k.endsWith('/') && exact.startsWith(k) && exact.length > k.length) return [k, routes[k]];
  return [exact, null];
}

/** A Node request handler over `ctx` for one listener: 'operator' (every route) or 'public'. */
export function createFrontierHandler(ctx, { surface = 'operator', routes = FRONTIER_ROUTES } = {}) {
  const pub = surface === 'public';
  const allowed = new Set(FRONTIER_PUBLIC_ROUTES);
  const funded = new Set(FRONTIER_FUNDED_ROUTES);
  return async function handle(req, res) {
    let url;
    try {
      url = new URL(req.url, 'http://relay');
    } catch {
      res.writeHead(400, { 'Content-Type': 'application/json' });
      return res.end(toJson({ error: 'bad request URL', code: 'BadRequest' }));
    }
    const ip = clientAddress(req, { heraldPeer: ctx.cfg.heraldPeer });
    let body;
    const request = { method: req.method, url, headers: req.headers, surface, ip, json: () => (body ??= readJson(req, FRONTIER_MAX_BODY_BYTES)) };
    let out;
    try {
      const [key, route] = findRoute(routes, req.method, url.pathname);
      if (!route || (pub && !allowed.has(key))) throw new RouteError(404, 'not found', 'NotFound');
      if (pub && FRONTIER_IP_LIMITS[key] && !isLoopback(ip)) ctx.limiter.check(`f-ip:${key}:${addressBucket(ip)}`, FRONTIER_IP_LIMITS[key]);
      if (funded.has(key)) await ctx.funds.check();
      out = await route(ctx, request);
    } catch (e) {
      const r = errorResponse(e);
      if (r.log) ctx.log(`${req.method} ${url.pathname}: ${e.message}`);
      out = r;
    }
    const headers = { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...(pub ? { 'X-Content-Type-Options': 'nosniff' } : {}) };
    res.writeHead(out.status ?? 200, headers);
    res.end(toJson(out.body));
  };
}

/** Both listeners' handlers over one context: `{ctx, operator, public}`. */
export function createFrontierApps(o) {
  const ctx = createFrontierContext(o);
  return { ctx, operator: createFrontierHandler(ctx), public: createFrontierHandler(ctx, { surface: 'public' }) };
}
