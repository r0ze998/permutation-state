// The Frontier map's pure parts (permutation-server/web/frontier/map/*):
// levels of detail with hysteresis, the screen/world projection and zoom
// around a point, culling to the provinces in view, picking a tile and its
// province under a point (v9's hex inverse + the province lattice),
// province cells that tile the plane, presentation-only fog with the "show
// everything" switch, fills from the overview; and the canvas class
// running without a DOM. Plus v9's map.mjs: only additive exports.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import * as fmap from '../../permutation-server/web/frontier/map/fmap.mjs';
import * as layers from '../../permutation-server/web/frontier/map/layers.mjs';
import * as geo from '../../permutation-server/web/frontier/fgeo.mjs';
import * as v9map from '../../permutation-server/web/map.mjs';
import { FACTION_COLORS } from '../../permutation-server/web/frontier/fi18n.mjs';

const size = { width: 390, height: 700 };

test('LOD with hysteresis: no flicker at an edge', () => {
  assert.equal(fmap.lodFor(0.05), 'world');
  assert.equal(fmap.lodFor(0.14), 'province');
  assert.equal(fmap.lodFor(0.13, 'province'), 'province', 'between the edges the level holds');
  assert.equal(fmap.lodFor(0.13, 'world'), 'world');
  assert.equal(fmap.lodFor(0.11, 'province'), 'world');
  assert.equal(fmap.lodFor(0.5, 'province'), 'tile');
  assert.equal(fmap.lodFor(0.47, 'tile'), 'tile');
  assert.equal(fmap.lodFor(0.44, 'tile'), 'province');
  assert.equal(fmap.lodFor(0.1, 'tile'), 'world');
});

test('projection round trip, and zoom keeps the point under the finger', () => {
  const view = { x: 1234, y: -567, zoom: 0.3 };
  const s = fmap.worldToScreen(view, size, 1500, -400);
  const w = fmap.screenToWorld(view, size, s.x, s.y);
  assert.ok(Math.abs(w.x - 1500) < 1e-9 && Math.abs(w.y + 400) < 1e-9);
  const z = fmap.zoomAround(view, size, 2, 100, 200);
  const before = fmap.screenToWorld(view, size, 100, 200), after = fmap.screenToWorld(z, size, 100, 200);
  assert.ok(Math.abs(before.x - after.x) < 1e-9 && Math.abs(before.y - after.y) < 1e-9);
  assert.equal(z.zoom, 0.6);
  assert.equal(fmap.zoomAround(view, size, 1000, 0, 0).zoom, fmap.ZOOM_MAX);
  assert.equal(fmap.zoomAround(view, size, 1e-6, 0, 0).zoom, fmap.ZOOM_MIN);
});

test('culling: the provinces in view, nearest first', () => {
  const far = fmap.visibleProvinces({ x: 0, y: 0, zoom: 0.02 }, size, 3);
  assert.equal(far.length, geo.provincesWithin(3), 'zoomed out, every province of rings 0–3');
  assert.deepEqual(far[0], { p: 0, q: 0 });
  const c = layers.provincePixel(2, 0);
  const near = fmap.visibleProvinces({ x: c.x, y: c.y, zoom: 1 }, size, 3);
  assert.deepEqual(near[0], { p: 2, q: 0 });
  assert.ok(near.length <= 12, `${near.length} at tile zoom (≤ 12 province records on a phone, web design §4.2)`);
});

test('picking: the tile under a point and its province', () => {
  const view = { x: 0, y: 0, zoom: 0.8 };
  for (const [q, r] of [[0, 0], [9, -4], [3, 2], [-7, 5], [20, -11], [-13, -2]]) {
    const w = v9map.project(q, r);
    const s = fmap.worldToScreen(view, size, w.x, w.y);
    const hit = fmap.pick(view, size, s.x, s.y);
    const l = geo.locate(q, r);
    assert.deepEqual({ q: hit.tileQ, r: hit.tileR }, { q, r });
    assert.deepEqual([hit.p, hit.q, hit.idx], [l.p, l.q, l.idx]);
  }
  const c = geo.provinceCentre(2, 0);
  assert.equal(geo.locate(c.q, c.r).idx, 30, 'the centre tile is index 30');
});

test('province cells tile the plane: neighbours share two corners', () => {
  const near = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]) < 1e-6;
  for (const [p, q] of [[0, 0], [2, -1], [-3, 4]]) {
    const mine = layers.provinceCorners(p, q);
    const c = layers.provincePixel(p, q);
    for (const pt of mine) assert.ok(Math.abs(Math.hypot(pt[0] - c.x, (pt[1] - c.y) / v9map.FLATTEN) - layers.PROVINCE_CIRCUMRADIUS) < 1e-6);
    for (const [dq, dr] of geo.DIRECTIONS) {
      const theirs = layers.provinceCorners(p + dq, q + dr);
      const shared = mine.filter(a => theirs.some(b => near(a, b)));
      assert.equal(shared.length, 2, `(${p},${q}) and (${p + dq},${q + dr})`);
    }
  }
});

test('fog is presentation only: levels, and the show-everything switch clears it', () => {
  assert.equal(layers.fogLevel({ ringOpen: false, showAll: true }), 'unopened', 'an unopened ring has nothing to show');
  assert.equal(layers.fogLevel({ ringOpen: true, showAll: true, known: false }), 'clear');
  assert.equal(layers.fogLevel({ ringOpen: true, sightDistance: 2 }), 'sight');
  assert.equal(layers.fogLevel({ ringOpen: true, sightDistance: 3, known: true }), 'known');
  assert.equal(layers.fogLevel({ ringOpen: true, sightDistance: 3 }), 'distant');
  assert.equal(fmap.sightDistance(2, 0, [{ p: 0, q: 0 }, { p: 3, q: -1 }]), 1);
  assert.equal(fmap.sightDistance(2, 0, []), Infinity);
  assert.ok(layers.FOG.distant > layers.FOG.known && layers.FOG.sight === 0 && layers.FOG.clear === 0);
});

test('fills: the majority owner\'s colour, camps neutral, empty land plain', () => {
  const rec = { owners: [2, 2, 3, 7, 7, 7, 7, 7, 7, 7, 7, 7], sites: [1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2] };
  assert.equal(layers.provinceFill(rec), FACTION_COLORS[2]);
  assert.notEqual(layers.provinceFill({ owners: new Array(12).fill(7), sites: [...new Array(11).fill(0), 2] }), layers.provinceFill(null));
});

test('the canvas class runs without a DOM; the view drives the LOD', () => {
  const canvas = { clientWidth: 390, clientHeight: 700 };
  const seen = [];
  const m = new fmap.FrontierMap(canvas, { source: () => ({ overviews: new Map(), ringsOpen: 3 }), onView: (v, lod) => seen.push(lod) });
  m.draw();
  m.focus(2, 0, 0.6);
  assert.equal(m.lod, 'tile');
  m.setView({ zoom: 0.13 });
  assert.equal(m.lod, 'province');
  assert.deepEqual(seen, ['tile', 'province']);
  m.destroy();
});

test('v9 map.mjs: the Frontier needs only additive exports', t => {
  for (const n of ['project', 'hexPoints', 'inverseHex', 'COLORS', 'polygon', 'rounded', 'shade', 'EDGE_NEIGHBOR', 'RADIUS', 'FLATTEN']) assert.ok(n in v9map, n);
  let diff;
  try {
    // Against the M1 base (d95fa25, as Gate W1's no-touch check): lines may be added, never removed or changed.
    diff = execFileSync('git', ['diff', '-U0', 'd95fa25', '--', 'permutation-server/web/map.mjs'], { cwd: new URL('../../', import.meta.url), encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {
    t.skip('no git history here');
    return;
  }
  const removed = diff.split('\n').filter(l => l.startsWith('-') && !l.startsWith('---'));
  assert.deepEqual(removed, [], 'no line of map.mjs removed or changed');
});
