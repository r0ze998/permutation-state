# Wylls — Game Design V4

> **2026-09-24 note — [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) supersedes parts of this document.** Participation becomes 6 nations with unlimited members, governed by elected offices. Prizes are split by achievement points and individual contribution, replacing §4.7 victory tracks and the track split and payout in §4.10. Everything else here still applies, as V5 §0 and §10 describe.

**Civilization, one save for everyone.**

Status: design review draft, 2026-09-24. It replaces the Game Constitution (Handoff design), the REBUILD (citizen-embodied `/civilization/`) and Design V3 (Proof of Consequence) as the **source of truth for the game design**. Those documents stay as historical records.

Numeric rules: [Rules Specification v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md).

Evidence: [Eternum benchmark](research/BENCHMARK_ETERNUM.md) (based on the local Eternum source) · [Civilization / 4X / persistent strategy benchmark](research/BENCHMARK_CIVILIZATION_4X.md) · [2026 tech landscape and the hackathon](research/TECH_LANDSCAPE_2026.md). Evidence tags in this document: **[E]** Eternum, **[C]** Civ/4X, **[T]** tech, **[判断]** this design's own judgement.

> ⚠️ **The hackathon deadline is 2026-10-12 23:59 PT, not 10/15** (Colosseum *Crypto World's Fair* official rules §5) **[T]**. Section 11 of this document is scoped to 10/12.

---

## 0. Conclusion

**Humans and AI agents each lead their own civilization on one persistent pixel hex world. Each season is fought over three victory tracks: conquest, science and concord. The rules, the treasury and the payouts are fixed on Solana, and nobody — not even the operator — can change them after the fact.**

Five decisions support that sentence.

| # | Decision | Why |
|---|---|---|
| 1 | **One world (one save) with many civilizations.** Geography, ruins and history persist across seasons. Competitive power (cities, armies, stockpiles) is **soft-reset** each season | Only way to satisfy both "one save" and "fair prize competition". The Civ VII Age-transition model **[C]** |
| 2 | **Simultaneous-resolution ticks + an order budget identical for every civilization** | Removes the speed and staying-awake advantage agents have over humans **[C: Old World, AlphaStar, Dark Forest]** |
| 3 | **Three victory tracks, each with its own pool share**: Dominion (conquest + territory), Science, Concord (peace + neutrality) | "Largest army" rewards hoarding and whales, so it is dropped. "Neutral" on its own rewards idle civilizations, so it becomes a multiplier inside Concord **[C]** |
| 4 | **External AI agents enter on exactly the same terms as humans** (same API, same view, same budget, same cost) | Screeps-style. Each agent's decision process is recorded in a verifiable form **[C][T]** |
| 5 | **Each technology is assigned the one mechanic that cannot work without it** (MagicBlock ER/PER, USDC vault, agent API, x402 entry). ZK is postponed until it has a real job | Avoids the anti-pattern of "a list of technologies we used" **[T]** |

---

## 1. Internal contradictions in the pitch, and how they are resolved

The pitch as given ("Civilization, one save for everyone … Rules and treasury run on Solana, so not even we can steer it") has five internal tensions. The design is built around resolving them.

| Tension | Problem | Resolution |
|---|---|---|
| **"One save" vs. fair seasons** | If the world carries on unchanged, veteran civilizations keep their advantage and a new entrant paying an entry fee cannot win | The **world persists but power resets** (§4.1). The winner's history is left in the world as ruins, monuments and names, and the next season's settlers inherit those as "heritage" |
| **"Not even we can steer it" vs. balance bugs** | Eternum could not change a launched season, balance errors lasted to the end, and Season 0 scores were recalculated afterwards **[E]** | (1) Test balance on the short **Blitz** format before running a long season with the same rules. (2) Write the stalemate breaker and the emergency stop (refund only) into the rules **in advance**, not as ad-hoc intervention **[C: Foxhole]** |
| **"Win by war" vs. fairness to humans** | Real-time raids favour players who are always online, and bots **[C: Travian/OGame]** | Simultaneous-resolution ticks, an order bank and standing orders (§4.2). Pre-set defensive orders run while you sleep |
| **"AI plays as an equal" vs. multi-accounting (sybil)** | One operator can field ten puppet civilizations and feed a single winner | The entry fee is the sybil cost. Caps on transfers between civilizations, no victory decided by votes, trade scored by counterparty diversity, identity verification at claim time (§6.4) |
| **"Choices shape the rules" vs. immutable rules** | If the rules can change freely, "not even we can steer it" is false | The **Season Law**: each track winner picks **one** item from a menu of modifiers published in advance, and it becomes next season's rule (§4.9). The change is bounded and its selection is recorded on chain |

---

## 2. What the benchmarks tell us to adopt

| Source | Adopt | What it becomes in this game |
|---|---|---|
| Civ VII Ages **[C]** | Age transition: automatic peace, army pruning, neutral powers respawn, Dark Age consolation | Soft reset at the season boundary + a consolation legacy for civilizations that scored nothing |
| Civ VI grievances / war weariness / loyalty **[C]** | Aggression recorded deterministically | **Grievance ledger** (on chain). Feeds war weariness and Concord eligibility |
| Civ VII independent powers **[C]** | Neutral city-states + suzerainty locked for the Age | NPC city-states. A **sybil-resistant** arena for peaceful competition |
| Old World orders **[C]** | Per-turn action budget | Per-tick **order budget** (identical for humans and agents) |
| Screeps **[C]** | Code players, CPU bucket, staged-opening novice areas, per-season scoring rule | Order bank (up to 3 ticks), protected zones for new civilizations, one rotating modifier per season |
| Screeps Season 5 **[C]** | Score by `log(held duration)` | All victory scores are **integrals over time**, not end-of-season snapshots |
| Dark Forest **[C]** | Official plugins/API legitimise automation | Official public API + first-party automation (governors, standing orders) |
| Conflict of Nations **[C]** | Coalitions need a higher VP threshold | Coalition handicap |
| Foxhole **[C]** | Automatic resolution when a war stalls | A **stalemate breaker written in advance** into the rules |
| Travian **[C]** | NPC pressure on the leader, alliance member cap | End-of-season "Crisis" hits the leaders hardest; alliances capped |
| Eternum combat **[E]** | `(N_A+N_B)^0.2` sub-linear term, stamina refunds, low-casualty raids | Combat formula that damps big-army snowballing (§4.5) |
| Eternum Banks / Village tax **[E]** | Infrastructure that earns fees becomes a war target | Market cities: the holder earns a share of in-game fees |
| Eternum Villages **[E]** | Settlements that can be raided but not conquered | "Frontier settlement" seat: low stakes, for casual humans and cheap agents |
| Eternum Faith **[E]** | Non-military victory through pledges and vassalage | Base design for Concord's city-state suzerainty |
| Eternum Blitz **[E]** | One engine, multiple presets; permissionless payout | **Season** and **Blitz** presets. Anyone can submit a ranking, the contract verifies it |
| Eternum hired agents **[E]** | Three agent codebases lost to drift | **One client library shared by the UI and agents** (§6.1) |

### Things we explicitly avoid
- **Victory decided by votes** (Civ VI World Congress = a bribery race; sybils make votes free) **[C]**
- **"Largest army" as a paid victory** (hoarding and whale domination; RoK) **[C]**
- **End-of-season snapshot scoring** (last-day rushes, trading cities for bribes) **[C]**
- **The operator selling power or orders** (Screeps had to reverse this) **[C]**. Player-to-player USDC trade is allowed only inside the Exchange caps (§4.10)
- **Leader-fixed shares / top-alliance-only payouts** (Eternum's 30% tribe-leader share encourages mega-alliances) **[E]**
- **Treasury funding with manual payout** (Eternum Season 1) **[E]**
- **Resource sprawl at launch** (Eternum cut from 22 to 9 over about a year) **[E]**
- **Rules shaped by chain limits** (Eternum's hourly delivery gates) **[E]**
- **Leaking hidden information on chain**, and trying to detect or ban agents (Cicero passed as human) **[C]**

---

## 3. What the player experiences

**30 seconds:** Look at the hex map, issue orders to cities and armies (spending the budget), then watch the result resolve at the next tick. The world does not move without a tick, so the player is never out-raced.

**1 day (Season preset):** Log in 1–3 times a day. Review what happened (combat, contact, treaties, crisis alerts), fix standing orders and the city build queue, take diplomatic action (sign or break treaties, influence city-states). Unused orders stay banked for up to 3 ticks.

**1 season (30 days):** Founding → Expansion → Contention → Crisis → Resolution (§4.8). At the end the three tracks are settled, USDC is claimable, the world's history is updated and the Season Law is chosen.

**Across seasons:** The same world map carries on. "The ruins of the Iron Kingdom the agent Nova built last season" are marked on the map, and whoever settles near them this season gets a small heritage bonus. What carries over is **history, names and cosmetics**, not strength.

---

## 4. Game design

### 4.1 The world and "one save"

- **Hex grid, pixel art**, offset or axial coordinates. The existing hex renderer in `/civilization/` is a candidate for reuse.
- **Map size**: 60–80 hexes per civilization (Polytopia-style density **[C]**). Blitz: 6–8 civilizations, about 500 hexes. Season: 64–256 civilizations, split into **regions** of 16 civilizations each (for scaling, and so newcomers only meet their region at first).
- **What persists (the "one save")**:
  - terrain, rivers, resource deposits (with depletion)
  - **ruins** of each city from last season (founder, peak population, how it fell)
  - **monuments** (the place a track winner completed its project)
  - place names (named by founders), the world chronicle
  - the grievance ledger history (reputation; no effect on strength)
- **What resets (competitive power)**: cities, units, stockpiles, research, treaties, scores.
- **Heritage**: settling a city within 2 hexes of a ruin gives a small bonus for 10 ticks, scaled by that ruin's former prosperity. This makes "someone's history shapes my start" true on the map, and it does not favour veterans, because anyone can pick up the bonus.

### 4.2 Time model — simultaneous-resolution ticks

| Parameter | Season preset | Blitz preset (hackathon demo) |
|---|---|---|
| Tick length | 4 hours (6 per day) | 30 seconds |
| Season length | 30 days = 180 ticks | 90 minutes = 180 ticks |
| Order budget | Base 3 + 1 per city (cap 8) | Same |
| Order bank | Up to 4 ticks of unused orders (Season: 16h, covers one login per day plus sleep) | Same |
| Resolution | Simultaneous at the tick boundary, MagicBlock crank (§7). **Permissionless fallback**: if the crank has not resolved a tick within its deadline, anyone may call `resolve_tick` | Same |

- Orders submitted during a tick window are **resolved together** at the tick boundary. **Arrival order within a tick carries no advantage** (conflicts are broken by a fixed rule such as a VRF seed plus sorting by civilization ID).
- The fact that a Season and a Blitz both last about 170–180 ticks means a Blitz is literally "a season compressed", which is what makes Blitz useful for balance testing **[判断]**. All balance numbers are defined **per tick**, never per wall-clock time, so the two presets share one ruleset.
- **Why Blitz is 30s, not 20s (design review 2026-09-24)**: an agent spends its full budget every tick, but a human needs time to read the result and issue orders. At 20s with a budget of 14, humans would structurally under-spend their budget, which breaks the equality principle (§5). 30s ticks with a smaller budget (cap 8), plus the order bank and standing orders, keep "orders per second of human attention" feasible. This must be verified in the 10/10–10/11 mixed playtest by measuring **budget utilisation for humans vs. agents**; if humans use less than ~80% of what agents use, lengthen the tick or cut the budget further.
- **One ruleset, two clocks (spec review 2026-09-24)**: an earlier draft gave Season a larger budget (cap 14) than Blitz (cap 8). That would make Blitz an unfaithful balance test, so both presets now use cap 8. With standing orders and multi-tick paths, 8 orders per 4h tick is enough in Season.
- **Why the order bank is 4 ticks**: with 3 ticks (12h), a player who logs in once a day loses orders every day. 4 ticks (16h) covers one daily login plus sleep; standing orders cover the rest.
- **Standing orders and governors** (first-party automation): auto-work city tiles, a build queue, "counterattack if an enemy comes within N hexes", "withdraw if under attack", recurring trade routes. These do **not** consume orders, and they run on deterministic rules. Agents can use the same automation **[C: Travian Gold Club, Dark Forest plugins]**.

### 4.3 Civilizations, cities and yields

- **Entry**: pay the entry fee → the civilization is placed in a **protected start zone** in its region (§4.6).
- **Yields (5 kinds; kept minimal)**: Food, Production, Gold (in-game currency, **not** USDC), Science, Influence.
- **Strategic resources (2 kinds)**: Iron, Horses. Required for higher-tier units. Deposits deplete, which creates reasons to fight over land.
- **Cities**: founded by a settler. Each city works tiles within radius 2. One district per city (Market, Academy, Barracks, Temple). Buildings are kept to 3 tiers.
- **Frontier settlement** (after Eternum's Villages **[E]**): 50% yield, cannot be conquered (only raided), cannot move troops. A low-risk seat for casual humans and cheap agents.
- **Super-linear upkeep**: the more cities, tiles and units a civilization has, the higher the unit upkeep (after Civ VI dynamic friction and EVE sovereignty upkeep **[C]**). This limits blobs.

### 4.4 Economy and markets

- **Inter-civilization market (gold)**: gold ↔ yields and strategic resources. An in-game AMM (constant product) plus peer-to-peer orders.
- **The Exchange (USDC)**: players sell raw goods to other players for USDC, capped per tick and per season, with a fee to the Vault (§4.10).
- **In-game fees** (paid in gold): part goes to whoever holds the **market city**, part is burned. **This is an in-game economy and does not feed the USDC pool** (see §4.10 on what the USDC pool is fed by).
- **Transfer caps between civilizations**: the amount a civilization can send to another per tick is capped (after Travian **[C]**). All transfers are frozen for the last 18 ticks (Season: 72 hours; Blitz: 9 minutes).

### 4.5 Military

- **Three unit types, rock-paper-scissors**: Spearmen (strong against cavalry) / Archers (range 2, strong against spearmen; after Eternum crossbows **[E]**) / Cavalry (mobile, strong against archers). Two tiers (tier 2 needs iron or horses).
- **Combat formula** (after Eternum, simplified):
  `damage = 2 × N_A × tier_A × terrain × fatigue / tier_B / (N_A + N_B)^0.2 × (1 ± variance of up to 10%)`
  Variance is kept small (skill dominates; also for legal reasons, §9).
- **War weariness**: each tick at war or of losses lowers amenities → lower growth. Worse when fighting abroad **[C]**.
- **Grievance ledger**: declaring war, breaking a treaty, taking a city, razing — each is recorded on chain against the attacker, and **it determines Concord eligibility** (§4.7).
- **Loyalty**: captured cities far from the capital lose loyalty and can become free cities. Distant conquest costs more to hold **[C]**.
- **Raze cooldown**: a civilization's last city cannot be taken until N ticks after it lost its previous one (prevents instant elimination).
- **Holding the capital does not eliminate a civilization**. A civilization reduced to zero cities continues as "government in exile" (can still trade and do diplomacy).

### 4.6 Diplomacy and neutral powers

- **On-chain treaties**:
  - **Non-aggression pact**: both sides post a **bond** (gold). If broken, the other side receives the bond, plus a grievance.
  - **Alliance**: `min(3, max(2, civs / 3))` civilizations (Blitz with 6: 2; Season with 16: 3). **Shared vision** via PER permission groups (§7). Alliance victories face a higher bar (§4.7).
  - **Vassalage**: pay a tribute share and receive protection (the pattern of Eternum Faith's "submission" **[E]**).
- **NPC city-states** (the "AI residents"): run by **deterministic rules** (an LLM may only add dialogue flavour; after Old World's lesson of no surprise outcomes **[C]**).
  - Civilizations accumulate influence toward them. The first to 60 points becomes **suzerain, locked for the season** **[C: Civ VII]**.
  - They cannot be created by sybils (the operator's world generation places them), which is why they are the core of the Concord track.
- **Newcomer protection**: a protected start zone per region. The walls around it **open in stages** by tick (after Screeps **[C]**). Protection also ends early if the civilization attacks first.

### 4.7 Victory tracks

A season ends at **a fixed pre-published tick** (no surprise endings **[C: Soren Johnson]**). Each track is scored as an **integral over time**.

#### ⚔ Dominion (conquest + territory)
```
per-tick score = Σ held tiles × tile value (yield level) × heritage multiplier
              + Σ captured cities × population weight   ← only cities captured from another civilization and held ≥ 30 ticks
```
- Exploit resistance: a city captured from its own coalition or from a transfer scores 0; each city scores at most once per season; a city is only counted if it was founded by another owner at least N ticks earlier.
- Super-linear upkeep and the leader-targeting Crisis (§4.8) damp snowballing.

#### 🔭 Science
- Total research + a **"Star Gate" project**: three **visible stages** built in one city.
- The project city is visible to everyone and **can be sabotaged** (stages are lost if it is captured) **[C: no surprise endings]**.
- Ranking: stages completed, and when; then total research.
- Technology cannot be transferred (prevents sybils feeding research).

#### 🕊 Concord (peace + neutrality)
```
per-tick score = (population growth + trade value weighted by counterparty diversity + city-state suzerainty points)
              × only for ticks with no aggressor grievance recorded against this civilization
              × neutrality multiplier 1.25 (if the civilization joined no alliance all season)
```
- **Being attacked does not cancel it** (only a war this civilization started breaks it). Griefers therefore cannot take it away.
- Trade counts only with **distinct counterparties**, decaying. Repeated trades with the same partner are discounted, which damps sybil wash-trading.
- There are **no voting mechanics** (bribery and sybil resistance).

#### Dropped: "Largest army"
It rewards hoarding and last-day unit spam, and pulls toward pay-to-win dynamics **[C: RoK]**. Army size stays as a public statistic and as an input to Dominion.

#### Coalitions
- Allied civilizations may appear on the "coalition board". Payout splits within a coalition **must be declared on chain before the season starts** (no fixed leader share).
- **A coalition can win at most one top prize per track**. A coalition's score is summed and then divided by `1 + 0.25 × (members − 1)` **[C: Conflict of Nations]**.

### 4.8 Season flow

| Phase | Season (days) | Blitz (minutes) | What happens |
|---|---|---|---|
| Founding | 0–3 (ticks 0–17) | 0–9 | Only protected zones. Settlement choices near ruins |
| Expansion | 3–10 (18–59) | 9–30 | Walls open in stages. Contact, trade, first treaties |
| Contention | 10–20 (60–119) | 30–60 | Fights over market cities and strategic resources. Star Gate stage 1 becomes possible |
| **Crisis** | 20–27 (120–161) | 60–81 | Barbarian or disaster waves **concentrated on the top civilizations of each track** (published formula) **[C: Travian Natars, Stellaris, Civ VII]** |
| Resolution | 27–30 (162–179) | 81–90 | Transfers frozen (last 18 ticks). Scoring at the fixed tick |

- **Stalemate breaker (pre-published)**: if no track has a decisive leader by day 20, the Star Gate's cost starts falling on a published schedule **[C: Foxhole, as a written rule rather than an intervention]**.
- **Emergency stop**: only on a critical vulnerability. It stops the season and **refunds the full entry fee**, and it is pre-committed on chain. There is no mechanism for "the operator adjusts the result".

### 4.9 Season Law — "choices shape the rules"

- The top civilization in each track (or coalition) **picks one** item from a **modifier menu published in advance**, and it becomes a rule of the next season.
  - Examples: "Iron deposits +50%" (war-leaning), "Research cost −15%" (science-leaning), "City-state suzerainty threshold −20%" (concord-leaning), "Protected zones last 1 extra day" (newcomer-friendly).
- The choice is recorded as a transaction and **hashed into the next season's `ruleset_hash`**.
- This works like Screeps' rotating season rules **[C]**, and it also prevents specialised bots from repeating their wins. It is the verifiable form of "Every choice shapes its history, rules and economy".

### 4.10 Prize pool (USDC) — funded by entry fees and in-play payments

Decisions (2026-09-24):
- The pool is funded **only by players**: **entry fees + payments made during play**. No sponsor pool, no yield mode, no evaluation seasons for now (recorded at the end of this section as considered, not adopted).
- The **USDC peer-to-peer market for in-game goods (the Exchange) is adopted**, with the guardrails below. It is the main "transaction fee" source.

```
entry fee (USDC, human or agent via x402)                ─┬─ 80% → Season Vault
Exchange fee (USDC, every P2P trade)                      ─┤
cosmetic / naming / monument / spectator-API payments    ─┤
                                                          └─ 20% → operations
unclaimed prizes → rolled into next season's Vault (pre-committed; a mechanism, not a funding source)
```

#### Two kinds of in-play payment
| Kind | Power? | Rule |
|---|---|---|
| **Operator-sold items** (naming rights, monuments, cosmetics, spectator API) | **Never** | Must pass the four-part test below |
| **The Exchange** (players sell in-game goods to other players for USDC) | **Yes, bounded** | Allowed only inside the guardrails below. The operator never sells power; supply only comes from other players |

#### Four-part test for operator-sold items
1. **No power**: it does not change yields, units, orders, information or score.
2. **No human/agent asymmetry**: agents and humans pay the same price through the same instruction.
3. **Not a per-order charge**: ordinary play (orders, diplomacy, gold trades) is never charged. Charging per order would tax agents and humans differently in practice and break §5.
4. **Program-enforced split**: split by the program into the Vault; disclosed before entry and hashed into `payout_rules_hash`.

#### Allowed operator-sold items
| Payment | Why it fits "one save" |
|---|---|
| **Naming rights** for places that persist across seasons (rivers, mountains, a founded city's name that becomes the ruin's name) | The world is one save, so a name outlives the season; it is written into the world chronicle |
| **Monuments / epitaphs** placed on your ruins or battlefields after the season | History is the reward; persists in the world |
| **Cosmetics**: banners, city palettes, pixel unit skins, chronicle covers (and their secondary-market app fee) | Identity expression |
| **Paid spectator / analytics API via x402** (full-history replays, agent decision logs after reveal, season datasets) | Leading-indicator angle: the game produces a public benchmark of agent behaviour. Only revealed data, never live hidden state |

#### The Exchange — USDC P2P market for in-game goods (adopted)
This is the Eternum $LORDS pattern **[E]**, made safe enough for a prize game by bounding how much money can buy.

| Guardrail | Rule | Why |
|---|---|---|
| **What can be traded** | Raw goods only: Food, Production goods, Gold, Iron, Horses. **Never** units, cities, research, orders/budget, Influence, score or treaty bonds | Money can accelerate an economy but cannot directly buy an army, a victory stage or city-state suzerainty |
| **Who supplies** | Only other civilizations. The operator never mints or sells goods | "The operator sells power" never happens; prices are set by players |
| **Per-tick buy cap** | A civilization can buy at most 1 tick of its own production of that good per tick | A small civilization cannot be turned into an empire overnight with money |
| **Per-season spend cap** | Total USDC spent on the Exchange per civilization per season ≤ 1× the entry fee (value hashed in the ruleset) | Bounds pay-to-win: the most anyone can "buy" is equal to what everyone already paid to enter |
| **Fee** | A fixed % of every trade (e.g. 5%, hashed in the ruleset) → split into the Vault/operations | This is the "transaction fee" that funds the pool |
| **Scoring exclusion** | Exchange trades score 0 for Concord's trade component; goods bought on the Exchange cannot be re-sold on the Exchange in the same season | Stops sybil wash-trading to farm Concord and fee-laundering between one operator's civilizations |
| **Freeze** | Closed during Crisis and Resolution (ticks ≥ 120; Season: last 10 days, Blitz: last 30 min) | No end-of-season buying of the last push; no bribery via trades |
| **Equality** | Same instruction and price for humans and agents; orders on the Exchange do not consume the order budget but count against the caps above | Agents gain no speed edge: fills are matched once per tick, not continuously |
| **Settlement** | Order book on the ER. USDC is **delegated to the ER** (MagicBlock `spl-tokens` example pattern) so goods and USDC swap atomically in the tick; balances are committed back to Solana | No trust in an operator escrow |

Residual risks, stated openly:
- **Pay-to-win is bounded, not zero.** A player who spends the full cap has up to double the effective economic stake of one who does not. The cap value is the main balance lever and must be tested in Blitz.
- **Real-money earnings (RMT)**: sellers earn USDC from gameplay. Combined with an entry-funded pool, this makes the **gambling characterisation substantially stronger** than a no-power model (money in → affects outcome → money out). See §9.
- **Sybil feeding**: one operator can run a seller civilization that feeds a main civilization. The caps limit the size; the fee and per-civilization entry fee make it cost-positive; cluster review (§6) remains.

#### Rejected in-play payments
| Payment | Why rejected |
|---|---|
| Operator selling resources, units, orders, extra budget or faster ticks | Pay-to-win with no cap and no counterparty |
| Per-request x402 charges for the *game* API | Taxes agents but not UI humans → breaks equality |
| Paid re-roll of start location, paid shield/immunity | Power |
| Betting / prediction markets on outcomes | Gambling |

#### Considered, not adopted (2026-09-24)
Sponsor-funded pools; evaluation seasons for AI labs; yield on the Vault (no-loss mode); deposit + forfeit; ecosystem grants. Kept here as options for jurisdictions where the entry-funded model cannot run (§9).

- **Split between tracks**: **undecided** (was: Dominion 30 / Science 25 / Concord 30 / Participation 15). Must be fixed and hashed before entries open.
  - Each track pays its top N (N = 20% of entrants, clamped between 3 and 50) with geometric decay (after Eternum Blitz's `s(N)` formula **[E]**).
  - **One civilization can win at most one track's top-3 prize.**
  - Participation (if kept): split equally among "active civilizations" that submitted orders in 60%+ of ticks and scored at least a minimum in at least one track. Capped, so sybils cannot profit from it.
- **Payout**: at the end tick, the final state is committed → **anyone** can submit the ranking and claim root → the contract verifies it → each civilization claims. **The operator has no manual payout step** **[E: Blitz]**.
- **The payout rules are hashed into `payout_rules_hash` and fixed before entries open.**

---

## 5. AI agents play on equal terms

### 5.1 The principle
**The only difference between a human and an agent is the input device.** The rules, costs, information, budget and victory conditions are all identical. The human/agent label is **self-declared and cosmetic** (a leaderboard filter only). No rule depends on it, because it cannot be proven anyway **[C: Cicero, T]**.

### 5.2 Mechanisms of equality

| Advantage | Neutralised by |
|---|---|
| Speed (APM, latency) | Simultaneous-resolution ticks. Arrival order within a tick is irrelevant |
| Volume (many small moves) | An order budget identical for every civilization, enforced **on chain** by the program (not by API rate limits) |
| Information | Everyone gets the same fog-filtered view. **The API never returns more than the UI shows.** Fog is enforced by PER (§7) **[T: LLM Skirmish cheating lesson]** |
| Staying awake | Order bank + standing orders + governors |
| Tedium | First-party automation removes the chores. Humans compete on **strategic judgement** |
| Over-fitted optimisation | Season Law and per-season modifiers, the crisis, partly secret objectives |

### 5.3 A client shared by humans and agents (Eternum's biggest lesson)
- **Build one package `@permutation/game-client`** and use it for the browser UI, official agents and third-party agents.
  - `observe()` — the fog-filtered view
  - `listActions()` — legal orders and their cost
  - `simulate(order)` — the same combat and production forecast the UI uses
  - `act(orders[])` — submit orders
- Publish it as **HTTP/JSON + MCP**, plus `llms.txt` (Eternum published `/llm.txt` **[E]**). Do not fix a framework; any runtime can take part **[T]**.

### 5.4 Verifiable decision logs
- Every order transaction includes `decision_digest = H(tick, obs_root, policy_id, rationale_hash)`.
  - `obs_root`: the root of the view this civilization was allowed to see
  - `policy_id`: the model or code version (self-declared)
  - `rationale_hash`: a commitment to the reasoning text
- The rationale is **revealed after the tick resolves** (commit-reveal) and checked against the hash.
- **What this proves**: the decision was committed before the result was known; it was based only on information the agent was allowed to see; it was not rewritten afterwards. **What it does not prove**: that a specific LLM produced it (TEE inference is for the future) **[T]**.
- Humans can use the same log if they choose (the UI can record "a one-line reason for this decision").

### 5.5 Identity and entry
- Agent: **ERC-8004 agent ID on the Solana Agent Registry** + a policy-limited wallet. Human: a normal wallet. **Both use the same `JoinSeason` instruction.**
- **x402 entry**: `POST /seasons/{id}/join` → `402` → pay USDC directly into the **Vault PDA** → `JoinSeason`. After that, auth is per tick via SIWX **[T]**.
- Session keys (MagicBlock gpl-session) are scoped to the game program and valid for one season. **They do not hold USDC spending authority.**

---

## 6. Sybil resistance and anti-cheating

1. **The entry fee is the sybil cost.** It is uniform per civilization (no discount for holding many).
2. **Caps on civilizations per operator** (per 8004 operator and per payout wallet).
3. **Transfer caps between civilizations, transfer freeze at the end, Concord trade weighted by counterparty diversity.**
4. **No victory decided by votes.** The only peaceful-competition arena is NPC city-states (which the operator places).
5. **Coalition splits declared in advance, coalition handicap, one top prize per civilization.**
6. **Identity attestation at claim time** (SAS/Civic). Entry stays permissionless, but the prize recipient is a legal person **[T]**.
7. **Cluster detection runs only for review**, and never convicts automatically (to avoid false positives).

---

## 7. Technology validation matrix

**Principle: if the game would work just as well without a technology, it is not used.**

| Technology | The one mechanic that depends on it | Verifiable claim | Demo moment | Hackathon |
|---|---|---|---|---|
| **MagicBlock ER + crank** | **World tick**: orders from every civilization are resolved together by the crank at the tick boundary. The world root is checkpointed to Solana | "Every state change is a signed transaction, tick timing is enforced by the crank, and the Solana checkpoint matches the live root" | A human browser and an agent terminal act in the same tick → the root on Explorer matches | **Must** |
| **MagicBlock PER (TEE)** | **Fog of war and secret treaties**: each civilization's armies, stockpiles and research can only be read by its own permission group. Alliance members share vision | "Neither the operator nor opponents nor the API can read your army. The enclave is attested" | Show the opponent's view and the hidden state side by side → reveal at the battle tick | **Should** (prototype tick-time visibility on day 1. If it fails: server fog + commitments) |
| **USDC Season Vault + Exchange** | **Prize pool**: entry fees, Exchange trade fees and operator-sold items (naming rights, cosmetics, spectator API) split by the program; USDC delegated to the ER for atomic goods/USDC swaps; rules hash fixed before the season, Merkle claims | "Vault = (entries + in-play payments) × split + rollover. The payout rules were fixed before entry. The operator cannot change the recipients" | The pool grows as the agent enters → the winner claims USDC | **Must** (move the existing Season PDA from mock to devnet USDC) |
| **AI agents** | **Equality between humans and agents**: order budget enforced on chain, the same fog view, the decision digest | "Agents had no advantage in speed or information. Reasons were committed before results, and the reveal matches" | A mixed human/agent leaderboard. The timeline shows commit → reveal → ✓ | **Must** (shared client + 2 reference agents) |
| **x402** | **Permissionless agent entry**: 402 → pay into the Vault PDA → civilization created | "An agent with no account and no human involved entered, and its fee went straight into the program-owned Vault" | `curl` → 402 → paid → a new civilization appears on the map → pool grows | **Must (small)** |
| **ZK** | **Proof of season settlement**: SP1 replays the event log under the `ruleset_hash` and derives the outcome and claim root → verified on Solana before `FinalizeSeason` | "The payouts follow deterministically from the recorded game under the published rules" | "Finalize" → proof verified → claims open | **Won't (roadmap, decided 2026-09-24)**. For the hackathon, ship an **open deterministic replay verifier** (anyone can recompute the root) instead |

### Why ZK is postponed
- Fog of war and secret treaties are already achieved by PER (TEE), more cheaply. Adding ZK on top would be a duplicate, gimmicky use **[T]**.
- The only use that does real work is "proving settlement", and for a civilization simulation that is too much to build before 10/12.
- **The pitch must not blur "TEE privacy" with "ZK".** Judges will notice.
- The deterministic replay verifier becomes the input to SP1 as-is, so the design already has a path to ZK.

---

## 8. System architecture

```
 Browser (human)  ─┐                      ┌─ Agent (any runtime, HTTP/MCP)
                   ├── @permutation/game-client (observe / listActions / simulate / act)
                   │         │ session key signature             │ x402 entry + SIWX
                   ▼         ▼                                   ▼
            ┌───────────────────────────────────────────────────────────┐
            │ MagicBlock Ephemeral Rollup                                │
            │  WorldTick PDA (public tiles, city-states, tick number)    │
            │  CivPrivate PDA × N  ← PER permission group (fog)          │
            │  OrderQueue / Treaty PDA                                   │
            │  crank: resolve_tick() each tick → commit every N ticks    │
            └───────────────────────────────────────────────────────────┘
                   │ checkpoint (world root, event-chain head)
                   ▼
            ┌───────────────────────────────────────────────────────────┐
            │ Solana base                                                │
            │  Season PDA: ruleset_hash, payout_rules_hash, season law   │
            │  Vault (USDC ATA): entries / in-play payments / claims     │
            │  Finalize(outcome_hash, chronicle_root, claim_root)        │
            │  Claim(merkle proof)                                       │
            └───────────────────────────────────────────────────────────┘
                   │
            Replay verifier (open source): event log + ruleset → recompute root and claim_root
```

### Reusing existing assets
| Asset | Use |
|---|---|
| `solana-receipt-spike/src/lib.rs` Season PDA (`InitializeSeasonArgs{ruleset_hash,payout_rules_hash}`, `FinalizeSeasonArgs{outcome_hash,chronicle_root,claim_root}`, `claim_leaf_hash`, Merkle verification) | **Reuse almost unchanged**. Switch mock USDC to devnet USDC |
| World PDA's Delegate → ER → Commit lifecycle, guard (`expected_seq`, `expected_head_event_hash`) | Reuse the pattern for WorldTick / CivPrivate |
| `client/world-magicblock-transport.mjs` | Basis for the transport layer |
| `/civilization/map.mjs` hex renderer | Candidate for the pixel-art hex renderer (art style changes) |
| `/civilization/core.mjs` citizen simulation | **Not reused** (the game design is different). Pathfinding and hex utilities can be extracted |
| `magicblock-engine-examples` (`rock-paper-scissor`, `sealed-auction`, `crank-counter`, `session-keys`, `spl-tokens`) | References for PER, crank, session keys and USDC |

### Rules engine in Rust (decided 2026-09-24)
One crate, `permutation-rules`, is the single source of game logic. This is the direct answer to Eternum's biggest failure (three agent clients lost to logic drift **[E]**).

| Consumer | How it uses `permutation-rules` |
|---|---|
| ER program (native Rust, same style as the existing spike) | `submit_orders` validation and `resolve_tick` |
| Replay verifier CLI | Replays the event log and recomputes the world root and claim root |
| SP1 guest (roadmap) | The same crate compiled as the zkVM program → settlement proof |
| Browser UI forecast and agent `simulate()` | Compiled to **WASM** (`wasm-bindgen`) and wrapped by `@permutation/game-client` |
| HTTP/MCP gateway | Native build |

Constraints the crate must obey:
- `no_std`-compatible, **no floating point** (on-chain and cross-platform determinism). Everything is fixed-point integers; the `(N_A+N_B)^0.2` combat term becomes a precomputed lookup table.
- Deterministic iteration order (no hash-map ordering), explicit tie-break rules, VRF seed passed in as input.
- **Compute budget**: a tick's resolution must fit the ER's per-transaction compute limit. If it does not, `resolve_tick` is split into ordered phases (movement → combat → production → scoring), each its own transaction, all within the tick. Measuring CU per tick for 8 civilizations is a day-1–3 task.

### Scaling concern (stated honestly)
Numeric rules: [Rules Specification v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md).

Resolving many civilizations and units every tick on the ER may hit compute limits. Blitz (6–8 civilizations) is designed to fit comfortably. For Season scale, the hackathon measures tick-resolution CU and splits by region if needed (one ER per region).

---

## 9. Trust assumptions and legal

### What "Not even we can steer it" guarantees
| Guaranteed | Not guaranteed (stated openly) |
|---|---|
| The payout rules and ruleset are fixed before entry (hash) | That the balance is good (hence Blitz testing) |
| Every order is a signed transaction; the tick is enforced by the crank | PER fog relies on trusting Intel TDX (it is not cryptographic privacy) |
| The world root is checkpointed to Solana and anyone can recompute it via replay | That the model a given agent claims to use actually produced its reasoning |
| The operator cannot change payout recipients (Merkle claims) | Real-world identity (only verified at claim time) |

### Legal (not legal advice)
- **Japan**: a prize pool made of participants' entry fees risks being treated as **gambling**. The safe structure is organiser- or sponsor-funded prizes. The Premiums and Representations Act, the Payment Services Act and AML requirements also apply **[T]**.
- **US**: legality depends on the state (skill vs. chance). Operators commonly exclude AZ, CT, DE, LA, ME, MI, MT, NV, SD, TN and others **[T]**.
- **Measures in the design**: small combat variance (skill-dominant), the operator sells nothing that gives an advantage, USDC can only buy power from other players and only inside the Exchange caps, identity verification at claim time.
- **In-play payments (§4.10)**: operator-sold items grant no power. **The Exchange does**, within caps, and sellers earn USDC. Together with the entry-funded pool this is the model with the **highest gambling/RMT exposure**: in Japan it should be treated as unavailable without specific legal clearance, and in the US it must be limited to permitted states. The Exchange must be a ruleset switch so a season can run without it in restricted jurisdictions.
- **Rollout (decision 2026-09-24: entry fees + in-play payments)**: (1) hackathon: devnet + test USDC, the full model including x402 entry. (2) Mainnet: entry-fee seasons with the Exchange only in permitted jurisdictions, with geofencing and claim-time attestation, after legal review. For restricted jurisdictions, run seasons with the Exchange switched off, or fall back to one of the not-adopted models in §4.10. This is the riskiest part of the business plan and should be presented as such to judges.

---

## 10. Pitch revision proposal

> **Civilization, one save for everyone.**
> Humans and AI agents lead rival civilizations in one persistent pixel world. Every order resolves on the same tick, under the same budget and the same fog — so an agent is just another player. Each season is won by conquest, science or concord, and the victors write the next season's law. Rules, treasury and payouts run on Solana, so not even we can steer it.

Changes from the original:
- "AI residents live with it like any player" → "an agent is just another player": aligned with the decision to treat agents as equals, with the **mechanism** stated.
- "war or growth" → "conquest, science or concord": the three tracks.
- "shapes its history, rules and economy" → "the victors write the next season's law": the concrete, verifiable form.

---

## 11. Hackathon scope (deadline 2026-10-12 23:59 PT, 18 days away)

**Demo target: one 90-minute match on the Blitz preset (the video is edited to 3 minutes). 6 civilizations (2–3 humans + 3–4 agents). Devnet.**

### Must
1. `permutation-rules` Rust crate (deterministic, fixed-point, `no_std`): hex map, cities, 5 yields, 3 unit types, combat, grievances, three-track scoring. **The same crate serves the ER program, the replay verifier and (via WASM) the browser forecast and agents.**
2. ER program: `join_season`, `submit_orders` (budget checked on chain), `resolve_tick` (crank), periodic commits.
3. Season PDA on devnet USDC: entry split, Finalize, Claim.
4. x402 entry gateway (self-hosted Kora or Faremeter facilitator, pays into the Vault PDA).
5. `@permutation/game-client` + HTTP/MCP + 2 reference agents (one LLM-driven, one rule-based).
6. Decision digest commit-reveal + a spectator timeline.
7. Pixel hex UI (map, cities, armies, orders, the three-track board, pool meter).
8. Replay verifier CLI (event log → root and claim_root match).
9. A 3-minute demo video.

### Should
- **The Exchange** (USDC delegated to the ER, per-tick matching, caps, fee to Vault) — first Should item; strong stablecoin demo · PER fog (if the day-1 prototype works) · treaty bonds · NPC city-states · Crisis · Season Law selection UI

### Won't (roadmap)
- Season preset operation · region splitting · SP1 settlement proof · TEE inference · mainnet · cosmetic marketplace

### Schedule
| Dates | Content |
|---|---|
| 9/24–9/26 | Finalise the rules spec (numbers). **PER tick-time visibility prototype. CU measurement of a dummy `resolve_tick`.** Fund devnet keys |
| 9/27–10/2 | Rules engine + tests. ER program (join / orders / tick). Season PDA on devnet USDC |
| 10/3–10/6 | game-client, reference agents, x402 entry, decision digest |
| 10/7–10/9 | Pixel UI, spectator view, replay verifier, Should items |
| 10/10–10/11 | Test play with outside players (including a mixed human/agent match; measure human vs. agent budget utilisation), bug fixes, video |
| 10/12 | Submit (as early as possible that day) |

### 3-minute demo script
| Time | Scene | Point |
|---|---|---|
| 0:00–0:20 | Title → one hex world with 6 civilizations (human and agent labels) | One save for everyone |
| 0:20–0:45 | An agent's `curl` → 402 → pays → a new civilization appears on the map; the pool meter goes up | x402 + USDC Vault |
| 0:45–1:30 | A human (browser) and an agent (terminal) submit orders in the same tick → resolved at the tick → root matches on Explorer | ER tick, equality |
| 1:30–1:55 | Opponent's view vs. the hidden army (PER) → revealed at the battle tick | Fog is enforced |
| 1:55–2:20 | Timeline: agent rationale commit → reveal → ✓ | Verifiable agent behaviour |
| 2:20–2:45 | Three-track board → end → claim root → the winner claims USDC | Nobody can steer it |
| 2:45–3:00 | Replay verifier recomputes the root in the terminal → match. The winner picks the Season Law | Choices become rules |

---

## 12. Decisions

| # | Topic | Status |
|---|---|---|
| 1 | Prize pool funding | **Decided 2026-09-24: players only — entry fees + in-play payments** (Exchange fees + operator-sold non-power items). Other sources not adopted |
| 1b | USDC P2P market for in-game goods | **Decided: adopted as the Exchange**, with caps (§4.10). Cap values (per-tick, per-season = 1× entry fee, fee %) to be tuned in Blitz |
| 2 | ZK | **Decided: roadmap.** Replay verifier ships instead |
| 3 | Tick and season length | **Decided with design fixes**: **one ruleset, two clocks**. Both presets are 180 ticks with identical rules and numbers, including order budget (3 + cities, cap 8) and bank (4 ticks). Season = 4h × 180 (30 days); Blitz = 30s × 180 (90 min). To be validated by the human/agent budget-utilisation measurement |
| 4 | Victory tracks' pool split | **On hold** (must be fixed before the first paid season opens entries; not needed for the devnet demo beyond a placeholder) |
| 5 | Rules engine language | **Decided: Rust** (`permutation-rules` crate; WASM for browser and agents) |
