# Conquest milestone: sim-balance check (game feel and balance)

- **Area:** sim-balance (territory contest before M2 money; owner decision 2026-10-01).
- **Tree read:** `codex/frontier` worktree `frontier-integ`, head `aae8617` (read only). DESIGN rev 3.1 §3, §4, §5.4–5.6, §6, §8; M1-CONTRACT v1.13 (I-17); M1-EXIT-NOTES.
- **Lab:** `scratchpad/frontier/conquest/lab/simbal/` — a copy of `frontier-sim` and `permutation-rules` with its own `CARGO_TARGET_DIR` (`lab/simbal/target`). Nothing in the repo was changed. No ports, no chain, no installs.
- **Status of every number below:** [sim] — the rules-v10 kernels (clash, siege bookkeeping, holdings, laurel index, faction index, pools, claims) run unchanged; **player behaviour is an assumption** (`frontier-sim/src/model.rs` plus the levers in §2). The lab's default build reproduces the repo simulator bit for bit (1k seed 1 digest `20ae1f1c287ccbbe` before and after every change).

## 0. Answer in one screen

1. **Under today's rules the map cannot move, and adding the M1-excluded features does not fix it.** With holdings 2–3, sieges, captures and occupation all switched on (the M0 simulator already plays them), 10k agents over 28 days change the controller of **0.2 Marches a day; 1% of Marches ever change hands; 0 provinces** (§3). Giving conquest every help I could find — occupation counting for the occupier, rallies of up to 4 hosts, control-aware targeting — gets **0.5 a day, 2% of Marches** at 10k and **nothing at all in 7-day seasons** (§4).
2. **"First holding never taken" (D9) is not what holds the map still.** Relaxing it makes the map move *less* (10k/28d: 0.3 vs 0.5 March changes a day), because a capture needs a free holding slot and most active wallets already own 3 holdings, while an occupation does not (§4.2). The real cause is the control rule: a March is 84 sites settled almost entirely by its homeland faction (98–99% of holdings sit in their own wedge), so flipping it means taking ~25% of its weight, one holding at a time.
3. **What moves the map: a capturable faction keep in every province, owned by no wallet** (the "K-model", §5). With the parameters proposed in §8 the faction map changes **12.8 Marches and ~100 provinces a day at 10k/28d (23% of Marches, 24% of provinces change hands at least once; 11% of provinces differ from a week earlier)**, **16 Marches a day at 10k/7d**, **8 a day at 1k/7d (42% of Marches change hands)**, and in a 1,000-bot 7-day season like the M1 exit **10 a day (46%)**. D9 stays: no wallet loses anything when a keep falls.
4. **No snowball, no stall.** Leader province share at the end: ≤ 17.2% at 10k and ≤ 18.3% at 1k (worst seed 20.1%); the weakest faction ≥ 15%. A faction with **3× the members** ends with 18–21% of provinces under keeps, against **28–31% under the settlement map** today — keeps *compress* size advantages. A "guild" faction with ×3 aggression and +0.3 decision quality gains nothing measurable. Days without any March flip: 0% at 10k, 6% at 1k/28d, 22% at 1k/7d (§6).
5. **Doctrines: 6 of 6 in the band (1,500 paired seasons, confirmation seeds 10001–10250) as long as keeps do not score Dominion** (largest gap 1.9 points, E at 18.6% near the upper edge). **If captured keeps add Dominion, the band breaks: 1 of 6** — B Tide 30.7%, F Iron 24.9%, A 8.2% — because keep-taking rewards march speed (slowing the two cavalry lines hands the edge to E Lumen's faster marches instead; a siege horn delay does not help). So in this milestone the keep map is the faction map and Dominion stays DESIGN §5.6's March control by holding weight (occupations counting for the occupier); scoring keeps needs a doctrine re-tune on the conquest simulator first (owner decision, §7.1). **Bot criterion:** worst cell **0.986** with keeps (0.985 without; M0 control on the same held-out seeds 30001–30003: 0.985) → passes, margin unchanged at ~1.4 points (§7.2). Keeps pay no laurels, Works or goods, which is why.
6. **Proposed defaults** (§8): keep siege **72 bells** (12 h, no vigil), **48 h** consolidation after a change of hands, home keeps open with a **100-troop** guard, the capturing host leaves **50%** as garrison, heartland keeps safe until M3, rallies bounded by the existing 4 arrival slots per faction, keeps worth no points (map only; Dominion stays §5.6). Most sensitive knob: the opening guard (300 → movement halves in 7-day seasons; 1,000 → the map freezes).

## 1. What was measured, and how

**Map-movement metrics (new, `src/conquest.rs`),** taken at each day's end from the control layer:

| Metric | Definition |
|---|---|
| March / province controller | Weight model (DESIGN §5.6): the faction holding ≥ 50% of the strength weight of live holdings there; occupied holdings count for the occupier when that lever is on. Keep model: a province is its keep's faction; a March needs > 50% of its open provinces' keeps |
| changes/day | controllers that differ from the previous day's end (first settlement of empty land excluded) |
| ever changed hands | share of Marches (provinces) that had ≥ 2 different controllers during the season |
| changed vs 7 d before | share of controlled provinces whose controller differs from a week earlier (net movement, not ping-pong) |
| re-taken < 3 d | share of keep captures that hand the keep back to the faction that lost it less than 3 days earlier (ping-pong) |
| max/min province share | largest/smallest faction share of controlled provinces at the season's end (snowball / elimination) |
| days w/o March flip | days from day 3 with no March changing from one faction to another (stall) |

**Seeds and scale.** 1,000 agents × 8 seeds and 10,000 agents × 4 seeds per cell, 28-day and 7-day seasons (7-day: joins over the first 75% of the season, terms cut short; conservation checks pass in every season run). A 10k season takes 5–6 s.

**Faction mix.** The M0 default (passive 15%, casual 45%, daily 30%, skilled 9%, very skilled 1%, 5% scripted bots, 0.5% Shades), equal faction sizes unless stated.

## 2. Lab changes to the simulator

All behind `--cq key=value,...`; the default build is the repo simulator byte for byte.

| Lever | Kind | What |
|---|---|---|
| `floor` | **sim bug fix** | A siege host is at least `MIN_HOST_TROOPS`. In the repo simulator **87% of the sieges that passed every check were silently dropped** (661 of 757 at 1k): the host size asked for a weak target was under 100 troops and `send` refused it. Free City and weak-target sieges were under-counted in every M0/M1 run. **Should land in `frontier-sim` main.** |
| `occ` | rule | An occupied first holding's strength weight counts for the occupier's faction (control and Dominion) — the D9-compatible way occupation can colour the map |
| `first` | rule | D9 relaxed: a completed siege of a first holding outside the heartland captures it |
| `rally=N` | behaviour | Up to N faction-mates' hosts (from holdings within march range) join a siege — a Company or Warden rally point; N ≤ 3 because a faction has 4 arrival slots per province-bell |
| `target` | behaviour | Attackers prefer targets that would tip a March to their faction |
| `keeps` + `k*` | rule (K-model) | §5 |
| `favf/favm/favq` | stress | One faction plays with aggression × m and decision quality + q |
| `--days N` | harness | Season length (7-day seasons) |

The keep needs one kernel constant changed in the lab copy: `clash::MAX_GARRISONS` 12 → 13 (the keep is a 13th garrison in its province). Diffs: `lab/simbal/out/frontier-sim-conquest-lab.diff`, `lab/simbal/out/rules-clash.diff`.

## 3. Why the map stands still today (M0 baseline, every M1-excluded feature on)

| | 1k / 28 d | 1k / 7 d | 10k / 28 d | 10k / 7 d |
|---|---|---|---|---|
| March changes / day | 0.3 | 0.1 | 0.3 | 0.2 |
| Marches ever changed hands | 6% | 1% | 1% | 0% |
| provinces ever changed hands | 1% | 0% | 0% | 0% |
| player captures + occupations / day | 1.0 | 0.1 | 1.1 | 0.1 |
| days without a March flip | 76% | 91% | 79% | 81% |

Where the attempts die (10k/28d, one season, launch fix on; `war()` called 200,046 times):

| Stage | Count | Note |
|---|---|---|
| no candidate in range | 111,068 | candidate holdings refused: **heartland 653,826**, **the attacker already has 3 holdings so cannot capture 570,813**, shielded 222,818, dormant first 162,409, already sieged/occupied 73,300 |
| target judged too strong (needs 3–4× its perceived defence plus the March's standing reinforcements) | 84,082 | 98% of the attempts that had a candidate |
| siege declared | 1,462 | mostly Free Cities |

And the settlement map is the faction map: at day 28, **99% of live holdings stand in their own faction's wedge** (10k; 97% at 1k). A March is 84 sites; a ≥ 50% weight majority built by dozens of homeland holdings does not flip because one holding is occupied.

## 4. Conquest of holdings alone (weight control), with every help

`W` = `floor, occ, rally=3, target`; `W-noD9` adds `first`.

| | 1k/28d M0 → W → W-noD9 | 1k/7d | 10k/28d | 10k/7d |
|---|---|---|---|---|
| March changes / day | 0.3 → 0.6 → 0.2 | 0.1 → 0.0 → 0.0 | 0.3 → 0.5 → 0.3 | 0.2 → 0.1 → 0.0 |
| Marches ever changed hands | 6% → 11% → 7% | 1% → 0% → 0% | 1% → 2% → 1% | 0% → 0% → 0% |
| provinces ever changed hands | 1% → 8% → 3% | 0% → 0% → 0% | 0% → 1% → 0% | 0% → 0% → 0% |
| holding captures + occupations / day | 1.0 → 7.6 → 3.9 | 0.1 → 5.1 → 4.5 | 1.1 → 21.5 → 24.8 | 0.1 → 21.3 → 42.2 |
| days without a March flip | 76% → 60% → 82% | 91% → 94% → 100% | 79% → 66% → 75% | 81% → 81% → 100% |

### 4.1 Reading

Holding sieges become frequent (21 a day at 10k) and people do lose and regain holdings, but the coloured map does not change: a besieged holding is one of ~40 in its March. Nothing short of a coordinated campaign of dozens of sieges in one March flips it, and the 4-slot arrival cap, vigils and the 36-bell siege make that a multi-day effort the defender's March answers by auto-reinforce.

### 4.2 D9 (first holding never taken)

Relaxing D9 is **worse for the map** in every cell and **doubles the homes lost** in 7-day seasons (42 vs 21 a day at 10k). A capture needs the attacker to have a free holding slot (≤ 3 per wallet) and 570k candidate checks at 10k fail exactly there; an occupation needs none. Under the keep model (§5), relaxing D9 adds 10–20% movement (10k/28d: 15.6 vs 12.8 March changes a day) at the price of 30 → 42 home losses a day in 7-day seasons. **Recommendation: keep D9.** The map should move through objects nobody lives in.

## 5. The keep model (K)

Each province gets one **keep** on the lowest-index passable non-site tile (the centre in practice — the terrain carve guarantees it is connected to every site and gate). It belongs to a faction, never to a wallet.

- **Opening:** every keep opens held by its wedge's faction with a small guard (`khome` troops). (A neutral-start variant was tested: no gain in movement, and 176 keeps are still unclaimed at the end of a 10k 7-day season.)
- **Taking it:** the same machinery as a holding siege. The keep is one more `Garrison` in the province's clash (walls on, it only retaliates, as `Combatant::City` does); progress +1 per bell while the declaring faction holds the hex (`GarrisonResult.holders`) and no defender stands on it; failure if the besiegers lose the hex after holding it, or never hold it within 72 bells. **No vigil** (no owner); `kbells` bells to complete.
- **On capture:** the keep changes faction; `kgar` of the capturing host stays as its garrison, the rest marches home. Then `kshield` bells of consolidation during which it cannot be besieged.
- **Defence:** the keep faction's holdings in the same March send 25% under the standing order (the 4 largest), as for a besieged holding; defenders in the hex stop progress.
- **Limits:** heartland keeps (rings 2–3 of the owner's wedge) cannot be besieged without War (M3). Supply: a keep with no holding of its faction within 3 provinces loses 1%/bell of its garrison (the design's supply rule; it never bound in the simulation because attackers only reach keeps within 2 provinces of their holdings).
- **What a keep gives:** control of the province on the map and March control by keep majority. Dominion from keeps was tested (`kdom` per held captured keep-hour) and is **not** proposed (§7.1). **No laurels, no Works, no goods, no production** — so taking land does not make a faction stronger (no snowball loop) and does not pay a script (bot criterion).

## 6. Results for the proposed parameters (`R`)

`R` = `floor, occ, target, rally=3, keeps, kbells=72, kshield=288, khome=100` (kgar 50%, heartland safe). Source `lab/simbal/out/cand.md` (seeds 1101–1108 / 1101–1104).

| | 1k / 7 d | 1k / 28 d | 10k / 7 d | 10k / 28 d | 1k bots / 7 d (M1-exit-like) |
|---|---|---|---|---|---|
| March changes / day | 8.2 | 5.3 | 16.2 | 12.8 | 10.1 |
| Marches ever changed hands | 42% | 47% | 19% | 23% | 46% |
| province flips / day | 27.8 | 29.0 | 110.1 | 102.2 | 42.6 |
| provinces ever changed hands | 36% | 51% | 22% | 24% | 47% |
| provinces changed vs 7 d before | – | 21.1% | – | 11.4% | – |
| keep captures / day | 28.2 | 29.3 | 111.8 | 102.8 | 48.2 |
| holding captures + occupations / day | 5.8 | 8.1 | 24.9 | 26.1 | 6.1 |
| re-taken by previous holder < 3 d | 42% | 45% | 39% | 45% | 38% |
| max province share at end (worst seed) | 18.0% (18.6%) | 18.3% (20.1%) | 17.0% (17.2%) | 17.1% (17.2%) | 18.0% (19.4%) |
| min province share at end | 15.5% | 15.1% | 16.2% | 16.0% | 15.4% |
| days without a March flip | 22% | 6% | 0% | 0% | 3% |

For comparison, the M0 baseline in the same table is 0.1 / 0.3 / 0.0 / 0.2 / 0.3 March changes a day.

**Rates the UI will see** (for the herald and the design chat): at 10k a keep changes hands about **4 times an hour**, a March about every 2 hours; at 1k a keep about every 50 minutes. In a 7-day season keeps start moving on day 1 (holding sieges only after the 48 h shields lapse, day 2+). Per-day curves: `out/curve_10k_28d.md`, `out/curve_1kbots_7d.md` (for the earlier guard of 300).

### 6.1 Snowball

| Stress | Faction 0 province share, Q1 → mid → end | max province share (worst seed) |
|---|---|---|
| 10k/28d, sizes 3:1:1:1:1:1, **M0 (settlement map)** | 31.7% → 28.6% → 27.7% | 27.7% (27.9%) |
| same, **R** | 19.1% → 18.9% → 18.2% | 18.2% (18.5%) |
| 1k/7d, sizes 3:1:1:1:1:1, M0 | 28.9% → 26.8% → 27.0% | 27.0% (28.0%) |
| same, R | 21.2% → 20.6% → 21.2% | 21.2% (22.1%) |
| 10k/28d equal sizes, faction 0 a "guild" (aggression ×3, quality +0.3), P (guard 300) | 16.8% → 17.4% → 16.7% | 17.0% (17.1%) |

No configuration produced a leader trending upward over the season (leader share Q1 → end within ±1 point at 10k in every home-start keep run). Why it holds: captured keeps add no strength; attackers must have holdings within 2 provinces; a faction lands at most 4 hosts per province-bell; heartland keeps are out of reach; the defender's March reinforces automatically. **Caveat:** simulated factions do not coordinate across Marches (no faction-wide offensives); with γ = 0.6 herding damping only on scores, a real coordinated faction could do better than the guild stress shows (§9).

### 6.2 Stall and dependence on coordination

| R with … (1k/7d; 10k/28d) | March changes/day | provinces ever changed hands | days without a March flip |
|---|---|---|---|
| rally ≤ 3 (R) | 8.2; 12.8 | 36%; 24% | 22%; 0% |
| rally ≤ 2 | 7.5; 11.3 | 37%; 23% | 12%; 0% |
| rally ≤ 1 | 5.6; 8.5 | 32%; 22% | 6%; 0% |
| no rally, guard 300 (stress set, equal sizes; 1k/28d; 10k/28d) | 0.9; 1.0 | 23%; 8% | 66%; 37% |

With the 100-troop guard the map moves even when every attacker acts alone; with the 300-troop guard it needs rallies (Company/Warden tools, Mandates "march to a named province").

### 6.3 Sensitivity (1k/28d; 10k/7d; 1k/7d — `out/sens.md`, `out/cand.md`)

| Knob | Values tried | Effect |
|---|---|---|
| opening guard `khome` | 100 / 200 / 300 / 1,000 | **dominant**: 1k/7d March changes 8.2 / 4.4 / 2.4 / 0.0; 10k/7d 16.2 / 10.0 / 4.8 / 0.2. 1,000 freezes the map (100% stall days at 1k/7d) |
| siege length `kbells` | 36 / 72 / 144 | small: 10k/7d 3.6 / 4.0 / 3.0 (guard 300); 72 chosen so a siege always spans waking hours in any time zone |
| consolidation `kshield` | 0 / 72 / 144 / 288 | ping-pong (re-taken < 3 d): 67–75% / 68–73% / 59–63% / 40–45%; movement unchanged or higher |
| garrison left `kgar` | 25% / 50% / 100% | 25%: more ping-pong (70%); 100%: fewer captures (−30%) |
| search radius (behaviour) | 2 / 3 provinces | 3 doubles movement (1k/28d provinces ever changed 47% → 71%) — reachability, i.e. travel and Waystones, is a second strong lever |
| supply decay | on / off | no effect (never binds) |

## 7. Balance gates

### 7.1 Doctrines (O5 band 16.7% ± 2 on 1,500 paired seasons)

Configuration `R` (keeps, `kdom` 0: Dominion = DESIGN §5.6 March control by holding weight, occupations counting for the occupier, plus captures), rules-v10 kernel table, 250 seeds × 6 rotations at 10k:

| Doctrine | R: win rate | Δ index (± SE) | R + keep Dominion (`kdom` 1000): win rate | Δ index | Dominion/cap ÷ civ |
|---|---|---|---|---|---|
| A Wardens of Stone | 15.5% | −0.043% (0.016) | **8.2%** | −0.708% | 0.974 |
| B Tide | 16.9% | +0.044% (0.017) | **30.7%** | +0.887% | 1.036 |
| C Ember | 17.2% | −0.030% (0.017) | **10.4%** | −0.426% | 0.983 |
| D Verdant | 15.5% | −0.066% (0.016) | **8.4%** | −0.474% | 0.982 |
| E Lumen | 18.6% | +0.079% (0.015) | 17.4% | +0.122% | 1.002 |
| F Iron | 16.3% | +0.015% (0.017) | **24.9%** | +0.599% | 1.026 |
| **in band** | **6 of 6** (gap 1.9 pt) | | **1 of 6** (gap 14.0 pt) | | |

1,500 of 1,500 seasons conserved in both. The M0 kernel table's record is 6/6 with largest |Δ| 0.043% (DESIGN §4.1); R's 0.079% is within the nightly ±0.12% bound but E sits 0.1 point under the band's top.

**Why keep Dominion breaks it** (proxies: 30 seeds × 6 rotations, seeds 20001–20030, `out/doct_proxy.md`):

| Variant (all with keep Dominion 1000 unless noted) | B Δ index | E Δ | F Δ | in band |
|---|---|---|---|---|
| as R | +0.99% | +0.05% | +0.59% | 1/6 |
| siege horn: progress only from 12 / 24 bells after declaration | +0.83% / +1.03% | | +0.71% / +0.62% | 1/6, 0/6 |
| keep Dominion 250 | +0.33% | −0.01% | +0.10% | 4/6 |
| keep Dominion 250 + horn 12 | +0.26% | +0.12% | +0.16% | 2/6 |
| B and F marching at Spearman speed (`travel` ×2) | −0.47% | **+0.90%** | −0.74% | 2/6 |
| B and F fielding Spearmen | −0.14% | **+0.64%** | −0.14% | 3/6 |

The edge follows **march speed**, not the unit: B and F (cavalry, ×0.5 march time) win while they are fastest, and E (Cartography, ×0.75) takes over the moment they are slowed. Faster hosts reach more keeps within the 3-province march limit and the session's window, so a score on keeps is a score on mobility. Options for the owner: (a) **default: keeps are the map, not a score** (R, 6/6); (b) score keeps and re-tune the mobility knobs (B/F cavalry, E's 0.75) with the doctrine gate — the proxy suggests the edge scales with the keep weight (250 → |Δ| ≈ 0.3%, still outside the ±0.2% CI proxy); (c) score keeps per March-control bell (the map's March colour) rather than per keep, which dilutes mobility; not simulated.

### 7.2 Bot criterion (best response, held-out seeds 30001–30003, 10k)

| Configuration | worst cell (bot's best choice) | margin under 1.0 |
|---|---|---|
| M0 (no conquest), same lab binary | 0.985 (1%, days 1–7, stake, bots in office) | 1.5 pt |
| W (holding conquest with every help) | 0.986 | 1.4 pt |
| **R (keeps)** | **0.986** | **1.4 pt** |
| R + Dominion from keeps (`kdom=1000`) | 0.985 | 1.5 pt |

Passes at 1, 2, 5 and 10% bots, with and without bots in office. The margin is the documented thin one (§2.7 of DESIGN: 0.980 on the default seeds); conquest does not move it, because the only laurel flows it adds are the existing occupation split and capture transfer, and keeps pay nothing.

## 8. Proposed parameters

| Parameter | Proposal | Evidence |
|---|---|---|
| Control layer (map colour, Dominion) | **province = its keep's faction; March = the faction holding > 50% of its open keeps** | §3–§5: the holding-weight rule cannot move at 10k |
| Keep siege | **72 bells (12 h) of holding the hex, no vigil**, same failure rules as a holding siege (lose the hex after holding it, or not hold it within 72 bells) | §6.3; 72 overlaps every time zone's waking hours |
| Opening guard of home keeps | **100 troops** (= one minimum host) | §6.3, the dominant knob; 300 halves 7-day movement, 1,000 freezes it |
| Garrison left by the capturing host | **50%** | §6.3 |
| Consolidation after a change of hands | **288 bells (48 h)** | ping-pong 60% → 40–45%, movement kept |
| Heartland keeps (rings 2–3 of the owner's wedge) | **not besiegeable** until M3 War decrees | governance default below |
| Supply | design rule (1%/bell beyond 3 provinces from an own holding) | backstop only |
| Rally | no new rule: the existing 4 arrival slots per faction per province-bell bound it; ship rally points (Warden) and "march to province X" Mandates as the coordination tools | §6.2 |
| What a keep pays | **nothing in this milestone**: no laurels, Works, goods or Dominion; the keep map is the faction map. Dominion stays §5.6 (March control by holding weight, an occupied holding counting for the occupier, plus captures) | §7.1 (keep Dominion breaks the doctrine band: 1/6), §7.2 |
| D9 | **keep it** | §4.2 |
| Holding sieges | unchanged (36 + walls/50 bells, vigil, occupation of first holdings, capture of 2–3) | they are the economic layer, not the map |
| Simulator | land the launch-floor fix; add the control metrics and the keep model to `frontier-sim` so the milestone's CI can gate "the map moves" | §2 |

**Governance defaults this milestone needs (M3 later):** relation between every pair of factions is **Rivalry**: keep sieges and holding sieges outside heartlands need no decree; **heartland keeps and heartland holdings cannot be besieged** (War, March hostility and truce are M3); no Peace/NAP/Alliance, so every other faction is hostile (the simulator already runs `Relations::ALL_HOSTILE`). Nothing in the measured movement depends on a decree.

**Suggested CI gate for the milestone** (one 10k/28d and one 1k/7d season set, 4 seeds): March changes ≥ 5/day (10k) and ≥ 3/day (1k/7d); ≥ 15% of Marches change hands; max province share ≤ 22% and min ≥ 12% at the end; days without a March flip ≤ 30%; plus the existing doctrine and bot gates. R passes each (the 1k/7d stall line most narrowly: 22% against 30%); M0 and W fail each.

## 9. What the simulation does not show (risks)

- **Coordination.** Agents rally locally (≤ 3 mates) but never run faction-wide campaigns. Real guilds might concentrate force on one front; the guild stress test (×3 aggression) moved nothing, but it does not model planning. Measure β and front concentration in the first playtest.
- **Engagement.** All results assume the M0 play profiles (aggression 0.15–0.6 per session). A population that does not fight leaves the map still; the 100-troop guard makes a single daily player enough to take an undefended keep, which is deliberate.
- **Ping-pong.** Even with consolidation ~40% of captures return a keep to its previous holder within 3 days. Fronts see-saw; net weekly movement is 11% (10k) – 21% (1k) of provinces. The map will look alive at borders and calm inside heartlands, which never change in this milestone.
- **Program cost.** The keep is a 13th garrison in a province's clash (`MAX_GARRISONS` 13) and a mirror in the Province account; ResolveFromInputs' 290k gate and the Province's 4,096 B need re-measuring by the program area. 100 keep captures a day at 10k add ~100 siege completions/day of keeper work.
- **Map and score disagree.** With keeps scoring nothing, a faction can hold the larger coloured map and still trail in Dominion (which reads holding weight). Players will notice; the UI should label the map "control" and the score "Dominion" until §7.1 option (b) or (c) is decided.
- **Event volume.** About 4 keep changes an hour at 10k plus ~25 holding sieges a day; the herald's control layer must be delta-encoded per province, and the spectator UI should summarise March flips rather than every keep.
- **7-day seasons.** Holding shields (48 h) and Frontier protection (7 d) are a large share of a 7-day season; holding sieges only start on day 2–5 (curve). Keeps carry the movement there.
- **Doctrines.** The band holds only because keeps do not score. The moment keeps (or anything else that rewards reaching places first) count for points, march speed becomes the best doctrine (§7.1). The doctrine gate must run on the conquest simulator from now on, and the gate's 180-season proxy should get a keep-Dominion negative control. R's E Lumen at 18.6% is 0.1 point under the band's top: re-run on fresh seeds before shipping.

## 10. Reproduce

```sh
L=scratchpad/frontier/conquest/lab/simbal; cd $L/frontier-sim
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"   # not needed for the sim
CARGO_TARGET_DIR=$L/target cargo build --release --offline
R=floor,occ,target,keeps,kbells=72,rally=3,khome=100,kshield=288
# movement table (CQ_SHORT=1 for the compact columns)
CQ_SHORT=1 CQ_SET="M0:;R:$R" $L/target/release/frontier-sim conquest --agents 10000 --days 7 --seeds 4 --first-seed 1100
# one season's per-day curve (+ CQ_SERIES=file.csv for the March controller per day)
$L/target/release/frontier-sim curve --agents 1000 --days 7 --seed 501 --bots 0.99 --cq $R
# gates
$L/target/release/frontier-sim criterion --best-response --seeds 3 --first-seed 30001 --cq $R
$L/target/release/frontier-sim doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --cq $R --gate          # 6/6
$L/target/release/frontier-sim doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --cq $R,kdom=1000      # 1/6
```

Outputs of this study: `lab/simbal/out/` (`main.md`, `cand.md`, `sens.md`, `snowball.md`, `criterion_M0_W.md`, `criterion_R.md`, `doct_R.md`, `doct_Rdom.md`, `doct_proxy.md`, curves, diffs).
