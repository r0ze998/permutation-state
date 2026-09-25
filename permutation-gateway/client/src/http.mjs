// JSON over HTTP for the client, with one error model: every failure the
// gateway or game server reports becomes an `HttpError` whose `code` is the
// gateway's machine-readable `code` (a program error name such as
// `TickFrozen`, or a gateway code such as `NotHosted`); failures detected
// locally are `GameError`s with a `code` too.

export const DEFAULT_SERVER = 'http://127.0.0.1:4185';
// Not 4190: browsers and Node's fetch refuse it (a "bad port" in the Fetch standard).
export const DEFAULT_GATEWAY = 'http://127.0.0.1:4191';

export class GameError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

export class HttpError extends GameError {
  constructor(status, body, url) {
    super(body?.code ?? null, `${url}: HTTP ${status} ${body?.error ?? ''}`.trim());
    this.status = status;
    this.body = body;
    this.url = url;
  }
}

/** One request; the body is parsed as JSON (text that is not JSON comes back as `{error: text}`). */
export async function request(url, { method = 'GET', body, headers = {} } = {}) {
  const res = await fetch(url, { method, headers: { ...(body ? { 'Content-Type': 'application/json' } : {}), ...headers }, body: body ? JSON.stringify(body) : undefined });
  const text = await res.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = { error: text }; }
  return { status: res.status, headers: res.headers, body: json };
}

/** A request whose answer must be a success: returns the body, throws `HttpError` otherwise. */
export async function requestJson(url, opts) {
  const r = await request(url, opts);
  if (r.status >= 400) throw new HttpError(r.status, r.body, url);
  return r.body;
}

/** Program error name of an error from the gateway (or null). */
export const errorCode = e => e?.code ?? e?.body?.code ?? null;
