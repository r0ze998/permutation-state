// The screenshot suite's fixture server (web design §13.2): node `http` on
// 127.0.0.1, port 0. It serves `permutation-server/web/` as the herald does
// (the Frontier pages under /frontier/ with the page CSP of web design §12),
// a fake herald from world.mjs under /h/*, and a fake relay under /gw/*
// that answers the reads the page makes (GET /f/relay, /f/quota) and
// refuses every write with 503 (the smoke never sends a transaction).
// Nothing touches a chain or a fixed port.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as W from './world.mjs';

export const WEB_ROOT = fileURLToPath(new URL('../../permutation-server/web/', import.meta.url));
export const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; frame-ancestors 'none'";
const TYPES = { '.html': 'text/html; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8',
  '.json': 'application/json', '.wasm': 'application/wasm', '.sha256': 'text/plain; charset=utf-8', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.woff2': 'font/woff2' };

/**
 * Start the server: `{url, port, stage(name), requests, close()}`. `stage`
 * sets the viewer stage /h/me answers (none | joined | holding); `requests`
 * logs every path served, with its status.
 */
export async function startServer() {
  const v = await W.viewer();
  let stage = 'none';
  const requests = [];
  const json = (res, code, body) => { res.writeHead(code, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); res.end(JSON.stringify(body)); return code; };

  async function herald(path, res) {
    let m;
    if (path === '/h/season') return json(res, 200, W.seasonRecord());
    if ((m = /^\/h\/overview\/(\d+)\/(?:latest|\d+)\.bin$/.exec(path))) {
      const d = Number(m[1]);
      if (d > 2) return json(res, 404, { code: 'NotFound' });
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Cache-Control': 'no-store' });
      res.end(W.overview(d));
      return 200;
    }
    if ((m = /^\/h\/roster\/(\d+)\/latest\.bin$/.exec(path))) {
      const d = Number(m[1]);
      if (d > 2) return json(res, 404, { code: 'NotFound' });
      res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Cache-Control': 'no-store' });
      res.end(W.roster(d));
      return 200;
    }
    if ((m = /^\/h\/province\/(-?\d+),(-?\d+)\/(latest|\d+)$/.exec(path))) return json(res, 200, W.provinceEnvelope(Number(m[1]), Number(m[2]), m[3] === 'latest' ? W.BELL : Number(m[3])));
    if ((m = /^\/h\/bell\/(\d+)\/region\/(\d+)$/.exec(path))) return json(res, 200, W.bellRegion(Number(m[1]), Number(m[2])));
    if ((m = /^\/h\/me\/(\w+)$/.exec(path))) return m[1] === v.wallet ? json(res, 200, W.meRecord(v, stage)) : json(res, 200, W.meRecord({ ...v, wallet: m[1] }, 'none'));
    if (path === '/h/events') {
      const list = W.events();
      return json(res, 200, { events: list, next: list[list.length - 1].seq, full: false });
    }
    if ((m = /^\/h\/clash\/(-?\d+),(-?\d+)\/(\d+)$/.exec(path))) {
      if (Number(m[1]) === W.HOME.p && Number(m[2]) === W.HOME.q && Number(m[3]) === W.REPORT_BELL) return json(res, 200, W.clashReport());
      return json(res, 404, { code: 'NotFound' });
    }
    return json(res, 404, { code: 'NotFound' });
  }

  function relay(path, method, res) {
    const quota = { left: 38, resetsAt: W.GENESIS_TS + 86_400, lamportsLeft: '400000000' };
    if (method === 'GET' && path === '/gw/f/quota') return json(res, 200, { ok: true, ...quota });
    if (method === 'GET' && path === '/gw/f/relay') return json(res, 200, { ok: true, feePayer: v.A.season, blockhash: '11111111111111111111111111111111', lastValidBlockHeight: 1000, programId: W.PROGRAM, quota });
    return json(res, 503, { ok: false, code: 'Unavailable', error: 'the screenshot fixture relay sends nothing' });
  }

  async function file(path, res) {
    if (path === '/' || path === '/frontier' || path === '/frontier/') path = '/frontier/index.html';
    const full = normalize(join(WEB_ROOT, decodeURIComponent(path)));
    if (!full.startsWith(WEB_ROOT) || full.includes(`${sep}..${sep}`)) return json(res, 403, { code: 'Forbidden' });
    let body;
    try { body = await readFile(full); } catch { return json(res, 404, { code: 'NotFound' }); }
    const type = TYPES[extname(full)] ?? 'application/octet-stream';
    const headers = { 'Content-Type': type, 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' };
    if (type.startsWith('text/html')) Object.assign(headers, { 'Content-Security-Policy': CSP, 'Referrer-Policy': 'no-referrer' });
    res.writeHead(200, headers);
    res.end(body);
    return 200;
  }

  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    const path = url.pathname;
    let code;
    try {
      if (path === '/favicon.ico') { res.writeHead(204); res.end(); code = 204; }
      else if (path.startsWith('/h/')) code = await herald(path, res);
      else if (path.startsWith('/gw/')) code = relay(path, req.method, res);
      else code = await file(path, res);
    } catch (e) {
      code = json(res, 500, { code: 'FixtureError', error: String(e?.message ?? e) });
    }
    requests.push({ method: req.method, path: url.pathname + url.search, code });
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const { port } = server.address();
  return {
    url: `http://127.0.0.1:${port}`,
    port,
    viewer: v,
    stage: s => { stage = s; },
    requests,
    close: () => new Promise(r => { server.closeAllConnections?.(); server.close(r); }),
  };
}
