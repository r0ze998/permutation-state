// HTTP access to the play server: who is asking (the member token in local
// mode, the member id in chain mode), JSON get/post, and the text for
// server errors (in the current language).
import { errorText } from './i18n.mjs';
import { L } from './lang.mjs';

// Member token (local mode only): this browser's membership of one nation
// (POST /api/join). Kept per origin under this key (unchanged, so existing
// browsers keep their membership); storage can be unavailable, so every
// access is guarded. Chain mode has no tokens: every member signs its own
// transactions in the browser and is viewed by id.
const TOKEN_KEY = 'ps-member-token';
export const tokenStore = {
  get() { try { return localStorage.getItem(TOKEN_KEY); } catch { return null; } },
  set(v) { try { if (v) localStorage.setItem(TOKEN_KEY, v); else localStorage.removeItem(TOKEN_KEY); } catch { /* private mode */ } },
};

let token = null;
export const memberToken = () => token;
export function setMemberToken(t) { token = t || null; }
const auth = () => (token ? { 'X-Member-Token': token } : {});

// Chain mode: the member this page plays. Its public view is `?member=M`
// (the same fields for every member), added to every /api GET that does not
// already name a nation or a member.
let viewer = null;
export const viewerMember = () => viewer;
export function setViewerMember(m) { viewer = Number.isInteger(m) ? m : null; }
/** `path` with `member=M` when a chain member is set (and the path names no civ or member). */
export function withViewer(path, m = viewer) {
  if (m === null || !path.startsWith('/api/') || /[?&](member|civ)=/.test(path)) return path;
  return `${path}${path.includes('?') ? '&' : '?'}member=${m}`;
}

/** GET JSON; rejects on a network error or a non-2xx status. */
export async function get(path) {
  const r = await fetch(withViewer(path), { cache: 'no-store', headers: auth() });
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

/**
 * Server errors → the current language (unknown ones pass through). Also takes a
 * chainio.mjs result or any `{code, error}`: chain, gateway and wallet
 * codes are translated by i18n.mjs `errorText`.
 */
export function translateError(error) {
  if (error && typeof error === 'object') return errorText(error);
  const e = String(error ?? '');
  let m;
  if ((m = e.match(/orders cost (\d+), only (\d+) spendable/))) return L`命令の枠が足りません（必要${m[1]}・使える枠${m[2]}）`;
  if (e.includes('frozen')) return L`終盤のため凍結中の命令が含まれています`;
  if (e.includes('more than one manual order')) return L`同じ部隊に2つの命令があります`;
  if (e.includes('TickFrozen') || e.includes('WrongTick')) return L`このティックは締め切られました。次のティックで出し直してください`;
  if (e === 'network') return L`サーバーに届きませんでした`;
  if (e === 'chain mode: sign in the browser') return L`オンチェーンのシーズンでは、この操作はこのブラウザのゲーム内の鍵で署名して送ります`;
  return errorText(e);
}

/**
 * Chain mode before the season plays: wait while the play server's lobby
 * says `registering` or `starting`, calling `show(phase)` on every check
 * (every 3 s); resolves with the first lobby that says otherwise.
 */
export async function untilPlaying(lobby, show = () => {}) {
  let l = lobby;
  while (l?.mode === 'chain' && (l.phase === 'registering' || l.phase === 'starting')) {
    show(l.phase);
    await new Promise(r => setTimeout(r, 3000));
    l = await tryGet('/api/lobby', l);
  }
  return l;
}
