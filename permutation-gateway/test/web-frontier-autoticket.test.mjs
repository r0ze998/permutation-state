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

test('autoTicket: a refusal before sending locks nothing; "file again now" files again at once (finding 1); overviews re-read once a bell (finding 3)', async () => {
  const C = await import('../../permutation-server/web/frontier/controller.mjs');
  const { FS } = await import('../../permutation-server/web/frontier/fstate.mjs');
  let provinceReads = 0, seasonReads = 0;
  C.useHerald({
    season: async () => { seasonReads++; return { ok: true, record: { rings: [{}, {}, {}, {}] } }; },
    overview: async d => ({ ok: d >= 2, ...(overviews().get(d) ?? {}) }),
    province: async (p, q) => { provinceReads++; return { ok: true, province: prov(p, q) }; },
  });
  Object.assign(FS, { land: { stage: 'joined' }, session: { publicKeyBytes: new Uint8Array(32) }, sessionProblem: null, citizen: { faction: 0 }, wallet: { address: 'W1' },
    overviews: new Map(), record: { rings: [{}] }, ui: { ticketSeen: true, lastTicketBell: null, autoTryBell: null }, autoTicket: null, autoTicketBusy: false, overviewsBell: undefined });
  await C.autoTicket();
  assert.equal(FS.autoTicket.state, 'failed', 'no pinned season: the send is refused before it leaves');
  assert.notEqual(FS.ui.ticketSeen, false, 'no "on its way" lock after a refusal');
  assert.equal(seasonReads, 1);
  assert.ok(FS.overviews.size >= 2, 'the open rings\' overviews were read');
  const before = provinceReads;
  await C.autoTicket({ force: true });
  assert.ok(provinceReads > before, 'the button plans and files again at once');
  assert.equal(seasonReads, 1, 'overviews once a bell');
  FS.land = { stage: 'final' };
  await C.autoTicket();
  assert.equal(FS.autoTicket, null, 'nothing to do with a holding');
});
