// The web client's display language (permutation-server/web/lang.mjs, the
// English twins in i18n.mjs, the dictionaries in web/lang/): L`…` keys and
// formatting (values reordered by the English), the Japanese fallback, Lh
// escaping, static markup (data-i18n, data-i18n-attr) switched both ways,
// the saved choice, the re-render on a switch, the toggle — and a
// completeness scan of the whole client:
//  (a) every L`…` / Lh`…` / t('…') key and every data-i18n text or
//      attribute has an English entry;
//  (b) no Japanese outside those, i18n.mjs's tables and comments.
// The scan reports file:line lists. PS_LANG_ONLY=drawers/,inspector/ narrows
// it to paths under web/ that start with one of the prefixes.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as lang from '../../permutation-server/web/lang.mjs';
import * as T from '../../permutation-server/web/i18n.mjs';
import { S, registerRenderers } from '../../permutation-server/web/state.mjs';

const { L, Lh, t, EN, EN_GROUPS, setLang } = lang;
const WEB = fileURLToPath(new URL('../../permutation-server/web/', import.meta.url));

afterEach(() => setLang('ja'));

// ================================================================== L, Lh, t
test('L: Japanese is exactly the template literal; the key numbers the values', () => {
  assert.equal(lang.lang(), 'ja', 'node starts in the source language');
  for (const v of [3, 0, 'x', null, undefined, 1.5, true]) assert.equal(L`国民 ${v}人`, `国民 ${v}人`);
  const a = 'アステル', b = 12;
  assert.equal(L`${a}が${b}ティックに宣戦`, `${a}が${b}ティックに宣戦`);
  assert.equal(lang.templateKey(['', 'が', 'ティックに宣戦']), '{0}が{1}ティックに宣戦');
  // A line break and its indentation are one space in the key (not in the Japanese).
  assert.equal(lang.templateKey(['長い\n      文']), '長い 文');
});

test('L in English: placeholders in order, reordered, functions and plurals', () => {
  const add = { 'テスト国民 {0}人': n => lang.plural(n, '{0} test member', '{0} test members'), '{0}がテスト{1}に宣戦': '{1} is attacked by {0}', 'テスト{0}と{1}': 'test {0} and {1}' };
  Object.assign(EN, add);
  try {
    setLang('en');
    assert.equal(L`テスト国民 ${1}人`, '1 test member');
    assert.equal(L`テスト国民 ${'1,234'}人`, '1,234 test members');
    assert.equal(L`${'Aster'}がテスト${'Borealis'}に宣戦`, 'Borealis is attacked by Aster');
    assert.equal(L`テスト${1}と${2}`, 'test 1 and 2');
    assert.equal(t('テスト{0}と{1}', 'a', 'b'), 'test a and b');
    setLang('ja');
    assert.equal(L`テスト${1}と${2}`, 'テスト1と2');
    assert.equal(t('テスト{0}と{1}', 'a', 'b'), 'テストaとb');
  } finally {
    for (const k of Object.keys(add)) delete EN[k];
  }
});

test('L in English: a missing key falls back to the Japanese and is recorded once', () => {
  setLang('en');
  assert.equal(L`辞書にない文 ${7}`, '辞書にない文 7');
  assert.equal(L`辞書にない文 ${8}`, '辞書にない文 8');
  assert.equal(lang.missingKeys().filter(k => k === '辞書にない文 {0}').length, 1);
  assert.equal(t('辞書にないキー{0}', 5), '辞書にないキー5');
});

test('Lh: literal parts are markup, values escaped; the English may reorder them', () => {
  EN['<b>{0}</b>がテスト{1}件'] = '{1} tests by <b>{0}</b>';
  try {
    const evil = '<img src=x onerror=alert(1)>';
    assert.equal(String(Lh`<b>${evil}</b>がテスト${3}件`), '<b>&lt;img src=x onerror=alert(1)&gt;</b>がテスト3件');
    setLang('en');
    assert.equal(String(Lh`<b>${evil}</b>がテスト${3}件`), '3 tests by <b>&lt;img src=x onerror=alert(1)&gt;</b>');
    // Missing in English: the Japanese markup, still escaped.
    assert.equal(String(Lh`<i>${'<x>'}</i>未訳`), '<i>&lt;x&gt;</i>未訳');
  } finally {
    delete EN['<b>{0}</b>がテスト{1}件'];
  }
});

test('lazyTable and twin read in the current language', () => {
  const verbs = lang.lazyTable({ Vote: () => (lang.lang() === 'en' ? 'Vote' : '投票') });
  const tw = lang.twin({ a: 'あ', b: 'い' }, { a: 'A' });
  assert.equal(verbs.Vote, '投票');
  assert.equal(tw.a, 'あ');
  setLang('en');
  assert.equal(verbs.Vote, 'Vote');
  assert.deepEqual(Object.entries(tw), [['a', 'A'], ['b', 'い']], 'a missing English entry stays Japanese');
});

// ================================================================== i18n.mjs in English
test('i18n tables follow the language (callers unchanged)', () => {
  assert.equal(T.ROLE_JA.Science, '科学官');
  setLang('en');
  assert.equal(T.ROLE_JA.Science, 'Science Officer');
  assert.deepEqual(T.PATH_JA.map(x => x), ['Hegemony', 'Prosperity', 'Science', 'Concord']);
  assert.equal(T.PATH_JA.length, 4);
  assert.deepEqual(Object.entries(T.FOCUS)[0], ['Balanced', 'Balanced']);
  assert.equal(T.TECH.IronWorking, 'Iron Working');
  assert.equal(T.civName('Aster'), 'Aster');
  assert.equal(T.cityName(0), 'Lana');
  assert.equal(T.cityName(21), 'Vel 2');
  assert.deepEqual(T.phaseOf(130), [120, 'Crisis', 'CRISIS']);
  assert.equal(T.itemName({ kind: 'Troops', unit: 'Knight', n: 3 }), 'Knight ×3');
  assert.equal(T.goodName({ kind: 'Horses' }), 'Horses');
  assert.equal(T.blockedText({ code: 'NotEnoughGold', need: 5, have: 2 }), 'Not enough gold (need 5, have 2)');
  assert.equal(T.blockedText({ code: 'NeedsTech', tech: 'Writing' }), 'Requires the tech “Writing”');
  assert.equal(T.blockedText({ code: 'SomethingNew' }), 'SomethingNew');
  assert.equal(T.standingText({ kind: 'Retreat', ratioBps: 15000 }), 'Retreat · when the enemy is over 1.5×');
  assert.equal(T.errorText({ code: 'SeasonFull' }), 'This season is full.');
  assert.equal(T.errorText('Error: custom program error: TickFrozen'), T.CHAIN_ERROR_JA.TickFrozen);
  assert.equal(T.errorText('unknown thing'), 'unknown thing');
  setLang('ja');
  assert.equal(T.errorText({ code: 'SeasonFull' }), 'このシーズンは満員です。');
});

test('i18n: every enum table has an English twin for every key', () => {
  const JP = /[\u3040-\u30ff\u4e00-\u9fff]/;
  const tables = ['CIV_NAMES', 'ROLE_JA', 'PATH_JA', 'MERIT_JA', 'PROPOSAL_KIND', 'TERRAIN', 'RESOURCE', 'UNIT', 'BUILDING', 'BUILDING_EFFECT', 'TECH', 'TECH_UNLOCK',
    'FOCUS', 'SPECIALTY', 'SPECIALTY_BONUS', 'RELATION', 'DIPLO_ACTION', 'CHAIN_ERROR_JA', 'MERIT_WHAT'];
  setLang('en');
  for (const name of tables) for (const [k, v] of Object.entries(T[name])) assert.doesNotMatch(String(v), JP, `${name}.${k}`);
  for (const [ja, en] of T.PHASES) { assert.doesNotMatch(ja, JP); assert.doesNotMatch(en, JP); }
  for (let id = 0; id < 20; id++) assert.doesNotMatch(T.cityName(id), JP);
  for (const code of ['NeedsTech', 'TooCloseToCity', 'ProtectedCapital', 'UnknownUnit', 'BadContract', 'NeedsConsent']) {
    assert.doesNotMatch(T.blockedText({ code, tech: 'Writing', distance: 1, min: 3, until: 5 }), JP, code);
  }
  for (const kind of ['AutoDefend', 'Patrol', 'QueueRepeat', 'AutoPurchase', 'Clear']) assert.doesNotMatch(T.standingText({ kind, radius: 1, route: [[0, 0]], on: true, maxGold: 5 }), JP);
});

test('chronicleText in English', () => {
  setLang('en');
  const cases = [
    ['war|Aster declares war on Borealis, breaking a pact', 'Aster declares war on Borealis (breaking a pact)'],
    ['tech|Cinder discovers IronWorking', 'Cinder discovers Iron Working'],
    ['recall|Borealis: the acting official becomes science officer (was carol)', 'Borealis: the caretaker becomes Science Officer after a recall (was carol)'],
    ['gov|First election — Dunmar: general alice, science officer acting', 'First election — Dunmar: alice (General), the caretaker (Science Officer)'],
    ['gov|Ember adopts 1 proposal', 'Ember adopts 1 proposal'],
    ['gov|Ember adopts 3 proposals', 'Ember adopts 3 proposals'],
    ['milestone|Fjordal reaches Hegemony 2', 'Fjordal reaches Hegemony tier 2'],
    ['milestone|Aster loses Glory 1', 'Aster loses Glory tier 1'],
    ['bounty|Aster conquers the home of agent-7 of Borealis, an operator AI member: bounty 25 USDC', 'Aster takes the home city of agent-7 of Borealis, an operator AI member: bounty 25 USDC'],
    ['other|Something unrecognised happens', 'Something unrecognised happens'],
  ];
  for (const [line, want] of cases) assert.equal(T.chronicleText(line)[1], want, line);
});

// ================================================================== the choice, the switch, the toggle
test('detectLang: saved choice, then the browser language; outside a browser Japanese', () => {
  assert.equal(lang.detectLang({ stored: 'en', language: 'ja-JP' }), 'en');
  assert.equal(lang.detectLang({ stored: 'ja', language: 'en-US' }), 'ja');
  assert.equal(lang.detectLang({ stored: 'fr', language: 'ja-JP' }), 'ja');
  assert.equal(lang.detectLang({ stored: null, language: 'ja' }), 'ja');
  assert.equal(lang.detectLang({ stored: null, language: 'en-GB' }), 'en');
  assert.equal(lang.detectLang({ stored: null, language: 'de' }), 'en');
  assert.equal(lang.detectLang({ stored: null, language: '' }), 'en');
  assert.equal(lang.detectLang({ stored: null, language: 'en-US', browser: false }), 'ja');
});

test('the choice is saved under ps-lang and read back by a fresh load', async () => {
  const saved = new Map();
  const had = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => saved.get(k) ?? null, setItem: (k, v) => saved.set(k, String(v)), removeItem: k => saved.delete(k) } });
  try {
    const a = await import(new URL('../../permutation-server/web/lang.mjs?load=a', import.meta.url));
    assert.equal(a.lang(), 'ja');
    a.setLang('en');
    assert.deepEqual([...saved], [['ps-lang', 'en']], 'only the language key');
    const b = await import(new URL('../../permutation-server/web/lang.mjs?load=b', import.meta.url));
    assert.equal(b.lang(), 'en');
    b.setLang('xx');
    assert.equal(b.lang(), 'en', 'unknown languages are ignored');
    // Storage that throws (private mode): the switch still works for this page.
    globalThis.localStorage.setItem = () => { throw new Error('quota'); };
    b.setLang('ja');
    assert.equal(b.lang(), 'ja');
  } finally {
    if (had) Object.defineProperty(globalThis, 'localStorage', had); else delete globalThis.localStorage;
  }
});

test('a switch re-renders every part (state.mjs) and runs the listeners once', async () => {
  const ran = [];
  registerRenderers([['langProbeA', () => ran.push(['A', lang.lang()])], ['langProbeB', () => ran.push(['B', lang.lang()])]]);
  const heard = [];
  const off = lang.onLangChange(l => heard.push(l));
  S.view = {};
  try {
    setLang('en');
    setLang('en'); // no change: nothing runs
    await new Promise(r => queueMicrotask(r));
    assert.deepEqual(ran, [['A', 'en'], ['B', 'en']]);
    assert.deepEqual(heard, ['en']);
  } finally {
    off();
    S.view = null;
  }
});

// A minimal DOM: elements, text nodes, attribute selectors.
class FText { constructor(v) { this.nodeType = 3; this.nodeValue = v; this.parentNode = null; } get textContent() { return this.nodeValue; } }
class FEl {
  constructor(doc, tag, attrs = {}, kids = []) {
    Object.assign(this, { nodeType: 1, tagName: tag.toUpperCase(), ownerDocument: doc, attrs: { ...attrs }, childNodes: [], parentNode: null, style: {} });
    this.dataset = new Proxy({}, { set: (o, k, v) => { this.attrs[`data-${k.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`] = String(v); return true; } });
    for (const k of kids) this.appendChild(typeof k === 'string' ? new FText(k) : k);
  }
  getAttribute(a) { return Object.prototype.hasOwnProperty.call(this.attrs, a) ? this.attrs[a] : null; }
  setAttribute(a, v) { this.attrs[a] = String(v); }
  get title() { return this.getAttribute('title') ?? ''; }
  set title(v) { this.setAttribute('title', v); }
  get lang() { return this.getAttribute('lang') ?? ''; }
  set lang(v) { this.setAttribute('lang', v); }
  appendChild(n) { if (n.parentNode) n.parentNode.childNodes = n.parentNode.childNodes.filter(x => x !== n); n.parentNode = this; this.childNodes.push(n); return n; }
  prepend(n) { this.appendChild(n); this.childNodes.unshift(this.childNodes.pop()); }
  replaceChildren(...ns) { for (const n of this.childNodes) n.parentNode = null; this.childNodes = []; for (const n of ns) this.appendChild(n); }
  get textContent() { return this.childNodes.map(n => n.textContent).join(''); }
  set textContent(v) { this.replaceChildren(new FText(String(v))); }
  matches(sel) { const m = /^\[([\w-]+)\]$/.exec(sel); return !!m && this.getAttribute(m[1]) !== null; }
  querySelectorAll(sel) { const out = []; const walk = e => { for (const c of e.childNodes) if (c.nodeType === 1) { if (c.matches(sel)) out.push(c); walk(c); } }; walk(this); return out; }
  querySelector(sel) { return this.querySelectorAll(sel)[0] ?? null; }
}
function fakeDocument() {
  const doc = { createTextNode: v => new FText(v), createElement: tag => new FEl(doc, tag) };
  doc.documentElement = new FEl(doc, 'html', { lang: 'ja' });
  doc.body = new FEl(doc, 'body');
  doc.documentElement.appendChild(doc.body);
  doc.querySelectorAll = sel => doc.documentElement.querySelectorAll(sel);
  doc.querySelector = sel => doc.documentElement.querySelector(sel);
  return doc;
}

test('applyStatic: data-i18n text (child elements as {0}…), explicit keys, attributes; switching back restores', () => {
  const doc = fakeDocument();
  const mode = new FEl(doc, 'span', { id: 'mode' }, ['ローカル']);
  const count = new FEl(doc, 'span', { id: 'count' }, ['6']);
  const desc = new FEl(doc, 'p', { 'data-i18n': '' }, [mode, '。国は\n    ', count, 'つ、テストです。']);
  const btn = new FEl(doc, 'button', { 'data-i18n': 'テスト閉じる', title: 'テスト拡大', 'aria-label': 'テスト拡大', 'data-i18n-attr': 'title aria-label' }, ['テスト閉じる']);
  const plain = new FEl(doc, 'h2', { 'data-i18n': '' }, ['訳のない見出し']);
  doc.body.replaceChildren(desc, btn, plain);
  const add = { '{0}。国は {1}つ、テストです。': '{0}. There are {1} test nations.', 'テスト閉じる': 'Test close', 'テスト拡大': 'Test zoom in' };
  Object.assign(EN, add);
  const had = globalThis.document;
  globalThis.document = doc;
  try {
    lang.applyStatic(doc); // first sight in Japanese: nothing changes
    assert.equal(desc.textContent, 'ローカル。国は\n    6つ、テストです。');
    setLang('en'); // translates the document
    assert.equal(doc.documentElement.lang, 'en');
    assert.equal(desc.textContent, 'ローカル. There are 6 test nations.');
    assert.equal(desc.childNodes[0], mode, 'the child elements themselves are kept');
    assert.equal(desc.querySelector('[id]'), mode);
    assert.equal(count.parentNode, desc);
    assert.equal(btn.textContent, 'Test close');
    assert.equal(btn.title, 'Test zoom in');
    assert.equal(btn.getAttribute('aria-label'), 'Test zoom in');
    assert.equal(plain.textContent, '訳のない見出し', 'no English: stays Japanese');
    // Script writes into a child: kept across switches.
    mode.textContent = 'オンチェーン';
    setLang('ja');
    assert.equal(doc.documentElement.lang, 'ja');
    assert.equal(desc.textContent, 'オンチェーン。国は\n    6つ、テストです。');
    assert.equal(btn.title, 'テスト拡大');
    assert.equal(btn.textContent, 'テスト閉じる');
  } finally {
    for (const k of Object.keys(add)) delete EN[k];
    globalThis.document = had;
  }
});

test('applyStatic: an English entry that drops a placeholder never loses the element', () => {
  const doc = fakeDocument();
  const kid = new FEl(doc, 'b', { id: 'k' }, ['x']);
  const el = new FEl(doc, 'p', { 'data-i18n': '' }, ['テスト前', kid, 'テスト後']);
  doc.body.appendChild(el);
  EN['テスト前{0}テスト後'] = 'dropped';
  try {
    setLang('en');
    lang.applyStatic(doc);
    assert.equal(el.textContent, 'droppedx');
    assert.equal(kid.parentNode, el);
  } finally {
    delete EN['テスト前{0}テスト後'];
  }
});

test('the toggle: "EN" while Japanese is shown, "日本語" while English is; clicks need no wiring', () => {
  assert.match(String(lang.langToggleHtml()), /data-lang-toggle[^>]*>EN<\/button>$/);
  assert.match(String(lang.langToggleHtml()), /lang="en"/);
  const doc = fakeDocument();
  const box = new FEl(doc, 'div', { class: 'top-actions' }, [new FEl(doc, 'button', { id: 'help-btn' }, ['?'])]);
  doc.body.appendChild(box);
  const b = lang.mountLangToggle(box);
  assert.equal(box.childNodes[0], b, 'first in the box');
  assert.equal(b.textContent, 'EN');
  assert.equal(b.getAttribute('data-lang-toggle'), '');
  assert.equal(lang.mountLangToggle(box), b, 'mounted once');
  const had = globalThis.document;
  globalThis.document = doc;
  try {
    setLang('en');
    assert.equal(b.textContent, '日本語', 'every toggle on the page is relabelled');
    assert.equal(b.getAttribute('lang'), 'ja');
    assert.match(String(lang.langToggleHtml({ className: 'btn small' })), /class="btn small lang-toggle"[^>]*>日本語<\/button>$/);
  } finally {
    globalThis.document = had;
  }
});

test('numbers and dates follow the language', () => {
  assert.equal(lang.locale(), 'ja-JP');
  assert.equal(lang.fmtNum(1234567.5), (1234567.5).toLocaleString('ja-JP'));
  setLang('en');
  assert.equal(lang.locale(), 'en-US');
  assert.equal(lang.fmtNum(1234567.5), '1,234,567.5');
  assert.equal(lang.fmtDateTime(Date.UTC(2026, 0, 2), { timeZone: 'UTC', year: 'numeric', month: 'short', day: 'numeric' }), 'Jan 2, 2026');
});

// ================================================================== the dictionaries
const JP = /[\u3000-\u303f\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uff01-\uff60\uff66-\uff9f]/;
const placeholders = s => [...String(s).matchAll(/\{(\d+)\}/g)].map(m => +m[1]);

test('dictionaries: English only, only the key\'s placeholders, the same English in every file', () => {
  const seen = new Map();
  for (const [group, dict] of Object.entries(EN_GROUPS)) {
    for (const [key, value] of Object.entries(dict)) {
      const where = `lang/en-${group}.mjs ${JSON.stringify(key)}`;
      assert.ok(typeof value === 'string' || typeof value === 'function', `${where}: a string or a function`);
      const n = new Set(placeholders(key)).size;
      const args = Array.from({ length: n }, () => 1);
      for (const out of typeof value === 'function' ? [value(...args), value(...args.map(() => 2))] : [value]) {
        assert.equal(typeof out, 'string', where);
        assert.doesNotMatch(out, JP, `${where}: Japanese in the English`);
        for (const p of placeholders(out)) assert.ok(p < n, `${where}: {${p}} is not in the key`);
      }
      if (seen.has(key) && typeof value === 'string') assert.equal(value, seen.get(key).value, `${where}: differs from lang/en-${seen.get(key).group}.mjs`);
      if (!seen.has(key)) seen.set(key, { group, value });
    }
  }
});

// ================================================================== the completeness scan
const BEFORE_REGEX = new Set(['return', 'typeof', 'case', 'do', 'else', 'in', 'of', 'new', 'delete', 'void', 'throw', 'instanceof', 'yield', 'await']);
const ESC = { n: '\n', t: '\t', r: '\r', b: '\b', f: '\f', v: '\v', 0: '\0' };
/** A template or string literal's value from its source text. */
const cook = raw => raw.replace(/\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|\r\n|[\s\S])/g, (_, e) => (
  e[0] === 'u' && e.length > 1 ? String.fromCodePoint(parseInt(e[1] === '{' ? e.slice(2, -1) : e.slice(1), 16))
    : e[0] === 'x' && e.length === 3 ? String.fromCharCode(parseInt(e.slice(1), 16))
      : /^(\r\n|[\n\r\u2028\u2029])$/.test(e) ? '' : ESC[e] ?? e));

function lineIndex(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') starts.push(i + 1);
  return pos => { let lo = 0, hi = starts.length - 1; while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= pos) lo = mid; else hi = mid - 1; } return lo + 1; };
}

/**
 * JS source → {keys: [{kind: 'L'|'Lh'|'t', key, line}], stray: [{line}]}:
 * L/Lh tagged templates and t('…') first arguments give keys; Japanese
 * anywhere else except comments (strings, other templates, regexes, code)
 * is stray.
 */
function scanJs(src) {
  const keys = [], stray = [], lineAt = lineIndex(src);
  let i = 0;
  const flag = (text, at) => { const k = text.search(JP); if (k >= 0) stray.push({ line: lineAt(at + k) }); };
  const isTCall = h => {
    const n = h.length;
    return n >= 2 && h[n - 1].t === 'p' && h[n - 1].v === '(' && h[n - 2].t === 'id' && h[n - 2].v === 't' && !(n >= 3 && h[n - 3].t === 'p' && h[n - 3].v === '.');
  };
  function template(tag, tArg) {
    const start = i++;
    const quasis = [];
    let raw = '', at = i;
    while (i < src.length) {
      const c = src[i];
      if (c === '\\') { raw += c + src[i + 1]; i += 2; continue; }
      if (c === '`') { i++; break; }
      if (c === '$' && src[i + 1] === '{') { quasis.push({ raw, at }); i += 2; code(true); raw = ''; at = i; continue; }
      raw += c; i++;
    }
    quasis.push({ raw, at });
    if (tag === 'L' || tag === 'Lh') {
      const key = lang.templateKey(quasis.map(q => cook(q.raw)));
      if (JP.test(key)) keys.push({ kind: tag, key, line: lineAt(start) });
    } else if (tArg && quasis.length === 1 && JP.test(quasis[0].raw)) keys.push({ kind: 't', key: lang.normalizeKey(cook(quasis[0].raw)), line: lineAt(start) });
    else for (const q of quasis) flag(q.raw, q.at);
  }
  function regex() {
    const start = i++;
    let inClass = false;
    while (i < src.length) {
      const c = src[i];
      if (c === '\\') { i += 2; continue; }
      if (c === '\n') break;
      if (c === '[') inClass = true;
      else if (c === ']') inClass = false;
      else if (c === '/' && !inClass) { i++; break; }
      i++;
    }
    while (i < src.length && /[a-z]/i.test(src[i])) i++;
    flag(src.slice(start, i), start);
  }
  function code(untilBrace) {
    const hist = [];
    const push = tok => { hist.push(tok); if (hist.length > 3) hist.shift(); };
    let depth = 0;
    while (i < src.length) {
      const c = src[i], d = src[i + 1], prev = hist[hist.length - 1];
      if (c === '/' && d === '/') { const e = src.indexOf('\n', i); i = e < 0 ? src.length : e; continue; }
      if (c === '/' && d === '*') { const e = src.indexOf('*/', i + 2); i = e < 0 ? src.length : e + 2; continue; }
      if (c === '"' || c === "'") {
        const start = i++;
        let raw = '';
        while (i < src.length && src[i] !== c && src[i] !== '\n') { if (src[i] === '\\') { raw += src[i] + src[i + 1]; i += 2; } else raw += src[i++]; }
        i++;
        if (isTCall(hist) && JP.test(raw)) keys.push({ kind: 't', key: lang.normalizeKey(cook(raw)), line: lineAt(start) });
        else flag(raw, start + 1);
        push({ t: 'str' });
        continue;
      }
      if (c === '`') { template(prev?.t === 'id' ? prev.v : null, isTCall(hist)); push({ t: 'tpl' }); continue; }
      if (c === '/') {
        const re = !prev || (prev.t === 'id' ? BEFORE_REGEX.has(prev.v) : prev.t === 'p' ? prev.v !== ')' && prev.v !== ']' : false);
        if (re) { regex(); push({ t: 're' }); } else { i++; push({ t: 'p', v: '/' }); }
        continue;
      }
      if (/[A-Za-z_$]/.test(c) || c > '\x7f') {
        const start = i;
        while (i < src.length && (/[\w$]/.test(src[i]) || src[i] > '\x7f')) i++;
        const v = src.slice(start, i);
        flag(v, start);
        push({ t: 'id', v });
        continue;
      }
      if (/[0-9]/.test(c)) { while (i < src.length && /[\w.]/.test(src[i])) i++; push({ t: 'num' }); continue; }
      if (/\s/.test(c)) { i++; continue; }
      if (c === '{') depth++;
      else if (c === '}') { if (untilBrace && depth === 0) { i++; return; } depth--; }
      push({ t: 'p', v: c });
      i++;
    }
  }
  code(false);
  return { keys, stray };
}

const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr']);
const ENTITY = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0' };
const decode = s => s.replace(/&(#[xX][0-9a-fA-F]+|#\d+|\w+);/g, (m, e) => (e[0] === '#'
  ? String.fromCodePoint(e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10)) : ENTITY[e] ?? m));

/** HTML source → a tree of {tag, attrs, attrAt, children, at} and {text, at}. */
function parseHtml(src) {
  const root = { tag: '#root', attrs: {}, attrAt: {}, children: [], at: 0 };
  const stack = [root];
  let i = 0;
  while (i < src.length) {
    if (src.startsWith('<!--', i)) { const e = src.indexOf('-->', i + 4); i = e < 0 ? src.length : e + 3; continue; }
    if (src.startsWith('<!', i)) { i = src.indexOf('>', i) + 1; continue; }
    const close = src.startsWith('</', i) && /^<\/([A-Za-z][\w-]*)\s*>/.exec(src.slice(i, i + 80));
    if (close) {
      const k = stack.map(e => e.tag).lastIndexOf(close[1].toLowerCase());
      if (k > 0) stack.length = k;
      i += close[0].length;
      continue;
    }
    if (src[i] === '<' && /[A-Za-z]/.test(src[i + 1] ?? '')) {
      const el = { tag: '', attrs: {}, attrAt: {}, children: [], at: i };
      i++;
      while (/[\w-]/.test(src[i])) el.tag += src[i++].toLowerCase();
      for (;;) {
        while (/\s/.test(src[i] ?? '')) i++;
        if (i >= src.length || src[i] === '>') { i++; break; }
        if (src[i] === '/' && src[i + 1] === '>') { i += 2; el.selfClosing = true; break; }
        let name = '';
        while (i < src.length && !/[\s=>]/.test(src[i]) && !(src[i] === '/' && src[i + 1] === '>')) name += src[i++];
        while (/\s/.test(src[i] ?? '')) i++;
        let value = '', at = i;
        if (src[i] === '=') {
          i++;
          while (/\s/.test(src[i] ?? '')) i++;
          const q = src[i];
          if (q === '"' || q === "'") { at = i + 1; const e = src.indexOf(q, i + 1); value = src.slice(i + 1, e); i = e + 1; } else { at = i; while (i < src.length && !/[\s>]/.test(src[i])) value += src[i++]; }
        }
        if (name) { el.attrs[name.toLowerCase()] = decode(value); el.attrAt[name.toLowerCase()] = at; }
      }
      stack[stack.length - 1].children.push(el);
      if (el.tag === 'script' || el.tag === 'style') { const e = src.toLowerCase().indexOf(`</${el.tag}`, i); i = e < 0 ? src.length : src.indexOf('>', e) + 1; continue; }
      if (!VOID.has(el.tag) && !el.selfClosing) stack.push(el);
      continue;
    }
    let e = src.indexOf('<', i + 1);
    if (e < 0) e = src.length;
    stack[stack.length - 1].children.push({ text: src.slice(i, e), at: i });
    i = e;
  }
  return root;
}

/**
 * HTML source → {keys: [{kind: 'html'|'attr', key, line, elements}], stray}:
 * the text of an element with data-i18n (its child elements as {0}…, or the
 * attribute's own key) and the attributes it names in data-i18n-attr give
 * keys; Japanese in any other text or attribute is stray.
 */
function scanHtml(src) {
  const keys = [], stray = [], lineAt = lineIndex(src);
  const walk = el => {
    const named = new Set(String(el.attrs['data-i18n-attr'] ?? '').split(/[\s|,]+/).filter(Boolean));
    for (const [name, value] of Object.entries(el.attrs)) {
      if (name === 'data-i18n' || !JP.test(value)) continue;
      if (named.has(name)) keys.push({ kind: 'attr', key: lang.staticKey(value), line: lineAt(el.attrAt[name]) });
      else stray.push({ line: lineAt(el.attrAt[name]) });
    }
    const marked = Object.prototype.hasOwnProperty.call(el.attrs, 'data-i18n');
    if (marked) {
      let k = 0;
      const derived = el.children.map(c => (c.tag ? `{${k++}}` : decode(c.text))).join('');
      const key = lang.staticKey(el.attrs['data-i18n'] || derived);
      if (JP.test(key)) keys.push({ kind: 'html', key, line: lineAt(el.at), elements: k });
    }
    for (const c of el.children) {
      if (c.tag) walk(c);
      else if (!marked) flag(c);
    }
    function flag(c) { const k = c.text.search(JP); if (k >= 0) stray.push({ line: lineAt(c.at + k) }); }
  };
  walk(parseHtml(src));
  return { keys, stray };
}

test('the scanner itself: keys, stray Japanese, comments, nesting, regexes, markup', () => {
  const js = [
    '// 日本語のコメント',
    'const a = L`国民 ${n}人`, b = html`<b>${L`首都 ${c}`}</b>`;',
    "const d = t('キー{0}', 1), e = x.t('別');",
    'const f = `未訳 ${g}`; /* 日本語 */ const h = a / 2 / 3;',
    "const r = /[ぁ-ん]/g; const s = 'ok' + \"あ\";",
    'const u = L`外 ${cond ? \'内\' : L`入れ子`}`;',
    'const v = Lh`<b>${x}</b>が参加\n      しました`;',
  ].join('\n');
  const { keys, stray } = scanJs(js);
  assert.deepEqual(keys.map(k => [k.kind, k.key, k.line]).sort(), [
    ['L', '入れ子', 6], ['L', '国民 {0}人', 2], ['L', '外 {0}', 6], ['L', '首都 {0}', 2], ['Lh', '<b>{0}</b>が参加 しました', 7], ['t', 'キー{0}', 3],
  ].sort());
  assert.deepEqual(stray.map(s => s.line), [3, 4, 5, 5, 6]);
  const page = `<!doctype html><html lang="ja"><head><title data-i18n>題 — テスト</title></head><body>
<!-- 注記 -->
<p data-i18n><span id="m"></span>。国は<span>6</span>つ &amp; 以上</p>
<button title="拡大" data-i18n-attr="title" aria-label="縮小">+</button>
<b>未訳</b><img alt="画像"><br><i data-i18n="明示キー">明示キー</i>
<script type="module">const x = '無視';</script></body></html>`;
  const h = scanHtml(page);
  assert.deepEqual(h.keys.map(k => [k.kind, k.key, k.line, k.elements ?? null]), [
    ['html', '題 — テスト', 1, 0], ['html', '{0}。国は{1}つ & 以上', 3, 2], ['attr', '拡大', 4, null], ['html', '明示キー', 5, 0],
  ]);
  assert.deepEqual(h.stray.map(s => s.line), [4, 5, 5]);
});

function webFiles() {
  const out = [];
  const walk = dir => {
    for (const name of readdirSync(dir).sort()) {
      const p = join(dir, name), rel = relative(WEB, p);
      if (statSync(p).isDirectory()) { if (rel !== 'sdk' && rel !== 'lang') walk(p); continue; }
      if (rel.endsWith('.mjs') && rel !== 'lang.mjs') out.push(rel);
    }
  };
  walk(WEB);
  return [...out, 'index.html', 'spectate.html'];
}
const ONLY = (process.env.PS_LANG_ONLY || '').split(',').map(s => s.trim()).filter(Boolean);
const scanned = () => webFiles().filter(f => !ONLY.length || ONLY.some(p => f.startsWith(p))).map(file => {
  const src = readFileSync(join(WEB, file), 'utf8');
  const r = file.endsWith('.html') ? scanHtml(src) : scanJs(src);
  const lines = src.split('\n');
  const at = line => `${file}:${line}  ${lines[line - 1].trim().slice(0, 110)}`;
  return { file, ...r, at };
});
/** The findings, one per line of source, with a count per file first. */
function report(title, list) {
  const uniq = [...new Set(list)];
  const perFile = new Map();
  for (const x of uniq) { const f = x.slice(0, x.indexOf(':')); perFile.set(f, (perFile.get(f) ?? 0) + 1); }
  return `${title} (${uniq.length}) — ${[...perFile].map(([f, k]) => `${f} ${k}`).join(', ')}:\n  ${uniq.join('\n  ')}`;
}

test('completeness (a): every marked Japanese text has an English entry', () => {
  const missing = [];
  for (const { keys, at } of scanned()) {
    for (const k of keys) {
      const en = EN[k.key];
      if (en === undefined) { missing.push(`${at(k.line)}\n      key: ${JSON.stringify(k.key)}`); continue; }
      if (k.kind === 'html' && k.elements) {
        const used = placeholders(typeof en === 'function' ? en() : en).sort((a, b) => a - b);
        const want = Array.from({ length: k.elements }, (_, j) => j);
        if (JSON.stringify(used) !== JSON.stringify(want)) missing.push(`${at(k.line)}\n      key: ${JSON.stringify(k.key)} — the English must use each of {0}…{${k.elements - 1}} once (child elements)`);
      }
    }
  }
  assert.equal(missing.length, 0, report('texts without English', missing));
});

test('completeness (b): no Japanese outside L`…`, data-i18n, i18n.mjs tables and comments', () => {
  const stray = [];
  for (const { file, stray: s, at } of scanned()) if (file !== 'i18n.mjs') for (const x of s) stray.push(at(x.line));
  assert.equal(stray.length, 0, report('unmarked Japanese', stray));
});
