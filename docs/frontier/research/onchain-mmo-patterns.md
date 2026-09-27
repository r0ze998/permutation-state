# On-chain MMO and strategy-game architectures: patterns for unlimited players and an expandable map (Solana + MagicBlock ER)

Research unit: onchain-mmo-patterns. Date: 2026-09-27. Repo `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs` was not touched. Source code of other projects was shallow-cloned into `scratchpad/openworld/lab/mmo-src/` (read only, nothing built or run).

Confidence tags:
- **[code]**: read in source code (clone path or local crate given).
- **[doc]**: official docs or the project's own blog.
- **[secondary]**: press or third-party write-up.
- **[inference]**: my reading of the code or docs, not stated by the project.
- **[estimate]**: arithmetic or a guess with stated assumptions.

Companion documents that this report does not repeat: `scratchpad/scale/understand/platform.md` (Solana and ER limits, fees, rent and delegation costs, with sources) and `scratchpad/scale/scale-design.md` (realms, cohorts and Commons). Eternum is surveyed only briefly here, because a separate Eternum study uses `openworld/lab/eternum-src`.

---

## 0. Bottom line

1. **No surveyed game that supports "any number of players" runs a global tick over the whole world.** They all do one of three things:
   - **per-entity accounts with lazy, time-based updates** that happen when an entity is touched (Dark Forest, Primodium, SAGE, DUST, Influence, Eternum, Heroes of the Sun);
   - **many small instances** (Sky Strife matches, Supersize rooms, Loot Survivor runs, Realms Blitz worlds from a factory);
   - **horizontal copies with a shared market** (Pirate Nation's Apex and Boss chains, MagicBlock's USDC PoC "sharded by cohort").

   PERMUTATION STATE's whole-world `ResolveTick` is the outlier. It is exactly why the world caps at about 48 members.
2. **Maps grow in two proven ways, and both are cheap because terrain is a pure function.**
   - (a) **The radius follows the population.** Dark Forest sets `r = sqrt((planets_L4+ + 20·players)·rarity·100/314)`, so area grows linearly with players and density stays constant. Primodium spawns player *n* at distance `260·ln((n+105)/10) − 580`, so the map grows logarithmically and gets denser.
   - (b) **Lazy materialisation.** A tile, chunk or sector account is created only when a player first explores it, and that player pays. Examples: OPCraft, Eternum `explore`, SAGE `discoverSector` with a `funder` signer, DUST `exploreChunk` against an operator-seeded Merkle root, Primodium secondary asteroids derived from a hash.

   On Solana this maps directly to **region PDAs keyed by coordinates, created and paid for by the first explorer, with terrain from a seed that is committed before the region opens.**
3. **Time models that scale** are:
   - `last_updated` accumulators (energy, food, production, stamina);
   - start/finish actions with a `ready_at` or `busy_until` (Influence crews, SAGE fleet state machine, Eternum stamina ticks);
   - locally spent "time units" (Heroes of the Sun).

   Simultaneous commit-reveal resolution exists only inside **small bounded scopes** (a match or a planet). To keep simultaneous turns with unlimited players, **the scope of simultaneity must shrink from "the world" to "a region"**, and every region transaction must read a **bounded number of order sources**.
4. **Hot shared accounts are the failure every large game hit.**
   - Star Atlas SAGE on Solana: every miner at an asteroid writes the shared `resource` and `planet` accounts, and every scanner writes `sector` and a shared SDU token account **[code: SAGE IDL]**. Priority fees reportedly took "as much as 30% of gross revenues of players" **[secondary]**, and ATMTA left Solana mainnet for its own SVM L1 z.ink (December 2025).
   - Realms Eternum Season 0: 800 players used about 50% of Starknet's blockspace **[secondary]**.
   - BOLT's own `add_entity` writes a global `World.entities` counter on every entity creation **[code]**.
   - Heroes of the Sun puts every settlement placement through one global spiral `LocationAllocator` **[code]**.

   **Rule: no instruction that an ordinary player can send may write an account that other players also need.**
5. **Horizontal ER scaling is real, but each shard is its own world.**
   - MagicBlock's own PoC measured **≈ 88k simple SPL transfers/s per ER node** (single-threaded sequencer) and ≈ 565k/s on one 128-core box with 12 co-located ERs, "sharded by payment cohort" **[code/doc: er-usdc-scaling-poc, 2026-09-02]**.
   - Dynamic colocation lets each account be delegated to a chosen regional validator. However, "state delegated to one regional validator is not writable to other regions" **[doc]**.
   - So **cross-shard interaction must be asynchronous, through base commits.** Design it as travel whose time is longer than the commit latency, the way Dark Forest arrivals, SAGE warps and Influence transits already take time.
6. **Spam, bot and Sybil defences that survived production are economic and local**, not identity-based:
   - energy or stamina that regenerates (Pirate Nation 200/day for VIP, DUST energy drain, Eternum stamina);
   - per-target inbound quotas (Dark Forest: **6 arrivals from the owner and 6 from others per planet**, counted separately, "so people can't deny service to planets with gas limit") **[code]**;
   - cooldowns (Dark Forest reveal cooldown, FrenPet bonk every 15–30 min, 24 h shields);
   - an entry cost per identity (Loot Survivor pays about $1 of VRF per game, Sky Strife uses orbs, Dark Forest used a whitelist).

   Several teams concluded that bots **cannot be kept out and must be designed for** (Kamigotchi, Dark Forest plugins). That supports PERMUTATION STATE's disclosed operator AIs.
7. **Randomness pattern: commit now, reveal a future beacon.**
   - Influence `random::commit(key, round_delay)` then `reveal` at finish **[code]**.
   - DUST `initChunkCommit` fixes a future drand round, which must be used within 2 min and expires after 10 min **[code]**.
   - Loot Survivor requests a VRF seed per level **[code]**.

   On our stack: **MagicBlock VRF requested at a deadline known in advance**, never an ER blockhash (the ER validator controls it) and never an operator seed.
8. **Liveness and exit.** Lattice shut down Redstone on 2026-05-15 and DUST moved to its own chain **[secondary]**. Star Atlas left for z.ink. Games whose liveness depends on one operator's chain or sequencer need exit paths. On our stack that means:
   - the DLP undelegation timeout (about 60 min, see platform.md);
   - **permissionless cranks** (MagicBlock native ER cranks, or Hydra, a permissionless base crank with a reward and a 466-CU trigger **[code]**);
   - claims computed from state, not from an operator.
9. **BOLT (v0.2.6) is not a fit for the resolve path.**
   - A system's outputs travel through Solana return data, so all written components together are capped at **1,024 B** per call (`MAX_RETURN_DATA`) **[code]**.
   - Each component is re-serialised on every call. CU overhead is about 15k for a 1-component apply and about 99k for 10 **[code: docs/REPORT.md]**.
   - Entity creation write-locks the World account.

   BOLT is fine for small per-player entities, and its session-key integration (`apply_with_session`) is useful. The region engine should stay a native zero-copy program, as today.
10. **Recommended shape for PERMUTATION STATE (details in §5):**
   - a hex map cut into **region accounts of ≤ 4 KiB**;
   - lazily opened frontier rings whose seeds are fixed by VRF before opening;
   - **per-region ticks** resolved by a permissionless crank, with neighbours allowed at most one tick apart;
   - sealed orders per **(region, faction)**, submitted by that faction's regional commander, so a region transaction reads at most 6 order accounts;
   - ordinary players act lazily on their own accounts (economy, pledges, votes) and never write region or faction accounts directly;
   - continents as ER shards, joined by sea lanes whose travel time is longer than a base commit round.

---

## 1. Local crate cache (checked 2026-09-27)

| Crate | Versions | Relevant contents | Conf. |
|---|---|---|---|
| `ephemeral-rollups-sdk` | 0.16.2, 0.17.2 | 0.17.2 adds `crank` (legacy `ScheduleCrankCpi` plus `hydra`), `access_control` (permission program), `spl` (ephemeral ATA, transfer queues), and `ephemeral_accounts` (ER-only accounts at `(len+60)·32` lamports) | code |
| `magicblock-magic-program-api` | 0.10.1 | `ScheduleTaskArgs { task_id, execution_interval_millis, iterations, instructions }`; `MAGIC_CONTEXT_SIZE = 5 MB`; `EPHEMERAL_RENT_PER_BYTE = 32`; `ScheduleCommit`, `ScheduleCommitAndUndelegate`, `ScheduleBaseIntent`, `ScheduleIntentBundle`, `AddActionCallback` | code |
| `magicblock-delegation-program-api` | 3.1.0 | session fee, commit fees, undelegation timeout (see platform.md §1) | code |
| `ephemeral-vrf-sdk` | 0.4.1 | VRF request/callback macros | code |
| **BOLT** (`bolt-lang`, `world`) | **not in cache** | cloned from GitHub instead (v0.2.6, commit `7190607`, 2026-05-28) | code |
| `session-keys` (`gpl_session`) | not in cache | cloned from GitHub | code |

Consts in 0.17.2 `src/consts.rs`:
- `PERMISSION_PROGRAM_ID ACLseoPo…`
- `ESPL_TOKEN_PROGRAM_ID SPLxh1LV…`
- `HYDRA_PROGRAM_ID Hydra17i…`

The repo pins 0.16.2 (platform.md). So cranks with `ScheduleTask` and ephemeral accounts are available without an upgrade, and Hydra needs 0.17.x.

---

## 2. Survey

### 2.1 MagicBlock platform pieces relevant to an MMO

**Ephemeral Rollup (ER).**
- Program-owned accounts are delegated to a chosen ER validator. The ER clones them just in time, runs SVM transactions with no fee, and commits them back to base.
- The engine runs up to 64 executors and serialises transactions that conflict on an account (platform.md §2.1).
- **Per-node ceiling measured by MagicBlock** in `magicblock-labs/er-usdc-scaling-poc` (validated 2026-09-02), with signed SPL transfers between delegated ephemeral ATAs **[code/doc]**:
  - 1 ER node: 88.3k TPS on a 128-core server, 97–106k on an M5 laptop. Latency is 0.5 ms at light load and 28–44 ms average at saturation (queueing).
  - N ER nodes on one box: 169k (2), 326k (4), 456k (6), and a plateau at ≈ 565k from 10 nodes. The limit is the machine's cores, not the architecture.
  - "every transaction passes through a single-threaded sequencer", so per-node throughput is bound by single-core speed. Sigverify is only about 10% of the cost.
  - Warning in the README: keep ≥ 256 users per shard, because "with too few pairs, write conflicts dominate the scheduler and the sequencer sheds most of the load as drops". **Hot accounts lower throughput, and the excess is dropped, not queued.**
- **Dynamic colocation** **[doc: magicblock.xyz/blog/dynamic-colocation]**:
  - the delegation names a validator pubkey (for example `MAS1Dt9q…` for Asia);
  - Supersize routes match rooms to the nearest regional ER;
  - "state delegated to one regional validator is not writable to other regions … cross-region visibility requires committing state back to Solana, then re-cloning";
  - there is no automatic sharding.
- **Magic Router** routes a transaction to base or to an ER by looking at the owners of its writable accounts **[doc]**. It does not make a transaction span two ERs.

**Native ER cranks** (`ScheduleTask`):
- intervals in milliseconds, an iteration count, no fee **[code: magic-program-api `ScheduleTaskArgs`; doc: blog/native-cranks-for-ers]**;
- the scheduled instructions must be permissionless, or check that the crank is the signer;
- failure handling is not documented **[doc gap]**. For us that means a crank is a convenience and never the only path: every tick instruction must also be callable by anyone.

**Hydra** (base-layer permissionless crank, `magicblock-labs/hydra`, 2026-09-19) **[code]**:
- stores up to 16 instructions in a crank PDA;
- anyone triggers them when due and collects a flat reward;
- `Trigger` costs 466 CU whatever the payload size (a single memcmp against the instructions sysvar);
- scheduled instructions run top-level and cannot need signers.

This is the right tool for **base-side** deadlines (close registration, request VRF, open a frontier ring, settle a season), because it removes the operator from liveness.

**Magic Actions** **[doc: blog/magic-actions]**:
- base-layer instructions attached to an ER commit, which run right after it with the committed state as input;
- "If any action fails, the entire commit reverts";
- fees come from an escrow PDA.

Useful for "commit the region checkpoint and credit the faction treasury on base in one step". But a failing action also blocks the commit, so actions must be infallible by construction. Our audit rule G3 already says "no ledger can make a resolve fail".

**Ephemeral accounts**:
- ER-only accounts, sponsor-paid at 32 lamports/byte, never committed (platform.md §4);
- right for per-player per-tick scratch data (ballots, inbox entries), wrong for anything that must settle.

**Session keys** (`magicblock-labs/session-keys`, `gpl_session`) **[code]**:
- A `SessionToken` PDA binds `authority`, `session_signer`, `target_program` and `valid_until`.
- The default validity is 1 h; `valid_until ≤ now + 7 days` is enforced.
- Optional top-up of the session signer, 0.01 SOL by default.
- `revoke_session` exists.
- The security page says the worst-case loss is the top-up, advises "treat session keys like JWT tokens", and suggests disabling top-up when a gasless relay pays **[doc]**.
- BOLT integrates it through `apply_with_session`.

**BOLT ECS** (v0.2.6) **[code: lab/mmo-src/bolt]**:
- **Model:** a `World` PDA (`[b"world", id]`). Entities are PDAs `[b"entity", world_id, counter | 0, extra_seed?]`. Components are separate programs whose accounts are PDAs per entity. Systems are programs that receive components and return new component bytes.
- **Execution:** `world::apply` CPIs `bolt_system::bolt_execute`, which returns `Vec<Vec<u8>>`. The world then CPIs each component's `update`.
- **Limits that matter for us:**
  1. The system output travels via Solana return data. `MAX_RETURN_DATA = 1024` bytes (`solana-cpi-3.1.0/src/lib.rs:330`), so **all components one system call writes together are limited to about 1 KiB**. A 4 KiB region cannot be a BOLT component written by a system.
  2. CU per apply grows linearly with components, from ≈ 15.3k CU (1 component, 2 CPIs) to ≈ 99.3k CU (10 components) (`docs/REPORT.md`).
  3. `add_entity` always does `world.entities += 1` on a `#[account(mut)] world`. **Every entity creation write-locks the World PDA**, even with `extra_seed`. Under ER FCFS that is a free global lock that griefers can hit.
  4. `apply` takes the world read-only and checks `approved_systems` unless the world is permissionless. That is good.
  5. The `#[component(delegate)]` macro generates delegate and undelegate helpers, which is convenient.
- **Verdict:** BOLT fits small games (Supersize, tic-tac-toe, Heroes of the Sun) and per-player entities. It is not the path for bounded-work region resolution. Our native zero-copy program is already better on every axis BOLT would touch.

### 2.2 MagicBlock example games

**Supersize** (agar.io-style, real money, BOLT + ER; community repo `Lewarn00/supersize-solana`) **[code]**:
- **State:** a `Map` component (4000 × 4000, `max_players: u8 = 20`, buy-in bounds, a food queue, `frozen`), `Section` components of 1000 × 1000 with up to 100 packed 4-byte food items each, and one `Player` component per player.
- **Time:** real time. Every `movement` call takes `map`, `player` and `section`, so the Map is written on each move **[inference: BOLT writes every returned component]**. That is fine at 20 players per room.
- **Scale model:** **many rooms**, routed to the nearest regional ER (colocation blog). MagicBlock recaps report 250k matches in July 2026 **[secondary]**.
- **Lesson:** the "instance" pattern. Unlimited players come from unlimited rooms, each with a hard cap. The world map is partitioned into sections so that food updates touch a bounded account.

**Heroes of the Sun** (`magicblock-labs/heroes-of-the-sun`, BOLT + ER + session keys, 2025-10) **[code]**:
- **Settlements run local turns.** `claim_time` converts elapsed wall-clock time into `time_units`, capped. `wait` spends time units to advance that settlement's own simulation (storage, workers, decay). Each settlement has its own clock, and there is no global tick.
- **Placement** uses one global `LocationAllocator { current_x, current_y, direction }`. `bump_location_allocator` walks a square spiral, and its source carries the comment "todo safety verify current slot is actually occupied so you dont bump endlessly".
- **Anti-pattern:**
  - one hot global account on every join;
  - a permissionless bump that anyone can loop to push new spawns outward (map inflation for free);
  - a 100-entry `LootDistribution` vector in one account.

**Other 2025–2026 MagicBlock apps** (monthly recaps, **[doc]**): Bakeland (an MMORPG on Solana Mobile that uses MagicBlock VRF for reward rolls), BlitzMine (a real-time mining game with VRF tiles), Slimecoin (1v1 skill games that pay USDC), JungleFun (gacha). None is a large shared-map strategy game with public architecture. **No MagicBlock-hosted open-world strategy game at thousands of concurrent players on one map was found.**

### 2.3 Dark Forest (Ethereum / Gnosis, v0.6, `darkforest-eth/eth`) **[code]**

- **State:** a Diamond (EIP-2535) with mappings keyed by planet location hash:
  - `planets`, `planetsExtendedInfo`, `planetEvents[location]` (an arrivals queue), `planetArrivals`;
  - `players` and a `playerIds` array.

  Planets are created lazily (`initializePlanetWithDefaults`) the first time a move targets them.
- **Terrain:** the procedural universe comes from a MiMC hash and a perlin function of the coordinates. Players prove in zero knowledge that a hashed location has the claimed perlin and radius (`init`, `move`, `reveal` and `biome` SNARKs). The contract never learns coordinates: **cryptographic fog of war**.
- **Time:** `LibLazyUpdate.updatePlanet` computes energy with a logistic growth formula (`exp(−4·growth·Δt/cap)`) and silver as linear with a cap, from `lastUpdated`, whenever a planet is touched. Arrivals land at `block.timestamp + travelTime` and are applied when the planet is next refreshed.
- **Conflict resolution:** instant and FCFS per transaction. Combat happens on arrival, in timestamp order during refresh.
- **Map growth:** `_getRadius()` = `sqrt((initializedPlanetCountByLevel[4] + 20·nPlayers)·cumulativeRarities[4]·100/314)`, floored at `WORLD_RADIUS_MIN`. `updateWorldRadius()` runs after moves unless `WORLD_RADIUS_LOCKED` is set. **Area grows linearly with players, so density stays constant.** New players must spawn at the rim (`SPAWN_RIM_AREA`) within an allowed perlin band.
- **DoS defence:** `checkPlanetDOS` counts pending arrivals on the target and requires `arrivalsFromOwner < 6` or `arrivalsFromOthers < 6`, depending on the sender ("Planet is rate-limited"). The code comment gives the reason: "need to do this so people can't deny service to planets with gas limit". **Refresh work per planet is bounded, and an attacker cannot use up the owner's quota.** There is also `LOCATION_REVEAL_COOLDOWN` per player, `planetArtifacts.length < 5`, and `onlyWhitelisted` for init and moves (whitelist keys).
- **Bots:** official plugins exposed the client API. Automation was part of the game **[doc/inference]**.
- **Lesson for us:**
  - a radius formula driven by population;
  - rim spawning (new players do not land next to veterans);
  - per-target inbound queues with **separate quotas for owner and outsiders**;
  - lazy growth;
  - zero-knowledge fog is powerful but costly (client proving). Our sealed commit-reveal plus MagicBlock Private ER is the cheaper analogue.

### 2.4 MUD family (Lattice): OPCraft → Biomes → DUST, Sky Strife, This Cursed Machine, Primodium

**MUD itself** **[doc: mud.dev]**:
- The Store is a table-based ECS in one `World` contract. Offchain tables only emit events for indexers.
- The **store-indexer** rebuilds state from events.
- **Account delegation:** `registerDelegation(delegatee, controlId, initData)` with unlimited, timebound or systembound controls, then `callFrom(delegator, systemId, data)`. This is the session-wallet pattern: "players can authorize a different wallet with its private key stored on the client".

**OPCraft (2022)** **[doc: lattice.xyz blog parts 1–2]**:
- **Terrain is a pure function** `f(x,y,z) → block type` (perlin octaves, biomes from heat and humidity, a structure grid). The same function exists in Solidity and WASM.
- **The chain stores only diffs**: blocks that were mined or placed. The world is effectively infinite, and storage grows only where players act.
- **Land:** 16 × 16 chunks claimed by staking. The highest stake wins and can be outbid, and claims can go to a proxy contract for groups.

**DUST** (the successor autonomous world, `dustproject/dust`, 2026-04) **[code]**:
- **Terrain:** generated offchain and **committed per 512 × 512 region as a Merkle root** by the operator (`regionRoot`). Anyone calls `exploreChunk(chunkCoord, chunkData, proof)`; the contract checks the proof and stores the 16³ chunk with SSTORE2. The **first explorer pays** to materialise it. `exploreRegionEnergy` does the same for a region's energy pool.
- **Time:** player and machine **energy drains lazily** (`lastUpdatedTime`, `drainRate`). The constants give about a 5-min full drain underwater and about 10 s on lava. Every action costs energy (move 2.555e13, build 2.555e14 on a max of 8.176e17). Machines pay upkeep from energy.
- **Randomness for ores:** `initChunkCommit` must come from an entity within 2 chunks. It records a future **drand** round (`timestamp + drand period`). Randomness must be submitted within `CHUNK_COMMIT_SUBMIT_TIME = 2 min`, and a commitment expires after 10 min. Resource respawn draws a random index into the set of collected resources, a **conservation of matter** in which burned resources return elsewhere.
- **Chain:** Redstone shut down on 2026-05-15, and DUST moved to "DUST Chain" (Conduit, with the Optimism Foundation's support) **[secondary: Lattice announcement]**.
- **Lesson:**
  - lazy materialisation against a **pre-committed root**, with the explorer paying;
  - locality checks on who may commit randomness;
  - a public beacon instead of operator randomness;
  - a closed resource loop (matches our vault-conservation rule);
  - operator-chain risk is real.

**Sky Strife** (MUD RTS) **[doc]**:
- 2–4-player **match instances** created from the "Sky Pool". An orb token has a fixed supply of 100M and pays to create matches.
- Season Pass holders can create private matches with entrance fees.
- Units move once every 15 s, and a transaction confirms in ≤ 2 s.
- **Lesson:** instancing, with match creation paid in a scarce token as the spam brake.

**This Cursed Machine** (Moving Castles, MUD on Redstone): a single-player "pod" per player that runs order fulfilment **[secondary]**. It is embarrassingly parallel by design: each player's pod is independent state. No shared hot state.

**Primodium** (MUD, `primodiumxyz/primodium`, 2025-02) **[code]**:
- **Spawn:** `createPrimaryAsteroid` places asteroid *n* at `distance = 260·ln((n+105)/10) − 580` and angle `(n mod 4)·90 + (n mod 3)·30 + (n mod 27)`, which is deterministic. Distance is 205 at n = 100, 643 at 1k, 1,219 at 10k and 1,635 at 50k **[computed]**, so **the map grows logarithmically**. Every `shardAsteroidSpawnFrequency`-th spawn also creates a contested "shard asteroid" (a victory objective).
- **Secondary asteroids:** not stored until someone targets a position. `createSecondaryAsteroid(position)` checks the `maxAsteroidsPerPlayer` ring slots around nearby primaries and derives existence from `keccak(source, "asteroid", x, y)`. This is **procedural content materialised on demand**.
- **Production:** `LastClaimedAt` per building or asteroid. `claimResources` and `claimUnits` settle lazily. Buildable tiles per asteroid are a bitmap.
- **Hot spot:** the `AsteroidCount` global counter is written on every spawn. This is harmless on an EVM L2 with a sequencer, but it would be a lock hot spot on Solana.
- **Lesson:** population-driven spawn distance, and hash-derived content that is created only when touched.

### 2.5 Influence (Starknet, `adaliafoundation/influence-starknet`, updated 2026-09-24) **[code]**

- **State:** one `Dispatcher` contract stores all components (`ReadComponent`/`WriteComponent` systems) as packed felts per entity. The world is 250,000 asteroids **[doc]**. Each asteroid's surface is divided into lots by area: Adalia Prime has ≈ 1,768,484 lots (`surface_area(MAX_ASTEROID_RADIUS)`) **[computed from code test]**. Lots are materialised only when used.
- **Time:** `TIME_ACCELERATION = 24` (in-game time is 24× real time). Crews have `ready_at`, `last_fed` (food decays lazily) and `action_*` fields. Every long action is **two transactions**: `*_start` sets `finish_time` and commits randomness, and `*_finish` asserts `finish_time <= now`. Examples: `scan_surface_start/finish`, `construction_start/finish`, deliveries, production.
- **Randomness:** `random::commit(key, round_delay)` stores a future entropy or blockhash round. `random::reveal(key)` at finish derives the seed. Scan results cannot be known at start.
- **Conflict resolution:** instant FCFS, with exclusivity by control (only the controller may act on a lot or asteroid).
- **Map growth:** the map is fixed (a pre-minted asteroid set). Growth comes from the depth of development, not from area.
- **Lesson:** start/finish with `busy_until` bounds action rate per crew without any global clock, and "commit now, reveal later" randomness fits start/finish naturally.

### 2.6 Pirate Nation (Proof of Play, Arbitrum Orbit L3s) **[doc/secondary]**

- **Scale by copying the chain:** Apex (February 2024) and Boss (July 2024) are "essentially twins, built with the same functionality and scale".
  - **Players are permanently assigned** to one chain ("Players will not switch back and forth"), and new players go to the newest chain.
  - The marketplace is unified: buy with ETH on either chain.
  - Islands and shipwrights are visible only on your own chain at first.
  - Apex processed about 2.5M tx/day at up to 70 Mgas/s with 250 ms blocks.
- **Fees:** gasless through a custodial per-browser **game wallet**, with the studio paying gas (about 1M transactions and about $150k of gas saved, per their docs).
- **Anti-bot and economy brake:** a wallet-based **Energy** system that limits earning per day. VIP regenerates 25% faster (up to 200 energy per rolling 24 h).
- **Lesson:**
  - cohort sharding works when shards are **thematically independent** and only the economy is shared;
  - energy is the universal rate limiter;
  - a sponsored-fee budget per player is the cost the operator carries.

### 2.7 Star Atlas SAGE (Solana mainnet, 2023–2025; now z.ink) **[code: `@staratlas/sage` IDL 1.9.0-alpha.16; secondary for numbers]**

- **State:** one PDA per thing:
  - `game` (config), `gameState` ("variables that may change frequently", separate from config);
  - `sector` keyed by `[i64;2]` coordinates, with `discoverer`, `numStars` and `numPlanets`;
  - `star`, `planet`, `resource`, `mineItem`, `starbase`, `starbasePlayer` (a per-player-per-starbase account);
  - `fleet` and `fleetShips`, `craftingInstance`, `loot`, `surveyDataUnitTracker`.
- **Players:** a player profile with indexed permissioned keys (`keyIndex` on every instruction), and a faction (MUD, ONI, Ustur). **Three factions share one map**, which is our identity.
- **Map growth:** `discoverSector { coordinates }` is signed by a fleet key, with a `funder` who pays for the new `sector` account and reads `gameState`. `addConnection(sector1, sector2)` links sectors (funder pays). **Discovery-driven map expansion paid by the discoverer.**
- **Time:** a fleet state machine (`Idle`, `StarbaseLoadingBay`, `MineAsteroid{start,end,amountMined,lastUpdate}`, `MoveWarp{warpStart,warpFinish}`, `MoveSubwarp{departureTime,arrivalTime,lastUpdate}`, `Respawn`). Settlement is lazy at the state exit (for example `stopMiningAsteroid`).
- **Hot accounts (the key lesson), from the IDL:**
  - `startMiningAsteroid` writes `fleet`, `resource` and `planet`;
  - `stopMiningAsteroid` also writes `tokenMint` and the points accounts;
  - `scanForSurveyDataUnits` writes `sector`, `sduTokenFrom` and `resourceMint`.

  So **every fleet mining the same asteroid, or scanning the same sector, write-locks the same accounts.** With about 2M tx/day, reported as about 15% of Solana's daily transactions, players entered priority-fee auctions. Reports say fees took "as much as 30% of gross revenues of players" **[secondary: Blockworks and Aephia on z.ink]**.
- **Outcome:** ATMTA launched **z.ink**, its own SVM L1, in December 2025 and moved the game there, expecting fees "at least 99% lower" **[secondary]**.
- **Lesson for PERMUTATION STATE on base Solana:** shared per-location counters and shared token accounts written by player actions turn into fee auctions. Put per-location contention on the ER (free, but see platform.md §2.3 on griefing). Or make players write only **their own** accounts and fold shared totals in a crank.

### 2.8 Loot Survivor (Starknet, `BibliothecaDAO/loot-survivor`) **[code/doc]**

- **State:** one adventurer with state "primarily in a single felt252" per game. Every run is an **independent instance**, so the game is embarrassingly parallel.
- **Randomness:** a Pragma VRF seed per level. `VRF_COST_PER_GAME = 1 USD` is taken from the entry. The callback cap is $0.05 on mainnet.
- **Accounts:** arcade (burner) accounts via Starknet account abstraction, as sessions.
- **Lesson:** instance isolation plus randomness paid per instance out of the entry fee. Our USDC entry can fund VRF the same way (and ER VRF is free anyway, per platform.md).

### 2.9 Realms Eternum and Blitz (Starknet / Dojo) — brief; see the Eternum study

- **Tiles** are Dojo models created on `explorer_move(..., explore=true)`: one tile is explored per move, the biome comes from a function, and stamina is burned with lazy refill by `armies_tick_in_seconds` **[code: lab/eternum-src]**.
- **Instancing:** a **world factory** deploys fresh Dojo worlds for Blitz matches in chunks of `max_actions` per call **[code: contracts/l3/factory/README]**. Systems also take `game_id`.
- **Villages** attach to a Realm's bounded slots (`the chosen slot is not available`) **[code]**. Newcomers join *under* existing structures instead of needing new land.
- **Load:** "800 players in Season 0 filling up to 50% of Starknet's blockspace and 75% of its compute" **[secondary]**. Even lazy per-entity worlds saturate a shared L2 at hundreds of active players, if every action is an L2 transaction.

### 2.10 FrenPet (Base) **[doc]**

- **Time:** per-pet TOD (time of death). Feeding or staking resets it (for example to 3 days). A missed TOD means 7 days of **hibernation** (weaker, loses up to 1% of points per bonk).
- **PvP:** a bonk every 15 min (V2 blog), or every 30 min (current docs). A 40% win chance takes 0.5% of the target's points (1% if the target hibernates). **Shields** give 24 h of immunity.
- **Fees:** the first **10 transactions per day are covered**, and after that the player funds ETH.
- **Economy:** the reward pool comes from trading tax and is claimed pro rata to points.
- **Lesson:** cooldowns, shields and sponsored-transaction quotas are simple, effective levers. A points-share payout is a closed form that needs no loop over players.

### 2.11 Kamigotchi and bots in general **[secondary: Bankless]**

A researcher reverse-engineered the contracts, built an indexer and a bot, and "quickly started dominating the in-game leaderboards". The developer concluded that on-chain games "must accept bots as part of the user base". The proposed mitigations are:
- design balance for bots;
- proof of individuality (social sign-up, reports, analytics);
- tightly coordinated human guilds;
- **giving bot tooling to everyone.**

### 2.12 Other 2025–2026 Solana titles

- **MafiaBits:** "fully on-chain" text MMORPG, 120 fixed districts, families, seasons since February 2026, reported ">15,000 active players" **[secondary, unverified; no public architecture]**. This is the "fixed territories, unlimited members grouped into families" pattern.
- **Gable Guardians, BattleFrens:** assets on chain, logic mostly off chain **[secondary]**.
- **Star Atlas:** left Solana mainnet (§2.7).
- **Bakeland:** VRF rolls on MagicBlock (§2.2).

No public source describes a Solana game with thousands of players resolving simultaneous turns on one shared map on chain. **PERMUTATION STATE would be first, which is a reason to adopt proven sub-patterns and not invent new ones.**

---

## 3. Comparison table

| Game / framework | State model | Time model | Conflict resolution | Map growth | Spam / bot / Sybil | Fees, who pays | Hot accounts | Sharding | Indexing |
|---|---|---|---|---|---|---|---|---|---|
| MagicBlock ER (platform) | delegated program accounts; ephemeral accounts (ER-only) | ER blocks ~10–50 ms; native cranks (ms intervals) | FCFS per account, serialised conflicts | n/a | none built in; free ER tx | ER tx 0; delegation 0.0003 SOL/acct; commits ≥ 0.0001 SOL | single-threaded sequencer; conflicts shed as drops | by validator (region or cohort); no cross-ER tx | RPC and websocket; logs |
| BOLT ECS | World, entity and component PDAs; component programs | as the host chain | FCFS | via new entities | world `approved_systems`; session tokens | payer creates PDAs | **World written on each `add_entity`** | none (delegate per component) | Anchor accounts |
| Supersize | Map + Sections (1000²) + Player | real time | FCFS | fixed 4000² per room | buy-in | player buy-in; ER free | Map written by each move (20-player cap) | **rooms per regional ER** | — |
| Heroes of the Sun | per-settlement, hero, player components | **local turns from claimed time units** | FCFS | spiral allocator | none | ER | **global LocationAllocator** | — | — |
| Dark Forest | per-planet mappings keyed by location hash | **lazy** growth + arrival queue | FCFS; arrivals applied on refresh | **radius ∝ sqrt(players)**; rim spawn | whitelist; **6+6 arrival quota**; reveal cooldown | player gas (Gnosis) | none by design (per planet) | none (single contract); later Arenas instances | subgraph / client cache |
| OPCraft | ECS; only diffs from a pure terrain function | real time | FCFS | infinite procedural | stake to claim chunks | player gas (OP Stack) | none | none | MUD sync |
| DUST | MUD tables; chunks via SSTORE2 | lazy energy drain | FCFS | **Merkle-seeded regions, explorer materialises** | energy costs; locality for commits | player gas (own chain) | resource-count tables | own chain after Redstone ended | MUD indexer |
| Sky Strife | MUD; match entities | 15 s unit cooldown | FCFS in match | new matches | orbs to create matches; season pass | player gas | per match | **match instances** | MUD indexer |
| Primodium | MUD; per-asteroid tables, tile bitmaps | lazy claims | FCFS | **spawn distance ∝ ln(n)**; secondary asteroids hashed on demand | — | player gas (Redstone) | `AsteroidCount` | none | MUD indexer |
| Influence | Dispatcher components, packed felts | **start/finish + ready_at**, 24× time | FCFS + control rights | fixed 250k asteroids, lots on demand | action time is the brake | player gas (Starknet) | per asteroid/lot | none | own server and indexer |
| Pirate Nation | contracts on L3 | energy regen | FCFS | — | **energy** (VIP 200/day) | **studio pays** (game wallet) | — | **permanent cohort chains** + shared market | own |
| SAGE (Solana) | PDA per sector/fleet/resource/planet | fleet state machine, lazy settle | FCFS | **discoverSector (funder pays)** | profile keys; fuel/food costs | player priority fees | **resource, planet, sector, shared token accounts** | none → left for z.ink | own |
| Loot Survivor | 1 felt per adventurer | turn per tx | per-instance | per-run dungeon | VRF cost $1 per game; entry fee | player | none | **instances** | Apibara / Torii |
| Eternum / Blitz | Dojo models, tiles on explore | stamina ticks, lazy | FCFS | **explore one tile per move** | stamina; season pass; villages in bounded slots | player gas; sessions | L2 saturation at 800 | **factory worlds** for Blitz | Torii |
| FrenPet | per-pet | TOD, 15–30 min bonk | FCFS + 40% roll | none | cooldown, **shield 24 h**, 10 free tx/day | studio pays first 10 tx/day | — | — | — |

---

## 4. Distilled patterns for Solana + MagicBlock ER

Each pattern states what it is, who proved it, how it maps to our stack, its limits (with numbers), and the audit rule it serves.

### P1. Per-entity and per-region accounts; never a world blob

- **Proven by:** everyone except PERMUTATION STATE today.
- **Solana form:**
  - Region PDAs `[b"region", season, q, r]` of fixed size (≤ 4 KiB zero-copy, which matches today's chunk rule and platform.md's commit limit of about 2.5 KB of dirty data per finalize);
  - per-player accounts on base (Member) and ER ephemeral accounts for per-tick scratch data.
- **Numbers:**
  - rent for a 4 KiB account is 0.0294 SOL at 6,960 lamports/byte or 0.0215 SOL at 5,080 lamports/byte (mainnet since 2026-09-11) **[computed]**;
  - 1,000 regions cost 29.4 or 21.5 SOL, and 2,500 regions cost 73.5 or 53.6 SOL, all refundable on close;
  - each delegation costs 0.0003 SOL, so 2,500 regions cost 0.75 SOL of session fees per season.
- **Limit:** a transaction touches ≤ 64 accounts (a v0 transaction with an ALT; 32 in practice for a legacy transaction). **Design every tick instruction to read at most about 20 accounts.**
- **Serves:** bounded work per transaction regardless of N.

### P2. Lazy time: accumulate on touch, never iterate

- **Proven by:**
  - Dark Forest `lastUpdated` with logistic growth;
  - Primodium `LastClaimedAt`;
  - SAGE `lastUpdate` in each state;
  - DUST energy `lastUpdatedTime·drainRate`;
  - Influence `last_fed`;
  - Eternum stamina;
  - HOTS time units.
- **Solana form:** every per-player or per-city economic quantity is `(value, rate, cap, t0)` and is evaluated in closed form when read. Settlement and prizes are closed forms at claim time, as in FrenPet (points share) and the scale study (payouts at claim time).
- **Limit:** closed forms must be monotone and integer-exact, and must not depend on unbounded histories. Anything with interaction (combat, contested harvest) cannot be lazy and needs P4.
- **Serves:** no loops over members; settlement cost is constant.

### P3. Actions with durations (start/finish, busy_until)

- **Proven by:** Influence (`*_start`/`*_finish`, `ready_at`), SAGE (`warpStart`/`warpFinish`), Eternum ticks.
- **Why it matters for us:**
  - duration is the **natural rate limiter** (a Sybil with one unit cannot act faster);
  - it gives a place for **commit-now/reveal-later randomness**;
  - it gives cross-shard travel a latency budget (P7).
- **Solana form:** `busy_until: i64` on each unit or stack. Finishing is permissionless (anyone may finish an expired action, with a crank tip), so a player going offline cannot freeze the world.

### P4. Simultaneous resolution only in bounded scopes, with bounded order sources

- **Proven by:** Sky Strife (match), Supersize (room), Dark Forest's per-planet queue with a cap of 6+6. Nobody resolves the whole world simultaneously.
- **Our form: region ticks.**
  - A region resolves tick *t* when:
    1. wall-clock ≥ deadline(t), and
    2. every neighbour region has resolved ≥ t−1 (conservative synchronisation, skew ≤ 1).
  - Inputs:
    - the region account (write);
    - neighbour **border outboxes** (read, ≤ 6);
    - **one sealed order batch per faction present** (read, ≤ 6);
    - the clock/season account (read).
  - That is **≤ 14 accounts**.
  - Players do not submit tick orders individually. Each faction's **regional commander** (elected or appointed by the faction's governance) submits one commit, then one reveal per region per tick. That is today's "officers submit sealed batches" moved from world scope to region scope.
- **Liveness:**
  - A missing reveal means "hold" orders by default.
  - A missing neighbour stops only its neighbours, and only while the missing region stays behind. Anyone may resolve the lagging region, because the instruction is permissionless.
  - There is no global barrier.
- **Per-target quotas (Dark Forest):**
  - each region has a fixed number of inbound border slots per faction, and a fixed number of stacks per faction per region;
  - overflow is rejected at order validation, never at resolve.

  So resolve CU is bounded: about 1/20 of today's whole-world resolve per region **[estimate: today 20 chunks × 4 KiB ≈ 1.1–1.4M CU adversarial]**, which is ≈ 60–120k CU per region-tick.
- **Serves:**
  - bounded work under adversarial load;
  - no permissionless freeze (a region only waits for neighbours that anyone can advance);
  - the operator cannot choose outcomes (resolution is deterministic from committed orders and VRF).

### P5. Map growth: population-driven frontier with pre-committed seeds and explorer-paid materialisation

- **Proven by:**
  - Dark Forest radius ∝ sqrt(players) with rim spawn;
  - Primodium distance ∝ ln(n);
  - OPCraft pure terrain;
  - DUST Merkle-seeded regions and `exploreChunk`;
  - SAGE `discoverSector` (funder pays);
  - Eternum one-tile `explore`.
- **Our form:**
  1. **Rings of regions** around the six faction capitals (or around one shared centre). Ring *k* has 6k regions in a hex layout, so R rings hold 1 + 3R(R+1) regions.
  2. **Opening rule, like Dark Forest:** a new ring opens at the next tick boundary when active citizens ≥ ρ × open regions (for example ρ = 20 per region), or when frontier occupancy exceeds a threshold. The check is permissionless: anyone may call `OpenRing`, and it succeeds only if the on-chain counters satisfy the rule.
  3. **Terrain seed:** a MagicBlock VRF result requested at a **deadline fixed in advance** (for example when ring k−1 opened), stored before anyone can know it. Terrain is a pure function `f(seed, q, r)`, so client and program agree without storing tiles, as in OPCraft.
  4. **Materialisation:** the region PDA is created on first entry (`Explore`), paid by the explorer or the faction treasury, and refunded to the payer on season close (as in SAGE `funder`).
  5. **New players spawn at the frontier** (Dark Forest rim spawn) or under an existing structure's bounded slots (Eternum villages). Veterans cannot snipe newcomers, and a spawn shield of 24 h protects them (FrenPet shield).
- **Counter hot spot:** the population counter used by the opening rule must not be written by each join. Keep **per-faction or per-shard counters** (six or more accounts) and have `OpenRing` read and sum them. Joins then contend only within their own faction's counter, on base, where a fee auction favours the defender (platform.md §2.3, scale-design G1).
- **Avoid:**
  - HOTS's endless-bump allocator (anyone can inflate the map);
  - opening rings on operator command (the operator must not choose timing or the map);
  - DUST's operator-seeded Merkle roots **unless** the seed is VRF and committed before the season.

### P6. Hot-account discipline

Evidence:
- SAGE mining and scanning;
- BOLT `add_entity`;
- HOTS allocator;
- Supersize Map (acceptable only at 20 players);
- er-usdc-scaling-poc ("write conflicts dominate the scheduler … sheds … as drops");
- the ER free write-lock griefing described in platform.md §2.3.

Rules:
1. A **player instruction writes only accounts owned by that player**: their Member, their unit stacks, their ephemeral ballot or order account.
2. **Shared totals** (faction tallies, region stock, population) are written only by permissionless crank instructions. Those run once per window and read S shard accounts.
3. **Region accounts are written only by resolve and explore**, both permissionless and idempotent.
4. **Critical-path instructions touch few accounts**, so an attacker must lock many accounts at once to delay much.
5. The residual is free ER write-lock griefing of a region account. It stalls that region, and its neighbours by one tick. It cannot be fixed in the program. **Dedicated or ingress-filtered ER (F13 in scale-design) remains a precondition.** Its blast radius is now one region plus a ring of neighbours, instead of the whole world.

### P7. Sharding: instances, cohorts or regions, with asynchronous borders

- **Proven by:**
  - instances: Sky Strife, Supersize, Blitz factory, Loot Survivor;
  - cohorts: Pirate Nation Apex and Boss, MagicBlock's USDC PoC;
  - regions: colocation (Supersize rooms per region).
- **The hard fact:** no transaction spans two ERs, and cross-ER visibility means committing to base and cloning again (1–2 s plus fees; platform.md §2.4).
- **Our form: continents as ER shards.**
  - Inside a continent, P4 region ticks run on one ER. Borders are synchronous, because read-only neighbour outboxes are on the same ER.
  - Between continents, **sea lanes**: a fleet that departs writes to its continent's port outbox. The outbox is committed to base at each checkpoint, and the destination ER clones it. Arrival is scheduled ≥ K ticks later, where K × tick length > commit + clone latency (for example ≥ 2 ticks at 30 s).
  - This is Dark Forest arrivals, SAGE warps and Influence travel reused as a **latency-hiding** device.
- **Capacity sizing [estimate]:**
  - One ER node does about 90k trivial tx/s, and game transactions are 10–50× heavier, so about 2–9k game tx/s per node.
  - 50k players at one own-account action every 30 s is about 1.7k tx/s. 1,000 regions at a 30 s tick is about 33 resolves/s, at 60–120k CU each.
  - **A single well-provisioned ER could carry the whole map at 50k players.** Continents are for latency (colocation), blast radius and headroom, not raw throughput.
  - Must be verified with a spike on a local ER with a heavy-instruction mix. This is not measured.
- **Commit cost:**
  - checkpoints every 10 ticks means 18 commits per region per season;
  - with a delegated fee payer (commit 26 onward at 0.0001 SOL per account), 1,000 regions × 18 × 0.0001 ≈ 1.8 SOL per season **[computed; fee schedule from platform.md]**;
  - commit roots and per-region hashes, never whole tables.
- **Cohort alternative (Pirate Nation):** parallel "worlds" under six global banners (scale-design's realms). It is simpler, but it gives up "one map". **Continents give the owner the Eternum feel** (one world that grows) while keeping realm-like isolation.

### P8. Spam, bot and Sybil defences

| Threat | Proven defence | Our form |
|---|---|---|
| Action spam | energy or stamina (Pirate Nation, DUST, Eternum); durations (Influence) | per-unit `busy_until`; per-member daily action budget counted in the member's own account |
| Targeted DoS on a victim's queue | Dark Forest `checkPlanetDOS` 6+6 | per-region, per-faction slot caps; attackers never share the owner's quota |
| Free ER write locks | none in any project (EVM L2s charge gas) | F13 (dedicated or filtered ER) + small blast radius (P6) + deadline aborts |
| Sybil farming of rewards | entry cost (Loot Survivor VRF $1, Sky Strife orbs); points-share payouts (FrenPet) | USDC entry per membership; merit and payout per membership are sub-linear or capped so ten Sybils earn ≤ ten real members; no free-tier rewards |
| Sybil capture of governance | none solved on chain; Kamigotchi suggests social or proof-of-individuality | vote weight tied to paid membership and merit, with caps per member; sortition (VRF) for small juries (scale-design); cells of 32 |
| Bots | Kamigotchi and Dark Forest: accept them, give tooling to all | public SDK and bot API; disclosed operator AIs (already planned); no hidden information that a bot reads faster than a human (sealed orders with deadlines) |
| Spawn camping and new-player harassment | Dark Forest rim spawn; FrenPet 24 h shield; Eternum villages | frontier spawn + a 24 h shield per new stack + joining under the faction's existing structures |

### P9. Fees: who pays what

- **Base:** the player pays for their own Member creation and joins. On base the fee auction protects the defender (SAGE shows the opposite case, where many players share one account).
- **ER:** zero fee. The operator pays delegation and commits, which is predictable (P1 and P7 numbers).
- **Sponsored transactions,** if any, need a cap per member (FrenPet: 10 per day; Pirate Nation's game wallet), paid from the operator's 20%.
- **Session keys:**
  - scope them to our program;
  - set `valid_until` ≤ the season window (≤ 7 days is enforced);
  - **no top-up** when a relay pays;
  - keep an explicit revoke path.

### P10. Randomness: commit to a future beacon

- **Proven by:** Influence entropy rounds, DUST drand with 2 min and 10 min windows, Loot Survivor VRF per level.
- **Our form:**
  - MagicBlock VRF requested by a permissionless crank at deadlines fixed before the inputs close (terrain seeds per ring, combat seeds per region-tick, sortition);
  - the seed is bound to `(season, region, tick)`;
  - never use ER blockhash or slot, and never an operator-supplied seed.

### P11. Indexing and verification

- **Proven by:** the MUD store-indexer (events only, offchain tables), Dojo Torii (the owner already runs it), SAGE's shared config and state accounts.
- **Our form:**
  - Every region-tick emits one compact event (≤ 1 KiB) with the pre-state hash, the order commitment hashes, the VRF output and the post-state hash. The 10 KB log cap per transaction is never close.
  - The replay verifier works **per region, independently and in parallel**: it checks the hash chain per region plus the border outbox hashes it consumed. At 1,000 regions × 180 ticks that is 180k small replays per season. That is trivially parallel, where today's single world chain is not.

### P12. Instances as a fallback and as an on-ramp

Sky Strife, Supersize and Blitz show that short matches recruit players and make it clear to them how the game plays. A **"skirmish" instance** (one region, 1–4 factions, 30 minutes) using the same region engine is almost free once P4 exists. It can also serve as onboarding and qualification for the persistent world.

---

## 5. Implications for PERMUTATION STATE (Eternum-like, unlimited players, expandable map)

A proposal consistent with the audit rules. It is for the design team to weigh against scale-design's realms.

1. **The world is a set of region accounts** on the ER:
   - an axial hex map with 7-hex or 19-hex regions (or 16 × 16 = 256 hexes);
   - each region is a ≤ 4 KiB zero-copy account holding terrain overrides, ownership, buildings, and up to *S* stacks per faction;
   - border outboxes are separate small accounts, one per region.
2. **Six factions stay. Membership becomes unlimited.**
   - A member belongs to a faction and to a **cell** (a bounded group, as in scale-design).
   - Members act on their own accounts: economy, pledges, votes, endorsements, and unit stacks they own.
   - Regional commanders are elected per faction per region-cluster and submit the sealed per-region orders (P4).
   - Faction governance and merit stay per faction, sharded into cell accounts and folded by a crank.
3. **Region ticks, not world ticks** (P4):
   - skew between neighbours ≤ 1 tick;
   - a permissionless resolve with a crank tip, driven by an ER native crank for convenience;
   - deadlines are wall-clock, so no region waits on the operator.
4. **Expandable map** (P5):
   - rings open by an on-chain population rule;
   - VRF seeds are fixed before a ring opens;
   - regions are materialised by the first entrant, who pays and is refunded at close;
   - newcomers spawn at the frontier or under their faction's structures, with a 24 h shield.

   The six capitals are fixed, so the "one civilisation, six factions" geography remains.
5. **Continents as ER shards, joined by sea lanes** (P7), once one ER is not enough or for colocation. Start with **one continent on one ER**, measured.
6. **Economy stays solvent.**
   - USDC entry fees go to the vault on base.
   - Prizes are closed-form at claim time: faction score share × member merit share (as FrenPet's points share, but capped per member).
   - Region rent is paid by explorers or treasuries and refunded on close, as a conservation invariant (DUST's matter loop is the analogue).
7. **Verifiability:** a per-region hash chain, per-tick events, and a parallel replay verifier (P11).
8. **Residual risk:** free ER write-lock griefing of region accounts (platform.md §2.3; scale-design F13). It is reduced to a regional stall, but it is not eliminated without a dedicated or filtered ER.

**Spikes suggested, all local, on non-owner ports:**
- **S-R1:** CU and heap for one region resolve at an adversarial per-region cap (6 factions × S stacks, full borders).
- **S-R2:** ER throughput with a mix of about 60–120k-CU resolves and 10–25k-CU member actions (extends er-usdc-scaling-poc to game-shaped transactions).
- **S-R3:** a region write-lock griefing loop: what stalls, and how far it spreads through the skew rule.
- **S-R4:** a cross-ER port round trip (commit, clone, arrival) and its latency distribution, to size K.

---

## 6. What to avoid (with the project that showed it)

1. **A whole-world transaction** of any kind: resolve, decode, election or settlement loop (PERMUTATION STATE today; nobody else does this).
2. **Global counters written by joins or entity creation:** BOLT `add_entity` World counter, Primodium `AsteroidCount`, HOTS `LocationAllocator`. Use PDAs derived from ids or coordinates, and sharded counters.
3. **Shared per-location accounts written by every player action on base.** SAGE's `resource`, `planet`, `sector` and shared token accounts led to priority-fee wars (≈ 30% of earnings) and an exit from mainnet.
4. **Unbounded per-target queues.** Dark Forest had to add `checkPlanetDOS`. Every inbox needs fixed slots, with separate quotas for owner and outsiders.
5. **Permissionless map expansion that costs nothing** (the HOTS bump), and **operator-timed expansion.** Growth must follow an on-chain rule, and the caller pays.
6. **Randomness from ER blockhash or operator seeds.** Use VRF at deadlines fixed in advance (Influence, DUST, Loot Survivor).
7. **BOLT systems for large state:** 1 KiB return-data cap, per-call re-serialisation, 15–99k CU overhead.
8. **Relying on one operator's chain or sequencer for liveness** (Redstone shut down; z.ink is a single-studio chain). Keep permissionless cranks (native ER plus Hydra on base), undelegation escape hatches and claim-by-proof settlement.
9. **Pretending bots can be excluded** (Kamigotchi). Design for disclosed automation.
10. **Synchronous cross-ER interaction.** It does not exist. Model it as travel time (P7).

---

## Sources

MagicBlock and Solana:
- MagicBlock docs index: https://docs.magicblock.gg/llms.txt
- Magic Router: https://docs.magicblock.gg/pages/ephemeral-rollups-ers/introduction/magic-router.md
- Cranks: https://docs.magicblock.gg/pages/tools/crank/introduction.md ; https://www.magicblock.xyz/blog/native-cranks-for-ers
- Session keys security: https://docs.magicblock.gg/pages/tools/session-keys/security.md
- Magic Actions: https://www.magicblock.xyz/blog/magic-actions
- Dynamic colocation: https://www.magicblock.xyz/blog/dynamic-colocation
- Recaps: https://www.magicblock.xyz/blog/august2026-recap ; https://www.magicblock.xyz/blog/july2026-recap ; https://www.magicblock.xyz/blog/supersize
- BOLT: https://github.com/magicblock-labs/bolt (clone: `openworld/lab/mmo-src/bolt`, `crates/programs/world/src/lib.rs`, `docs/REPORT.md`)
- Session keys: https://github.com/magicblock-labs/session-keys (`programs/gpl_session/src/lib.rs`)
- Hydra: https://github.com/magicblock-labs/hydra (README)
- USDC ER scaling PoC: https://github.com/magicblock-labs/er-usdc-scaling-poc (README)
- Heroes of the Sun: https://github.com/magicblock-labs/heroes-of-the-sun
- Supersize (community copy): https://github.com/Lewarn00/supersize-solana
- Local crates: `~/.cargo/registry/src/index.crates.io-6f17d22bba15001f/ephemeral-rollups-sdk-0.17.2/src/{consts.rs,ephemeral_accounts.rs,crank/}` ; `…-1949cf8c6b5b557f/magicblock-magic-program-api-0.10.1/src/{args.rs,lib.rs,instruction.rs}` ; `solana-cpi-3.1.0/src/lib.rs:330` (`MAX_RETURN_DATA = 1024`)

Dark Forest:
- https://github.com/darkforest-eth/eth (`contracts/libraries/LibGameUtils.sol` `_getRadius`, `checkPlanetDOS`; `LibPlanet.sol`; `LibLazyUpdate.sol`; `facets/DFMoveFacet.sol`; `facets/DFCoreFacet.sol`)
- https://blog.zkga.me/announcing-darkforest ; https://deepwiki.com/darkforest-eth/darkforest-v0.6

MUD, Lattice and DUST:
- https://lattice.xyz/blog/making-of-opcraft-part-1-building-an-on-chain-voxel-game ; https://lattice.xyz/blog/making-of-opcraft-part-2-on-chain-procedural-terrain-generation
- https://mud.dev/world/account-delegation ; https://mud.dev/store/tables ; https://mud.dev/indexer
- https://github.com/dustproject/dust (`packages/world/src/Constants.sol`, `systems/TerrainSystem.sol`, `systems/NatureSystem.sol`, `utils/EnergyUtils.sol`)
- Lattice wind-down: https://x.com/latticexyz/status/2044103611072835744 ; https://www.panewslab.com/en/articles/019d8f0b-2d85-712a-8f9f-8db4bf1bb60a
- Sky Strife: https://lattice.xyz/blog/unveiling-season-0-sky-strife-redstone-testnet-release ; https://lattice.xyz/blog/sky-strife-mainnet ; https://x.com/skystrifeHQ/status/1783556467104088403
- Primodium: https://github.com/primodiumxyz/primodium (`packages/contracts/src/libraries/LibAsteroid.sol`, `LibMath.sol`, `LibResource.sol`, `LibUnit.sol`)
- This Cursed Machine: https://world.mirror.xyz/rkksgs0tK5KQWxvNRfcmo8KYXYLc-Ljr7V1Gt29Z7rA ; https://app.devcon.org/schedule/UBFQ9V

Other games:
- Influence: https://github.com/adaliafoundation/influence-starknet (`src/components/crew.cairo`, `src/common/random.cairo`, `src/common/position.cairo`, `src/systems/scanning/*`) ; https://www.starknet.io/blog/influence-live-on-mainnet/
- Pirate Nation: https://docs.piratenation.game/important/multichain/boss-faqs ; https://blog.arbitrum.io/building-the-future-of-gaming-how-proof-of-play-scaled-pirate-nation-with-arbitrum/ ; https://docs.piratenation.game/learn/about-our-tech/wallet-popup-free-and-gas-less-gameplay ; https://piratenation.medium.com/introducing-wallet-based-energy-system-in-pirate-nation-2a3239418b6e ; https://docs.piratenation.game/learn/the-game/vip
- Star Atlas SAGE: https://unpkg.com/@staratlas/sage/src/idl/sage.ts (IDL 1.9.0-alpha.16) ; https://build.staratlas.com/dev-resources/apis-and-data/sage ; https://blockzeit.com/sage-labs-daily-transactions/ ; https://blockworks.com/news/svm-layer-1-star-atlas-launch ; https://aephia.com/star-atlas/z-ink-everything-you-should-know/
- Loot Survivor: https://github.com/BibliothecaDAO/loot-survivor (`contracts/game/src/game/constants.cairo`) ; https://www.starknet.io/blog/onchain-gaming/loot-survivor-and-influence-fully-onchain-games-on-starknet/
- Eternum and Blitz: https://github.com/BibliothecaDAO/eternum (local clone `openworld/lab/eternum-src`) ; https://medium.com/@AlexiaChukwuma/eternum-or-blitz-choosing-your-battle-in-starknets-flagship-rts-c017ef48cb98 ; https://eternum-docs.realms.world/
- FrenPet: https://docs.frenpet.xyz/ ; https://docs.frenpet.xyz/blog/v2/ ; https://docs.frenpet.xyz/pvp/
- Bots: https://www.bankless.com/onchain-games-bots
- MafiaBits and other Solana titles: https://playtoearn.com/news/5-solana-games-blowing-up-right-now-march-2026

Companion research: `scratchpad/scale/understand/platform.md` (ER costs, rent, write-lock griefing, delegation) and `scratchpad/scale/scale-design.md` (realms, Commons, F13).
