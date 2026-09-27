#!/usr/bin/env node
// Synthetic herald fixtures for the Frontier web tests (W2-E): the files a
// herald serves (contract §8.4, §9.2, §9.3) for one small localnet season
// on the test beacon (I-53), built from the ABI layout table so every
// account is byte-exact. They stand in for the recording of a local season
// that W6-D will take (web design §13.1); nothing here came from a chain.
//
//   node test/fixtures/frontier/make-fixtures.mjs          write them here
//   (web-frontier-herald.test.mjs checks they are fresh)
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';
import { ACCOUNTS, RECORDS, RULESET_HASH, TEST_BEACON } from '../../../../permutation-server/web/frontier/abi.mjs';
import { ringProvinces, regionOf, wedgeOf } from '../../../../permutation-server/web/frontier/fgeo.mjs';
import { seasonAddresses, hostId } from '../../../../permutation-server/web/frontier/faddr.mjs';
import { decode as fromBase58, encode as toBase58 } from '../../../../permutation-server/web/sdk/base58.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const vectors = JSON.parse(readFileSync(join(HERE, '..', '..', 'frontier-vectors.json'), 'utf8'));
const presets = JSON.parse(readFileSync(join(HERE, '..', '..', '..', '..', 'frontier-abi', 'vectors', 'presets.json'), 'utf8'));

export const PROGRAM = vectors.addresses.program;
export const SEASON_ID = 1;
export const GENESIS_TS = 1_800_000_000;
export const BELL = 40;
export const PROVINCE = { p: 2, q: 0 };
export const WALLET = vectors.addresses.citizen.wallet;
export const SESSION = toBase58(new Uint8Array(32).fill(7));
export const LATEST_UNIX = GENESIS_TS + 600 * BELL + 1_234;
export const LATEST_SLOT = 123_456;

const hex = h => Uint8Array.from(Buffer.from(h, 'hex'));
const b64 = b => Buffer.from(b).toString('base64');

// ------------------------------------------------------------------ an encoder over the layout table
function writeScalar(dv, o, ty, v) {
  switch (ty) {
    case 'u8': return dv.setUint8(o, Number(v));
    case 'u16': return dv.setUint16(o, Number(v), true);
    case 'u32': return dv.setUint32(o, Number(v), true);
    case 'u64': return dv.setBigUint64(o, BigInt(v), true);
    case 'i16': return dv.setInt16(o, Number(v), true);
    case 'i32': return dv.setInt32(o, Number(v), true);
    case 'i64': return dv.setBigInt64(o, BigInt(v), true);
    default: throw new Error(`scalar ${ty}`);
  }
}
const snake = name => name.replace(/[A-Z]/g, c => `_${c}`).toUpperCase();
function writeFields(bytes, dv, base, fields, values) {
  for (const [key, v] of Object.entries(values)) {
    const f = fields.find(x => x[0] === snake(key));
    if (!f) throw new Error(`no field ${key}`);
    const [, off, len, ty] = f;
    const o = base + off;
    let m;
    if (/^[ui](8|16|32|64)$/.test(ty)) writeScalar(dv, o, ty, v);
    else if ((m = /^\[u8;(\d+)\]$/.exec(ty))) { const b = typeof v === 'string' ? hex(v) : Uint8Array.from(v); if (b.length !== +m[1]) throw new Error(`${key}: ${b.length} B`); bytes.set(b, o); }
    else if ((m = /^\[(u16|u32|u64|i64);(\d+)\]$/.exec(ty))) { const w = { u16: 2, u32: 4, u64: 8, i64: 8 }[m[1]]; v.forEach((x, i) => writeScalar(dv, o + i * w, m[1], x)); }
    else if ((m = /^rec:(\w+) x(\d+)$/.exec(ty))) {
      const rec = RECORDS[m[1]];
      const list = +m[2] === 1 ? [v] : v;
      list.forEach((x, i) => { if (x) writeFields(bytes, dv, o + i * rec.size, rec.fields, x); });
    } else throw new Error(`type ${ty} (${len})`);
  }
}
/** An account's bytes: magic, the header's season id and layout version, then the given fields. */
export function encodeAccount(kind, values) {
  const a = ACCOUNTS[kind];
  const bytes = new Uint8Array(a.size), dv = new DataView(bytes.buffer);
  bytes.set(Buffer.from(a.magic, 'latin1'), 0);
  const header = a.chained ? { seasonId: SEASON_ID, layoutVersion: 1 } : { seasonId: SEASON_ID };
  writeFields(bytes, dv, 0, a.fields, { ...header, ...values });
  return bytes;
}

// ------------------------------------------------------------------ the season
function seasonBytes() {
  const p = presets.presets.find(x => x.name === 'M1_LOCAL_7D').fields;
  return encodeAccount('Season', {
    status: 2, bump: seasonAddresses(PROGRAM, SEASON_ID).bump, regions: p.regions, genesisRing: p.genesis_ring, rMax: p.r_max,
    officeTermsPerWallet: 1, rulesetHash: RULESET_HASH, rulesVersion: 10, programVersion: p.program_version, bellSecs: 600,
    genesisTs: GENESIS_TS, createdTs: GENESIS_TS - 900, joinCloseBell: p.join_close_bell, endBell: p.end_bell,
    drandGenesis: TEST_BEACON.genesis, drandPeriod: TEST_BEACON.period, network: 2, quicknetPkHash: TEST_BEACON.pkHash,
    revealWindow: p.reveal_window, seedMargin: p.seed_margin, windowNext: p.reveal_window, windowFromBell: 0xffffffff,
    genesisRound: 35_732_212, genesisSeed: '11'.repeat(32), archiveAfter: p.archive_after, minLead: 2, maxLead: 72, transitSlots: 4,
    marchFee: p.march_fee, sealBond: p.seal_bond, minRevealPriorityMilli: p.min_reveal_priority_milli, revealCuLimit: p.reveal_cu_limit,
    bucketRatePerH: p.bucket_rate_per_h, bucketBurst: p.bucket_burst, defenceCapMilli: p.defence_cap_milli, latenessSlots: p.lateness_slots,
    revealLoadedLimit: p.reveal_loaded_limit,
  });
}

// ------------------------------------------------------------------ a province and its bell
const { p: P, q: Q } = PROVINCE;
const HOST = hostId({ p: P, q: Q, site: 3, gen: 0, seq: 7 });
function provinceBytes() {
  const mirror = Array.from({ length: 12 }, (_, s) => ({ state: s === 3 ? 1 : 0, faction: s === 3 ? 0 : 6, tier: s === 3 ? 1 : 0, garrison: s === 3 ? 300 : 0 }));
  return encodeAccount('Province', {
    eventSeq: 9, p: P, q: Q, ring: 2, wedge: wedgeOf(P, Q), region: regionOf(P, Q), resolvedNext: BELL + 1, openedBell: 3,
    nEntries: 1, nSitesUsed: 1, rosterEpoch: 4, sites: [2, 5, 9, 14, 20, 24, 30, 36, 41, 47, 52, 58], siteCount: 12,
    passableMask: (1n << 61n) - 1n, siteMirror: mirror,
    entries: [{ id: HOST, faction: 0, unit: 0, tile: 14, state: 1, troops: 500, staminaValue: 100 }],
    camp: { tile: 33, state: 1, troops: 250 },
  });
}
function slotBytes() {
  return encodeAccount('ArrivalSlot', { p: P, q: Q, bell: BELL, faction: 1, i: 0, unit: 0, stance: 1, tile: 33, retreatBps: 0, hostId: 999n, citizenTag: 42n, depMass: 800, dealtBps: 10_000 });
}
function dayBytes() {
  const bits = new Uint8Array(18);
  bits[BELL >> 3] |= 1 << (BELL & 7);
  return encodeAccount('ArrivalDay', { p: P, q: Q, day: 0, bits });
}
function anchorBytes(region) {
  return encodeAccount('BellAnchor', { bell: BELL, region, net: 2, round: 35_732_412, a: GENESIS_TS + 600 * (BELL + 1) + 2, slot: 100_000, sig48: '93'.repeat(48) });
}

// ------------------------------------------------------------------ the overview of ring 2
function overviewBytes() {
  const provs = ringProvinces(2).sort((a, b) => a.p - b.p || a.q - b.q);
  const out = new Uint8Array(32 + 24 * provs.length), dv = new DataView(out.buffer);
  out.set(Buffer.from('PSFOV1\0\0', 'latin1'), 0);
  dv.setBigUint64(8, BigInt(SEASON_ID), true);
  dv.setUint16(16, 2, true);
  dv.setUint16(18, provs.length, true);
  dv.setUint32(20, BELL, true);
  dv.setBigUint64(24, BigInt(LATEST_SLOT), true);
  provs.forEach((v, i) => {
    const o = 32 + 24 * i;
    dv.setInt16(o, v.p, true);
    dv.setInt16(o + 2, v.q, true);
    let owners = 0n, sites = 0;
    for (let s = 0; s < 12; s++) {
      const held = (s + i) % 4 === 0;
      owners |= BigInt(held ? (i % 6) : 7) << BigInt(3 * s);
      sites |= (held ? 1 : s === 11 ? 2 : 0) << (2 * s);
    }
    for (let j = 0; j < 5; j++) out[o + 4 + j] = Number((owners >> BigInt(8 * j)) & 0xffn);
    out[o + 9] = sites & 0xff; out[o + 10] = (sites >> 8) & 0xff; out[o + 11] = (sites >> 16) & 0xff;
    for (let f = 0; f < 7; f++) out[o + 12 + f] = f === i % 6 ? 2 : 0;
    out[o + 19] = (v.p === P && v.q === Q ? 1 : 0) | (i === 5 ? 2 : 0);
    dv.setUint32(o + 20, BELL + 1, true);
  });
  return out;
}

// ------------------------------------------------------------------ one viewer
function citizenBytes() {
  return encodeAccount('Citizen', {
    wallet: fromBase58(WALLET), session: fromBase58(SESSION), sessionExpiry: GENESIS_TS + 30 * 86_400, faction: 0, flags: 1 | 4, holdingsN: 1,
    exploresFloorLeft: 3, joinBell: 2, holding: [{ p: P, q: Q, site: 3, gen: 0 }], ticketBell: 0xffffffff,
    citizenTag: BigInt(vectors.addresses.citizen.citizen_tag),
  });
}
export const SEAL_ROOT = 'a5'.repeat(32);
function holdingBytes() {
  return encodeAccount('Holding', {
    p: P, q: Q, site: 3, gen: 0, tile: 14, state: 2, faction: 0, order: 1, tier: 1, hostSeq: 8, foundedTs: GENESIS_TS + 1_300,
    transit: [null, { state: 1, unit: 0, faction: 0, originTile: 14, originP: P, originQ: Q, hostId: HOST, departBell: BELL - 1, arriveBell: BELL + 3, depMass: 400, sealRoot: SEAL_ROOT, tip: 14_441, flags: 3 }, null, null],
  });
}

/** Every fixture file: name → Buffer. */
export function fixtures() {
  const A = seasonAddresses(PROGRAM, SEASON_ID);
  const region = regionOf(P, Q);
  const files = new Map();
  const json = (name, v) => files.set(name, Buffer.from(`${JSON.stringify(v, null, 1)}\n`));
  json('season.json', {
    v: 1, programId: PROGRAM, cluster: 'localnet', season: String(SEASON_ID), seasonAddress: A.season, genesisTs: GENESIS_TS, bellSecs: 600, W: 600, delta: 60,
    drand: { chainHash: TEST_BEACON.chainHash, publicKey: TEST_BEACON.publicKey, period: TEST_BEACON.period, genesis: TEST_BEACON.genesis },
    rulesetHash: RULESET_HASH, rMax: 16, rings: [{ d: 0, seed: '01'.repeat(32) }, { d: 1, seed: '02'.repeat(32) }, { d: 2, seed: '03'.repeat(32) }],
    tipPriorityMilli: 433, revealCuLimit: 26_000, marchFee: '10000', sealBond: '20000', quotas: { perDay: 40, burst: 60 },
    headSeq: '812', latestSlot: LATEST_SLOT, latestUnix: LATEST_UNIX, bytes_b64: b64(seasonBytes()),
  });
  const env = {
    v: 1, key: `pv:${P},${Q}`, bell: BELL, slot: LATEST_SLOT - 50, seq: '9', head: '7e'.repeat(32), bytes: b64(provinceBytes()),
    slots: [{ key: `ar:${P},${Q},${BELL},1,0`, slot: LATEST_SLOT - 300, bytes: b64(slotBytes()) }],
    day: { key: `ad:${P},${Q},0`, bytes: b64(dayBytes()) },
    inputs: null,
  };
  json(`province-${P},${Q}-${BELL}.json`, env);
  files.set(`overview-2-${BELL}.bin`, Buffer.from(overviewBytes()));
  json(`bell-${BELL}-region-${region}.json`, {
    v: 1, bell: BELL, region, anchor: { key: `an:${BELL},${region}`, address: A.of('BellAnchor', { bell: BELL, region }), bytes_b64: b64(anchorBytes(region)) },
    S: 35_732_633, caches: [{ nonce: 0, round: 35_732_633, seed: '5e'.repeat(32) }], tombstoned: false, archived: false, resolved: [[P, Q]],
  });
  json('me.json', {
    v: 1, wallet: WALLET, citizen: { address: A.of('Citizen', { wallet: WALLET }), bytes_b64: b64(citizenBytes()) },
    holdings: [{ address: A.of('Holding', { p: P, q: Q, site: 3 }), bytes_b64: b64(holdingBytes()) }], slots: [], quota: { left: 38, resetsAt: GENESIS_TS + 86_400 },
  });
  json('events-0.json', { events: [{ seq: '1', slot: 10, sig: '1'.repeat(64), kind: 2, body_b64: 'AQI=' }, { seq: '2', slot: 11, sig: '2'.repeat(64), kind: 4, body_b64: 'AQQ=' }], next: '2' });
  return files;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  for (const [name, data] of fixtures()) writeFileSync(join(HERE, name), data);
  console.log(`wrote ${fixtures().size} fixture files to ${HERE}`);
}
