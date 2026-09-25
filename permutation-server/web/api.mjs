// HTTP access to the play server: the member token, JSON get/post, and the
// Japanese text for server errors.

// Member token: this browser's membership of one nation (POST /api/join, or
// /api/claim in chain mode). Kept per origin under this key (unchanged, so
// existing browsers keep their membership); storage can be unavailable, so
// every access is guarded.
const TOKEN_KEY = 'ps-member-token';
export const tokenStore = {
  get() { try { return localStorage.getItem(TOKEN_KEY); } catch { return null; } },
  set(v) { try { if (v) localStorage.setItem(TOKEN_KEY, v); else localStorage.removeItem(TOKEN_KEY); } catch { /* private mode */ } },
};

let token = null;
export const memberToken = () => token;
export function setMemberToken(t) { token = t || null; }
const auth = () => (token ? { 'X-Member-Token': token } : {});

/** GET JSON; rejects on a network error or a non-2xx status. */
export async function get(path) {
  const r = await fetch(path, { cache: 'no-store', headers: auth() });
  if (!r.ok) throw new Error(String(r.status));
  return r.json();
}
/** GET JSON, or `fallback` on any failure (for loaders that can wait for the next poll). */
export const tryGet = (path, fallback = null) => get(path).catch(() => fallback);

/**
 * POST JSON. Never rejects: a network failure (or a body that is not JSON)
 * resolves to `{ ok: false, error: 'network' }`, which translateError shows.
 */
export async function post(path, body) {
  try {
    const r = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json', ...auth() }, body: JSON.stringify(body) });
    return await r.json();
  } catch {
    return { ok: false, error: 'network' };
  }
}

/** Server error strings → Japanese (unknown ones pass through). */
export function translateError(error) {
  const e = String(error ?? '');
  let m;
  if ((m = e.match(/orders cost (\d+), only (\d+) spendable/))) return `命令の枠が足りません（必要${m[1]}・使える枠${m[2]}）`;
  if (e.includes('frozen')) return '終盤のため凍結中の命令が含まれています';
  if (e.includes('more than one manual order')) return '同じ部隊に2つの命令があります';
  if (e.includes('TickFrozen') || e.includes('WrongTick')) return 'このティックは締め切られました。次のティックで出し直してください';
  if (e === 'network') return 'サーバーに届きませんでした';
  return e;
}
