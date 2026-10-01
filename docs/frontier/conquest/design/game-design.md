# The conquest loop: game design (area: game-design)

- **For:** the conquest milestone (territory contest before M2 money), owner decision 2026-10-01.
- **Read:** DESIGN.md rev 3.1 (§2, §3, §4, §5.4–§5.6, §6, §8.3, §12, §14), M1-CONTRACT.md v1.13 (§0–§1, §5.9–§5.11, §9.3, I-17), DECISIONS.md (I-17, S1, T1), M1-EXIT-NOTES.md, `docs/frontier/m1/runs/m1-exit/report.md`, kernels `permutation-rules/src/frontier/{siege,geometry,holding}.rs`, `frontier-sim/src/sim.rs` (`war`, `expand`), the K3 suite output `scratchpad/frontier/m0b/sim/suite-output.md`, and the web map `permutation-server/web/frontier/map/layers.mjs`. Read-only; no code was run.
- **Tags:** [measured] = in a run record; [sim] = simulator output; [design] = a choice this note proposes; [target] = a number to tune in the simulator, not a promise.

## オーナー向け要約 (Japanese summary)

- **なぜ勢力図が動かないか。** 最初の拠点は自勢力の扇形 (wedge) にしか置けず、地図は「州の拠点の多数派」で色が決まる。最初の拠点は奪えない (D9)。だから M1 では構造上、色が変わる手段がない。シミュレータ (1万人・28日) でも、プレイヤー同士の奪取は 24 件と占領 14 件だけで、動きのほとんどは中立都市 1,183 件だった。
- **提案の核心: D9 は残す。そのうえで「所有」と「支配」を分ける。** 地図の色は所有ではなく**支配**で決める。最初の拠点が占領されたら、所有者は変わらないが、その拠点の支配は占領した勢力のものになる。これで家を失う人を出さずに地図が動く。
- **奪い合いの基本ループ。** 封印した行軍で相手の拠点のヘックスに奇襲する → 野戦に勝ってそこに留まった部隊だけが、公開の「包囲の角笛」を鳴らせる → 36 ベル (6 時間) 以上ヘックスを守り切ると占領または奪取になる。守備側は、その間に 1 ベルでもヘックスを取り返せば包囲は失敗する。
- **地図を毎日動かす供給源は 4 つ。** ① 扇形の境目に最初から置く中立の**境界の町 (Seam Towns)**、② 放置された拠点が戻る**自由都市**、③ 封印行軍で建てる 2・3 番目の拠点、④ 占領と解放の往復。占領には最長期間を設け、地図が固まらないようにする。
- **ガバナンスは M3 まで既定値で動かす。** 全勢力は既定で「対立 (Rivalry)」状態。中核地 (heartland) への包囲は M3 まで無効。役職者の Mandate の代わりに、ルールで決まる日替わりの目標「**伝令の召集 (Herald's Call)**」を出す。
- **オーナーに決めてほしいこと:** 占領で支配が移るか (推奨: はい)、占領の最長期間、境界の町を置くか、小さいシーズンの中核地の大きさ、7 日デモ用に時間の値を縮めるか。詳細は §10。

---

## 0. Verdict in five lines

1. **By construction, the M1 map cannot move.** A first holding must sit in its own wedge (DESIGN §3.1). The web fills each province with the colour of the faction that owns most of its sites (`layers.mjs`, `majorityOwner`). First holdings are never taken (D9). And I-17 excluded every way a site can change hands. Even with sieges switched on, a province full of first holdings can never change colour. At 1.15 holdings per citizen, about 87% of all holdings are first holdings.
2. **Keep D9, but split ownership from control.** A completed siege on a first holding **occupies** it. The owner keeps it, keeps playing and keeps the site. For as long as the occupation lasts, the holding's **control weight** counts for the occupier's faction. The map, the March banners and Dominion all read control, not ownership. On this design check, the map can move enough with D9 in force, so D9 does not need to be broken (§3.1).
3. **Make land-taking cheap to start and expensive to finish.** The opening strike stays sealed. Only a host that has already won the field on the target hex may sound the public siege horn. The siege then needs 36 + walls/50 bells outside the defender's vigil, and the defender breaks it by winning **any one** bell on that hex.
4. **Feed the map from four sources,** so it changes every day even when nobody coordinates: Seam Towns (neutral towns at the wedge seams, seeded at genesis), Free Cities (released dormant holdings), sealed Settler marches that found holdings 2–3, and occupations with a hard maximum length followed by liberations.
5. **Governance defaults until M3:** all factions are at Rivalry, there are no heartland sieges, and there are no truces, treaties or alliances. A rule-chosen daily objective (**Herald's Call**) stands in for Ministers' Mandates. It gives each faction a focal point, and gives the losing faction a reconquest target.

---

## 1. Diagnosis: why the 7-day exit season's map never changed

| Cause | Where | Effect |
|---|---|---|
| The first holding must be in the citizen's own wedge; overflow goes only to an adjacent wedge's outer ring | DESIGN §3.1, contract §5.9 FileTicket | Every province's owner majority is its wedge's faction |
| The map is coloured by the majority site owner | `map/layers.mjs` `provinceFill` → `majorityOwner(rec)` | The colour follows ownership, and ownership is locked |
| No holdings 2–3, sieges, captures, occupation or diplomacy | I-17 | No transfer path exists at all |
| Lifecycle timers are absolute days: dormant 5 d, released 10 d (`holding.rs` `DORMANT_AFTER`), Frontier protection 7 d | DESIGN §3.5, §6.6 | In a 7-day season no holding can ever be released, so even the passive churn source is off |
| Little military activity | exit report: 1,000 bots, **1,712 Departs in 7 game days** (≈ 0.24 per bot per day), 858 camp records, 169 provinces [measured] | The armies are used almost only against camps |

**The simulator shows what the M3 rules alone would do.** The K3 suite, 10k wallets over 28 days, which plays sieges, occupation and capture with the DESIGN rules [sim, `m0b/sim/suite-output.md`]:

- 1,301 sieges declared; 1,267 completed; 21 failed.
- **1,183 Free City captures, 24 captures of player holdings, 14 occupations, 1 liberation.**
- 7,219 holdings 2–3 founded.

So even with the full M3 rules, the contest is almost entirely against neutral land. Player-versus-player territory change is rare. Part of the cause is the rules (land never runs out; garrisons and auto-reinforce; the "within 2 provinces" targeting) and part is the simulator's behaviour model (no coordination, a 3–4× margin, the first plausible target). **The conquest milestone has to fix both. More rules alone will not make the map move.**

---

## 2. What the loop should feel like

### 2.1 The ordinary player (15–30 minutes a day)

**Morning check (2 minutes).** The bell chip and a front report: "Your March: 2 horns overnight; your town held at bell 1,412; Ember took the Seam Town of Kessel." Anything aimed at the player's holdings is at the top:

- A **siege horn** on one of your holdings, with a progress bar (for example 14/38 bells), when your vigil pauses it, and the latest bell by which relief must land.
- **Threat rings.** Public departures within reach of your province that will arrive in the next bells, for example "3 hosts, about 4,200 troops, could land here at bell 1,436". This is derived only from public data (origin, arrival bell, mass) and from the 4-province / 32-step reach.

**Act (10–20 minutes).** Pick one of five verbs. Each is a single flow in the UI.

1. **Strike.** Send a sealed march at a target: a Seam Town, a Free City, an enemy holding outside its heartland, or the faction's Herald's Call. If your host wins and stays on the hex, the client offers **Sound the horn** at the next bell.
2. **Relieve.** Send a sealed march to a besieged friendly hex. A single bell won there breaks the siege.
3. **Raise the levy.** Train and muster on your own holding's hex. This is the fastest counter to a siege: the host fights from the next bell, with no travel. The UI should present it as one button on a besieged holding.
4. **Settle.** Send a sealed Settler march to a free site to found holding 2 or 3. Rivals racing for the same site are settled by the bell's lottery.
5. **Build and fortify.** Walls lengthen sieges (each 50 walls adds 1 bell), and the holding tier raises its control weight.

**Set and forget.** Vigil hours (8 h; exists in M1), a relief preset ("if my March sounds a horn, send 25% of this garrison"; client or bot automation in v1, §7), and the arrival planner's "decisive" tip preset.

**What a player can lose, stated on the join page:**

- Holdings 2–3 can be **captured**.
- The first holding can be **occupied**. While occupied, it pays 20% of its production as tribute and its control counts for the occupier. It is never taken. An occupation ends when the occupier's host leaves or dies, when the owner's side wins the hex, or at the maximum length. After that the holding is immune to sieges for a while (§3.4).

### 2.2 The faction (thousands of players without officers until M3)

- **The front is the seams.** A wedge is a 60° slice, so every faction borders exactly two others along radial seams that grow as rings open. A 7-province March on the lattice often straddles a seam. These **mixed Marches** are the natural battlefields.
- **Three layers of land:**
  - The **Seat and heartland**: inviolable until M3. This is the comeback base, and it means no faction can be eliminated.
  - The **wedge interior**: own first holdings. Occupiable, slow to flip, and the flips there are dramatic.
  - The **seams and rim**: Seam Towns, salients of holdings 2–3, and Free Cities. Fast to flip.
- **Focal point without governance.** At each day boundary, **Herald's Call** names, for each faction, one March by a public deterministic rule (§3.8). Completing an action there earns Works. Most players follow it because it is the easiest decision; organised Companies can ignore it. This is the M3 Mandate board with a rule in place of the officers.
- **How a losing faction comes back.** See §3.7: inviolable core, defender's advantage, interior supply lines, occupations that expire and cost a host each, a Rally call, and per-capita scores.

### 2.3 The spectator and the demo

The demo should show three things on one screen: **(1) a faction map that visibly moves, (2) the tension of sealed marches, (3) the bell resolving everything at once.**

- **Headline layer: March banners.** Each 7-province March shows the faction that controls it (≥ 50% of its control weight), or "contested". This is the 勢力図. Province fills show control on zoom; occupied holdings show the occupier's colour hatched over the owner's sigil. Colour is never the only signal (web design §10): sigils and hatches carry it.
- **Event ticker:** horns sounded, sieges broken, holdings occupied, liberations, captures, Seam Towns and Free Cities claimed, March banners changing hands.
- **Hosts in the air:** arcs from each origin that end in a "?" sized by the host's mass, landing at a public bell. At the bell's end the destinations open and the arcs snap to their targets. This is the game's signature moment and costs nothing to show, because the reveals are public.
- **Time-lapse:** a scrubber over `/h/overview/{ring}/{bell}.bin` (one record per province per bell already exists, contract §9.3), with control added (§8). Seven game days in 60 seconds is the demo video's money shot.
- **Scoreboard:** Marches held, provinces held, and Dominion per active member (§3.9), plus the day's biggest swing.

### 2.4 What changes on the map each day

**7-day demo (Frontier-7 preset, §3.10; about 1,000 bots, rings to about 7, about 170 provinces):**

| Game day | What the spectator sees | Driver |
|---|---|---|
| 0 | Six wedges fill; camps are cleared; grey Seam Towns sit on every seam | Joins and tickets; camps |
| 0.5–1 | The first Seam Towns fall, and coloured dots appear on the seams | Neutral targets ignore shields; shielded newcomers may attack neutrals (§4, C18) |
| 1–2 | Shields lapse for the day-0 cohort. Settler marches plant holdings 2–3 across the seams. The first horns sound | Settle-by-march; DeclareSiege |
| 2–3 | Seam provinces flip by occupation and capture; liberations flip some back. Contested Marches flicker between banners | Occupation counts for control |
| 3–4 | Inactive bots' holdings go dormant and are released as Free Cities inside the wedges; neighbours bite | Season-relative dormancy |
| 4–6 | Herald's Call concentrates each faction; the weakest faction gets a Rally target. The largest swings of the season | Rule-chosen focal points |
| 6–7 | The last sieges must have been declared at least `required` bells before `end_bell`. The final map is written to the Chronicle | Season end (§3.11) |

**28-day Season 1:** the same arc with days roughly ×4 (shields 48 h, dormancy 5 d, release 10 d), plus late-joiner cohorts at the rim from day 7 to day 21.

**Targets for "the map moves"** are in §6.

---

## 3. The conquest rules (proposal for the first version)

Each rule says what is new against M1 and DESIGN.

### 3.1 Control, separate from ownership (new; keeps D9)

- **Control weight of a holding** = tier weight (Hamlet 1, Town 1.3, City 1.6, Stronghold 2.0; the existing laurel tier weights), credited to:
  - the **occupier's faction** while the holding is occupied;
  - otherwise the owner's faction;
  - **neutral** for Free Cities and Seam Towns.
  - Garrison size does not count (C5): control must follow conquest, not turtling.
- **Province control:** a faction controls a province when it has ≥ 50% of the province's total control weight. Otherwise the province is **contested** if two or more parties have weight, or **open** if nobody has any.
- **March control:** the same rule over the 7 provinces' combined control weight. This matches DESIGN §5.6's "≥ 50% of the March's weight", with control weight in place of strength weight.
- **Control changes only on discrete events:** settle, occupy, liberate, capture, release, tier change. So the colour never flickers from bell to bell, and no hysteresis is needed.
- **Why D9 survives.** Flipping a province of 12 enemy first holdings needs about 7 occupations, each 6+ hours and each held by a host on the hex. That is a real campaign, which a capital province should be. Seams, rim, Free Cities and salients flip much faster, so the map moves daily without evicting anyone.
- **The fallback if the simulator disagrees:** if the §6 targets fail with occupation-as-control, the next lever is the occupation length and the Seam Town count, not breaking D9. Breaking D9 (taking first holdings) would make newcomer churn the main risk of the game.

### 3.2 Seam Towns: neutral objectives from day 0 (new)

- At genesis and at each `OpenRing(d)` for d ≥ 4, place **one neutral town per seam per ring band**. Each sits on a non-reserved site of a province touching a wedge seam, symmetric across the six seams (the terrain is already symmetric by rotation, DESIGN §3.2).
- A neutral garrison and walls that grow with the ring. Always attackable: no shield, no Frontier protection.
- Siege rules as for a Free City. On capture it becomes the captor's holding 2 or 3, or is razed at the cap (§3.6).
- **Purpose:** contest from the first day, while everyone is shielded; a reason to look across the seam; and tier-2 loot (more Works) than camps.
- **Count:** [target] about 1 per 4 seam provinces. Tune so that 40–70% are claimed by day 2 of a 7-day season.

### 3.3 Holdings 2–3 by a sealed Settler march (specifies DESIGN §3.1's "founded anywhere open")

- DESIGN gives no founding procedure. The simulator founds instantly within 2 provinces, preferring its own wedge (`sim.rs` `expand`).
- **Proposal:** a Settler host (civilian; it never fights, DESIGN §6.1) marches sealed to a free site. At the arrival bell's resolve, the settler founds a holding there if both of these hold:
  - the site is still free;
  - no hostile side holds the field on that hex.
- If two or more settlers target the same site, the lottery `rand(S, "found", P‖Q‖site‖citizen)` decides, as for tickets. The losers bounce home with no loss.
- **Gates, as in DESIGN:**
  - at most 3 holdings per citizen;
  - founding only while free sites are ≥ 20% of open sites;
  - Settler cost `base × (1 + 0.5(n − 1)²)`;
  - the founder's first holding is at least a Town (the simulator's gate).
- **Shield for holdings 2–3: 12 bells (2 h)**, not 48 h [design]. They are deliberate forward moves. A 48-hour shielded salient next to an enemy is a free fortress.
- **The land-grab is sealed.** Nobody sees which site a settler is heading for. This is the expansion-phase version of the sealed march.

### 3.4 Sieges: strike sealed, then sound the horn from the hex (changes DESIGN §6.3; fixes C2)

1. **Strike.** A sealed march lands on the target holding's hex at bell b. It fights the garrison (`Combatant::City`, retaliating at ×0.5) and any defending hosts. The hex fair share is unchanged: the owner's side keeps 3 slots on its holding's hex.
2. **Horn.** From bell b + 1, if a host of the attacker's faction **is resident on that hex**, its owner may call `DeclareSiege(target)`. The horn is public: the target, the attacker faction, the requirement, and the defender's vigil window, which is already public on chain.
   - The `may_besiege` checks apply with Rivalry as the default relation (§5).
   - The stake is **5 laurels in the M2 design. In this milestone it is 500 Gold** [design, C14], because laurels and reward indices are M2 infrastructure.
   - **Refused if `now_bell + required > end_bell`** (C11).
   - **Why from the hex:** declaring before arrival, as the kernel's `SIEGE_START_WINDOW_BELLS` = 72 and the simulator do, publishes the target 2 or more bells before the sealed march lands. That makes the sealed destination meaningless for exactly the attack that matters most.
3. **Progress:** +1 per resolved or quiet bell while all three hold:
   - the declaring faction holds the hex;
   - no defending host is present;
   - it is outside the owner's vigil.
   Required: **36 + walls/50 bells** (kernel `required_bells`), at least 6 hours outside the vigil. Unchanged.
4. **Fail:** at any counted bell where the besiegers do not hold the hex (`Siege::advance` → `Failed`).
   - The stake goes to the defender.
   - **New: Relief immunity.** The holding cannot be besieged again by any faction for 36 bells [design]. Without it, a chain of re-declarations pins a defender who has just won.
5. **Complete:** occupy a first holding (§3.5); capture holdings 2–3 and Free Cities or Seam Towns (§3.6).

**Costs to state for the program team.** Hostile residents sharing a hex fight every bell, so **a besieged province is never quiet**. A siege costs at least 36 full resolves (≈ 274k CU each, Phase B worst [measured]) instead of about 6 SkipQuiet transactions a day. The keeper budget and the D18 class-D spend scale with the number of concurrent sieges. That needs a model before the demo [estimate].

### 3.5 Occupation and liberation (first holdings; D9 kept, made active)

- **Occupation holds while a host of the occupier's faction is resident on the holding's hex.** DESIGN says only "in the province" (C9). With "in the province", one host could keep all 12 sites of a province occupied.
  - The per-faction cap of 8 hosts per province limits a faction to 8 occupations per province, which is enough to flip it (about 7 of 12) but not cheaply.
- **Effects while occupied:**
  - control weight goes to the occupier (§3.1);
  - 20% of production accrues as tribute, which the occupier collects with `CollectTribute` (DESIGN §8.3 lists it);
  - the occupier's faction earns Dominion for the control;
  - the owner keeps building, training, mustering, and marching from the holding;
  - in M2 the laurel share also moves (50%, with the pair rules). Not in this milestone.
- **Liberation happens at the first resolved bell where any of these is true:**
  - no occupier-faction host is on the hex (it left, was destroyed, or starved under supply attrition);
  - the owner's side holds the field on that hex;
  - the occupation has lasted `occupation_max` (**48 h** in Frontier-28, **12 h** in Frontier-7 [design, owner decision]).
  After liberation the holding has **36 bells of siege immunity**.
- **Supply:** an occupied holding does **not** count as friendly for the occupier's supply rule (DESIGN §3.6: 1% troop loss per bell beyond 3 provinces from a friendly holding). Deep occupations bleed; border occupations are sustainable. These are interior lines for the defender.
- **Slots:** while occupied, the owner's side still keeps the holding hex's 3 owner slots (C17), so liberation fights are not handicapped.

### 3.6 Capture (holdings 2–3, Free Cities, Seam Towns)

- **On completion the holding transfers to the attacker:**
  - the garrison resets to 0;
  - walls are halved (Iron keeps them, per its doctrine);
  - the tier is kept;
  - if the captor already has 3 holdings, it **razes** instead: the site becomes free and the holding closes [design, owner decision; the simulator instead skips such targets].
- **The victim's hosts that belonged to the captured holding** (residents elsewhere, or in transit) **retire to the victim's first holding's reserve** when they settle [design]. M1's host-id → holding-generation link would otherwise strand them (`DisbandStranded`, troops lost), and the design never says so (C13). If the program cost is too high, state the harsh rule on the join page instead.
- Refugee kit: as in DESIGN, unless the pair has a history.

### 3.7 Newcomer safety and comeback

| Mechanism | Who it protects | Status |
|---|---|---|
| Shield 48 h (72 h after day 7); a shielded holding's hosts may not target other factions' holdings | New holdings | M1 has it. **Make explicit that Free Cities and Seam Towns are allowed targets while shielded** (C18) |
| Frontier protection: 7 days after the shield, only factions with a holding within 2 provinces may besiege | Later cohorts at the rim | Kernel exists. **Fix: "nearby" counts only the attacker faction's *first* holdings** (C3) |
| Vigil hours: siege progress pauses for 8 h a day | Everyone offline at night | M1 has `SetVigil` |
| First holding never transferred; occupations expire; 36-bell immunity after a failed siege or a liberation | Everyone | New (§3.4, §3.5) |
| Seat and heartland inviolable until M3 War decrees | Every faction | Kernel `may_besiege` with Rivalry; **heartland size becomes a season parameter** (C4) |
| Defender's advantage: the attacker needs 36+ bells; the defender needs to win 1 bell; levy on the hex in 1 bell versus a march of at least 2 bells | Defenders | Follows from §3.4 |
| Interior lines: supply attrition on deep occupations | The losing side | §3.5 |
| **Rally call:** the faction with the fewest controlled provinces (or the largest 24-h loss) gets Herald's Call on its nearest lost March, with doubled Works | The losing faction | New (§3.8) |
| Per-capita Dominion with herding damping γ = 0.6; Frontier Grant on the picker for small factions | Small factions | DESIGN §5.6, §2.2 |
| Crisis (top 2 pay +10% upkeep from mid-season) | — | **Later**: needs the full faction index |

**Two newcomer risks to measure,** not assume away:

- An occupation in a player's first 72 hours is the worst churn event in this game.
- A faction's visible decline on the picker map herds new joiners away from it. The picker already shows a Frontier Grant; in the no-money milestone nothing else counters herding.

### 3.8 Herald's Call: a rule-chosen faction objective (stand-in for M3 Mandates)

- **At each day boundary**, for each faction f, a public pure function of the post-resolve state picks one March:
  - the March adjacent to f's controlled territory with the highest `enemy_or_neutral_control_weight / (1 + distance)`;
  - ties broken by `rand(S_day, "call", f)`;
  - for the **Rally** faction (§3.7), the nearest March it controlled within the last 2 days and lost.
- **Completing it:** any one qualifying action in the window. A strike that lands, a horn sounded, a relief that breaks a siege, a liberation, or a settle in that March. This is the DESIGN §4.2 menu rule: one action, a ≥ 24-hour window, no streak.
- **Reward:** Works (citizen points), never control or Dominion directly.
- **Why a rule and not officers:** it needs no governance; it is replayable by the verifier; bots and humans see the same target; and it gives the spectator a narrative ("Ember marches on Kessel").
- **M3:** Ministers' Mandates replace it, or the Call becomes slot 1 of the Mandate board.

### 3.9 Faction score in this milestone (game points)

- **Dominion only.** Prosperity, Knowledge and Concord need the economy and Engine work of M3.
- **Formula:** per bell, the faction's March-control count, weighted:
  - **×1.0** for a contested March (two or more factions have weight in it, or it lies outside the controller's wedge);
  - **×0.25** for an uncontested home March;
  - plus a fixed amount per capture and per occupation-bell.
  Divide by active members and smooth as in §5.6 (clamp, γ = 0.6).
- **Why the weighting:** the simulator measured Dominion's β at 0.44–0.55, favouring small factions because they spread thin over empty home land [sim]. That is a land-claim score, not a contest score (C5). [target: re-measure β with the weighting]
- **The scoreboard is integrated over the season,** so the last day is not decisive and there is no end-rush. The **final map** is a Chronicle record, not the score.

### 3.10 Season-relative timers (presets)

M1's 7-day season used 28-day absolute timers (C6). Proposal: express the lifecycle timers as season parameters fixed at `CreateSeason`. Combat and siege timers stay human-scale in both presets.

| Timer | Frontier-28 | Frontier-7 (demo, playtest) |
|---|---|---|
| Join window | day 21 | day 5 |
| Shield (first holding) | 48 h, 72 h after day 7 | 12 h, 18 h after day 2 |
| Shield (holdings 2–3) | 12 bells | 12 bells |
| Frontier protection | 7 d, for holdings founded after day 2 | 36 h, for holdings founded after day 0.5 |
| Dormant after / released after | 5 d / 10 d | 30 h / 60 h |
| Free City window before the site is free | 48 h | 12 h |
| Occupation maximum | 48 h | 12 h |
| Immunity after a failed siege or a liberation | 36 bells | 36 bells |
| Siege requirement | 36 + walls/50 bells | 36 + walls/50 bells |
| Vigil | 8 h | 8 h |
| Heartland | rings 2–3 if expected N ≥ 2,000, else ring 2 | ring 2 |

### 3.11 Season end ("everything ends with the season", T1)

- **Already decided:** Depart refuses `arrive ≥ end_bell`; the last useful Depart is `end_bell − 3`.
- **Add:**
  - `DeclareSiege` refuses when `now_bell + required > end_bell`;
  - a siege still active at `end_bell` ends **without a winner**, and its stake returns to the attacker;
  - occupations end at `end_bell`;
  - tribute stops at `end_bell`;
  - the control map and Dominion freeze at the resolve of `end_bell − 1`;
  - the final map is written to the Chronicle.
- **Nothing carries into the next season.** Terrain does not persist (DESIGN §3.7).

---

## 4. What is visible and what stays sealed

| Information | Who sees it | When |
|---|---|---|
| Departure: origin, host size and unit, faction, arrival bell | Everyone | At Depart (already public in the log) |
| Destination, path, stance, retreat ratio | Nobody | Until the arrival bell's end (tlock to `T(b)`) |
| Threat rings (where a public departure could land) | Everyone, derived | At Depart |
| Clash results, holding of the field, fates | Everyone | At resolve |
| Siege horn: target, attacker, progress, requirement, the defender's vigil | Everyone | From declaration |
| Occupations, captures, liberations, razes | Everyone | At resolve |
| Garrison and walls of a holding | Everyone. The Province mirrors them on chain, so the UI must not pretend they are fogged; honest UI gives humans the same view as bots (P6) | Always |
| Settler destinations | Nobody | Until the arrival bell's end |
| Herald's Call targets | Everyone | At each day boundary |
| Discoveries (explore) | Owner, then everyone | At the seed |
| Postures (M3), Shade identities (M2/M3) | Nobody | Bell end; Reckoning |

**Design principle: everything that changes the map is public the moment it is decided, and every intent is sealed until it lands.** The sealed part is where the skill and the drama are. The public part is what makes defence and spectating possible.

---

## 5. Governance defaults until M3

| M3 mechanism | Default in this milestone | Kernel input |
|---|---|---|
| Faction relations | **Rivalry between every pair**. One faction is one side for the hex fair share | `Relation::Rivalry` |
| War decrees (heartland sieges) | **None**. Heartland sieges are refused (`SiegeRefusal::Heartland`) | `march_hostility = false` |
| Peace, NAP, Alliance | None | — |
| March truce or hostility | None | `march_truce = false` |
| Mandates | **Herald's Call** (§3.8) | new pure function |
| Auto-reinforce (kernel `auto_reinforce`) | **Client or bot automation** using ordinary sealed Departs (DESIGN §2.4 lists auto-reinforce as client automation); the on-chain standing order comes in M3 | — |
| Delegated command | None; Companies coordinate off-chain | — |
| Ministers, Wardens, Steward, Granary | None | — |
| Doctrine civic powers tied to governance (A: +12 bells on heartland sieges; C: war horn) | Inactive (no War) | doctrine table |

**What M3 must plug into later:**

- `relation(f, g)` and `march_flags(m, f, g)`, read by `may_besiege`;
- the Mandate board, replacing or absorbing Herald's Call;
- an on-chain auto-reinforce order;
- the heartland-siege path with A's +12 bells.

If the conquest program keeps these as explicit inputs with the defaults above, M3 changes inputs, not rules.

---

## 6. Targets: "the map moves" [target, to tune in `frontier-sim` and the bot stack]

| # | Metric | Target |
|---|---|---|
| T1 | Provinces outside heartlands whose controller changes at least once over a 7-day season | ≥ 30% |
| T2 | March banner changes per game day after day 1 | ≥ 3% of Marches per day, every day |
| T3 | Every faction both gains and loses at least one March in a season | ≥ 80% of seeds |
| T4 | Snowball bound: the largest faction's share of controlled provinces at the end | ≤ 30% (fair share 16.7%) in ≥ 95% of seeds |
| T5 | Comeback: the faction lowest at day 3 regains at least one March by day 7 | ≥ 50% of seeds |
| T6 | Newcomer safety: holdings occupied within their owner's first 72 h (Frontier-28) or 18 h (Frontier-7) | ≤ 5% |
| T7 | PvP share of completed sieges (not Free City or Seam Town) | 25–60%; today 38 of 1,267 completions ≈ 3% (24 captures + 14 occupations) [sim] |
| T8 | Sieges broken by the defender, of those declared against defended player holdings | 35–60% (today 21 of 1,301 fail overall [sim]) |
| T9 | Doctrine gate with the siege mechanics simulated | still 6/6 in band (DESIGN §4.1) |
| T10 | Demo activity: Departs per bot per day | ≥ 2 (exit season ≈ 0.24 [measured]) |

**The bots and the simulator need new behaviour to meet these** (C16):

- Personas: Raider (strikes and horns), Responder (relief and levy), Expander (settlers), Opportunist (Free Cities and Seam Towns), Liberator, Caller (follows Herald's Call), Turtle (walls), Sleeper (goes dormant: feeds Free Cities), plus the M1 adversarial personas.
- The simulator's `war()` should consider targets beyond 2 provinces of the largest garrison when they are in the Call's March, and use `may_besiege` with the "first holdings only" nearby rule.

---

## 7. First version versus later

| In the conquest milestone (v1) | Later |
|---|---|
| Rivalry default; heartland sieges off; heartland as a season parameter | War decrees and heartland sieges; truce and hostility; treaties (M3) |
| Seam Towns; dormancy → Free City → free site | Relic Sites, Engine, Eras (M3) |
| Settler marches for holdings 2–3, lottery ties, 12-bell shield | Pair tickets, Colonist invitations (M3) |
| DeclareSiege from the hex; progress in resolve and skip; vigil; fail with immunity; complete | Raids (10% loot every 6 h); postures and stances (M3) |
| Occupation held by a host on the hex; tribute (goods); liberation; maximum length | Laurel share on occupation, laurel transfer on capture, pair rules on points (M2) |
| Capture and raze; the victim's hosts retire home | Auto-reinforce on chain; delegated command (M3) |
| Control weight; province and March control; Dominion (weighted, per capita) as points | Prosperity, Knowledge, Concord paths; Crisis; Civilisation Share (M2/M3) |
| Herald's Call and Rally (Works) | The Mandate board (M3) |
| Frontier protection with the "first holdings only" fix | Accusations and Shades (M2/M3) |
| Season-relative timer presets; season-end rules | Waystones, roads, caravans, Bourse (M3) |
| Herald: control, siege, occupation and event data; time-lapse; threat rings | Push notifications for horns (off-chain service) |
| Siege stake in Gold | Siege stake in laurels (M2) |

---

## 8. What the design chat needs to render (game level; the data shapes belong to the program, herald and data areas)

- **Per province:**
  - controller (0–5, neutral, contested, open) and the control margin;
  - occupied-site mask with the occupier faction per site;
  - site kinds (first holding, holding 2–3, Free City, Seam Town, camp);
  - a siege marker per site (attacker faction, progress, required, vigil-paused flag);
  - immunity-until bell;
  - the March id.
  The current overview record has owners and site states only (contract §9.3).
- **Per March:** controller, contested flag, controlled-since bell, Herald's Call flag per faction.
- **Events:** horn, siege broken, siege completed (occupy, capture, raze), liberation (by host, by expiry), Seam Town or Free City claimed, March banner change, founding by Settler, Rally.
- **Threat rings:** derivable on the client from public departures. It needs the departure's origin province and arrival bell (both already logged) and the reach rule (≤ 4 provinces, ≤ 32 steps).
- **Visual grammar (proposal):**
  - occupier colour hatched over the owner's sigil;
  - a pulsing ring with a progress arc for a siege;
  - grey towers for neutral towns;
  - a striped fill for contested;
  - the March outline in the banner colour at world zoom.

---

## 9. Contradictions and dull spots in the current text, with fixes

| # | Where | Problem | Fix |
|---|---|---|---|
| **C1** | §3.1 + D9 + web `majorityOwner` | The map colour is ownership; first holdings are in-wedge and never move, so the map is frozen by construction | Control layer with occupation (§3.1) |
| **C2** | §6.2 sealed destination versus §6.3 "a siege is declared publicly"; kernel start window of 72 bells; the simulator declares at departure | Declaring before arrival publishes the target before the sealed march lands | Declare only from the hex (§3.4) |
| **C3** | §6.6 Frontier protection; kernel `attacker_nearby` | A veteran plants holding 2 or 3 within 2 provinces of a newcomer band and unlocks sieges on it, which defeats "distant veteran armies cannot farm new cohorts" | Count only the attacker faction's first holdings, or holdings older than the target |
| **C4** | §3.1 heartland = rings 2–3, fixed | At 200 players (the M1 playtest size) every first holding fits in the 60 heartland sites per faction, so no siege is possible. At 1,000, about 36% are untouchable. At 50k it is negligible | Season parameter `heartland_max_ring`: 2 below 2,000 expected players |
| **C5** | §5.6 Dominion uses strength weight (garrison factor up to 1.5) and counts any March; β 0.44–0.55 | Turtling raises Dominion without conquest; spreading thin over home land wins | Control weight without garrison; contested-March weighting (§3.9) |
| **C6** | §3.5, §6.6 timers in absolute days | In a 7-day season nothing is released and Frontier protection outlasts the season | Season-relative presets (§3.10) |
| **C7** | §3.4 "a released first holding becomes a Free City site" versus M1 `ReleaseDormant` → released-free (ticketable) | Different map consequences: a Free City is a capturable town, a free site is just land | Free City for a window, then a free site (§3.10) |
| **C8** | §3.1 "Holdings 2–3 may be founded anywhere open" | No procedure; the simulator founds instantly; a 48-h shield on a forward salient is a free fortress | Sealed Settler march, lottery ties, 12-bell shield (§3.3) |
| **C9** | §6.3 occupation "while keeping a host in the province"; "Liberate by winning a clash there" | One host holds a whole province's occupations; liberation is undefined | Host on the hex; three liberation conditions; maximum length (§3.5) |
| **C10** | §6.3 a garrison never attacks (City retaliates ×0.5) | Walls and garrison alone never break a siege. Players will expect them to | UI copy: "walls delay, hosts defend"; Raise the levy as one tap |
| **C11** | §2.6 / T1 season end versus sieges | A siege that cannot finish is free horn spam with a returned stake | Refuse at declaration; no winner at the end (§3.11) |
| **C12** | §3.4 rings open at 55–65% fill | Free land never runs out, so war is optional and peaceful expansion dominates (1,183 Free City captures versus 38 PvP outcomes [sim]) | Seam Towns; Herald's Call; contested Dominion; Free Cities from dormancy |
| **C13** | M1 hosts keyed by holding generation | Capturing a holding 2 or 3 silently strands and destroys its armies elsewhere | Retire them to the first holding's reserve, or state the rule |
| **C14** | §5.4 occupation and capture move counted laurels; siege stakes are laurels | Laurels, stakers and pair rules are M2 money infrastructure | Gold stake and goods tribute now; keep pair-history bookkeeping so M2 plugs in |
| **C15** | Contract §9.3 overview | No control, occupation or siege fields | §8 hand-off |
| **C16** | `frontier-sim` `war()` and `expand()` | Targets only within 2 provinces of the largest garrison at a 3–4× margin, never coordinates, founds instantly, so PvP is rare. A bot demo on these policies will look static | New personas; Call-aware targeting (§6) |
| **C17** | §6.1 owner's side keeps 3 slots on its holding hex | Ambiguous while occupied | The owner's side keeps them; the occupier sits in a hostile slot |
| **C18** | Contract v1.12 shield rule "another faction's holding site" | Neutral towns are not a faction's holdings, but it is not stated | State it: a shielded newcomer may strike Free Cities and Seam Towns |
| **C19** | §2.6 "late joiners file in the outermost ring" plus Frontier protection measured from day 2 | In a 7-day preset almost everyone counts as "after day 2" only on day 3 or later. The day-0 wave fights unprotected, which is intended, but day-0 newcomers in a playtest are first-time players | Frontier-7 thresholds (§3.10) |

**Dull spots:**

- **D-1. Armies have nothing to do but camps.** In M1, 0.24 Departs per bot per day [measured]. Seam Towns and the Call give a daily target that is not another player's home.
- **D-2. A siege is a 6-hour wait.** Make the middle active:
  - each bell is a clash against the garrison, and both sides bleed;
  - relief can land at any bell and wins by holding one bell;
  - the horn shows the defender the latest useful relief bell;
  - the spectator watches the progress bar race the relief arcs.
- **D-3. Occupation is a passive tax.** It now costs a host on the hex, bleeds through supply when deep, and expires. The owner always has a play (levy, relief, or wait it out).
- **D-4. The score is abstract.** Dominion is tied to the visible banners, and the scoreboard says "Marches held".

---

## 10. Owner decisions needed

1. **Occupation moves control** (map, March banner, Dominion) while the first holding stays with its owner. Recommended: yes. This is how D9 stays.
2. **Occupation maximum length:** 48 h for Frontier-28, 12 h for Frontier-7. Recommended as listed.
3. **Seam Towns at genesis and per ring.** Recommended: yes.
4. **Heartland size as a season parameter:** ring 2 only below 2,000 expected players, and for the demo and playtest. Recommended.
5. **Capture at the 3-holding cap:** raze (recommended) or refuse.
6. **The victim's hosts of a captured holding:** retire to the first holding (recommended) or are lost (stated).
7. **Siege stake in this milestone:** 500 Gold (recommended) or wait for laurels.
8. **Frontier-7 timer preset** for the demo and the playtest. Recommended.
9. **Dominion weighting** (contested ×1, uncontested home ×0.25). Recommended, pending simulator β.
10. **Herald's Call** as the M3 stand-in. Recommended.

---

## 11. Risks

- **Snowball:** a large, coordinated faction occupies the wedge interiors of a weak neighbour. Mitigations: occupations cost hosts on hexes, bleed when deep, and expire; the heartland is inviolable; Rally. T4 measures it.
- **Newcomer churn** from early occupation. Mitigations: the shields, the Frontier protection fix, and the T6 metric.
- **Keeper cost:** besieged provinces never skip; each siege costs at least 36 full resolves. This needs a cost model before the demo.
- **Province storage:** 56 entries and 8 hosts per faction per province, with occupation hosts added. The storage-aware room (I-43) bounces arrivals, which is safe, but players will see "bounced" more often near fronts.
- **Doctrine balance:** sieges, walls (A, F), supply (D) and capture rules become live mechanisms. The doctrine gate must simulate them (DESIGN §4.1 "a gate row per mechanism").
- **Herding to the winner** on the picker map. In the no-money milestone, only the Frontier Grant counters it.
- **The demo looks scripted:** all actors are bots. The §6 personas must vary timing and targets, and the spectator should say "bots" honestly.

---

## Appendix A. The owner's two side questions, answered from the repository (other areas may go deeper)

**1. AI agents: which model, and who pays?**

- **In the Frontier, no language model decides any game action.** The operator's hidden players (Shades) run deterministic, open-source bot code. Its hash is committed at genesis, and the verifier replays it (DESIGN §7.1, §7.3). A language model may only write chat flavour, which has no game effect.
- The language model in the repository today is in the **v9 gateway** (`permutation-gateway/src/advisor.mjs`): `claude-haiku-4-5-20251001`. It only phrases what the game server already decided its AI members say, within a season budget, and falls back to the server's own sentence without a key. **The operator pays, with its own `ANTHROPIC_API_KEY`.**
- The conquest milestone's bots (`frontier-bots`) and the simulator use no language model. They cost only local CPU and, on a real cluster, transaction fees.
- **Who pays transaction fees and rent:** the relay fronts rent and a sponsored fee quota for every joiner, human or Shade, so funding graphs do not separate them (DESIGN §2.2, §7.1). In M2, Shades pay entry with operator USDC.

**2. What x402 does, concretely.**

- x402 is "HTTP 402 Payment Required" used as a payment handshake.
  1. The client asks to join (`POST /x402/join`) and gets **402 with the price and how to pay**.
  2. It signs the program's own Register/Join transaction, which moves exactly the entry fee into the season vault and records the member in the same instruction.
  3. It retries with that transaction in the `X-PAYMENT` header.
  4. The gateway, as facilitator, checks it, adds only its fee-payer signature (it cannot change the amount or the payee), submits it, and answers **200**.
- So **an AI agent or a browser wallet can pay and join in one ordinary HTTP exchange**, with no special SDK, and people and the operator's bots take the identical path (`permutation-gateway/src/routes/x402.mjs`, v9).
- In the Frontier it is the default join path once money exists (DESIGN §2.2, §11.3). **In this conquest milestone joining is free, so x402 is not used yet.** It returns with M2's citizen fee and laurel stake.
