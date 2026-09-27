// Every refusal the Frontier page can show has Japanese and English text
// (contract §9.6; web design §5.4): every program error code of the ABI
// (frontier-abi/vectors/errors.json, the generated abi.mjs), every code the
// relay can answer (read from its sources, so a new refusal without a text
// fails here), the keeper link's answers, and every code the page's own
// modules return; `failureText` prefers the program's number, then the
// name, then the page's and the relay's codes, then the HTTP status.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as fi18n from '../../permutation-server/web/frontier/fi18n.mjs';
import { ERRORS } from '../../permutation-server/web/frontier/abi.mjs';
import { FRONTIER_ERROR_STATUS } from '../src/frontier/chain.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

afterEach(() => setLang('ja'));
const REPO = fileURLToPath(new URL('../../', import.meta.url));
const JP = /[぀-ヿ一-鿿]/;
const ABI = JSON.parse(readFileSync(join(REPO, 'frontier-abi/vectors/errors.json'), 'utf8'));
const abiErrors = (ABI.errors ?? ABI).map(e => ({ code: e.code, name: e.name }));

const walk = dir => readdirSync(dir).flatMap(n => { const p = join(dir, n); return statSync(p).isDirectory() ? walk(p) : [p]; });
const src = p => readFileSync(p, 'utf8');

/** The last string literal naming a code in each `new RouteError(…)` of a source, and every `refuse('X'`. */
function relayCodes(files) {
  const out = new Set();
  for (const f of files) {
    const s = src(f);
    let i = 0;
    while ((i = s.indexOf('RouteError(', i)) >= 0) {
      let depth = 0, j = i + 'RouteError'.length;
      for (; j < s.length; j++) { if (s[j] === '(') depth++; else if (s[j] === ')' && --depth === 0) break; }
      const names = [...s.slice(i, j).matchAll(/'([A-Z][A-Za-z0-9]+)'/g)].map(m => m[1]);
      if (names.length) out.add(names.at(-1));
      i = j;
    }
    for (const m of s.matchAll(/refuse\('([A-Z][A-Za-z0-9]+)'/g)) out.add(m[1]);
  }
  return out;
}

/** Codes the page's modules return: `fail('X'`, `code: 'X'`, `codeError('X'`, `…Error('X'`, `add('X')`. */
function pageCodes(files) {
  const out = new Set();
  const res = [/\bfail\('([A-Za-z][A-Za-z0-9]+)'/g, /\bcode: '([A-Za-z][A-Za-z0-9]+)'/g, /codeError\('([A-Za-z][A-Za-z0-9]+)'/g, /(?:Herald|Codec|Wasm)Error\('([A-Za-z][A-Za-z0-9]+)'/g, /\badd\('([A-Za-z][A-Za-z0-9]+)'\)/g];
  for (const f of files) for (const re of res) for (const m of src(f).matchAll(re)) out.add(m[1]);
  return out;
}

const hasText = code => {
  setLang('ja');
  const ja = fi18n.failureText({ code });
  setLang('en');
  const en = fi18n.failureText({ code });
  setLang('ja');
  return { ja, en, ok: JP.test(ja) && !JP.test(en) && ja !== en && !/不明なエラー|Unknown error/.test(ja + en) };
};

test('every program error code of the ABI has its own Japanese and English text', () => {
  assert.ok(abiErrors.length >= 60);
  assert.deepEqual(abiErrors.map(e => [e.code, e.name]), ERRORS.map(([c, n]) => [c, n]), 'abi.mjs carries frontier-abi\'s table');
  const seen = new Map();
  for (const { code, name } of abiErrors) {
    setLang('ja');
    const ja = fi18n.errorText(code);
    assert.equal(fi18n.errorText(name), ja);
    assert.match(ja, JP, `${name} (${code}): Japanese`);
    assert.doesNotMatch(ja, /不明なエラー/, `${name}: a text of its own`);
    setLang('en');
    const en = fi18n.errorText(code);
    assert.doesNotMatch(en, JP, `${name}: English`);
    assert.doesNotMatch(en, /Unknown error/, name);
    assert.equal(fi18n.failureText({ programCode: code, code: 'SomethingElse' }), en, 'the program\'s number wins');
    if (name !== 'Reserved14' && name !== 'NotImplemented') seen.set(en, [...(seen.get(en) ?? []), name]);
  }
  for (const [en, names] of seen) assert.ok(names.length === 1 || names.every(n => ['Duplicate', 'AlreadyProcessed'].includes(n)), `${names} share "${en}"`);
  // The codes the contract names for the UI (§5.4, I-08, I-44, I-47, I-51).
  setLang('en');
  assert.match(fi18n.errorText(51), /tip/i);
  assert.match(fi18n.errorText(58), /settled/i);
  assert.match(fi18n.errorText(60), /next bell/i);
  assert.match(fi18n.errorText(61), /three/i);
  assert.match(fi18n.errorText(24), /final/i);
});

test('every code the relay can answer has a text (read from its sources)', () => {
  const files = [...walk(join(REPO, 'permutation-gateway/src/frontier')), ...['cosign.mjs', 'guards.mjs', 'send.mjs', 'routes/errors.mjs'].map(f => join(REPO, 'permutation-gateway/src', f)),
    join(REPO, 'permutation-gateway/client/src/frontier/shapes.mjs')];
  const codes = relayCodes(files);
  for (const must of ['RelayRejected', 'UseRevealRoute', 'TipNotPreset', 'QuotaExceeded', 'InviteRequired', 'OperatorLowFunds', 'Duplicate', 'RateLimited', 'KeeperUnavailable']) assert.ok(codes.has(must), `scan found ${must}`);
  const missing = [...codes].filter(c => !hasText(c).ok);
  assert.deepEqual(missing, [], 'relay codes without a text');
  // Program errors the relay maps to a status are the ABI's names.
  const names = new Set(abiErrors.map(e => e.name));
  assert.deepEqual(Object.keys(FRONTIER_ERROR_STATUS).filter(n => !names.has(n)), []);
  // The keeper link's answers to a reveal (§8.2) are program names.
  for (const c of ['CommitMismatch', 'WindowClosed', 'BadPlaintext']) assert.ok(hasText(c).ok, c);
});

test('every code the page\'s own modules return has a text', () => {
  const web = join(REPO, 'permutation-server/web');
  const files = [...walk(join(web, 'frontier')).filter(f => f.endsWith('.mjs')), join(web, 'wallet.mjs')];
  const codes = pageCodes(files);
  for (const must of ['MessageRefused', 'RelayMessageChanged', 'NotSaved', 'NoKernel', 'HostInTransit', 'TipNotPreset', 'NoDestination', 'SealAuditFailed']) assert.ok(codes.has(must), `scan found ${must}`);
  // Seal-worker audit steps and the planner's reasons are internal detail (the page shows their code's text).
  const internal = new Set(['V', 'W', 'body', 'commitment', 'length', 'SealFailed']);
  const missing = [...codes].filter(c => !internal.has(c) && !hasText(c).ok);
  assert.deepEqual(missing, [], 'page codes without a text');
});

test('failureText: number, then name, then the page\'s codes, then the HTTP status; unknowns say so', () => {
  setLang('en');
  assert.equal(fi18n.failureText({ code: 'HostInTransit' }), fi18n.errorText(58));
  assert.equal(fi18n.failureText({ code: 'Whatever', programCode: 51 }), fi18n.errorText(51));
  assert.equal(fi18n.failureText({ code: 'RelayRejected' }), fi18n.clientText('RelayRejected'));
  assert.equal(fi18n.failureText({ code: 'HTTP429' }), fi18n.clientText('QuotaExceeded'));
  assert.equal(fi18n.failureText({ code: 'HTTP503' }), fi18n.clientText('Unavailable'));
  assert.equal(fi18n.failureText({ code: null, httpStatus: 502 }), fi18n.clientText('Unavailable'));
  assert.equal(fi18n.failureText({ code: 'Zzz' }), 'Unknown error (Zzz)');
  assert.equal(fi18n.failureText(null), 'Unknown error (?)');
  assert.ok(fi18n.CLIENT_CODES.includes('TipNotPreset') === false, 'TipNotPreset is an ABI code (61), not duplicated');
  setLang('ja');
  assert.equal(fi18n.failureText({ code: 'QuotaExceeded' }), '今日の中継の枠を使い切りました。次のゲーム日まで待ってください');
});
