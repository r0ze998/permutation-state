# permutation-rules

Deterministic rules engine for Wylls. It implements [Rules Specification v0.2](../PERMUTATION_STATE_RULES_SPEC_v0.2.md) for [Game Design V5](../PERMUTATION_STATE_GAME_DESIGN_V5.md): six nations in one hex world, members who elect officers, four paths of achievements, merit and the prize settlement, the USDC market, and (rules version 7, V5 §18) treasury contracts and the operator's hidden AI members with home-city bounties. The current rules version is 8 (flatter milestone points, V5 §18.13).

One crate runs everywhere the rules run, so a tick resolved in one place is byte-identical to the same tick resolved in any other:

| Consumer | Where |
|---|---|
| The Solana program on the MagicBlock Ephemeral Rollup | `../permutation-chain` (built for SBF with `cargo build-sbf`) |
| The game server, the hosted AI members, the season simulation | `../permutation-server` (`play`, `sim`) |
| The replay verifier | `../permutation-server` (`verify`) |

## Properties

- **`no_std` + `alloc`, no floating point.** `#![deny(unsafe_code)]`, with one exception: the `sol_sha256` syscall in `hash` on SBF. Resources are milli-units, troops are milli-troops, and multipliers are basis points (§0.1).
- **Perfect information.** Every account is public on chain, so the rules make no attempt to hide state; every player decides from the full world. What stays hidden until the deadline is an officer's sealed batch: only `sha256("permutation-rules/orders" ‖ borsh(OrderBatch) ‖ salt32)` is sent (`orders`), and a batch that is not revealed does not run.
- **Rotation-equivariant.** Maps are six copies of one sextant turned by 60°, and tie-breaks are turned with the sextant, so a season replayed in the world turned by 60° with turned orders gives identical scores (`../permutation-server/tests/symmetry.rs`).
- **Deterministic.** Iteration is by ascending id. Randomness comes from `sha256(seed_t ‖ domain ‖ id)`, where `seed_t` mixes the season seed with the tick's randomness (§0.2). The tick's randomness is supplied by the chain (the MagicBlock VRF output drawn after the tick's input froze, mixed with the frozen order salts: `permutation_chain::randomness`) and is opaque to the rules.
- **One ruleset, two clocks.** `Ruleset::new(Preset::Season | Preset::Blitz)` differ only in tick length, civ count, map radius and entry window.
- **Hashed rules.** `Ruleset::hash()` covers the parameters in `Ruleset` and the static tables (units, techs, buildings, terrain). Its value is stored in the world, so it is part of every state root. Some constants are still inline in the code; see "Known debt" below.
- **Resumable ticks.** `run_phase` advances `phase_cursor`, so a tick can be split across transactions and finished by anyone (§15.1–15.2). `resolve_tick` runs all remaining phases.
- **Borsh-serialized state.** `WorldState::state_root()` = `sha256(borsh(state))`. Renaming a field or type is safe. Adding, removing, reordering or retyping a field of anything in `WorldState` changes every root. So does changing a `Blocked` code (codes are recorded in `last_skipped`).

## Layout

| Module | Spec | Contents |
|---|---|---|
| `fixed`, `hex`, `rng`, `hash` | §0 | numeric types, axial hexes, seeded randomness and tie-breaks, SHA-256 |
| `params` | §1 | `Ruleset`, presets, protection schedule, office budget split, tariff table, `ruleset_hash` |
| `map`, `mapgen` | §2 | terrain table, tiles, territory; `mapgen`: six-fold rotationally symmetric maps from `map_seed(world_seed, season_seed)` (ridges with two passes per border, a city-state in each outer pass, the trade hub at the centre, rivers running downhill) |
| `genesis` | §3 | `new_season`: capitals (starts shuffled by the season seed), starting units, city-states, hubs; `nation_entries` |
| `state` | — | `WorldState` and everything in it, tile and territory lookups |
| `orders` | §4 | `Order`, `OrderBatch`, the order commitment, costs, which office may issue what, `validate_batch`, per-office banks |
| `checks` | §4.4 and throughout | shared validity checks with typed reasons (`Blocked`), used by resolution and previews alike |
| `gov` | §14.6 | members and offices; `actions` (phase 0: stand, vote, propose, support, recall), `terms` (phase 11: recalls and elections), `caretaker` (fills a vacant office each tick from the members' most-supported open proposal, else a minimal default; the operator never gives orders) |
| `tick` | §15 | the phase pipeline; one module per phase group: `intake` (0), `city_orders` (2), `production` (6), `society` (7–9), `milestones` (10) |
| `diplomacy` | §10 | phase 1: war, peace, NAPs, alliances, offers |
| `trade` | §10.5 | transfers between nations (phase 2) and the trade accounting behind Concord and merit |
| `envoys` | §12.1 | city-state envoys and suzerainty (phase 2) |
| `markets` | §11 | `amm`: the gold AMM (`clear_amm`); `usdc`: the USDC market (`call_auction`, tariff, delivery) |
| `standing` | §13 | phase 3: AutoDefend, Retreat, Patrol, AutoPurchase |
| `movement` | §7.3 | phase 4, plus the path planner that standing rules, previews and agents use |
| `battle` | §8, §9, §12.1 | phase 5: `plan`, `damage`, `capture`, `raze` |
| `combat` | §8 | the `F` table, `damage`, `resolve_engagement` with modifiers in spec order |
| `economy` | §5–6 | growth threshold, upkeep, amenities, tech cost, order budget, yields |
| `tech`, `buildings`, `units` | §5.6, §6.2, §7 | static tables |
| `scoring` | §14.1–14.2 | facts, milestone tiers, eras, achievement points |
| `merit` | §14.3 | merit credited per path |
| `payout` | §14.4 | `settle`: nation shares, the equal share, merit components, refunds, dust; `settle_with`: the same with the revealed AI roster and bounties (AI payouts redistributed to people, AI-only nations' shares split equally among counted nations) |
| `contracts` | §4.2 (v7) | treasury contracts: `OfferContract` / `AcceptContract` / `CancelContract` (phase 2, escrow within the spend limit), conditions checked and paid or returned in phase 10, `contract_income` locked out of the market |
| `roster` | V5 §18.2–§18.4 | operator AI tags (`roster_tag`), the roster chain, home cities (`home_city`, from `home_snapshot` at `ai_home_tick`) and where bounties go (`bounties`) |
| `vision` | §7.4 | line of sight (shown as a display-only "sight"), memory, and the `belief` machinery kept for a possible fog mode on a private rollup; nothing uses it to hide state |
| `decision` | §4.3, §7.5 | decision commitments, observation roots and Merkle proofs |
| `preview` | — | legal actions with costs, forecasts and blocked reasons, from the same checks and formulas the engine uses |
| `history` | — | the record a finalized season leaves (final root, per-nation results, every city's founder, final holder and captor, ruins) and the history root chained onto the previous season's |
| `invariants` | §17 | state and monotonicity checks |

## Tests

| File | Covers |
|---|---|
| `tests/season.rs` | genesis, determinism, a full 180-tick season, phase resumption, movement |
| `tests/governance.rs` | elections, offices and budgets, proposals, recalls, consents, merit, milestones and eras |
| `tests/rule_fixes.rs` | the spec v0.2 fixes C1–C9 |
| `tests/battle.rs`, `tests/diplomacy.rs`, `tests/markets.rs`, `tests/standing.rs`, `tests/vision.rs`, `tests/decision.rs`, `tests/payouts.rs` | one area each |
| `tests/pacing.rs` | eurekas, the crisis on the leaders, the dark age |
| `tests/ai_members.rs` | version 7: treasury contracts, home cities, first conquests and bounties, settlement with the roster |
| `tests/spec_vectors.rs` | the combat test vectors of spec §8.4 |
| `tests/common/` | shared helpers (`vrf`, `step`, `free`, and `nations` for member/office setups) |

Whole-season behaviour (every tick's root, every payout, the views) is pinned by `../permutation-server/tests/golden.rs`. Refactors must keep it passing; a change meant to alter behaviour regenerates it with `UPDATE_GOLDEN=1` and the diff is reviewed.

## Development

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build-sbf        # in ../permutation-chain: the Solana SBF build
```

The toolchain is pinned to Rust 1.89.0, with `borsh =1.6.1`.

## Known debt (changes state roots, so deferred to the next rules version)

These are recorded here rather than fixed, because each one changes the state root. A season verified with this version would then no longer replay with the new build.

- Several rule constants are inline instead of in `Ruleset`, so `Ruleset::hash()` does not cover them. Examples: grievance amounts, loyalty steps, city-state growth, suzerain bonuses, the Science focus and Barracks multipliers, raze and fortify durations.
- Unused `Ruleset` fields: `entry_close_tick`, `entry_fee_usdc`.
- `Civ.last_aggression` and `aggressor_window` are recorded but no rule reads them.
- `Scores` duplicates achievement records (`star_gate_stages`, `star_gate_tick` vs `last_star_gate`).
- Some `Blocked` reasons are reused for other cases; for example, `NothingToSell` also means "amount 0".
- The Science focus gives `12_500` milli science per point (`SCIENCE_FOCUS_MILLI` in `tick/production.rs`), that is ×12.5, where 1.25× was probably meant (12,500 reads as basis points). Fixing it changes the balance, so it needs a new `sim` calibration as well.
