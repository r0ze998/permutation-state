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

test('milestones: reached from state, the first load records quietly, later ones are news; the banner and timeline in both languages', async () => {
  const M = await import('../../permutation-server/web/frontier/hud/milestones.mjs');
  const FS = { holdings: [{ p: 2, q: 0, site: 3, tier: 1, state: 2, transit: [] }], marches: [], record: { rings: [{}, {}, {}] } };
  const ids = M.reachedMilestones(FS).map(m => m.id);
  assert.deepEqual(ids, ['first-holding', 'confirmed', 'tier:2,0,3:1', 'ring:2']);
  const first = M.newMilestones(null, M.reachedMilestones(FS), 40);
  assert.equal(first.fresh.length, 0, 'old news on the first load');
  FS.marches = [{ dest: { p: 3, q: 1 }, facts: { settled: { outcome: 'Stays' } } }];
  const next = M.newMilestones(first.record, M.reachedMilestones(FS), 44);
  assert.deepEqual(next.fresh.map(m => m.id), ['first-march', 'first-win']);
  assert.equal(next.record.seen['first-win'].bell, 44);
  setLang('ja');
  assert.match(String(M.renderBanner(next.fresh[1], 2)), /最初の勝利：州 3,1/);
  setLang('en');
  const tl = String(M.renderTimeline(next.record)).replace(/<[^>]+>/g, ' ');
  assert.match(tl, /Season timeline/);
  assert.doesNotMatch(tl.replace(/data-name/g, ''), /[぀-ヿ一-鿿]/);
  assert.match(String(M.renderBanner(next.fresh[1], 2)), /First victory: province 3,1/);
  setLang('ja');
  assert.equal(M.loadSeen({ get: () => '{bad' }, 'k'), null);
});

test('the guide: levels filter, the onboarding card hides below "all"', async () => {
  const G = await import('../../permutation-server/web/frontier/hud/guide.mjs');
  const card = await import('../../permutation-server/web/frontier/screens/onboarding.mjs');
  assert.equal(G.guideLevel({ ui: { guide: 'warn' } }), 'warn');
  assert.equal(G.guideLevel({ ui: { guide: 'bogus' } }), 'all');
  assert.ok(G.WARN_KINDS.has('incoming') && !G.WARN_KINDS.has('idle'));
  assert.equal(String(card.render({ ui: { guide: 'off', dismissed: [] }, mode: 'play' })), '');
  assert.equal(G.guideTarget({ ui: { guide: 'warn' }, mode: 'play' }), null);
  setLang('en');
  for (const k of ['march', 'build', 'scout', 'settle']) assert.doesNotMatch(G.guideLabel({ kind: k }) + G.goText({ kind: k, host: '1' }), /[぀-ヿ一-鿿]/);
  setLang('ja');
});

test('the spectator: bells with events, highlights filtered by faction and bell, clashes count for the province\'s factions', async () => {
  const SP = await import('../../permutation-server/web/frontier/screens/spectate.mjs');
  const U = await import('../../permutation-server/web/frontier/people/ui.mjs');
  const owners = Array(12).fill(7); owners[0] = 2; owners[1] = 4;
  const sites = Array(12).fill(0); sites[0] = 1; sites[1] = 1;
  const overviews = new Map([[1, { provinces: [{ p: 1, q: 0, owners, sites }] }]]);
  const chronicle = [{ record: { name: 'CLASH', p: 1, q: 0, bell: 40 } }, { record: { name: 'CLASH', p: 1, q: 0, bell: 42 } }, { record: { name: 'HARVEST', bell: 42 } }];
  assert.deepEqual(SP.eventBells(chronicle), [{ bell: 42, events: 1, clashes: 1 }, { bell: 40, events: 1, clashes: 1 }]);
  assert.equal(U.highlights(chronicle, overviews, null, { faction: 2 }).length, 2);
  assert.equal(U.highlights(chronicle, overviews, null, { faction: 3 }).length, 0);
  assert.deepEqual(U.highlights(chronicle, overviews, null, { bell: 40 }).map(x => x.bell), [40]);
  assert.match(String(U.renderHighlights(U.highlights(chronicle, overviews, null), { go: true })), /data-act="battle-play" data-p="1" data-q="0" data-bell="42"/);
  const panel = [SP.render({ chronicle, overviews, watch: { faction: 2, bell: 42 } })].flat(9).map(String).join('');
  assert.match(panel, /data-act="watch-bell" data-bell="42" aria-pressed="true"/);
  assert.match(panel, /data-act="watch-faction" data-f="2" aria-pressed="true"/);
});

test('the replay\'s people: battles of the bell entered from its envelopes, names only of owners founded by then', async () => {
  const RP = await import('../../permutation-server/web/frontier/people/replay.mjs');
  const R = await import('../../permutation-server/web/frontier/people/roster.mjs');
  const inputs = { arrivals: [{ present: 1, hostId: 9n, faction: 0, troops: 100000, troopsAfter: 40000, stance: 1, fate: 1, tile: 3 }] };
  const env = { bell: 40, inputs, province: { p: 1, q: 0, entries: [], sites: [], siteMirror: [] } };
  assert.equal(RP.replayBattles(40, [env]).length, 1);
  assert.equal(RP.replayBattles(41, [env]).length, 0, 'an envelope of another bell plays nothing');
  let t = 0;
  const roster = R.createRoster({ fetch: async () => ({ ok: false }) });
  roster.put(1, 0, 2, 0x1234n, 50, 1);
  const rp = RP.createReplayPeople({ roster, clock: () => t });
  assert.equal(rp.at(40, { envelopes: [env] }).battles.length, 0, 'nothing plays before the bell is entered');
  rp.enter(40);
  assert.equal(rp.at(40, { envelopes: [env] }).battles.length, 1);
  t = 60;
  assert.equal(rp.at(40, { envelopes: [env] }).battles.length, 0, 'the scene ends');
  assert.equal(rp.at(40).nameOf(1, 0, 2, 0), null, 'founded at bell 50: no name at bell 40');
  assert.ok(rp.at(60).nameOf(1, 0, 2, 0)?.name);
});

test('a clash of residents (no arrivals) plays from the province before and after; nothing plays where no one lost troops', async () => {
  const B = await import('../../permutation-server/web/frontier/people/battle.mjs');
  const before = { p: 1, q: 0, sites: [5], siteMirror: [{ state: 1, faction: 2, garrison: 50000 }], camp: { state: 1, tile: 9, troops: 300 },
    entries: [{ id: 1n, state: 1, tile: 5, faction: 0, troops: 200000 }, { id: 2n, state: 1, tile: 9, faction: 3, troops: 100000 }, { id: 3n, state: 1, tile: 7, faction: 4, troops: 10000 }] };
  const after = { p: 1, q: 0, sites: [5], siteMirror: [{ state: 1, faction: 2, garrison: 10000 }], camp: { state: 0 },
    entries: [{ id: 1n, state: 1, tile: 5, faction: 0, troops: 150000 }, { id: 2n, state: 1, tile: 9, faction: 3, troops: 80000 }, { id: 3n, state: 1, tile: 7, faction: 4, troops: 10000 }] };
  const s = B.battleScene({ p: 1, q: 0, bell: 7, inputs: { arrivals: [] }, before, after });
  assert.equal(s.residents, true);
  assert.deepEqual(s.tiles.map(t => t.idx).sort(), [5, 9]);
  const t5 = s.tiles.find(t => t.idx === 5);
  assert.deepEqual([t5.attackers.map(x => x.faction), t5.defenders.map(x => x.kind)], [[0], ['garrison']]);
  assert.equal(s.tiles.find(t => t.idx === 9).defenders[0].fate, 'Destroyed');
  assert.equal(B.battleScene({ p: 1, q: 0, bell: 7, inputs: { arrivals: [] }, before, after: null }), null, 'without the province after, no guessing');
  assert.ok(B.battleTime(B.startBattle(s, 0), 0) >= B.PHASE.deploy);
});
