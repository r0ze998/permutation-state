// A local HTTP → HTTPS relay for JSON-RPC, so tools with a plain-HTTP client
// (the Rust `verify` and `play --chain`) can read devnet endpoints. It only
// forwards POST bodies and answers; the verifier still checks everything it
// reads against roots and hashes, so the relay adds no trust.
//
//   node scripts/rpc-proxy.mjs --port 18999 --target https://api.devnet.solana.com
//   node scripts/rpc-proxy.mjs --port 17999 --target https://devnet-as.magicblock.app
//
// Public RPCs rate-limit (HTTP 429): the relay retries those with backoff.
import http from 'node:http';

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const port = Number(arg('--port', '18999'));
const target = arg('--target', 'https://api.devnet.solana.com');
const sleep = ms => new Promise(r => setTimeout(r, ms));

http.createServer(async (req, res) => {
  let body = '';
  for await (const c of req) body += c;
  for (let i = 0; ; i++) {
    try {
      const r = await fetch(target, { method: req.method, headers: { 'content-type': 'application/json' }, body: req.method === 'POST' ? body : undefined });
      const text = await r.text();
      if (r.status === 429 && i < 8) { await sleep(500 * 2 ** Math.min(i, 4)); continue; }
      res.writeHead(r.status, { 'content-type': 'application/json' });
      return res.end(text);
    } catch (e) {
      if (i < 4) { await sleep(1000); continue; }
      res.writeHead(502, { 'content-type': 'application/json' });
      return res.end(JSON.stringify({ jsonrpc: '2.0', error: { code: -32000, message: `relay: ${e.message}` }, id: null }));
    }
  }
}).listen(port, '127.0.0.1', () => console.log(`relaying http://127.0.0.1:${port} → ${target}`));
