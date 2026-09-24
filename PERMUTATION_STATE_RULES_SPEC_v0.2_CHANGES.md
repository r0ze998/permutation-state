# Rules Spec v0.2 — changes from v0.1 (draft)

> **Merged (2026-09-24):** these changes are now part of [Rules Spec v0.2](PERMUTATION_STATE_RULES_SPEC_v0.2.md). This file is kept as the record of the change list.

> **2026-09-24 note:** C10 (Dominion) and C11 (Concord) are superseded by [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) §6–§7. All other changes still apply. The balance check at the end of this file is replaced by the calibration targets in V5 §6.5.

Status: draft for review, 2026-09-24. It goes with [Game Design V4.1](PERMUTATION_STATE_GAME_DESIGN_V4.1.md). Once approved, these changes are merged into [Rules Spec v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md), which is then renamed v0.2, and implemented in `permutation-rules`. Section numbers refer to v0.1.

| Change | Section | v0.1 | v0.2 |
|---|---|---|---|
| C1 | §5.6 Buildings (production) | "Production overflow carries over to the next item." Several items can complete in one tick | **A city completes at most one queue item per tick.** After production is added, the store is capped at the current item's remaining cost; the excess is lost. Overflow from a completion still carries to the next item, up to that item's cost |
| C2 | §5.6, Star Gate stages | Any order, as soon as each is built | **Stage n+1 cannot complete within 6 ticks of stage n** for the same civ (`star_gate_spacing = 6`). Until then the item waits at the head of the queue and production keeps accruing up to the cap (C1). Each completion is a public chronicle event (unchanged) |
| C3 | §4.2 `Purchase` | Several per city per tick | **At most one `Purchase` per city per tick.** A second is invalid at resolution. **`Purchase` cannot target a Star Gate stage** (`CannotBuyStarGate`). `AutoPurchase` (§13) follows the same rules |
| C4 | §10.5 Transfers | Caps checked per order | The caps apply to the **sum of all transfers from civ a to civ b in one tick**. Orders beyond the cap are skipped in batch order |
| C5 | §8.2 row 2, §8.3 (ranged attacks on cities) | "The defender deals no retaliation" | Unchanged against armies. **A city attacked by a ranged army strikes back with its city defence at modifier 5000 (row 9) × 5000** (half of melee city retaliation). City-states (§12.1) get the same strike |
| C6 | §6.1, §5.1 Science focus | Academy output ×12500 computed in whole units, truncating to no effect | The focus multiplier is applied in milli units before rounding: `science_milli × 12500 / 10000` |
| C7 | §9.1 Grievance, and phase order (§15) | `DeclareWar` without casus belli adds 30; grievance decays by 1 in the same tick's phase 8 | Grievance added in phase 1 of tick t **does not decay in tick t**. The victim therefore holds exactly 30 at tick t+1 and can declare with casus belli |
| C8 | §11.3 Exchange | Any match | **No self-matches, and no matches between civs at war** (the orders stay in the book unmatched). **Production bought on the Exchange cannot be delivered to a city whose current item is a Star Gate stage** (the order is skipped) |
| C9 | §14.5 Participation, and "active tick" | A batch with ≥ 1 order | A tick is active when the batch holds **at least one order other than `RevealRationale`** |
| C10 | §14.1 Dominion | Captured city `5 × pop` after 30 ticks held | **`10 × pop` after 12 ticks held** (`capture_hold_ticks = 12`, `capture_points_per_pop = 10`) |
| C11 | §14.3 Concord | `total_pop + 10 × new highs + 5 × suzerain`, 0 while aggressor; ×1.25 if never allied | Per active, non-aggressor tick: **`20 × suzerainties + 10 × (civs with a NAP or alliance) + 2 × min(avg pop per city, 8)`**. ×**1.10** if never allied |
| C12 | new §4.4 Skipped orders | — | `WorldState.last_skipped: Vec<(civ, order index, reason)>` holds the orders of the tick just resolved that did not take effect, using the reason codes of the `Blocked` catalogue (§ preview). It is cleared at phase 0 and part of the state root |
| C13 | §17 Invariants | — | Invariant 2 (occupancy) must hold for every seed of the balance simulation. The failing case (sim seed 35, tick 79) becomes a regression test |

## New and changed parameters (Blitz and Season presets alike)

| Parameter | v0.1 | v0.2 |
|---|---|---|
| `capture_hold_ticks` | 30 | 12 |
| `capture_points_per_pop` | 5 (constant) | 10 |
| `star_gate_spacing` | — | 6 |
| `neutrality_mult_bps` | 12500 | 11000 |
| `concord_suzerain` / `concord_treaty` / `concord_avg_pop` | — / — / — | 20 / 10 / 2 |
| `ranged_city_strike_bps` | — | 5000 |

## Balance check (must hold before merging)

Run `cargo run --release --bin sim -- 40`. It must show:
- three different track winners in at least 70% of seasons;
- no persona winning all three tracks in any season;
- no Star Gate stages less than 6 ticks apart;
- no invariant violations.
