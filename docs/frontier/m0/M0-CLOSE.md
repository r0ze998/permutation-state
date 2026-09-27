# The Sixfold Frontier: M0 close (addendum to M0-FINAL)

- **Date:** 2026-09-27, written by unit W1-D at the end of M1 wave 1 (CL-38). **Base:** `frontier/m1-integ` at `d9d32ec`.
- **What it does:** maps every line of `M0-FINAL.md` §5 ("What remains to close M0") to its closeout task (CL-xx, `m1/design/closeout.md`), the M1 unit that owns it (`m1/M1-CONTRACT.md` §11), the commit that carries it and the test that shows it, and re-grades the M0 exit table.
- **N5 overshoot, recorded:** the owner decided to start M1 at once and close M0 in M1's first week (N5). The closeout needs **1.5 weeks**, not 1: wave 1 runs 2026-09-28 → 10-07 and carries the whole closeout plus the M1 foundations (contract §0 item 4, §14).
- **How to read the commit column.** Each wave-1 unit commits on its own branch `frontier/m1-W1-<x>`; the integrator merges them in the order W1-B, W1-A, W1-C, W1-E, W1-F, W1-D and runs Gate W1 (§12). A hash below is the unit's branch commit as it stood when this file was written; "on merge" means the unit had not committed yet, and the integrator writes the merged commit into this table in the wave-1 integration window (§3.4). Test names of units that had not committed are the ones the closeout plan names; the integrator replaces them with the landed names. **G0** (contract §11 / closeout §2) is the set that must be merged before the first program instruction links a rules-v10 kernel: CL-01…08, 11, 12, 16, 19…21, 23…25.

## 1. M0-FINAL §5, line by line

| M0-FINAL §5 item | Task | Gate | Unit, branch | Commit | Test or evidence |
|---|---|---|---|---|---|
| 1. Walls, production and upkeep caps | CL-02 | G0 | W1-B | on merge | `frontier_bounds.rs::holding_effects_are_capped` |
| 1. `clash::validate` refuses troops > `MAX_HOST_TROOPS`, `dealt_bps` > `COMBAT_MAX_BPS`, stamina > cap (+ retreat ≤ 60,000, faction ≤ 6) | CL-01, CL-06 | G0 | W1-A (clash), W1-B (`geometry::valid_faction`, siege) | on merge | `frontier_clash_bounds.rs::clash_validate_refuses_out_of_range`, `clash_bounds_do_not_change_honest_outcomes`; `faction_ids_are_limited` |
| 1. Checked `duplicate_cost` | CL-03 | G0 | W1-B | on merge | `duplicate_cost_is_checked` |
| 1. `ProvinceCoord` ring bound | CL-04 | G0 | W1-B | on merge | `coordinates_are_bounded` (ring 128/129, i32 extremes, round trip) |
| 1. `Stamina::set` monotone | CL-05 | G0 | W1-B | on merge | `stamina_set_refuses_an_earlier_bell`, `stamina_spend_then_late_set` |
| 1. Conservation checks written as bounds | CL-07 | G0 | **W1-D** | `e60173a` | `frontier-sim` `a_small_season_conserves_money_and_laurels` with mutation controls (1 extra unit paid, one laurel credited twice, one Mandate unit paid twice, a claim above 5×) |
| 1. A per-faction Ledger | CL-08 | G0 | W1-B (kernel `FactionLedger`/`SeasonLedger`), **W1-D** (simulator side) | kernel on merge; sim `e60173a` | kernel `ledger_is_per_faction`; sim `the_ledger_is_per_faction` |
| 2. `PayoutParams::validate` refuses `office_ceiling_bps > 10,000` | CL-11 | G0 | W1-B | on merge | `office_ceiling_above_paid_is_refused` |
| 2. Remove `CitizenRecord::weights()` | CL-12 | G0 | W1-B (kernel); **W1-D** (callers) | kernel on merge; the simulator already calls `weights_at` everywhere (checked at `e60173a`, no change needed) | compile check; `weights_follow_the_season_rate` |
| 2. Closed-form `accrual_left` | CL-13 | G2 (done in week 1) | W1-B | on merge | `accrual_left_closed_form_matches_the_loop` |
| 2. Bound the product of combat multipliers | CL-14 | G0 | W1-A | on merge | `damage_product_cannot_overflow`, `validate_table_refuses_a_product_above_the_bound` |
| 2. `criterion --first-seed`, held-out restatement | CL-16 | G0 (flag), GX (table) | **W1-D** | `e60173a` | `criterion_first_seed_changes_the_seeds`; held-out table in `m1/W1-D-NOTES.md` §3 (worst 0.985, seeds 30002–30004, D23 on) |
| 2. Mandate claim deadline and its order to the Reckoning | CL-15 | G2 (kernel and text in week 1) | W1-B (kernel); **W1-D** (text, DESIGN §8.10) | kernel on merge; text in W1-D's docs commit | `mandate_claims_close_before_the_reckoning`, `final_sweep_conserves` |
| 2. Season-1 sweep target | CL-17 | G2 | owner (O-M1-16) | — | open: recommendation in `DECISIONS.md` part D (escrow for a successor, fee pro-rata after 90 days) |
| 3. `round_at` → "first round at or after" (bell, ring, genesis) | CL-19 | G0 | W1-C (kernel `beacon`), **W1-D** (DESIGN §0.5, §8.5) | W1-C `1dfc96a`; text in W1-D's docs commit | `seed_rounds_round_up`; `clock-vectors-v1.json` |
| 3. `T(b)` rounding; roster and posture cutoffs | CL-20 | G0 | W1-C, **W1-D** (DESIGN §2.1) | W1-C `1dfc96a`; text in W1-D's docs commit | `tlock_round_is_at_or_after_bell_end`, `cutoffs_do_not_depend_on_round_phase` |
| 3. With-seed grammar and length bounds | CL-21 | G0 | W1-C, **W1-D** (DESIGN §8.2) | W1-C `1dfc96a`; text in W1-D's docs commit | `seed_strings_fit_32_bytes`, `seed_strings_are_injective`; `addr-vectors-v1.json` = SP-V2 `acct.rs` |
| 3. Minimum reveal tip in priority terms | CL-22 | G1(Depart) | W1-C (`fees::min_tip_lamports`), **W1-D** (DESIGN §6.2, §6.4) | W1-C `1dfc96a`; text in W1-D's docs commit | `min_tip_matches_priority`; Depart's `TipTooLow` is a wave-3 program test |
| 3. One-way reveal latch | CL-23 | G0 (layout) | W1-E (Reveal account list), W3-B (Reveal), **W1-D** (DESIGN §6.2, §8.3, §9.4) | layout on merge; text in W1-D's docs commit | program gate G4 `reveal_refused_after_first_gather` (wave 3) |
| 3. Genesis re-roll guard | CL-24 | G0 (layout) | W1-E (AnnounceSeason layout), W2-A (instruction), **W1-D** (DESIGN §8.5, §8.10) | layout on merge; text in W1-D's docs commit | `create_before_announcement_refused`, `genesis_round_fixed_by_announcement`, `season_id_single_use`, `pre_join_abort_forfeits_bond` (wave 2) |
| 3. Mark the 0.4-s slot rows [model] | CL-25 | G0 | **W1-D** | W1-D's docs commit | DESIGN §8.7, §8.9 tagged |
| 4. D18 pool sizing (whole-block attack delays every write) | CL-30, CL-31a | GX / G2 | **W1-D** | sim counts `e60173a`; table in W1-D's docs commit | `frontier-sim c4` v3 per-bell writes; lab `m1/lab/d18/d18-v3.txt`; DESIGN §6.4, §21.3; owner confirms (D18) |
| 4. 1,200-s windows; relic clashes at the relic tip | CL-26 | GX | **W1-D** (model v3; W6-E re-runs it with the measured Reveal) | W1-D's docs commit | lab `m1/lab/c4-v3/c4-model-v3.txt` |
| 4. Clock-drift label; lookup-table lock count | CL-27 | DOC | **W1-D** | W1-D's docs commit | `SPIKE-SP-FEE.md` correction header (also the loaded-data limit, I-45); DESIGN §8.5–§8.7 |
| 4. Seed Join's wallet RNG | CL-28 | GX | W2-B | wave 2 | Join budget test over 1,000 seeded wallets + an adversarial one |
| 4. 1,200-fill worst-case search on ResolveFromInputs | CL-29 | G1(RFI) | W1-A (lab `m1/lab/rfi-search/`) | on merge | worst RFI ≤ 340k CU, heap ≤ 28,672 B on SBF v2 (W1-A notes); full write-back over all fills is W4-A's |
| 5. Push and GitHub CI | CL-34, CL-35, CL-36 | GX | **W1-D** | `e60173a` (civ tests), W1-D's docs commit (CI jobs, run record) | run 36303403992 recorded (`DECISIONS.md` part F); the two legacy failures fixed (47/47 locally); M1 jobs defined; **no push** (O-M1-17) |

Tasks outside M0-FINAL §5 that the closeout added: CL-09 (vigil at a UTC midnight, W1-B, outcome-changing), CL-10 (cap recount and scouts, W1-A, failing-first), CL-18 (γ test, W1-B), CL-31 (D23 in the simulator: **W1-D** `e60173a`; kernel `office`: W1-C `1dfc96a`), CL-32/CL-33 (D22 and D24 measured, **W1-D**, `DECISIONS.md` part F), CL-37 (decisions log, **W1-D**), CL-38 (this file).

## 2. M0 exit, re-graded

| # | Exit criterion (DESIGN §12) | M0-FINAL | Now | Why |
|---|---|---|---|---|
| (a) | Spike numbers, CU rows on SBPF v2 | met | **met** | unchanged; the full Reveal is measured in M1 wave 3 |
| (b) | Stance and doctrine balance (proxy gate + controls; O5 band on ≥ 1,500 paired seasons) | met locally | **met locally, re-checked with D23** | proxy gate with its three controls re-run at W1-D (all rejected, calibrated harness); O5 band with D23 on: 6/6, largest gap 1.0 point on 1,500 paired seasons (`m1/W1-D-NOTES.md` §3). Outcome-changing kernel fixes of W1-A/W1-B (CL-09, CL-10, I-43) re-run it at the Gate W1 merge |
| (c) | C4 restated: pool cap 2.0, ≥ 150 rotating payers, on the valuation the owner accepts | partial | **met in the model on valuation (a)** (N2) | c4 v3 (CL-26); unverified until the M1 Reveal CU and the M4 soak; the default-tip shortfall stays a recorded result |
| (d) | β measured, γ set | met in simulation | met in simulation | unchanged |
| (e) | §9.4 kernel tests incl. quota fairness and seed margin | partial | **met on the merged wave-1 branch** once W1-A and W1-B land (kernel bounds) — **the simulator's conservation bounds and per-faction book are in** (W1-D) | the grade is final only after Gate W1 passes on `frontier/m1-integ` |
| — | GitHub CI green | not met | **not met (push not approved)** | the local equivalent is Gate W1; the two legacy failures are fixed locally; the M1 jobs are defined |

**M0 is closed when Gate W1 passes on the merged branch** with every G0 task above mapped to a merged commit (contract §12 pass conditions). Until the integrator fills the "on merge" cells, this table is the plan of record, not the result.
