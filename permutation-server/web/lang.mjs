// The display language: Japanese (the source language, written inline in
// every module) or English (dictionaries in lang/, keyed by the Japanese).
//
// Marking text (see lang/GLOSSARY.md for the English of every game term):
//   L`国民 ${n}人`        plain text. In Japanese: exactly what the template
//                         literal would give. In English: the entry for the
//                         key "国民 {0}人" (values numbered {0}, {1}… in
//                         order; an English entry may reorder them), else
//                         the Japanese. The result is a string, so html``
//                         escapes it like any other value.
//   Lh`<b>${name}</b>が参加` markup: the literal parts are trusted markup (as in
//                         html``) and belong to the key "<b>{0}</b>が参加";
//                         the values are escaped exactly as html`` escapes
//                         them. Returns html`` markup. Use it only when the
//                         markup sits inside a sentence; markup around a
//                         whole sentence stays outside: html`<b>${L`…`}</b>`.
//   t('国民 {0}人', n)    the same as L for a key that is not a template
//                         literal (a table of keys, a key chosen at run time).
// Never evaluate L/Lh/t at module top level (the language can change): wrap
// such constants in functions, or use lazyTable({ k: () => L`…` }).
//
// Static markup (index.html, spectate.html): data-i18n on an element
// translates its text; the key is its Japanese content with whitespace
// collapsed, and each child element stands in the key as {0}, {1}… in order
// (the child elements themselves are kept and moved, so ids inside stay
// live). data-i18n="…" gives the key explicitly instead. data-i18n-attr=
// "title aria-label placeholder" translates those attributes (key: their
// Japanese value). Script must not write into such an element's own text
// (its children are fine): the static text comes back on a language switch.
//
// Dictionaries: lang/en-core.mjs (shared words) and one file per group
// (lang/en-{lobby,play,inspector,drawers,pages}.mjs), each
// `export default { 'Japanese key': 'English', … }`. A value may be a
// function of the values that returns the English template, for plurals:
//   '国民 {0}人': n => plural(n, '{0} member', '{0} members')
// (plural is in lang/helpers.mjs). The same key in two files must have the
// same English (a test checks it).
//
// On a switch (setLang), in this order: the choice is saved (localStorage
// 'ps-lang'), <html lang> is set, the static markup is translated, the
// toggle buttons are relabelled, then every onLangChange listener runs
// (state.mjs re-renders every part; a screen that renders outside the
// render scheduler registers its own). No reload.
import { html } from './util.mjs';
import { plural } from './lang/helpers.mjs';
import core from './lang/en-core.mjs';
import lobby from './lang/en-lobby.mjs';
import play from './lang/en-play.mjs';
import inspector from './lang/en-inspector.mjs';
import drawers from './lang/en-drawers.mjs';
import pages from './lang/en-pages.mjs';

export { plural };
export const LANGS = ['ja', 'en'];
export const STORAGE_KEY = 'ps-lang';

/** The English dictionaries by file (a test checks they agree with each other). */
export const EN_GROUPS = { core, lobby, play, inspector, drawers, pages };
/** The merged English dictionary: Japanese key → English template (string or function). */
export const EN = Object.assign(Object.create(null), core, lobby, play, inspector, drawers, pages);

// ================================================================== the current language
const store = {
  get() { try { return globalThis.localStorage?.getItem(STORAGE_KEY) ?? null; } catch { return null; } },
  set(v) { try { globalThis.localStorage?.setItem(STORAGE_KEY, v); } catch { /* private mode: this page only */ } },
};

/**
 * The starting language: the saved choice, else the browser's language
 * ('ja…' → Japanese, anything else → English). Outside a browser (node:
 * tests, tools) the source language, Japanese.
 */
export function detectLang({ stored = null, language = '', browser = true } = {}) {
  if (LANGS.includes(stored)) return stored;
  if (!browser) return 'ja';
  return String(language ?? '').toLowerCase().startsWith('ja') ? 'ja' : 'en';
}

const inBrowser = !globalThis.process?.versions?.node && typeof globalThis.document !== 'undefined';
let current = detectLang({ stored: store.get(), language: globalThis.navigator?.language, browser: inBrowser });

/** 'ja' or 'en'. */
export const lang = () => current;
/** The locale for numbers and dates in the current language. */
export const locale = () => (current === 'en' ? 'en-US' : 'ja-JP');
/** A number in the current language's format. */
export const fmtNum = (n, opts) => Number(n ?? 0).toLocaleString(locale(), opts);
/** A date/time (Date, ms or ISO string) in the current language's format. */
export const fmtDateTime = (d, opts) => new Date(d).toLocaleString(locale(), opts);

// ================================================================== keys and lookup
const NEWLINE_RUN = /[ \t\r\f]*\n[ \t\n\r\f]*/g;
/** A key as written in code: a line break and the indentation around it read as one space. */
export const normalizeKey = s => String(s).replace(NEWLINE_RUN, ' ');
/** The key of a template's literal parts: joined with {0}, {1}… */
export const templateKey = strings => normalizeKey(strings.reduce((out, s, i) => out + (i ? `{${i - 1}}` : '') + s, ''));
/** The key of static markup text: HTML whitespace collapsed, trimmed. */
export const staticKey = s => String(s).replace(/[ \t\n\r\f]+/g, ' ').trim();

const keyCache = new WeakMap(); // a call site's strings array → its key
function keyOf(strings) {
  let k = keyCache.get(strings);
  if (k === undefined) { k = templateKey([...strings].map((s, i) => s ?? strings.raw[i])); keyCache.set(strings, k); }
  return k;
}

const DEV = (() => {
  try { const h = globalThis.location?.hostname ?? ''; return h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h.endsWith('.localhost'); } catch { return false; }
})();
const missed = new Set();
/** Keys asked for in English that have no entry (they showed in Japanese). */
export const missingKeys = () => [...missed];
function lookup(key, args) {
  const v = EN[key];
  if (v === undefined) {
    if (!missed.has(key)) { missed.add(key); if (DEV) console.warn('[lang] no English for:', JSON.stringify(key)); }
    return undefined;
  }
  return typeof v === 'function' ? String(v(...args)) : v;
}

const PLACEHOLDER = /\{(\d+)\}/g;
/** Fill {0}, {1}… with `args` (a placeholder without a value stays as written). */
export const format = (template, args) => template.replace(PLACEHOLDER, (m, i) => (+i < args.length ? String(args[+i]) : m));

/** Plain text in the current language (see the top of this file). */
export function L(strings, ...values) {
  if (current === 'en') {
    const en = lookup(keyOf(strings), values);
    if (en !== undefined) return format(en, values);
  }
  let out = strings[0];
  for (let i = 0; i < values.length; i++) out += String(values[i]) + strings[i + 1];
  return out;
}

/** Markup in the current language: literal parts trusted, values escaped as html`` does. */
export function Lh(strings, ...values) {
  if (current === 'en') {
    const en = lookup(keyOf(strings), values);
    if (en !== undefined) {
      const parts = [], vals = [];
      let last = 0;
      for (const m of en.matchAll(PLACEHOLDER)) {
        if (+m[1] >= values.length) continue; // no such value: stays as text
        parts.push(en.slice(last, m.index)); vals.push(values[+m[1]]); last = m.index + m[0].length;
      }
      parts.push(en.slice(last));
      return html(parts, ...vals);
    }
  }
  return html(strings, ...values);
}

/** A key that is not a template literal: t('国民 {0}人', n). */
export function t(key, ...args) {
  if (current === 'en') {
    const en = lookup(normalizeKey(key), args);
    if (en !== undefined) return format(en, args);
  }
  return format(String(key), args);
}

/**
 * A table whose entries follow the language: `ja` and `en` have the same
 * keys (or indices); reading an entry gives the English one while English
 * is shown (the Japanese one when English lacks it). Keys, entries and
 * array methods work as on `ja`.
 */
export function twin(ja, en) {
  return new Proxy(ja, {
    get(target, k, receiver) {
      if (current === 'en' && Object.prototype.hasOwnProperty.call(en, k)) return en[k];
      return Reflect.get(target, k, receiver);
    },
  });
}

/** A table of thunks read as values: lazyTable({ Vote: () => L`投票` }).Vote → the text now. */
export function lazyTable(thunks) {
  return new Proxy(thunks, {
    get(target, k, receiver) {
      const v = Reflect.get(target, k, receiver);
      return typeof v === 'function' && Object.prototype.hasOwnProperty.call(target, k) ? v() : v;
    },
  });
}

// ================================================================== static markup
const texts = new WeakMap(); // element → {nodes, kids, key, shown}
const attrs = new WeakMap(); // element → {attribute: Japanese value}

function applyText(el) {
  let o = texts.get(el);
  if (!o) {
    const nodes = [...el.childNodes];
    const kids = nodes.filter(n => n.nodeType === 1);
    let k = 0;
    const derived = nodes.map(n => (n.nodeType === 3 ? n.nodeValue : n.nodeType === 1 ? `{${k++}}` : '')).join('');
    o = { nodes, kids, key: staticKey(el.getAttribute('data-i18n') || derived), shown: 'ja' };
    texts.set(el, o);
  }
  const en = current === 'en' ? lookup(o.key, []) : undefined;
  const want = en === undefined ? 'ja' : 'en';
  if (want === 'ja') {
    if (o.shown !== 'ja') { el.replaceChildren(...o.nodes); o.shown = 'ja'; }
    return;
  }
  const doc = el.ownerDocument ?? globalThis.document;
  const used = new Set(), out = [];
  let last = 0;
  for (const m of en.matchAll(PLACEHOLDER)) {
    const kid = o.kids[+m[1]];
    if (!kid || used.has(kid)) continue;
    if (m.index > last) out.push(doc.createTextNode(en.slice(last, m.index)));
    out.push(kid); used.add(kid); last = m.index + m[0].length;
  }
  if (last < en.length) out.push(doc.createTextNode(en.slice(last)));
  for (const kid of o.kids) if (!used.has(kid)) out.push(kid); // never lose an element (code may hold it)
  el.replaceChildren(...out);
  o.shown = 'en';
}

function applyAttrs(el) {
  let o = attrs.get(el);
  if (!o) {
    o = {};
    for (const a of String(el.getAttribute('data-i18n-attr') || '').split(/[\s|,]+/).filter(Boolean)) {
      const v = el.getAttribute(a);
      if (v !== null && v !== undefined) o[a] = v;
    }
    attrs.set(el, o);
  }
  for (const [a, ja] of Object.entries(o)) {
    const en = current === 'en' ? lookup(staticKey(ja), []) : undefined;
    el.setAttribute(a, en ?? ja);
  }
}

/**
 * Translate the static markup under `root` (and `root` itself): elements
 * with data-i18n / data-i18n-attr. The Japanese is remembered on first
 * sight, so switching back restores it exactly. Runs on load and on every
 * switch for the whole document; call it for markup added later.
 */
export function applyStatic(root = globalThis.document) {
  if (!root?.querySelectorAll) return;
  const each = (sel, fn) => {
    if (root.matches?.(sel)) fn(root);
    for (const el of root.querySelectorAll(sel)) fn(el);
  };
  each('[data-i18n]', applyText);
  each('[data-i18n-attr]', applyAttrs);
}

// ================================================================== the toggle
const TOGGLE_STYLE = 'width:auto;min-width:34px;padding:0 9px;font:600 11.5px/1 system-ui,sans-serif;letter-spacing:.3px';
/** What the toggle shows: the language it switches to, named in that language. */
const toggleText = () => (current === 'ja'
  ? { label: 'EN', title: 'Switch to English', lang: 'en' }
  : { label: '日本語', title: '日本語に切り替える', lang: 'ja' });

/**
 * The language toggle as html`` markup, for any screen (the top bar, the
 * lobby, the registration dialog, the spectator page). Clicks are handled
 * here for every [data-lang-toggle] on the page; nothing else to wire.
 */
export function langToggleHtml({ className = 'icon-btn' } = {}) {
  const x = toggleText();
  return html`<button type="button" class="${className} lang-toggle" data-lang-toggle lang="${x.lang}" title="${x.title}" aria-label="${x.title}" style="${TOGGLE_STYLE}">${x.label}</button>`;
}

function syncToggle(b) {
  const x = toggleText();
  b.textContent = x.label;
  b.title = x.title;
  b.setAttribute?.('aria-label', x.title);
  b.setAttribute?.('lang', x.lang);
}

/** Put the toggle first in `box` unless it already holds one; returns the button. */
export function mountLangToggle(box, { className = 'icon-btn' } = {}) {
  if (!box) return null;
  let b = box.querySelector?.('[data-lang-toggle]');
  if (!b) {
    b = (box.ownerDocument ?? globalThis.document).createElement('button');
    b.type = 'button';
    b.className = `${className} lang-toggle`;
    if (b.dataset) b.dataset.langToggle = '';
    if (b.style) b.style.cssText = TOGGLE_STYLE;
    box.prepend(b);
  }
  syncToggle(b);
  return b;
}

// ================================================================== switching
const listeners = new Set();
/** Run `fn(lang)` after every switch; returns the unsubscribe function. */
export function onLangChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }

function applyDocument() {
  const doc = globalThis.document;
  if (!doc) return;
  try {
    if (doc.documentElement) doc.documentElement.lang = current;
    applyStatic(doc);
    for (const b of doc.querySelectorAll?.('[data-lang-toggle]') ?? []) syncToggle(b);
  } catch (e) { console.error('lang:', e); }
}

/** Switch the display language ('ja' | 'en'): saved, and everything re-rendered in place. */
export function setLang(l) {
  if (!LANGS.includes(l) || l === current) return;
  current = l;
  store.set(l);
  applyDocument();
  for (const fn of listeners) {
    try { fn(l); } catch (e) { console.error('lang listener:', e); }
  }
}
export const toggleLang = () => setLang(current === 'ja' ? 'en' : 'ja');

// ================================================================== boot (browser only)
// Module scripts run after the document is parsed: translate the static
// markup now, and take the toggle's clicks for the whole page.
if (inBrowser) {
  applyDocument();
  globalThis.document.addEventListener?.('click', e => {
    const b = e.target?.closest?.('[data-lang-toggle]');
    if (!b) return;
    e.preventDefault();
    toggleLang();
  });
}
