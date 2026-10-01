# Wylls: M0 close (addendum to M0-FINAL)

- **Date:** 2026-09-27, written by unit W1-D at the end of M1 wave 1 (CL-38). **Base:** `frontier/m1-integ` at `d9d32ec`.
- **What it does:** maps every line of `M0-FINAL.md` §5 ("What remains to close M0") to its closeout task (CL-xx, `m1/design/closeout.md`), the M1 unit that owns it (`m1/M1-CONTRACT.md` §11), the commit that carries it and the test that shows it, and re-grades the M0 exit table.
- **N5 overshoot, recorded:** the owner decided to start M1 at once and close M0 in M1's first week (N5). The closeout needs **1.5 weeks**, not 1: wave 1 runs 2026-09-28 → 10-07 and carries the whole closeout plus the M1 foundations (contract §0 item 4, §14).
- **How to read the commit column.** Each wave-1 unit commits on its own branch `frontier/m1-W1-<x>`; the integrator merges them in the order W1-B, W1-A, W1-C, W1-E, W1-F, W1-D and runs Gate W1 (§12). Each cell gives the unit's branch commit and, in brackets, the `--no-ff` merge commit on `frontier/m1-integ` that carries it (filled by the integrator in the wave-1 integration window, §3.4); test names are the landed ones (file::name where the file is not obvious). **G0** (contract §11 / closeout §2) is the set that must be merged before the first program instruction links a rules-v10 kernel: CL-01…08, 11, 12, 16, 19…21, 23…25.

## 1. M0-FINAL §5, line by line

| M0-FINAL §5 item | Task | Gate | Unit, branch | Commit | Test or evidence |
|---|---|---|---|---|---|
| 1. Walls, production and upkeep caps | CL-02 | G0 | W1-B | `ffa960a` (merged `218f906`) | `frontier_bounds.rs::holding_effects_are_capped` |
| 1. `clash::validate` refuses troops > `MAX_HOST_TROOPS`, `dealt_bps` > `COMBAT_MAX_BPS`, stamina > cap (+ retreat ≤ 60,000, faction ≤ 6) | CL-01, CL-06 | G0 | W1-A (clash), W1-B (`geometry::valid_faction`, siege) | W1-A `a15559d` (merged `f62d190`); W1-B `ffa960a` (merged `218f906`) | `frontier_clash_bounds.rs::clash_validate_refuses_out_of_range`, `clash_bounds_do_not_change_honest_outcomes`, `faction_ids_are_limited`; `frontier_bounds.rs::faction_ids_are_limited` |
| 1. Checked `duplicate_cost` | CL-03 | G0 | W1-B | `ffa960a` (merged `218f906`) | `frontier_bounds.rs::duplicate_cost_is_checked` |
| 1. `ProvinceCoord` ring bound | CL-04 | G0 | W1-B | `ffa960a` (merged `218f906`) | `frontier_world.rs::coordinates_are_bounded` (ring 128/129, i32 extremes, round trip) |
| 1. `Stamina::set` monotone | CL-05 | G0 | W1-B | `ffa960a` (merged `218f906`) | `frontier_bounds.rs::stamina_set_refuses_an_earlier_bell`, `stamina_spend_then_late_set` |
| 1. Conservation checks written as bounds | CL-07 | G0 | **W1-D** | `e60173a` (merged `f6d1ab7`) | `frontier-sim` `a_small_season_conserves_money_and_laurels` with mutation controls (1 extra unit paid, one laurel credited twice, one Mandate unit paid twice, a claim above 5×) |
| 1. A per-faction Ledger | CL-08 | G0 | W1-B (kernel `FactionLedger`/`SeasonLedger`), **W1-D** (simulator side) | kernel `ffa960a` (merged `218f906`); sim `e60173a` (merged `f6d1ab7`); the simulator's settlement books claims through the kernel `SeasonLedger` since integ `afaf5a1` | kernel `frontier_bounds.rs::ledger_is_per_faction`; sim `the_ledger_is_per_faction` |
| 2. `PayoutParams::validate` refuses `office_ceiling_bps > 10,000` | CL-11 | G0 | W1-B | `ffa960a` (merged `218f906`) | `frontier_bounds.rs::office_ceiling_above_paid_is_refused` |
| 2. Remove `CitizenRecord::weights()` | CL-12 | G0 | W1-B (kernel); **W1-D** (callers) | kernel `ffa960a` (merged `218f906`); the simulator already calls `weights_at` everywhere (checked at `e60173a`, no change needed) | compile check; `weights_follow_the_season_rate` |
| 2. Closed-form `accrual_left` | CL-13 | G2 (done in week 1) | W1-B | `ffa960a` (merged `218f906`) | `frontier_bounds.rs::accrual_left_closed_form_matches_the_loop` |
| 2. Bound the product of combat multipliers | CL-14 | G0 | W1-A | `a15559d` (merged `f62d190`) | `frontier_clash_bounds.rs::damage_product_cannot_overflow`, `validate_table_refuses_a_product_above_the_bound` |
| 2. `criterion --first-seed`, held-out restatement | CL-16 | G0 (flag), GX (table) | **W1-D** | `e60173a` (merged `f6d1ab7`) | `criterion_first_seed_changes_the_seeds`; held-out table in `m1/W1-D-NOTES.md` §3 (worst 0.985, seeds 30002–30004, D23 on) |
| 2. Mandate claim deadline and its order to the Reckoning | CL-15 | G2 (kernel and text in week 1) | W1-B (kernel); **W1-D** (text, DESIGN §8.10) | kernel `ffa960a` (merged `218f906`); text `35713a2` (merged `f6d1ab7`); simulator callers integ `afaf5a1` | `frontier_bounds.rs::mandate_claims_close_before_the_reckoning`, `final_sweep_conserves` |
| 2. Season-1 sweep target | CL-17 | G2 | owner (O-M1-16) | — | open: recommendation in `DECISIONS.md` part D (escrow for a successor, fee pro-rata after 90 days) |
| 3. `round_at` → "first round at or after" (bell, ring, genesis) | CL-19 | G0 | W1-C (kernel `beacon`), **W1-D** (DESIGN §0.5, §8.5) | W1-C `1dfc96a` (merged `bb16d01`); text `35713a2` (merged `f6d1ab7`) | `seed_rounds_round_up`; `clock-vectors-v1.json` |
| 3. `T(b)` rounding; roster and posture cutoffs | CL-20 | G0 | W1-C, **W1-D** (DESIGN §2.1) | W1-C `1dfc96a` (merged `bb16d01`); text `35713a2` (merged `f6d1ab7`) | `tlock_round_is_at_or_after_bell_end`, `cutoffs_do_not_depend_on_round_phase` |
| 3. With-seed grammar and length bounds | CL-21 | G0 | W1-C, **W1-D** (DESIGN §8.2) | W1-C `1dfc96a` (merged `bb16d01`); text `35713a2` (merged `f6d1ab7`) | `seed_strings_fit_32_bytes`, `seed_strings_are_injective`; `addr-vectors-v1.json` = SP-V2 `acct.rs` |
| 3. Minimum reveal tip in priority terms | CL-22 | G1(Depart) | W1-C (`fees::min_tip_lamports`), **W1-D** (DESIGN §6.2, §6.4) | W1-C `1dfc96a` (merged `bb16d01`); text `35713a2` (merged `f6d1ab7`) | `min_tip_matches_priority`; Depart's `TipTooLow` is a wave-3 program test |
| 3. One-way reveal latch | CL-23 | G0 (layout) | W1-E (Reveal account list), W3-B (Reveal), **W1-D** (DESIGN §6.2, §8.3, §9.4) | layout W1-E `56eda82` (merged `d63e136`); text `35713a2` (merged `f6d1ab7`) | program gate G4 `reveal_refused_after_first_gather` (wave 3) |
| 3. Genesis re-roll guard | CL-24 | G0 (layout) | W1-E (AnnounceSeason layout), W2-A (instruction), **W1-D** (DESIGN §8.5, §8.10) | layout W1-E `56eda82` (merged `d63e136`); text `35713a2` (merged `f6d1ab7`) | `create_before_announcement_refused`, `genesis_round_fixed_by_announcement`, `season_id_single_use`, `pre_join_abort_forfeits_bond` (wave 2) |
| 3. Mark the 0.4-s slot rows [model] | CL-25 | G0 | **W1-D** | `35713a2` (merged `f6d1ab7`) | DESIGN §8.7, §8.9 tagged |
| 4. D18 pool sizing (whole-block attack delays every write) | CL-30, CL-31a | GX / G2 | **W1-D** | sim counts `e60173a` (merged `f6d1ab7`); table `35713a2` (merged `f6d1ab7`) | `frontier-sim c4` v3 per-bell writes; lab `m1/lab/d18/d18-v3.txt`; DESIGN §6.4, §21.3; owner confirms (D18) |
| 4. 1,200-s windows; relic clashes at the relic tip | CL-26 | GX | **W1-D** (model v3; W6-E re-runs it with the measured Reveal) | `35713a2` (merged `f6d1ab7`) | lab `m1/lab/c4-v3/c4-model-v3.txt` |
| 4. Clock-drift label; lookup-table lock count | CL-27 | DOC | **W1-D** | `35713a2` (merged `f6d1ab7`) | `SPIKE-SP-FEE.md` correction header (also the loaded-data limit, I-45); DESIGN §8.5–§8.7 |
| 4. Seed Join's wallet RNG | CL-28 | GX | W2-B | wave 2 | Join budget test over 1,000 seeded wallets + an adversarial one |
| 4. 1,200-fill worst-case search on ResolveFromInputs | CL-29 | G1(RFI) | W1-A (lab `m1/lab/rfi-search/`) | `a15559d` (merged `f62d190`) (measured on the kernel merged unchanged) | worst RFI ≤ 340k CU, heap ≤ 28,672 B on SBF v2 (W1-A notes); full write-back over all fills is W4-A's |
| 5. Push and GitHub CI | CL-34, CL-35, CL-36 | GX | **W1-D** | `e60173a` (merged `f6d1ab7`) (civ tests), `35713a2` (merged `f6d1ab7`) (CI jobs, run record) | run 36303403992 recorded (`DECISIONS.md` part F); the two legacy failures fixed (47/47 locally); M1 jobs defined; **no push** (O-M1-17) |

Tasks outside M0-FINAL §5 that the closeout added: CL-09 (vigil at a UTC midnight, W1-B, outcome-changing), CL-10 (cap recount and scouts, W1-A, failing-first), CL-18 (γ test, W1-B), CL-31 (D23 in the simulator: **W1-D** `e60173a`; kernel `office`: W1-C `1dfc96a`), CL-32/CL-33 (D22 and D24 measured, **W1-D**, `DECISIONS.md` part F), CL-37 (decisions log, **W1-D**), CL-38 (this file).

## 2. M0 exit, re-graded

| # | Exit criterion (DESIGN §12) | M0-FINAL | Now | Why |
|---|---|---|---|---|
| (a) | Spike numbers, CU rows on SBPF v2 | met | **met** | unchanged; the full Reveal is measured in M1 wave 3 |
| (b) | Stance and doctrine balance (proxy gate + controls; O5 band on ≥ 1,500 paired seasons) | met locally | **met locally, re-checked with D23** | proxy gate with its three controls re-run at W1-D (all rejected, calibrated harness); O5 band with D23 on: 6/6, largest gap 1.0 point on 1,500 paired seasons (`m1/W1-D-NOTES.md` §3). Re-run on the merged branch `frontier/m1-integ` (CL-09, CL-10, I-43 and D23 together): proxy gate kernel table 4/6, gap 3.3 points, largest Δ index 0.072%, all three controls rejected (= m0c); O5 band 6/6, gap 1.0 point, largest Δ index 0.055%, 1,500/1,500 conserve (= the W1-D run) |
| (c) | C4 restated: pool cap 2.0, ≥ 150 rotating payers, on the valuation the owner accepts | partial | **met in the model on valuation (a)** (N2) | c4 v3 (CL-26); unverified until the M1 Reveal CU and the M4 soak; the default-tip shortfall stays a recorded result |
| (d) | β measured, γ set | met in simulation | met in simulation | unchanged |
| (e) | §9.4 kernel tests incl. quota fairness and seed margin | partial | **met on the merged wave-1 branch** (kernel bounds of W1-A and W1-B, the simulator's conservation bounds and per-faction book of W1-D) | Gate W1 on `frontier/m1-integ`, §3 below |
| — | GitHub CI green | not met | **not met (push not approved)** | the local equivalent is Gate W1; the two legacy failures are fixed locally; the M1 jobs are defined |

**M0 is closed when Gate W1 passes on the merged branch** with every G0 task above mapped to a merged commit (contract §12 pass conditions). Every G0 row now names its merged commit; the gate result is in §3.

## 3. Gate W1 on `frontier/m1-integ` (integrator, 2026-09-27)

Merged `--no-ff` in the §11 order: W1-B `218f906`, W1-A `f62d190`, W1-C `bb16d01`, W1-E `d63e136`, W1-F `6916500`, W1-D `f6d1ab7`. Integration-window commits: `719d2b2` root workspace members += `frontier-abi`, exclude `frontier-node` (I-37, I-55); `bab4620` `frontier-node` manifest, lock, toolchain and cargo config adopted from W1-F R1/R2; `afaf5a1` `frontier-sim` callers of W1-B's `SeasonLedger` and CL-15 Mandate API (the build broke after the merge; W1-B-NOTES §5); `0795d8e` `seal` re-exports `clash::RETREAT_MAX_BPS` (W1-C request). The gate was run on `0795d8e`; the commit that adds this section changes only this file.

| Gate W1 item (§12) | Result [measured] |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings` | exit 0 |
| `cargo test --locked --release -p permutation-rules` | exit 0 |
| `cargo test --locked -p frontier-abi` | exit 0 (38 tests) |
| `cargo run --locked -p frontier-abi --bin abi-vectors -- --check` | exit 0 (9 files fresh) |
| `cargo test --locked -p permutation-chain` | exit 0 |
| `frontier-sim` fmt, clippy `-D warnings`, `cargo test --release` | exit 0 (15 + 3 tests; `catalog_equality` also with `FRONTIER_REQUIRE_CATALOG=1`) |
| `criterion --best-response --seeds 3 --first-seed 30001 --gate` | exit 0, worst cell 0.985 |
| `doctrine-gate --controls` | exit 0, kernel 4/6 gap 3.3 points; draft, Knight (−0.219%), A boost (+0.384%) rejected |
| `frontier-node` `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings` | **PENDING-OWNER**: the installed 1.95.0 toolchain has no rustfmt or clippy component; `rustup component add` is a toolchain install, not approved with O-M1-12 (W1-F R3). Substitutes: `cargo +1.89.0 fmt --all -- --check` exit 0; `cargo +1.89.0 clippy --locked --ignore-rust-version` on the eight crates that build on 1.89 (all but `localnet`, `drand-replay`, `itest`, which link LiteSVM) exit 0; the whole workspace builds on 1.95.0 with 0 rustc warnings |
| `frontier-node` `cargo test --locked --workspace` | exit 0 (57 tests, 1 ignored: needs the SP-V2 lab `.so`; with `PSF_SPV2_SO` set it passes too) |
| `permutation-gateway` `npm ci --ignore-scripts && npm test` | exit 0 (327/327) |
| `node --test permutation-state-prototype/civilization/*.test.mjs` | exit 0 (47/47) |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| Overnight `doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate` (CL-10 changes kernel outcomes) | exit 0: 6/6 in band, gap 1.0 point, largest Δ index 0.055%, 1,500/1,500 conserve |

Pass conditions beyond exit codes: Phase A equivalence 4,320/4,320 (`phase_a_equals_the_reference_on_4320_inputs`, plus the occupancy, refund-corner and ties-and-edges variants) green; `occupancy_empty_keeps_the_d95fa25_digests` green; CL-29 worst ResolveFromInputs 307,139 CU and heap 18,856 B on SBF v2 (W1-A lab, measured on the `clash.rs` that is merged byte for byte unchanged); `catalog_equality` green and comparing; `localnet`'s `loaded_data_control_one_page_below_fails_at_the_need_passes` and `drand-replay`'s `test_key_mode_signs_on_demand` green; every G0 task mapped to a merged commit in §1.

PENDING-OWNER in this gate: the `frontier-node` fmt and clippy line only (above). O-M1-12 items (wasm32, Playwright, round archive, Agave) are not used by Gate W1.

### 3.1 Gate W1 re-run after the wave-1 review (integ-W1 window, 2026-09-27)

The review of all six units (six "needs-fix" verdicts) was answered in the integration window. Every blocker/major item was confirmed and fixed with a test; the minor items were fixed or rebutted. Details are in `docs/frontier/m1/integ-W1-NOTES.md`, the contract changes in `M1-CONTRACT.md` v1.2 §18 and the decisions in `DECISIONS.md` part G.

Commits: `7b0f115` (W1-A), `ba135ff` (W1-B), `cc35e34` (W1-C, RULESET_HASH and seal vectors), `c0359a5` (created-day flag, fee rounding), `eab7a7e` (W1-E), `e11abbb` (W1-F), `907dc9f` (W1-D), `2553a69` (docs). **707e65d** (W1-A's shim in W1-B/W1-D files) is accepted under the integration window.

The gate was run on `2553a69`, all 13 items as §12 writes them; the commit that adds this section changes only this file.

| Gate W1 item (§12) | Result [measured] |
|---|---|
| root `cargo fmt --all -- --check` | exit 0 |
| clippy `permutation-rules`, `frontier-abi` `-D warnings` | exit 0 |
| `cargo test --locked --release -p permutation-rules` | exit 0 (434 passed) |
| `cargo test --locked -p frontier-abi` | exit 0 (43 passed) |
| `abi-vectors -- --check` | exit 0 (9 files fresh) |
| `cargo test --locked -p permutation-chain` | exit 0 |
| `frontier-sim` fmt, clippy, `cargo test --release` | exit 0 (18 passed: 15 unit tests incl. the four 60-seed doctrine gate tests under D23, plus 3 `catalog_equality` tests, compiled unconditionally) |
| `criterion --best-response --seeds 3 --first-seed 30001 --gate` | exit 0, worst cell 0.985 (caretaker term exempt) |
| `doctrine-gate --controls` (shipping economy, 60 seeds) | exit 0: kernel 6/6, gap 1.4 points, largest \|Δ\| 0.063%. All three controls rejected: draft (−1.291% A, +8.77% E), Knight (−0.294%), A boost (+0.338%) |
| `frontier-node` `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace` | **exit 0 as written** (68 passed, 1 ignored). The 1.95.0 rustfmt and clippy components appeared on this machine during the review; this session did not install them. The owner should know. |
| `permutation-gateway` `npm ci --ignore-scripts && npm test` | exit 0 (327/327) |
| civilization tests | exit 0 (47/47) |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| Overnight O5 band (outcomes changed: CL-09 day rule, caretaker exemption, garrison cap) | exit 0: 6/6 in band, gap 1.1 points, largest \|Δ\| 0.069%, 1,500/1,500 conserve, 500 s (run on the code of `907dc9f`; later commits change docs only) |

Pass conditions beyond exit codes:
- Phase A equivalence 4,320/4,320 green, now also asserting the I-43 caps. `occupancy_empty_keeps_the_d95fa25_digests` and `clash_bounds_do_not_change_honest_outcomes` green.
- **CL-29 re-measured on the integ kernel:** worst ResolveFromInputs **307,546 CU**, heap **18,856 B** over 1,240 fills on SBF v2, with on-chain digests equal to the native kernel's (lab `integ-w1r/rfi/`).
- `catalog_equality` green, with no environment variable.
- The localnet loaded-data control (now asserting 602,471 B independently) and the drand-replay test-key tests green.
- The G0 map in §1 is unchanged; the named tests still exist.

**PENDING-OWNER in this gate: none.**

