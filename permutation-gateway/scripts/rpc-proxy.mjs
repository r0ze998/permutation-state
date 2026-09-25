// A local HTTP → HTTPS relay for JSON-RPC, so tools with a plain-HTTP client
// (the Rust `verify` and `play --chain`) can read devnet endpoints. It only
// forwards POST bodies and answers; the verifier still checks everything it
// reads against roots and hashes, so the relay adds no trust.
//
//   node scripts/rpc-proxy.mjs --port 18999 --target https://api.devnet.solana.com
//   node scripts/rpc-proxy.mjs --port 17999 --target https://devnet-as.magicblock.app
//
// Public RPCs rate-limit (HTTP 429): the relay retries those with backoff,
// and network failures a few times.
import http from 'node:http';
import { retry } from '../client/src/retry.mjs';
import { DEFAULTS, parseArgs } from '../src/config.mjs';

const args = parseArgs(process.argv.slice(2), { port: String(DEFAULTS.proxyPort), target: DEFAULTS.proxyTarget });
const port = Number(args.port);
const target = args.target;

class RateLimited extends Error {
  constructor(status, text) { super(`HTTP ${status}`); this.status = status; this.text = text; }
}

async function forward(method, body) {
  // 429: up to 9 tries, 0.5 s doubling to 8 s. Network errors: up to 5 tries, 1 s apart.
  let networkFailures = 0;
  return retry(async () => {
    let r;
    try {
      r = await fetch(target, { method, headers: { 'content-type': 'application/json' }, body: method === 'POST' ? body : undefined });
    } catch (e) {
      networkFailures++;
      throw e;
    }
    const text = await r.text();
    if (r.status === 429) throw new RateLimited(r.status, text);
    return { status: r.status, text };
  }, {
    attempts: 9, delayMs: 500, backoff: 2, maxDelayMs: 8000,
    retryIf: e => (e instanceof RateLimited ? true : networkFailures < 5),
  }).catch(e => (e instanceof RateLimited ? { status: e.status, text: e.text } : Promise.reject(e)));
}

http.createServer(async (req, res) => {
  let body = '';
  for await (const c of req) body += c;
  try {
    const { status, text } = await forward(req.method, body);
    res.writeHead(status, { 'content-type': 'application/json' });
    res.end(text);
  } catch (e) {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ jsonrpc: '2.0', error: { code: -32000, message: `relay: ${e.message}` }, id: null }));
  }
}).listen(port, '127.0.0.1', () => console.log(`relaying http://127.0.0.1:${port} → ${target}`));
