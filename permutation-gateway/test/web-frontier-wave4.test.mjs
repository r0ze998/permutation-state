// Wave 4 of the UI plan: the clash report's headline and tiles (D1–D3), battle playback speed (D4).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
if (!globalThis.crypto) globalThis.crypto = webcrypto;
const rep = await import('../../permutation-server/web/frontier/screens/report.mjs');
const B = await import('../../permutation-server/web/frontier/people/battle.mjs');
const { setLang } = await import('../../permutation-server/web/lang.mjs');

const row = (o) => ({ id: '1', kind: 'arrival', faction: 0, before: 100, after: 100, posture: 0, fate: 'Stays', tile: 4, mine: false, ...o });

test('summaryOf: the viewer\'s result, losses and arrival; a spectator gets the side left on the field', () => {
  const won = rep.summaryOf([row({ mine: true, after: 70, posture: 1 }), row({ id: '2', kind: 'resident', faction: 2, before: 50, after: 0, fate: 'Destroyed' })]);
  assert.deepEqual([won.result, won.lost, won.before, won.reached], ['won', 30, 100, true]);
  const held = rep.summaryOf([row({ kind: 'garrison', mine: true, after: 80, fate: null }), row({ id: '2', faction: 1, after: 60 })]);
  assert.equal(held.result, 'held');
  assert.equal(held.reached, null, 'a garrison did not march');
  assert.equal(rep.summaryOf([row({ mine: true, after: 0, fate: 'Destroyed' })]).result, 'fell');
  assert.equal(rep.summaryOf([row({ mine: true, fate: 'Retreated' })]).result, 'turned');
  assert.equal(rep.summaryOf([row({ mine: true, after: null, fate: null })]).result, 'none');
  const watch = rep.summaryOf([row({ after: 20 }), row({ id: '2', faction: 3, after: 60 })]);
  assert.deepEqual([watch.mine, watch.leader, watch.lost], [false, 3, 120]);
});

test('tileDetail: per tile, losses taken and dealt, the stance edge, the defenders\' retaliation', () => {
  const t = rep.tileDetail([
    row({ faction: 0, posture: 1, after: 60 }),
    row({ id: '2', faction: 1, kind: 'resident', posture: 2, after: 10 }),
    row({ id: '3', faction: 1, kind: 'garrison', posture: 0, before: 30, after: 30, walls: 1, fate: null }),
    row({ id: '4', faction: 5, tile: 7, after: 100 }),
  ]);
  assert.equal(t.length, 1, 'a tile with one side is not a fight');
  const [x] = t;
  assert.equal(x.tile, 4);
  const by = Object.fromEntries(x.sides.map(s => [s.faction, s]));
  assert.deepEqual([by[0].lost, by[0].dealt, by[1].lost, by[1].dealt], [40, 90, 90, 40]);
  assert.deepEqual(x.edges.map(e => [e.winner, e.loser]), [[0, 1]], 'Assault beats Flank');
  assert.equal(x.retaliation, 40);
  assert.equal(x.walls, true);
});

test('the report headline in both languages: replay and map buttons, the proof folded', () => {
  const FS = { report: { p: 2, q: 0, bell: 40, clash: { inputs: { arrivals: [
    { present: 1, hostId: 9n, faction: 0, unit: 0, troops: 100000, troopsAfter: 40000, stance: 1, fate: 1, tile: 3 },
    { present: 1, hostId: 8n, faction: 2, unit: 0, troops: 90000, troopsAfter: 0, stance: 2, fate: 5, tile: 3 },
  ] } } } };
  setLang('ja');
  const ja = String(rep.render(FS, id => String(id) === '9'));
  assert.match(ja, /data-act="battle-play" data-p="2" data-q="0" data-bell="40"/);
  assert.match(ja, /data-act="goto" data-p="2" data-q="0"/);
  assert.match(ja, /<details class="report-proof">/);
  assert.match(ja, /report-won/);
  assert.match(ja, /マスごとの戦い/);
  setLang('en');
  const en = String(rep.render(FS, () => false)).replace(/<[^>]+>/g, ' ');
  assert.doesNotMatch(en, /[぀-ヿ一-鿿]/);
  assert.match(en, /held the field/);
  setLang('ja');
});

test('battle playback: a faster speed ends sooner; the tiles of a playing scene are known', () => {
  const scene = { p: 1, q: 0, bell: 3, tiles: [{ idx: 2, attackers: [], defenders: [] }] };
  const slow = B.startBattle(scene, 0), fast = B.startBattle(scene, 0, B.BATTLE_SPEEDS.fast);
  assert.equal(B.battleLive(slow, 5), true);
  assert.equal(B.battleLive(fast, 5), false);
  assert.deepEqual([...B.battleTiles([slow], 1)], ['1,0,2']);
  assert.equal(B.battleTiles([slow], 99).size, 0);
  assert.equal(B.startBattle(scene, 0, 0).speed, 1, 'speed 0 (off) never freezes a scene asked for by hand');
});
