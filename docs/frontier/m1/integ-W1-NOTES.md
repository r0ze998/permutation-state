# Integration window W1: response to the wave-1 review

- **Date:** 2026-09-27. **Role:** integrator, integration window of wave 1 (contract §3.4). **Branch:** `frontier/m1-integ`, from `bf7fb33`.
- **Contract:** `M1-CONTRACT.md` v1.2. The amendments are listed in §18. The decisions are in `docs/frontier/DECISIONS.md` part G. The unit notes W1-A to W1-F each end with a short addendum.
- **Lab material** is under `(session scratch)/scratchpad/frontier/m1/integ-w1r/`:
  - `seal-vectors/`: the seal-vector generator with the new cases.
  - `gate-cal/`: the doctrine-gate calibration.
  - `sim/`: the simulator gate re-runs.
  - `rfi/`: the CL-29 SBF v2 screen on the integ kernel.
  - `gate/`: the final Gate W1 run.

Every review item was checked against the code. Each one is either fixed with a test, or answered in one line with the evidence. "Fixed" names the test that pins the fix.

## A. W1-A rules-clash

| Item | Verdict | Disposition |
|---|---|---|
| Garrison above `MAX_HOST_TROOPS` freezes a province (major) | **Confirmed** | Fixed with option (a). `GarrisonState::change` refuses a delta past `room()`, and `settle`, `apply_clash` and `new` clamp. `validate` keeps the CL-01 refusal. Test: `a_garrison_at_the_cap_resolves_and_cannot_pass_it`, which covers the reviewer's sequence, `is_quiet` at the cap, and a garrison at cap + 1 still refused by the clash. |
| Occupancy test pins only the storage bound | Confirmed | Fixed. The per-faction and total cap asserts were added. The generator can hand in factions that are already over the caps, so for those the assert is that "arrivals add nothing"; 1,670 of 4,320 cases are fully inside the caps. |
| Stance test is a tautology | Confirmed | Fixed: pins 12,500 and 14,375 by value. |
| No private `valid_faction` and no equality test (§7) | Confirmed | Fixed: `clash::valid_faction` plus `clash_faction_rule_equals_geometry`, which checks every `u8` through `validate` (resident, arrival, garrison) and `set_peaceful`. |
| No version input moved for CL-10/I-43 | Confirmed | Fixed in §C: `CLASH_VERSION = 2`. |
| CL-10 re-run uninformative; D1/D3 not logged; DESIGN l.536 | Partly | D1 and D3 are logged (G2, G3), and DESIGN §6.1 is reconciled. The draft control is **not** rejected because of kernel refusals: its rejection comes from E Lumen at +8.77%, 44× the bound (`gate-cal/draft-12.md`). A scout/binding-cap sim scenario is **not added**; it is left for W5-A's balance work. |
| `ENTRIES_N` hand-copied | Confirmed | Fixed. `ENTRIES_N`, `ROSTER_CAP` and `FACTION_CAP` are now taken from the kernel, and const asserts tie the literal x12/x56 layout to it. |
| 707e65d outside the unit's paths | Confirmed | Recorded as accepted under the integration window (Gate W1 record, M0-CLOSE §3). |
| Missing: refund-strict corner | Confirmed | Fixed. The refund rule is now one `const fn` that both bodies call; `the_refund_starts_at_exactly_ten_times` kills the `>=`→`>` mutant. `mutate.py` was not re-run. |
| Missing: `resolve_clash_ref` cfg superset | Kept | Harmless: it still excludes SBF and WASM. No manifest change was made. |
| Missing: CL-29 not re-measured | **Re-measured** | Run on the integ kernel (all integ-W1 fixes; `rfi/`), 1,240 fills on SBF v2: worst ResolveFromInputs **307,546 CU** (W1-A: 307,139; wide#306), heap **18,856 B**, tx 358 B. Every on-chain digest equals the native kernel's. The gate passes: ≤ 340,000 CU and ≤ 28,672 B. |

## B. W1-B

| Item | Verdict | Disposition |
|---|---|---|
| CL-09 still allows a 16-hour vigil at bell granularity (major) | **Confirmed** | Fixed with the day rule: the first new window is skipped unless `ws ≥ prev + DAY`. `bells_outside_vigil` now also cuts at `from + 2 DAY`; without that cut the bell-by-bell test fails (case 52). Test `no_vigil_covers_more_than_48_bells_across_a_change`: every pair on a 30-minute grid, plus a 60-s grid around both edges for every old start, genesis offset 7 min 13 s, kernel == model. Both controls give 96 covered bells: the naive midnight rule, and the first rule on 16:00→00:01 and 15:59→00:00. `bells_outside_vigil_matches_bell_by_bell_across_a_change` now runs 3,000 cases. |
| Caps pinned from sim peaks; walls refusal unmapped | Confirmed | Documented rather than changed: W1-B notes addendum, DECISIONS G5, contract §5.10 (`AboveCap` → `Kernel` 15). |
| Unchecked `index` / `from_index` | Confirmed | Fixed: both are total. Beyond ring 128, `index` gives `u32::MAX` and `from_index` gives the Concord (and asserts in debug). `addr` uses the checked forms. Tests: `from_index_beyond_ring_128_is_refused` and the extremes in `province_coordinates_are_checked`. |
| `valid_faction` doc names the wrong test | Confirmed | Fixed: the doc now points at the new test (§A). |
| `final_sweep` with an open term | Confirmed | Fixed: `final_sweep(open_terms)` refuses while `open_terms > 0`. The `term_end` precondition is documented. Tested in `mandate_claims_close_before_the_reckoning`. |
| `troop_upkeep_per_hour` can wrap | Confirmed | Fixed: it saturates. Tested with 1,000 max-size entries and with 100,000 × `u32::MAX` troops. |
| No version decision | Confirmed | Fixed (G6). |
| Missing: CL-13 SBF CU constant in day | Not measured | Left to W4-A, which measures SkipQuiet on SBF. |
| Missing: criterion not re-run for CL-09 | Rebutted | frontier-sim never calls `Vigil::request_change`, so the criterion cannot see CL-09 either way. It was re-run anyway for the §D changes (below). |

## C. W1-C

| Item | Verdict | Disposition |
|---|---|---|
| RULESET_HASH blind to most rules (major) | **Confirmed** | Fixed: 23 version entries (one per module; a test parses `pub mod` lines), plus the doctrine table, the stance table and `KERNEL_CONSTANTS`. The hash is now `1ac11f85fde3b898ebcd8c246964d9be2a29b4144a7a7ddfa81006999a6dd03f`. It is embedded as `frontier_abi::presets::RULESET_HASH` with `ruleset_hash_is_the_kernels`, and published in `presets.json`. |
| ArrivalSlot has no `created_day` (major) | **Confirmed** | Fixed by amendment: flags bit 2, `arrival_slot::{FLAG_CREATED_DAY, evidence}`. No offset moves, since the bit goes into the existing flags byte. |
| Seal vectors miss Tile/Direction/PathTooLong/Host/Arrive (major) | **Confirmed** | Fixed: 7 new cases generated with the lab generator. It runs offline, and stock `tlock::decrypt` reopens every untampered seal. File sha256 `9b93a9fb…4887`; the first 17 cases are byte-identical, and a re-run of the old generator reproduced `fd8749f2…` first. The kernel test now requires a vector for every `PlainError`, and fclient consumes the file (§F). I-28 is amended. |
| `addr::seed` accepts a wrong length | Confirmed | Fixed: it now returns `(buf, 0)`; new `try_seed`. |
| `IntoChecked` adapter | Confirmed | Removed. |
| §4.1 `po` row | Confirmed | Amended to 13 / 28. |
| Definition-pin tests | Confirmed | Labelled as definition pins; the program-level CL-20 test is assigned to W2-A/W3. |
| Missing: no consumer of the vector files | Confirmed | Fixed: fclient now tests the seal, clock and addr vectors. frontier-abi's addresses stay checked against SP-V2 byte for byte, now through the kernel's builders. |

## D. W1-D

| Item | Verdict | Disposition |
|---|---|---|
| Doctrine proxy gate pinned to a non-shipping economy (major) | **Confirmed** | Fixed by re-calibration. `gate_config()` is now the default config (D23 on), and `GATE_SEEDS` goes 30 → 60 with the bound unchanged. Results at this head [sim]: kernel largest \|Δ\| **0.063%**; controls **Knight −0.294%**, **A boost +0.338%**, **draft +8.77%** (12 seeds), all rejected. `doctrine-gate --controls` exits 0 with all three rejected, and the four cargo tests pass. Cost: the three gate tests take 379 s of `cargo test --release` on this machine; CI will need a longer time limit (not measured on a runner). |
| Simulator counts the caretaker term (major) | **Confirmed** | Fixed: a per-agent `limit_terms` via `office::counts_toward_limit`. Re-runs [sim]: criterion 201–203 worst **0.979**, held-out 30002–30004 **0.985**, the same worst cells as before. Bots' office-terms in the default mix are 16% at 1% bots and 80% at 10% (W1-D, counting the caretaker term: 9% / 71–72%). O5 band on 1,500 paired seasons: **6/6, largest gap 1.1 points, largest \|Δ\| 0.069%, 1,500/1,500 conserve**, 500 s. DESIGN §21.4 is updated. The D22/D24 variants were not re-run. |
| `catalog_equality` can silently no-op (major) | **Confirmed** | Fixed: `build.rs` removed and the test compiles unconditionally. |
| CL-07 entitlement bound cannot fail in the sim | Confirmed, partly fixed | The per-faction pot draws **are** enforced on the settlement path by the kernel `SeasonLedger::record(..).expect(..)`. An independent per-wallet entitlement needs a second implementation, which is the verifier's V13 (W4-D). This is recorded, not changed. |
| `FactionBook` ignores swept | Confirmed | Fixed: it now draws paid + swept, as the kernel does. |
| CI steps differ from §12; missing overflow-checks request | Confirmed | CI is fixed. The profile request is recorded in the W1-D addendum; it is applied when `permutation-frontier` exists (W2-A), because a profile for a non-member package only warns. |
| `catalog_equality` coverage gaps | Confirmed | Fixed: `n` not a multiple of 100, `n = 0`, no item outside the catalog up to `u8::MAX`, and walls for several copies. The train-formula text in §7 follows the sim, as W1-C and W1-D noted; no further amendment. |
| core.test described as a stale count | Confirmed | Corrected in DECISIONS F2. |
| c4 usage text / DESIGN §5.4 | Confirmed | The usage text now points to the lab scripts. The held-out criterion row stays in §21.4, which §5.4 references. |
| Missing: c4 v3 caveats | Kept open | Recorded for W6-E: posture reveals ≈ 0 and reveals ≈ 25 per bell are activity-model artefacts. |

## E. W1-E

| Item | Verdict | Disposition |
|---|---|---|
| Prologue order (major) | **Confirmed** | Fixed (§5.6 amended as well). Tests: a non-running season with the wrong ruleset gives `WrongStatus`; an announced season gives `WrongStatus`; an empty bucket after `end_bell` gives `Bucket`; the keeper prologue orders its checks the same way. |
| Address grammar twice (major) | **Confirmed** | Fixed: frontier-abi's builders, tags and host-id codec delegate to the kernel; `tags_are_the_kernels`; `loaded_limit_for` calls `fees::loaded_limit`. The ProgramData's 64-B overhead was missing from `loaded_need` (+64 in budgets.json; every `loaded_limit` is unchanged). |
| FoldOccupancy part 1 over the packet (major) | **Confirmed** | Fixed with a three-part fold (§5.9 amended). Every part fits the packet; fclient's "every builder fits a packet" test now expects no oversize shape. |
| `chains_of` CLOSE panics | Confirmed | Fixed: checked reads, plus fuzzing of every kind with arbitrary keys. |
| drand genesis/period unchecked | Confirmed | Fixed: both must equal quicknet's. |
| `payout_borsh` hand-encoded | Confirmed | Fixed: it now uses `PayoutParams::to_borsh` (vectors unchanged). |
| overflow-checks for frontier-abi | Partly | `bell_at` now uses `checked_sub`. The profile entry is left for the integrator at W2-A, together with the program's. |
| `Auth` vs `BadAccount` | Confirmed | Fixed: signatures give `Auth`, writability gives `BadAccount`. |
| Offsets test transcribes 63 of about 250 offsets | Confirmed, not done | The review compared every layout and found no mismatch. A text-driven test is left for W2-A. |
| `WHOLE_ACCOUNT_REFUND_FLOOR` | Confirmed | Now `rent(0)` (erratum §4.2); every kind's rent clears it (`rent_is_the_sysvar_formula`). |
| Missing: RULESET_HASH, equality test, amendments | Done | §C and §18; DECISIONS G8–G10. |

## F. W1-F

| Item | Verdict | Disposition |
|---|---|---|
| Clippy fails on 1.95.0 (major) | **Confirmed** | Fixed. The blockhash fix is feature-independent: `solana_hash::Hash` is `Copy` only when the workspace enables the `copy` feature, so `-p fclient` alone failed with the reviewer's suggestion. |
| fclient seal lacks the tile check (major) | **Confirmed** | Fixed: fclient now re-exports the kernel's seal rules; the kernel vectors are judged with the stock opener. |
| `fclient::abi` untied to frontier-abi | Confirmed | Fixed: dev-dependency and twin tests. Error names now agree; the gateway `frontier-vectors.json` was regenerated. |
| `defence_refund` rounding | Confirmed | Fixed: the kernel now rounds up (G7); 200,000-case equality test. |
| Clock and host-id edges | Confirmed | Fixed: fclient re-exports the kernel's functions; tests run over the clock and addr vectors. |
| check-ports misses wildcard/IPv6 | Confirmed | Fixed with the `lsof` rule, which never does a wildcard bind. The test binds loopback only (v4 and v6). |
| localnet control is circular | Confirmed | Fixed: the independent 602,471-B total is asserted. The absent-account accounting stays open for the W2-B validator drill. |
| drand-replay extrapolates | Confirmed | Fixed: it uses the last observed Clock; test `chain_clock_never_extrapolates`. |
| ix mismatches | Confirmed | ResolveClash is now in frontier-abi's shape. SettleTicket gets `seedcache\|archive` in the ABI and contract. `Budgets::from_json` reads budgets.json. The builder-vs-ABI bounds test also caught fclient's CreateSeason test data (256 → 224 B). |
| Observation: rustfmt/clippy for 1.95.0 appeared during the review | Noted | Not installed by this session either. They are present now, so the frontier-node fmt/clippy gate line runs **as written** and passes. **The owner should know that someone installed them.** |

## G. Gate W1 re-run

See M0-CLOSE.md §3 (integ-W1 re-run) for the item-by-item results, and `(session scratch)/…/integ-w1r/gate/` for the logs.
