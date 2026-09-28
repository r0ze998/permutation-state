// The W5-E page logic without a browser (node, like the gateway's web
// tests): the bottom sheet's steps, the map's terrain cache and first view,
// the faction sigils, the map buttons' names in both languages, and the
// site picker's adjacent-wedge offer (W4-E D8 folded into the picker).
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { setLang } from '../../permutation-server/web/lang.mjs';
import * as shell from '../../permutation-server/web/frontier/screens/shell.mjs';
import * as join from '../../permutation-server/web/frontier/screens/join.mjs';
import { createTerrain, TERRAIN_CACHE } from '../../permutation-server/web/frontier/map/terrain.mjs';
import { FrontierMap, MAP_TOOLS, TILE_FOGS, coveredBelow, LOD_EDGES } from '../../permutation-server/web/frontier/map/fmap.mjs';
import { SIGILS, paintSigil, paintVeil, FOG } from '../../permutation-server/web/frontier/map/layers.mjs';
import { ringProvinces, wedgeOf } from '../../permutation-server/web/frontier/fgeo.mjs';
import { homeWedge } from '../../permutation-server/web/frontier/fland.mjs';

afterEach(() => setLang('ja'));

test('the bottom sheet: a tap cycles peek → half → full → peek; a drag moves one step; a short drag is a tap', () => {
  assert.deepEqual(shell.SHEET_STATES, ['peek', 'half', 'full']);
  assert.equal(shell.nextSheet('peek'), 'half');
  assert.equal(shell.nextSheet('half'), 'full');
  assert.equal(shell.nextSheet('full'), 'peek');
  assert.equal(shell.nextSheet(undefined), 'peek', 'an unknown state starts over');
  assert.equal(shell.dragSheet('half', -80), 'full', 'up expands');
  assert.equal(shell.dragSheet('half', 80), 'peek', 'down collapses');
  assert.equal(shell.dragSheet('full', -80), 'full', 'no step past full');
  assert.equal(shell.dragSheet('peek', 80), 'peek');
  assert.equal(shell.dragSheet('half', 10), null, 'under 24 px it is a tap');
  assert.equal(shell.sheetLabel('half'), 'パネルを広げる');
  setLang('en');
  assert.equal(shell.sheetLabel('full'), 'Shrink the panel');
  assert.equal(shell.mountSheet(undefined), null, 'no document: nothing to mount');
});

test('the map buttons: three, ASCII or symbol glyphs, names in both languages', () => {
  assert.deepEqual(MAP_TOOLS.map(t => t.id), ['in', 'out', 'home']);
  for (const t of MAP_TOOLS) assert.doesNotMatch(t.glyph, /[　-鿿＀-￯]/, `${t.id}: the glyph is not text to translate`);
  const ja = MAP_TOOLS.map(t => t.label());
  setLang('en');
  const en = MAP_TOOLS.map(t => t.label());
  assert.deepEqual(en, ['Zoom in', 'Zoom out', 'Go to my holding']);
  assert.notDeepEqual(ja, en);
  assert.deepEqual(TILE_FOGS, ['sight', 'known', 'clear'], 'a distant province is never drawn as tiles');
});

test('terrain for the tile LOD: lazy module, one onReady, cached (LRU), null without a seed or a module', async () => {
  let loads = 0, calls = 0, ready = 0;
  const kernel = { call: (name, a) => { calls++; assert.equal(name, 'generate_province'); assert.equal(a.ring_seed.length, 32); return { ok: true, value: { terrain: { terrain: new Array(61).fill(a.p & 3), sites: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], siteCount: 10 } } }; } };
  const seeds = { 0: '01'.repeat(32), 1: '02'.repeat(32), 2: '03'.repeat(32) };
  const t = createTerrain({ load: async () => { loads++; return kernel; }, seedOf: d => seeds[d] ?? null, onReady: () => ready++, limit: 3 });
  assert.equal(t.status(), 'loading');
  assert.equal(t(2, 0), null, 'the first ask starts the load');
  assert.equal(t(2, 0), null, 'still loading: no second load');
  await new Promise(r => setTimeout(r, 0));
  assert.equal(loads, 1);
  assert.equal(ready, 1);
  assert.equal(t.status(), 'ready');
  const a = t(2, 0);
  assert.equal(a.terrain.length, 61);
  assert.equal(a.sites.length, 10, 'sites beyond siteCount do not exist');
  assert.equal(t(2, 0), a, 'cached');
  assert.equal(calls, 1);
  assert.equal(t(9, 0), null, 'ring 9 has no seed');
  t(1, 0); t(0, 1); t(1, -1);
  t(2, 0);
  assert.equal(calls, 5, 'the oldest entry was evicted past the limit, so (2,0) is generated again');
  assert.equal(TERRAIN_CACHE, 256);
  const failed = createTerrain({ load: async () => { throw new Error('no wasm'); } });
  assert.equal(failed(0, 0), null);
  await new Promise(r => setTimeout(r, 0));
  assert.equal(failed.status(), 'unavailable');
  assert.equal(failed(0, 0), null, 'a missing module leaves province detail');
});

test('the map\'s first view fits the open rings above the sheet, at world or province LOD; never after the viewer moved it', () => {
  const m = new FrontierMap({ clientWidth: 390, clientHeight: 700 }, { source: () => ({ overviews: new Map(), ringsOpen: 3 }) });
  assert.equal(coveredBelow({}), 0, 'no page: nothing covers the canvas');
  m.fit({ ringsOpen: 3 }, { width: 390, height: 700 });
  assert.ok(m.view.zoom > 0.06 && m.view.zoom < LOD_EDGES.provinceOut, `zoom ${m.view.zoom}`);
  assert.equal(m.lod, 'world');
  m.setView({ zoom: 0.3 });
  m.fitted = false;
  m.fit({ ringsOpen: 3 }, { width: 390, height: 700 });
  assert.equal(m.view.zoom, 0.3, 'a moved view is kept');
  m.destroy();
});

/** A 2D context that records paths and fills. */
function recorder() {
  const log = [];
  let n = 0;
  return {
    log,
    beginPath: () => { n = 0; }, moveTo: () => { n++; }, lineTo: () => { n++; }, closePath: () => {}, arc: () => { n += 100; },
    fill() { log.push({ fill: this.fillStyle, points: n }); }, stroke() { log.push({ stroke: this.strokeStyle, width: this.lineWidth }); },
  };
}

test('faction sigils: a distinct shape per faction (colour is never the only signal), outlined in ink', () => {
  assert.equal(new Set(SIGILS).size, 7);
  const shapes = new Set();
  for (let f = 0; f <= 6; f++) {
    const ctx = recorder();
    paintSigil(ctx, { x: 0, y: 0, r: 10, faction: f });
    const fill = ctx.log.find(e => 'fill' in e), stroke = ctx.log.find(e => 'stroke' in e);
    assert.ok(fill && stroke, `faction ${f}`);
    assert.equal(stroke.stroke, '#1b2e28');
    shapes.add(`${SIGILS[f]}:${fill.points}`);
  }
  assert.equal(shapes.size, 7);
  const v = recorder();
  paintVeil(v, { p: 2, q: 0, fog: 'known', scale: 1, selected: true });
  assert.ok(v.log.some(e => String(e.fill).includes(String(FOG.known))), 'the known veil');
  assert.ok(v.log.some(e => e.stroke === '#1b2e28' && e.width === 4), 'the selection outline');
});

/** Overviews of rings 2 and 3 with every home-wedge site of faction 0 held. */
function overviews(fullHome) {
  const map = new Map();
  for (const d of [2, 3]) {
    const provinces = ringProvinces(d).map(({ p, q }) => {
      const full = fullHome && wedgeOf(p, q) === homeWedge(0);
      return { p, q, ring: d, sites: new Array(12).fill(full ? 1 : 0), owners: new Array(12).fill(full ? 3 : 7) };
    });
    map.set(d, { ring: d, provinces });
  }
  return map;
}

test('the site picker: the home wedge first; a full home wedge offers the adjacent wedges\' outermost ring (§5.9, W4-E D8)', () => {
  const base = { citizen: { faction: 0 }, record: { rings: [{}, {}, {}, {}] }, land: { stage: 'joined', escrowNeeded: 7_152_640n } };
  const open = join.sitePicker({ ...base, overviews: overviews(false) });
  assert.equal(open.overflow, null);
  assert.ok(open.provinces.length > 0 && open.provinces.every(p => wedgeOf(p.p, p.q) === homeWedge(0)));
  const full = join.sitePicker({ ...base, overviews: overviews(true) });
  assert.deepEqual(full.overflow, { ring: 3 });
  assert.ok(full.provinces.length > 0);
  for (const p of full.provinces) {
    assert.equal(p.ring, 3);
    assert.notEqual(wedgeOf(p.p, p.q), homeWedge(0));
  }
});
