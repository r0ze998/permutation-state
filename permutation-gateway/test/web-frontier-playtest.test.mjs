// W6-D web fixes found by the scripted onboarding run on the local stack
// (M1 contract §13.6 E7): what failed in the browser, pinned here so it
// stays fixed without a stack.
//
// 1. Number fields whose default value breaks their own `min`/`step`: the
//    browser's constraint validation then blocks the form's submit event
//    silently (Train's `min=1 step=100 value=100` never submitted).
// 2. The founded Hamlet's base production is the catalog's, pinned here
//    (the onboarding card took it for a finished building).
// 3. The app hand-overs of W5-E (R3): the sheet is mounted by `boot`, the
//    map gets the page's `terrainOf`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as land from '../../permutation-server/web/frontier/fland.mjs';
import * as C from '../../permutation-server/web/frontier/controller.mjs';
import * as card from '../../permutation-server/web/frontier/screens/onboarding.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const read = rel => readFileSync(`${ROOT}${rel}`, 'utf8');
const WEB = 'permutation-server/web/frontier/';

/** Every `<input type="number">` of the Frontier page sources with its literal attributes. */
function numberInputs() {
  const out = [];
  const files = ['index.html', 'practice.html', 'spectate.html', ...readdirSync(`${ROOT}${WEB}screens`).map(f => `screens/${f}`)];
  for (const f of files) {
    const src = read(`${WEB}${f}`);
    for (const m of src.matchAll(/<input\b[^>]*type="number"[^>]*>/g)) {
      const attr = k => { const a = new RegExp(`\\s${k}="([^"$]*)"`).exec(m[0]); return a ? a[1] : null; };
      out.push({ file: f, tag: m[0], min: attr('min'), step: attr('step'), max: attr('max'), value: attr('value') });
    }
  }
  return out;
}

test('number fields: a literal default value satisfies its own min, max and step (else the form never submits)', () => {
  const inputs = numberInputs();
  assert.ok(inputs.length >= 8, `found ${inputs.length}`);
  const bad = [];
  for (const i of inputs) {
    const step = i.step === null ? 1 : i.step === 'any' ? null : Number(i.step);
    const min = i.min === null ? 0 : Number(i.min);
    // Only literal values can be checked here (a template value is the page's state).
    if (i.value === null || i.value === '' || !/^-?\d+(\.\d+)?$/.test(i.value)) continue;
    const v = Number(i.value);
    if (i.min !== null && v < min) bad.push(`${i.file}: value ${v} < min ${min}: ${i.tag}`);
    if (i.max !== null && v > Number(i.max)) bad.push(`${i.file}: value ${v} > max ${i.max}: ${i.tag}`);
    if (step !== null && Math.abs(((v - min) / step) - Math.round((v - min) / step)) > 1e-9) bad.push(`${i.file}: value ${v} is not min ${min} + k × step ${step}: ${i.tag}`);
  }
  assert.deepEqual(bad, []);
  // The integer counts the program takes one by one are not stepped by 100.
  for (const name of ['n', 'troops', 'delta']) {
    const i = inputs.find(x => x.file === 'screens/holding.mjs' && x.tag.includes(`name="${name}"`));
    assert.ok(i, name);
    assert.equal(i.step, '1', `${name}: any whole number`);
  }
});

test('base production: the Hamlet the program founds (catalog base_production) and the tier bonus equal catalog.rs', () => {
  const cat = read('permutation-rules/src/frontier/catalog.rs');
  const base = /pub const BASE_PROD: \[i64; RESOURCES\] = \[([^\]]+)\]/.exec(cat);
  assert.ok(base);
  assert.deepEqual([...land.BASE_PROD], base[1].split(',').map(x => Number(x.trim())));
  const bonus = /pub const fn tier_bonus_pct[\s\S]*?Hamlet => (\d+),\s*Tier::Town => (\d+),\s*Tier::City => (\d+),\s*Tier::Stronghold => (\d+)/.exec(cat);
  assert.ok(bonus);
  assert.deepEqual([...land.TIER_BONUS_PCT], bonus.slice(1).map(Number));
  // catalog.rs's own test: Hamlet food 40,000, Stronghold food 110,000 milli per hour.
  assert.equal(land.baseProduction(0)[0], 40_000n);
  assert.equal(land.baseProduction(3)[0], 110_000n);
});

test('app hand-overs (W5-E R3): boot mounts the bottom sheet and gives the map its terrain', () => {
  const app = read(`${WEB}app.mjs`);
  const shell = read(`${WEB}screens/shell.mjs`);
  assert.match(app, /mountSheet\(\)/, 'boot calls mountSheet');
  assert.doesNotMatch(shell, /^if \(globalThis\.document[^\n]*mountSheet\(\);$/m, 'the sheet no longer mounts itself on import');
  assert.match(app, /createTerrain\(/, 'the app makes the terrain source');
  assert.match(app, /selected: FS\.selected, terrainOf[,\s}]/, 'the map source names terrainOf');
});

test('catch-up (W6-D): the page asks the keeper once per bell while the home province lags, and retries a resident action refused NotResident once', () => {
  const h = { state: 2 };
  // The program's rule: resolved_next + 1 ≥ b.
  assert.equal(C.provinceLags({ resolvedNext: 40 }, 41), false);
  assert.equal(C.provinceLags({ resolvedNext: 40 }, 42), true);
  assert.equal(C.shouldAutoNudge({ holding: h, province: { resolvedNext: 24 }, nowBell: 40 }), true);
  assert.equal(C.shouldAutoNudge({ holding: h, province: { resolvedNext: 24 }, nowBell: 40, lastBell: 40 }), false, 'once per bell');
  assert.equal(C.shouldAutoNudge({ holding: h, province: { resolvedNext: 24 }, nowBell: 41, lastBell: 40 }), true);
  assert.equal(C.shouldAutoNudge({ holding: h, province: { resolvedNext: 39 }, nowBell: 40 }), false, 'caught up');
  assert.equal(C.shouldAutoNudge({ holding: h, province: { resolvedNext: 24 }, nowBell: 40, hidden: true }), false, 'not from a hidden tab');
  assert.equal(C.shouldAutoNudge({ holding: null, province: { resolvedNext: 24 }, nowBell: 40 }), false, 'no land, no nudge');
  assert.equal(C.shouldAutoNudge({ holding: { state: 3 }, province: { resolvedNext: 24 }, nowBell: 40 }), false, 'a released holding');
  for (const n of ['Muster', 'Garrison', 'Dissolve', 'Explore']) assert.equal(C.retryAfterCatchUp(n, 'NotResident'), true, n);
  assert.equal(C.retryAfterCatchUp('Depart', 'NotResident'), false, 'the march has its own path');
  assert.equal(C.retryAfterCatchUp('Muster', 'NotFinal'), false);
});

test('no site picker anywhere (owner decision V2): joining is choosing a nation', () => {
  for (const f of ['screens/onboarding.mjs', 'screens/join.mjs', 'hud/inspect.mjs']) assert.doesNotMatch(read(`${WEB}${f}`), /data-act="(pick-province|toggle-site|file-ticket)"|act: '(pick-province|toggle-site)'/, f);
  assert.equal(typeof card.renderOverflow, 'undefined');
});
