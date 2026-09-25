// Small helpers shared by the player client, the spectator view and the map.
//
// HTML is built with the `html` tagged template: every interpolated value is
// escaped unless it is itself the result of `html`/`raw` (or an array of
// those). Build markup only with `html`, never with plain template strings,
// so server text (member names, errors, chronicle lines) cannot inject markup.

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
export const usdcFixed = x => (Number(x ?? 0) / 1e6).toFixed(2);
/** abcd…wxyz for hashes and keys. */
export const short = (s, head = 4, tail = 4, empty = '') => (s ? `${s.slice(0, head)}…${s.slice(-tail)}` : empty);
/** Troops arrive ×10 (units) and display with one decimal. */
export const troops = t => (t / 10).toFixed(1);
export const hexDist = (a, b) => (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;

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
