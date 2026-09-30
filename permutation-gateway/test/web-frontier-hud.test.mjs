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
  assert.equal(hud.attentionText(items), '要対応 4');
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
