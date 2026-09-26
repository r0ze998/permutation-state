// Small helpers shared by the player client, the spectator view and the map.
//
// HTML is built with the `html` tagged template: every interpolated value is
// escaped unless it is itself the result of `html`/`raw` (or an array of
// those). Build markup only with `html`, never with plain template strings,
// so server text (member names, errors, chronicle lines) cannot inject markup.
//
// lang.mjs imports `html` from here and this module imports `L` from there:
// neither uses the other while loading, so the cycle is harmless.
import { L } from './lang.mjs';

export const $ = s => document.querySelector(s);
export const $$ = s => document.querySelectorAll(s);

const ENTITY = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ENTITY[c]);

/** Markup that is already safe. */
class Html {
  constructor(s) { this.s = s; }
  toString() { return this.s; }
}
/** Mark a string of trusted, static markup as safe (never pass server text). */
export const raw = s => new Html(String(s ?? ''));
const part = v => (v instanceof Html ? v.s
  : Array.isArray(v) ? v.map(part).join('')
  : v === null || v === undefined || v === false ? '' : esc(v));
export const html = (strings, ...values) => new Html(strings.reduce((out, s, i) => out + s + (i < values.length ? part(values[i]) : ''), ''));
/** JSON for a data-* attribute (read back with JSON.parse(el.dataset.x)). */
export const attrJson = o => raw(esc(JSON.stringify(o)));

/**
 * Replace an element's markup only when it changed (keeps focus and hover).
 * `markup` is html`…`, an array of those, or plain text (escaped).
 */
export function setHtml(el, markup) {
  const s = part(markup);
  if (el.__html !== s) { el.innerHTML = s; el.__html = s; }
}

export const fmt = n => Number(n ?? 0).toLocaleString('ja-JP');
/** Like fmt, but an unknown value reads as a dash. */
export const fmtOr = n => (n === null || n === undefined ? '—' : fmt(n));
/** Micro-USDC → display amount. */
export const usdc = x => (Number(x ?? 0) / 1e6).toLocaleString('ja-JP', { maximumFractionDigits: 2 });
/** Micro-USDC → amount with exactly two decimals (tables, contracts, prices). */
export const usdcFixed = x => (Number(x ?? 0) / 1e6).toFixed(2);
/** abcd…wxyz for hashes and keys. */
export const short = (s, head = 4, tail = 4, empty = '') => (s ? `${s.slice(0, head)}…${s.slice(-tail)}` : empty);
/** Troops arrive ×10 (units) and display with one decimal. */
export const troops = t => (t / 10).toFixed(1);
export const hexDist = (a, b) => (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;

/** One shell word: as is when plain, else single-quoted (a URL with `?` or `&`). */
const shellWord = s => (/^[A-Za-z0-9_./:@%+=,~-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`);

/**
 * The `verify` command line for a chain season (`c`: the view's `chain`),
 * or null without a gateway: `{text, gateway, rpcKnown}`. The view may give
 * the gateway relative to this page (`/gw`, the play server's proxy);
 * verify runs on someone's own machine, so it gets the absolute URL
 * (resolved against `href`, this page). `--base` / `--er` are the RPC URLs
 * the view shows; a public deployment usually shows none (the gateway does
 * not give out the operator's RPC), and then placeholders stand in their
 * place and `rpcKnown` is false.
 */
export function verifyCommand(c, href = globalThis.location?.href) {
  if (!c?.gateway) return null;
  let gateway = String(c.gateway);
  try { gateway = new URL(gateway, href).href; } catch { /* no page to resolve against: as given */ }
  gateway = gateway.replace(/\/+$/, '');
  const rpc = u => (/^https?:\/\//.test(String(u ?? '')) ? String(u) : null);
  const base = rpc(c.endpoints?.base), er = rpc(c.endpoints?.er);
  const text = `cargo run --release --bin verify -- \\\n  --gateway ${shellWord(gateway)} \\\n  --base ${base ? shellWord(base) : '<base RPC>'} --er ${er ? shellWord(er) : '<ER RPC>'}`;
  return { text, gateway, rpcKnown: !!(base && er) };
}

/** What to tell whoever copies the verify command (verifyCommand's answer). */
export const verifyNote = cmd => (cmd?.rpcKnown
  ? L`verify はゲートウェイの記録を、ベース層と ER の RPC から自分で読み直して照合します。`
  : L`verify はゲートウェイのほかに、シーズンのベース層と ER の RPC を読みます。このサイトは運営の RPC を公開していないので、<base RPC> と <ER RPC> には同じクラスター（と ER）の RPC の URL を入れてください（運営が公開 RPC を指定しているときは、ここに表示されます）。`);

/**
 * Text that follows the display language, for a constant other modules
 * import (chainplay.mjs NO_KEY, rules.mjs AMM_FEE_TEXT…): call it for the
 * text of the moment, or use it where a string is expected (html``, a
 * template literal, toast, textContent) — it reads as that same text.
 */
export const liveText = fn => Object.assign(() => fn(), { toString: fn, valueOf: fn, toJSON: fn, [Symbol.toPrimitive]: () => fn() });

/** The separator of a short list (office names, verbs): 「・」 in Japanese. */
export const listSep = () => L`・`;

/**
 * A short notice at the top of the screen. `text` is plain text (never
 * parsed as markup); `action` adds one button ({label, run}).
 */
export function toast(text, kind = '', action = null) {
  const box = $('#notifications');
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  const span = document.createElement('span');
  span.textContent = text;
  el.appendChild(span);
  if (action) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = action.label;
    b.onclick = () => { action.run(); el.remove(); };
    el.appendChild(b);
  }
  box.prepend(el);
  while (box.children.length > 3) box.lastChild.remove();
  setTimeout(() => el.remove(), kind === 'error' || kind === 'war' ? 8000 : 4500);
}

/**
 * Rejection handler for background tasks (intervals, fire-and-forget loads):
 * logs the first failure only, so a server that is down does not flood the
 * console every beat. One handler per task.
 */
export function logOnce(label) {
  let logged = false;
  return e => { if (!logged) { logged = true; console.warn(`${label}:`, e); } };
}

/**
 * Run `fn` one call at a time: a call while one is running schedules exactly
 * one more run after it, and every caller's promise settles after the run
 * that covers its request. Used for polling, so an older response can never
 * be applied after a newer one.
 */
export function singleFlight(fn) {
  let running = null, again = false;
  return function run() {
    if (running) { again = true; return running; }
    running = (async () => {
      try { do { again = false; await fn(); } while (again); }
      finally { running = null; }
    })();
    return running;
  };
}
