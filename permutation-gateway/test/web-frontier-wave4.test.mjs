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

test('the own name: a signed profile kept per season and wallet, the form in both languages', async () => {
  const P = await import('../../permutation-server/web/frontier/people/profile.mjs');
  const U = await import('../../permutation-server/web/frontier/people/ui.mjs');
  const store = new Map(); const st = { get: k => store.get(k) ?? null, set: (k, v) => store.set(k, v) };
  const p = { v: 1, season: '7', wallet: 'W1', name: 'Mira', ts: 1, sig: 'AA==' };
  P.saveOwnProfile(st, p);
  assert.equal(P.loadOwnProfile(st, 7, 'W1')?.name, 'Mira');
  assert.equal(P.loadOwnProfile(st, 7, 'W2'), null, 'another wallet\'s profile is not ours');
  P.clearOwnProfile(st, '7');
  assert.equal(P.loadOwnProfile(st, 7, 'W1'), null);
  const FS = { holdings: [{ ownerCitizen: Uint8Array.from([1, 2, 3, 4, 5, 6, 7, 8]) }], wallet: { address: 'W1' }, citizen: { faction: 1 }, ownProfile: p };
  assert.equal(U.ownIdentity(FS).given.en, 'Mira');
  const ja = String(U.renderNameForm(FS));
  assert.match(ja, /data-form="profile-name"/);
  assert.match(ja, /data-act="profile-clear"/);
  setLang('en');
  assert.doesNotMatch(String(U.renderNameForm({ ...FS, ownProfile: null })).replace(/<[^>]+>/g, ' ').replace(/data-name/g, ''), /[぀-ヿ一-鿿]/);
  setLang('ja');
  assert.equal(String(U.renderNameForm({ ...FS, wallet: null })), '', 'no wallet: nothing to sign with');
});

test('the forecast\'s input: only the viewer\'s host arrives, with its stance, retreat, troops and the route\'s stamina cost', async () => {
  const F = await import('../../permutation-server/web/frontier/hud/forecast.mjs');
  const c = { host: { id: '77', unit: 0, troops: 600, staminaValue: 100, staminaBell: 40 }, dest: { p: 2, q: 0, tile: 32 }, arriveBell: 44, stance: 1, retreat: 'never', ratio: null, route: { dirs: [0, 0, 1, 1] } };
  const i = F.forecastInputs(c, 3);
  assert.deepEqual([i.p, i.q, i.bell, i.arrivals.length], [2, 0, 44, 1]);
  const a = i.arrivals[0];
  assert.deepEqual([a.hostId, a.faction, a.troops, a.stance, a.retreat, a.tile], [77n, 3, 600000, 1, 0, 32]);
  assert.equal(a.stamina, 104 - (10 + 2 * 4), 'stamina regained to the arrival bell, less the march');
  assert.equal(F.forecastKey(c), '77|2,0,32|44|1|never|null');
  assert.deepEqual(F.forecast(null, c, null, 3), { ok: false, why: 'NoInput' });
});

test('pins: toggled per place, newest first, at most 20, kept per season; found by searching "ピン"', async () => {
  const PN = await import('../../permutation-server/web/frontier/hud/pins.mjs');
  const S = await import('../../permutation-server/web/frontier/hud/search.mjs');
  let pins = PN.togglePin([], { p: 1, q: 0, tile: 5 }, 1);
  pins = PN.togglePin(pins, { p: 2, q: -1, tile: null }, 2);
  assert.deepEqual(pins.map(PN.pinKey), ['2,-1,-', '1,0,5']);
  assert.ok(PN.hasPin(pins, { p: 1, q: 0, tile: 5 }));
  pins = PN.togglePin(pins, { p: 1, q: 0, tile: 5 });
  assert.equal(pins.length, 1);
  for (let i = 0; i < 30; i++) pins = PN.togglePin(pins, { p: i, q: 0, tile: 1 });
  assert.equal(pins.length, PN.PIN_MAX);
  const store = new Map(); const st = { get: k => store.get(k) ?? null, set: (k, v) => store.set(k, v) };
  PN.savePins(st, '7', pins);
  assert.equal(PN.loadPins(st, '7').length, 20);
  assert.deepEqual(PN.loadPins({ get: () => '{bad' }, '7'), []);
  setLang('ja');
  const hits = S.searchMap('ピン', { pins: [{ p: 3, q: 0, tile: 4 }] });
  assert.deepEqual([hits[0].kind, hits[0].p, hits[0].tile], ['pin', 3, 4]);
  assert.equal(S.searchMap('pins', { pins: [{ p: 3, q: 0, tile: null }] })[0].tile, undefined);
});

test('a battle of three factions on one tile: every side in the scene, and it paints without error at every phase', async () => {
  const B = await import('../../permutation-server/web/frontier/people/battle.mjs');
  const arr = (id, faction, stance, fate, after) => ({ present: 1, hostId: BigInt(id), faction, unit: 0, troops: 400000, troopsAfter: after, stance, fate, tile: 9 });
  const inputs = { arrivals: [arr(1, 0, 1, 1, 300000), arr(2, 2, 2, 4, 400000), arr(3, 4, 3, 5, 0)] };
  const before = { p: 1, q: 0, sites: [9], siteMirror: [{ state: 1, faction: 5, garrison: 80000 }], camp: { state: 0 }, entries: [{ id: 9n, state: 1, tile: 9, faction: 5, troops: 120000 }] };
  const after = { p: 1, q: 0, sites: [9], siteMirror: [{ state: 1, faction: 5, garrison: 20000 }], camp: { state: 0 }, entries: [] };
  const s = B.battleScene({ p: 1, q: 0, bell: 9, inputs, before, after });
  assert.equal(s.tiles.length, 1);
  const t = s.tiles[0];
  assert.deepEqual(t.attackers.map(x => x.faction).sort(), [0, 2, 4]);
  assert.deepEqual(t.defenders.map(x => x.kind).sort(), ['garrison', 'resident']);
  assert.deepEqual(t.attackers.map(x => x.fate).sort(), ['Destroyed', 'Retreated', 'Stays']);
  const noop = () => {};
  const ctx = new Proxy({}, { get: (o, k) => (k in o ? o[k] : k === 'measureText' ? () => ({ width: 10 }) : k === 'createRadialGradient' || k === 'createLinearGradient' ? () => ({ addColorStop: noop }) : noop), set: (o, k, v) => { o[k] = v; return true; } });
  const play = B.startBattle(s, 0);
  for (const at of [0.5, 1.5, 3, 4.5, 6, 6.9]) assert.equal(B.paintBattle(ctx, play, { zoom: 2, now: at }), true, `phase at ${at} s`);
  assert.equal(B.paintBattle(ctx, play, { zoom: 2, now: 8 }), false, 'over after the end');
});

test('units redesign: one token per faction per tile, status from state; moments from real changes only', async () => {
  const U = await import('../../permutation-server/web/frontier/people/units.mjs');
  const M = await import('../../permutation-server/web/frontier/people/moments.mjs');
  const hosts = [
    { id: 1n, faction: 0, unit: 0, tile: 5, state: 1, troops: 600, stamina: 110 },
    { id: 2n, faction: 0, unit: 6, tile: 5, state: 1, troops: 100, stamina: 120 },
    { id: 3n, faction: 2, unit: 2, tile: 5, state: 1, troops: 300, stamina: 90 },
    { id: 4n, faction: 0, unit: 0, tile: 9, state: 3, troops: 400, stamina: 50 },
  ];
  const toks = U.provinceTokens({ p: 1, q: 0, hosts, viewerFaction: 0 });
  const on5 = toks.filter(t => t.tile === 5);
  assert.equal(on5.length, 2, 'two factions on tile 5: two tokens, not three figures');
  const mine = on5.find(t => t.faction === 0);
  assert.deepEqual([mine.kind, mine.troops, mine.n, mine.status, mine.own], ['spearman', 700, 2, 'fight', true]);
  assert.equal(on5.find(t => t.faction === 2).kind, 'horseman');
  assert.equal(toks.find(t => t.tile === 9).status, 'sealed');
  assert.equal(U.provinceTokens({ p: 1, q: 0, hosts, marching: new Set(['4']) }).some(t => t.tile === 9), false, 'the own column walks its road instead');
  const pts = U.routePoints({ p: 1, q: 0, tile: 5, dirs: [0, 0, 1] });
  assert.equal(pts.length, 4);
  const a = U.alongPath(pts, 0), b = U.alongPath(pts, 1);
  assert.ok(Math.abs(a.x - pts[0].x) < 1e-6 && Math.abs(b.x - pts[3].x) < 1e-6);
  const s0 = M.momentSnapshot({ life: new Map([['1,0,3', { harvest: 40 }]]), constructions: [{ p: 1, q: 0, site: 3, name: 'Farm' }] });
  assert.deepEqual(M.detectMoments(null, s0, 0), [], 'the first look is not news');
  const s1 = M.momentSnapshot({ life: new Map([['1,0,3', { harvest: 41 }]]), constructions: [] });
  assert.deepEqual(M.detectMoments(s0, s1, 5).map(m => [m.kind, m.label ?? null]), [['harvest', null], ['built', 'Farm']]);
  assert.equal(M.liveMoments([{ kind: 'built', t0: 0 }], 10).length, 0);
});
