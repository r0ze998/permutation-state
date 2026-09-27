// The Frontier page's shell pieces: config (same-origin endpoints; the
// ?herald= override only on a loopback page, only to a loopback herald),
// the render scheduler (one pass per task, order kept, a failing renderer
// isolated, a language switch re-renders all), the bell chip's text in
// both languages, the enum tables and a JA/EN text for every program error
// code of the ABI, and the pages' static markup (landmarks, the bell chip,
// the map's accessible name, no inline script).
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { config } from '../../permutation-server/web/frontier/config.mjs';
import * as fstate from '../../permutation-server/web/frontier/fstate.mjs';
import { chipText } from '../../permutation-server/web/frontier/app.mjs';
import * as fi18n from '../../permutation-server/web/frontier/fi18n.mjs';
import { ERRORS } from '../../permutation-server/web/frontier/abi.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

afterEach(() => setLang('ja'));
const loc = href => { const u = new URL(href); return { href, origin: u.origin, hostname: u.hostname }; };
const JP = /[぀-ヿ㐀-鿿]/;

test('config: same origin by default; ?herald= only from and to a loopback host', () => {
  assert.deepEqual(config(loc('https://play.example/frontier/'), { body: { dataset: { mode: 'spectate' } } }), { origin: 'https://play.example', herald: 'https://play.example', relay: 'https://play.example/gw', dev: false, mode: 'spectate' });
  assert.equal(config(loc('https://play.example/frontier/?herald=http://127.0.0.1:41040')).herald, 'https://play.example', 'ignored on a public host');
  assert.equal(config(loc('http://127.0.0.1:5000/frontier/?herald=http://127.0.0.1:41040')).herald, 'http://127.0.0.1:41040');
  assert.equal(config(loc('http://127.0.0.1:5000/frontier/?herald=https://evil.example')).herald, 'http://127.0.0.1:5000', 'never a remote herald');
  assert.equal(config(loc('http://localhost:5000/?herald=javascript:alert(1)')).herald, 'http://localhost:5000');
  assert.equal(config(loc('http://localhost:5000/'), { body: { dataset: { mode: 'bogus' } } }).mode, 'play');
});

test('the render scheduler: one pass per task, in order, a failing renderer isolated', async () => {
  const ran = [];
  fstate.registerRenderers([['a', () => ran.push('a')], ['b', () => { ran.push('b'); throw new Error('boom'); }], ['c', s => ran.push(s === fstate.FS ? 'c' : '?')]]);
  const err = console.error; console.error = () => {};
  try {
    fstate.invalidate('c', 'a');
    fstate.invalidate('a');
    await Promise.resolve();
    assert.deepEqual(ran, ['a', 'c']);
    fstate.invalidate('all');
    await Promise.resolve();
    assert.deepEqual(ran, ['a', 'c', 'a', 'b', 'c']);
    setLang('en');
    await Promise.resolve();
    assert.deepEqual(ran.slice(5), ['a', 'b', 'c'], 'a language switch re-renders every part');
  } finally { console.error = err; }
});

test('the bell chip text in both languages', () => {
  assert.equal(chipText({ bell: 1034, secondsLeft: 372, beforeGenesis: false, ended: false }), '鐘 1,034 · 残り 6:12');
  assert.equal(chipText(null), '鐘 —');
  assert.equal(chipText({ bell: null, secondsLeft: 65, beforeGenesis: true }), '開始まで 1:05');
  setLang('en');
  assert.equal(chipText({ bell: 1034, secondsLeft: 372, beforeGenesis: false, ended: false }), 'Bell 1,034 · 6:12 left');
  assert.equal(chipText({ bell: 1008, secondsLeft: 1, ended: true }), 'Bell 1,008 · ended');
});

test('tables and every program error code have Japanese and English text', () => {
  const names = new Set(ERRORS.map(e => e[1]));
  assert.deepEqual([...names].filter(n => !fi18n.ERROR_NAMES.includes(n)), [], 'every ABI error has a text');
  for (const [code, name] of ERRORS) {
    setLang('ja');
    const ja = fi18n.errorText(code);
    assert.equal(fi18n.errorText(name), ja);
    assert.match(ja, JP, `${name}: Japanese`);
    setLang('en');
    const en = fi18n.errorText(code);
    assert.doesNotMatch(en, JP, `${name}: English`);
    assert.notEqual(en, ja);
  }
  assert.equal(fi18n.errorText(12345), 'Unknown error (12345)');
  setLang('en');
  assert.equal(fi18n.UNITS.Scout, 'Scout');
  assert.equal(fi18n.STANCES.Brace, 'Brace');
  assert.equal(fi18n.DOCTRINE_C(), 'Flame');
  assert.equal(fi18n.factionName(4), 'Ember');
  assert.equal(fi18n.factionName(6), 'Neutral');
  assert.equal(fi18n.RESOURCE_ORDER.map(r => fi18n.RESOURCES[r]).join(','), 'Food,Wood,Stone,Ore,Horses,Gold,Science,Influence');
  for (const code of ['network', 'SealAuditFailed', 'NotQuicknet', 'TestBeaconOffLocalnet', 'Kernel']) assert.doesNotMatch(fi18n.clientText(code), JP, code);
  setLang('ja');
  assert.equal(fi18n.STANCES.Hold, '待機');
});

test('the pages: landmarks, the bell chip, an accessible map, a module script and no inline code', () => {
  for (const page of ['index.html', 'practice.html', 'spectate.html']) {
    const html = readFileSync(new URL(`../../permutation-server/web/frontier/${page}`, import.meta.url), 'utf8');
    assert.match(html, /<html lang="ja">/);
    assert.match(html, /<meta name="viewport" content="width=device-width, initial-scale=1/);
    for (const re of [/<header /, /<main /, /<nav /, /id="bell-chip"/, /id="lang-box"/, /<canvas id="frontier-map" tabindex="0" aria-label="[^"]+"[^>]*aria-describedby="map-summary"/]) assert.match(html, re, `${page}: ${re}`);
    assert.match(html, /<script type="module" src="app\.mjs"><\/script>/);
    assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)[^>]*>/, `${page}: inline script (the CSP allows 'self' only)`);
    assert.doesNotMatch(html, /\son[a-z]+="/, `${page}: inline handler`);
    assert.doesNotMatch(html, /https?:\/\//, `${page}: no third-party origin`);
  }
});
