# permutation-rules

Deterministic rules engine for PERMUTATION STATE. It implements [Rules Specification v0.1](../PERMUTATION_STATE_RULES_SPEC_v0.1.md) for [Game Design V4](../PERMUTATION_STATE_GAME_DESIGN_V4.md).

A single crate runs everywhere the rules run:

| Consumer | Status |
|---|---|
| MagicBlock ER program (native Rust) | builds for SBF (`cargo-build-sbf`) ✅ |
| Replay verifier CLI | not yet written; will call `genesis::new_season` + `tick::resolve_tick` |
| Browser forecast / agent `simulate()` via WASM | not yet built (`wasm32-unknown-unknown` target not installed here) |

## Properties

- **`no_std` + `alloc`, no floating point, `#![forbid(unsafe_code)]`.** Resources are milli-units, troops are milli-troops, multipliers are basis points (§0.1).
- **Deterministic.** Iteration is by ascending id. Randomness comes from `sha256(seed_t ‖ domain ‖ id)`, where `seed_t` includes the tick's VRF output (§0.2).
- **One ruleset, two clocks.** `Ruleset::new(Preset::Season | Preset::Blitz)` differ only in tick length, civ count, map radius and entry window. A test enforces this.
- **Hashed rules.** `Ruleset::hash()` covers every parameter and every static table (units, techs, buildings, terrain).
- **Resumable ticks.** `run_phase` advances `phase_cursor`, so a tick can be split across transactions and finished by anyone (§15.1–15.2). `resolve_tick` runs all remaining phases.
- **Borsh-serializable state.** `WorldState::state_root()` = `sha256(borsh(state))`.

## Layout

| Module | Spec | Contents |
|---|---|---|
| `fixed`, `hex`, `rng` | §0 | numeric types, axial hexes, seeded randomness and tie-breaks |
| `params` | §1 | `Ruleset`, presets, protection schedule, `ruleset_hash` |
| `map` | §2 | terrain table, tiles, deterministic generation with fair starts, territory |
| `genesis` | §3 | `new_season`: capitals, starting units, city-states, hubs |
| `orders` | §4 | `Order`, `OrderBatch`, costs, `validate_batch`, order bank |
| `economy` | §5–6 | growth threshold, upkeep, amenities, tech cost, governor yields |
| `tech`, `buildings`, `units` | §5.6, §6.2, §7 | static tables |
| `combat` | §8 | `F` table, `damage`, `resolve_engagement` with modifiers in spec order |
| `scoring` | §14 | Dominion, Concord, Science key, winners count, payout weights |
| `tick` | §15 | phase pipeline |
| `invariants` | §17 | state and monotonicity checks |

## Implementation status of `resolve_tick`

| # | Phase | Status |
|---|---|---|
| 0 | Seed | ✅ |
| 1 | Diplomacy | `DeclareWar` with casus belli, grievance, aggressor flag, loss of protection; NAP expiry. **TODO:** peace, NAPs, alliances |
| 2 | Economy orders | queue (tech, star-gate and duplicate checks), focus, research, purchase. **TODO:** transfers, envoys, gold AMM, Exchange |
| 3 | Standing rules | **TODO** |
| 4 | Movement | ✅ paths, MP, one free tile at full MP, occupancy, foreign territory, protected zones, tie-break, no swapping |
| 5 | Combat | **TODO:** wire `combat::resolve_engagement` into engagements, captures, raze. The damage math is done and matches every spec test vector |
| 6 | Production and growth | ✅ governor, amenities, growth and starvation, territory, queue completion and spawning, strategic reserves, research |
| 7 | Upkeep | ✅ including deficit disbanding |
| 8 | Society | ✅ war weariness, loyalty and Free Cities, grievance decay, city regeneration. **TODO:** casualties term |
| 9 | Neutral actors | city-state growth and regeneration, suzerainty cycle reset. **TODO:** envoys, the Crisis |
| 10 | Scoring | ✅ Dominion, Concord, science, alliance record, activity, order bank. **TODO:** prize coalitions |
| 11 | Commit | ✅ next tick's budget, event chain |

## Development

```sh
cargo test          # 47 tests: unit, spec reference vectors, full 180-tick season
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo-build-sbf     # Solana SBF build check (Solana CLI)
```

The toolchain is pinned to Rust 1.89.0, with `borsh =1.6.1`, the same versions as `permutation-state-solana-receipt-spike`.
