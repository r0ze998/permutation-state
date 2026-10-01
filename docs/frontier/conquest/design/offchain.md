# Conquest milestone ("the territory contest"): off-chain design

- **Date:** 2026-10-01. **Area:** off-chain. This covers keeper duties, the herald's faction-map data and its file formats for the design chat, verifier checks and tamper classes, bots and personas, the simulator, the stack and its report (including the "the map moved" criterion), and the replay-page data for the English demo. **Web changes** are limited to data and controller modules plus a written hand-off. The design chat owns `web/frontier/hud/*`, `map/sprites.mjs`, `art/**`, `app.mjs` and `frontier.css` on `frontier/ui-shell`.
- **Owner decision this answers (2026-10-01):** build the territory contest before M2 money, so that the faction map changes through play. Keep D9 ("the first holding is never taken") unless the design check shows that the map cannot move enough with it. Everything ends with the season (O-M1-25). Money stays out: laurels and faction scores are game points. Governance is M3, so this milestone runs on governance defaults.
- **Normative inputs:** DESIGN rev 3.1 §3 (wedges, heartlands, Marches), §4.2 (relations), §5.6 (Dominion, `FoldMarch`), §6.3 (sieges and capture), §6.6 (protection), §8. M1-CONTRACT v1.13 §5–§10 and I-17. DECISIONS parts A, T and U. M1-EXIT-NOTES.
- **Code read** (`codex/frontier` at `aae8617`, read-only):
  - `frontier-node/crates/{herald,keeper,agents,bots,stack,verify}`;
  - `permutation-rules/src/frontier/{siege,geometry,laurel,holding}.rs`;
  - `frontier-sim/src/{sim,config,main,suite}.rs`;
  - `permutation-server/web/frontier/{herald,controller,fgeo}.mjs`, `map/{layers,fmap}.mjs`;
  - the `frontier/demo-replay` branch (`7ebee71`: `replay/{data,timeline,overlay}.mjs`);
  - `permutation-gateway/src/{advisor.mjs,routes/x402.mjs}` (for the appendix).
- **Rules kept:** the repo stayed read-only. The one experiment ran in a copy (`conquest/lab/offchain-mapmove/`, its own `CARGO_TARGET_DIR`; §17). I started no server and sent no chain transaction, installed nothing and did not touch any port.
- **Tags:** [measured], [sim], [model], [estimate], [design], used as in DESIGN. "Program area" means the sibling design `conquest/design/program.md`. Where I need something from it, §2 states it as an interface requirement with a proposed shape that program.md is free to renumber.

---

## 0. Decisions in one table

| # | Decision | Why |
|---|---|---|
| C-F1 | **The faction map is a published, versioned *control layer*, never a renderer's guess.** The herald writes one small binary per bell (`/h/control/{bell}.bin`, §4.3). It holds the control of every province and every March, computed by **one pinned kernel function** (`permutation_rules::frontier::control`). The herald, the verifier, the simulator, the bots and the browser WASM all call that function. | Today the map is coloured by `majorityOwner` in `herald.mjs`, which is the holding majority. With first holdings pinned to the wedge, that is the homeland wedge. One function, used everywhere, means the demo map, the score and the verifier can never disagree. |
| C-F2 | **The lab result drives the design.** In the simulator, under the current per-holding rules, **no province-level or March-level colour moves in a 7-day, 1,000-wallet season. D9 is not what holds it back** (§1). The control layer therefore needs a **province-level contest** from the program area (§2.3, decision CD-1). Occupation counts as the occupier's control in every layer (CD-2). | The base rules: 0 sieges in 7 days. Relaxed gates: 140–180 sieges, 50–85 occupations, but still ≤ 8 province-majority flips and **0 March flips after day 1**. A banner-like field-control proxy moves 34–85 provinces per season [sim, §17]. |
| C-F3 | **Outcomes are fixed at the bell. Settlement may lag (class D).** Siege progress, completion, occupation, liberation, capture and auto-reinforcement are decided by the resolve of a bell from that bell's state. A keeper write that comes later only applies the result. | This keeps the M1 rule that delay only makes things wait (I-21, DESIGN §8.4). Without it a held account could change who wins a siege, and the crank would have to be class W. |
| C-F4 | **Besieged and occupied provinces resolve bell by bell, never in skip batches.** The keeper's planner adds a target every bell for such provinces. | The defender must see progress, and must be able to act, at each bell. A siege hex is never quiet anyway (hostile residents on a garrison hex fight every bell), so batching saves nothing. |
| C-F5 | **Additive formats.** `PSFOV1` and every existing `/h/*` path stay byte-identical. New data goes into new files: the control layer `PSFCT1`, the overview `PSFOV2` (whose first 24 bytes are the v1 record), the siege list and detail, conquest events per day, and the standings series `PSFSD1`. | The design chat's renderer and the `frontier/demo-replay` page keep working unchanged and opt in field by field. |
| C-F6 | **Bots coordinate the way Companies do, with no messages.** Each bot runs the same deterministic `campaign::plan(faction, epoch)` over the same immutable herald files (the hourly epoch), so every bot of a faction computes the same campaigns and takes its own assignment. | This keeps the M1 property that bots observe only what people observe. The sim's lone attackers barely siege at all (§1). Coordination is what Companies and Mandates are for (E3), and Mandates are M3. |
| C-F7 | **"The map moved" is a gating criterion (§13.4 criterion 10)** with definitions, not impressions: lasting province control changes per day, March changes, the share of foreign-controlled provinces, the Hamming distance between the day-2 map and the final map, and every faction both gaining and losing. Thresholds come from the simulator by a fixed rule (§9.3). | The owner's goal is in the replay. Without a number, a season with a static map could pass the exit again. |
| C-F8 | **Verifier v3 adds V14–V20 and tamper classes T25–T36**, including `FirstHoldingTransferred` (D9 as a check), `CompletionAfterEnd` (the season end) and an optional cross-check of the herald's control files (`HeraldControlMismatch`). | Conquest outcomes become checkable facts like clashes. The demo's map is then verified data, not a claim. |
| C-F9 | **Governance defaults are hard-wired data, not code paths:** every faction pair is in Rivalry, there is no War decree, March truce and hostility are false, and heartland sieges are refused. The verifier and the bots read them from the season parameters. | M3 replaces the defaults without changing formats: the control file reserves the truce and hostility bits. |
| C-F10 | **Ports.** Conquest runs use **41300–41999 only**: the exit run on base 41300, nightlies on base 41800, unit dev servers on 41600 + 10k. 41000–41099 (the paused `m1-exit` stack the owner spectates) and 41041 (the design chat) are never bound. `frontier-stack` refuses them. | Task rule. M1's §10.3 scheme stays; only the bases move. |

---

## 1. The design check: does the map move? [sim]

**Question.** With the existing rules (D9, sieges outside heartlands, occupation of first holdings, capture of holdings 2–3 and Free Cities, 48-h Shield, Frontier protection) and the simulator's agents, does a province-level or March-level faction colour change during a 7-day season of 1,000 wallets?

**Method.** I copied `frontier-sim` and `permutation-rules` into `conquest/lab/offchain-mapmove/` with their own target dir. I added a `--days` flag (join days clamped to the season) and an hourly fold that measures six layers each game hour (`patch.diff`):
1. Holding-majority province colour, plain and occupation-aware. This is what `majorityOwner` draws today.
2. March control as DESIGN §5.6 defines it (≥ 50% of strength weight), plain and occupation-aware.
3. Troop-majority province control (garrisons plus stationed hosts).
4. A **banner-like field control**: a province follows the faction whose stationed hosts (not garrisons) hold more than 50% of host troops there for ≥ 3 consecutive hours, and otherwise keeps its controller.
5. The number of provinces with any foreign-held or foreign-occupied site.
6. Siege, occupation, liberation and capture counts.

I also counted why `war()` declined to besiege. Variants, set by environment variables in the copy only:
- `m`: attacker margin 1.5 instead of 3 + (1 − q);
- `s`: Shield 24 h instead of 48 h;
- `hl`: heartland sieges allowed;
- `f`: focus, where attackers prefer provinces their faction already holds, occupies or besieges (a crude Company campaign).

**Results** (1,000 wallets, 7 days, 5% bots, every wallet joining on day 0 unless noted; three seeds each; from `out/*.md`):

| Variant | March control changes, occupation-aware (in the day-0-join runs all fall on day 1, the initial settling) | Province-majority changes, occupation-aware | Sieges declared / completed / failed | Occupations | Liberations | Captures | End: provinces with a foreign site | Field (banner) changes in the season (max per day) | End: field-foreign provinces |
|---|---|---|---|---|---|---|---|---|---|
| base (current rules) | 6 / 6 / 8 | 0 / 3 / 2 | 0 / 0 / 0 | 0 | 0 | 0 | 6–10 | **0 / 0 / 0** | 0–2 of ~140 |
| `s` (Shield 24 h) | 6 / 6 / 8 | 0 / 0 / 3 | 3–5 declared | 0 | 0 | 0–3 | 7–11 | 0–1 | 1–2 |
| `m` (bold attackers) | 6 / 6 / 8 | 0–2 | 84–106 / 12–21 / 59–71 | 12–21 | 0 | 0–2 | 18–27 | 15–28 (5–16) | 11–16 |
| `ms` | 6 / 6 / 8 | 3–5 | 139–146 / 57–70 / 56–76 | 47–63 | 7–13 | 5–10 | 32–41 | **64–85 (16–21)** | **27–34 of ~143** |
| `msf` (focus) | 6 / 6 / 8 | 2–4 | 100–120 / 48–68 / 40–48 | 40–52 | 4–15 | 4–16 | 22–27 | 34–43 (12–17) | 17–24 |
| `hlmsf` (+ heartlands) | 6 / 6 / 8 | 0–4 | 117–126 / 58–67 / 48–53 | 51–63 | 7–12 | 4–7 | 28–29 | 47–65 (15–20) | 21–24 |
| `ms`, day-0 share 0.6 | 9–13 | 7–8 | 99–109 / 45–52 / 44–50 | 37–41 | 9–12 | 5–9 | 29–33 | 33–53 (11–13) | 26–29 of ~210 |
| base, 28 days, 1,000 wallets | 23 | 15 | 143 / 141 / 0 (108 are Free Cities) | 19 | 10 | 9 | 44 | 58 (12) | 33 of 255 |
| base, 28 days, 10,000 wallets | 35 (of 348 Marches) | 8 | 1,303 / 1,253 / 37 (1,184 Free Cities) | 6 | 1 | 18 | 111 | 125 (65) | 71 of 1,917 |
| `ms`, 7 days, 10,000 wallets | 35 | 0 | 629 / 305 / 293 | 305 | 30 | 0 | 175 | 328 (115) | 168 of 1,010 |

**Why the base season does nothing.** In seed 1, 5,507 `war()` attempts led to **0 declarations**. 3,226 found the defence too strong (the attacker wants 3–4× the perceived defence from its own largest garrison alone). 1,966 had no legal target: most candidates were refused for Heartland (110,884 refusals over candidates) or Shield (68,945). The 48-h Shield also means that in a 7-day season nothing can be besieged before day 3.

**What follows** [sim, with the caveat that the agents do not coordinate and the field metric is a proxy]:
1. **D9 is not the bottleneck.** Counting occupied first holdings for the occupier (which is what capturing them would do to the colour) still flips at most 8 provinces in a season, because a province holds about 10 holdings of mostly one faction and a siege takes 36+ bells outside vigil. **Keep D9** and count occupation as control (CD-2).
2. **Neither holding-majority colours nor the holding-weight March control (DESIGN §5.6) can show conquest in 7 days.** Even at 10,000 wallets and 28 days, March control moved on 10 of 28 days, a few Marches at a time.
3. **A province-level contest decided by hosts in the field moves the map** under moderate gates: 34–85 lasting flips a season, 12–24% of provinces under a foreign banner at the end. Attackers' boldness (an off-chain bot parameter) is the main lever; the Shield length sets the start day.
4. **Coordination alone (focus) concentrates conflict but does not raise flips.** It needs a province-level target to pay off.

These are inputs to the program area (CD-1, CD-3) and the reason criterion 10 exists. This document is written so that the formats work whichever control rule the program adopts (§2.3).

---

## 2. Interfaces the off-chain side needs from the program area

These are requirements with a proposed shape. Numbering and layouts are the program area's. Where it chooses differently, the off-chain units code against what it pins.

### 2.1 Instructions and their keeper class

| Instruction (proposed) | Who | Class | What the off-chain side needs |
|---|---|---|---|
| `FileTicket` with `order ∈ {2, 3}` (holdings 2–3, anywhere open, while free sites ≥ 20%) | player | P | `SETTLE` carries `order`; the overview v2 kind bits come from it |
| `DeclareSiege(target holding, host)` | player | P | refusal codes `Heartland`, `Shielded`, `FrontierProtected`, `Seat`, `Friendly`, `SiegeActive`, `SeasonEnd`, `NoStake`; the stake is in laurels (game points) |
| `SetAutoReinforce(holding, bps ≤ 2,500)` | player | P | the standing order is public (the herald shows it) |
| Siege advance, completion, occupation, liberation | inside `ResolveFromInputs` and `SkipQuiet` (C-F3) | D | no new keeper transaction for progress; records per §2.2 |
| `SettleConquest(holding)` (applies a completion recorded at bell b: occupation, or capture with Citizen and Holding writes) | anyone | **D** | refuses until the completing bell is resolved; the outcome is read from the recorded completion, so a late call changes nothing |
| `LaunchReinforcement(siege)` (if auto-reinforce cannot be applied inside the resolve) | anyone | **D** only if donors and the arrival bell are fixed at the declaring bell's resolve (C-F3); otherwise W | donors' hosts arrive at a bell fixed by rule, public destination |
| `FoldMarch(march, hour)` | anyone | **D** | refuses until all 7 provinces are resolved through the hour's last bell; reads a per-hour snapshot the resolve stamps, so a late fold gives the same answer |
| `FoldFaction(faction, hour)` (if faction scores are on chain in this milestone) | anyone | D | as above (CD-6) |
| `CloseSiege` | anyone | N | rent to its payer after a grace |

### 2.2 Log records (PS2, proposed kinds 80–92)

All are chained like M1 records (§6 of the contract). The verifier needs every field listed; the herald derives every file of §4 from these records and the account bytes.

| Kind | Name | Key | Payload |
|---|---|---|---|
| 80 | SIEGE_DECLARE | P, Q, site | attacker tag, attacker faction, owner faction, holding kind (first, other, free city), host_id, declared_bell, required, walls, vigil schedule digest, stake |
| 81 | SIEGE_PROGRESS | P, Q, site | bell_from, bell_to (a skip run gives a range), holds u8, defender_present u8, vigil-covered bells, progress, status (active, completed, failed) |
| 82 | SIEGE_END | P, Q, site | status (completed, failed-never-held, failed-lost, season-end), progress, end bell, stake outcome |
| 83 | OCCUPY | P, Q, site | occupier tag, occupier faction, host_id, bell |
| 84 | LIBERATE | P, Q, site | bell, cause (clash won, occupier host gone, dormancy release) |
| 85 | CAPTURE | P, Q, site | from tag (or free city), to tag, new order, laurels moved, refugee kit u8 |
| 86 | REINFORCE | host_id | donor holding, target holding, troops, arrive_bell |
| 87 | MARCH_FOLD | m, n, hour | weight[6] u64, controller u8, contested u8, control bells added |
| 88 | FACTION_FOLD | faction, hour | the four path facts, Dominion bells, captures (CD-6) |
| 89 | CONTROL | P, Q | from u8, to u8, bell, cause (banner taken, occupation majority, …) — **only if the program stores province control** (§2.3) |
| 90 | RAID | P, Q, site | looted digest (only if raids are in scope) |
| 91 | AUTO_REINFORCE_ORDER | P, Q, site | bps |
| 92 | SEASON_END_SIEGES | id | sieges ended at `end_bell` count |

### 2.3 Where province control comes from (C-F1, CD-1)

- **Preferred:** the program stores `Province.control {faction u8, since_bell u32, progress u16, contender u8}`. This is a banner or field contest decided at the resolve by a pinned kernel `control::advance_banner(&BellReport-like input)`, and the CONTROL record is logged on change. The herald copies it; the verifier replays it.
- **Fallback:** if the program stores no province control, `control::province_control(site owners, occupiers, …)` is the pinned derived rule: the occupation-aware holding majority, ties to the incumbent. It is computed off chain by the herald and checked by the verifier. §1 shows that this layer will not move enough, so criterion 10 is then expected to fail.
- **March control:** `control::march_control(7 province controls, the March fold)` gives two fields, kept apart because they answer different questions:
  - `dominion_controller`, the program's §5.6 fold (≥ 50% strength weight) that scores Dominion;
  - `banner_controller`, ≥ 4 of 7 provinces, for display and criterion 10.
  CD-5 asks the owner whether Dominion should move to banner majority.

### 2.4 Season parameters the off-chain side reads

These come from `Season` and `/h/season`:
- `relations_default = Rivalry`, `heartland_sieges = false` (M3: needs War), `march_truce = march_hostility = 0` (C-F9);
- `shield_secs` (CD-3);
- `siege_base_bells` 36, `siege_start_window` 72;
- `banner_hold_bells` (if CD-1);
- `end_bell`;
- `siege_stake_laurels` 5.

### 2.5 Season end (O-M1-25)

- DeclareSiege refuses (`SeasonEnd`) when `declared_bell + required ≥ end_bell`, even ignoring vigil.
- No completion, occupation, capture or banner change is recorded at or after `end_bell`.
- Every active siege is ended `season-end` by the last resolve, and its stake is returned.
- Occupations end with the season.
- The final map is the control file of `end_bell − 1`.

---

## 3. The keeper

### 3.1 New duties (added to contract §8.2)

| Duty | Trigger | Deadline | Transactions | Class |
|---|---|---|---|---|
| **Siege bells** | a province with an active siege, occupation or banner contest | close → resolve p99 ≤ 8 slots (as criterion 3's active provinces) | GatherClash (fast path if no arrivals), ResolveFromInputs **every bell**; the skip planner never batches such a province (C-F4) | D |
| **Conquest settlement** | a SIEGE_END completed or an OCCUPY, CAPTURE or LIBERATE pending in the Province | none (outcome fixed) | SettleConquest; for a capture also the Citizen and Holding writes it needs | D |
| **Reinforcement launch** (if not inside the resolve) | the declaring bell resolved and donors recorded | before the rule's arrival bell − 2 | LaunchReinforcement | D (C-F3) |
| **March folds** | the hour's last bell resolved in all 7 provinces of a March | none | FoldMarch(m, h) for every March with a holding; FoldFaction(f, h) after the hour's March folds | D |
| **Horn watcher** | SIEGE_DECLARE, SIEGE_PROGRESS milestones (25/50/75%), OCCUPY, CAPTURE, LIBERATE, CONTROL | — | none: a nudge of the besieged province to the front of the queue, an operator alert above a rate, `/v1/status` counters | — |
| **Season-end flush** | `end_bell` | before EndSeason | the last resolves (sieges ended season-end), the last hour's FoldMarch and FoldFaction, SettleConquest for every completion before the end | D |
| **Closes** | siege ended + grace | — | CloseSiege | N |

The **order** extends M1's: resolve(p, b) → SettleDeparture → gather(dest) → resolve(dest, b) → SettleConquest(completions of b) → FoldMarch(hour) after the hour's last resolve.

### 3.2 Scheduling changes

- **Split rule.** The v1.12 split rule gains a latency-relevant target: "a province with an active siege, an occupation or a banner contest". Such provinces are planned bell by bell. Other provinces keep 24-bell batches.
- **Nudges.** `/v1/nudge` from a besieged defender's client (posting a garrison or a reinforcement) is served first.
- **Folds.** One per March per game hour. At 1,000 bots (≈ 40–55 Marches with holdings) that is ≈ 1,000–1,300 FoldMarch a day [estimate].

### 3.3 Cost and budget [estimate]

- A besieged province resolves every bell for the siege's ≥ 36 counted bells, plus vigil pauses: about 50–80 bells, at about 50k CU per RFI in play (M1's in-play max was 50,984).
- In the lab's `ms` regime (≈ 140 sieges a week), that is ≈ 10k extra resolves a season, about 1.3% of a 1,000-bot season's 144k transactions.
- The keeper's budget, `/v1/status` and the payer bands need no change at this scale. The D-pool floor is re-derived from the conquest exit run's p99 per-bell D creations.

### 3.4 Status, metrics and journal

- `/v1/status` gains `conquest {sieges_active, besieged_provinces, settle_pending, fold_lag_hours_p99, horn_events_last_bell}`.
- Prometheus gains `fk_siege_resolve_latency_slots`, `fk_fold_lag_hours` and `fk_conquest_settle_pending`.
- The journal gains a `conquest` table: `(kind, holding, bell, status, attempts)`.
- **Crash safety is unchanged:** the chain is the state, and the queues are rebuilt from the Provinces' siege fields.

### 3.5 Adversary model (keeper)

- **Holding a besieged Province** delays resolves (D). The siege's progress and completion bell cannot change (C-F3). This is held in the stack (§9.2).
- **Holding a March account** delays the fold, never its value.
- **A keeper that skips SettleConquest** delays the transfer; anyone can settle it.

---

## 4. The herald: control layer, files and streams

### 4.1 Fold additions

- `herald-fold` decodes kinds 80–92.
- It keeps per-province conquest state: siege per site, occupier per site, kind per site, banner, auto-reinforce orders. It keeps per-March state from MARCH_FOLD (or the derived rule) and per-faction standings counters.
- At each province's resolve or skip of bell b, it calls `control::province_control` (or copies `Province.control`, §2.3). When **every opened province** has resolved through b, it writes the bell's control file. This is the same completeness rule as the overview files.
- Determinism (byte-identical output from the same archive), atomic writes and `.gz` siblings follow M1's rules. The new alarms are `control_mismatch` (a CONTROL record that disagrees with the herald's recompute) and `conquest_bad_record`.

### 4.2 Paths

| Path | Content | Cache |
|---|---|---|
| `GET /h/control/{bell}.bin` (+ `/latest.bin`) | the control layer, §4.3 | immutable once complete; latest `max-age=2` |
| `GET /h/overview2/{ring}/{bell}.bin` (+ `/latest.bin`) | overview v2, §4.4 (v1 at `/h/overview/…` unchanged) | as v1 |
| `GET /h/sieges/latest.json` | active sieges, §4.5 | `max-age=2` |
| `GET /h/siege/{P},{Q},{site}/{declared_bell}.json` | one siege: its records and per-bell series, §4.5 | immutable once ended; else `max-age=2` |
| `GET /h/conquest/{day}.json` | the day's conquest events, §4.6 | immutable once the day's last bell is complete |
| `GET /h/standings/series.bin` | per-hour standings, §4.7 | `max-age=30`; append-only |
| `GET /h/standings/latest.json` | the latest standings (and `final: true` after EndSeason) | `max-age=5` |
| `GET /h/season/final.json` | after EndSeason: final standings, the movement summary of §9.3 (the same figures criterion 10 reads), the first and last control bells | immutable |
| `GET /h/me/{wallet}` | adds `alerts[]` (horns on own holdings, occupation, capture, liberation) and `sieges[]` the wallet is part of | `no-store` |

### 4.3 The control layer `PSFCT1` (`/h/control/{bell}.bin`)

```
header 32 B : magic "PSFCT1\0\0" · season u64 · bell u32 · n_prov u16 · n_march u16 · slot u64
provinces   : n_prov × 4 B, in dense province order (kernel ProvinceCoord::index; rings 0..open), unopened = control 7
  control u8      0–5 faction · 6 neutral (Concord, Seats, camps only) · 7 none / unopened
  share u8        the controller's share ×255 (banner progress for a contested banner, else holding share) — shading only
  sieges u8       bits 0–3 active sieges (saturating 15) · bit 4 any occupied site · bit 5 heartland · bit 6 seat / ring 0–1 · bit 7 reserved
  flags u8        1 control changed this bell · 2 banner contested (a contender holds the field) · 4 capture this bell · 8 siege completed this bell
                  · 16 siege failed this bell · 32 liberation this bell · 64 occupation began this bell · 128 clash this bell
marches     : n_march × 4 B, sorted by the March's centre province index
  dominion u8     the Dominion controller (program fold, §2.3), 7 none
  banner u8       ≥ 4 of 7 provinces, 7 none
  contest u8      second-largest faction's share ×255 (Dominion weights)
  flags u8        1 dominion changed at this hour's fold · 2 banner changed this bell · 4 truce (M3, 0) · 8 hostility (M3, 0)
```

- **Size:** a 1,000-bot season (rings ≤ 7: 169 province slots, ≈ 40–55 Marches) is ≈ 0.9 KB per bell and ≈ 0.9 MB per 7-day season before gzip. At 50k players (7,644 provinces) it is ≈ 31 KB per bell [computed].
- **Semantics** are the kernel's. The JS decoder lives in `herald.mjs` (§11) with shared vectors (`frontier-abi/vectors/control.json`).

### 4.4 Overview v2 `PSFOV2` (`/h/overview2/{ring}/{bell}.bin`)

- **Header:** v1's header with magic `PSFOV2\0\0`.
- **Record:** 40 B, sorted by (P, Q).

| Bytes | Field |
|---|---|
| 0–23 | **exactly the v1 record** (owners are titles: an occupied first holding stays its owner's) |
| 24–28 | occupiers 40 bits (12 × 3: 0–5 the occupier's faction, 7 none) |
| 29–31 | siege 24 bits (12 × 2: 0 none, 1 declared and waiting for the hex, 2 progressing, 3 paused by vigil or defender) |
| 32–34 | kind 24 bits (12 × 2: 0 first, 1 holding 2–3, 2 Free City, 3 seat or reserved) |
| 35 | province control (as `PSFCT1`) |
| 36 | share |
| 37 | flags2 (1 capture, 2 occupation began, 4 liberation, 8 siege completed, 16 siege failed, 32 control changed, 64 heartland, 128 raid) |
| 38–39 | reserved 0 |

### 4.5 Sieges

**`/h/sieges/latest.json`:**
```json
{"v":1,"bell":0,"sieges":[{"key":"sg:P,Q,site,declared","p":0,"q":0,"site":0,"owner":"tag","ownerFaction":0,"kind":"first|other|free",
  "attacker":"tag","attackerFaction":0,"declared":0,"required":36,"progress":0,"status":"waiting|progressing|paused","pauseReason":"vigil|defender|notHeld",
  "etaBell":0,"startWindowEnds":0,"autoReinforce":[{"holding":"P,Q,site","bps":2500}]}]}
```
`etaBell` is the earliest completion given the owner's vigil (kernel `siege::bells_outside_vigil`, the same as the program's).

**`/h/siege/{P},{Q},{site}/{declared}.json`:**
- the SIEGE_DECLARE, SIEGE_PROGRESS and SIEGE_END records (decoded and raw);
- the bell series `[{bell, holds, defender, vigil, progress}]`;
- the end status;
- the occupation or capture that followed.

The web inspector, the bots and the replay read these files.

### 4.6 Conquest events `/h/conquest/{day}.json`

```json
{"v":1,"day":0,"events":[{"seq":"u64","bell":0,"slot":0,"sig":"…","kind":"siege_declared|siege_milestone|siege_completed|siege_failed|siege_ended_season|occupied|liberated|captured|founded|province_control|march_dominion|march_banner|reinforce|raid",
  "p":0,"q":0,"site":null,"march":null,"from":0,"to":0,"attackerFaction":0,"defenderFaction":0,"citizens":["tag"],"detail":{}}]}
```

- Every event names the PS2 record it comes from (`sig`, `seq`), so a client or the verifier can check it against the raw log.
- `province_control` and `march_*` events are derived (or come from the CONTROL record).
- `founded` covers holdings 2–3 (salients).

### 4.7 Standings `PSFSD1` and `latest.json`

```
header 32 B : magic "PSFSD1\0\0" · season u64 · first_hour u32 · n u32 · reserved 8
per hour    : 6 factions × 32 B
  holdings u16 · sites_controlled u16 (owned unoccupied + occupied by the faction) · provinces u16 (control) · marches_dominion u16 · marches_banner u16
  dominion_bells u32 (cumulative) · captures u16 (cum) · occupations_active u16 · sieges_won u16 (cum) · sieges_lost u16 (cum) · liberations u16 (cum)
  prosperity u32 · knowledge u16 · concord u16   (the §5.6 path facts if FoldFaction runs; else 0 and `official: false` in latest.json)
```

- Size: 192 B per hour, ≈ 32 KB per 7-day season.
- `latest.json` carries the same figures plus the per-capita indices `s_k` when FoldFaction is on chain, labelled `official`. Faction scores are game points (no money).

### 4.8 WebSocket additions

- **Subscriptions:** `{op:"sub", control:true, sieges:true, standings:true}`.
- **Server messages:**
  - `kind:"control"`: `key` is the bell's file path, and `bytes_b64` holds only the changed province records `[index u16, record 4 B]…` (≤ 64 per message, else a resync hint);
  - `kind:"siege"`: the siege's JSON delta;
  - `kind:"alert"` in the Wallet scope: `alert ∈ {horn, occupied, captured, liberated, siegeFailed}` with the record key.
- **Ingest → WS p99 ≤ 2 s** as criterion 6. A horn is an alert, and the alert latency is reported (criterion 12).

### 4.9 Trust

The herald is still never a trust root: every file names its slot and the records it comes from. The verifier can check every control file (`--herald-dir`, V20).

---

## 5. The data contract for the design chat (semantics, not visuals)

These are the facts the UI can render. The design chat decides how. They are the content of the hand-off note `docs/frontier/conquest/HANDOFF-UI.md`, written with the herald unit (§14).

| Fact | Source | States | Notes |
|---|---|---|---|
| Province control (the faction map) | `PSFCT1` `control`, `share` | faction 0–5, neutral, none; contested (flags 2) | **the map's fill should come from here, not from `majorityOwner`**. `majorityOwner` stays for the v1 overview |
| Site title and occupier | `PSFOV2` owners + occupiers | owner pip; occupier ring; free; Free City; seat | D9: the title never changes on a first holding; the occupier is shown separately |
| Siege on a site | `PSFOV2` siege bits; `/h/sieges/latest.json` | waiting (start window), progressing, paused (vigil or defender) with progress/required and ETA | the horn is an alert (WS) |
| Events this bell | `PSFCT1` flags; `PSFOV2` flags2 | capture, occupation began, liberation, siege completed or failed, control changed | for flashes in live play and in the replay |
| March control | `PSFCT1` marches | Dominion controller, banner controller, contest share; truce and hostility reserved (M3) | the March borders are `geometry::march_of`, exported by WASM |
| Heartland and seat | `PSFCT1` sieges bits 5–6 | | "no sieges here without War (M3)" |
| Standings over time | `PSFSD1`, `standings/latest.json` | per faction per hour | a bar race or area chart |
| Own alerts | `/h/me`, WS `alert` | horn, occupied, captured, liberated | sound and notification policy belong to the design chat |
| Composer data | wasm `may_besiege`, `siege_required_bells`, `siege_eta`; `fconquest.mjs` | legal or the refusal code; required bells; ETA | the program's error codes have JA/EN text in `fi18n.mjs` (ours) |

Out of scope until M3: Ministers, decrees, War horn, truce and hostility actions. The bits are reserved so the art can plan for them.

---

## 6. Verifier v3 (`frontier-verify`)

### 6.1 Checks

| # | Check | FAIL codes |
|---|---|---|
| V14 | **Siege legality:** every SIEGE_DECLARE re-judged with `siege::may_besiege` from post-states at the declaring bell (Shield, Frontier protection, heartland under the season's default relations, seat, friendly, one active siege per holding, the season-end rule `declared + required < end_bell`, the stake) | `SiegeIllegal`, `SiegeAfterEndRule` |
| V15 | **Siege progress:** every bell from declaration to end replayed with `Siege::advance` (resolved bells, with the BellReport from V7's clash replay: `holders`, `defender_present`) and `advance_quiet` (skip runs), with the owner's vigil schedule from the VIGIL records; SIEGE_PROGRESS and SIEGE_END must match; civilian hosts never count | `SiegeProgressMismatch`, `SiegeEndMismatch` |
| V16 | **Completion effects:** occupation (occupier, host present, tribute rate), capture (only holdings 2–3 and Free Cities; the attacker's holdings ≤ 3; order; banked-laurel transfer by `laurel::capture_transfer` with pair rules; refugee kit), liberation (cause matches the clash or the occupier's absence); **a first holding never changes owner** | `CompletionMismatch`, `CaptureRule`, **`FirstHoldingTransferred`**, `LiberationMismatch` |
| V17 | **Auto-reinforce:** donors are the kernel's `auto_reinforce` set (same March, same faction, ≤ 25%, the 4 largest), and arrival bells are by rule | `ReinforceMismatch` |
| V18 | **Control and folds:** each province's control per bell = `control::*` over the replayed state (or the CONTROL records replayed from the banner kernel); each MARCH_FOLD = the fold over the stamped snapshots; Dominion bells and captures per faction = Σ | `ControlMismatch`, `MarchFoldMismatch`, `FactionFoldMismatch` |
| V19 | **Season end:** no completion, occupation, capture or control change at or after `end_bell`; every active siege ended season-end with its stake returned | **`CompletionAfterEnd`**, `SiegeNotEnded` |
| V20 | **(optional `--herald-dir`)** every `PSFCT1`, `PSFOV2` and `conquest/{day}.json` the herald wrote equals the verifier's recompute | `HeraldControlMismatch` (warn when the herald is not under test, FAIL in the stack) |

Existing V1–V13 are unchanged. V7's quiet check now also refuses a SkipQuiet over a bell where an active siege's hex had hostile residents (never quiet).

### 6.2 Tamper classes (each MUST FAIL with the named code)

| # | Tamper | Code |
|---|---|---|
| T25 | a declaration in a heartland | `SiegeIllegal` |
| T26 | a declaration against a shielded holding | `SiegeIllegal` |
| T27 | +1 progress on a vigil-covered bell | `SiegeProgressMismatch` |
| T28 | progress counted while a defender stood on the hex | `SiegeProgressMismatch` |
| T29 | a first holding's owner rewritten to the occupier at completion | `FirstHoldingTransferred` |
| T30 | a capture by an attacker that already holds 3 holdings | `CaptureRule` |
| T31 | a CAPTURE moving more laurels than `capture_transfer` | `CompletionMismatch` |
| T32 | a completion logged at `end_bell` | `CompletionAfterEnd` |
| T33 | a MARCH_FOLD controller flipped | `MarchFoldMismatch` |
| T34 | a CONTROL record (or the derived control) changed for one bell | `ControlMismatch` |
| T35 | a herald control file with one province recoloured | `HeraldControlMismatch` |
| T36 | a liberation without a won clash | `LiberationMismatch` |

- **Checks of the checks:** `mutate-v14` … `mutate-v20`; each lets its tampers pass.
- **Honest-but-adverse fixtures (MUST PASS):**
  - a siege paused by the owner's vigil across midnight, including a vigil change taking effect mid-siege;
  - a siege waiting in its 72-bell start window, then failing never-held;
  - a siege won in a skip run (completed by `advance_quiet`'s binary search);
  - a besieged Province held past its close, so the completion is late but at the right bell;
  - an occupation ended by the occupier's host leaving;
  - a capture of a Free City with F's walls kept;
  - a season-end cut of an active siege.

---

## 7. Bots (`frontier-agents`, `frontier-bots`)

### 7.1 A strategic layer: campaigns without messages (C-F6)

New module `frontier-agents::campaign`. It is pure and deterministic in `(fleet seed, faction, epoch)`.
- **Epoch.** The last completed game hour. Inputs are only immutable herald files of that epoch: `PSFCT1` of its last bell, `PSFOV2` of the rings, `sieges/latest.json` as of the epoch (served at `/h/sieges/{bell}.json` once complete, an added immutable path), and `PSFSD1`. **Every bot of a faction computes the same plan.**
- **Frontier.** Provinces adjacent to the faction's controlled provinces (or containing its holdings), excluding enemy heartlands (C-F9), seats and provinces whose holdings are all shielded at the planned arrival.
- **Campaign choice.** Up to `K = ⌈members/40⌉` campaigns per faction (one per Company of ≤ 32 in spirit). Score = (gain: banner value, sites, Dominion swing) ÷ (estimated defence: garrisons, walls, defender hosts within 3 provinces, standing auto-reinforce). Keep a campaign until it is won, has failed twice, or its target becomes illegal. Prefer continuing over switching (hysteresis).
- **Assignment.** Members within reach (≤ 3 provinces, one march) are ranked by spare troops. A campaign gets an assault group (the field host: to take and hold the banner hex or the target hexes), siege groups (one per target holding, from the planner's ordered list of the province's weakest legal holdings) and a reserve. **Everyone's sealed marches name the same arrival bell** (the campaign's `muster_bell`, by rule `≥ max(earliest arrival) + 1`, ≤ `end_bell − 1`), so the assault arrives together, as a coordinated Company would. The plan clamps everything to the season end (§2.5).
- **Defence plan.** Each holding under a horn in the faction's Marches gets defenders by the same rule: hosts that can arrive before 25% progress go to the besieged hex (`defender_present` pauses progress). A counter-attack goes when the estimated strength of the besiegers is below 1 ÷ margin of the available relief.
- **Boldness.** Margin `m` per archetype: very skilled 1.3, skilled 1.5, daily 2.0, casual 3.0, idle never. The lab's lever (§1). The defaults are pinned by the simulator gate (§8), not tuned on the stack.

### 7.2 Behaviours (honest bots; every write goes through the relay as in M1)

| Behaviour | Who (from `profile`) | Rule |
|---|---|---|
| **Expand** | daily and up | file `order 2/3` tickets at the faction's seam provinces (adjacent to another wedge) while the 20% free-site rule allows; this builds salients |
| **Besiege** | campaign assignees | DeclareSiege when the group's first host is due within the start window; only legal targets (the bot runs `may_besiege` on the epoch state before a relay call, so honest refusals ≈ 0) |
| **Hold the field / banner** | assault group | sealed march to the banner hex (or target hexes); stays; `retreat_bps` from the persona |
| **Defend / reinforce** | holdings in the besieged March; `SetAutoReinforce` 25% by default for skilled and up | as §7.1; garrison top-up on a horn |
| **Occupy and keep** | the completing siege's host | stays one host in the province; relieves with a second host before leaving |
| **Liberate** | members near an occupied own-faction holding | strike when stronger than the occupier's host by the margin |
| **Capture** | an attacker with < 3 holdings | targets holdings 2–3 and Free Cities; otherwise skips |
| **Opportunist** | casual | dormant holdings, Free Cities, camps |

### 7.3 Adversarial conquest personas (default on in the exit run, each ≤ 1% of bots)

The expected outcome is in brackets.

| Persona | Behaviour | Expected outcome |
|---|---|---|
| `siege_heartland` | declares in an enemy heartland | refused `Heartland` |
| `siege_shielded` | declares on a shielded holding | refused `Shielded` |
| `siege_seat` | declares on a ring-1 seat | refused `Seat` |
| `siege_late` | declares when it cannot complete before `end_bell` | refused `SeasonEnd` |
| `siege_double` | a second faction besieges a besieged holding | refused `SiegeActive` |
| `first_taker` | completes a siege on a first holding and tries to Capture | occupation only; V16 never sees a transfer |
| `capture_cap` | holds 3 holdings and completes a capture siege | no 4th holding; the host goes home by rule |
| `phantom_defender` | a Scout on the besieged hex | progress continues; civilians never count |
| `vigil_hopper` | changes vigil twice in a week, or right before a siege | refused, or effective at the next midnight ≥ 24 h |
| `occupation_squatter` | a 100-troop occupier | legal; liberated when a defender attacks |
| `reinforce_outsider` | auto-reinforce from another March | refused |
| `siege_spammer` | many declarations without hosts | relay quota (§12); stakes forfeited to the defenders at never-held failure |

These are criterion 13's personas.

### 7.4 Observations and reports

- Bots read the new herald files only, through the same paths as people.
- `bots/report.json` gains `conquest {campaigns[], sieges {declared, won, lost, refused_by_code}, occupations, liberations, captures, personas{…}}`.
- `frontier-bots --conquest` turns the campaign layer on. `--margin-scale x` exists only for the sim-matching run.

---

## 8. The simulator (`frontier-sim`)

1. **`--days N`.** Join days clamp to `min(21, N − 1)` (as in the lab). `--day0`, `--bot-aggression` and `--margin-scale` exist already or are added.
2. **The map-movement fold.** Promote the lab's hourly fold, but have it call `control::province_control`, `control::march_control` and the banner kernel the program adopts, so the simulator measures exactly what the herald writes.
3. **The campaign policy** (`policy = lone | campaign`). It is the same planner as §7.1, ported, or a faithful simplification with a field-equality test of its scoring (as I-36 did for profiles). It is off by default for the doctrine gates, so their digests stay stable.
4. **`frontier-sim mapmove --agents 1000 --days 7 --seeds 10`.** Prints criterion 10's metrics per seed and their p10, p50 and p90. This is how the stack's thresholds are set (§9.3).
5. **The CI gate `mapmove-gate`** (per push, 5 seeds, ≈ 2 s at 1,000 agents [measured: 0.35 s per run]). It PASSes when every metric of criterion 10 meets the threshold on 4 of 5 seeds. **Negative controls must fail:** the current rules with lone bots (the lab base), and the holding-majority control layer with the campaign bots.
6. **Doctrine balance re-run** with conquest on. Previously unsimulated civic powers are now exercised (F keeps walls on captured Free Cities; A's +12 bells stays dormant while heartland sieges are off). The O5 band is re-checked nightly.
7. **Criterion 7, extended:** stack bots against the sim, per day, for sieges declared, won and lost, occupations, captures and control changes. They must be within an order of magnitude; this is reported, not gating.

---

## 9. Stack and report

### 9.1 Runs

```sh
frontier-node/target/release/frontier-stack up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 --conquest \
  --run-id cq-exit --base-port 41300 --chaos --viewers 5000 --viewer-window-hours 24
frontier-node/target/release/frontier-stack verify --run-id cq-exit --herald-check
frontier-node/target/release/frontier-stack tamper --run-id cq-exit
frontier-node/target/release/frontier-stack report --run-id cq-exit
```

- Port bases:
  - the conquest exit run, base **41300** (41310 RPC … 41375 viewers);
  - nightly, base **41800**;
  - unit dev servers, 41600 + 10k.
- `frontier-stack` refuses 41000–41099, 41041, the reserved list, and anything outside 41000–41999. Conquest runs are additionally confined to 41300–41999 by the preset.
- The paused `m1-exit` stack is never touched.
- `--conquest` sets:
  - the season's conquest parameters (§2.4);
  - the bots' campaign layer;
  - the conquest personas;
  - the conquest adversary holds (§9.2).

### 9.2 New adversary holds (each must fire, as v1.12)

| Hold | Expected effect |
|---|---|
| `besieged-province` | hold a besieged Province from its completing bell for 20 bells; the completion lands at the recorded bell (V15) and the settlement waits |
| `march-fold` | hold one March's fold account for 3 hours; the folds land late with identical values |
| `conquest-settle` | hold a capture's Citizen accounts; SettleConquest waits, and the outcome is unchanged |

### 9.3 Criterion 10: the faction map moved (gating)

All are computed from the herald's `PSFCT1` series. The verifier agrees through V20. They are also printed by `frontier-sim mapmove` from the same kernel.

**Definitions**
- A **lasting change** is a province whose `control` is faction *f* at bell *b* after being faction *g ≠ f* at *b − 1*, and which stays *f* for ≥ 6 bells (or to the season end). Changes from or to neutral and none are counted separately as *claims*. This removes flicker.
- **Movement metrics** over days 1–7:
  - (a) lasting changes per day, and their total;
  - (b) the number of days with ≥ 1 lasting change;
  - (c) the share of controlled provinces at `end_bell − 1` whose controller is not the province's wedge faction;
  - (d) the Hamming distance between the control map at the end of day 2 and at `end_bell − 1`, over provinces controlled at both;
  - (e) the factions with ≥ 1 province gained and ≥ 1 province lost;
  - (f) March banner changes and Dominion changes;
  - (g) sieges completed and failed, occupations, liberations and captures.

**Pass (1,000 bots, 7 days)**
- Every metric must reach `max(floor, ½ × the simulator's p10 over 10 seeds with the same rules and policy)`.
- The floors stand until the rules are pinned. They are set from the lab's `ms` and `msf` banner proxy, about half its lower values:

| Metric | Floor |
|---|---|
| (a) lasting changes, season total | ≥ 20 |
| (b) days 4–7 with ≥ 1 change | 4 of 4 |
| (c) foreign-controlled share at the end | ≥ 8% |
| (d) Hamming distance, day 2 to end | ≥ 10% of controlled provinces |
| (e) factions that both gained and lost | ≥ 4 of 6 |
| (g) sieges completed / failed | ≥ 30 / ≥ 10 |
| (g) occupations / liberations / captures | ≥ 20 / ≥ 3 / ≥ 3 |
| (f) March banner changes | ≥ 2 |

- (f) Dominion changes are **reported, not gating**: the lab shows the holding-weight fold is nearly static (CD-5).
- **Report section "Map movement":**
  - a per-day table of (a)–(g);
  - per-faction gains and losses;
  - the top 10 contested provinces;
  - a JSON `mapmove.json` that the replay's captions also use.

### 9.4 Other new criteria

| Criterion | What | Gating? |
|---|---|---|
| 11 | Conquest correctness: verify PASS including V14–V20, with `HeraldControlMismatch` 0 | gating |
| 12 | Horn and siege latency: SIEGE_DECLARE → WS `alert` p99 ≤ 2 s; a besieged province's S → resolve p99 ≤ 8 slots at 20× (criterion 3's rule); FoldMarch lag p99 ≤ 2 game hours | gating |
| 13 | Conquest personas observed or exercised as criterion 5 (v1.13), and **no honest declaration refused by rule** | gating |
| 7 (extended) | as §8 | reported |
| 1–6, 8, 9 | unchanged | gating |

**Report additions:**
- the conquest keeper duties' latencies;
- resolves per besieged province-day;
- refused declarations per (persona, code).

---

## 10. Replay-page data (`frontier/demo-replay`)

`7ebee71` replays the per-bell `PSFOV1` overviews (`replay/data.mjs`, `timeline.mjs`, `overlay.mjs`). For the English demo of the map changing, it gets data-only additions. The visuals stay with the design chat.

- **`replay/data.mjs`:**
  - **`ControlStore`**: fetches `/h/control/{bell}.bin` for the range, keeps the raw bytes (≈ 0.9 KB per bell, ≈ 0.9 MB per season) and decodes them on demand with `herald.mjs decodeControl`;
  - **`EventStore`**: `/h/conquest/{day}.json` (7 files);
  - **`StandingsStore`**: `/h/standings/series.bin`;
  - all immutable, using the browser cache;
  - `loadSeason` also reads `/h/season/final.json` when present.
- **`replay/timeline.mjs`:**
  - `summarizeControl(bell, control)` gives provinces per faction, contested provinces, changes this bell and March banners;
  - `controlChanges(prev, cur)` gives the lasting-change logic of criterion 10 (§9.3) as one shared pure function, used by the report test vectors too;
  - `conquestFacts(events, range)` gives the first horn, the first occupation, the first province changing hands, the first March banner change, the swing day (most lasting changes), the biggest siege and the end standings;
  - new caption kinds `horn`, `occupied`, `flip`, `march`, `swing`, `standings` in `captionSchedule`, with the same placement rules and `CAPTION_SECS`;
  - the strings live in `en-frontier-replay.mjs` (JA mirror).
- **Overlay hooks** (data only): `Effects.bell({…, flips, captures, liberations, horns})` receives the bell's events. How they look is the design chat's choice.
- **Demo beats** (`?demo=1`, 0.2 s per bell ≈ 3.4 min):
  1. day 1: rings open, sealed marches to camps;
  2. day 3: Shields lapse, the first horns;
  3. days 4–6: campaigns at the seams, provinces change hands, a March banner falls, a liberation;
  4. day 7: the standings caption at `end_bell − 1` ("everything ends with the season").
- The `?from=&to=` parameters jump to the swing day.
- **Tests:** `web-frontier-replay.test.mjs` adds the `PSFCT1` decode vectors, the `controlChanges` vectors shared with `frontier-stack report` (the same JSON), and a caption schedule over a fixture season with a flip.
- **Merge order:** `frontier/demo-replay` is not on `codex/frontier` yet. These changes land on top of it after the herald files exist (wave C3, §14) and are rebased when the design chat merges `ui-shell` work there.

---

## 11. Web data and controller modules (our side of the split)

| File | Change |
|---|---|
| `web/frontier/herald.mjs` | `decodeControl`, `decodeOverviewV2` (v1 decode unchanged), `fetchSieges`, `fetchConquestDay`, `decodeStandings`; WS handling for `control`, `siege`, `alert` |
| `web/frontier/fstate.mjs` | state slices `control`, `sieges`, `standings`, `alerts` |
| `web/frontier/controller.mjs` | subscribe `{control, sieges, standings}`; own-holding alerts into state; the composer's actions (DeclareSiege, SetAutoReinforce, order-2/3 tickets) through the relay shapes |
| `web/frontier/fconquest.mjs` (new) | siege composer data: legality via WASM `may_besiege` on the herald state (the same inputs as the program; the relay's simulation is authoritative), required bells and ETA (vigil-aware), reinforcement candidates; the campaign helper "where can my hosts arrive together" (`reachable`) |
| `web/frontier/fland.mjs` | order-2/3 ticket data (free-site rule, seam hints) |
| `web/frontier/fi18n.mjs` | JA/EN text for the new program error codes (table generated from `frontier-abi/vectors/errors.json`) |
| `frontier-wasm` exports | `may_besiege`, `siege_required_bells`, `siege_eta`, `bells_outside_vigil`, `province_control`, `march_control`, `march_of`, `is_heartland`, `capture_transfer` |
| `abi.mjs`, `fcodec.mjs`, `sdk/frontier/*` | regenerated from `frontier-abi` |

`app.mjs`, `hud/*`, `map/sprites.mjs`, `art/**` and `frontier.css` are untouched. `map/layers.mjs` keeps `majorityOwner`. The hand-off (§5) says which state slice replaces it for the fill. **Hand-off note:** `docs/frontier/conquest/HANDOFF-UI.md` covers the data facts and their states (§5), the decoders, the state slices and their update events, fixture files (a recorded mini-season with a flip, a siege, an occupation and a capture, served by `herald --fixture conquest`), the reserved M3 bits, and what is not available yet.

---

## 12. Relay (`permutation-gateway/src/frontier/`)

- **Shape allowlist:** + DeclareSiege, SetAutoReinforce, and FileTicket with `order 2/3`; + SettleConquest, FoldMarch and LaunchReinforcement as settle shapes (charged to the requester, as v1.3).
- **Drain guard:** any new account's rent is an allowance for that kind (escrowed, refunded to the payer); everything else 0.
- **Quotas:** ≤ 3 DeclareSiege per citizen per game day (sim regime: ≤ 1 a day per bold wallet [sim]); `QuotaExceeded` otherwise, with no simulation and no charge.
- **Bodies:** the keeper's and relay's refusal bodies keep `{error, code, detail}` (v1.12).

---

## 13. Tests

| Unit | Tests |
|---|---|
| kernel `control` (program area) | property tests: a holding-majority layer is invariant to unrelated bells; banner progress counts like `Siege::advance` in closed form over quiet runs; vectors `control.json` |
| herald | determinism (same archive → same `PSFCT1`/`PSFOV2`/events, byte for byte); completeness (a control file only when every opened province resolved b); a recorded mini-season fixture with every conquest record kind; WS delta = file diff |
| keeper | the split rule plans besieged provinces bell by bell; FoldMarch waits for resolution; SettleConquest is idempotent; season-end flush (`keeper --test play` gains `conquest_*` cases on the test-beacon `.so`) |
| verifier | V14–V20 on fixtures; T25–T36 FAIL; `mutate-*` builds; honest-but-adverse fixtures PASS (§6.2) |
| bots | `campaign::plan` is identical across bots for the same epoch (property: 32 bots, shuffled observation order); assignments respect reach and the season end; personas' expected codes |
| sim | `mapmove-gate` with its negative controls; the doctrine nightly with conquest on |
| stack | criterion 10–13 deciders on recorded runs (a static-map run must FAIL 10); hold kinds fire |
| web | decoder vectors shared with Rust; `controlChanges` vectors shared with the report; replay captions on the fixture |

---

## 14. Plan [estimate]

Waves follow the M1 conventions: exclusive file ownership, an integrator, a gate per wave. They assume program.md pins the ABI by the end of C1.

| Wave | Off-chain units | Gate |
|---|---|---|
| C1 (1.5 wk) | sim: `--days`, mapmove fold, campaign policy, `mapmove-gate` with controls (feeds CD-1/CD-3 with numbers before the program builds); herald formats spec + vectors (`PSFCT1`, `PSFOV2`, `PSFSD1`); verifier check specs | mapmove-gate negative controls fail; vectors in `frontier-abi` |
| C2 (2 wk) | keeper conquest duties; herald fold and files; bots `campaign` + behaviours | in-process conquest day (`itest`) with a flip; herald determinism |
| C3 (2 wk) | verifier V14–V20, T25–T36; stack criteria 10–13, holds; web data modules + wasm exports; replay data; hand-off note | W-gates as M1 plus the new tamper suite |
| C4 (1.5 wk) | nightlies (base 41800), threshold calibration from `frontier-sim mapmove --seeds 10`, tuning only via the sim | three green nightlies with criterion 10 |
| C5 (1 wk) | the 7-day 1,000-bot conquest exit run (base 41300), the report, the replay recording for the English demo | §9 criteria |

- **Off-chain effort:** ≈ 16–20 engineer-weeks over ≈ 8 calendar weeks with 3 engineers.
- **Contingency:** if CD-1 is decided late, C2–C3 run against the fallback control rule, and criterion 10 is expected to fail until the banner kernel lands.

---

## 15. Risks

| Risk | Likelihood / impact | Answer |
|---|---|---|
| **The map stays static** because the program keeps holding-based control | high / high (the owner's goal) | §1 evidence; CD-1; criterion 10 catches it before the exit; the sim gate catches it per push |
| Bots look orchestrated (perfect coordination) | medium / medium (demo credibility) | campaign size ≤ 32, epoch lag of an hour, session pacing from profiles, sim parity (criterion 7) |
| Resolve load from besieged provinces | low at 1k / medium at 50k | ≈ 1.3% more transactions at 1k [estimate]; budgets unchanged; re-measured in the exit run |
| A static Dominion score while banners move confuses players | medium / low (game points only) | CD-5; the control file carries both |
| A 48-h Shield leaves days 1–2 static in a 7-day season | high / medium | CD-3 (24 h for short seasons), or the demo starts at day 3 |
| Fold and settlement lateness changes outcomes | low with C-F3 / high without | requirement on the program (§2.1); V15/V18 replay catches violations |
| Herald size at 50k (`PSFCT1` 31 KB per bell) | low | gzip; WS deltas; per-day keyframes are possible later |
| The replay branch drifts from `ui-shell` | medium / low | data-only changes, shared vectors, rebased in C3 |
| Heartland sieges off make small seasons quieter | medium / low | lab `hl` variants show little extra movement; M3 adds War |

---

## 16. Decisions needed (owner or main session)

| # | Question | Working default |
|---|---|---|
| CD-1 | **A province-level contest in the program** (a banner or field control per province, decided at the resolve, recorded in the Province) as the faction map's source. Without it the map does not move (§1) | adopt (program area designs the rule; the off-chain side supports both sources) |
| CD-2 | Occupation counts as the occupier's control in every layer; the title stays with the owner (D9 kept) | yes |
| CD-3 | Shield for 7-day seasons: 24 h (season parameter) instead of 48 h | 24 h for short seasons; 48/72 h for 28-day seasons |
| CD-4 | Criterion 10 gating, with thresholds `max(floor, ½ sim p10)` (§9.3) | yes |
| CD-5 | Dominion's March control: keep the ≥ 50% strength-weight fold (static in the lab) or move to banner majority (≥ 4 of 7) | keep for scoring in this milestone; show both; decide in M3 |
| CD-6 | FoldFaction (faction scores) on chain now, or herald-derived standings labelled unofficial until M2/M3 | FoldMarch on chain (Dominion needs it); FoldFaction on chain if the program area fits it, else unofficial |
| CD-7 | Auto-reinforce decided at the declaring bell (so its launch is a D-class crank) | yes |
| CD-8 | DeclareSiege refused when it cannot complete before `end_bell`; active sieges end with the season, stake returned | yes |
| CD-9 | Additive herald formats (`PSFCT1`, `PSFOV2`) rather than changing `PSFOV1` | yes |
| CD-10 | Conquest runs on 41300–41999 (exit base 41300, nightly 41800) | yes |
| CD-11 | Heartland sieges disabled until M3 (Rivalry default; no War decree) | yes (owner's suggested default) |

---

## 17. Lab evidence and how to rerun it

- **Directory:** `scratchpad/frontier/conquest/lab/offchain-mapmove/`. It contains copies of `frontier-sim` and `permutation-rules` from `aae8617`, plus:
  - `patch.diff` (405 lines);
  - `run.sh`;
  - `summarize.py`;
  - `out/*.md` (one full sim report per run, with the lab table at the end);
  - its own `target/`.
- **Build:** `cd frontier-sim && CARGO_TARGET_DIR=$PWD/../target cargo build --release --offline`. The rules copy needs a `[workspace]` line because the scratchpad root has a manifest.
- **Run:** `./run.sh <label> <agents> <days> <day0> <seed> [LAB_MARGIN=x LAB_SHIELD_H=h LAB_HEARTLAND_OPEN=1 LAB_FOCUS=k LAB_BANNER_H=h]`. A 1,000-agent, 7-day run takes ≈ 0.35 s [measured].
- **What the patch changes:**
  - `--days`, with join days clamped to the season;
  - an hourly `lab_mapmove` fold with the six layers of §1;
  - `war()` decline counters;
  - env knobs for margin, Shield, heartland and focus.
- **Caveats** [sim]:
  - the agents are the simulator's archetypes, not the stack's bots;
  - "field control" is a proxy for a banner rule the program has not specified;
  - Marches are counted only where the sim created provinces;
  - three seeds per variant (one for the 28-day and 10k runs).
- The repository was not changed.

---

## Appendix: the owner's two questions, from the off-chain side (facts for the main session)

**1. AI agents: what model, and who pays?**
- In the Frontier (this milestone and M1), **no language model plays.** The 1,000 bots are deterministic Rust policies (`frontier-agents::policy`, profiles copied from `frontier-sim`). The Shades are, by design, "policy committed at genesis and replayed by the verifier; no LLM in actions" (DESIGN §0.3 item 6). They cost local CPU only. On the local chain their fees are test SOL from the stack's faucet; on a playtest, the relay's sponsorship (operator-funded, DESIGN §8.8) pays every player's fees, bots' included.
- The only model call in the repository is the **v9 game's** gateway advisor (`permutation-gateway/src/advisor.mjs`). It uses `claude-haiku-4-5-20251001` to phrase the operator AI members' answers, with the operator's `ANTHROPIC_API_KEY`, so the operator pays. It is not part of the Frontier.

**2. x402's role.** HTTP 402 payments are "pay to enter over HTTP".
- In the v9 gateway (`routes/x402.mjs`), an agent or a browser wallet asks to join and gets a `402` with the price. It returns a signed transaction of the program's `Register` instruction, which moves exactly the entry fee into the season vault and records the member in one instruction. The gateway acts as facilitator: it can only add its fee-payer signature, never change the amount or the payee.
- In the Frontier, DESIGN §2.2 keeps it as the **default join path for everyone, people and Shades alike**, so that funding graphs cannot separate them (§7.4).
- It carries money, so it belongs to **M2**. The conquest milestone does not use it: joins stay free, invite-gated (`join_gate`) for a playtest.
