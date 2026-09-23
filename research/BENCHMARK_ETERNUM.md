# Eternum (Realms.World) benchmark for a hex-based seasonal civilization game on Solana with human and AI players

**Evidence tags.** Every claim carries one of these:
- **[SRC]** comes from the local repo at `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/work/eternum`. The clone is shallow, one commit, around 2026-09-20.
- **[WEB]** is reported by a public web source. Those are listed at the end.
- **[INF]** is my own inference.

**Docs vs. code.** The player docs (`apps/game-docs/docs/pages/...`) mostly describe **Season 1 (2025)**. The config (`config/source/...`) and contracts show the **current 2026 state**. Where they disagree, both are given.

---

## 0. Executive summary

- **Shape of the game [SRC].** Eternum is a Travian/Catan-style 4X game: hex world, production chains, armies driven by stamina. It runs as one long season built around **Hyperstructures** and Victory Points (VP). Over 2025–26 it split into two formats:
  - **Eternum**: multi-week seasons, entry via NFT.
  - **Blitz**: 60-minute matches, up to 24 players per lobby, $LORDS entry fees.

  One set of Cairo contracts serves both. Each match is a `game_id` row inside one persistent Dojo world, and balance comes from immutable presets (`AGENTS.md`).
- **The main trend: complexity went down.** The team cut from 22 resources to 9 (Blitz and the current Eternum recipes). They added "simple mode" production that runs on labor alone, and moved from weeks to 60 minutes. Blitz became "the arena" and Eternum "the sandbox" (README). **[SRC]**
- **Infrastructure churn was heavy.** The stack went from Starknet mainnet (Season 0, paid gas) → Cartridge Controller/paymaster (Season 1) → a fee-free Cartridge Katana appchain → a self-hosted **Madara L3** with no Cartridge dependency. Cartridge is described as end-of-life (`docs/plans/realms-phase-1-brief.md`). **[SRC]**
- **Agents are a stated pillar.** The docs describe an "Agent-Native Environment" where humans and AI play under one rule set (`overview/introduction.mdx`). In practice, agents have come in two forms:
  - Season 1 **NPC "Daydreams" agents**: bounty-carrying armies driven by one controller address.
  - A **"hired agents"** product (designed 2026-09-12, not built): external LLM agents play *as a player's own account* through key delegation.

  Three earlier headless-agent codebases were deleted. **[SRC]**

---

## 1. Core loop and map

**The loop [SRC, docs + config]:**
1. Settle a Realm (burn a Season Pass) or a Village (mint a Village Pass).
2. Build on a local hex grid ("hexes within hexes").
3. Produce food, then resources, then labor and troops.
4. Explore fog with armies (stamina + food).
5. Claim world structures from bandits.
6. Build or hold Hyperstructures for VP.
7. Trade via Banks/AMM and the order book, and raid or conquer neighbours.

**Hex model [SRC].**
- Offset hex coordinates centred at `SETTLEMENT_CENTER = 2147483646` (`config/source/common/base-config.ts`).
- The docs call the world "infinite, procedurally-generated". In practice settlement is limited to concentric **layers** around the centre: `base_distance 8` (Eternum 10), `layers_skipped 2`, `layer_max 6`, `layer_capacity_increment 6`, `layer_capacity_bps 8000`.
- Players choose from a list of slots offered in those layers. They cannot place freely.
- There are two map views: a world hex view and a local build view (`eternum/world-physics.mdx`).

**Entities and scale [SRC].**
- 8,000 Realms, each producing 1–7 resources according to the original NFT metadata.
- Up to 6 Villages per Realm (48,000 possible). Villages produce at 50%, have one random resource, and **cannot be conquered**, only raided. They can receive troops only from their parent Realm, which stops people "teleporting" troops via donkeys.
- 50 Wonder Realms give +20% production within 12 tiles in the Season 1 docs. The current config has `WONDER_PRODUCTION_BONUS_PERCENT_NUM = 0`, and Wonders have been repurposed for **Faith** (see §4).
- Realm levels run Settlement → City → Kingdom → Empire. Each level unlocks buildable hexes, guard slots and deployment caps (`eternum/levels.ts`). Upgrades cost labor, wheat, fish, **Essence**, and some wood/coal/copper.

**Fog and exploration [SRC].**
- At start only the 6 Banks are visible. Settling reveals the adjacent ring.
- Exploring a hex costs stamina (30 in the docs; the current config uses 30 of a 120 maximum) plus 0.03 wheat and 0.03 fish per troop. It **reveals the hex permanently to everyone** and pays a random resource stack.
- Discovery rolls happen on exploration (`eternum/exploration.ts`):
  - Fragment Mines: 1,000/50,000 (2%). The Season 1 docs said 1/150.
  - Bandit camps: 1,500/50,000 (3%).
  - Hyperstructure foundations: the chance decays with distance from the centre and with the number already found. Docs formula: `0.975^dist × 4% − 0.1% × found`, so effectively within about 300 tiles of the centre.
- A global Relic Chest timer spawns a chest after an exploration once enough time has passed.

**Biomes [SRC].**
- 16 biomes. Each troop type gets +30%, −30% or 0% damage in each biome, plus biome-specific stamina costs for movement.
- Starting troop type is picked by the settlement's biome (`models/troop.cairo`, biome → T1 type table).

**How space constrains play [SRC].**
- One army per hex, and armies cannot pass through occupied hexes. Armies are therefore walls.
- Field armies deploy onto one of the 6 hexes next to their structure. If those hexes are full, you cannot deploy.
- Value is concentrated at the centre: Banks sit in a ring, and Hyperstructures cluster inside it.
- In Blitz, Hyperstructure VP/second scales with **how many player Realms are within 8 tiles** (1–6 VP/s). Remote hyperstructures earn only the one-time claim VP (`blitz/victory.mdx`). **[INF]** This deliberately pulls players into contested ground.

---

## 2. Economy

**Resources [SRC].**
- `packages/types/src/constants/resource-ids.ts` lists 22 resources (ids 1–22):
  - Common: Wood, Stone, Coal, Copper, Obsidian, Silver
  - Mid: Ironwood, Cold Iron, Gold, Hartwood, Diamonds, Sapphire, Ruby, Deep Crystal, Ignium, Ethereal Silica
  - Rare: True Ice, Twilight Quartz, Alchemical Silver, Adamantine, Mithral, Dragonhide
- Other materials: Labor (23), Ancient Fragment (24), Donkey (25), 9 troop ids (3 types × 3 tiers), Wheat and Fish, LORDS, Essence (38), relic items (39+), and **Research (57)**.
- Tiers show up as AMM seed liquidity, from Wood 2,000,000 down to Dragonhide 30,000 (`eternum/economy.ts`), and as labor yield when burned, from 10 to 30 labor per unit.

**Production chains (current config, `eternum/resources.ts`) [SRC].**
- Only **9 resources have recipes**, the same 9 as Blitz: Wood, Coal, Copper, Ironwood, Cold Iron, Gold, Adamantine, Mithral, Dragonhide. The other 13 have empty input arrays in this config.
- Example recipes:
  - Wood = 1 wheat + 1 fish + 0.2 coal + 0.2 copper
  - Adamantine = 3 wheat + 3 fish + 0.9 coal + 0.6 ironwood
- Food (wheat 6/s, fish 6/s) is the only input-free production.
- **Standard mode** uses resources and is efficient. **Simple mode** uses food and labor only and is inefficient. It exists so a player without the right inputs is never blocked.
- In Season 1, production changed from "streaming while inputs last" to **committing the full inputs upfront for a timed order** (`production.mdx`).
- **Labor** is made by burning resources in the Keep and cannot be bridged out. It functions as a universal sink and a converter between resources.

**Troops [SRC].**
- T1 costs 2 wheat, 2 fish and 0.4 copper.
- T2 costs 10 T1 + rare metal + 1 Essence.
- T3 costs 10 T2 + rarer metal + 3 Essence.
- Damage multipliers are T2 = 3×, T3 = 9×. The docs' Troop Tiers page says 2 lower-tier troops make 1 higher-tier troop. That differs from the 10-unit recipe inputs, so the exact exchange rate depends on how output amounts are applied **[INF]**.

**Storage and weight [SRC].**
- Every material has a weight. Storehouses add capacity, and **production past full storage is wasted** (`storage.mdx`).
- Blitz removed storage entirely in September 2025 (changelog 3-Sep-2025). **[INF]** It was friction that did not pay for itself in short games.

**Donkeys and transport [SRC].**
- Every transfer between structures consumes **single-use donkeys**. The Season 1 docs give 500 kg per donkey; the current config's `CapacityConfig.Donkey` is `50 * 1000`.
- Speed is `donkey_for_resources 9` and `donkey_for_troops 27`.
- In Season 1, donkeys cost **$LORDS** to produce. The docs call them "the gas of this onchain world". In Blitz they cost food only.
- Season 1 added **arrival gates**: deliveries can only be claimed at the start of the next hourly Eternum Day, "to reduce the load that resource arrival entities have on the game". This is a gameplay rule created by chain limits. The config has `DELIVERY_TICK_INTERVAL_SECONDS = 180`.
- Hyperstructure contributions need no donkeys.

**Trade [SRC].**
1. **Bank AMM (in-world)**
   - 6 Banks, each starting with bandits, all sharing liquidity.
   - Trades route to the **nearest** Bank, and **the conqueror of that Bank earns the owner fees**.
   - Config: `lpFees 15/100`, `ownerFees 15/100`, `lordsLiquidityPerResource 2,000`, Bank cost 1,000 LORDS. The docs warn that AMM fees are "relatively high" and suggest the order book first.
2. **Order book** (P2P, lower fees, OTC with allies), `trade.maxCount 10`.
3. **The Agora** (newer): a constant-product L2 AMM with LORDS/resource pools, routed swaps, and ERC-20 LP tokens (`docs/agora.md`).
   - Fees: 1% LP + 3% protocol, the protocol share going to veLORDS.
   - Separately, `realms-value-plane-design.md` records an L2 ammv2 already live with 35 LORDS pools at a 1.5% fee.
   - It is indexed by a Hono/Postgres indexer with OHLCV candles.

**Bridging fees (`common/base-config.ts`) [SRC].**
- Each deposit and each withdrawal is charged:
  - 2.5% to veLORDS
  - 2.5% to the **season prize pool**
  - 2.5% to the client/frontend
  - 5% to the Realm (the Realm owner earns 5% on everything its Villages bridge out)
- **[INF]** Up to 12.5% per direction is a strong friction on extracting value.
- The docs add that portal efficiency improves as Hyperstructures are completed.
- The 2026-08-29 audit found the "bridge" is really a same-chain ERC-20 window that **mints when short on withdraw** (`systems/utils/bridge.cairo:40-48`) **[SRC]**. That is an inflation surface a designer should avoid.

**Where fees go, in practice [SRC].** Money routes to veLORDS stakers, the season pool, the client operator, Realm owners (village tax) and Bank holders. **[INF]** Controlling a Bank or a Realm is therefore an income position, which creates military targets out of economic infrastructure. That is a good design pattern.

---

## 3. Military

**Units [SRC].**
- Three types: Knight, Crossbowman, Paladin. Three tiers.
- An army is **one type and one tier**. Maximum army size is 30,000 (Blitz cut this from 100,000 in November 2025).
- Deployment caps by level: Settlement 3,000 / City 15,000 / Kingdom 45,000 / Empire 90,000 "strength". Strength = troops × tier strength (1/3/9), with tier modifiers 0.5/1.0/1.5 (`eternum/troop.ts`).
- **Combat v3** (changelog 5-Jun-2026):
  - Crossbowmen get **range 2**: 70% damage to field armies, 30% to guards, no counter-damage, no biome modifier.
  - Knights get +15% when assaulting guards, and take 15% less damage when guarding.
  - Paladins are the mobile cavalry.
  - This produces a rock-paper-scissors of roles by combat context.

**Stamina [SRC].**
- Current config: initial 20, +20 per army tick, max 120. Attack costs 50, defence 40, travel 20, explore 30.
- The Season 1 docs used smaller numbers: +2 per 10-minute phase, attack 5.
- Stamina refills at phase boundaries, not continuously.
- A defender below the defence requirement deals **−30% damage**, so an exhausted army is vulnerable.

**Combat formula [SRC, `contracts/l3/game/src/models/troop.cairo:720-906`]:**

```
Damage_A→B = S · N_A · Tier_A · Biome_A · Stam_A · Timer_A / Tier_B / (N_A + N_B)^0.2
  S = 2 (scaling), Tier = 100 / 300 / 900
  Stam_B = 0.7 if the defender lacks stamina; Timer_B = 0.85 if the defender is on battle cooldown
  × (1 + d20 roll %) each side; × relic boosts; × role multipliers (crossbow / knight)
```

- Damage is applied to both sides at the same time.
- Each side then gets a **battle timer** of about one phase. **Damage-ratio refunds** apply: at a ratio of 10× or more, the winner gets its stamina back and no timer; at 2.5× or less, nothing. This stops small armies stalling large ones indefinitely.
- **[INF]** The `(N_A+N_B)^0.2` term softens Lanchester-style snowballing of big stacks. Worth copying.

**Raiding [SRC] (`raiding.mdx`).**
- An adjacent army steals the **rarest materials first**, up to its carry capacity.
- Success chance scales linearly from 0% (raider damage below 50% of the guards') to 100% (above 200%).
- Casualties are only **10%** of normal combat (`damageRaidPercentNum 1000`).
- The docs contradict themselves on whether LORDS can be raided. The later text says tokens can only be taken by **claiming a Realm**.

**Defense and claiming [SRC].**
- Guard armies sit in defence slots and are attacked from the outermost slot inward. A destroyed slot is locked for a delay (config `guardResurrectionDelay 600` s).
- Once all guards are gone, an adjacent field army can **claim the structure**. That transfers the Realm's gameplay rights, and in Blitz the captured Realm's deployed armies go to the conqueror.
- Villages cannot be claimed.
- New structures get 24 ticks of immunity (`BATTLE_GRACE_TICK_COUNT`). Season 1 made **all** structures immune for the first 24 hours, with no immunity for late settlers, who were told to scout with spectator mode first.

**How war ties into the economy [SRC + INF].**
- Troops are resources, tokenised until deployed. Once deployed they are "doomed to their fate within this season".
- Movement burns food. Tier-ups burn rare metals and Essence.
- Structures like Banks, Fragment Mines and Hyperstructures produce income or VP, so war is fought over economic chokepoints.
- Raiding is a low-casualty way to tax weak producers. **[INF]** This is exactly what punishes casual players, and it is why Villages were made unconquerable.

---

## 4. Victory, season structure, prizes

**Eternum victory [SRC, `victory.mdx`, `eternum/hyperstructures.ts`, `points.ts`].**
1. **Find** a foundation (exploration), then **clear its bandits**.
2. **Activate** it with Ancient Fragments. Fragments only come from finite Fragment Mines holding 300k–3M, which have a single guard slot.
3. **Construct** it with randomised amounts of all resources plus 50M labor, public or tribe-only.
4. **Contributors earn VP** in proportion to what they put in: 21,732 VP per resource slot, so about 500k VP per structure.
5. The **owner accrues VP per second** (config: 1 VP/s) and can split that stream with other players using a share split.
6. Awarded VP **cannot be lost**.

The first player to reach `pointsForWin` can call **End Season** (`systems/season/contracts.cairo: season_close`).
- Season 0's threshold was 6,320,000 VP [WEB].
- The current base configs have `pointsForWin = 0`, i.e. **no win condition configured**; the value-plane audit says so explicitly.
- The Madara sandbox sets 10M VP with a 30-day duration.

Current VP sources also include exploration (10/tile), bandit claims (500, or 3,000 for a hyperstructure) and relic chests (1,000).

**Alternative "peace" victory: Faith [SRC, `systems/faith/contracts.cairo`, `economy.ts`].**
- Realms and Villages **pledge** to a Wonder and generate Faith Points (Wonder 50/s, Realm 10/s, Village 5/s per config).
- The FP split is 30% to the Wonder owner and 70% to the pledger.
- A Wonder can **submit** to another Wonder, feeding it 50 FP/s.
- Wonder owners can blacklist pledgers.
- A separate prize contract (`faith/prize_contracts.cairo`: `fund_prizes`, `distribute_wonder_prizes`) pays the winning Wonders and their followers.
- **[INF]** This is the closest existing analogue to the "peace/neutral" condition you want: a diplomatic, coalition-based leaderboard that runs in parallel with military VP. Directly relevant.

**Other side-tracks [SRC].**
- **Artificer:** burn Research to get relics.
- **Bitcoin Mines:** found on an "Ethereal" alternate layer. Labor is contributed per 10-minute phase, and a VRF lottery mints "SATOSHI" to one winning mine.
- **[INF]** These look like experiments with "science" and lottery-style sinks.

**Season structure [SRC + WEB].**
- Season 0: 9 Dec 2024 → closed 30 Dec 2024, earlier than the ~6 Jan target. Scores were recalculated and resubmitted. [WEB]
- Season 1: passes 30 Apr 2025 → settling 7 May → "Empire" 14 May 2025. Season 1 docs describe a 7-day settling window. [WEB/SRC]
- Current config timing: settling starts 1 day after creation, main phase at 1.5 days, 1-day end grace, **bridge closes 7 days after the end**.
- The team states outright that nothing, "not even the developers", can change a launched season (`world-physics.mdx`). **[INF]** Balance errors therefore persist for the whole season, which is part of why the team moved to short Blitz matches.

**Season 1 prize pool [SRC, `eternum/prize-pool.mdx`]: 1,000,000 LORDS + 100,000 STRK.**
- **Victory: 300k LORDS + 50k STRK, plus 2.5% of LORDS bridging volume.**
  - Paid to the **top 10 tribes** by summed VP: 30/18/12/9/7/6/5/5/4/4%.
  - Within a tribe, the **leader takes 30% regardless of VP**, and members split 70% pro rata by VP.
- **Achievements: 300k LORDS**, pro rata by achievement points. Details were hidden until launch "to prevent planned farming". Quests were later "temporarily disabled" (changelog 31-Jan-2026).
- **Daydreams agent prizes: 250k LORDS + 25k STRK.** Each NPC agent carried 10–35 LORDS, won by combat or by "persuasion via chat". 25k STRK went to the first 100 players to defeat 10 agents ("Nexus-6").
- **Arts & Emissaries: 150k LORDS + 25k STRK**, awarded at team discretion.
- Season 1 prizes were **distributed manually by the team**; only the agent rewards were collected in-game. Unused LORDS went to veLORDS.
- Web: the Season 1 pool was reported as "≈$40,000" [WEB]; 2,244 reward chests went out based on achievement score [WEB].
- **[INF]** Most of the pool was **treasury-funded, not player-funded**. Fees only topped it up.

**Blitz prize pool [SRC, `blitz/prize-pool.mdx` + contracts].**
- Entry fees are split **70% prize / 15% veLORDS / 15% dev**, and sponsorship is added on top. Gladiator entry was 250 LORDS; the historical mainnet intent was 100 LORDS with a 30% cut.
- The number of winners depends on lobby size and the share of sponsorship, clamped to 2–60% of the lobby.
- Payouts decay geometrically with `s(N)=0.3+0.64(1−N^−0.7)`.
- Only players who scored can win. A lobby of one gets a refund.
- **Distribution is permissionless**: anyone submits the ordered ranking, the contract verifies the order, and players claim.
- Chests go to anyone with ≥500 VP.
- Brackets: Recruit (free) → Gladiator (paid) → Warrior (series) → Elite (entry by NFT invite, DAO-funded pool).
- MMR is onchain and non-transferable. The audit found the MMR token was **only deployed on mainnet, so MMR never ran on the chain Blitz runs on**, plus two maths defects (`realms-value-plane-design.md`).

**What carries over between seasons [SRC].**
- Realms NFTs, and a free Season Pass per Realm each season, tradable on a marketplace.
- $LORDS and any bridged-out resource ERC-20s.
- Cosmetics and Loot Chests (L2 ERC-721s).
- MMR.
- **Nothing inside the world persists.** Blitz explicitly resets every game.

---

## 5. Onchain architecture

**Everything gameplay-relevant is onchain [SRC].**
- Cairo/Dojo models (`contracts/l3/game/src/models`: structure, troop, resource, hyperstructure, guild, faith, bank/market, rank, season, agent…).
- Systems (`systems/`: combat, production, trade, bank, hyperstructure, season, prize_distribution, faith, guild, village, relic…).
- Shared libraries (`combat_library`, `raid_library`, `biome_library`, `rng_library`).
- Randomness: dice rolls, discoveries, lotteries via VRF, originally Cartridge VRF.

**What is offchain [SRC].**
- Rendering (React + Three.js).
- Indexing: first Torii, now **Herald** (`apps/herald`). Herald folds blocks, keeps a pre-confirmed overlay, and streams snapshots and ordered diffs into the client's RECS store.
- The AMM indexer, chat (`realtime-server`), identity/login (`apps/realms`), launch scheduling (`launch-service`), and the L2↔L3 relay (`apps/operator`).
- **Browser-side automation.** Production and transfer "orders" run in your tab every minute, and "the game tab must remain open" (`automation.mdx`). **[INF]** This is effectively a first-party bot for everyone, which levels the field against scripts and reduces the pressure to play 24/7.

**Time model [SRC].**
- `default tick 1 s`, `armies tick 60 s`, `delivery tick 180 s`, `phase 600 s`.
- Eternum Day = 1 hour with 6 phases (Season 1 docs); Blitz Day = 6 minutes.
- Accrual is **lazy**: balances are computed as `stored + rate × elapsed` and harvested whenever touched (`resource.cairo:78-94`). That means no global tick transactions. It also causes client/chain display mismatches, e.g. the "negative wheat" bug in `negative-food-balance-codex-brief.md`.

**Transaction costs and chain [SRC + WEB].**
- Season 0 ran on Starknet mainnet with paid gas. About 800 players used "up to 50% of Starknet blockspace and 75% of compute" [WEB].
- The team said it "underestimated demand" and re-engineered contracts and servers for Season 1 [WEB].
- Game chains then became **fee-free appchains**: Katana `no_fee = true`, and Madara `--no-charge-fee` with a 2 s block time and 2.2 ms p50 block close (`realms-phase-1-brief.md`).
- The target is a 96-player Blitz, load-tested with a 96-bot harness (`deploy/madara-lab`). A live September 2026 Blitz had 84 players, and boot-to-playable took about 50 s (`boot-to-playable-codex-brief.md`).

**Accounts and session keys [SRC].**
- Season 1 used the Cartridge Controller: passkeys, session policies, paymaster.
- Now: a per-world **gameplay account**. Its key lives in the browser, it is deployed fee-free, and it is bound to a Sign-in-with-Starknet identity.
- The onchain registry allows **one gameplay account per identity**. Key rotation is the recovery and delegation primitive (`overview/controller.mdx`, `player_account.cairo rotate_public_key`).

**Value split [SRC, decided 2026-08-26].** "Value lives on L2 where identity lives; the L3 holds game state and nothing worth stealing."
- Entries, deposits and loadouts are relayed L2→L3.
- Ranks, chests and withdrawals are posted L3→L2 by an operator, planned to become proven messages later.
- **[INF]** On Solana the direct analogue is: the game runs on an Ephemeral Rollup, MagicBlock-style, and escrow and payout sit on the L1 program.

---

## 6. AI and agents (extra depth, per the coordinator's request)

**A. Season 1 NPC agents ("Daydreams") [SRC + WEB]**
- The model is `models/agent.cairo`: `AgentOwner` per explorer, with a `controller_address`, a concurrent cap of 1,000, a lifetime cap of 10,000, and a 10–35 LORDS bounty per spawn (`base-config.ts`).
- Agents spawned via exploration (`agentFindProbability`). Troop bounds were 500–15,000.
- Personalities came from onchain randomness, ranging "aggressive to pacifist". They could explore, attack, pillage and **chat**, and players could persuade them in chat to hand over their LORDS [WEB].
- Daydreams is an open-source LLM agent framework from the Realms/Loaf side, related to the Eliza ecosystem [WEB]. The team aimed for "1,000 agent players" [WEB].
- **Today**, `agentFindProbability: 0` in `eternum/exploration.ts`, so **NPC agents are switched off** in the current config.
- **[INF] Lessons:**
  - NPC agents holding bounties worked as a **PvE income faucet and a tutorial target** ("Nexus-6").
  - Chat-persuasion payouts invite prompt-injection farming, so they are an exploit surface.
  - A single controller address makes all agents one centralised actor.
  - I found **no public evidence** of how much of the 250k-LORDS agent pool was actually claimed, or how often the LLM behaviour was exploited.

**B. External and "hired" agents as players [SRC]**
- Blitz docs state that "human and AI agent players compete under the same ruleset" and that agents may enter every bracket.
- **The failure history is candid.** Three headless-agent efforts were deleted (`hired-agents-architecture-brief.md §1`):
  - `packages/client`: a re-implemented headless client whose maths drifted (`computeStrength = count*tier`, building costs wrong).
  - `packages/game-agent`: a Pi-based framework with no call site.
  - "Axis" onchain-agent: 10k lines tied to Cartridge sessions, which died along with Cartridge.
  - The lesson they drew: **one shared client for UI and agents**, never a parallel re-implementation.
- **The planned design (not built yet):**
  - One `createGameClient()` package provides `views` and `actions` for both the web UI and agents.
  - Each agent runs in its own sandbox using `observe`, `list_actions`, `act`, `simulate` (the same combat and raid simulators the UI uses), `remember`, and `report_to_owner`.
  - **Wake-on-diff**, not a fixed cadence, plus heartbeats of 5 minutes in Blitz and 30 in Eternum.
  - A deterministic "nothing changed, skip the LLM call" gate as the main cost lever.
  - **Keys never enter the sandbox**: a signing lane per owner signs intents. The LLM key is injected at a proxy that meters cost per agent, and Stripe bills it.
- **Fairness decision:** an agent plays *as the owner's single gameplay account*, keeping one seat per identity per game. Prizes and MMR stay with the owner, and control is taken back by rotating the key. So "agent vs human" is not a separate class. Each **identity** gets one seat, whoever drives it.
- **[INF]** Sybil resistance therefore rests on the identity binding, a Starknet wallet plus SIWS, which is weak unless entry fees or NFT gates add cost. Eternum's real gates were the Realm NFT / Season Pass and paid Blitz entry.

**C. Documentation for LLMs [SRC].** The docs are exported at `/llm.txt` (`development/llm.mdx`). Copy this.

---

## 7. What worked vs. what failed in practice

Public post-mortem data is **thin**. Most figures come from promotional Bankless articles or X. One widely indexed Medium piece ("The Evolution of Eternum", @Oddsanjay) claims:
- "Season 4";
- 60% of Realms held by the top 15 guilds;
- $30–50 of gas per player over 10 weeks;
- bot detection in Season 3.

**I treat those as unverified and probably unreliable [WEB-low].** They clash with the repo, which shows only Season 0 and Season 1 as Eternum seasons and fee-free chains from 2025 on, and the page returned 403 so I could not check its sources.

**Worked**
- **Concept and demand [WEB].** About 800 Season 0 sign-ups saturated Starknet capacity, and the game was repeatedly called Starknet's flagship game.
- **Two-tier risk entry [SRC].** Realms carry high stakes and can be conquered. Villages carry low stakes, cannot be conquered, and are cheap. Season 1 also added stablecoin entry through Controller [WEB]. That gives casual players a way in without being farmed.
- **Short, fair, repeatable format [SRC].** Blitz gives three identical Realms, no NFT advantages, 60 minutes, brackets, MMR and scheduled prime-time slots for US, EU and APAC. It directly answers the "month-long campaign" problem. Web sources credit Blitz to the retention gap: "veterans hooked, new players needed better onboarding" [WEB].
- **Mechanics that damp snowballing [SRC]:**
  - the `(N_A+N_B)^0.2` damage term;
  - stamina and battle-timer refunds;
  - 10% raid casualties;
  - lost VP never lost;
  - Blitz hyperstructure VP tied to nearby Realms;
  - a claim bonus paid only the first time a structure is taken from bandits.
- **Economy infrastructure that creates politics [SRC].** Bank owner fees, the Realm tax on villages, public vs. tribe-only hyperstructure construction, and **VP share-splitting** by owners. Together these make diplomacy concrete and onchain.
- **Transparent, permissionless payout (Blitz) [SRC].** Anyone can submit a verified ranking and claim.

**Failed or struggled**
- **Complexity and onboarding [SRC + WEB].** Season 1 had 22 resources, 2 production modes, donkeys, weights, storage, labor, fragments, relics and tribes. The team's own trajectory is a retreat: 9 resources, no storage in Blitz, "resource/labor mode" renames, Realms starting at City, starting stockpiles doubled (changelogs Nov 2025–Mar 2026).
- **Chain limits leaking into design [SRC].** Hourly arrival gates, the Season 0 overload, and players manually claiming arrivals.
- **Immutable seasons plus manual prize distribution [SRC + WEB].** Season 0 closed early (30 Dec vs. about 6 Jan) and **scores were recalculated and resubmitted** [WEB]. Season 1 prizes were paid manually, and the Arts pool was discretionary. **[INF]** Credibility suffers when payout is off-chain.
- **Tribe prize design [SRC + INF].** Leader 30% "regardless of VP", top 10 tribes only. This rewards forming mega-tribes and recruiting passive members, and concentrates value in leaders. That fits the unverified claims of guild concentration, but I have no hard numbers. The guild contract is minimal: create, join, whitelist, kick. There are no alliance or treaty primitives, and "alliances and declarations of war" are purely social (`tribes.mdx`).
- **24/7 pressure [SRC + INF].**
  - Hour-long days, 10-minute phases and raids at any time mean multi-week seasons favour always-online players and scripts.
  - The first-party automation needs an open tab.
  - Only one 24-hour global immunity window, with none for late joiners.
  - Blitz scheduling by time zone is the implicit admission.
- **Achievement farming [SRC].** Quest details were hidden to stop farming, and quests were later disabled.
- **Value leakage [SRC].** Mint-on-short bridging, a hardcoded burner marked `todo` as the veLORDS recipient, a dead sponsorship branch, an MMR token never deployed where Blitz runs, and `pointsForWin = 0`. These are symptoms of config drift across many chains and presets.
- **Platform dependency [SRC].** Relying on Cartridge (Controller, paymaster, VRF, Torii, Slot) forced a full stack migration when Cartridge went end-of-life.

---

## 8. Lessons for your game

The target is a Solana hex civilization game: one persistent shared world, many civilizations, humans and external AI agents as full equals, several victory types, and a prize pool funded by entry and transaction fees.

### Copy this
1. **One engine, many presets.** Treat a season or match as a `game_id` row with immutable balance presets. Run a short "Blitz" format alongside the long world as a learning ramp and a way to tune balance (`AGENTS.md`, Blitz).
2. **One shared client and simulator for humans and agents.** Expose typed `views`/`actions`/`simulate` from the same code the UI uses. Eternum lost three agent stacks to drift. Publish `llm.txt` docs and an action catalogue.
3. **Economic infrastructure that is capturable.** Banks/markets that pay their holder, a Realm tax on dependents, VP share-splits. These make war, trade and diplomacy the same game.
4. **Anti-snowball combat maths.** Sub-linear army-size term, stamina and timer refunds, low-casualty raids, VP that cannot be lost once banked, a first-capture bonus from neutrals, and objectives that pay more where players are dense.
5. **A low-stakes seat type.** A settlement that can be raided but not conquered, for casual humans and cheap agents, with reduced output and troop-import limits so it cannot be used to teleport armies.
6. **Permissionless, verifiable payout.** Anyone submits the ranking, the contract verifies it, winners claim. Include a refund for a lobby of one, and a formula for the number of winners that adapts to lobby size.
7. **A Faith-style peaceful path.** Pledging, vassalage ("submission") and blacklists already model a non-military, coalition-based victory. Build your peace/neutral condition from this pattern.

### Avoid this
8. **Treasury-funded pools paid by hand.** Eternum's Season 1 pool was mostly DAO tokens, distributed manually, with discretionary categories. Fund the pool from entry and transaction fees in escrow, with formula-only payout.
9. **Rewards set by tribe leaders or fixed by rank.** A 30% leader cut regardless of contribution, and top-10-tribe-only payouts, encourage mega-blobs. Pay individuals by verifiable contribution, and cap coalition size or its share of each victory track.
10. **Rules created by chain limits.** Hourly arrival gates, manual claiming and single-use donkeys-as-gas added friction. Size your throughput first (Eternum saturated a chain at about 800 players). Use lazy accrual, but make the client prediction harvest-aware.
11. **Resource sprawl at launch.** Start with about 9 resources and 1–2 production modes. Eternum shipped 22 and spent a year cutting back.
12. **Always-online pressure.** Hour-long days and constant raids advantage bots and one time zone. Use daily action budgets or stamina that caps out, protected offline windows or shields, and first-party automation that runs server-side, not in a browser tab. Otherwise human-agent parity is fiction.
13. **Mutable economic escape hatches.** No mint-on-short bridge, no `todo` fee recipients, no silent zero defaults. Their own rule is "no silent defaults" after Eternum's `pointsForWin = 0` and MMR token at `0x0`.

### Improve this (Eternum lacks these and you need them)
14. **Agent parity with real sybil cost.**
    - Eternum gives one seat per identity, but that identity is only a wallet signature.
    - For open agent entry, make the **entry fee (or stake) the sybil cost**, and apply identical rate limits per seat. Expose the same actions per tick and the same information to everyone: fog is onchain truth, so no "agent vision" advantage.
    - Consider publicly **labelling seats as agent or human** for spectators without changing the rules. Eternum does not label seats.
15. **Several separate victory tracks with independent pools.**
    - Eternum effectively had one decisive track (Hyperstructure VP) plus side leaderboards bolted on (achievements, Faith, agent bounties).
    - Define war, territory, army, science and peace each with its own onchain score and its own slice of the fee pool, and **one game-ending trigger** that everyone knows in advance.
    - Keep "banked, cannot be lost" scoring on some tracks and "held at the end" on others, so late comebacks stay possible.
16. **Real diplomacy primitives.** Onchain alliances and treaties (non-aggression pacts with bonds that are slashed when broken, shared vision, VP-split contracts) instead of Eternum's social-only tribes. This matters especially when agents negotiate: make agreements enforceable, not chat-based. Eternum's chat-persuasion bounties are an injection-farming risk.
17. **Late joiners and the persistent world.** Eternum's only immunity was the first 24 hours of the season. Give each new civilization personal immunity and spawn it away from the frontier. Blitz's identical starting triangle is another answer.

---

## Sources

**Local repo (primary):**
- `README.md`, `AGENTS.md`, `CONTEXT.md`
- `apps/game-docs/docs/pages/{overview,eternum,blitz,changelog}/*.mdx`
- `config/source/common/base-config.ts`, `config/source/eternum/{points,hyperstructures,economy,troop,exploration,levels,resources,chains}.ts`, `config/source/blitz/{base,points,official-60}.ts`
- `contracts/l3/game/src/models/{troop,agent,guild}.cairo`, `systems/{season,faith,prize_distribution,guild,artificer,bitcoin_mine}/`
- `docs/agora.md`
- `docs/plans/{hired-agents-architecture-brief,hired-agents-milestones,realms-phase-1-brief,realms-value-plane-design,negative-food-balance-codex-brief,boot-to-playable-codex-brief}.md`
- `deploy/madara-lab/README.md`

**Web:**
- [Bankless – Eternum Season 1](https://www.bankless.com/read/eternum-season-1-starknet)
- [Bankless – Eternum Returns](https://www.bankless.com/read/eternum-returns-to-starknet)
- [Bankless – Eternum 101 (S0)](https://www.bankless.com/read/play-eternum-starknets-mmo)
- [Bankless – Eliza Daydreams](https://www.bankless.com/read/eliza-daydreams-onchain-gaming)
- [Sovereign Frontier – Play Eternum (S0 blockspace claim)](https://sovereignfrontier.substack.com/p/play-eternum-to-experience-fully)
- [@RealmsEternum – S0 champions post](https://x.com/RealmsEternum/status/1872493966916235630) (not retrievable; its content came from search snippets)
- [Odin – S0 onboarding thread](https://x.com/odin_free/status/1865855014524289094)
- [ChainPlay – S1 launch](https://chainplay.gg/blog/season-one-of-eternum-launches-april-30-with-major-updates/)
- [Medium – "Evolution of Eternum"](https://medium.com/@Oddsanjay/the-evolution-of-eternum-or-how-i-learned-to-stop-worrying-and-love-onchain-chaos-2e9f29fd06cd) (unverified, low reliability)
- [GitHub PR #4974 – daily Blitz rotation](https://github.com/BibliothecaDAO/eternum/pull/4974)

**Gaps I could not fill:** Season 1 final results (winner, how and when it ended), actual Season 1 player counts, how much of the 250k-LORDS agent pool was claimed, Blitz daily player numbers beyond one 84-player lobby, and any verified data on guild concentration or bots.
