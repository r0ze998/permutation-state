// The screens of the smoke matrix (contract §13.6 E7, web design §13.2):
// the 10 screens — join/faction, site picker, map world LOD, map tile LOD
// with fog, holding, march composer, march tracker, bell sheet, clash
// report, practice result — plus the onboarding card and the spectator.
// Each scene names its page, the viewer stage the fixture herald answers,
// and how the test reaches it: clicks on the page's own controls
// (`data-act`, tabs, forms), never state written into the page.
import { HOSTS } from './world.mjs';

const click = async (page, sel) => { await page.locator(sel).first().waitFor({ state: 'visible' }); await page.locator(sel).first().click(); };
const tab = (page, id) => click(page, `#tabs [data-act="tab"][data-tab="${id}"]`);
/** Open the bottom sheet fully on a phone (no-op on desktop, where the panel is a column). */
async function sheetFull(page) {
  const h = page.locator('[data-sheet-handle]');
  if (!(await h.isVisible().catch(() => false))) return;
  for (let i = 0; i < 3 && (await page.locator('#panel').getAttribute('data-sheet')) !== 'full'; i++) await h.click();
}
/** Wait until the page's text no longer contains `text` (a pending state). */
const gone = (page, text) => page.waitForFunction(t => !document.body.textContent.includes(t), text);

export const SCENES = [
  {
    id: 'join', title: 'join and faction', page: 'index.html', stage: 'none',
    async go(page) {
      await click(page, '[data-act="pick-faction"][data-f="2"]');
      await page.locator('[data-act="join"]:not([disabled])').waitFor();
    },
  },
  {
    id: 'sites', title: 'site picker', page: 'index.html', stage: 'joined',
    async go(page) {
      await click(page, '[data-act="pick-province"]');
      await click(page, '[data-act="toggle-site"]');
      await page.locator('[data-act="file-ticket"]:not([disabled])').waitFor();
    },
  },
  {
    id: 'map-world', title: 'map, world LOD', page: 'index.html', stage: 'holding',
    async go(page) {
      await page.locator('#ob-title').waitFor();
      await page.locator('#frontier-map[data-lod="world"]').waitFor();
    },
  },
  {
    id: 'map-tile', title: 'map, tile LOD with fog', page: 'index.html', stage: 'holding',
    async go(page) {
      await click(page, '[data-map="home"]');
      for (let i = 0; i < 16 && !(await page.locator('#frontier-map[data-lod="tile"]').count()); i++) await page.locator('[data-map="in"]').click();
      await page.locator('#frontier-map[data-lod="tile"][data-terrain="ready"]').waitFor();
    },
  },
  {
    id: 'holding', title: 'holding', page: 'index.html', stage: 'holding',
    async go(page) {
      await tab(page, 'holding');
      await page.locator('#holding-title').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'march', title: 'march composer', page: 'index.html', stage: 'holding',
    async go(page) {
      await tab(page, 'hosts');
      await click(page, `[data-act="compose"][data-host="${HOSTS.free}"]`);
      await click(page, '[data-act="dest-quick"]');
      await page.locator('#march-title').waitFor();
      await gone(page, '行き先を決めると道のりを探します');
      // The planner found the way (the route line, not a refusal).
      await page.locator('section[aria-labelledby="march-title"] fieldset:nth-of-type(2) > p:first-of-type:not(.muted)').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'tracker', title: 'march tracker', page: 'index.html', stage: 'holding',
    async go(page) {
      await tab(page, 'marches');
      await page.locator('#tracker-title').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'bell', title: 'bell sheet', page: 'index.html', stage: 'holding',
    async go(page) {
      await tab(page, 'more');
      await page.locator('#bell-title').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'report', title: 'clash report', page: 'index.html', stage: 'holding',
    async go(page) {
      await tab(page, 'more');
      await click(page, '[data-act="report-open"]');
      await page.locator('#report-title').waitFor();
      await page.locator('[data-act="report-verify"]').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'practice', title: 'practice result', page: 'practice.html', stage: 'none',
    async go(page) {
      await click(page, 'form[data-form="practice-run"] button[type="submit"]');
      await page.locator('#practice-title ~ div[role="status"] table, [role="status"] h4').first().waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'onboarding', title: 'onboarding card', page: 'index.html', stage: 'joined',
    async go(page) {
      await page.locator('#ob-title').waitFor();
      await sheetFull(page);
    },
  },
  {
    id: 'spectate', title: 'spectator', page: 'spectate.html', stage: 'none',
    async go(page) {
      await page.locator('#bell-title').waitFor();
      await page.locator('[data-act="report-open"]').first().waitFor();
      await sheetFull(page);
    },
  },
];

export const VIEWPORTS = [
  { id: '360', width: 360, height: 740, phone: true },
  { id: '390', width: 390, height: 844, phone: true },
  { id: '1440', width: 1440, height: 900, phone: false },
];
