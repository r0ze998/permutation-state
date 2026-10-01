// People (web/frontier/people/): names and faces derived from citizen tags,
// the roster file, the people scene (departures show their origin and
// arrival bell only), highlights, leaders and signed profiles.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import * as I from '../../permutation-server/web/frontier/people/identity.mjs';
import * as A from '../../permutation-server/web/frontier/people/avatar.mjs';
import * as R from '../../permutation-server/web/frontier/people/roster.mjs';
import * as S from '../../permutation-server/web/frontier/people/scene.mjs';
import * as U from '../../permutation-server/web/frontier/people/ui.mjs';
import * as LD from '../../permutation-server/web/frontier/people/leaders.mjs';
import * as P from '../../permutation-server/web/frontier/people/profile.mjs';
import { encode as toBase58 } from '../../permutation-server/web/sdk/base58.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

const tags = n => Array.from({ length: n }, (_, i) => BigInt.asUintN(64, BigInt(i + 1) * 0x9e3779b97f4a7c15n));

test('identity: the same tag gives the same name and face; tags from bytes are little-endian', () => {
  const a = I.identityOf(123456789n), b = I.identityOf(123456789n);
  assert.equal(a, b);
  assert.deepEqual(I.identityOf(I.tagOf(Uint8Array.of(0x15, 0xcd, 0x5b, 0x07, 0, 0, 0, 0, 9, 9))), a, 'the first 8 bytes, little-endian');
  assert.equal(I.tagOf('75bcd15'), 123456789n);
  assert.match(a.given.en, /^[A-Z][a-z]+$/);
  assert.match(a.given.ja, /^[゠-ヿ]+$/u);
  assert.equal(I.displayName(a, { language: 'en', full: true }), `${a.given.en} ${a.house.en}`);
  assert.equal(I.displayName(a, { language: 'ja', full: true }), `${a.given.ja}・${a.house.ja}`);
});

test('identity: 1,000 players get readable, mostly distinct names (and full names never repeat)', () => {
  const ids = tags(1000).map(t => I.identityOf(t));
  const given = new Set(ids.map(x => x.given.en)), full = new Set(ids.map(x => I.displayName(x, { language: 'en', full: true })));
  assert.ok(given.size > 850, `${given.size} distinct given names`);
  assert.equal(full.size, 1000);
  for (const x of ids) assert.doesNotMatch(x.given.en.toLowerCase(), /(..)\1|[aeiou]{3}/, x.given.en);
});

test('identity: nothing in it tells a human from a shade (the tag is the only input)', () => {
  assert.deepEqual(Object.keys(I.identityOf(1n)).sort(), ['face', 'given', 'house', 'profile', 'tag']);
});

test('avatar: SVG markup only (no style attribute), faction colours, the badge only where it reads', () => {
  const id = I.identityOf(42n);
  const big = A.avatarSvg(id, 3, { size: 64, title: 'X' }), small = A.avatarSvg(id, 3, { size: 20 });
  assert.match(big, /^<svg [^>]*viewBox="0 0 64 64"/);
  assert.doesNotMatch(big, /style=/);
  assert.match(big, new RegExp(A.FACTION_FILL[3]));
  assert.match(big, /role="img" aria-label="X"/);
  assert.match(small, /aria-hidden="true"/);
  assert.ok(big.length > small.length, 'the badge is drawn at 64 px, not at 20');
});

test('roster: decode the herald file; owners by site, only from their founding bell; own holdings seed it', async () => {
  const bytes = new Uint8Array(32 + 196);
  bytes.set(new TextEncoder().encode('PSFRS1\0\0'));
  const dv = new DataView(bytes.buffer);
  dv.setUint16(16, 3, true); dv.setUint16(18, 1, true); dv.setUint32(20, 99, true);
  dv.setInt16(32, -2, true); dv.setInt16(34, 5, true);
  dv.setBigUint64(36 + 4 * 16, 77n, true); dv.setUint32(44 + 4 * 16, 50, true); bytes[48 + 4 * 16] = 2;
  const d = R.decodeRoster(bytes);
  assert.equal(d.ring, 3);
  assert.deepEqual(d.provinces.get('-2,5')[4], { tag: 77n, bell: 50, tier: 2 });
  assert.equal(d.provinces.get('-2,5')[0], null);
  assert.throws(() => R.decodeRoster(bytes.subarray(0, 40)), /roster/);
  let changed = 0;
  const r = R.createRoster({ fetch: async () => ({ ok: true, arrayBuffer: async () => bytes.buffer }), onChange: () => changed++ });
  r.ensure(3);
  await new Promise(res => setTimeout(res, 0));
  assert.equal(changed, 1);
  assert.equal(r.ownerOf(-2, 5, 4).tag, 77n);
  assert.equal(r.ownerOf(-2, 5, 4, 49), null, 'not before its founding bell');
  const missing = R.createRoster({ fetch: async () => ({ ok: false, status: 404 }) });
  missing.ensure(0);
  await new Promise(res => setTimeout(res, 0));
  assert.equal(missing.ownerOf(-2, 5, 4), null, 'no roster: no made-up owner');
  S.seedOwn(missing, [{ p: 1, q: 1, site: 2, ownerCitizen: Uint8Array.of(5, 0, 0, 0, 0, 0, 0, 0) }]);
  assert.equal(missing.ownerOf(1, 1, 2).tag, 5n);
});

const overviews = new Map([[3, { provinces: [{ p: 3, q: 0, owners: [2, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7], sites: [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }] }]]);
// host id: province index << 44 | site << 40 | gen << 32 | seq; (3, 0) is found through hostParts
import { hostId } from '../../permutation-server/web/frontier/faddr.mjs';

test('scene: a departure shows its origin and arrival bell only, while on the road', () => {
  const host = hostId({ p: 3, q: 0, site: 0, gen: 1, seq: 1 });
  const chronicle = [{ record: { name: 'DEPART', bell: 40, host_id: host, origin_p: 3, origin_q: 0, origin_tile: 30, depart_bell: 40, arrive_bell: 43, dep_mass: 1200, seal: new Uint8Array(165), commit: new Uint8Array(32) } }];
  const on = S.departuresAt(chronicle, overviews, 41);
  assert.deepEqual(on, [{ p: 3, q: 0, tile: 30, faction: 2, troops: 1200, arriveBell: 43, host: String(host) }]);
  assert.deepEqual(Object.keys(on[0]).sort(), ['arriveBell', 'faction', 'host', 'p', 'q', 'tile', 'troops'], 'nothing that points at a destination');
  assert.deepEqual(S.departuresAt(chronicle, overviews, 43), [], 'arrived');
  assert.deepEqual(S.departuresAt(chronicle, overviews, 39), [], 'not yet departed');
});

test('highlights: departures name their lord and the arrival bell; settlements and clashes', () => {
  setLang('en');
  const host = hostId({ p: 3, q: 0, site: 0, gen: 1, seq: 1 });
  const roster = { ownerOf: (p, q, s) => (p === 3 && q === 0 && s === 0 ? { tag: 9n, bell: 0 } : null) };
  const who = I.displayName(I.identityOf(9n));
  const items = U.highlights([
    { record: { name: 'SETTLE', bell: 10, p: 3, q: 0, site: 0, outcome: 0, citizen_tag: 9n } },
    { record: { name: 'DEPART', bell: 40, host_id: host, arrive_bell: 43 } },
    { record: { name: 'CLASH', bell: 43, p: 3, q: 0 } },
  ], overviews, roster);
  assert.deepEqual(items.map(x => x.kind), ['clash', 'depart', 'settle']);
  assert.equal(items[1].text, `${who} of Cinder sets out (arrives at bell 43)`);
  assert.equal(items[2].text, `${who} of Cinder settles in province 3,0`);
  setLang('ja');
});

test('leaders: six portraits, one per faction, with names in both languages', () => {
  assert.equal(LD.LEADERS.length, 6);
  for (let f = 0; f < 6; f++) {
    const svg = LD.leaderSvg(f, { size: 120 });
    assert.match(svg, /viewBox="0 0 240 300"/);
    assert.doesNotMatch(svg, /style=/);
    assert.match(svg, new RegExp(A.FACTION_FILL[f]));
    assert.match(LD.LEADERS[f].name.en, /^[A-Z][a-z]+ [A-Z][a-z]+$/);
  }
});

test('profile: a wallet-signed name verifies; another wallet or an edited name does not', async () => {
  const subtle = webcrypto.subtle;
  const kp = await subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify']);
  const wallet = toBase58(new Uint8Array(await subtle.exportKey('raw', kp.publicKey)));
  const sign = async m => new Uint8Array(await subtle.sign({ name: 'Ed25519' }, kp.privateKey, m));
  const p = await P.makeProfile({ season: 7, wallet, name: 'Kaito', ts: 1000 }, sign);
  assert.equal(await P.verifyProfile(p, subtle), true);
  assert.equal(await P.verifyProfile({ ...p, name: 'Mallory' }, subtle), false);
  const other = await subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify']);
  assert.equal(await P.verifyProfile({ ...p, wallet: toBase58(new Uint8Array(await subtle.exportKey('raw', other.publicKey))) }, subtle), false);
  assert.equal(P.validName('<b>x</b>'), null);
  assert.equal(P.validName('  カイト  '), 'カイト');
  assert.equal(P.validName('x'.repeat(25)), null);
  const id = I.withProfile(I.identityOf(1n), p);
  assert.equal(I.displayName(id, { full: true, language: 'en' }), 'Kaito');
});

import * as ACT from '../../permutation-server/web/frontier/people/activity.mjs';

test('activities: walls, recruits, garrison, muster, rest, guard, battle, depart; a sealed march has no destination', () => {
  setLang('ja');
  const NO = ACT.NO_BELL;
  const e = {
    p: 3, q: 0, rec: { clash: true, dormant: false, sites: [1], owners: [2] },
    prov: {
      sites: Uint8Array.from([9]), camp: { state: 1, tile: 40, troops: 5000 },
      siteMirror: [{ state: 1, faction: 2, garrison: 250_000, wallItem0Bell: 45, wallItem0Delta: 10, wallItem1Bell: NO, wallItem1Delta: 0, pend0Bell: 43, pend0Delta: 100_000, pend1Bell: NO, pend1Delta: 0, shieldUntilBell: 0 }],
      entries: [
        { id: 1n, faction: 2, tile: 9, state: 2, troops: 300_000, staminaValue: 120, staminaBell: 0, readyBell: 0 },
        { id: 2n, faction: 2, tile: 12, state: 1, troops: 500_000, staminaValue: 120, staminaBell: 0, readyBell: 0 },
        { id: 3n, faction: 4, tile: 13, state: 1, troops: 200_000, staminaValue: 2, staminaBell: 41, readyBell: 44 },
      ],
    },
  };
  const acts = ACT.tileActivities(e, { bell: 41, departures: [{ p: 3, q: 0, tile: 20, faction: 2, troops: 9, arriveBell: 44, host: '7' }] });
  assert.deepEqual(acts.get(9).map(a => a.kind), ['muster', 'walls', 'recruit', 'garrison']);
  assert.equal(acts.get(9).find(a => a.kind === 'garrison').n, 250, 'milli-troops shown as troops');
  assert.deepEqual(acts.get(12).map(a => a.kind), ['battle', 'guard']);
  assert.deepEqual(acts.get(13).map(a => a.kind), ['battle', 'rest']);
  assert.deepEqual(acts.get(20).map(a => a.kind), ['depart']);
  assert.deepEqual(acts.get(40).map(a => a.kind), ['camp']);
  assert.equal(ACT.activityText(acts.get(20)[0]), '出陣中（第44鐘に到着、行き先は秘密）');
  assert.equal(ACT.activityText(acts.get(9)[1]), '城壁を建設中（第45鐘に完成）');
  setLang('en');
  assert.match(ACT.activityText(acts.get(13)[1]), /^Resting \(ready at bell \d+\)$/, 'ready once it has the stamina to march again');
  setLang('ja');
});

import * as BT from '../../permutation-server/web/frontier/people/battle.mjs';

test('battle scene: the arrivals where they fought, the defenders of that tile, losses and fates from the bytes', () => {
  const inputs = { arrivals: [
    { present: 1, hostId: 11n, faction: 0, tile: 55, stance: 3, troops: 300_000, troopsAfter: 260_000, fate: 3, citizenTag: 9n },
    { present: 0, hostId: 12n, faction: 1, tile: 20, stance: 0, troops: 1, troopsAfter: 0, fate: 0 },
  ] };
  const before = { sites: Uint8Array.from([55]), siteMirror: [{ state: 1, faction: 2, garrison: 50_000 }], camp: { state: 0 },
    entries: [{ id: 5n, faction: 2, tile: 55, state: 1, troops: 100_000 }, { id: 6n, faction: 2, tile: 3, state: 1, troops: 100_000 }] };
  const after = { siteMirror: [{ state: 1, faction: 2, garrison: 20_000 }], entries: [] };
  const sc = BT.battleScene({ p: 2, q: 1, bell: 45, inputs, before, after });
  assert.equal(sc.tiles.length, 1, 'only the tile an arrival fought on');
  const t = sc.tiles[0];
  assert.deepEqual(t.attackers.map(a => [a.faction, a.stance, a.before, a.after, a.fate]), [[0, 3, 300, 260, 'Bounced']]);
  assert.deepEqual(t.defenders.map(d => [d.kind, d.before, d.after, d.fate]), [['resident', 100, 0, 'Destroyed'], ['garrison', 50, 20, 'Stays']]);
  assert.equal(BT.losses(t.attackers), 40);
  assert.equal(BT.losses(t.defenders), 130);
  assert.equal(BT.battleScene({ p: 0, q: 0, bell: 1, inputs: { arrivals: [] } }), null);
  assert.equal(BT.battleScene({ p: 2, q: 1, bell: 45, inputs, before }).tiles[0].defenders[0].after, null, 'without the province after, losses are unknown');
});
