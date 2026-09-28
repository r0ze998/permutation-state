// The fixture world of the screenshot smoke (web design §13.2): one small
// localnet season on the test beacon, built from the ABI layout table by
// the same encoder as the node tests' herald fixtures
// (test/fixtures/frontier/make-fixtures.mjs), so every account the page
// decodes is byte-exact. Nothing here came from a chain.
//
// The viewer is the page's own dev wallet with a fixed key (the page reads
// `ps-dev-wallet` from its storage; the test writes it before load) and a
// fixed in-game key, so /h/me can name them. Three viewer stages:
//   none     no Citizen yet (join and faction)
//   joined   a Citizen with its in-game key, no land (the site picker)
//   holding  a Citizen with a final Holding at province (2,0), three hosts
//            (one marching, one free, one Scout) and a resolved clash at
//            bell 39 in its province (report, tracker, bell sheet)
import { readFileSync } from 'node:fs';
import { keyFromSeed } from '../../permutation-server/web/session.mjs';
import { ACCOUNTS } from '../../permutation-server/web/frontier/abi.mjs';
import { toHex } from '../../permutation-server/web/sdk/bytes.mjs';
import { decode as fromBase58 } from '../../permutation-server/web/sdk/base58.mjs';
import { KINDS } from '../../permutation-server/web/frontier/flog.mjs';
import { ringProvinces, regionOf, wedgeOf } from '../../permutation-server/web/frontier/fgeo.mjs';
import { seasonAddresses, hostId } from '../../permutation-server/web/frontier/faddr.mjs';
import { encodeAccount, fixtures, PROGRAM, SEASON_ID, GENESIS_TS, BELL, LATEST_UNIX, LATEST_SLOT } from '../test/fixtures/frontier/make-fixtures.mjs';

export { PROGRAM, SEASON_ID, GENESIS_TS, BELL, LATEST_UNIX, LATEST_SLOT };
export const DEV_WALLET_NAME = 'Dev Wallet (localnet)';
/** The dev wallet's and the in-game key's seeds (test keys, never funded anywhere). */
export const DEV_SEED = new Uint8Array(32).fill(0x11);
export const SESSION_SEED = new Uint8Array(32).fill(0x22);
// The home province's sites and passable camp tile follow frontier.wasm's
// generate_province for ring seed 03…03 (the fixture season's ring 2), so
// the tile LOD, the site picker and the path planner agree.
export const HOME_SITES = Object.freeze([22, 39, 3, 7, 10, 19, 24, 26, 43, 56, 0, 29]);
export const CAMP_TILE = 32;
export const HOME = Object.freeze({ p: 2, q: 0, site: 3, gen: 0, tile: HOME_SITES[3] });
export const REPORT_BELL = BELL - 1;
const NO_BELL = 0xffffffff;
const b64 = b => Buffer.from(b).toString('base64');

export const HOSTS = Object.freeze({
  marching: hostId({ ...HOME, seq: 7 }),
  free: hostId({ ...HOME, seq: 8 }),
  scout: hostId({ ...HOME, seq: 9 }),
});

/** Keys and addresses of the viewer (async: WebCrypto Ed25519). */
export async function viewer() {
  const wallet = (await keyFromSeed(DEV_SEED)).publicKey;
  const session = (await keyFromSeed(SESSION_SEED)).publicKey;
  const A = seasonAddresses(PROGRAM, SEASON_ID);
  return { wallet, session, citizen: A.of('Citizen', { wallet }), holding: A.of('Holding', HOME), A };
}

/** localStorage the page finds at load for a viewer stage and language. */
export function storageFor(v, { stage, lang }) {
  const out = { 'ps-lang': lang, 'ps-dev-wallet': toHex(DEV_SEED), 'ps-wallet': DEV_WALLET_NAME };
  if (stage !== 'none') out[`ps-fsession:localnet:${PROGRAM}:${SEASON_ID}:${v.wallet}`] = toHex(SESSION_SEED);
  return out;
}

// ------------------------------------------------------------------ accounts
function citizenBytes(v, stage) {
  const land = stage === 'holding';
  return encodeAccount('Citizen', {
    wallet: fromBase58(v.wallet), session: fromBase58(v.session), sessionExpiry: GENESIS_TS + 30 * 86_400, faction: 0,
    flags: land ? 1 | 4 : 1, holdingsN: land ? 1 : 0, exploresFloorLeft: 3, joinBell: 2, vigilStartMin: 0,
    holding: land ? [{ p: HOME.p, q: HOME.q, site: HOME.site, gen: HOME.gen }] : [], ticketBell: NO_BELL, citizenTag: 0x5eed_0001n,
  });
}

const accrual = (value, rate, cap) => ({ value: value * 1000, rate: rate * 1000, cap: cap * 1000, t0: LATEST_UNIX - 1800, frac: 0 });
function holdingBytes(v) {
  return encodeAccount('Holding', {
    p: HOME.p, q: HOME.q, site: HOME.site, gen: HOME.gen, tile: HOME.tile, state: 2, ownerCitizen: fromBase58(v.citizen), faction: 0, order: 1, tier: 1,
    ticketBell: 3, foundedTs: GENESIS_TS + 1_300, foundedDay: 0, hostSeq: 10, lastOwnerAction: LATEST_UNIX - 3_600, shieldUntil: LATEST_UNIX + 7_200,
    stores: [accrual(1_200, 40, 6_000), accrual(800, 30, 6_000), accrual(650, 25, 6_000), accrual(300, 10, 4_000), accrual(120, 5, 2_000), accrual(90, 0, 2_000), accrual(40, 0, 1_000), accrual(20, 0, 1_000)],
    queue: [{ doneAt: LATEST_UNIX + 1_500, kind: 1, arg: 0, delta: 0 }],
    reserve: [300, 0, 0, 0, 0, 0, 100, 0],
    transit: [null, { state: 1, unit: 0, faction: 0, originTile: HOME.tile, originP: HOME.p, originQ: HOME.q, hostId: HOSTS.marching, departBell: BELL - 1, arriveBell: BELL + 3,
      departTs: LATEST_UNIX - 900, depMass: 400, marchStamina: 74, sealRoot: 'a5'.repeat(32), tip: 14_472, flags: 3 }, null, null],
    finalTs: GENESIS_TS + 20_000,
  });
}

const FREE_MIRROR = () => Array.from({ length: 12 }, () => ({ state: 0, faction: 6, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallItem0Bell: NO_BELL, wallItem1Bell: NO_BELL }));
/** A province account: the home province carries the viewer's holding, hosts and the camp; every other one is open land. */
function provinceBytes(p, q) {
  const home = p === HOME.p && q === HOME.q;
  const mirror = FREE_MIRROR();
  const entries = [];
  if (home) {
    mirror[HOME.site] = { state: 1, faction: 0, order: 1, tier: 1, gen: HOME.gen, garrison: 300_000, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallItem0Bell: NO_BELL, wallItem1Bell: NO_BELL };
    mirror[7] = { state: 1, faction: 2, order: 1, tier: 1, gen: 0, garrison: 120_000, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallItem0Bell: NO_BELL, wallItem1Bell: NO_BELL };
    entries.push(
      { id: HOSTS.marching, faction: 0, unit: 0, tile: HOME.tile, state: 3, troops: 400_000, staminaValue: 46, staminaBell: BELL - 1, dealtBps: 10_000, fromBell: 20 },
      { id: HOSTS.free, faction: 0, unit: 0, tile: HOME.tile, state: 1, troops: 600_000, staminaValue: 110, staminaBell: BELL, dealtBps: 10_000, fromBell: 30 },
      { id: HOSTS.scout, faction: 0, unit: 6, tile: HOME.tile, state: 1, troops: 100_000, staminaValue: 120, staminaBell: BELL, dealtBps: 10_000, fromBell: 30 },
    );
  } else if ((p * 7 + q * 3) % 4 === 0) {
    mirror[0] = { state: 1, faction: ((p + 6) % 6 + 6) % 6, order: 1, tier: 1, gen: 0, garrison: 90_000, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallItem0Bell: NO_BELL, wallItem1Bell: NO_BELL };
  }
  return encodeAccount('Province', {
    eventSeq: 9, p, q, ring: (Math.abs(p) + Math.abs(q) + Math.abs(p + q)) / 2, wedge: wedgeOf(p, q) ?? 0xff, region: regionOf(p, q), resolvedNext: BELL + 1, openedBell: 3,
    nEntries: entries.length, nSitesUsed: mirror.filter(m => m.state === 1).length, rosterEpoch: 4, sites: home ? HOME_SITES : [2, 5, 9, 14, 20, 24, 30, 36, 41, 47, 52, 58], siteCount: 12,
    passableMask: (1n << 61n) - 1n, siteMirror: mirror, entries,
    camp: home ? { tile: CAMP_TILE, state: 1, troops: 250, nextCheckDay: 1, gen: 1 } : { tile: 0, state: 0, troops: 0, nextCheckDay: 1, gen: 0 },
  });
}

/** The overview of ring d (§9.3), a few sites held per province. */
function overviewBytes(d) {
  const provs = ringProvinces(d).sort((a, b) => a.p - b.p || a.q - b.q);
  const out = new Uint8Array(32 + 24 * provs.length), dv = new DataView(out.buffer);
  out.set(Buffer.from('PSFOV1\0\0', 'latin1'), 0);
  dv.setBigUint64(8, BigInt(SEASON_ID), true);
  dv.setUint16(16, d, true);
  dv.setUint16(18, provs.length, true);
  dv.setUint32(20, BELL, true);
  dv.setBigUint64(24, BigInt(LATEST_SLOT), true);
  provs.forEach((v, i) => {
    const o = 32 + 24 * i;
    dv.setInt16(o, v.p, true);
    dv.setInt16(o + 2, v.q, true);
    let owners = 0n, sites = 0;
    const home = v.p === HOME.p && v.q === HOME.q;
    for (let s = 0; s < 12; s++) {
      const held = home ? s === HOME.site || s === 7 : d > 0 && (s + i) % 5 === 0;
      const f = home ? (s === HOME.site ? 0 : 2) : (i + d) % 6;
      owners |= BigInt(held ? f : 7) << BigInt(3 * s);
      sites |= (held ? 1 : home && s === 11 ? 2 : 0) << (2 * s);
    }
    for (let j = 0; j < 5; j++) out[o + 4 + j] = Number((owners >> BigInt(8 * j)) & 0xffn);
    out[o + 9] = sites & 0xff; out[o + 10] = (sites >> 8) & 0xff; out[o + 11] = (sites >> 16) & 0xff;
    for (let f = 0; f < 7; f++) out[o + 12 + f] = home ? (f === 0 ? 3 : 0) : f === (i + d) % 6 ? 1 : 0;
    out[o + 19] = home ? 1 : 0;
    dv.setUint32(o + 20, BELL + 1, true);
  });
  return out;
}

// ------------------------------------------------------------------ the clash of bell 39 in the home province
const hid = (site, gen, seq) => hostId({ p: HOME.p, q: HOME.q, site, gen, seq });
const ARRIVALS = [
  { hostId: hid(7, 0, 1), citizenTag: 11n, depMass: 900, troops: 900_000, stamina: 60, retreat: 0, dealt: 10_000, faction: 2, unit: 0, tile: HOME.tile, stance: 1, present: 1, fate: 5, troopsAfter: 0 },
  { hostId: HOSTS.free, citizenTag: 0x5eed_0001n, depMass: 600, troops: 600_000, stamina: 90, retreat: 15_000, dealt: 10_000, faction: 0, unit: 0, tile: CAMP_TILE, stance: 3, present: 1, fate: 1, troopsAfter: 520_000 },
];
function clashInputsBytes() {
  return encodeAccount('ClashInputs', { p: HOME.p, q: HOME.q, bell: REPORT_BELL, arrivalsMask: 0xffffff, flags: 2, nPresent: 2, arrivals: ARRIVALS });
}
export function clashReport() {
  const region = regionOf(HOME.p, HOME.q);
  return { v: 1, bell: REPORT_BELL, province: [HOME.p, HOME.q], inputs_b64: b64(clashInputsBytes()), seed: '5e'.repeat(32),
    anchor: { key: `an:${REPORT_BELL},${region}`, A: GENESIS_TS + 600 * BELL + 2 }, cache: { key: `sd:${REPORT_BELL},${region},0` },
    outcomeDigest: '3c'.repeat(32), inputDigest: '00'.repeat(32), decoded: { engagements: 3 } };
}

// ------------------------------------------------------------------ PS2 events (§6), encoded field for field from flog.mjs's table
function ps2(kind, bell, fields) {
  const spec = KINDS[kind];
  const parts = [Uint8Array.of(1, kind, bell & 0xff, (bell >> 8) & 0xff, (bell >> 16) & 0xff, (bell >>> 24) & 0xff)];
  for (const [name, len] of [...spec.key, ...spec.payload]) {
    const v = fields[name] ?? 0;
    const b = new Uint8Array(len);
    if (v instanceof Uint8Array) b.set(v.slice(0, len));
    else { let x = BigInt(v); if (x < 0n) x += 1n << BigInt(8 * len); for (let i = 0; i < len; i++) { b[i] = Number(x & 0xffn); x >>= 8n; } }
    parts.push(b);
  }
  parts.push(Uint8Array.of(0));
  return Buffer.concat(parts);
}
export function events() {
  const list = [
    ps2(4, 3, { d: 2, t_open: GENESIS_TS + 1_800 }),
    ps2(6, 3, { p: HOME.p, q: HOME.q, ring: 2, wedge: wedgeOf(HOME.p, HOME.q), region: regionOf(HOME.p, HOME.q), site_count: 12, camp_tile: CAMP_TILE, camp_troops: 250 }),
    ps2(10, 4, {}),
    ps2(14, 5, { p: HOME.p, q: HOME.q, site: HOME.site, outcome: 0 }),
    ps2(16, 29, { p: HOME.p, q: HOME.q, site: HOME.site, final_ts: GENESIS_TS + 20_000 }),
    ps2(43, 30, { p: HOME.p, q: HOME.q, tile: CAMP_TILE, troops: 250, day: 0 }),
    ps2(30, 36, { host_id: hid(7, 0, 1), origin_p: 3, origin_q: -1, origin_tile: 20, depart_bell: 36, arrive_bell: REPORT_BELL, dep_mass: 900 }),
    ps2(41, 39, { p: HOME.p, q: HOME.q, bell: REPORT_BELL, engagements: 3 }),
    ps2(30, 39, { host_id: HOSTS.marching, origin_p: HOME.p, origin_q: HOME.q, origin_tile: HOME.tile, depart_bell: BELL - 1, arrive_bell: BELL + 3, dep_mass: 400 }),
    ps2(34, 40, { host_id: hid(7, 0, 1), outcome: 4 }),
  ];
  const base = 800;
  return list.map((body, i) => ({ seq: String(base + i), slot: LATEST_SLOT - 1_000 + i * 50, sig: String(i + 1).repeat(64).slice(0, 64), body_b64: b64(body) }));
}

// ------------------------------------------------------------------ the herald's files
const SEASON = JSON.parse(fixtures().get('season.json').toString());
/**
 * The node tests' season record, with the M1_LOCAL_7D dormancy and release
 * times written into the Season bytes (the shared fixture leaves them 0,
 * which would show every holding as dormant).
 */
const PRESET = JSON.parse(readFileSync(new URL('../../frontier-abi/vectors/presets.json', import.meta.url), 'utf8')).presets.find(x => x.name === 'M1_LOCAL_7D').fields;
function seasonBytes() {
  const b = Buffer.from(SEASON.bytes_b64, 'base64');
  const field = name => ACCOUNTS.Season.fields.find(f => f[0] === name)[1];
  b.writeUInt32LE(PRESET.dormant_after_secs, field('DORMANT_AFTER_SECS'));
  b.writeUInt32LE(PRESET.release_after_secs, field('RELEASE_AFTER_SECS'));
  return b.toString('base64');
}
const SEASON_B64 = seasonBytes();
export const seasonRecord = () => ({ ...SEASON, headSeq: '812', bytes_b64: SEASON_B64 });

/** `/h/me/{wallet}` for a stage (a wallet that has not joined: a record with no Citizen). */
export function meRecord(v, stage) {
  if (stage === 'none') return { v: 1, wallet: v.wallet, citizen: null, holdings: [], slots: [] };
  const rec = { v: 1, wallet: v.wallet, citizen: { address: v.citizen, bytes_b64: b64(citizenBytes(v, stage)) }, holdings: [], slots: [], quota: { left: 38, resetsAt: GENESIS_TS + 86_400 } };
  if (stage === 'holding') rec.holdings = [{ address: v.holding, bytes_b64: b64(holdingBytes(v)) }];
  return rec;
}

/** `/h/province/{P},{Q}/{bell}` (the latest file is bell 40's). */
export function provinceEnvelope(p, q, bell = BELL) {
  return { v: 1, key: `pv:${p},${q}`, bell, slot: LATEST_SLOT - 50, seq: '9', head: '7e'.repeat(32), bytes: b64(provinceBytes(p, q)), slots: [], day: null, inputs: null };
}

export const overview = d => Buffer.from(overviewBytes(d));

/** `/h/bell/{b}/region/{r}`: anchored and seeded up to bell 40, open after it. */
export function bellRegion(bell, region) {
  if (bell > BELL) return { v: 1, bell, region, anchor: null, S: null, caches: [], tombstoned: false, archived: false, resolved: [] };
  const a = GENESIS_TS + 600 * (bell + 1) + 2;
  const anchor = encodeAccount('BellAnchor', { bell, region, net: 2, round: 35_732_412, a, slot: 100_000, sig48: '93'.repeat(48) });
  return { v: 1, bell, region, anchor: { key: `an:${bell},${region}`, bytes_b64: b64(anchor) }, S: 35_732_633, caches: [{ nonce: 0, round: 35_732_633, seed: '5e'.repeat(32), A: a }],
    tombstoned: false, archived: false, resolved: bell === REPORT_BELL ? [[HOME.p, HOME.q]] : [] };
}
