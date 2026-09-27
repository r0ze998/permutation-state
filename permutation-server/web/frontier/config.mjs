// Where the page reads and writes (web design §3.1, §12). The herald's HTTP
// listener serves the page under /frontier/, the reads under /h/* and the
// relay under /gw/* — one origin, so the CSP can say connect-src 'self'.
// A `?herald=` override exists for development and is honoured only when
// the page itself is on a loopback host; it is never read from anywhere
// else.
const LOOPBACK = h => h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h.endsWith('.localhost');

/** The page's endpoints and mode from its location and <body data-mode>. */
export function config(loc = globalThis.location, doc = globalThis.document) {
  const origin = loc?.origin ?? '';
  const host = loc?.hostname ?? '';
  const dev = LOOPBACK(host);
  let herald = origin;
  try {
    const o = new URL(loc?.href ?? origin).searchParams.get('herald');
    if (o && dev) {
      const u = new URL(o);
      if ((u.protocol === 'http:' || u.protocol === 'https:') && LOOPBACK(u.hostname)) herald = u.origin;
    }
  } catch { /* no override */ }
  const mode = doc?.body?.dataset?.mode;
  return {
    origin,
    herald,
    relay: `${herald}/gw`,
    dev,
    mode: mode === 'practice' || mode === 'spectate' ? mode : 'play',
  };
}
