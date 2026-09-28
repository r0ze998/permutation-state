// The per-shot assertions of the screenshot suite (web design §13.2,
// contract §13.6 E7), as functions evaluated in the page (no closures:
// Playwright serialises their source), plus the axe run.
//
//   layout(phone)   no horizontal overflow; the banner, main and
//                   navigation landmarks and the bell chip visible; every
//                   visible interactive element at least 44 × 44 CSS px at
//                   phone widths (24 × 24 on desktop); a radio or checkbox
//                   is measured by its label, a disabled control too
//   japanese()      rendered text and accessible names in English: no
//                   Japanese glyph except inside an element marked
//                   lang="ja" (the language toggle's "日本語") or
//                   data-name (proper names, user text)
//   placeholders()  no rendered "null", "undefined" or "NaN" (or an empty
//                   " · · " separator) in the visible text or accessible
//                   names (wave-5 review: "region null" shipped in the
//                   bell sheet with 72 shots and 0 findings)
//   doubled()       English only: no doubled word ("in in", "the the")
//   axe             axe-core, violations of impact serious or critical
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
export const AXE_SOURCE = readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');

/** Evaluated in the page: `{overflow, landmarks, bellChip, small: [...]}`. */
export function layout(phone) {
  const min = phone ? 44 : 24;
  const visible = el => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return false;
    const s = getComputedStyle(el);
    if (s.visibility === 'hidden' || s.display === 'none') return false;
    return !el.closest('[hidden], details:not([open]) > :not(summary)');
  };
  const doc = document.documentElement;
  const landmark = sel => { const el = document.querySelector(sel); return !!el && visible(el); };
  const chip = document.getElementById('bell-chip');
  const small = [];
  const sel = 'a[href], button, input:not([type=hidden]), select, textarea, summary, [role=button], [tabindex]:not([tabindex="-1"])';
  for (const el of document.querySelectorAll(sel)) {
    if (el.tagName === 'CANVAS' || !visible(el)) continue;
    const box = (el.type === 'radio' || el.type === 'checkbox') && el.closest('label') ? el.closest('label') : el;
    const r = box.getBoundingClientRect();
    if (r.width + 0.5 < min || r.height + 0.5 < min) {
      small.push({ tag: el.tagName.toLowerCase(), id: el.id || null, act: el.dataset?.act ?? null, cls: String(el.className || ''), text: (el.textContent || el.value || '').trim().slice(0, 40), w: Math.round(r.width), h: Math.round(r.height) });
    }
  }
  // Content cut off at the sides: an element beyond the viewport that no
  // horizontally scrolling box holds (the body clips, so scrollWidth alone
  // cannot see it); only the outermost such element is reported.
  const clipped = [];
  const out = el => { const r = el.getBoundingClientRect(); return r.width > 0 && (r.right > window.innerWidth + 1 || r.left < -1); };
  const scroller = el => { for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) { const s = getComputedStyle(a); if (/(auto|scroll)/.test(s.overflowX) && a.scrollWidth > a.clientWidth + 1 && a.getBoundingClientRect().right <= window.innerWidth + 1) return true; } return false; };
  for (const el of document.body.querySelectorAll('*')) {
    if (!visible(el) || !out(el) || (el.parentElement && el.parentElement !== document.body && out(el.parentElement)) || scroller(el)) continue;
    const r = el.getBoundingClientRect();
    clipped.push({ tag: el.tagName.toLowerCase(), id: el.id || null, cls: String(el.className || '').slice(0, 40), text: (el.textContent || '').trim().slice(0, 40), left: Math.round(r.left), right: Math.round(r.right) });
  }
  // The page fills the viewport: the map and panel take the free height (phones: the tab bar at the bottom edge).
  const nav = document.querySelector('nav.tabs'), mainEl = document.querySelector('main');
  const bottom = phone ? nav?.getBoundingClientRect().bottom : Math.max(document.getElementById('frontier-map')?.getBoundingClientRect().bottom ?? 0, document.getElementById('panel')?.getBoundingClientRect().bottom ?? 0);
  const fill = { bottom: Math.round(bottom ?? 0), innerHeight: window.innerHeight, main: Math.round(mainEl?.getBoundingClientRect().height ?? 0) };
  return {
    fill,
    clipped,
    overflow: { scrollWidth: doc.scrollWidth, innerWidth: window.innerWidth, bodyScrollWidth: document.body.scrollWidth },
    landmarks: { banner: landmark('header, [role=banner]'), main: landmark('main, [role=main]'), navigation: landmark('nav, [role=navigation]') },
    bellChip: { visible: !!chip && visible(chip), text: chip?.textContent ?? '' },
    small,
  };
}

/** Evaluated in the page: Japanese glyphs in English text and names, as `[{where, text}]`. */
export function japanese() {
  const JP = /[　-ヿ㐀-䶿一-鿿豈-﫿＀-￯]/;
  const exempt = el => !!el?.closest?.('[lang="ja"], [data-name]');
  const shown = el => {
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (s.display === 'none' || s.visibility === 'hidden' || e.hidden) return false;
    }
    return true;
  };
  const out = [];
  if (JP.test(document.title)) out.push({ where: 'title', text: document.title });
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.parentElement;
    if (!el || ['SCRIPT', 'STYLE', 'NOSCRIPT'].includes(el.tagName) || exempt(el) || !shown(el)) continue;
    if (JP.test(n.data)) out.push({ where: el.tagName.toLowerCase() + (el.id ? `#${el.id}` : ''), text: n.data.trim().slice(0, 60) });
  }
  for (const el of document.querySelectorAll('[aria-label], [title], [placeholder], [alt]')) {
    if (exempt(el) || !shown(el)) continue;
    for (const a of ['aria-label', 'title', 'placeholder', 'alt']) {
      const v = el.getAttribute(a);
      if (v && JP.test(v)) out.push({ where: `${el.tagName.toLowerCase()}[${a}]`, text: v.slice(0, 60) });
    }
  }
  return out;
}

/** Evaluated in the page: placeholder text a template leaked, as `[{where, text}]`. */
export function placeholders() {
  const BAD = /(^|[^A-Za-z0-9_])(null|undefined|NaN)([^A-Za-z0-9_]|$)|·\s*·/;
  const shown = el => {
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (s.display === 'none' || s.visibility === 'hidden' || e.hidden) return false;
    }
    return true;
  };
  const out = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.parentElement;
    if (!el || ['SCRIPT', 'STYLE', 'NOSCRIPT'].includes(el.tagName) || el.closest('[data-name]') || !shown(el)) continue;
    if (BAD.test(n.data)) out.push({ where: el.tagName.toLowerCase() + (el.id ? `#${el.id}` : ''), text: n.data.trim().slice(0, 80) });
  }
  // A leaf's text split over nodes: judge each element's own full text too.
  for (const el of document.querySelectorAll('li, p, dd, dt, h1, h2, h3, button, span, strong')) {
    if (el.closest('[data-name]') || !shown(el) || el.children.length > 4) continue;
    const t = el.textContent ?? '';
    if (BAD.test(t) && !out.some(o => t.includes(o.text))) out.push({ where: el.tagName.toLowerCase(), text: t.trim().slice(0, 80) });
  }
  for (const el of document.querySelectorAll('[aria-label], [title], [alt], [placeholder]')) {
    if (!shown(el)) continue;
    for (const a of ['aria-label', 'title', 'alt', 'placeholder']) {
      const v = el.getAttribute(a);
      if (v && BAD.test(v)) out.push({ where: `${el.tagName.toLowerCase()}[${a}]`, text: v.slice(0, 80) });
    }
  }
  return out;
}

/** Evaluated in the page (English): a word written twice in a row, as `[{where, text}]`. */
export function doubled() {
  const DUP = /\b([A-Za-z]{2,})\s+\1\b/i;
  const out = [];
  for (const el of document.querySelectorAll('li, p, dd, dt, .row, h1, h2, h3, button, label')) {
    if (el.closest('[data-name]') || el.children.length > 6) continue;
    const s = getComputedStyle(el);
    if (s.display === 'none' || s.visibility === 'hidden') continue;
    // The element's own text, without the options of a select it holds
    // (a label's "Arrival bell" and its option "Bell 44" are not a phrase).
    let t = '';
    const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, { acceptNode: n => (n.parentElement?.closest('select, option, datalist') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT) });
    for (let n = w.nextNode(); n; n = w.nextNode()) t += ` ${n.data}`;
    t = t.replace(/\s+/g, ' ');
    const m = DUP.exec(t);
    if (m) out.push({ where: el.tagName.toLowerCase(), text: t.trim().slice(0, 80) });
  }
  return out;
}

/** Run axe in the page: violations of impact serious or critical, `[{id, impact, help, nodes: [target]}]`. */
export async function axe(page) {
  if (!(await page.evaluate(() => typeof globalThis.axe === 'object'))) await page.evaluate(AXE_SOURCE);
  const r = await page.evaluate(async () => {
    const res = await globalThis.axe.run(document, { resultTypes: ['violations'] });
    return res.violations.map(v => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.slice(0, 6).map(n => n.target.join(' ')) }));
  });
  return r.filter(v => v.impact === 'serious' || v.impact === 'critical');
}
