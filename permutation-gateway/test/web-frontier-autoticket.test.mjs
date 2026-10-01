// The automatic site ticket (owner decision V2) after the review of 2026-10-02 (PLAM):
// per-citizen order, cohort room, decoded sites (no phantom free sites), overflow, read
// failures, and the controller's lock that only a sent ticket sets.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
if (!globalThis.crypto) globalThis.crypto = webcrypto;
const J = await import('../../permutation-server/web/frontier/fjoin.mjs');
const G = await import('../../permutation-server/web/frontier/fgeo.mjs');
const L = await import('../../permutation-server/web/frontier/fland.mjs');

const HOME = L.homeWedge(0);
const ring = d => G.ringProvinces(d).map(({ p, q }) => ({ p, q }));
// an overview of rings 2 and 3, every site free (owner 7) — or, `full`, the home wedge taken
const overviews = (fullHome = false) => new Map([2, 3].map(d => [d, { ring: d, provinces: ring(d).map(({ p, q }) => {
  const full = fullHome && G.wedgeOf(p, q) === HOME;
  return { p, q, ring: d, sites: Array(12).fill(full ? 1 : 0), owners: Array(12).fill(full ? 2 : 7) };
}) }]));
// a decoded Province: `siteCount` sites, the free ones listed; optional cohorts
const prov = (p, q, { count = 12, free = null, cohorts = [] } = {}) => ({ p, q, siteCount: count, sites: Array.from({ length: 12 }, (_, i) => i * 5),
  siteMirror: Array.from({ length: 12 }, (_, i) => ({ state: i >= count ? 0 : (free ?? [...Array(count).keys()]).includes(i) ? 0 : 1 })), ticketCohorts: cohorts });

test('two citizens of one faction get different sites from the same herald state (review finding 2)', async () => {
  const read = async (p, q) => prov(p, q);
  const a = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'WalletA|42', nowBell: 42, read });
  const b = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'WalletB|42', nowBell: 42, read });
  assert.equal(a.ok && b.ok, true);
  assert.equal(a.sites.length, 3);
  assert.notDeepEqual(a.sites, b.sites);
  for (const s of [...a.sites, ...b.sites]) assert.equal(G.wedgeOf(s.p, s.q), HOME);
  for (const l of [a.sites, b.sites]) {
    const rings = l.map(s => G.ringOf(s.p, s.q));
    assert.equal(rings[0], 2, 'the nearest open ring first');
    assert.deepEqual(rings, [...rings].sort((x, y) => x - y));
  }
  assert.equal(new Set(a.sites.map(s => `${s.p},${s.q}`)).size, 3, 'three different provinces');
  const again = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'WalletA|42', nowBell: 42, read });
  assert.deepEqual(again.sites, a.sites, 'stable for the same citizen and bell');
});

test('a short province never offers its missing sites; a full cohort table is skipped (findings 2, 4)', () => {
  const [a, b] = ring(2).filter(x => G.wedgeOf(x.p, x.q) === HOME);
  const short = prov(a.p, a.q, { count: 7, free: [] });   // the overview reads sites 7–11 as free; they do not exist
  assert.deepEqual(J.pickSites([short]), []);
  const busy = Array.from({ length: 8 }, (_, i) => ({ bell: 40 + (i % 2), filed: 3, settled: 1 }));
  assert.equal(J.cohortRoom(prov(b.p, b.q, { cohorts: busy }), 42), false);
  assert.equal(J.cohortRoom(prov(b.p, b.q, { cohorts: [{ bell: 42, filed: 2, settled: 0 }, ...busy.slice(1)] }), 42), true, 'this bell\'s record is shared');
  assert.equal(J.cohortRoom(prov(b.p, b.q, { cohorts: busy }), 40 + L.COHORT_BELLS), true, 'closed after COHORT_BELLS');
  assert.deepEqual(J.pickSites([prov(b.p, b.q, { cohorts: busy })], { nowBell: 42 }), []);
});

test('home wedge with no real free site: the adjacent wedges\' outermost ring (§5.9); read failures are not "no free site" (findings 3, 4)', async () => {
  // the overview says the home wedge has free sites, but every province read is short and full
  const read = async (p, q) => (G.wedgeOf(p, q) === HOME ? prov(p, q, { count: 8, free: [] }) : prov(p, q));
  const r = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|1', nowBell: 1, read });
  assert.equal(r.source, 'overflow');
  assert.ok(r.sites.every(s => G.ringOf(s.p, s.q) === 3 && G.wedgeOf(s.p, s.q) !== HOME));
  const none = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|1', nowBell: 1, read: async (p, q) => prov(p, q, { free: [] }) });
  assert.deepEqual([none.ok, none.why], [false, 'nofree']);
  const down = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|1', nowBell: 1, read: async () => null });
  assert.deepEqual([down.ok, down.why], [false, 'readfail']);
  assert.ok(down.reads <= J.AUTO_READ_MAX);
});

test('autoTicket with a pinned season: a refusal before sending locks nothing; the button retries once, throttled, and respects another tab\'s attempt (findings 1, 6; re-check 3, 4, 8, 9)', async () => {
  const C = await import('../../permutation-server/web/frontier/controller.mjs');
  const { FS } = await import('../../permutation-server/web/frontier/fstate.mjs');
  const io = await import('../../permutation-server/web/frontier/fchainio.mjs');
  const fui = await import('../../permutation-server/web/frontier/fui.mjs');
  const { PROGRAM, SEASON_ID } = await import('../screens/world.mjs');
  io.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID });
  const sc = io.scope();
  let provinceReads = 0, seasonReads = 0;
  C.useHerald({
    season: async () => { seasonReads++; return { ok: true, record: { rings: [{}, {}, {}, {}] } }; },
    overview: async d => ({ ok: d >= 2, ...(overviews().get(d) ?? {}) }),
    province: async (p, q) => { provinceReads++; return { ok: true, province: prov(p, q) }; },
  });
  Object.assign(FS, { land: { stage: 'joined' }, session: { publicKeyBytes: new Uint8Array(32) }, sessionProblem: null, citizen: { faction: 0 }, wallet: { address: 'WalletT' },
    overviews: new Map(), record: { rings: [{}] }, autoTicket: null, autoTicketBusy: false, overviewsBell: undefined, autoProv: undefined, autoFull: undefined, autoForceAt: 0 });
  const key = fui.landKey(sc, 'WalletT');
  const rec = () => fui.loadLand(fui.uiStorage, key);
  await C.autoTicket();
  assert.equal(FS.autoTicket.state, 'failed', 'the relay is unreachable: refused before sending');
  assert.deepEqual([rec().ticketSeen, rec().lastTicketBell, rec().autoTryBell, rec().autoTryFailed], [true, null, 0, true], 'no lock; this bell tried and failed (stored per wallet)');
  assert.equal(seasonReads, 1);
  assert.equal(fui.loadLand(fui.uiStorage, fui.landKey(sc, 'OtherWallet')).autoTryBell, null, 'another wallet in this browser is not blocked');
  // not forced, same bell: nothing new
  FS.autoProv = undefined; provinceReads = 0;
  await C.autoTicket();
  assert.equal(provinceReads, 0);
  // the button: plans again at once, then is throttled
  await C.autoTicket({ force: true });
  assert.ok(provinceReads > 0, 'the button files again at once');
  FS.autoProv = undefined; const n = provinceReads;
  await C.autoTicket({ force: true });
  assert.equal(provinceReads, n, 'throttled');
  // another tab is mid-attempt this bell: the button waits for it
  fui.saveLand(fui.uiStorage, key, { autoTryBell: 0, autoTryFailed: false });
  FS.autoForceAt = 0;
  await C.autoTicket({ force: true });
  assert.equal(provinceReads, n, 'another tab\'s attempt is respected');
  // a ticket that left this browser and is not shown yet: 'sent'
  fui.saveLand(fui.uiStorage, key, { ticketSeen: false, lastTicketBell: 0, autoTryBell: 0, autoTryFailed: false });
  await C.autoTicket();
  assert.equal(FS.autoTicket.state, 'sent');
  FS.land = { stage: 'final' };
  await C.autoTicket();
  assert.equal(FS.autoTicket, null, 'nothing to do with a holding');
});

test('the planner waits for cohort room instead of an overflow the program refuses; home read failures are not an overflow; the final order is by ring (re-check 1, 2, 5)', async () => {
  const busy = Array.from({ length: 8 }, () => ({ bell: 10, filed: 3, settled: 1 }));
  const room = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|11', nowBell: 11, read: async (p, q) => prov(p, q, { cohorts: busy }) });
  assert.deepEqual([room.ok, room.why], [false, 'room']);
  const half = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|11', nowBell: 11, read: async (p, q) => (G.wedgeOf(p, q) === HOME ? null : prov(p, q)) });
  assert.deepEqual([half.ok, half.why], [false, 'readfail'], 'home reads failed: never an overflow ticket');
  const notFull = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|11', nowBell: 11, homeFull: false, read: async (p, q) => (G.wedgeOf(p, q) === HOME ? prov(p, q, { free: [] }) : prov(p, q)) });
  assert.deepEqual([notFull.ok, notFull.why], [false, 'nofree'], 'the program says the home wedge is not full: no overflow');
  assert.ok(notFull.full.length > 0, 'full provinces are reported for the next bells\' skip');
  const skipped = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|11', nowBell: 11, homeFull: false, skip: new Set(notFull.full), read: async () => { throw new Error('read'); } }).catch(() => 'read');
  assert.notEqual(skipped, 'read', 'known-full provinces are not read again');
  const ok = await J.planTicket({ overviews: overviews(), faction: 0, ringsOpen: 4, seed: 'W|11', nowBell: 11, read: async (p, q) => prov(p, q) });
  const rings = ok.sites.map(s => G.ringOf(s.p, s.q));
  assert.deepEqual(rings, [...rings].sort((x, y) => x - y));
});

test('a ticket sent and then failed or expired on chain clears the lock and shows the retry; an unknown outcome keeps waiting (re-check major, 10)', async () => {
  const C = await import('../../permutation-server/web/frontier/controller.mjs');
  const { FS } = await import('../../permutation-server/web/frontier/fstate.mjs');
  const io = await import('../../permutation-server/web/frontier/fchainio.mjs');
  const fui = await import('../../permutation-server/web/frontier/fui.mjs');
  const { PROGRAM, SEASON_ID } = await import('../screens/world.mjs');
  io.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID });
  C.useHerald({
    season: async () => ({ ok: true, record: { rings: [{}, {}, {}, {}] } }),
    overview: async d => ({ ok: d >= 2, ...(overviews().get(d) ?? {}) }),
    province: async (p, q) => ({ ok: true, province: prov(p, q) }),
  });
  const { encode: b58 } = await import('../../permutation-server/web/sdk/base58.mjs');
  let nth = 0;
  const run = async (state) => {
    const wallet = b58(new Uint8Array(32).fill(0x40 + nth++));
    let sends = 0;
    C.useTransport({ submit: async () => { sends++; return { ok: true, signature: 'S' }; }, track: async () => (state === 'landed' ? { ok: true, state } : { ok: false, state, code: state === 'expired' ? 'Expired' : state === 'failed' ? 'CohortFull' : 'Unconfirmed' }) });
    Object.assign(FS, { land: { stage: 'joined' }, session: { publicKeyBytes: new Uint8Array(32) }, sessionProblem: null, citizen: { faction: 0 }, wallet: { address: wallet },
      overviews: new Map(), record: { rings: [{}] }, autoTicket: null, autoTicketBusy: false, overviewsBell: undefined, autoProv: undefined, autoFull: undefined, autoForceAt: 0, notice: null });
    await C.autoTicket();
    const rec = fui.loadLand(fui.uiStorage, fui.landKey(io.scope(), wallet));
    await C.autoTicket();   // what the refresh right after does
    return { sends, state: FS.autoTicket.state, rec };
  };
  for (const st of ['failed', 'expired']) {
    const r = await run(st);
    assert.equal(r.sends, 1);
    assert.equal(r.state, 'failed', `${st}: the card stays on the failure (with its retry)`);
    assert.equal(r.rec.ticketSeen, true, `${st}: the lock is cleared`);
    assert.equal(r.rec.autoTryFailed, true);
  }
  const unknown = await run('unknown');
  assert.equal(unknown.state, 'sent', 'unknown outcome: keep waiting, no second send');
  assert.deepEqual([unknown.rec.ticketSeen, unknown.rec.lastTicketBell], [false, 0], 'the lock names the planning bell');
  const landed = await run('landed');
  assert.equal(landed.state, 'sent');
  C.useTransport(null);
});
