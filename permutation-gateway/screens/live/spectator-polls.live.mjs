// Spectator memory, the poll-count side (companion of
// spectator-memory.live.mjs): an accelerated stack shows 24 game hours of
// data in 72 wall minutes, but the page polls on wall time (every 30 s), so
// it polls 144 times where a real day polls 2,880. Here the spectator page
// runs on the fixture server (server.mjs, 127.0.0.1:0) under Playwright's
// fake clock, and the clock is run forward 24 hours in 30-s steps: every
// poll timer fires as it would in a real day (2,880 polls of the events,
// the bell sheet and the season record). Retained JS heap is sampled after
// a forced GC every simulated hour. Pass: growth ≤ LIVE_MAX_GROWTH_MB (200)
// and no page error.
//
//   node --test live/spectator-polls.live.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { startServer } from '../server.mjs';
import { LATEST_UNIX } from '../world.mjs';

const HOURS = Number(process.env.LIVE_HOURS ?? 24);
const MAX_GROWTH_MB = Number(process.env.LIVE_MAX_GROWTH_MB ?? 200);
const ARTIFACTS = fileURLToPath(new URL('../artifacts/live/', import.meta.url));

test(`spectator: ${HOURS} wall hours of polls (fake clock, ${HOURS * 120} polls): retained JS heap growth ≤ ${MAX_GROWTH_MB} MB`, { timeout: 30 * 60_000 }, async () => {
  mkdirSync(ARTIFACTS, { recursive: true });
  const srv = await startServer();
  const browser = await chromium.launch();
  const out = { hours: HOURS, maxGrowthMb: MAX_GROWTH_MB, samples: [], errors: [], requests: 0, byPath: {} };
  try {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, locale: 'ja-JP', timezoneId: 'UTC' });
    const page = await context.newPage();
    page.on('pageerror', e => out.errors.push(`page error: ${e.message}`));
    page.on('console', m => { if (m.type() === 'error') out.errors.push(`console error: ${m.text()}`); });
    // Per path shape too (integ-W6 review: the rate per bell per viewer against web design §4.2).
    page.on('request', r => { if (r.url().includes('/h/')) { out.requests++; const k = new URL(r.url()).pathname.replace(/\d+/g, 'N'); out.byPath[k] = (out.byPath[k] ?? 0) + 1; } });
    await page.clock.install({ time: new Date(LATEST_UNIX * 1000 + 500) });
    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    await page.goto(`${srv.url}/frontier/spectate.html`);
    await page.clock.runFor(2_000);
    await page.locator('#bell-title').waitFor();
    const sample = async hour => {
      await cdp.send('HeapProfiler.collectGarbage');
      const m = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(x => [x.name, x.value]));
      const row = { hour, requests: out.requests, heapMb: +(m.JSHeapUsedSize / 2 ** 20).toFixed(2), nodes: m.Nodes, listeners: m.JSEventListeners };
      out.samples.push(row);
      return row;
    };
    const first = await sample(0);
    for (let h = 1; h <= HOURS; h++) {
      for (let i = 0; i < 120; i++) {
        await page.clock.fastForward(30_000);
        // Let the fetches the timers started land (real time; the fixture server answers in ~1 ms).
        await page.waitForTimeout(8);
      }
      const row = await sample(h);
      console.log(`[hour ${h}] requests ${row.requests} heap ${row.heapMb} MB nodes ${row.nodes} listeners ${row.listeners}`);
    }
    const last = out.samples[out.samples.length - 1];
    out.growthMb = +(last.heapMb - first.heapMb).toFixed(2);
    out.maxHeapMb = Math.max(...out.samples.map(s => s.heapMb));
    writeFileSync(`${ARTIFACTS}spectator-polls.json`, `${JSON.stringify(out, null, 1)}\n`);
    await context.close();
    assert.ok(out.requests >= HOURS * 120, `the page polled ${out.requests} times`);
    assert.ok(out.growthMb <= MAX_GROWTH_MB, `retained heap grew ${out.growthMb} MB`);
    assert.deepEqual(out.errors, []);
  } finally {
    await browser.close();
    await srv.close();
  }
});
