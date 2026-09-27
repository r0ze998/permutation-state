# W1-A rules-clash: notes

- **Unit:** W1-A (wave 1), branch `frontier/m1-W1-A`, cut from `frontier/m1-integ` at `d9d32ec` (= `d95fa25` + the M1 plan docs).
- **Contract:** M1-CONTRACT v1.1 §11 (W1-A), §7 (clash rows), §2 (I-14, I-27, I-42, I-43), §12 (Gate W1), §13.1 (E1 ResolveFromInputs).
- **Date:** 2026-09-27. Nothing pushed; no devnet or mainnet; no service started; no port bound (LiteSVM only, in process).
- Tags: [measured] = run here; [sim] = frontier-sim; [design] = a choice made here.

## 1. What landed

| Item | Where | What |
|---|---|---|
| CL-01, CL-06 (clash part), I-27 | `clash.rs` `validate` / `check_fighter` | Refuses troops > `MAX_HOST_TROOPS` (hosts **and** garrisons, `TroopsAboveCap`), stamina > `STAMINA_CAP` (`StaminaAboveCap`), `dealt_bps` outside `[BPS_ONE, COMBAT_MAX_BPS]` (`BadMultiplier`), a retreat ratio > `RETREAT_MAX_BPS` (`BadRetreat`), a faction > `NEUTRAL` (`BadFaction`, so 7 is refused; array sizes keep `FACTION_LIMIT = 8`). `pub const RETREAT_MAX_BPS: u16 = 60_000` is defined in `clash.rs` (W1-C re-exports it from `seal`). `Relations::set_peaceful` is a no-op for ids ≥ `NEUTRAL`. |
| CL-14 | `clash.rs`, `doctrine.rs`, `stance.rs` | `MAX_STANCE_BPS` (computed from `stance::damage_bps` at compile time = 12,500) and `MAX_DAMAGE_PRODUCT_BPS = MAX_STANCE_BPS × COMBAT_MAX_BPS / BPS_ONE` (= 14,375); `const _: () = assert!` that `MAX_HOST_TROOPS × MAX_DAMAGE_PRODUCT_BPS ≤ u64::MAX` and that `u32::MAX × stance × doctrine` (each intermediate of `scale`) fits u64. `doctrine::validate_table` refuses a doctrine whose `max_dealt_bps()` (variant × drill × arrival in its best posture, new const fn) passes the bound: new `DoctrineError::DamageProduct`. The Season 1 table passes (max 11,025, C drilled and arriving). |
| CL-10 (a) | `clash.rs` step 2c, `readmit_free_slots` | Failing-first test failed at `d95fa25` (§3), so fixed. **Single pass**: once the hex fair share has bounced a host, the room is recounted from the hosts still in, and arrivals the room refused are re-admitted in mass order where the room **and their hex** have a free slot (fewer than 6 hosts; on a holding's hex, for an arrival hostile to the owner, fewer than 3 hostile). A re-admitted arrival never displaces a host the fair share admitted, so one pass reaches stable counts. See §4 D1 for why not a cascade. |
| CL-10 (b) | `clash.rs` step 5 and garrison results, `Unit::holds` | Failing-first test failed at `d95fa25`, so fixed. Civilians (Scout, Settler) never make a hex contested, never count in the field ranking (strength or the defender flag), and never count in `attackers_hold`, `holders` or `defender_present` (so a scout never advances or pauses a siege). When a hex is contested, a civilian withdraws if its faction ranked and lost, or if its faction only has civilians there and is hostile to a faction that holds. Civilians still do not engage (`fights()` unchanged). |
| I-43 | `clash.rs` `Occupancy`, `ClashInput.occupancy`, step 2a `admit_by_room` | `pub struct Occupancy { pub pending: [u8; 8], pub storage_free: u8 }`, `Occupancy::EMPTY` (no pending, `storage_free = Occupancy::STORAGE = 56`), `Default` = `EMPTY`. Step 2a seeds the per-faction and total counts with `pending` and admits an arrival only while `total < 48`, `per_faction < 8` and arrivals in `< storage_free`; the rest bounce with no loss in mass order. With `EMPTY` the outcome is bit-identical to `d95fa25` (golden digests, §3). `is_quiet` takes the same input (no arrivals: no effect). The kernel does not refuse an inconsistent `Occupancy` (a refusal would freeze the province-bell); over-full counts only bounce arrivals. |
| I-14 Phase A | `clash.rs` `resolve_clash` | The lab body of `m1/lab/clash-opt/phaseA.patch` (tile buckets, per-hex damage arrays, occupancy table and const neighbour table for the withdraw search, per-faction field key), carrying the rules above. |
| I-14 oracle | `clash.rs` `resolve_clash_ref` | The original step-by-step body with the same M1 rules; kept on host builds (§4 D2). Shared helpers (`validate`, `build_units`, `share_hex`, `admit_by_room`, `readmit_free_slots`, `scale`) are used by both bodies. |
| I-42 | `tests/frontier_clash_equiv.rs` | Lab equivalence promoted (4,320 inputs) + occupancy variant + the refund `d > 0` corner + a ties-and-edges family; mutation set re-run (§3.4). |
| CL-29 | lab `m1/lab/rfi-search/` | 40 SP-V2 fills + **all** 1,200 generated fills on SBF v2 (§3.3). |

Tests (all new, owned): `permutation-rules/tests/frontier_clash_bounds.rs` (9 tests: `clash_validate_refuses_out_of_range`, `faction_ids_are_limited`, `clash_bounds_do_not_change_honest_outcomes`, `damage_product_cannot_overflow`, `validate_table_refuses_a_product_above_the_bound`, `a_bounce_by_fair_share_frees_a_cap_slot`, `the_cap_recount_takes_only_free_hex_slots`, `scouts_do_not_contest_a_tile`, `pending_musters_and_departed_entries_bound_the_stays`), `permutation-rules/tests/frontier_clash_equiv.rs` (5 tests: `phase_a_equals_the_reference_on_4320_inputs`, `phase_a_equals_the_reference_with_occupancy`, `phase_a_equals_the_reference_on_the_refund_corner`, `phase_a_equals_the_reference_on_ties_and_edges`, `occupancy_empty_keeps_the_d95fa25_digests`), unit test `stance::the_stance_table_is_inside_the_damage_bound`.

## 2. Commits

1. `a15559d` Frontier M1 W1-A: clash bounds, cap recount, scouts, storage room, Phase A (owned files only).
2. `5c4df33` this notes file (and a later notes-only commit with the 1,500-season result).
3. **Build shim, separable** (`707e65d`) (see §5): `occupancy: Occupancy::EMPTY` in the 5 `ClashInput` literals of `permutation-rules/tests/frontier_world.rs` (W1-B's file) and the one in `frontier-sim/src/sim.rs` (W1-D's file), plus the import. Without it the branch's `frontier_world` test target and `frontier-sim` do not compile, because §7 pins the new field. The owners must write exactly this line; the integrator may keep the commit or revert it in favour of the owners' versions.

## 3. Measurements and gate evidence

### 3.1 Failing-first (CL-10, I-43) [measured]

Run against a scratch copy of `d95fa25`'s rules crate (`scratchpad/w1a/logs/cl10-failing-first-at-d95fa25.log`):
- `a_bounce_by_fair_share_frees_a_cap_slot` **FAILED** at `d95fa25` ("A2 takes the freed slot": A2 `Bounced`).
- `scouts_do_not_contest_a_tile` **FAILED** at `d95fa25` ("the scout does not contest": the scout `Bounced`).
- `pending_musters_and_departed_entries_bound_the_stays` could not be written against `d95fa25` (no way to pass the room); its "without the room it would stay" control asserts the old behaviour. Both CL-10 items are therefore **outcome changes**; I-43 changes outcomes only with pending or departed entries (none in frontier-sim, which passes `EMPTY`).

### 3.2 Digests [measured]

- **Phase A equivalence: 4,320 / 4,320 identical** (outcome and digest), with the M1 rules in both bodies: 341,558 engagements, 13,354 withdrawals, 134,148 bounces exercised, 0 errors. The port of the lab generator reproduces the lab's inputs exactly: on `d95fa25` it gives 335,180 engagements and 14,189 withdrawals, the lab's numbers.
- Also identical: the same 4,320 fills with a random storage room (thinned residents, random pending musters and departed entries; the storage bound `stays ≤ storage_free` asserted on every one), 5,000 tie-and-edge fills, 32 refund-corner bells.
- **`Occupancy::EMPTY` digests identical to `d95fa25`:** two goldens recorded by running the unmodified `d95fa25` kernel (harness in `scratchpad/w1a/base-sim/permutation-rules/tests/w1a_golden.rs`, log `logs/golden-at-d95fa25.log`): the 4,320 fills trimmed to 4 residents per faction and without Scouts (so no cap binds and no civilian is present: none of the new rules can apply) `c3946cb0…2790bd`, and 300 honest clashes (doctrine multipliers, caps not binding, every field inside its bound) `c2c75bd7…b554ba`. Both match on the W1-A head, through Phase A, the new bounds and `EMPTY`.
- **How much CL-10 moves adversarial outcomes:** 3,165 of the 4,320 lab fills change outcome against `d95fa25`; with every Scout replaced by a Spearman (recount alone) 1,413 do. These fills are cap-saturated (48 residents, 24 arrivals) and one unit in seven is a Scout, so this is the upper end; honest play rarely has both a binding cap and a fair-share bounce (§3.5).

### 3.3 CL-29 on SBF v2 (lab `m1/lab/rfi-search/`) [measured]

Build: `cargo-build-sbf 3.1.9`, platform-tools v1.52, `--arch v2`, `e_flags 2`; plain `9867934c…6e1f` (488,616 B). LiteSVM 0.16; SP-V2 write-back (the full M1 write-back is W4-A's). Every on-chain digest equals the native kernel's.

| Measure | Result | Gate |
|---|---|---|
| ResolveFromInputs (full gather), **all 1,240 fills** (40 SP-V2 + 1,200 generated) | worst **307,139 CU** (wide#306, 173 engagements); mean 239,021; tx 358 B | ≤ 340,000 **pass** (32.9k margin) |
| Heap peak, trace build, same 1,240 fills | max **18,856 B** | ≤ 28,672 **pass** |
| 40 SP-V2 fills, full gather | worst 285,895 CU (wide#4) vs the clash-opt Phase A lab 274,798: **+11.1k** (mean +13.3k per fill, range −2.7k..+37.7k) | — |
| 40 SP-V2 fills, hybrid ResolveClash (oracle path) | worst 306,681 CU, 1,150 B, 31 locks; heap (trace) 21,736 B | — |
| Gathers | ≤ 30,323 CU, 1,187 B, 32 locks (unchanged) | — |

Kernel steps (t5b, 40 fills, max / mean CU; Phase A lab in brackets): K1 validate+units 26,736 / 26,728 [25,577]; K2 retreat+caps 12,367 / 10,040 [9,790 mean]; K3 fair share + recount 31,012 / 23,591 [25,664 / 18,895]; K4 engagements 166,505 / 116,674 [164,234 / 110,259]; K6 field 41,158 / 31,925; per engagement 1,112–1,178. The CU growth is the new validation (+1.2k), the recount pass (+4.7k mean) and the arrivals it re-admits, which then fight (+6.4k mean in K4).

A first version of the recount (cascade: re-admit, re-share the hexes, repeat) measured worst **328,894 CU** over the same 1,240 fills (K3 up to 76.7k); it is superseded (§4 D1). Its results are kept in `rfi-search/results/cascade-recount-superseded/`.

**Risk for W4-A:** the M1 write-back adds to these numbers; program.md estimated ≈ +25k. 307.1k + 25k ≈ 332k leaves ≈ 8k under 340k [estimate]. W4-A should measure early.

### 3.4 Mutation set (I-42) [measured]

`scratchpad/w1a/mutate.py`: 16 mutants of the Phase A body only (the reference untouched), each run against `tests/frontier_clash_equiv.rs` (release). **14 of 16 caught** (`logs/mutation.log`). Survivors:
- `withdraw-mask-update` (drop `mask_at[nt] |= 1 << f` after a withdrawal): **equivalent**, as in the lab (the bit is already set: a host only withdraws to a hex where its faction is present).
- `refund-strict` (`d >= 10·tk` → `d > 10·tk`): reachable only when a faction's damage dealt is **exactly** ten times the damage taken on a hex; no generated fill hit it. Untested corner, recorded.
Caught, among others: the refund `d > 0` guard (by the new refund-corner test: the lab's survivor), the defender flag, the seeded field key and `attacker_exhausted` at exactly 20 stamina (all three only by the new ties-and-edges test; before it, 11 of 16), both CL-10 rules and the recount's tile bookkeeping. The lab's counts (5 of 7, 4 of 5) are superseded by this 14 of 16.

### 3.5 Doctrine proxy gate and criterion (CL-10 changed an outcome) [sim]

Run on the W1-A head (with the §2 shim in `sim.rs`, `Occupancy::EMPTY`) and on `d95fa25` (scratch copy), same machine, loaded by other units' runs. Logs: `scratchpad/w1a/{base,new}-doctrine-gate.txt`, `{base,new}-criterion.txt`.
- `doctrine-gate --controls`: **kernel table identical to `d95fa25` and to m0c** at printed precision (4 of 6 in band, largest win-rate gap 3.3 points, largest |Δ index| 0.072%, 180/180 conservation). Knight control (F −0.219%) and A-boost control (A +0.384%): identical, both still rejected. **Draft control changed** (A −2.061% → −1.331%, C −1.895% → −5.015%, E +8.163% → +8.921%) and is still rejected: the draft table's C (drill ×1.10 × arrival ×1.10 = ×1.21) passes `COMBAT_MAX_BPS`, so CL-01 refuses C's arriving Assault hosts and the simulator counts those clashes as errors. This is CL-01/CL-14 working as intended (the draft table fails `validate_table` with `DamageProduct` now); W1-D may want the simulator to refuse an invalid table up front instead of playing it.
- `criterion --best-response --seeds 3 --first-seed 30001 --gate`: **identical** to `d95fa25` (worst cell 0.980, passes; every cell equal).
- So CL-10 and I-43 change **no simulated season outcome** at printed precision: frontier-sim has no Scouts, and its caps rarely bind together with a fair-share bounce.
- Overnight item `doctrines … --seeds 250 … --gate`: identical to `d95fa25`, passes (§3.6).

### 3.6 1,500-season band [sim]

`doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate` (1,500 seasons), run on the W1-A head and on `d95fa25`: **exit 0 on both, per-doctrine table identical** (6 of 6 in the band, largest win-rate gap 1.1 points, largest |Δ index| 0.043%, 1,500/1,500 conservation). Logs `scratchpad/w1a/{new,base}-doctrines-1500.txt`.

### 3.7 Gate W1 items for these files [measured, on this branch with the shim]

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked -p permutation-rules --all-targets -- -D warnings` | pass (`-p frontier-abi` does not exist yet: W1-E) |
| `cargo test --locked --release -p permutation-rules` | pass (27 test binaries; debug run of the three frontier clash files also passes, with overflow checks) |
| `cargo test --locked -p permutation-chain` | pass |
| `(cd frontier-sim && cargo fmt -- --check && cargo clippy --locked --release --all-targets -- -D warnings && cargo test --locked --release)` | pass (11 tests, 314 s) |
| `(cd frontier-sim && cargo run --release -- criterion --best-response --seeds 3 --first-seed 30001 --gate)` | pass (0.980) |
| `(cd frontier-sim && cargo run --release -- doctrine-gate --controls)` | pass (exit 0; kernel table passes, all 3 controls rejected) |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | pass |
| Phase A equivalence 4,320/4,320; `EMPTY` digests = `d95fa25`; CL-29 ≤ 340,000 CU and ≤ 28,672 B | pass (§3.2, §3.3) |

Not run by this unit (other units' files): `frontier-abi`, `frontier-node`, `permutation-gateway`, civilization tests.

## 4. Deviations and decisions

- **D1 [design] CL-10 (a) recount is one pass into free hex slots, not a cascade.** The closeout text says "re-admit bounced-by-cap arrivals in mass order, until the counts are stable". A cascade (re-admit, re-share the hex, which may bounce someone, re-admit…) was implemented first: it doubled K3 (up to 76.7k CU) and pushed the RFI worst to 328.9k, 11k under the budget before the M1 write-back. It also let an arrival the caps had refused displace, through the hex share, a host (even a resident) the first share had admitted. The single pass admits exactly the arrivals the hex share would admit without bouncing anyone, so counts are stable after it; cost +4.7k mean. Both the reference and Phase A implement it; the tests pin it (`the_cap_recount_takes_only_free_hex_slots`). Outcome-changing either way; the gates in §3.5 were re-run on the final form.
- **D2 `resolve_clash_ref` cfg.** §7 says `#[cfg(any(test, feature = "std"))]`. An integration test in `tests/` builds the library without `cfg(test)` and `cargo test -p permutation-rules` does not enable `std`, so the promoted equivalence test could not see it. It is gated `#[cfg(any(test, feature = "std", not(any(target_os = "solana", target_arch = "wasm32"))))]`: present on host builds, absent from the SBF and WASM builds. The alternative is a self dev-dependency with `features = ["std"]` (integrator-owned manifest); not requested.
- **D3 Civilians and the siege inputs.** CL-10 (b) says scouts "never advance a siege". Applied to all three garrison-result fields (`attackers_hold`, `holders`, `defender_present`): a defending scout also does not pause a siege (DESIGN §6.1: scouts "do not count for holding the field or for siege progress"). DESIGN §6.1 also says scouts "explore and fight"; the kernel has never let civilians engage and this unit does not change that. **W1-D (docs):** reconcile the DESIGN sentence.
- **D4** `validate` checks the retreat bound on residents too (the field is ignored for residents); `retreat_bps = Some(0)` stays accepted by the kernel (the plaintext's 0 = "never" maps to `None` in `seal`, W1-C).
- **D5** `Relations::set_peaceful` now also ignores ids ≥ `NEUTRAL` (was ≥ `FACTION_LIMIT`), so faction 7 can never be made peaceful.
- **D6** No kernel refusal of an inconsistent `Occupancy` (e.g. `pending` summing past 48): an over-full room only bounces arrivals, which is safe; a refusal would freeze the province-bell.

## 5. Requests

- **Integrator / W1-B / W1-D (cross-edit, §2 commit 3):** `permutation-rules/tests/frontier_world.rs` (W1-B) needs `use permutation_rules::frontier::clash::Occupancy;` and `occupancy: Occupancy::EMPTY,` in its 5 `ClashInput` literals; `frontier-sim/src/sim.rs` (W1-D) needs `Occupancy` in the `clash` import and `occupancy: Occupancy::EMPTY,` in its one `ClashInput` literal (the simulator has no musters or departures in flight). Any new `ClashInput` literal in either unit's work needs the same line.
- **W1-C:** `seal` re-exports `clash::RETREAT_MAX_BPS` (`u16`, 60,000) as pinned.
- **W1-D:** record in `M0-CLOSE.md`: CL-01, CL-06 (clash part), CL-14 → commit `a15559d`; CL-10 fixed (outcome change, gates re-run, §3.5); CL-29 measured (§3.3). The simulator plays the draft table even though `validate_table` now refuses it (§3.5).
- **W4-A:** pass the real `Occupancy` from the Province entries; measure RFI with the M1 write-back early (§3.3 risk); the storage fill of §13.1 is exercised natively in `phase_a_equals_the_reference_with_occupancy`.
- **Dependencies:** none. No manifest, lockfile or toolchain change.

## 6. Pending / not done

- PENDING-OWNER: nothing in this unit needs O-M1-12 (no wasm32, Playwright, drand archive or Agave).
- `refund-strict` mutant survives (exact 10× damage ratio untested).

## 7. Links

- Code: `permutation-rules/src/frontier/{clash,doctrine,stance}.rs`; tests `permutation-rules/tests/frontier_clash_{bounds,equiv}.rs`.
- Lab: `scratchpad/frontier/m1/lab/rfi-search/` (`README.md`, `RESULTS.md`, `results/t5d_rfi_all_fills-v2.json`, `results/t5_gather_resolve-v2.json`, `results/t5b_kernel_breakdown-v2.json`, `logs/run-w1a.log`, `run.sh`).
- Session scratch: `scratchpad/w1a/` (`base-sim/` = `d95fa25` copy with the golden and failing-first harnesses, `mutate.py`, `logs/`, sim outputs).

## 8. Integration window addendum (integ-W1, 2026-09-27)

The wave-1 review of this unit was answered by the integrator (`integ-W1-NOTES.md` §A):
- **Garrison cap (major):** option (a). `host::GarrisonState` never exceeds `MAX_HOST_TROOPS` (refused past `room()`, clamped on settle), so a Garrison top-up can no longer freeze a province; `clash::validate` keeps the CL-01 refusal. The note above that "an honest program never builds" such input now holds by construction (test `a_garrison_at_the_cap_resolves_and_cannot_pass_it`).
- `clash::valid_faction` private copy + `clash_faction_rule_equals_geometry`; the occupancy test asserts the I-43 caps; the stance test pins 12,500 / 14,375; the refund rule is one `const fn` and `the_refund_starts_at_exactly_ten_times` kills the `refund-strict` mutant (both bodies call the one rule; `mutate.py` was not re-run; the other survivor, `withdraw-mask-update`, is equivalent).
- Dependency request resolved: `CLASH_VERSION = 2` is in `KERNEL_VERSIONS` (W1-C's file, by the integrator). frontier-abi takes `ENTRIES_N` from `Occupancy::STORAGE`.
- 707e65d (the shim in W1-B/W1-D files) was accepted under the integration window (Gate W1 record).
- D1 and D3 are in DECISIONS part G (G2, G3); DESIGN §6.1 reconciled. The `resolve_clash_ref` cfg superset stays (harmless; no manifest change).
