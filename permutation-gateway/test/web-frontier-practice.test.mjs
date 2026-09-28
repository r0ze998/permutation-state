// The clash report, "verify in this browser" and practice mode (W4-E;
// contract §5.11, §9.5, §13.6; web design §4.3, §7.9, §7.12):
//  - the `resolve_clash` codec (ClashArgs / ClashOut, borsh) against the
//    recorded native call of frontier-wasm/vectors/wasm-vectors.json (the
//    permutation-rules kernel's own answer), byte for byte, and the page's
//    re-hash of the outcome against the kernel's digest;
//  - the real frontier.wasm in node: the recorded args give the recorded
//    digest (the native vector), the ruleset hash is the pinned one;
//  - every practice scenario resolves in the kernel for every stance; the
//    retreat, quota and rout scenarios teach what they say; the rule bot is
//    deterministic, counter-picks, and its triangle is the kernel's;
//  - the §5.11 builder (Province before the resolve + ClashInputs → kernel
//    input) field by field; the anchor seed rule; verify end to end over a
//    fake herald: match, and mismatch on a changed digest, fate or seed,
//    and "could not check" without the Province bytes;
//  - "what if" changes only the viewer's arrivals; markup in JA and EN; the
//    app's routing of the report and practice panels.
// The real module is required once the wasm32 target is installed (owner
// approval of O-M1-12 item 1, 2026-09-28): a missing frontier.wasm then
// fails (run scripts/build-wasm.sh); without the target it is PENDING-OWNER.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import * as W from '../../permutation-server/web/frontier/wasm.mjs';
import * as rep from '../../permutation-server/web/frontier/screens/report.mjs';
import * as pr from '../../permutation-server/web/frontier/screens/practice.mjs';
import { decode } from '../../permutation-server/web/frontier/fcodec.mjs';
import { hostId } from '../../permutation-server/web/frontier/faddr.mjs';
import { regionOf } from '../../permutation-server/web/frontier/fgeo.mjs';
import { seasonClock, bellEnd, seedRound } from '../../permutation-server/web/frontier/clock.mjs';
import { RULESET_HASH } from '../../permutation-server/web/frontier/abi.mjs';
import { FS } from '../../permutation-server/web/frontier/fstate.mjs';
import { panelMarkup, mineOf, myReports, W4_ACTIONS, w4Bind } from '../../permutation-server/web/frontier/app.mjs';
import { encodeAccount } from './fixtures/frontier/make-fixtures.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

afterEach(() => setLang('ja'));
const REPO = new URL('../../', import.meta.url);
const V = JSON.parse(readFileSync(new URL('frontier-wasm/vectors/wasm-vectors.json', REPO), 'utf8'));
const WASM = new URL('permutation-server/web/frontier/wasm/frontier.wasm', REPO);
const hex = h => Uint8Array.from(Buffer.from(h, 'hex'));
const toHex = b => Buffer.from(b).toString('hex');
const JP = /[぀-ヿ㐀-鿿]/;
const js = v => JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? `${x}n` : x));
const text = markup => String(markup).replace(/<[^>]*>/g, ' ');
const SEASON_JSON = JSON.parse(readFileSync(new URL('fixtures/frontier/season.json', import.meta.url), 'utf8'));
const SEASON = decode('Season', Buffer.from(SEASON_JSON.bytes_b64, 'base64'));
const CLOCK = seasonClock(SEASON);

const wasmTargetInstalled = () => {
  const r = spawnSync('rustup', ['target', 'list', '--installed', '--toolchain', '1.95.0'], { encoding: 'utf8' });
  return r.status === 0 && /wasm32-unknown-unknown/.test(r.stdout);
};
let kernelPromise = null;
/** The real frontier.wasm through the page's own loader (hash, ABI and ruleset checks), or a skip / failure. */
async function realKernel(t) {
  if (!existsSync(WASM)) {
    assert.ok(!wasmTargetInstalled(), 'frontier.wasm is missing although the wasm32 target is installed (O-M1-12 item 1 approved): run scripts/build-wasm.sh');
    t.skip('PENDING-OWNER: frontier.wasm is not built (no wasm32-unknown-unknown target)');
    return null;
  }
  const bytes = readFileSync(WASM);
  const sha = readFileSync(new URL('frontier.wasm.sha256', WASM), 'utf8').trim().split(/\s+/)[0];
  kernelPromise ??= W.loadKernel({ url: 'x/frontier.wasm', sha256Hex: sha, fetch: async () => ({ ok: true, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.length) }) });
  return kernelPromise;
}

// ------------------------------------------------------------------ the codec
const recorded = V.calls.find(c => c.export === 'resolve_clash' && c.status === W.OK);

test('resolve_clash codec: the recorded native call decodes and re-encodes byte for byte; the page re-hashes the outcome to the kernel digest', () => {
  assert.ok(recorded, 'wasm-vectors.json records a resolve_clash call');
  const args = rep.decodeClashArgs(hex(recorded.input));
  assert.equal(toHex(rep.encodeClashArgs(args)), recorded.input);
  assert.deepEqual(args.province, { p: 2, q: 0 });
  assert.equal(args.bell, 40);
  assert.equal(args.residents.length + args.arrivals.length + args.garrisons.length, 4);
  assert.equal(args.garrisons[0].posture, 3, 'a garrison in Brace');
  assert.equal(args.arrivals[0].posture, 1, 'an arrival in Assault');
  const out = rep.decodeClashOut(hex(recorded.output));
  assert.equal(out.outcome.bell, 40);
  assert.equal(rep.outcomeDigest(out.outcome), out.digest, 'sha256("frontier/clash-outcome" ‖ borsh(outcome))');
  assert.ok(out.outcome.fighters.every(f => rep.FATE_KINDS.includes(f.fate.kind)));
  // A flipped byte in the outcome no longer hashes to the digest.
  const bad = hex(recorded.output); bad[20] ^= 1;
  const b = rep.decodeClashOut(bad);
  assert.notEqual(rep.outcomeDigest(b.outcome), b.digest);
  assert.throws(() => rep.decodeClashArgs(hex(`${recorded.input}00`)), /trailing/);
});

test('the real frontier.wasm: the recorded native args give the recorded digest; the ruleset is the pinned one', async t => {
  const k = await realKernel(t);
  if (!k) return;
  assert.equal(k.rulesetHash(), RULESET_HASH);
  const want = rep.decodeClashOut(hex(recorded.output));
  const r = rep.resolveArgs(k, rep.decodeClashArgs(hex(recorded.input)));
  assert.ok(r.ok, js(r));
  assert.equal(r.digest, want.digest, 'the browser kernel = the native permutation-rules vector');
  assert.deepEqual(r.outcome, want.outcome);
  // A refusal comes back as a code, not a throw (faction 9 is invalid).
  const bad = rep.decodeClashArgs(hex(recorded.input));
  bad.arrivals[0].faction = 9;
  const x = rep.resolveArgs(k, bad);
  assert.equal(x.ok, false);
  assert.equal(x.why, 'Refused');
  assert.equal(x.kernelCode, 5, 'ClashError::BadFaction');
});

// ------------------------------------------------------------------ practice
const seedOf = n => new Uint8Array(32).fill(n);

test('practice: every scenario resolves in the kernel for every stance; results are deterministic for a seed', async t => {
  const k = await realKernel(t);
  if (!k) return;
  for (const s of pr.SCENARIO_IDS) {
    for (let stance = 0; stance < 4; stance++) {
      const st = { ...pr.practiceState({ scenario: s, faction: 2 }), stance };
      const a = pr.runScenario(k, st, seedOf(stance + 1));
      assert.ok(a.ok, `${s}/${stance}: ${js(a)}`);
      assert.equal(rep.outcomeDigest(a.outcome), a.digest);
      assert.ok([...a.args.arrivals, ...a.args.residents, ...a.args.garrisons].some(f => f.faction === 2), `${s}: the viewer's faction plays`);
      const b = pr.runScenario(k, st, seedOf(stance + 1));
      assert.equal(b.digest, a.digest, `${s}: same seed, same result`);
    }
  }
  assert.equal(pr.PRACTICE_TERRAIN.terrain.length, 61);
  assert.ok(!pr.PRACTICE_TERRAIN.sites.includes(pr.CAMP_TILE), 'the camp is on a non-site tile (I-56)');
});

test('practice: the retreat ratio turns back without loss; never retreat fights; the quota keeps the four largest; the unrevealed arrival is routed at half', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const you = r => r.outcome.fighters.find(f => f.arrival && r.args.arrivals.find(a => a.id === f.id).faction === 0);
  const ret = pr.runScenario(k, { ...pr.practiceState({ scenario: 'retreat' }), retreat: 'x1' }, seedOf(9));
  assert.equal(you(ret).fate.kind, 'Retreated');
  assert.equal(you(ret).troops, 500_000, 'no loss');
  const never = pr.runScenario(k, { ...pr.practiceState({ scenario: 'retreat' }), retreat: 'never' }, seedOf(9));
  assert.notEqual(you(never).fate.kind, 'Retreated');
  assert.ok(you(never).troops < 500_000, 'it fought');
  const q = pr.runScenario(k, pr.practiceState({ scenario: 'quota' }), seedOf(3));
  assert.equal(q.args.arrivals.length, 4);
  assert.deepEqual(q.args.arrivals.map(f => f.troops / 1000), [520, 480, 450, 400]);
  assert.deepEqual(q.displaced.map(f => f.troops / 1000), [350, 300]);
  const r = pr.runScenario(k, pr.practiceState({ scenario: 'rout' }), seedOf(4));
  assert.equal(r.routed.length, 1);
  assert.equal(r.routed[0].after, r.routed[0].troops / 2);
  assert.ok(!r.args.arrivals.some(f => f.id === r.routed[0].id), 'an unrevealed arrival is not in the clash');
  // ROUT_LOSS_BPS is the kernel's.
  const hostRs = readFileSync(new URL('permutation-rules/src/frontier/host.rs', REPO), 'utf8');
  assert.match(hostRs, new RegExp(`pub const ROUT_LOSS_BPS: Bps = ${pr.ROUT_LOSS_BPS.toLocaleString('en-US').replace(/,/g, '_')};`));
});

test('the rule bot: deterministic, never Hold, counter-picks the viewer\'s usual stance about 70% of the time', () => {
  for (let s = 0; s < 4; s++) assert.equal(pr.counterTo(s) === null, s === 0);
  assert.equal(pr.counterTo(2), 1, 'Assault beats Flank');
  assert.equal(pr.counterTo(3), 2, 'Flank beats Brace');
  assert.equal(pr.counterTo(1), 3, 'Brace beats Assault');
  assert.equal(pr.botStance(seedOf(1), [2, 2, 1]), pr.botStance(seedOf(1), [2, 2, 1]));
  let counter = 0;
  const n = 400;
  for (let i = 0; i < n; i++) {
    const seed = Uint8Array.from({ length: 32 }, (_, j) => (i * 7 + j * 13) & 0xff);
    const s = pr.botStance(seed, [2, 2, 1], i % 3);
    assert.ok(s >= 1 && s <= 3);
    if (s === 1) counter++;
  }
  assert.ok(counter / n > 0.6 && counter / n < 0.9, `counter-picks ${counter}/${n}`);
  const noHistory = new Set(Array.from({ length: 60 }, (_, i) => pr.botStance(seedOf(i), [])));
  assert.deepEqual([...noHistory].sort(), [1, 2, 3], 'no history: a mix of the three stances');
});

test('the bot\'s triangle is the kernel\'s: equal hosts, the stance that beats the other loses fewer troops', async t => {
  const k = await realKernel(t);
  if (!k) return;
  for (const [a, b] of [[1, 2], [2, 3], [3, 1]]) {
    let wins = 0;
    for (let i = 0; i < 6; i++) {
      const { args } = pr.scenarioArgs({ ...pr.practiceState({ scenario: 'stance' }), stance: a }, seedOf(20 + i));
      args.arrivals[1].posture = b;
      const r = rep.resolveArgs(k, args);
      const lost = f => 500_000 - r.outcome.fighters.find(x => x.id === f.id).troops;
      if (lost(args.arrivals[0]) < lost(args.arrivals[1])) wins++;
    }
    assert.equal(wins, 6, `${a} beats ${b} (pr.BEATS[${a}] = ${pr.BEATS[a]})`);
    assert.equal(pr.BEATS[a], b);
  }
});

// ------------------------------------------------------------------ the §5.11 builder
const P = 2, Q = 0, BELL = 40;
const NO_BELL = 0xffffffff;
const hid = (site, gen, seq) => hostId({ p: P, q: Q, site, gen, seq });

function provinceBefore({ camp = { tile: 33, state: 1, troops: 250, nextCheckDay: 1, gen: 3 }, extra = [] } = {}) {
  const mirror = Array.from({ length: 12 }, (_, s) => ({ state: 0, faction: 6, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallItem0Bell: NO_BELL, wallItem1Bell: NO_BELL }));
  mirror[3] = { state: 1, faction: 0, gen: 2, garrison: 300_000, pend0Bell: BELL, pend0Delta: 5_000, pend1Bell: NO_BELL, wallsCommitted: 0, wallItem0Bell: BELL, wallItem0Delta: 10, wallItem1Bell: NO_BELL };
  mirror[5] = { state: 1, faction: 1, gen: 0, garrison: 100_000, pend0Bell: NO_BELL, pend1Bell: NO_BELL, wallsCommitted: 0, wallItem0Bell: BELL + 1, wallItem0Delta: 10, wallItem1Bell: NO_BELL };
  const entries = [
    { id: hid(3, 2, 1), faction: 0, unit: 0, tile: 14, state: 1, troops: 800_000, staminaValue: 90, staminaBell: BELL - 5, dealtBps: 10_000, fromBell: BELL - 3 },
    { id: hid(3, 2, 2), faction: 0, unit: 6, tile: 15, state: 1, troops: 100_000, staminaValue: 118, staminaBell: BELL - 5, dealtBps: 10_000, fromBell: BELL + 1 },
    { id: hid(3, 2, 3), faction: 0, unit: 1, tile: 14, state: 2, troops: 200_000, staminaValue: 120, dealtBps: 10_000 },
    { id: hid(5, 0, 1), faction: 1, unit: 2, tile: 20, state: 3, troops: 300_000, staminaValue: 50, dealtBps: 10_500 },
    { id: hid(3, 2, 4), faction: 0, unit: 0, tile: 14, state: 1, troops: 150_000, staminaValue: 40, staminaBell: BELL, dealtBps: 10_000, fromBell: BELL, pendBell: BELL, pendOp: 1 },
    ...extra,
  ];
  return encodeAccount('Province', {
    p: P, q: Q, ring: 2, wedge: 0, region: regionOf(P, Q), resolvedNext: BELL, openedBell: 3, relations: 0,
    terrain: Uint8Array.from({ length: 61 }, (_, i) => (i === 7 ? 2 : 0)), resource: Uint8Array.from({ length: 61 }, (_, i) => (i === 9 ? 2 : 0)),
    sites: [2, 5, 9, 14, 20, 24, 30, 36, 41, 47, 52, 58], siteCount: 12, siteMirror: mirror, entries, camp,
  });
}
function clashInputs(arrivals, { flags = 2 } = {}) {
  return encodeAccount('ClashInputs', { p: P, q: Q, bell: BELL, arrivalsMask: 0xffffff, flags, nPresent: arrivals.filter(a => a.present === 1).length, arrivals });
}
const ARRIVALS = [
  { hostId: hid(7, 0, 1), citizenTag: 1n, depMass: 900, troops: 900_000, stamina: 60, retreat: 0, dealt: 10_000, faction: 1, unit: 0, tile: 14, stance: 1, present: 1 },
  { hostId: hid(8, 0, 1), citizenTag: 2n, depMass: 400, troops: 400_000, stamina: 70, retreat: 15_000, dealt: 11_000, faction: 2, unit: 5, tile: 14, stance: 3, present: 1 },
  { hostId: hid(9, 0, 1), citizenTag: 3n, depMass: 300, troops: 300_000, stamina: 70, retreat: 0, dealt: 10_000, faction: 2, unit: 0, tile: 20, stance: 2, present: 0 },
];

test('the §5.11 builder: residents, garrisons, walls, camp, arrivals, relations and the storage room from the account bytes', () => {
  const pv = decode('Province', provinceBefore());
  const ci = decode('ClashInputs', clashInputs(ARRIVALS));
  const seed = seedOf(5);
  const b = rep.buildClashArgs({ province: pv, inputs: ci, bell: BELL, seed });
  assert.ok(b.ok, js(b));
  assert.equal(b.certain, true);
  const a = b.args;
  // Residents: state 1 with from_bell ≤ bell only, stamina refilled +1 per bell to the cap, Hold, no retreat.
  assert.deepEqual(a.residents.map(f => [f.id, f.troops, f.stamina, f.posture, f.retreatBps]), [
    [hid(3, 2, 1), 800_000, 95, 0, null],
    [hid(3, 2, 4), 150_000, 40, 0, null],
  ]);
  // Garrisons: holdings only, ids with seq 0, walls effective at the bell; the camp as NEUTRAL, id u64::MAX − gen.
  assert.deepEqual(a.garrisons.map(g => [g.id, g.faction, g.tile, g.troops, g.walls]), [
    [hid(3, 2, 0), 0, 14, 300_000, true],
    [hid(5, 0, 0), 1, 24, 100_000, false],
    // A camp stores whole troops: × 1,000 (integ-W4 review, W4-E).
    [0xffffffffffffffffn - 3n, 6, 33, 250_000, false],
  ]);
  // Arrivals: present records only; stance from the reveal; retreat 0 = never.
  assert.deepEqual(a.arrivals.map(f => [f.id, f.faction, f.unit, f.troops, f.stamina, f.posture, f.retreatBps, f.dealtBps]), [
    [hid(7, 0, 1), 1, 0, 900_000, 60, 1, null, 10_000],
    [hid(8, 0, 1), 2, 5, 400_000, 70, 3, 15_000, 11_000],
  ]);
  // Room: one pending muster of faction 0; entries in states 1–3 = 5 → 51 free.
  assert.deepEqual(a.occupancy, { pending: [1, 0, 0, 0, 0, 0, 0, 0], storageFree: 51 });
  assert.equal(a.terrain.terrain[7], 2);
  assert.equal(a.terrain.resource[9], 1, 'resource byte 1 + TileResource');
  assert.equal(a.terrain.resource[0], null);
  assert.equal(a.relations, 0n);
  // An encoded input round-trips through the codec.
  assert.deepEqual(rep.decodeClashArgs(rep.encodeClashArgs(a)), { ...a, seed: Uint8Array.from(seed) });
});

test('the builder refuses what the program cannot have built, and flags a camp respawn it does not model', () => {
  const ci = decode('ClashInputs', clashInputs(ARRIVALS));
  // A host change from an earlier bell never settled.
  const late = decode('Province', provinceBefore({ extra: [{ id: hid(3, 2, 9), faction: 0, unit: 0, tile: 14, state: 1, troops: 1_000, staminaValue: 1, dealtBps: 10_000, fromBell: 1, pendBell: BELL - 1, pendOp: 2 }] }));
  assert.equal(rep.buildClashArgs({ province: late, inputs: ci, bell: BELL, seed: seedOf(1) }).why, 'Unsettled');
  // Another province-bell's inputs.
  const other = decode('ClashInputs', encodeAccount('ClashInputs', { p: P, q: Q, bell: BELL + 1, flags: 2 }));
  assert.equal(rep.buildClashArgs({ province: decode('Province', provinceBefore()), inputs: other, bell: BELL, seed: seedOf(1) }).why, 'WrongKey');
  // No camp now, and the day's check is due: the program respawns first; the page says it is not certain.
  const noCamp = decode('Province', provinceBefore({ camp: { tile: 33, state: 0, troops: 0, nextCheckDay: 0 + 1, gen: 3 } }));
  const due = rep.buildClashArgs({ province: noCamp, inputs: ci, bell: 200, seed: seedOf(1) });
  assert.equal(due.why, 'WrongKey');
  const ci200 = decode('ClashInputs', encodeAccount('ClashInputs', { p: P, q: Q, bell: 200, flags: 2 }));
  const r = rep.buildClashArgs({ province: noCamp, inputs: ci200, bell: 200, seed: seedOf(1) });
  assert.equal(r.ok, false, 'the garrison change of bell 40 is unsettled by bell 200');
  const clean = decode('Province', encodeAccount('Province', { p: P, q: Q, resolvedNext: 200, siteCount: 0, camp: { tile: 33, state: 0, nextCheckDay: 1, gen: 3 } }));
  const r2 = rep.buildClashArgs({ province: clean, inputs: ci200, bell: 200, seed: seedOf(1) });
  assert.ok(r2.ok);
  assert.equal(r2.certain, false, 'day 1 ≤ day(200): a respawn may be due');
  // A present camp is replaced by a spawn of the day's check too (integ-W4 review): not certain either.
  const present = decode('Province', encodeAccount('Province', { p: P, q: Q, resolvedNext: 200, siteCount: 0, camp: { tile: 33, state: 1, troops: 90, nextCheckDay: 1, gen: 3 } }));
  assert.equal(rep.buildClashArgs({ province: present, inputs: ci200, bell: 200, seed: seedOf(1) }).certain, false);
  assert.equal(rep.buildClashArgs({ province: present, inputs: ci200, bell: 143, seed: seedOf(1) }).why, 'WrongKey');
  const ci143 = decode('ClashInputs', encodeAccount('ClashInputs', { p: P, q: Q, bell: 143, flags: 2 }));
  assert.equal(rep.buildClashArgs({ province: present, inputs: ci143, bell: 143, seed: seedOf(1) }).certain, true, 'day 0 < next_check_day 1');
});

// Integ-W4 review (W4-E): the page's builder = the native one (fclient
// `clash_model::build`, the program's model as the herald and the verifier
// read it) on account bytes: frontier-vectors.json `clash_model`, written by
// fclient's `writes_frontier_vectors`.
const CM = JSON.parse(readFileSync(new URL('frontier-vectors.json', import.meta.url), 'utf8')).clash_model;
const postureOf = s => rep.POSTURES.indexOf(/^Stance\((\w+)\)$/.exec(s)?.[1] ?? s);

test('the §5.11 builder = the native clash_model on the same account bytes (camp × 1,000 capped, the 12-garrison limit, dealt 0 = 1.0, id order, certain)', async t => {
  assert.ok(CM.cases.length >= 4);
  const k = await realKernel(t);
  for (const c of CM.cases) {
    const pv = decode('Province', Buffer.from(c.province_b64, 'base64'));
    const ci = decode('ClashInputs', Buffer.from(c.inputs_b64, 'base64'));
    const b = rep.buildClashArgs({ province: pv, inputs: ci, bell: c.bell, seed: hex(c.seed) });
    assert.ok(b.ok, `${c.name}: ${js(b)}`);
    assert.equal(b.certain, c.certain, `${c.name}: certain`);
    if (!c.certain) continue; // the program's camp check ran: the page does not model it (and says so)
    const a = b.args;
    assert.deepEqual(a.residents.map(f => [String(f.id), f.troops, f.stamina, f.dealtBps, f.posture]),
      c.residents.map(([id, tr, st, d, po]) => [id, tr, st, d, postureOf(po)]), `${c.name}: residents`);
    assert.deepEqual(a.garrisons.map(g => [String(g.id), g.faction, g.tile, g.troops, g.walls]), c.garrisons, `${c.name}: garrisons`);
    assert.deepEqual(a.arrivals.map(f => [String(f.id), f.troops, f.stamina, f.dealtBps, f.retreatBps, f.posture]),
      c.arrivals.map(([id, tr, st, d, re, po]) => [id, tr, st, d, re, postureOf(po)]), `${c.name}: arrivals`);
    assert.deepEqual(a.occupancy, { pending: c.occupancy.pending, storageFree: c.occupancy.storage_free }, `${c.name}: occupancy`);
    if (!k) continue;
    const r = rep.resolveArgs(k, a);
    assert.ok(r.ok, `${c.name}: ${js(r)}`);
    assert.equal(r.digest, c.digest, `${c.name}: the browser's outcome digest = the native one`);
    assert.deepEqual(r.outcome.fighters.map(f => [String(f.id), f.arrival, f.troops]), c.fighters, `${c.name}: fighters`);
  }
});

// ------------------------------------------------------------------ the seed of THE anchor
const A = bellEnd(CLOCK.genesisTs, BELL) + 2;
const S = seedRound(CLOCK.drand, A + CLOCK.window(BELL), CLOCK.margin);
const SEED = '5e'.repeat(32);
const anchorBytes = () => encodeAccount('BellAnchor', { bell: BELL, region: regionOf(P, Q), net: 2, round: 1, a: A, slot: 1 });
const bellRecord = (caches = [{ nonce: 0, round: S, seed: SEED, A }], extra = {}) => ({ ok: true, record: { v: 1, bell: BELL, region: regionOf(P, Q), S, caches, archived: false, tombstoned: false, ...extra }, anchor: decode('BellAnchor', anchorBytes()) });

test('the anchor seed: only a cache of round S = seed_round(A + W + Δ) with THE anchor\'s A; the archive after archiving', () => {
  const ok = rep.anchorSeed(CLOCK, BELL, bellRecord());
  assert.deepEqual({ ok: ok.ok, a: ok.a, s: ok.s, seed: ok.seed, source: ok.source }, { ok: true, a: A, s: S, seed: SEED, source: { cache: 0 } });
  assert.equal(rep.anchorSeed(CLOCK, BELL, bellRecord([{ nonce: 1, round: S + 1, seed: '11'.repeat(32), A }])).why, 'SeedNotReady', 'another round is not the seed');
  assert.equal(rep.anchorSeed(CLOCK, BELL, bellRecord([{ nonce: 1, round: S, seed: '11'.repeat(32), A: A + 3 }])).why, 'SeedNotReady', 'another anchor\'s cache');
  // A cache that does not name its A is not THE anchor's (integ-W4 review, W4-E).
  assert.equal(rep.anchorSeed(CLOCK, BELL, bellRecord([{ nonce: 1, round: S, seed: '11'.repeat(32) }])).why, 'SeedNotReady', 'a cache without A');
  const arch = rep.anchorSeed(CLOCK, BELL, { ok: true, anchor: null, record: { archived: true, archive: { aOff: 2, seed: 'AB'.repeat(32) } } });
  assert.deepEqual([arch.ok, arch.a, arch.seed, arch.source], [true, A, 'ab'.repeat(32), { archive: true }]);
  assert.equal(rep.anchorSeed(CLOCK, BELL, { ok: true, anchor: null, record: { caches: [] } }).why, 'NoAnchor');
});

// ------------------------------------------------------------------ verify end to end
/** A resolved province-bell: the "program" resolves with the kernel and writes the fate table and the Province digest. */
async function resolvedWorld(k) {
  const before = provinceBefore();
  const pv = decode('Province', before);
  const ci0 = decode('ClashInputs', clashInputs(ARRIVALS));
  const seed = hex(SEED);
  const built = rep.buildClashArgs({ province: pv, inputs: ci0, bell: BELL, seed });
  const res = rep.resolveArgs(k, built.args);
  assert.ok(res.ok);
  const fated = ARRIVALS.map(a => {
    const f = res.outcome.fighters.find(x => x.arrival && x.id === a.hostId);
    return f ? { ...a, fate: rep.FATE_KINDS.indexOf(f.fate.kind) + 1, troopsAfter: f.troops } : a;
  });
  const inputs = clashInputs(fated);
  const after = (digest = res.digest) => encodeAccount('Province', { p: P, q: Q, resolvedNext: BELL + 1, lastDigest: digest, resolveSummary: { bell: BELL } });
  return { before, inputs, after, res, fated };
}
function fakeHerald(w, { post = w.after() } = {}) {
  const calls = [];
  return {
    calls,
    async bellRegion(b, r) { calls.push(['bell', b, r]); return bellRecord(); },
    async province(p, q, b) { calls.push(['province', p, q, b]); return post ? { ok: true, province: decode('Province', post) } : { ok: false, code: 'NotFound' }; },
  };
}
const clashOf = (w, extra = {}) => ({
  ok: true,
  report: { v: 1, bell: BELL, province: [P, Q], inputs_b64: Buffer.from(w.inputs).toString('base64'), seed: SEED, anchor: { key: `an:${BELL},${regionOf(P, Q)}`, A },
    cache: { key: `sd:${BELL},${regionOf(P, Q)},0` }, outcomeDigest: w.res.digest, inputDigest: '00'.repeat(32), province_before_b64: Buffer.from(w.before).toString('base64'), ...extra },
  inputs: decode('ClashInputs', w.inputs),
});

test('verify in this browser: every step passes on a faithful record; the Province\'s own digest is the reference', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const w = await resolvedWorld(k);
  const h = fakeHerald(w);
  const v = await rep.verifyClash({ p: P, q: Q, bell: BELL, clash: clashOf(w), herald: h, kernel: async () => k, clock: CLOCK, season: SEASON });
  assert.equal(v.result, 'match', js(v.steps));
  assert.deepEqual(Object.values(v.steps).map(s => s.ok), [true, true, true, true, true, true]);
  assert.equal(v.steps.digest.detail, 'province');
  assert.equal(v.builder, 'page', 'resolve_from_inputs answers UNAVAILABLE in this build: the §5.11 transcription');
  assert.equal(v.digest, w.res.digest);
  assert.deepEqual(h.calls, [['bell', BELL, regionOf(P, Q)], ['province', P, Q, BELL]]);
  // Without the Province envelope of the bell, the CLASH log's digest (as the herald reports it) is the reference.
  const v2 = await rep.verifyClash({ p: P, q: Q, bell: BELL, clash: clashOf(w), herald: fakeHerald(w, { post: null }), kernel: k, clock: CLOCK, season: SEASON });
  assert.equal(v2.result, 'match');
  assert.equal(v2.steps.digest.detail, 'log');
});

test('verify: a changed digest, fate table or seed is a mismatch; a missing record is "could not check", never green', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const w = await resolvedWorld(k);
  const run = (clash, herald = fakeHerald(w), season = SEASON) => rep.verifyClash({ p: P, q: Q, bell: BELL, clash, herald, kernel: k, clock: CLOCK, season });
  const d = await run(clashOf(w), fakeHerald(w, { post: w.after('77'.repeat(32)) }));
  assert.equal(d.result, 'mismatch');
  assert.equal(d.steps.digest.why, 'DigestMismatch');
  // The program's fate table says another troop count for the first arrival.
  const forged = clashInputs(w.fated.map((a, i) => (i === 0 ? { ...a, troopsAfter: a.troopsAfter + 1000 } : a)));
  const f = await run({ ...clashOf(w), inputs: decode('ClashInputs', forged), report: { ...clashOf(w).report, inputs_b64: Buffer.from(forged).toString('base64') } });
  assert.equal(f.result, 'mismatch');
  assert.equal(f.steps.fates.why, 'FateMismatch');
  assert.deepEqual(f.steps.fates.detail, [String(ARRIVALS[0].hostId)]);
  // A report whose seed is not THE anchor's.
  const s = await run(clashOf(w, { seed: '11'.repeat(32) }));
  assert.equal(s.result, 'mismatch');
  assert.equal(s.steps.seed.why, 'SeedMismatch');
  // Without the Province bytes before the resolve (the herald's report today): could not check.
  const n = await run(clashOf(w, { province_before_b64: undefined }));
  assert.equal(n.result, 'unchecked');
  assert.equal(n.steps.recompute.why, 'NoProvinceBefore');
  assert.equal(n.steps.seed.ok, true, 'the seed is still checked');
  // Another ruleset: nothing recomputed.
  const r = await run(clashOf(w), fakeHerald(w), { ...SEASON, rulesetHash: new Uint8Array(32) });
  assert.equal(r.steps.kernel.why, 'RulesetMismatch');
  assert.notEqual(r.result, 'match');
  // No kernel at all.
  const nk = await rep.verifyClash({ p: P, q: Q, bell: BELL, clash: clashOf(w), herald: fakeHerald(w), kernel: async () => { const e = new Error('x'); e.code = 'NoWasm'; throw e; }, clock: CLOCK, season: SEASON });
  assert.equal(nk.result, 'unchecked');
  assert.equal(nk.steps.kernel.why, 'NoWasm');
  // Unresolved inputs.
  const u = clashInputs(ARRIVALS, { flags: 0 });
  const ur = await run({ ...clashOf(w), inputs: decode('ClashInputs', u) });
  assert.equal(ur.steps.inputs.why, 'NotResolved');
  assert.equal(ur.result, 'unchecked');
});

test('what if: only the viewer\'s arrivals change; the same seed and inputs otherwise', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const w = await resolvedWorld(k);
  const built = rep.buildClashArgs({ province: decode('Province', w.before), inputs: decode('ClashInputs', w.inputs), bell: BELL, seed: hex(SEED) });
  const mine = id => id === ARRIVALS[1].hostId;
  const same = pr.whatIf(k, { args: built.args, outcome: w.res.outcome }, mine, { stance: 3, retreat: 15_000 });
  assert.ok(same.ok);
  assert.deepEqual(same.changed, [], 'its own orders again: nothing changes');
  const alt = pr.whatIf(k, { args: built.args, outcome: w.res.outcome }, mine, { stance: 3, retreat: 1 });
  assert.ok(alt.ok);
  assert.equal(alt.alt.args.arrivals[0].retreatBps, null, 'another faction\'s arrival is untouched');
  assert.equal(alt.alt.args.arrivals[1].retreatBps, 1);
  assert.equal(alt.alt.outcome.fighters.find(f => f.id === ARRIVALS[1].hostId).fate.kind, 'Retreated', 'a retreat ratio of 0.0001 always turns back');
  assert.ok(alt.changed.includes(String(ARRIVALS[1].hostId)));
});

// ------------------------------------------------------------------ markup and routing
test('the report panel in both languages: input rows before verifying, the verdict after', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const w = await resolvedWorld(k);
  const store = { report: { p: P, q: Q, bell: BELL, clash: clashOf(w) } };
  const before = String(rep.render(store));
  assert.match(before, /data-act="report-verify"/);
  assert.match(before, /data-act="report-whatif" disabled/);
  assert.match(before, /data-act="report-close"/);
  assert.equal((before.match(/<tr/g) ?? []).length, 1 + 2, 'header + the two present arrivals');
  store.report.verify = await rep.verifyClash({ p: P, q: Q, bell: BELL, clash: clashOf(w), herald: fakeHerald(w), kernel: k, clock: CLOCK, season: SEASON });
  const after = String(rep.render(store, id => id === ARRIVALS[1].hostId || String(id) === String(ARRIVALS[1].hostId)));
  assert.match(after, /notice ok/);
  assert.match(after, /data-act="report-whatif" >|data-act="report-whatif">/);
  assert.equal((after.match(/<tr/g) ?? []).length, 1 + 2 + 2 + 3, 'header, residents, arrivals, garrisons and the camp');
  assert.match(after, /（あなた）/);
  setLang('en');
  const en = text(rep.render(store));
  assert.doesNotMatch(en, JP, en.match(JP)?.index !== undefined ? en.slice(Math.max(0, en.match(JP).index - 40), en.match(JP).index + 40) : '');
  assert.match(en, /Match: the outcome recomputed in this browser/);
  assert.match(en, /Verify in this browser/);
  store.report.verify = { ...store.report.verify, result: 'mismatch' };
  assert.match(String(rep.render(store)), /notice error/);
  assert.equal(rep.render({ report: null }), '');
  assert.match(text(rep.render({ report: { p: 1, q: 2, bell: 3, error: 'NotFound' } })), /NotFound/);
});

test('the practice panel in both languages: seven scenarios, controls per scenario, the result table, the what-if', async t => {
  const k = await realKernel(t);
  if (!k) return;
  const st = pr.practiceState({ scenario: 'retreat' });
  let m = String(pr.render(st));
  assert.equal((m.match(/data-act="practice-scenario"/g) ?? []).length, 7);
  assert.match(m, /data-bind="pr-retreat"/);
  assert.doesNotMatch(m, /data-bind="pr-stance"/, 'the retreat scenario has no stance control');
  assert.match(m, /data-act="practice-reroll" disabled/);
  st.result = pr.runScenario(k, st, seedOf(2));
  m = String(pr.render(st));
  assert.match(m, /<table class="stores">/);
  assert.match(m, /data-act="practice-reroll" >|data-act="practice-reroll">/);
  setLang('en');
  for (const s of pr.SCENARIO_IDS) {
    const x = { ...pr.practiceState({ scenario: s }) };
    x.result = pr.runScenario(k, x, seedOf(5));
    const en = text(pr.render(x));
    const hit = en.match(JP);
    assert.equal(hit, null, `${s}: ${hit ? en.slice(Math.max(0, hit.index - 40), hit.index + 40) : ''}`);
  }
  assert.match(text(pr.render({ ...st, result: null }, { kernelError: 'NoWasm' })), /cannot be loaded \(NoWasm\)/);
  const w = await resolvedWorld(k);
  const built = rep.buildClashArgs({ province: decode('Province', w.before), inputs: decode('ClashInputs', w.inputs), bell: BELL, seed: hex(SEED) });
  const wi = { ...pr.practiceState(), whatif: { p: P, q: Q, bell: BELL, args: built.args, outcome: w.res.outcome, mine: () => false, mineCount: 0, result: null } };
  const wm = text(pr.render(wi));
  assert.match(wm, /What if: Province 2,0 · Bell 40/);
  assert.match(wm, /None of your hosts arrived/);
});

test('the game page routes the report and practice panels; the viewer\'s reports; the wave-4 actions and binds', () => {
  const saved = { ...FS };
  try {
    const holdings = [{ p: P, q: Q, site: 3, gen: 2 }];
    const mine = mineOf(holdings);
    assert.equal(mine(hid(3, 2, 7)), true);
    assert.equal(mine(hid(3, 2, 0)), true, 'its garrison');
    assert.equal(mine(hid(3, 1, 7)), false, 'another generation');
    assert.equal(mine(0xffffffffffffffffn - 3n), false, 'a camp');
    Object.assign(FS, { tab: 'marches', holdings, marches: [{ facts: { pipeline: 'resolved', nowBell: 55 }, dest: { p: 4, q: -1 }, entry: null, transit: { slot: 0, state: 2, hostId: 1n, arriveBell: 51 } }, { facts: { pipeline: 'revealing', nowBell: 60 }, dest: { p: 5, q: 0 }, entry: null, transit: { slot: 1, state: 1, hostId: 2n, arriveBell: 60 } }],
      chronicle: [{ seq: '1', record: { name: 'CLASH', p: P, q: Q, bell: 44 } }, { seq: '2', record: { name: 'CLASH', p: 9, q: 9, bell: 45 } }], report: null, practice: null, ui: { dismissed: ['onboarding'] }, land: { stage: 'final' }, citizen: null, view: { fog: true } });
    assert.deepEqual(myReports(FS).map(x => [x.p, x.q, x.bell]), [[4, -1, 51], [P, Q, 44]]);
    const marches = panelMarkup(FS).map(String).join('');
    assert.match(marches, /data-act="report-open" data-p="4" data-q="-1" data-bell="51"/);
    FS.tab = 'more';
    const more = panelMarkup(FS).map(String).join('');
    assert.match(more, /data-act="practice-open"/);
    assert.match(more, /data-act="ob-restore"/, 'a dismissed guide can come back');
    assert.match(more, /data-bell="45"/, 'recent clashes of the season');
    FS.report = { p: P, q: Q, bell: 44, loading: true };
    assert.match(panelMarkup(FS).map(String).join(''), /report-title/);
    W4_ACTIONS['report-close']();
    assert.equal(FS.report, null);
    W4_ACTIONS['practice-open']();
    assert.equal(FS.practice.scenario, 'camp');
    assert.match(panelMarkup(FS).map(String).join(''), /practice-title/);
    assert.match(panelMarkup(FS).map(String).join(''), /data-act="practice-close"/, 'the game page can close practice');
    assert.doesNotMatch(String(pr.render(FS.practice)), /practice-close/, 'practice.html has nothing to close');
    W4_ACTIONS['practice-scenario']({ scenario: 'defend' });
    assert.equal(FS.practice.troops, 300);
    W4_ACTIONS['practice-scenario']({ scenario: 'bogus' });
    assert.equal(FS.practice.scenario, 'defend');
    assert.equal(w4Bind('pr-troops', '999999'), true);
    assert.equal(FS.practice.troops, 30_000);
    assert.equal(w4Bind('pr-stance', '7'), true);
    assert.equal(FS.practice.stance, 3);
    assert.equal(w4Bind('pr-retreat', 'custom'), true);
    assert.equal(FS.practice.retreat, 'never', 'no custom ratio in practice');
    assert.equal(w4Bind('stance', '1'), false, 'the composer\'s binds stay the controller\'s');
    W4_ACTIONS['practice-close']();
    assert.equal(FS.practice, null);
    for (const a of ['report-open', 'report-verify', 'report-whatif', 'practice-reroll', 'ob-seen', 'ob-skip', 'ob-dismiss', 'ob-restore']) assert.equal(typeof W4_ACTIONS[a], 'function', a);
  } finally { Object.assign(FS, saved); }
});

test('every verification reason the report gives has its own Japanese and English text', () => {
  const src = readFileSync(new URL('permutation-server/web/frontier/screens/report.mjs', REPO), 'utf8');
  const whys = new Set([...src.matchAll(/\bwhy: '([A-Za-z]+)'/g)].map(m => m[1]));
  for (const must of ['NoProvinceBefore', 'SeedMismatch', 'DigestMismatch', 'FateMismatch', 'SeedNotReady']) assert.ok(whys.has(must), must);
  for (const w of [...whys, 'Refused', 'BadInput', 'NoWasm', 'RulesetMismatch']) {
    setLang('ja');
    const ja = rep.codeText(w);
    setLang('en');
    const en = rep.codeText(w);
    assert.match(ja, JP, `${w}: Japanese`);
    assert.doesNotMatch(en, JP, `${w}: English`);
  }
});
