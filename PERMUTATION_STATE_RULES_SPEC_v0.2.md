# PERMUTATION STATE — Rules Specification v0.2 (`permutation-rules`)

Status: implemented in `permutation-rules`, 2026-09-24. `RULES_VERSION = 5` (v0.1 + v0.2 changes + [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md)).
Audience: implementers of the `permutation-rules` Rust crate, the ER program (`permutation-chain`), the replay verifier, the game client and agent authors.
Normative words: **MUST / MUST NOT / SHOULD / MAY**. Every number here is a ruleset parameter (`params.rs`, `Ruleset::new`); the full parameter set and the static tables are serialized and hashed into `ruleset_hash` before a season opens.

> **Principle: one ruleset, two clocks.** Season and Blitz use **identical rules and numbers**, defined per tick. Only the wall-clock length of a tick, the civilization cap, the map radius and the entry window differ (§1). This is what makes Blitz a faithful balance test for Season.

**Changelog from v0.1** (history: [v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md), [v0.2 changes](PERMUTATION_STATE_RULES_SPEC_v0.2_CHANGES.md))
- **C1** (§5.6): a city completes at most one queue item per tick; the production store is capped at the current item's cost.
- **C2** (§5.6): Star Gate stages of one civ are at least `star_gate_spacing` = 6 ticks apart.
- **C3** (§4.2, §13): at most one `Purchase` per city per tick; `Purchase` and `AutoPurchase` cannot target a Star Gate stage.
- **C4** (§10.5): transfer caps apply to the sum of transfers from one civ to another, per good, per tick.
- **C5** (§8.2, §8.3): a city (or city-state) attacked by a ranged army strikes back at 5000 × `ranged_city_strike_bps`.
- **C6** (§5.1): the Science focus multiplier is applied in milli units, so it always has an effect.
- **C7** (§9.1, §15): grievance added in a tick does not decay in that tick.
- **C8** (§11.3): no market trades with oneself or an enemy; bought production never reaches a city building a Star Gate stage.
- **C9** (§14.6): an office acted in a tick if its batch held at least one order other than `RevealRationale`.
- **C12** (§4.4): `WorldState.last_skipped` records every order that did not take effect.
- **C13** (§17): the occupancy invariant is checked on every seed of the balance simulation, with a regression test.
- **C10 and C11 are superseded** by Game Design V5 and not merged. The v0.1 §14 scoring (Dominion / Science / Concord tracks, prize coalitions, §14.5 track payouts) is replaced by V5 achievements and payout (§14).
- **V5 replacements:** nations of members instead of civs-as-seats; four offices with their own budgets; elections, proposals, recalls and consents (§3.1, §4, §14.6); four paths × five milestone tiers, eras and achievement points (max 1125); payout by points and merit (§14); the USDC market with nation treasuries, call auctions, a rising tariff and delivery (§11.3); on-chain input publication (§15.1–15.2).
- **Corrections to v0.1 found in the code:** there is no late entry (§3.1); the order window ends when the input is frozen, not 2% before the boundary (§1); the Science focus multiplies the city's whole science, not only Academy output (§5.1); captured cities count as held from capture, with no hold period (§8.3.1); USDC-market sellers sell from stored food and production (§11.3); Star Gate records move to phase 6 and aggressor flags are set when the act happens (§15); no Crisis and no Season Law are implemented (§12.2, §16); the aggressor flag is recorded but no rule reads it (§9.2).

---

## 0. Conventions

### 0.1 Numeric types
| Quantity | Type | Unit |
|---|---|---|
| Resources (food, production, gold, science, influence, iron, horses) | `u64` / `i64` | **milli-units** (1 unit = 1000) |
| Troop counts | `u32` | **milli-troops** (1 troop = 1000) |
| Merit | `u32` per path | **milli-merit** (1 merit = 1000) |
| Multipliers and percentages | `u32` | **basis points (bps)**, 10000 = 100% |
| USDC amounts | `u64` | 6 decimals (1 USDC = 1_000_000) |
| Ticks | `u16` | 0 … 179 |
| IDs | `u16` civ_id, `u32` unit/city/order id, `u32` member id | assigned sequentially, never reused within a season |

- **No floating point anywhere.** All fractional formulas in this document are specified as integer operations.
- **Rounding:** every division is integer division with truncation toward zero, applied at the point written. A chain `x × a / b × c / d` is evaluated left to right.
- Displayed values are milli-values divided by 1000 and truncated.

### 0.2 Determinism
- Iteration order is always by ascending ID (civ_id, then entity id). Hash maps MUST NOT determine iteration order.
- **Tick seed:** `seed_t = sha256(season_seed ‖ vrf_t ‖ t)` (`rng::tick_seed`), set in phase 0. `vrf_t` is the tick's randomness, carried in the tick input (`TickInput.vrf`).
  - **As implemented on chain:** `vrf_t` is drawn when the tick's input is frozen by the first `LogTickInput` call (§15.2): `vrf_t = sha256("PS/tick-vrf/v2" ‖ world_root ‖ slot ‖ unix_time)`, where `world_root` is the hash of the committed world before the tick. No order or governance action can be added after that point, so `seed_t` is unknown while orders are being submitted.
  - **Known limit:** whoever freezes the input chooses the slot. MagicBlock VRF is the planned replacement (V5 §9).
- `rand(seed_t, domain, id)` = the first 8 bytes (little-endian) of `sha256(seed_t ‖ len(domain) as u8 ‖ domain ‖ id)` as `u64`. Each random draw names its own `domain` string, so draws are independent and reproducible. The one-byte length prefix keeps domains from colliding (`"ab"‖"c"` ≠ `"a"‖"bc"`).
- **Tie-break:** when two entities contend for the same thing (a tile, a capture, a trade fill, an election), priority is given by ascending `rand(seed_t, "tie", id)`.

### 0.3 Coordinates
Axial hex coordinates `(q, r)` with `s = −q − r`. Distance is `(|dq| + |dr| + |ds|) / 2`. Neighbour order: E, NE, NW, W, SW, SE, i.e. `(+1,0) (+1,−1) (0,−1) (−1,0) (−1,+1) (0,+1)`.

---

## 1. Presets

| Parameter | Season | Blitz |
|---|---|---|
| Tick length | 4 h | 30 s |
| Ticks per season | **180** (30 days) | **180** (90 minutes) |
| Order window | from the end of the previous tick until the input is frozen: once the deadline (`tick_seconds` after the previous tick resolved) has passed, or as soon as every office of every nation has submitted (§15.2) | same rule |
| Max civilizations (nations) | 16 (single region) | 8 |
| Nations in a V5 season | six (`genesis::NATIONS`); the engine accepts 2 … max | same |
| Map radius | 19 (1,141 tiles) | 13 (547 tiles) |
| Registration | before tick 0 only (§3.1) | same |
| Everything else | identical | identical |

`entry_close_tick` (30 in Season, 0 in Blitz) is still in `params.rs` but no rule reads it: there is no late entry.

### 1.1 Phases (ticks)
| Phase | Ticks | Season | Blitz |
|---|---|---|---|
| Founding | 0–17 | day 0–3 | 0–9 min |
| Expansion | 18–59 | 3–10 | 9–30 |
| Contention | 60–119 | 10–20 | 30–60 |
| Crisis | 120–161 | 20–27 | 60–81 |
| Resolution | 162–179 | 27–30 | 81–90 |

- Transfers between civilizations are frozen for **ticks ≥ 162**.
- The USDC market (`ExchangeOrder`, `ConsentSpend`) and treasury deposits are frozen for **ticks ≥ 120**.
- Elections start new terms at ticks 30, 60, 90, 120 and 150 (§14.6).
- After tick 179 resolves, `FinishSeason` computes the payout (§14.4).

---

## 2. Map

### 2.1 Terrain
| Terrain | Share of land+water | Food | Prod | Gold | Move cost | Defence (damage taken) | Notes |
|---|---|---|---|---|---|---|---|
| Grassland | 30% | 2 | 0 | 0 | 1 | — | |
| Plains | 25% | 1 | 1 | 0 | 1 | — | |
| Forest | 18% | 1 | 2 | 0 | 2 | ×8000 bps | |
| Hills | 12% | 0 | 2 | 0 | 2 | ×8000 bps | +1 vision |
| Mountain | 7% | — | — | — | impassable | — | blocks vision beyond it |
| Water | 8% | 1 | 0 | 1 | impassable | — | workable only by an adjacent city |

**River:** a flag on 10% of land tiles. It adds +1 gold. An attacker attacking *from* a river tile deals ×8500.

### 2.2 Resources on tiles
| Resource | Density | Effect when worked |
|---|---|---|
| Wheat | 1 per 30 land tiles, on Grassland/Plains | +2 food |
| Iron | 1 per 40 land tiles, on Hills/Plains | +1 prod, and +1 iron per tick until the deposit's **reserve of 90** runs out; after that only the +1 prod remains |
| Horses | 1 per 40 land tiles, on Grassland/Plains | +1 food, and +1 horse per tick, reserve 90 |

Reserves persist across seasons (the "one save" world). Depletion is part of the persistent world state.

### 2.3 Special sites
- **City-states:** `max(2, civs / 3)` sites. Each is at least 6 tiles from every start and every other city-state.
- **Trade hubs:** `max(2, civs / 4)` tiles. Each is at least 5 tiles from every start. See §11.1.
- **Ruins** from the previous season: carried over from persistent state (§3.4).

### 2.4 Generation and fairness
- Generation is deterministic from `world_seed` for the first season. After that the persistent map is reused; only city-states and hubs respawn, from `season_seed`.
- **Start sites:** each lies ≥ **7** tiles from any other start and not on the map's outermost ring. Within radius 2 it must have ≥ 2 tiles with food ≥ 2 and ≥ 1 Forest or Hills. Within radius 5 it must have ≥ 1 Iron or Horses. A start tile carries no resource.
- **Placement (farthest-point):** the first start is the valid site with the lowest `rand(…, "start", tile_index)`. Each next start is the valid site farthest from all chosen starts (ties: lowest random key). If the best remaining spacing is below 7, the attempt fails and the generator rerolls with the next attempt seed.
- **Start value** `V = Σ over the radius-3 tiles of (2×food + 2×prod + gold) + 6 × strategic resources within radius 5`.
- **Balancing:** while any start has `V < ceil(max(V) × 10000 / 11000)`, that start gets one upgrade per round. Only tiles within radius 3 that are strictly closer to that start than to any other start are eligible, taken by distance then tile index. The upgrade is the first available of: Mountain→Hills, Wheat on a plain Grassland/Plains tile, Water→Grassland. If no upgrade remains, the attempt fails. Starts therefore end within ±10% of each other.
- *Why (v0.1 fix):* random greedy placement with spacing 8 generated 0 of 10 maps for Blitz with 8 civs and for Season with 9 or more. Farthest-point placement plus balancing generates every supported configuration (Blitz 2–8, Season up to 16).

---

## 3. Entry, start and protection

### 3.1 Entry: members of nations (V5 §4)
- A season has a fixed set of **nations** (the civilizations of the world). Players, human or AI, join one nation as **members**.
- **Registration happens before the season only** (`Register` on the base layer; `gov::join` refuses anything after tick 0 has started). There is no late entry.
- **Fee:** the same for every member: `entry_fee_usdc` (10 USDC in the default ruleset; the season account stores the fee it charges). It is paid in USDC, from a wallet or via x402 for agents; both use the same `Register` instruction.
- **Split at registration:** `ops_share_bps` = 20% to operations, the rest (80%) to the prize pool.
- **One member per wallet per season** (the member PDA is keyed by the wallet). At most `max_members` = 256 members per season. No limit per nation.
- `Register` also carries: the nation, a display name, a session key (signs governance actions and, in office, orders), the declared kind (`Human | Agent | Undeclared`, cosmetic only — no rule reads it), an optional agent attestation hash (for example ERC-8004, shown as "verified AI"), the offices the member stands for, the member's votes for the first election, and an optional treasury deposit (§11.3; only in seasons with the market on).
- During registration a member may change its candidacy and votes (`UpdateMember`).
- **Seating:** after genesis the operator adds members to the world in registration order (`SeatMembers`), then holds the first election (`OpenGovernment`, §14.6). Each step's state root is logged (`PS_SEAT`, `PS_OPEN`), so anyone can recompute it.
- A nation with no members is run entirely by the acting official (§4.5). It plays normally but never shares the pool (§14.4).
- *Removed from v0.1:* the 2-civs-per-wallet and 2-per-ERC-8004-operator limits, and prize coalitions.

### 3.2 Starting state
| Item | Value |
|---|---|
| Capital | founded automatically on the start site: pop 1, territory radius 1, loyalty 100 |
| Units | 1 Scout; 1 army of 3 Spearmen, placed on the capital |
| Gold | 20 |
| Order bank | 0 for every office |
| Treasury (USDC) | the sum of the nation's members' deposits |
| Techs | none. Spearman, Scout, Settler, Granary and Workshop are available without techs |

*Removed from v0.1:* the late-joiner grant (there is no late entry).

### 3.3 Protected start zones
- Other civilizations' units MUST NOT enter a protected zone. The zone is centred on the capital and its radius shrinks over time:

| Ticks | 0–17 | 18–29 | 30–44 | 45–59 | ≥ 60 |
|---|---|---|---|---|---|
| Radius | 4 | 3 | 2 | 1 | 0 (none) |

- A civilization loses its own protection immediately and permanently if it declares war (§10.2) or attacks a city-state.
- Units already inside a zone when the radius shrinks are not displaced.

### 3.4 Heritage (persistent world)
Founding a city within 2 tiles of a ruin grants that city `+(ruin_peak_pop / 2)` food and production per tick for 10 ticks. Only the first city founded near a given ruin each season gets it.

---

## 4. Tick structure and orders

Only **office holders** give orders (V5 §5.1). Each nation has four offices: **General**, **Steward**, **Science** and **Diplomat**. Each office submits at most one **batch** per tick. Other members take part through proposals, support, votes and recalls (§14.6).

### 4.1 Order budget
- **Nation budget per tick:** `B = min(3 + cities, 8)` (`budget_base` = 3, `budget_cap` = 8). It is computed in phase 11 of the previous tick, so cities are counted at the start of the tick. A government in exile gets 3.
- **Split by office** (`role_split`, V5 §5.2). Rows B = 0–2 exist in the table but cannot occur.

  | B | General | Steward | Science | Diplomat |
  |---|---|---|---|---|
  | 3 | 1 | 1 | 0 | 1 |
  | 4 | 1 | 1 | 1 | 1 |
  | 5 | 2 | 1 | 1 | 1 |
  | 6 | 2 | 2 | 1 | 1 |
  | 7 | 2 | 3 | 1 | 1 |
  | 8 | 3 | 3 | 1 | 1 |

- **Bank, per office:** unused budget accumulates up to `bank_ticks × share` = `4 × share`. Spending draws from this tick's share first, then from the bank.
- **Spendable in a tick** by one office: `share + bank`.
- The budget is checked **on chain** at submission (`SubmitOrders`: cost ≤ spendable) and again in phase 0 (`validate_batch`). A batch that fails in phase 0 is rejected whole (§4.4, `BatchRejected`). An order that is invalid at resolution time is dropped **without refund**. Clients MUST show the order's precondition before it is submitted.

### 4.2 Order catalogue
| Order | Cost | Office | Valid if |
|---|---|---|---|
| `MoveUnit(unit, path ≤ 12 tiles)` | 1 | General (armies, scouts); Steward (settlers) | the unit is owned and the path is passable. Movement continues over later ticks without new orders |
| `Attack(army, target)` | 1 | General | at war with the target's owner (or the target is a city-state or barbarian); target adjacent, or within range 2 for ranged units |
| `FoundCity(settler)` | 1 | Steward | the site is ≥ 3 tiles from any city, not in foreign territory, not in another civ's protected zone, and not Water or Mountain |
| `SetQueue(city, items[≤3])` | 1 | Steward | items are unlocked |
| `SetFocus(city, Balanced|Food|Production|Gold|Science)` | 1 | Steward | — |
| `Purchase(city, gold)` | 1 | Steward | pays gold for production at 3 gold per production, up to 50% of the current item's remaining cost. **At most one per city per tick**; a second is skipped (`AlreadyPurchased`). **Never a Star Gate stage** (`CannotBuyStarGate`) (C3) |
| `SetResearch(techs[≤3])` | 1 | Science | prerequisites are met or queued earlier |
| `DeclareWar(civ)` | 1 | Diplomat | not allied with the target, no NAP in force, no truce, **and a `ConsentWar(civ)` in the same tick** (§4.5). Takes effect at the **start of the next tick** |
| `ProposePeace / AcceptPeace` | 1 each | Diplomat | at war |
| `ProposeNAP(civ, bond, 30 ticks) / AcceptNAP` | 1 each | Diplomat | at peace, bond ≥ 30 gold each |
| `BreakNAP(civ)` | 1 | Diplomat | forfeits the bond (§10.3); needs a `ConsentWar(civ)` in the same tick (§4.5) |
| `ProposeAlliance / AcceptAlliance / LeaveAlliance` | 1 each | Diplomat | §10.4 |
| `SendEnvoy(city_state, influence)` | 1 | Diplomat | enough influence in stock |
| `Transfer(civ, goods)` | 1 | Diplomat | within the caps (§10.5) and tick < 162 |
| `MarketTrade(good, side, amount, limit)` | 1 | Diplomat | gold AMM (§11.2) |
| `ExchangeOrder(good, side, amount, price)` | **0** | Diplomat | the season has the market on and tick < 120 (§11.3) |
| `Raze(city)` | 1 | General | the city was captured this tick or the previous one |
| `SetStanding(unit|city, rule)` | 1 | unit: as `MoveUnit`; city: Steward | §13. **Execution** of standing rules is free |
| `ConsentWar(civ)` | **0** | General or Steward | agrees to this tick's `DeclareWar` or `BreakNAP` against `civ` (§4.5) |
| `ConsentSpend(usdc)` | **0** | any office but Diplomat | allows the diplomat to spend up to `usdc` from the treasury this tick (§11.3); market on and tick < 120 |
| `RevealRationale(tick, policy, salt, text)` | 0 | any | opens the commitment of a resolved `tick` (§7.5), not a game action |

- At most **one** manual order per unit per batch. A manual order overrides that unit's standing rule for that tick.
- An order sent by the wrong office is refused on chain at submission (`WrongOffice`). The engine checks the office again per order in phase 0, where unit orders are assigned by unit type (settlers to the Steward, all other units to the General).

### 4.3 Order commitments
Every `SubmitOrders` transaction carries `decision_digest = sha256("PS/decision/v1" ‖ tick ‖ obs_root ‖ policy_id ‖ rationale_hash)` (§7.5). `obs_root` is the Merkle root of the fog-filtered view served to that civilization for tick `t`. In phase 0 the digest of each accepted batch is appended to the event chain as `decision(civ, office, member, digest)`, before anything resolves. It is never interpreted by the rules.

- **A batch from an office holder MUST carry a non-zero digest** (V5 D17); otherwise it is refused on chain and rejected by the engine (`MissingRationale`). The acting official (§4.5) may send a zero digest.
- The game server publishes each civ's `obs_root` for the open tick (`decision.obsRoot` in that civ's view). Hosted players and bots commit through the server's ledger; an outside agent computes the digest itself against the published root, and the server learns it from the published tick input (`PS_INPUT`, §15.2).
- The reveal is a `RevealRationale { tick, policy, salt, text }` order in a later batch of the same office (cost 0; only for ticks already resolved: `tick < open_tick`). Anyone recomputes the digest; a mismatch is shown as unverified, never trusted.
- A zero digest means "no commitment". The commitment proves what was claimed and when, not that the claim is true.

### 4.4 Skipped orders (C12)
- `WorldState.last_skipped: Vec<Skip>` holds every order of the tick just resolved that did not take effect.
- `Skip = (civ, office, index, reason)`. `index` is the order's position in that office's batch (own orders first, then adopted proposals); `u16::MAX` means the whole batch was rejected. `reason` is a `checks::Blocked` code (`BLOCKED_NAMES` gives the names), the same catalogue the client previews use.
- Codes added for v0.2 and V5: `BatchRejected`, `WrongOffice`, `NeedsConsent`, `AlreadyPurchased`, `CannotBuyStarGate`, `NoCounterparty`, `NeedsSpendConsent`, `NotEnoughUsdc`.
- It is cleared in phase 0 and is part of the state root.

### 4.5 Office batches, the acting official, adoption and consent
- **One batch per office per tick.** On chain, a new `SubmitOrders` replaces the office's earlier batch for the open tick. The engine uses the first valid batch per office in the input.
- **Signer:** the office holder's session key. For a vacant office, the operator's crank key signs as the **acting official** (`NOBODY`, V5 §8). The acting official's orders count normally but earn no merit.
- **Merge order:** in phase 0 the accepted batches are merged in office order General, Steward, Science, Diplomat. Each order carries a merit credit (§14.3).
- **Adoption of proposals** (V5 §5.4):
  - by id: a batch lists proposal ids in `adopt`. Their orders run after the batch's own orders, count toward the office's budget, and credit merit half to the proposer and half to the officer. An unknown, already adopted or wrong-office id rejects the whole batch;
  - by auto-match: an officer's own order identical to an order in an unadopted proposal for the same office, made on an earlier tick and with at least one supporter, adopts that proposal (against claim-jumping). The proposer gets half the merit.
- **War consent** (V5 §5.6, §16): a `DeclareWar(civ)` or `BreakNAP(civ)` takes effect only if the General's or the Steward's batch of the same tick holds `ConsentWar(civ)`, from a different member than the Diplomat (any member if the Diplomat office is vacant). Otherwise it is skipped (`NeedsConsent`).
- **Spend consent:** see §11.3.

---

## 5. Cities

### 5.1 Tiles and yields
- A city works its **centre tile plus `pop` tiles** within its territory.
- **Centre yield:** `max(tile food, 2)` food, `max(tile prod, 1)` production, `max(tile gold, 1) + 1` gold, and +1 science.
- **Tile assignment:** the governor assigns tiles deterministically, maximising `Σ yield × weight`.
  - Weights (food, prod, gold) by focus: Balanced (3,2,1); Food (5,1,1); Production (2,5,1); Gold (2,1,5); Science (3,2,1).
  - **Science focus (C6):** the city's science is multiplied by 12500 bps in milli units, before any rounding: `science_milli = science × 12500` instead of `science × 1000`. (v0.1 said "Academy output ×12500"; the code multiplies the whole city's science.)
  - Ties go to the lower tile index (q, then r).

### 5.2 Per-city yields per tick
```
food        = Σ worked tile food + buildings
production  = Σ worked tile prod + buildings
gold        = centre gold + Σ worked tile gold + pop / 2 + buildings
science     = 1 + pop / 3 + buildings
influence   = (capital ? 1 : 0) + buildings
```
Everything above is computed in whole units and then stored ×1000 as milli (science with the Science focus: §5.1).

### 5.3 Food and growth
- **Consumption:** 2 food per pop.
- **Surplus** `S = food − 2 × pop`, adjusted by amenities (§5.5).
- **Growth threshold:** `G(p) = 15 + 8 × (p − 1) + isqrt((p − 1)³)`, where `isqrt` is the integer square root.

  | p | 1 | 2 | 3 | 4 | 5 | 6 | 8 | 10 | 12 |
  |---|---|---|---|---|---|---|---|---|---|
  | G(p) | 15 | 24 | 33 | 44 | 55 | 66 | 89 | 114 | 139 |

- When the food store reaches `G(p)`: pop +1 and the store drops by `G(p)`.
- **Starvation:** if the store would go below 0, pop −1 (minimum 1) and the store is set to 0.

### 5.4 Territory
- **Territory radius:** 1 at founding, 2 at pop 3, 3 at pop 6.
- Tiles are claimed in ascending distance, then by tile index. A tile already owned by another city is never taken, except by capture.

### 5.5 Amenities
```
amenities = 3 + 2 × [Temple] − pop / 3 − (cities − 1) / 3 − min(4, war_weariness / 20)
```

| Amenities | Effect |
|---|---|
| ≥ 2 | positive surplus ×11000 |
| 0 … 1 | none |
| −1 … −2 | positive surplus ×5000 |
| ≤ −3 | surplus = min(S, 0); production ×7500; loyalty −2 per tick |

### 5.6 Buildings
Each building can be built once per city.

| Building | Prod cost | Tech | Effect |
|---|---|---|---|
| Granary | 40 | — | +2 food |
| Workshop | 40 | — | +2 production |
| Temple | 50 | Mysticism | +2 amenities, +2 influence |
| Market | 60 | Currency | +3 gold, and city gold ×12000 |
| Academy | 60 | Writing | +3 science |
| Barracks | 50 | Bronze Working | troop production cost ×7500 in this city |
| Walls | 60 | Masonry | damage dealt to the city's virtual defence ×6667 (i.e. +50%) |
| Star Gate I / II / III | 200 / 300 / 400 | Astronomy / Physics / Celestial Mechanics | Science path tiers 4 and 5 (§14.1). **One city per civilization**, publicly visible |

- **Stalemate breaker (pre-committed):** from tick 120, Star Gate stage costs are multiplied by `max(6000, 10000 − 100 × (t − 120))` bps.
- **One completion per tick (C1).** After the tick's production is added, a city completes **at most one** queue item. Then the production store is capped at the cost of the item now at the head of the queue; the excess is lost. Overflow from a completion therefore carries to the next item, up to that item's cost. With an empty queue the cap is 0: nothing is banked without an item.
- **Star Gate spacing (C2).** Stage n+1 of a civilization cannot complete within `star_gate_spacing` = 6 ticks of its stage n. Until then the stage waits at the head of the queue and production keeps accruing up to the cap. Each completion is a public event (`star_gate`).
- Troops wait at the head of the queue until the civ holds their strategic resources. Completed units spawn on the city tile or the first free neighbour; if there is none, the unit is delayed by one tick.

### 5.7 Founding
- **Settler:** 30 production. Requires city pop ≥ 2, and pop −1 on completion.
- **New city:** pop 1, territory radius 1, loyalty 100, empty stores.

---

## 6. Civilization-level economy

### 6.1 Gold, upkeep and deficit
```
income        = Σ city gold (+ hub fees, + suzerain bonuses)
unit_upkeep   = T × (20 + T) / 80         where T = Σ troops (T2 troops count as 1.5), in whole troops
city_upkeep   = (cities − 1)² / 3
gold_t+1      = gold_t + income − unit_upkeep − city_upkeep
```
Reference values:

| Troops T | 10 | 20 | 30 | 40 | 60 |
|---|---|---|---|---|---|
| unit upkeep | 3 | 10 | 18 | 30 | 60 |

| Cities | 2 | 4 | 6 | 8 | 10 |
|---|---|---|---|---|---|
| city upkeep | 0 | 3 | 8 | 16 | 27 |

**Deficit:** if `gold_t+1 < 0`, troops are disbanded **one whole troop at a time**, always from the army with the highest priority (T2 before T1, then the largest army, then the highest id). Upkeep is recomputed after each troop until the balance is ≥ 0; if no troops remain, gold is set to 0. Because upkeep (phase 7) runs after production (phase 6), the deficit's −2 amenities applies to that tick's **society phase** (loyalty, phase 8), not to that tick's growth.

**Wealth** (for the Prosperity path, §14.1) is the cumulative whole gold produced by cities and Mercantile suzerainty this season. Spending does not reduce it; AMM trades, transfers and the USDC market do not add to it.

### 6.2 Science and technology
- **Tech cost:** `base × (10000 + 1000 × (cities − 1)) / 10000`. Each city beyond the first adds 10%, so going wide does not buy science for free.
- Science goes to the first queued tech. Overflow carries over.
- **Technology cannot be transferred.**

| Era | Tech | Base cost | Prerequisites | Unlocks |
|---|---|---|---|---|
| I | Agriculture | 40 | — | Wheat tiles +1 food (total +3) |
| I | Bronze Working | 40 | — | Barracks |
| I | Archery | 40 | — | Archer |
| I | Horseback Riding | 40 | — | Horseman; reveals Horses |
| II | Masonry | 90 | Bronze Working | Walls |
| II | Mysticism | 90 | Agriculture | Temple |
| II | Writing | 90 | Agriculture | Academy |
| II | Currency | 90 | Bronze Working | Market; gold AMM access |
| II | Iron Working | 90 | Bronze Working | Pikeman; reveals Iron |
| III | Mathematics | 160 | Writing, Currency | Crossbowman (with Archery) |
| III | Chivalry | 160 | Horseback Riding, Iron Working | Knight |
| III | Philosophy | 160 | Mysticism, Writing | +1 influence in every city with a Temple |
| III | Engineering | 160 | Masonry, Mathematics | Walls ×5000 instead of ×6667 (i.e. +100%) |
| IV | Astronomy | 260 | Mathematics, Philosophy | Star Gate I |
| IV | Physics | 320 | Astronomy | Star Gate II |
| IV | Celestial Mechanics | 400 | Physics | Star Gate III |

The minimum path to Celestial Mechanics costs **1,650 base**: Agriculture, Bronze Working, Writing, Mysticism, Currency, Mathematics, Philosophy, Astronomy, Physics, Celestial Mechanics.

("Era" in this table is the tech tier. It is unrelated to a nation's era in §14.2.)

### 6.3 Influence
- Influence is a civilization-level stock, produced by cities (§5.2).
- It is spent only on `SendEnvoy` (§12.1). It cannot be traded or transferred.

### 6.4 Strategic resources
- Iron and horses are civilization-level stocks, fed by worked deposits (§2.2).
- They are consumed by T2 troops (§7.1). They can be traded on the gold AMM and the USDC market.

---

## 7. Units and armies

### 7.1 Unit types
| Unit | Kind | Tier | Strength / troop | Move | Prod / troop | Strategic / troop | Tech | Counter (×15000 vs) |
|---|---|---|---|---|---|---|---|---|
| Spearman | melee | 1 | 10 | 1 | 6 | — | — | Horseman, Knight |
| Archer | ranged (range 2) | 1 | 10 | 1 | 7 | — | Archery | Spearman, Pikeman |
| Horseman | mounted | 1 | 10 | 2 | 9 | — | Horseback Riding | Archer, Crossbowman |
| Pikeman | melee | 2 | 22 | 1 | 12 | 1 iron | Iron Working | Horseman, Knight |
| Crossbowman | ranged (range 2) | 2 | 22 | 1 | 14 | 1 iron | Mathematics | Spearman, Pikeman |
| Knight | mounted | 2 | 22 | 2 | 16 | 2 horses | Chivalry | Archer, Crossbowman |
| Scout | civilian | — | 0 | 3 | 10 | — | — | vision 3; ignores Forest/Hills extra cost |
| Settler | civilian | — | 0 | 1 | 30 (+1 pop) | — | — | founds a city |

### 7.2 Armies
- An **army** is a stack of **one unit type**, holding between 0.5 and **20** troops (500 … 20,000 milli-troops).
- **Only one army or civilian unit may stand on a tile.** Friendly units cannot share a tile, except inside a city, where 1 army and 1 civilian may coexist.
- **Production:** a city produces troops into a queue item `Troops(type, n ≤ 20)`. They spawn as one army.
- `MergeArmies`, as a `MoveUnit` into an adjacent same-type friendly army, combines them up to 20. Surplus stays in the moving army.
- An army destroyed below 500 milli-troops is removed.
- Civilian units are captured, not destroyed, when an enemy army enters their tile. A captured Settler becomes the captor's Settler.

### 7.3 Movement and vision
- Each tick a unit gets its **movement points** and advances along its path, paying terrain costs.
- A unit may always move one tile if it has full MP, even into a tile that costs more than its MP.
- Entering an enemy-occupied tile is not movement. It requires `Attack`.
- **Foreign territory:** units may enter foreign territory only when at war with its owner, allied with it, or if it is a city-state they are suzerain of.
- **Vision:** radius 2 for units and cities, 3 for Scouts, +1 on Hills. Mountains block vision.
- A civilization's fog view is the union of its own vision and its allies' vision (§10.4).

### 7.4 Fog of war and the belief state
Rules resolve on the full state. Every player — human client or agent — decides from its **belief state**, built by one function for everyone (`vision::belief`), so no player sees more than another.

- **Line of sight:** a viewer at `a` with radius `R` sees `b` if `dist(a,b) ≤ R` and no Mountain lies on a hex strictly between them. A Mountain itself is seen. The hexes between are the cube lerp of `a→b` for `i = 1..n−1`, computed in integers scaled by `1000·n` and nudged by `(+1, +2, −3)` before cube rounding, so ties always break the same way.
- **Vision sources:** every living unit (its `vision` stat) and city (radius 2) of the civilization and its allies; +1 when the source stands on Hills.
- **Memory:** per civilization, the tiles ever seen; each tile's owner city and ruin as last seen; and a snapshot of each foreign city as last seen, with that tick. Memory is derived from past states, so it is not part of `WorldState` or its hash.
- **Belief state:**
  - Terrain, rivers and resources are **public** (they follow from the published world seed).
  - Units are shown only inside vision.
  - Foreign cities, borders and ruins are shown as last seen, or omitted if never seen.
  - Other civilizations' gold, science, influence, iron, horses, techs, research queue, USDC and city queues/food/production are removed — from allies too.
- **Public announcements** stay global: war and peace, treaties, captures, revolts, razing, city founding, Star Gate stages, milestones and eras (§14), elections and recalls (§14.6). Tech discoveries are private to the civilization and its allies.
- **Validation** of orders always uses the full state; an order the belief state made look possible can still fail (for example a move into a tile an unseen army occupies stops, §7.3).
- `obs_root` (§4.3) is the Merkle root of this belief view (§7.5).
- The fog is a client-side rule: on-chain data is public. Private execution (PER / TEE) is on the roadmap (V5 §9).

### 7.5 Decision logs (commit-reveal)
- **Hashes:**
  - `policy_id = sha256("PS/policy/v1" ‖ policy)`, where `policy` ≤ 64 bytes (for example `human` or `bot/warlord@1`).
  - `rationale_hash = sha256("PS/rationale/v1" ‖ salt₁₆ ‖ text)`, where `text` ≤ 512 bytes. The random salt stops short reasons from being guessed before the reveal.
  - `decision_digest = sha256("PS/decision/v1" ‖ tick_le16 ‖ obs_root ‖ policy_id ‖ rationale_hash)`.
- **Observation leaves**, in this order; each body is Borsh-encoded:
  1. `header`: `("PS/obs/v1", tick, civ, ruleset_hash)`.
  2. `tile` × every tile: `(index u32, q i32, r i32, fog u8, owner_city Option<u32>, ruin Option<u16>)`. `fog` is 0 = never seen, 1 = remembered, 2 = in sight.
  3. `civ` × every civ, as in the belief state (private fields cleared).
  4. `city` × every city present in the belief state: `(City, last_seen Option<u16>)`.
  5. `unit` × every unit present in the belief state.
  6. `city_state` × every explored city-state: `(id, q, r, pop, defense, specialty, suzerain, captured_by, own influence, top influence)`.
  7. `diplomacy`: `(relations, truce_until, proposals to/from civ, [(other, their grievance, own grievance)])`.
  8. `market`: `pools`.
- **Merkle tree:**
  - `leaf = sha256(0x00 ‖ len(kind) ‖ kind ‖ body)` and `node = sha256(0x01 ‖ left ‖ right)`.
  - An odd node at the end of a level is carried up unchanged.
  - The root of an empty list is `sha256("")`.
- **Reveal:** `RevealRationale(tick, policy, salt, text)` is valid only for `tick < current tick`. Phase 2 appends `reveal(civ, office, tick, policy_id, rationale_hash)` to the event chain.
- **Verification:** a verifier recomputes `obs_root` from the replayed state and the memory, recomputes the digest from the revealed parts, and compares it with the `decision` event of that civ, office and tick. An inclusion proof for a single leaf proves one fact about what the civ saw, for example "this tile was fog".
- **Office holders MUST seal a digest** with every batch (§4.3, V5 D17). For other members' proposals it is optional.
- **What it proves:**
  - The choice was fixed before the tick resolved.
  - It was made with only that civ's view.
  - The reason was not edited afterwards.
- **What it does not prove:** who or what wrote the reason.

---

## 8. Combat

### 8.1 Damage formula
Every engagement is resolved **simultaneously**. Both sides' damage is computed from their **pre-combat** troop counts, then applied.

```
F[n] = round(n^0.2 × 1000),  n = clamp((N_A + N_B) / 1000, 1, 64)      -- table §8.5
x    = N_A × str_A × K / 10000 / str_B          K = 5500
x    = x × 1000 / F[n]
x    = x × m_1 / 10000 × m_2 / 10000 …          modifiers in the order of §8.2
x    = x × v / 10000                             v = 9000 + rand(seed_t, "var", engagement_id ‖ side) mod 2001
damage_to_B = x   (milli-troops)
```
`N` is in milli-troops. `str` comes from §7.1.

### 8.2 Modifiers
Applied in this order, each only when its condition holds.

| # | Modifier | bps | Applies to the damage dealt by |
|---|---|---|---|
| 1 | Counter (§7.1) | 15000 | the countering side |
| 2 | Ranged attack | 8000 | a ranged attacker at range 1 or 2. **An army deals no retaliation.** A city or city-state strikes back (§8.3, C5) |
| 3 | Melee attacking a ranged army | 5000 | the ranged defender's retaliation |
| 4 | Defender terrain (Forest/Hills) | 8000 | the attacker |
| 5 | Defender fortified (did not move in the last 2 ticks) | 8700 | the attacker |
| 6 | Attacker used all its MP this tick | 8500 | the attacker |
| 7 | Attacking from a River tile | 8500 | the attacker |
| 8 | Walls (on damage to the city defence) | 6667 (5000 with Engineering) | the attacker |
| 9 | City retaliation | 5000 | the city defence |
| 10 | Crisis barbarians vs anyone | 10000 | — (reserved for tuning) |

### 8.3 Cities in combat
- **Garrison first.** If an army stands on the city tile, it is the defender (with terrain and fortify modifiers as normal). The city defence is untouched while a garrison exists.
- **City defence:** a virtual army with strength 10 and `D_max = (4 + pop) × 1000` milli-troops.
  - It regenerates `+2000` per tick if it was not attacked that tick.
  - It never exceeds `D_max`. Any pop change updates `D_max` immediately.
- **Ranged attacks on cities (C5):** a city attacked by a ranged army strikes back at the attacker, garrisoned or not, with its current city defence as the troop count, strength 10, and modifiers 9 (5000) × `ranged_city_strike_bps` (5000). That is half of the melee city retaliation. City-states (§12.1) strike back the same way.
- **Capture:** after combat, if the city defence is 0 and no garrison remains, the city is captured by the attacking **melee or mounted** army with the most troops remaining among those that attacked it this tick. Ties go to the tie-break (§0.2). **Ranged armies cannot capture.**
- **On capture:**
  - pop −1 (minimum 1);
  - Walls and Barracks are destroyed, other buildings are kept;
  - loyalty is set to 50;
  - the city's food and production stores are halved;
  - a Star Gate in progress loses **all** completed stages. The victim's permanent Star Gate record for the Science path (§14.1) is not lost.
  - The capturing army moves into the city.
- **Last-city protection:** a civilization's last remaining city cannot be captured within 12 ticks of that civilization losing its previous city. It can still be attacked.
- **Raze:** `Raze` within one tick of capture destroys the city over 3 ticks. The city becomes a ruin and its tiles become unowned. Razing gives +60 grievance (§9.1).
- **Government in exile:** a civilization with 0 cities keeps its armies, gold, influence, diplomacy and members, with budget 3. It may found a new city with a Settler, or recapture one. A nation without a city at the end of the season takes no share of the pool (§14.4).

### 8.3.1 Resolution details (v0.1, fixed while implementing phase 5)
These close gaps in §8.1–8.3. They are normative.

1. **Engagement order and ids.** Attacks are sorted by (civ id, army id). The engagement id used for variance (§8.1) is the position in that order. Side 0 is the attacker, side 1 the defender.
2. **Several attackers on one defender.** Each attack is a full engagement against the defender's **pre-combat** count. The defender retaliates against each attacker, and all damage it takes is summed.
3. **Attacking holds position.** An army with an `Attack` order has its path cleared in phase 4, so it does not move before combat.
4. **Protected zones.** A target standing inside another civilization's active protected zone (§3.3) cannot be attacked.
5. **Units in their own city.** Attacking a unit that stands in its owner's city is an attack on that city (the garrison defends first, §8.3).
6. **Civilians.** A melee or mounted army attacking an adjacent civilian that stands alone outside a city captures it: the civilian changes owner and stays on its tile. No combat takes place. Capturing a Settler adds +5 grievance (§9.1).
7. **Neutrals.** Attacking a city-state or a Free City sets the attacker's aggressor flag and ends its protection. Attacking a city-state also zeroes the attacker's influence with every city-state.
8. **Capture bookkeeping.** When a capital is captured, capital status passes to the victim's lowest-id remaining city (none means government in exile). A captured city's queue is cleared and its defence restarts at **50% of its new maximum** `(4 + pop) × 1000` (balance fix, see 12 below). The old owner's civilians on the tile are captured with it.
9. **Captured cities held (V5).** At capture the rules decide whether the city counts as a *captured city held* for the Hegemony path (§14.1): the founder must differ from the captor, and the city must be at least `capture_min_founded_age` = 12 ticks old. It counts from the tick of capture for as long as the captor holds it. There is no hold period and no once-per-captor rule. (v0.1's 30-tick hold and once-per-season rule were Dominion rules, removed with it.)
10. **Tiles that stop being cities.** When a city becomes a ruin (§8.3 Raze) or a Free City (§9.4), any civilian sharing that tile with an army moves to the first free passable neighbour, in §0.3 order. If there is none, the civilian is disbanded.
11. **Razing timeline.** `Raze` at tick t sets a 3-tick countdown. The countdown advances at the start of phase 5 in ticks t+1 and t+2, and the city becomes a ruin in phase 5 of tick t+3. A city being razed produces nothing.
12. **Why captures restart at half defence (balance fix, 2026-09-24).** In a six-bot Blitz match with defence reset to 0, the same city changed hands after 1, 1 and 3 ticks: an adjacent army simply walked back in. At 50%, the shortest gap was 2 ticks, and it took a real fight; quick recaptures (≤ 10 ticks) fell from 5 to 2. No capture-immunity window was needed.
13. **Merit from combat** (§14.3): a capture, enemy troops destroyed, and a city that was attacked and held.

### 8.4 Test vectors (N in milli-troops, v = 10000 unless stated)
| # | Setup | A→B | B→A |
|---|---|---|---|
| a | 10 Spearmen attack 10 Spearmen, plains | 3020 | 3020 |
| b | 20 Spearmen attack 10 Horsemen (counter) | 8358 | 2786 |
| c | 10 Archers, ranged, vs 12 Spearmen on Hills (counter, ranged, terrain) | 2844 | 0 |
| d | 20 Spearmen vs an ungarrisoned pop-10 city with Walls (defence 14 troops) | 3622 | 1902 |
| e | 10 Pikemen vs 20 Spearmen | 6129 | 2532 |
| f | Case a with v = 9000 / 11000 | 2718 / 3322 | — |

An implementation MUST reproduce these exactly.

### 8.5 `F[n]` table (`n^0.2 × 1000`)
```
 1:1000  2:1149  3:1246  4:1320  5:1380  6:1431  7:1476  8:1516
 9:1552 10:1585 11:1615 12:1644 13:1670 14:1695 15:1719 16:1741
17:1762 18:1783 19:1802 20:1821 21:1838 22:1856 23:1872 24:1888
25:1904 26:1919 27:1933 28:1947 29:1961 30:1974 31:1987 32:2000
33:2012 34:2024 35:2036 36:2048 37:2059 38:2070 39:2081 40:2091
41:2102 42:2112 43:2122 44:2132 45:2141 46:2151 47:2160 48:2169
49:2178 50:2187 51:2195 52:2204 53:2212 54:2221 55:2229 56:2237
57:2245 58:2253 59:2260 60:2268 61:2275 62:2283 63:2290 64:2297
```

---

## 9. Grievances, aggression, war weariness, loyalty

### 9.1 Grievance ledger
- `G[a][v]` is the grievance that victim `v` holds against civilization `a`. It is an integer ≥ 0 and is recorded on chain.
- **Decay:** −1 per tick, in phase 8. **Grievance added in a tick does not decay in that tick (C7).** So after a `DeclareWar` without casus belli in phase 1 of tick t, the victim holds exactly 30 at tick t+1 and can declare back with casus belli.

| Event (by a against v) | +G[a][v] |
|---|---|
| `DeclareWar` without casus belli | 30 |
| Break a NAP | 40 |
| Capture a city of v | 20 |
| Raze a city of v | 60 |
| Capture a Settler of v | 5 |

**Casus belli:** `v` has casus belli against `a` when `G[a][v] ≥ 30`. Declaring war with casus belli adds no grievance and sets no aggressor flag.

### 9.2 Aggressor flag
A civilization is an **Aggressor** in tick `t` if, in any of ticks `t−11 … t`, it did any of the following:
- declared war without casus belli;
- broke a NAP;
- captured or razed a city of a civilization that had **not** declared war on it;
- attacked a city-state.

Fighting barbarians, defending, and capturing cities of a civilization that declared war on you are **not** aggression.

*v0.2:* the flag is recorded (`Civ.last_aggression`) when the act happens, but no rule reads it. It drove Concord eligibility in v0.1; V5 achievements (§14) do not use it. It stays in the state for displays and for the Crisis roadmap.

### 9.3 War weariness (civilization-level, integer ≥ 0)
- Per tick at war: +2 for each war this civilization declared without casus belli, +1 for each other war.
- +1 per 2 whole troops lost this tick.
- At peace with everyone: −3 per tick. Otherwise −1 per tick.
- Amenity penalty: `min(4, WW / 20)` in every city (§5.5).

### 9.4 Loyalty (per city, 0–100)
```
Δ = +2                                  (base)
    +1 if distance to own capital ≤ 6
    +3 if garrisoned
    −(Σ pop of foreign non-allied cities within 6) / 3
    −2 if amenities ≤ −3
```
At 0 the city becomes a **Free City**: neutral NPC, it keeps its pop and buildings, and behaves like a city-state without a specialty (§12.1).

---

## 10. Diplomacy

All diplomatic orders are the **Diplomat's** (§4.2). War and breaking a NAP need a second officer's consent (§4.5).

### 10.1 States between each pair of civilizations
The pairwise states are `Peace` (the default), `War`, `NAP` (a peace with bonds) and `Alliance`.

### 10.2 War and peace
- `DeclareWar` takes effect at the start of the next tick. Attacks are valid from then on.
- Declaring war ends the declarer's protected zone (§3.3).
- **Peace:** a proposal plus an acceptance ends the war at the start of the next tick. Each side's units inside the other's territory are teleported to the nearest own-territory free tile.
- **Truce (balance fix, 2026-09-24):** when peace takes effect at tick p, neither side may declare war on the other before tick **p + 12**, even with casus belli. Without it, grievances from the war kept casus belli alive, and the bot match re-declared war within 12 ticks of peace 6 times. With it, war can only resume once the truce ends.

### 10.3 Non-aggression pact (NAP)
- **Duration:** 30 ticks.
- **Bond:** each side posts ≥ 30 gold. The amounts may differ and are stated in the proposal.
- **Expiry:** bonds are returned.
- **Break:** the breaker's bond goes to the other party, the breaker gets +40 grievance, the aggressor flag is set, and war begins at the next tick.

### 10.4 Alliances
- **Maximum members:** `min(3, max(2, civs / 3))`. Blitz with 6 civilizations gives 2; Season with 16 gives 3.
- Allies share vision, may enter each other's territory and cannot declare war on each other.
- `LeaveAlliance` takes effect after 6 ticks, with no grievance.
- **Membership is recorded per tick** (`Civ.ever_allied`, phase 10). No v0.2 rule reads it; the Concord path counts alliances at the end of the season (§14.1).

### 10.5 Transfers between civilizations
| Good | Per-tick cap per sender and receiver |
|---|---|
| Gold | `max(10, sender's gold income this tick)` |
| Iron / Horses | `max(1, sender's income of that good)` |
| Food / Production (delivered to a named city of the receiver) | the sender's largest single-city surplus or production this tick |

- **The cap applies to the sum (C4)** of all transfers of one good kind from civ a to civ b in one tick. Transfers are applied in batch order; one that would exceed the cap is skipped (`OverCap`).
- **No transfers at tick ≥ 162.**
- **Transfers count as trade volume** for both sides (Concord path, §14.1) and earn trade merit for the sender's issuer (§14.3). They never count as wealth. (v0.1 said transfers never count for any score.)
- *v0.1 details:* "this tick" means the sender's yields from the **previous** tick's phase 6, since transfers resolve in phase 2, before this tick's production. Transfers are not allowed between civilizations at war. Food and Production are taken from the sender's city holding the most of that good (ties: lowest id).
- **Value** for trade volume, in whole gold: gold and food at 1, iron and horses at the gold AMM's spot price, production at the purchase rate (3).

### 10.6 Proposals and timing (v0.1)
- `Propose*` records a proposal. `Accept*` only matches a proposal made on an **earlier** tick, so a proposal and its acceptance can never race within one tick. Proposals expire after 6 ticks. A new proposal of the same kind to the same civ replaces the old one.
- **Peace:** acceptance sets the war's `peace_at = t + 1`; the war is inactive from then on. At the start of phase 1 of that tick, the relation becomes Peace and each side's units in the other's territory move to the nearest free tile of their own territory (by distance, then tile). If no tile is free, the unit stays.
- **NAP:** both bonds (≥ 30 gold each) are escrowed at acceptance. If either civ cannot pay, the pact fails. At expiry both bonds are returned.
- **Alliances are closed groups.** Accepting civ X's proposal joins X's whole group: the joiner must have no allies, the group must be below the cap, the joiner must be at Peace or under a NAP with every member (a NAP is superseded and its bonds returned), and no member may be leaving. `LeaveAlliance` marks all of the leaver's alliances to end after 6 ticks.
- **Treaty merit:** when a peace, NAP or alliance is accepted, the proposing and the accepting issuers each earn treaty merit (§14.3).
- **Envoys:** all envoys of a tick are applied first. Then each city-state without a suzerain goes to the civilization with the **most** influence at or above 60 (ties by tie-break), so civ order never decides suzerainty.
- **Founding** (`FoundCity`, phase 2): the settler founds at its own tile if the tile is passable, not foreign territory, ≥ 3 tiles from every city and city-state, and not inside another civilization's active protected zone. The centre tile always belongs to the new city. A civilization without a capital makes the new city its capital.

(These `Propose*` orders are diplomacy between nations. Proposals by members to their own offices are a different thing, §14.6.)

---

## 11. Markets

### 11.1 Trade hubs
A hub tile inside a city's territory makes that city the **hub holder**. The holder receives 1% of the notional of every gold-AMM trade (§11.2).

### 11.2 Gold AMM (in-game; requires Currency)
- **Pools:** Iron/Gold and Horses/Gold, constant product, seeded each season with 200 iron (or horses) and 2,000 gold of world liquidity (not operator funds).
- **Fee:** 3%, taken on input. 1% goes to the hub holders (split equally), 2% is burned.
- **Batch clearing:**
  - All `MarketTrade` orders in a tick are netted per pool.
  - The net amount executes once against the curve.
  - Every trader in that tick gets the **same average price**.
  - Orders whose `limit` is violated at that price are dropped, and the batch recomputes. At most 3 iterations; after that, remaining violators are dropped.
- This makes arrival order irrelevant (V4 §5.2).
- **Trade volume:** each filled trade counts its gold value (paid or received, whole units) as trade volume with the counterparty "market" (§14.1), and earns trade merit for its issuer (§14.3).

### 11.3 The USDC market (V5 §7.5)
The v0.1 Exchange is replaced by a market between **nation treasuries**. Its goal: USDC can buy goods, but money cannot move the result too much.

| Rule | Value |
|---|---|
| Switch | `market_enabled`, fixed per season at creation and part of the ruleset. With the market off, `ExchangeOrder`, `ConsentSpend` and deposits are refused |
| Treasury | each nation's USDC (`Civ.usdc`). Members deposit at registration only (on chain); the engine also accepts deposits in a tick input before tick 120. The treasury is separate from the prize pool and never counts toward achievements or merit |
| Who trades | only the Diplomat (`ExchangeOrder`, cost 0) |
| Goods | Gold, Iron, Horses, Food→city, Production→city. Nothing else (no units, cities, research, budget or influence) |
| Supply | only other nations' stocks. The operator never sells. Food and Production are sold from the seller's named city's **stored** food or production |
| Matching | once per tick, in phase 2, a **uniform-price call auction** per good kind (`markets::call_auction`) |
| Per-tick buy cap | ≤ the buyer's own income of that good in the previous tick (Gold: gold income; Iron/Horses: `max(1, income)`; Food/Production: its largest city surplus / production) |
| Fee | `exchange_fee_bps` = 5% of notional, paid by the buyer |
| Tariff | a rising rate on the buyer's cumulative market spend `s` this season (below) |
| Income split | fee + tariff: `vault_share_bps` = 80% to the prize pool, 20% to operations (V5 D11) |
| Spend consent | the Diplomat may reserve up to `spend_consent_usdc` = 5 USDC per tick. More needs `ConsentSpend(usdc)` from another office, by a different member than the Diplomat; the tick's limit becomes `max(5 USDC, largest consent)` |
| Delivery | the seller's goods leave at the fill; the buyer receives them `delivery_ticks` = 3 ticks later, at the start of phase 2 |
| Re-sale | goods bought on the market may not be sold on the market again in the same season (`exchange_bought[good]` is subtracted from sellable stock) |
| No self or enemy trades (C8) | a buyer is never paired with itself or with a nation it is at war with |
| No money into a Star Gate (C8) | Production cannot be bought for a city whose current item is a Star Gate stage |
| Scoring | market trades count for nothing: not wealth, not trade volume, not merit |
| Window | ticks 0–119. Frozen from tick 120 |
| End of season | USDC left in a treasury is returned to the nation's depositors in proportion to their deposits, with their `Claim` |

**Tariff** (`tariff_table_bps`, `tariff_full_usdc` = 100 USDC): the table gives the rate at cumulative spend `s` = 0, 10, 20, … 100 USDC; between two points it is linear; at or above 100 USDC it is 100%. The table is `5% + 95% × (s / 100)^1.3`, rounded:

| s (USDC) | 0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | ≥ 100 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Tariff (bps) | 500 | 976 | 1672 | 2486 | 3387 | 4358 | 5391 | 6475 | 7608 | 8784 | 10000 |

- `s` (`Civ.market_spent`) counts everything the buyer paid: notional, fee and tariff.
- The rate for a tick is fixed from `s` before the tick's auctions.
- There is one curve per nation, however many members it has. It replaces v0.1's per-season spend cap.

**Resolution details** (`markets::apply_exchange`)
- Orders are read in merge order. Each gets a tie key `rand(seed_t, "tie", civ ‖ order serial)`.
- **Buy orders are clamped, not rejected:**
  - to the per-tick cap, summed over the civ's buy orders of that good kind this tick;
  - to what is affordable at the order's limit price: `unit = price + fee + price × tariff`, within the room left under `min(spend limit, treasury)` after the civ's earlier buy orders this tick.
  - A buy clamped to 0 is skipped (`NeedsSpendConsent` if the spend limit was the constraint, else `NotEnoughUsdc`). A Production buy for a city building a Star Gate stage, or a Food/Production buy for a city that is not the buyer's, is skipped.
- **Sell orders** are clamped to sellable stock (stock minus units bought on the market this season, minus the civ's earlier sell orders of that good).
- **Clearing price** per good kind: maximum volume, then smallest imbalance, then lowest price. Fills: buyers by price high→low, sellers low→high, ties by key.
- **Pairing (C8):** buyers in priority order take from sellers in priority order, skipping themselves and enemies. What cannot be paired stays unfilled this tick, at the same price.
- **Settlement per pair:** buyer pays `q × P + fee + tariff`; seller receives `q × P`; the income goes 80% to the prize-pool vault (`exchange_vault`) and 20% to operations (`exchange_ops`).
- **Delivery:** at the start of phase 2 of tick `t + 3`, bought Gold, Iron and Horses go to the buyer's stock. Food and Production go to the named city if it is still the buyer's (and, for Production, not building a Star Gate stage), else to the capital under the same rule, else they are lost.
- Invariant 1 (§17): `Σ treasuries + exchange_vault + exchange_ops = Σ deposits` after every tick.
- At `FinishSeason` the prize pool is `80% of fees + exchange_vault`; operations receive `20% of fees + exchange_ops + payout dust` (§14.4).

### 11.4 Resolution details (v0.1, fixed while implementing)
**Gold AMM**
- Orders are whole units; reserves are milli. Let `B` = total buy units and `S` = total sell units of the live orders in a pool.
  - `S = B`: everyone trades at spot `gold × 1000 / goods` milli-gold per unit; the pool is untouched.
  - `S > B`: the pool takes `S − B` units. Price `p = floor((y − ceil(k / (x + net))) / net)` per unit; the pool pays out `p × net`.
  - `S < B`: the pool gives `B − S` units. Price `p = ceil((ceil(k / (x − net)) − y) / net)`; the pool receives `p × net`.
  - The rounding always favours the pool, so `k = x × y` never decreases.
- Buyers pay `q × p + fee`; sellers receive `q × p − fee`; the fee is 3% of `q × p`. 1% of notional goes to hub holders, split equally over all hubs (an unheld hub's share is burned); the other 2% is burned. Gold is conserved exactly.
- Sellers must hold the goods when the tick resolves. A buyer whose cost at the batch price exceeds its limit **or its gold** is a violator.
- The limit loop: price, drop all violators, re-price; up to 3 rounds. If violators remain after the third round, they are dropped and the remaining orders clear at the re-computed price without further checks.
- If the net demand would empty the pool, the largest buy order (ties: earliest) is dropped and the batch re-priced.

(The v0.1 Exchange details are replaced by §11.3.)

---

## 12. Neutral actors and the Crisis

### 12.1 City-states
- **Count:** §2.3. Each has pop 3 at tick 0 and gains +1 pop every 30 ticks, up to 6. It never expands or builds units.
- **Defence:** virtual army `(8 + 2 × pop)` troops at strength 10. Against a ranged attack it strikes back as a city does (§8.3, C5). Regeneration is +2 per tick.
- **Specialty** (random): Scientific (+3 science to its suzerain), Mercantile (+4 gold), or Agrarian (+2 food in the suzerain's capital).
- **Influence:** `SendEnvoy` adds influence points `I[c][cs]`. They never decay within a contest cycle.
- **Suzerainty:**
  - The first civilization to reach **60** becomes suzerain.
  - Suzerainty is **locked for 45 ticks**, then the contest restarts: every civilization's `I` is halved and the lock is removed.
  - Contest cycles start at ticks 0, 45, 90 and 135.
- **Attacking a city-state** is allowed. It sets the aggressor flag and removes the attacker's influence with every city-state.
- **Capturing a city-state** makes it the captor's city and ends that city-state's suzerainty. The captor counts as its founder, so it is **not** a captured city held for the Hegemony path (§14.1). Its tiles count as territory.

### 12.2 Crisis (ticks 120–161) — not implemented (roadmap)
The engine has no Crisis in v0.2 (`tick.rs`, phase 9: TODO). No barbarian is ever spawned. The v0.1 design below is kept for the roadmap. V5 §10 retargets it: the waves go to the nations with the **most achievement points**, not to track leaders.
- **Targets:** every 6 ticks (at 120, 126, …, 156) the top civilizations by achievement points so far.
- **Wave per target:** a barbarian army of `4 + (t − 120) / 6` troops.
  - Spearmen until tick 137, Pikemen after.
  - Horsemen and Knights alternate by wave index.
- **Spawn:** on a free land tile at distance exactly 4 from the target's most populous city. Choice is by `rand(seed_t, "crisis", civ_id)`; if no tile is valid, distance 5.
- **Behaviour:** barbarians move by the shortest path to that city and attack whatever they meet. Standing rules and players fight them normally. They produce no grievance and no aggressor flag.

---

## 13. Standing rules (first-party automation)

`SetStanding(target, rule)` costs 1 order and is applied in phase 2. The target is a unit or a city. Unit rules are the General's (a settler's are the Steward's); city rules are the Steward's. Execution is free and deterministic, in phase 3 (§15): each rule is compiled into **implicit orders** kept in `WorldState::implicit`. Phases 4 and 5 treat those exactly like manual orders, and phase 11 clears them. A unit that received any manual order this tick (move, attack, found, set standing) skips its rule for that tick; the rule stays set.

| Rule | Target | Behaviour |
|---|---|---|
| `AutoDefend(r)`, 1 ≤ r ≤ 3 | army | The anchor is the army's hex when the rule is set. Attack the weakest hostile army (lowest troops, then id) that is within the army's attack range (1, or 2 for ranged) **and** within `r` of the anchor. |
| `Retreat(ratio_bps)`, 1000–100000 | army | Strength = troops × unit strength. If Σ adjacent hostile strength × 10000 > own × `ratio_bps`, move one tile to the enterable, empty neighbour that most reduces the distance to the nearest own city (ties: lowest hex). Nothing happens inside a city. |
| `Patrol(route)`, 1–6 waypoints | any unit | When the unit has no path left, head for the next waypoint using the engine's path (`preview::path_to`). On reaching a waypoint, advance to the next, looping. |
| `QueueRepeat(on)` | city | When the queue empties after a unit item, repeat it (default: on). |
| `AutoPurchase(max_gold)`, ≤ 500 | city | Apply `Purchase(max_gold)` to the current item, unless the city made a manual `Purchase` this tick. Same rules as a manual purchase: never a Star Gate stage (C3). |
| `Clear` | either | Remove the unit's rule, or reset the city to its defaults. |

- "Hostile" means a barbarian or an army of a civilization at war with the owner.
- A captured unit or city, or a city that revolts, loses its rules. The new owner does not inherit automation.
- Agents and humans get exactly the same set. The acting official (§4.5) relies on them first.

---

## 14. Achievements, merit and payout (V5 §6–§7)

This section replaces v0.1 §14 (Dominion, Science and Concord tracks, prize coalitions and track payouts). A nation's progress is measured on four **paths** (Hegemony, Prosperity, Science, Concord), each with five **milestone tiers**. Milestones give **achievement points**; points decide each nation's share of the pool; **merit** decides each member's share inside the nation.

### 14.1 Milestones
Every milestone is an absolute threshold, so any number of nations can reach it. **A path's tier is the highest `k` such that milestones 1 … k all hold** (a ladder: a tier needs every lower tier too). Tiers and eras are recomputed in phase 10 of every tick; a change is announced (`milestone`, `era` events).

| Tier | Hegemony | Prosperity | Science | Concord |
|---|---|---|---|---|
| 1 | tiles ≥ 25 | pop ≥ 12 | techs ≥ 4 | partners ≥ 1, **or** an envoy sent |
| 2 | tiles ≥ 40 | pop ≥ 20 and wealth ≥ 1,000 | techs ≥ 9 | partners ≥ 1 and suzerain for at least one tick |
| 3 | tiles ≥ 60, **or** captured cities held ≥ 1 | pop ≥ 30 and wealth ≥ 2,000 | techs ≥ 13 | partners ≥ 1 and suzerainties ≥ 1 |
| 4 | tiles ≥ 80 and captured cities held ≥ 1 | pop ≥ 42 and wealth ≥ 3,500 | Star Gate I completed | partners ≥ 1, suzerainties ≥ 1, alliances ≥ 1 and trade ≥ 400 |
| 5 | tiles ≥ 100 and captured cities held ≥ 2 | pop ≥ 55 and wealth ≥ 5,000 | Star Gate III completed | partners ≥ 2, suzerainties ≥ 2 and trade ≥ 1,000 |

Parameters: `hegemony_tiles` [25, 40, 60, 80, 100], `hegemony_cities` [0, 0, 1, 1, 2], `prosperity_pop` [12, 20, 30, 42, 55], `prosperity_wealth` [0, 1000, 2000, 3500, 5000], `science_techs` [4, 9, 13], `concord_partners` [1, 1, 1, 1, 2], `concord_suzerains` [0, 0, 1, 1, 2], `concord_trade` [400, 1000].

**Definitions** (`scoring::Facts`)
| Fact | Meaning | Kind |
|---|---|---|
| tiles | tiles owned by the nation's living cities | state |
| captured cities held | living cities the nation holds that were captured from another founder and qualified at capture (§8.3.1 item 9) | state |
| pop | total population of the nation's living cities | state |
| wealth | cumulative whole gold produced this season (§6.1) | permanent |
| techs | technologies held | permanent (techs are never lost) |
| Star Gate | the most stages the nation ever completed; losing the city does not lower it | permanent |
| partners | nations with which it has a NAP or an alliance | state |
| alliances | nations with which it is allied | state |
| suzerainties | city-states (not captured) of which it is suzerain now | state |
| suzerain for one tick | it was suzerain of some city-state at the end of any phase 10 | permanent |
| envoy sent | it sent at least one envoy | permanent |
| trade | effective trade volume: gold AMM trades and transfers (both sides), valued in gold (§10.5, §11.2). Each counterparty (each nation, and the AMM as one) counts for at most `trade_counterparty_bps` = 40% of the raw total. USDC market trades are excluded | permanent |

- **Permanent** facts never decrease, so milestones built only on them are kept once reached. **State** facts are judged on the current state; the payout uses the state **after the last tick**. During the season they are shown as "if the season ended now".
- Conquest therefore matters: taking a rival's cities lowers its tiles, pop and captured cities held, and can remove its state milestones.
- The ladder means Concord tier 5 also needs tier 4's alliance and trade ≥ 400.

### 14.2 Eras and achievement points
- **Era:** the highest `k` such that, for every `j ≤ k`, at least **two** paths have tier ≥ `j` (**three** paths for `j = 5`).
- **Points per milestone and era bonus** (`tier_points`):

  | Tier / era | 1 | 2 | 3 | 4 | 5 |
  |---|---|---|---|---|---|
  | Points per milestone | 10 | 20 | 35 | 60 | 100 |
  | Era bonus | 10 | 20 | 35 | 60 | 100 |

- A path at tier `k` scores `Σ tier_points[1..k]` (tier 5 = 225). The era bonus is the same ladder sum for the era.
- **Nation points** = the four path scores + the era bonus. The maximum is 4 × 225 + 225 = **1125**.
- A Science ranking key (stages, earliest completion, cumulative science) remains for displays only (`scoring::science_key`).

### 14.3 Merit
Merit records what each member did for the nation, per path (plus a `Common` slot for office duty). It is stored in **milli-merit** per member and per path, credited by the rules as events happen, and part of the state root. Only the members named in a credit are touched, so the cost does not grow with the number of members.

**Who is credited.** Each order carries a credit: the officer who issued it, and the proposer if it came from an adopted proposal (§4.5). With a proposer, half goes to the proposer and the rest (including rounding) to the officer. The acting official (`NOBODY`) earns nothing. Some events credit "the active officer" of an office: the holder, if its office executed an order other than a reveal within the last `officer_active_ticks` = 10 ticks (`office_last_act`).

| Path | Event | Merit | Credited to |
|---|---|---|---|
| Hegemony | a city captured | pop at capture (before −1) × `merit_capture_per_pop` (10) | the captor's surviving attacks on that city, shared by their remaining troops |
| Hegemony | enemy troops destroyed | `merit_per_troop` (1) per troop | the attack's credit; several attacks on one unit share its losses by damage dealt |
| Hegemony | an own city attacked this tick and still held | `merit_city_held` (5) per tick | the active General |
| Prosperity | a city founded | `merit_found_city` (20) + `merit_found_per_tile` (1) × tiles of its territory | the `FoundCity` credit |
| Prosperity | a building completed (not a Star Gate) | production cost / `merit_building_div` (10) | whoever last set that city's queue |
| Prosperity | pop +1 in a city | `merit_pop` (5) | whoever last set that city's queue or focus |
| Prosperity | city gold produced this tick | gold / `merit_gold_div` (20) | the active Steward |
| Science | a technology completed | research cost / `merit_tech_div` (10) | whoever last set the research queue |
| Science | a Star Gate stage completed | `merit_star_gate` (100): 50 and 50 | whoever last set that city's queue, and the active Science officer |
| Concord | a peace, NAP or alliance accepted | `merit_treaty` (20) each | the proposing issuer and the accepting issuer |
| Concord | suzerain of a city-state, per tick | `merit_suzerain` (2) | the civ's envoys to it, shared by influence sent |
| Concord | trade (AMM trade, or a transfer by the sender) | growth of the nation's effective trade volume / `merit_trade_div` (20) | the order's credit |
| Common | an office acted this tick (C9) | `merit_office_tick` (1) | the officer |

- Votes and support earn no merit. They count only toward activity (§14.6).
- USDC market trades earn no merit.

### 14.4 Payout (`payout::settle`)
The same function gives the final payouts (`FinishSeason`, verifier) and the "if the season ended now" projection.

1. **Pool.** `pool = 80% of entry fees + exchange_vault` (§3.1, §11.3). Operations get `20% of entry fees + exchange_ops + dust`.
2. **Active member:** active in at least `active_windows_needed` = 9 of the season's 18 activity windows of `activity_window_ticks` = 10 ticks (§14.6).
3. **Counted nation:** it has at least one city after the last tick **and** at least one active member. A nation without members, destroyed, or with no active member takes no share, and its points do not count in the total.
4. **Refund:** if the counted nations' points sum to 0, every member receives `pool / members` (fees are equal, so this is pro rata); the remainder is dust.
5. **Nation share:** `share = pool × points / Σ counted points`.
6. **Equal share:** `equal_total = min(share × equal_share_bps (20%), active members × entry fee × equal_cap_bps (50%))`. Each active member gets `equal_total / active members`. The rest of the share, `rest = share − each × active`, goes to merit.
7. **Merit share:** `rest` is split into components, then each component is split among the nation's members by merit:
   - one component per path with points > 0 and some member merit on that path, weighted by that path's points, split by merit on that path;
   - one era component, if era points > 0 and some member has merit, weighted by the era points, split by total merit (all paths and Common).
   - Component amount = `rest × weight / Σ weights`; member amount = `amount × member merit / component merit`.
   - If no component exists, `rest` goes equally to the active members.
8. **Dust:** every division truncates. `dust = pool − Σ payouts` goes to operations, so the vault always ends at zero after all claims and the operations withdrawal.
9. **Claims:** `FinishSeason` (permissionless) writes the amount per member into the Season account (at most 256 members, no Merkle tree). Each member's wallet claims once with `Claim`: its prize plus its share of its nation's remaining treasury (§11.3). The operations share is withdrawn with `WithdrawOps`.

The payout rules are fixed before registration opens (they are part of the ruleset and its hash).

### 14.5 Removed from v0.1
Dominion, Concord and the Science track as payout tracks; prize coalitions; track winners per track, the rank weights `7500^(k−1)`, the one-top-3-per-entry rule and the participation pool. v0.2 changes C10 and C11 are not merged.

### 14.6 Governance (V5 §5, `gov.rs`)
Governance is part of the deterministic world. Every action arrives in a tick's input (`TickInput.gov`, submitted with `SubmitGov`), is applied in phase 0 in input order, and elections and recalls resolve in phase 11 and take effect from the next tick. Replaying the inputs replays who held which office and why. An action whose signer is not the member's registered key is ignored.

**Actions**
| Action | Effect | Counts as activity |
|---|---|---|
| `Stand { roles }` | stand for these offices (bit mask; 0 withdraws). Any time | no |
| `Vote { role, candidate }` | one vote per office for the coming term; a new vote replaces the old one. Accepted only in the vote window, for a member of the voter's own nation | yes |
| `Propose { role, orders }` | propose 1 … `max_proposal_orders` = 4 orders for an office; they must pass the structural checks and belong to that office. At most `max_open_proposals` = 24 open per nation | yes |
| `Support { proposal }` | support another member's proposal, once | yes |
| `Recall { role }` | open a recall of that office's holder, or vote yes on the open one | yes |

**Terms and elections**
- A term lasts `term_ticks` = 30 ticks. Terms start at ticks 0, 30, 60, 90, 120 and 150.
- **First election:** held by `OpenGovernment` before tick 0, from the candidacies and votes given at registration.
- **Vote window:** votes for a term are accepted in the last `vote_window` = 10 ticks before it starts (ticks 20–29 for the term at 30, and so on).
- **Election:** in phase 11 of the tick before a term starts. Offices are filled in the order General, Steward, Science, Diplomat. Only members standing for the office are candidates. Most votes wins; ties go to the lower `rand(e, "tie", civ ‖ office ‖ member)`, where `e = sha256("PS/election" ‖ season_seed ‖ seed_t ‖ start)`. A candidate with no votes can win an uncontested office. A member who already holds `max_offices_per_member` = 2 of the new offices is passed over. The next eligible candidate is recorded as **runner-up**. An office with no candidate goes to the acting official until the next election.
- A re-elected holder keeps its tenure, its idle clock and any open recall. A new holder starts fresh. Votes are cleared after each election. Each result is an `elected` event.

**Recalls**
- Any member may open a recall of an office holder; other members vote yes with the same action.
- **Electorate:** the nation's members active in the last `recall_electorate_ticks` = 10 ticks.
- **Passes** when yes votes × 2 > electorate, checked in phase 11 of each tick. A recall stays open `recall_ticks` = 5 ticks. It lapses if the office changes hands.
- **On success:** the holder is removed from tick t+1. The last election's runner-up succeeds if it is not the removed holder and holds fewer than 2 offices; otherwise the acting official takes the office until the next election. A `recalled` event is logged.
- **Automatic idle recall:** in phase 11, if an office holder has not sealed **any** batch (even an empty one, `office_seen`) for `idle_recall_ticks` = 30 ticks since the later of its last batch and the start of its tenure, a recall of it is opened automatically. It passes only by the same majority. An officer with nothing to order is not idle as long as it seals a batch each tick; "active officer" merit (§14.3) still needs an executed order (`office_last_act`).

**Proposals**
- A proposal lives until adopted or for `proposal_ttl_ticks` = 10 ticks (it is removed at the end of tick `p + 9`). Adopted proposals are removed at the end of the tick.
- Adoption by id or by auto-match: §4.5. Merit is shared half and half (§14.3).

**Consents:** war and breaking a NAP need a `ConsentWar` from the General or the Steward (§4.5). Treasury spending above 5 USDC per tick needs a `ConsentSpend` (§11.3). Each consent is an order in the consenting officer's batch, so it is on chain and in the decision log.

**Activity (C9, V5 §7.3)**
- **An office acted** in a tick if its accepted batch held at least one order, other than `RevealRationale`, that passed the office and consent checks. That marks the office holder active, sets `office_last_act` and earns office merit.
- **A member is active in a tick** if its office acted, or if it successfully voted, proposed, supported or voted in a recall. Standing for office does not count.
- Activity is recorded per window of 10 ticks (`Member.windows`) and as `last_active` (for recall electorates).

---

## 15. Tick resolution order

`resolve_tick(t)` MUST execute these phases in order (`tick::run_phase`, `PHASE_COUNT` = 12). Within each phase, iteration is by ascending id unless stated.

| # | Phase | Contents |
|---|---|---|
| 0 | Seed and governance | set `seed_t` (§0.2); clear `last_skipped` and the merit log; reset per-tick flags; apply governance actions in input order (§14.6); apply treasury deposits (tick < 120, market on); for each civ and office, accept the first valid batch, log `decision(civ, office, member, digest)`, drop orders of the wrong office (`WrongOffice`) or war orders without consent (`NeedsConsent`), auto-match proposals, record `office_seen`, activity and office merit; merge the offices' orders in office order |
| 1 | Diplomacy | war declarations and peace from tick t−1 take effect; declarations; NAP and alliance accepts; breaks; leave-alliance timers; treaty merit |
| 2 | Economy orders | market deliveries due; FoundCity, SetQueue, SetFocus, SetResearch, Purchase (C3), SetStanding, RevealRationale (`reveal` event); transfers (C4); envoys and suzerainty; gold-AMM batch (§11.2); USDC call auctions (§11.3) |
| 3 | Standing rules | compile standing rules into implicit orders for units without a manual order |
| 4 | Movement | sub-steps s = 1 … 3 (max MP). In each sub-step every unit with remaining MP attempts one step. When several units target the same free tile, tie-break (§0.2) picks one and the others wait for that sub-step. Units never swap through each other |
| 5 | Combat | razing countdowns; collect all attacks (manual + implicit); compute all damage from pre-combat counts; apply; remove dead armies; resolve captures (§8.3) and razes; record aggression against neutrals; combat merit |
| 6 | Production and growth | yields, food, growth or starvation, production and at most one completion per city (C1, C2), spawning, Star Gate records, suzerain bonuses, research, influence, strategic income and depletion; wealth; production and science merit |
| 7 | Upkeep | gold, upkeep, deficit disbanding (§6.1) |
| 8 | Society | war weariness, loyalty (Free City flips), city defence regeneration, grievance decay (C7) |
| 9 | Neutral actors | city-state growth and regeneration, suzerainty cycles. (No Crisis, §12.2) |
| 10 | Scoring | suzerain records and merit; office banks (§4.1); alliance record; milestones and eras, with `milestone` and `era` events (§14.1–14.2) |
| 11 | Commit | next tick's budget `B`; governance end of tick: resolve recalls, open idle recalls, hold the election if a term starts at t+1, expire proposals (§14.6); clear implicit and accepted orders; `tick` event; `tick += 1`, `phase_cursor = 0`. Then `state_root = sha256(borsh(state))` |

- The aggressor flag is set when the act happens (phases 1 and 5), not in phase 8. Star Gate records are made in phase 6. (v0.1 listed both under phase 8.)
- The event chain is a hash chain: `event_head = sha256(event_head ‖ tick ‖ len(kind) ‖ kind ‖ payload)`.

### 15.1 Compute budget and split resolution
If a tick exceeds the ER transaction compute limit, it MUST be split into consecutive transactions at phase boundaries. The next phase index is stored in state (`phase_cursor`), so the sequence can resume permissionlessly: `ResolveTick { to }` runs phases from `phase_cursor` up to `to` (12 = the rest of the tick). Every call uses the same frozen input. A tick is complete only when phase 11 is committed. Orders for tick t+1 are accepted only after that.

### 15.2 Input publication and permissionless liveness
- **Freeze.** The first `LogTickInput { chunk: 0 }` freezes the tick's input. It is allowed once the deadline has passed, or as soon as every office of every nation has submitted. It draws `vrf_t` (§0.2). From then on `SubmitOrders` and `SubmitGov` are refused with `TickFrozen` until the tick resolves.
- **Publication.** The input (`TickInput`: `vrf`, office batches, governance actions, deposits) is Borsh-encoded and logged in `PS_INPUT` records of `INPUT_CHUNK` = 6000 bytes each, in order: `(tick, chunk, total, input hash, bytes)`.
- **Resolution.** `ResolveTick` fails with `InputNotPublished` until every chunk has been logged. Each call logs `PS_TICK (tick, to, pre_root, root, input hash)`: only the input's hash, not the input. Every resolved tick can therefore be replayed from the chain alone, whatever the size of its input.
- **Liveness.** `LogTickInput`, `ResolveTick` and `FinishSeason` are permissionless. If the operator stops, **any** signer may call them. The result is identical regardless of who calls it.

---

## 16. Season Law menu (v0.1) — not implemented (roadmap)

No Season Law is implemented in v0.2: no code reads or applies a law. The v0.1 selection rule ("each track winner picks one") no longer applies, because there are no track winners. V5 §10 suggests another method, for example a vote of the members of nations that reached era 4 or higher; it is undecided (V5 §15). The menu is kept for the roadmap.

| # | Law | Effect next season |
|---|---|---|
| 1 | Iron Age | Iron and Horse reserves +50% |
| 2 | Levée en masse | T1 troop production cost ×8500 |
| 3 | Open Skies | Tech base costs ×8500 |
| 4 | Great Library | Academy +1 science |
| 5 | Pax | City-state suzerainty threshold 60 → 48 |
| 6 | Guild Charter | Gold AMM fee 3% → 2% |
| 7 | Frontier Grant | Protected zone radius +1 in every phase |
| 8 | Long Harvest | Granary +1 food |

---

## 17. Invariants (MUST hold after every tick; checked in tests, the balance simulation and the replay verifier)

Checked by `invariants::check` (state) and `invariants::check_monotonic` (across a tick), or enforced where noted.

1. Sum of all nations' treasuries + `exchange_vault` + `exchange_ops` = USDC deposited (no creation or loss).
2. No tile holds more than one army or civilian unit, except a city tile of the units' owner (1 army + 1 civilian).
3. Every army holds 500–20,000 milli-troops.
4. Orders applied this tick per office ≤ that office's spendable budget (enforced by `SubmitOrders` and `validate_batch`).
5. No resource stock is negative (civ stocks and city stores).
6. Achievement records never decrease (wealth, Star Gate record, "was suzerain", "sent an envoy", trade volume, cumulative science), and neither do members' merit and activity windows.
7. The same state and input give the same `state_root` on the ER program, the native verifier and WASM.
8. No Transfer is applied at t ≥ 162, and no market order or spend consent at t ≥ 120 (enforced by `check_structure`).
9. *(v0.1: aggressors accrue no Concord. Removed with Concord scoring.)*
10. Technology counts never decrease, and no technology is ever held without its prerequisites.
11. No city owns a tile beyond the maximum territory radius.
12. Every office is held by a member of that nation, and no member holds more than `max_offices_per_member` = 2 offices.

**C13:** the balance simulation checks invariants 1–12 after every tick of every seed and prints the number of violations (`permutation-server`, `sim`). `SIM_DEBUG="seed:tick"` checks them after every phase of one tick. A case the simulation found (a third civ's settler left on a captured city's tile, sim seed 20, tick 97) is a regression test in `permutation-rules/tests/battle.rs`.

---

## 18. Deferred to later versions

- Frontier settlement seat (V4 §4.3)
- Vassalage
- Direct P2P gold trades
- Multi-region Season maps
- Roads
- Naval units
- Espionage / sabotage other than capture
- Heritage multipliers beyond §3.4
- The Crisis (§12.2) and Season Law (§16)
- Treasury deposits during play on chain (the engine supports them; the chain accepts deposits at registration only)
- Sealed orders (commit-reveal of the orders themselves, against last-second reactions)
- MagicBlock VRF for `vrf_t` (§0.2)
- Private execution (PER / TEE) for the fog (§7.4)
- Vote delegation, and better automatic matching of proposals

---

## 19. Balance

The v0.1 single-civilization notes are superseded. The calibration targets and results are in [Game Design V5 §6.5](PERMUTATION_STATE_GAME_DESIGN_V5.md). To reproduce them, run the season simulation in `permutation-server`:

```
cargo run --release --bin sim -- 40
```

It plays 40 seasons with six nations (members per nation 3, 3, 2, 2, 1, 0 by default; a second argument such as `3,3,2,2,1,0` changes them) and prints eras, points, pool shares, tiers per path, merit by path, adopted proposals, recalls, budget use and invariant violations. Milestone values can be swapped for a run with `SIM_SET` (V5 §6.2).
