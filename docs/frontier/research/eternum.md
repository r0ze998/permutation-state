# Eternum (Realms.World / BibliothecaDAO): research for an open-world PERMUTATION STATE

Research date: 2026-09-27. Primary sources:

- The `BibliothecaDAO/eternum` repository at `main` commit `a375655955d08b96201e40500a52ddc6a9278b48` (2026-09-24). A shallow clone is in `scratchpad/openworld/lab/eternum-src/eternum`.
- Two historical snapshots fetched by commit date:
  - Season 0 era: `99988680da8874fa28b7bf451c88a31ffe6ad14f` (2024-12-19), in `lab/eternum-src/s0`.
  - Season 1 era: `d3dcb46788672f15753e08a357c4879caad4cd7a` (2025-05-31), in `lab/eternum-src/s1`.
- The player docs, which ship in the repo (`apps/game-docs`) and are published at docs.realms.world.
- The team's own internal briefs in `docs/plans/*.md`, dated Aug–Sep 2026.
- Press coverage (Bankless, ChainPlay, Bitrue, Sovereign Frontier).

Path conventions:
- **Code paths** are relative to `contracts/l3/game/src/` unless they are given in full.
- **Brief paths** are relative to `docs/plans/`.
- **Doc pages** are cited by their path in docs.realms.world.

Confidence tags:
- **[code]** means I read it in source.
- **[docs]** means official player docs.
- **[brief]** means the team's internal design notes and measurements.
- **[press]** means third-party coverage.
- **[unverified]** means a claim I could not trace to a primary source.

---

## 0. Executive summary (the ten things that matter for us)

1. **Eternum has no global tick. Every piece of state is per entity and lazily evaluated from timestamps** [code].
   - Production, stamina, battle timers, hyperstructure points and faith points each store a rate and a last-updated time. The value is "harvested" or "refilled" the first time any transaction touches the entity (`ProductionImpl::harvest`, `StaminaImpl::refill`).
   - "Days", "phases" and "ticks" are just `block_timestamp / interval`. Nobody cranks them.
   - This is the single design choice that decouples cost per transaction from the number of players.
2. **The map is conceptually infinite and physically sparse** [code].
   - A tile is a 128-bit packed row (`TileOpt`, keyed `(game_id, alt, col, row)`), written only when an army explores it.
   - The biome is a pure function of `(col, row, seed)` using simplex noise, so undiscovered tiles cost nothing.
   - Coordinates are `u32`, centred near 2³¹ (`SETTLEMENT_CENTER = 2147483646`).
   - Realm spawn slots are generated lazily in expanding hex rings as players arrive. The map "grows" by exploration and by settlement demand, not by an admin resize.
3. **Combat was redesigned from "battles over time" (Season 0) to "instant local exchanges" (Season 1 onward)** [code].
   - Season 0 battles were joinable, multi-army and lazily evaluated, with an 8 h siege delay, a 2-day cap and resource escrow.
   - Season 1 replaced them with instant one-shot damage between adjacent units, gated by stamina and per-army battle-timer cooldowns.
   - The complex model was abandoned for gameplay and engineering reasons.
4. **"Anyone can join" in Eternum is capped by NFTs, not by compute** [docs][code].
   - There are 8,000 Realms. Each can mint one Season Pass per season; the pass is burned to settle.
   - Up to 6 Villages can sit around each settled Realm, so the ceiling is 48,000 Villages.
   - Villages are the cheap, casual, non-conquerable entry path. Mid-season joining is allowed, but there is no protection after day 1.
5. **Actual concurrency was hundreds to low thousands, not tens of thousands.**
   - Season 0 (Dec 2024) had about 800 sign-ups or players [press]. It reportedly used about 36% of Starknet transactions [press], and one unverified quote says 50% of blockspace and 75% of compute.
   - Season 1 was marketed as "20× capacity" [press]. I found no published Season 1 player count.
6. **The team retreated from "one big world on a public L2".**
   - Blitz is the competitive format: 60-minute games with 24-player lobbies (live preset), and 96-player lobbies on their own chain.
   - The stack is moving off Starknet mainnet to a self-hosted, fee-free Madara L3 ("gameplay chain holds nothing worth stealing"). Value (entry fees, prizes, MMR) stays on Starknet L2 through a relay [brief].
   - Their own measurement: one laptop Madara carries about 2 concurrent 96-player games at about 13 tx/s, and the wall is about 25 tx/s. Merklization costs about 10 ms per transaction, serially [brief].
7. **Their indexer (Torii) is being replaced by a per-game "fold" server ("herald")** [brief].
   - Herald sends a snapshot plus sequence-numbered diffs over WebSocket, built from pre-confirmed blocks.
   - The reasons: Torii polling added about 1–1.5 s of user-visible latency, while the chain answered in 50–77 ms. Client-side optimism also produced bugs such as negative wheat balances.
   - Cartridge (Katana, Torii, Controller, paymaster, VRF) is described as end-of-life [brief, owner input].
8. **Randomness comes from Cartridge VRF, used synchronously inside the same transaction** [code][docs].
   - It is scoped by `Source::Salt(tile_seed)`, so exploring the same tile always gives the same result and players cannot re-roll. Other actions use `Source::Nonce(caller)`.
   - The VRF key holder can predict outcomes. The provider is a trusted liveness and privacy party.
9. **End-of-season settlement for N players is paged and verified, never looped on-chain** [code].
   - A caller submits pages of players sorted by points. The contract asserts non-increasing order and no duplicates.
   - The final page must match the stored global totals (count = registrations, Σpoints = `SeasonPrize.total_registered_points`).
   - Hyperstructure share points are checkpointed in pages (`checkpoint_share_points(start_index, count)`).
   - Their own Aug 2026 audit found a **roster-substitution bug that permanently locks the prize pool (B1)** and **no refund path after a game starts (F1)**. These are the same class of bug as our audit's "funds lock / permissionless freeze".
10. **Governance is social, not constitutional.**
    - Tribes (guilds) are public/private groups with a whitelist. Contracts do not do elections.
    - Coordination levers are hyperstructure construction access (Public / Private / GuildOnly) and owner-chosen VP share splits.
    - Season 1 tribal prizes gave the **tribe leader 30%** off the top.
    - The newer "Faith" system lets structures pledge allegiance to one of 50 Wonders, with rate-based points and a prize pool. That is the closest analogue to our "six factions".
    - AI agents (Daydreams) are **openly labelled** on-chain (distinct `TileOccupier` variants), not hidden.

---

## 1. Game design

### 1.1 Modes and seasons

| | Eternum (seasonal sandbox) | Blitz (competitive arena) |
|---|---|---|
| Length | "multi-week" seasons. S0 ran 2024-12-09 → ~2025-01-06. S1 phases: pass 2025-04-30, settling 2025-05-07, empire 2025-05-14, open-ended until VP threshold [press] | Exactly 60 min (was 90). Eternum Day = 6 min, 10 days per game [docs `blitz/world-physics`] |
| Entry | Burn a Season Pass (free to Realm NFT holders, tradable), or a Village Pass [docs `eternum/game-entry`] | Register per game. Brackets: Recruit (free), Gladiator (LORDS fee, e.g. 250), Warrior (series), Elite (NFT invite) [docs `blitz/game-entry`] |
| Pieces per player | Any number of Realms or Villages you hold | 3 identical Realms in a triangle |
| Lobby size | Up to 8,000 Realms + 48,000 Villages by construction | 24 on the live preset, 96 on Madara preset. Map size and hyperstructure rings are derived from the registration count [docs][brief] |
| Win | First player to reach the VP threshold may call End Season. S0 threshold 9,620,000 VP in code (6,320,000 per Bankless). The current config has `pointsForWin = 0`, i.e. no win condition configured [code][brief] | Ranking by VP at 60 min, with a prize curve by rank |
| Persistence | Materials are ERC20s bridgeable in and out (with fees). Conquest does not transfer the underlying NFT | None: fresh world per game. Only MMR and cosmetics persist |

Time model [docs `eternum/world-physics`, code `models/config.cairo` `TickConfig`]:
- An "Eternum Day" is 1 real hour, divided into 6 phases of 10 min.
- Stamina is granted per phase, and resource arrivals open on day boundaries.
- In code there are two intervals: `armies_tick_in_seconds` (3,600 s in S0/S1 configs; 60 s in the 2026 base config) and `delivery_tick_in_seconds` (180 s).
- Both are computed from `block_timestamp`. There is no scheduler.

### 1.2 How people join (and the scale ceiling)

**Realms (high stakes).**
- 8,000 NFT Realms. Each mints one Season Pass per season, which is burned to settle.
- Metadata fixes 1–7 producible resources and one of 16 Orders. 50 Realms are Wonders: +20% production to all structures within 12 tiles, non-stacking.
- Realms can be conquered. Conquest transfers in-season control, not the NFT.
- A 7-day settling window came before the S1 game started. Everyone was immune for the first 24 h of the game; there was no immunity after that, and late settlers are advised to scout with spectator mode [docs `eternum/realm-and-villages/realm`].

**Villages (low stakes).**
- Minted with a Village Pass (press says bought with stablecoins; the minting role sits off-chain, and the price is not in the repo).
- Placed next to a chosen or random settled Realm, in one of 6 directions. That Realm becomes the parent.
- One random resource, **50% production rate**, max level City.
- Can be raided but **never conquered**.
- Can receive troops **only from the parent Realm**, "to prevent … spawning a Village on their rival's Realm and 'teleporting' troops there by using the donkey network".
- Bridging out goes through the parent's portal, with a 5% fee to the Realm owner [docs `eternum/realm-and-villages/villages`, `resources/bridging`].
- 6 × 8,000 = 48,000 Villages max.

**Season Pass marketplace.** Non-holders buy passes from holders. S0 passes cost about $6 of LORDS [press Bankless 2024-12-12].

**Mid-season joining.** Allowed. Settlement slots are drawn randomly from an **open pool that is refilled on demand in expanding rings** [code `systems/realm/season/contracts.cairo`, `systems/utils/settlement.cairo`]:
- `fill_open_settlement_pool` generates the next hex-ring positions until the pool holds a target number of open slots: 6 while fewer than 3 players have settled, 9 while fewer than 15, then a step constant.
- `claim_open_settlement` picks one uniformly with VRF, swap-removes it, and retries up to 64 times if an army has since occupied the tile.
- The settled region therefore grows outward only as demand arrives.
- (The team's own brief flags that `fill_open_settlement_pool` is unbounded per call.)

**Implication.** Eternum is "any number can join" only up to the NFT supply. The real throughput limit (Starknet mainnet in S0/S1) was never hit at 8,000 Realms; participation was hundreds to low thousands.

### 1.3 Resources, production, labor, storage

- **Materials** [docs `eternum/resources/*`]:
  - 22 resources in rarity tiers (Wood … Dragonhide).
  - 2 foods (Wheat, Fish).
  - Troops: 3 types × 3 tiers.
  - Donkeys (single-use transport, 500 kg each).
  - Labor (non-transferable).
  - Ancient Fragments (only from Fragment Mines).
  - Relics and Essence (newer).
  - All but Labor are ERC20-bridgeable.
- **Production** [docs, code `models/resource/production/production.cairo`]:
  - Only food is free: Farms and Fishing Villages produce indefinitely.
  - Everything else needs inputs. *Standard mode* uses 2 other resources plus food. *Simple mode* uses food plus Labor: less efficient, but accessible to small holders.
  - Labor comes from burning resources in the Keep; rarer inputs give more Labor.
  - **S1 change:** inputs are committed *upfront* to a production order (`output_amount_left`), and output accrues at `rate × seconds` until exhausted. S0 streamed inputs continuously. The upfront model makes lazy accrual a two-variable closed form.
- **Lazy accrual in code** [code `harvest`]:
  - `produced = min((now - last_updated_at) × production_rate, output_amount_left)`, then `last_updated_at = now`.
  - `SingleResourceStoreImpl::retrieve` calls `harvest` before any balance read, and storage capacity clamps it.
  - This is the pattern we would copy.
- **Buildings** [docs `eternum/realm-and-villages/buildings`]:
  - Placed on "buildable hexes" inside a Realm's local hex grid ("hexes within hexes"). The grid grows with level: Settlement → City → Kingdom → Empire.
  - Population capacity: Keep 5, +5 per Worker Hut. Resource buildings cost 2 pop, military 3.
  - **Quadratic duplicate cost:** `cost = base × (1 + 0.5 × (N-1)²)`, an anti-snowball, anti-whale throttle per structure.
- **Storage.** Weight-based: every material has grams, and Storehouses add capacity. Overflowing production is wasted. S1 world structures have fixed caps.
- **Automation.** Runs **in the player's browser** (the tab must stay open), with 1-minute cycles, production orders and transfer orders. The chain stays simple and players become their own keepers [docs `resources/automation`].

### 1.4 Transport and "arrival gates"

- Transfers use donkeys through an "invisible donkey network". They take time proportional to distance, cannot be cancelled, and must be *claimed* on arrival.
- **Load fix in S1.** Arrivals are bucketed. The team said "resource arrivals are restricted by 'gates' to assist in reducing the load that resource arrival entities have on the game".
  - In code: `ResourceArrival` is keyed `(game_id, structure_id, day)` with 48 fixed slots, one per delivery tick [code `models/resource/arrivals.cairo`].
  - Unbounded per-transfer entities were replaced by at most 48 rows per structure per day. This is a direct lesson in bounding per-entity state growth.
- **Donkeys cost LORDS to produce** in Eternum ("donkeys are the 'gas' of this onchain world"). This is an in-game sink and an anti-spam cost on logistics.

### 1.5 Troops, armies, stamina, movement

- **Armies** [docs `eternum/military/*`, config `config/source/eternum/troop.ts`]:
  - One troop type and tier per army. **One army per hex.** Field armies deploy onto the 6 hexes adjacent to their structure; guard armies fill defence slots.
  - Max army size is 30,000. Deployment caps by level: 3k / 15k / 45k / 90k.
  - Field army count is bounded by level plus military buildings.
  - Deployed troops cannot be re-tokenized.
- **Tiers.** T2 = 2×T1 as input and 3× strength; T3 = 2×T2 and 9× strength (standardised in Combat v3, 2026-06).
- **Stamina** (lazy) [code `models/stamina.cairo`]:
  - Stored as `amount, updated_tick`. Refilled on touch: `min(amount + ticks_passed × gain_per_tick, max)`.
  - Costs: explore 30, travel 20 per hex (biome-modified), attack 50, defend 40 (defending below 40 gives a −30% damage penalty).
  - Max 120 in the 2026 config. Docs describe +2 per 10-minute phase in the S1 Eternum balance.
  - Stamina is the action-rate limiter. The team's 96-bot harness found the fastest legal cadence was **one action per bot every 16 s because stamina binds** [brief phase-1 D.4.1].
- **Food upkeep.** 0.03 wheat or fish per troop per hex travelled or explored.
- **Movement.** A multi-hex path is allowed only over explored, unoccupied tiles. Exploring is one hex per transaction.

### 1.6 Exploration, fog, map growth, biomes

- At season start only the **6 Banks** (ring, equidistant from centre) are visible. Settling reveals the 6 neighbours and 6 village slots. Explored tiles are **revealed permanently to everyone** [docs `worldmap-movement/worldmap`].
- **Biomes (16 types).** Deterministic procedural generation: two simplex-noise fields (elevation octaves and moisture) with per-game seed, scale and bias, then a Whittaker-style lookup [code `utils/map/biomes.cairo`, `system_libraries/biome_library.cairo`].
  - Consequence: "fog" hides only *what was found* (structures, rewards), not terrain. Anyone can compute every biome off-chain. Fog is a state flag (`biome == 0` means undiscovered), not secrecy.
- **Explore transaction** [code `systems/combat/contracts/troop_movement.cairo`]:
  1. Assert the target is not occupied and not discovered.
  2. Write the tile's biome.
  3. Grant exploration VP.
  4. Draw VRF with `Source::Salt(hash(game_id, game_seed, coord))` and run discovery lotteries in order: relic chest → hyperstructure → mine → village/camp/agent.
  - If something is found, the army is bounced back to its origin tile. It also gets a random resource stack, capped by carry capacity.
- **Discovery odds** [docs `eternum/world-structures`, S1 config]:
  - Hyperstructure foundation: `p = base × 0.975^distance_from_centre − 0.001 × found_so_far`, with base 4% in the docs and 2% with a 0.982 multiplier in the S1 config. So foundations cluster within about 300 tiles of centre and dwindle as they are found.
  - Fragment mine: 1/150 in the docs, 1/200 in the S1 config. Contains 300k–3M fragments and depletes.
  - Agent spawn: 5% (S1), with a cap of 1,000 concurrent agents and 10,000 lifetime.
- **Relic chests.** A global cooldown means that when enough time has passed, an exploration anywhere spawns a chest at a distance.
- **Map extent.** Unbounded in practice (u32 coords around 2³¹). Blitz sizes the arena by player count instead: one central hyperstructure, then rings every 15 hexes (docs formula `(P/6)^(1/2)`) until everyone fits, with realms 8 hexes from a hyperstructure and from the nearest foe.

### 1.7 Combat resolution

**Season 0 (Dec 2024): battles over time** [code S0 `models/combat.cairo` `Battle`, `systems/combat/contracts/battle_systems.cairo`, S0 constants]:
- A `Battle` entity has attack and defence army aggregates, health, per-second `attack_delta`/`defence_delta`, `duration_left` and `last_updated`.
- `update_state()` lazily subtracts `delta × seconds`. Any army can `battle_join` either side, which recomputes the deltas.
- `battle_leave` slashes 25% of troops. Resources are locked into escrow during battle.
- Constants: `BATTLE_DELAY_SECONDS = 8 h` (siege warning before a structure fight), `TROOP_BATTLE_MAX_TIME_SECONDS = 2 days`, grace 24 ticks.
- This is elegant and lazy, but hard to reason about, and it creates long-lived shared hot entities that many parties touch.

**Season 1 onward: instant exchange** [code current `models/troop.cairo`, docs `eternum/military/damage`]:
- An attacker adjacent to a target (Crossbowmen at range 2 since 2026-06) spends 50 stamina. The defender spends 40.
- Both damages are computed simultaneously with a Lanchester-like formula:
  `damage_A = k × N_A × tier_A × biome_A × stamina_A × timer_A / tier_B / (N_A+N_B)^β`, with β = 0.2, biome ±30%, and a 1–20% VRF die per side.
- Casualties are applied at once. Each side gets a **battle timer** (one phase); while it runs the army cannot initiate, and takes 15% less damage if attacked.
- **Damage-ratio refunds.** If one side's damage ratio is ≥ 10, it gets a full stamina refund and no timer. Between 2.5 and 10 the refund is linear. The stated goal is to "prevent smaller, weaker armies from indefinitely stalling larger forces", i.e. anti-chaff and anti-grief.
- **Structures.** Guards fight slot by slot from the outermost. Destroyed slots can't be refilled for a resurrection delay (10 min in the 2026 config). With all guards dead, an adjacent field army can **claim** a Realm (never a Village).
- **Raids.** Steal without defeating the guards. Success is 0% below 50% of guard damage, 100% above 200%, linear between. Damage is scaled to 10%. The richest materials are taken first, up to carry capacity. LORDS can only be taken by conquering.
- **Press summary of the change:** "Transitioned from timer-based to real-time battles where 'everything happens instantly'"; "stamina system limiting attack frequency to prevent spam strategies" [ChainPlay].

### 1.8 Tribes (guilds), cooperation, "Faith"

- **Tribes** [code `models/guild.cairo`, docs `eternum/tribes`]: `Guild{public, name, member_count}`, `GuildMember`, `GuildWhitelist`. That is the whole contract surface. No roles, votes or treasury. Alliances and wars are social.
- **Hyperstructure cooperation** [code `models/hyperstructure.cairo`]:
  - Construction access can be `Public`, `Private` or `GuildOnly`.
  - Contributors earn **construction VP** in proportion to what they contribute; contributions can come directly from Realms or Villages without donkeys.
  - The owner earns **accumulation VP per second** and sets `HyperstructureShareholders` (list of address → basis points) to share it.
  - VP, once awarded, cannot be lost.
- **Faith (2026, newer)** [code `models/faith.cairo`, `systems/faith/contracts.cairo`, config `economy.ts`]:
  - Any Realm or Village can `pledge_faith` to one of the 50 Wonders. A Wonder must self-pledge first and can "submit" to another Wonder.
  - Faith points accrue per second: Wonder 50, Realm 10, Village 5 (×10 precision in config). 30% goes to the wonder owner and 70% to the pledger, and each party claims lazily.
  - A season-end `FaithPrizePool` is split between the top Wonder and its followers. Wonder owners can blacklist pledgers.
  - This is an **open-membership faction mechanic with no member loop**: all per-member state is keyed `(game, player, wonder)`, and totals are updated as rate deltas.

### 1.9 Hyperstructures, victory points, prizes

- Foundations are found by exploring. Activating one needs Ancient Fragments deposited (20 units × precision in current config). The construction bill is randomised within per-resource min/max ranges across all 22 resources, plus 50,000,000 Labor (S1 docs).
- **S1 prize pool:** 1,000,000 LORDS + 100,000 STRK (about $40k per press), split four ways [docs `eternum/prize-pool`]:
  - **Victory:** 300k LORDS + 50k STRK, plus 2.5% of LORDS bridging volume. Top 10 **tribes** by summed member VP, 30/18/12/9/7/6/5/5/4/4%. Inside a tribe, **the leader takes 30% regardless of VP** and members split 70% by VP.
  - **Achievements:** 300k, proportional to quest points. Details were withheld until launch "to prevent planned farming". Quests were later "temporarily disabled".
  - **Daydreams agents:** 250k + 25k STRK. Each agent carries 10–35 LORDS, won by combat or persuasion via chat. The first 100 players to kill 10 agents got a STRK bounty.
  - **Arts & Emissaries:** 150k + 25k STRK, discretionary.
  - "All Season 1 prizes will be **distributed manually** by the development team". The same page says a snapshot is taken "once a player triggers the end of the game (**or the game contract breaks**)".
- **Blitz prizes** [docs `blitz/prize-pool`]:
  - Entry fees split 70% pool / 15% veLORDS / 15% dev.
  - Winner count `W = min(ceil(N·r), scorers)`, with r from a lobby-size formula (2–60%). Geometric decay `s(N) = 0.3 + 0.64(1 − N^−0.7)`.
  - Claims are "permissionless — anyone can submit the ranked player list" (now restricted to the registrar after audit B1).
  - A lone registrant gets a full refund.
  - 2026 redesign: presets with immutable `payout_bps[]` on an L2 ledger; the recommendation is to pay the top ~20% with rank 1 ≈ 5× entry [brief value-plane §7].

### 1.10 Bank / AMM and fees

- **Six Banks**, garrisoned by bandits at start. The conqueror receives the **owner fee** on trades routed through that bank. Liquidity is shared across banks, and trades auto-route to the nearest bank.
- Constant-product market per resource vs LORDS (`Market{lords_amount, resource_amount, total_shares}` keyed `(game_id, resource_type)`), with LP shares [code `models/bank/market.cairo`].
- **Fees are 15% LP + 15% owner** (S1 and current config), and the docs warn that fees are "relatively high". There is also a player orderbook with lower fees (`TRADE_MAX_COUNT = 10` open offers).
- **Bridging fees (S1):** deposit and withdraw each take 2.5% veLORDS + 2.5% season pool + 2.5% client + 5% realm. Portal "inefficiency" burns scale down as more Hyperstructures are completed.
- The 2026 value-plane design admits "**Eternum has no bridge**". The in-world bridge `transfer_from`s on deposit and **mints when short** on withdraw [brief value-plane §0]. The new design uses a capped, 24 h-delayed, guardian-cancellable release, with proofs later.

### 1.11 Anti-whale / anti-bot / anti-grief measures (explicit and implicit)

| Measure | Mechanism | Source |
|---|---|---|
| Action rate limit | Stamina per tick; attack/defend thresholds; food upkeep per troop-hex | code, docs |
| Snowball throttles | Quadratic cost for duplicate buildings; deployment caps by level; one army per hex; field-army slots | docs, config |
| Anti-teleport | Villages accept troops only from their parent Realm | docs |
| Anti-stall | Damage-ratio refunds; battle timers; guard resurrection delay | docs, code |
| Newcomer protection | 24 h global immunity at start; Villages cannot be conquered; spectator scouting | docs |
| Farming prevention | Achievement specs hidden until launch; exploration VP cut repeatedly in Blitz balance passes (50→25→10→5 VP per tile) | docs, changelog |
| Economic sinks | Donkeys cost LORDS; 15% + 15% AMM fees; bridge fees; labor is non-transferable | docs, config |
| Sybil | Eternum: none beyond NFT scarcity (pass = entry ticket). Blitz: "each wallet address can only be registered once per game" plus an entry fee; MMR for matchmaking. No identity or proof-of-personhood | docs |
| Bots | Explicitly *welcomed*: "Agent-Native Environment … Humans and AI agents play … under the same chain-enforced ruleset"; client automation; an official headless agent ("Axis") | docs `overview/introduction`, changelog 2026-03-28 |
| Operator neutrality | "Nothing, not even the developers, can stop or alter the course of a season once launched". In practice prizes were distributed manually, and audit A1 found `create_game` permissionless with a caller-chosen `dev_mode_on` | docs, brief audit |

Press claims about "Season 3/4" bot waves and guild concentration ("top 15 guilds control 60% of Realms") appear only in one Medium post that I could not load. Its season numbering does not match the official record, so treat it as **[unverified]**.

### 1.12 Onboarding

- **Cartridge Controller:** a passkey smart wallet with session keys and a paymaster (gasless); Discord and Gmail login.
- Simple mode (Labor) for small holders. Villages as a casual entry. A mobile companion app (S1 press).
- A "Recruit" free Blitz bracket runs multiple times per day.
- A spectator mode.
- The 2026 rebuild has one web identity (Sign-in-with-Starknet), gameplay burner accounts on the L3 bound to the L2 wallet, and a deleted "guest" path. The guest path was deleted after a hidden guest account settled three realms "while not connected" [brief phase-1].

---

## 2. Technical architecture

### 2.1 Stack evolution

| Period | Chain | Indexer / client | Randomness / accounts |
|---|---|---|---|
| S0 (Dec 2024) | Starknet mainnet, Dojo world | Torii (SQL/GraphQL/gRPC), RECS client mirror | Cartridge Controller, VRF |
| S1 (May 2025) | Starknet mainnet (`s1_eternum` namespace) | Torii with `pending = true`, 500 ms polling | Cartridge VRF synchronous, paymaster |
| Blitz 2025–26 | Cartridge Katana **appchain** (fee-free), cap 24 players | Torii | Cartridge |
| 2026-08 → | **Self-hosted Madara L3**, 2 s blocks, 250 ms pre-confirm, `--no-charge-fee`. Value on Starknet L2 through `apps/operator` relay | **Herald**: fold of world events into snapshot + ordered diffs over WS; Torii deleted | VRF provider `0x0` falls back to tx-hash randomness on the lab chain; Cartridge VRF is kept only for L2 chest opening "until the VRF is replaced" |

Sources: README, `contracts/l3/game/torii-mainnet-game.toml`, phase-1, phase-2 and value-plane briefs.

Stated reason for the L2/L3 split [brief value-plane §1]: "**Value lives on L2 where identity lives; the L3 holds game state and nothing worth stealing**". Every crossing is either an *entitlement* (L2→L3: registration, deposit, loadout), relayed idempotently after `ACCEPTED_ON_L2` + 1 block, or an *outcome* (L3→L2: ranks, withdrawals), posted by an operator key now and by validity proof later.

### 2.2 ECS models (Dojo) and per-entity state

Every model is keyed by `game_id`. One world contract hosts many concurrent games through a factory. Representative models [code `models/*`]:
- `Structure(game_id, entity_id)`: category, owner, coord, level, guard slots α/β/γ/δ with per-slot `destroyed_tick`.
- `Resource(game_id, entity_id)`: **one wide row** with about 60 `*_BALANCE: u128` and about 40 `*_PRODUCTION: Production{building_count, production_rate, output_amount_left, last_updated_at}` members. Accessed member-wise with `read_member`/`write_member` and per-resource selectors, so a transaction touches only the storage slots it needs.
- `ExplorerTroops(game_id, explorer_id)`: owner structure, coord, `Troops{category, tier, count, stamina{amount, updated_tick}, boosts, battle_cooldown_end}`.
- `TileOpt(game_id, alt, col, row) → data: u128`: packed occupier_is_structure (1 bit), occupier_type (8), occupier_id (32), biome (8), row (32), col (32), reward_extracted (1), alt flag at bit 127 [code `models/map2.cairo`].
- `ResourceArrival(game_id, structure_id, day)`: 48 slots.
- `Market(game_id, resource_type)`, `Liquidity(game_id, player, resource)`.
- `Guild`, `GuildMember`, `GuildWhitelist`.
- `Hyperstructure`, `HyperstructureShareholders`, `PlayerConstructionPoints`, `PlayerRegisteredPoints(game_id, address)`.
- `SeasonPrize(game_id){total_registered_points}`: a **global counter** updated on every point grant.
- `RNG(tx_hash){seed}`: per-transaction seed, bumped per use so multicalls get distinct values.
- Config: `WorldConfig`, `PresetConfig`, `ChainConfig`, per-preset `ResourceFactoryConfig`, `BuildingCategoryConfig`, `StructureLevelConfig`. Presets are immutable rows, and "tuning means registering new ids" [brief].

Systems are separate Dojo contracts (`systems/*`): combat (movement, battle, raid, management), production, resources, trade, bank, hyperstructure, guild, village, realm (season and blitz), relic, faith, prize_distribution, registrar, season. Shared logic lives in `#[dojo::library]` classes (`rng_library`, `biome_library`, `combat_library`, `structure_creation_library`), resolved by DNS name and version.

### 2.3 Time without global ticks

- Everything is "rate + last_updated, settle on touch" (production, stamina, battle cooldown, share and faith points, S0 battle health).
- "Ticks" are derived: `current_tick = now / interval`.
- No keeper is required for game progress. Players (or their browser automation) pay for their own catch-up.
- The one keeper-like job is **season settlement**, which is split into idempotent pages callable by a registrar: share-point checkpoints, then ranking trials.
- The client extrapolates balances for display (`stored + rate × Δt`). Mismatch between that extrapolation and optimistic patches caused the "negative wheat" bug class [brief `negative-food-balance-codex-brief.md`].

### 2.4 Map and tile storage

- Sparse key-value per tile, written on first exploration or occupancy. Undiscovered means an absent or zero row.
- Terrain is computed, not stored, so the client renders unexplored terrain from the same noise function.
- Occupancy is exclusive per tile (one army or structure), enforced by reading and writing the target tile in the same transaction. This is the collision primitive.
- An `alt` map layer (surface vs "ethereal/underground") shares coordinates, with bit 127 as the flag.
- Settlement positions come from a ring generator (`BlitzSettlementConfig{side, step, point}` advanced by `next()`). Each claim pulls a random open slot, and fresh slots are generated lazily.

### 2.5 Randomness

- Cartridge VRF uses `request_random` as the first call of the multicall. The Cartridge paymaster wraps the transaction with `submit_random(proof)`. `consume_random(source)` verifies the proof on-chain, and `assert_consumed` checks the value was used.
- `Source::Nonce(addr)` gives a fresh value per request. `Source::Salt(s)` gives the same value for the same salt [github.com/cartridge-gg/vrf].
- Eternum uses `Salt(tile_seed)` for explore and reward outcomes (no re-rolling a tile) and `Nonce(caller)` for combat dice, village rolls, settlement and relics [code grep `get_random_number`].
- **Trust.** The VRF server holds the secret key. It cannot bias a proven value, but it *can foresee* outcomes and can refuse service (liveness). The fallback uses the tx hash as randomness on non-mainnet chains, which is grindable [code `utils/random.cairo`].
- Changelog 2026-03-28: "VRF: single request per transaction enforced".

### 2.6 Scale achieved and what broke

**Participation.**
- S0: about 800 sign-ups / "100s of players" [Bankless], about 36% of Starknet transactions [Sovereign Frontier].
- A search-engine snippet quotes "800 players filling up to 50% of Starknet blockspace and 75% of its compute" **[unverified]**.
- S1 claimed "20× more" capacity [ChainPlay]; no published S1 count found.

**Evidence that one world did not scale as hoped:**
1. **Competitive play moved to small lobbies.** Blitz caps are 24 (live preset 6 "official-60"), raised to 96 on Madara. The cap is enforced in four places [brief phase-1].
2. **Their own chain throughput measurement on a laptop:**
   - 96 bots play at most one action per 16 s each: 6.4 tx/s per game.
   - 2 concurrent games (12.8 tx/s) pass: pre-confirmed p95 101 ms, block close 249 ms.
   - 4 games (25.6 tx/s) fail: p95 7.9 s, close 849 ms; one block executed 174 transactions in 11.5 s with 1.68 s of merklization.
   - 8 games collapse.
   - "Merklization is the hardware-independent cost line: ~10 ms per transaction and serial … why Eternum-scale worlds need their own measurement" [brief phase-1 D.4.1, `deploy/madara-lab/README.md` Headroom table].
   - A rented 12-core, 5.7 GHz box was chosen because "the sequencer executes one action's Cairo on one core".
3. **Load-shedding design changes:**
   - Arrival gates (bucketed deliveries).
   - Member-wise resource reads.
   - Packed tiles.
   - Removal of the long-lived shared `Battle` entity.
   - Production orders with upfront inputs.
   - Browser-side automation.
   - "Fee estimation is doubled work on a fee-free chain" (every estimate is a full execution).
4. **Read path.** Torii polling gave about 1–1.5 s perceived latency against a 50–77 ms chain. Client optimism with RECS overrides produced reconciliation stalls of 30 s and negative balances. Boot to playable took about 50 s on a slow machine (2026-09-03) [briefs].
5. **Costs.** Fee-free L3 chosen for gameplay. One unverified press claim puts gas at $30–50 per active player over 10 weeks [unverified].
6. **Operational and trust gaps (self-reported, Aug 2026 audit):**
   - B1: a permissionless ranker can substitute a roster, which permanently locks the pool.
   - A1: `create_game` is permissionless with caller-set dev mode.
   - F1: no fund-release path after a game starts.
   - F3: a cancelled game eats burned passes.
   - O1–O4: relay reorg and double-submit risk; one poisoned game halts all payouts.
   - S1 prizes were paid manually.
   - `pointsForWin = 0` in every current config.

### 2.7 Indexer and client

- **Torii**:
  - Indexes Dojo world events (`StoreSetRecord`/`StoreUpdateMember`/…) into SQL, and serves GraphQL, gRPC subscriptions and ERC tracking.
  - Mainnet config: `events_chunk_size = 1024`, `pending = true`, `polling_interval = 500`, curated model indices.
  - The client mirrors state in **RECS** (a client-side ECS) and applies optimistic overrides.
- **Herald** (2026):
  - "Madara has no 'all entities of model X': Dojo entity keys are hashes and current state exists only as a fold of the world's store events". Herald holds the fold per game in Postgres.
  - It subscribes to pre-confirmed blocks and serves `snapshot` + `diff{epoch, seq}` + resume over WebSocket.
  - Replay is free (a pure function of the log), and pre-confirmed blocks form a *replaceable overlay*.
  - "Throughput is a non-problem at target scale (~100 tx/s ≈ ~100 KB/s of decoded diffs)".
- **Client:** React + Three.js. World view plus local "hexes within hexes" view. Spectator via URL. There is also an "Axis" headless agent and a "hired agents" plan (LLM agents in sandboxes signing through delegated key rotation).

---

## 3. Criticisms and lessons (from players, press and the team's own notes)

1. **Complexity and time commitment.** "demands commitment with daily logins for weeks, managing complex trade routes … not for everyone" [Bankless S1]. The team responded with Simple/Labor mode, Villages, browser automation, a mobile companion, a 60-minute Blitz, and a Recruit bracket.
2. **Timer-based battles were replaced** by instant stamina-gated combat. "spamming no longer works" [ChainPlay, Bankless]. The S0 model (joinable battles with escrow) was a design-complexity sink.
3. **Whales and holders.** Entry is gated by NFT supply. Realms and passes are tradable, so capital buys pieces. The tribe prize gave the **leader 30% off the top**. Victory is decided by the tribe VP sum, which favours large guilds. Big-guild dominance is reported only by an [unverified] source.
4. **Prizes were not trustless** in S1 (manual distribution, "or the game contract breaks" clause). Operator and registrar roles persist in the 2026 design until proofs exist.
5. **End conditions.** Relying on "first player to X VP ends the season" makes the season length unknowable. Current configs carry `pointsForWin = 0`, so there is no Eternum win condition and the design is open.
6. **Infra dependency risk.** Cartridge's end of life forced a rewrite of accounts, VRF, indexer and paymaster [brief phase-1 facts].
7. **Latency and UX.** Indexer polling and client optimism were the dominant sources of perceived lag and state bugs. The fix was a server-side fold with sequence numbers.
8. **Balance churn.** Frequent numeric retuning in Blitz: exploration VP 50→25→10→5, essence rift odds 1/20→1/30→1/50→1/40, army cap 100k→30k, game length 90→60 min. They moved to immutable presets instead of mutating config.

---

## 4. What this means for PERMUTATION STATE on Solana + MagicBlock ER

(Analysis, not source. These map Eternum's patterns onto our hard requirements.)

### 4.1 Copy

- **No global tick; lazy per-entity accrual.** Store `(rate, last_ts, remaining_budget)` per resource or stamina PDA and settle on touch. This removes the whole-world `ResolveTick` that caps us at about 48 members. Commit-reveal simultaneity can be kept *locally* (per region or per engagement) rather than globally.
- **Sparse, procedural, expandable map.**
  - Terrain = `noise(col, row, season_seed)` computed on-chain and in the client. Store only the *delta* (explored flag, occupant, owner, improvements).
  - Rent guidance: per-tile PDAs are too costly. A 144-byte account is about 0.001 SOL, and 1M tiles is about 1,000 SOL.
  - Instead, use **region/chunk PDAs** (e.g. 16×16 tiles × 8–16 B = 2–4 KiB, about 0.014–0.028 SOL each), created lazily by the first explorer, who pays the rent. The payer is refunded or credited at season close.
  - Chunk locking also bounds the write-lock blast radius.
- **Settlement ring generator with an open-slot pool.** New joiners claim a VRF-random slot from a small pool that is refilled one ring at a time. The map grows exactly as fast as demand. The pool refill must be **bounded per call** (Eternum's own brief flags theirs as unbounded).
- **Occupancy as the collision primitive.** One unit per hex; move = read/write two tile slots in one transaction. On Solana this is two chunk accounts at most (or one if intra-chunk). That keeps per-transaction account locks tiny and makes contention local.
- **Paged, verified settlement.** For N members:
  - Hyperstructure/faction shares go through paged checkpoints.
  - Ranking uses submitted sorted pages with the order asserted on-chain.
  - The final page must match the stored `count` and `Σpoints`.
  - Anyone should be able to *submit*, but submissions must **only list registered, settled participants**. This is Eternum's B1 lesson, applied directly.
  - There must be an end-gated **abort → refund** path so that a disputed or dead game can never lock the vault (Eternum's F1).
- **Faith as the faction template.** Open membership by pledging to one of six factions. Rate-based points per member, lazily claimed; faction totals kept as rate sums updated on pledge/unpledge. No member loops. Prize splits between the faction and its pledgers by accrued points.
- **Instant, local, stamina-gated combat with damage-ratio refunds and cooldowns.** Bounded work per transaction, no long-lived shared battle objects, and no griefing by chaff armies.
- **Anti-teleport and anti-Sybil structural rules.** Examples: Villages receive troops only from their parent; one registration per wallet per game (weak on its own); quadratic duplicate costs; deployment caps; immunity windows for new joiners.
- **Immutable presets** for balance and economics, referenced by `game_id`.
- **Server-side fold with ordered diffs** as the read path (MagicBlock ER blocks → our gateway fold → WS snapshot + seq diffs), with replay from logs serving the verifier.

### 4.2 Avoid (hot shared accounts that Starknet's sequential sequencer hides)

Eternum writes several global rows on hot paths. On Solana/ER each would be a write-lock bottleneck, and an attacker could contend them for free:
- `SeasonPrize.total_registered_points` on every VP grant.
- `realm_count_config` / `blitz_settlement_config` on every settle.
- `Market(resource)` on every swap.
- `RNG(tx_hash)` per transaction.
- `HyperstructureGlobals`.

Replacements:
- Shard counters per region or faction and sum them in paged settlement.
- Batch AMM trades per epoch (frequent batch auctions) or run one pool per region.
- Derive global totals from verified sorted pages rather than a live counter.

Other things to avoid:
- **Tx-hash randomness fallback** (grindable).
- **An operator-held VRF key** that can foresee outcomes. With MagicBlock VRF, use `Salt(tile/season)`-style scoping so outcomes cannot be re-rolled. Even with a VRF, do not let the operator see pending sealed orders.
- **Manual prize distribution and permissionless game creation with dev flags** (Eternum A1).

### 4.3 Scale reality check

- Eternum's "anyone can join" was bounded by 8,000 + 48,000 NFT slots. Real concurrency peaked in the hundreds or low thousands on a public L2, and the team is now shipping 96-player sessions on its own chain.
- The "one living world for tens of thousands" promise was not demonstrated.
- Their chain-level numbers suggest about 6–7 tx/s per 96 active players when stamina binds, i.e. about 0.07 tx/s per player.
- At 10,000 concurrent players that is about 700 tx/s. That is plausible only if the load is spread across many independent accounts (regions/chunks, multiple ER sessions) with no global hot account. That is the architecture our redesign needs.

---

## Sources

Repository (commit `a375655`, 2026-09-24): https://github.com/BibliothecaDAO/eternum/tree/a375655955d08b96201e40500a52ddc6a9278b48

- README: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/README.md
- Lazy production: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/resource/production/production.cairo
- Resource model: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/resource/resource.cairo
- Arrival gates: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/resource/arrivals.cairo
- Packed tiles: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/map2.cairo
- Biomes (simplex noise): https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/utils/map/biomes.cairo
- Stamina: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/stamina.cairo
- Combat formula: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/troop.cairo
- Explore/move: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/combat/contracts/troop_movement.cairo
- RNG library / VRF: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/system_libraries/rng_library.cairo and https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/utils/random.cairo
- Settlement pool / rings: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/utils/settlement.cairo and https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/realm/season/contracts.cairo
- Guild: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/guild.cairo
- Hyperstructures / points: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/hyperstructure.cairo
- Faith: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/faith/contracts.cairo
- Paged ranking: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/prize_distribution/contracts.cairo and https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/systems/utils/ranking.cairo
- AMM market: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/src/models/bank/market.cairo
- Config (economy, troops, points): https://github.com/BibliothecaDAO/eternum/tree/a375655955d08b96201e40500a52ddc6a9278b48/config/source/eternum
- Torii mainnet config: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/contracts/l3/game/torii-mainnet-game.toml
- Internal briefs:
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/realms-phase-1-brief.md
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/realms-phase-2-brief.md
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/realms-value-plane-design.md
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/value-plane-loop-audit-2026-08-31.md
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/negative-food-balance-codex-brief.md
  - https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/docs/plans/realms-client-brief.md
- Madara lab measurements: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/deploy/madara-lab/README.md
- Herald: https://github.com/BibliothecaDAO/eternum/blob/a375655955d08b96201e40500a52ddc6a9278b48/apps/herald/README.md
- Season 0 snapshot (battles over time, constants): https://github.com/BibliothecaDAO/eternum/tree/99988680da8874fa28b7bf451c88a31ffe6ad14f (`contracts/src/models/combat.cairo`, `contracts/src/systems/combat/contracts/battle_systems.cairo`, `sdk/packages/eternum/src/constants/global.ts`)
- Season 1 snapshot (config): https://github.com/BibliothecaDAO/eternum/tree/d3dcb46788672f15753e08a357c4879caad4cd7a (`config/environments/_shared_.ts`)

Player docs:
- https://docs.realms.world/eternum/key-concepts
- https://docs.realms.world/eternum/game-entry
- https://docs.realms.world/eternum/realm-and-villages/villages
- https://docs.realms.world/eternum/victory
- https://docs.realms.world/eternum/prize-pool
- https://docs.realms.world/eternum/world-structures
- https://docs.realms.world/eternum/world-physics
- https://docs.realms.world/eternum/military/damage
- https://docs.realms.world/eternum/resources/transfers-and-trade
- https://docs.realms.world/blitz/key-concepts
- https://docs.realms.world/blitz/prize-pool
- https://docs.realms.world/blitz/mmr
- https://docs.realms.world/blitz/worldmap-movement/worldmap
- Also the changelog pages (source in repo `apps/game-docs/docs/pages/changelog/*`)

Cartridge VRF: https://github.com/cartridge-gg/vrf

Press:
- https://www.bankless.com/read/eternum-season-1-starknet
- https://www.bankless.com/read/play-eternum-starknets-mmo
- https://chainplay.gg/blog/season-one-of-eternum-launches-april-30-with-major-updates/
- https://www.bitrue.com/blog/starknets-realms-eternum-update
- https://sovereignfrontier.substack.com/p/play-eternum-to-experience-fully
- https://www.starknet.io/blog/starknet-starter-pack/
- https://x.com/RealmsEternum/status/1872493966916235630 (S0 end; not loadable)
- Unverified: https://medium.com/@Oddsanjay/the-evolution-of-eternum-or-how-i-learned-to-stop-worrying-and-love-onchain-chaos-2e9f29fd06cd (403; claims about Season 3/4 bots, gas, and guild concentration are not corroborated)
