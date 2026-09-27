# W1-B rules-bounds: notes

- **Unit:** W1-B (wave 1), branch `frontier/m1-W1-B`, cut from `frontier/m1-integ` at `d9d32ec`.
- **Brief (contract v1.1 §11):** CL-02, CL-03, CL-04, CL-05, CL-06, CL-08, CL-09, CL-11, CL-12, CL-13, CL-15, CL-18. No `Effect::Troops` (I-56): none was added.
- **Files changed (all inside the W1-B ownership list):** `permutation-rules/src/frontier/{holding,host,geometry,siege,payout,pools,mandate}.rs`; `permutation-rules/tests/{frontier_bounds (new),frontier_world,frontier_economy}.rs`; this file. `index.rs`, `laurel.rs`, `travel.rs` and `terrain.rs` needed no change.
- **Tags:** [measured] = run for these notes; [sim] = frontier-sim output.

## 1. What landed

| CL | Kernel change | Tests |
|---|---|---|
| CL-02 | `holding::{MAX_WALLS = 1,200, MAX_PRODUCTION_PER_HOUR = 1,000 units/h, MAX_UPKEEP_PER_HOUR = 10⁸ units/h}` (milli-units in code). `Holding::enqueue` refuses (`HoldingError::AboveCap`) an effect that, on top of the current values **and every queued item**, would pass a cap, and a negative delta larger than the cap (garbage). `apply` clamps (saturating) as the second line; `net_rate` reads production and upkeep through the caps; `set_upkeep` clamps; `walls_at` and `commit_walls` stay ≤ `MAX_WALLS`. `siege::required_bells` reads walls at most `MAX_WALLS` and saturates; `siege::MAX_REQUIRED_BELLS = required_bells(MAX_WALLS, doctrine::bounds::SIEGE_EXTRA_MAX) = 84` | `frontier_bounds::{holding_effects_are_capped (10,000 seeded sequences), enqueue_refuses_effects_past_the_caps, accruals_at_the_caps_do_not_overflow}` |
| CL-03 | `holding::duplicate_cost(base, n) -> Option<u64>`: `None` for `n > MAX_DUPLICATES` (64) or on overflow; values unchanged | `duplicate_cost_is_checked`; `holding::tests::duplicate_rule`; `frontier_world` line 429 |
| CL-04 | `ProvinceCoord::checked(p, q, r_max: u16) -> Result<_, OutOfBounds>` (ring ≤ min(r_max, 128)); `ring()` computed in `i64` (saturates at `u32::MAX`, reached only by `(i32::MIN, i32::MIN)`); `checked_index()` / `checked_from_index(i)` refuse ring > 128 / `i ≥ 49,537` | `province_coordinates_are_checked`; `frontier_world::coordinates_are_bounded` extended (i32 extremes, ring 128/129, round trip over all 49,537 provinces) |
| CL-05 | `host::Stamina::set(b, v) -> Result<(), HostError>`, `HostError::TimeReversed` for `b < self.bell` (the same bell replaces). Callers in `host.rs` propagate: `apply_clash`, `settle` (Spend), `settle_merge` check before writing, so a refusal changes nothing; `Host::route(b)` now returns `Result` | `stamina_set_refuses_an_earlier_bell`, `stamina_spend_then_late_set` (the first-pass scenario, and through a host); the lag-invariance tests in `frontier_world` still pass |
| CL-06 | `geometry::valid_faction(f, allow_neutral)` (`0..=5`, plus `NEUTRAL` = 6); `siege::may_besiege` refuses a bad attacker or owner faction (`SiegeRefusal::BadFaction`; a Free City's owner may be NEUTRAL); `BellReport::holds` counts only valid ids | `faction_ids_are_limited` |
| CL-08 | `payout::Ledger` replaced by `SeasonLedger` + `FactionLedger` (`pot_citizen`, `pot_laurel`, `claimed`, `swept`, plus `drawn_citizen`, `drawn_laurel`). `SeasonLedger::new(&Settlement)`; `record(faction, &Claim)` refuses (nothing changes) a claim whose parts do not add up, one that takes its faction past either pot, the Civilisation Share past its pot, or the season past its prize | `ledger_is_per_faction` (an over-claim in faction 2 offset by an under-claim in faction 4 is refused although the season total would accept it); the 300- and 200-season conservation tests in `frontier_economy` now book through `SeasonLedger` and pass unchanged |
| CL-09 | `siege::Vigil::request_change` takes effect at `change_effective_at(now)` = the first UTC midnight ≥ `now + 24 h`. **Plus a window rule (deviation D1 below).** `bells_outside_vigil` cuts its periodic segments also one day after each change | `vigil_change_lands_on_a_midnight` (`now` swept over two days at 60-s steps), `no_vigil_window_exceeds_8h_across_a_change` (every pair of starts on a 30-min grid; the three worst pairs with `now` swept over a day at 60-s steps; a **control** implementing the midnight rule alone finds the 16-hour vigil), `bells_outside_vigil_matches_bell_by_bell_across_a_change` (300 random vigils with up to two changes); `frontier_world::{vigils_change_weekly_with_notice, quiet_runs_count_like_single_bells}` unchanged and green |
| CL-11 | `PayoutParams::validate` refuses `office_ceiling_bps > 10,000` (except `u32::MAX`, kept for REV2 comparison runs); `validate_for_season()` also refuses `u32::MAX` (CreateSeason calls it) | `office_ceiling_above_paid_is_refused` |
| CL-12 | `CitizenRecord::weights()` removed, and `payout::works_weight()` (same trap: the constant 105) with it. `add_stake(stake, works_per_usdc)` and `transfer_counted(to, amount, works_per_usdc)` take the season's rate; `WORKS_PER_USDC` stays as the REV3 constant, read by no kernel | `weights_follow_the_season_rate` (140 vs 105 differ; transfer, AddStake and settle agree; totals summed at one rate and claimed at another are refused, never overpaid) |
| CL-13 | `EntrySchedule::accrual_left(day)` in closed form, `u128` (now `pub`, with `accrual_rate`) | `accrual_left_closed_form_matches_the_loop` (56,885 points: S ∈ {1,2,7,28,29,100,365,366} × up to 4 join days × 5 ramps up to 100,000 × every day 0..=366, plus `u32` extremes), `season1_laurel_stake_is_unchanged` |
| CL-15 | `mandate::claim_deadline(term_end, season_end) = min(term_end + 1 term, season_end + 72 h)`; `MandateTerm.claim_deadline` fixed at close: `close(reserve, term_end, season_end)`, `close_with_floor(reserve, floor, term_end, season_end)`; `claim(now, reserve, shares)` refuses after the deadline; `sweep(now, reserve)` (was `deadline_passed: bool`) sweeps unclaimed shares only once `now > deadline`; `Reserve::final_sweep()` refuses while a closed term is unswept, moves the whole balance out (`final_swept`) for the laurel general split and finalises the reserve (no deposit or close after it) | `mandate_claims_close_before_the_reckoning`, `final_sweep_conserves` (200 random 7-term histories: `deposited == paid + burned + final_swept`) |
| CL-18 | none (already resolved at `d95fa25`) | `pow_frac_accepts_gamma_above_one` |

Test counts [measured]: `cargo test --locked --release -p permutation-rules` **373 passed, 0 failed** (354 before: +19 in the new `frontier_bounds.rs`, the extended `coordinates_are_bounded` counted once); the same 373 in a debug build (overflow checks on), so the fuzzers found no overflow.

## 2. How the caps were pinned (CL-02, CL-03)

The closeout asked for "the largest legal value, with headroom". I measured the simulator instead of guessing [sim, scratch copy of `frontier-sim` instrumented at `refresh_upkeep`/`apply_tier_bonus`, 10,000 agents seeds 1 and 7 (10% bots), 20,000 agents seed 3 with doctrines]:

| Quantity | Highest in the sim | Cap | Headroom |
|---|---|---|---|
| Production of one resource | 230 units/h | 1,000 units/h | 4.3× |
| Upkeep of one resource | 47.3 units/h | 10⁸ units/h | binds only above ≈ 6 M tier-2 troops on one holding (upkeep is quadratic); it exists to keep every `i64` product far from overflow |
| Walls (committed + queued) | 600 | 1,200 | 2×; a siege then needs at most 36 + 24 + 24 = 84 bells |
| Copies of one building | 12 | 64 | 5× |
| One Production delta | 12 units/h | — | — |

`rate × 28 days` at the caps is ≤ 6.7 × 10¹³ milli-units, far inside `i64`.

## 3. Outcome check (bounds only refuse invalid input)

frontier-sim is W1-D's; it does not build against this branch until its callers are patched (§5). In a scratch copy with the §5 patch, pointed at this branch's `permutation-rules` [measured]:

| Run | digest at base (`m1-integ` rules) | digest with W1-B |
|---|---|---|
| `run --agents 3000 --seed 1` | `9251c46b1e5908df` | `9251c46b1e5908df` |
| `run --agents 3000 --seed 2` | `dae5dcaca0fce43a` | `dae5dcaca0fce43a` |
| `run --agents 3000 --seed 3` | `1aadeab5c8fba060` | `1aadeab5c8fba060` |
| `run --agents 10000 --seed 1` | `dcb040cb5084f905` | `dcb040cb5084f905` |
| `run --agents 10000 --seed 7 --bots 0.05` | `3858705903e34e2f` | `3858705903e34e2f` |
| `run --agents 5000 --seed 4 --doctrines` | `1f8f2bbda004b042` | `1f8f2bbda004b042` |

Also in that copy: `cargo fmt -- --check` clean, `cargo clippy --release --all-targets -- -D warnings` clean, `cargo test --release` 11 passed (5.5 min). No `expect` in the patch fired, so the simulator never sets stamina for an earlier bell, never exceeds 64 copies, and every Mandate claim lands inside its deadline.

**CL-09 and the gate re-run.** CL-09 changes an outcome only when a vigil changes; frontier-sim never calls `request_change` (it sets each vigil once at join), so no simulated outcome can move, as the identical digests show. The doctrine proxy gate was re-run anyway: By the Gate W1 rule the overnight 1,500-season doctrine run is **not triggered by W1-B** (no outcome changed); W1-A's CL-10/I-43 may still trigger it.

**Doctrine proxy gate re-run** [measured, scratch copies, `doctrine-gate --controls`, 10,000 wallets × 30 paired seeds × 6 rotations + the three controls]: with W1-B's kernels it **exits 0**: the kernel table passes the ±0.2% index gate (4 of 6 in the win-rate band, largest |Δ index| 0.072%, 180/180 seasons conserved) and all three controls are rejected (draft −2.061%, Knight −0.219%, A boost +0.384%). The same command on the base kernels (`m1-integ`) prints **byte-identical output apart from the wall-time figures**. The bot criterion (`criterion --best-response --gate`) was not re-run: its inputs are the same simulated seasons, whose digests are identical above, and its `--first-seed` form is W1-D's CL-16.

## 4. Deviations and choices (for the integrator and DECISIONS)

- **D1 (CL-09, rule text).** The midnight rule alone does not remove the long vigil: a change from a window that ends at (or wraps past) midnight to one that starts at midnight still joins them, e.g. 16:00 → 00:00 gives one 16-hour vigil (the control in `no_vigil_window_exceeds_8h_across_a_change` measures exactly 16 h). So besides the midnight, **each 8-hour window belongs to the schedule in force when it starts** (a window that began before the change runs to its end), and **the new schedule's first window is skipped if it would start before the last old window has ended**. Every covered stretch is then ≤ 8 h (checked on all 2,304 pairs of a 30-min grid). A player who makes such a change loses at most part of one day's vigil, by their own choice. `vigil_from_ts` in the Citizen layout (contract §5.3) is still the midnight. DESIGN §6.3 ("a change takes effect at the first UTC midnight at least 24 h after the request") should add the window rule (DOC part: W1-D).
- **D2 (CL-04).** `index()` / `from_index()` keep their signatures (frontier-sim calls them); the checked forms are `checked_index()` / `checked_from_index()`, and the program must use them (and `checked`) for anything read from data. `r_max` above 128 is treated as 128, not refused.
- **D3 (CL-05).** `Host::route(b)` returns `Result<(), HostError>` (it sets stamina).
- **D4 (CL-08).** `SeasonLedger::new` takes the `Settlement` (the per-faction pots come from it); `record` takes the claimant's faction. The Civilisation Share is season-wide (`civ_pot`, `civ_drawn`); a claim's steward rows are drawn from `pot_citizen` (which includes `steward_paid()`), its laurel part from `pot_laurel`. `FactionLedger.claimed/swept` attribute the Civilisation Share part of a capped claim to "paid" first. The old `Ledger` is gone.
- **D5 (CL-12).** `payout::works_weight()` removed along with `weights()` (it had the same trap).
- **D6 (CL-15).** Argument order `claim(now, reserve, shares)` and `sweep(now, reserve)` (the contract writes `claim(now, …)`). `close`/`close_with_floor` take `(term_end, season_end)` and compute the deadline themselves, so no caller can pass a later one. New fields: `MandateTerm.claim_deadline`, `Reserve.{final_swept, finalized}` (M2 layouts; M1 does not store Mandates).
- **D7 (CL-06, cross-unit).** `clash.rs` is W1-A's: `Relations::set_peaceful` still accepts id 7 on this branch (it refuses ≥ 8), and the "private copy with identical semantics" test must sit in `clash.rs` (a private function cannot be reached from `tests/`). `frontier_bounds::faction_ids_are_limited` pins `valid_faction` against `clash::NEUTRAL` and `clash::FACTION_LIMIT`.
- **New error variants:** `HoldingError::AboveCap`, `HostError::TimeReversed`, `SiegeRefusal::BadFaction` (no exhaustive `match` on them anywhere in the repo [measured by grep]).
- **Rules version.** CL-09 is a rule change for vigil changes. `RULES_VERSION_FRONTIER` and the `RULESET_HASH` input function live in `frontier/mod.rs` (W1-C); whether to bump is W1-C's/the integrator's call.

## 5. Handoff: frontier-sim callers (W1-D or the integrator)

Merging W1-B first (the listed order) **breaks the frontier-sim build until its callers follow**: `duplicate_cost` returns `Option`, `Stamina::set` returns `Result` (a `must_use` warning fails clippy), `Ledger` → `SeasonLedger`, and `MandateTerm::{close_with_floor, claim, sweep}` changed. This patch (applied and tested in the scratch copy above: digests identical, fmt/clippy/tests green) makes it build; it touches only frontier-sim, which W1-B does not own:

```diff
--- a/frontier-sim/src/settle.rs
+++ b/frontier-sim/src/settle.rs
@@ use permutation_rules::frontier::payout::{
-    claim, settle, tenure_units, CitizenRecord, Claim, FactionTotals, Ledger, PayoutParams,
+    claim, settle, tenure_units, CitizenRecord, Claim, FactionTotals, PayoutParams, SeasonLedger,
     Settlement,
 };
@@ pub struct Outcome {
-    pub ledger: Ledger,
+    pub ledger: SeasonLedger,
@@ pub fn settle_run(sim: &Sim, p: &IndexParams) -> Outcome {
-    let mut ledger = Ledger::new(prize);
+    let mut ledger = SeasonLedger::new(&st).expect("ledger");
+    assert_eq!(ledger.prize, prize);
@@
-        ledger.record(&c).expect("ledger");
+        ledger.record(r.faction, &c).expect("ledger");
--- a/frontier-sim/src/sim.rs
+++ b/frontier-sim/src/sim.rs
-use permutation_rules::frontier::mandate::{share_floor, MandateTerm, Reserve, TERM_DAYS};
+use permutation_rules::frontier::mandate::{
+    share_floor, MandateTerm, Reserve, TERM_DAYS, TERM_SECS,
+};
@@ (buildings)
-            let c: [Milli; RESOURCES] =
-                core::array::from_fn(|r| duplicate_cost(bd.cost[r] as u64, n) as i64 * MILLI);
+            let c: [Milli; RESOURCES] = core::array::from_fn(|r| {
+                duplicate_cost(bd.cost[r] as u64, n).expect("building copies within MAX_DUPLICATES")
+                    as i64
+                    * MILLI
+            });
@@ (settlers)
-                duplicate_cost(SETTLER_COST[r] as u64, n - 1) as i64,
+                duplicate_cost(SETTLER_COST[r] as u64, n - 1).expect("settler cost") as i64,
@@ (routs)
-                x.stamina.set(b, 0);
+                x.stamina.set(b, 0).expect("stamina bells in order");
@@ (clash results)
-                x.stamina.set(b, f.stamina);
+                x.stamina.set(b, f.stamina).expect("stamina bells in order");
@@ fn term_end(&mut self, term: u32) {
-            t.close_with_floor(&mut self.reserve[f], floor)
+            // CL-15: the term ends now; claims close one term later (or at
+            // the end of the banking window). The sim claims at once.
+            let term_end = (term as i64 + 1) * TERM_SECS;
+            let season_end = self.cfg.days as i64 * 86_400;
+            t.close_with_floor(&mut self.reserve[f], floor, term_end, season_end)
                 .expect("close term");
             for (a, sh) in who {
-                let pay = t.claim(&mut self.reserve[f], sh).expect("mandate claim");
+                let pay = t
+                    .claim(term_end, &mut self.reserve[f], sh)
+                    .expect("mandate claim");
@@
-            t.sweep(&mut self.reserve[f], false).expect("sweep term");
+            t.sweep(term_end, &mut self.reserve[f]).expect("sweep term");
```

(The full `git diff` with line numbers is in the session scratchpad, `w1b/frontier-sim-callers.patch`.) The simulator does not yet call `Reserve::final_sweep` (its end-of-season `mandate_left` stat is the leftover balance); wiring the Reckoning order into the sim is CL-15's G2 part, not needed for M1.

## 6. Gate W1 items for these files [measured, on `frontier/m1-W1-B`]

| Item | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked -p permutation-rules --all-targets -- -D warnings` | clean (`-p frontier-abi` does not exist yet on this branch: W1-E) |
| `cargo test --locked --release -p permutation-rules` | 373 passed, 0 failed (debug build: 373 / 0) |
| `cargo test --locked -p permutation-chain` | 157 passed, 0 failed |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| frontier-sim fmt / clippy / test / doctrine gate | not runnable on this branch until §5 lands (W1-D); run in a scratch copy with §5 applied: see §3 |

Not run by W1-B (other units' files or the integrator's): `frontier-abi`, `frontier-node`, `permutation-gateway`, the civilization tests, `criterion --first-seed` (CL-16 is W1-D's flag). No PENDING-OWNER item concerns W1-B's files. No port was bound, no service started, nothing downloaded or installed.

## 7. Dependency requests

None: no manifest, lockfile, toolchain or `.gitignore` was changed.
