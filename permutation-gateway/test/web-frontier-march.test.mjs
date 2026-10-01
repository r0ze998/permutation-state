// The Frontier march and play logic of the web page (W3-F; contract §5.10,
// §5.11, §6, §8.3, §9.1, §9.4, I-08, I-27, I-29, I-32, I-44, I-47, I-56):
//  - the rules constants and tables the page pre-checks with, pinned against
//    their Rust definitions; the PS2 kinds it decodes, against logs.json;
//  - the composer: the retreat list ("never" = 0 first), exactly the three
//    tip presets (no zero tip), the arrival window, every pre-send refusal,
//    the plaintext; the WASM planner, earliest bell and `reachable` through
//    the kernel's recorded answers;
//  - the send flow: seal (real IBE, test key) → marchbook saved before
//    Depart is signed → sent → on chain; refusals, lost answers, no storage;
//  - the Depart transaction ≤ 800 B;
//  - the tracker (host locked until settled: HostInTransit), incoming
//    warnings, the holding (stores as the kernel settles them), land states,
//    the site picker, the refile offer, and the UI preferences.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import * as march from '../../permutation-server/web/frontier/fmarch.mjs';
import * as land from '../../permutation-server/web/frontier/fland.mjs';
import * as flog from '../../permutation-server/web/frontier/flog.mjs';
import * as fui from '../../permutation-server/web/frontier/fui.mjs';
import * as book from '../../permutation-server/web/frontier/marchbook.mjs';
import * as seal from '../../permutation-server/web/frontier/seal.mjs';
import * as W from '../../permutation-server/web/frontier/wasm.mjs';
import * as geo from '../../permutation-server/web/frontier/fgeo.mjs';
import * as io from '../../permutation-server/web/frontier/fchainio.mjs';
import { accountsFor, buildShape, ITEM_WALLS } from '../../permutation-server/web/frontier/fplay.mjs';
import { decode } from '../../permutation-server/web/frontier/fcodec.mjs';
import { hostId } from '../../permutation-server/web/frontier/faddr.mjs';
import { seasonClock } from '../../permutation-server/web/frontier/clock.mjs';
import { decodeOverview } from '../../permutation-server/web/frontier/herald.mjs';
import { TEST_BEACON } from '../../permutation-server/web/frontier/abi.mjs';
import * as composer from '../../permutation-server/web/frontier/screens/march.mjs';
import * as trackerScreen from '../../permutation-server/web/frontier/screens/tracker.mjs';
import * as hostScreen from '../../permutation-server/web/frontier/screens/host.mjs';
import * as incomingScreen from '../../permutation-server/web/frontier/screens/incoming.mjs';
import { encodeAccount } from '../../permutation-server/web/sdk/frontier/codec.mjs';
import { minTipLamports, tipPresets } from '../../permutation-server/web/sdk/frontier/fees.mjs';
import { wireTransaction } from '../../permutation-server/web/sdk/solana-tx.mjs';
import { encode as b58 } from '../../permutation-server/web/sdk/base58.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

const REPO = new URL('../../', import.meta.url);
const read = p => readFileSync(new URL(p, REPO), 'utf8');
const FIX = new URL('fixtures/frontier/', import.meta.url);
const V = JSON.parse(read('frontier-wasm/vectors/wasm-vectors.json'));
const LOGS = JSON.parse(read('frontier-abi/vectors/logs.json'));
const ADDR = JSON.parse(readFileSync(new URL('frontier-vectors.json', import.meta.url), 'utf8')).addresses;
const hex = h => Uint8Array.from(Buffer.from(h, 'hex'));
const toHex = b => Buffer.from(b).toString('hex');
const sha = (...parts) => new Uint8Array(createHash('sha256').update(Buffer.concat(parts.map(p => Buffer.from(p)))).digest());

const SEASON = decode('Season', Buffer.from(JSON.parse(readFileSync(new URL('season.json', FIX), 'utf8')).bytes_b64, 'base64'));
const CLOCK = seasonClock(SEASON);
const acct = (kind, values) => decode(kind, encodeAccount(kind, { SEASON_ID: 1n, ...values }), { seasonId: 1 });

// ------------------------------------------------------------------ pinned against the rules and the ABI
test('the rules constants and tables the page pre-checks with equal their Rust definitions', () => {
  const host = read('permutation-rules/src/frontier/host.rs'), travel = read('permutation-rules/src/frontier/travel.rs'), catalog = read('permutation-rules/src/frontier/catalog.rs');
  const num = (src, re) => { const m = re.exec(src); assert.ok(m, re); return Number(m[1].replace(/_/g, '')); };
  assert.equal(march.RULES.STAMINA_CAP, num(host, /pub const STAMINA_CAP: u16 = ([\d_]+);/));
  assert.equal(march.RULES.MIN_HOST_TROOPS, num(host, /pub const MIN_HOST_TROOPS: MilliTroops = ([\d_]+) \* MILLI/));
  assert.equal(march.RULES.MILLI, num(read('permutation-rules/src/fixed.rs'), /pub const MILLI: i64 = ([\d_]+);/));
  assert.equal(march.RULES.MARCH_STAMINA_BASE, num(travel, /pub const MARCH_STAMINA_BASE: u16 = ([\d_]+);/));
  assert.equal(march.RULES.MARCH_STAMINA_PER_HEX, num(travel, /pub const MARCH_STAMINA_PER_HEX: u16 = ([\d_]+);/));
  assert.equal(march.DEPART_STAMINA, 74, 'travel::march_stamina(32), charged at every Depart (I-32)');
  assert.equal(ITEM_WALLS, num(catalog, /pub const ITEM_WALLS: u8 = (\d+);/));
  // catalog BUILDINGS: resource, per-hour rate and first-copy cost, in item order.
  const block = catalog.slice(catalog.indexOf('pub const BUILDINGS'), catalog.indexOf('];', catalog.indexOf('pub const BUILDINGS')));
  const rows = [...block.matchAll(/resource: Resource::(\w+),\s*per_hour: (\d+),\s*cost: \[([^\]]+)\]/g)].map(m => ({ resource: m[1], perHour: +m[2], cost: m[3].split(',').map(x => +x.trim()) }));
  assert.equal(rows.length, 6);
  assert.deepEqual(land.BUILD_ITEMS.slice(0, 6).map(b => ({ resource: b.resource, perHour: b.perHour, cost: b.cost })), rows);
  assert.deepEqual(land.BUILD_ITEMS.map(b => b.item), [0, 1, 2, 3, 4, 5, 6]);
  assert.equal(land.BUILD_ITEMS[ITEM_WALLS].resource, 'Walls');
  // Units in kernel order; the Scout explores, the Settler never trains nor musters.
  assert.deepEqual(land.UNIT_ORDER, W.UNITS);
  assert.equal(W.UNITS[land.SCOUT], 'Scout');
  assert.equal(W.UNITS[land.SETTLER], 'Settler');
  // The home wedge is the faction's (the simulator's rule; the program's FileTicket checks it).
  assert.match(read('frontier-sim/src/sim.rs'), /let wedges = \[faction, \(faction \+ 1\) % 6, \(faction \+ 5\) % 6\];/);
  assert.deepEqual([0, 1, 2, 3, 4, 5].map(land.homeWedge), [0, 1, 2, 3, 4, 5]);
});

test('PS2 records: every kind the page decodes is frontier-abi\'s, field for field, and its vector decodes', () => {
  const byKind = new Map(LOGS.kinds.map(k => [k.kind, k]));
  assert.equal(LOGS.prefix, 'PS2');
  assert.equal(LOGS.version, flog.PS2_VERSION);
  for (const [kind, spec] of Object.entries(flog.KINDS)) {
    const abi = byKind.get(+kind);
    assert.ok(abi, `kind ${kind}`);
    assert.equal(spec.name, abi.name);
    assert.deepEqual(spec.key, abi.key.map(f => [f.name, f.len]), `${abi.name} key`);
    assert.deepEqual(spec.payload, abi.payload.map(f => [f.name, f.len]), `${abi.name} payload`);
    const body = hex(abi.vector.body_without_tail);
    assert.equal(body.length, abi.body_without_tail_len);
    const r = flog.decodeRecord(body);
    assert.equal(r.kind, +kind);
    assert.equal(r.logBell, abi.vector.bell);
    for (const [name] of [...spec.key, ...spec.payload]) assert.ok(name in r, `${abi.name}.${name}`);
    if (abi.vector.body) assert.equal(flog.decodeRecord(hex(abi.vector.body)).name, abi.name, 'the tail is ignored');
  }
  const dep = flog.decodeRecord(hex(byKind.get(30).vector.body_without_tail));
  assert.equal(dep.seal.length, 165);
  assert.equal(dep.commit.length, 32);
  assert.equal(typeof dep.host_id, 'bigint');
  assert.equal(flog.decodeRecord(Uint8Array.of(1, 99, 0, 0, 0, 0)), null, 'a kind the page does not read');
  assert.throws(() => flog.decodeRecord(Uint8Array.of(2, 30, 0, 0, 0, 0)), /version/);
  assert.throws(() => flog.decodeRecord(Uint8Array.of(1, 30, 0, 0, 0, 0, 1)), /short/);
  const page = flog.decodePage([{ seq: '1', body_b64: Buffer.from(hex(byKind.get(41).vector.body_without_tail)).toString('base64') }, { seq: '2', body_b64: 'AQ==' }, { seq: '3', body_b64: 'AWMAAAAA' }]);
  assert.equal(page.records.length, 1);
  assert.equal(page.records[0].record.name, 'CLASH');
  assert.equal(page.bad, 1);
  assert.equal(flog.TRANSIT_OUTCOMES.length, 8);
});

// ------------------------------------------------------------------ the composer
test('the retreat list: "never" (0) first, the ratios in bps, a custom ratio within 1..60,000 bps (I-27)', () => {
  assert.deepEqual(march.RETREAT_CHOICES.map(c => [c.id, c.bps]), [['never', 0], ['x2', 20_000], ['x1.5', 15_000], ['x1', 10_000], ['x0.5', 5_000], ['custom', null]]);
  assert.equal(march.retreatBps('never'), 0);
  assert.equal(march.retreatBps('custom', 1.25), 12_500);
  assert.equal(march.retreatBps('custom', 6), seal.RETREAT_MAX_BPS);
  assert.equal(march.retreatBps('custom', 6.001), null);
  assert.equal(march.retreatBps('custom', 0), null, 'never is its own choice, not a zero ratio');
  assert.equal(march.retreatBps('custom', 'x'), null);
  assert.equal(march.retreatBps('bogus'), null);
});

test('the tip: exactly the three sponsored presets from the Season, never zero (I-08, I-51)', () => {
  const tipMin = minTipLamports(SEASON.minRevealPriorityMilli, SEASON.revealCuLimit, SEASON.revealLoadedLimit);
  assert.equal(tipMin, 14_668n, '§10.1 at p 0.433, 26.5k CU, L(Reveal) 1,146,880 (v1.8, W5-A)');
  assert.equal(march.seasonTipMin(SEASON), tipMin);
  const o = march.tipOptions(SEASON);
  assert.deepEqual(o.map(x => x.id), ['standard', 'decisive', 'maximum']);
  assert.deepEqual(o.map(x => x.lamports), tipPresets(tipMin));
  assert.deepEqual(o.map(x => x.lamports), [14_668n, 22_002n, 29_336n]);
  assert.ok(o.every(x => x.lamports > 0n && x.priorityMilli >= 433n));
  const costs = march.marchCosts(SEASON, o[1].lamports);
  assert.deepEqual(costs, { tip: 22_002n, marchFee: 10_000n, sealBond: 20_000n, total: 52_002n });
});

/** A final holding with one host (Spearman, 500 troops) in province (2, 0), a free transit slot. */
// A provisional holding carries its final_ts (round_time(S) + 600) and its ticket bell; default: not yet due.
function world({ state = 2, stamina = 120, ready = 0, pendOp = 0, transit = [], entryState = 1, finalTs = 4_000_000_000, ticketBell = 30, cohorts = [] } = {}) {
  const id = hostId({ p: 2, q: 0, site: 3, gen: 1, seq: 7 });
  const holding = acct('Holding', { P: 2, Q: 0, SITE: 3, GEN: 1, TILE: 30, STATE: state, FACTION: 0, FINAL_TS: finalTs, TICKET_BELL: ticketBell,
    TRANSIT: [0, 1, 2, 3].map(i => transit[i] ?? { STATE: 0 }) });
  const province = acct('Province', { P: 2, Q: 0, RESOLVED_NEXT: 40, SITE_COUNT: 1, SITES: Uint8Array.of(30, ...new Array(11).fill(0)),
    TICKET_COHORTS: [0, 1, 2, 3, 4, 5, 6, 7].map(i => cohorts[i] ?? { BELL: 0, FILED: 0, SETTLED: 0 }),
    ENTRIES: [{ ID: id, FACTION: 0, UNIT: 0, TILE: 30, STATE: entryState, TROOPS: 500_000, STAMINA_VALUE: stamina, STAMINA_BELL: 40, READY_BELL: ready, PEND_OP: pendOp }] });
  return { id, holding, province, host: province.entries[0] };
}
const route = { dirs: [1, 0, 0], pathLen: 3, path: seal.encodePath([1, 0, 0]).path, secs: 360, hexes: 3, provinces: [{ p: 2, q: 0 }] };
const good = (w, patch = {}) => ({ season: SEASON, nowBell: 41, resolvedNext: 40, holding: w.holding, host: w.host, dest: { p: 2, q: 0, tile: 33 }, route,
  earliest: null, arriveBell: 43, stance: 1, retreat: { choice: 'never' }, tip: 14_668n, ...patch });

test('the arrival window: depart + min_lead … depart + max_lead from the Season, raised to the planner\'s earliest bell', () => {
  assert.deepEqual(march.arrivalWindow(SEASON, 41), { min: 43, max: 113 });
  assert.deepEqual(march.arrivalWindow(SEASON, 41, 50), { min: 50, max: 113 });
  assert.deepEqual(march.arrivalWindow(SEASON, 41, 12), { min: 43, max: 113 });
});

test('checkMarch names every refusal before anything is sealed', () => {
  const w = world();
  assert.deepEqual(march.checkMarch(good(w)), []);
  const has = (m, code) => assert.ok(march.checkMarch(m).includes(code), `${code}: ${march.checkMarch(m)}`);
  has(good(world({ state: 1 })), 'NotFinal');
  has(good(w, { resolvedNext: 38 }), 'NotResident');
  has(good(world({ pendOp: 5 })), 'HostBusy');
  has(good(world({ entryState: 3 })), 'HostBusy');
  const busy = world({ transit: [{ STATE: 2, HOST_ID: hostId({ p: 2, q: 0, site: 3, gen: 1, seq: 7 }), ARRIVE_BELL: 30 }] });
  has(good(busy), 'HostInTransit');
  has(good(world({ stamina: 72 })), 'Cooldown');
  assert.deepEqual(march.checkMarch(good(world({ stamina: 73 }))), [], 'stamina refills +1 per bell: 73 at bell 40 is the 74 Depart needs at 41');
  has(good(world({ ready: 42 })), 'Cooldown');
  has(good(world({ transit: [0, 1, 2, 3].map(() => ({ STATE: 1, HOST_ID: 9n })) })), 'TransitState');
  has(good(w, { dest: null }), 'NoDestination');
  has(good(w, { dest: { p: 2, q: 0, tile: 61 } }), 'NoDestination');
  has(good(w, { route: null }), 'NoPath');
  has(good(w, { route: { ...route, dirs: new Array(33).fill(0) } }), 'Path');
  has(good(w, { route: { ...route, provinces: [1, 2, 3, 4, 5] } }), 'Path');
  has(good(w, { arriveBell: 42 }), 'ArrivalBell');
  has(good(w, { arriveBell: 114 }), 'ArrivalBell');
  has(good(w, { earliest: 45, arriveBell: 44 }), 'ArrivalBell');
  has(good(w, { stance: 4 }), 'Stance');
  has(good(w, { retreat: { choice: 'custom', ratio: 9 } }), 'Retreat');
  has(good(w, { tip: 0n }), 'TipTooLow');
  has(good(w, { tip: 14_440n }), 'TipTooLow');
  has(good(w, { tip: 20_000n }), 'TipNotPreset');
  has(good(w, { tip: 'abc' }), 'TipTooLow');
});

test('the plaintext is the kernel\'s packing, validated for this host and bell; "never" retreats as 0', () => {
  const w = world();
  const plain = march.plaintextOf(good(w));
  assert.equal(plain.length, 37);
  const p = seal.unpack(plain);
  assert.equal(p.hostId, w.id);
  assert.deepEqual([p.arriveBell, p.destP, p.destQ, p.destTile, p.stance, p.retreatBps, p.pathLen], [43, 2, 0, 33, 1, 0, 3]);
  assert.equal(seal.validate(p, w.id, 43), null);
  assert.equal(seal.unpack(march.plaintextOf(good(w, { retreat: { choice: 'x1.5' } }))).retreatBps, 15_000);
});

/** A kernel answering from the recorded frontier-wasm calls (the real module is PENDING-OWNER, O-M1-12). */
function recordedKernel() {
  const memory = new WebAssembly.Memory({ initial: 4 });
  let top = 1024;
  const alloc = n => { const p = top; top += n + 8; return p; };
  const answers = new Map(V.calls.map(c => [`${c.export}:${c.input}`, c]));
  const x = { memory, alloc, free: () => {} };
  for (const name of W.EXPORTS) {
    x[name] = (ptr, len) => {
      const c = answers.get(`${name}:${toHex(new Uint8Array(memory.buffer, ptr, len))}`);
      const payload = c ? hex(c.output) : new TextEncoder().encode('no vector');
      const out = alloc(5 + payload.length), view = new Uint8Array(memory.buffer, out, 5 + payload.length);
      view[0] = c ? c.status : W.BAD_INPUT;
      new DataView(memory.buffer).setUint32(out + 1, payload.length, true);
      view.set(payload, 5);
      return out;
    };
  }
  return new W.Kernel(x);
}

test('the WASM planner, the earliest bell and `reachable` feed the composer and the warnings', () => {
  const k = recordedKernel();
  const pc = V.calls.find(c => c.export === 'plan_path');
  const from = geo.locate(pc.args.start.q, pc.args.start.r), to = geo.locate(pc.args.dest.q, pc.args.dest.r);
  const r = march.planRoute(k, { from: { p: from.p, q: from.q, tile: from.idx }, to: { p: to.p, q: to.q, tile: to.idx }, unit: pc.args.unit, seeds: pc.args.seeds, blocked: pc.args.blocked });
  assert.equal(r.ok, true, r.code);
  assert.equal(r.route.dirs.length, r.route.hexes);
  assert.ok(r.route.provinces.length <= 4);
  assert.equal(march.planRoute(null, {}).code, 'NoKernel');
  assert.equal(march.planRoute(k, { from: { p: 0, q: 0, tile: 70 }, to: { p: 0, q: 0, tile: 1 }, unit: 0, seeds: [] }).code, 'BadTile');
  assert.equal(march.planRoute(k, { from: { p: from.p, q: from.q, tile: from.idx }, to: { p: to.p, q: to.q, tile: to.idx }, unit: 5, seeds: [] }).code, 'PlannerRefused');
  const ea = V.calls.find(c => c.export === 'earliest_arrival_bell').args;
  const earliest = march.earliestBell(k, { genesisTs: ea.genesis_ts, departTs: ea.depart_ts, secs: ea.secs });
  assert.equal(earliest, 17);
  assert.equal(march.earliestBell(null, ea), null);
  assert.deepEqual(march.arrivalWindow(SEASON, 10, earliest), { min: 17, max: 82 });
  // Incoming: a departure of another faction that can reach the holding by its arrival bell.
  const ra = V.calls.find(c => c.export === 'reachable').args;
  const o = geo.locate(ra.origin.q, ra.origin.r), d = geo.locate(ra.dest.q, ra.dest.r);
  const departBell = (ra.depart_ts - ra.genesis_ts) / 600;
  const dep = { host_id: 1n, origin_p: o.p, origin_q: o.q, origin_tile: o.idx, depart_bell: departBell, arrive_bell: ra.target_bell, dep_mass: 5_000_000, faction: 1 };
  const holdings = [{ p: d.p, q: d.q, site: 2, tile: d.idx }];
  const w = march.incomingWarnings({ kernel: k, departures: [dep, { ...dep, faction: 0 }, { ...dep, arrive_bell: 3 }], holdings, faction: 0, genesisTs: ra.genesis_ts, nowBell: 11 });
  assert.deepEqual(w, [{ holding: { p: d.p, q: d.q, site: 2 }, bell: 16, hosts: 1, troops: 5_000 }]);
  assert.equal(march.incomingWarnings({ kernel: null, departures: [dep], holdings, faction: 0 }), null);
  setLang('ja');
  assert.match(String(incomingScreen.render({ incoming: w })), /最大 1 の軍勢（5,000 兵）/);
  assert.match(String(incomingScreen.render({ incoming: null })), /ルールのモジュールが必要/);
  setLang('en');
  assert.match(String(incomingScreen.render({ incoming: w })), /up to 1 hosts \(5,000 troops\) may arrive.*destinations unknown/);
  setLang('ja');
});

test('the composer offers the three presets only, "never" first, and names what blocks sending', () => {
  const w = world();
  const FS = { season: SEASON, nowBell: 41, holdings: [w.holding], provinces: new Map([['2,0', { province: w.province }]]),
    compose: { host: { ...w.host, troops: 500, unit: 0 }, dest: { p: 2, q: 0, tile: 33 }, route, arriveBell: 43, stance: 0, retreat: 'never', tip: '14668' } };
  const ch = composer.composerChoices(FS);
  assert.deepEqual(ch.tips.map(t => String(t.lamports)), ['14668', '22002', '29336']);
  assert.equal(ch.retreat[0].id, 'never');
  const out = String(composer.render(FS));
  const tips = [...out.matchAll(/name="tip" data-bind="tip" value="(\d+)"/g)].map(m => m[1]);
  assert.deepEqual(tips, ['14668', '22002', '29336'], 'exactly the presets; no zero, no custom tip');
  assert.doesNotMatch(out, /value="0" [^>]*data-bind="tip"|data-bind="tip" value="0"/);
  assert.match(out, /<option value="never" selected>/);
  assert.match(out, /data-act="march-send" >|data-act="march-send"\s*>/, 'ready to send');
  const blocked = String(composer.render({ ...FS, compose: { ...FS.compose, tip: '0' } }));
  assert.match(blocked, /data-act="march-send" disabled/);
  assert.match(blocked, /チップが最低額に足りません/);
  assert.match(String(composer.render({ ...FS, compose: { ...FS.compose, route: null, routeError: 'NoWasm' } })), /frontier\.wasm/);
});

// ------------------------------------------------------------------ the send flow
function memoryStorage() { const m = new Map(); return { m, get: k => m.get(k) ?? null, set: (k, v) => { m.set(k, v); return true; } }; }

test('sendMarch: seal (T(arrive), self-audited) → marchbook saved before signing → Depart → on chain', async () => {
  const w = world();
  const clock = { ...CLOCK };
  const storage = memoryStorage();
  const key = 'ps-fmarch:localnet:P:1:W';
  const steps = [];
  let submitted = null;
  const r = await march.sendMarch({ ...good(w), holdingAddress: 'HoLdInG1111111111111111111111111111111111111' }, {
    seal: req => seal.sealMarch(req, { inline: true }), clock, publicKey: TEST_BEACON.publicKey, storage, bookKey: key, session: { publicKey: 'S' },
    accounts: { a: 1 },
    submit: async req => {
      // The record is on this device before any key signs (§9.1).
      const saved = book.loadBook(storage, key);
      assert.equal(saved.length, 1);
      assert.equal(saved[0].state, 'sealed');
      assert.ok(!('k' in saved[0]) && !('sigma' in saved[0]));
      submitted = req;
      return { ok: true, signature: 'SIG' };
    },
    track: async sig => { assert.equal(sig, 'SIG'); return { ok: true, state: 'landed', slot: 9 }; },
    onStep: s => steps.push(s),
  });
  assert.equal(r.ok, true, `${r.code} ${r.error ?? ''}`);
  assert.deepEqual(steps, march.SEND_STEPS);
  assert.equal(submitted.name, 'Depart');
  assert.deepEqual(submitted.signer, { session: { publicKey: 'S' } });
  const f = submitted.fields;
  assert.equal(f.transit_slot, 0);
  assert.equal(f.arrive_bell, 43);
  assert.equal(f.tip, 14_668n);
  assert.equal(f.host_id, w.id);
  assert.equal(f.seal.length, 165);
  const e = book.loadBook(storage, key)[0];
  assert.equal(e.state, 'landed');
  assert.equal(e.signature, 'SIG');
  // The stored pair is the one Depart carried, and it names the transit's seal root.
  assert.equal(e.commit_hex, toHex(f.commit));
  assert.equal(e.seal_b64, Buffer.from(f.seal).toString('base64'));
  assert.equal(e.sealRoot_hex, toHex(sha(f.commit, sha(f.seal))));
  assert.equal(e.round, seal.sealRound(clock, 43), 'sealed to T(arrive_bell)');
  assert.equal(e.holding, 'HoLdInG1111111111111111111111111111111111111');
  // The material a self-reveal posts: no key, no signature.
  assert.deepEqual(Object.keys(book.revealMaterial(e)).sort(), ['ct_hash_b64', 'holding', 'plain_b64', 'salt_b64', 'transit_slot']);
});

test('sendMarch: a refusal marks the entry failed (it may be re-sealed); a lost answer keeps it; no storage, no send', async () => {
  const w = world();
  const base = { seal: req => seal.sealMarch(req, { inline: true }), clock: CLOCK, publicKey: TEST_BEACON.publicKey, bookKey: 'k', session: {}, accounts: {}, track: async () => ({ ok: true }) };
  let storage = memoryStorage();
  let r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage, submit: async () => ({ ok: false, code: 'TipNotPreset' }) });
  assert.deepEqual([r.ok, r.step, r.code], [false, 'sent', 'TipNotPreset']);
  assert.equal(book.loadBook(storage, 'k')[0].state, 'failed');
  r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage, submit: async () => ({ ok: true, signature: 'S2' }) });
  assert.equal(r.ok, true, 'a failed march may be sealed again');
  storage = memoryStorage();
  r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage, submit: async () => ({ ok: false, code: 'network' }) });
  assert.equal(book.loadBook(storage, 'k')[0].state, 'sealed', 'a lost answer may still land: kept for reconciliation');
  r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage, submit: async () => ({ ok: true, signature: 'S3' }), track: async () => ({ ok: false, state: 'failed', code: 'HostInTransit', programCode: 58 }) });
  assert.deepEqual([r.ok, r.step, r.code, r.programCode], [false, 'onChain', 'HostInTransit', 58]);
  assert.equal(book.loadBook(storage, 'k')[0].state, 'failed');
  let called = false;
  r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage: { get: () => null, set: () => false }, submit: async () => { called = true; return { ok: true }; } });
  assert.deepEqual([r.ok, r.code, called], [false, 'NotSaved', false], 'never sent without its record');
  r = await march.sendMarch({ ...good(w, { tip: 0n }), holdingAddress: 'h' }, { ...base, storage: memoryStorage(), submit: async () => { called = true; return { ok: true }; } });
  assert.deepEqual([r.ok, r.step, r.code, called], [false, 'check', 'TipTooLow', false]);
  r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, { ...base, storage: memoryStorage(), seal: async () => ({ ok: false, code: 'SealAuditFailed' }), submit: async () => { called = true; return { ok: true }; } });
  assert.deepEqual([r.ok, r.step, r.code, called], [false, 'sealing', 'SealAuditFailed', false]);
});

test('sendMarch: an earliest bell gone stale by send time refuses ArrivalBell before sealing (wave-3 review)', async () => {
  const w = world();
  let sealed = false, sent = false;
  const storage = memoryStorage();
  const r = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, {
    earliestAtSend: () => 44, // the chosen bell is 43
    seal: async () => { sealed = true; return { ok: false }; }, clock: CLOCK, publicKey: TEST_BEACON.publicKey,
    storage, bookKey: 'k', session: {}, accounts: {}, submit: async () => { sent = true; return { ok: true }; }, track: async () => ({ ok: true }),
  });
  assert.deepEqual([r.ok, r.step, r.code, r.earliest], [false, 'check', 'ArrivalBell', 44]);
  assert.deepEqual([sealed, sent, storage.m.size], [false, false, 0], 'nothing sealed, saved or sent');
  // A fresh earliest bell at or before the chosen one changes nothing.
  const ok = await march.sendMarch({ ...good(w), holdingAddress: 'h' }, {
    earliestAtSend: () => 43, seal: req => seal.sealMarch(req, { inline: true }), clock: CLOCK, publicKey: TEST_BEACON.publicKey,
    storage: memoryStorage(), bookKey: 'k', session: {}, accounts: {}, submit: async () => ({ ok: true, signature: 'S' }), track: async () => ({ ok: true }),
  });
  assert.equal(ok.ok, true, ok.code);
  assert.equal(march.DEPART_MARGIN_SECS, 90);
});

test('the Depart transaction the page builds is at most 800 bytes (§9.4) and within its budget ceiling', () => {
  io.setPin({ programId: ADDR.program, cluster: 'localnet', seasonId: 1 });
  const A = io.pinned().addresses;
  const session = b58(new Uint8Array(32).fill(7)), feePayer = b58(new Uint8Array(32).fill(9)), blockhash = b58(new Uint8Array(32).fill(3));
  const accounts = accountsFor('Depart', { addresses: A, wallet: ADDR.citizen.wallet, actor: session, holding: { p: 2, q: 0, site: 3 }, province: { p: 2, q: 0 } });
  const fields = { host_id: 2n ** 63n, commit: new Uint8Array(32), seal: new Uint8Array(165), arrive_bell: 4_000_000, tip: 29_336n, transit_slot: 3 };
  const { message, budget, signers } = buildShape({ programId: A.programId, name: 'Depart', accounts, fields, feePayer, blockhash });
  assert.deepEqual(signers, [session]);
  const wire = wireTransaction(message, [new Uint8Array(64), new Uint8Array(64)]);
  assert.ok(wire.length <= 800, `Depart is ${wire.length} B`);
  assert.ok(wire.length <= budget.txCeiling);
  assert.deepEqual(io.messageProblems(message, { feePayer, blockhash, tag: 0x50, signers, cuLimit: budget.cuLimit, loadedLimit: budget.loadedLimit, expected: buildShape({ programId: A.programId, name: 'Depart', accounts, fields, feePayer, blockhash }).expected }), []);
  assert.throws(() => accountsFor('Reveal', { addresses: A }), /not a shape this page sends/, 'a Reveal is never a client transaction (I-24)');
});

// ------------------------------------------------------------------ the tracker and the lock
test('the tracker: steps from chain facts; the host stays locked until its transit is settled (HostInTransit)', () => {
  const e = { ...book.entryOf({ host: 5n, transitSlot: 1, departBell: 40, arriveBell: 43, holding: 'h', plain: new Uint8Array(37), salt: new Uint8Array(32), commit: new Uint8Array(32), sealRoot: new Uint8Array(32), ctHash: new Uint8Array(32), round: 1, tip: 14472 }), state: 'landed' };
  const live = { state: 1, hostId: 5n, arriveBell: 43 };
  let t = march.tracker({ entry: e, transit: live, nowBell: 41, pipeline: 'open' });
  assert.deepEqual(t.steps.map(s => s.state), ['done', 'now', 'next', 'next', 'next', 'next']);
  assert.equal(t.locked, true);
  t = march.tracker({ entry: e, transit: live, nowBell: 44, pipeline: 'revealing', slotPresent: true });
  assert.deepEqual(t.steps.map(s => s.state), ['done', 'done', 'done', 'done', 'now', 'next']);
  t = march.tracker({ entry: e, transit: { ...live, state: 2 }, nowBell: 46, pipeline: 'resolved' });
  assert.equal(t.closedUnrevealed, true, 'no slot when the window closed: routed or refused at settlement');
  assert.equal(t.locked, true, 'values settled (state 2) is still in transit until SettleTransit');
  t = march.tracker({ entry: e, transit: null, nowBell: 50, pipeline: 'resolved', settled: { outcome: 'Stays' } });
  assert.deepEqual([t.locked, t.steps.every(s => s.state === 'done'), t.outcome], [false, true, 'Stays']);
  t = march.tracker({ entry: null, transit: live, nowBell: 41 });
  assert.equal(t.hint, 'keepersWillReveal');
  const selfE = { ...e, attempts: [{ t: 1, route: 'self', result: 'accepted' }], state: 'revealing' };
  assert.equal(march.tracker({ entry: selfE, transit: live, nowBell: 43 }).route, 'self');
  setLang('ja');
  const card = String(trackerScreen.renderMarch({ entry: e, transit: live, facts: { nowBell: 41, pipeline: 'open' } }));
  assert.match(card, /進軍の精算が済むまで出発・解散・探索できません/);
  assert.match(card, /あなたにだけ見えます/);
  setLang('en');
  assert.match(String(trackerScreen.renderMarch({ entry: e, transit: live, facts: { nowBell: 41, pipeline: 'open' } })), /cannot depart, dissolve or explore until its march is settled/);
  setLang('ja');
  // The host panel says the same and disables its actions.
  const w = world({ transit: [{ STATE: 1, HOST_ID: hostId({ p: 2, q: 0, site: 3, gen: 1, seq: 7 }), ARRIVE_BELL: 43 }] });
  const FS = { holdings: [w.holding], provinces: new Map([['2,0', { province: w.province }]]), nowBell: 41 };
  const rows = hostScreen.hostRows(FS);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].inTransit, true);
  assert.deepEqual(land.actionBlocks('Depart', { holding: w.holding, province: w.province, nowBell: 41, host: rows[0] }), ['HostInTransit']);
  const html = String(hostScreen.render(FS));
  assert.match(html, /data-act="compose" data-host="\d+" disabled/);
  assert.match(html, /この軍勢は進軍の精算が済むまで/);
});

// ------------------------------------------------------------------ holdings, land and preferences
test('stores settle as the kernel does (Accrual::value_at), pinned by the recorded accrual_at call', () => {
  const c = V.calls.find(x => x.export === 'accrual_at');
  const a = Object.fromEntries(Object.entries(c.args.accrual).map(([k, v]) => [k, BigInt(v)]));
  assert.equal(land.accrualAt(a, Number(c.args.t)), W.DECODE.accrual_at(hex(c.output)));
  assert.equal(land.accrualAt({ value: 1000n, rate: 3600n, cap: 5000n, t0: 0n, frac: 0n }, 3600), 4600n);
  assert.equal(land.accrualAt({ value: 6000n, rate: 3600n, cap: 5000n, t0: 0n, frac: 0n }, 3600), 6000n, 'above the cap: kept, not raised');
  assert.equal(land.accrualAt({ value: 1000n, rate: -3601n, cap: 5000n, t0: 0n, frac: 0n }, 1), 998n, 'floor division as div_euclid');
  assert.equal(land.accrualAt({ value: 10n, rate: -36_000n, cap: 0n, t0: 0n, frac: 0n }, 7200), 0n, 'never below zero');
  assert.equal(land.accrualAt({ value: 7n, rate: 99n, cap: 9n, t0: 100n, frac: 0n }, 50), 7n, 'before t0');
  const h = acct('Holding', { STORES: [{ VALUE: 250_000n, RATE: 12_000n, CAP: 1_000_000n, T0: 0n, FRAC: 0n }] });
  assert.deepEqual(land.storesAt(h, 3600, ['Food', 'Wood', 'Stone', 'Ore', 'Horses', 'Gold', 'Science', 'Influence'])[0], { resource: 'Food', value: 262, perHour: 12, cap: 1000 }, '250 + 12 per hour after an hour');
});

test('land states, the ticket card, the refile offer and the escrow (I-47)', () => {
  const c = (over = {}) => acct('Citizen', { TICKET_BELL: 0xffffffff, FLAGS: 1, ...over });
  assert.equal(land.landState(null).stage, 'none');
  assert.equal(land.landState(null).escrowNeeded, 7_152_640n, 'rent of a 1,280-B Holding');
  assert.equal(land.landState(c()).stage, 'joined');
  const t = land.landState(c({ TICKET_BELL: 40, TICKET_NEXT: 1, TICKET_SITES: [{ P: 3, Q: 0, SITE: 2 }, { P: 3, Q: 0, SITE: 5 }], TICKET_ESCROW: 7_152_640n }));
  assert.deepEqual([t.stage, t.ticket.bell, t.ticket.next, t.ticket.sites.length, t.escrowNeeded], ['ticket', 40, 1, 2, 0n]);
  assert.equal(land.landState(c({ FLAGS: 9, HOLDING: [{ P: 3, Q: 0, SITE: 2, GEN: 1 }] })).stage, 'provisional');
  assert.equal(land.landState(c({ FLAGS: 5, HOLDING: [{ P: 3, Q: 0, SITE: 2, GEN: 1 }] })).stage, 'final');
  assert.equal(land.landState(c({ FLAGS: 17 })).stage, 'refugee');
  assert.equal(land.landState(c({ TICKET_ESCROW: 1_000n })).escrowNeeded, 7_151_640n);
  const times = land.ticketTimes(CLOCK, 40);
  assert.equal(times.resultAbout, CLOCK.genesisTs + 600 * 41 + CLOCK.window(40) + CLOCK.margin);
  assert.equal(times.cohortEndsBy, CLOCK.genesisTs + 600 * 64, 'the cohort closes by ticket_bell + 24');
  const last = [{ p: 3, q: 0, site: 2 }];
  assert.deepEqual(land.refileOffer(land.landState(c()), last, { hadHolding: true }), { sites: last, why: 'displaced' });
  assert.equal(land.refileOffer(land.landState(c()), last).why, 'ended');
  assert.equal(land.refileOffer(land.landState(c()), null), null);
  assert.equal(land.refileOffer(t, last), null, 'not while a ticket is open');
});

test('the site picker: free sites of the home wedge in rings ≥ 2, from the overview and then the Province', () => {
  const ov = decodeOverview(readFileSync(new URL('overview-2-40.bin', FIX)));
  const overviews = new Map([[2, ov]]);
  const free = land.freeByWedge(overviews);
  assert.equal(free.length, 6);
  for (const f of [0, 1, 2, 3, 4, 5]) {
    for (const c of land.candidateProvinces(overviews, f)) {
      assert.equal(geo.wedgeOf(c.p, c.q), f);
      assert.ok(c.ring >= 2 && c.free > 0);
    }
  }
  assert.equal(land.candidateProvinces(new Map([[1, { ring: 1, provinces: ov.provinces }]]), 0).length, 0, 'rings 0–1 are reserved (I-30)');
  const pr = acct('Province', { P: 3, Q: -1, SITE_COUNT: 3, SITES: Uint8Array.of(4, 9, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0), SITE_MIRROR: [{ STATE: 0 }, { STATE: 1 }, { STATE: 3 }, { STATE: 0 }] });
  assert.deepEqual(land.freeSitesOf(pr), [{ p: 3, q: -1, site: 0, tile: 4 }, { p: 3, q: -1, site: 2, tile: 17 }], 'a released site is free; none beyond site_count');
  assert.deepEqual(land.freeSitesOf({ ...pr, p: 1, q: 0 }), []);
  assert.equal(land.siteAllowed(geo.wedgeOf(3, -1), { p: 3, q: -1, site: 2 }), true);
  assert.equal(land.siteAllowed((geo.wedgeOf(3, -1) + 1) % 6, { p: 3, q: -1, site: 2 }), false);
});

test('explore targets: passable, unexplored tiles next to the Scout; resident actions wait for finality (I-29)', () => {
  assert.deepEqual(land.tileNeighbours(30).sort((a, b) => a - b), [21, 22, 29, 31, 38, 39]);
  assert.equal(land.tileNeighbours(0).length, 3);
  const pass = [21, 22, 29, 31, 38].reduce((m, i) => m | (1n << BigInt(i)), 0n);
  const pr = { exploredMask: 1n << 22n, passableMask: pass };
  assert.deepEqual(land.exploreTargets(pr, { tile: 30 }).sort((a, b) => a - b), [21, 29, 31, 38]);
  const w = world({ state: 1 });
  assert.deepEqual(land.actionBlocks('Muster', { holding: w.holding, province: w.province, nowBell: 41 }), ['NotFinal']);
  assert.deepEqual(land.actionBlocks('Harvest', { holding: w.holding, province: w.province, nowBell: 41 }), [], 'harvest, build and train run while provisional');
  assert.deepEqual(land.actionBlocks('Garrison', { holding: world().holding, province: w.province, nowBell: 45 }), ['NotResident']);
  const scoutless = { ...land.hostsIn(world().province, world().holding, 41)[0] };
  assert.ok(land.actionBlocks('Explore', { holding: world().holding, province: w.province, nowBell: 41, host: scoutless }).includes('NotScout'));
});

test('lazy finality (W6-D): a provisional holding counts as final once final_ts passed and its cohort is closed, as the program flips it', () => {
  const T = 1_800_000_000;
  const at = (w, now, nowBell = 41) => land.actionBlocks('Muster', { holding: w.holding, province: w.province, nowBell, now });
  // Not yet: final_ts ahead.
  assert.deepEqual(at(world({ state: 1, finalTs: T }), T - 1), ['NotFinal']);
  // final_ts passed, the cohort of bell 30 still open (1 of 2 settled): not yet.
  const open = world({ state: 1, finalTs: T, ticketBell: 30, cohorts: [{ BELL: 30, FILED: 2, SETTLED: 1 }] });
  assert.deepEqual(at(open, T), ['NotFinal']);
  // … unless 24 bells passed since the ticket bell (the cohort expires).
  assert.deepEqual(at(open, T, 54), ['NotResident'], 'bell 54 = 30 + 24: closed (the province now lags, a separate block)');
  // Settled cohort, or no record of that bell at all: the flip is due; Muster is allowed.
  assert.deepEqual(at(world({ state: 1, finalTs: T, ticketBell: 30, cohorts: [{ BELL: 30, FILED: 2, SETTLED: 2 }] }), T), []);
  assert.deepEqual(at(world({ state: 1, finalTs: T, ticketBell: 30, cohorts: [{ BELL: 29, FILED: 3, SETTLED: 0 }] }), T), []);
  // The flip needs the holding's own Province (an action carrying another province does not flip it).
  const w = world({ state: 1, finalTs: T });
  const elsewhere = { ...w.province, p: 3 };
  assert.deepEqual(land.actionBlocks('Muster', { holding: w.holding, province: elsewhere, nowBell: 41, now: T }), ['NotFinal']);
  assert.equal(land.cohortClosed(w.province, 30, 41), true);
  assert.equal(land.finalityDue(world().holding, w.province, T, 41), false, 'a final holding has nothing due');
  assert.equal(land.isFinal(world().holding, null, 0, 0), true);
  // The composer applies the same rule.
  assert.ok(!march.checkMarch(good(w, { province: w.province, now: T })).includes('NotFinal'));
  assert.ok(march.checkMarch(good(w, { province: w.province, now: T - 1 })).includes('NotFinal'));
});

test('refile offer (W6-D): not while the herald has not shown the ticket just filed', () => {
  const last = [{ p: 3, q: 0, site: 2 }];
  const joined = { stage: 'joined' };
  assert.equal(land.refileOffer(joined, last, { seen: false }), null, 'filed, not yet folded: no "your ticket ended"');
  assert.equal(land.refileOffer(joined, last, { seen: true }).why, 'ended');
  assert.equal(land.refileOffer(joined, last).why, 'ended', 'default: seen (records from before W6-D)');
  // integ-W6 review: a ticket that ended between two polls was never seen open;
  // TICKET_SEEN_BELLS after the filing bell it counts as seen anyway.
  assert.equal(land.TICKET_SEEN_BELLS, 3);
  assert.equal(land.refileOffer(joined, last, { seen: false, filedBell: 40, nowBell: 42 }), null, 'filed at 40, now 42: could still be folding');
  assert.equal(land.refileOffer(joined, last, { seen: false, filedBell: 40, nowBell: 43 }).why, 'ended', 'filed at 40, now 43: ended unseen');
  assert.equal(land.refileOffer(joined, last, { seen: false, filedBell: 40, nowBell: 43, hadHolding: true }).why, 'displaced');
  assert.equal(land.refileOffer({ stage: 'ticket' }, last, { seen: false, filedBell: 40, nowBell: 99 }), null, 'still open (lag): no offer whatever the bell');
  assert.equal(land.refileOffer(joined, last, { seen: false, filedBell: null, nowBell: 99 }), null, 'no filing bell recorded (records from before this fix): the sticky flag decides');
  assert.equal(land.refileOffer(joined, last, { seen: false, filedBell: 40, nowBell: null }), null, 'no bell yet');
});

test('ps-fui: season-scoped preferences, defaults for anything damaged, the last ticket for a refile', () => {
  const s = memoryStorage();
  const key = fui.uiKey({ cluster: 'localnet', programId: 'P', seasonId: '1' });
  assert.equal(key, 'ps-fui:localnet:P:1');
  assert.deepEqual(fui.loadUi(s, key), { ...fui.DEFAULTS, dismissed: [] });
  fui.saveUi(s, key, { fog: false, tab: 'marches', lastTicket: [{ p: 3, q: 0, site: 2 }] });
  assert.deepEqual(fui.loadUi(s, key), { v: 1, fog: false, lod: 'world', tab: 'marches', dismissed: [], lastTicket: [{ p: 3, q: 0, site: 2 }], ticketSeen: true, lastTicketBell: null, autoPan: false, battleFx: 'normal', guide: 'all' });
  s.set(key, JSON.stringify({ v: 1, fog: 'no', tab: 'evil', lod: 'x', lastTicket: [{ p: 1, q: 0, site: 99 }], dismissed: [1, 'a'], ticketSeen: 'x', lastTicketBell: -3 }));
  assert.deepEqual(fui.loadUi(s, key), { v: 1, fog: true, lod: 'world', tab: 'map', dismissed: ['a'], lastTicket: null, ticketSeen: true, lastTicketBell: null, autoPan: false, battleFx: 'normal', guide: 'all' });
  // W6-D: a ticket filed but not yet shown by the herald is not "ended"; the
  // filing bell is kept with it (integ-W6 review).
  fui.saveUi(s, key, { lastTicket: [{ p: 3, q: 0, site: 2 }], ticketSeen: false, lastTicketBell: 40 });
  assert.equal(fui.loadUi(s, key).ticketSeen, false);
  assert.equal(fui.loadUi(s, key).lastTicketBell, 40);
  assert.equal(fui.saveUi(s, key, { lastTicketBell: 7.5 }).lastTicketBell, null, 'a damaged bell reads as none');
  s.set(key, '{not json');
  assert.equal(fui.loadUi(s, key).fog, true);
});
