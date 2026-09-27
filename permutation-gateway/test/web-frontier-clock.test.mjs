// The Frontier bell clock (permutation-server/web/frontier/clock.mjs):
// the round and bell arithmetic against the kernel's clock vectors
// (permutation-rules/vectors/clock-vectors-v1.json), the chain clock
// (extrapolation, scale detection, monotone), the bell chip, and the bell
// pipeline of web design §6.2 from herald facts — no countdown before THE
// anchor exists, the window closed by the Clock or by the BeaconLog
// reaching S, the seed round at or after A + W + Δ, archived bells.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as clock from '../../permutation-server/web/frontier/clock.mjs';
import * as herald from '../../permutation-server/web/frontier/herald.mjs';
import { NO_WINDOW_CHANGE } from '../../permutation-server/web/frontier/fcodec.mjs';

const vectors = JSON.parse(readFileSync(new URL('../../permutation-rules/vectors/clock-vectors-v1.json', import.meta.url), 'utf8'));
const drand = vectors.clock;

test('bell bounds, days and T(b) equal the kernel over every drand phase', () => {
  assert.ok(vectors.bells.length > 20);
  for (const v of vectors.bells) {
    const at = `phase ${v.phase} bell ${v.bell}`;
    assert.equal(clock.bellStart(v.genesis_ts, v.bell), v.bell_start, at);
    assert.equal(clock.bellEnd(v.genesis_ts, v.bell), v.bell_end, at);
    assert.equal(clock.dayOf(v.bell), v.day, at);
    assert.equal(clock.tlockRound(drand, v.genesis_ts, v.bell), v.tlock_round, at);
    assert.equal(clock.roundTime(drand.genesis, drand.period, v.tlock_round), v.tlock_round_time, at);
    assert.ok(v.tlock_round_time >= v.bell_end, 'T(b) is at or after the bell end');
    if (v.bell < 2 ** 32 - 1) assert.equal(clock.bellAt(v.genesis_ts, v.bell_start), v.bell, at);
  }
  assert.equal(clock.bellAt(1000, 999), null, 'no bell before genesis');
});

test('S = first round at or after A + W + Δ equals the kernel; genesis_ts too', () => {
  for (const v of vectors.seed_rounds) {
    assert.equal(clock.revealClose(v.anchor_ts, v.window), v.close);
    assert.equal(clock.seedRound(drand, v.close, v.margin), v.seed_round, JSON.stringify(v));
    assert.equal(clock.roundTime(drand.genesis, drand.period, v.seed_round), v.round_time);
  }
  for (const v of vectors.genesis_ts) {
    assert.equal(clock.roundTime(drand.genesis, drand.period, v.genesis_round) + 600, v.genesis_ts);
  }
  assert.equal(clock.firstRoundFrom(100, 3, 50), 1);
  assert.equal(clock.firstRoundFrom(100, 3, 100), 1);
  assert.equal(clock.firstRoundFrom(100, 3, 101), 2);
  assert.equal(clock.firstRoundFrom(100, 3, 103), 2);
  assert.equal(clock.firstRoundFrom(100, 3, 104), 3);
});

const season = { genesisTs: 1_800_000_000n, drandGenesis: BigInt(drand.genesis), drandPeriod: drand.period, seedMargin: 60, revealWindow: 600, windowNext: 900, windowFromBell: 200, endBell: 1008 };

test('W(b) follows the window schedule vectors', () => {
  for (const v of vectors.windows) {
    const s = { ...season, revealWindow: v.reveal_window, windowNext: v.window_next, windowFromBell: v.window_from_bell ?? NO_WINDOW_CHANGE };
    assert.equal(clock.seasonClock(s).window(v.bell), v.window, JSON.stringify(v));
  }
});

test('the chain clock extrapolates the herald\'s sample, detects the scale and never goes back', () => {
  let wall = 1_000_000;
  const c = new clock.ChainClock({ wall: () => wall, accelerated: true });
  assert.equal(c.now(), null);
  c.observe(5000, 10);
  assert.equal(c.now(), 5000);
  wall += 1500;
  assert.equal(c.now(), 5001.5, 'rate 1 by default');
  wall += 500;
  c.observe(5040, 15); // 40 chain seconds in 2 wall seconds: 20×
  assert.equal(c.rate, 20);
  wall += 1000;
  assert.equal(c.now(), 5060);
  c.observe(4000, 3); // an older answer arriving late is ignored
  assert.equal(c.now(), 5060);
  c.observe(5059, 16); // a sample behind the extrapolation: the clock holds, never goes back
  assert.ok(c.now() >= 5060);
  assert.equal(c.age(), 0);
  const d = new clock.ChainClock({ wall: () => wall });
  d.observe(wall / 1000 - 3, 1);
  assert.equal(Math.round(d.offset()), 3, 'local minus chain');
});

test('a real cluster\'s clock runs at rate 1: a lagging herald never makes it run ahead; a stuck one shows as stale', () => {
  // True chain time = wall seconds − 1,000,000 (rate 1).
  let wall = 2_000_000_000;
  const chain = () => wall / 1000 - 1_000_000;
  const c = new clock.ChainClock({ wall: () => wall });
  c.observe(chain(), 1);
  // The herald lags 40 s (answers stay 40 s old), then catches up at once.
  for (let i = 1; i <= 10; i++) { wall += 4_000; c.observe(chain() - 40, 1 + i); }
  assert.equal(c.rate, 1, 'no rate estimate off localnet');
  wall += 4_000;
  c.observe(chain(), 20);
  for (let i = 0; i < 30; i++) { wall += 1_000; assert.ok(c.now() <= chain() + 1e-9, `never ahead of the chain (${c.now() - chain()} s)`); }
  // A stuck herald: the same latestUnix on every poll for 300 s.
  const stuck = chain();
  c.observe(stuck, 21);
  for (let i = 0; i < 60; i++) { wall += 5_000; c.observe(stuck, 22 + i); }
  assert.equal(c.rate, 1);
  assert.ok(c.behind() >= 299, `behind ${c.behind()}`);
  assert.equal(herald.staleness({ behind: c.behind() }).stale, true, 'the 60-s banner fires');
  // The chip keeps moving (rate 1), it does not freeze.
  const t0 = c.now();
  wall += 10_000;
  assert.ok(c.now() - t0 >= 9.99);
});

test('an accelerated (localnet) clock keeps its rate while the herald is stuck and reports it stale', () => {
  let wall = 1_000_000;
  const c = new clock.ChainClock({ wall: () => wall, accelerated: true });
  c.observe(5000, 1);
  wall += 2_000;
  c.observe(5040, 2);
  assert.equal(c.rate, 20);
  for (let i = 0; i < 40; i++) { wall += 2_000; c.observe(5040, 3 + i); }
  assert.equal(c.rate, 20, 'a stuck herald does not drive the rate down');
  assert.ok(c.behind() >= 80);
  assert.equal(herald.staleness({ behind: c.behind() }).stale, true);
  // Switching to a real cluster resets the rate.
  c.setAccelerated(false);
  assert.equal(c.rate, 1);
});

test('the bell chip', () => {
  const k = clock.seasonClock(season);
  assert.deepEqual(clock.bellChip(k, 1_800_000_000 - 30), { bell: null, secondsLeft: 30, beforeGenesis: true, ended: false });
  assert.deepEqual(clock.bellChip(k, 1_800_000_000 + 600 * 1034 + 228), { bell: 1034, secondsLeft: 372, beforeGenesis: false, ended: true });
  assert.equal(clock.bellChip(k, null).bell, null);
  assert.equal(clock.countdown(372), '6:12');
  assert.equal(clock.countdown(3725), '1:02:05');
  assert.equal(clock.countdown(-4), '0:00');
});

test('the bell pipeline from herald facts (web design §6.2)', () => {
  const k = clock.seasonClock(season);
  const b = 40, end = clock.bellEnd(k.genesisTs, b);
  // Open until the bell ends.
  assert.equal(clock.pipeline(k, b, { now: end - 1 }).state, 'open');
  // No anchor: waiting for the beacon, no countdown.
  const wait = clock.pipeline(k, b, { now: end + 5 });
  assert.equal(wait.state, 'awaitingBeacon');
  assert.equal(wait.until, null);
  // THE anchor at A: revealing until A + W.
  const a = end + 2;
  const rev = clock.pipeline(k, b, { now: a + 100, anchor: { a } });
  assert.equal(rev.state, 'revealing');
  assert.equal(rev.until, a + 600);
  assert.equal(rev.seedRound, clock.firstRoundFrom(drand.genesis, drand.period, a + 600 + 60));
  // The BeaconLog reaching S closes the window before the Clock does.
  assert.equal(clock.pipeline(k, b, { now: a + 100, anchor: { a }, latestRound: rev.seedRound }).state, 'awaitingSeed');
  // After the close: the seed round's time, at or after A + W + Δ.
  const seedWait = clock.pipeline(k, b, { now: a + 600, anchor: { a } });
  assert.equal(seedWait.state, 'awaitingSeed');
  assert.ok(seedWait.until >= a + 660 && seedWait.until < a + 663);
  assert.equal(clock.pipeline(k, b, { now: a + 700, anchor: { a }, seed: true }).state, 'resolving');
  assert.equal(clock.pipeline(k, b, { now: a + 700, anchor: { a }, seed: true, resolvedNext: b + 1 }).state, 'resolved');
  assert.equal(clock.pipeline(k, b, { now: a + 700, anchor: { a }, seed: true, resolvedNext: b }).state, 'resolving');
  // A window change applies from its bell (W = 900 from bell 200).
  const e200 = clock.bellEnd(k.genesisTs, 200);
  assert.equal(clock.pipeline(k, 200, { now: e200 + 700, anchor: { a: e200 } }).state, 'revealing');
  assert.equal(clock.pipeline(k, 199, { now: clock.bellEnd(k.genesisTs, 199) + 700, anchor: { a: clock.bellEnd(k.genesisTs, 199) } }).state, 'awaitingSeed');
  // Archived: A from the archive entry, the seed from the archive.
  const arch = clock.pipeline(k, b, { now: end + 200_000, archive: { archived: true, tombstoned: true, aOff: 2, seed: true } });
  assert.equal(arch.state, 'resolving');
  assert.equal(arch.close, end + 2 + 600);
  assert.equal(arch.archived, true);
});
