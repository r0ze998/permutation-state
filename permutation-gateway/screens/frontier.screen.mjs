// The Frontier screenshot smoke (contract §13.6 E7, web design §13.2):
// 12 scenes (scenes.mjs) × JA and EN × 360×740, 390×844, 1440×900 in
// Playwright's Chromium, against the fixture server (server.mjs, 127.0.0.1
// port 0). Each (scene, viewport) loads once in Japanese, is checked and
// shot, then the language toggle switches it to English without a reload
// (the page's marker survives), and it is checked and shot again.
//
// Per shot: no console error, page error or failed request; no horizontal
// overflow; banner, main, navigation and the bell chip visible; interactive
// elements ≥ 44 × 44 px at phone widths (≥ 24 on desktop); axe-core with
// no serious or critical violation; in English, no Japanese glyph outside
// lang="ja" / data-name; no "null", "undefined", "NaN" or empty separator
// in the text or names; in English, no doubled word (wave-5 review).
// Screenshots go to artifacts/ (not committed) with
// summary.json; pixel comparison is not a gate (advisory, none yet).
//
//   cd permutation-gateway/screens && npm ci && node --test *.screen.mjs
//   SCREENS_ONLY=report,march   run a subset of scenes
//   SCREENS_VIEWPORTS=390       run a subset of viewports
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { startServer } from './server.mjs';
import { SCENES, VIEWPORTS } from './scenes.mjs';
import { layout, japanese, placeholders, doubled, axe } from './checks.mjs';
import { storageFor, LATEST_UNIX } from './world.mjs';

const ARTIFACTS = fileURLToPath(new URL('./artifacts/', import.meta.url));
const only = (env, list, key) => { const want = (process.env[env] ?? '').split(',').map(s => s.trim()).filter(Boolean); return want.length ? list.filter(x => want.includes(x[key])) : list; };
const scenes = only('SCREENS_ONLY', SCENES, 'id');
const viewports = only('SCREENS_VIEWPORTS', VIEWPORTS, 'id');

let browser, srv;
const summary = [];

before(async () => {
  mkdirSync(ARTIFACTS, { recursive: true });
  srv = await startServer();
  browser = await chromium.launch();
});
after(async () => {
  await browser?.close();
  await srv?.close();
  writeFileSync(`${ARTIFACTS}summary.json`, `${JSON.stringify({ chromium: browser?.version?.() ?? null, shots: summary }, null, 1)}\n`);
});

/** Seeded Math.random and crypto.getRandomValues-free determinism, the storage the page finds, the reload marker. */
function initScript({ storage }) {
  let s = 0x2545f491;
  Math.random = () => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 2 ** 32; };
  try { for (const [k, v] of Object.entries(storage)) localStorage.setItem(k, v); } catch { /* no storage */ }
}

async function shot(page, { scene, vp, lang, problems }) {
  await page.waitForTimeout(120);
  const lay = await page.evaluate(layout, vp.phone);
  const where = `${scene.id} ${lang} ${vp.id}`;
  if (lay.overflow.scrollWidth > lay.overflow.innerWidth) problems.push(`${where}: horizontal overflow ${lay.overflow.scrollWidth} > ${lay.overflow.innerWidth}`);
  if (Math.abs(lay.fill.bottom - lay.fill.innerHeight) > 2) problems.push(`${where}: the layout ends at ${lay.fill.bottom} px of ${lay.fill.innerHeight}`);
  for (const c of lay.clipped) problems.push(`${where}: cut off at the side (${c.left}..${c.right} of ${vp.width}): <${c.tag}${c.id ? `#${c.id}` : ''} class="${c.cls}"> "${c.text}"`);
  for (const [k, ok] of Object.entries(lay.landmarks)) if (!ok) problems.push(`${where}: landmark ${k} not visible`);
  if (!lay.bellChip.visible || !/\d/.test(lay.bellChip.text)) problems.push(`${where}: bell chip "${lay.bellChip.text}" (visible ${lay.bellChip.visible})`);
  for (const s of lay.small) problems.push(`${where}: target ${s.w}×${s.h} < ${vp.phone ? 44 : 24}: <${s.tag}${s.id ? `#${s.id}` : ''}${s.act ? ` data-act=${s.act}` : ''}> "${s.text}"`);
  const violations = await axe(page);
  for (const v of violations) problems.push(`${where}: axe ${v.impact} ${v.id} (${v.help}): ${v.nodes.join(' | ')}`);
  const jp = lang === 'en' ? await page.evaluate(japanese) : [];
  for (const j of jp) problems.push(`${where}: Japanese in English at ${j.where}: "${j.text}"`);
  const ph = await page.evaluate(placeholders);
  for (const x of ph) problems.push(`${where}: placeholder text at ${x.where}: "${x.text}"`);
  const dup = lang === 'en' ? await page.evaluate(doubled) : [];
  for (const x of dup) problems.push(`${where}: doubled word at ${x.where}: "${x.text}"`);
  const file = `${scene.id}-${lang}-${vp.width}x${vp.height}.png`;
  await page.screenshot({ path: `${ARTIFACTS}${file}` });
  summary.push({ scene: scene.id, lang, viewport: vp.id, file, small: lay.small.length, axe: violations.map(v => v.id), japanese: jp.length, placeholders: ph.length, doubled: dup.length, overflow: lay.overflow.scrollWidth > lay.overflow.innerWidth, clipped: lay.clipped.length });
}

for (const vp of viewports) {
  for (const scene of scenes) {
    test(`${scene.title} @ ${vp.width}×${vp.height}: JA, then EN by the toggle`, { timeout: 120_000 }, async () => {
      srv.stage(scene.stage);
      const context = await browser.newContext({ viewport: { width: vp.width, height: vp.height }, deviceScaleFactor: 1, isMobile: vp.phone, hasTouch: vp.phone, locale: 'ja-JP', timezoneId: 'UTC', reducedMotion: 'reduce' });
      const problems = [];
      try {
        await context.addInitScript(initScript, { storage: storageFor(srv.viewer, { stage: scene.stage, lang: 'ja' }) });
        const page = await context.newPage();
        await page.clock.setFixedTime(new Date(LATEST_UNIX * 1000 + 500));
        page.on('console', m => { if (m.type() === 'error') problems.push(`${scene.id} ${vp.id}: console error: ${m.text()}`); });
        page.on('pageerror', e => problems.push(`${scene.id} ${vp.id}: page error: ${e.message}`));
        page.on('requestfailed', r => problems.push(`${scene.id} ${vp.id}: request failed: ${r.url()} ${r.failure()?.errorText ?? ''}`));
        page.on('response', r => { if (r.status() >= 400) problems.push(`${scene.id} ${vp.id}: HTTP ${r.status()} ${r.url()}`); });
        await page.goto(`${srv.url}/frontier/${scene.page}`);
        await page.waitForFunction(() => /\d/.test(document.getElementById('bell-chip')?.textContent ?? ''));
        await scene.go(page);
        await shot(page, { scene, vp, lang: 'ja', problems });
        // The toggle: English in place, no reload (the marker survives), the chip re-rendered.
        await page.evaluate(() => { globalThis.__screensMarker = 'kept'; });
        await page.locator('#lang-box [data-lang-toggle]').click();
        await page.waitForFunction(() => document.documentElement.lang === 'en');
        await page.waitForFunction(() => /^(Bell|Starts)/.test(document.getElementById('bell-chip')?.textContent ?? ''));
        assert.equal(await page.evaluate(() => globalThis.__screensMarker), 'kept', 'the language switch reloaded the page');
        await shot(page, { scene, vp, lang: 'en', problems });
      } finally {
        await context.close();
      }
      assert.deepEqual(problems, [], `${problems.length} problem(s):\n  ${problems.join('\n  ')}`);
    });
  }
}
