// The gateway's HTTP application: routes (routes/*.mjs) over one context,
// served on two listeners.
//
// * Operator listener (`--port`, always 127.0.0.1): every route. The game
//   server uses it (the operator token never leaves this machine).
// * Public listener (`--public-port`, default the operator port + 3;
//   `--public-host`): only PUBLIC_ROUTES, everything else 404; rate limits
//   per client address (IP_LIMITS; a request from this machine, e.g. the
//   play server's /gw, is the address its X-Forwarded-For names unless
//   `--no-trust-proxy`); no RPC URLs unless `--public-base-rpc` /
//   `--public-er-rpc` say which to publish.
// * On both: whatever makes the crank pay for a member (FUNDED_ROUTES)
//   pauses (503 OperatorLowFunds) while the crank's SOL is below
//   `--min-crank-sol`, with the crank's own AI registrations (one FundsGuard
//   for both, so the operator's AI members never act while people cannot).
//
// A route is `async (ctx, req) => ({ status = 200, body, headers, raw })`
// where `req` is { method, url: URL, headers, surface ('operator' |
// 'public'), ip, json() }; it throws a RouteError (or lets a program error
// through) to fail. `createApp` returns a plain Node request handler, so it
// is testable without a socket.
import { toJson } from '../client/src/bytes.mjs';
import { ChainClient } from '../client/src/chain.mjs';
import { DEFAULTS, namedKey } from './config.mjs';
import { RegistrationDesk } from './crank.mjs';
import { addressBucket, clientIp, FundsGuard, IP_LIMITS, RateLimiter, ReplayCache, TtlCache } from './guards.mjs';
import { memberKeyName } from './season.mjs';
import { createMemberRegistry } from './registry.mjs';
import { claimsRoutes } from './routes/claims.mjs';
import { errorResponse, RouteError, routeSeason } from './routes/errors.mjs';
import { faucetRoutes } from './routes/faucet.mjs';
import { CommitCounter, relayRoutes } from './routes/relay.mjs';
import { rosterRoutes } from './routes/roster.mjs';
import { SealCounter, sealRoutes } from './routes/seal.mjs';
import { seasonRoutes } from './routes/season.mjs';
import { x402Routes } from './routes/x402.mjs';
import { BlockhashBook } from './send.mjs';

export const ROUTES = Object.freeze({ ...seasonRoutes, ...relayRoutes, ...x402Routes, ...faucetRoutes, ...rosterRoutes, ...sealRoutes, ...claimsRoutes });

/** What the public listener serves (browsers through the play server's /gw, agents). */
export const PUBLIC_ROUTES = Object.freeze([
  'GET /season', 'GET /tick', 'GET /ticks', 'GET /history', 'GET /roster', 'GET /talk', 'GET /relay', 'GET /claim-relay', 'GET /usdc', 'GET /world.bin',
  'GET /claims',
  'POST /x402/join', 'POST /relay', 'POST /claim-relay', 'POST /seal', 'POST /talk', 'POST /faucet',
]);
/** Public routes that make the crank pay (fees, rent). */
export const COSIGN_ROUTES = Object.freeze(['POST /x402/join', 'POST /relay', 'POST /claim-relay', 'POST /faucet']);
/**
 * Everything that makes the crank pay for a member, on either listener: the
 * co-signing routes and the faucet, and the operator's /submit and /gov (its
 * AI members' version of /relay). Paused while the crank is low on SOL, as
 * are the crank's AI registrations (crank.mjs `registerAis`).
 */
export const FUNDED_ROUTES = Object.freeze([...COSIGN_ROUTES, 'POST /submit', 'POST /gov']);
export const MAX_BODY_BYTES = 1 << 20;
/** Every public request is small (a transaction, a batch of orders). */
export const PUBLIC_MAX_BODY_BYTES = 64 << 10;
const CORS = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Expose-Headers': 'X-PAYMENT-RESPONSE' };

/** JSON with bigints as decimal strings and byte arrays as hex (re-exported: tests and tools import it from here). */
export { toJson };

function readJson(req, limit) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', c => {
      if (size > limit) return;
      size += c.length;
      // Too large: answered at once, and the connection closed after the answer.
      if (size > limit) { reject(new RouteError(413, 'request body too large', 'BodyTooLarge', { close: true })); return; }
      chunks.push(c);
    });
    req.on('error', reject);
    req.on('end', () => {
      const text = Buffer.concat(chunks).toString('utf8');
      try { resolve(text ? JSON.parse(text) : {}); } catch { reject(new RouteError(400, 'the request body is not JSON', 'InvalidJson')); }
    });
  });
}

/**
 * The routes' shared context (one per gateway: both listeners share its
 * limits, caches and registrations in flight).
 * @param {object} o
 * @param {object} o.cfg     loadConfig()
 * @param {object} o.base    base-layer Connection
 * @param {object} o.er      ER Connection
 * @param {object} o.store   season state store (createStateStore)
 * @param {object} o.crank   the Crank (its key pays fees; phase, snapshots, tick index, registrations in flight)
 * @param {Function} [o.keys] named key loader (config.mjs namedKey)
 */
export function createContext({ cfg, base, er, store, crank, keys = namedKey, log = console.log, now = Date.now, registry, advisor = null, limiter, funds }) {
  const chain = new ChainClient(cfg.programId, BigInt(store.state.seasonId));
  let nationKeys = null;
  const ctx = {
    cfg, base, er, store, crank, keys, log, now, chain, advisor,
    registry: registry ?? createMemberRegistry({ base, chain, store, now }),
    blockhashes: { base: new BlockhashBook(base, { now }), er: new BlockhashBook(er, { now }) },
    limiter: limiter ?? new RateLimiter({ now }),
    funds: funds ?? FundsGuard.forSol({ read: () => base.getBalance(crank.crank.publicKey, 'confirmed'), minSol: cfg.minCrankSol ?? DEFAULTS.minCrankSol, now, log }),
    desk: crank.desk ?? new RegistrationDesk(),
    usdc: new TtlCache({ ttlMs: 5000, now }),
    claims: new TtlCache({ ttlMs: 5000, now }),
    /** Member signatures already charged to a signer's bucket (/relay, /claim-relay): a replay is 409 Duplicate. */
    relayed: new ReplayCache({ now }),
    commits: new CommitCounter(),
    seals: new SealCounter(),
    /** The session key of one of the operator's AI members (the game server runs them), or null. */
    hostedKey(member) {
      const h = store.state.members.find(x => x.index === member);
      return h && h.hosted === 'ai' ? keys(memberKeyName(h, 'session')) : null;
    },
    /** This season's nation accounts: base58 → civ. */
    async nationKeys() {
      if (!nationKeys) {
        const n = crank.nationCount ? await crank.nationCount() : (await routeSeason({ base, chain })).nations;
        nationKeys = new Map(chain.nations(n).map((k, civ) => [k.toBase58(), civ]));
      }
      return nationKeys;
    },
    /** The seasons a claim may be for: this one and the ones in its lineage, by season address (base58) → ChainClient. */
    claimSeasons() {
      const ids = [store.state.seasonId, ...(store.state.lineage ?? []).map(l => l.seasonId)];
      return new Map(ids.map(id => new ChainClient(cfg.programId, BigInt(id))).map(c => [c.season.toBase58(), c]));
    },
    claimSeasonById(id) {
      if (!/^\d{1,20}$/.test(String(id))) return null;
      return [...ctx.claimSeasons().values()].find(c => c.seasonId === BigInt(id)) ?? null;
    },
  };
  return ctx;
}

/**
 * A Node request handler over `ctx` for one listener: `surface` 'operator'
 * (every route) or 'public' (PUBLIC_ROUTES, limits and the breaker).
 */
export function createHandler(ctx, { surface = 'operator', routes = ROUTES, trustProxy = DEFAULTS.trustProxy } = {}) {
  const pub = surface === 'public';
  const allowed = new Set(PUBLIC_ROUTES);
  const publicPaths = new Set(PUBLIC_ROUTES.map(r => r.split(' ')[1]));
  const funded = new Set(FUNDED_ROUTES);
  const limit = pub ? PUBLIC_MAX_BODY_BYTES : MAX_BODY_BYTES;
  let warnedProxy = false;
  return async function handle(req, res) {
    let url;
    try {
      url = new URL(req.url, 'http://gateway');
    } catch {
      res.writeHead(400, { ...CORS, 'Content-Type': 'application/json' });
      return res.end(toJson({ error: 'bad request URL', code: 'BadRequest' }));
    }
    if (req.method === 'OPTIONS') {
      const known = !pub || publicPaths.has(url.pathname);
      res.writeHead(known ? 204 : 404, { ...CORS, 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': 'GET,POST', 'Access-Control-Max-Age': '600' });
      return res.end();
    }
    const key = `${req.method} ${url.pathname}`;
    const ip = clientIp(req, { trustProxy });
    if (pub && !trustProxy && !warnedProxy && req.headers?.['x-forwarded-for'] && clientIp(req, { trustProxy: true }) !== ip) {
      warnedProxy = true;
      ctx.log('WARNING: public listener: requests from a proxy on this machine carry X-Forwarded-For, but --no-trust-proxy is set: every client shares one rate-limit budget');
    }
    let body;
    const request = { method: req.method, url, headers: req.headers, surface, ip, json: () => (body ??= readJson(req, limit)) };
    let out;
    try {
      const route = routes[key];
      if (!route || (pub && !allowed.has(key))) throw new RouteError(404, 'not found', 'NotFound');
      if (pub && IP_LIMITS[key]) ctx.limiter.check(`ip:${key}:${addressBucket(ip)}`, IP_LIMITS[key]);
      if (funded.has(key)) await ctx.funds.check();
      out = await route(ctx, request);
    } catch (e) {
      const r = errorResponse(e);
      if (r.log) ctx.log(`${req.method} ${url.pathname}: ${e.message}`);
      out = r;
    }
    const headers = { ...out.headers, ...CORS, ...(pub ? { 'X-Content-Type-Options': 'nosniff' } : {}), ...(out.body?.close ? { Connection: 'close' } : {}) };
    if (out.body?.close) delete out.body.close;
    if (out.raw) {
      res.writeHead(out.status ?? 200, headers);
      return res.end(out.raw);
    }
    res.writeHead(out.status ?? 200, { ...headers, 'Content-Type': 'application/json' });
    res.end(toJson(out.body));
  };
}

/** The operator listener's handler (or `surface: 'public'`), with a context of its own (tests, tools). */
const trusts = cfg => !!(cfg.trustProxy ?? DEFAULTS.trustProxy);

export function createApp({ surface = 'operator', routes = ROUTES, ...o }) {
  return createHandler(createContext(o), { surface, routes, trustProxy: trusts(o.cfg) });
}

/**
 * Both listeners' handlers over one context: `{ctx, operator, public}`.
 * Pass the crank's FundsGuard as `funds` (server.mjs), so the routes and the
 * crank's AI registrations pause together.
 */
export function createApps(o) {
  const ctx = createContext(o);
  return { ctx, operator: createHandler(ctx), public: createHandler(ctx, { surface: 'public', trustProxy: trusts(o.cfg) }) };
}

