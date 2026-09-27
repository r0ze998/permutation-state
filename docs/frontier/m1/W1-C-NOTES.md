# W1-C rules-shared: notes

Unit W1-C of wave 1 (M1 "First Bell"), branch `frontier/m1-W1-C`, cut from `frontier/m1-integ` at `d9d32ec`. Contract: M1-CONTRACT v1.1 §4.1, §5.1, §7, §10.1, §11 (W1-C row). Owner decisions of 2026-09-27 applied (O-M1-12 not approved: nothing was downloaded or installed; every vector uses recorded rounds).

## What landed

New pure kernels in `permutation-rules/src/frontier/` (no Solana types, integer only, `no_std` + `alloc`, saturating or checked arithmetic, no panics on any input):

| Module | Contents | CL / I |
|---|---|---|
| `beacon.rs` | `round_time`, `first_round_from`, `bell_start`, `bell_end`, `bell_at -> Option<u32>`, `day`, `tlock_round`, `reveal_close(a, w)`, `seed_round(c, close, Δ)`, `ring_seed_round`, `genesis_seed_round`, `genesis_ts`, `WindowSchedule` + `window`, `window_valid`, `window_change_allowed`, `reveal_open(c, now, A, W, Δ, latest)`. Re-exports `clash::{BeaconClock, QUICKNET}` (not moved). Every draw is "the first round at or after x" | CL-19, CL-20, I-18 |
| `addr.rs` | `SeedKind` (18 kinds incl. reserved `sv`, `po`), `SeedStr`, `seed(kind, raw) -> ([u8; 32], usize)`, `parse_seed`, one constructor per kind, `with_seed_address` (= sha256(base ‖ seed ‖ owner)), `citizen_tag15`, `keeper_tag8`, `citizen_tag`, `host_id` / `host_parts` | CL-21, I-02 |
| `seal.rs` | domains, `Plain` + `pack`/`unpack` (total, round-trips every 37-byte input), `validate`, path step codec (`path_step`, `set_path_step`, `encode_path`), `salt_of`, `commit`, `commit_with`, `ct_hash`, `seal_root`, `body_xor`, `seal_body`, `open_body`, `RETREAT_MAX_BPS` (see deviations) | I-06, I-27, I-28 |
| `fees.rs` | `cost`, `priority_milli`, `fee_for_priority`, `cu_price_micro`, `fee_of_price`, `loaded_limit`, `deploy_max_len`, `reveal_cost`, `min_tip_lamports`, `tip_priority_milli`, `Evidence`, `DefenceParams`, `defence_refund` | CL-22, I-08, I-45 |
| `office.rs` | `GovernanceParams { office_terms_per_wallet }` + `validate` (only 1 accepted: D23 decided), `may_stand`, `TermKind` + `counts_toward_limit` (caretaker term does not count), `seat_vacant` | CL-31 |
| `catalog.rs` | model.rs tables copied field for field (`BASE_PROD`, `tier_bonus_pct`, `BUILDINGS`, `build_secs`, `tier_up`, `TROOP_COST_PER_100`, `WALL_STEP`, `WALL_COST_STONE`, `SETTLER_COST`, `STARTER_KIT`, `WORKS_*`), `building(item, n, &Doctrine) -> Option<(Cost, Effect, u32)>`, `tier_up_item`, `train(unit, n) -> Option<Cost>` (immediate), `base_production`, `starter_kit`, `write_tables` | I-41, I-56 |
| `camp.rs` | `Camp { tile, troops }`, `place(ring_seed, p, terrain, day, has_holding, initial)`, `camp_tile_ok`, `below`, `loot() = 10` | I-56 |
| `explore.rs` | `Find { works }`, `roll(seed, p, tile, host, floor)`, `EXPLORE_FLOOR = 3` | I-56 |
| `mod.rs` | module list, `KERNEL_VERSIONS`, **`ruleset_hash_input()`** and `ruleset_hash()` (the `RULESET_HASH` input function) | §3.2 |

`RULESET_HASH` at this commit: `3c374846a12082ad946e7105f9c1478315fe1947d7065d08de05642a16a9c2dc` (golden in `ruleset_hash_binds_versions_and_catalog`; it binds the domain `PSF-RULESET-v1`, the nine kernel version constants, `clash::frontier_ruleset().hash()`, the catalog tables, the camp/explore constants, the seal limits and the office default).

Vectors in `permutation-rules/vectors/`:

| File | Producer | Freshness check |
|---|---|---|
| `clock-vectors-v1.json` | `tests/frontier_shared.rs::clock_vectors` (T(b), bell bounds, days over three drand phases incl. bell `u32::MAX`; S over windows 600/1,800 and margins 60/61/90; ring and genesis rounds; genesis_ts; window schedule) | `vectors_are_fresh` (rewrite with `PSF_WRITE_VECTORS=1`) |
| `addr-vectors-v1.json` | `tests/frontier_shared.rs::addr_vectors` (every kind's tag/raw/seed length; seeds at the extremes: i32::MIN/MAX coordinates, u32::MAX bells and days, u64::MAX host; citizen/keeper tags; host ids incl. the invalid ones; five full with-seed addresses for a fixed base and program) | `vectors_are_fresh` |
| `seal-vectors-v1.json` | `vectors/seal-vectors-gen.rs` (lab copy: `scratchpad/frontier/m1/lab/w1c-seal-vectors`, tlock `=0.0.10`, ark 0.6, built offline from the cargo cache with S-TLOCK rs-interop's lock) | `seal_vectors_match_the_kernel` (every non-pairing field recomputed by the kernel) |

Seal vectors: 17 cases over 4 recorded quicknet rounds (S-TLOCK q3 32556137 and q4 32556350, SP-V2 fixtures 32551361 and 32553016, so svm-tests can anchor them). `k` and `sigma` are derived from the q3/q4 commitments; the IBE block is tlock 0.0.10's `ibe::encrypt` with a fixed sigma, and **every untampered seal was reopened with the stock `tlock::decrypt` and the recorded signature**; the body and commitment are computed twice (independent `sha2` code in the generator, and the kernel) and compared. Classes: 5 `valid`, 6 `bad_plaintext` (retreat 60,001, version 2, reserved ≠ 0, stray path bits, stance 4, and S-TLOCK q4's recorded tlock-js seal, whose pre-3.1 plaintext layout fails `validate` with `Reserved`), 2 `commit_mismatch` (tampered body, forged commitment), 3 `fo_fail` / `bad_point_or_fo_fail` (wrong round, tampered U, V, W). Each valid case carries a `genesis_ts` for which `tlock_round(genesis_ts, arrive_bell) == round`. The generator output is deterministic (two runs byte-identical; file sha256 `fd8749f2f2fc063611871e25411c4b530ef5e6430d16ea6a94f1eef323dcbde9`). These are the G10 inputs for W2-A/W4-B and the interop inputs for W1-F's `fclient::seal`.

## Tests (contract §3.5; all in `permutation-rules`)

Unit tests next to each module (45 in `frontier::*` incl. the existing ones) and `tests/frontier_shared.rs` (20):

- CL-19 `seed_rounds_round_up` (ring, genesis and bell seed draws, 7,200 times × every phase of periods 3 and 30: `round_time ≥ t + 660` and `< t + 660 + period`, and the first such round); `spv2_seed_round_margin_vectors_pass` (SP-V2's `seed_round_margin` and `reveal_close_boundary` on the kernel fns); `beacon_seed_round_matches_the_clash_kernel` (unit: equals `clash::seed_round`/`reveal_open` at W = 600, Δ = 60).
- CL-20 `tlock_round_is_at_or_after_bell_end` (`0 ≤ round_time(T(b)) − bell_end(b) < period`, bells 0..4,032 and `u32::MAX/2`, three genesis_ts phases × all drand phases); `cutoffs_do_not_depend_on_round_phase`; `bell_at_agrees_with_travel` (unit).
- CL-21 `seed_strings_fit_32_bytes` (extremes + 10,000 random keys × 18 kinds); `seed_strings_are_injective` (1,000,000 random keys × 18 kinds parse back to exactly their raw bytes; fixed length per kind; distinct tags; a direct cross-kind collision map over 360,000 seeds); **`seeds_match_spv2_byte_for_byte`** (SP-V2 `acct.rs` transcribed verbatim; `an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa` equal on a grid of edge values and 200,000 random keys each); `host_id_round_trips` (unit).
- CL-22 `min_tip_matches_priority` (the 10,000-lamport tip at 16k/20k/26k CU with the 64-KiB limit gives 432/351/274 milli, i.e. DESIGN §8.7's 0.433/0.35/0.27 within the floor rounding of 1 milli; `min_tip_lamports` is the least tip reaching p, over 6 priorities × 4 limits × 3 loaded limits; 14,441 and 10,111 pinned); `fee_round_trips_and_loaded_limit`; `defence_refund_follows_the_formula`; `contract_values` (unit: 10,007 is the integer-ceil v1.0 figure, not 10,006).
- CL-31 `second_office_term_refused`.
- I-27/I-28 `validate_refuses_each_field`, `pack_round_trips_every_byte` (10,000 random inputs), `path_steps_round_trip`, `body_xor_is_an_involution_and_opens` (units); `retreat_encoding_is_pinned`; `seal_vectors_match_the_kernel`.
- I-56 `camps_are_deterministic_and_bounded` (tile passable, non-site, not the centre; troops 100..=400; spawn rate ½ over 3,000 province-days); `inner_rings_get_no_initial_camp`; `explore_rolls` (floor always 4; otherwise ½ over 4,000 rolls); `catalog_matches_the_simulator_tables`; catalog unit tests.
- `vectors_are_fresh`, `ruleset_hash_binds_versions_and_catalog`.

## Gate W1 items run on this branch (2026-09-27)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked -p permutation-rules --all-targets -- -D warnings` | exit 0 (`-p frontier-abi` does not exist on this branch; W1-E creates it) |
| `cargo test --locked --release -p permutation-rules` | all pass (lib 123, `frontier_shared` 20 in 3.1 s, every other suite unchanged) |
| `cargo test --locked -p permutation-rules --test frontier_shared` (debug, overflow checks on) | 20 pass in 36 s |
| `cargo test --locked -p permutation-chain` | all pass |
| `(cd frontier-sim && cargo fmt -- --check && cargo clippy --locked --release --all-targets -- -D warnings)` | exit 0 |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | clean (not touched) |

Not run by this unit (other units' files or integrator items): frontier-abi tests and `abi-vectors --check` (W1-E), frontier-sim tests/criterion/doctrine gate (W1-D; this unit changes no outcome), frontier-node (W1-F), gateway `npm test`, the civilization tests (W1-D). No item of this unit needs a download or install, so nothing is `PENDING-OWNER`.

## Deviations and contract errata

1. **`seal::RETREAT_MAX_BPS` is defined here, not re-exported.** The contract has W1-A define it in `clash.rs` and `seal` re-export it; both units start from the same base, where `clash.rs` has no such constant, so a `pub use` would not build. The value is `60_000` in both places. **Integrator:** after merging W1-A, replace the `pub const` in `seal.rs` with `pub use super::clash::RETREAT_MAX_BPS;` (one line; the ruleset hash does not change).
2. **`catalog` calls `holding::duplicate_cost` through a tiny `IntoChecked` adapter** that accepts both today's `u64` and CL-03's `Option<u64>` (W1-B), so this branch builds before and after W1-B's merge. **Integrator:** drop the adapter once W1-B is in (optional).
3. **`po` seed width (contract erratum).** §4.1's table lists PosturePDA with 14 raw bytes (a trailing zero, 30-byte seed); SP-V2's `posture_seed` uses 13 raw bytes (28 B), and I-02 plus §4.1's last bullet pin SP-V2 byte for byte. This unit follows SP-V2 (13 B). Suggested amendment: correct the §4.1 row to `P i32, Q i32, bell u32, pos u8 | 13 | 28`. (Reserved for M3; nothing in M1 derives it.)
4. **`Plain` has a `reserved: [u8; 3]` field** in addition to the pinned field list, so that `validate` can check "reserved bytes zero" on a `Plain` and `pack(unpack(b)) == b` holds for every input.
5. **`seal::validate` checks two syntax rules beyond §7's list:** every path step is a real direction (`< 6`, `PlainError::Direction`) and `dest_tile < 61` (`PlainError::Tile`). Both are plaintexts no honest client produces; without them a sealed out-of-range tile or direction would pass `validate` and reach the program as a "valid" order. Consequence: such a seal is a bad seal (code 5) at SettleTransit and `BadPlaintext` at Reveal. The web self-audit and `fclient` call the same function.
6. **`catalog::train` folds the simulator exactly:** with `k = ⌈n/100⌉`, food `60 k`, ore `⌊20 k·pc/6⌋`, gold `⌊10 k·pc/6⌋`. The sim charges the Spearman cost at purchase (l. 1535–1560) and a unit-variant surcharge on ore and gold only (l. 1790–1800); the contract's wording scales all three by `pc/6`. For the Spearman (`pc = 6`) both agree; for other units food differs (sim-exact here). Unit ids 0..=6 (Settler excluded, M2).
7. **`catalog::building`:** `n` is the copy number (≥ 1), as the sim passes it (`buildings[k] + 1`), so the first copy takes 5,400 s. Item 6 is walls (`ITEM_WALLS`); tier-up is `tier_up_item(tier)`. The kernel doctrine table has no science/food/ore multipliers (O5), so only `wall_cost` applies; the "doctrine's science bps" of §7 is neutral.
8. **`fees::Evidence.created_day` has no home in the ArrivalSlot layout** (§5.3: the slot stores `ev_slot`, `ev_price`, `ev_limit`, `ev_loaded`, `claimed`, but not whether the Reveal also created the ArrivalDay), while `defence_refund`'s cost is `300·(2 + created_day)`. ClaimDefence cannot recompute it from the slot. **Ask for W1-E before the ABI freeze at the end of wave 1:** e.g. flag bit 2 of ArrivalSlot `flags` (offset 29) = "this Reveal created the ArrivalDay", set by Reveal, reset on displacement with the other `ev_*`.
9. `addr::host_id` returns `HOST_ID_INVALID = u64::MAX` for a ring > 128 or a site ≥ 12 (no panic, no silent masking); `host_parts` refuses it. If W1-B's CL-04 changes `ProvinceCoord::index()`'s signature, `host_id` needs a one-line adjustment (it only calls `index()` on coordinates it has bounded itself, in i64).
10. `beacon` treats a drand period of 0 as 1 and saturates at the i64/u64 limits; `window_from_bell = u32::MAX` means "no change". `office::validate` accepts only `office_terms_per_wallet == 1` (SeasonParams validation, D23 decided).

## Dependency requests (integrator)

- **No manifest or lockfile change.** The rules crate uses only its existing `sha2`/`borsh`. The seal-vector generator is a lab tool outside every workspace (its manifest is quoted in the header of `vectors/seal-vectors-gen.rs`; tlock `=0.0.10`, ark `=0.6.0`, served from the local cargo cache).
- After W1-A: the `RETREAT_MAX_BPS` re-export swap (deviation 1). After W1-B: optionally drop the adapter (deviation 2).
- W1-E: `RULESET_HASH` = `permutation_rules::frontier::ruleset_hash()`; ArrivalSlot `created_day` bit (deviation 8).
- Contract amendment for the `po` row (deviation 3).

## Links

- Kernels: `permutation-rules/src/frontier/{beacon,addr,seal,fees,office,camp,explore,catalog,mod}.rs`
- Tests: `permutation-rules/tests/frontier_shared.rs`
- Vectors: `permutation-rules/vectors/{clock-vectors-v1,addr-vectors-v1,seal-vectors-v1}.json`, generator `permutation-rules/vectors/seal-vectors-gen.rs`
- Lab: `scratchpad/frontier/m1/lab/w1c-seal-vectors/` (session scratch)

## Integration window addendum (integ-W1, 2026-09-27)

Answered by the integrator (`integ-W1-NOTES.md` §C):
- `RULESET_HASH` now binds every frontier module's version (23), the doctrine and stance tables and `KERNEL_CONSTANTS`: **`1ac11f85fde3b898ebcd8c246964d9be2a29b4144a7a7ddfa81006999a6dd03f`** (the value above is superseded). frontier-abi embeds it as `presets::RULESET_HASH`.
- ArrivalSlot flags bit 2 = created_day (contract v1.2); `fees::fee_of_price` rounds up.
- `seal-vectors-v1.json` regenerated with 7 more bad-plaintext cases (tile, direction, path 33, host/arrive mismatch): 24 cases, sha256 `9b93a9fb89e4c254942a78037210228ec91642f311b55cfefa076e5b9e294887`; the first 17 are byte-identical. fclient consumes it.
- `addr::seed` refuses a wrong-length raw key (`try_seed`); the `IntoChecked` adapter is gone; the `po` erratum is amended (§4.1).
