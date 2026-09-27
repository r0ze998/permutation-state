# PERMUTATION STATE: the current game, mapped for an open-world redesign

Research unit "current-game", 2026-09-27. Repo read-only at HEAD `96a3464` (branch `codex/magicblock-playable`). No code was changed, no server was started, no transaction was sent.

Scope: the owner asked for an Eternum-like game that any number of people can join, possibly on an expandable map (「ハッカソンの締切は考慮せず、ゲームの設計を優先して、eternumの様な感じで何人でも参加できる様にして欲しい。…マップも拡張可能にした方がいいかもしれない」). This report does **not** propose the new architecture. It maps what exists so the architecture can be chosen with full knowledge of:

1. the fun core and identity the docs promise;
2. every mechanic, and whether it truly needs a global simultaneous tick or the whole world, or can be made local, per-entity or lazy;
3. which rules-engine modules survive as pure functions, and what each needs;
4. the 85-finding audit turned into a requirement checklist for any architecture;
5. which conclusions of the scale study still hold.

Labels: **[code]** read in the repo at HEAD; **[doc]** stated in a repo document; **[audit]** from `scratchpad/fix/…`; **[scale]** from `scratchpad/scale/…`; **[web]** public source, URL given; **[judgement]** this report's own reasoning.

---

## 要約（日本語）

- **ゲームの核**：シーズン前に勢力（国）を選び、同じ参加費を払う。勢力の中は選挙で役職者（将軍・内政官・科学官・外交官）を選び、国民は献策・支持・リコールで関わる。役職者は命令を封印して出し、全勢力の命令が同時に決まった順番で解決される。4つの道（覇権・繁栄・科学・協調）と時代で勢力の達成を測り、賞金は達成で勢力へ、功績で個人へ。運営の AI が正体を隠して混ざり、終了後に公開される。全部をチェーンのログから誰でも再計算できる。「ひとつの文明、6つの勢力」という言い方は、HEAD の最後のコミットで画面の文言として入ったもの（中身は6つの国のまま）。
- **全体ティックが本当に必要なもの**は少ない。必要なのは「同じ瞬間に同じ相手とぶつかる決定」（戦闘・同じマスへの移動・一括競売）の同時性と封印だけで、それは**その場に関わる相手の間**で閉じられる。生産・研究・成長・忠誠・防御の回復・配達・契約の判定・節目は、エンティティごとの遅延評価（Eternum の `stored + rate × elapsed`）にできる。ただし今のルールには、都市の計算が勢力全体の値（都市数・厭戦・赤字）に依存する結合があり、遅延評価にするにはこれを切るか、期ごとの固定値にする必要がある。危機・暗黒時代・支払いの勢力間分配だけは本当に全体の比較が要るが、年に数回の「エポック集計」で足りる。
- **再利用できるもの**：戦闘の式（`combat::damage`・`resolve_engagement`）、経済の式、一括約定（`clear_amm`・`call_auction`）、節目と時代（`scoring::milestone`・`tiers`・`era`・`score_of`）、乱数・ハッシュ・封印・Merkle、支払いの式（勢力内の分け方は既に閉じた式）、単位・建物・技術の表。`&WorldState` を丸ごと受け取るフェーズ関数（移動・戦闘フェーズ・外交・社会・生産・統治）は、中の計算は使えるが入出力を作り直す必要がある。地図は「半径 R の六角形全体」を前提にした添字計算で、拡張可能な地図には座標系から作り直しが要る。
- **監査の教訓（設計の必須条件）**：誰でも無料でシーズンを止められる道を作らない／1命令の仕事量を敵対的な入力でも上限つきにする／誰でも無料で書き込みロックできる共有アカウントを重要経路に置かない／運営が乱数も結果も選べない／金庫の支払能力と保存則を毎回チェーンで確かめる／隠す情報は目隠し付きの約束にする／止まったら必ず返金できる出口がある／検証は失敗側に倒す。37項目のチェックリストにした（§6）。
- **スケール調査で今も有効な結論**：ER の書き込みロック妨害は無料（1取引で最大32〜64アカウント）、専用か入口で選別する ER（F13）が大規模運用の前提。国民はベース層で書き、ER には固定サイズの集計だけを渡す。1人あたりの家賃は約 0.0023 SOL（320 B の Member）。直接民主制＋人数に比例する功績プールは、人数が増えると乗っ取りが得になる（役職者の取り分の上限が要る）。AI は「班4つに1体」程度に増やす必要があり、隠す一覧は最大64では足りない。

---

## 0. Sources read

| Source | What was used |
|---|---|
| `PERMUTATION_STATE_GAME_DESIGN_V5.md` (761 lines, all) | D1–D25; §2 one-line pitch; §3 season flow; §4–§5 nations and governance; §6 paths/eras; §7 prize, merit, USDC market; §8 caretaker; §9 cheating table; §11 chain; §16 implementation; §17 v6 revision (sealed orders, symmetric map, crisis, dark age, history); §18 v7 hidden AI, bounties, contracts, talk; §18.13–18.14 |
| `PITCH.md`, `README.md` (What it is, Honest limits), `SUBMISSION.md` (skimmed) | identity and promises |
| `PLAY_GUIDE.ja.md` | **historical**: it describes the earlier `/civilization/` prototype (one shared civilization, embodied citizens, logistics). Its banner says so. Useful as the owner's earlier Eternum-like fantasy. |
| `PERMUTATION_STATE_GAME_DESIGN_V4.md` §0, §4.1 (grep) | "one persistent pixel hex world", "world persists, power resets", Season preset 64–256 civs in regions of 16 |
| `PERMUTATION_STATE_GAME_CONSTITUTION.md` §5 (grep) | one civilization divided into Fronts and Worksites |
| `research/BENCHMARK_ETERNUM.md` | Eternum's lazy accrual, time model, arrival gates, ~800-player Starknet saturation, lessons |
| `permutation-chain/DESIGN.md` | accounts, lifecycle, compute table, trust model |
| `permutation-rules/src/**` | module inventory, signatures, phase table (`tick/mod.rs`), state layout (`state.rs`), couplings (`economy.rs`, `tick/society.rs`, `tick/production.rs`), mapgen |
| `scratchpad/fix/findings/WP01…WP18` (titles and severities of every finding; bodies of the critical ones) | §6 checklist |
| `scratchpad/fix/contract.md` (§0, §1.1–1.2, §1.7–1.8, §4, §6), `contract-amendments.md`, `audit-map.md` (head) | §6 checklist, v9 decisions |
| `scratchpad/scale/scale-design.md` (§0–§3, §7, §8, §10–§12, §15–§16) | §7 |
| `scratchpad/scale/understand/{game-intent, rules-members, chain-members, platform}.md` | §2, §5, §7 |
| Web: [Realms docs, key concepts](https://docs.realms.world/eternum/key-concepts), [Realms docs, Blitz world physics](https://docs.realms.world/blitz/world-physics), [BibliothecaDAO/eternum](https://github.com/BibliothecaDAO/eternum) | Eternum reference points (8,000 realms, up to 48,000 villages in Season 1, stamina per phase, exploration reveals hexes, 60-minute Blitz of 10 six-minute days) |

---

## 1. What the game is today (short)

- **Shape.** One season = one world on a radius-13 hexagon (547 tiles) with 6 nations, 6 city-states, one central trade hub, ~180 ticks of 30 s (Blitz; the 4-hour "Season" preset exists in code but is not creatable, contract O25) **[code: `params.rs:240-250`; doc]**.
- **Membership.** Wallets join a nation before the season (entry fee in USDC, 80 % pool / 20 % operations). No per-nation cap in the design; 256 per season at HEAD (payout table in the Season account), 48 in the v9 round **[doc; audit C1]**.
- **Who acts on the map.** Only four elected officers per nation (general, steward, science, diplomat), each within its order families and a per-office budget (nation budget `B = min(3 + cities, 8)` per tick, split by office, bankable 4 ticks). Everyone else proposes, supports, votes, recalls **[doc V5 §5; code `gov/`]**.
- **Tick.** Officers commit `sha256(orders ‖ salt)`; after the deadline anyone closes commits; a reveal window; the input is frozen and published in 6,000-byte log chunks (5,000 in v9); `ResolveTick` runs 12 fixed phases over the **whole world** in 1–7 transactions **[code `tick/mod.rs`; doc DESIGN.md]**.
- **Scoring.** 4 paths × 5 tiers of absolute milestones; eras when 2 (tier 5: 3) paths reach a tier; points 10/20/30/40/55 per milestone plus era bonus; pool split between counted nations by points, within a nation 20 % equal to active members (capped at ½ fee) and 80 % by path-weighted merit **[doc V5 §6–§7, §18.13]**.
- **Hidden operator AI.** Committed roster, hidden during play, revealed after; home city drawn at T45; bounty to the first nation conquering it; AI payouts redistributed to people **[doc V5 §18]**.
- **Verification.** The replay verifier rebuilds every root, seal, salt-derived randomness and payout from `PS_GENESIS`, `PS_SEAT`, `PS_OPEN`, `PS_COMMITS`, `PS_SALTS`, `PS_INPUT`, `PS_TICK`, `PS_HISTORY` logs **[doc DESIGN.md]**.

### 1.1 Why it stops at about 48 members (the numbers any redesign must beat)

| Fact | Value | Source |
|---|---|---|
| World body | one borsh `WorldState` over 20 × 4 KiB chunk PDAs, ≤ 81,844 B; practical heap ceiling ~60–66 KB | [code `state.rs`; audit WP07] |
| Fixed cost of a ResolveTick part | decode ~130k CU, encode ~90k, chunk PDA checks ~64k, input read ~35–47k, reopen nations ~27–40k; 0.4–0.45M CU of every part is carriage, not rules | [doc DESIGN.md Compute] |
| Rules cost per tick | production 150–200k, society ~55k, intake 55–80k, standing and movement 40–60k each; phases 1–10 do **not** grow with members | [doc; scale rules-members §2] |
| Per member | 61 B body, 64 B heap, **≈ 295 CU per member per part ≈ 3.5k CU per member per tick**, even for members who never act | [scale rules-members §0, C1–C3] |
| Governance input | all actions go through the tick input; v9 budget 96 slots per tick world-wide, quota ≥ 2 ⇒ ≤ 48 members | [audit WP03; contract O9] |
| Elections | O(standing × votes) per nation at HEAD; > 1.2M CU at 256 honest members | [audit WP06] |
| Settlement | `settle_with` O(civs × M), fails at 1,000 members; payout table 8 B per member in a 4 KiB Season account | [scale rules-members C15; chain-members §0] |
| Peak tick | 1.35M of 1.4M CU with 13–15 members on devnet (before optimisations); 1.04–1.23M after | [doc DESIGN.md] |

**The rules' game logic is already independent of member count; the architecture is not.** What scales with people is data carriage (every member decoded every part), elections, recall electorate counting, the tick-input governance budget, seating and settlement. What scales with *world size* (tiles, cities, units) is the whole-world decode and every phase that iterates all units/cities, which is exactly what an expandable map would grow. **[scale; judgement]**

---

## 2. The fun core and identity, as the documents describe it

### 2.1 Pillars (what a redesign must keep, or explicitly replace)

The scale study's game-intent unit listed ten member promises I1–I10 (`scale/understand/game-intent.md` §1). Regrouped as design pillars, with where each comes from:

| # | Pillar | Evidence | Open-world tension |
|---|---|---|---|
| P1 | **Crowd politics: a faction is a small on-chain state governed by its members.** Elections every 30 ticks, anyone stands, proposals with adoption credit (50/50 merit), recall by majority, two-officer consent for war and large spending. PITCH's "Problem" is literally "multiplayer strategy games have no politics … there is no way for a crowd, people and agents together, to govern one." | V5 D2, D6, §5; PITCH | Direct democracy over 4 offices works only for small crowds (§7.4). At thousands it needs tiers (delegation, councils, local governors). |
| P2 | **Simultaneity and fairness of moves.** Sealed commit–reveal orders, fixed phase order, identical budgets, randomness nobody picks, symmetric maps nobody can compute before choosing a side. | V4 §4.2, §5.2; V5 §17.1–§17.2 | Global simultaneity is the tick's reason to exist. Must become local (§3.1). Symmetric maps conflict with an expandable map. |
| P3 | **A deep 4X on chain**: cities, units, combat, diplomacy with bonds, city-states and envoys, markets, standing rules, tech tree of 16 techs, Star Gate. | PITCH "What we built"; spec v0.2 | Most of it is local (§3). The cost is whole-world iteration, not the rules. |
| P4 | **Four paths and eras**: specialise or diversify; conquest can take milestones back; eras announced to the world. | V5 §6 | Absolute milestones per faction are fine; "held at end" milestones need a season end; relative mechanics (crisis, dark age) need global comparisons. |
| P5 | **Prize by achievement, then by contribution.** Pool to factions by points, inside by merit per path; voting gives no merit (anti-farming); equal share capped at ½ fee (anti-Sybil). | V5 D4–D5, D9, §7 | At scale the merit pot concentrates on officers (≈100× fee at 10k); capture becomes profitable (§7.4). |
| P6 | **Agents are citizens on equal terms** (same view, same budget, same rights), joining over x402/MCP/`llms.txt`, with sealed decision rationale. | V4 §5; V5 D14–D17, §13 | Unchanged in principle; per-player actions at scale make agent throughput a fairness question (rate limits per seat, not per IP). |
| P7 | **"Is there one among us?"** Hidden operator AI members, committed roster, bounties on their home cities, their prize goes to people. | V5 §18 | 12 AIs among thousands are noise; scaling them up makes the operator a voting bloc (V5 §18.9 already asks for a per-nation AI cap). |
| P8 | **Nobody, the operator included, can steer the result; anyone can replay it.** | V5 §5.7, §11, §17; DESIGN.md trust model | Replay of an open world must work from per-entity event logs, with no gaps, and must fail closed (§6.L). |
| P9 | **Legible causality**: "my proposal was adopted, a city fell, I got merit" traceable to orders. | V5 §2, §12 自分の功績 | An open world must give every ordinary player a direct action that moves the map (the scale study's "mobilisation"; the older `/civilization/` prototype's embodied citizen). |
| P10 | **History, not power, carries over**: symmetric fresh map each season; record of cities, founders, captors and ruins chained by hash. | V5 §17.5; V4 "world persists, power resets" | An expandable, persistent Eternum-like map pushes toward carrying *terrain* over; power reset must still hold for fairness to new entrants. |

**Identity wording.** HEAD's last commit (`96a3464`, "Wording: one civilization, six factions (display text only)") renamed nation → faction (国 → 勢力) in the UI only: IDs, APIs and stored data still say nation/civ **[code: git show]**. The Constitution (`PERMUTATION_STATE_GAME_CONSTITUTION.md` §5) had the stronger version: "The whole player population remains one civilization, but its work is divided into Fronts and Worksites… not separate teams." The owner's "one civilization, six factions" therefore sits between the Constitution's single civilization and V5's six rival nations. **[doc; judgement]**

**The two fantasies in the repo.** The current game is a *governance* fantasy: you are a citizen of a nation and you move the map only through officers. The historical `/civilization/` prototype (PLAY_GUIDE.ja) was an *embodied* fantasy: you walk your own citizen, gather, build roads, explore unknown tiles. Eternum is embodied (you own Realms, armies, villages). An "any number of players" Eternum-like design very likely needs an embodied layer (each player owns and acts on entities) under the governance layer, otherwise P9 fails at scale (at 10k members a member gets 0.86 order slots per season: `game-intent.md` §2.2). **[judgement]**

### 2.2 Promises that are specifically fragile at scale

- "One vote costs one entry fee, so capture does not pay" (V5 §9 row 1) holds at 8 per nation and **inverts** at 167+ per nation with a merit pot proportional to members (§7.4) **[scale game-intent §3.1]**.
- "Per-person dilution is the natural brake on herding" (V5 §4) holds on average, but officer seats grow in value with nation size, so power-seekers herd into big factions **[scale game-intent §3.2]**.
- "Recall by a majority of recently active members within 5 ticks" becomes arithmetically impossible under any per-tick governance budget **[scale game-intent §4]**.
- "Perfect information" (v6) was chosen *because* the chain is public; an Eternum-like explored map with fog needs private state (MagicBlock PER/TEE) or accepts that fog is cosmetic **[doc V5 §17.1; README Honest limits]**.

### 2.3 Eternum reference points that matter for this map

- Eternum accrues resources **lazily** (`stored + rate × elapsed`, harvested when touched; no global tick transactions), with an army tick of 60 s, delivery tick 180 s, phases of 600 s; Blitz is 60 minutes = 10 days × 6 phases **[doc `research/BENCHMARK_ETERNUM.md` §5; web [Blitz world physics](https://docs.realms.world/blitz/world-physics)]**.
- It has 8,000 Realms and up to 6 villages per Realm (up to 48,000 villages in Season 1), stamina (+2 per phase, +12 per day; 30 per explored hex), exploration that reveals hexes permanently **[web [Realms key concepts](https://docs.realms.world/eternum/key-concepts)]**.
- Its Season 0 (~800 players) used up to ~50 % of Starknet blockspace; chain limits leaked into rules (hourly arrival gates, manual claiming); it retreated to 9 resources and to 60-minute Blitz lobbies of up to 24 (target 96) players **[doc BENCHMARK_ETERNUM §5, §7]**.
- Lesson recorded in the repo: "Size your throughput first… Use lazy accrual, but make the client prediction harvest-aware" and "always-online pressure… advantages bots" **[doc BENCHMARK_ETERNUM §8 items 10, 12]**.

---

## 3. Every mechanic: does it need a global tick or the whole world?

Three things are bundled into today's "global tick", and they must be separated **[judgement]**:

1. **A shared clock** (tick numbers, deadlines, T120 market freeze, T162 transfer freeze, T45 home snapshot, T90 dark age, crisis from T120, season end). A clock is cheap: any instruction can compare `now` with season parameters. It does **not** require one transaction to touch the world.
2. **Simultaneity of interacting decisions** (sealed orders, pre-combat counts, contested tiles, batch clearing). This is needed only among the parties that interact in that instant.
3. **One transaction resolving the whole world** (decode/encode everything, iterate all units/cities/members). This is what caps the game, and nothing in the rules requires it except a few global comparisons.

Legend for the "Needs" column: **G** = inherently global (needs all factions' data at one point in time); **L** = local (a tile neighbourhood, one engagement, one city-state, one pair); **E** = per-entity (one city, one unit, one player, one faction); **Lazy** = can be evaluated on touch as a function of elapsed time. "Coupling" names what stops naive lazy evaluation.

### 3.1 Time, orders and randomness

| Mechanic | Where | Today | Needs | Local/lazy alternative | Coupling and risks |
|---|---|---|---|---|---|
| Sealed commit–reveal orders per office per tick | `orders::order_commitment`, chain `CommitOrders`/`CloseCommits`/`RevealOrders` | one global deadline for all nations; everything frozen together | **L** (only among parties that can affect each other before resolution) | (a) timed actions: an action starts now and lands at `t + travel/cast time`, so reacting is itself a timed action (Eternum style; reduces the value of seeing others' moves); (b) sealed only for contested events: a battle or contested tile opens a local commit–reveal round among its participants; (c) regional micro-ticks per map chunk | FCFS on the ER makes non-sealed simultaneous actions a latency race; bots win races (P6). Last-revealer withholding gives a 1-bit randomness option (WP11 `free-withholding-option`), which goes away with VRF. |
| Fixed phase order (0 intake … 11 commit) | `tick::run_phase` | 12 phases over the world | **L** per event type | Each entity's update applies the same internal order (economy before movement before combat before production) when it is settled; cross-entity events ordered by timestamp, ties by VRF | Phase order currently gives "all moves, then all combat from pre-combat counts". An event model must pick a rule for same-timestamp interactions (batch them). |
| Tick randomness | `rng::tick_vrf(world root, salts)` → VRF in v9 (Plan A) | one seed per world per tick | **E** per event | MagicBlock VRF per battle/draw, or one VRF per region-epoch with domain separation (`rng::rand(seed, domain, id)` is already domain-separated) | Operator must never know or choose seeds (WP11). A shared per-tick VRF queue can be filled for free (amendment A21). |
| Order budget `B = min(3 + cities, 8)` split by office, bank of 4 | `economy::order_budget`, `orders::spendable`, `next_bank` | per nation per tick | **E** per faction | per-actor action points / stamina regenerating with time (lazy, Eternum stamina) | Per-faction budgets shared by thousands are a hot account; per-player stamina is naturally sharded. |
| Standing rules (AutoDefend, Retreat, Patrol, AutoPurchase) | `standing.rs`, phase 3 | every rule evaluated every tick for every unit/city; v9 bounds ≤ 48 running rules | **E** | trigger-based: a rule fires when its entity is touched or when a keeper calls `run_rule(entity)` with bounded work | WP05: one Patrol made phase 3 exceed 1.4M CU. Any per-tick "for all automation" loop grows with world size. |
| Clock-gated rules (T120 market freeze, T162 transfer freeze, stalemate multiplier `economy::stalemate_multiplier(rules, tick)`) | params, checks | global clock | **clock only** | keep: compare `now` to parameters | none |

### 3.2 Map, movement, vision

| Mechanic | Where | Today | Needs | Alternative | Coupling and risks |
|---|---|---|---|---|---|
| Map storage and indexing | `map::Map { radius, tiles }`, `index_of` arithmetic over a full hexagon | whole map in the world body | **L** | tiles in chunk accounts (region × region), addressed by chunk id; only touched chunks loaded | `Map` assumes one hexagon of radius R; `hex` uses `i32` and attacker coordinates overflow (WP08 `hex-coord-i32-overflow`). Expansion changes indexing everywhere. |
| Symmetric map generation | `mapgen::step_symmetric`, `generate_symmetric`, `rotate_world` | 6 rotated copies of one sextant, rank-assigned terrain per sextant, ridges with passes on seams, central hub | **G** for the sextant (rank assignment and connectivity are over the whole sextant) | per-chunk generation: `field(base, salt, h)` noise is already a pure function of the hex; rank assignment and connectivity can be done per chunk; symmetry can be kept per **ring of chunks** (a new ring = 6 rotated chunk copies) | Fairness today is "every start has identical surroundings". An expandable map needs a new fairness statement: e.g. every spawn *cohort* gets rotated copies of the same chunk; late entrants spawn on a fresh symmetric ring. |
| Territory claim | `Map::claim_territory`, `MAX_TERRITORY_RADIUS = 3` | on city founding | **L** (radius 3) | unchanged, per chunk neighbourhood | cross-chunk claims need both chunks writable. |
| Movement with occupancy sub-steps | `movement::phase_movement` | all units move one sub-step at a time, contested tiles by `tie_key(tick_seed, unit)` | **L** | travel with arrival time (Eternum armies, stamina), occupancy checked at arrival; or micro-ticks per region | Units listed per world (`Vec<Unit>`); `tile_free_for` scans all units. Arrival order races on FCFS unless arrivals are batched per slot/epoch. |
| Path planning (planner, Patrol) | `movement::search`, `reachable`, `path_to` | A* over the map | **L** | client-side planning, chain verifies a submitted path step by step (O(path)) | WP05: unbounded A* on chain is a freeze vector. |
| Protection zones, borders (`checks::enter`) | `battle::plan::is_protected`, `checks::enter` | per tile | **L** | unchanged | reads relations of the pair (faction level). |
| Vision/fog | `vision.rs` (`visible`, `belief`, `sees`, `between`) | v6: off; perfect information; `belief` used only for display and `decision::obs_leaves` | **E** per viewer | if fog returns, only on a private ER (PER/TEE); or Eternum-style explored/unexplored as *public* state | The chain is public; client fog advantages scripts over humans (V5 §17.1). |
| Exploration | not in V5 (PLAY_GUIDE's prototype had it) | – | **L** | explore(hex) reveals and rolls a reward (VRF), permanent and public | new mechanic; pairs naturally with an expandable map. |

### 3.3 Combat and conquest

| Mechanic | Where | Today | Needs | Alternative | Coupling and risks |
|---|---|---|---|---|---|
| Damage and engagements | `combat::damage`, `variance`, `resolve_engagement` (pure); `battle::damage` builds all engagements from **pre-combat counts** and applies them at once | phase 5 over all attacks | **L** per engagement cluster | per-battle instruction: attacker + defender (+ tile) settled lazily to `now`, resolved with VRF variance | "Pre-combat counts across all engagements" means an army attacked twice in one tick deals damage computed before either hit. Per-battle resolution changes this; batching attacks on the same target in the same epoch preserves it. |
| Captures, capital handover, civilians | `battle::capture` | phase 5 | **E** (city, its tile) | per city | capital handover touches the faction record. |
| Razing | `battle::raze` | multi-tick `razing: Option<u8>` | **E Lazy** | timestamps | – |
| City retaliation, walls | `combat::Situation` | per engagement | **L** | unchanged | – |
| Grievance, casus belli | `state.grievance`, `grievance_fresh` (per pair) | per pair per tick decay | **E** per pair, **Lazy** decay | lazy decay by elapsed time | pairs are faction-level: 15 pairs for 6 factions. Per-player pairs would explode. |
| Bounties on hidden-AI home cities | `roster::home_city`, `bounties`, `City.first_conquest` | T45 snapshot of each AI nation's cities (`home_snapshot`) | **E** per AI | draw at an event with VRF; record first conquest on the city | The snapshot is a per-nation list at one instant (bounded by cities). |

### 3.4 Economy, cities, research, society

| Mechanic | Where | Today | Needs | Alternative | Coupling and risks |
|---|---|---|---|---|---|
| City yields (tiles worked, focus) | `economy::city_yield(map, city, is_capital, has_philosophy)` | phase 6, every city every tick | **E Lazy** | settle city to `now` | reads the tile window (radius 3) and 2 faction flags. |
| Growth, food, production queue, one item per tick, Star Gate spacing | `tick::production::city_turn`, `try_complete`, `spawn` | phase 6 | **E Lazy by stepping** (nonlinear: thresholds, pop changes yields) | bounded catch-up loop per touch (k ticks per instruction, checkpointed), or piecewise-linear redesign (Eternum-like rates) | **Coupled to faction-global values**: `amenities(city, civ_cities, war_weariness)` (`economy.rs:89-99`) and `apply_amenities`; `civ.deficit` (−2 amenities); suzerain bonuses (`production::suzerain_bonuses`). A lazy city must see these as piecewise-constant between settlements, otherwise touching one city means settling every city of the faction first. Remove the coupling or sample it per epoch. |
| Upkeep (gold for units, cities) and deficit | `tick::society::phase_upkeep`, `economy::unit_upkeep`, `city_upkeep(cities)` | civ-level per tick | **E Lazy** per owner | rates per owner, settled on touch; deficit as a lazily computed state | A faction treasury written by every city/unit settlement is a hot account (§7.1). Per-player treasuries avoid it. |
| Research | `tick::production::research`, `Civ.science_store`, `economy::civ_tech_cost` (depends on city count), eurekas (`milestones::eurekas`) | civ-level per tick | **E Lazy** per faction (or per player) | science accrues lazily; tech completes when store ≥ cost at settlement | cost depends on current city count (piecewise); eurekas depend on events (event-driven). |
| War weariness | `phase_society` | per civ, from war pairs and troops lost | **E Lazy** per faction | lazy with elapsed ticks and event increments | feeds every city's amenities (above). |
| Loyalty, Free City | `phase_society` | per city: capital distance, garrison, **pressure from foreign cities within `loyalty_pressure_radius` = 6** | **L** (radius 6) | settle on touch using neighbours' current pop; or epoch-sampled pressure | neighbour pop changes continuously → exact lazy evaluation needs neighbours settled in time order. A defensible simplification: pressure sampled at epoch boundaries. |
| City defence regeneration | `phase_society` | per city per tick unless attacked | **E Lazy** (linear) | `min(max, def + regen × elapsed)` | – |
| Heritage, ruins | `City.heritage_*`, `Tile.ruin_peak_pop` | from history | **E** | unchanged | persistent map would make ruins meaningful across seasons. |
| Deliveries (market goods after 3 ticks) | `markets::deliver`, `state.deliveries` | per tick scan | **E Lazy** (timestamped) | claim on touch (Eternum arrivals) | Eternum's "arrival gates" were a chain-limit leak; avoid by lazy delivery. |
| Transfers between nations | `trade::apply_transfers` | phase 2 | **E** per pair | per-transfer instruction | per-tick caps (u32 wrap bug, WP08) → per-epoch caps in u64. |

### 3.5 Neutral actors, diplomacy, markets

| Mechanic | Where | Today | Needs | Alternative | Coupling and risks |
|---|---|---|---|---|---|
| City-states: growth, suzerainty by influence | `envoys::apply_envoys`, `phase_neutral`, `CityState.influence: Vec<Milli>` per civ, `envoys: Vec<EnvoyShare>` | 6 city-states, per tick | **L** per city-state | per city-state account; suzerainty recomputed when influence arrives (event-driven); ties by VRF | `envoys` grows with distinct (civ, credit) pairs (scale rules-members D13; amendment A15 caps at 8). A city-state is a natural contention point (everyone sends envoys). |
| Diplomacy: war (needs consent), peace, NAP with bonds, alliances, scheduled transitions | `diplomacy.rs` phase 1; `state.relations`, `proposals`, `truce_until` | pairwise among ≤ 6 civs | **E** per pair | per-pair account (15 for 6 factions), transitions lazily by timestamp | Keep relations at the faction level. If players own entities, player-to-player hostility must derive from faction relations (plus maybe guild-level), never per-player pairs. |
| Treaty contracts (escrowed USDC, conditions on the world) | `contracts.rs`: offer/accept/cancel, `settle_contracts` checks conditions each tick | per tick over ≤ 4 per nation | **E** per contract | permissionless `settle_contract(id)` that reads exactly the accounts its condition names (pair relation, city owner) | Money path: must keep WP08/WP10/WP12 bounds (escrow counted, no tariff pump, per-term caps). |
| Gold AMM at the hub | `markets::clear_amm` (pure), `apply_amm` | one uniform-price batch per pool per tick | **L** per pool | per-pool batch auction per epoch (orders escrowed into per-trader records, cleared by a permissionless call reading an aggregate) | One pool is a hot account if every trade writes it. WP08: checks per order instead of per batch minted gold. |
| USDC exchange (call auction, rising tariff, 3-tick delivery) | `markets::call_auction` (pure), `apply_exchange`, `spend_limit`, `treasury_room` | one auction per good per tick among treasuries | **L** per good (**G** among all traders of that good) | per-good epoch auction, order records sharded per trader, clearing reads aggregates or runs as bounded multi-step | Global per good is fine if the order set is bounded per epoch (it is: treasuries of 6 factions). With per-player trading, it becomes a big book → sharding and bounded clearing. WP08 arithmetic lessons apply. |
| Trade accounting (`trade_value`, 40 % per-partner cap on trade volume) | `trade.rs`, `scoring::trade_effective` | per civ vector | **E** per faction | unchanged | – |

### 3.6 Governance, scoring, payout, AI, talk, history

| Mechanic | Where | Today | Needs | Alternative | Coupling and risks |
|---|---|---|---|---|---|
| Elections (plurality per office, term 30 ticks, runner-up, pre-season votes, tie by hash) | `gov::terms::run_election`, `first_election`, `apply_pre_season` | phase 11, over all members | **E** per faction | per-candidate tally accounts updated by each vote (O(1)); bounded candidate set; winner determination by a permissionless scan over candidates, split across transactions | WP06: O(standing × votes) froze the season. Scale study: capture at low turnout (§7.4). |
| Proposals, support, adoption (auto-match), caretaker | `gov::actions`, `tick::intake::auto_match`, `gov::caretaker::batch` | ≤ 24 (v9: 8) open per nation | **E** per faction | bounded proposal board per faction or per guild/cell; counts, not lists | 8 slots per nation for thousands is a cheap DoS (WP07/O13 "4-colluder slot DoS"); needs deposits or eviction by support. |
| Recall (majority of active in last 10 ticks, idle recall) | `gov::terms` (`electorate` scans all members every tick) | per tick O(civs × M) | **E** per faction | counters/buckets; recall only vacates (scale study) | amendment A16 already asks to stop scanning. |
| War consent, spend consent | `ConsentWar`, `ConsentSpend` orders | per tick | **E** | unchanged | WP10: caretaker adopted a ConsentSpend → bypass; treasury orders must not be proposable. |
| Activity windows (9 of 18) | `gov::mark_active`, `Member.windows` | in the world | **E** per player | per-player record on base | – |
| Merit crediting | `merit::credit`, `credit_shared` (13 call sites); `Credit {officer, proposer}` stored on cities, civs, envoy shares | O(1) per credit, but needs `members[id]` in memory | **E** | per-tick merit journal drained into player records; or merit = f(events) recomputed at claim | Merit sums commute; must be u64/checked (not saturating) or order-dependence appears (scale rules-members §5). |
| Milestones, tiers, eras | `scoring::facts_all` (one pass over all tiles and cities), `milestone`, `tiers`, `era`, `score_of` | phase 10 every tick | **E** per faction, from **incremental aggregates** | maintain per-faction counters (tiles owned, pop, wealth, techs, conquests, partners, suzerainties, trade) on each event; `milestone(rules, facts)` is already pure | `facts_all` iterates the whole map every tick: fine at 547 tiles, not for an expandable map. "Held at end" milestones need an end snapshot. |
| Crisis on the top 2 (from T120, every 6 ticks) | `tick::society::crisis` | global ranking of nation scores | **G** | periodic epoch job: rank 6 aggregates (cheap if aggregates exist) | fine at faction level (6 numbers). Meaningless per player. |
| Dark age for far-behind nations (at T90) | `milestones::dark_age` | compare to the leader | **G** | one-shot epoch job over 6 aggregates | same |
| Prize split among nations by points | `payout::settle_with` | once, whole world, all members | **G** over factions (6 numbers) + **E** per player | FinishSeason computes per-faction scalars; each `Claim` evaluates its own amount in O(1) (the within-nation formula is already per member: `payout.rs:236-279`) | Redistribution rules (§18.5 "still left by receipts", zero-merit equal-split fallback, dust) need O(M) sums today and a rules change (scale rules-members §0 item 4). |
| Hidden operator AI roster | `roster::roster_tag`, `roster_link`, `roster_chain`; chain `RevealRoster` | ≤ 64 AIs, revealed after the season | **E** | per-AI commitment with blinding (WP09: the unblinded chain let anyone identify AIs); reveal after end, operator-only, in order | scale: 64 is far too few for thousands (≈ 1 per 4 cells of 32 → ≈ 20 per 2,400). |
| Talk (anchored per tick) | chain `AnchorTalk`, gateway `/talk` | one Merkle root per tick for everyone | **L** | roots per channel (cell, faction, region) | global talk unreadable at 500+ (scale game-intent §4). |
| History layer | `history::season_record`, `history_root` | one record of every city at FinishSeason | **E**, chained | per-season Merkle root of per-entity records | A persistent expandable map changes the meaning: terrain and ruins would persist; power still resets. |

### 3.7 What is genuinely global (short list)

After separating clock, local simultaneity and whole-world transactions, only these need a view of all factions at one instant **[judgement from §3.1–§3.6]**:

1. The **prize split among factions** at season end (6 aggregates).
2. **Crisis** and **dark age** (rank or compare 6 aggregates at clock-defined instants).
3. **"Held at end" milestones** and the **history record** (a snapshot at the end; per-entity records can be summed incrementally).
4. The **map seed and spawn fairness** (one VRF seed; per-chunk generation).
5. The **season boundary** itself (all accounts reach "finished" before settlement; escape hatches must cover an entity that never settles).

Everything else is local to a tile neighbourhood, an engagement, a city-state, a faction pair, a faction, or one player, **provided** the faction-wide couplings in city economics (city count, war weariness, deficit, suzerain bonus) are removed or sampled per epoch.

---

## 4. Which mechanics depend on "faction owns everything"

This decides whether the redesign is "Eternum-like players inside factions" or "V5 factions with more members". Today every map entity belongs to a nation (`Owner::Civ`, `City.owner: Option<CivId>`), and players touch the map only through officers. **[code]**

| Entity/state | Owner today | If players own entities (Eternum-like) | If factions keep owning |
|---|---|---|---|
| Cities, queues, focus | nation (steward) | player-owned realms/cities; faction adds bonuses and law | stewards per city/region (district governors: scale game-intent §5.4) |
| Units | nation (general) | player armies with stamina | generals per front |
| Research | nation (science officer) | faction research fed by players' labour, or per-player tech | unchanged but lazy |
| Gold, iron, horses, influence | nation stocks | per player, taxed to faction | per faction (hot account) |
| Treasury (USDC) | nation vault shares | per player wallet + faction treasury | per faction, capped per term (R7) |
| Diplomacy | nation (diplomat) | faction-level only; players inherit | unchanged |
| Merit attribution (`Credit`) | officer/proposer of the order | the acting player directly (P9 restored) | unchanged |
| Order budget | per nation per tick | per player stamina/AP | per office |

Consequence **[judgement]**: the pure rule functions (§5) survive either way; the phase functions and the ownership model (`Owner::Civ(CivId)` with `CivId: u16` and nations as the only actors) must be redesigned under the Eternum-like option. The governance layer (P1) then governs *faction-level* decisions (war, treaties, faction law/taxes, shared projects like the Star Gate, treasury), which is also where V5's politics are most meaningful.

---

## 5. Rules-engine reuse inventory (`permutation-rules/src`)

Assets that carry over wholesale: `no_std`, allocation-only, **integer-only** arithmetic (`fixed.rs`: milli units, bps), borsh encodings, deterministic tie keys, domain-separated hashing, one crate shared by program, server, verifier and (via WASM) clients, golden tests that pin roots, the symmetry test, the invariants module, the season simulator used for balance. **[code `lib.rs`; doc]**

Classification:
- **A — pure, reuse as is**: takes explicit inputs, no `WorldState`.
- **B — reuse the logic, change the interface**: today takes `&WorldState`/`&mut WorldState` but only reads a local slice; refactor to take entity views.
- **C — redesign**: its structure assumes the whole world or the monolithic member table.

| Module | Class | What it is | Changes needed for an entity/lazy design |
|---|---|---|---|
| `fixed.rs` | A | milli/bps conventions | none |
| `hex.rs` | A (with fix) | axial coords, distance, neighbours, rotations, sextant | widen or bound coordinates (WP08 i32 overflow); add chunk ↔ hex mapping |
| `rng.rs` | A | `tick_seed`, `rand(seed, domain, id)`, `rand_id`, `tie_key`; `tick_vrf` (to be removed by v9 P1) | seeds become per event/region from VRF; keep domain separation |
| `hash.rs`, `decision.rs` | A | sha256 (syscall on SBF), decision digest, rationale hash, Merkle root/proof/verify, `obs_leaves` | reuse for commitments, talk roots, per-entity logs, claim proofs |
| `units.rs`, `buildings.rs`, `tech.rs` | A | static tables (8 unit types, buildings, 16 techs, prereqs) | none; balance may change |
| `combat.rs` | A | `damage`, `variance(rules, seed, id, side)`, `resolve_engagement(rules, attacker, defender, situation, v_a, v_d)` | none; the caller supplies VRF variance and pre-battle counts |
| `economy.rs` | A/B | `growth_threshold`, `unit_upkeep`, `city_upkeep`, `tech_cost`, `amenities`, `apply_amenities`, `order_budget`, `city_yield(map, city, …)`; `in_dark_age(state, civ)` | `city_yield` needs a tile window instead of `&Map`; `amenities`' faction inputs (city count, war weariness) are the coupling to remove or sample (§3.4) |
| `markets/amm.rs::clear_amm`, `markets/usdc.rs::call_auction` | A | uniform-price clearing of one pool/good | reuse for epoch batch auctions; apply WP08 bounds (price ≤ 100 USDC, amount ≤ 10,000, per-batch reservations, rounding up) |
| `markets` `apply_amm`, `apply_exchange`, `deliver`, `spend_limit`, `treasury_room` | B | per-tick application over nations | per pool/good epoch; deliveries lazy |
| `scoring.rs` | A (`milestone`, `tiers`, `era`, `ladder_points`, `score_of(rules, facts)`, `trade_effective`) / C (`facts_all`, which scans the whole map) | milestones and eras | keep the pure scorers; replace `facts_all` by incremental per-faction `Facts` maintained on events |
| `payout.rs` | A for the within-nation per-member arithmetic (`split_nation`, `merit_components` are closed forms over nation aggregates) / C for `settle_with`'s O(M) loops, redistribution and dust | settlement | closed-form claim; `outstanding` decremented per claim (WP12); rewrite §18.5 redistribution as closed form (scale rules-members §4, WP10 R4 row) |
| `merit.rs` | B | `credit`, `credit_shared` | append to a journal or credit the acting player's own record; u64/checked sums |
| `roster.rs` | A (`roster_tag`, `roster_link`) / C (`roster_chain` unblinded; `home_city` uses a T45 snapshot) | hidden AI | blinded commitment (WP09); home draw by VRF at an event; scale AI count |
| `history.rs` | B | season record and chained root | per-entity records → Merkle root; decide what persists on a persistent map |
| `checks.rs` | B | typed validity checks (`found_city`, `queue_item`, `research`, `declare_war`, `propose_*`, `enter`, `standing`) | mostly local reads; take views |
| `orders.rs` | B/C | `Order` enum (28 variants incl. MoveUnit, Attack, FoundCity, SetQueue, SetFocus, Purchase, SetResearch, diplomacy, SendEnvoy, Transfer, MarketTrade, ExchangeOrder, Raze, SetStanding, RevealRationale, ConsentWar/Spend, contracts); `order_commitment`, `check_structure`, `role_allows`, budgets | order *payloads* and structure checks reuse (with v9 caps: 24 orders, 8 free, 950 B); office batching and per-tick budgets are tick-model constructs |
| `battle/plan.rs` (`forecast_attack`, `is_protected`, `hostile`), `battle/damage.rs`, `battle/capture.rs`, `battle/raze.rs` | B | combat phase | per-battle instruction over the participants' accounts; keep "pre-combat counts" semantics within a batch |
| `movement.rs` | B/C | occupancy, sub-steps, A* planner | planner moves off chain (verify submitted paths); movement becomes travel-with-arrival or regional micro-ticks; occupancy must not scan all units (`tile_free_for` today scans `state.units`) |
| `standing.rs` | C | per-tick automation for all units | trigger-based or keeper-called with bounded work (WP05) |
| `diplomacy.rs`, `envoys.rs`, `trade.rs`, `contracts.rs` | B | per pair / per city-state / per contract | per-pair and per-city-state accounts; lazy scheduled transitions; `settle_contract(id)` permissionless |
| `tick/*` (`intake`, `city_orders`, `production`, `society`, `milestones`, `mod`) | C (orchestration) / B (inner per-city logic) | the 12 phases | orchestration is replaced; `city_turn`, `try_complete`, `spawn`, upkeep, loyalty and regen bodies are reused as per-entity steppers |
| `gov/*` (`actions`, `terms`, `caretaker`, `mod`) | C for data layout (`Vec<Member>`, votes, `electorate` scans) / B for rules (caretaker policy, consent rules, auto-match) | governance | per-player records on base, per-candidate tallies, bounded boards, recall that only vacates |
| `state.rs` (`WorldState`) | C | the monolithic world | replaced by entity accounts; keep `state_root`-style hashing per entity and an event hash chain (`event_head`) per region/faction |
| `map.rs` (`Map`, `generate`, `claim_territory`, `start_value`, `place_sites`) | C (container) / B (generation steps) | full-hexagon map | chunked map; `claim_territory` local |
| `mapgen.rs` | B | symmetric generator (`field` noise is a pure per-hex function; rank assignment, connectivity, rivers, resources are per sextant) | per-chunk generation with rank inside the chunk and guaranteed passes on chunk seams; symmetry per ring of chunks |
| `genesis.rs` | C | whole-season genesis (`new_season`, `map_seed`, `start_order`) | spawn per player/cohort; `map_seed` reuse |
| `vision.rs` | B | belief states for display, `obs_leaves` | off-chain only unless fog on PER |
| `preview.rs` | B | legal-action previews for UI/agents | keep as the shared agent/UI surface (Eternum lesson: one shared simulator) |
| `invariants.rs` | B | post-tick invariants (incl. USDC conservation in v9, u128) | per-entity invariants + global money invariant per vault |
| `params.rs` (`Ruleset`) | A | all tunables, `RULES_VERSION = 8`, presets | new presets; ruleset hash binding (WP15 `RulesMismatch`) stays |
| `error.rs`, `probe.rs` | A | errors, CU probes | – |

**ID widths to revisit** **[code]**: `CivId = u16` (fine for 6 factions, not for per-player owners), `CityId`/`UnitId = u32`, `MemberId = u32` but v9 ballots are `[u16; 4]` with `NO_VOTE = u16::MAX` (caps members at 65,534: chain-members §7), map radius `u8`.

---

## 6. Audit lessons as design requirements for any architecture

Source: 85 findings in `fix/findings/WP01…WP18` (18 work packages), the integration contract `fix/contract.md` and `fix/contract-amendments.md`. Each requirement below is a generalisation of a concrete failure; the "source" column names it so the redesign can be checked against the original exploit. **Every requirement must hold under adversarial load, not just honest load.**

### A. No permissionless freeze or fund lock (liveness)

| # | Requirement | Source (finding) |
|---|---|---|
| A1 | No sequence of permissionless or member-sendable instructions may leave any season/world object in a state from which progress and settlement are impossible. Prove it per state machine. | WP01 `seating-freeze-play-on-base` [critical]: play instructions ran on base during Seating and froze the season; WP02 `permissionless-undelegation-order` [critical] |
| A2 | Every instruction checks the lifecycle status **and the layer** (base vs ER) it may run in; "the account happens to be writable" is not permission. | WP01 (play on base), WP02 `redelegate-after-finish`, `delegate-after-finished` |
| A3 | A critical-path instruction must never fail because of data a third party can add (inboxes, proposals, journals, order lists). Either bound the data per payer at write time or keep it out of the critical path. | WP03 `submitgov-inbox-flood-heap-oom` [critical]; WP07 `world-overflow-by-proposals` [critical]; scale B's MeritLog freeze |
| A4 | No "fallback" that lets an honest stall or a lock change outcomes (e.g. incumbents continue, partial tallies). Use barriers: the step waits, and escape hatches refund. | scale-design G2; rejection of designs B and C (§1) |
| A5 | Wind-up (commit/undelegate) is shape-checked, ordered and idempotent; anyone may push it forward only in the fixed order; crank retries never wedge. | WP02 `er-open-oversized-undelegation` [critical], `chunk0-first-breaks-undelegation` [high], `crank-undelegate-not-idempotent` [medium] |
| A6 | Every creatable configuration has an end-to-end test from creation to final claim. | WP13 `season-preset-genesis` [high]: Season preset with 2/3/6 nations never finished genesis |

### B. Bounded work per instruction under adversarial load

| # | Requirement | Source |
|---|---|---|
| B1 | Every instruction has a proven worst-case CU ≤ 1.2M and heap ≤ 224 KiB (v9 margins under 1.4M/256 KiB), measured on the real SBF build with adversarial fill, not honest averages. | contract §4.2–§4.3 (joint capacity gate); WP07 `world-ceiling-below-256-members` |
| B2 | Work per instruction must be independent of the number of players and of world size: no O(members) scans (electorate, elections, settlement), no whole-map scans (`facts_all`), no whole-world decode. | WP06 `election-cu-freeze` [critical]; scale rules-members C4, C8, C13, C15 |
| B3 | Zero-cost or free actions still count against a hard cap (orders per batch, free orders, bytes). | WP04 `zero-cost-order-flood-heap-freeze` [critical] (caps 24 orders, 8 free, 950 B) |
| B4 | No unbounded search on chain (A*, patrol planning). Paths are submitted and verified step by step. | WP05 `patrol-phase3-cu-freeze` [critical] |
| B5 | The atomic unit of work must be splittable below any adversarially growable cost: today "a phase" is atomic and one phase could exceed the budget. | WP05 (phase is the split unit), WP06 (election cannot be split) |
| B6 | Bytes per transaction ≤ 1,232 (base) with every required account and signature; logs per transaction ≤ 10 KB; one data-carrying log call per transaction ("alone" rule). | WP01 `log-truncation-da-break` [medium]; WP03 `no-program-byte-caps-on-batches-and-gov` [low] |

### C. Contention and hot accounts

| # | Requirement | Source |
|---|---|---|
| C1 | No shared account on the critical path that an outsider can write-lock **for free**. On the ER, transactions cost 0 and anyone can list any account as writable next to a CU burner; the program's code never runs, so it cannot defend. | WP03 `er-transactions-are-free` [medium]; scale platform §2.3; scale-design §12.3 |
| C2 | Player writes go to player-owned or sharded accounts; aggregate reads are done by a bounded reducer. | scale platform §2.2 (tally account = single queue); scale-design G1 |
| C3 | On base, hot accounts cost the attacker fees (12M CU per writable account per block): shard registration counters so no single account is on every Register. | scale platform §2.3 |
| C4 | Blast radius: one attacker transaction can lock up to 32 (legacy) or ~64 (v0 + ALT) accounts; design so that this stalls as little as possible, and plan a dedicated or ingress-filtered ER (F13) before large public play. | scale-design §0 item 9, §12.3 |

### D. Authentication, admission, Sybil

| # | Requirement | Source |
|---|---|---|
| D1 | Every player-sent instruction authenticates the player on chain before storing anything (no "store now, the engine drops it later"). | WP03 `submitgov-no-member-authentication` [medium] |
| D2 | Session keys must sign their own registration; duplicate keys cannot stop the season. | WP17 `register-session-frontrun-by-facilitator` [medium]; V5 §18.14 duplicate-key fix |
| D3 | Token accounts are checked for owner as well as mint. | WP17 `register-wallet-token-owner-unchecked` [low] |
| D4 | Registration capacity limits must not create lockouts or side channels (season full locks out AIs; "full" answers leak AI identity). | WP07 `seasonfull-ai-lockout-verified`, `ai-leak-seasonfull-side-channel` |
| D5 | Relays may pay fees but cannot change or forge actions; anyone may relay; relay flooding cannot deny a player who sends directly. | scale-design §8.4, §12.3 |

### E. Operator neutrality and randomness

| # | Requirement | Source |
|---|---|---|
| E1 | The operator cannot know or choose any randomness in advance: tick/event randomness and the season seed come from MagicBlock VRF after every input to the draw is sealed (Plan A, owner-approved). | WP11 `operator-chooses-tick-vrf` [high], `season-seed-atomic-grind-first-election` [high]; contract-amendments (VRF Plan A confirmed) |
| E2 | Commitments are closed at the deadline; late commits or same-transaction close cannot let a lone committer choose randomness. | WP01 `sole-committer-post-deadline-close` [medium] |
| E3 | If players deposit sealed orders with an operator service, the operator must not see them early or choose reveal order; default is self-reveal (O4). | WP11 `operator-sees-seals-grinds-vrf` [high] |
| E4 | No free withholding option on randomness (commit empty, reveal or not after seeing others). VRF removes it. | WP11 `free-withholding-option` [low] |
| E5 | A shared VRF queue can be filled for free; a timed-out draw has a deterministic, logged, verifier-visible fallback, never a silent one. | amendments A19–A21; O2 |
| E6 | Operator-run AI players cannot hold privileged paths (no operator-only order path, no keys to vacant offices). | V5 §17.1 caretaker; DESIGN.md trust model |

### F. Money: solvency, conservation, arithmetic

| # | Requirement | Source |
|---|---|---|
| F1 | On-chain solvency check at settlement and at every payout: owed ≤ vault; `outstanding` decremented exactly per claim; `Σ claims + outstanding + ops + dust == paid_in` holds at every moment. | WP12 `no-solvency-or-conservation-check` [high]; contract C26, O23 |
| F2 | All money arithmetic is checked (overflow checks on in the chain crate) and bounded (price ≤ 100 USDC/unit, amount ≤ 10,000, fee/deposit ≤ 1,000 USDC, 6-decimal mint, mint without freeze authority). | WP08 `exchange-rounding-underflow` [critical], `exchange-price-mul-overflow` [critical], `overflow-checks-off`; WP12 `unbounded-fee-and-decimals` |
| F3 | Batch checks are per batch, not per order (cumulative budget/stock reservations). | WP08 `gold-amm-no-cumulative-cap` [medium] |
| F4 | No value-transfer channel bypasses the tariff/caps (self-dealing via unbounded prices, returned escrow still counting toward tariffs). | WP08 `exchange-price-unbounded-transfer-channel`, `contract-escrow-tariff-pump` |
| F5 | Rounding: remainders go to a defined sink; no dust stranded; no orphan balance (treasury nobody deposited into) stays in the vault. | WP12 `refund-dust-stranded`, `vault-dust-left`; V5 §18.6 leak fix |
| F6 | Per-player deposits are uniform or have a floor, so a 1-unit depositor cannot capture orphan income. | WP12 `deposit-no-minimum-sink` [low] |
| F7 | The verifier checks vault token flows and the program that holds the money, not only game roots. | WP12 `verifier-ignores-vault-and-upgrades` [medium]; WP15 `verifier-trusts-gateway-program` [high] |

### G. Hidden information and roster privacy

| # | Requirement | Source |
|---|---|---|
| G1 | Any commitment that hides identities must be blinded (salted per entry); an unblinded hash of public tags reveals everyone. | WP09 `roster-chain-not-hiding` [high] |
| G2 | Reveals of hidden data happen only after the end, only in order, only by the committed party; a leaked secret cannot be used to poison the reveal. | WP09 `roster-self-reveal-grief` [critical], `roster-poison-verified`, `roster-salt-published-midseason` |
| G3 | The operator cannot profit from withholding the reveal after seeing the outcome: bond floor on chain, forfeiture to players. | WP09 `operator-forfeit-option` [high]; O21 |
| G4 | Hidden players' money paths (funding, refunds, treasury shares) must not leak value back to the operator. | WP10 `operator-ai-treasury-refund-leak` [high] |
| G5 | Tag authenticity depends on who chose the tag: a person's client must choose it. | WP09 `operator-planted-tags` [low] |
| G6 | Side channels (registration timing, "full" responses, naming, policy ids, reveal habits) are listed and minimised; accepted residuals are documented. | V5 §18.10, §18.14; WP07 side channel |

### H. Governance and treasury abuse

| # | Requirement | Source |
|---|---|---|
| H1 | Treasury-spending orders are never proposable/adoptable; consent must come from a seated, different officer; AI officers never consent. | WP10 `ai-officers-adopt-drain-proposals` [high], `caretaker-consentspend-bypass` [high]; O22 |
| H2 | First-term offices cannot be bought by registering early/directly (no pre-season vote capture in AI seasons). | WP10 `t0-election-capture-direct-register` [high] |
| H3 | Redistribution caps must hold for any nonzero merit and at every step (no "to every person" backdoor). | WP10 `ai-redistribution-farming-cap-bypass` [medium] |
| H4 | At scale: capture must not pay. Cap per-person officer pay; cap treasury spend per term (κ = 25 %); recall can only vacate. | scale game-intent §3.1; scale-design §7.4, R7, F8, F12 |
| H5 | Bounded boards (proposals) need anti-squatting: deposits or eviction by support, not first-come slots. | WP07/O13 (4-colluder slot DoS); scale game-intent §3.2 |

### I. Escape hatches and lifecycle

| # | Requirement | Source |
|---|---|---|
| I1 | Every state has a timeout after which anyone can abort and every player is refunded in full (fee + deposits + share of forfeited escrow); the operator bears the loss (O6/F17). | WP14 `no-escape-hatch` [medium]; contract O6–O7; scale-design §11.4 |
| I2 | Stuck delegations have an owner-side escape (DLP `RequestUndelegation` after its timeout, `RollbackUndelegation`); a rolled-back world refunds rather than mis-pays. | WP14 `no-dlp-escape-hatch` [high]; tags 34–35 |
| I3 | The ER validator is pinned per season and delegation happens once, in order. | WP14/WP15 (O8), `WrongValidator`, `DelegationOrder` |
| I4 | Rent is reclaimable (close instructions after claim); nothing permanent is created per player without a close path. | WP14 `rent-never-reclaimed` [low]; scale platform §5 |
| I5 | MagicBlock's sponsored-commit budget (10 per account without a delegated fee payer) must never block the final undelegation. | WP02 `sponsored-commit-budget-no-fee-vault` [medium] |
| I6 | The operator's delay of any step is bounded: after a grace, anyone may take over the step (seating takeover after 600 s). | contract O7, tag 15/16 changes |

### J. Upgrade and trust roots

| # | Requirement | Source |
|---|---|---|
| J1 | Upgrades never strand live seasons: layouts are append-only, magics versioned, migration or "deploy between seasons" guard. | WP15 `upgrade-strands-live-seasons` [high], `strict-magic-no-migration` |
| J2 | The ruleset (and settlement logic) is bound at creation and checked on every tick/settlement (`RulesMismatch`). | WP15 `no-ruleset-version-binding` [medium] |
| J3 | Upgrade authority is a multisig with time lock or final. | WP15 `single-key-upgrade-authority` [high]; O27 |
| J4 | The ER validator and the VRF oracle are named trust roots; what they can forge is documented (the validator decides the settled world). | WP15 `er-validator-unstated-trust-root` [high]; contract §0 item 11 |
| J5 | Builds are reproducible from any path with a pinned toolchain. | WP15 `build-not-reproducible-off-author-path` |

### K. Verification and data availability

| # | Requirement | Source |
|---|---|---|
| K1 | No state change without its complete input published on chain first; the verifier rebuilds everything from chain logs only. | DESIGN.md (`InputNotPublished`); WP01 log truncation |
| K2 | The verifier reads every record of every transaction (not the first per tx) and fails closed: missing records ⇒ INCOMPLETE/FAIL, never a fallback to the operator's index. | WP16 `verifier-first-ps-tick-only` [medium], `verifier-accepts-gateway-claims` [low] |
| K3 | The verifier pins the program id and checks the program, upgrade history, mint and vault flows. | WP15 `verifier-trusts-gateway-program` [high]; WP12 |
| K4 | Signatures needed by the verifier cannot be pushed out of a bounded search window by spam. | WP16 `findclose-window-verifier-dos` [low] |
| K5 | Cross-account consistency (chunks written by the same resolution) is checked, e.g. a root/trailer per write. | WP01 `no-cross-chunk-consistency` [low]; contract C27 |
| K6 | At scale, per-entity or per-region event chains must cover every write (including player writes on base) so a replay has no gaps; base logs are durable, ER retention must be planned. | scale rules-members §4 WP16 row; scale-design §13 |

### L. Testing discipline

| # | Requirement | Source |
|---|---|---|
| L1 | Instruction-level tests on the real SBF build for every (instruction, error) pair; audit repros flipped into permanent tests. | WP18 `no-instruction-level-tests` [low]; contract §4.1–§4.2 |
| L2 | Golden replays pin roots; any behaviour change bumps the rules version. | contract §2 |

### Count and severity

18 work packages; the critical findings were: permissionless freezes (WP01 ×2, WP02 ×2, WP03 ×2, WP04, WP05, WP06, WP07), money minting (WP08 ×2) and roster poisoning (WP09). All but the two WP08 findings are liveness failures from unbounded or permissionless work on a shared world. **The single most important lesson for an open world is that every one of these came from a shared object (the world, a nation inbox, a roster, a phase) that anyone could make bigger, slower or stuck.** **[judgement from audit titles]**

---

## 7. Scale-study conclusions that still apply

The scale study (`scale/scale-design.md`, revision 2) chose "many 48-seat realms under six banners now; Commons realms of up to 2,400 citizens on base with a 48-seat ER 'Table' later". The owner has since rejected caps and asked for an Eternum-like open world. Many of its **facts** stay valid regardless of the chosen shape.

### 7.1 Still valid, and binding

| Conclusion | Numbers | Why it survives |
|---|---|---|
| **Free ER write-lock griefing** is the main platform risk. ER txs cost 0; the engine serializes writes per account FCFS (≤ 64 executors; backpressure at 16 × executors pending); one legacy tx can lock 32 accounts (1,226 B measured), up to ~64 with v0 + ALT. | scale-design §0.9, §12.3; platform §2.1–§2.3 | Any design with delegated shared accounts on a public ER inherits it. Mitigations: no critical shared account; sharding; bounded blast radius per validator; a dedicated or ingress-filtered ER (F13) before large public play; spike S1 must measure it. |
| **Citizens (players) write on base, not the ER** (G1). On base, contention is a fee auction the defender wins for micro-SOL; relays cannot forge. | scale-design G1, §12.2 (≤ 230 base tx/s at 50k; ~1.4 % of base capacity) | Holds for any open-world design where most player actions are infrequent and lazy. |
| **Few, small delegated accounts; commit roots, not tables.** Committor finalize: ≤ 64 keys, ~2.5 KB dirty data, 359,700 CU per intent; 10 sponsored commits per account; 0.0003 SOL session fee per delegation; 41k-account mass delegation stalled until a fix (validator#1603). | platform §1 rows 7–12, 24; §3 | An open world on the ER must keep hot regions delegated in small chunks and commit roots; per-player delegation is costly (50k ≈ 15 SOL fees, ≈ 117 SOL deposits locked). |
| **Rent.** 5,080 lamports/B on mainnet since 2026-09-11 (SIMD-0437 step 2; later steps target 696); a 320-B Member PDA ≈ 0.00228 SOL; 10k ≈ 22.8 SOL, 50k ≈ 114 SOL, reclaimable only with a close instruction. Packed 64-B records: 3.3 SOL (10k). ZK-compressed PDAs ≈ 1 SOL for 50k (non-refundable, +200–300k CU per tx, Photon indexer). ER ephemeral accounts `(len + 60) × 32` lamports (50k × 64 B ≈ 0.2 SOL) but never committed. | platform §0, §1, §4–§6 ([SolanaCompass SIMD-0437](https://solanacompass.com/news/simd-0437-step-2-goes-live-on-solana-mainnet-rent-drops-to-5080-lamports-per-byte), [MagicBlock pricing](https://docs.magicblock.gg/pages/overview/additional-information/pricing.md), [ephemeral accounts](https://docs.magicblock.gg/pages/ephemeral-rollups-ers/introduction/ephemeral-accounts.md), [ZK compression considerations](https://www.zkcompression.com/learn/considerations.md)) | The operator pays rent out of its 20 % unless players deposit SOL (F10). An expandable map adds per-chunk rent (4 KiB ≈ 0.028 SOL at 6.96e-3 SOL/KiB). |
| **Per-action base costs are small and constant in N**: CheckIn 8.0k CU, Endorse ≤ 23.6k, claim math 5.4k, sortition 4.4k per try (measured in the D lab). | scale-design §0.7, §12.1 | Templates for per-player instructions in any design. |
| **Relays**: instructions check the session key, not the fee payer, so anyone may relay; clients ship a relay list and can send directly. | scale-design §8.4 | Needed for "no SOL for players" at scale. |
| **Off-chain stack breaks at ~500–1,000 viewers**: O(N) `getProgramAccounts` per join, `/season` returning every member, `/api/state` every 750 ms per browser under one mutex, whole-file state saves. A CDN state cache per (region, faction, tick) and header polling are required. | chain-members §0.4, §4; scale-design §8.4 | Independent of chain design. |
| **Public devnet RPC** allows 40 calls per method per 10 s per IP: a paid/dedicated RPC is required for any mass test. | platform §1 row 26 | – |
| **Governance at scale**: direct democracy over 4 offices with a merit pot ∝ members becomes capturable for profit (≈ 5.7× return at t = 0.3, ≈ 28× at t = 0.1 for 1,667-member nations under V5 rules). Remedies measured: capped officer pay (≤ 0.5 f per office-term, person cap 2 f), treasury caps per term (κ = 25 %), recall only vacates, jury with quorum; even then a 10 % always-active bloc captures 21–27 % of office-terms at 30 % turnout. | game-intent §3.1; scale-design §7.4 (capture2.py, 6,000 trials per cell) | Any faction-governance design at scale must bound what capture can gain; "capture never pays" cannot be promised. |
| **Payouts as closed forms at claim time**, `outstanding` as Σ allocations decremented per claim; bounded "citizen share" `u_i = 1 + β·min(w_i,12)/12` (β = 0.25) keeps a script wallet below its fee (≤ 0.93 f at n = 400). | scale-design §11.3; rules-members §6 C15 | Directly reusable for many players. |
| **Mobilisation**: a per-player action every window whose effect depends on the faction's participation *rate* (concave, capped), not head count; Sybils raise numerator and denominator alike; no merit for pledges. | scale-design §7.3; game-intent §5.6; Constitution §5 | The cleanest precedent for P9 at scale. |
| **Cells of ~32** as the social unit (chat, voice, team stake), dealt by VRF after close. | scale-design §8.1 | Useful whatever the map. |
| **AI at scale**: ~1 AI per 4 cells (≈ 20 per 2,400) keeps "is our tribune an AI?" alive; `MAX_AI = 64` and per-realm rosters must scale; bond floor `⌈(a·P0 + N·π)/(N − a)⌉` tends to π at large N; AI rows voided at reveal and paid to people. LLM usage: rule bots by default, LLM only at key moments (V5 D24), budget per season. | scale-design §10; V5 §18.8 | Hidden-AI pillar P7 needs a count proportional to players, with blinded per-AI commitments. |
| **Abort economics**: an abort from Running refunds players in full (fee + deposits + escrow share); the operator loses its 20 %, escrow and SOL; attributing a stall is impossible on chain, so the operator cannot keep its share (F17). | scale-design §11.4 | Any escape hatch design inherits this trade-off; a third party can cause aborts for free on a public ER. |
| **Tick length**: thousands cannot deliberate in 30 s; long ticks/windows (60 s ticks with ~13-min windows; 120–300 s for mass waves) were recommended. | scale-design F6; game-intent §4 | In a lazy/timed-action world the analogue is action durations and stamina regeneration sized for humans (Eternum's always-online lesson). |
| **Rejected designs and why** (design lessons): B (one world, externalized members) — a merit log over lockable shards made ResolveTick freezable in ~11 ticks, an election scan stall kept incumbents, recall lanes lockable in one tx; C (one world, cells, assembly) — tallies proceeding "when every page is stamped or the deadline passed" let a free lock exclude cells; 1,566 page undelegations could be stalled. | scale-design §1 | Directly relevant to an open world: **never let resolution depend on accounts an outsider can lock, and never let a deadline turn a lock into a different outcome.** |

### 7.2 Conclusions that the owner's new request changes

| Scale-study conclusion | Status now | Why |
|---|---|---|
| "The ER world never holds more than 48 seats" (R1) and horizontal scale only via realms | **Superseded as a product answer** (owner: any number of players; amendments already said 48 is not accepted). Still valid as a *measured* bound for the monolithic `WorldState` design. | The limit is the monolithic world, not the platform. An entity/lazy design removes the per-member term. |
| Six banners across many parallel realms | **An option, not the default.** | It keeps each realm small but does not give "one shared map" (Eternum's 8,000 realms share one map). |
| Phase 1/2/3 schedule, 10-12 cut line | **Void** (deadline ignored). | – |
| Symmetric 6-sextant map as the fairness guarantee | **Must be restated** for an expandable map (§3.2). | Fixed radius and whole-sextant rank assignment. |
| "Commons" windows applied at a Table tick (G2 barriers, G4 digests) | **Pattern still useful** for any design where base-written player data feeds ER resolution: barrier, digest reconciliation, storage that never wraps (G5). | – |

### 7.3 Open spikes named by the scale study (still open)

S1 (ER write-lock latency under a looping CU burner), S2 (ephemeral accounts persistence), S6 (read-only base clones on the ER), S7 (multi-validator throughput), S8 (ingress-filtered/dedicated ER options, BSL-licensed self-hosting), plus ALT availability on the ER. None has run on a real ER. **[scale-design §15; platform §14]**

---

## 8. Implications for the open-world redesign (inputs, not the design)

**[judgement]** The following forks fall out of §3–§7 and should be decided explicitly:

1. **Time model.** Keep a shared clock; drop the whole-world tick. Choose between (a) Eternum-style timed actions with lazy settlement, (b) regional micro-ticks per map chunk with local sealed orders, (c) a hybrid: lazy economy + sealed local rounds only for contested events (battles, contested tiles, auctions). All three keep P2 locally; (a) alone turns simultaneity into a latency race on FCFS, which favours bots (P6).
2. **Ownership.** Players own entities (Eternum-like, restores P9 at any scale) vs factions own everything (V5, needs tiers of governors). The former keeps faction politics for faction-level decisions only.
3. **Decoupling city economics.** Remove or epoch-sample the faction-wide inputs to per-city computation (city count in amenities, war weariness, deficit, suzerain bonus), otherwise lazy evaluation cascades to the whole faction.
4. **Map growth and fairness.** Chunked map with per-chunk generation; fairness per spawn ring (rotated copies) rather than one global symmetric hexagon; each chunk a separately delegated ≤ 4 KiB (or small) account; world size must never enter any instruction's cost.
5. **Global jobs.** Only prize split, crisis/dark age, end-of-season snapshots and history need global views; they run over O(factions) aggregates maintained incrementally.
6. **Governance at scale.** Offices only for faction-level decisions; tiers (cells, delegates, district governors, jury) and capped officer pay (H4); treasury caps per term.
7. **Money.** Per-player closed-form claims; one vault per season (or per campaign) with the WP12 invariant; per-player rent reclaim.
8. **Hidden AI.** Blinded per-AI commitments; AI count proportional to players; home entity drawn by VRF at an event.
9. **Verification.** Per-entity or per-chunk event hash chains; every player write logged; verifier fails closed; DA for ER logs planned.
10. **Platform preconditions.** F13 (dedicated/ingress-filtered ER) and S1 before large public play; paid RPC; CDN state cache.

---

## 9. Appendix: key constants and file references

| Item | Value | Where |
|---|---|---|
| `RULES_VERSION` | 8 (v9 in progress) | `permutation-rules/src/params.rs:12` |
| Blitz preset | 30 s ticks, 8 max civs, map radius 13 (547 tiles), 180 ticks | `params.rs:240-250` |
| Season preset (not creatable) | 4 h ticks, 16 civs, radius 19 | same; contract O25 |
| `max_members` | 256 (v9: 48) | `params.rs:324`; contract §1.7 |
| `term_ticks` | 30 | `params.rs:326` |
| `PHASE_COUNT` | 12 | `tick/mod.rs` |
| `MAX_TERRITORY_RADIUS` | 3 | `map.rs:196` |
| `loyalty_pressure_radius` | 6 | `params.rs:298` |
| `CITY_VISION` | 2 | `vision.rs` |
| Order budget | `min(3 + cities, 8)` | `economy::order_budget`; V5 §5.2 |
| Techs / unit types | 16 / 8 | `tech.rs`, `units.rs` |
| Order variants | 28 | `orders.rs` |
| Gov actions | Stand, Vote, Propose, Support, Recall | `gov/mod.rs` |
| World | 20 × 4 KiB chunk PDAs, body ≤ 81,844 B; ~23 KB with ~15 members | DESIGN.md; audit-map §1 |
| Nation account | 8 KiB (`PSNATN07`) | DESIGN.md |
| Season account | 4 KiB, payout table 8 B per member | DESIGN.md |
| Program size | 1,691,896 B at HEAD | contract §5.1 |
| Devnet program | `J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n` | DESIGN.md |
| v9 new instructions | StartClock, PostBond, FreezeTick, ConsumeTickRandomness, RetryTickRandomness, ConsumeSeasonSeed, RetrySeasonSeed, Abort, RequestUndelegation, RollbackUndelegation, CloseSeasonAccounts (tags 26–36) | contract §1.1 |
