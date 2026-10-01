// The Frontier HUD models (web/frontier/hud/hud.mjs): the next-bell pill's
// bar and urgency, the resource strip's time to full, the attention items
// and the active holding.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as hud from '../../permutation-server/web/frontier/hud/hud.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

const clock = { genesisTs: 1000, endBell: 10_000 };
const store = (value, perHour, cap) => ({ value: value * 1000, frac: 0, rate: perHour * 1000, cap: cap * 1000, t0: 0 });
const holding = (p = 2, q = 0, stores = [store(100, 60, 200), store(0, 0, 100), store(50, 0, 50)]) => ({ p, q, site: 3, tier: 1, stores });

test('bell pill: bar fraction and urgency across the ten minutes', () => {
  const at = s => hud.bellModel(clock, 1000 + s);
  assert.equal(at(0).frac, 0);
  assert.equal(at(0).urgency, 'calm');
  assert.equal(at(300).frac, 0.5);
  assert.equal(at(600 - 121).urgency, 'calm');
  assert.equal(at(600 - 120).urgency, 'warn');
  assert.equal(at(600 - 30).urgency, 'crit');
  assert.equal(hud.bellModel(clock, 500).urgency, 'calm', 'before genesis: no urgency');
  assert.equal(hud.bellModel(null, null).frac, 0);
});

test('resource strip: only produced or held stores, time to full, full and near-full', () => {
  const r = hud.resourceModel(holding(), 0);
  assert.deepEqual(r.map(x => x.resource), ['Food', 'Stone'], 'an empty store with no production is left out');
  assert.equal(r[0].fullIn, 6000, '100 more at 60 an hour: 100 minutes');
  assert.equal(r[0].state, 'ok');
  assert.equal(r[1].state, 'full');
  assert.equal(r[1].fullIn, 0);
  assert.equal(hud.resourceModel(holding(), 3000)[0].state, 'near', 'within an hour of the cap');
  assert.deepEqual(hud.resourceModel(null, 0), []);
});

test('span: minutes, hours, days', () => {
  setLang('ja');
  assert.equal(hud.span(90), '1:30');
  assert.equal(hud.span(3 * 3600 + 20 * 60), '3時間20分');
  assert.equal(hud.span(5 * 86_400 + 5), '5日');
  setLang('en');
  assert.equal(hud.span(86_400), '1 day');
  assert.equal(hud.span(2 * 86_400), '2 days');
  setLang('ja');
});

test('attention: incoming first, then settlements, the unsent draft, full stores; the pill counts them', () => {
  const FS = {
    holdings: [holding()], chain: { now: () => 0 },
    incoming: [{ bell: 44, holding: { p: 2, q: 0 }, hosts: 2, troops: 900 }],
    marches: [{ facts: { settleReady: true }, dest: { p: 1, q: 1 } }, { facts: { settleReady: false }, dest: { p: 0, q: 0 } }],
    compose: { origin: { p: 2, q: 0 }, sending: false },
  };
  const items = hud.attentionItems(FS);
  assert.deepEqual(items.map(x => x.kind), ['incoming', 'settle', 'draft', 'full']);
  assert.deepEqual([items[1].p, items[1].q, items[1].tab], [1, 1, 'marches']);
  assert.equal(items[3].tab, 'holding');
  setLang('ja');
  assert.equal(hud.attentionText(items), '来襲 第44鐘 · ほか 3', 'the first item named, then how many more');
  assert.equal(hud.attentionText([]), null);
  FS.compose.sending = true;
  assert.ok(!hud.attentionItems(FS).some(x => x.kind === 'draft'), 'a march being sent is not a draft');
});

test('active holding: the chosen index, else the first', () => {
  const a = holding(1, 1), b = holding(2, 2);
  assert.equal(hud.activeHolding({ holdings: [a, b] }), a);
  assert.equal(hud.activeHolding({ holdings: [a, b], activeHolding: 1 }), b);
  assert.equal(hud.activeHolding({ holdings: [a], activeHolding: 4 }), a);
  assert.equal(hud.activeHolding({ holdings: [] }), null);
});

test('rail markup: holdings with the warning mark, tiles, the to-do list', () => {
  setLang('ja');
  const FS = { mode: 'play', tab: 'holding', citizen: { faction: 0 }, holdings: [holding()], chain: { now: () => 0 },
    incoming: [{ bell: 44, holding: { p: 2, q: 0 }, hosts: 1, troops: 100 }], marches: [] };
  const out = [hud.renderRail(FS)].flat(Infinity).map(String).join('');
  assert.match(out, /data-act="holding-pick" data-i="0" aria-current="true"/);
  assert.match(out, /来襲の恐れ/);
  assert.match(out, /data-act="tab" data-tab="holding" aria-pressed="true"/);
  assert.match(out, /data-act="attn-go" data-i="0"/);
});

import * as inspect from '../../permutation-server/web/frontier/hud/inspect.mjs';

test('inspector: a tile with a site, hosts and relations; the actions it offers', () => {
  setLang('ja');
  const prov = {
    p: 2, q: 0, relations: 0n, sites: Uint8Array.from([9, 20]), resolveSummary: { bell: 41 },
    siteMirror: [{ state: 1, faction: 2, tier: 1, garrison: 300000, shieldUntilBell: 0 }, { state: 0, faction: 7, tier: 0, garrison: 0, shieldUntilBell: 0 }],
    entries: [{ id: 5n, faction: 2, unit: 0, tile: 9, state: 1, troops: 400000 }, { id: 6n, faction: 2, unit: 0, tile: 3, state: 1, troops: 1 }], camp: { state: 0 },
  };
  const FS = { mode: 'play', citizen: { faction: 0 }, holdings: [], nowBell: 42, land: { stage: 'joined' },
    overviews: new Map([[2, { provinces: [{ p: 2, q: 0, owners: [2, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7], sites: [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], clash: true }] }]]),
    provinces: new Map([['2,0', { province: prov }]]), selected: { p: 2, q: 0, idx: 9 } };
  const terrainOf = () => ({ terrain: Array(61).fill(0), sites: [9, 20], names: ['Grassland'] });
  const m = inspect.inspectModel(FS, terrainOf);
  assert.equal(m.tile.terrain, 'Grassland');
  assert.deepEqual([m.tile.site.state, m.tile.site.faction, m.tile.site.garrison], ['holding', 2, 300]);
  assert.deepEqual(m.tile.hosts.map(h => h.troops), [400], 'only the hosts on this tile');
  assert.deepEqual(m.relation, [{ faction: 2, friendly: false }]);
  assert.deepEqual(inspect.inspectActions(FS, m).map(a => a.act), ['battle-play', 'report-open', 'pin-toggle']);
  FS.selected = { p: 2, q: 0, idx: 20 };
  const free = inspect.inspectModel(FS, terrainOf);
  assert.equal(free.tile.site.state, 'free');
  assert.deepEqual(inspect.inspectActions(FS, free).map(a => a.act), ['pick-province', 'battle-play', 'report-open', 'pin-toggle'], 'a free site first opens the province for the ticket');
  FS.joinDraft = { envelope: { province: prov }, sites: [] };
  assert.equal(inspect.inspectActions(FS, free)[0].act, 'toggle-site');
  FS.compose = { origin: { p: 2, q: 0 } };
  assert.ok(inspect.inspectActions(FS, free).some(a => a.act === 'dest-from-map'));
  assert.equal(inspect.friendly(1n << 2n, 0, 2), true, 'bit a*8+b marks not hostile');
});

import * as CTRL from '../../permutation-server/web/frontier/controller.mjs';
import { FS as STORE } from '../../permutation-server/web/frontier/fstate.mjs';

test('wantProvince: at most 12 in flight, slots free when a load ends, a stale copy is fetched again once per bell', async () => {
  const pending = [];
  CTRL.useHerald({ province: (p, q) => new Promise(res => pending.push(() => res({ ok: true, bell: 7, province: { p, q } }))) });
  STORE.provinces = new Map();
  let started = 0;
  for (let i = 0; i < 20; i++) if (CTRL.wantProvince(i, 0, () => {}, { now: 0, bell: 7 })) started++;
  assert.equal(started, CTRL.WANT_IN_FLIGHT);
  pending.splice(0).forEach(f => f());
  await new Promise(r => setTimeout(r, 0));
  for (let i = 12; i < 20; i++) CTRL.wantProvince(i, 0, () => {}, { now: 0, bell: 7 });
  pending.splice(0).forEach(f => f());
  await new Promise(r => setTimeout(r, 0));
  assert.equal(STORE.provinces.size, 20, 'past the old 12-province ceiling');
  assert.equal(CTRL.wantProvince(3, 0, () => {}, { now: 1000, bell: 7 }), false, 'fresh for its bell');
  assert.equal(CTRL.wantProvince(3, 0, () => {}, { now: 1000, bell: 8 }), false, 'a new bell, but fetched under 30 s ago');
  assert.equal(CTRL.wantProvince(3, 0, () => {}, { now: CTRL.WANT_REFRESH_MS + 1, bell: 8 }), true, 'stale: fetched again');
  pending.splice(0).forEach(f => f());
  STORE.provinces = new Map();
});

import * as FEED from '../../permutation-server/web/frontier/hud/feed.mjs';

test('feed: changes between two polls become notifications; a turned bell folds into a summary; nothing at first sight', () => {
  setLang('ja');
  const base = { nowBell: 40, chain: { now: () => 0 }, holdings: [{ p: 2, q: 0, site: 3, state: 1, tier: 0, queue: [] }], incoming: [], chronicle: [],
    marches: [{ entry: { host: '7', arriveBell: 41 }, dest: { p: 3, q: 0 }, facts: { pipeline: 'open', slotPresent: false, settleReady: false } }] };
  const s0 = FEED.snapshot(base);
  assert.deepEqual(FEED.diffFeed(null, s0), [], 'the first snapshot is not news');
  const next = { ...base, nowBell: 41, holdings: [{ ...base.holdings[0], state: 2, queue: [{ kind: 1, doneAt: 1n }] }], chain: { now: () => 10 },
    incoming: [{ bell: 43, holding: { p: 2, q: 0 } }], chronicle: [{ record: { name: 'CLASH', p: 2, q: 0, bell: 40 } }],
    marches: [{ entry: { host: '7', arriveBell: 41 }, dest: { p: 3, q: 0 }, facts: { pipeline: 'resolved', slotPresent: true, settleReady: true } }] };
  const items = FEED.diffFeed(s0, FEED.snapshot(next));
  assert.deepEqual(items.map(x => x.kind).sort(), ['battle', 'battle', 'build', 'holding', 'incoming', 'march', 'march']);
  const res = items.find(x => x.id.startsWith('rs:'));
  assert.deepEqual(res.battle, { p: 3, q: 0, bell: 41 }, 'a resolved march offers its battle');
  let feed = FEED.pushFeed([], items, 1000);
  feed = FEED.pushFeed(feed, items, 2000);
  assert.equal(feed.length, items.length, 'no duplicates');
  const sum = FEED.bellSummary(feed.map(x => ({ ...x, bell: 41 })), 41);
  assert.match(sum.text, /^第41鐘のまとめ：戦い 2 · 進軍の知らせ 2 · 来襲の恐れ 1 · 完成 1 · 拠点 1$/);
  assert.equal(FEED.bellSummary([], 41), null);
  const toasts = [FEED.renderToasts(feed, { now: 1500 })].flat(Infinity).map(String).join('');
  assert.equal((toasts.match(/class="toast /g) ?? []).length, FEED.TOAST_MAX);
  assert.match(toasts, /data-act="feed-dismiss"/);
  assert.equal([FEED.renderToasts(feed, { now: 1000 + FEED.TOAST_MS + 1 })].flat(Infinity).join(''), '', 'toasts expire into the list');
});

import * as MC from '../../permutation-server/web/frontier/hud/marchcard.mjs';

test('march card: the route as hexes, the defenders frozen at the destination, a guide word from the ratio', () => {
  setLang('ja');
  const c = { origin: { p: 2, q: 0 }, host: { tile: 30, troops: 600 }, route: { dirs: [0, 0, 5] } };
  const hx = MC.routeHexes(c);
  assert.equal(hx.length, 4);
  assert.deepEqual([hx[3].q - hx[0].q, hx[3].r - hx[0].r], [2, 1], 'two steps east, one south-east (fgeo DIRECTIONS)');
  const prov = { sites: Uint8Array.from([32]), siteMirror: [{ state: 1, faction: 2, garrison: 100_000 }], camp: { state: 1, tile: 32, troops: 50 },
    entries: [{ state: 1, tile: 32, faction: 2, troops: 200_000 }, { state: 1, tile: 32, faction: 0, troops: 900_000 }, { state: 1, tile: 5, faction: 2, troops: 1 }] };
  const FS = { citizen: { faction: 0 }, provinces: new Map([['2,0', { province: prov }]]) };
  const d = MC.defendersAt(FS, { p: 2, q: 0, tile: 32 });
  assert.deepEqual([d.residents.map(r => r.troops), d.garrison, d.camp, d.total], [[200], 100, 50, 350], 'other factions only, on that tile');
  assert.equal(MC.oddsWord(800, 350).id, 'strong');
  assert.equal(MC.oddsWord(350, 350).id, 'even');
  assert.equal(MC.oddsWord(100, 350).id, 'bad');
  assert.equal(MC.oddsWord(100, 0).id, 'free');
});

import * as I from '../../permutation-server/web/frontier/people/identity.mjs';
import * as MINI from '../../permutation-server/web/frontier/hud/minimap.mjs';
import * as SEARCH from '../../permutation-server/web/frontier/hud/search.mjs';

test('minimap: the majority holder of a province, a frame that maps world and minimap both ways', () => {
  assert.equal(MINI.majority({ owners: [2, 2, 1, 7, 7, 7, 7, 7, 7, 7, 7, 7], sites: [1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0] }), 2);
  assert.equal(MINI.majority({ owners: Array(12).fill(7), sites: Array(12).fill(0) }), null);
  const fr = MINI.frameOf(4, 188);
  const p = fr.toPx(123, -45), w = fr.toWorld(p.x, p.y);
  assert.ok(Math.abs(w.x - 123) < 1e-6 && Math.abs(w.y + 45) < 1e-6);
  assert.deepEqual(fr.toPx(0, 0), { x: 94, y: 94 }, 'the Concord at the centre');
  assert.deepEqual(MINI.LENSES, ['realm', 'war', 'land', 'settle']);
});

test('map search: a province by coordinates, a faction by name, a lord by derived name', () => {
  setLang('ja');
  const recs = new Map([['3,0', { p: 3, q: 0, owners: [2, 2, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7], sites: [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }]]);
  assert.deepEqual(SEARCH.searchMap('州 3,0', { recs }).map(x => [x.kind, x.p, x.q]), [['province', 3, 0]]);
  assert.deepEqual(SEARCH.searchMap('province 3，0', { recs }).length, 1);
  assert.deepEqual(SEARCH.searchMap('9,9', { recs }), [], 'a province not open is not found');
  const roster = { entries: () => [{ p: 3, q: 0, site: 1, tag: 9n, tier: 0 }] };
  const name = I.displayName(I.identityOf(9n), { language: 'en' });
  const hit = SEARCH.searchMap(name.slice(0, 4), { recs, roster, sitesOf: () => [5, 12] });
  assert.equal(hit[0].kind, 'lord');
  assert.equal(hit[0].tile, 12);
});
