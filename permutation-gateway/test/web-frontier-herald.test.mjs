// The Frontier read path (permutation-server/web/frontier/herald.mjs) over
// a fixture herald on 127.0.0.1:0 serving the synthetic fixtures of
// test/fixtures/frontier (checked fresh here): raw bytes decoded by the
// client and checked against the key asked for and the pinned season (the
// herald's JSON conveniences are ignored), immutable files cached and
// "latest" not, the overview binary of §9.3, the WS diff sequence and
// resync, the poll schedule (own 10/30 s, overview once a bell with a
// 0–15 s jitter, backoff ×2 to 5 min, paused when hidden), on-screen
// province selection and the staleness banner. Never rejects.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync, readdirSync } from 'node:fs';
import * as herald from '../../permutation-server/web/frontier/herald.mjs';
import { checkBeacon } from '../../permutation-server/web/frontier/seal.mjs';
import { reconcile, entryOf } from '../../permutation-server/web/frontier/marchbook.mjs';
import { TEST_BEACON } from '../../permutation-server/web/frontier/abi.mjs';
import * as F from './fixtures/frontier/make-fixtures.mjs';

const DIR = new URL('./fixtures/frontier/', import.meta.url);
const file = name => readFileSync(new URL(name, DIR));

test('the fixtures are fresh (node test/fixtures/frontier/make-fixtures.mjs)', () => {
  const want = F.fixtures();
  // `recorded/` holds the files recorded from a local season (W6-D; web-frontier-recorded.test.mjs checks them).
  const have = readdirSync(DIR).filter(n => n !== 'make-fixtures.mjs' && n !== 'recorded').sort();
  assert.deepEqual(have, [...want.keys()].sort());
  for (const [name, data] of want) assert.ok(file(name).equals(data), `${name} is stale`);
});

// ------------------------------------------------------------------ a fixture herald on port 0
const hits = [];
let overrides = {};
const server = http.createServer((req, res) => {
  hits.push(req.url);
  const send = (status, body, type = 'application/json') => { res.writeHead(status, { 'Content-Type': type }); res.end(body); };
  if (overrides[req.url]) return send(200, overrides[req.url]);
  const routes = {
    '/h/season': 'season.json',
    '/h/province/2,0/40': 'province-2,0-40.json',
    '/h/province/2,0/latest': 'province-2,0-40.json',
    '/h/province/3,0/40': 'province-2,0-40.json',
    '/h/overview/2/40.bin': 'overview-2-40.bin',
    '/h/overview/3/40.bin': 'overview-2-40.bin',
    [`/h/me/${F.WALLET}`]: 'me.json',
    '/h/events?after=0': 'events-0.json',
  };
  const bell = readdirSync(DIR).find(n => n.startsWith('bell-'));
  routes[`/h/bell/40/region/${bell.match(/region-(\d+)/)[1]}`] = bell;
  const name = routes[req.url];
  if (!name) return send(404, '{"error":"not found"}');
  send(200, file(name), name.endsWith('.bin') ? 'application/octet-stream' : 'application/json');
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
after(() => server.close());
const base = `http://127.0.0.1:${server.address().port}`;
const H = () => herald.createHerald({ base, seasonId: F.SEASON_ID });

test('GET /h/season: the Season decoded from its bytes; the beacon pin passes on localnet only', async () => {
  const r = await H().season();
  assert.equal(r.ok, true, r.error);
  assert.equal(r.season.seasonId, 1n);
  assert.equal(r.season.status, 2);
  assert.equal(r.season.genesisTs, BigInt(F.GENESIS_TS));
  assert.equal(Buffer.from(r.season.quicknetPkHash).toString('hex'), TEST_BEACON.pkHash);
  assert.equal(checkBeacon({ drand: r.record.drand, seasonPkHash: r.season.quicknetPkHash, cluster: r.record.cluster }).ok, true);
  assert.equal(checkBeacon({ drand: r.record.drand, seasonPkHash: r.season.quicknetPkHash, cluster: 'devnet' }).code, 'TestBeaconOffLocalnet');
  // A herald that edits its JSON convenience fields changes nothing the client uses.
  const rec = JSON.parse(file('season.json'));
  overrides = { '/h/season': JSON.stringify({ ...rec, genesisTs: 1, season: '1' }) };
  const r2 = await H().season();
  assert.equal(r2.season.genesisTs, BigInt(F.GENESIS_TS), 'bytes, not the JSON');
  overrides = { '/h/season': JSON.stringify({ ...rec, season: '2' }) };
  assert.equal((await herald.createHerald({ base }).season()).code, 'WrongSeason', 'record and bytes disagree');
  assert.equal((await herald.createHerald({ base, seasonId: 9 }).season()).code, 'WrongSeason', 'not the pinned season');
  overrides = {};
});

test('province envelopes: decoded, checked against the key and the season; immutable bells cached, latest not', async () => {
  const h = H();
  hits.length = 0;
  const r = await h.province(2, 0, 40);
  assert.equal(r.ok, true, r.error);
  assert.equal(r.province.p, 2);
  assert.equal(r.province.resolvedNext, 41);
  assert.equal(r.slots.length, 1);
  assert.equal(r.slots[0].account.bell, 40);
  assert.equal(r.day.day, 0);
  assert.equal(r.inputs, null);
  await h.province(2, 0, 40);
  assert.equal(hits.filter(u => u === '/h/province/2,0/40').length, 1, 'an immutable bell is fetched once');
  await h.province(2, 0);
  await h.province(2, 0);
  assert.equal(hits.filter(u => u === '/h/province/2,0/latest').length, 2, 'latest is never cached');
  assert.equal((await h.province(3, 0, 40)).code, 'WrongKey', 'the body is another province');
  const env = JSON.parse(file('province-2,0-40.json'));
  const bad = (patch, code) => assert.throws(() => herald.parseEnvelope({ ...env, ...patch }, { seasonId: 1, p: 2, q: 0 }), e => e.code === code);
  bad({ key: 'pv:2,1' }, 'WrongKey');
  bad({ slots: [{ ...env.slots[0], key: 'ar:2,0,41,1,0' }] }, 'WrongKey');
  bad({ day: { ...env.day, key: 'ad:2,0,1' } }, 'WrongKey');
  bad({ v: 2 }, 'BadEnvelope');
  assert.throws(() => herald.parseEnvelope(env, { seasonId: 2 }), e => e.code === 'WrongSeason');
  const tampered = Buffer.from(env.bytes, 'base64');
  tampered[0] ^= 1;
  bad({ bytes: tampered.toString('base64') }, 'WrongMagic');
});

test('the overview binary (§9.3): header, sorted records, owners, site states, flags', async () => {
  const h = H();
  const r = await h.overview(2, 40);
  assert.equal(r.ok, true, r.error);
  assert.equal(r.ring, 2);
  assert.equal(r.bell, 40);
  assert.equal(r.provinces.length, 12);
  for (let i = 1; i < 12; i++) { const a = r.provinces[i - 1], b = r.provinces[i]; assert.ok(a.p < b.p || (a.p === b.p && a.q < b.q)); }
  const home = r.provinces.find(x => x.p === 2 && x.q === 0);
  assert.equal(home.clash, true);
  assert.equal(home.resolvedNext, 41);
  const i0 = r.provinces[0];
  assert.deepEqual(i0.owners.filter((_, s) => i0.sites[s] === 1), [0, 0, 0]);
  assert.equal(i0.sites[11], 2, 'a camp');
  assert.equal(herald.majorityOwner(i0), 0);
  assert.equal(herald.majorityOwner({ owners: new Array(12).fill(7), sites: new Array(12).fill(0) }), null);
  assert.equal(r.provinces[5].dormant, true);
  assert.equal((await h.overview(3, 40)).code, 'WrongKey');
  const bin = Uint8Array.from(file('overview-2-40.bin'));
  const swapped = Uint8Array.from(bin);
  swapped.set(bin.subarray(32, 56), 56); swapped.set(bin.subarray(56, 80), 32);
  assert.throws(() => herald.decodeOverview(swapped), e => e.code === 'BadOverview');
  assert.throws(() => herald.decodeOverview(bin.subarray(0, 40)), e => e.code === 'BadOverview');
  assert.throws(() => herald.decodeOverview(bin, { seasonId: 2 }), e => e.code === 'WrongSeason');
});

test('bell-region, me and events: decoded and checked; the marchbook reconciles with the Holding', async () => {
  const h = H();
  const bellFile = readdirSync(DIR).find(n => n.startsWith('bell-'));
  const region = +bellFile.match(/region-(\d+)/)[1];
  const b = await h.bellRegion(40, region);
  assert.equal(b.ok, true, b.error);
  assert.equal(b.anchor.bell, 40);
  assert.equal(b.anchor.region, region);
  const me = await h.me(F.WALLET);
  assert.equal(me.ok, true, me.error);
  assert.equal(me.citizen.faction, 0);
  assert.equal(me.holdings.length, 1);
  const t = me.holdings[0].transit[1];
  assert.equal(t.state, 1);
  assert.equal(Buffer.from(t.sealRoot).toString('hex'), F.SEAL_ROOT);
  const e = { ...entryOf({ host: t.hostId, transitSlot: 1, departBell: t.departBell, arriveBell: t.arriveBell, holding: 'x', plain: new Uint8Array(37), salt: new Uint8Array(32), commit: new Uint8Array(32), sealRoot: Buffer.from(F.SEAL_ROOT, 'hex'), ctHash: new Uint8Array(32), round: 1, tip: 14441 }), state: 'sent' };
  const transits = me.holdings[0].transit.map((x, slot) => ({ slot, state: x.state, hostId: x.hostId, arriveBell: x.arriveBell, sealRoot: Buffer.from(x.sealRoot).toString('hex') }));
  assert.equal(reconcile([e], { transits })[0].state, 'landed');
  const ev = await h.events(0);
  assert.equal(ev.ok, true);
  assert.equal(ev.events.length, 2);
  assert.equal(ev.next, '2');
});

test('never rejects: network errors, 404 and bad bodies are answers', async () => {
  assert.equal((await herald.createHerald({ base: 'http://127.0.0.1:9' }).season()).code, 'network');
  assert.equal((await H().province(9, 9, 1)).code, 'NotFound');
  overrides = { '/h/season': 'not json' };
  assert.equal((await H().season()).code, 'BadBody');
  overrides = {};
});

test('WS diffs: in sequence, duplicates ignored, a gap resyncs from the files', () => {
  let resyncs = 0;
  const d = new herald.DiffStream({ onResync: () => resyncs++ });
  assert.equal(d.accept({ seq: '10' }), 'applied');
  assert.equal(d.accept({ seq: '11' }), 'applied');
  assert.equal(d.accept({ seq: '11' }), 'duplicate');
  assert.equal(d.accept({ seq: '9' }), 'duplicate');
  assert.equal(d.accept({ seq: '13' }), 'gap');
  assert.equal(resyncs, 1);
  assert.equal(d.accept({ seq: '20' }), 'applied', 'a new run after the resync');
  assert.equal(d.accept({ seq: 'x' }), 'invalid');
  assert.equal(d.accept({ seq: '18446744073709551615' }), 'gap');
});

test('polling: own 10 s in flight / 30 s idle, overview after the bell + 0–15 s, backoff ×2 to 5 min, paused when hidden', () => {
  assert.equal(herald.nextPoll({ kind: 'own', inFlight: true }), 10);
  assert.equal(herald.nextPoll({ kind: 'own' }), 30);
  assert.equal(herald.nextPoll({ kind: 'own', hidden: true }), null);
  assert.equal(herald.nextPoll({ kind: 'own', errors: 2 }), 120);
  assert.equal(herald.nextPoll({ kind: 'own', errors: 9 }), 300);
  for (const rnd of [0, 0.5, 0.999]) {
    const t = herald.nextPoll({ kind: 'overview', secondsToBellEnd: 100, random: () => rnd });
    assert.ok(t >= 100 && t < 115, String(t));
  }
  assert.equal(herald.nextPoll({ kind: 'overview', secondsToBellEnd: 100, hidden: true }), null);
});

test('only what is on screen: own provinces always, ≤ 12 visible at tile LOD; the staleness banner past 60 s', () => {
  const visible = Array.from({ length: 30 }, (_, i) => ({ p: i, q: 0 }));
  const own = [{ p: 50, q: 1 }];
  assert.deepEqual(herald.wantedProvinces({ visible, own, lod: 'world' }), own);
  const tile = herald.wantedProvinces({ visible, own, lod: 'tile' });
  assert.equal(tile.length, 13);
  assert.equal(herald.wantedProvinces({ visible: [{ p: 50, q: 1 }], own, lod: 'tile' }).length, 1);
  assert.deepEqual(herald.staleness({ latestUnix: 1000, chainNow: 1060 }), { behind: 60, stale: false });
  assert.deepEqual(herald.staleness({ latestUnix: 1000, chainNow: 1061 }), { behind: 61, stale: true });
  assert.deepEqual(herald.staleness({ latestUnix: null, chainNow: 5 }), { behind: null, stale: false });
  const lru = new herald.Lru(2);
  lru.set('a', 1); lru.set('b', 2); lru.get('a'); lru.set('c', 3);
  assert.deepEqual([...lru.m.keys()], ['a', 'c']);
});
