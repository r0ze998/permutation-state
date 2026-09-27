// The Frontier play controller (W3-F's controller.mjs) and the fixes of
// the wave-3 review (integ-W3, contract v1.5): the send-time earliest bell,
// the chronicle starting near the head and reading full pages, the cavalry
// pace of the incoming warnings, remote hosts' provinces, the invite field
// of a gated season, SettleExplore's seed source, the hidden page's poll
// cadence, a failed send revived by its live transit, null holdings in
// /h/me, and the positive-only garrison form. The controller's refresh runs
// against a fake herald (no network, no DOM).
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import * as C from '../../permutation-server/web/frontier/controller.mjs';
import * as march from '../../permutation-server/web/frontier/fmarch.mjs';
import * as book from '../../permutation-server/web/frontier/marchbook.mjs';
import * as W from '../../permutation-server/web/frontier/wasm.mjs';
import { createHerald } from '../../permutation-server/web/frontier/herald.mjs';
import { FS } from '../../permutation-server/web/frontier/fstate.mjs';
import * as joinScreen from '../../permutation-server/web/frontier/screens/join.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

test('the chronicle starts near the head, the warnings judge at cavalry pace, the poll keeps a hidden cadence', () => {
  assert.equal(C.chronicleStart('10000'), 10_000 - C.CHRONICLE_WINDOW);
  assert.equal(C.chronicleStart(12), 0);
  assert.equal(C.chronicleStart(undefined), 0);
  assert.equal(W.UNITS[C.WARNING_UNIT], 'Horseman');
  // incomingWarnings passes the unit the caller names to `reachable`.
  const seen = [];
  const kernel = { call: (name, a) => { seen.push([name, a.unit]); return { ok: true, value: true }; } };
  const dep = { host_id: 1n, origin_p: 2, origin_q: 0, origin_tile: 30, depart_bell: 10, arrive_bell: 14, dep_mass: 5_000_000, faction: 1 };
  march.incomingWarnings({ kernel, departures: [dep], holdings: [{ p: 3, q: 0, site: 0, tile: 30 }], faction: 0, genesisTs: 0, nowBell: 11, unitOf: () => C.WARNING_UNIT });
  assert.deepEqual(seen, [['reachable', C.WARNING_UNIT]]);
  // Polls: 10 s in flight, 30 s idle, 60 s hidden (never faster than visible), backoff in every case.
  assert.equal(C.pollDelay({ inFlight: true }), 10);
  assert.equal(C.pollDelay({}), 30);
  assert.equal(C.pollDelay({ hidden: true }), C.HIDDEN_POLL);
  assert.ok(C.pollDelay({ hidden: true }) >= C.pollDelay({}));
  assert.equal(C.pollDelay({ hidden: true, errors: 2 }), 240);
  assert.equal(C.pollDelay({ errors: 9 }), 300);
});

test('SettleExplore names a present cache of round S of THE anchor, or the archive; never caches[0] blindly', () => {
  assert.equal(C.seedSourceOf(null), null);
  const rec = { archived: false, S: 77, anchor: { A: 5 }, caches: [
    { nonce: 1, round: 70, A: 5, present: true },
    { nonce: 2, round: 77, A: 4, present: true },
    { nonce: 3, round: 77, A: 5, present: false },
    { nonce: 4, round: 77, A: 5, present: true },
  ] };
  assert.deepEqual(C.seedSourceOf(rec), { nonce: 4 });
  assert.equal(C.seedSourceOf({ ...rec, caches: rec.caches.slice(0, 3) }), null);
  assert.deepEqual(C.seedSourceOf({ ...rec, archived: true }), { archived: true });
});

test('hosts elsewhere: /h/me hosts give the provinces to load (the home one excluded, each once)', () => {
  const rec = { hosts: [{ province: [2, 0] }, { province: [5, -1] }, { province: [5, -1] }, { province: 'x' }] };
  assert.deepEqual(C.hostProvinces(rec, { p: 2, q: 0 }), [{ p: 5, q: -1 }]);
  assert.deepEqual(C.hostProvinces(null), []);
});

test('a gated season shows the invite field before any refusal', () => {
  setLang('ja');
  const season = { joinGate: new Uint8Array(32).fill(9) };
  assert.equal(C.inviteRequired(season), true);
  assert.equal(C.inviteRequired({ joinGate: new Uint8Array(32) }), false);
  const out = String(joinScreen.render({ ...FS, season, citizen: null, land: { stage: 'none' }, wallet: { address: 'W' }, session: null, overviews: new Map(), joinDraft: {} }));
  assert.match(out, /data-bind="invite"/);
  const open = String(joinScreen.render({ ...FS, season: { joinGate: new Uint8Array(32) }, citizen: null, land: { stage: 'none' }, wallet: { address: 'W' }, session: null, overviews: new Map(), joinDraft: {} }));
  assert.doesNotMatch(open, /data-bind="invite"/);
});

test('a send reported failed that landed anyway is revived by its live transit (not a seal mismatch)', () => {
  const e = { host: '7', transitSlot: 1, arriveBell: 50, sealRoot_hex: 'ab'.repeat(32), state: 'failed', failure: 'RelayRejected' };
  const live = [{ slot: 1, state: 1, hostId: 7n, arriveBell: 50, sealRoot: 'ab'.repeat(32) }];
  assert.equal(book.reconcile([e], { transits: live })[0].state, 'landed');
  assert.equal(book.reconcile([{ ...e, mismatch: true }], { transits: live })[0].state, 'failed');
  assert.equal(book.reconcile([e], { transits: [{ ...live[0], sealRoot: 'cd'.repeat(32) }] })[0].state, 'failed');
  assert.equal(book.reconcile([e], { transits: [] })[0].state, 'failed');
});

test('herald client: a null holding in /h/me is skipped; /h/events reports a full page', async () => {
  const pages = {
    '/h/me/W': { v: 1, wallet: 'W', citizen: null, holdings: [null], hosts: [] },
    '/h/events?after=0': { v: 1, events: [], next: '500', full: true },
  };
  const fetch = async url => {
    const j = pages[url];
    return { ok: !!j, status: j ? 200 : 404, headers: { get: () => 'application/json' }, json: async () => j, text: async () => JSON.stringify(j), arrayBuffer: async () => new ArrayBuffer(0) };
  };
  const h = createHerald({ fetch });
  const me = await h.me('W');
  assert.equal(me.ok, true, me.code);
  assert.deepEqual(me.holdings, []);
  const ev = await h.events(0);
  assert.equal(ev.full, true);
});

test('the garrison form offers increases only (the M1 program refuses delta ≤ 0)', () => {
  const src = readFileSync(new URL('../../permutation-server/web/frontier/screens/holding.mjs', import.meta.url), 'utf8');
  assert.match(src, /name="delta" type="number" min="1"/);
  assert.doesNotMatch(src, /増減/);
});

test('refresh: remote host provinces, the invite flag, the explore seed and the chronicle from near the head', async () => {
  const G = 1_800_000_000;
  const saved = { ...FS };
  const calls = { events: [], provinces: [], bells: [] };
  const citizen = { ticketEscrow: 0n, holding: [{ p: 2, q: 0, site: 0, gen: 1 }], ticketBell: 0xFFFF_FFFF, ticketSites: [], flags: 0, faction: 0, holdingsN: 1 };
  const holding = { p: 2, q: 0, site: 0, tile: 5, state: 2, transit: [], explore: { state: 1, bell: 40, p: 2, q: 1 } };
  const fake = {
    me: async () => ({ ok: true, record: { hosts: [{ province: [2, 0] }, { province: [5, -1] }] }, citizen, holdings: [holding] }),
    province: async (p, q, bell = 'latest') => { calls.provinces.push(`${p},${q}@${bell}`); return { ok: true, bell: 49, province: { p, q, resolvedNext: 49 }, slots: [] }; },
    bellRegion: async (b, r) => { calls.bells.push([b, r]); return { ok: true, record: { archived: false, S: 77, anchor: { A: 5 }, caches: [{ nonce: 3, round: 77, A: 5, present: true }] }, anchor: null }; },
    events: async after => { calls.events.push(Number(after)); return { ok: true, events: [], next: Number(after) + 500, full: true }; },
  };
  try {
    Object.assign(FS, {
      clock: { genesisTs: G }, chain: { now: () => G + 600 * 50 + 10 }, wallet: { address: 'W' }, session: null, kernel: null,
      season: { joinGate: new Uint8Array(32).fill(1) }, record: { headSeq: '10000' }, provinces: new Map(), overviews: new Map(),
      chronicle: [], inviteRequired: false, exploreSeedReady: false,
    });
    C.useHerald(fake);
    await C.refresh();
    assert.ok(FS.provinces.has('2,0') && FS.provinces.has('5,-1'), [...FS.provinces.keys()].join());
    assert.equal(FS.inviteRequired, true);
    assert.equal(FS.exploreSeedReady, true);
    assert.deepEqual(calls.bells, [[40, calls.bells[0][1]]]);
    assert.deepEqual(calls.events, [9_500, 10_000, 10_500, 11_000], 'from head − 500, full pages up to four');
    // The next refresh continues from where it stopped.
    await C.refresh();
    assert.equal(calls.events[4], 11_500);
  } finally {
    Object.assign(FS, saved);
  }
});
