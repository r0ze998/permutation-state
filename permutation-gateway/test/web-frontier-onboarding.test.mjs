// The guided first bells (W4-E; web design §7.11, §6.3; DESIGN §2.3 as
// restated by O-M1-13): the onboarding state machine is driven by chain
// facts first (decoded Citizen and Holding bytes, the marches' pipeline
// states) and local "seen" flags only; it gives the same state after a
// reload (flags through ps-fui storage) and a sensible one on a new device
// (chain facts alone); every step can be skipped and the card dismissed and
// restored; the waits carry the O-M1-13 times; the adjacent-wedge ticket
// offer of §5.9 (integ-W3 deferred K11); the card's markup in JA and EN.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as ob from '../../permutation-server/web/frontier/onboarding.mjs';
import * as card from '../../permutation-server/web/frontier/screens/onboarding.mjs';
import { decode } from '../../permutation-server/web/frontier/fcodec.mjs';
import { landState, ticketTimes, freeByWedge } from '../../permutation-server/web/frontier/fland.mjs';
import { seasonClock } from '../../permutation-server/web/frontier/clock.mjs';
import { ringProvinces, wedgeOf } from '../../permutation-server/web/frontier/fgeo.mjs';
import { loadUi, saveUi } from '../../permutation-server/web/frontier/fui.mjs';
import { FS } from '../../permutation-server/web/frontier/fstate.mjs';
import { W4_ACTIONS } from '../../permutation-server/web/frontier/app.mjs';
import { encodeAccount } from './fixtures/frontier/make-fixtures.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

afterEach(() => setLang('ja'));
const JP = /[぀-ヿ㐀-鿿]/;
const text = markup => String(markup).replace(/<[^>]*>/g, ' ');
const SEASON_JSON = JSON.parse(readFileSync(new URL('fixtures/frontier/season.json', import.meta.url), 'utf8'));
const CLOCK = seasonClock(decode('Season', Buffer.from(SEASON_JSON.bytes_b64, 'base64')));
const NO_TICKET = 0xffffffff;

const citizen = (v = {}) => decode('Citizen', encodeAccount('Citizen', { faction: 2, flags: 1, ticketBell: NO_TICKET, ...v }));
const holding = (v = {}) => decode('Holding', encodeAccount('Holding', { p: 2, q: 0, site: 3, gen: 0, tile: 14, state: 2, faction: 2, order: 1, ...v }));
const facts = (c, h = null, marches = []) => ({ citizen: c, stage: landState(c).stage, holding: h, scoutHost: false, marches });
const statuses = st => Object.fromEntries(st.steps.map(s => [s.id, s.status]));
// catalog `base_production(Hamlet)` and one Lumber camp more (+10 wood/h), milli-units.
const BASE = [40_000, 30_000, 20_000, 15_000, 0, 15_000, 5_000, 0];
const BUILT = BASE.map((x, i) => (i === 1 ? x + 10_000 : x));
const HELD = { flags: 1 | 4, holdingsN: 1, holding: [{ p: 2, q: 0, site: 3, gen: 0 }] };

test('the steps follow the chain: join, site, build, scout, march; the seen flags only for welcome, practice and report', () => {
  let st = ob.onboardingState(facts(null));
  assert.equal(st.current, 'welcome');
  st = ob.onboardingState(facts(null), [ob.FLAG.welcome]);
  assert.equal(st.current, 'join');
  // Joined, no ticket.
  st = ob.onboardingState(facts(citizen()), []);
  assert.equal(statuses(st).welcome, 'done', 'a Citizen already exists: the welcome is behind');
  assert.equal(st.current, 'join');
  assert.equal(st.steps.find(s => s.id === 'join').wait, null);
  // A ticket filed at bell 40: the wait says when (O-M1-13: about 11–21 min).
  st = ob.onboardingState(facts(citizen({ ticketBell: 40, ticketSites: [{ p: 2, q: 0, site: 3 }] })), [], CLOCK);
  const w = st.steps.find(s => s.id === 'join').wait;
  assert.deepEqual(w, { kind: 'ticket', at: ticketTimes(CLOCK, 40).resultAbout, minutes: [11, 21] });
  // A provisional holding counts (play is not gated on finality); a final one too.
  st = ob.onboardingState(facts(citizen({ flags: 1 | 8, holding: HELD.holding }), holding({ state: 1 })));
  assert.equal(statuses(st).join, 'done');
  assert.equal(st.current, 'build');
  // A build in the queue.
  const built = holding({ queue: [{ doneAt: 1_800_000_900, kind: 0 }, null, null, null] });
  st = ob.onboardingState(facts(citizen(HELD), built));
  assert.equal(st.current, 'scout');
  // Or production above the tier's base from a finished building (a founded Hamlet already has the base: W6-D).
  assert.equal(ob.hasBuilt(holding({ production: BASE })), false, 'the base production of a fresh Hamlet is not a building');
  assert.equal(ob.hasBuilt(holding({ production: BUILT })), true);
  assert.equal(ob.hasBuilt(holding({ tier: 1, production: BUILT })), false, 'a Town\'s base is higher');
  assert.equal(ob.hasBuilt(holding()), false);
  // Scouts trained is not yet the step: the exploration is (pending in the Holding, then counted in the Citizen).
  assert.equal(ob.hasScouts(holding({ reserve: [0, 0, 0, 0, 0, 0, 120, 0] })), true);
  const exploring = holding({ queue: [{ doneAt: 1, kind: 0 }, null, null, null], explore: { bell: 41, p: 2, q: 0, host: 5, state: 1 } });
  st = ob.onboardingState(facts(citizen(HELD), exploring));
  assert.equal(statuses(st).scout, 'done');
  assert.equal(st.current, 'practice');
  st = ob.onboardingState(facts(citizen({ ...HELD, explores: 2 }), built), [ob.FLAG.practice]);
  assert.equal(st.current, 'march');
  // A departure (the Citizen counts it; the transit sits in the Holding).
  const marching = holding({ queue: [{ doneAt: 1, kind: 0 }, null, null, null], transit: [{ state: 1, hostId: 9, arriveBell: 50 }, null, null, null] });
  const inFlight = [{ facts: { pipeline: 'revealing' }, dest: { p: 3, q: 0 }, entry: { arriveBell: 50 } }];
  st = ob.onboardingState(facts(citizen({ ...HELD, explores: 1 }), marching, inFlight), [ob.FLAG.practice], CLOCK);
  assert.equal(st.current, 'report');
  assert.deepEqual(st.steps.find(s => s.id === 'report').wait, { kind: 'report', pipeline: 'revealing', at: null, minutes: [31, 41] });
  const resolved = [{ facts: { pipeline: 'resolved' }, dest: { p: 3, q: 0 }, entry: { arriveBell: 50 } }];
  st = ob.onboardingState(facts(citizen({ ...HELD, explores: 1, arrivals: 1 }), built, resolved), [ob.FLAG.practice], CLOCK);
  assert.equal(st.current, 'report');
  assert.equal(st.steps.find(s => s.id === 'report').wait, null, 'resolved: nothing to wait for');
  assert.deepEqual(ob.reportOffer(resolved), { p: 3, q: 0, bell: 50 });
  assert.equal(ob.reportOffer(inFlight), null);
  st = ob.onboardingState(facts(citizen({ ...HELD, explores: 1, arrivals: 1 }), built, resolved), [ob.FLAG.practice, ob.FLAG.report]);
  assert.equal(st.complete, true);
  assert.equal(st.current, 'done');
  assert.equal(ob.STEPS.length, 8, 'design §7.11: eight steps');
});

test('a reload gives the same state (the flags go through ps-fui); a new device keeps every chain fact', () => {
  const store = new Map();
  const storage = { get: k => store.get(k) ?? null, set: (k, v) => { store.set(k, v); return true; } };
  const key = 'ps-fui:localnet:prog:1';
  const f = facts(citizen({ ...HELD, explores: 1 }), holding({ queue: [{ doneAt: 1, kind: 0 }, null, null, null] }));
  for (const flags of [[], [ob.FLAG.welcome], [ob.FLAG.practice], [ob.FLAG.practice, ob.FLAG.skip('march')], [ob.FLAG.dismissed]]) {
    const saved = saveUi(storage, key, { dismissed: flags });
    assert.deepEqual(ob.onboardingState(f, loadUi(storage, key).dismissed), ob.onboardingState(f, flags), JSON.stringify(flags));
    assert.deepEqual(saved.dismissed, flags);
  }
  // A new device: no flags; the chain facts still count, the local "seen" steps come back once.
  const fresh = ob.onboardingState(f, []);
  assert.deepEqual(statuses(fresh), { welcome: 'done', join: 'done', build: 'done', scout: 'done', practice: 'current', march: 'todo', report: 'todo', done: 'todo' });
  // A damaged record reads as the defaults (fui), so the card simply shows.
  store.set(key, '{not json');
  assert.deepEqual(loadUi(storage, key).dismissed, []);
});

test('skip, dismiss and restore: a skipped step moves the card on; a later chain fact still marks it done', () => {
  const c = citizen(HELD), h = holding();
  let flags = ob.withFlag([], ob.FLAG.skip('build'));
  let st = ob.onboardingState(facts(c, h), flags);
  assert.equal(statuses(st).build, 'skipped');
  assert.equal(st.current, 'scout');
  st = ob.onboardingState(facts(c, holding({ production: BUILT })), flags);
  assert.equal(statuses(st).build, 'done', 'a fact wins over a skip');
  for (const id of ['scout', 'practice', 'march', 'report']) flags = ob.withFlag(flags, ob.FLAG.skip(id));
  st = ob.onboardingState(facts(c, h), flags);
  assert.equal(st.complete, true, 'every step can be skipped');
  flags = ob.withFlag(flags, ob.FLAG.dismissed);
  flags = ob.withFlag(flags, ob.FLAG.welcome);
  assert.equal(ob.onboardingState(facts(c, h), flags).dismissed, true);
  const back = ob.restoreFlags(flags);
  assert.deepEqual(back, [ob.FLAG.welcome], 'restore keeps the seen flags only');
  assert.equal(ob.withFlag(back, ob.FLAG.welcome, false).length, 0);
  assert.equal(ob.withFlag(Array.from({ length: 80 }, (_, i) => `x${i}`), 'y').length, 64, 'ps-fui keeps 64 strings');
});

test('the adjacent-wedge offer (§5.9): only when the home wedge shows no free site; the adjacent wedges\' outermost open ring', () => {
  const ring = d => ({ ring: d, provinces: ringProvinces(d).map(pr => ({ p: pr.p, q: pr.q, sites: Array(12).fill(1), owners: Array(12).fill(0) })) });
  const withFree = (ov, wedges) => ({ ...ov, provinces: ov.provinces.map(pr => (wedges.includes(wedgeOf(pr.p, pr.q)) ? { ...pr, sites: [0, 3, ...Array(10).fill(1)], owners: [7, 7, ...Array(10).fill(0)] } : pr)) });
  // Rings 2 and 3 open; wedge 2 (faction 2) full; wedges 1 and 3 have free sites in both rings.
  const overviews = new Map([[2, withFree(ring(2), [1, 3])], [3, withFree(ring(3), [0, 1, 3])]]);
  assert.equal(freeByWedge(overviews)[2], 0);
  const o = ob.overflowProvinces(overviews, 2, 4);
  assert.equal(o.full, true);
  assert.equal(o.ring, 3, 'the outermost open ring (rings_opened − 1)');
  assert.ok(o.provinces.length > 0);
  assert.ok(o.provinces.every(pr => pr.ring === 3 && [1, 3].includes(pr.wedge) && pr.free === 2), JSON.stringify(o.provinces.slice(0, 3)));
  assert.deepEqual(ob.adjacentWedges(0), [5, 1]);
  // Not full: no offer.
  const open = new Map([[2, withFree(ring(2), [2])]]);
  assert.deepEqual(ob.overflowProvinces(open, 2, 3), { full: false, ring: null, provinces: [] });
  setLang('ja');
});

test('the card in both languages: open on the map tab, one line elsewhere, nothing when dismissed', () => {
  const store = { citizen: null, land: { stage: 'none' }, ui: { dismissed: [] } };
  let m = String(card.render(store));
  assert.match(m, /data-act="ob-seen" data-flag="welcome"/);
  assert.match(m, /data-act="ob-skip" data-step="welcome"/);
  assert.match(m, /data-act="ob-dismiss"/);
  assert.match(m, /aria-current="step"/);
  assert.equal((m.match(/<li/g) ?? []).length, 8);
  const closed = String(card.render(store, { open: false }));
  assert.match(closed, /^<details/);
  assert.match(closed, /<summary>ガイド 0\/7：ようこそ<\/summary>/);
  assert.equal(card.render({ ...store, ui: { dismissed: [ob.FLAG.dismissed] } }), '');
  // The ticket wait shows the time and the 11–21 min range; the report wait the pipeline state.
  const tk = { citizen: citizen({ ticketBell: 40 }), land: { stage: 'ticket' }, clock: CLOCK, ui: { dismissed: [] } };
  assert.match(text(card.render(tk)), /約11〜21分/);
  const done = { citizen: citizen({ ...HELD, explores: 1, arrivals: 1 }), land: { stage: 'final' }, holdings: [holding({ production: BUILT })], provinces: new Map(),
    marches: [{ facts: { pipeline: 'resolved' }, dest: { p: 3, q: 0 }, entry: { arriveBell: 50 } }], ui: { dismissed: [ob.FLAG.practice] } };
  assert.match(String(card.render(done)), /data-act="report-open" data-p="3" data-q="0" data-bell="50"/);
  setLang('en');
  for (const s of [store, tk, done, { ...done, ui: { dismissed: [ob.FLAG.practice, ob.FLAG.report] } }]) {
    const en = text(card.render(s));
    const hit = en.match(JP);
    assert.equal(hit, null, hit ? en.slice(Math.max(0, hit.index - 40), hit.index + 40) : '');
  }
  assert.match(text(card.render(tk)), /about 11–21 min/);
  assert.match(text(card.render(store, { open: false })), /Guide 0\/7: Welcome/);
  assert.match(String(card.renderRestore({ ui: { dismissed: [ob.FLAG.dismissed] } })), /ob-restore/);
  assert.equal(card.renderRestore({ ui: { dismissed: [ob.FLAG.welcome] } }), '');
});

test('the card\'s actions keep their flags in the UI preferences (memory before a season is pinned)', () => {
  const saved = { ...FS };
  try {
    FS.ui = { dismissed: [] };
    W4_ACTIONS['ob-seen']({ flag: 'welcome' });
    assert.deepEqual(FS.ui.dismissed, [ob.FLAG.welcome]);
    W4_ACTIONS['ob-seen']({ flag: 'nonsense' });
    assert.deepEqual(FS.ui.dismissed, [ob.FLAG.welcome], 'only known seen flags');
    W4_ACTIONS['ob-skip']({ step: 'build' });
    W4_ACTIONS['ob-dismiss']();
    assert.deepEqual(FS.ui.dismissed, [ob.FLAG.welcome, 'ob:skip:build', 'onboarding']);
    W4_ACTIONS['ob-restore']();
    assert.deepEqual(FS.ui.dismissed, [ob.FLAG.welcome]);
  } finally { Object.assign(FS, saved); }
});
