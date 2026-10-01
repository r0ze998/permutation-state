# The Sixfold Frontier: conquest rules (kernel design for the territory-contest milestone)

- **Area:** rules (kernel changes in `permutation-rules/src/frontier`). **Milestone:** the territory contest, called **MC** here, built after M1 "First Bell" and before M2 money (owner decision 2026-10-01).
- **Base:** `codex/frontier` worktree `frontier-integ` at `aae8617`. Read: DESIGN rev 3.1 §3, §4, §5.4–§5.6, §6, §6.6, §8; M1-CONTRACT v1.13 §4–§10 and I-17; DECISIONS (D9, O-M1-25/T1, O5, U1–U10); M1-EXIT-NOTES. Kernels read: `siege.rs`, `laurel.rs`, `index.rs`, `geometry.rs`, `clash.rs` (garrison holders), `holding.rs`, `travel.rs`, `host.rs` (supply).
- **Rules followed:** the repo was only read. Experiments ran in a lab copy (`scratchpad/frontier/conquest/lab/rules-mapmove/`, own `CARGO_TARGET_DIR`), no services, no chain, no installs.
- **Status words:** [measured] = run in the lab copy of `frontier-sim` (numbers below); [sim-design] = what the simulator must model before a number is final; [design] = argued, not measured; **owner** = needs the owner's decision.

---

## 0. The answer in one page

**What the owner saw is real, and it is not only an M1 exclusion.** I put a control layer into a lab copy of the simulator (control = strict majority of strength weight, as DESIGN §5.6 defines it for Marches) and ran the season the design describes today (sieges outside heartlands, occupation, captures of holdings 2–3 and Free Cities, all in the simulator already). **The map still does not move:** in a 7-day, 1,000-wallet season 0–2 of ~205 settled provinces are ever controlled by a faction other than their wedge's, with 0–4 sieges in the whole season; in a 28-day, 10,000-wallet season 0.2–0.3% of provinces, with 11–13 occupations and 15–30 captures of player holdings against 1,208–1,259 Free City captures [measured, §1].

**Why.** (1) First holdings sit in their own wedge and carry weight 1.0 against 0.5 / 0.25 for holdings 2–3, so a province full of one faction's first holdings can only change colour if about half of them are occupied at once. (2) The simulator's holdings 2–3 go to the own wedge first. (3) Attackers look only two provinces around themselves and want 3–4× the defence, so player-against-player sieges are rare and mostly fail. (4) Nothing neutral stands between the wedges until dormant first holdings are released (day 10+).

**D9 is not the constraint: keep it.** Removing D9 (first holdings capturable) changed the share of provinces ever under foreign control by −1.2 to +0.4 points over five paired seeds (rec play: 14.0 / 9.1 / 11.0 / 11.4 / 10.6% with D9, 13.3 / 8.7 / 11.4 / 10.2 / 9.8% without) [measured]. "The first holding is never taken" stays.

**What moves the map, in order of effect** (7-day, 1,000 wallets, 5 seeds; "ever foreign" = share of settled provinces controlled by a non-wedge faction in at least one hourly fold) [measured]:

| Package | Ever foreign | Foreign at the end | Provinces that change colour after day 1 | Sieges completed | Occupations / captures of player holdings / Free City captures |
|---|---|---|---|---|---|
| M1 rules as the simulator plays them today | 0.0–1.0% | 0.0–0.5% | 0–1% | 0–3 | 0–2 / 0–1 / 0 |
| R without the outpost rule (holdings 2–3 nearest site, anywhere) | 3.0–6.4% | 1.9–3.8% | 66–71% | 78–86 | 0 / 0–1 / 70–81 |
| **R (recommended rules)** | **6.8–9.1%** | **3.0–5.7%** | **69–75%** | **70–82** | **0–1 / 0–2 / 60–78** |
| R + a conquest-playing bot policy (faction focus, 2× willingness, attack at 1.5–2× defence) | 9.1–14.0% | 5.7–8.3% | 68–77% | 109–137 | 12–20 / 5–9 / 69–105 |
| R + conquest bots, D9 removed | 8.7–13.3% | 4.2–7.2% | 70–75% | 101–142 | 0 / 7–10 / 63–106 |

So the map moves through **rules that create contestable land** (neutral Free Cities between and inside the wedges, holdings 2–3 as outposts where the faction is not yet in control) plus **bots that actually play the contest**. In a 28-day, 10,000-wallet season the same package gives 2.7–3.1% (rules) and 4.2–5.0% (with conquest bots) of provinces ever foreign, 68–73% changing colour [measured]: the map has ~9× more settled provinces and the seams and rim, where all contest happens, are a smaller share of it. That is the expected shape (stable homelands, moving fronts) and the join page should say so.

**The recommended rules (R), in one list** (all numbers pinned in §16):
1. **Control layer** (new kernel `frontier::control`): every final holding's strength weight counts for its owner's faction, **an occupied first holding's weight counts for the occupier's faction**, a Free City's weight counts for the neutral side, dormant holdings count until released. A province or March is controlled by the side with ≥ 50% of the weight **and strictly more than every other side**; otherwise it is contested. Province control is recomputed at every province write (the map colour, live per bell); March control is sampled hourly for Dominion (§3).
2. **Holdings 2–3 ("outposts")**: by site ticket, ring ≥ 4 (outside every heartland), within 3 provinces of one of the citizen's final holdings, first holding at Town or better, land gate 20%; **recommended (owner): only in a province where the citizen's faction holds < 50% of the control weight at filing** (§4).
3. **Free Cities**: one neutral Free City per province from ring 4 outward, on the same canonical site in every wedge, garrison 300, no vigil, no shield, no emission; captured as a holding 2–3; released first holdings also become Free Cities (§5).
4. **Sieges** as in DESIGN §6.3 and the `siege` kernel: declare (horn), 36 + walls/50 bells of progress outside the owner's vigil while the declaring faction holds the hex and no defender stands there, 72-bell start window, 5-laurel stake, fail on losing the hex; plus: at most 2 active sieges per citizen, one per target, refused if it cannot complete before `end_bell` (§6).
5. **Occupation** of a first holding (never taken): control weight and 50% of the holding's laurel credit to the occupier while the occupier's faction holds the hex, **at most 288 bells (48 h)**, then **288 bells of Respite** from new sieges; liberated by any bell the occupier's faction loses the hex; tribute deferred (§7).
6. **Capture** of holdings 2–3 and Free Cities: the holding (stores, buildings, tier; not its garrison or reserve) goes to the declarer, 25% of the victim's banked laurels with the pair rule, refugee kit unless the pair has a history; a capture siege reserves a holding slot; if the declarer still cannot take it the holding is razed to a Free City (§8).
7. **No governance (M3) defaults**: every faction pair is in Rivalry; **heartland sieges are disabled** (no War decree exists); no Peace, NAP, alliance, March truce or hostility; Seats never (§9).
8. **Everything ends with the season**: DeclareSiege refuses a siege that cannot complete before `end_bell`; outposts cannot be filed after `end_bell − 24`; nothing completes at or after `end_bell`; open sieges lapse with the stake returned; occupations end; the last hourly fold is the final map and goes to the Chronicle (§10).
9. **Laurels and faction scores are game points in MC** (no stake exists, so all banked laurels count; the pair rule still applies) (§11).

**Re-runs.** Items 1–6 and 8 change honest outcomes (Dominion path, laurel flows, holdings, sieges): the doctrine CI proxy gate, the nightly 1,500-season band and the bot criterion (best response) must be re-run on a simulator that models them (§13). Preliminary lab read (§18): the CI proxy gate passes on R (largest |Δ index| 0.086%, bound 0.2%), and the default-mix bot criterion passes with slightly more margin (0.933 / 0.962 against 0.944 / 0.966).

---

## 1. The design check

### 1.1 Method [measured]

A copy of `frontier-sim` and `permutation-rules` at `aae8617` in `lab/rules-mapmove/`, with lab-only switches read from the environment (`MM_*`, all off by default so the stock simulator is unchanged). Added:

- an hourly **control metric** inside the simulator's existing `FoldMarch` stand-in (`Sim::hourly_fold`): per province and per March, the side holding the majority of strength weight; "foreign" = a faction other than the province's wedge (for a March, the majority wedge of its seven provinces);
- rule switches: occupation counts for the occupier (`MM_OCC`), dormant holdings count (`MM_DORMCTL`), Free Cities count as neutral (`MM_NEUTRAL`), strict majority (`MM_STRICT`), one genesis Free City per province from ring 4 (`MM_GFC`), holdings 2–3 placement (`MM_EXPAND`: 0 = the simulator's own-wedge-first, 1 = nearest free site outside other heartlands within 3 provinces, 2 = foreign wedge first, 3 = the outpost rule), outposts from ring 4 (`MM_OUTPOST_RING=4`), occupation tenure and Respite (`MM_TENURE`, `MM_RESPITE`), D9 off (`MM_FIRSTCAP`);
- behaviour switches standing in for bots that play the contest: `MM_FOCUS=3` (each faction picks, daily, the three enemy-held provinces its members can reach in the largest numbers against their garrisons, a stand-in for Mandates; attackers also look at those within 3 provinces and value them 3×), `MM_AGGR=2` (twice the session's chance to look for a fight), `MM_MARGIN=0.5` (attack at 1.5–2× the perceived defence instead of 3–4×);
- 7-day seasons: `--days 7`, 60% of wallets join on day 0 and the rest on days 1–5 (`MM_JOINDAYS=5`, like the exit season's join close at bell 756).

Runs: 7 days × 1,000 wallets × seeds 1–5; 28 days × 10,000 wallets × seeds 1–2. Scripts `final7.sh`, `final7b.sh`, `matrix7.sh`, `matrixB.sh`, `matrixC.sh`, `matrix28.sh`; raw hourly logs `out/*.mm`, reports `out/*.report` (§17).

### 1.2 Findings

| # | Finding | Evidence [measured] |
|---|---|---|
| F1 | Today's rules and behaviour leave the map still. | 7 d: 0.0–1.0% provinces ever foreign, 0–4 sieges per season; 28 d: 0.2–0.3%, 1,375–1,388 sieges of which ~1,250 against Free Cities, 11–13 occupations, 15–30 captures. |
| F2 | **D9 is not binding.** | Paired seeds 1–5, R + conquest bots: ever foreign 14.0 / 9.1 / 11.0 / 11.4 / 10.6% with D9, 13.3 / 8.7 / 11.4 / 10.2 / 9.8% without; 28 d, 10,000 wallets (an earlier package with nearest-site outposts): 1.0% foreign at the end with and without D9. |
| F3 | Occupation moving control weight changes few provinces by itself (one first holding of ~12), but it is what makes a siege visible on the map's site layer and in contested provinces. | R + conquest bots (seeds 1–3) with and without `MM_OCC`: ever foreign 13.6 / 8.0 / 10.6% vs 12.5 / 7.6 / 10.2% (+0.4 to +1.1 points); with nearest-site outposts the counts are identical. |
| F4 | Genesis Free Cities give the contest something to take from day 2 and put neutral grey between the colours. | 7 d: 60–110 Free City captures per season (vs 0); 66–77% of provinces change colour after day 1 (most of it neutral → faction or contested → faction). |
| F5 | Where holdings 2–3 go is the largest rule lever. | 7 d ever foreign: M1 rules with the simulator's own-wedge-first 0–1%; with the other R rules, nearest free site anywhere 3.0–6.4%, **outpost rule 6.8–9.1%**, foreign wedge first (a behaviour, not a rule; seeds 1–3) 12.9–17.4%. |
| F6 | Bots must play the contest. | The simulator's default `war()` looks 2 provinces around the largest garrison and wants 3–4× the defence; on R, the conquest stand-in adds 0 to +6.4 points of ever-foreign provinces (paired seeds) and raises occupations + captures of player holdings from 0–2 to 17–28 per 7-day season. |
| F7 | The simulator's March fold has a tie bias. | `sim.rs` `hourly_fold`: `if v * 2 >= tot { control[m] = f }` over f = 0..5, so in a 50/50 March the higher faction index wins. The kernel rule must be strict (§3.2). |
| F8 | Concentrating the Free City as a heavy "bastion" (control weight 4–6) does not help. | 7 d: ever foreign 4.2–6.1% with weight 4 or 6 against 3.0–6.4% without; a bastion in wedge k is taken by faction k. Not adopted. |

### 1.3 What this means for the design

- Interiors stay their faction's colour; that is the homeland pull (§3.1, §4.1) working, not a bug.
- The contest is at the **seams, the rim and the Free Cities**. The rules can widen it (Free Cities everywhere outside heartlands; outposts that must go where the faction is not yet in control) but cannot make interiors flip without taking first holdings, which D9 forbids and which F2 shows would not help much anyway.
- For the demo (a 7-day, 1,000-bot season): with R and conquest bots, 9–14% of provinces turn to a foreign colour at some hour, 69–77% change colour at least once, 100–140 sieges complete. That is visible as moving fronts and grey patches being taken.

---

## 2. Kernel map

| Kernel | Change | Version | Outcome change |
|---|---|---|---|
| **`control` (new)** | Site side and weight, province and March controller, hourly sample, Dominion credit (§3) | `CONTROL_VERSION = 1`, added to `KERNEL_VERSIONS` | yes (Dominion path; new) |
| `siege` | `Option<&Vigil>` for Free Cities; `SiegeStatus::Lapsed`; `can_complete_before(…)`; `earliest_completion_bell(…)`; occupation state with tenure, liberation and Respite; `SiegeCheck` gains `occupied`, `respite_until_bell`, `declarer_shielded`, `target_final`; capture beneficiary and raze fallback; heartland always refused while `relation == Rivalry` (unchanged code) | `SIEGE_VERSION` 2 → 3 | yes |
| `holding` | `may_found_outpost` (ring, range, tier, land gate, outpost rule, slot count); refugee kit constant; capture transfer of a Holding (what moves, what is lost) | `HOLDING_VERSION` 2 → 3 | yes |
| `terrain` / `catalog` | `free_city_site(ring_seed, canonical P, Q) → site`, `FREE_CITY_GARRISON`, `FREE_CITY_MIN_RING` | `TERRAIN_VERSION` or `CATALOG_VERSION` +1 | yes (sites change) |
| `laurel` | No formula change. MC calls `occupation_split(credit, owner_staker = true, h)` and `capture_transfer(victim_banked, h)`, `siege_settle(ok, stake_counted = true, h)` because no stake exists (§11) | unchanged (v1) | no (the caller's inputs change in MC only) |
| `index` | Unchanged; Dominion facts come from `control` | unchanged | no |
| `clash` | Unchanged. A Free City is a site garrison with owner `NEUTRAL` (hostile to every faction, like the camp garrison); `GarrisonResult.holders` already reports which faction holds a site hex | unchanged (`CLASH_VERSION` 3) | no |
| `travel`, `host` | Unchanged; supply attrition applies to besieging and occupying hosts where the program implements it | unchanged | no |

`RULESET_HASH` changes with the new and bumped versions.

---

## 3. Control: provinces and Marches

### 3.1 Inputs: each site's side and weight

| Site state (Province site mirror) | Side | Control weight |
|---|---|---|
| free, released-free (legacy), reserved (rings 0–1) | none | 0 |
| provisional holding | none | 0 — **reason:** it may still be displaced; MarchRoll and Warden eligibility already count final holdings only (§2.2) |
| final holding, not occupied (dormant included) | owner's faction | `laurel::strength_weight(tier, garrison, order − 1)` |
| final first holding, occupied | **occupier's faction** | same weight |
| captured, settlement pending (§8.4) | captor's faction from the completion bell | weight as captor's order (2 or 3), garrison 0 |
| Free City | neutral (side 6) | `strength_weight(tier, garrison, 0)` (a 300-troop Hamlet Free City = 1.15) |

**Reasons.**
- *Strength weight, not counts:* DESIGN §5.6 defines control by strength weight; the same number already splits laurel emission, so a player sees one notion of "how strong is this site".
- *Dormant counts until released:* dormancy is a time rule (5 days without an owner action) and owner actions such as Harvest do not write the Province, so the Province cannot see it without a write; counting dormant holdings keeps the fold lag-free and deterministic. Release (`ReleaseDormant`, which writes the Province) turns the site into a Free City (§5). The simulator excludes dormant holdings today (`attached == false`): [sim-design] change.
- *Occupied counts for the occupier:* D9 keeps ownership; the army on the site holds the land. F3: it does not flip interiors, it makes sieges visible.
- *Neutral side in the denominator:* neutral land must be taken, not ignored; a lone settler next to a Free City does not control the province. The simulator excludes Free Cities from the March weight today: [sim-design] change.
- *Garrison factor at bell start:* the mirror's committed garrison (`Holding::walls_at(bell_start)` pattern), so actions during a bell do not change that bell's control.

### 3.2 The controller rule

```
controller(w[0..=6]) =
  Unsettled                if Σw = 0
  side s                   if 2·w[s] ≥ Σw and w[s] > w[t] for every t ≠ s   (s = 6 → Neutral)
  Contested                otherwise
```

- **Province control**: on the province's 12 sites, recomputed by every instruction that writes a site mirror or a garrison (ResolveFromInputs, SkipQuiet's applied pending changes, SettleTicket, Garrison, Muster, SettleSiege, ReleaseDormant, and the resolve that starts or ends an occupation). Walls are not in the weight. It is the map's colour, live at every resolved bell. It is **not scored**.
- **March control**: the same rule on the sum of the seven member provinces' weight vectors (DESIGN §5.6: "≥ 50% of the March's strength weight"). It is scored (Dominion).
- **Seats** (ring 1) show their faction; the **Concord** (ring 0) shows neutral. Neither takes part in Marches' Dominion (no holdings there).

**Reason for strict majority:** §5.6's "≥ 50%" leaves a 50/50 tie undefined; the simulator resolves it by faction index (F7). "≥ 50% and strictly ahead of every other side" is identical to §5.6 everywhere except exact ties, which become Contested.

### 3.3 Sampling, folding and lag

- **Sample:** March control for hour k is computed from the seven provinces' weight vectors **as of `bell_start(6k)`** (after the clash of bell 6k − 1 and its pending changes). Hourly = DESIGN §5.6 ("FoldMarch(m) hourly"). **Reason:** an hour is short enough for a demo map to move within a session and long enough that one-bell garrison swings do not dominate Dominion.
- **Fold:** `FoldMarch(m, through_hour)` (class D, permissionless) credits each hour once, in order, from the provinces' samples. A run of hours whose seven samples are unchanged is credited in closed form (like `SkipQuiet`).
- **Lag only waits (§8.4):** the province keeps its weight vector for each hour boundary it crosses until the March has folded that hour; a Province that would have to drop an unfolded sample **refuses to resolve** (`MarchFoldPending`) instead. So a held FoldMarch delays the Province, never changes a credit. [design; the program area sizes the ring: 12 hours × 7 × u32 = 336 B, or a change log, see §12.]

### 3.4 What control gives (MC)

| Effect | Value | Reason |
|---|---|---|
| **Dominion**, per folded hour | the March's controller faction gets **6 control-bells** (one per bell of the hour) | DESIGN §5.6 "March-control bells"; simulator `hourly_fold` (6,000 milli-units per hour) |
| **Dominion**, per capture of a holding 2–3 or a Free City | **36 control-bells** to the captor's faction | simulator `capture()` (`facts[0] += 36 × 1000`); 36 = the minimum siege, so a capture is worth what holding a March for the length of a siege is |
| Per-citizen attribution (for M3 Shade voiding and the member view) | each hour's 6 control-bells are split among the controller faction's holdings in the March by weight, lazily: the March keeps a per-faction "control-bells per unit weight" index, each holding credits `weight × Δindex` at its next touch | the laurel reward-index pattern (§5.4); no per-member loop in the fold |
| Map colour (province) and Marches layer | as §3.2 | — |
| Anything else (combat, production, emission, supply, travel) | **nothing** | a combat or economic bonus to the controller would feed back into holding control (snowball); O5's rule against multipliers on scored facts applies in spirit; it keeps the re-runs about Dominion only |

Dominion enters the faction index per active member, unchanged (`index` kernel). In MC, Knowledge and Concord have no facts yet (M3); `per_capita_ratio` returns 1.0 for a path nobody scored, so the MC index is `clamp(mean(Dominion, Prosperity, 1, 1), 0.5, 2) × h_k`.

---

## 4. Holdings 2–3 (outposts)

| Rule | Value | Reason |
|---|---|---|
| Count | at most 3 holdings per citizen (`MAX_HOLDINGS_PER_WALLET`), counting final + provisional holdings, open outpost tickets and active **capture** sieges (§8.1) | existing cap; a capture siege must have a slot to land in |
| Prerequisite | first holding final and at **Town** or better; a third holding needs the second final | simulator `expand()` (Town); finality rule I-29 |
| Cost | settler cost `duplicate_cost(SETTLER_COST, n − 1)` from the anchor holding's stores, escrowed at filing, refunded if the ticket expires or is exhausted | existing kernel cost; simulator constants (800 food, 800 wood, 400 stone, 200 gold × 1.5 / × 3) |
| Placement | **ring ≥ 4** (outside every faction's heartland, including the own) | heartlands are where sieges are impossible: a holding 2–3 there could never be captured, contradicting "2–3 capturable"; heartland land is the first-holding supply (60 sites per faction at 1,000 players) |
| Range | within **3 provinces** of one of the citizen's own final holdings (named in the ticket) | the simulator's `MAX_MARCH_DIST` and the supply range; **not** modified by Verdant's "+1 supply range", so no doctrine gains a new lever [design] |
| Land gate | only while folded free sites ≥ 20% of open sites | DESIGN §3.4 |
| **Outpost rule (recommended; owner)** | only in a province where the citizen's faction holds **< 50% of the control weight** at filing (an empty province qualifies) | F5: 6.8–9.1% of provinces ever foreign against 3.0–6.4% without it; keeps outposts at fronts and neutral land instead of stacking safe interiors. Cost: less freedom for players who want a compact cluster |
| Placement mechanism | the site-ticket cohort of §2.2 (≤ 3 preferred sites, lottery by the bell's seed, displacement inside a cohort, finality by cohort) with the outpost checks at filing | speed wins nothing, as for first holdings; one mechanism for keepers and verifier |
| Shield | 48 h (72 h for holdings founded after day 7), as every new holding (§6.6) | unchanged design; a shielded holding's hosts cannot target other factions' holdings (Reveal step 6), so an outpost cannot be used as a shielded siege base |
| Emission and weight | order factor 0.5 / 0.25 (K3) | unchanged |
| Pair tickets | **not in MC** (M3) | they place first holdings next to friends; they do not move control and need a two-citizen cohort rule |
| Season end | no outpost ticket after `end_bell − 24` | a cohort closes within 24 bells, so a filed ticket always settles before the end (§10) |

---

## 5. Free Cities

| Rule | Value | Reason |
|---|---|---|
| Genesis Free Cities | **one per province of ring ≥ 4**, created by `OpenProvince` | outside every heartland; F4: they give the contest neutral land from day 2 |
| Site | `free_city_site = H("PSF-FREE-CITY" ‖ ring_seed ‖ canonical P, Q) mod site_count`, computed on the canonical (wedge-0) copy | every wedge gets the same Free City at the same ring (§3.2's symmetry) |
| Garrison | **300 troops**, Hamlet, walls 0, no regrowth | the simulator's `FREE_CITY_GARRISON` and a Hamlet's garrison target; beatable by one Town host, not by a newcomer's first 100-troop host |
| Released first holdings | become Free Cities (garrison 300, the released holding's walls kept) | DESIGN §3.4 ("becomes a Free City site"); M1's `released-free` state is replaced |
| Vigil, Shield, Frontier protection, heartland | none | no owner; `may_besiege` already accepts a Free City before those checks |
| Emission | none (neutral holdings do not emit) | simulator; laurels stay a zero-sum player emission |
| Control | neutral side (§3.1) | — |
| Capture | becomes a holding 2–3 of the captor (order 2 or 3), no laurel transfer, no refugee kit; walls reset to 0 except for Iron (doctrine F: "captured Free Cities keep their walls") | DESIGN §5.4 item 3, §4.1 |
| Capacity | one site of twelve (8.3%) in ring ≥ 4 is not settleable by tickets | rings open slightly earlier; the ProvinceFund sizing in §3.4 needs +8.3% of provinces for rings ≥ 4 at the same citizen count [design] |
| Doctrine interplay | genesis Free Cities have no walls, so Iron's power applies only to released Free Cities | keeps F's power exactly as simulated today; no new doctrine leverage |

---

## 6. Sieges

### 6.1 DeclareSiege (the siege horn)

A player instruction by the **declarer** (a citizen) against a **target site**. Refusals, in order:

| # | Check | Refusal |
|---|---|---|
| 1 | season effective Running and `bell < end_bell` | `WrongStatus` |
| 2 | declarer has a final first holding; for a player target, **the declarer's own first holding is not shielded** (unless dormant) | `NotFinal`, `Shielded` |
| 3 | declarer's banked laurels ≥ 5; they move into the siege's escrow | `NoStake` |
| 4 | declarer has < 2 active sieges | `TooManySieges` |
| 5 | target is a final holding or a Free City in ring ≥ 2; not provisional; no active siege on it; not occupied; Respite over (§7) | `NotFinal`, `Besieged`, `Occupied`, `Respite` |
| 6 | `siege::may_besiege` with `relation = Rivalry`, no March truce or hostility: not a Seat, not the declarer's faction, not shielded (unless dormant), Frontier protection only against a declarer whose faction has a final holding within 2 provinces (the declarer names that Province), **not in the owner's heartland** | `Seat`, `Friendly`, `Shielded`, `FrontierProtected`, `Heartland` |
| 7 | capture targets (order 2–3, Free City): holdings + open outpost tickets + active capture sieges of the declarer < 3 | `NoSlot` |
| 8 | the siege can still complete: `bells_outside_vigil(owner's vigil, genesis, b + 1, end_bell) ≥ required` (Free City: `end_bell − (b + 1) ≥ required`) | `SiegeTooLate` |

- `required = siege::required_bells(walls_at(bell_start(b)), extra)`, fixed at the horn (the existing `Siege::declare`); `extra` is Wardens of Stone's +12 only in their heartland, which MC never besieges. **Reason:** the defender knows the target length at the horn; a wall race during the siege cannot extend it.
- **Reason for the shield check (2):** a shielded holding's hosts may not target other factions' holdings (O-M1-27, Reveal step 6), so a shielded declarer could not hold the hex anyway; refusing at the horn avoids a free horn.
- **Reason for 2 active sieges per citizen:** a citizen has 4 transit slots per holding; two sieges allow a main effort and a feint while bounding horn spam (each also costs a 5-laurel stake).

### 6.2 Progress, minimum duration and vigil

Unchanged kernel (`Siege::advance`, `advance_quiet`, `bells_outside_vigil`): progress +1 per resolved or quiet bell **only while the declaring faction holds the target's hex** (`BellReport.holds`), **no defending host stands there**, and the bell's scheduled start is **outside the owner's vigil**. A Free City has no vigil (`Option<&Vigil>::None`, new). The siege record lives with the target's Province so ResolveFromInputs and SkipQuiet advance it in the write they already make (§12).

| Walls | Required bells | Minimum wall-clock time if held every bell |
|---|---|---|
| 0 | 36 | 6 h outside the vigil (≥ 6 h; 14 h if it spans one 8-h vigil) |
| 600 | 48 | 8 h outside the vigil |
| 1,200 (`MAX_WALLS`) | 60 | 10 h outside the vigil |
| Free City | 36 | 6 h (no vigil) |

**Reason:** DESIGN §6.3 / D9 ("≥ 36 bells outside an 8-hour vigil", "an attack launched at bedtime cannot complete before the defender wakes"); the review issue 17 fix.

### 6.3 Start window, failure and the stake

| Outcome | When | Stake (5 laurels) | Reason |
|---|---|---|---|
| **Failed (never held)** | the declaring faction has not held the hex by `declared_bell + 72` | to the target's owner by the pair rule (`siege_settle(false, stake_counted = true, h)`); a Free City: burned | `SIEGE_START_WINDOW_BELLS` = the maximum arrival lead, so a horn can precede the march by its full lead and no more |
| **Failed (lost)** | after first holding the hex, a counted bell in which the declaring faction does not hold it | same | defence is paid; spam costs |
| **Completed** | progress reaches `required` | back to the declarer | DESIGN §5.4 item 4 |
| **Lapsed** (new) | the season ends (`end_bell`), or the target is released or razed while the siege is active | back to the declarer | nobody won; nothing crosses the season end |

### 6.4 Horn and alerts (data, §12)

The horn is public: `SIEGE_DECLARED {target, owner faction, attacker faction, declarer, bell, required, start_window_end, earliest_completion_bell}`; then `SIEGE_ENGAGED` at the first held bell, progress in each resolve summary, `SIEGE_DONE {completed | failed | lapsed}`. `earliest_completion_bell` (new kernel helper: the smallest bell e with `required` bells outside the vigil in `[b + 1, e]`) is what the defender's client shows ("can fall at bell e at the earliest"). The herald serves the owner, the March and the faction the same feed (DESIGN: "visible to the owner, the March and the faction").

### 6.5 Auto-reinforce: off-chain in MC

DESIGN §2.4 already places auto-reinforce among the **standing behaviours that run in the client or a relay the player chooses**. In MC it stays there: a player's standing order ("send up to 25% of this holding's garrison to a besieged holding of my faction in the same March") is executed by their client, the bots and, if the player opts in, the relay, as ordinary sealed marches. The kernel's `auto_reinforce` stays the reference selector (same faction, same March, ≤ 25%, the **4 largest shares** because a faction has 4 arrival slots per province-bell).
**Reason:** an on-chain automatic transfer would be an outcome-changing keeper write (each bell of lateness is a bell of siege progress), so it would need class-W treatment, a defence-pool budget and a deadline; MC does not need it for the map to move. M3 can add it with delegation.

### 6.6 Raids

Not in MC (M3, with caravans). **Reason:** a raid moves goods between holdings and changes no control; DESIGN's 10% / 6 h cap stays for M3.

---

## 7. Occupation of a first holding (D9 kept)

| Rule | Value | Reason |
|---|---|---|
| Start | the completion bell e of a siege on a first holding; effective from bell e + 1 | the roster rule: changes take effect after a bell's clash |
| Occupier | the **declarer** (citizen) and the declarer's faction | the declarer staked; allies of the same faction help by holding the hex, as the faction-level progress rule already lets them |
| Ownership | unchanged: **never transferred** | D9 |
| Control | the holding's weight counts for the occupier's faction (§3.1) | F3 |
| Laurels | the holding stops emitting (§5.4 "non-occupied") and its credit is split `occupation_split(credit, owner_staker = true, h)`: 50% × `pair_bps(h)` to the occupier (h as of the start) | existing kernel and DESIGN §5.4 item 2; `owner_staker = true` in MC (§11) |
| Tribute (20% of production) | **deferred to M3** | it needs goods to move between holdings (caravans, M3) and would move the Prosperity fact between factions, which the simulator does not model |
| Owner's actions | all of them continue; the owner may Garrison and Muster on the holding, and those troops fight the occupiers every bell | DESIGN: "the owner keeps playing and can Liberate by winning a clash there" |
| **Liberation** | at the first resolved bell after which the occupier's faction does not hold the holding's hex (`!BellReport.holds(occupier_faction)`) | same predicate as siege failure; no special instruction needed |
| **Tenure cap** | **288 bells (48 h)** after the start, then it ends by itself | a first holding's owner is never evicted (D9's intent); 48 h = one Shield's length; for a holding credited the average 1/12 laurel a bell, one occupation moves at most 288 × 1/24 = 12 laurels |
| **Respite** | after any occupation ends (liberated, expired or season end): no siege may be declared on the holding for **288 bells** | with tenure 288 and a ≥ 36-bell siege, a holding is occupied at most 288 / (288 + 288 + 36) ≈ 47% of any period, so an owner loses at most ~23% of the holding's laurel credit to occupations |
| Concurrency | one occupation per holding; an occupied holding cannot be besieged | simulator |
| Pair history | an occupation counts as a laurel transfer between the pair | DESIGN §5.4 item 2, `PairHistory.prior_captures` |
| Dominion | no direct credit; the control weight does it | — |
| Season end | ends at `end_bell` | §10 |

The simulator today keeps an occupation for as long as the occupying host stands on the hex, with no cap: [sim-design] change (tenure, Respite, faction-level liberation).

---

## 8. Capture of holdings 2–3 and Free Cities

### 8.1 Who captures

The **declarer**. A capture siege reserves one of the declarer's three holding slots from the horn (§6.1 check 7), so the declarer can always take it. If the slot is gone anyway (a holding re-gained by displacement or a rule change), the holding is **razed**: it becomes a Free City (garrison 300, walls 0), the victim loses it as in a capture (laurels and kit by §8.2), and the captor's faction gets the 36 Dominion control-bells. **Reason:** completion must have a deterministic effect that is known at the resolve, without reading Citizens.

### 8.2 What moves

| Item | Rule | Reason |
|---|---|---|
| The holding | owner := declarer, faction := declarer's faction, order := the declarer's next order (2 or 3, never 1) | D9: a first holding exists only from a first ticket in the own wedge |
| Stores, buildings, tier, queue | stores and buildings and tier stay with the holding; queued items are cancelled without refund | a captured city is taken whole; a raid takes 10% (M3) |
| Garrison and trained reserve | **lost** (they were the defenders) | the captor garrisons from its hosts on the hex with `Garrison` |
| Victim's hosts homed there | re-homed to the victim's first holding; the transfer waits until the holding has no transit in states 1–3 (`HasTransits`, as `ReleaseDormant`) | lag only waits |
| Laurels | `capture_transfer(victim_banked, h)`: 25% of the victim's banked laurels on the pair's first capture, 25% of that on the second, then nothing; nothing if the pair has another tie | DESIGN §5.4 item 3, O9 |
| Refugee kit | the victim's first holding receives `STARTER_KIT` (300 food, 300 wood, 200 stone, 100 ore, 100 gold) unless the pair has any history | DESIGN §6.3 ("unless the pair has a history"); `STARTER_KIT` is the founding kit the simulator already pays; [sim-design] the simulator does not pay it on captures today |
| Free City | no laurels, no kit | no owner |
| Dominion | +36 control-bells to the captor's faction | §3.4 |
| Shield | none | sieges are ≥ 36 bells, so a recapture cannot complete before the captor has had 6 hours to garrison |
| Vigil | the captor's vigil applies from the next bell | the vigil belongs to the owner |

### 8.3 Order of effects at the completion bell

1. ResolveFromInputs (or SkipQuiet) of bell e reaches `required`: the siege is Completed, the Province's site mirror switches side (occupation: occupier faction; capture: captor faction, garrison 0) — **the map changes at bell e + 1**.
2. `SettleSiege` (class D, anyone, after that resolve) applies the Holding and Citizen effects (§8.2, the stake, the pair history). It only waits; nothing in its result depends on when it lands. **Reason:** the resolve writes only the Province and ClashInputs (M1 D6), so Holdings and Citizens are settled later, as SettleTransit does for marches.

---

## 9. Protection and the default stance without governance

| Protection | MC rule | Source |
|---|---|---|
| Shield | 48 h for every new holding (72 h after day 7); a shielded holding can be besieged only when dormant | §6.6, `holding::shield_secs` |
| Frontier protection | holdings founded after day 2 can be besieged for 7 days after their Shield only by a faction with a final holding within 2 provinces | §6.6, `may_besiege` |
| Vigil | 8 h a day, siege progress pauses; changes as CL-09 | §6.3, `Vigil` |
| Heartlands (rings 2–3 of a faction's own wedge) | **no sieges at all in MC** | §3.1 "No sieges there without a War decree"; no War exists before M3 |
| Seats (ring 1), Concord | never | §3.1 |
| First holdings | occupied, never taken; tenure 48 h; Respite 48 h | D9, §7 |
| Respite | 288 bells after any occupation | §7 |

**Default stance (what M3 governance will later change):**

| M3 lever | MC default | When M3 lands |
|---|---|---|
| Relations | **Rivalry between every pair**: field clashes and sieges outside heartlands; `relation = Rivalry` everywhere in `may_besiege` | Peace/NAP (no attacks), Alliance (friendly hosts, shared slots by side) via Envoy decrees |
| War decree | none: heartland sieges disabled | General + second Minister, 36-bell horn (24 for Ember), then heartland sieges against that faction; Wardens of Stone +12 bells in their heartland becomes live |
| March truce / hostility | none | Warden + adjacent Warden: truce (no sieges in the March for a term) or hostility (heartland rules as at war) |
| Mandates | none on chain; bots use an off-chain "faction focus" (§14) | typed Mandates from the fixed menu replace the bots' focus |
| Auto-reinforce | client/relay standing behaviour (§6.5) | on-chain standing orders with delegation, if wanted |
| Tribute, raids, caravans | none | with caravans |

**Doctrine powers in MC:** Wardens of Stone's heartland +12 and Ember's war horn are inert (no heartland sieges, no War); Iron keeps walls only on released Free Cities; Verdant's supply +1 does not extend the outpost range. These are the same states the simulator plays them in today, so MC adds no unsimulated doctrine power.

---

## 10. Everything ends with the season (T1, O-M1-25)

| Item | Rule |
|---|---|
| Marches | Depart refuses `arrive_bell ≥ end_bell` (M1 v1.12) |
| Sieges | DeclareSiege refuses one that cannot complete before `end_bell` (`SiegeTooLate`, exact with `bells_outside_vigil`, since a vigil change needs ≥ 24 h notice and the siege uses the schedule recorded at the horn; a change requested during the siege updates the record, §12). A siege still active at `end_bell` **lapses**: stake returned |
| Completions | only at bells < `end_bell` (no resolve exists for a later bell); `SettleSiege` of an earlier completion may land after the end, like SettleTransit |
| Occupations | end at `end_bell`; the laurel index is frozen at T_end anyway (§8.4) |
| Outposts | no ticket after `end_bell − 24` (a cohort closes within 24 bells) |
| Control and Dominion | the last folded hour is the one ending at `end_bell`; Dominion stops there; the final province and March control is the season's map and goes to the **Chronicle** (the only state that crosses seasons, §3.7) |
| After the end | only settlement work (SettleSiege, closes); the next season starts from a fresh map |

---

## 11. Laurels and faction scores as game points (MC, no money)

- **Laurels exist in MC** (the M1 contract reserved them for M2): the per-province reward index and the per-holding stake of DESIGN §5.4 / `laurel::RewardIndex`, banked into the Citizen. They buy nothing and convert to nothing; they are the siege stake and the prize of occupation and capture.
- **No stake exists, so every banked laurel counts.** MC calls the laurel kernel with `owner_staker = true` and `stake_counted = true`, and `capture_transfer` with the victim's banked laurels. The pair rule (first transfer in full, second 25%, then nothing; any other tie nothing) applies unchanged. **In M2** the stake returns and these calls switch to counted laurels only (§5.4); nothing else changes.
- The 10% Mandate reserve split stays in the kernel calls (the reserve accumulates unused until M3), so the chain and the simulator compute the same credits.
- **Faction scores** are the `index` kernel's per-capita index over Dominion (§3.4) and Prosperity (production), with Knowledge and Concord neutral until M3; shown as points, paying nothing.

---

## 12. Data the rules need (hand-off to the program, herald and design-chat areas)

These are rule inputs and outputs, not layouts; the program area sizes them.

| Where | Data | Written by |
|---|---|---|
| Province site mirror (per site) | site kind (free / holding / **Free City** / reserved), owner faction, **occupier faction**, order, tier, committed garrison (exists), **Respite until bell** | SettleTicket, Garrison, Muster, resolve, SettleSiege, ReleaseDormant |
| Province | **siege records, one per besieged site**: attacker faction, declarer tag, status, held, `declared_bell`, `last_bell`, progress, required, **the owner's vigil schedule copied at the horn** (SetVigil on a besieged holding also updates it) | DeclareSiege, resolve/skip (advance), SettleSiege |
| Province | **occupation records**: occupier faction and tag, from bell, pair history snapshot | resolve (start, liberation, expiry), SettleSiege |
| Province | **control**: the 7-side weight vector, controller, `since_bell`; hourly samples until folded (§3.3) | every site write; resolve/skip |
| MarchState (new, per March) | folded-through hour, controller, `since_hour`, per-faction cumulative control-bells and the per-weight Dominion index | FoldMarch |
| Citizen | active sieges (≤ 2), capture slots reserved, banked laurels, Dominion facts | DeclareSiege, SettleSiege, Bank |
| Holding | occupation credit owed to the occupier (collected by the occupier, so Bank never writes two Citizens), capture state | Bank, SettleSiege |
| Log records | `SIEGE_DECLARED`, `SIEGE_ENGAGED`, `SIEGE_DONE{completed, failed, lapsed}`, `OCCUPIED`, `LIBERATED`, `OCCUPATION_ENDED{expired, season_end}`, `CAPTURED`, `RAZED`, `FREE_CITY{genesis, released}`, `CONTROL{province, from, to, bell}`, `MARCH_FOLD{march, hour, controller, credit}` | the instructions above |

**Herald layers the design chat can render** (field names are proposals for the herald area):
- `province.control = {state: unsettled|contested|neutral|faction, faction?, shares_bps[7], since_bell}` per bell; `march.control = {…, since_hour, dominion_bells[6]}` per folded hour;
- `site = {kind, faction, order, tier, occupier_faction?, respite_until_bell?, siege?: {attacker_faction, declarer, status, held, progress, required, declared_bell, start_window_end, earliest_completion_bell, paused_by_vigil_now}}`;
- a per-faction alert feed (horns against the faction's holdings and Free Cities in its Marches).

---

## 13. What changes outcomes, and the re-runs

### 13.1 Outcome-changing items

| Item | Changes | Gates to re-run |
|---|---|---|
| Control rule (occupation for the occupier, dormant counted, neutral side, strict majority) | the Dominion path for every faction | doctrine CI proxy gate (60 seeds × 6 rotations), nightly 1,500-season band (O5), bot criterion (best response) |
| Genesis Free Cities, released → Free City | sites, emission (captured Free Cities emit as holdings 2–3), Dominion (captures) | all three |
| Outpost placement rules | holdings 2–3 everywhere, ring growth | all three |
| Occupation tenure, Respite, faction-level liberation | laurels between players, Dominion | all three |
| Siege rules (2 per citizen, `SiegeTooLate`, lapse, capture slot, raze, declarer shield) | siege counts, laurels | all three |
| Refugee kit on capture | goods (Prosperity) | doctrine gate, criterion |
| MC's "all laurels count" | only MC's chain; the simulator's money runs keep counted laurels | none for MC; the M2 criterion is run on M2 rules |

### 13.2 Simulator changes required (frontier-sim) [sim-design]

Each behind a config flag so the K3 results stay reproducible: control inputs (§3.1) and the strict rule (also fixes F7); genesis Free Cities and released → Free City; outposts (ring ≥ 4, 3-province range, Town, land gate, the outpost rule); tenure/Respite/faction-level liberation; DeclareSiege's checks 2, 4, 7, 8 and the lapse; capture beneficiary and raze; refugee kit on capture; no tribute; MC preset with postures off (M1 has none, I-16) for chain-parity runs, while the doctrine gate stays on the full-ruleset preset. **Behaviour:** add a conquest policy (faction focus, reach 3 provinces, value of control) as a profile parameter, used by the bots (I-36 copies the profiles) and as a criterion variant, not as the default human model.

### 13.3 Preliminary lab results

In the lab, with R approximated (§18): the doctrine CI proxy gate **passes** (largest |Δ index| 0.086% against 0.060% on stock rules; bound 0.2%), and the default-mix bot criterion **passes with slightly more margin** (worst bot + stake 0.933 barred / 0.962 in office against 0.944 / 0.966; 0.926 / 0.962 with conquest-playing bots). The 1,500-season O5 band and the best-response criterion were not run and are owed.

---

## 14. Proposed acceptance for the map (MC exit, 7-day season, 1,000 bots)

Measured on R + the conquest stand-in, seeds 1–5 (worst seed in brackets); thresholds set below the worst seed so a pass means the bots play the contest, not that the seed was lucky:

| Criterion | Proposed threshold | Measured (worst) |
|---|---|---|
| Settled provinces under a foreign faction's control in at least one hourly fold | ≥ 6% | 9.1–14.0% (9.1%) |
| Settled provinces whose controller changes at least once after day 1 | ≥ 50% | 68–77% (68%) |
| Sieges completed | ≥ 80 | 109–137 (109) |
| Occupations + captures of player holdings | ≥ 10 | 17–28 (17) |
| Every faction controls at least one province outside its wedge at some hour | 6 of 6 | not yet measured |

The bots need the conquest policy for these; with the simulator's default behaviour the rules alone give 6.8–9.1% / 69–75% / 70–82 / 0–2, which fails the third and fourth rows.

---

## 15. Owner decisions

| # | Question | Recommendation | Alternatives (measured where possible) |
|---|---|---|---|
| C-1 | D9: keep "the first holding is never taken"? | **Keep** | Removing it moves the map no further (F2) |
| C-2 | Outpost rule for holdings 2–3 (only where the faction holds < 50%)? | **Adopt** | Without it 3.0–6.4% ever foreign (7 d) instead of 6.8–9.1% |
| C-3 | Genesis Free Cities (one per province from ring 4)? | **Adopt** | Without them, no neutral contest until day 10 releases; 0 Free City captures in a 7-day season |
| C-4 | Occupation tenure / Respite | **288 / 288 bells (48 h / 48 h)** | 432 / 144 (3 d / 1 d): up to 70% of any period occupied; no cap (DESIGN as written): a first holding can be occupied indefinitely |
| C-5 | Heartland sieges in MC | **Disabled** (no War until M3) | A permanent "always at war" default would expose 36% of first holdings at 1,000 players to sieges with no governance to stop them |
| C-6 | Tribute and on-chain auto-reinforce | **Deferred to M3** | — |

---

## 16. Pinned numbers

| Constant | Value | Kernel | Reason |
|---|---|---|---|
| `CONTROL_SIDES` | 7 (factions 0–5, neutral 6) | control | Free Cities are a side |
| Control threshold | ≥ 50% and strictly ahead | control | §5.6; F7 |
| `FOLD_BELLS` | 6 (hourly) | control | §5.6 |
| `DOMINION_PER_FOLD` | 6 control-bells | control | one per bell; simulator |
| `DOMINION_PER_CAPTURE` | 36 control-bells | control | simulator; = minimum siege |
| `SIEGE_BASE_BELLS` | 36 | siege (unchanged) | D9, issue 17 |
| `WALL_POINTS_PER_BELL` | 50 | siege (unchanged) | §6.3 |
| `MAX_REQUIRED_BELLS` | 84 (60 without doctrine extra) | siege (unchanged) | CL-02 |
| `VIGIL_SECS` | 8 h | siege (unchanged) | §2.2 |
| `SIEGE_START_WINDOW_BELLS` | 72 | siege (unchanged) | = `MAX_ARRIVAL_LEAD_BELLS` |
| `SIEGE_STAKE` | 5 laurels | laurel (unchanged) | ≈ 60–67 bells of an average first holding's credit (1/12 a bell, less the 10% reserve): one siege a day is affordable, spam is not |
| `MAX_ACTIVE_SIEGES_PER_CITIZEN` | 2 | siege (new) | main + feint |
| `OCCUPATION_BPS` | 5,000 | laurel (unchanged) | §5.4 |
| `OCCUPY_TRIBUTE_BPS` | 2,000, **unused in MC** | siege (unchanged) | deferred |
| `OCCUPATION_TENURE_BELLS` | 288 | siege (new) | §7 |
| `RESPITE_BELLS` | 288 | siege (new) | §7 |
| `CAPTURE_BPS` | 2,500; second capture 25% of it | laurel (unchanged) | O9 |
| `AUTO_REINFORCE_MAX_BPS` | 2,500, 4 largest | siege (unchanged, off-chain use) | §6.3 |
| `OUTPOST_MIN_RING` | 4 | holding (new) | outside every heartland |
| `FOUNDING_RANGE` | 3 provinces, not doctrine-modified | holding (new) | `MAX_MARCH_DIST`; no new doctrine lever |
| `OUTPOST_TIER_MIN` | Town | holding (new) | simulator |
| Outpost control limit | < 50% own share | holding (new) | F5; owner C-2 |
| Land gate | free ≥ 20% of open | holding / Join (existing) | §3.4 |
| Outpost filing close | `end_bell − 24` | holding (new) | cohort bound |
| `FREE_CITY_MIN_RING` | 4 | terrain/catalog (new) | outside every heartland |
| `FREE_CITIES_PER_PROVINCE` | 1 | terrain/catalog (new) | F4; 8.3% of sites |
| `FREE_CITY_GARRISON` | 300 troops | terrain/catalog (new; simulator constant) | Hamlet garrison target |
| Refugee kit | `STARTER_KIT` | holding (new use) | §6.3 |
| Heartland sieges | disabled | siege (`Rivalry` everywhere) | no War before M3 |

---

## 17. Lab reproduction

```
cd scratchpad/frontier/conquest/lab/rules-mapmove
CARGO_TARGET_DIR=$PWD/target cargo build --release --offline --manifest-path frontier-sim/Cargo.toml
./final7.sh                      # 7 d × 1,000 × seeds 1–5: M1 base, R(rec), R + play, R + play − D9, alt, alt + play
./final7b.sh                     # the same with outposts from ring 4 (identical results: no 7-day outpost lands in rings 2–3)
AGENTS=10000 DAYS=28 JD=21 PFX=g28 SEEDS="1 2" ./final7b.sh
./matrixB.sh                     # bastion-weight variants (F8)
./heavy.sh                       # criterion (default mix, aggressive bots) and the doctrine proxy gate with R
python3 summ2.py out/<run>.mm <name>   # one-line summary of an hourly control log
```
Rules package R = `MM_OCC=1 MM_DORMCTL=1 MM_NEUTRAL=1 MM_STRICT=1 MM_GFC=1 MM_EXPAND=3 MM_OUTPOST_RING=4 MM_TENURE=288 MM_RESPITE=288` (7-day: `MM_JOINDAYS=5`); conquest stand-in = `MM_FOCUS=3 MM_AGGR=2 MM_MARGIN=0.5`. The `MM_*` code is in `frontier-sim/src/sim.rs` (`mm_env`, `hourly_fold`, `compute_focus`, `expand`, `open_ring`, `siege_done`, `war`) and `main.rs` (`--days`, `MM_OUT`).

---

## 18. Appendix: preliminary gate and criterion results in the lab

These approximate R in the simulator (`MM_*` switches: control inputs, genesis Free Cities, the outpost rule via `MM_EXPAND=3` from ring 4, tenure/Respite 288/288). They do not model §6.1 checks 2, 4, 7, 8, the raze fallback or the refugee kit, and run with the simulator's postures. They are a first read, not the gates.

**Doctrine CI proxy gate** (`doctrine-gate --set kernel`, 60 seeds × 6 rotations, 10,000 wallets) [measured]:

| Doctrine | Stock rules: win rate / Δ index | R: win rate / Δ index |
|---|---|---|
| A Wardens of Stone | 18.3% / +0.036% | 17.2% / −0.030% |
| B Tide | 15.8% / −0.020% | 17.5% / −0.070% |
| C Ember | 15.8% / −0.034% | 19.2% / +0.022% |
| D Verdant | 17.2% / −0.021% | 15.6% / +0.086% |
| E Lumen | 19.2% / +0.060% | 16.9% / −0.007% |
| F Iron | 13.6% / −0.020% | 13.6% / −0.001% |
| Gate (every |Δ index| ≤ 0.2%, win rates 16.7 ± 10) | **pass** (largest 0.060%) | **pass** (largest 0.086%, SE ≈ 0.047) |

Both pass the CI proxy; R's largest gap (Verdant +0.086%) is within 2 SE. The O5 band (6/6 at 16.7 ± 2 on 1,500 paired seasons) is the real check and was not run here (≈ 1,500 seasons; 22 minutes for 360 on the shared machine).

**Bot criterion, default mix** (`criterion --seeds 3`, seeds 201–203, bot + stake, worst over 1/2/5/10% bots) [measured]:

| Variant | Bots barred from office | Bots in office |
|---|---|---|
| Stock rules | 0.944 | 0.966 |
| Stock rules, bot aggression 1.0 | 0.946 | 0.966 |
| R | 0.933 | 0.962 |
| R, bot aggression 1.0 | 0.931 | 0.962 |
| R, conquest bots (aggression 1.0, faction focus, attack at 1.5–2×) | 0.926 | 0.962 |

R does not raise a scripted wallet's return; aggressive or conquest-playing bots do slightly worse (sieges against defended holdings mostly fail and cost the stake). **Still owed:** the best-response criterion (`criterion --best-response`, the binding one: 0.980 worst cell today, 2 points of margin) with an aggression/conquest choice added to the bot's choices, on the simulator that models §13.2, before M2 ships money on these rules. The risk to watch: Free Cities have no vigil, so an always-online wallet times their sieges best and gains holdings 2–3 that emit.
