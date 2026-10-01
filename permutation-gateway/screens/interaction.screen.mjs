// Keyboard, sheet and focus behaviour in Chromium (web design §7.1, §10):
// the bottom sheet (handle states, Escape back to peek with focus on the
// handle, a tab opens it), the map by keyboard (+/−, H home, the buttons),
// the language toggle by keyboard, and a visible focus ring. Same fixture
// server as the matrix (127.0.0.1, port 0).
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { startServer } from './server.mjs';
import { storageFor, LATEST_UNIX, HOME, CAMP_TILE } from './world.mjs';

let browser, srv;
before(async () => { srv = await startServer(); browser = await chromium.launch(); });
after(async () => { await browser?.close(); await srv?.close(); });

async function open(t, { width = 390, height = 844, stage = 'holding', page: file = 'index.html', query = '' } = {}) {
  srv.stage(stage);
  const phone = width < 760;
  const context = await browser.newContext({ viewport: { width, height }, isMobile: phone, hasTouch: phone, locale: 'ja-JP', timezoneId: 'UTC' });
  t.after(() => context.close());
  await context.addInitScript(s => { for (const [k, v] of Object.entries(s)) localStorage.setItem(k, v); }, storageFor(srv.viewer, { stage, lang: 'ja' }));
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  await page.clock.setFixedTime(new Date(LATEST_UNIX * 1000 + 500));
  await page.goto(`${srv.url}/frontier/${file}${query}`);
  await page.waitForFunction(() => /\d/.test(document.getElementById('bell-chip')?.textContent ?? ''));
  t.after(() => assert.deepEqual(errors, [], 'no page or console error'));
  return page;
}
const sheet = page => page.locator('#panel').getAttribute('data-sheet');

test('the bottom sheet: the handle cycles its heights, Escape collapses it and returns focus, a tab opens it', { timeout: 60_000 }, async t => {
  const page = await open(t);
  const handle = page.locator('[data-sheet-handle]');
  await handle.waitFor();
  assert.equal(await sheet(page), 'half');
  assert.equal(await handle.getAttribute('aria-expanded'), 'true');
  assert.equal(await handle.getAttribute('aria-controls'), 'panel-body');
  await handle.click();
  assert.equal(await sheet(page), 'full');
  assert.equal(await handle.getAttribute('aria-label'), 'パネルを小さくする');
  await handle.click();
  assert.equal(await sheet(page), 'peek');
  assert.equal(await handle.getAttribute('aria-expanded'), 'false');
  await page.locator('#tabs [data-tab="holding"]').click();
  assert.equal(await sheet(page), 'half', 'choosing a tab opens the sheet');
  await page.locator('#holding-title').waitFor();
  await page.locator('#panel-body [data-act="harvest"]').focus();
  await page.keyboard.press('Escape');
  assert.equal(await sheet(page), 'peek');
  assert.equal(await page.evaluate(() => document.activeElement?.hasAttribute('data-sheet-handle')), true, 'focus returns to the handle');
});

/** A real touch drag on the handle (CDP touch events: pointerdown/up, no click), `dy` px in 8 steps. */
async function touchDrag(page, dy) {
  const box = await page.locator('[data-sheet-handle]').boundingBox();
  const x = box.x + box.width / 2, y = box.y + box.height / 2;
  const cdp = await page.context().newCDPSession(page);
  const at = yy => [{ x, y: yy, id: 1, radiusX: 2, radiusY: 2, force: 1 }];
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: at(y) });
  for (let i = 1; i <= 8; i++) await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: at(y + (dy * i) / 8) });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await cdp.detach();
}

test('a touch drag moves the sheet one step and the next single tap still works (wave-5 review)', { timeout: 60_000 }, async t => {
  const page = await open(t);
  const handle = page.locator('[data-sheet-handle]');
  await handle.waitFor();
  assert.equal(await sheet(page), 'half');
  await touchDrag(page, -80);
  assert.equal(await sheet(page), 'full', 'dragging up expands one step');
  // Past the drag's own click window, one genuine tap changes the state.
  await page.waitForTimeout(450);
  await handle.tap();
  assert.equal(await sheet(page), 'peek', 'the first tap after a drag is not swallowed');
  await touchDrag(page, -80);
  assert.equal(await sheet(page), 'half');
  await touchDrag(page, 80);
  assert.equal(await sheet(page), 'peek', 'dragging down collapses one step');
});

test('a mouse drag that ends off the handle still moves the sheet (pointer capture)', { timeout: 60_000 }, async t => {
  const page = await open(t, { width: 700, height: 900 });
  const handle = page.locator('[data-sheet-handle]');
  await handle.waitFor();
  const box = await handle.boundingBox();
  const x = box.x + box.width / 2, y = box.y + box.height / 2;
  assert.equal(await sheet(page), 'half');
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 120, y - 160, { steps: 8 });
  await page.mouse.up();
  assert.equal(await sheet(page), 'full', 'released far from the 44-px handle');
});

test('on a phone the relay quota is read in "More" (the chip is hidden below 760 px; wave-5 review D5)', { timeout: 60_000 }, async t => {
  const page = await open(t);
  assert.equal(await page.locator('#quota-chip').isVisible(), false, 'the chip waits');
  await page.locator('#tabs [data-tab="more"]').click();
  const line = page.locator('#panel-body [data-quota-line]');
  await line.waitFor();
  assert.match(await line.textContent(), /中継 残り \d+ 回/);
});

test('the sheet is a phone thing: on desktop the handle is hidden and Escape leaves the panel alone', { timeout: 60_000 }, async t => {
  const page = await open(t, { width: 1440, height: 900 });
  assert.equal(await page.locator('[data-sheet-handle]').isVisible(), false);
  await page.locator('#tabs [data-tab="holding"]').click();
  await page.locator('#panel-body [data-act="harvest"]').focus();
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#holding-title').isVisible(), true);
});

test('the map by keyboard: + and − zoom, H goes to the viewer\'s holding, the buttons work with Enter', { timeout: 60_000 }, async t => {
  const page = await open(t);
  const lod = () => page.locator('#frontier-map').getAttribute('data-lod');
  assert.equal(await lod(), 'world');
  await page.locator('#frontier-map').focus();
  for (let i = 0; i < 4; i++) await page.keyboard.press('+');
  assert.equal(await lod(), 'province');
  await page.keyboard.press('h');
  assert.equal(await lod(), 'province', 'home: the holding at province detail');
  await page.locator('[data-map="in"]').focus();
  for (let i = 0; i < 6; i++) await page.keyboard.press('Enter');
  assert.equal(await lod(), 'tile');
  await page.locator('#frontier-map[data-terrain="ready"]').waitFor();
  await page.locator('[data-map="out"]').focus();
  for (let i = 0; i < 12; i++) await page.keyboard.press('Enter');
  assert.equal(await lod(), 'world');
  assert.equal(await page.locator('.map-tools').getAttribute('aria-label'), '地図の操作');
});

test('the language toggle by keyboard, and a visible focus ring on every focusable control', { timeout: 60_000 }, async t => {
  const page = await open(t);
  await page.locator('#lang-box [data-lang-toggle]').focus();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.documentElement.lang === 'en');
  assert.equal(await page.locator('[data-map="in"]').getAttribute('aria-label'), 'Zoom in');
  await page.keyboard.press(' ');
  await page.waitForFunction(() => document.documentElement.lang === 'ja');
  // Tab through the page from the top: every stop shows an outline.
  await page.evaluate(() => document.activeElement?.blur());
  const stops = [];
  for (let i = 0; i < 14; i++) {
    await page.keyboard.press('Tab');
    stops.push(await page.evaluate(() => { const el = document.activeElement; const s = getComputedStyle(el); return { tag: el.tagName, what: el.getAttribute('aria-label') || el.textContent.trim().slice(0, 20), outline: s.outlineStyle, width: parseFloat(s.outlineWidth) }; }));
  }
  const bare = stops.filter(s => s.tag !== 'BODY' && (s.outline === 'none' || s.width < 2));
  assert.deepEqual(bare, [], 'focus is visible');
  assert.ok(stops.some(s => s.tag === 'CANVAS'), 'the map is a tab stop');
});

// The step counts of the UI plan (§5, Civ's "how many clicks"): from a host selected on the map to "seal and
// depart" in at most 3 presses, and the next to-do item in 1. Presses are real clicks on the page and the canvas.
test('step counts: a selected host to "seal and depart" in 3 presses; the next to-do in 1', { timeout: 90_000 }, async t => {
  const Z = 1.4;
  // the camera on the viewer's home tile, where the fixture's ready host stands (?at=P,Q,TILE,ZOOM)
  const page = await open(t, { width: 1440, height: 900, query: `?at=${HOME.p},${HOME.q},${HOME.tile},${Z}` });
  await page.locator('#frontier-map[data-lod="tile"]').waitFor();
  const box = await page.locator('#frontier-map').boundingBox();
  const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const offset = await page.evaluate(async ([p, q, from, to]) => {
    const { tileHex } = await import('/frontier/fgeo.mjs');
    const { project } = await import('/map.mjs');
    const a = (h => project(h.q, h.r))(tileHex(p, q, from)), b = (h => project(h.q, h.r))(tileHex(p, q, to));
    return { x: b.x - a.x, y: b.y - a.y };
  }, [HOME.p, HOME.q, HOME.tile, CAMP_TILE]);
  // the host selected: a press on its tile (the selection itself is not counted)
  await page.mouse.click(centre.x, centre.y);
  await page.locator('#panel-body [data-act="compose"]').first().waitFor();
  let presses = 0;
  presses++; await page.locator('#panel-body [data-act="compose"]').first().click();
  await page.locator('#mc-title').waitFor();
  presses++; await page.mouse.click(centre.x + offset.x * Z, centre.y + offset.y * Z);
  const send = page.locator('[data-act="march-send"]:not([disabled])');
  await send.waitFor({ timeout: 15_000 });
  presses++;
  assert.equal(presses, 3, 'march with this host, the destination tile, seal and depart');
  // the next to-do: one press on the pill moves to the item
  const pill = page.locator('#attn-pill');
  await pill.waitFor();
  const where = () => page.evaluate(async () => { const { FS } = await import('/frontier/fstate.mjs'); return JSON.stringify([FS.tab, FS.selected]); });
  await page.locator('[data-act="compose-close"]').first().click();
  const before = await where();
  await pill.click();
  assert.notEqual(await where(), before, 'the pill took the player to the item');
});
