# PERMUTATION STATE — Rules Specification v0.1 (`permutation-rules`)

Status: draft for implementation, 2026-09-24. Implements [Game Design V4](PERMUTATION_STATE_GAME_DESIGN_V4.md).
Audience: implementers of the `permutation-rules` Rust crate, the ER program, the replay verifier, the game client and agent authors.
Normative words: **MUST / MUST NOT / SHOULD / MAY**. Every number here is a ruleset parameter; the full parameter set is serialized and hashed into `ruleset_hash` before a season opens.

> **Principle: one ruleset, two clocks.** Season and Blitz use **identical rules and numbers**, defined per tick. Only the wall-clock length of a tick differs (§1). This is what makes Blitz a faithful balance test for Season.

---

## 0. Conventions

### 0.1 Numeric types
| Quantity | Type | Unit |
|---|---|---|
| Resources (food, production, gold, science, influence, iron, horses) | `u64` / `i64` | **milli-units** (1 unit = 1000) |
| Troop counts | `u32` | **milli-troops** (1 troop = 1000) |
| Multipliers and percentages | `u32` | **basis points (bps)**, 10000 = 100% |
| USDC amounts | `u64` | 6 decimals (1 USDC = 1_000_000) |
| Ticks | `u16` | 0 … 179 |
| IDs | `u16` civ_id, `u32` unit/city/order id | assigned sequentially, never reused within a season |

- **No floating point anywhere.** All fractional formulas in this document are specified as integer operations.
- **Rounding:** every division is integer division with truncation toward zero, applied at the point written. A chain `x × a / b × c / d` is evaluated left to right.
- Displayed values are milli-values divided by 1000 and truncated.

### 0.2 Determinism
- Iteration order is always by ascending ID (civ_id, then entity id). Hash maps MUST NOT determine iteration order.
- **Tick seed:** `seed_t = sha256(season_seed ‖ vrf_t ‖ t)`, where `vrf_t` is the MagicBlock VRF output requested when tick `t`'s order window closes. `seed_t` is therefore unknown while orders are being submitted. Nobody can pre-simulate the variance; that includes agents, humans and the UI.
- `rand(seed_t, domain, id)` = the first 8 bytes (little-endian) of `sha256(seed_t ‖ len(domain) as u8 ‖ domain ‖ id)` as `u64`. Each random draw names its own `domain` string, so draws are independent and reproducible. The one-byte length prefix keeps domains from colliding (`"ab"‖"c"` ≠ `"a"‖"bc"`).
- **Tie-break:** when two entities contend for the same thing (a tile, a capture, a trade fill), priority is given by ascending `rand(seed_t, "tie", id)`.

### 0.3 Coordinates
Axial hex coordinates `(q, r)` with `s = −q − r`. Distance is `(|dq| + |dr| + |ds|) / 2`. Neighbour order: E, NE, NW, W, SW, SE, i.e. `(+1,0) (+1,−1) (0,−1) (−1,0) (−1,+1) (0,+1)`.

---

## 1. Presets

| Parameter | Season | Blitz |
|---|---|---|
| Tick length | 4 h | 30 s |
| Ticks per season | **180** (30 days) | **180** (90 minutes) |
| Order window | the whole tick; it closes 2% before the boundary (4h48s / 0.6s) for VRF request and settlement | same rule |
| Max civilizations | 16 (single region in v0.1) | 8 |
| Map radius | 19 (1,141 tiles) | 13 (547 tiles) |
| Entry closes | tick 30 | tick 0 (before start) |
| Everything else | identical | identical |

### 1.1 Phases (ticks)
| Phase | Ticks | Season | Blitz |
|---|---|---|---|
| Founding | 0–17 | day 0–3 | 0–9 min |
| Expansion | 18–59 | 3–10 | 9–30 |
| Contention | 60–119 | 10–20 | 30–60 |
| Crisis | 120–161 | 20–27 | 60–81 |
| Resolution | 162–179 | 27–30 | 81–90 |

- Transfers between civilizations are frozen for **ticks ≥ 162**.
- The Exchange (USDC) is frozen for **ticks ≥ 120**.
- Scoring closes after tick 179 resolves.

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

### 3.1 Entry
- `join_season(civ_name, declared_kind: Human|Agent|Undeclared, payout_wallet)` is paid in USDC (directly, or via x402 for agents).
- `declared_kind` is cosmetic only (V4 §5.1). No rule reads it.
- At most **2 civilizations per payout wallet** and **2 per ERC-8004 operator ID** per season.
- **Prize coalitions** (§14.4) must be registered, with fixed membership and payout split, before entry closes.

### 3.2 Starting state
| Item | Value |
|---|---|
| Capital | founded automatically on the start site: pop 1, territory radius 1, loyalty 100 |
| Units | 1 Scout; 1 army of 3 Spearmen, placed on the capital |
| Gold | 20 |
| Order bank | 0 |
| Techs | none. Spearman, Scout, Settler, Granary and Workshop are available without techs |

**Late joiners (Season only, joining at tick j ≤ 30):** receive `j × 3` gold and `j × 1` production into the capital's production store.

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

### 4.1 Order budget
- **Budget per tick:** `B = min(3 + cities, 8)`. Cities are counted at the start of the tick; a government in exile gets 3.
- **Bank:** unused budget accumulates up to `4 × B`.
- **Spendable in a tick:** `B + bank`.
- The budget is checked **on chain** at submission (`submit_orders`). An order that is invalid at resolution time is dropped **without refund**. Clients MUST show the order's precondition before it is submitted.

### 4.2 Order catalogue
| Order | Cost | Valid if |
|---|---|---|
| `MoveUnit(unit, path ≤ 12 tiles)` | 1 | the unit is owned and the path is passable. Movement continues over later ticks without new orders |
| `Attack(army, target)` | 1 | at war with the target's owner (or the target is a city-state or barbarian); target adjacent, or within range 2 for ranged units |
| `FoundCity(settler)` | 1 | the site is ≥ 3 tiles from any city, not in foreign territory, not in another civ's protected zone, and not Water or Mountain |
| `SetQueue(city, items[≤3])` | 1 | items are unlocked |
| `SetFocus(city, Balanced|Food|Production|Gold|Science)` | 1 | — |
| `Purchase(city, gold)` | 1 | pays gold for production at 3 gold per production, up to 50% of the current item's remaining cost |
| `SetResearch(techs[≤3])` | 1 | prerequisites are met or queued earlier |
| `DeclareWar(civ)` | 1 | not allied with the target and no NAP in force. Takes effect at the **start of the next tick** |
| `ProposePeace / AcceptPeace` | 1 each | at war |
| `ProposeNAP(civ, bond, 30 ticks) / AcceptNAP` | 1 each | at peace, bond ≥ 30 gold each |
| `BreakNAP(civ)` | 1 | forfeits the bond (§10.3) |
| `ProposeAlliance / AcceptAlliance / LeaveAlliance` | 1 each | §10.4 |
| `SendEnvoy(city_state, influence)` | 1 | enough influence in stock |
| `Transfer(civ, goods)` | 1 | within the caps (§10.5) and tick < 162 |
| `MarketTrade(good, side, amount, limit)` | 1 | gold AMM (§11.2) |
| `ExchangeOrder(...)` | **0** | within the Exchange caps (§11.3) and tick < 120 |
| `Raze(city)` | 1 | the city was captured this tick or the previous one |
| `SetStanding(unit|city, rule)` | 1 | §13. **Execution** of standing rules is free |
| `RevealRationale(tick, text)` | 0 | a commit-reveal disclosure (V4 §5.4), not a game action |

At most **one** manual order per unit per tick. A manual order overrides that unit's standing rule for that tick.

### 4.3 Order commitments
Every `submit_orders` transaction carries `decision_digest = sha256(tick ‖ obs_root ‖ policy_id ‖ rationale_hash)`. `obs_root` is the Merkle root of the fog-filtered view served to that civilization for tick `t`. The digest is stored and never interpreted by the rules.

---

## 5. Cities

### 5.1 Tiles and yields
- A city works its **centre tile plus `pop` tiles** within its territory.
- **Centre yield:** `max(tile food, 2)` food, `max(tile prod, 1)` production, `max(tile gold, 1) + 1` gold, and +1 science.
- **Tile assignment:** the governor assigns tiles deterministically, maximising `Σ yield × weight`.
  - Weights (food, prod, gold) by focus: Balanced (3,2,1); Food (5,1,1); Production (2,5,1); Gold (2,1,5); Science (3,2,1, and Academy output ×12500).
  - Ties go to the lower tile index (q, then r).

### 5.2 Per-city yields per tick
```
food        = Σ worked tile food + buildings
production  = Σ worked tile prod + buildings
gold        = centre gold + Σ worked tile gold + pop / 2 + buildings
science     = 1 + pop / 3 + buildings
influence   = (capital ? 1 : 0) + buildings
```
Everything above is computed in whole units and then stored ×1000 as milli.

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
| Star Gate I / II / III | 200 / 300 / 400 | Astronomy / Physics / Celestial Mechanics | §14.2. **One city per civilization**, publicly visible |

- **Stalemate breaker (pre-committed):** from tick 120, Star Gate stage costs are multiplied by `max(6000, 10000 − 100 × (t − 120))` bps.
- **Production overflow** carries over to the next item. Completed units spawn on the city tile or the first free neighbour; if there is none, the unit is delayed by one tick.

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

### 6.3 Influence
- Influence is a civilization-level stock, produced by cities (§5.2).
- It is spent only on `SendEnvoy` (§12.1). It cannot be traded or transferred.

### 6.4 Strategic resources
- Iron and horses are civilization-level stocks, fed by worked deposits (§2.2).
- They are consumed by T2 troops (§7.1). They can be traded on the gold AMM and the Exchange.

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
- **Public announcements** stay global: war and peace, treaties, captures, revolts, razing, city founding and Star Gate stages (§14.2). Tech discoveries are private to the civilization and its allies.
- **Validation** of orders always uses the full state; an order the belief state made look possible can still fail (for example a move into a tile an unseen army occupies stops, §7.3).
- `obs_root` (§4.3) is the Merkle root of this belief view.

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
| 2 | Ranged attack | 8000 | a ranged attacker at range 1 or 2. **The defender deals no retaliation** |
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
- **Capture:** after combat, if the city defence is 0 and no garrison remains, the city is captured by the attacking **melee or mounted** army with the most troops remaining among those that attacked it this tick. Ties go to the tie-break (§0.2). **Ranged armies cannot capture.**
- **On capture:**
  - pop −1 (minimum 1);
  - Walls and Barracks are destroyed, other buildings are kept;
  - loyalty is set to 50;
  - the city's food and production stores are halved;
  - a Star Gate in progress loses **all** completed stages (§14.2).
  - The capturing army moves into the city.
- **Last-city protection:** a civilization's last remaining city cannot be captured within 12 ticks of that civilization losing its previous city. It can still be attacked.
- **Raze:** `Raze` within one tick of capture destroys the city over 3 ticks. The city becomes a ruin and its tiles become unowned. Razing gives +60 grievance (§11.1).
- **Government in exile:** a civilization with 0 cities keeps its armies, gold, influence, diplomacy and scores, with budget 3. It may found a new city with a Settler, or recapture one.

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
9. **Capture scoring.** Whether a holding can score (§14.1) is decided at capture: the founder must differ and the city must have been founded ≥ 12 ticks earlier. On the first tick the holding becomes eligible (the 30th tick held), it is disqualified if this captor has already scored this city this season. Otherwise the captor is recorded.
10. **Tiles that stop being cities.** When a city becomes a ruin (§8.3 Raze) or a Free City (§9.4), any civilian sharing that tile with an army moves to the first free passable neighbour, in §0.3 order. If there is none, the civilian is disbanded.
12. **Why captures restart at half defence (balance fix, 2026-09-24).** In a six-bot Blitz match with defence reset to 0, the same city changed hands after 1, 1 and 3 ticks: an adjacent army simply walked back in. At 50%, the shortest gap was 2 ticks, and it took a real fight; quick recaptures (≤ 10 ticks) fell from 5 to 2. No capture-immunity window was needed.

11. **Razing timeline.** `Raze` at tick t sets a 3-tick countdown. The countdown advances at the start of phase 5 in ticks t+1 and t+2, and the city becomes a ruin in phase 5 of tick t+3. A city being razed produces nothing.

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
- **Decay:** −1 per tick.

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

Fighting barbarians, defending, and capturing cities of a civilization that declared war on you are **not** aggression. The flag drives Concord eligibility (§14.3).

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
- **Membership is recorded per tick.** Concord's neutrality multiplier (§14.3) needs "never in an alliance".
- In-game alliances are independent of prize coalitions (§14.4).

### 10.5 Transfers between civilizations
| Good | Per-tick cap per sender |
|---|---|
| Gold | `max(10, sender's gold income this tick)` |
| Iron / Horses | `max(1, sender's income of that good)` |
| Food / Production (delivered to a named city of the receiver) | the sender's largest single-city surplus or production this tick |

- **No transfers at tick ≥ 162.**
- Transfers never count for any score.
- *v0.1 details:* "this tick" means the sender's yields from the **previous** tick's phase 6, since transfers resolve in phase 2, before this tick's production. Transfers are not allowed between civilizations at war. Food and Production are taken from the sender's city holding the most of that good (ties: lowest id).

### 10.6 Proposals and timing (v0.1)
- `Propose*` records a proposal. `Accept*` only matches a proposal made on an **earlier** tick, so a proposal and its acceptance can never race within one tick. Proposals expire after 6 ticks. A new proposal of the same kind to the same civ replaces the old one.
- **Peace:** acceptance sets the war's `peace_at = t + 1`; the war is inactive from then on. At the start of phase 1 of that tick, the relation becomes Peace and each side's units in the other's territory move to the nearest free tile of their own territory (by distance, then tile). If no tile is free, the unit stays.
- **NAP:** both bonds (≥ 30 gold each) are escrowed at acceptance. If either civ cannot pay, the pact fails. At expiry both bonds are returned.
- **Alliances are closed groups.** Accepting civ X's proposal joins X's whole group: the joiner must have no allies, the group must be below the cap, the joiner must be at Peace or under a NAP with every member (a NAP is superseded and its bonds returned), and no member may be leaving. `LeaveAlliance` marks all of the leaver's alliances to end after 6 ticks. Membership in any alliance at any tick forfeits the Concord neutrality multiplier for the season.
- **Envoys:** all envoys of a tick are applied first. Then each city-state without a suzerain goes to the civilization with the **most** influence at or above 60 (ties by tie-break), so civ order never decides suzerainty.
- **Founding** (`FoundCity`, phase 2): the settler founds at its own tile if the tile is passable, not foreign territory, ≥ 3 tiles from every city and city-state, and not inside another civilization's active protected zone. The centre tile always belongs to the new city. A civilization without a capital makes the new city its capital.

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

### 11.3 The Exchange (USDC P2P, V4 §4.10)
| Rule | Value |
|---|---|
| Goods | Gold, Iron, Horses, Food→city, Production→city |
| Supply | only other civilizations' stocks. Food and Production come from the seller's named city's surplus or production this tick |
| Matching | once per tick, a **uniform-price call auction** per good (maximum volume; ties by price priority, then tie-break §0.2) |
| Per-tick buy cap | ≤ the buyer's own per-tick income of that good (Gold: gold income; Iron/Horses: max(1, income); Food/Prod: that city's surplus/production) |
| Per-season spend cap | total USDC spent by a civilization ≤ **1 × entry fee** |
| Fee | **5%** of notional, paid by the buyer. Split 80% Vault / 20% operations |
| Re-sale | goods bought on the Exchange may not be sold on the Exchange again in the same season (tracked per civ as `exchange_bought[good]`, which is subtracted from sellable stock) |
| Scoring | Exchange trades score 0 for Concord |
| Window | ticks 0–119. Frozen from tick 120 |
| Settlement | USDC is delegated to the ER. Goods and USDC swap atomically in the resolution step. Balances are committed to Solana with the tick checkpoint |
| Budget | `ExchangeOrder` costs 0 orders |

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

**Exchange**
- USDC reaches the ER as a deposit declared at entry (`exchange_deposit`). Invariant 1 (§17): `Σ civ USDC + Vault + operations = Σ deposits` after every tick.
- Buy orders are clamped, not rejected: to the per-tick cap (summed over the civ's buy orders for that good kind that tick), to the civ's USDC balance at its limit price plus fee, and to the remaining season cap. Sell orders are clamped to sellable stock (stock minus units bought on the Exchange this season).
- One auction per good kind (Gold, Iron, Horses, Food, Production). Food and Production orders name a city: the seller's source city, or the buyer's delivery city; each must belong to that civ.
- Clearing price: maximum volume, then smallest imbalance, then lowest price. Fills and pairing: buyers by price high→low, sellers low→high, ties by `rand(seed_t, "tie", civ ‖ order serial)`.
- Settlement per matched pair: buyer pays `q × P + 5%`; seller receives `q × P`; the fee splits 80% Vault / 20% operations; goods move atomically in the same step.

---

## 12. Neutral actors and the Crisis

### 12.1 City-states
- **Count:** §2.3. Each has pop 3 at tick 0 and gains +1 pop every 30 ticks, up to 6. It never expands or builds units.
- **Defence:** virtual army `(8 + 2 × pop)` troops at strength 10. It does not retaliate against ranged attacks. Regeneration is +2 per tick.
- **Specialty** (random): Scientific (+3 science to its suzerain), Mercantile (+4 gold), or Agrarian (+2 food in the suzerain's capital).
- **Influence:** `SendEnvoy` adds influence points `I[c][cs]`. They never decay within a contest cycle.
- **Suzerainty:**
  - The first civilization to reach **60** becomes suzerain.
  - Suzerainty is **locked for 45 ticks**, then the contest restarts: every civilization's `I` is halved and the lock is removed.
  - Contest cycles start at ticks 0, 45, 90 and 135.
- **Attacking a city-state** is allowed. It sets the aggressor flag and removes the attacker's influence with every city-state.
- **Capturing a city-state** makes it the captor's city. It counts for Dominion and ends that city-state's suzerainty.

### 12.2 Crisis (ticks 120–161)
- **Targets:** every 6 ticks (at 120, 126, …, 156) the rules find the **top 2 civilizations of each track** by cumulative score so far. Duplicates are merged. Prize coalitions count as their members.
- **Wave per target:** a barbarian army of `4 + (t − 120) / 6` troops.
  - Spearmen until tick 137, Pikemen after.
  - Horsemen and Knights alternate by wave index.
- **Spawn:** on a free land tile at distance exactly 4 from the target's most populous city. Choice is by `rand(seed_t, "crisis", civ_id)`; if no tile is valid, distance 5.
- **Behaviour:** barbarians move by the shortest path to that city and attack whatever they meet. Standing rules and players fight them normally. They produce no grievance and no aggressor flag.

---

## 13. Standing rules (first-party automation)

Setting or changing a rule costs 1 order. Execution is free and deterministic, in phase 3 (§15).

| Rule | Behaviour |
|---|---|
| `AutoDefend(radius r ≤ 3)` | attack the weakest adjacent hostile army (lowest troops, then id) if any is within `r` of the city or army's anchor |
| `Retreat(ratio_bps)` | if adjacent hostile total strength × 10000 / own strength > ratio, move one tile toward the nearest own city |
| `Patrol(path ≤ 6)` | loop the path |
| `CityQueueRepeat` | when the queue empties, repeat the last item (default: on) |
| `AutoPurchase(max_gold_per_tick)` | apply `Purchase` to the current item, up to the cap |

Agents and humans get exactly the same set.

---

## 14. Scoring

All scores are **cumulative sums over ticks** (V4 §4.7). Integers, `u64`.

### 14.1 Dominion
```
D_t = Σ owned tiles (1 + [tile has resource])
    + Σ held_captured_cities (5 × pop)       if held for ≥ 30 consecutive ticks at tick t
```
A captured city is eligible only if all of the following hold:
- it was founded by a different civilization at least 12 ticks before capture;
- that civilization is not a member of the captor's prize coalition;
- the city was not obtained by transfer;
- the city has not already been scored by this captor this season.

Points start at the 30th tick held. They are not retroactive. Captures after tick 149 can never score.

### 14.2 Science
- **Ranking key:**
  1. Star Gate stages completed (descending);
  2. the tick the last stage completed (ascending);
  3. cumulative science produced (descending).
- If the Star Gate city is captured, all stages are lost; the stage tick record resets.

### 14.3 Concord
```
C_t = 0                                                   if Aggressor in tick t (§9.2)
C_t = total_pop_t
    + 10 × max(0, total_pop_t − max_pop_before_t)         (new population highs only)
    + 5 × suzerain_count_t                                otherwise

Concord = Σ_t C_t × M / 10000,   M = 12500 if the civ was never in an alliance this season, else 10000
```
- Gold-AMM trades have no counterparty and never score. Transfers never score. Exchange (USDC) trades never score (§11.3).
- **v0.2 (deferred, §18):** direct P2P gold trades add `trade_pts_t = Σ_x isqrt(value_gold_x) / (1 + k_x)`, where `k_x` is the number of earlier scored trades with the same counterparty `x` this season. This decays repeated trades with one partner so they cannot farm wash-trading.

Being attacked never sets the flag, so griefers cannot take Concord away.

### 14.4 Prize coalitions
- Registered before entry closes (§3.1), with 2 to 3 members and a fixed payout split in bps.
- A coalition appears on every track as **one entry**:
  - `score = Σ member scores × 10000 / (10000 + 2500 × (members − 1))`;
  - Science uses the best member's key.
- Members do not also rank individually.

### 14.5 Payout computation (the track split is ON HOLD)
- **Split:** `track_share_bps[Dominion, Science, Concord, Participation]` is a ruleset parameter. It is **undecided** (V4 §12). The devnet placeholder is 3000 / 2500 / 3000 / 1500.
- **Winners per track:** `N = clamp(entrants × 2000 / 10000, 3, 50)`.
- **Weights:** rank `k` (1-based) gets `w_k = 7500^(k−1)` in bps-power, computed iteratively as `w_1 = 10000`, `w_{k+1} = w_k × 7500 / 10000`. Its share is `pool_track × w_k / Σ w`.
- **One top-3 per entry:**
  1. Compute all tracks.
  2. For any entry holding more than one top-3 placement, keep the placement with the highest amount (ties: the earlier track in the order Dominion, Science, Concord). Remove the entry from the other tracks' top-3 and recompute those tracks.
  3. Repeat until stable. This takes at most 3 iterations.
- **Participation:** an equal split among entries that submitted ≥ 1 order in ≥ 60% of ticks since joining **and** placed in the top 50% of at least one track. The per-entry amount is capped at `2 × entry fee`; the remainder rolls over.
- **Claims:** each leaf is `claim_leaf_hash(season_id, payout_wallet, index, amount)`. Coalition amounts are split to member wallets by the pre-registered bps. Remainders from integer division go to rollover.

---

## 15. Tick resolution order

`resolve_tick(t)` MUST execute these phases in order. Within each phase, iteration is by ascending id.

| # | Phase | Contents |
|---|---|---|
| 0 | Seed | reveal `seed_t` (§0.2) |
| 1 | Diplomacy | war declarations and peace from tick t−1 take effect; NAP and alliance accepts; breaks; leave-alliance timers |
| 2 | Economy orders | SetQueue, SetFocus, SetResearch, Purchase, Transfer, SendEnvoy; gold-AMM batch (§11.2); Exchange auction (§11.3) |
| 3 | Standing rules | compile standing rules into implicit orders for units without a manual order |
| 4 | Movement | sub-steps s = 1 … 3 (max MP). In each sub-step every unit with remaining MP attempts one step. When several units target the same free tile, tie-break (§0.2) picks one and the others wait for that sub-step. Units never swap through each other |
| 5 | Combat | collect all attacks (manual + implicit + barbarian); compute all damage from pre-combat counts; apply; remove dead armies; resolve captures (§8.3) |
| 6 | Production and growth | yields, food, growth or starvation, production completion and spawning, research, influence, strategic income and depletion |
| 7 | Upkeep | gold, upkeep, deficit disbanding (§6.1) |
| 8 | Society | amenities, war weariness, loyalty (Free City flips), grievance decay, aggressor flags, Star Gate stage checks |
| 9 | Neutral actors | city-state growth and regeneration, suzerainty cycles, Crisis spawns (§12.2), barbarian movement intents for t+1 |
| 10 | Scoring | accrue D_t, C_t and cumulative science; record alliance membership |
| 11 | Commit | `state_root = sha256(borsh(state))`; append the event log; emit the checkpoint every 6 ticks |

### 15.1 Compute budget
If phases 4–10 exceed the ER transaction compute limit for the preset's maximum number of civilizations, `resolve_tick` MUST be split into consecutive transactions at phase boundaries. The **phase index is stored in state**, so the sequence can resume permissionlessly. A tick is complete only when phase 11 is committed. Orders for tick t+1 are accepted only after that.

### 15.2 Permissionless liveness
If `resolve_tick(t)` has not completed within one tick length after its boundary, **any** signer may call it. The result is identical regardless of who calls it.

---

## 16. Season Law menu (v0.1)

Each track winner (§14.5 rank 1, or a coalition) picks one item. Picks are applied to the next season in the order Dominion, Science, Concord. The same item picked twice applies once.

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

## 17. Invariants (MUST hold after every tick; checked in tests and the replay verifier)

1. Sum of all civilizations' USDC Exchange balances + Vault + operations = USDC delegated to the ER (no creation or loss). No civilization's Exchange spend exceeds the season cap.
2. No tile holds more than one army or civilian unit, except a city tile (1 army + 1 civilian).
3. Every army holds 500–20,000 milli-troops.
4. Orders applied this tick per civilization ≤ its spendable budget at submission.
5. No resource stock is negative.
6. Scores never decrease.
7. The same state and orders give the same `state_root` on the ER program, the native verifier and WASM.
8. No Transfer is applied at t ≥ 162, and no Exchange fill at t ≥ 120.
9. A civilization with an aggressor flag in tick t accrues C_t = 0.
10. Technology counts never decrease, and no technology is ever held without its prerequisites.

---

## 18. Deferred to v0.2+

- Frontier settlement seat (V4 §4.3)
- Vassalage
- Direct P2P gold trades (and with them Concord `trade_pts`)
- Multi-region Season maps
- Roads
- Naval units
- Espionage / sabotage other than capture
- Heritage multipliers beyond §3.4

---

## 19. Balance simulation notes

These numbers were checked with a simplified single-civilization simulation (worked-tile average 2.2 food / 0.9 prod; no conflict). Scripts: [research/econ3.py](research/econ3.py) (economy) and [research/combat.py](research/combat.py) (test vectors).

| Profile | Cities | Pop @90 | Pop @179 | Gold income @120 | Upkeep @120 | Star Gate stages done |
|---|---|---|---|---|---|---|
| Tall science | 4 | 31 | 49 | 38 | 7 | ticks 117 / 137 / 157 |
| Wide builder (20 troops) | 8 | 53 | 96 | 72 | 26 | 112 / 135 / 156 |
| Wide military (60 troops) | 8 | 53 | 96 | 72 | 76 | 112 / 135 / 156 (gold stays positive) |

Star Gate I lands just before the Crisis (tick 120), and stage III lands inside the Crisis window (ticks 120–161). A science leader is therefore a visible Crisis target while completing it, which is the intended counterplay.

**Findings that shaped v0.1:**
- **City upkeep:** `(C−1)²/2` bankrupted an 8-city expansion by tick 60. It is now `/3`.
- **Science:** `pop/2` science let a wide civilization finish the tree by about tick 75. It is now `pop/3`, and tech costs rise 10% per city. Tall and wide now reach the Star Gate within a few ticks of each other, and the last stage lands in the Crisis window, where it can be contested.
- **Gold sinks:** gold piles up without sinks. `Purchase` (3 gold per production), NAP bonds and T2 troop upkeep are the sinks. **Verify in Blitz that gold is not dead weight by tick 120.**

**Tuning knobs to watch first in Blitz playtests:**
1. Human vs agent budget utilisation (V4 §4.2)
2. The Exchange per-season cap (1 × entry fee)
3. Walls vs siege (test vector d)
4. The Crisis wave size
5. Concord weight of new population highs
