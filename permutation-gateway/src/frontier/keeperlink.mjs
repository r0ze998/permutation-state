// The relay's link to a keeper's loopback API (contract §8.2, §8.3, I-24):
//
//   POST /f/reveal {holding, transit_slot, plain_b64, salt_b64, ct_hash_b64}  →  keeper POST /v1/reveal
//   POST /f/nudge  {province: [P, Q], bell}                                    →  keeper POST /v1/nudge
//
// The browser sends reveal material, not a transaction; the keeper builds the
// Reveal, signs it with a random payer of its reveal pool and escalates it.
// The relay checks the body's shape (sizes, ranges) so garbage never reaches
// the keeper, forwards it with the keeper's bearer token (read from its
// token file; the keeper binds 127.0.0.1 only) and passes the keeper's
// answer through: 202 {accepted, track}, 409 CommitMismatch, 410
// WindowClosed, 422 BadPlaintext… A keeper that does not answer is 502
// KeeperUnavailable.
import { readFileSync } from 'node:fs';
import { decode as fromBase58 } from '../../client/src/base58.mjs';
import { fromBase64, toBase64 } from '../../client/src/bytes.mjs';
import { RouteError } from '../routes/errors.mjs';

export const KEEPER_TIMEOUT_MS = 10_000;
const TRANSIT_SLOTS = 4;

const bad = (what, code = 'BadRequest') => new RouteError(400, what, code);

/** Exact base64 of `n` bytes (canonical: re-encoding gives the same string). */
function b64(v, n, what) {
  let b = null;
  try { b = typeof v === 'string' ? fromBase64(v) : null; } catch { b = null; }
  if (!b || b.length !== n || toBase64(b) !== v) throw bad(`${what} must be base64 of ${n} bytes`);
  return v;
}

function key(v, what) {
  let ok = false;
  try { ok = typeof v === 'string' && fromBase58(v).length === 32; } catch { ok = false; }
  if (!ok) throw bad(`${what} must be a base58 public key`);
  return v;
}

/** A POST /f/reveal body checked and normalised (only the contract's fields go on). */
export function revealBody(b) {
  if (!b || typeof b !== 'object') throw bad('a JSON object is required');
  if (!Number.isInteger(b.transit_slot) || b.transit_slot < 0 || b.transit_slot >= TRANSIT_SLOTS) throw bad(`transit_slot must be 0–${TRANSIT_SLOTS - 1}`);
  return {
    holding: key(b.holding, 'holding'), transit_slot: b.transit_slot, plain_b64: b64(b.plain_b64, 37, 'plain_b64'),
    salt_b64: b64(b.salt_b64, 32, 'salt_b64'), ct_hash_b64: b64(b.ct_hash_b64, 32, 'ct_hash_b64'),
  };
}

/** A POST /f/nudge body checked: `{province: [P, Q] (i16), bell (u32)}`. */
export function nudgeBody(b) {
  const p = b?.province;
  const i16 = x => Number.isInteger(x) && x >= -32768 && x <= 32767;
  if (!Array.isArray(p) || p.length !== 2 || !p.every(i16)) throw bad('province must be [P, Q] (integers)');
  if (!Number.isInteger(b.bell) || b.bell < 0 || b.bell > 0xffffffff) throw bad('bell must be a u32');
  return { province: [p[0], p[1]], bell: b.bell };
}

export class KeeperLink {
  /** `url`: the keeper API (loopback); `token` or `tokenFile`: its bearer token; `fetch`: injectable. */
  constructor({ url, token = null, tokenFile = null, fetch: f = globalThis.fetch, timeoutMs = KEEPER_TIMEOUT_MS }) {
    this.url = String(url).replace(/\/$/, '');
    this.tokenFile = tokenFile;
    this.token = token;
    this.fetch = f;
    this.timeoutMs = timeoutMs;
  }

  bearer() {
    if (this.token) return this.token;
    if (this.tokenFile) return readFileSync(this.tokenFile, 'utf8').trim();
    return null;
  }

  /** POST `path` with `body`; the keeper's `{status, body}`, or 502 KeeperUnavailable. */
  async post(path, body) {
    const token = this.bearer();
    let r;
    try {
      r = await this.fetch(this.url + path, {
        method: 'POST', body: JSON.stringify(body), signal: AbortSignal.timeout(this.timeoutMs),
        headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
      });
    } catch (e) {
      throw new RouteError(502, `the keeper did not answer (${e.name === 'TimeoutError' ? 'timeout' : e.message})`, 'KeeperUnavailable');
    }
    let json;
    try { json = await r.json(); } catch { json = null; }
    if (r.status >= 500 || json === null || typeof json !== 'object') {
      throw new RouteError(502, `the keeper answered HTTP ${r.status}`, 'KeeperUnavailable');
    }
    return { status: r.status, body: json };
  }

  reveal(body) { return this.post('/v1/reveal', revealBody(body)); }
  nudge(body) { return this.post('/v1/nudge', nudgeBody(body)); }
}
