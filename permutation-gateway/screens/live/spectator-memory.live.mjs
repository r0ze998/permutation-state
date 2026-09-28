// Spectator memory check (M1 contract §13.6 E7: "Spectator open 24 game
// hours: memory growth ≤ 200 MB"). Opens spectate.html on a running local
// stack (the herald on base + 40) and keeps it open until the page's chain
// clock has advanced LIVE_GAME_HOURS (default 24) game hours — on an
// accelerated stack (Mode A, e.g. 20×: 72 min wall) — sampling after a
// forced garbage collection: the JS heap (CDP Performance.getMetrics
// JSHeapUsedSize), DOM nodes, documents and event listeners. Pass: the
// retained heap at the end minus the start ≤ LIVE_MAX_GROWTH_MB (200), no
// page error. The page's own poll runs on wall time (30 s), so an
// accelerated run polls fewer times than a real day; the fixture-server
// companion (spectator-polls.live.mjs) covers a real day's poll count.
//
//   LIVE_HERALD=http://127.0.0.1:41340 node --test live/spectator-memory.live.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const HERALD = (process.env.LIVE_HERALD ?? 'http://127.0.0.1:41340').replace(/\/$/, '');
const PAGE = `${HERALD}/frontier/frontier/spectate.html`;
const GAME_HOURS = Number(process.env.LIVE_GAME_HOURS ?? 24);
const MAX_GROWTH_MB = Number(process.env.LIVE_MAX_GROWTH_MB ?? 200);
const SAMPLE_MS = Number(process.env.LIVE_SAMPLE_MS ?? 60_000);
const WALL_LIMIT_MS = Number(process.env.LIVE_WALL_LIMIT_MS ?? 3 * 3600_000);
const ARTIFACTS = fileURLToPath(new URL('../artifacts/live/', import.meta.url));
const sleep = ms => new Promise(r => setTimeout(r, ms));

test(`spectator open ${GAME_HOURS} game hours on the local stack: retained JS heap growth ≤ ${MAX_GROWTH_MB} MB`, { timeout: WALL_LIMIT_MS + 120_000 }, async () => {
  mkdirSync(ARTIFACTS, { recursive: true });
  const browser = await chromium.launch();
  const out = { page: PAGE, gameHours: GAME_HOURS, maxGrowthMb: MAX_GROWTH_MB, samples: [], errors: [] };
  try {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, locale: 'ja-JP', timezoneId: 'UTC' });
    const page = await context.newPage();
    page.on('pageerror', e => out.errors.push(`page error: ${e.message}`));
    page.on('console', m => { if (m.type() === 'error') out.errors.push(`console error: ${m.text()}`); });
    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    await page.goto(PAGE);
    await page.waitForFunction(() => /\d/.test(document.getElementById('bell-chip')?.textContent ?? ''));
    await page.locator('#bell-title').waitFor();
    const sample = async () => {
      await cdp.send('HeapProfiler.collectGarbage');
      const m = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(x => [x.name, x.value]));
      const s = await page.evaluate(async () => {
        const { FS } = await import('./fstate.mjs');
        const { bellAt } = await import('./clock.mjs');
        const now = FS.chain?.now() ?? null;
        return { chainNow: now, bell: FS.clock && now !== null ? bellAt(FS.clock.genesisTs, now) : null, chronicle: FS.chronicle?.length ?? null, provinces: FS.provinces?.size ?? null, reports: document.querySelectorAll('[data-act="report-open"]').length };
      });
      const row = { wallMs: Date.now() - t0, ...s, heapMb: +(m.JSHeapUsedSize / 2 ** 20).toFixed(2), heapTotalMb: +(m.JSHeapTotalSize / 2 ** 20).toFixed(2), nodes: m.Nodes, documents: m.Documents, listeners: m.JSEventListeners };
      out.samples.push(row);
      console.log(`[${(row.wallMs / 1000).toFixed(0)} s] bell ${row.bell} heap ${row.heapMb} MB nodes ${row.nodes} listeners ${row.listeners} chronicle ${row.chronicle}`);
      writeFileSync(`${ARTIFACTS}spectator-memory.json`, `${JSON.stringify(out, null, 1)}\n`);
      return row;
    };
    const t0 = Date.now();
    const first = await sample();
    let last = first;
    while (Date.now() - t0 < WALL_LIMIT_MS) {
      await sleep(SAMPLE_MS);
      last = await sample();
      if (last.chainNow - first.chainNow >= GAME_HOURS * 3600) break;
    }
    out.gameHoursWatched = +((last.chainNow - first.chainNow) / 3600).toFixed(2);
    out.growthMb = +(last.heapMb - first.heapMb).toFixed(2);
    out.maxHeapMb = Math.max(...out.samples.map(s => s.heapMb));
    out.bells = [first.bell, last.bell];
    writeFileSync(`${ARTIFACTS}spectator-memory.json`, `${JSON.stringify(out, null, 1)}\n`);
    await page.screenshot({ path: `${ARTIFACTS}spectator-end.png` });
    await context.close();
    assert.ok(out.gameHoursWatched >= GAME_HOURS, `watched ${out.gameHoursWatched} game hours of ${GAME_HOURS}`);
    assert.ok(out.growthMb <= MAX_GROWTH_MB, `retained heap grew ${out.growthMb} MB`);
    assert.deepEqual(out.errors, []);
  } finally {
    await browser.close();
  }
});
