// The gateway's HTTP application: routes (routes/*.mjs) over one context.
// A route is `async (ctx, req) => ({ status = 200, body, headers, raw })`
// where `req` is { method, url: URL, headers, json() }; it throws a
// RouteError (or lets a program error through) to fail. `createApp` returns
// a plain Node request handler, so it is testable without a socket.
import { toJson } from '../client/src/bytes.mjs';
import { ChainClient } from '../client/src/chain.mjs';
import { namedKey } from './config.mjs';
import { memberKeyName } from './season.mjs';
import { createMemberRegistry } from './registry.mjs';
import { errorResponse, RouteError } from './routes/errors.mjs';
import { FaucetLimiter, faucetRoutes } from './routes/faucet.mjs';
import { relayRoutes } from './routes/relay.mjs';
import { rosterRoutes } from './routes/roster.mjs';
import { seasonRoutes } from './routes/season.mjs';
import { x402Routes } from './routes/x402.mjs';
import { BlockhashBook } from './send.mjs';

export const ROUTES = Object.freeze({ ...seasonRoutes, ...relayRoutes, ...x402Routes, ...faucetRoutes, ...rosterRoutes });
export const MAX_BODY_BYTES = 1 << 20;
const CORS = { 'Access-Control-Allow-Origin': '*' };

/** JSON with bigints as decimal strings and byte arrays as hex (re-exported: tests and tools import it from here). */
export { toJson };

function readJson(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', c => {
      size += c.length;
      if (size > MAX_BODY_BYTES) { reject(new RouteError(413, 'request body too large', 'BodyTooLarge')); req.destroy(); return; }
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
 * @param {object} o
 * @param {object} o.cfg     loadConfig()
 * @param {object} o.base    base-layer Connection
 * @param {object} o.er      ER Connection
 * @param {object} o.store   season state store (createStateStore)
 * @param {object} o.crank   the Crank (its key pays fees; phase, snapshots, tick index)
 * @param {Function} [o.keys] named key loader (config.mjs namedKey)
 */
export function createApp({ cfg, base, er, store, crank, keys = namedKey, log = console.log, now = Date.now, routes = ROUTES, registry, advisor = null }) {
  const chain = new ChainClient(cfg.programId, BigInt(store.state.seasonId));
  const ctx = {
    cfg, base, er, store, crank, keys, log, now, chain, advisor,
    registry: registry ?? createMemberRegistry({ base, chain, store, now }),
    blockhashes: { base: new BlockhashBook(base), er: new BlockhashBook(er) },
    faucet: new FaucetLimiter({ now }),
    /** The session key of a member this gateway hosts, or null. */
    hostedKey(member) {
      const h = store.state.members.find(x => x.index === member);
      return h && h.hosted !== 'external' ? keys(memberKeyName(h, 'session')) : null;
    },
  };

  return async function handle(req, res) {
    const url = new URL(req.url, 'http://gateway');
    if (req.method === 'OPTIONS') {
      res.writeHead(204, { ...CORS, 'Access-Control-Allow-Headers': '*', 'Access-Control-Allow-Methods': 'GET,POST' });
      return res.end();
    }
    let body;
    const request = { method: req.method, url, headers: req.headers, json: () => (body ??= readJson(req)) };
    let out;
    try {
      const route = routes[`${req.method} ${url.pathname}`];
      if (!route) throw new RouteError(404, 'not found', 'NotFound');
      out = await route(ctx, request);
    } catch (e) {
      const r = errorResponse(e);
      if (r.log) log(`${req.method} ${url.pathname}: ${e.message}`);
      out = r;
    }
    if (out.raw) {
      res.writeHead(out.status ?? 200, { ...out.headers, ...CORS });
      return res.end(out.raw);
    }
    res.writeHead(out.status ?? 200, { ...out.headers, 'Content-Type': 'application/json', ...CORS });
    res.end(toJson(out.body));
  };
}
