// The scripted onboarding run (M1 contract §13.6 E7, §11 W6-D): Playwright
// drives the page's own dev wallet on a local stack (`frontier-stack up`,
// the herald on base + 40) at 390 × 844, once in Japanese and once in
// English: join → site → build → scout/explore → sealed march on a camp →
// report → "verify in this browser" green. Every step goes through the
// page's own controls (tabs, `data-act` buttons, forms); nothing is written
// into the page's state. Each step is shot to artifacts/live/ with the
// matrix checks of checks.mjs (no console error or failed request, no
// overflow, 44-px targets, axe serious/critical, no Japanese in English).
//
//   LIVE_HERALD=http://127.0.0.1:41340 LIVE_LANGS=ja,en node --test live/onboarding.live.mjs
//
// Not part of `node --test *.screen.mjs` (it needs a running stack); the
// runner script live/run-onboarding.sh starts one, runs this and stops it.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { layout, japanese, placeholders, axe } from '../checks.mjs';

const HERALD = (process.env.LIVE_HERALD ?? 'http://127.0.0.1:41340').replace(/\/$/, '');
// The herald serves `permutation-server/web` under /frontier/, so the game page is /frontier/frontier/.
const PAGE = `${HERALD}/frontier/frontier/index.html`;
const LANGS = (process.env.LIVE_LANGS ?? 'ja,en').split(',').map(s => s.trim()).filter(Boolean);
// Wall-clock limits. At scale 20 a bell is 30 s; the report is due ≈ 31–41 game minutes after departure plus the travel.
const WAIT_MS = Number(process.env.LIVE_WAIT_MS ?? 20 * 60_000);
const ARTIFACTS = fileURLToPath(new URL('../artifacts/live/', import.meta.url));
const VP = { width: 390, height: 844 };

let browser;
const summary = { herald: HERALD, viewport: VP, runs: [] };
before(async () => { mkdirSync(ARTIFACTS, { recursive: true }); browser = await chromium.launch(); });
after(async () => {
  summary.chromium = browser?.version?.() ?? null;
  await browser?.close();
  writeFileSync(`${ARTIFACTS}summary.json`, `${JSON.stringify(summary, null, 1)}\n`);
});

const sleep = ms => new Promise(r => setTimeout(r, ms));
const T0 = Date.now();
const log = (...a) => console.log(`[${((Date.now() - T0) / 1000).toFixed(1)} s]`, ...a);

/** Faction per language run (two fresh wallets; different factions so the runs do not contend for one wedge). */
const FACTION = { ja: 1, en: 4 };

for (const lang of LANGS) {
  test(`onboarding on the local stack @ 390×844 in ${lang.toUpperCase()}`, { timeout: WAIT_MS * 3 }, async () => {
    const run = { lang, steps: [], problems: [] };
    summary.runs.push(run);
    const context = await browser.newContext({ viewport: VP, deviceScaleFactor: 1, isMobile: true, hasTouch: true, locale: lang === 'en' ? 'en-US' : 'ja-JP', timezoneId: 'UTC' });
    const problems = run.problems;
    const page = await context.newPage();
    // Chromium logs every 404 as a console error; those are judged by URL (expected404) in the response handler.
    page.on('console', m => { if (m.type() === 'error' && !/^Failed to load resource: the server responded with a status of 404/.test(m.text())) problems.push(`console error: ${m.text()}`); });
    page.on('pageerror', e => problems.push(`page error: ${e.message}`));
    page.on('requestfailed', r => { const f = r.failure()?.errorText ?? ''; if (!/ERR_ABORTED/.test(f)) problems.push(`request failed: ${r.url()} ${f}`); });
    run.notFound = {};
    page.on('response', r => {
      if (r.status() < 400) return;
      if (expected404(r)) { const k = new URL(r.url()).pathname.replace(/\d+/g, 'N'); run.notFound[k] = (run.notFound[k] ?? 0) + 1; } else problems.push(`HTTP ${r.status()} ${r.url()}`);
    });
    const t0 = Date.now();
    const step = async (id, fn) => {
      const s = { id, startMs: Date.now() - t0 };
      run.steps.push(s);
      log(lang, 'step', id);
      try {
        await fn();
        s.ok = true;
      } catch (e) {
        s.ok = false;
        s.error = String(e?.message ?? e).split('\n')[0];
        await page.screenshot({ path: `${ARTIFACTS}${lang}-${id}-FAILED.png` }).catch(() => {});
        writeFileSync(`${ARTIFACTS}${lang}-${id}-FAILED.txt`, await page.evaluate(() => document.body.innerText).catch(() => ''));
        throw e;
      } finally {
        s.endMs = Date.now() - t0;
      }
      await shot(page, lang, id, problems);
    };
    try {
      await page.goto(PAGE);
      await page.waitForFunction(() => /\d/.test(document.getElementById('bell-chip')?.textContent ?? ''));
      if (lang === 'en' && (await page.evaluate(() => document.documentElement.lang)) !== 'en') {
        await page.locator('#lang-box [data-lang-toggle]').click();
        await page.waitForFunction(() => document.documentElement.lang === 'en');
      }
      await step('welcome', async () => {
        await click(page, '[data-act="ob-seen"][data-flag="welcome"]');
      });
      await step('join', async () => {
        await click(page, '[data-act="connect"]');
        await click(page, `[data-act="pick-faction"][data-f="${FACTION[lang] ?? 1}"]`);
        // Join, and join again after a refusal (a player retries what the notice says failed; e.g. a season
        // that has just opened). Each refusal's text is kept in the summary.
        const end = Date.now() + WAIT_MS;
        let sent = false;
        while (!(await page.locator('[data-act="pick-province"], [data-act="toggle-site"]').count())) {
          if (Date.now() > end) throw new Error(`no site picker after Join (${JSON.stringify(run.joinRefusals ?? [])})`);
          const err = page.locator('.notice.error');
          if (!sent || (await err.count())) {
            if (sent) { (run.joinRefusals ??= []).push(await err.first().textContent()); await sleep(15_000); }
            if (await page.locator('[data-act="join"]:not([disabled])').count()) { await click(page, '[data-act="join"]:not([disabled])'); sent = true; }
          }
          await sleep(1000);
        }
      });
      await step('site', async () => {
        if (!(await page.locator('[data-act="toggle-site"]').count())) await click(page, '[data-act="pick-province"]');
        await waitFor(page, '[data-act="toggle-site"]', 'the free sites of the province');
        const sites = page.locator('[data-act="toggle-site"]');
        const n = Math.min(3, await sites.count());
        for (let i = 0; i < n; i++) {
          await sites.nth(i).click();
          await page.locator(`[data-act="toggle-site"][aria-pressed="true"]`).nth(i).waitFor({ state: 'attached' });
        }
        await click(page, '[data-act="file-ticket"]:not([disabled])');
        // The holding arrives with the ticket's seed (≈ 11–21 game minutes): the Holding tab appears.
        await waitFor(page, '#tabs [data-act="tab"][data-tab="holding"]', 'the Holding tab (a holding)');
      });
      await step('build', async () => {
        await tab(page, 'holding');
        await waitFor(page, '#holding-title', 'the holding panel');
        await click(page, '[data-act="build"]:not([disabled])');
        await notice(page, 'Build');
      });
      await step('scout', async () => {
        await tab(page, 'holding');
        await train(page, 6, 100);
        await muster(page, 6, 100);
        // A new host joins the roster at the next bell; then Explore opens on it.
        await tab(page, 'hosts');
        await enabledOrNudge(page, '[data-act="explore-open"]:not([disabled])', 'a scout host that can explore');
        await click(page, '[data-act="explore-open"]:not([disabled])');
        const tiles = page.locator('[data-act="explore-tile"]');
        await tiles.first().waitFor();
        const n = Math.min(2, await tiles.count());
        for (let i = 0; i < n; i++) await tiles.nth(i).click();
        await click(page, '[data-act="explore-send"]:not([disabled])');
        await notice(page, 'Explore');
      });
      await step('march', async () => {
        await tab(page, 'holding');
        // 100 Spearmen (the Hamlet's stores after the build and the scouts pay for about that many).
        await train(page, 0, 100);
        await muster(page, 0, 100);
        await tab(page, 'hosts');
        // The Spearman host (unit 0): Scouts do not contest a tile (kernel), so a camp would not fight them.
        // The panel re-renders on every poll: a click can land on a row that moved under the pointer, so the
        // composer's host is checked (the page's own state) and chosen again if it is not the Spearmen.
        const spear = 'li.host[data-unit="0"] [data-act="compose"]:not([disabled])';
        for (let i = 0; ; i++) {
          await enabledOrNudge(page, spear, 'the Spearman host ready to march');
          await page.locator(spear).first().click();
          await waitFor(page, '#march-title', 'the march composer');
          const unit = await page.evaluate(async () => (await import('./fstate.mjs')).FS.compose?.host?.unit ?? null);
          if (unit === 0) break;
          run.recomposed = (run.recomposed ?? 0) + 1;
          if (i >= 3) throw new Error(`the composer holds unit ${unit}, not the Spearmen`);
          await click(page, '[data-act="compose-close"]');
          await tab(page, 'hosts');
        }
        await waitFor(page, '[data-act="dest-quick"]', 'a quick destination (the camp)');
        const camp = page.locator('[data-act="dest-quick"]').filter({ hasText: lang === 'en' ? /camp/i : /野営地/ });
        if (await camp.count()) await camp.first().click(); else await click(page, '[data-act="dest-quick"]');
        await waitFor(page, '[data-act="march-send"]:not([disabled])', 'the march ready to seal');
        run.march = await page.evaluate(async () => { const c = (await import('./fstate.mjs')).FS.compose; return c?.dest ? { unit: c.host?.unit ?? null, p: c.dest.p, q: c.dest.q, tile: c.dest.tile, arriveBell: Number(c.arriveBell) } : null; });
        await click(page, '[data-act="march-send"]:not([disabled])');
        await notice(page, 'Depart');
      });
      await step('report', async () => {
        await tab(page, 'marches');
        // The report of this march (its destination and arrival bell), not any clash of the home province.
        const m = run.march;
        const sel = m && Number.isInteger(m.arriveBell) ? `[data-act="report-open"][data-p="${m.p}"][data-q="${m.q}"][data-bell="${m.arriveBell}"]` : '[data-act="report-open"]';
        await waitFor(page, sel, 'the report of the march (resolved)', WAIT_MS);
        const d = await page.locator(sel).first().evaluate(e => ({ ...e.dataset }));
        run.clash = { p: Number(d.p), q: Number(d.q), bell: Number(d.bell) };
        await click(page, sel);
        await waitFor(page, '#report-title', 'the clash report');
      });
      await step('verify', async () => {
        await click(page, '[data-act="report-verify"]');
        await page.waitForFunction(() => document.querySelector('[data-verify-result]') && document.querySelector('[data-verify-result]').dataset.verifyResult !== 'running', null, { timeout: 120_000 });
        const result = await page.locator('[data-verify-result]').first().getAttribute('data-verify-result');
        run.verify = result;
        run.verifySteps = await page.evaluate(async () => { const { FS } = await import('./fstate.mjs'); const v = FS.report?.verify; return v ? { builder: v.builder, pageBuilder: v.pageBuilder ?? null, steps: Object.fromEntries(Object.entries(v.steps).map(([k, x]) => [k, x.ok])) } : null; });
        assert.equal(result, 'match', 'verify in this browser is green');
      });
      run.wallet = await page.evaluate(async () => (await import('./fstate.mjs')).FS.wallet?.address ?? null);
      // One recording per run of this file (the first language that verifies), so the directory holds one season's files.
      if (process.env.LIVE_RECORD && run.clash && run.wallet && !summary.runs.some(r => r.recorded)) run.recorded = await record(process.env.LIVE_RECORD, run);
    } finally {
      run.wallMs = Date.now() - t0;
      run.nudges = NUDGES.splice(0);
      await context.close();
    }
    assert.deepEqual(problems, [], `${problems.length} problem(s):\n  ${problems.join('\n  ')}`);
  });
}

/**
 * A 404 the page expects and handles: the two "not yet" shapes of the herald
 * (a bell-region record before the bell is anchored, a province envelope
 * before the province is written; W6-D F8). Every other 4xx/5xx under `/h/`
 * (`/h/me`, `/h/clash`, `/h/season`, …) is a problem with its URL
 * (integ-W6 review), not a wait.
 */
const NOT_YET = [/^\/h\/bell\/\d+\/region\/\d+$/, /^\/h\/province\/-?\d+,-?\d+\/\d+$/];
export function expected404(r) {
  if (r.status() !== 404) return false;
  const path = new URL(r.url()).pathname;
  return NOT_YET.some(re => re.test(path));
}

async function click(page, sel, timeout = WAIT_MS) {
  const l = page.locator(sel).first();
  await l.waitFor({ state: 'visible', timeout });
  await l.scrollIntoViewIfNeeded().catch(() => {});
  await l.click();
}
async function waitFor(page, sel, what, timeout = WAIT_MS) {
  try {
    await page.locator(sel).first().waitFor({ state: 'attached', timeout });
  } catch (e) {
    throw new Error(`waited ${Math.round(timeout / 1000)} s for ${what} (${sel}): ${e.message.split('\n')[0]}`);
  }
}
/** Nudges the page's run: how often the "catch up" control was needed (a player would press it too). */
export const NUDGES = [];
/**
 * Wait for `sel`; while the page shows the "catch up" control (the province
 * lags: NotResident), press it as a player would, at most once a minute.
 */
async function enabledOrNudge(page, sel, what, timeout = WAIT_MS) {
  const end = Date.now() + timeout;
  let last = 0;
  while (Date.now() < end) {
    if (await page.locator(sel).count()) return;
    const n = page.locator('[data-act="nudge"]:not([disabled])').first();
    if (Date.now() - last > 60_000 && (await n.count()) && (await n.isVisible().catch(() => false))) {
      await n.click();
      last = Date.now();
      NUDGES.push({ what, at: new Date().toISOString() });
      log('nudge:', what);
    }
    await sleep(2000);
  }
  const st = await page.evaluate(async () => {
    const { FS } = await import('./fstate.mjs');
    const h = FS.holdings?.[0];
    const pr = h ? FS.provinces.get(`${h.p},${h.q}`)?.province : null;
    return { nowBell: FS.nowBell, state: h?.state, resolvedNext: pr?.resolvedNext ?? null, blocked: [...document.querySelectorAll('.blocked')].map(e => e.textContent) };
  }).catch(e => ({ error: String(e) }));
  throw new Error(`waited ${Math.round(timeout / 1000)} s for ${what} (${sel}); page: ${JSON.stringify(st)}`);
}
async function tab(page, id) {
  await sheet(page, 'full');
  await click(page, `#tabs [data-act="tab"][data-tab="${id}"]`);
  await sheet(page, 'full');
}
/** Bring the phone sheet to a height by its handle (peek → half → full). */
async function sheet(page, want) {
  const h = page.locator('[data-sheet-handle]');
  if (!(await h.isVisible().catch(() => false))) return;
  for (let i = 0; i < 3 && (await page.locator('#panel').getAttribute('data-sheet')) !== want; i++) await h.click();
}
/** Wait for the action's notice to leave "busy": ok, else throw with its text. */
async function notice(page, what, timeout = 180_000) {
  await page.waitForFunction(() => { const n = document.querySelector('.notice'); return n && !n.classList.contains('busy'); }, null, { timeout });
  const n = page.locator('.notice').first();
  const cls = await n.getAttribute('class');
  if (!/\bok\b/.test(cls ?? '')) throw new Error(`${what}: ${await n.textContent()}`);
}
async function train(page, unit, n) {
  await waitFor(page, 'form[data-form="train"] button[type="submit"]:not([disabled])', 'Train enabled');
  await page.locator('form[data-form="train"] select[name="unit"]').selectOption(String(unit));
  await page.locator('form[data-form="train"] input[name="n"]').fill(String(n));
  await page.locator('form[data-form="train"] button[type="submit"]').click();
  await notice(page, `Train ${unit}`);
}
async function muster(page, unit, troops) {
  await waitFor(page, `form[data-form="muster"] select[name="unit"] option[value="${unit}"]`, `unit ${unit} in the reserve`);
  await page.locator('form[data-form="muster"] select[name="unit"]').selectOption(String(unit));
  // Muster waits for the holding to be final (its cohort closed; web design §6.3) and the province caught up.
  await enabledOrNudge(page, 'form[data-form="muster"] button[type="submit"]:not([disabled])', 'Muster enabled (a final holding, the province caught up)');
  await page.locator('form[data-form="muster"] select[name="unit"]').selectOption(String(unit));
  await page.locator('form[data-form="muster"] input[name="troops"]').fill(String(troops));
  await page.locator('form[data-form="muster"] button[type="submit"]:not([disabled])').click();
  await notice(page, `Muster ${unit}`);
}

async function shot(page, lang, id, problems) {
  await sleep(150);
  const lay = await page.evaluate(layout, true);
  const where = `${lang} ${id}`;
  if (lay.overflow.scrollWidth > lay.overflow.innerWidth) problems.push(`${where}: horizontal overflow ${lay.overflow.scrollWidth} > ${lay.overflow.innerWidth}`);
  // The same judgement as the screen matrix (frontier.screen.mjs; §13.6): cut-off content, the three landmarks, the bell chip.
  for (const c of lay.clipped) problems.push(`${where}: cut off at the side (${c.left}..${c.right} of ${VP.width}): <${c.tag}${c.id ? `#${c.id}` : ''} class="${c.cls}"> "${c.text}"`);
  for (const [k, ok] of Object.entries(lay.landmarks)) if (!ok) problems.push(`${where}: landmark ${k} not visible`);
  if (!lay.bellChip.visible || !/\d/.test(lay.bellChip.text)) problems.push(`${where}: bell chip "${lay.bellChip.text}" (visible ${lay.bellChip.visible})`);
  for (const s of lay.small) problems.push(`${where}: target ${s.w}×${s.h} < 44: <${s.tag}${s.id ? `#${s.id}` : ''}${s.act ? ` data-act=${s.act}` : ''}> "${s.text}"`);
  for (const v of await axe(page)) problems.push(`${where}: axe ${v.impact} ${v.id}: ${v.nodes.join(' | ')}`);
  if (lang === 'en') for (const j of await page.evaluate(japanese)) problems.push(`${where}: Japanese in English at ${j.where}: "${j.text}"`);
  for (const x of await page.evaluate(placeholders)) problems.push(`${where}: placeholder text at ${x.where}: "${x.text}"`);
  await page.screenshot({ path: `${ARTIFACTS}${lang}-${id}.png` });
}

/**
 * Record the herald files of this run (web design §13.1: fixtures from a
 * local season): the season record, every open ring's overview, the
 * viewer's `me`, the home and clash provinces (latest and at the clash
 * bell), the bell-region record and the clash report of the first march,
 * and the first page of events — with a manifest (sha256 per file) that
 * web-frontier-recorded.test.mjs reads to verify the clash offline.
 */
async function record(dir, run) {
  mkdirSync(dir, { recursive: true });
  const get = async path => { const r = await fetch(`${HERALD}${path}`); if (!r.ok) throw new Error(`record ${path}: HTTP ${r.status}`); return new Uint8Array(await r.arrayBuffer()); };
  const files = {};
  const put = async (name, path) => {
    const b = await get(path);
    writeFileSync(`${dir}/${name}`, b);
    files[name] = { path, sha256: createHash('sha256').update(b).digest('hex'), bytes: b.length };
  };
  await put('season.json', '/h/season');
  const season = JSON.parse(Buffer.from(await get('/h/season')).toString('utf8'));
  for (const r of season.rings ?? []) await put(`overview-${r.d}-latest.bin`, `/h/overview/${r.d}/latest.bin`);
  await put('me.json', `/h/me/${run.wallet}`);
  const me = JSON.parse(Buffer.from(await get(`/h/me/${run.wallet}`)).toString('utf8'));
  const { p, q, bell } = run.clash;
  const { regionOf } = await import('../../../permutation-server/web/frontier/fgeo.mjs');
  await put(`clash-${p},${q}-${bell}.json`, `/h/clash/${p},${q}/${bell}`);
  await put(`province-${p},${q}-${bell}.json`, `/h/province/${p},${q}/${bell}`);
  await put(`province-${p},${q}-latest.json`, `/h/province/${p},${q}/latest`);
  await put(`bell-${bell}-region-${regionOf(p, q)}.json`, `/h/bell/${bell}/region/${regionOf(p, q)}`);
  for (const h of me.holdings ?? []) {
    const hp = h.p ?? h.province?.[0], hq = h.q ?? h.province?.[1];
    if (Number.isInteger(hp) && Number.isInteger(hq) && !(hp === p && hq === q)) await put(`province-${hp},${hq}-latest.json`, `/h/province/${hp},${hq}/latest`);
  }
  await put('events-0.json', '/h/events?after=0');
  const manifest = { format: 'frontier-web-recorded-v1', recordedAt: new Date().toISOString(), herald: HERALD, lang: run.lang, wallet: run.wallet, clash: run.clash, verify: run.verify, programId: season.programId, season: season.season, latestSlot: season.latestSlot, files };
  writeFileSync(`${dir}/manifest.json`, `${JSON.stringify(manifest, null, 1)}\n`);
  return { dir, files: Object.keys(files).length };
}
