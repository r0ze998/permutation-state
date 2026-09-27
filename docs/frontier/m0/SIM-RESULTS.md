# Frontier M0: host balance simulator — results

- **Scope:** design §12 M0, "Host balance simulator for 10k agents incl. doctrines and faction-size scaling (β)"; §2.7 archetypes; §5.4 payouts; §5.6 herding (β, γ); D17 doctrines; the M3 acceptance idea "a scripted staking wallet at the SDK default returns < 1.0×".
- **Code:** branch `frontier/SIM` at `22fd708` (worktree `.claude/worktrees/frontier-SIM`), crate `frontier-sim/`. It is built on `codex/frontier` with `frontier/K1-world` (`9390a44`) and `frontier/K2-economy` (`4aeafe7`) merged in (`--no-ff`; `frontier/mod.rs` resolved by listing both module sets). Local commits only; nothing pushed; no servers started; no network use.
- **Date:** 2026-09-27. Machine: Apple M4 Max, 16 threads; rustc 1.89.0 (the repo's pinned toolchain).

**Tags used here**

| Tag | Meaning |
|---|---|
| **[measured]** | a property of the program itself, measured on this machine: run time, memory, determinism, the conservation checks |
| **[sim]** | output of the simulator: the real rules-v10 kernels driven by **assumed** player behaviour and placeholder in-game costs (listed in "Model"). A model of play, not a measurement of play |
| **[estimate]** | an extrapolation beyond what was simulated |

## Revision after the M0 review (2026-09-27, `codex/frontier` at `cea89be`)

**Every table in the appendix is now the output of the new kernels and simulator.** The command is `suite --agents 10000 --seeds 5`. It took 642 s on 16 threads [measured]. The output is also saved as `suite-output-review.md`; the previous run is kept as `suite-output.md`. Headline findings 1–8 and F1–F5 below were written for `22fd708`. Where they differ from this section, this section wins.

**What changed in the code** (the kernels, then the simulator):
- Clash engagements keep both halves of `resolve_engagement`: garrisons retaliate at v9's ×0.5 and ranged defenders at ×0.5.
- Hex fair share is allocated by side rather than by faction.
- A siege advances only while its declarer holds the hex.
- Terrain carves a path from every gate and site to the province centre.
- Late stakes count only the laurels banked after the stake.
- The Works weight is linear and capped at 140 Works per USDC of fee. It used to be `√Works`.
- Occupations and siege stakes follow the pair rule. A fee-only wallet transfers no laurels: occupation gives nothing, and an uncounted siege stake is burned.
- The simulator's default Works cap is now 20 a day (it was 60).

**Measured again [measured]:**
- Conservation: 1,575 of 1,575 settlements passed every check.
- Seed-1 digest: `c71a8a0eb7641b53` on the main thread and on a worker thread, replacing `99ad5f7d28bfb487`. Seed 2 differs.
- One 10k-wallet season takes 2.9 s and 35 MB RSS.

**Findings that changed [sim]:**
- **Finding 4 and F1: the bot criterion still fails at low bot shares.**
  - A staking bot returns 1.07× at 1%, 1.05× at 2%, 1.00× at 5% and 0.95× at 10%. A fee-only bot returns 1.01–1.03× (§D).
  - The old F1a result (a 20/day Works cap brings a fee-only bot to 0.92×) **does not carry over** to the linear, fee-capped Works weight. With it, the 60/day and 20/day rows of §G are identical: every bot reaches the fee cap (140 per USDC), whatever the daily cap.
  - Linear weighting removes the √N gain from splitting play across wallets. It does not remove the always-online wallet's edge in the Works pot.
  - If the owner wants the Works pot out of the bot edge, the next step is a lower `WORKS_PER_USDC`. Not run.
  - No Relic Sites plus order-weighted emission gives 0.98 / 0.96 / 0.93× at 2 / 5 / 10%.
- **Finding 4 holds only if scripted wallets never win paid offices.** In the "bots stand for office" variant, the sim's own rule (the most engaged wallet wins; bots eligible) gives:
  - Staking bots return 1.52× at 2%, 1.22× at 5% and 1.06× at 10%. Fee-only bots return 2.18 / 1.53 / 1.27×.
  - Very skilled stakers fall from about 1.61× to 1.00 / 0.94 / 0.89×.
  - Farms voting for each other were not modelled; this variant models eligibility only.
  - The design must choose: either bound officer pay so that an always-online wallet cannot profit from it, or publish the measured bot multiple.
- **Late AddStake.** Backdating is closed in the kernel: `CitizenRecord::counted_laurels`, test `late_stakes_do_not_reach_back`.
  - The review's lab variant measured bots that stake on day 21 at 1.61×. The in-sim variant now gives 1.14× at 2%, 1.09× at 5% and 1.04× at 10%, against a baseline of 1.05 / 1.00 / 0.95×.
  - The residual edge is the join-day effect of F2: a stake priced by days left against laurel accrual that rises late in the season. Pricing a late stake by expected remaining accrual would close it. Not implemented.
  - When every staker stakes late, very skilled stakers show 2.12×. Their multiple is claim ÷ paid, and they pay far less for the same steward pay.
- **Payouts by archetype (§A, bots 5%)**, as fee only / with stake: idle 0.51 / 0.24; casual 0.80 / 0.64; daily 0.87 / 0.81; skilled 1.00 / 0.90; very skilled 2.87 / 1.58; bot 1.02 / 1.00.
- Occupation laurels are now 0.0 per wallet in every archetype (§A.6). The sim's occupations are almost all of fee-only or idle owners, and those transfer nothing now.
- Herding (§C) and doctrines (§E) are within ±0.01 of the old tables: β = 0.86, ratio at m = 3 of 0.910 / 0.786 / 0.680 for γ = 0 / 0.3 / 0.6. The tuned doctrines put 3 of 6 in the band.

**Not done:**
- A "main with k alts" archetype. The kernel now makes transfers from fee-only or tied wallets zero (test `only_counted_laurels_move_between_wallets`). Two things remain: refugee kits minted per fresh alt, and captures feeding Dominion facts.
- A bot-farm voting model.

## Commands (exact)

```sh
cd /Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-SIM/frontier-sim
cargo build --release
cargo test --release            # 3 tests: conservation (3 emission modes), bit-for-bit replay, doctrines + unequal sizes
cargo test                      # same tests in debug (overflow checks on): 3 passed
cargo clippy --release --all-targets && cargo fmt --check

# every table below (appendix = this command's output, 485 s on 16 threads)
./target/release/frontier-sim suite --agents 10000 --seeds 5 \
  --out (session scratch)/scratchpad/frontier/m0/sim/suite-output.md

# one season with its full report (payouts, claim parts, join days, factions, laurel sources, stats, conservation)
./target/release/frontier-sim run --agents 10000 --seed 1                   # digest c71a8a0eb7641b53 at cea89be (99ad5f7d28bfb487 at 22fd708), 2.9 s
./target/release/frontier-sim run --agents 50000 --seed 1                   # 31 s, 197 MB
./target/release/frontier-sim run --agents 10000 --sizes 3,1,1,1,1,1 --gamma 3/10
./target/release/frontier-sim run --agents 10000 --bots 0.02 --emission order-weighted --no-relics --works-cap 20
./target/release/frontier-sim run --agents 10000 --doctrines --rotation 2   # or --doctrines-tuned
```

Kernel tests on the merged branch still pass: `cargo test -p permutation-rules --test frontier_economy --test frontier_world` (repo root) → 18 + 38 passed.

## Headline results

1. **Performance [measured].** A 10,000-wallet, 28-day season (4,032 bells, ~460k sessions, ~85k clashes through `resolve_clash`) takes **2.7–3.2 s** and 35 MB on one thread; 50,000 wallets take 31 s and 197 MB. The full suite (1,372 seasons settled 1,530 times, plus 3 determinism runs) takes 485 s on 16 threads.
2. **Conservation [measured].** Every settlement in the suite (1,530 of 1,530) passed every check: vault = paid − withdrawn; no escrow left pending; operator escrow = 20%; Σ claims + swept + dust = prize exactly (rounding dust 0.016 USDC in the seed-1 season); allocated ≤ prize; no claim above 5× paid; voided Shades paid 0; laurels held = laurels credited + relic laurels minted, exactly; reward-index rounding dust 0.04 laurel in the seed-1 season.
3. **Determinism [measured].** The same seed gives the same digest on the main thread and on a worker thread (`99ad5f7d28bfb487`); seed 2 differs. Same-seed runs are bit-for-bit identical across the suite.
4. **The M3 bot criterion fails at the SDK default when bots are few [sim].** A scripted wallet that stakes returns **1.07× at 1% bots, 1.04× at 2%, 0.99× at 5%, 0.95× at 10%**, 0.83× at 50% (§D). Late-joining bots do better: 1.21–1.27× for joins after day 0 at 5% bots. A "tuned" script (better decisions, more aggression) is no better than the default (1.00× at 5%): the edge is presence, not cleverness. What the edge is made of (§G): Relic Sites (worth ≈ 0.03–0.10×), the Works cap (a fee-only bot returns 1.01–1.03×; with a 20/day cap 0.92×), and the late-join effect of finding 4. With all three changed (no Relic Sites, Works cap 20/day, holdings emitting by order factor) bots return **0.90–0.93×** at 2–10% share while humans' multiples move by ≤ 0.02.
5. **Late joiners out-earn day-0 joiners in the laurel pool [sim]; the design expects parity.** Stakers who joined on day 0 get 0.67× their stake back from the laurel pool; days 1–14 get 1.07–1.08×; days 15–21 0.95× (§A.3). Cause (§A.4): late joiners settle in the newest rings, whose provinces are full of other wallets' second and third holdings (4.8 of 10.7 holdings are first holdings, vs 9.6 of 11.5 for day-0 joiners). Those holdings emit a full 1/12 laurel a bell but carry ¼–½ of the weight, so the first holdings among them (1.6–1.75× the province's mean weight) collect their emission. Making holdings emit by their order factor (a proposal, §B) narrows the gap to 0.72× vs 0.96×; not emitting at all (§G) closes it but hands the extra holdings' collecting to multi-holding wallets (bots 1.14× at 2%).
6. **Herding: β < 1 in the simulator, so the ratio is ≤ 1.0 at m = 3 for every γ ≥ 0 [sim].** Measured β = 0.86 on the undamped index (per path: Dominion 0.45, Prosperity 0.99, Knowledge 0.99, Concord 0.98). Dominion (March-control bells) strongly favours small factions: rings open when the crowded wedge fills, so small factions spread thinly and control more Marches per member. Herding ratio at m = 3: **0.913 (γ = 0), 0.787 (0.3), 0.682 (0.6), 0.566 (1.0)**; at m = 1.5: 0.972 / 0.915 / 0.862 / 0.797. If real players scale better than simulated ones, the minimum γ for ratio ≤ 1.0 at m = 3 is 0 (β = 1.0), 0.26 (β = 1.2), 0.53 (β = 1.4), 0.81 (β = 1.6) [estimate, §C.4].
7. **Doctrines: the draft table fails the CI band badly [sim]; a tuned proposal gets close.** Draft: **Lumen wins 99.5%** of 600 seasons (Knowledge per capita ×1.34 from +20% science and ×1.2 pledge weight); the other five win 0–0.5%. Tuned proposal (drop the direct multipliers on scored facts and on growth: Verdant's food, cheaper settlers and doubled grant; Lumen's science and Knowledge weight; Iron's ore): mean undamped index within ±0.27% for all six; win rates 12.0–21.2%, 3 of 6 inside 16.7 ± 2. With a per-season index spread of 1.4%, a 0.25% systematic edge moves a win rate by ~4 points.
8. **Payouts by archetype [sim]** (5 seeds × 10k, bots 5%, γ = 0.6; design §2.7 model in brackets): idle 0.51 fee-only / 0.24 staked [0.56 / 0.23]; casual 0.81 / 0.64 [0.75 / 0.40]; daily 0.86 / 0.81 [0.83 / 0.68]; skilled 0.96 / 0.89 [core 0.87 / 1.04]; very skilled 2.71 / 1.56 [whale 0.87 / 1.38]; scripted bot 1.01 / 0.99 [0.87 / 0.86]. The very skilled numbers are almost entirely steward pay (5.7–6.5 USDC of officer rows against 3.6–8.4 paid; §A.1), which the simulator assigns to the most active humans. Per seed, every key cell moves by ≤ 0.07.

## Model

**Run with the kernels (unchanged, `permutation_rules::frontier`):**

| Kernel | Used for |
|---|---|
| `geometry` | rings and wedges (`ring_provinces`, `ProvinceCoord::index/wedge/ring`), Marches (`march_of`), heartlands (`is_heartland` via `may_besiege`) |
| `terrain::generate_province` | every province's land and 12 sites, from a per-ring seed |
| `holding::Holding` | every holding: `found`, `touch_owner`, `settle`, `enqueue` (buildings, tier-ups, walls), `pay`, `credit`, `set_upkeep`, dormancy (`is_dormant`, `dormant_at`), release (`is_released`), shields; `duplicate_cost`, `troop_upkeep_per_hour` |
| `host` | `Stamina` (march cost, clash refill), `rout_survivors` (unrevealed arrivals), `supply_attrition` (Relic hosts out of supply), `strength` |
| `travel` | `open_ground_secs`, `earliest_arrival_bell`, `march_stamina`, `BELL_SECS` |
| `clash::resolve_clash` | every clash: arrivals (≤ 4 per faction and one per citizen, by mass), frozen residents, garrisons, barbarian camps (`NEUTRAL`), postures, `retreat_bps`, doctrine `dealt_bps`; the outcome drives troops, stamina, cooldowns, fates, siege reports |
| `stance` | postures (Hold default, committed stances, Disarray when a committed posture goes unrevealed) |
| `siege` | `Siege::declare/advance` per resolved bell with `BellReport` from the clash, `Vigil` (8-hour window at the owner's local midnight), `may_besiege` (shields, frontier protection, heartlands, seats), `auto_reinforce`, `completion` |
| `laurel` | one `RewardIndex` per province (`accrue`, `attach`, `detach`, `reweigh`, `settle`), `strength_weight`, `mandate_reserve_split`, `occupation_split`, `capture_transfer` + `PairHistory`, `transfer`, `siege_settle`, `SIEGE_STAKE`, `relic_credit` |
| `index` | `FactionFacts` (add; `remove` for revealed Shades), `faction_index`, `clamped_mean`, `herding`, `IndexParams` for γ |
| `pools` | `EntrySchedule::FRONTIER_28` prices by join day, `Pools::pay_pending/release/withdraw_pending`, `steward_rows` |
| `payout` | `tenure_units`, `CitizenRecord`, `FactionTotals` (add; `remove` for Shades), `settle`, `claim`, `Ledger` |

**Assumed [sim] (all in `frontier-sim/src/model.rs` and `sim.rs`):**

| Topic | Assumption |
|---|---|
| Season | 28 days × 144 bells; genesis rings 2–3; `OpenRing` by the crowding rule (55% for 72 h, then 65%, any wedge), ≤ 1 ring a bell, `R_MAX` 64 |
| Mix | humans: idle 15%, casual 45%, daily 30%, skilled 9%, very skilled 1% (rev2 §C2 shares); bots 5% of all wallets unless swept; Shades 0.5% (bot policy, voided at the Reckoning) |
| Joins | 60% on day 0 (spread over the day), the rest uniform on days 1–21; fee and stake from `FRONTIER_28`; stake opt-in idle 5%, casual 10%, daily 50%, skilled 90%, very skilled 90%, bots 90% (the idle/casual probes and the 10% non-staking bots and very skilled exist so every table cell has wallets) |
| Placement | first holding in the own wedge: day-0 joiners in the innermost ring with room, later joiners in the outermost (frontier cohort), overflow to adjacent wedges; second/third holdings within 2 provinces of the first, own wedge first, from Town tier, only while free sites ≥ 20% |
| Play | sessions: casual 1 on 3 days of 7, daily 1, skilled 2, very skilled 4 (local 08:00–24:00), bots every hour; actions per session 15/30/45/60/30; decision quality q 0.3/0.5/0.75/0.9/0.6 (estimate noise, stance play, retreat orders, target choice); aggression 0.15/0.3/0.5/0.6/0.5; thrift (payback and tier saving) 0.4/0.6/0.85/0.95/1.0; Mandate take-up 0.2/0.5/0.8/0.95/1.0; withheld reveals or postures 2%/1%/0.5%/0/0 |
| Economy | placeholder base production, 6 building types (quadratic duplicates), tier-up costs Town 1,000/600/300, City 4,000/3,000/1,500, Stronghold 12,000/10,000/5,000 (wood/stone/gold), garrison targets 300/800/2,000/5,000, troops 60 food + 20 ore + 10 gold per 100, walls 300 stone per 100 points, 100-troop militia at founding, starter kit ×(1 + day/7) capped at 3, Frontier Grant +50% for below-average factions |
| Works | explore 4 (first three always), camp win 10, pledge 2, Mandate 6; daily cap 60; builder threshold 400 pledged |
| Engine | stage k when Σ pledges ≥ k(k+1)/2 × 450 × settled holdings (calibrated so the five stages land on days ~6, 12, 15, 18, 21); six Relic Sites per stage, three rings inside the rim |
| War | all factions in Rivalry (no War decrees, so no heartland sieges); siege targets within 2 provinces; attackers need 3–4× perceived defence (the skilled add the March's expected reinforcements); auto-reinforce sends the 4 largest 25% shares as hosts; online defenders set postures and put a host on the hex; camps respawn daily in half the settled provinces (100–400 troops) |
| Offices | Ministers: 4 per faction per term among active skilled/very skilled humans, weighted by sessions; Wardens: per (March, faction) with ≥ 3 holdings, paid when ≥ 12 of its active holders vote (50% turnout), won by its most engaged human |
| Not simulated | Bourse, caravans, research tree, raids, tribute, delegation, diplomacy, War, Waystones and roads, path-level terrain (marches use open-ground time over ≤ 3 provinces), accusations and bounties, the drand seed (a per-bell sha256 of the run seed stands in) |

Numbers that depend mostly on these assumptions: absolute multiples (fee/stake split is fixed by the design, but the laurel distribution is driven by who out-weighs whom), the siege mix (PvP is rare: per 10k season ~1,160 Free City captures vs ~14 captures and ~12 occupations), Relic Site capture, and steward pay. Numbers that depend mostly on the kernels: conservation, the closed-form split among factions (γ, √s vs s), tenure, and the 5× cap.

## Findings and recommendations

**F1. Bot criterion (M3).** The design's claim (§5.4: a strength-1.5 bot returns 0.86×) does not hold in the simulator at low bot shares: the SDK default returns ≥ 1.0× below ~5% of wallets. Recommendations, each measured in §G: (a) cap Works lower (20/day brings a fee-only bot from 1.01× to 0.92× and leaves humans' fee-only multiples unchanged, since daily players earn < 20 Works a day); (b) do not pay Relic Site laurels to a single host holder by the bell (bots are online hourly and hold them: 72 relic laurels per bot vs 11 per very skilled human); pay them into the holder faction's Mandate reserve, or split by the holding faction's stakers; (c) fix the late-join effect (F2). With (a)+(b)+order-weighted emission: 0.93× at 2%, 0.92× at 5%, 0.90× at 10%.

**F2. Join-day parity.** Second and third holdings emit a full 1/12 a bell while carrying ¼–½ of the weight, and they cluster in the rings where late joiners settle. Proposal for K2: replace `RewardIndex`'s boolean `emits` with an emission weight in quarters (first 4, second 2, third 1), i.e. `emission_per_bell = Σ quarters × HOLDING_EMISSION_PER_BELL / 4`. The simulator implements it by replaying `accrue` on the index's public fields (`Emission::OrderWeighted`, `sim.rs::accrue`); it narrows the day-0 vs day-1–14 laurel-pool gap from 0.67 vs 1.08 to 0.72 vs 0.97. The remaining gap is placement (late joiners' first holdings out-weigh young extra holdings); settling late joiners in the innermost ring with room, or weighting the stake by expected accrual, are the next things to try.

**F3. γ.** In the simulator, joining a bigger faction never raises expectation even at γ = 0 (β = 0.86 because Dominion rewards spreading thinly). γ = 0.6 (the design value) costs a member of a 3× faction 32% per capita and stays ≤ 1.0 for β up to ~1.45 [estimate]; γ = 0.3 costs 21% and covers β ≤ ~1.2. Recommendation: keep γ = 0.6 until M3 measures β with people (Companies, Mandates and delegated command, which the simulator lacks, are exactly the coordination that could raise β); if the owner wants a softer penalty, γ = 0.3 is the smallest grid value with margin for β = 1.2.

**F4. Doctrines.** Any doctrine bonus that multiplies a scored fact (production, science, Knowledge weight, cheaper expansion) wins almost every season, because per-capita indices of 1,600-member factions vary by only ~1.4% between seasons. The tuned proposal (`DOCTRINES_TUNED`) keeps unit variants, stance bonuses, travel, walls and upkeep, and removes the rest; the remaining edges are Ember and Verdant +0.2–0.3% (their stance bonuses also win barbarian camps, whose loot feeds the economy) and Iron −0.26% (Knights cost extra ore and gold at muster). Either tune those three further or state the CI criterion on the mean index (e.g. |Δ| ≤ 0.5%), which the tuned set meets.

**F5. Other observations [sim].** 92% of idle wallets' first holdings are released after 10 days and become Free Cities, which active players then capture (~1,160 per 10k season); the Mandate reserve pays always-active wallets ~68 laurels a season (~11–13% of their laurels; 11% for bots) and is split equally per completer per term; officer pay concentrates on the most active humans and reaches the 10 USDC person cap for some very skilled players; PvP sieges against players are rare under these behaviour assumptions because the March's auto-reinforcement makes most holdings too strong to attack at a 3× margin.

**Kernel notes found while integrating (for K1/K2 owners):**
- `holding::Holding.order` is 1-based ("1 for a citizen's first holding") while `laurel::strength_weight(tier, garrison, order)` takes a 0-based order; the simulator passes `order − 1`. One convention would prevent a silent 2× weight error.
- `laurel::Tier` duplicates `holding::Tier`; the simulator maps one onto the other.
- `siege::auto_reinforce` returns every eligible donor in the March (dozens in a dense March) while a faction has 4 arrival slots per province and bell; the simulator sends the 4 largest. The kernel (or the program) should state which donors go.
- `index::pow_frac` documents `num ≤ den`, but γ > 1 (6/5, 3/2, 2/1) is used in §C and behaves correctly; the doc or an assert should say which is intended.
- `RewardIndex`'s boolean `emits` cannot express order-weighted emission (F2).

## Appendix: full suite output

The tables below are the verbatim output of `suite --agents 10000 --seeds 5` at `cea89be` (also saved as `suite-output-review.md`; the `22fd708` output is `suite-output.md`).

<!-- generated by frontier-sim suite: 10000 wallets, 5 seeds per cell -->

## A. Payout multiple by archetype and join day (baseline)

5 seasons × 10000 wallets, equal factions, bots 5%, Shades 0.5%, doctrines off, γ = 0.6. Pooled over seeds 1..=5. Multiples are Σ claims / Σ paid for the group [sim].

| Archetype | wallets | citizen fee only (x fee) | with laurel stake (x fee+stake) | stakers |
|---|---|---|---|---|
| idle wallet | 7090 | 0.51 | 0.24 | 324 |
| casual 3x/week | 21260 | 0.80 | 0.64 | 2112 |
| daily 30 min | 14175 | 0.87 | 0.81 | 7171 |
| skilled 1-2 h | 4255 | 1.00 | 0.90 | 3834 |
| very skilled 4 h | 470 | 2.87 | 1.58 | 418 |
| scripted bot | 2500 | 1.02 | 1.00 | 2250 |

Per-seed spread of key cells:

| Cell | per seed | mean |
|---|---|---|
| scripted bot, with stake | 1.009, 0.986, 0.994, 1.009, 1.003 | 1.000 |
| scripted bot, fee only | 1.014, 1.016, 1.016, 1.015, 1.015 | 1.015 |
| very skilled, with stake | 1.581, 1.568, 1.575, 1.579, 1.613 | 1.583 |
| skilled, with stake | 0.890, 0.895, 0.910, 0.907, 0.905 | 0.901 |
| daily, with stake | 0.812, 0.817, 0.811, 0.806, 0.812 | 0.812 |
| daily, fee only | 0.871, 0.872, 0.873, 0.873, 0.872 | 0.872 |
| casual, fee only | 0.795, 0.796, 0.795, 0.796, 0.796 | 0.796 |
| idle, fee only | 0.506, 0.506, 0.506, 0.506, 0.506 | 0.506 |

### A.1 Claim parts (USDC per wallet)

| Archetype | stake | wallets | paid | citizen pool | Civilisation Share | laurel pool | steward | total claim | multiple |
|---|---|---|---|---|---|---|---|---|---|
| idle wallet | no | 6766 | 3.37 | 1.70 | 0.00 | 0.00 | 0.00 | 1.70 | 0.51 |
| idle wallet | yes | 324 | 8.47 | 1.72 | 0.00 | 0.35 | 0.00 | 2.06 | 0.24 |
| casual 3x/week | no | 19148 | 3.38 | 2.21 | 0.48 | 0.00 | 0.00 | 2.69 | 0.80 |
| casual 3x/week | yes | 2112 | 8.41 | 2.21 | 0.47 | 2.67 | 0.00 | 5.34 | 0.64 |
| daily 30 min | no | 7004 | 3.35 | 2.45 | 0.47 | 0.00 | 0.00 | 2.92 | 0.87 |
| daily 30 min | yes | 7171 | 8.38 | 2.45 | 0.47 | 3.88 | 0.00 | 6.80 | 0.81 |
| skilled 1-2 h | no | 421 | 3.40 | 2.66 | 0.48 | 0.00 | 0.27 | 3.41 | 1.00 |
| skilled 1-2 h | yes | 3834 | 8.43 | 2.64 | 0.47 | 4.17 | 0.31 | 7.60 | 0.90 |
| very skilled 4 h | no | 52 | 3.61 | 3.05 | 0.51 | 0.00 | 6.79 | 10.35 | 2.87 |
| very skilled 4 h | yes | 418 | 8.35 | 2.83 | 0.47 | 4.32 | 5.60 | 13.22 | 1.58 |
| scripted bot | no | 250 | 3.36 | 2.94 | 0.47 | 0.00 | 0.00 | 3.41 | 1.02 |
| scripted bot | yes | 2250 | 8.34 | 2.91 | 0.47 | 4.95 | 0.00 | 8.34 | 1.00 |

### A.2 By join day

| Archetype | stake | day 0 | days 1-7 | days 8-14 | days 15-21 |
|---|---|---|---|---|---|
| idle wallet | no | 0.51 | 0.51 | 0.51 | 0.51 |
| idle wallet | yes | 0.24 | 0.25 | 0.26 | 0.31 |
| casual 3x/week | no | 0.80 | 0.80 | 0.80 | 0.80 |
| casual 3x/week | yes | 0.60 | 0.77 | 0.71 | 0.67 |
| daily 30 min | no | 0.87 | 0.88 | 0.88 | 0.88 |
| daily 30 min | yes | 0.75 | 1.00 | 0.97 | 0.89 |
| skilled 1-2 h | no | 1.00 | 1.01 | 1.01 | 0.97 |
| skilled 1-2 h | yes | 0.83 | 1.09 | 1.08 | 1.00 |
| very skilled 4 h | no | 3.19 | 2.25 (n=9) | 1.34 (n=4) | 1.00 (n=4) |
| very skilled 4 h | yes | 1.68 | 1.53 | 1.27 | 1.10 |
| scripted bot | no | 1.02 | 1.02 | 1.01 | 1.01 |
| scripted bot | yes | 0.90 | 1.23 | 1.31 | 1.15 |

### A.3 Laurels by join day (stakers, idle excluded)

| Join | stakers | laurels / wallet | per day available: all | holding share | Mandates | relics | other | laurels per staked USDC | laurel-pool claim / stake | first holding Stronghold |
|---|---|---|---|---|---|---|---|---|---|---|
| day 0 | 9323 | 525.5 | 18.77 | 16.18 | 2.39 | 0.20 | -0.01 | 87.6 | 0.67 | 100% |
| days 1-7 | 2185 | 725.8 | 30.31 | 26.48 | 2.67 | 1.15 | -0.00 | 141.6 | 1.08 | 98% |
| days 8-14 | 2110 | 512.9 | 30.18 | 25.10 | 2.99 | 2.10 | -0.00 | 141.1 | 1.08 | 52% |
| days 15-21 | 2167 | 264.2 | 25.95 | 21.79 | 3.28 | 0.88 | -0.00 | 123.0 | 0.94 | 0% |

### A.4 Where first holdings sit at T_end (stakers, idle excluded)

| Join | first holdings | attached holdings in its province | of which first holdings | own weight / province mean |
|---|---|---|---|---|
| day 0 | 9320 | 11.55 | 9.60 | 1.12 |
| days 1-7 | 2185 | 10.69 | 4.75 | 1.61 |
| days 8-14 | 2108 | 9.17 | 3.45 | 1.76 |
| days 15-21 | 2165 | 8.12 | 3.35 | 1.68 |

### A.5 First holding at T_end

| Archetype | Hamlet | Town | City | Stronghold | none (released) | holdings / wallet |
|---|---|---|---|---|---|---|
| idle wallet | 8% | 0% | 0% | 0% | 92% | 0.08 |
| casual 3x/week | 1% | 4% | 22% | 73% | 0% | 1.69 |
| daily 30 min | 0% | 1% | 18% | 81% | 0% | 2.14 |
| skilled 1-2 h | 0% | 1% | 18% | 80% | 0% | 2.32 |
| very skilled 4 h | 0% | 1% | 20% | 79% | 0% | 2.53 |
| scripted bot | 0% | 1% | 21% | 78% | 0% | 2.85 |

### A.6 Laurel sources, Works, sessions (per wallet)

| Archetype | laurels banked / wallet | holding share | occupation | relic | mandate | captures in | captures out | siege stakes net | works / wallet | sessions / wallet |
|---|---|---|---|---|---|---|---|---|---|---|
| idle wallet | 45.0 | 45.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| casual 3x/week | 349.1 | 328.0 | 0.0 | 0.4 | 20.8 | 0.0 | 0.0 | -0.0 | 60.3 | 10.7 |
| daily 30 min | 505.4 | 440.6 | 0.0 | 1.4 | 63.3 | 0.1 | 0.0 | -0.0 | 189.0 | 23.5 |
| skilled 1-2 h | 543.7 | 469.5 | 0.0 | 6.1 | 68.4 | 0.0 | 0.3 | -0.0 | 295.6 | 46.2 |
| very skilled 4 h | 566.2 | 484.0 | 0.0 | 13.9 | 68.7 | 0.1 | 0.5 | 0.0 | 418.8 | 91.3 |
| scripted bot | 642.2 | 502.2 | 0.0 | 71.9 | 68.1 | 0.1 | 0.1 | 0.0 | 466.5 | 537.6 |

### A.7 One season in numbers (seed 1)

Final ring 27, rings opened 26; sessions 458433; clashes 82781 (19818 engagements, 0 kernel refusals); sieges declared 1319 / completed 1261 / failed 44 (never held 27, lost 17); occupations 11, liberations 1, captures 23, Free City captures 1180; camps beaten 15906; routs 109; Disarray postures 0; Engine stages 5 on days [6, 12, 15, 18, 21]; Relic Sites 30 minting 57906 laurels; dormancies 5252, first holdings released 1438; 2nd/3rd holdings founded 7147; withdrawn joins 0.

### A.8 Conservation (seed 1)

| Check | result | detail |
|---|---|---|
| vault = paid - withdrawn | PASS | pools.total 50414497676 = paid 50414497676 - withdrawn 0 |
| no escrow left pending | PASS | pending 0 |
| operator escrow = 20% | PASS | operator 10082901549 of settled 50414497676 |
| claims + swept + dust = prize | PASS | claimed 40331581723 + swept 0 + dust 14404 = prize 40331596127 |
| allocated <= prize | PASS | allocated 40331596119 prize 40331596127 |
| wallet cap 5x paid | PASS | 0 claims above 5x |
| voided Shades paid nothing | PASS | 0 voided claims > 0 |
| laurels: held = credited + relic minted | PASS | held 46207490491220 sources 46207490491220 |
| reward index: credited <= emitted - orphaned | PASS | emitted 45512619000000, credited 45512618491220, orphaned 0, rounding dust 508780 base units |

Season wall time [measured]: 3.2–3.4 s per 10000-wallet season (release build, one thread each, 5 in parallel).

## B. Variant: holdings emit in proportion to their order factor

Same seeds and settings as A; a second holding emits ½ × 1/12 laurel a bell and a third ¼ × 1/12, matching the order factor of their weight [sim variant, a proposal; not the design].

| Archetype | wallets | citizen fee only (x fee) | with laurel stake (x fee+stake) | stakers |
|---|---|---|---|---|
| idle wallet | 7090 | 0.51 | 0.25 | 324 |
| casual 3x/week | 21260 | 0.80 | 0.65 | 2112 |
| daily 30 min | 14175 | 0.87 | 0.80 | 7171 |
| skilled 1-2 h | 4255 | 1.00 | 0.90 | 3834 |
| very skilled 4 h | 470 | 2.87 | 1.60 | 418 |
| scripted bot | 2500 | 1.01 | 1.03 | 2250 |

### B.1 By join day

| Archetype | stake | day 0 | days 1-7 | days 8-14 | days 15-21 |
|---|---|---|---|---|---|
| idle wallet | no | 0.51 | 0.51 | 0.51 | 0.51 |
| idle wallet | yes | 0.24 | 0.26 | 0.28 | 0.33 |
| casual 3x/week | no | 0.79 | 0.80 | 0.80 | 0.80 |
| casual 3x/week | yes | 0.63 | 0.71 | 0.69 | 0.67 |
| daily 30 min | no | 0.87 | 0.88 | 0.88 | 0.88 |
| daily 30 min | yes | 0.76 | 0.90 | 0.89 | 0.85 |
| skilled 1-2 h | no | 1.01 | 0.98 | 1.02 | 0.94 |
| skilled 1-2 h | yes | 0.86 | 1.02 | 0.99 | 0.94 |
| very skilled 4 h | no | 3.17 | 2.38 (n=9) | 1.34 (n=4) | 0.99 (n=4) |
| very skilled 4 h | yes | 1.75 | 1.42 | 1.12 | 1.05 |
| scripted bot | no | 1.01 | 1.02 | 1.01 | 1.01 |
| scripted bot | yes | 0.94 | 1.24 | 1.29 | 1.08 |

### B.2 Laurels by join day

| Join | stakers | laurels / wallet | per day available: all | holding share | Mandates | relics | other | laurels per staked USDC | laurel-pool claim / stake | first holding Stronghold |
|---|---|---|---|---|---|---|---|---|---|---|
| day 0 | 9323 | 432.6 | 15.45 | 13.35 | 1.91 | 0.20 | -0.00 | 72.1 | 0.72 | 99% |
| days 1-7 | 2185 | 498.5 | 20.83 | 17.31 | 2.11 | 1.42 | 0.00 | 97.2 | 0.97 | 99% |
| days 8-14 | 2110 | 355.5 | 21.02 | 16.79 | 2.24 | 1.98 | -0.00 | 97.8 | 0.97 | 52% |
| days 15-21 | 2167 | 188.0 | 18.60 | 15.51 | 2.40 | 0.69 | 0.00 | 87.5 | 0.87 | 0% |

### B.3 Where first holdings sit

| Join | first holdings | attached holdings in its province | of which first holdings | own weight / province mean |
|---|---|---|---|---|
| day 0 | 9320 | 11.56 | 9.60 | 1.12 |
| days 1-7 | 2185 | 10.77 | 4.81 | 1.61 |
| days 8-14 | 2109 | 9.28 | 3.53 | 1.75 |
| days 15-21 | 2167 | 8.00 | 3.28 | 1.68 |

## C. Herding: faction size, β and γ

Faction 0 has m× the members of each other faction (same archetype mix in every faction, stratified). 5 seeds per size, 10000 wallets. β is measured as `1 + ln(r_big / r_small) / ln m`, where r is the per-active-member path value (per path) or the undamped index `clamp(mean ratio)` [sim].

### C.1 β by path (mean over seeds; min–max)

| m | Dominion | Prosperity | Knowledge | Concord | index (undamped) |
|---|---|---|---|---|---|
| 1.5 | 0.55 (0.50–0.62) | 0.99 (0.95–1.01) | 0.98 (0.95–0.99) | 1.00 (0.97–1.02) | 0.89 (0.87–0.91) |
| 2 | 0.48 (0.46–0.51) | 0.98 (0.97–0.99) | 0.98 (0.96–1.01) | 0.97 (0.95–0.99) | 0.86 (0.84–0.88) |
| 3 | 0.44 (0.43–0.46) | 0.99 (0.98–1.00) | 0.99 (0.98–1.00) | 0.98 (0.97–0.99) | 0.86 (0.85–0.87) |

### C.2 Herding ratio: per-capita payout (Σ claims / Σ paid) of the big faction ÷ the small factions

| m | γ = 0.00 | γ = 0.30 | γ = 0.60 | γ = 0.80 | γ = 1.00 | γ = 1.20 | γ = 1.50 | γ = 2.00 |
|---|---|---|---|---|---|---|---|---|
| 1 | 0.994 (max 1.006) | 0.994 (max 1.006) | 0.994 (max 1.006) | 0.993 (max 1.005) | 0.993 (max 1.005) | 0.993 (max 1.005) | 0.993 (max 1.005) | 0.993 (max 1.005) |
| 1.5 | 0.972 (max 0.978) | 0.915 (max 0.921) | 0.863 (max 0.868) | 0.829 (max 0.834) | 0.798 (max 0.803) | 0.767 (max 0.772) | 0.725 (max 0.729) | 0.660 (max 0.664) |
| 2 | 0.944 (max 0.953) | 0.855 (max 0.864) | 0.777 (max 0.784) | 0.729 (max 0.736) | 0.685 (max 0.692) | 0.644 (max 0.650) | 0.588 (max 0.594) | 0.508 (max 0.513) |
| 3 | 0.910 (max 0.916) | 0.786 (max 0.790) | 0.680 (max 0.684) | 0.619 (max 0.623) | 0.564 (max 0.569) | 0.516 (max 0.521) | 0.452 (max 0.457) | 0.368 (max 0.373) |

### C.3 Stakers and fee-only citizens separately (big ÷ small)

| m | γ | fee only | with stake | s_big | s_small (mean) | h_big |
|---|---|---|---|---|---|---|
| 1.5 | 0.00 | 0.979 | 0.967 | 0.965 | 1.011 | 1.000 |
| 1.5 | 0.60 | 0.896 | 0.838 | 0.792 | 1.011 | 0.821 |
| 1.5 | 1.00 | 0.845 | 0.763 | 0.694 | 1.011 | 0.720 |
| 2 | 0.00 | 0.959 | 0.932 | 0.933 | 1.027 | 1.000 |
| 2 | 0.60 | 0.830 | 0.737 | 0.675 | 1.027 | 0.723 |
| 2 | 1.00 | 0.755 | 0.633 | 0.544 | 1.027 | 0.582 |
| 3 | 0.00 | 0.939 | 0.889 | 0.906 | 1.057 | 1.000 |
| 3 | 0.60 | 0.756 | 0.626 | 0.556 | 1.057 | 0.614 |
| 3 | 1.00 | 0.656 | 0.500 | 0.402 | 1.057 | 0.443 |

### C.4 If real players scale better than the simulated ones [estimate]

At m = 3 the payout ratio moves as h_big^α with α = 0.59 (fitted from the γ grid; α is between ½ for the citizen pool's √s and 1 for the laurel pool's s). With the simulated β = 0.86 and ratio 0.910 at γ = 0, a population with a higher β needs γ ≥ (ln r0 + α (β − β_sim) ln 3) / (α ln 2.25):

| assumed β | minimum γ for ratio ≤ 1.0 at m = 3 |
|---|---|
| 1.0 | 0.00 |
| 1.2 | 0.26 |
| 1.4 | 0.53 |
| 1.6 | 0.81 |

The undamped index stays within 0.902–1.072 in every herding run, inside the clamp [0.5, 2], so the clamp never binds here.

**Smallest γ on the grid with the ratio ≤ 1.0 at m = 3 in every seed: γ = 0/1 = 0.00.**

## D. Scripted bots: return vs bot share

Bot share of all wallets; the rest is the baseline human mix. 5 seeds per row, 10000 wallets. SDK default: decision quality 0.6, aggression 0.5, hourly sessions, never withholds; tuned script: quality 0.9, aggression 0.8 [sim].

| strategy | bot share | bot with stake | (min–max) | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | bots' share of laurels |
|---|---|---|---|---|---|---|---|---|---|---|
| SDK default | 1% | **1.07** | 1.01–1.10 | 1.03 | 1.64 | 0.93 | 0.83 | 0.88 | 0.80 | 4.2% |
| SDK default | 2% | **1.05** | 1.02–1.06 | 1.03 | 1.64 | 0.92 | 0.82 | 0.88 | 0.80 | 8.0% |
| SDK default | 5% | **1.00** | 0.99–1.01 | 1.01 | 1.61 | 0.91 | 0.81 | 0.87 | 0.79 | 17.9% |
| SDK default | 10% | **0.95** | 0.94–0.96 | 0.99 | 1.59 | 0.89 | 0.79 | 0.86 | 0.79 | 30.5% |
| SDK default | 20% | **0.90** | 0.89–0.91 | 0.95 | 1.56 | 0.88 | 0.77 | 0.84 | 0.78 | 48.7% |
| SDK default | 30% | **0.87** | 0.87–0.87 | 0.92 | 1.59 | 0.86 | 0.76 | 0.83 | 0.78 | 61.9% |
| SDK default | 50% | **0.83** | 0.83–0.83 | 0.88 | 1.59 | 0.90 | 0.74 | 0.82 | 0.77 | 79.3% |
| tuned script | 5% | **1.01** | 1.00–1.03 | 1.01 | 1.60 | 0.91 | 0.81 | 0.87 | 0.80 | 18.2% |
| tuned script | 10% | **0.96** | 0.95–0.97 | 0.99 | 1.58 | 0.88 | 0.79 | 0.86 | 0.79 | 30.8% |
| tuned script | 30% | **0.87** | 0.87–0.87 | 0.92 | 1.57 | 0.86 | 0.75 | 0.83 | 0.78 | 62.0% |

SDK-default bots at 5%, with stake, by join day: day 0 0.90, days 1-7 1.22, days 8-14 1.32, days 15-21 1.15.

## G. What drives the bot edge

SDK-default bots; each row switches one mechanism off [sim variants]. 5 seeds per cell, 10000 wallets.

| variant | bot share | bot + stake | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | late-join daily + stake (days 1-21) vs day 0 |
|---|---|---|---|---|---|---|---|---|---|
| design (baseline) | 2% | **1.05** | 1.03 | 1.63 | 0.93 | 0.82 | 0.88 | 0.80 | 0.99 vs 0.76 |
| design (baseline) | 5% | **1.00** | 1.01 | 1.61 | 0.91 | 0.81 | 0.87 | 0.80 | 0.97 vs 0.75 |
| design (baseline) | 10% | **0.95** | 0.99 | 1.59 | 0.89 | 0.79 | 0.86 | 0.79 | 0.96 vs 0.73 |
| no Relic Sites | 2% | **0.97** | 1.03 | 1.63 | 0.93 | 0.83 | 0.88 | 0.80 | 0.99 vs 0.77 |
| no Relic Sites | 5% | **0.95** | 1.01 | 1.61 | 0.92 | 0.82 | 0.87 | 0.80 | 0.98 vs 0.76 |
| no Relic Sites | 10% | **0.92** | 0.99 | 1.59 | 0.90 | 0.80 | 0.86 | 0.79 | 0.97 vs 0.74 |
| Works cap 60/day (the old default) | 2% | **1.05** | 1.03 | 1.63 | 0.93 | 0.82 | 0.88 | 0.80 | 0.99 vs 0.76 |
| Works cap 60/day (the old default) | 5% | **1.00** | 1.01 | 1.61 | 0.91 | 0.81 | 0.87 | 0.80 | 0.97 vs 0.75 |
| Works cap 60/day (the old default) | 10% | **0.95** | 0.99 | 1.60 | 0.89 | 0.79 | 0.86 | 0.79 | 0.96 vs 0.73 |
| holdings 2-3 do not emit | 2% | **1.12** | 1.03 | 1.66 | 0.91 | 0.81 | 0.88 | 0.80 | 0.84 vs 0.80 |
| holdings 2-3 do not emit | 5% | **1.05** | 1.01 | 1.61 | 0.90 | 0.79 | 0.87 | 0.80 | 0.82 vs 0.78 |
| holdings 2-3 do not emit | 10% | **0.98** | 0.99 | 1.58 | 0.88 | 0.78 | 0.86 | 0.79 | 0.81 vs 0.77 |
| holdings emit by order factor | 2% | **1.08** | 1.03 | 1.65 | 0.92 | 0.82 | 0.88 | 0.80 | 0.91 vs 0.78 |
| holdings emit by order factor | 5% | **1.03** | 1.01 | 1.61 | 0.90 | 0.80 | 0.87 | 0.80 | 0.89 vs 0.77 |
| holdings emit by order factor | 10% | **0.96** | 0.99 | 1.58 | 0.89 | 0.79 | 0.86 | 0.79 | 0.88 vs 0.75 |
| no Relic Sites + order-weighted emission | 2% | **0.98** | 1.03 | 1.63 | 0.92 | 0.83 | 0.88 | 0.80 | 0.92 vs 0.79 |
| no Relic Sites + order-weighted emission | 5% | **0.96** | 1.01 | 1.62 | 0.91 | 0.82 | 0.87 | 0.80 | 0.91 vs 0.78 |
| no Relic Sites + order-weighted emission | 10% | **0.93** | 0.99 | 1.60 | 0.90 | 0.80 | 0.86 | 0.79 | 0.89 vs 0.76 |
| bots add their stake late (day 21) | 2% | **1.14** | 1.03 | 1.63 | 0.92 | 0.82 | 0.88 | 0.80 | 0.98 vs 0.76 |
| bots add their stake late (day 21) | 5% | **1.09** | 1.01 | 1.61 | 0.91 | 0.81 | 0.87 | 0.79 | 0.97 vs 0.75 |
| bots add their stake late (day 21) | 10% | **1.04** | 0.98 | 1.59 | 0.90 | 0.80 | 0.85 | 0.79 | 0.96 vs 0.73 |
| every staker adds its stake late (day 21) | 2% | **1.05** | 1.01 | 2.14 | 0.96 | 0.84 | 0.87 | 0.79 | 0.92 vs 0.80 |
| every staker adds its stake late (day 21) | 5% | **1.01** | 1.00 | 2.12 | 0.95 | 0.83 | 0.86 | 0.78 | 0.91 vs 0.79 |
| every staker adds its stake late (day 21) | 10% | **0.96** | 0.97 | 2.12 | 0.94 | 0.81 | 0.84 | 0.78 | 0.90 vs 0.77 |
| bots stand for office | 2% | **1.52** | 2.18 | 1.00 | 0.90 | 0.82 | 0.88 | 0.80 | 0.99 vs 0.76 |
| bots stand for office | 5% | **1.22** | 1.53 | 0.94 | 0.87 | 0.81 | 0.87 | 0.79 | 0.97 vs 0.74 |
| bots stand for office | 10% | **1.06** | 1.27 | 0.89 | 0.85 | 0.79 | 0.85 | 0.79 | 0.95 vs 0.73 |

## E1. Doctrines: draft table of design §4.1

600 seasons: every doctrine in every wedge (6 rotations × 100 seeds), 10000 wallets, equal expected sizes with each wallet's faction drawn at random (so faction composition varies as it would in a real season). "Win" = highest s_k at the Reckoning. CI band: 16.7% ± 2 points [sim].

| Doctrine | win rate | in band | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |
|---|---|---|---|---|---|---|---|---|---|
| A Wardens of Stone | 0.0% | **no** | 0.9800 | 0.0144 | 1.0007 | 0.9912 | 0.9300 | 0.9978 | 0.796 |
| B Tide | 0.0% | **no** | 0.9805 | 0.0138 | 0.9998 | 0.9931 | 0.9323 | 0.9968 | 0.797 |
| C Ember | 0.0% | **no** | 0.9822 | 0.0135 | 1.0022 | 0.9941 | 0.9332 | 0.9994 | 0.798 |
| D Verdant | 0.5% | **no** | 0.9982 | 0.0157 | 1.0019 | 1.0313 | 0.9411 | 1.0185 | 0.805 |
| E Lumen | 99.5% | **no** | 1.0803 | 0.0152 | 0.9991 | 0.9905 | 1.3364 | 0.9951 | 0.846 |
| F Iron | 0.0% | **no** | 0.9804 | 0.0144 | 0.9994 | 1.0008 | 0.9279 | 0.9933 | 0.796 |

0 of 6 doctrines in the band. Binomial standard error of one win rate at this sample: 1.5 points.

## E2. Doctrines: tuned proposal (no direct multipliers on scored facts)

600 seasons: every doctrine in every wedge (6 rotations × 100 seeds), 10000 wallets, equal expected sizes with each wallet's faction drawn at random (so faction composition varies as it would in a real season). "Win" = highest s_k at the Reckoning. CI band: 16.7% ± 2 points [sim].

| Doctrine | win rate | in band | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |
|---|---|---|---|---|---|---|---|---|---|
| A Wardens of Stone | 18.2% | yes | 1.0005 | 0.0150 | 1.0013 | 0.9996 | 0.9996 | 1.0013 | 0.806 |
| B Tide | 16.8% | yes | 1.0003 | 0.0141 | 0.9997 | 1.0007 | 1.0013 | 0.9996 | 0.806 |
| C Ember | 20.2% | **no** | 1.0029 | 0.0136 | 1.0025 | 1.0027 | 1.0033 | 1.0031 | 0.808 |
| D Verdant | 18.7% | yes | 1.0021 | 0.0149 | 1.0006 | 1.0020 | 1.0022 | 1.0034 | 0.807 |
| E Lumen | 13.8% | **no** | 0.9988 | 0.0136 | 0.9993 | 0.9986 | 0.9987 | 0.9987 | 0.806 |
| F Iron | 12.3% | **no** | 0.9969 | 0.0146 | 0.9996 | 0.9975 | 0.9958 | 0.9947 | 0.804 |

3 of 6 doctrines in the band. Binomial standard error of one win rate at this sample: 1.5 points.

## F. Determinism

Seed 1 on the main thread: `c71a8a0eb7641b53`; seed 1 on a worker thread: `c71a8a0eb7641b53` (identical); seed 2: `7d475556f23f16ea` (differs: true).

## Conservation over the whole suite

Every settlement (each season × each γ) ran all conservation checks: **1575 of 1575 passed every check**.

Suite wall time [measured]: 642 s.
