import test from 'node:test';
import assert from 'node:assert/strict';
import { CivilizationMap } from './map.mjs';
import { BUILDINGS, createCivilization, applyCivilizationAction } from './core.mjs';

// Canvas calls are recorded, not rasterized: these are interaction/geometry
// regressions. Screenshot review remains a separate visual acceptance check.
function createContext() {
  const calls = [];
  const context = { calls };
  const methods = [
    'arc', 'beginPath', 'bezierCurveTo', 'clip', 'closePath', 'drawImage', 'ellipse',
    'fill', 'fillRect', 'fillText', 'lineTo', 'moveTo', 'quadraticCurveTo', 'restore',
    'save', 'scale', 'setLineDash', 'setTransform', 'stroke', 'translate',
  ];
  for (const method of methods) context[method] = (...args) => calls.push({ method, args });
  context.measureText = (text) => ({ width: String(text).length * 9 });
  for (const method of ['createLinearGradient', 'createRadialGradient']) {
    context[method] = (...args) => {
      calls.push({ method, args });
      return { addColorStop() {} };
    };
  }
  return context;
}

function createCanvas({ width = 1400, height = 900, left = 0, top = 0 } = {}) {
  const context = createContext();
  const attributes = new Map(), listeners = new Map(), captures = new Set();
  const rect = { width, height, left, top };
  return {
    context, attributes, listeners, captures, rect, width: 0, height: 0, style: {},
    getContext: () => context,
    getBoundingClientRect: () => ({ ...rect }),
    hasAttribute: (name) => attributes.has(name),
    setAttribute: (name, value) => attributes.set(name, String(value)),
    addEventListener(type, handler) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(handler);
    },
    removeEventListener(type, handler) { listeners.get(type)?.delete(handler); },
    setPointerCapture(id) { captures.add(id); },
    hasPointerCapture(id) { return captures.has(id); },
    releasePointerCapture(id) { captures.delete(id); },
    dispatch(type, values = {}) {
      const event = { button: 0, pointerId: 1, clientX: 0, clientY: 0,
        defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...values };
      for (const handler of listeners.get(type) || []) handler(event);
      return event;
    },
  };
}

function fixture(t, { width = 1400, height = 900, dpr = 2, callbacks = {}, ...position } = {}) {
  const globals = ['window', 'document', 'ResizeObserver', 'requestAnimationFrame', 'cancelAnimationFrame'];
  const previous = new Map(globals.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const frames = new Map(), cancelled = [], observers = [];
  let frameId = 0;
  globalThis.window = { devicePixelRatio: dpr };
  globalThis.document = { hidden: false, createElement: (tag) => {
    assert.equal(tag, 'canvas'); return createCanvas({ width, height });
  } };
  globalThis.requestAnimationFrame = (callback) => { frames.set(++frameId, callback); return frameId; };
  globalThis.cancelAnimationFrame = (id) => { cancelled.push(id); frames.delete(id); };
  globalThis.ResizeObserver = class {
    constructor(callback) { this.callback = callback; this.disconnected = false; observers.push(this); }
    observe(target) { this.target = target; }
    disconnect() { this.disconnected = true; }
  };
  const canvas = createCanvas({ width, height, ...position });
  const map = new CivilizationMap(canvas, callbacks);
  const world = createCivilization({ sessionId: 'renderer-test', nowMs: 0 });
  applyCivilizationAction(world, { type: 'JOIN', actorId: 'mara', name: 'マラ' }, 0);
  map.setState(world, 'mara');
  const result = { map, world, canvas, frames, cancelled, observers,
    frame(now = 100) {
      const [id, callback] = frames.entries().next().value || [];
      assert.ok(callback, 'an animation frame must be scheduled');
      frames.delete(id); callback(now);
    },
  };
  t.after(() => {
    map.destroy();
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  });
  return result;
}

function close(actual, expected, tolerance = 1e-7) {
  assert.ok(Math.abs(actual - expected) < tolerance, `${actual} should equal ${expected}`);
}

function worldPosition(map, tileId) {
  const point = map.screenPosition(tileId);
  return { x: (point.x - map.offset.x) / map.zoom, y: (point.y - map.offset.y) / map.zoom };
}

test('canvas is accessible, DPI-aware and clamps excessive backing resolution', t => {
  const { map, canvas, observers } = fixture(t, { dpr: 3 });
  assert.equal(canvas.tabIndex, 0);
  assert.match(canvas.attributes.get('aria-label'), /ダブルクリック/);
  assert.equal(canvas.style.touchAction, 'none');
  assert.equal(canvas.width, 2800);
  assert.equal(canvas.height, 1800);
  assert.equal(map.cache.width, canvas.width);
  assert.equal(observers[0].target, canvas);
  canvas.rect.width = 900; canvas.rect.height = 700;
  observers[0].callback();
  assert.equal(map.width, 900); assert.equal(map.height, 700);
  assert.equal(canvas.width, 1800); assert.equal(canvas.height, 1400);
  assert.equal(map.dirty, true);
});

test('all 217 tile centers invert correctly before and after pan and zoom', t => {
  const { map, world } = fixture(t);
  assert.equal(world.tiles.length, 217);
  for (const zoom of [.55, 1, 1.75, 2.6]) {
    map.zoomBy(zoom / map.zoom);
    map.offset.x += 43; map.offset.y -= 21;
    for (const tile of world.tiles) {
      const point = map.screenPosition(tile.id);
      assert.equal(map._hit(point.x, point.y), tile.id);
      assert.equal(map._hit(point.x + 2, point.y - 1), tile.id);
    }
  }
  assert.equal(map.screenPosition('missing'), null);
  assert.equal(map._hit(-1e6, -1e6), null);
});

test('every lens renders every terrain, every building type and construction/blocked states', t => {
  const { map, world, frame } = fixture(t);
  world.tiles.forEach(tile => { tile.explored = true; });
  const types = Object.keys(BUILDINGS);
  // Nine since cb92a2f added the warehouse (eight buildable kinds plus the town hall).
  assert.equal(types.length, 9);
  const drawn = new Set(), drawShape = map._buildingShape.bind(map);
  map._buildingShape = (ctx, x, y, type) => { drawn.add(type); drawShape(ctx, x, y, type); };
  for (const [i, type] of types.entries()) {
    const tile = world.tiles.find(tile => tile.id === `${i - 3},0`);
    const id = `renderer-${type}`;
    tile.buildingId = id;
    world.buildings[id] = { id, type, tileId: tile.id, status: i % 3 === 1 ? 'building' : i % 3 === 2 ? 'blocked' : 'active',
      progress: .42, connected: i % 3 !== 2, localStock: {}, ownerId: 'mara', reason: '' };
  }
  map.setState(world, 'mara');
  for (const [index, lens] of ['normal', 'food', 'industry', 'logistics', 'danger'].entries()) {
    map.setLens(lens); map.setSelection('0,0'); map.hover = '1,0';
    assert.doesNotThrow(() => frame(100 + index * 40));
    assert.equal(map.lens, lens); assert.equal(map.dirty, false);
  }
  assert.deepEqual([...drawn].sort(), [...types].sort());
  assert.ok(map.ctx.calls.some(call => call.method === 'fillText' && call.args[0] === '共同倉庫'));
  map.setLens('invented-lens'); assert.equal(map.lens, 'normal');
});

test('unexplored terrain never draws resource scenery or hidden building labels', t => {
  const { map, world, frame } = fixture(t);
  const hidden = world.tiles.find(tile => tile.id === '3,0');
  hidden.buildingId = 'hidden-mine';
  world.buildings['hidden-mine'] = { id: 'hidden-mine', type: 'mine', tileId: hidden.id, status: 'active', connected: true };
  map.setState(world, 'mara');
  const decorated = [], decorate = map._drawDecor.bind(map);
  map._drawDecor = (ctx, tile) => { decorated.push(tile.id); decorate(ctx, tile); };
  frame();
  assert.ok(decorated.length > 0);
  assert.ok(decorated.every(id => world.tiles.find(tile => tile.id === id).explored));
  assert.ok(!decorated.includes(hidden.id));
  assert.ok(!map.ctx.calls.some(call => call.method === 'fillText' && call.args[0] === '鉱山'));
});

test('click selects; double-click requests a move without issuing game mutations', t => {
  const selected = [], moved = [];
  const { map, world, canvas } = fixture(t, { left: 37, top: 21,
    callbacks: { onSelect: id => selected.push(id), onMove: id => moved.push(id) } });
  const before = JSON.stringify(world), point = map.screenPosition('1,0');
  const event = { clientX: point.x + 37, clientY: point.y + 21, pointerId: 5 };
  canvas.dispatch('pointerdown', event);
  assert.equal(canvas.hasPointerCapture(5), true);
  canvas.dispatch('pointerup', event);
  assert.deepEqual(selected, ['1,0']); assert.deepEqual(moved, []);
  assert.equal(canvas.hasPointerCapture(5), false);
  const double = canvas.dispatch('dblclick', event);
  assert.equal(double.defaultPrevented, true);
  assert.deepEqual(moved, ['1,0']);
  assert.equal(JSON.stringify(world), before);
});

test('dragging pans without selecting, while right clicks and cancelled drags do nothing', t => {
  const selected = [];
  const { map, canvas } = fixture(t, { callbacks: { onSelect: id => selected.push(id) } });
  const initial = { ...map.offset }, point = map.screenPosition('0,0');
  canvas.dispatch('pointerdown', { clientX: point.x, clientY: point.y, button: 2 });
  assert.equal(map.pointer, undefined);
  canvas.dispatch('pointerdown', { clientX: point.x, clientY: point.y });
  canvas.dispatch('pointermove', { clientX: point.x + 45, clientY: point.y - 30 });
  canvas.dispatch('pointerup', { clientX: point.x + 45, clientY: point.y - 30 });
  close(map.offset.x, initial.x + 45); close(map.offset.y, initial.y - 30);
  assert.deepEqual(selected, []); assert.equal(canvas.style.cursor, 'grab');
  canvas.dispatch('pointerdown', { clientX: point.x, clientY: point.y });
  canvas.dispatch('pointercancel');
  canvas.dispatch('pointerup', { clientX: point.x, clientY: point.y });
  assert.deepEqual(selected, []); assert.equal(map.pointer, null);
});

test('hover only notifies when tiles change and clears on pointer leave', t => {
  const hovered = [];
  const { map, canvas } = fixture(t, { callbacks: { onHover: id => hovered.push(id) } });
  const center = map.screenPosition('0,0'), next = map.screenPosition('1,0');
  canvas.dispatch('pointermove', { clientX: center.x, clientY: center.y });
  canvas.dispatch('pointermove', { clientX: center.x + 2, clientY: center.y });
  canvas.dispatch('pointermove', { clientX: next.x, clientY: next.y });
  canvas.dispatch('pointerleave');
  assert.deepEqual(hovered, ['0,0', '1,0', null]);
  assert.equal(map.hover, null);
});

test('wheel zoom preserves the map point under the cursor and clamps limits', t => {
  const { map, canvas } = fixture(t);
  const anchor = map.screenPosition('1,-1'), previous = map.zoom;
  const wheel = canvas.dispatch('wheel', { clientX: anchor.x, clientY: anchor.y, deltaY: -100 });
  const after = map.screenPosition('1,-1');
  assert.equal(wheel.defaultPrevented, true); assert.ok(map.zoom > previous);
  close(after.x, anchor.x); close(after.y, anchor.y);
  map.zoomBy(100); assert.equal(map.zoom, 2.6);
  map.zoomBy(.0001); assert.equal(map.zoom, .55);
  for (const invalid of [NaN, Infinity, 0, -1]) map.zoomBy(invalid);
  assert.equal(map.zoom, .55);
});

test('keyboard navigation pans, focuses, zooms and only requests moves for a selected tile', t => {
  const moved = [];
  const { map, canvas } = fixture(t, { callbacks: { onMove: id => moved.push(id) } });
  const previous = { ...map.offset }, zoom = map.zoom;
  const event = canvas.dispatch('keydown', { key: 'ArrowRight' });
  assert.equal(event.defaultPrevented, true); close(map.offset.x, previous.x - 65);
  canvas.dispatch('keydown', { key: '+' }); assert.ok(map.zoom > zoom);
  canvas.dispatch('keydown', { key: '-' }); close(map.zoom, zoom);
  canvas.dispatch('keydown', { key: 'Enter' }); assert.deepEqual(moved, []);
  map.setSelection('1,0'); canvas.dispatch('keydown', { key: 'Enter' }); assert.deepEqual(moved, ['1,0']);
  canvas.dispatch('keydown', { key: 'f' }); assert.ok(map.targetOffset);
  map.setSelection('missing'); assert.equal(map.selection, null);
});

test('focus converges to a visible tile; narrow screens leave space for the bottom sheet', t => {
  const { map, canvas, observers, frame } = fixture(t, { width: 554, height: 956 });
  map.focusTile('2,-1');
  for (let i = 0; i < 50; i++) frame(100 + i * 40);
  const selected = map.screenPosition('2,-1');
  close(selected.x, 554 * .52); close(selected.y, 956 * .34);
  assert.equal(map.targetOffset, null);
  map.focusPlayer();
  for (let i = 0; i < 50; i++) frame(2200 + i * 40);
  const player = map.screenPosition('0,0');
  close(player.x, 554 * .52); close(player.y, 956 * .48);
  const previous = { ...map.offset };
  map.focusTile('missing'); assert.deepEqual(map.offset, previous);
  canvas.rect.width = 1200; canvas.rect.height = 850; observers[0].callback();
  map.focusTile('1,0');
  for (let i = 0; i < 50; i++) frame(4300 + i * 40);
  close(map.screenPosition('1,0').x, 1200 / 2 - 115);
});

test('citizen interpolation uses accepted path progress and real terrain speeds', t => {
  const { map, world } = fixture(t);
  const actor = { ...world.players.mara, path: ['1,0'], moveProgress: 292.5 };
  const origin = worldPosition(map, '0,0'), destination = worldPosition(map, '1,0');
  const halfRoad = map._modelPosition(actor, 0);
  close(halfRoad.x, (origin.x + destination.x) / 2); close(halfRoad.y, (origin.y + destination.y) / 2);
  const road = world.tiles.find(tile => tile.id === '1,0');
  road.road = false; road.terrain = 'mountain';
  const slower = map._modelPosition({ ...actor, moveProgress: 450 }, 0);
  close(slower.x, origin.x + (destination.x - origin.x) * .2);
  close(slower.y, origin.y + (destination.y - origin.y) * .2);
  const idle = map._modelPosition({ ...actor, path: [] }, 20000);
  assert.deepEqual(idle, origin, 'no accepted path means no invented movement');
});

test('couriers follow bends in their authoritative route rather than cutting across the map', t => {
  const { map, world } = fixture(t);
  const caravan = { id: 'bent-route', courier: true, fromTileId: '0,0', toTileId: '2,-1',
    route: ['0,0', '1,0', '2,0', '2,-1'], durationMs: 6000, elapsedMs: 3000, progress: .5,
    resource: 'wood', amount: 4, q: 1.5, r: 0 };
  world.caravans.push(caravan); map.setState(world, 'mara');
  const bendFrom = worldPosition(map, '1,0'), bendTo = worldPosition(map, '2,0');
  const modeled = map._modelPosition(caravan, 0);
  close(modeled.x, (bendFrom.x + bendTo.x) / 2); close(modeled.y, (bendFrom.y + bendTo.y) / 2);
  const start = worldPosition(map, '0,0'), end = worldPosition(map, '2,-1');
  assert.ok(Math.hypot(modeled.x - (start.x + end.x) / 2, modeled.y - (start.y + end.y) / 2) > 20);
  const entry = map.positions.get(caravan.id);
  map.ctx.calls.length = 0; map._drawUnits(map.ctx, entry.since);
  const cartShadow = map.ctx.calls.find(call => call.method === 'ellipse' && call.args[2] === 11 && call.args[3] === 4);
  assert.ok(cartShadow, 'the rendered courier must use its interpolated route position');
  close(cartShadow.args[0], modeled.x + 2); close(cartShadow.args[1], modeled.y + 4);
  const destination = map._modelPosition({ ...caravan, elapsedMs: 6000 }, 10000);
  close(destination.x, end.x); close(destination.y, end.y);
});

test('animation prediction stops after one polling interval and removes departed entities', t => {
  const { map, world } = fixture(t);
  world.players.mara.path = ['1,0', '2,0']; world.players.mara.moveProgress = 0;
  map.setState(world, 'mara');
  const since = map.positions.get('mara').since;
  const limited = map._entityPosition('mara', since + 700);
  assert.deepEqual(map._entityPosition('mara', since + 20_000), limited);
  assert.notDeepEqual(map._entityPosition('mara', since + 100), limited);
  assert.ok(map.positions.has('steward'));
  world.npcs = []; map.setState(world, 'mara');
  assert.equal(map.positions.has('steward'), false);
});

test('hidden tabs skip painting, static terrain is cached, destroy removes every listener and frame', t => {
  const { map, canvas, frames, cancelled, observers, frame } = fixture(t);
  let staticPaints = 0;
  const render = map._renderStatic.bind(map);
  map._renderStatic = () => { staticPaints++; render(); };
  frame(100); frame(140);
  assert.equal(staticPaints, 1);
  const paints = map.ctx.calls.length;
  document.hidden = true; frame(180);
  assert.equal(map.ctx.calls.length, paints);
  document.hidden = false; map.setLens('food'); frame(220);
  assert.equal(staticPaints, 2);
  assert.ok([...canvas.listeners.values()].some(listeners => listeners.size > 0));
  const pending = map.frame; map.destroy();
  assert.equal(map.destroyed, true); assert.equal(observers[0].disconnected, true);
  assert.ok([...canvas.listeners.values()].every(listeners => listeners.size === 0));
  assert.equal(frames.size, 0); assert.ok(cancelled.includes(pending));
  const finalPaints = map.ctx.calls.length; map._frame(300);
  assert.equal(map.ctx.calls.length, finalPaints); assert.equal(frames.size, 0);
});
