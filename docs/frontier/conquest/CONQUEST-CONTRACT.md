# MC "Contested Ground": the conquest milestone's integration contract

**Version v1.1** (2026-10-01, review revision of v1.0; every change and every rebuttal is in §17 *Revision notes*, R-01…R-26).

- **Date:** 2026-10-01. **Role:** conquest-milestone (MC) integration architect. **Status:** normative for every MC implementer from the moment the owner accepts it. Owner decisions still open are listed in §15. Each has a working default, and implementers build to it.
- **Owner decision this implements (2026-10-01):** build the territory contest, the game's core, **before** M2 money, so that the faction map changes through play. Keep "the first holding is never taken" (D9) unless the design check shows the map cannot move enough with it. Everything ends with the season. Money (fees, stakes, payouts) stays in M2. Faction scores are game points. Governance (Ministers, War decrees, March truce and hostility votes) is M3, so MC runs on stated defaults.
- **Base:** branch `codex/frontier` at **`d11d058`** (worktree `.claude/worktrees/frontier-integ`; `d11d058` is a docs-only commit over `aae8617`, the M1 exit plus the design chat's `ui-shell` merge). Called **`CQ0`** below. Planning phase: nothing in the repo was changed, no commit, no push, no server, no chain transaction.
- **Merged from:** the five area designs in `scratchpad/frontier/conquest/design/`:
  - `rules.md` (kernel rules, lab `lab/rules-mapmove`);
  - `program.md` (on-chain program, lab `lab/program-conquest`);
  - `offchain.md` (keeper, herald, verifier, bots, simulator, stack, web data, lab `lab/offchain-mapmove`);
  - `sim-balance.md` (map movement and balance, lab `lab/simbal`);
  - `game-design.md` (the loop, protection, defaults).
- **Normative above them:** DESIGN rev 3.1; M1-CONTRACT v1.13 (the frozen M1 ABI and every M1 convention not changed here); DECISIONS.md; the owner decision above. Where this contract and an area design differ, **this contract wins**. §2 lists every conflict and its resolution.
- **Evidence it relies on:**
  - M1 exit (`M1-EXIT-NOTES.md`): release `.so` `d85e1bd7…2281`, ResolveFromInputs worst 271,673 CU of 290,000, the 7-day 1,000-bot real-round season, 1,712 Departs in 7 game days (≈ 0.24 per bot per day) [measured].
  - The three map-movement labs, which agree that **holding-based colour cannot move the map** [sim]:
    - rules lab: 0–1% of provinces ever foreign under M1 rules, 7 days, 1,000 wallets;
    - off-chain lab: 0 March flips after day 1, even with relaxed siege limits;
    - balance lab: 0.2 March changes a day at 10k wallets / 28 days, with every M1-excluded feature on.
  - The **keep model** (balance lab, `lab/simbal/out/cand.md`): 8.2 March changes a day and 36% of provinces changing hands at 1k/7d; 10.1 a day and 47% with 1,000 bots over 7 days; doctrine band 6/6 when keeps score nothing; bot criterion worst cell 0.986 (M0 control 0.985) [sim].
  - Program costs (LiteSVM probe on SBPF v2): conquest step +6.6k CU worst in a resolve, FoldMarch 8,335 CU, DeclareSiege checks 6,664 CU, SettleCapture bookkeeping 8,084 CU [measured, lab].
- **Tags:** [measured], [sim], [model], [estimate], [design], as in DESIGN. **MUST / MUST NOT** bind implementers; "should" is advice.

---

## 0. The contract in one page

1. **The faction map moves through captured keeps.** Every province from ring 2 outward gets one **keep**: a fort on a fixed non-site tile, owned by a faction and by no wallet. A keep changes hands when a hostile faction holds its hex for **72 bells (12 h)** with no defender of the keep's faction on it. **The map colour of a province is its keep's holder; a March's banner is the faction holding more than half of its open keeps.** This is the one rule the labs show moves the map with D9 kept, without a snowball, with the doctrine band intact (6/6) and with the bot criterion unchanged (§3.2).
   **What to expect (v1.1, R-12):** movement happens mostly on the borders and goes back and forth (about 40% of captured keeps return to the previous holder within 3 days); faction shares stay at about 15–20% of provinces; heartland interiors never change [sim]. Keeps pay nothing yet, so human players have no built-in reason to take them; bots do because they are programmed to (R-10, OD-14).
2. **The holding contest is built too, with D9 kept.** I-17's exclusions are lifted:
   - holdings 2–3 as **outposts** (site tickets at the front);
   - **sieges** declared from the hex by the hex's **lead host** (§3.4);
   - **occupation** of first holdings (never taken);
   - **capture** of holdings 2–3 and of **Free Cities** (one neutral town per province from ring 4), into a holding slot **reserved at the horn** (no raze in MC).
   Holdings carry the personal stakes; keeps carry the map.
3. **No money, no governance.** No laurels, stakes in SOL, tribute or payouts. The siege stake is 500 Gold. Dominion (DESIGN §5.6, from holding strength weight) is folded on chain per March as **game points**; keeps score nothing.
   Governance defaults: **Rivalry between every pair, no War decree, heartland sieges and heartland keeps disabled until M3**, no truce, hostility, treaty or alliance, and auto-reinforce only as client or bot automation.
4. **Everything ends with the season.**
   - No siege that cannot finish before `end_bell` may start.
   - Nothing completes at or after `end_bell`; open sieges lapse with the stake returned.
   - The control map of `end_bell − 1` is the season's final map.
5. **ABI v2, a strict superset of the frozen M1 ABI.**
   - **Province grows 4,096 → 4,736 B** (a 640-B conquest block); no v1 offset moves.
   - **One new account kind:** MarchState.
   - **New tags 0xA0–0xA7, errors 62–78, log kinds 80–89.**
   - `RULES_VERSION_FRONTIER` 10 → 11 and a new `RULESET_HASH`, so a v2 program never acts on an M1 season. The paused `m1-exit` stack keeps its own `.so`.
   - **Staged (v1.1, R-16):** Wave 1 only *adds* v2 names beside the v1 ones; Wave 2 switches the program and the node crates; Wave 3 switches the web, SDK, WASM and verifier. Readers dispatch on the version, so M1 seasons and fixtures stay readable. `codex/frontier` is not fast-forwarded until the MC exit.
6. **Off-chain:**
   - **keeper:** per-bell resolves for contested provinces; hourly FoldMarch; settles; season-end flush;
   - **herald:** a per-bell **control layer** `PSFCT1`, the faction map the design chat renders, plus an overview v2, siege files, conquest events and standings;
   - **verifier:** V14–V22 and tamper classes T25–T48;
   - **bots:** a deterministic faction campaign planner, plus conquest personas;
   - **simulator:** the MC rules, a `mapmove` gate with negative controls;
   - **stack:** a `--conquest` preset on ports 41300–41999 only, and new criteria 10–13.
7. **Web:** data and controller modules only, plus `docs/frontier/conquest/HANDOFF-UI.md`. The design chat's files are never touched (§4.5).
8. **Delivery:** 5 waves, 23 units, exclusive file ownership, an integrator on `frontier/cq-integ`, exact gate commands (§12).
   **Exit (§13):** a 7-day accelerated 1,000-bot season on real drand rounds (archive at G0 1788998400) with the release `.so`, chaos and the adversary schedule, in which:
   - **the faction map measurably moves** (criterion 10, §13.4);
   - every M1 criterion still passes;
   - the verifier passes, and every tamper class fails.
9. **Estimate (§14):** ≈ 46 engineer-weeks. That is ≈ 8 calendar weeks for a human team of six plus an integrator. At the agent pace M1 actually ran, ≈ 5–8 calendar days (50% ≈ 6.5 days, 80% ≈ 9.5 days). The main risk is a stack season whose map moves less than the simulator's: the simulator's bots march far more often than M1's stack bots did, so Wave 1 re-measures with a bot profile matched to the real cadence (R-11).

---

## 1. Scope and non-goals

**In (rules and program):**

- Keeps and the keep contest; the province and March control layer.
- Holding sieges: DeclareSiege from the hex, progress, failure, immunity, completion.
- Occupation of first holdings, with tenure, liberation and Respite.
- Capture of holdings 2–3 and Free Cities into a slot reserved at the horn; the victim retires its own hosts home.
- Genesis Free Cities. (v1.1: released dormant holdings become free sites, as in M1, not Free Cities; R-07, R-14.)
- Holdings 2–3 as outposts (FileOutpost and SettleTicket).
- Dominion folded per March (FoldMarch), with a minimal **capture-credit rule** against ping-pong farming (§3.6, R-06).
- Season-relative timer presets (Frontier-7, Frontier-28).
- The governance defaults and the season-end rules.
- ABI v2 and its gates (G1–G14 extended).

**In (off-chain and web):**

- The keeper conquest duties and the herald control layer and files.
- Verifier v3, bots (campaign planner, behaviours, personas), the simulator conquest preset and `mapmove` gate.
- The stack `--conquest` preset, adversary holds and criteria 10–13, with a report `mapmove.json` and a static `mapmove.svg` (day-end control maps).
- Relay shapes and quotas.
- Web data modules, WASM exports, the hand-off note and its fixtures.

**Out of MC (layouts reserve room; the milestone named is the target):**

| Item | Where it goes | Why it is not needed for the map to move |
|---|---|---|
| Laurels (emission, reward index, banking), the 50% occupation share, the 25% capture transfer, the full wallet-pair rules, refugee kit | M2 | Laurels are M2 money infrastructure; the map does not read them. **Corrected in v1.1 (R-06):** Dominion *does* read captures (36 control-bells each), so MC carries a minimal in-season pair rule, the capture-credit rule of §3.6; the cross-season wallet-pair rules stay in M2 |
| Fees, stakes in money, x402 joins, payouts, ScoreBoard / TallyDominion on chain | M2 | money |
| War decrees, heartland sieges, March truce and hostility, Peace/NAP/Alliance, Ministers, Wardens, Mandates | M3 | governance; MC runs on the §3.9 defaults |
| On-chain auto-reinforce, delegated command | M3 | an outcome-changing keeper write would need class W (§2, K-11) |
| Tribute (20% of production), raids, caravans | M3 | moves goods; changes no control |
| Sealed Settler marches for holdings 2–3 (tags reserved 0xA8) | M3 | outposts by ticket do the same job with existing machinery (K-07) |
| Seam Towns | not planned | genesis Free Cities cover the purpose (K-08) |
| Herald's Call **rewards** | M3 (Mandates) | MC publishes the Call as display data only (§3.10) |
| Keep Dominion, contested-March weighting of Dominion | owner decision after a doctrine re-tune | breaks the doctrine band (1 of 6) [sim] |
| Replay-page data (`frontier/demo-replay`) | hand-off only (§9.4); an add-on unit if the owner asks (OD-13) | the branch is not on `codex/frontier` |
| PosturePDAs, Skirmish, Arena, Relic Sites, Engine | M3 / later | unchanged from M1 |

**Exit:**

- Every new or changed instruction within budget on SBF at adversarial fill.
- A pre-funding test for every new creation path and a forgery test for every new keyed read.
- Property tests P1–P13.
- An in-process conquest day (test preset `MC_TEST`) with a keep flip, a siege, an occupation and a capture.
- The 7-day accelerated real-round season (§13.4) with criteria 1–13.
- Verifier PASS, and every tamper class FAIL.
- `npm test` green, the design chat's files untouched.

**Then, only with the owner's approval,** a private devnet playtest of the conquest rules (no money).

---

## 2. Conflict register (resolved)

| # | Conflict between the area designs | Resolution | Reason |
|---|---|---|---|
| K-01 | **Source of the faction map.** rules: holding strength weight with occupation, Free Cities and outposts. game-design and program: tier-weight control with occupation. off-chain: a province-level field contest (CD-1). balance: a capturable keep per province. | **Keep model** (§3.2). Province colour = keep holder; March banner = more than half the open keeps. Holding-weight control is kept only for Dominion (K-03). | Holding-weight control moved 0–2% of Marches in every lab; rules' 6.8–14% "ever foreign at one hourly fold" is mostly neutral → faction and one-hour flicker. The keep model moved 42–46% of Marches in 7-day 1k seasons with no snowball (largest share ≤ 20.1%) and passes the doctrine and bot gates [sim]. It is also the "province-level contest decided at the resolve" that off-chain CD-1 asked for. |
| K-02 | D9 | **Kept.** First holdings are occupied, never transferred. | Every lab: relaxing D9 moves the map no further (−1.2 to +0.4 points; 10k: 0.3 vs 0.5 March changes a day) and doubles homes lost [sim]. |
| K-03 | Dominion. game-design: tier weight with ×1 / ×0.25 contested weighting, in quarter-hours. rules: strength weight, 6 control-bells an hour, 36 per capture. balance: keeps must not score. | **DESIGN §5.6 as measured.** Strength weight; an occupied first holding counts for the occupier; dormant holdings count; Free Cities are a neutral side; strict majority. 6 control-bells per controlled hour, 36 per capture. **Keeps score nothing.** The contested weighting is deferred. | The doctrine band was measured 6/6 only on this configuration. Keep Dominion breaks it (1/6). The weighting is unsimulated (β pending). |
| K-04 | Siege declaration. rules: kernel horn with a 72-bell start window before arrival. game-design and program: declare only from the hex. | **From the hex** (§3.4). The start window never applies. | A horn before arrival publishes a sealed destination (GD C2). |
| K-05 | Siege stake. rules: 5 laurels. game-design and program: 500 Gold. | **500 Gold** from the declarer's source holding. | No laurels in MC (K-16). |
| K-06 | Immunity after a failed siege. game-design: always 36 bells. program F1: only when the defender's side broke it. | **Program F1, scoped per faction (v1.1, R-05).** Immunity (36 bells) and the stake to the defender only when the failing bell had a defender present or the owner's side held the hex; otherwise no immunity and the stake is burned. The immunity bars **only the besieging faction**; other factions may besiege at once. | F1 closes farming by a deserted siege; the faction scope closes farming by a staged "defence" against an alt of another faction. |
| K-07 | Holdings 2–3. game-design and program: sealed Settler march (+ SettleFounding). rules: site tickets with the outpost rule. off-chain: tickets anywhere open. | **Outposts by ticket** (§3.8) with the outpost rule, ring outside every heartland, range 3, Town prerequisite, 20% land gate. Settler marches go to M3; tags reserved. | No kernel or clash change; cohort lottery already makes speed worthless. Outposts measured +3–4 points of foreign control in the rules lab. Saves ≈ 1 engineer-week and a clash-path risk. |
| K-08 | Neutral land. game-design: Seam Towns per seam per ring band. rules: one genesis Free City per province from ring 4. | **Genesis Free Cities** (rules' version) as a season parameter (`free_city_min_ring`, 0 = off). **v1.1 (R-07, R-14):** released dormant holdings become free sites as in M1, **not** Free Cities. | Measured (F4); one placement rule instead of two. Released-home Free Cities were capturable loot, alt-farmable, and the source of a live-record state change (Free City expiry under a siege). |
| K-09 | Occupation length. rules: 288 bells, then 288 bells of Respite. game-design: 48 h (Frontier-28) / 12 h (Frontier-7). program: `occupation_max_bells` 288 / 72. | **Season parameters:** Frontier-28 tenure 288 / Respite 288; Frontier-7 tenure 72 / Respite 72. **v1.1 (R-05):** Respite is granted only on expiry or when the owner's faction liberates the hex, and it bars only the occupier's faction. | The 7-day season must not lock a home for a quarter of the season. An occupier that walks away must not hand its friend a shield. |
| K-10 | Liberation. game-design and program: occupier host gone, owner wins, or maximum length. rules: occupier faction loses the hex. | **Rules' predicate:** at the first resolved or quiet bell after which the occupier's faction does not hold the hex, or at tenure. | One predicate covers "host gone" and "owner won"; it needs a host on the hex (GD C9). |
| K-11 | Auto-reinforce. off-chain: optional on-chain launch (D class if decided at the declaring bell). rules and game-design: client/bot automation. | **Off-chain only** (client, bots, opt-in relay). No SetAutoReinforce instruction, no REINFORCE log. | An on-chain transfer would be an outcome-changing keeper write. |
| K-12 | Tribute (CollectTribute). program: in. rules: deferred. | **Deferred to M3**; tag 0xA4 reserved. | Moves goods only; no control effect. |
| K-13 | March fold under lag. rules: the Province refuses to resolve while an unfolded sample would be dropped (`MarchFoldPending`). program: a 6-hour ring; an hour whose sample was overwritten folds as "lost". | **Program's ring (K = 6) with lost hours.** | Keeps provinces independent; a lost hour needs a MarchState held more than 5 hours at the D price (≈ $75k–204k per lost March-hour) [model]. |
| K-14 | Snapshot weight. program: tier weight in tenths. rules: strength weight. | **Strength weight in centi-units** (`floor(laurel::strength_weight / 10,000)`, u16 per side). | The doctrine gate measured strength weight; ≤ 3,600 per province fits u16. |
| K-15 | Log kinds and tamper numbering. program: logs 80–88, T23–T30. off-chain: logs 80–92, T25–T36. M1 already uses T1–T24 (plus T1b, T6b, T23b, H1, H1b, V9a). | **Logs 80–89 as §6; checks V14–V22; tampers T25–T48 as §8.5** (v1.1 adds V22 and T39–T48). | No collision with M1's T23/T24. |
| K-16 | Laurels in MC. rules: laurels exist as game points (reward index, banking). game-design and program: not in MC. | **No laurels in MC.** | M2 infrastructure (reward indices, stakes, pair rules); the owner's "game points" are satisfied by Dominion and standings. |
| K-17 | Shield for a 7-day season. off-chain CD-3: 24 h. game-design: 12 h. | **24 h** in Frontier-7 (first holdings; 24 h also after day 2); outposts 2 h. | Days 1–2 are not static, and newcomers keep a full day. |
| K-18 | Heartland size. game-design: ring 2 below 2,000 expected players. Others: rings 2–3. | **`heartland_max_ring` season parameter, default 3** in both presets (the measured configuration). | The balance lab measured rings 2–3; owner may lower it (OD-7). |
| K-19 | The keep's garrison after capture. balance: 50% of the capturing host stays. | **v1.1 (R-01), the simulator's rule exactly:** the **donor** is the single largest non-civilian host of the taking faction resident on the keep tile (most troops, then lowest host id). `floor(troops × keep_garrison_bps / 10,000)` of it becomes the keep's garrison; the donor then returns home with the rest (a retire-style Leave in the same bell); a rest below `MIN_HOST_TROOPS` joins the keep too. Other hosts stay. The keep's troops are ≤ `MAX_HOST_TROOPS` in every reachable state (≤ 15,000 + `MIN_HOST_TROOPS` from a handoff, ≤ `keep_home_guard` at opening, which CreateSeason bounds). | Measured (kgar 50%: ping-pong 40–45%; 25% → 70%) with exactly this rule (`lab/simbal/frontier-sim/src/sim.rs` 3811–3830). v1.0's Σ over every host could reach 90,000 troops and make every later clash of the province fail `clash::validate` (CL-01): a permanent freeze. |
| K-20 | Siege daily cap. rules: ≤ 2 active per citizen. program: `sieges_per_day` 4. off-chain relay: ≤ 3 a day. | **≤ 2 DeclareSiege per citizen per game day** on chain; the relay quota is the same number. | A daily counter needs no decrement at siege end, so the resolve still writes only the Province. |
| K-21 | Where the keep sits in the clash. balance: `MAX_GARRISONS` 12 → 13. | **13 garrisons:** 12 sites + the keep. The camp fights only while fewer than 12 site garrisons stand, exactly as in M1. **v1.1 (R-16):** `clash::MAX_GARRISONS` stays **12** (it is in `ruleset_hash_input`, sizes `clash_model`'s camp rule and is asserted equal to `SITES_N`); a new `MAX_GARRISONS_WITH_KEEP = 13` is used only by `validate` and the keep path. | M1 outcomes stay bit-identical in provinces without a keep (rings 0–1) and in every M1 digest test; the M1 `RULESET_HASH` input is unchanged. |
| K-22 | Province size. program: 4,608 B (512-B block). | **4,736 B (640-B block)**, to hold the keep record and per-faction keep counts. | +0.00325 SOL refundable per province (§5.2). |
| K-23 | Free City and holding control during capture. | The site mirror flips at the completion bell (owner faction := captor, `gen += 1`); SettleCapture only does the bookkeeping. | Bell-exact map; lag only waits (program §6.3). |
| K-24 | (v1.1, R-03) Who may declare a siege. v1.0: the first DeclareSiege to land. | **The lead host rule:** only the owner of the hex's lead host (the largest non-civilian host of the faction holding the hex, then the lowest host id) may declare; new code `NotLead` (78). | Rosters change only at a bell's settle, so at any moment exactly one wallet may declare: no transaction race, and the capture goes to the largest contributor. |
| K-25 | (v1.1, R-04) Capture or raze. v1.0: decided at SettleCapture from `holdings_n` and the escrow. | **Holding slots reserved at the horn.** Each citizen has slots 1–3; DeclareSiege on a capture target reserves the lowest free slot and escrows one `rent(1,280)` for it; FileOutpost counts reservations. A capture always lands in its reserved slot. **No raze in MC.** | The outcome is fixed at the completion bell; settle order and settle timing choose nothing. |
| K-26 | (v1.1, R-06) Capture farming between a whale's wallets. | **Capture credit:** a capture adds to `captures_by` (and so to Dominion and the standings) only when the victim held the holding at least `capture_credit_min_bells` (Frontier-7 144, Frontier-28 288); an uncredited capture also loses the holding's stores. | Recapture ping-pong (≈ 36–100 bells per cycle) earns nothing and moves no goods. |
| K-27 | (v1.1, R-02) RetireHost timing. v1.0: N, anyone, any time. | **The victim's own call** (its wallet or session) during the season; anyone only after `end_bell`. A host never retired keeps fighting for its faction. | No third party chooses when a defender leaves a hex. |

---

## 3. Rules (normative)

### 3.1 Terms

| Term | Meaning |
|---|---|
| **keep** | One per province of ring ≥ 2. A faction-held fort on the province's keep tile. Owned by no wallet. Fights as a garrison (walls on, retaliates only). |
| **keep tile** | `keep::keep_tile(terrain, sites)`: the lowest-index passable tile that is not a site tile. The terrain carve makes it reachable from every gate and site (DESIGN §3.2). |
| **holder** | A keep's faction (0–5). |
| **contender** | The hostile faction currently counting progress on a keep. |
| **province control** | The keep's holder. Rings 0–1 have no keep: the Concord shows *neutral*, a Seat province shows its seat faction and is flagged *seat*. |
| **March banner** | Faction f when f holds strictly more than half of the March's open keeps; otherwise *contested*. A March with no open keep has no banner. |
| **Dominion lead** | DESIGN §5.6's holding-strength-weight controller of a March, folded hourly (§3.10). **Points only, never a map colour** (v1.1, R-15): the map shows keeps; the Dominion lead appears only in standings. It can differ from the banner. |
| **holding siege** | A siege on a holding or a Free City site (§3.4). |
| **lead host** | (v1.1) On a hex, the non-civilian resident host of the faction holding the hex with the most troops, then the lowest host id (state 1, `from_bell ≤ now_bell`, no pending op). Only its owner may declare a siege there. |
| **slot** | (v1.1) A citizen's holding positions 1–3. Slot 1 is the first holding. A slot is *free* when it holds no holding, no open outpost ticket and no capture reservation. The slot is the holding's `order` (its Dominion weight factor). |
| **keep donor** | (v1.1) The lead host of the taking faction on the keep tile at the bell a keep is taken (§3.2 step 5). |
| **heartland** | Rings 2..=`heartland_max_ring` of a faction's own wedge (default 3). |
| **Frontier-7 / Frontier-28** | The timer presets of §3.12. The exit runs Frontier-7. |

### 3.2 Keeps and the faction map

**Opening (OpenProvince):**

- Each ring ≥ 2 province gets its keep at genesis or ring opening.
- It is held by the province's wedge faction, with `keep_home_guard` troops (default **100**).
- It carries flag `heartland_safe` when the province is in its holder's heartland. A heartland keep can never be contested in MC: progress never counts there.

**The contest, inside every resolved or quiet bell b (kernel `keep::advance`).** The report comes from the clash's `GarrisonResult` for the keep (`holders`, `defender_present`), or from the quiet model in a skip. Rules in order:

1. If `heartland_safe`, or `b < consolidated_until_bell`: nothing counts. Any contender is cleared without an event.
2. If `defender_present`, or no hostile faction holds the hex (`holders == 0`): a running contest is **broken** (event `KEEP_BROKEN`). Contender and progress are cleared.
3. *(M3 only; unreachable in MC.)* If two or more hostile factions hold the hex: the contest **pauses**. Under Rivalry the clash's field step keeps exactly one hostile-free set per hex (`clash.rs` 1466–1471), so `holders` has at most one bit and this step never fires in MC. It stays in the kernel for M3 alliances; `KEEP_PAUSED` (code 10) and keep flags bit 1 are reserved (R-09).
4. If exactly one hostile faction f holds the hex:
   - if `contender == f`, `progress += 1`;
   - otherwise `contender := f`, `progress := 1`, `contest_from_bell := b` (event `KEEP_CONTEST`, the keep's public horn).
5. If `progress ≥ keep_bells` (default **72**), the keep is **taken** at b (event `KEEP_TAKEN`):
   - `holder := f`; the old garrison is discarded;
   - **the donor** (v1.1, K-19) is f's lead host on the keep tile after the clash: `troops := floor(donor.troops × keep_garrison_bps / 10,000)` (default 50%); the donor gets a retire-style pending Leave at b (it returns home with the rest at the bell's settle, credited by the return settle as RetireHost's is); if the rest is below `MIN_HOST_TROOPS` (100), it joins the keep too and the donor's entry is removed;
   - **cap:** `troops ≤ MAX_HOST_TROOPS` holds by construction (a host is ≤ 30,000); `keep::open`, `keep::garrison` and `keep::advance` assert it and return `KeepError::TroopsAboveCap` instead of building a garrison `clash::validate` would refuse;
   - `consolidated_until_bell := b + 1 + keep_consolidate_bells` (default **288**);
   - `gen += 1`, `changes += 1`, `since_bell := b + 1`, `last_taken_from := old holder`;
   - contender and progress cleared.

**What the keep is and is not:**

- **No declaration and no stake.** The first counted bell is public (`KEEP_CONTEST` in the CONQUEST log and the herald), so defenders get at least 71 bells (≈ 12 h) to relieve.
- **The garrison never regrows.** No instruction adds troops to it; hosts standing on the hex defend it. A keep with 0 troops still needs 72 held bells.
- **A keep pays nothing:** no goods, Works, laurels or Dominion, and it gives no combat or economic bonus. So taking land does not make a faction stronger (no snowball loop) and does not pay a script (bot criterion).
- **The downside, stated plainly (v1.1, R-10):** a human player has no built-in reason to take a keep. The simulator's keep attacks are a separate dice roll per session (`keep_aggr`), not a value judgement, so the measured movement assumes players and bots choose to attack keeps. MC therefore adds **display-only recognition** (no points, no goods, no bonus, so the bot criterion and the doctrine band are untouched): per-player *keeps taken* (the citizens with a non-civilian host on the tile at the taking bell) and *keep-bells held* (bells a citizen's host stood on a keep its faction held while a contender counted), in `/h/standings/players.json` and `final.json`, with Chronicle titles (Warden of the Marches, Breaker of Keeps). Whether keeps should score (March-banner hours at a small Dominion weight) is **OD-14**, measured in CQ1 and off by default.
- **A keep is a forward base already:** M1's Depart looks the host up in the Province it names (`proc/host.rs`, M1 §5.11 step 1: "province = the host's"), so a host that stays on a taken keep can march on from there (≤ 72 bells ahead). Only the simulator lacks this (it sends the capturer home and searches within 2 provinces of holdings); CQ1-B measures it (`--forward`, R-12).
- **Reachability:** any faction's hosts may target a keep tile, including from a shielded holding. A keep is not "another faction's holding", so M1's Reveal step 6 already allows it.
- **Clash:** the keep is the 13th garrison of its province (K-21). Garrison id `u64::MAX − 0x1_0000 − gen`, faction = holder, walls on, posture Hold, troops × 1,000 milli-troops. A keep with hostile hosts on its hex and troops > 0 is never quiet.

**Why these numbers [sim, `sim-balance.md` §6.3]:**

| Knob | Value | Effect of changing it |
|---|---|---|
| opening guard | 100 | the dominant knob: 1k/7d March changes a day 8.2 / 4.4 / 2.4 / 0.0 at 100 / 200 / 300 / 1,000 |
| contest length | 72 bells | spans waking hours in any time zone; 36 and 144 change little |
| consolidation | 288 bells | ping-pong (re-taken within 3 days) 60–75% → 40–45% |
| garrison left | 50% of the donor | 25% raises ping-pong to 70%; 100% cuts captures by 30% |
| keep interest (`keep_aggr`) | 1.0 in the simulator | 0.5 at 1k/28d: March changes 3.0 → 2.9 a day, keep captures 44.7 → 34.3 a day, provinces ever changed 42% → 38% (`out/k1_1k28.md`); 0.25 and the 7-day presets are measured in CQ1 (R-10) |

### 3.3 March banner and the control layer

- `control::march_banner(keeps of the March's open members)`:
  - f if `2 × held_by(f) > open_keeps`;
  - otherwise contested;
  - a March whose seven members are all unopened, or all rings 0–1, has none.
- The **faction map** is the per-province control and per-March banner of the herald's `PSFCT1` file for each bell (§8.4). It is a pure function of the Provinces' bytes: keep records and site mirrors. The herald, verifier, simulator, bots, stack report and WASM all call the same kernel functions (`control::*`), so the demo map, the report and the verifier cannot disagree.
- **No on-chain banner account.** The banner is derived. Dominion is the on-chain fold (§3.10).

### 3.4 Holding sieges

**Declaration (`DeclareSiege`, P), from the hex.** Checks, in order. Codes are §5.3's.

1. Season Running, `now_bell < end_bell`.
2. The declarer's source holding (it pays the stake) is final and passes the capture lock (§5.8).
3. The target site is in state 1 (holding) or 5 (Free City) (`NotBesiegeable`).
   - Holding: the Holding account is at the mirror's generation (`CapturePending`).
   - Free City: no Holding account exists.
4. **A non-civilian host of the declarer's own holding is resident on the target's site tile** (state 1, `from_bell ≤ now_bell`, no pending op), else `NotOnHex`. Only a host that has won the field there can sound the horn. **v1.1 (K-24):** that host must be the hex's **lead host** (§3.1), else `NotLead` (78). The program ranks the Province's current entries on the tile; rosters change only at a bell's settle, so within any bell exactly one wallet can pass this check, and the order in which DeclareSieges land cannot change the result.
5. The site's conquest record is kind 0 (`SiegeBusy`), and nothing is owed on it: no stake and no reservation release (flags bits 1, 2, 5; `StakeUnsettled`). For a holding, the record's immunity does not bar the declarer's faction: `barred_faction ∉ {attacker, ALL}` or `immune_until_bell ≤ now_bell` (`Immune`; this covers post-siege immunity, post-capture immunity and Respite, §3.5).
6. The citizen's declarations today < `sieges_per_day` (default **2**; `SiegeCap`).
7. A capture target (order 2–3 or Free City) needs a **free slot** (§3.1): none of slots 2–3 holds a holding, an open outpost ticket or a capture reservation (`HoldingsFull`). The lowest free slot is **reserved** for this siege (v1.1, K-25).
8. `siege::may_besiege` v3 with relation Rivalry and no March flags:
   - Seat → `ReservedSite`;
   - same faction → `Friendly`;
   - shielded and not dormant → `Shielded`;
   - Frontier protection, where "nearby" counts **only the attacker faction's first holdings** within 2 provinces (the declarer names that Province and site) → `FrontierProtected`;
   - the owner's heartland → `Heartland`.

   A Free City skips 8. For a player target, the declarer's own first holding must not be shielded unless dormant (`Shielded`).
9. `required = siege::required_bells(walls at bell_start(now), 0)`, which is 36 + walls/50, ≤ 60. The siege must be able to finish before the season ends: `siege::can_complete_before(required, snapshot vigil, now_bell + 1, end_bell)` for a holding, `end_bell − (now_bell + 1) ≥ required` for a Free City; else `TooLate`. **v1.1 (R-08), bounded:** `can_complete_before` returns true at once when `end_bell − (now_bell + 1) ≥ 288` (any 288 consecutive bells hold ≥ 96 bells outside a vigil snapshot with at most one pending change, a kernel-proved bound pinned by `cq_vigil_window_bound`); otherwise it scans forward bell by bell and stops when `required` bells are counted or the range ends: **≤ 287 `covers()` calls**, whatever the horizon or the schedule.
10. The source holding pays `siege_stake_gold` (default **500 Gold**, `Insufficient`). For a capture target, the payer tops up the Citizen's holding escrow to **`rent(1,280)` × (open outpost ticket + capture reservations, this one included)**, as FileTicket does (v1.1, K-25).

**Effects:**

- The record becomes a siege: attacker faction, `held = 1`, `progress = 0`, `required`, the owner's **vigil schedule snapshotted at the horn** (none for a Free City), declared bell, declarer tag, source Holding key, and for a capture target the **reserved slot** (2 or 3).
- The Citizen marks the slot reserved (§5.2.3).
- Log `SIEGE_DECLARED` (the public horn). The ETA is not on chain; the herald and `fconquest.mjs` compute it with the kernel (v1.1, R-08).

**Progress** (kernel `Siege::advance` / `advance_quiet`, unchanged rules, inside every resolved or quiet bell b > declared bell):

- +1 only while all three hold:
  - the attacker faction holds the site's hex (`holders` bit);
  - no defender is present;
  - `bell_start(b)` is outside the snapshotted vigil (always, for a Free City).
- The siege **fails** at the first counted bell where the attacker faction does not hold the hex.
- It **completes** when progress reaches `required`.

**Failure** (event `FAILED`):

- If the failing bell had `defender_present`, or the owner's side held the hex ("broken by the defender"):
  - the stake is owed to the target holding **at its current generation** (`owed_gen`);
  - the holding gets `immune_until_bell = b + 1 + immunity_bells` (default **36**) **against the besieging faction only** (`barred_faction := attacker`; v1.1, K-06).
- Otherwise (the besiegers left): no immunity, and the stake is burned.
- A Free City's stake is always burned.
- A reserved slot is owed back (flags bit 5): SettleSiege (N, anyone) releases it and refunds one `rent(1,280)` to the escrow's funder.

**Completion** at bell b: occupation (§3.5) for a first holding (the reserved slot, if any, is released the same way); capture due (§3.6) otherwise.

**Season end:** a siege still active when the season ends lapses. SettleSiege returns the stake to the source holding and releases the reserved slot.

### 3.5 Occupation of a first holding (D9 kept)

- **Starts** at the completion bell b of a siege on an order-1 holding.
  - The occupier is the attacker's faction; the record keeps the declarer as `actor`.
  - **Ownership never changes.** The owner keeps building, training, mustering, garrisoning and marching. The owner's side keeps the holding hex's 3 owner slots; the occupier sits in a hostile slot.
- **Control effect:** for Dominion points, the holding's strength weight counts for the occupier's faction (§3.10). It has **no effect on the map colour**: an occupation moves points only.
- **Ends** at the first resolved or quiet bell after which:
  - the occupier's faction does not hold the hex (event `LIBERATED`); this covers an occupier host that left or died, an owner's side that won the hex and a third faction that took it;
  - or `b ≥ start + occupation_tenure_bells` (event `OCCUPATION_EXPIRED`): 72 in Frontier-7, 288 in Frontier-28.
- **Respite (v1.1, K-09, R-05):** `immune_until_bell = end + respite_bells` (72 / 288) with `barred_faction := occupier`, granted **only** on `OCCUPATION_EXPIRED` or on a `LIBERATED` bell at which the **owner's faction holds the hex**. An occupier that walks away, or a third faction that takes the hex, gives no Respite (event detail bit `no_respite`). Respite never bars a faction other than the occupier's.
- **Why:** an owner's alt in another faction could otherwise occupy the home, walk away, and buy 72 / 288 bells of immunity against everyone, again and again. Now the alt's occupation shields nothing: any other faction can break it by force (taking the hex ends it with no Respite) and declare at once.
- **Nothing else moves in MC:** no tribute, no laurel share. An occupation ends when the season ends; the final map is taken at `end_bell − 1`.

### 3.6 Capture (holdings 2–3 and Free Cities)

- **At the completion bell b (in the resolve):**
  - the site mirror flips for bell b + 1: owner faction := attacker, `gen += 1`, garrison := 0, pending garrison slots cleared, walls halved (kept whole if the attacker's doctrine is F Iron), shield 0, `held_since_hour := ⌈(b + 1) / 6⌉`;
  - **capture credit (v1.1, K-26):** the capture is *credited* when `b + 1 − 6 × held_since_hour(before the flip) ≥ capture_credit_min_bells` (a genesis Free City is always credited). Only a credited capture does `captures_by[attacker] += 1` (Dominion's 36 control-bells and the standings' captures); the record keeps the credit bit;
  - the record becomes *capture due*, with the completion bell b.

  **The map's site layer changes at b + 1.**
- **SettleCapture (D, anyone)** applies the rest. **The outcome is fixed at the completion bell** (v1.1, K-25): the captor is the declarer, the slot is the one reserved at the horn, the rent is already escrowed for it, and the credit bit is set. The caller, the timing and the order of several settles change nothing.
  - **Capture of a holding:**
    - the Holding keeps its address; owner := declarer, `order := reserved slot`, `gen := mirror gen`;
    - buildings, queue and tier stay; trained reserve := 0; **stores stay if the capture is credited, else they are zeroed** (an uncredited recapture moves no goods);
    - `last_owner_action := now`; shield 0;
    - `prev_owner_tag`, `prev_gen`, `prev_home` (the victim's first holding) and `captured_bell` (= b) are recorded;
    - the victim's escrowed seal bonds are refunded to the victim's funder at once;
    - the Holding's rent moves from the captor's escrow (the reservation's rent) to the victim's rent payer (the rent swap);
    - both Citizens' slot lists and both JoinShards are updated; the reservation is consumed;
    - the stake returns to the source holding.
  - **Free City capture:** the Holding is created from the reservation's rent (Hamlet at the mirror tier, empty stores, `order := reserved slot`).
  - **No raze in MC.** A reservation guarantees the slot and the rent, so v1.0's raze branch is unreachable and is removed (CAPTURE_SETTLED outcomes 1 and 3 are reserved). A raze rule can return in M3 as a chosen action.
- **The victim's hosts:**
  - a host whose id names the captured Holding's previous generation keeps fighting for the victim's faction;
  - in transit, it settles normally and returns to `prev_home` (the victim's first holding), never into the captured Holding;
  - resident elsewhere, it stays where it is, defending as before, until **the victim retires it** with RetireHost (v1.1, K-27: the victim's wallet or session during the season; anyone only after `end_bell`). A host never retired simply keeps fighting until it dies or the season ends.

  The captor cannot command them, and no third party can choose when they leave a hex. One generation of history is kept; a second capture before the first victim's transits settle strands the oldest under M1's rule, and the verifier reports `DoubleCaptureStranded`.
- **Immunity after a capture:** the captured holding gets `immunity_bells` (36) **counted from the completion bell** (`immune_until_bell = b + 1 + immunity_bells`, against every faction), so a delayed settle never lengthens it.

### 3.7 Free Cities

| Rule | Value |
|---|---|
| Genesis Free Cities | one per province with `ring ≥ free_city_min_ring` (default **4**; 0 = none), placed by OpenProvince on `terrain::free_city_site(ring_seed, canonical P,Q)`, the same canonical site in every wedge |
| Garrison | `free_city_garrison` (default **300** troops), Hamlet, walls 0, no regrowth, faction NEUTRAL (hostile to all) |
| Released holdings | **v1.1 (R-07, R-14):** ReleaseDormant works as in M1: the site becomes released-free (state 3), never a Free City. A genesis Free City never expires (`neutral_until_bell` is gone), so no Free City changes state under a live siege |
| Vigil, shield, Frontier protection, heartland | none |
| Emission and control | no production; a neutral side in Dominion's denominator |
| Siege and capture | as §3.4 and §3.6; no defence immunity, no Respite; post-capture immunity as §3.6 |
| Capacity | one site in twelve from ring 4 is not ticketable; OpenRing's fill trigger counts occupied sites as in M1 |

### 3.8 Holdings 2–3: outposts

`FileOutpost` (P) files a site ticket that SettleTicket settles as order 2 or 3, through M1's cohort machinery: ≤ 3 preferred sites, the lottery by the bell's seed, displacement inside a cohort, finality by cohort. Checks:

| Rule | Value |
|---|---|
| Count | a free slot (no holding, no open ticket, **no capture reservation**; v1.1, K-25); the ticket takes the lowest free slot; one ticket set at a time (M1's Citizen ticket fields, with the slot in `slots`, §5.2.3) |
| Prerequisite | first holding final and at **Town** or better; a ticket for slot 3 needs slot 2's holding final (a slot 2 that is only reserved by a siege does not count) |
| Ring | **> `heartland_max_ring`** (outside every heartland, including the own) |
| Range | within **3 provinces** of one of the citizen's final holdings (named in the instruction, read-only) |
| Outpost rule | in a province where the citizen's faction holds **< 50%** of the province's strength weight at filing (an empty province qualifies) |
| Land gate | the Frontier's folded free sites ≥ 20% of open sites |
| Cost | settler cost `duplicate_cost(SETTLER_COST, n − 1)` from the named holding's stores, paid at filing, refunded if the ticket expires or is displaced |
| Close | refused at or after `end_bell − 24` (a cohort closes within 24 bells) |
| Shield | `outpost_shield_secs` (default **2 h**); a shielded outpost's hosts still cannot target other factions' holdings |

### 3.9 Protection and the governance defaults

| Protection | MC rule |
|---|---|
| Shield (first holdings) | `shield_secs` (Frontier-7 24 h, Frontier-28 48 h); `shield_late_secs` after `shield_late_after_secs` (24 h after day 2 / 72 h after day 7); a shielded holding can be besieged only when dormant |
| Frontier protection | holdings founded after `frontier_protect_after_secs` (12 h / 2 d) can be besieged for `frontier_protect_secs` after their shield (36 h / 7 d) only by a faction with a **first** holding within 2 provinces |
| Vigil | 8 h a day; snapshotted at the horn; changes as M1 (CL-09) |
| Heartlands | no holding sieges and no keep contests |
| Seats, Concord | never |
| First holdings | occupied, never taken; tenure, then Respite (against the occupier's faction). **The one way a home is lost is M1's dormancy release**, unchanged: in Frontier-28 after `release_after_secs` (10 days) without an owner action the site becomes a plain free site (not a Free City). In Frontier-7 no home is released (`release_after_secs` = the season length, v1.1 R-14); a home dormant for 3 days can be besieged while shielded, which means occupied, never taken |
| Immunity | 36 bells after a siege the defender broke (against the besieging faction only), and after a capture (against every faction, counted from the completion bell) |

| M3 lever | MC default (season parameters; the program refuses any other value) |
|---|---|
| Relations | **Rivalry between every pair**. Each faction is its own side for the hex fair share |
| War decree | none: heartland sieges and heartland keep contests are disabled |
| March truce / hostility | none (`false`) |
| Peace, NAP, Alliance | none |
| Mandates | Herald's Call as display data (§3.10) |
| Auto-reinforce | client, bots, opt-in relay as ordinary sealed marches; the kernel's `auto_reinforce` (same March, ≤ 25%, 4 largest) is the reference selector |
| Doctrine civic powers tied to War (A +12 bells; C war horn) | inert |

M3 plugs in by changing these inputs (`relation(f, g)`, March flags, the heartland path, a Mandate board), not the rules.

### 3.10 Dominion, standings and Herald's Call (game points)

- **Hourly snapshot.** At every bell `b = 6h`, inside that bell's resolve or skip, each Province writes `snap[h mod 6] = {h, weight[7]}`. `weight` is in centi-strength-weight units per side (factions 0–5, neutral 6):
  - each final holding: `floor(laurel::strength_weight(tier at b, committed garrison at bell_start(b), order − 1) / 10,000)`;
  - the side is the occupier's faction while occupied, the mirror faction otherwise (dormant included), neutral for a Free City.
  - Provisional, free and released sites add nothing.
- **FoldMarch(m, h)** combines the seven members' hour-h samples in strict hour order:
  - controller = the side with `2·w ≥ Σw` **and strictly more than every other side**; otherwise contested;
  - the controller faction gets `dominion_per_hour` (**6**) control-bells and one control-hour;
  - `captures[f]` = Σ members' `captures_by[f]` (credited captures only, §3.6);
  - a member resolved only up to `≤ 6h` refuses the fold (`FoldTooEarly`; lag only waits);
  - a member whose ring slot already moved past h makes the hour **lost** (no credit to anyone, `lost_hours += 1`).
- **Faction Dominion** = Σ over MarchStates of `dominion_bells[f] + dominion_per_capture (36) × captures[f]`. The herald and verifier sum it after each fold, divide by active members (citizens with a final holding) and show it as an **unofficial** index with the clamp and γ = 0.6 damping of DESIGN §5.6. Prosperity, Knowledge and Concord are not scored in MC.
- **Standings** (herald, per hour): provinces and Marches by keep control, keeps taken and lost, Dominion, sieges won and lost, occupations, liberations, credited captures. **Per player (v1.1, R-10, display only):** keeps taken, keep-bells held, sieges won, captures, liberations; Chronicle titles in `final.json`.
- **Naming (v1.1, R-15):** the map colour is *control* (keeps). The Dominion controller of a March is shown only as *Dominion lead* in standings, never as a map colour or fill; HANDOFF-UI says so in its first line.
- **Herald's Call** (display only, no reward): at each day boundary, `control::herald_call(control map, day seed) → one March per faction`:
  - the March adjacent to the faction's controlled provinces with the most enemy-held contestable keeps per distance;
  - for the faction with the fewest provinces, its nearest March lost within 2 days (Rally).

  Bots use it as a tie-breaker; the UI may show it. M3's Mandates replace it.

### 3.11 Season end

| Item | Rule |
|---|---|
| Marches | Depart refuses `arrive_bell ≥ end_bell` (M1 O-M1-25) |
| Sieges | DeclareSiege refuses one that cannot finish before `end_bell` (`TooLate`); one active at the end lapses, and SettleSiege (N) returns the stake |
| Keep contests | no resolve exists for bells ≥ `end_bell`, so none completes at or after the end; a running contest just stops |
| Occupations | end with the season (no state crosses it) |
| Outposts | FileOutpost refused at `≥ end_bell − 24` |
| Folds | the last foldable hour is the one whose bell `6h < end_bell` |
| The final map | `PSFCT1` of `end_bell − 1` and the standings at the last fold; `/h/season/final.json` publishes them, and the verifier recomputes them (the Chronicle record) |
| After the end | only settles and closes (SettleSiege, SettleCapture, RetireHost by anyone, CloseMarch after end + 72 h) |

### 3.12 Timer presets (SeasonParams v2, fixed at CreateSeason)

| Parameter | Frontier-7 (`MC_LOCAL_7D`: exit, nightlies, demo) | Frontier-28 (`MC_SEASON_28`) |
|---|---|---|
| `heartland_max_ring` | 3 | 3 |
| `keep_bells` / `keep_consolidate_bells` | 72 / 288 | 72 / 288 |
| `keep_home_guard` / `keep_garrison_bps` | 100 / 5,000 | 100 / 5,000 |
| `sieges_per_day` / `siege_stake_gold` / `immunity_bells` | 2 / 500 / 36 | 2 / 500 / 36 |
| `occupation_tenure_bells` / `respite_bells` | 72 / 72 | 288 / 288 |
| `free_city_min_ring` / `free_city_garrison` | 4 / 300 | 4 / 300 |
| `capture_credit_min_bells` (v1.1, K-26; replaces v1.0's `free_city_window_bells` at the same offset) | 144 | 288 |
| `shield_secs` / `shield_late_secs` / `shield_late_after_secs` | 86,400 / 86,400 / 172,800 | 172,800 / 259,200 / 604,800 |
| `outpost_shield_secs` | 7,200 | 7,200 |
| `frontier_protect_secs` / `frontier_protect_after_secs` | 129,600 / 43,200 | 604,800 / 172,800 |
| `dormant_after_secs` / `release_after_secs` | **259,200 / 604,800** (v1.1, R-14: 3 days / the whole season, so no home is released in a 7-day season; v1.0's 30 h / 60 h were harsher than M1's own 7-day preset, 5 / 10 days) | 432,000 / 864,000 (M1's values) |
| `dominion_per_hour` / `dominion_per_capture` | 6 / 36 | 6 / 36 |
| `outpost_range` / `outpost_tier_min` / `outpost_share_bps` / `outpost_close_bells` | 3 / Town / 5,000 / 24 | 3 / Town / 5,000 / 24 |
| `retire_hosts` | 1 | 1 |
| relations / war / truce / hostility | Rivalry / 0 / 0 / 0 | Rivalry / 0 / 0 / 0 |

**Test preset `MC_TEST` (v1.1, R-18; itests and smokes only, never the exit or the demo):** Frontier-7 except `heartland_max_ring` 2, `free_city_min_ring` 3, `shield_secs` = `shield_late_secs` 7,200, `outpost_tier_min` Hamlet, `keep_bells` 24, `keep_consolidate_bells` 48, `occupation_tenure_bells` = `respite_bells` 24, `immunity_bells` 12, `capture_credit_min_bells` 36, `frontier_protect_secs` 0. CreateSeason accepts it (every value is inside §5.2.5's ranges); `presets.json` carries it beside `MC_LOCAL_7D` and `MC_SEASON_28`.

**Unchanged from M1:**

- siege base 36 bells, 50 walls per bell, required ≤ 60 without a doctrine extra;
- vigil 8 h;
- 4 arrival slots per faction per province-bell; ≤ 8 residents per faction, ≤ 48 per province;
- the join window and the bell length.

### 3.13 RULES_VERSION bump policy and re-runs

- **MC bumps `RULES_VERSION_FRONTIER` 10 → 11** and these module versions:

  | Module | Version |
  |---|---|
  | `SIEGE_VERSION` | 2 → 3 |
  | `HOLDING_VERSION` | 2 → 3 |
  | `CLASH_VERSION` | 3 → 4 (13 garrisons) |
  | `CAMP_VERSION` | 1 → 2 (never on the keep tile) |
  | `TERRAIN_VERSION` | 1 → 2 (`keep_tile`, `free_city_site`) |
  | `GEOMETRY_VERSION` | 1 → 2 (heartland as a parameter) |
  | `KEEP_VERSION` | new, 1 |
  | `CONTROL_VERSION` | new, 1 |

  Each is added to `KERNEL_VERSIONS`. The keep constants and the conquest-parameter validation ranges join `ruleset_hash_input()`.
- **`RULESET_HASH` changes.** `frontier_abi::presets::RULESET_HASH` is regenerated and pinned by a test. The M1 value (`72c6b583…4bd9`) is kept in that test's doc comment and in DECISIONS. The v2 program refuses any Season with another hash (`RulesetMismatch`), so an M1 season can never meet a conquest program.
- **M1 outcomes stay bit-identical where conquest does not reach:** `m1_rules_keep_the_phase_b_digests`, `occupancy_empty_keeps_the_phase_b_digests` and the 4,320-input clash equivalence MUST stay green (no keep and ≤ 12 garrisons give the M1 clash).
- **Any later outcome-changing kernel change inside MC** bumps its module version again, is an amendment (§16), and re-runs:
  - the doctrine CI proxy gate;
  - the best-response criterion;
  - `mapmove-gate`;
  - overnight, the 1,500-season band.

  A change that keeps every constant but changes an algorithm MUST bump.
- **Thresholds freeze at Gate CQ1** (`frontier-sim/thresholds/mc-7d-1k.json`). Changing them later is an amendment with a reason, never a tuning step on the stack.

### 3.14 Pinned constants not in SeasonParams

| Constant | Value | Kernel |
|---|---|---|
| `MAX_GARRISONS` | **12, unchanged** (sites; in `ruleset_hash_input`, the camp rule and `SITES_N`) | clash |
| `MAX_GARRISONS_WITH_KEEP` | 13 (12 sites + keep), used by `validate` and the keep path only | clash |
| keep troops | ≤ `MAX_HOST_TROOPS` (30,000) in every reachable state; `keep_home_guard ≤ MAX_HOST_TROOPS` checked by CreateSeason | keep |
| keep garrison id | `u64::MAX − 0x1_0000 − gen` | keep |
| Control sides | 7 (factions 0–5, neutral 6) | control |
| Snapshot unit | centi-strength-weight (`/ 10,000`), u16 | control |
| Snapshot ring | 6 hours | control / ABI |
| Strict majority | `2·w ≥ Σ` and strictly ahead | control |
| Banner | `2 × held > open keeps` | control |
| Lasting change (criterion 10) | ≥ 6 bells | control (`lasting_changes`) |
| `SETTLER_COST`, `STARTER_KIT`, tier weights, `garrison_factor_bps` | unchanged | catalog / laurel |

### 3.15 Site-state invariants (v1.1, R-07)

Every one is a program check, a P12 property (§13.3) and a verifier rule (V22):

| # | Invariant | Enforced by |
|---|---|---|
| S1 | A site in state 0, 3 or 4 has an all-zero conquest record. | ReleaseDormant zeroes the record; SettleTicket and FileTicket/FileOutpost refuse a site whose record is not zero (`SiegeBusy`) |
| S2 | A record of kind 1, 2 or 3 sits only on a site in state 1 or 5. | DeclareSiege's check 3; nothing changes a site's state while such a record lives (S3, S4) |
| S3 | ReleaseDormant refuses while the site's record is kind 1, 2 or 3 or owes anything (`SiegeBusy` / `StakeUnsettled`); it runs once the record is kind 0 and settled. | ReleaseDormant |
| S4 | A genesis Free City never changes state except by capture. | no expiry exists in v1.1 |
| S5 | A stake owed to a site's Holding is paid only to that Holding at `owed_gen`; any other generation burns it. A stake owed to `src` names a generation in its host-id form. | SettleSiege |
| S6 | Every change of a site's owner (capture completion, SettleTicket, ReleaseDormant) leaves no kind-0 bits of the previous owner except the post-capture immunity the change itself writes. | the three writers |
| S7 | No raze state exists; CloseHolding is M1's (after a release only). | §3.6 |

---

## 4. Conventions every implementer follows

M1-CONTRACT §3 applies unchanged, except where this section says otherwise:

- toolchains and builds (§3.2), with `scripts/build-frontier.sh --twice`;
- code rules (§3.3): no `unwrap` in the program, checked arithmetic, fixed-offset accessors, never `CreateAccount`, recompute every keyed address, no hand-copied constants;
- the integrator's ownership of manifests and locks (§3.4, I-55);
- the tests every unit writes (§3.5).

### 4.1 Repository layout (new paths)

```
permutation-rules/src/frontier/{keep,control}.rs           new kernels (§7)
permutation-rules/tests/frontier_conquest.rs
permutation-rules/vectors/{keep-vectors-v1,control-vectors-v1}.json
frontier-abi/src/conquest_model.rs                         the shared conquest step (program, herald, verifier, itest, wasm)
frontier-abi/src/layout/{march.rs, conquest.rs}            MarchState, the Province conquest block
permutation-frontier/src/proc/conquest.rs                  0xA0–0xA2, 0xA5–0xA7
permutation-frontier/svm-tests/src/{ix,cover,world}/conquest.rs, tests/conquest.rs
frontier-sim/src/{conquest,campaign}.rs, frontier-sim/thresholds/mc-7d-1k.json, mc-28d-10k.json
frontier-node/crates/agents/src/campaign.rs
frontier-node/crates/herald/src/{cqfmt,control,conquest,standings,cqroutes}.rs
frontier-node/crates/verify/src/checks/{v14..v22}.rs
frontier-abi/src/v2/**, frontier-abi/vectors/v2/**             v1.1: the v2 names live beside v1 until each owner switches (§5.1)
scripts/cq-regen.sh                                         the integrator's generator run (§4.2)
frontier-node/crates/stack/src/mapmove.rs
frontier-node/configs/{cq-exit,cq-nightly,cq-rehearsal,cq-smoke}.toml
frontier-node/fixtures/cq/**                                recorded conquest mini-seasons
scripts/{cq-nightly.sh,cq-run-exit.sh,cq-ownership-check.sh}
permutation-server/web/frontier/{fconquest,fcontrol,fcqstate}.mjs   web data modules (ours; fcqstate holds MC's state slices, v1.1)
permutation-gateway/test/web-frontier-cq-*.test.mjs, test/fixtures/frontier/cq/**
docs/frontier/conquest/{CONQUEST-CONTRACT.md, HANDOFF-UI.md, CQ*-NOTES.md, runs/**, CQ-EXIT-NOTES.md}
```

### 4.2 Git, branches and approvals

- **Branches:**
  - Each unit works in its own worktree `.claude/worktrees/cq-<unit>` on branch **`frontier/cq-<unit>`** (for example `frontier/cq-1a-rules`).
  - Wave 1 branches are cut from `codex/frontier` at `CQ0`. **v1.1 (R-16, R-19):** a later wave's branches are cut from `frontier/cq-integ` at the previous wave's gate commit.
- **Integration branch: `frontier/cq-integ`** (worktree `.claude/worktrees/cq-integ`).
  - The integrator merges the wave's branches in the order of §11 and runs the wave gate (§12).
  - **`codex/frontier` is not fast-forwarded during MC** (v1.1). It stays the design chat's base with the M1 program and v1 web artifacts, so its preview on 41041 keeps working. The integrator fast-forwards it once, after the MC exit (Gate CQ5) and with the owner's consent.
  - At the start of every wave, and whenever `codex/frontier` moves (for example the design chat's `ui-shell` merged with the owner's consent), the integrator merges `codex/frontier` into `frontier/cq-integ` and re-runs the web and herald lines.
  - **Generated outputs** (`web/frontier/abi.mjs`, `web/sdk/frontier/**`, `permutation-gateway/client/src/frontier/abi-*.mjs`, `permutation-gateway/test/frontier-vectors.json`, the WASM build and its vectors, every `vectors/*.json`) are regenerated by the integrator (`scripts/cq-regen.sh`: the existing generators, then their `--check` lines) **in the same merge** that changes their source, in every wave.
- **Local commits only. No push, ever, without a new owner approval.**
  - Commit subject: `Frontier CQ <unit-id>: <what changed>`.
  - Body: the §-ids, K-ids and gate ids touched.
  - Trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **No devnet or mainnet transaction. No install or download.** The real-round archive at `data/drand-archive-quicknet-g0-1788998400` (approved and fetched for M1) is reused read-only. A new G0 would need a new download, which is not approved.
- **Notes:** each unit writes `docs/frontier/conquest/<unit-id>-NOTES.md` (what landed, measurements, deviations, dependency requests). If a hook refuses a file named `REPORT.md`, the `-NOTES.md` name is the rule anyway.
- **Integration window:** up to two working days after each merge. The integrator may change any file except the design chat's (§4.5) to make the gate green, one commit per gate item, subject `Frontier CQ integ-W<n>: <gate item>`.
- **Out-of-ownership files:** a unit MUST NOT modify a file outside its ownership list. If it needs one, it asks the integrator, who either amends this contract (§16) or assigns the change to the owning unit.

### 4.3 Ports

- **MC binds only 41300–41999:**

  | Use | Ports |
  |---|---|
  | exit and its re-runs | base **41300** (41300–41399) |
  | unit and itest dev stacks | 41400–41599 (base 41400 + 20 × unit index) |
  | wave smokes (`cq-smoke.toml`) | base **41600** |
  | the 7-day rehearsal | base **41700** |
  | nightlies | base **41800** |

  | the design chat's MC data source: `frontier-herald --fixture conquest` (from Gate CQ3), and later a recorded rehearsal slice, read-only | **41900** (v1.1, R-20; told to the design chat in the wave-3 note) |

- **Never bound:** 41000–41099 (the paused `m1-exit` stack the owner spectates), 41041 (the design chat), 41100–41299, and M1's reserved list (4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191).
- `frontier-stack` with `--conquest` refuses any port outside 41300–41999 (`check-ports` exit 1).
- In-process tests bind `127.0.0.1:0` only.
- The `m1-exit` stack's processes, files and chain are never touched. Reading its run directory is allowed only for the negative control of criterion 10 (§13.4), from a copy.

### 4.4 Tests every unit writes (prefixes, so gates filter)

| Tree | Prefix |
|---|---|
| svm-tests | `g01_cq_*` (budgets), `g02_cq_*` (pre-funding), `g03_cq_*` (forgery and re-creation), `g11_cq_*` (skip ≡ resolve), `g13_cq_*` (one per (instruction, error)), `p_cq_*` (properties P1–P13, §13.3) |
| kernels | `cq_*` in `frontier_conquest.rs` and unit tests |
| frontier-node | `cq_*`, plus `inproc_cq_day` and `g14_cq_*` (itest) |
| JS | `web-frontier-cq-*.test.mjs`, `frontier-cq-*.test.mjs` |

Every vector file has exactly one producer and one freshness test, as M1 §3.5.

### 4.5 The design chat's files (never touched by an MC unit)

On `codex/frontier` and the design chat's branch `frontier/ui-shell` (v1.1, R-19: the list now covers everything the eight `ui-shell` commits after `CQ0`, `d4a3448`…`246b1fd`, touch):

- `permutation-server/web/frontier/hud/**`, `art/**`, `people/**`, `screens/**`;
- `map/sprites.mjs`, `map/fmap.mjs`;
- `app.mjs`, `frontier.css`, `index.html`, `practice.html`, `spectate.html`, `fui.mjs`, `onboarding.mjs`, **`controller.mjs`, `fstate.mjs`** (MC no longer edits these two; §9.1 moves MC's state into new modules);
- `permutation-server/web/lang/en-frontier-play.mjs`;
- `frontier-node/crates/herald/src/roster.rs`, `frontier-node/crates/herald/tests/server.rs`;
- `permutation-gateway/screens/**`;
- `permutation-gateway/test/web-frontier-{hud,people,playtest,march}.test.mjs`;
- and `permutation-server/web/session.mjs` (M1 rule).

**Shared herald files, edited by MC only at named hooks:** `frontier-node/crates/herald/src/{lib,fold,server}.rs`. MC's herald code lives in new modules (`cqfmt`, `control`, `conquest`, `standings`, `cqroutes`); MC touches `lib.rs` only with `mod` lines, `server.rs` only with one `cqroutes::mount(…)` call, and `fold.rs` only with one `conquest::on_record(…)` dispatch and one completeness hook, each inside a `// MC hook` block. The integrator resolves conflicts there when `codex/frontier` is merged in.

`scripts/cq-ownership-check.sh <branch>…` fails when a commit **on the first-parent history of a `frontier/cq-*` branch since that branch's own fork point** (`git merge-base --fork-point`) touches one of the listed paths, or touches a shared herald file outside a `// MC hook` block. Commits that arrive through a merge of `codex/frontier` are never counted. It runs in every gate.

MC's only map-module change is the fill and sigil source in `map/layers.mjs` (§9.1), which no design-chat commit has touched since `CQ0`. If one does before CQ3, the change moves into the hand-off instead.

---

## 5. Program interface (normative)

### 5.1 Versioning against the frozen M1 ABI

- **Frozen:** `frontier-abi` v1 (M1 §5.3–§6): byte layouts, tags 0x01–0x70, errors 1–61, log kinds 1–70, entity kinds 1–7, seed tags, `ABI_VERSION = 1`.
- **ABI v2 is a strict superset:**
  - no v1 offset, tag, error or log kind changes meaning;
  - v2 chained headers write `layout_version = 2`. **v1.1 (R-22): readers dispatch on the version.** `fclient`, the herald and the verifier read v1 and v2: a Season's `program_version` and every account's `layout_version` pick the decoders and the checks (an M1 season: V1–V13 with v1 decoders; an MC season: V1–V22). The M1 fixtures in `frontier-node/fixtures/verify/` (15 MB: `land-program`, `march-program*`, `w6s7-*`) stay as **v1 regression inputs** and are never re-recorded; T1–T24 keep running on them. The v2 program itself refuses v1 accounts (`RulesetMismatch`), as before;
  - new numbers: tags 0xA0–0xA7 (0xA8–0xAF reserved), errors 62–78, log kinds 80–89, entity kind 8 (MarchState), seed tag `mc`.
- **Staged (v1.1, R-16).** Wave 1 is additive: `frontier_abi::v2::*` (layouts, tags, errors, logs, `Ix::ALL_V2`, `AccountKind` v2 additions, `RULESET_HASH_V2`, `ruleset_hash_input_v2()`), kernel additions under new names (`geometry::is_heartland_in(p, f, max_ring)`, `clash::MAX_GARRISONS_WITH_KEEP`, `siege` v3 functions beside v2), and every v1 name keeps its value. So M1's `RULESET_HASH`, `Ix::ALL`, the fclient twin tests, the JS vectors and the WASM hash are unchanged at Gate CQ1. Wave 2 switches the program, `fclient` (its twin tests compare v2 too), keeper, herald and bots; Wave 3 switches the web, SDK, WASM and verifier. v1 names are never removed (the M1 record).
- **Per season, never in place.**
  - The program crate on `codex/frontier` becomes the v2 program after Wave 2. It embeds the new `RULESET_HASH` and refuses any other Season.
  - v1 accounts are never migrated.
  - The M1 release `.so` (`d85e1bd7…2281`) stays the build of record for M1 seasons, and the paused `m1-exit` stack keeps it.
  - The MC release `.so` gets its own build record (`scripts/build-frontier.sh --twice` → `docs/frontier/conquest/CQ4-A-NOTES.md`), pinned by `scripts/cq-run-exit.sh --expect-so-sha256`.
- **Regenerated by the integrator, never hand-entered:**
  - `budgets.rs` (CU from G1; `L(kind)` from the release `.so` and its `--max-len`);
  - `presets.json` (`MC_LOCAL_7D`, `MC_SEASON_28`; the M1 presets stay for the record but CreateSeason v2 refuses `program_version = 1`);
  - every vector file;
  - the web SDK.

### 5.2 Accounts and layouts

#### 5.2.1 Province v2 (`pv‖P,Q`, 4,736 B; created by OpenProvince from its wedge's ProvinceFund)

- **Bytes 0..4,096:** exactly M1, including the 232-B reserve at 3,864 (kept for M2/M3).
- **Site mirror (64 B per site), v1 reserved bytes given a meaning:**

| Offset | v1 | v2 |
|---|---|---|
| 0 `state` | 0 free, 1 holding, 3 released-free, 4 reserved | + **5 Free City** |
| 5 | rsv | `tier_next u8` (a tier-up Build's new tier) |
| 6 | rsv u16 (with 7) | **v1.1:** `held_since_hour u16`: the hour from which the current owner holds the site (set by SettleTicket, by capture completion and at genesis for a Free City; the capture-credit rule, §3.6) |
| 28 | rsv u32 | holding: `tier_next_bell u32` (the bell from which `tier_next` counts); Free City: rsv (v1.1: no expiry) |

- **The conquest block at 4,096 (640 B):**

| Offset | Size | Field |
|---|---|---|
| 4,096 | 384 | `conquest[12] × 32 B`: one record per site (below) |
| 4,480 | 32 | `keep` (below) |
| 4,512 | 120 | `snap[6] × 20 B {hour u32, weight[7] u16, rsv u16}`, ring slot `hour mod 6` (§3.10) |
| 4,632 | 12 | `captures_by[6] u16`: completed captures in this province by captor faction (cumulative) |
| 4,644 | 12 | `keeps_taken_by[6] u16` (cumulative, display) |
| 4,656 | 80 | rsv |

- **Conquest record (32 B), a tagged union on `kind`:**

| Offset | Field | kind 0 (none) | 1 siege | 2 occupation | 3 capture due |
|---|---|---|---|---|---|
| 0 | `kind u8` | | | | |
| 1 | `faction u8` | **`barred_faction`** (v1.1): the faction the immunity bars (0–5), `0xFE` every faction (post-capture), `0xFF` none | attacker | occupier | captor |
| 2 | `flags u8` | bit 1 stake owed to the site's holding at `owed_gen`; bit 2 stake owed to `src`; **bit 5 reserved slot owed back to `actor`** | bit 0 held; bit 3 neutral target (no vigil) | — | **bit 4 credited** |
| 3 | `progress u8` | **`owed_gen`** (v1.1) | progress | — | — |
| 4 | `required u8` | **`slot`** to release (bit 5) | ≤ 60 | — | — |
| 5 | `target u8` | — | low nibble 1 first, 2 other, 3 Free City; **high nibble the reserved slot (2 or 3; 0 for a first holding)** | 1 | low nibble 2 or 3; high nibble the slot |
| 6 | 6 B | — | vigil snapshot `start_min u16, next_min u16, from_day u16` | — | — |
| 12 | `bell u32` | `immune_until_bell` (immunity and Respite, for `barred_faction`) | declared bell | start bell | completion bell |
| 16 | `actor u64` | the citizen whose slot is owed back (bit 5) | declarer `citizen_tag` | declarer tag | declarer tag |
| 24 | `src u64` | stake return Holding key (bit 2) | the source Holding key (host-id form `index<<44 \| site<<40 \| gen<<32`) | source key | source key |

**v1.1:** every site change rewrites the record per §3.15 (S1–S6).

- **Keep (32 B):**

| Offset | Field |
|---|---|
| 0 | `tile u8` (0xFF: no keep, rings 0–1) |
| 1 | `holder u8` (0–5) |
| 2 | `contender u8` (0xFF none) |
| 3 | `progress u8` |
| 4 | `required u8` (= `keep_bells`, ≤ 255) |
| 5 | `flags u8` (bit 0 heartland_safe; bit 1 reserved for M3: paused by a multi-faction hold, never set in MC) |
| 6 | `changes u16` |
| 8 | `troops u32` (whole troops) |
| 12 | `since_bell u32` |
| 16 | `consolidated_until_bell u32` |
| 20 | `contest_from_bell u32` |
| 24 | `gen u32` |
| 28 | `last_taken_from u8` (0xFF) |
| 29 | rsv 3 |

- **Rent:**
  - (128 + 4,736) × 5,080 = **24,709,120 lamports**, which is +3,251,200 (**+0.00325 SOL**, refundable) per province against M1;
  - ≈ 170 provinces in the exit season: +0.55 SOL of local test SOL;
  - at `R_MAX` 64 full (12,474 provinces): +40.6 SOL of refundable ProvinceFund float (+15% on DESIGN §3.4's 268 SOL).
  - OpenRing's fund check becomes `d × rent(4,736)`.
  - 4,736 B is under the 10,240-B CPI allocation limit. `init_funded` is unchanged in kind.

#### 5.2.2 Holding (1,280 B, unchanged size): v1 reserve 1,248..1,280

| Offset | Field |
|---|---|
| 1,248 | `prev_owner_tag u64` (0 = never captured) |
| 1,256 | `prev_gen u8` |
| 1,257 | `capture_flags u8` (1 captured; v1.1: no raze value) |
| 1,258 | rsv u16 |
| 1,260 | `captured_bell u32` |
| 1,264 | `prev_home u64` (the victim's first-holding key) |
| 1,272 | rsv 8 |

The field `order` (113) takes 2 and 3. `shield_until`, `founded_ts` and `founded_day` are used for outposts and captures.

#### 5.2.3 Citizen (384 B, unchanged size)

- 145 `sieges_today u8`;
- 150 `siege_day u16`;
- 187 **`slots u8`** (v1.1, K-25): bits 0–1 the open ticket's slot (0 none, 1 first holding, 2 or 3 outpost); bit 2 slot 2 reserved by a capture siege; bit 3 slot 3 reserved;
- **`holding[3]` is indexed by slot** (entry i is slot i + 1; an empty entry has `gen = 0xFF`), and `holdings_n` counts the non-empty entries. A slot is free when its entry is empty and neither the ticket nor a reservation names it.
- `ticket_escrow` and `ticket_funder` are reused as the holding escrow for outposts and captures (M1 pattern; returned by CloseCitizen). It holds `rent(1,280)` per open outpost ticket and per reservation.
- The M2 reserve at 312..384 is untouched.

#### 5.2.4 JoinShard (256 B)

- 108 `extra_holdings u32`;
- 112 `captured_in u32`;
- 116 `captured_out u32`;
- 120 `razed u32` (v1.1: reserved, always 0);
- 124 `outposts u32`.

FoldOccupancy adds `extra_holdings` to occupied sites, so Join's reserve and the 20% gate see every holding.

#### 5.2.5 Season (2,048 B): `SeasonParams` v2

- The 128-B conquest block goes at **896..1,024**, in this order (offsets from 896):

| Offset | Field |
|---|---|
| 0 | `conquest_version u8` = 1 |
| 1 | `heartland_max_ring u8` |
| 2 | `sieges_per_day u8` |
| 3 | `free_city_min_ring u8` |
| 4 | `keep_bells u16` |
| 6 | `keep_consolidate_bells u16` |
| 8 | `keep_home_guard u32` |
| 12 | `keep_garrison_bps u16` |
| 14 | `immunity_bells u16` |
| 16 | `occupation_tenure_bells u16` |
| 18 | `respite_bells u16` |
| 20 | `siege_stake_gold u32` |
| 24 | `free_city_garrison u32` |
| 28 | `capture_credit_min_bells u32` (v1.1; was `free_city_window_bells`) |
| 32 | `outpost_shield_secs u32` |
| 36 | `shield_secs u32` |
| 40 | `shield_late_secs u32` |
| 44 | `shield_late_after_secs u32` |
| 48 | `frontier_protect_secs u32` |
| 52 | `frontier_protect_after_secs u32` |
| 56 | `dormant_after_secs u32` |
| 60 | `release_after_secs u32` |
| 64 | `dominion_per_hour u16` |
| 66 | `dominion_per_capture u16` |
| 68 | `outpost_range u8` |
| 69 | `outpost_tier_min u8` |
| 70 | `outpost_share_bps u16` |
| 72 | `outpost_close_bells u16` |
| 74 | `retire_hosts u8` |
| 75 | `relations u8` (0 = Rivalry everywhere) |
| 76 | `flags u8` (bit 0 war, bit 1 truce, bit 2 hostility) |
| 77 | rsv 51 |

- **CreateSeason validation:**
  - `relations == 0` and `flags == 0` (M3 values refused, `BadParams`);
  - `2 ≤ heartland_max_ring ≤ 6`, `free_city_min_ring ∈ {0} ∪ [heartland_max_ring + 1, 64]`;
  - `1 ≤ keep_bells ≤ 255`, `keep_consolidate_bells ≤ 4,320`;
  - `keep_garrison_bps ≤ 10,000`, `1 ≤ sieges_per_day ≤ 8`, tenure and Respite ≤ 4,320;
  - `keep_home_guard ≤ 30,000` and `free_city_garrison ≤ 30,000` (`MAX_HOST_TROOPS`; v1.1, R-01);
  - `capture_credit_min_bells ≤ 4,320`;
  - every timer ≤ the season length.
- The 516..896 reserve stays for M2/M3.

#### 5.2.6 MarchState (new; `mc‖m i32, n i32`; 256 B; chained header, entity kind 8, magic `PSF1MRCH`)

| Offset | Field |
|---|---|
| 0 | H (64): magic, season, `layout_version` 2, seq, head |
| 64 | `m i32` |
| 68 | `n i32` |
| 72 | `next_hour u32` |
| 76 | `controller u8` (0–5; 6 neutral; 0xFF contested or open) |
| 77 | `contested u8` |
| 78 | rsv u16 |
| 80 | `last_flip_hour u32` |
| 84 | `lost_hours u32` |
| 88 | `weight[7] u32` (the last folded hour, centi units) |
| 116 | `dominion_bells[6] u32` |
| 140 | `control_hours[6] u32` |
| 164 | `captures[6] u16` |
| 176 | `rent_to [32]` |
| 208 | rsv 48 (M3: Warden roll counts) |

- **Rent:** 1,950,720 lamports.
- **Created by** the first FoldMarch of its March (`init_with_seed`, the fee payer pays, pre-funding-safe). `next_hour = ceil(min opened_bell over present members / 6)`.
- **Closed by** CloseMarch after end + 72 h, to `rent_to`.
- **Count:** ≈ provinces / 7 plus the rim (≈ 30 in the exit season).

#### 5.2.7 No other new accounts

- No per-siege accounts.
- No keep accounts: the keep lives in its Province.
- No banner account.
- No ScoreBoard (M2).
- The Frontier is still written only by OpenRing and FoldOccupancy.

### 5.3 Error codes (new, stable forever)

| Code | Name | Code | Name |
|---|---|---|---|
| 62 | CapturePending | 70 | NotBesiegeable |
| 63 | Immune (immunity or Respite) | 71 | Friendly |
| 64 | SiegeBusy (any conquest record on the site) | 72 | FrontierProtected |
| 65 | StakeUnsettled | 73 | Heartland |
| 66 | SiegeCap | 74 | NotDue |
| 67 | NotOnHex | 75 | OutpostRule (ring, range, tier, share or close) |
| 68 | TooLate (cannot finish before `end_bell`) | 76 | FoldOutOfOrder |
| 69 | HoldingsFull (no free slot) | 77 | FoldTooEarly |
| | | **78** | **NotLead** (v1.1: the declaring host is not the hex's lead host; also RetireHost by a non-victim during the season) |

M1 codes reused: `Shielded`, `ReservedSite`, `Insufficient`, `BadAccount`, `BadAddress`, `AlreadyDone`, `RulesetMismatch`, `BadParams`, `WrongStatus`, `NotResident`, `NotFinal`, `TransitState`, `Overflow`.

### 5.4 Tag map, classes and budgets (v2 additions and changes)

Budget means the CU ceiling the G1 gate asserts at the named worst fill (§13.1) on the release `.so`. The client and keeper limits come from `budgets.rs` (measured max + 5%, regenerated at Gate CQ4).

| Tag | Instruction | Class | Writes | CU budget | Tx B max |
|---|---|---|---|---|---|
| **0xA0** | DeclareSiege | P (relay) | Province, Citizen, source Holding (+ Citizen escrow lamports) | **30,000** (est. ≈ 25k: checks 6,664 [measured lab, horizon and vigil not recorded] + prologue and Holding settle ≈ 16.4k [M1 measured] + escrow ≈ 1.5k + lead-host ranking over ≤ 48 entries ≈ 1k). v1.1 (R-08): the TooLate kernel is bounded (§3.4 step 9: O(1) when ≥ 288 bells remain, else ≤ 287 `covers()` calls) and the ETA is off chain; G1's fill is pinned (§13.1). If G1 still exceeds 30,000, CQ1-A replaces the scan by closed-form interval arithmetic (identical results, vector-tested) before Gate CQ2 | 640 |
| **0xA1** | SettleSiege | N | Province, recipient Holding, **the reservation's Citizen when a slot is owed back** | 25,000 | 480 |
| **0xA2** | SettleCapture | D | Holding (or init), Province, 2 Citizens, 2 JoinShards, victim rent payer, stake Holding | **30,000** (est. 16k capture / 21k Free City init; bookkeeping 8,084 [measured lab]) | 720 |
| **0xA3** | FileOutpost(≤ 3) | P | Citizen (+ escrow), ≤ 3 Provinces (cohorts), named Holding (stores) | 24,000 | 640 |
| 0xA4 | *(reserved: CollectTribute, M3)* | | | | |
| **0xA5** | FoldMarch(m, n, hour, count ≤ 6) | D | MarchState (init on first fold) | **20,000** (8,335 one hour [measured lab]; est. ≈ 14k with prologue and init) | 720 |
| **0xA6** | RetireHost(entry) | **P** during the season (the victim's wallet or session, player prologue on the victim's Citizen); N after `end_bell` (v1.1, K-27) | Province | 17,000 | 480 |
| **0xA7** | CloseMarch | N | closes MarchState | 8,000 | 300 |
| 0xA8–0xAF | *(reserved: Settler march, SettleFounding, Raid, AutoReinforce)* | | | | |
| 0x01 | CreateSeason | O | as M1 + params v2 | 70k | 1,100 |
| 0x20 / 0x22 | OpenRing / OpenProvince | D | as M1 (fund check `rent(4,736)`; keep; genesis Free City) | 30k / **220k** (148.5k + est. 7k) | 600 / 400 |
| 0x23 | FoldOccupancy | D | Frontier | 30k per part | 1,232 |
| 0x34 | SettleTicket | D | as M1 (order 2–3 path) | 40k | 900 |
| 0x35 | ReleaseDormant | N | as M1 (the site becomes released-free); v1.1: refuses while the site's record is live or owes anything (S3), zeroes the record | 25k | 480 |
| 0x36 | CloseHolding | N | as M1 (v1.1: no razed path) | 15k | 330 |
| 0x40 / 0x41 / 0x42 | Harvest / Build / Train | P | as M1 + `[province r]` (capture lock); Build tier-up writes the Province (`tier_next`) | **19,000 / 23,500 / 19,000** (M1 16,409 / 19.1k / 16,641 + ≈ 1.2k) | 360 / 400 / 360 |
| 0x43–0x46, 0x50 | Muster, Dissolve, Garrison, Explore, Depart | P | as M1 (capture lock on the Province they already name) | as M1 + ≤ 0.5k within the M1 budgets | as M1 |
| 0x51 | Reveal | W | as M1; a Free City site is an allowed target while shielded | 26k | 1,100 |
| 0x52 / 0x54 | SettleDeparture / SettleTransit | D | as M1; `prev_gen` accepted; return to `prev_home` (mandatory extra account when the generations differ) | 48k / 85k | 400 / **1,022** |
| 0x60 | GatherClash | D | ClashInputs; a `prev_gen` host on a captured Holding gathers as present | 40k | 1,232 |
| 0x61 | ResolveFromInputs | D | Province, ClashInputs | **290,000** (271,673 [M1 measured] + conquest step 6.6k [measured lab] + keep garrison est. ≤ 6k ≈ 284k). If G1 exceeds it: **300,000** by amendment, and the worst clash budget restated 410k → 420k | 460 |
| 0x63 | SkipQuiet(b0, n ≤ 24) | D | Province | 60k + 30k per recomputed bell **+ 3.5k per bell with an active record or keep contest**; prefix commit once `bells × active records > 288` (I-50 rule) | 1,232 |
| 0x47 | SettleExplore | N | as M1; a record whose host generation differs from the Holding's credits nothing | 15k | 400 |

**Heap and size:**

- The conquest step uses fixed arrays (12 records, 13 garrisons, a 61-tile faction mask). The heap stays ≤ 28 KiB (M1 peak 15,320 B).
- The release `.so` is est. 0.98–1.02 MB (+100–140 KB over 875,824 B) [estimate]. `--max-len` = round_up(1.25 × size, 4 KiB). Every `L(kind)` is regenerated.
- Every transaction stays ≤ 1,232 B and ≤ 64 locks (DeclareSiege ≈ 560 B and 12 keys; SettleCapture ≈ 560 B; FoldMarch ≈ 600 B).

### 5.5 New instructions

Common rules (M1 §5.6):

- player prologue P or keeper prologue K;
- every keyed account is checked at its canonical with-seed address;
- pre-funding-safe init;
- `pay_or_divert` with sink `Holding.pool_owed`;
- no D or N instruction writes the DefencePool except CloseHolding (N, as M1).

#### 0xA0 DeclareSiege — P, not top-level-only

- **Accounts:**
  - `[0 actor s] [1 payer s,w] [2 season r] [3 citizen w]`
  - `[4 src_holding w]`: the declarer's Holding that owns the host on the hex
  - `[5 province w]`: the target's
  - `[6 target_holding r]`: canonical; absent at its canonical address for a Free City
  - `[7 owner_citizen r]`: canonical; absent for a Free City
  - `[8 nearby_province r]`: canonical; any absent canonical address when not needed
  - `[9 system]`
- **Data:** `site u8, entry u8, nearby_site u8`.
- **Checks:** §3.4 steps 1–10, in that order.
  - "Resident" means the target Province is resolved through `now_bell − 2`, else `NotResident`.
  - The host on the hex is entry `entry`: its host id names `src_holding` at its current generation, its faction is the citizen's, its tile is the site's tile, it is not a Scout or other civilian, state 1, `from_bell ≤ now_bell`, no pending Spend, Leave or Forfeit.
  - **v1.1:** entry `entry` is the tile's lead host: no other qualifying entry on the tile has more troops, or as many troops and a lower host id (`NotLead`).
  - The vigil snapshot comes from `owner_citizen` after M1's lazy roll-forward, including a change already requested.
- **Effects:**
  - write the record (with the reserved slot for a capture target);
  - `sieges_today += 1` (reset when `siege_day ≠ day(now)`);
  - stake paid by `Holding::pay`;
  - for a capture target: Citizen `slots` bit for the reserved slot; escrow top-up to `rent(1,280)` × (open ticket + reservations);
  - log `SIEGE_DECLARED`; chains Province, Citizen and source Holding.

#### 0xA1 SettleSiege — N, anyone

- **Accounts:** `[0 payer s] [1 season r] [2 province w] [3 recipient_holding w (canonical; may be absent)] [4 slot_citizen w (canonical for the record's actor; absent when no slot is owed)]`.
- **Data:** `site u8`.
- **Applies when** a stake is owed (flags bit 1 → the site's Holding at `owed_gen`; bit 2 → the `src` key), a reserved slot is owed back (bit 5), or the record is a siege and the season has ended (no winner; recipient = `src`; the slot is released).
- **Effects:**
  - recipient `Holding::settle(now)`, then Gold += stake up to the store cap (excess burned);
  - **slot release (v1.1):** clear the Citizen's reservation bit for `slot` and refund one `rent(1,280)` from `ticket_escrow` to `ticket_funder`;
  - flags cleared (season end: kind 0);
  - an absent recipient, or one whose generation is not `owed_gen` (bit 1) or the `src` key's (bit 2), burns the stake;
  - log `SIEGE_SETTLED`.
- **Refusals:** nothing owed → `AlreadyDone`.

#### 0xA2 SettleCapture — D, anyone (the captor's client usually first, the keeper as backstop)

- **Accounts:**
  - `[0 fee_payer s,w] [1 season r] [2 holding w]` (canonical `ho‖P,Q,site`; absent for a Free City)
  - `[3 province w] [4 captor_citizen w] [5 captor_joinshard w]`
  - `[6 victim_citizen w] [7 victim_joinshard w] [8 victim_rent_payer w]` (canonical placeholders for a Free City)
  - `[9 stake_holding w]` (the record's `src`; may be absent)
  - `[10 system]`
- **Data:** `site u8, beneficiary [32]`.
- **Checks:**
  1. The record is kind 3: kind 0 → `AlreadyDone`; another kind → `NotDue`.
  2. `captor_citizen` is canonical with `citizen_tag == actor`.
  3. Holding site: `holding.gen + 1 == mirror.gen`; `victim_citizen == holding.owner_citizen`; `victim_rent_payer == holding.rent_payer`; both JoinShards are canonical for their (faction, shard).
  4. (v1.1) The captor Citizen's `slots` reserves the record's slot (it always does; a mismatch is `CaptureRule`, a program bug the verifier also checks).
- **Effects:** §3.6 (capture or Free City capture, into the reserved slot; stores kept only if credited); record → kind 0 with `barred_faction = 0xFE` and `immune_until_bell = completion bell + 1 + immunity_bells` (v1.1: from the completion bell, not from the settle).
  - Log `CAPTURE_SETTLED` with outcome 0 capture, 2 Free City capture (1 and 3 reserved: no raze in MC), and the credit bit.
  - Chains Holding, Province, both Citizens and both JoinShards.

#### 0xA3 FileOutpost — P

- **Accounts:** M1 FileTicket's list `+ [anchor_holding w]` (a final holding of the citizen; its stores pay the settler cost).
- **Data:** `n u8, sites [≤ 3] × {P i16, Q i16, site u8}, anchor_site_key u64`.
- **Checks:**
  - §3.8 (`OutpostRule` for ring, range, tier, share or close; `HoldingsFull`);
  - M1 FileTicket's checks (cohort room, free sites, land gate, escrow top-up).
- **Effects:** as FileTicket, with the ticket's slot = the lowest free slot (v1.1: a slot reserved by a capture siege is not free) and the settler cost escrowed in the anchor Holding's `pending` stores (refunded on expiry or displacement).
- SettleTicket founds the Holding with `order` = that slot, the outpost shield and no starter kit.

#### 0xA5 FoldMarch — D, anyone

- **Accounts:** `[0 fee_payer s,w] [1 season r] [2 march w] [3..9 province × 7 r]` (canonical, in `geometry::march_members(m, n)` order; absent allowed) `[10 system]`.
- **Data:** `m i32, n i32, hour u32, count u8 (1–6), beneficiary [32]`.
- **Checks:**
  - the season is Running, or Ended with `6 × (hour + count − 1) < end_bell`;
  - March canonical (init if absent);
  - `hour == next_hour`, else `FoldOutOfOrder`.
- **Per member and hour h:**

  | Member state | Treatment |
  |---|---|
  | absent, or `opened_bell > 6h` | weight 0 |
  | `resolved_next ≤ 6h` | `FoldTooEarly` |
  | ring slot holds h | add its weights |
  | slot moved past h | the hour is lost |

- **Effects:** §3.10 per hour; log `MARCH_FOLD` per hour.

#### 0xA6 RetireHost — P (the victim) during the season; N after `end_bell` (v1.1, K-27)

- **Accounts:** `[0 actor s] [1 payer s,w] [2 season r] [3 victim_citizen r] [4 province w] [5 captured_holding r] [6 home_holding r]`.
- **Data:** `entry u8`.
- **Checks:**
  - `retire_hosts == 1`;
  - **while `now_bell < end_bell`:** player prologue on `victim_citizen` (its wallet or a live session key signs), and `victim_citizen.citizen_tag == captured_holding.prev_owner_tag`, else `NotLead` (78). After `end_bell` the actor may be anyone;
  - the entry's host id names `captured_holding` with `id.gen == prev_gen ≠ gen`;
  - `home_holding` is `prev_home`, present at its generation;
  - entry state 1, no pending op.
- **Effects:**
  - pending op Leave at `bell(now)` with `op_a = 1` (retire), effective after that bell's clash (roster freeze);
  - the extended return settle (0x52) credits the troops to `prev_home`'s reserve;
  - log `RETIRE`.

#### 0xA7 CloseMarch — N

- **Accounts:** `[0 any s] [1 season r] [2 march w] [3 rent_to w]`.
- **Checks:** season Ended and `now ≥ end + 72 h`.
- **Effects:** close to `rent_to`; log `CLOSE`.

### 5.6 Changed M1 instructions (v2 shapes)

| Tag | Instruction | Change |
|---|---|---|
| 0x01 | CreateSeason | `program_version = 2`; SeasonParams v2 validated (§5.2.5) |
| 0x20 | OpenRing | fund check `d × rent(4,736)` |
| 0x22 | OpenProvince | writes the keep (`keep::open(P, Q, wedge, ring, terrain, sites, params)`); for `ring ≥ free_city_min_ring`, places the genesis Free City (state 5, garrison, Hamlet, `held_since_hour = 0`, permanent); the camp never on the keep tile; logs `KEEP` (placed) and `NEUTRAL` |
| 0x23 | FoldOccupancy | adds `extra_holdings` |
| 0x33 / 0x34 | FileTicket / SettleTicket | SettleTicket reads the ticket's slot from `slots`: slot 2–3 founds an outpost (Citizen `holding[slot − 1]`, JoinShard `extra_holdings`, `outposts`), sets the mirror's `held_since_hour`, asserts a zero record (S1); log `OUTPOST_SETTLED` beside `SETTLE`; FileTicket refuses while an outpost ticket is open (`TransitState`); FileTicket and FileOutpost refuse a site whose record is not zero (`SiegeBusy`) |
| 0x35 | ReleaseDormant | as M1 (site → released-free, state 3); uses `dormant_after_secs` / `release_after_secs`; **v1.1:** refuses `SiegeBusy` / `StakeUnsettled` while the record is kind 1–3 or owes anything (S3), and zeroes the record |
| 0x36 | CloseHolding | as M1 |
| 0x40–0x46, 0x50 | Harvest, Build, Train, Muster, Dissolve, Garrison, Explore, Depart | **capture lock:** refuse `CapturePending` when `mirror.gen ≠ holding.gen` and `capture_flags == 0`; Harvest, Build and Train gain `[province r]` (Build tier-up: `w`, writes `tier_next`, `tier_next_bell = bell_at(done_at) + 1`) |
| 0x51 | Reveal | step 6: a site in state 5 is an allowed destination from a shielded holding (keeps already are: non-site tiles) |
| 0x52 | SettleDeparture (+ return settle) | accepts `id.gen == prev_gen` on a captured Holding; a retire Leave credits `prev_home` |
| 0x54 | SettleTransit | as 0x52; **mandatory `[prev_home_holding w]` at a fixed position when `holding.gen ≠ id.gen`**; bonds already refunded at capture are not paid twice (flag) |
| 0x60 | GatherClash | a slot whose host id has `gen == prev_gen` on a captured Holding gathers as present |
| 0x61 | ResolveFromInputs | **the conquest step** (§5.7); Free City and keep garrisons in the clash input; CLASH `input_digest` domain `PSF-CLASH-INPUT-v2` over `province[SITE_MIRROR..TICKET_COHORTS] ‖ province[4096..4736]`; log `CONQUEST` |
| 0x63 | SkipQuiet | the conquest step per skipped bell with the quiet model; prefix commit at 288 record-bells; SKIP `quiet_digest` v2 adds the conquest block |
| 0x47 | SettleExplore | a generation mismatch credits nothing |

**Not changed:** Join, beacons, ArchiveAnchors, ClaimDefence, the closes, Dissolve's semantics, DisbandStranded (still the fallback when `retire_hosts = 0`).

### 5.7 The conquest step (inside ResolveFromInputs and SkipQuiet)

A pure function, **`frontier_abi::conquest_model::step(province_bytes, bell, report, params) -> StepOut`**. The program, herald, verifier, itest and WASM all run the same code, as with `clash_model` (M1 W4-A D8). ResolveFromInputs calls it after the clash write-back and `settle_bell`; SkipQuiet calls it for every skipped bell.

**The report:**

- **Resolve:** from the kernel's `out.garrisons`, found by garrison id: sites `host_id(P, Q, site, gen, 0)`, the keep `u64::MAX − 0x1_0000 − gen`.
- **Skip:** the quiet model. One pass over the entries builds a per-tile faction mask of non-civilian residents (cached per transaction by `roster_epoch`). Then `holders = mask(tile) & !side_bit(owner)` and `defender_present = mask(tile) & side_bit(owner) ≠ 0`.

  In a quiet bell every resident stays on its tile, so this equals the kernel's report. **G11 is extended:** resolve and skip of the same quiet bell are byte-identical across the whole 4,736-B Province.

**Order within bell b** (each step reads the state the previous one left):

1. Records with `kind ≠ 0` and `b > record.bell`, by site index:
   - siege → `Siege::advance` (failure, completion; capture credit from `held_since_hour`);
   - occupation → liberation or expiry (Respite per §3.5);
   - capture due → unchanged (waits for SettleCapture).
2. *(v1.1: removed; no Free City expires. Code 6 `FREE_CITY_EXPIRED` is reserved.)*
3. Keep → `keep::advance` (§3.2), including the donor handoff (the donor's entry gets a retire-style Leave, or is removed when its rest is below `MIN_HOST_TROOPS`).
4. If `b mod 6 == 0`: the hour snapshot (§3.10).
5. One `CONQUEST` log record when anything in 1–4 happened, any record or keep contest is active, or a snapshot was written.

**Work bound:** ≤ 12 records + 1 keep per province. ≈ 263 CU per record-bell in a skip and ≈ 365 CU per active record per resolve [measured lab].

### 5.8 Capture lock and the victim's hosts

- From the completing resolve on, `mirror.gen = holding.gen + 1`. Every instruction that writes that Holding refuses `CapturePending` until SettleCapture: Harvest, Build, Train, Muster, Dissolve, Garrison, Explore and Depart (each names the Province), and every settle except SettleCapture.
  - The victim cannot drain stores or train after completion; the captor cannot act before the settle.
  - Draining before completion, while progress is public, is ordinary play.
- **The M1 generation trap is fixed** (`program.md` §6.2). A capture that only bumped the generation would leave the captured Holding's transits unsettleable forever (`SettleTransit` → `BAD_ACCOUNT`; GatherClash drops the host). That is a permissionless freeze. The transit path accepts `prev_gen`, and P9 is a failing-first test against M1's code.

### 5.9 Security properties (each one a test, §13.3)

| Property | How |
|---|---|
| **No permissionless freeze** | Every conquest record reaches kind 0 or a Holding through permissionless calls (P6). A siege needs a host on the hex and fails the bell it loses it. Capture due → SettleCapture (D), which always succeeds because the slot and rent were reserved at the horn. A stake or slot owed → SettleSiege (N) any time. FoldMarch only waits (`FoldTooEarly`) or loses an hour; it never blocks a Province. **v1.1:** every keep garrison and Free City garrison is ≤ `MAX_HOST_TROOPS` in every reachable state, so `clash::validate` accepts every input the program can build (P13); v1.0's Σ handoff could freeze a province for the season. |
| **No last-look** | Outcomes come only from clashes over frozen rosters in the resolve or skip of their bell. The horn is declared from the hex, so it reveals nothing sealed, and only the lead host's owner can declare, so landing order inside a bell decides nothing (P2). The vigil is snapshotted at the horn. The capture lock holds. Capture versus slot, credit and immunity are fixed at the completion bell, so settles choose nothing (P2, P4). Only the victim retires its hosts during the season, so no third party times a defender's departure (P11). The keep contest has no instruction at all. |
| **Lag only waits** | A held Province pauses its sieges, keep contest and snapshots; every bell is still counted in order with the same inputs (G7 extended, P4). |
| **No new hot global writer** | No global siege table, capture counter or banner account. Dominion is per March, the keep per Province. |
| **Locks and prices** (D class, cap 0.5; DESIGN §8.7 model) | Province (DeclareSiege P, SettleCapture D, SettleSiege N, RetireHost P): delay only, $2.5k–6.8k per 10 min (unchanged). MarchState (FoldMarch D): folds wait; a hold over 5 hours loses hours, ≈ $75k–204k per lost March-hour. Holding/Citizen/JoinShard at SettleCapture: bookkeeping waits, the mirror already shows the outcome. |
| **Pre-funding** | New creation paths: MarchState (FoldMarch); Holding by SettleCapture (Free City) and by SettleTicket order 2–3 (same path as M1's fresh path). G2: 19 + 2 = **21 creation paths**. |
| **Forgery** | Each keyed read recomputed: DeclareSiege (target Province, target Holding, owner Citizen, nearby Province, source Holding from the entry's host id); SettleCapture (Holding, both Citizens with `citizen_tag == actor` for the captor, both JoinShards from each Citizen's faction and shard); FoldMarch (the 7 members from `march_members`, the March); RetireHost (`prev_home` from the Holding; v1.1: the victim Citizen from `prev_owner_tag`); SettleSiege (the recipient from the record and `owed_gen`; v1.1: the slot Citizen from `actor`); FileOutpost (the anchor Holding). |
| **Immunity farming** | Immunity and the stake payment only when the defender's side broke the siege (K-06; P10). v1.1: immunity and Respite bar only the besieging or occupying faction, and Respite needs expiry or the owner's own liberation (P10, persona `respite_farmer`). |
| **Capture farming** | (v1.1, K-26) A capture credits Dominion and moves stores only when the victim held the holding ≥ `capture_credit_min_bells` (persona `pingpong_pair`, T47). |
| **The 8-byte tag** | `citizen_tag` matched together with the canonical address of a present program-owned Citizen: a 2⁻⁶⁴ collision, ≈ 2⁶⁴ wallet keys to grind. |

---

## 6. Log records (PS2, verifier and herald input)

New kinds. The encoding is M1's (§6 of M1-CONTRACT): every body ≤ 128 B, chained per entity.

| Kind | Name | Key | Payload | Chains |
|---|---|---|---|---|
| 80 | SIEGE_DECLARED | P, Q, site | attacker u8, owner faction u8 (6 = Free City), target u8 (with the reserved slot), declarer tag u64, required u8, vigil {start u16, next u16, from_day u16}, stake u32, src key u64, owner tag u64, lead host id u64 (v1.1; replaces v1.0's on-chain `earliest_completion_bell`, which the herald now computes) | Province, Citizen, Holding (src) |
| 81 | SIEGE_SETTLED | P, Q, site | reason u8 (0 to defender, 1 to attacker on completion, 2 season end, 3 burned), recipient key u64, amount u32, burned u32, slot released u8 (v1.1) | Province, Holding (recipient) |
| 82 | CONQUEST | P, Q, bell | n u8, events [≤ 14] × {site u8 (0xFE keep), code u8, faction u8, progress u8}, records digest [32] (sha256 of `province[4096..4512]`), keep {holder u8, contender u8, progress u8, troops u32, donor host id u64 when taken}, snapshot present u8 + weight[7] u16 | Province |
| 83 | CAPTURE_SETTLED | P, Q, site | outcome u8 (0 capture, 2 Free City capture), credited u8, captor tag u64, victim tag u64 (0 Free City), new gen u8, slot u8, rent moved u64, bonds refunded u64, walls after u32 | Holding, Province, Citizen × 2, JoinShard × 2 |
| 84 | KEEP | P, Q | cause u8 (0 placed, 1 taken), holder u8, from u8, troops u32, consolidated_until u32, gen u32 | Province |
| 85 | MARCH_FOLD | m, n, hour | weight[7] u32, controller u8, contested u8, credit u8, lost u8, captures[6] u16 | MarchState |
| 86 | RETIRE | host_id | troops u32, home key u64, by u8 (0 victim, 1 after end, 2 keep donor) | Province |
| 87 | NEUTRAL | P, Q, site | kind u8 (0 genesis Free City; 1 and 2 reserved), garrison u32, tier u8 | Province |
| 88 | OUTPOST_SETTLED | P, Q, site | citizen tag u64, order u8, gen u8, shield_until i64, anchor key u64 | Holding, Province, Citizen, JoinShard |
| 89 | *(reserved)* | | | |

**CONQUEST event codes:**

| Code | Event | Code | Event |
|---|---|---|---|
| 1 | SIEGE_FAILED (detail bit: broken by defender) | 6 | *(reserved; v1.0 FREE_CITY_EXPIRED)* |
| 2 | OCCUPIED | 7 | KEEP_CONTEST (contender, progress 1) |
| 3 | CAPTURE_DUE (detail bit: credited) | 8 | KEEP_BROKEN |
| 4 | LIBERATED (detail bit: `no_respite`) | 9 | KEEP_TAKEN |
| 5 | OCCUPATION_EXPIRED | 10 | *(reserved for M3: KEEP_PAUSED, unreachable under Rivalry)* |

Per-bell siege and keep progress is not logged as separate records. The herald and verifier recompute it with `conquest_model` over the replayed clash, and the CONQUEST records digest pins the state after every bell that emits one.

---

## 7. Kernel interfaces (`permutation-rules::frontier`, pinned signatures)

```rust
// keep.rs (KEEP_VERSION 1)
pub struct KeepParams { pub bells: u8, pub consolidate_bells: u32, pub home_guard: u32, pub garrison_bps: u16 }
pub struct Keep { pub tile: u8, pub holder: u8, pub contender: u8, pub progress: u8, pub required: u8,
                  pub heartland_safe: bool, pub paused: bool, pub changes: u16, pub troops: u32,
                  pub since_bell: u32, pub consolidated_until_bell: u32, pub contest_from_bell: u32,
                  pub gen: u32, pub last_taken_from: u8 }
pub fn keep_tile(terrain: &[u8; 61], sites: &[u8], site_count: u8) -> Option<u8>;
pub fn open(p: ProvinceCoord, wedge: u8, heartland_max_ring: u8, tile: u8, prm: &KeepParams, bell: u32) -> Option<Keep>; // None for rings 0–1
pub fn garrison(k: &Keep) -> Result<clash::Garrison, KeepError>; // id u64::MAX − 0x1_0000 − gen, walls on, Hold; TroopsAboveCap if > MAX_HOST_TROOPS
pub struct KeepReport { pub holders: u8, pub defender_present: bool }
pub enum KeepEvent { None, Contest(u8), Broken, Paused /* M3 only */, Taken { from: u8, to: u8, garrison: u32, donor: u8, donor_removed: bool } }
pub enum KeepError { TroopsAboveCap }
/// v1.1 (K-19): `capturers` = (entry index, host id, troops) of the contender's non-civilian residents on the
/// tile after the clash. On Taken, the donor = max troops then min host id; its troops are reduced in place.
pub fn advance(k: &mut Keep, b: u32, r: KeepReport, prm: &KeepParams, capturers: &mut [(u8, u64, u32)]) -> Result<KeepEvent, KeepError>;
pub fn lead_host(cands: &[(u8, u64, u32)]) -> Option<u8>;    // shared by the keep donor and DeclareSiege's lead-host check

// control.rs (CONTROL_VERSION 1)
pub const SIDES: usize = 7;
pub fn site_weight_centi(tier: Tier, garrison: MilliTroops, order: u8) -> u16;   // floor(strength_weight / 10_000)
pub fn controller(w: &[u32; SIDES]) -> Controller;            // Unsettled | Side(s) | Contested (strict rule)
pub fn province_control(keep: Option<&Keep>, ring: u16, seat_faction: Option<u8>) -> ProvinceControl;
pub fn march_banner(members: &[ProvinceControl]) -> Banner;
pub fn lasting_changes(series: &[(u32, [u8; N])], min_bells: u32) -> Vec<Change>; // criterion 10's definition, §13.4
pub fn herald_call(map: &ControlMap, day_seed: &[u8; 32]) -> [Option<MarchId>; 6];

// siege.rs (SIEGE_VERSION 3): additions
pub struct SiegeCheck { /* M1 fields */ pub heartland_max_ring: u8, pub frontier_protect_secs: i64,
                        pub frontier_protect_after_secs: i64, pub genesis_ts: i64 }   // attacker_nearby = first holdings only (caller)
pub fn advance(&mut self, b: u32, start: i64, r: BellReport, vigil: Option<&Vigil>) -> SiegeStatus; // Option: Free City
/// v1.1 (R-08): O(1) when end_bell − from ≥ 288 (VIGIL_WINDOW_BOUND); else a forward scan that stops at
/// `required` counted bells: ≤ 287 covers() calls. `cq_covers_bound` counts calls through a test hook.
pub fn can_complete_before(required: u8, vigil: Option<&Vigil>, from: u32, end_bell: u32, genesis: i64) -> bool;
/// Off chain only (herald, wasm, bots): a forward scan bounded by `required` + the vigil bells of ≤ 3 days.
pub fn earliest_completion_bell(required: u8, vigil: Option<&Vigil>, from: u32, genesis: i64) -> u32;
pub fn broken_by_defender(r: BellReport) -> bool;            // defender_present || owner side holds
/// v1.1 (R-05): `respite` is true only on expiry or when the owner's faction holds the hex.
pub fn occupation_ends(holds_occupier: bool, owner_holds: bool, b: u32, start: u32, tenure: u32) -> Option<OccupationEnd /* { kind, respite: bool } */>;
pub fn capture_credited(b: u32, held_since_hour: u16, min_bells: u32, genesis_free_city: bool) -> bool; // v1.1 (K-26)

// holding.rs (HOLDING_VERSION 3): additions
pub fn may_found_outpost(c: &OutpostCheck) -> Result<(), OutpostRefusal>;   // ring, range, tier, share, gate, close, count
pub fn capture_effects(h: &Holding, captor_doctrine: Doctrine) -> CaptureEffects;  // walls, reserve, queue
pub struct LifecycleParams { pub shield_secs: i64, pub shield_late_secs: i64, pub shield_late_after_secs: i64,
                             pub dormant_after_secs: i64, pub release_after_secs: i64, pub outpost_shield_secs: i64 }

// v1.1 (R-16): every change below is ADDITIVE in Wave 1; the v1 signatures and constants keep their values.
// geometry.rs (GEOMETRY_VERSION 2): is_heartland_in(p, faction, heartland_max_ring) (new);
//                                   is_heartland(p, f) == is_heartland_in(p, f, 3) stays; march_members unchanged
// terrain.rs (TERRAIN_VERSION 2): free_city_site(ring_seed, canonical P,Q) -> u8
// camp.rs (CAMP_VERSION 2): place_v2(..., keep_tile: Option<u8>) never returns keep_tile; place(...) stays
// clash.rs (CLASH_VERSION 4): MAX_GARRISONS stays 12; MAX_GARRISONS_WITH_KEEP = 13 for validate;
//                             the camp joins only while fewer than 12 site garrisons stand (unchanged)
// siege.rs: v3 additions beside the v2 API; the program's vigil path (proc/citizen.rs) keeps compiling
```

**Reachable-state test (v1.1, R-01):** `cq_keep_states_validate` builds keeps from every handoff the kernel can produce (up to six 30,000-troop capturers on the tile, `MIN_HOST_TROOPS` remainders, home guards up to the CreateSeason maximum) and asserts `clash::validate` accepts the province's input with that keep and 12 Free City or site garrisons.

`frontier_abi::conquest_model` wraps these over the Province bytes:

- `step`;
- `control_weights(province, bell)`;
- `snapshot_slot`;
- `fold(members, hour)`;
- `decode_records`.

Its unit tests share `control-vectors-v1.json` and `keep-vectors-v1.json` with the kernels.

---

## 8. Off-chain interfaces

### 8.1 Shared spine (`fclient`)

`fclient` gains:

- **instruction builders** for 0xA0–0xA3, 0xA5–0xA7 and the v2 shapes of §5.6;
- **decoders** for Province v2 (the conquest block, keep, snapshots), MarchState, SeasonParams v2;
- **the PS2 parser** for kinds 80–89;
- **`L(kind)`** for the new kinds.

Herald, keeper, bots, verifier and stack use only these, never their own offsets.

### 8.2 Keeper (`frontier-keeper`)

**New duties (added to M1 §8.2):**

| Duty | Trigger | Deadline | Transactions | Class |
|---|---|---|---|---|
| Contested bells | a Province with an active siege, occupation or keep contest, or hostile residents on a garrison or keep hex | close → resolve p99 ≤ 8 slots (criterion 3's active rule) | GatherClash (fast path), ResolveFromInputs **every bell**; the skip planner never batches such a province | D |
| Idle provinces | no activity | at least every **4 game hours** (the fold ring holds 6); **v1.1 (R-23) fold-driven skip:** when a March's oldest unfolded hour is ≥ 3 game hours old, its lagging idle members are skipped at once | SkipQuiet | D |
| Capture settlement | a record of kind 3 | none (the outcome is fixed); p99 ≤ 12 slots after the completing resolve, outside adversary-hold windows (criterion 12) | SettleCapture. **v1.1:** no RetireHost during the season (the victim's own call, K-27) | D |
| Stakes and slots | a stake or slot owed, or the season ended with an active siege | none | SettleSiege | N |
| March folds | each hour h once all 7 members resolved through bell 6h | p99 ≤ **5 game hours** (v1.1, R-23), zero lost hours outside the `march-fold` hold | FoldMarch(m, h, ≤ 6 hours) for every March with an open member | D |
| Horn watcher | SIEGE_DECLARED, KEEP_CONTEST, OCCUPIED, CAPTURE_DUE, LIBERATED, KEEP_TAKEN | — | moves the province to the front of the resolve queue; `/v1/status` counters; an operator alert above a rate | — |
| Season-end flush | `end_bell` | before EndSeason + 2 game hours | the last resolves and skips; FoldMarch for the last hours; SettleCapture for every completion; SettleSiege for every lapsed siege and owed slot; RetireHost (now permissionless) for every remaining previous-generation host | D / N |
| Closes | end + 72 h | — | CloseMarch | N |

**Order of operations** (extends M1's): resolve(p, b) → SettleDeparture → gather(dest) → resolve(dest, b) → SettleCapture for completions of b → FoldMarch(hour) after the hour's last member resolve.

**Status, metrics and journal:**

- `/v1/status` gains `conquest {sieges_active, keeps_contested, occupations, settle_pending, fold_lag_hours_p99, horns_last_bell}`.
- Prometheus gains `fk_contested_resolve_latency_slots`, `fk_fold_lag_hours` and `fk_capture_settle_pending`.
- The journal gains a `conquest` table.
- **Crash safety is unchanged:** queues are rebuilt from the Provinces and MarchStates.

**Adversary model:** holding a contested Province, a MarchState or a capture's accounts only delays. Outcomes are fixed at the bell (§5.9).

**Cost [estimate]:**

- A contested province resolves every bell for the contest or siege length plus vigil pauses: ≈ 72–120 resolves for a keep contest, ≈ 50–80 for a holding siege.
- At the measured 1k rates (≈ 48 keep captures and ≈ 6 holding outcomes a day [sim]) that is ≈ 25–35k extra resolves per season, ≈ 15–25% more than the M1 exit season's ≈ 144k transactions. That is within the keeper's budget at 20×, and the exit run measures it.
- Hourly folds: ≈ 30 Marches × 168 hours ≈ 5k FoldMarch per season.
- Fold-driven skips (v1.1): at most one extra SkipQuiet per idle member per 3 game hours, i.e. at most +33% on M1's idle skips (6 per idle province-day measured → ≤ 8); the rehearsal measures it.

### 8.3 Relay (`permutation-gateway/src/frontier/`)

- **Shape allowlist:**
  - + DeclareSiege and FileOutpost (P, sponsored);
  - + SettleSiege, SettleCapture, FoldMarch and CloseMarch as settle shapes, charged to the requester (v1.3 rule);
  - + RetireHost as a P shape (the victim, sponsored) during the season, and a settle shape after `end_bell` (v1.1).
- **Drain guard:** DeclareSiege's escrow top-up and FileOutpost's escrow are allowances for that kind (escrowed, refunded to the payer); everything else is 0.
- **Quota:** ≤ `sieges_per_day` DeclareSiege per citizen per game day. Beyond it, `QuotaExceeded` with no simulation and no charge.
- **Bodies:** refusals keep `{error, code, detail}` (M1 v1.12). The program's new codes come from `frontier-abi/vectors/errors.json`.

### 8.4 Herald (`frontier-herald`, lib `herald-fold`)

**Code placement (v1.1, R-19):** new modules `cqfmt`, `control`, `conquest`, `standings`, `cqroutes`; `lib.rs`, `fold.rs` and `server.rs` are touched only at the `// MC hook` blocks of §4.5; `roster.rs` and `tests/server.rs` are the design chat's. The herald reads M1 seasons with v1 decoders and MC seasons with v2 (§5.1).

**Fold additions:**

- `herald-fold` decodes kinds 80–89 and keeps per-province conquest state with `conquest_model`: records, keep, snapshots.
- It replays per-bell siege and keep progress over the clash it already recomputes, and checks each CONQUEST records digest. A mismatch raises the `conquest_mismatch` alarm.
- When every opened province has resolved or skipped through b, it writes bell b's control file (M1's completeness rule for overviews).
- Determinism, atomic immutable writes and `.gz` siblings follow M1.

**Paths (all additive; every existing `/h/*` path stays byte-identical):**

| Path | Content | Cache |
|---|---|---|
| `GET /h/control/{bell}.bin`, `/h/control/latest.bin` | `PSFCT1`, the faction map (below) | immutable once complete; latest `max-age=2` |
| `GET /h/overview2/{ring}/{bell}.bin` (+ `latest.bin`) | `PSFOV2` (below); `PSFOV1` unchanged at `/h/overview/…` | as v1 |
| `GET /h/sieges/latest.json`, `/h/sieges/{bell}.json` | active holding sieges and keep contests (below) | latest `max-age=2`; per-bell immutable once complete |
| `GET /h/siege/{P},{Q},{site}/{declared}.json` | one siege: its records and per-bell series `[{bell, holds, defender, vigil, progress}]`, the end and what followed | immutable once ended |
| `GET /h/keep/{P},{Q}.json` | a keep's history: holders, contests, captures | `max-age=5` |
| `GET /h/conquest/{day}.json` | the day's conquest events (below) | immutable once the day is complete |
| `GET /h/standings/series.bin` (`PSFSD1`), `/h/standings/latest.json` | per-hour standings, `official: false` | `max-age=30` / 5 |
| `GET /h/standings/players.json` | (v1.1, R-10) per-player keeps taken, keep-bells held, sieges won, credited captures, liberations; top 100 per faction plus the caller's row via `/h/me` | `max-age=30` |
| `GET /h/call/{day}.json` | Herald's Call per faction (§3.10) | immutable |
| `GET /h/season/final.json` | after EndSeason: the final control map, standings, the movement summary of §13.4 (the same figures criterion 10 reads) | immutable |
| `GET /h/me/{wallet}` | adds `alerts[]` (horn on own holding, keep contest in own province, occupied, captured, liberated) and `sieges[]` | `no-store` |

**`PSFCT1` (`/h/control/{bell}.bin`):**

```
header 32 B : magic "PSFCT1\0\0" · season u64 · bell u32 · n_prov u16 · n_march u16 · reserved u64
province    : n_prov × 8 B, dense province order (kernel ProvinceCoord::index over rings 0..open)
  0 control u8      0–5 keep holder (Seat: seat faction) · 6 neutral (Concord) · 7 unopened
  1 contender u8    0–5 · 7 none
  2 progress u8     keep progress (0..required)
  3 required u8     keep_bells
  4 flags u8        1 control changed this bell · 2 consolidating · 4 heartland (never contestable) · 8 seat or Concord
                    · 16 keep taken this bell · 32 contest broken this bell · 64 clash this bell · 128 reserved
  5 holdings u8     low nibble active holding sieges (sat. 15) · high nibble occupations (sat. 15)
  6 points_lead u8  the side leading the province's strength weight (0–6; 7 none). Points, never a map colour (v1.1, R-15)
  7 points_share u8 that side's share × 255
march       : n_march × 4 B, sorted by the March's centre province index
  0 banner u8       0–5 · 7 contested/none (keep majority, §3.3). The only March colour
  1 points_lead u8  Dominion lead at the last fold (0–6; 7 contested/none). Standings only, never a map colour
  2 keeps u8        keeps held by the banner faction (or the largest holder)
  3 flags u8        1 banner changed this bell · 2 Dominion changed at the last fold · 4 Herald's Call target (any faction)
                    · 8 truce (M3, 0) · 16 hostility (M3, 0)
```

The exit season's control layer is ≈ 1.5 KB per bell and ≈ 1.5 MB per season before gzip; at 50k players ≈ 61 KB per bell [computed].

**`PSFOV2` (`/h/overview2/{ring}/{bell}.bin`):** v1's header with magic `PSFOV2\0\0`. Each record is 40 B, sorted by (P, Q):

| Bytes | Field |
|---|---|
| 0–23 | **exactly the v1 record** (owners are titles: an occupied first holding stays its owner's) |
| 24–28 | occupier per site, 12 × 3 bits (0–5, 7 none) |
| 29–31 | siege state per site, 12 × 2 bits (0 none, 1 progressing, 2 paused by vigil, 3 capture due) |
| 32–34 | site kind, 12 × 2 bits (0 first, 1 holding 2–3, 2 Free City, 3 reserved or free) |
| 35 | keep tile |
| 36–37 | keep troops (u16, saturating) |
| 38 | immune mask (sites with `immune_until > bell`, low 8 sites) |
| 39 | immune mask (sites 8–11) + reserved |

**`/h/sieges/latest.json`:**

```json
{"v":1,"bell":0,"sieges":[{"key":"sg:P,Q,site,declared","p":0,"q":0,"site":0,"kind":"first|other|free",
  "owner":"tag","ownerFaction":0,"attacker":"tag","attackerFaction":0,"declared":0,"required":36,"progress":0,
  "status":"progressing|paused","pauseReason":"vigil|defender|null","etaBell":0}],
 "keeps":[{"p":0,"q":0,"holder":0,"contender":1,"progress":0,"required":72,"since":0,"etaBell":0}]}
```

**`/h/conquest/{day}.json`:**

```json
{"v":1,"day":0,"events":[{"seq":"u64","bell":0,"sig":"…","kind":"siege_declared|siege_failed|occupied|liberated|occupation_expired|capture_due|captured|keep_contest|keep_broken|keep_taken|province_control|march_banner|march_points_lead|free_city|outpost|retired",
  "p":0,"q":0,"site":null,"march":null,"from":0,"to":0,"citizens":["tag"],"detail":{}}]}
```

Every event names the PS2 record it comes from (`sig`, `seq`). `province_control` and `march_banner` are derived from `PSFCT1`.

**`PSFSD1` (`/h/standings/series.bin`):**

- header 32 B: magic `PSFSD1\0\0`, season u64, `first_hour` u32, `n` u32, reserved;
- per hour, 6 factions × 32 B:
  - provinces u16 (keep control), banners u16, keeps_taken u16 (cum), keeps_lost u16 (cum), dominion_bells u32 (cum, from folds);
  - captures u16 (cum), occupations_active u16, sieges_won u16 (cum), sieges_lost u16 (cum), liberations u16 (cum);
  - holdings u16, members_active u16, reserved u32.

**WebSocket additions:**

- Subscription: `{op:"sub", control:true, sieges:true, standings:true}`.
- Server messages:
  - `kind:"control"`: the bell's changed province records `[index u16, record 8 B]…`, ≤ 64 per message, otherwise a resync hint naming the file;
  - `kind:"siege"`: JSON delta;
  - `kind:"alert"` in the wallet scope.
- Ingest → WS p99 ≤ 2 s (criterion 6). The horn alert latency is criterion 12.

**Trust:** the herald is never a trust root. Every file names its slot and source records. The verifier can check every control file (`--herald-dir`, V21).

**Fixture mode:** `frontier-herald --fixture conquest` serves a recorded mini-season (from CQ3-E, `MC_TEST`) with a keep taken, a broken contest, a siege broken by a defender, an occupation and its liberation, a Free City capture, an uncredited recapture and an outpost (v1.1: no raze). The web tests and the design chat use it; from Gate CQ3 the integrator runs it read-only on port 41900 for the design chat (§4.3).

### 8.5 Verifier v3 (`frontier-verify`)

**Checks:**

| # | Check | FAIL codes |
|---|---|---|
| V14 | **Siege legality:** every SIEGE_DECLARED re-judged from post-states at the declaring bell: host on the hex **and the lead host** (v1.1), `may_besiege` v3 with the season's defaults, faction-scoped immunity, cap, **slot reservation and escrow**, `TooLate`, stake | `SiegeIllegal`, `SiegeAfterEndRule` |
| V15 | **Siege progress:** every bell from declaration to end replayed with `Siege::advance` / `advance_quiet` over the replayed BellReport (V7's clash replay) and the snapshotted vigil; must equal the CONQUEST events and digests; civilians never count | `SiegeProgressMismatch`, `SiegeEndMismatch` |
| V16 | **Completion effects:** occupation (occupier, tenure, liberation predicate, **Respite only on expiry or owner liberation, barred faction**), capture due (mirror flip, walls, **credit** and `captures_by`), immunity only when broken by the defender and **only against the besieger's faction**; **stake routing** (owed to the right Holding at `owed_gen`, or burned); **a first holding never changes owner** | `CompletionMismatch`, **`FirstHoldingTransferred`**, `LiberationMismatch`, `ImmunityMismatch`, `StakeMisrouted` |
| V17 | **Capture bookkeeping:** SettleCapture owners, generations, the reserved slot, lists, JoinShards, rent swap, bond refunds, stores kept only if credited; **the capture lock: no Holding-writing instruction on the captured Holding between its CAPTURE_DUE and its CAPTURE_SETTLED**; RetireHost legality (**signed by the victim during the season, anyone only after `end_bell`**) and the `prev_gen` transit path; `DoubleCaptureStranded` reported | `CaptureRule`, `CaptureBookkeeping`, `CaptureLockBroken`, `RetireMismatch` |
| V18 | **Keep contests:** `keep::advance` replayed every bell (resolve and skip): contest start, break, capture, **the donor handoff (the right entry, 50%, the Leave, the cap)**, consolidation, heartland safety | `KeepMismatch`, `KeepInHeartland` |
| V19 | **Control and folds:** each snapshot equals `control` over the replayed state at bell 6h; each MARCH_FOLD equals the fold over the stamped snapshots (lost hours only when a slot really moved past); per-faction Dominion = Σ of credited captures and control-bells | `SnapshotMismatch`, `MarchFoldMismatch`, `CaptureCreditMismatch` |
| V20 | **Season end:** no completion, keep capture or occupation start at or after `end_bell`; every lapsed siege's stake returned and slot released; no outpost filed at or after `end_bell − 24` | **`CompletionAfterEnd`**, `SiegeNotSettled`, `OutpostAfterClose` |
| V21 | (optional `--herald-dir`; FAIL in the stack) every `PSFCT1`, `PSFOV2`, `conquest/{day}.json`, **`sieges/{bell}.json`, `PSFSD1`, `standings/players.json`, `call/{day}.json`** and `final.json` equals the verifier's recompute (v1.1: bots and players act on the sieges, standings and call files) | `HeraldControlMismatch` |
| V22 | (v1.1) **Land and neutral legality:** every FileOutpost and order-2/3 SettleTicket re-judged (ring > `heartland_max_ring`, range 3 from a named final holding, Town prerequisite, share < 50% at filing, the 20% land gate, the free slot, settler-cost escrow and refund); every genesis Free City placed on `terrain::free_city_site` with `free_city_garrison`, Hamlet, walls 0; every keep opened on `keep_tile` with the wedge holder, `keep_home_guard` and the right heartland flag; the site-state invariants S1–S6 (§3.15) after every instruction | `OutpostIllegal`, `NeutralMismatch`, `KeepPlacement`, `SiteStateBroken` |

V1–V13 are unchanged. V7's quiet check also refuses a SkipQuiet over a bell where a siege or keep hex had hostile residents and a live garrison (never quiet). **v1.1 (R-22):** the verifier dispatches on the Season's `program_version`: an M1 season runs V1–V13 with v1 decoders (the recorded M1 fixtures stay valid inputs), an MC season V1–V22.

**Tamper classes (each MUST FAIL with the named code; all required):**

| # | Tamper | Code |
|---|---|---|
| T25 | a declaration in a heartland | `SiegeIllegal` |
| T26 | a declaration against a shielded holding | `SiegeIllegal` |
| T27 | a declaration without a host on the hex | `SiegeIllegal` |
| T28 | +1 progress on a vigil-covered bell | `SiegeProgressMismatch` |
| T29 | progress counted while a defender stood on the hex | `SiegeProgressMismatch` |
| T30 | a first holding's owner rewritten to the occupier | `FirstHoldingTransferred` |
| T31 | a capture into a slot that was not reserved at the horn (or a fourth holding) | `CaptureRule` |
| T32 | a capture's bond refund paid to the captor's funder | `CaptureBookkeeping` |
| T33 | a completion logged at `end_bell` | `CompletionAfterEnd` |
| T34 | a keep taken after 71 bells | `KeepMismatch` |
| T35 | a heartland keep's progress counted | `KeepInHeartland` |
| T36 | a MARCH_FOLD controller flipped, or a fold credited with a member missing | `MarchFoldMismatch` |
| T37 | a snapshot weight off by one tier | `SnapshotMismatch` |
| T38 | a herald control file with one province recoloured | `HeraldControlMismatch` |
| T39 | (v1.1) an outpost filed in a heartland ring (interior outpost) | `OutpostIllegal` |
| T40 | a genesis Free City's garrison raised at OpenProvince | `NeutralMismatch` |
| T41 | a Harvest on a captured Holding between CAPTURE_DUE and CAPTURE_SETTLED | `CaptureLockBroken` |
| T42 | a RetireHost during the season signed by someone other than the victim (foreign retire) | `RetireMismatch` |
| T43 | a stake paid to the site's next owner (wrong generation) | `StakeMisrouted` |
| T44 | immunity granted after a deserted siege, or Respite after an occupier walked away | `ImmunityMismatch` |
| T45 | a keep garrison taken from a host other than the donor, or above 50% of it | `KeepMismatch` |
| T46 | a declaration by a host that is not the hex's lead host | `SiegeIllegal` |
| T47 | a recapture within `capture_credit_min_bells` credited to `captures_by` | `CaptureCreditMismatch` |
| T48 | a `/h/sieges/{bell}.json` with one siege's progress changed | `HeraldControlMismatch` |

**Checks of the checks:** `mutate-v14` … `mutate-v22` features; each lets its own tampers PASS and every other class still FAIL (`crates/verify/mutate.sh`).

**Honest-but-adverse fixtures (MUST PASS):**

- a siege paused by the vigil across midnight, with a vigil change requested mid-siege (the snapshot wins);
- a siege completed in a skip run;
- a besieged Province held past its close (late, at the right bell);
- an occupation ended by the occupier's host leaving (no Respite);
- (v1.1, replaces v1.0's "a contest paused by a two-faction hold", which Rivalry makes impossible: `clash.rs` 1466–1471) a keep contest broken by a defender's arrival and restarted by another faction the next bell;
- a keep taken while its garrison was already 0, with six capturing hosts on the tile (the donor alone pays);
- a Free City captured by Iron (walls kept);
- (replaces "a raze at the 3-holding cap") one citizen's two capture sieges completing in the same bell, settled in reverse order (slots 2 and 3 as reserved);
- a recapture inside `capture_credit_min_bells` (uncredited, stores zeroed);
- a season-end lapse with a slot released;
- a lost fold hour under a 6-hour MarchState hold;
- an M1 fixture (`march-program.json.gz`) verified through the v1 path.

### 8.6 Bots (`frontier-agents`, `frontier-bots`)

**Campaigns without messages:** `agents::campaign::plan(fleet_seed, faction, epoch) -> Plan`, pure and deterministic.

- **Epoch:** the last completed game hour.
- **Inputs:** only immutable herald files of that epoch: `PSFCT1` of its last bell, `PSFOV2`, `/h/sieges/{bell}.json`, `PSFSD1`, `/h/call/{day}.json`. Every bot of a faction computes the same plan.
- **Targets:**
  - contestable enemy keeps adjacent to the faction's controlled provinces or holdings (Herald's Call March first, then by score = value ÷ estimated defence);
  - enemy holdings 2–3 and first holdings outside heartlands;
  - Free Cities.
- **Up to `⌈members / 40⌉` campaigns** with hysteresis: keep a campaign until it is won, has failed twice, or its target is illegal.
- **Assignment:** members within one march (≤ 3 provinces) ranked by spare troops; assault group and reserve; **one common arrival bell** (`muster_bell`, clamped to `end_bell − 1`).
- **Defence:** hosts that can arrive before 25% progress go to a contested keep or besieged hex; the levy (Train + Muster) on a besieged own holding.
- **Boldness margin per archetype:** very skilled 1.3, skilled 1.5, daily 2.0, casual 3.0, idle never. These are pinned by the simulator gate, not tuned on the stack.

**Behaviours** (honest; every write through the relay):

| Behaviour | Who | Rule |
|---|---|---|
| Expand | daily and up | FileOutpost at front provinces where the faction holds < 50% |
| Take the keep | campaign assault group | sealed march to the keep tile; stay; relieve before leaving |
| Besiege | campaign siege group | strike the holding's hex, then the **lead host's owner** declares from the hex when legal (`may_besiege` and the lead-host rank run locally on the freshest state the bot has, re-checked at send time; designed races are allowed by criterion 13's ratio) |
| Defend / relieve | holdings in the threatened March; standing order 25% for skilled and up | as the plan |
| Occupy and hold | the completing host | stays; a second host relieves it |
| Liberate | members near an occupied own holding | strike when stronger by the margin |
| Capture | a declarer with a free slot | holdings 2–3, Free Cities |
| Retire | a victim of a capture | RetireHost its previous-generation hosts when they are not defending (v1.1: only the victim can) |
| Opportunist | casual | Free Cities, dormant holdings, camps |

**Adversarial conquest personas** (on in the exit, each ≤ 1% of bots; criterion 13):

| Persona | Behaviour | Expected outcome |
|---|---|---|
| `siege_heartland` | declares in an enemy heartland | refused `Heartland` |
| `siege_shielded` | declares on a shielded holding | refused `Shielded` |
| `siege_seat` | declares on a Seat | refused `ReservedSite` |
| `siege_late` | declares when it cannot finish before `end_bell` | refused `TooLate` |
| `siege_double` | declares on a besieged site | refused `SiegeBusy` |
| `siege_offhex` | declares without a host on the hex | refused `NotOnHex` |
| `first_taker` | completes a siege on a first holding | occupation only; V16 sees no transfer |
| `capture_cap` | has no free slot (holdings, ticket and reservations) and declares on a holding 2–3; then files an outpost while a capture siege holds its last slot | refused `HoldingsFull` both times (v1.1: no raze) |
| `lead_racer` | (v1.1) a faction-mate with a smaller host on the hex declares in the same bell as the lead host's owner | refused `NotLead`; the record is the lead's whatever the landing order |
| `retire_foreign` | (v1.1) the captor calls RetireHost on the victim's host standing on a contested hex | refused `NotLead`; the host stays and fights |
| `respite_farmer` | (v1.1) an alt in another faction occupies the persona's home, then walks away | `LIBERATED` with `no_respite`; a third faction may declare the next bell |
| `pingpong_pair` | (v1.1) two wallets of one operator in two factions capture one outpost back and forth | captures after the first are uncredited (`captures_by` unchanged, stores zeroed) |
| `phantom_defender` | a Scout on a besieged hex or keep tile | progress continues; civilians never count |
| `vigil_hopper` | changes vigil right before a horn | the horn's snapshot applies |
| `immunity_farmer` | an alt declares, then leaves the hex | the siege fails with no immunity; stake burned |
| `keep_heartland` | marches on an enemy heartland keep | no progress (flag heartland) |
| `keep_consolidation` | attacks a keep inside its consolidation | no progress until `consolidated_until_bell` |
| `outpost_interior` | files an outpost where its faction holds ≥ 50% | refused `OutpostRule` |
| `siege_spammer` | many declarations | relay `QuotaExceeded` beyond `sieges_per_day`; program `SiegeCap` |

**Reports:** `bots/report.json` gains `conquest {campaigns[], keeps {contested, taken, lost}, sieges {declared, won, lost, refused_by_code}, occupations, liberations, captures, outposts, personas{…}}` and **(v1.1, R-11) `activity {departs_per_bot_day, keep_marches_per_bot_day, keep_captures_per_bot_day, declares_per_bot_day}`** per game day. `frontier-bots --conquest` turns the layer on.

### 8.7 Simulator (`frontier-sim`)

1. **`--rules m1|mc`** (default `m1`; every M1 digest and gate unchanged) and **`--policy lone|campaign`** (default `lone`).
   - `mc` = §3 exactly, calling the real `keep`, `control`, `siege` v3 and `holding` v3 kernels.
   - The launch-floor fix lands under `mc`: a siege host is at least `MIN_HOST_TROOPS`; 87% of legal sieges were silently dropped [sim].
   - The dormant and Free City inputs of `control` also land under `mc`.
2. **`--days N`**: join days clamp to `min(21, N − 1)`; the 7-day schedule joins 60% on day 0 and the rest over days 1–5.
3. **`frontier-sim mapmove`**: prints criterion 10's metrics (§13.4) per seed and their p10, p50 and p90, computed with `control::lasting_changes` and `march_banner` from per-bell control series.
   - `--json <file>` writes them; `--check <file>` refuses drift.
4. **`frontier-sim mapmove-gate`**: PASS when every criterion-10 metric meets its threshold on ≥ 4 of 5 seeds. `--controls` adds two negative controls, both of which MUST FAIL:
   - `--rules m1 --policy lone`;
   - `--rules mc-weightmap --policy campaign`, the holding-weight map without keeps.

   The 10k / 28-day gate (`thresholds/mc-28d-10k.json`, balance lab §8):
   - ≥ 5 March banner changes a day;
   - ≥ 15% of Marches changing hands;
   - largest faction ≤ 22% and smallest ≥ 12% of provinces at the end;
   - ≤ 30% of days without a banner change.
5. **Doctrine and criterion** re-runs with `--rules mc`: the CI proxy (`doctrine-gate --set kernel --rules mc --controls`), the 1,500-season band overnight (`doctrines … --rules mc --gate`, 6/6), and `criterion --best-response --rules mc --gate`.
   - The keep-Dominion variant (`--rules mc,keepdom`) is a documented negative control for the band. It is expected to break it (1/6).
6. **Campaign policy:** the same planner as §8.6, or a faithful port with a field-equality test of its scoring (as I-36 did for profiles).
7. **v1.1 measurements owed before the thresholds freeze (Gate CQ1; R-10, R-11, R-12, R-13, R-26):**
   - **Bot profiles.** `Arch::Bot` today plays 24 sessions a day at aggression 0.5 plus a keep roll per session (`model.rs`), one to two orders of magnitude above M1's stack bots (1,712 Departs by 1,000 bots in 7 days ≈ 0.24 per bot-day [measured]). CQ1-B adds **`--bot-profile cq`**: one decision per bot per planner epoch (1 game hour), acting only when the campaign plan assigns it, which is the cadence the stack's bots will run; and **`--bot-profile m1`**, matched to 0.24 Departs per bot-day, as a sensitivity row. The CQ1-B notes print Departs, keep marches and keep captures per bot-day for each. **`thresholds/mc-7d-1k.json` is derived from `--bot-profile cq`**, and those per-bot-day rates become the stack's bot-activity gate (§8.8).
   - **Keep interest:** `keep_aggr` 0.25 / 0.5 / 1.0 rows at 1k/7d and 10k/28d, so the owner sees how movement scales with player interest in keeps.
   - **Coordination:** the 3:1:1:1:1:1 size stress under `--policy campaign`, and a run where one faction plays `campaign` and the other five `lone`; the leader's end share and its Q1 → end trend are reported. If the campaign faction ends above 22% of provinces, the snowball claim is withdrawn and the owner is told before Wave 2.
   - **Net movement:** a 7-day metric, control at `end_bell − 1` against bell 287 (criterion 10e's definition), for every 7-day row (the balance lab's "vs 7 d before" column is 0.0% by construction there).
   - **Forward staging:** `--forward` lets campaign targets be chosen within 2 provinces of a held keep with a resident host, as the chain already allows (§3.2); movement, net movement and snowball are reported with and without it. Off in the thresholds unless the owner decides otherwise (OD-15).
   - **Banner-hour Dominion (OD-14):** `--rules mc,bannerdom` (March-banner hours at a small weight, e.g. 1 control-bell per banner-hour) through the doctrine CI gate; reported, off by default.
   - **The v1.1 rule changes** (donor handoff, lead host, slots, faction-scoped immunity and Respite, capture credit, Frontier-7 dormancy) are all in `--rules mc`; the K-01 package is re-measured as a whole against `out/cand.md`.

### 8.8 Stack (`frontier-stack`)

- **`--conquest`:**
  - creates the Season with `MC_LOCAL_7D` (or the config's preset);
  - turns on the bots' campaign layer and conquest personas, and the conquest holds;
  - confines ports to 41300–41999.
- **Configs:**
  - `cq-exit.toml`: base 41300, `run_id = "cq-exit"`, `season_id = 8`, `g0 = 1788998400`, the M1 archive path, scale 20, 7 days, `drain_bells = 26`, 1,000 bots, chaos, adversary, 5,000 viewers for 24 game hours, keeper B backup delay 8, funded beneficiaries;
  - `cq-rehearsal.toml`: base 41700, test key, otherwise as exit;
  - `cq-nightly.toml`: base 41800, test key, **`MC_TEST`**, 100 bots × **2** game days at 100× (v1.1, R-18: under Frontier-7 no first holding can be besieged on day 1);
  - `cq-smoke.toml`: base 41600, `MC_TEST`.
- **New adversary holds** (each must fire; `hold-skipped` makes a run not exit-grade):

| Hold | Expected effect |
|---|---|
| `contested-province` | hold a Province with an active keep contest or siege for 20 bells from a completing bell; the completion lands at its recorded bell (V15 / V18); settles wait |
| `march-fold` | hold one MarchState for 3 game hours; the folds land late with identical values, no lost hour |
| `capture-settle` | hold a capture's Citizens for 20 bells, **across an outpost SettleTicket of the captor** (v1.1); SettleCapture waits; the outcome is unchanged (the slot was reserved) |

- **Bot-activity gate (v1.1, R-11):** `cq-nightly` and the rehearsal fail when the bots' Departs, keep marches or keep captures per bot-day fall below **50%** of the `--bot-profile cq` simulator rates frozen in `thresholds/mc-7d-1k.json`, so a quiet bot layer is caught before the 9-hour rehearsal.
- **`frontier-stack report`** gains criteria 10–13, **criterion 7's per-day bots-vs-simulator comparison (v1.1, R-26: M1 never computed it; "n.a." in every M1 run)**, `mapmove.json` (the figures, per day and per faction) and **`mapmove.svg`**: seven small hex maps of the day-end control layer, plus the per-day lasting-change bars, drawn by the report in the stack's fixed palette. This is a report artifact, not web UI, so the owner can see the map move without the design chat's renderer.

---

## 9. Web data contracts and the design-chat hand-off

### 9.1 What MC changes on the web (ours)

| File | Change |
|---|---|
| `web/frontier/herald.mjs` | `decodeControl` (PSFCT1), `decodeOverviewV2`, `decodeStandings`, `fetchSieges`, `fetchConquestDay`, `fetchCall`, `fetchFinal`; WS `control` / `siege` / `alert`; v1 decoders unchanged |
| `web/frontier/fcqstate.mjs` (new; v1.1, R-19) | MC's state slices `control`, `sieges`, `standings`, `alerts`, `call` with their update events, and its own herald feed (`startConquestFeed({base})`: poll + WS `{control, sieges, standings}`), started lazily on first use. **`fstate.mjs` and `controller.mjs` are not edited** (they are the design chat's since `ui-shell` changed them after `CQ0`); HANDOFF-UI names the one call the design chat may add to start the feed early |
| `web/frontier/fcontrol.mjs` (new) | pure helpers over the `control` slice: `provinceControl(i)`, `marchBanner(m)`, `controlChanges(prev, cur)` (criterion 10's lasting-change logic, shared vectors with the report), and **`mapFaction(p, q, rec)`** (v1.1, R-17): the keep holder when the control slice has the province, else `majorityOwner(rec)` (M1 data) |
| `web/frontier/fconquest.mjs` (new) | siege composer data: legality via WASM `may_besiege` and the lead-host rank on the herald state (the relay's simulation stays authoritative), required bells and ETA (vigil-aware, the kernel's off-chain `earliest_completion_bell`), "who can arrive together" (`reachable`), keep contest ETA; **action builders** for DeclareSiege, FileOutpost and RetireHost through the relay shapes (the composer UI that calls them is the design chat's) |
| `web/frontier/fland.mjs` | outpost data: the outpost rule, range and share hints, the 20% gate |
| `web/frontier/fi18n.mjs`, `web/lang/en-frontier.mjs` | JA/EN text for errors 62–78 and the event kinds (generated table from `errors.json`); additive |
| `web/frontier/abi.mjs`, `fcodec.mjs`, `web/sdk/frontier/**` | regenerated from `frontier-abi`. **v1.1 (R-20):** `abi.mjs` exports `RULESET_HASHES = {m1, mc}`; `fchainio`'s pin (`setPin`, ours) accepts both in **read-only spectate mode** (v1 decoders for an M1 season) and only the MC hash for play. The v2 WASM carries both rule sets (the kernel changes are additive, §5.1), and `wasm.mjs` (ours) reports the pinned season's hash from `rulesetHash()`, so `app.mjs`'s `seasonKernel` check (the design chat's file) passes for practice and in-browser verification of an M1 season too. The design chat's preview of the paused M1 herald keeps working after it merges MC |
| `web/frontier/map/layers.mjs` | **one change:** `provinceFill(rec)` and `paintProvince`'s sigil take their faction from `fcontrol.mapFaction(p, q, rec)` (same palette, no new visual element), which falls back to `majorityOwner` for M1 data. This covers the vector path (`?art=0`). **The art path (the default) is `sprites.mjs paintStrategic`, the design chat's file**, which tints and draws the sigil from `majorityOwner(e.rec)`: until the design chat switches it to `mapFaction` (HANDOFF-UI's explicit request), art mode shows the old colouring (R-17) |
| `frontier-wasm` exports | `may_besiege`, `siege_required_bells`, `siege_eta`, `bells_outside_vigil`, `keep_eta`, `province_control`, `march_banner`, `march_of`, `is_heartland`, `may_found_outpost` |

Untouched: §4.5's list. `npm test` covers decoder vectors shared with Rust, the `controlChanges` vectors shared with the report, `web-lang` completeness, `web-sdk` freshness and the WASM hash.

### 9.2 Facts the design chat can render (semantics, not visuals)

| Fact | Source | States | Notes |
|---|---|---|---|
| **The faction map** | `PSFCT1` province `control` (slice `control`), via `fcontrol.mapFaction(p, q, rec)` | faction 0–5, neutral (Concord), unopened; heartland and seat flags | The map fill **and the province sigil**. `layers.mjs` (vector path) already switches to it. **Request to the design chat:** switch `sprites.mjs paintStrategic`'s tint and sigil, and any other `majorityOwner` use that colours a province, to `mapFaction`; until then art mode shows holding majorities |
| A keep and its contest | `PSFCT1` contender, progress/required, flags; `PSFOV2` keep tile and troops; `/h/sieges` `keeps[]` | held, contested (contender, progress, ETA), consolidating, heartland-safe | the keep sits on a fixed tile; a contest is public from its first bell |
| March banner and Dominion lead | `PSFCT1` marches | banner faction or contested; Dominion lead (points); Herald's Call flag | **only the banner may colour the map.** The Dominion lead is a standings figure: show faction totals, never a second March colour. They differ by design, and an occupation moves only points (v1.1, R-15) |
| Site title and occupier | `PSFOV2` owners + occupiers + kinds | owner; occupier; first, outpost or Free City | D9: a first holding's title never changes |
| A holding siege | `PSFOV2` siege bits; `/h/sieges/latest.json`; `/h/siege/…` | progressing, paused (vigil, defender), capture due; progress, required, ETA | the horn is a WS alert |
| Immunity and Respite | `PSFOV2` immune masks | — | "cannot be besieged until bell b" |
| Events of a bell | `PSFCT1` flags; `/h/conquest/{day}.json` | keep taken, contest broken, siege completed or failed, occupation began or ended, capture (credited or not), outpost, retire | for flashes, ticker and captions |
| Standings | `PSFSD1`, `/h/standings/latest.json` | per faction per hour | unofficial game points |
| Herald's Call | `/h/call/{day}.json` | one March per faction, Rally flag | display only |
| Own alerts | `/h/me`, WS `alert` | horn, keep contest at home, occupied, captured, liberated | sound and notification policy are the design chat's |
| Composer data | `fconquest.mjs`, `fland.mjs` | legal or the refusal code; required bells, ETA | error text in `fi18n.mjs` |
| Reserved for M3 | `PSFCT1` march flags 8 and 16 | truce, hostility | always 0 in MC |

### 9.3 `docs/frontier/conquest/HANDOFF-UI.md` (written by CQ3-D, refreshed by CQ4-D)

It contains:

- the facts table above, with every field and state;
- the decoders and state slices and their update events;
- the WS messages;
- **the fixture files** (`herald --fixture conquest`; `permutation-gateway/test/fixtures/frontier/cq/*`: a mini-season with every event kind; from CQ4-D, a recorded slice of the rehearsal season);
- the reserved M3 bits;
- what is not available yet (Dominion per member is unofficial; no rewards for the Call);
- the one-line note on `layers.mjs`;
- (v1.1) **the explicit requests**: switch `sprites.mjs paintStrategic` (tint and sigil) to `fcontrol.mapFaction`; optionally call `fcqstate.startConquestFeed()` from the controller; wire the composer to `fconquest`'s action builders; never paint the Dominion lead as a colour;
- (v1.1) the data sources: `herald --fixture conquest` on 41900 from Gate CQ3; the spectate mode that accepts the M1 and MC hashes; and the statement that **until the design chat ships the `sprites.mjs` switch, the moving map is visible in vector mode (`?art=0`) and in the report's `mapmove.svg`**.

The design chat is told when the hand-off lands (the integrator's wave-3 note). MC does not wait for the design chat.

### 9.4 Replay page (`frontier/demo-replay`)

Out of MC's units. The hand-off documents how the replay's data stores read `PSFCT1`, `/h/conquest/{day}.json` and `PSFSD1` through the same `herald.mjs` decoders and `fcontrol.controlChanges`. An add-on unit can be assigned by amendment if the owner wants the English demo video to be cut from the replay page in MC (OD-13).

---

## 10. Operations constants

| Item | Value |
|---|---|
| Ports | §4.3 |
| Province v2 rent | 24,709,120 lamports (+3,251,200 vs M1) |
| MarchState rent | 1,950,720 lamports |
| RFI gate | 290,000 (fallback 300,000 by amendment) |
| Worst clash budget | 410k (420k with the fallback) |
| Budgets | §5.4, regenerated at Gate CQ4 |
| Release `.so` | recorded by CQ4-A (`--twice`), est. 0.98–1.02 MB |
| Exit archive | `data/drand-archive-quicknet-g0-1788998400`, G0 **1788998400** (M1's, reused read-only) |
| Thresholds | `frontier-sim/thresholds/mc-7d-1k.json`, `mc-28d-10k.json` (frozen at Gate CQ1) |

---

## 11. Implementation units and file ownership

**Ownership rules:**

- A unit owns exactly the paths listed for its wave (globs are recursive). Every other path is read-only for it.
- Paths no unit lists are frozen for the wave, except for the integrator's files (every dependency section of every `Cargo.toml`, every lockfile, `package.json`, `rust-toolchain.toml`, `.gitignore`) and the integration window.
- **§4.5's design-chat files belong to no unit in any wave.**
- `frontier-abi/src/layout/**` is **frozen after Gate CQ1**, and `permutation-frontier/src/layout/**` after Gate CQ2. Changes are by amendment only, with vectors regenerated in the same merge.
- Within a wave, a unit that needs another unit's new API codes against the signatures pinned in §5–§9, and the integrator merges in the listed order.
- Unit ids are `CQ<wave>-<letter>`; branch names are `frontier/cq-<wave><letter>-<name>`.

### Wave 1 — Rules measured, ABI v2 pinned. Merge order: CQ1-A, CQ1-C, CQ1-B, CQ1-D

| Unit | Brief | Owns |
|---|---|---|
| **CQ1-A rules-conquest** | Kernels per §7: `keep` (new), `control` (new, incl. `lasting_changes` and `herald_call`), `siege` v3, `holding` v3 (outposts, capture effects, lifecycle params), `geometry` v2 (heartland param), `terrain` v2 (`keep_tile`, `free_city_site`), `camp` v2, `clash` v4 (13 garrisons; the camp rule unchanged); `RULES_VERSION_FRONTIER` 11 and the `ruleset_hash_input_v2` additions; vectors; tests (`cq_*`), including keep-contest closed form over quiet runs (equal to bell by bell), the strict controller rule, `lasting_changes` on hand-built series, M1 digests unchanged. **v1.1:** every change additive (§5.1, §7: `is_heartland_in`, `MAX_GARRISONS` stays 12, `MAX_GARRISONS_WITH_KEEP`, siege v3 beside v2), so `permutation-frontier`, `frontier-abi` v1 and `frontier-sim` keep compiling unchanged; the donor handoff and `lead_host`; `cq_keep_states_validate` (R-01); the bounded `can_complete_before` with `cq_vigil_window_bound` and `cq_covers_bound` (R-08); `capture_credited`; `occupation_ends` with Respite (R-05) | `permutation-rules/src/frontier/{keep,control,siege,holding,geometry,terrain,camp,clash,mod}.rs`; **`permutation-rules/tests/frontier_*.rs`** (all of them, incl. `frontier_world.rs`, `frontier_bounds.rs`, `frontier_clash_*.rs`); `permutation-rules/vectors/{keep,control}-vectors-v1.json`; `docs/frontier/conquest/CQ1-A-NOTES.md` |
| **CQ1-B sim-conquest** | `frontier-sim`: `--rules mc`, `--policy campaign`, `--days`, the launch-floor fix under `mc`, `mapmove`, `mapmove-gate --controls`, the 10k/28d gate; doctrine and criterion runs on `mc`; **writes `thresholds/mc-7d-1k.json` and `mc-28d-10k.json`** (p10/p50/p90 over 10 seeds and the floors of §13.4); reports the K-01 package's numbers against the balance lab's (`out/cand.md`) in its notes. **v1.1:** the measurements of §8.7 item 7 (bot profiles `cq` and `m1` with per-bot-day rates, `keep_aggr` 0.25/0.5/1.0, the coordination stresses, 7-day net movement from bell 287, `--forward`, `bannerdom`, the v1.1 rules); thresholds derived from `--bot-profile cq` | `frontier-sim/**` (manifest dependency sections: integrator); `docs/frontier/conquest/CQ1-B-NOTES.md` |
| **CQ1-C abi-v2** | `frontier-abi` v2 **under new names beside v1** (`frontier_abi::v2::*`; v1.1, R-16): layouts §5.2 (Province v2 block, site mirror v2 fields, Holding / Citizen / JoinShard reserve fields, SeasonParams v2, MarchState), tags §5.4 (`Ix::ALL_V2`), errors §5.3, logs §6, budget placeholders and `L(kind)` sets, presets `MC_LOCAL_7D`, `MC_SEASON_28` and `MC_TEST`, `conquest_model` (§5.7) over the CQ1-A kernels, **`clash_model` v2** (keep and Free City garrisons in the input, `PSF-CLASH-INPUT-v2` digest, the skip tile-mask quiet model; v1 `clash_model` unchanged), `prologue` and `entry` updates, `abi-vectors` (+ `v2/conquest.json`, `v2/presets.json`, `v2/errors.json`), the `RULESET_HASH_V2` pin test (M1's `RULESET_HASH` unchanged and still pinned) | `frontier-abi/**`; `docs/frontier/conquest/CQ1-C-NOTES.md` |
| **CQ1-D formats-docs** | The herald format codecs `PSFCT1`, `PSFOV2`, `PSFSD1` and the JSON schemas of §8.4 (`herald/src/cqfmt.rs`, encoder and decoder) with shared vectors; the JS decoders in `herald.mjs` (additive) with `web-frontier-cq-formats.test.mjs`; `scripts/cq-ownership-check.sh`; docs: this contract committed as `docs/frontier/conquest/CONQUEST-CONTRACT.md`, `DECISIONS.md` part CQ (K-01…K-27, OD-1…OD-16, R-01…R-26), a DESIGN rev 3.2 section "Conquest milestone" (the keep, the control layer, the defaults) | `frontier-node/crates/herald/src/cqfmt.rs`; `frontier-node/fixtures/cq/formats/**`; `permutation-server/web/frontier/herald.mjs`; `permutation-gateway/test/web-frontier-cq-formats.test.mjs`; `permutation-gateway/test/fixtures/frontier/cq/formats/**`; `scripts/cq-ownership-check.sh`; `docs/frontier/**` except other units' notes |

**Integrator in wave 1:**

- cuts `frontier/cq-integ` at `CQ0`;
- applies dependency requests;
- runs `scripts/cq-regen.sh` after each merge (in Wave 1 every v1 generated output must come out byte-identical, which proves the staging);
- runs the overnight items of Gate CQ1;
- records `CQ0` and the new `RULESET_HASH` in `CQ1-NOTES` (`docs/frontier/conquest/integ-CQ1-NOTES.md`).

### Wave 2 — Program, keeper, herald, bots. Merge order: CQ2-A, CQ2-B, CQ2-C, CQ2-D, CQ2-E, CQ2-F

| Unit | Brief | Owns |
|---|---|---|
| **CQ2-A prog-core-land** | v2 plumbing (`layout/**` accessors, dispatch for 0xA0–0xA7, errors, events, init paths); CreateSeason v2; OpenRing / OpenProvince (keep, genesis Free City, rent); FoldOccupancy; FileOutpost (0xA3) and SettleTicket into slots 2–3 (v1.1: Citizen `slots`, slot-indexed `holding[]`, `held_since_hour`); ReleaseDormant with the S3 refusal and the record reset (§3.15); the capture lock and `[province]` on Harvest, Build, Train and Explore (Build `tier_next`); Reveal's Free City rule; SettleExplore; svm `g01_cq_`/`g02_cq_`/`g03_cq_`/`g13_cq_` rows for these. **Its first commit** (the v2 dispatch table for every tag of §5.4 with the handler names of §5.5 as `NotImplemented` stubs in `proc/conquest.rs`, `proc/mod.rs`, the `layout/**` accessors and the svm registries) **is pre-merged by the integrator on day 1 of the wave**; CQ2-B and CQ2-C rebase on it, and `proc/conquest.rs` belongs to CQ2-C from then on | `permutation-frontier/src/{lib,ix,error,addr,init,events,prologue}.rs`; `permutation-frontier/src/layout/**`; `permutation-frontier/src/proc/{mod,season,map,citizen,holding,reveal}.rs`; `permutation-frontier/svm-tests/src/{ix,cover,world}/mod.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{season,map,citizen,holding,reveal}.rs`; `permutation-frontier/svm-tests/src/world/{land,holding}.rs`; `permutation-frontier/svm-tests/tests/{map,citizen,holding,reveal,harness,coverage}.rs`; `permutation-frontier/svm-tests/tests/common/**`; `docs/frontier/conquest/CQ2-A-NOTES.md` |
| **CQ2-B prog-clash** | ResolveFromInputs and SkipQuiet with the conquest step (§5.7): keep and Free City garrisons, the donor handoff, snapshots, CONQUEST log, digests v2; GatherClash `prev_gen`; **G1 RFI worst at 13 garrisons with 12 completions at an hour boundary and a keep taken with six 30,000-troop capturers** (+ M1's 1,240 fills) → the 290k / 300k decision recorded; G11 extended (`g11_cq_`); the quiet model and its tile-mask cache; **fixes to the shared models it is the first to execute** (v1.1, R-21) | `permutation-frontier/src/proc/clash.rs`; `permutation-frontier/svm-tests/src/{ix,cover,world}/clash.rs`; `permutation-frontier/svm-tests/tests/clash.rs`; **`frontier-abi/src/{conquest_model,clash_model}.rs` and their vectors** (layout/** stays frozen; vectors regenerated in the same merge); `docs/frontier/conquest/CQ2-B-NOTES.md` |
| **CQ2-C prog-conquest** | `proc/conquest.rs`: DeclareSiege, SettleSiege, SettleCapture, FoldMarch, RetireHost, CloseMarch; the `prev_gen` / `prev_home` transit path (SettleDeparture and its return settle in `proc/host.rs`, SettleTransit in `proc/transit.rs`); the capture lock on Muster, Dissolve, Garrison and Depart (`proc/host.rs`); v1.1: the lead-host check, slot reservations and their release, faction-scoped immunity, the victim-only RetireHost, the donor Leave path shared with RetireHost; properties P1–P13 (`p_cq_*`); `g01_cq_`/`g02_cq_`/`g03_cq_`/`g13_cq_` rows for these | `permutation-frontier/src/proc/{conquest,transit,host}.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{conquest,transit,host}.rs`; `permutation-frontier/svm-tests/src/world/{conquest,transit}.rs`; `permutation-frontier/svm-tests/tests/{conquest,transit,host}.rs`; `docs/frontier/conquest/CQ2-C-NOTES.md` |
| **CQ2-D keeper-cq** | `fclient` (§8.1) **with version-dispatching readers (v1 and v2) and its twin tests switched to compare both** (`instructions_are_frontier_abis` over `Ix::ALL` and `Ix::ALL_V2`, `magics_and_rent_are_frontier_abis` with MarchState and Province v2; R-16, R-22), and the keeper duties of §8.2 (contested-bell planner, 4-hour idle skips and fold-driven skips, settles, folds, horn watcher, season-end flush, status and metrics); in-process tests over the native `conquest_model` and the test-beacon `.so` once CQ2-A/B/C merge | `frontier-node/crates/{fclient,keeper}/**`; `docs/frontier/conquest/CQ2-D-NOTES.md` |
| **CQ2-E herald-cq** | Fold of kinds 80–89, the files and routes of §8.4 (incl. `standings/players.json`), WS additions, standings, Call, final.json, `--fixture conquest` (a synthetic fixture now; the recorded one from CQ3-E); v1/v2 dispatch; determinism and completeness tests (`cq_*`) | `frontier-node/crates/{herald,findex}/**` **except** `herald/src/cqfmt.rs` (which it uses), `herald/src/roster.rs` and `herald/tests/server.rs` (the design chat's); `herald/src/{lib,fold,server}.rs` only inside `// MC hook` blocks (§4.5); `docs/frontier/conquest/CQ2-E-NOTES.md` |
| **CQ2-F bots-cq** | `agents::campaign`, the behaviours and personas of §8.6, `--conquest`, the report additions; `campaign::plan` identical across 32 bots with shuffled observation order; personas' expected codes on recorded herald fixtures | `frontier-node/crates/{agents,bots}/**`; `docs/frontier/conquest/CQ2-F-NOTES.md` |

### Wave 3 — Verifier, stack, relay, web data, integration. Merge order: CQ3-E, CQ3-A, CQ3-B, CQ3-C, CQ3-D

| Unit | Brief | Owns |
|---|---|---|
| **CQ3-A verifier-cq** | V14–V22, T25–T48, `mutate-v14…v22`, the honest-but-adverse fixtures of §8.5 (recorded from CQ3-E's mini-seasons), report schema additions; **the v1/v2 dispatch (R-22): M1 seasons and the existing `fixtures/verify/*` run V1–V13 through the v1 path unchanged, so T1–T24 and every M1 `tamper_` test keep their inputs; nothing is re-recorded** | `frontier-node/crates/verify/**`; `frontier-node/fixtures/**` (except `fixtures/cq/{formats,mini,g14}/**`); `docs/frontier/conquest/CQ3-A-NOTES.md` |
| **CQ3-B stack-cq** | `--conquest` (§8.8), the configs, the three holds, criteria 10–13 deciders and `mapmove.json` / `mapmove.svg`, **criterion 7's per-day bots-vs-simulator comparison (R-26) and the bot-activity gate (R-11)**, the 41300–41999 port guard, `scripts/cq-nightly.sh`, `scripts/cq-run-exit.sh` (`--expect-so-sha256`, `--adversary`); the criterion-10 decider tested on a recorded static run (MUST FAIL) and a recorded moving run (MUST PASS) | `frontier-node/crates/{stack,localnet,drand-replay}/**`; `frontier-node/configs/**`; `scripts/{cq-nightly.sh,cq-run-exit.sh}`; `docs/frontier/conquest/CQ3-B-NOTES.md` |
| **CQ3-C relay-sdk-cq** | Relay shapes, allowances and quotas (§8.3); JS SDK codecs, shapes and budgets for v2; `sync-web-sdk.mjs --check` | `permutation-gateway/src/frontier/**`; `permutation-gateway/client/src/frontier/**`; `permutation-gateway/test/frontier-*.test.mjs`; `permutation-gateway/scripts/sync-web-sdk.mjs`; `permutation-server/web/sdk/frontier/**` (generated); `docs/frontier/conquest/CQ3-C-NOTES.md` |
| **CQ3-D web-data-cq** | §9.1's files, WASM exports (both rule sets, the pinned hash), JA/EN strings (additive), `HANDOFF-UI.md` with its explicit requests, web tests and fixtures; **E7's script checks the vector path (`?art=0`)** | `permutation-server/web/frontier/{herald,fcqstate,fcontrol,fconquest,fland,fi18n,abi,fcodec,fchainio,wasm}.mjs`; `permutation-server/web/frontier/map/layers.mjs`; `permutation-server/web/lang/en-frontier.mjs`; `frontier-wasm/**` (lock: integrator); `scripts/build-wasm.sh`; `permutation-gateway/test/web-frontier-cq-*.test.mjs` (except `-formats`); `permutation-gateway/test/fixtures/frontier/cq/**` (except `formats/`); `docs/frontier/conquest/HANDOFF-UI.md`; `docs/frontier/conquest/CQ3-D-NOTES.md` |
| **CQ3-E itest-cq** | `inproc_cq_day` (100 bots + keeper + herald over `ChainPort::InProcess`, test key, **one game day at `MC_TEST`** with scripted actors where needed (v1.1, R-18): ≥ 1 keep taken, ≥ 1 contest broken, ≥ 1 siege broken by a defender, ≥ 1 occupation and liberation, ≥ 1 Free City capture, ≥ 1 outpost); `g14_cq_` (100 bots × 2 game days at `MC_TEST` through the program with the native `conquest_model` re-run every bell; must show ≥ 1 keep taken and ≥ 1 siege completed; verifier PASS; a tampered CONQUEST digest → FAIL); records the mini-season fixtures for CQ3-A and the herald; fixes found in keeper, herald, bots and the shared models | `frontier-node/crates/{itest,keeper,herald,findex,agents,bots,fclient}/**` (herald exceptions as CQ2-E); **`frontier-abi/src/{conquest_model,clash_model}.rs` and their vectors** (R-21); `frontier-node/fixtures/cq/{mini,g14}/**`; `docs/frontier/conquest/CQ3-E-NOTES.md` |

CQ3-E merges first, because the verifier's fixtures come from its recordings. CQ3-A codes against synthetic fixtures until then.

### Wave 4 — Hardening and runs. Merge order: CQ4-A, CQ4-B, CQ4-D, CQ4-C, CQ4-E

| Unit | Brief | Owns |
|---|---|---|
| **CQ4-A prog-gates** | G1–G13 complete for every new and changed kind (`RELEASE_CHECK=1`: no pending row, no `NotImplemented`); budgets and `L(kind)` regenerated from the release `.so`; **the MC release build record** (`--twice`); the RFI decision applied (amendment if 300k) | `permutation-frontier/**`; `frontier-abi/src/budgets.rs`; `frontier-abi/vectors/budgets.json`; `docs/frontier/conquest/CQ4-A-NOTES.md` |
| **CQ4-B node-fix** | Fixes in keeper, herald, verifier, bots, itest, localnet, relay and SDK from the nightlies and the rehearsal | `frontier-node/crates/{fclient,findex,keeper,herald,verify,agents,bots,itest,localnet,drand-replay}/**`; `frontier-node/fixtures/**`; `permutation-gateway/src/frontier/**`; `permutation-gateway/client/src/frontier/**`; `permutation-gateway/test/frontier-*.test.mjs`; `permutation-server/web/sdk/frontier/**`; `docs/frontier/conquest/CQ4-B-NOTES.md` |
| **CQ4-C season-ops** | Three consecutive green nightlies (41800); **the 7-day test-key rehearsal** (41700, `cq-rehearsal.toml`, chaos, adversary, 5,000 viewers): all of §13.4's criteria, marked "not exit-grade"; criterion 7's stack-vs-sim comparison; run records | `frontier-node/crates/stack/**`; `frontier-node/configs/**`; `docs/frontier/conquest/runs/**`; `docs/frontier/conquest/CQ4-C-NOTES.md` |
| **CQ4-D web-fix** | Web data fixes; `HANDOFF-UI.md` refreshed with a recorded slice of the rehearsal; WASM hash; `npm test` | as CQ3-D's list |
| **CQ4-E docs** | DESIGN rev 3.2 conquest section final (measured numbers), DECISIONS, the join-page rule text (what a player can lose, D9, keeps), the keeper guide additions, the cost model of §8.2 with the rehearsal's numbers | `docs/frontier/**` except `docs/frontier/conquest/{runs/**,CQ4-[A-D]-NOTES.md,HANDOFF-UI.md}`; `docs/frontier/conquest/CQ4-E-NOTES.md` |

### Wave 5 — Exit. Merge order: CQ5-B, CQ5-A, CQ5-C

| Unit | Brief | Owns |
|---|---|---|
| **CQ5-A exit-run** | The exit season (§13.4) with real rounds and the MC release `.so`, verify, tamper, report; a re-run after any CQ5-B fix that touches the program, the kernels or the keeper | `frontier-node/crates/stack/**`; `frontier-node/configs/**`; `docs/frontier/conquest/runs/**` |
| **CQ5-B last-fixes** | Any fix the exit finds, anywhere except stack, configs, docs and §4.5's files; a kernel fix that changes outcomes follows §3.13 | `permutation-frontier/**`; `frontier-abi/**`; `permutation-rules/**`; `frontier-sim/**`; `frontier-node/crates/{fclient,findex,keeper,herald,verify,agents,bots,itest,localnet,drand-replay}/**` (herald exceptions as CQ2-E); `frontier-node/fixtures/**`; `permutation-gateway/**` (except package files, `screens/**` and §4.5's design-chat tests); §9.1's web files; `frontier-wasm/**`; `scripts/**` |
| **CQ5-C exit-report** | `docs/frontier/conquest/CQ-EXIT-NOTES.md` (EN) with the `mapmove.svg` and the criteria; a JA summary; DECISIONS final; the owner's playtest decision sheet | `docs/frontier/**` except `docs/frontier/conquest/runs/**` |

---

## 12. Wave gates (exact commands)

**Common preamble.** Run from the integration worktree `$R` (`.claude/worktrees/cq-integ`) on `frontier/cq-integ` after the wave's merges. `CQ0` is recorded in `integ-CQ1-NOTES.md`.

```sh
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
cd "$R"; CQ0=d11d058
for p in ${CQ_PORTS:-}; do            # empty for gates CQ1–CQ3 except the itest line (127.0.0.1:0)
  { [ "$p" -ge 41300 ] && [ "$p" -le 41999 ]; } || { echo "MC port $p is outside 41300-41999"; exit 1; }
  if lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; then echo "MC port $p is busy"; exit 1; fi
done
scripts/cq-ownership-check.sh $(git for-each-ref --format='%(refname:short)' 'refs/heads/frontier/cq-*' | grep -v cq-integ)   # v1.1: first-parent, each branch's own fork point
git diff --quiet "$CQ0" -- permutation-server/web/session.mjs permutation-chain/src
scripts/cq-regen.sh --check                                    # v1.1: every generated output fresh (§4.2)
```

The ownership check is skipped in Gate CQ1 until CQ1-D has merged the script: CQ1's last line runs it.

**Gate CQ1** (all must exit 0):

```sh
cargo fmt --all -- --check
cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings
cargo test --locked --release -p permutation-rules            # incl. cq_*, m1_rules_keep_the_phase_b_digests, occupancy_empty_keeps_the_phase_b_digests, the 4,320-input equivalence
cargo test --locked -p frontier-abi
cargo run --locked -p frontier-abi --bin abi-vectors -- --check
cargo test --locked -p permutation-chain
(cd frontier-sim && cargo fmt -- --check && cargo clippy --locked --release --all-targets -- -D warnings && cargo test --locked --release)
(cd frontier-sim && cargo run --release -- criterion --best-response --seeds 3 --first-seed 30001 --gate)               # M1 rules: unchanged
(cd frontier-sim && cargo run --release -- doctrine-gate --controls)                                                  # M1 rules: unchanged
(cd frontier-sim && cargo run --release -- criterion --best-response --seeds 3 --first-seed 30001 --rules mc --policy campaign --gate)
(cd frontier-sim && cargo run --release -- doctrine-gate --set kernel --rules mc --controls)
(cd frontier-sim && cargo run --release -- mapmove --rules mc --policy campaign --bot-profile cq --agents 1000 --bots 0.99 --days 7 --seeds 10 --first-seed 2001 --check thresholds/mc-7d-1k.json)
(cd frontier-sim && cargo run --release -- mapmove-gate --rules mc --policy campaign --bot-profile cq --agents 1000 --bots 0.99 --days 7 --seeds 5 --first-seed 1101 --thresholds thresholds/mc-7d-1k.json --controls)
# v1.1 reported rows (R-10…R-13), printed into CQ1-B-NOTES; not exit-gating except the coordination check below
(cd frontier-sim && cargo run --release -- mapmove --rules mc --policy campaign --bot-profile m1 --agents 1000 --bots 0.99 --days 7 --seeds 10 --first-seed 2001)
(cd frontier-sim && for k in 0.25 0.5 1.0; do cargo run --release -- mapmove --rules mc --policy campaign --bot-profile cq --keep-aggr $k --agents 1000 --days 7 --seeds 10 --first-seed 2001; done)
(cd frontier-sim && cargo run --release -- mapmove --rules mc --policy campaign --sizes 3,1,1,1,1,1 --agents 1000 --days 7 --seeds 10 --first-seed 2001)
(cd frontier-sim && cargo run --release -- mapmove --rules mc --policy campaign:0,lone:1-5 --agents 1000 --days 7 --seeds 10 --first-seed 2001 --check-leader-max 0.22)
(cd frontier-sim && cargo run --release -- mapmove --rules mc --policy campaign --forward --agents 1000 --days 7 --seeds 10 --first-seed 2001)
(cd frontier-sim && cargo run --release -- doctrine-gate --set kernel --rules mc,bannerdom --controls --report-only)
(cd frontier-node && cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace)   # v1.1: green because Wave 1 is additive (§5.1)
(cd permutation-gateway && npm ci --ignore-scripts && npm test)                                                                                         # likewise: v1 vectors, SDK and WASM hash unchanged
cargo check --locked -p permutation-frontier                                                                                                            # v1.1: the M1 program still compiles against the additive kernels
scripts/cq-ownership-check.sh $(git for-each-ref --format='%(refname:short)' 'refs/heads/frontier/cq-1*')
```

**Overnight (must pass before Gate CQ2 opens):**

```sh
(cd frontier-sim && cargo run --release -- doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --rules mc --policy lone --gate)   # 6/6 in band
(cd frontier-sim && cargo run --release -- mapmove-gate --rules mc --policy campaign --agents 10000 --days 28 --seeds 4 --first-seed 1101 --thresholds thresholds/mc-28d-10k.json)
```

**Pass beyond exit codes:**

- M1 digests identical.
- `mapmove-gate`: the MC rules pass on ≥ 4 of 5 seeds, and **both negative controls FAIL**.
- The thresholds file's p10 values (from `--bot-profile cq`) are ≥ 2 × every floor of §13.4. If not, the rules or floors are amended before CQ2, never after.
- Doctrine band 6/6.
- Best-response worst cell ≤ the M1 control + 0.005.
- **M1's `RULESET_HASH` unchanged**; `RULESET_HASH_V2` pinned once (v1.1, R-16).
- The campaign-vs-lone coordination run ends with the campaign faction ≤ 22% of provinces on ≥ 8 of 10 seeds; if not, the integrator stops and the owner decides (OD-16) before Wave 2.
- **`codex/frontier` is not fast-forwarded** (§4.2).
- If `mapmove-gate` fails, Wave 2 does not start. The integrator and CQ1-B tune `keep_home_guard` (100 → 50), `keep_consolidate_bells` (288 → 144) and `free_city_min_ring` within §3.12's ranges by amendment, and re-run the gate.

**Gate CQ2:**

```sh
# everything in Gate CQ1 except the overnight lines, then:
cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings
cargo test --locked -p permutation-frontier --no-default-features
scripts/build-frontier.sh --twice
(cd permutation-frontier/svm-tests && ./run.sh --release -- g01_cq_ g02_cq_ g03_cq_ g11_cq_ g13_cq_ p_cq_)
(cd permutation-frontier/svm-tests && ./run.sh --release)                       # every M1 test still green on v2
(cd permutation-frontier/svm-tests && PSF_TRACE=1 ./run.sh --release -- g01_cq_resolve_worst g01_cq_skip_worst g01_cq_fold g01_cq_declare g01_cq_settle_capture --nocapture)
(cd frontier-node && cargo test --locked --release --workspace -- cq_)
```

Pass:

- ResolveFromInputs worst ≤ 290,000 CU (or the 300,000 amendment recorded with the clash budget restated);
- heap ≤ 28 KiB;
- every new kind within §5.4;
- P1–P13 green, P9 failing-first against M1's transit code (recorded in CQ2-C notes);
- `cq-regen.sh --check` green with the v2 generated outputs (the web reads both versions, §9.1);
- G11 byte-identical over 4,736 B;
- keeper, herald and bots `cq_*` green.

**Gate CQ3:**

```sh
# everything in Gate CQ2, then:
(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_cq_day g14_cq_ lag_gate crash_injection)
(S=$PWD/permutation-frontier/target/deploy-test-beacon/permutation_frontier.so; cd frontier-node && PSF_FRONTIER_SO=$S cargo test --locked --release -p keeper --test play -- --include-ignored)
(cd frontier-node && cargo test --locked --release -p verify -- --include-ignored tamper_)
(cd frontier-node && cargo build --locked --release --workspace && crates/verify/mutate.sh)
(cd permutation-gateway && npm test && node scripts/sync-web-sdk.mjs --check)
scripts/build-wasm.sh --check
S=frontier-node/target/release/frontier-stack
$S check-ports --config frontier-node/configs/cq-smoke.toml
$S up --mode accel --beacon test-key --scale 100 --days 2 --bots 100 --conquest --preset mc-test --run-id cq3-smoke --base-port 41600
$S verify --run-id cq3-smoke --herald-check && $S tamper --run-id cq3-smoke && $S report --run-id cq3-smoke
$S down --run-id cq3-smoke
```

Pass:

- `inproc_cq_day` (`MC_TEST`) shows every listed event with **zero stuck province-bells**, every transit settled exactly once, and every conquest record terminal or legitimately open at the end of the day;
- G14-CQ PASS, and its tamper FAILs;
- V1–V22 PASS on the fixtures (the M1 fixtures through the v1 path); T1–T24 and T25–T48 FAIL with their codes;
- every `mutate-*` build lets its tampers PASS;
- the smoke run's criterion-10 report is produced (not gating at 1 day), verify PASS, tamper all FAIL;
- `HANDOFF-UI.md` and its fixtures present;
- `npm test` green.

**Gate CQ4:**

```sh
(cd permutation-frontier/svm-tests && RELEASE_CHECK=1 ./run.sh --release)
scripts/build-frontier.sh --twice                                     # the MC release build record (CQ4-A notes)
scripts/cq-nightly.sh                                                  # base 41800, test key; three consecutive nights green
S=frontier-node/target/release/frontier-stack
$S check-ports --config frontier-node/configs/cq-rehearsal.toml
$S up --config frontier-node/configs/cq-rehearsal.toml                # base 41700, test key, 7 days, 1,000 bots, chaos, adversary, 5,000 viewers
$S verify --run-id cq-rehearsal --herald-check && $S tamper --run-id cq-rehearsal && $S report --run-id cq-rehearsal
$S down --run-id cq-rehearsal
```

Pass:

- G1–G13 green with no pending row;
- three green nightlies, each with criteria 11–13;
- **the rehearsal meets every criterion of §13.4 (1–13)** except the real-round environment row (not exit-grade);
- budgets regenerated; release hash recorded.

If criterion 10 fails on the rehearsal while the simulator passes, CQ4-B and CQ4-C diagnose the bots against criterion 7's per-day comparison. Fixes go to bots or keeper, never to the thresholds. A rules change is an amendment that re-runs Gate CQ1's simulator lines.

**Gate CQ5 = the MC exit:** §13 in full on the final commit (MC release `.so`, real rounds), then the exit report.

---

## 13. Exit test plan

Everything runs on the **MC release SBPF v2 `.so`** built by `scripts/build-frontier.sh` (hash recorded by CQ4-A), except trace builds for heap reports.

### 13.1 E1 — SBF budgets at adversarial fill (G1)

M1 §13.1's fills, plus:

| Instruction | Adversarial fill |
|---|---|
| ResolveFromInputs | M1's 1,240 fills **with the keep as a 13th garrison**; plus 12 sites each with an active siege, all completing at an hour boundary (snapshot), the keep taken in the same bell with **six 30,000-troop capturing hosts on the tile** (v1.1: the donor alone pays; the next bell's clash must validate), on top of the worst Phase B fill |
| SkipQuiet | 24 bells, 12 active records (occupations) + a keep contest with an empty garrison, 4 hour boundaries; prefix commit at 288 record-bells |
| DeclareSiege | the full check path **pinned (v1.1, R-08): Frontier-28, day 0 (≈ 4,000 bells left), the owner's vigil with a pending CL-09 change, `end_bell − now` = 287 (the scan's worst case) and also ≥ 288 (the O(1) path)**, 48 entries on the Province with 6 on the tile (lead-host ranking), Frontier protection proof, a capture target with a slot reservation and escrow top-up; plus a kernel bound test of ≤ 287 `covers()` calls |
| SettleCapture | holding capture with 4 transit bonds refunded and the rent swap; Free City capture with init (v1.1: no raze) |
| SettleSiege | stake to the defender plus a slot release with the escrow refund |
| RetireHost | the victim's session key, a host with a long return path |
| FoldMarch | 7 present members, first-fold init, 6 hours in one call, one lost hour |
| FileOutpost | 3 sites in 3 provinces with full cohorts and the anchor Holding's stores |
| OpenProvince | the worst of ring 2..10 × 6 wedges with a keep and a Free City |
| SettleTransit | a returning host of a captured Holding with `prev_home` |
| Harvest, Build, Train | M1's fills + the Province read |
| every kind | `g01_loaded_limit_*` against the release `.so` |

### 13.2 E2/E3 — Pre-funding and forgery (G2, G3)

- **Pre-funding:** 21 creation paths (M1's 19, plus MarchState and the Free City capture Holding). The outpost Holding uses SettleTicket's fresh path and is tested on its order-2 branch.
- **Re-creation after close:** MarchState (season status); a captured Holding's address (the generation and `capture_flags` stop a re-creation; M1's rules otherwise).
- **Forgery:** one test per new keyed read (§5.9), including a forged member Province in FoldMarch, a captor Citizen of the right faction and the wrong tag, a `prev_home` of another citizen, a forged nearby Province, and a forged anchor Holding in FileOutpost.

### 13.3 E4 — Property tests (LiteSVM, prefix `p_cq_`)

| Id | Property |
|---|---|
| P1 | Siege equivalence: the program's progress = a native replay of `Siege::advance` over 1,000 random rosters and vigils (incl. CL-09 changes and the horn snapshot) |
| P2 | No last-look (v1.1 rewritten so it tests what it claims): (a) for random sets of DeclareSieges by several citizens of the holding faction landing in one bell, every permutation of their order gives the same record (only the lead host's owner succeeds; the others get `NotLead`); (b) for two capture completions of one citizen in one bell, every order and delay of the two SettleCaptures, including one held across the captor's outpost SettleTicket, gives the same slots, owners, credit and immunity; (c) FoldMarch's timing within the ring changes nothing |
| P3 | Capture lock: every Holding-writing instruction between completion and SettleCapture refuses `CapturePending` |
| P4 | Lag gate: holding the Province, the anchors, the MarchState or the victim's Holding past completion leaves the final state byte-identical (folds: no lost hour within 5 hours) |
| P5 | Keep equivalence: the program's keep = a native replay of `keep::advance` over 1,000 random rosters, incl. pauses, breaks, consolidation and the garrison handoff; skip ≡ resolve |
| P6 | Every record terminates: a random walk over every instruction reaches kind 0 or a Holding for every record using only permissionless calls |
| P7 | The captor cannot command the victim's hosts; RetireHost sends them to `prev_home` only |
| P8 | Bonds go to the victim's funder |
| P9 | The transits of a captured Holding settle (fails on M1's code: failing-first) |
| P10 | Immunity only when broken by the defender; a deserted siege grants none and burns the stake; immunity and Respite bar only the recorded faction; an occupier that walks away gives no Respite |
| P11 | (v1.1) RetireHost: during the season only the victim's wallet or session succeeds (others `NotLead`); for any bell at which a third party could call it, the outcome equals not calling it |
| P12 | (v1.1) Site-state invariants S1–S6 (§3.15) hold after every step of P6's random walk, which now includes ReleaseDormant, SettleTicket, FileOutpost, capture completion and SettleSiege |
| P13 | (v1.1) Every reachable keep and Free City garrison is ≤ `MAX_HOST_TROOPS` and `clash::validate` accepts every input the program builds (random walks with up to six 30,000-troop hosts on keep tiles) |
| G7 | Lag invariance (M1), now with the conquest step |
| G11 | Skip ≡ resolve, byte-identical over 4,736 B, with records and a keep contest |
| G13 | One test per (new or changed instruction, error code); `RELEASE_CHECK=1` |
| G14-CQ | 100 bots × 2 game days through the program, native `conquest_model` every bell; verifier PASS; a tampered CONQUEST digest → FAIL |

### 13.4 E5 — The 7-day accelerated real-round conquest season (Mode A)

```sh
scripts/cq-run-exit.sh --expect-so-sha256 <MC release build record> --adversary
#  = frontier-stack up --config frontier-node/configs/cq-exit.toml
#    (up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 --conquest --run-id cq-exit
#        --base-port 41300 --chaos --adversary --viewers 5000 --viewer-window-hours 24; g0 1788998400)
frontier-node/target/release/frontier-stack verify --run-id cq-exit --herald-check
frontier-node/target/release/frontier-stack tamper --run-id cq-exit
frontier-node/target/release/frontier-stack report --run-id cq-exit
frontier-node/target/release/frontier-stack down --run-id cq-exit
```

**Environment (exit-grade):**

- the pinned MC release `.so` (deployed with its `--max-len`);
- `drand-replay` with the M1 archive of real quicknet rounds at **G0 1788998400**;
- the Season from `MC_LOCAL_7D` (Frontier-7);
- relay, keepers A and B (each ≥ 150 reveal payers, ≥ 32 delay payers, ≥ 4 funders, funded beneficiaries), the herald;
- **1,000 bots** with M1's 13 personas, the 19 conquest personas and the campaign layer;
- **the adversary schedule:** M1's §13.4 holds plus `contested-province`, `march-fold` and `capture-settle`; every hold must fire;
- **chaos:** `kill -9` a random component every 2–6 game hours, restart after 0–60 game seconds;
- 5,000 simulated viewers (4,000 polling, 1,000 WS subscribed to `control`) for 24 game hours;
- ports 41300–41399; 400-ms slots, 8 game seconds each;
- wall time ≈ 8.7 h plus pre-season.

**Pass criteria.** M1's 1–9 still pass, as written in M1 §13.4 (v1.13 with A1–A3), with these additions in bold:

1. The season reaches `end_bell` and EndSeason lands. Within 2 game hours after the end:
   - zero province-bells stuck;
   - zero transits unsettled or in state 1–2 older than `close + 2 bells`;
   - every ClashInputs closable after grace;
   - **every capture-due record settled, every lapsed siege's stake and reserved slot settled, every hour < `end_bell / 6` folded for every March; every province-bell resolved or skipped (no frozen province, R-01).**
2. Max CU per kind in play ≤ its §5.4 budget; the Reveal CU distribution reported, **and the ResolveFromInputs distribution for contested bells**.
3. Keeper latencies in slots at 20× as M1, **plus contested provinces: S → resolve p99 ≤ 8 slots** (no contested-province exclusion).
4. Liveness as M1 (no valid seal unrevealed outside above-cap hold windows; A2's `unrevealed_by_rule`).
5. M1's 13 personas as M1 v1.13.
6. Herald: file p99 ≤ 250 ms, ingest → WS p99 ≤ 2 s, error rate < 0.1% at 5,000 viewers for 24 game hours (A3), **with the `control` WS subscription in the load**.
7. Bots vs `frontier-sim --rules mc --policy campaign --bot-profile cq` per day: Departs, keep marches and captures, sieges declared, completed and failed, occupations, captures, lasting changes; within an order of magnitude. Reported, not gating. **v1.1 (R-26): computed by CQ3-B's report (M1 never computed it: "n.a." in every M1 run); the bot-activity gate of §8.8 is its gating cousin in the nightlies and the rehearsal.**
8. Bad seals as M1.
9. Tickets as M1, **outpost cohorts included**; `ticket_holder` never keeps a site it did not win.

**Criterion 10 — the faction map moved (gating).**

Every figure is computed from the herald's `PSFCT1` series with `control::lasting_changes` and `march_banner` (the verifier recomputes the series, V21). The same code is printed by `frontier-sim mapmove`.

*Population:*

- **P** = provinces of ring ≥ 2 outside every heartland, opened by bell 144 (the end of game day 1).
- **P₂** = those opened by bell 288.

*Definitions:*

- **Lasting change:** province p has control f at bell b and g ≠ f at b − 1, both factions, and keeps f for bells b … b + 5 (or to `end_bell − 1`).
- **Game day d** covers bells `144(d − 1)` … `144d − 1`.

| # | Metric | Threshold (v1.1, R-25): **every row must reach its fixed floor**; `½ × p10` from `thresholds/mc-7d-1k.json` is reported beside it, not gated |
|---|---|---|
| 10a | lasting changes in the season | floor **60** |
| 10b | game days 2–7, each with ≥ 1 lasting change | **6 of 6** (fixed) |
| 10c | share of P with ≥ 2 distinct controllers during the season | floor **15%** |
| 10d | March banner changes (faction to a different faction, through any contested interval) in the season | floor **6**, and ≥ 10% of Marches with ≥ 2 banners |
| 10e | net movement: share of P₂ whose control at `end_bell − 1` differs from bell 287 | floor **10%**, confirmed or amended at Gate CQ1 from CQ1-B's 7-day net-movement measurement (R-12; the balance lab never measured it at 7 days) |
| 10f | breadth: factions with ≥ 1 lasting gain and ≥ 1 lasting loss | **≥ 4 of 6** (fixed) |
| 10g | balance at `end_bell − 1`: the largest faction's share of controlled provinces / the smallest's | **≤ 30% / ≥ 8%** (fixed) |
| 10h | the holding contest was played: sieges declared / completed / failed (any cause); occupations; liberations; captures (holdings 2–3 and Free Cities); outposts founded | floors **20 / 10 / 3; 3; 1; 5; 10** |
| 10i | D9: first holdings that changed owner | **0** (V16) |

*Negative controls* (the decider's tests, CQ3-B; they MUST FAIL criterion 10):

- a copy of the M1 exit season's overview series converted to a control series by wedge, which is the static map the owner saw;
- a synthetic series in which every change reverts within 5 bells (flicker);
- a series that moves only from heartland provinces (outside P).

*Reference [sim, balance lab, `out/cand.md`, g100/sh288, 8 seeds]:*

| Population, 1k agents × 7 days | province changes a day | provinces ever changed | March changes a day | days without a March change | largest faction at the end |
|---|---|---|---|---|---|
| human mix | 27.8 | 36% | 8.2 | 22% | 18.0% (worst seed 18.6%) |
| 99% simulated bots (`Arch::Bot`) | 42.6 | 47% | 10.1 | 3% | 18.0% (19.4%) |

**v1.1 (R-11, R-25):** the bot row assumes bots that play 24 sessions a day with a keep roll in each, one to two orders of magnitude more active than M1's stack bots (≈ 0.24 Departs per bot-day). The human-mix row is the conservative reference, and it is already close to the CI gate (22% of days without a March change against a 30% limit). The floors sit about 2× below the thresholds file's p10, not "4–6× below the simulator", and the p10 now comes from `--bot-profile cq` (the stack's real cadence). A pass means the stack's bots play the contest at the cadence the simulator assumed; the bot-activity gate (§8.8) checks that cadence directly.

**Criterion 11 — conquest correctness (gating):** `verify --herald-check` PASS, including V14–V22, with `HeraldControlMismatch` = 0 and `DoubleCaptureStranded` reported.

**Criterion 12 — horn and settle latency (gating):**

| Measure | Target |
|---|---|
| SIEGE_DECLARED and KEEP_CONTEST → WS `alert` | p99 ≤ 2 s |
| capture-due → SettleCapture landed | p99 ≤ 12 slots, **excluding captures whose accounts were inside an adversary hold window** (v1.1, R-24; held cases reported separately with their delay) |
| FoldMarch lag | p99 ≤ **5 game hours** (v1.1, R-23: the keeper's idle cadence is 4 h, so 2 h could not pass), and **zero lost hours outside the `march-fold` hold window** (the real gate) |
| contested resolves (criterion 3's rule) | as criterion 3, **excluding the `contested-province` hold window**, reported separately |

**Criterion 13 — conquest personas (gating):** every conquest persona observed or exercised with its expected outcome (as criterion 5's v1.13 rule). **v1.1 (R-24), honest refusals by ratio:** honest DeclareSiege and FileOutpost refusals by rule are **≤ 2% of honest attempts**, counting only refusals that the bot's own `may_besiege` / lead-host / `may_found_outpost` check on the state it held at send time should have caught. Designed races never count: `NotResident` within the keeper's lag, `NotLead` across a bell boundary, `Immune` / `TooLate` / `SiegeBusy` drift between the planning epoch and the send, and outpost share, cohort or land-gate drift. (M1 failed G14 and `inproc_day` on exactly such a zero-tolerance rule.)

**Report additions:**

- `mapmove.json` and `mapmove.svg`;
- per-faction gains and losses;
- the 10 most contested provinces;
- resolves per contested province-day;
- refused declarations per (persona, code), and the designed-race refusals per code;
- bot activity per bot-day against the `--bot-profile cq` rates;
- the keeper's extra cost against the M1 exit (§8.2's estimate checked).

### 13.5 E6 — Verifier

`frontier-verify` over the exit season's RPC and archive:

- **PASS** in < 10 min on 8 threads;
- the tamper suite on a copy of the run: **every required class FAILS** with its code: M1's 23 (T1–T22, T24) **and T25–T48 (24)**, so **47 required**; the extras (T1b, T6b, T23, T23b, H1, H1b, V9a) are judged too;
- each `mutate-<check>` build lets its own tampers PASS;
- the honest-but-adverse fixtures PASS.

### 13.6 E7 — Web

- `(cd permutation-gateway && npm test)`: every `web-frontier-*` test incl. `-cq-*`, v9 web tests unchanged, `web-lang` completeness, `web-sdk` freshness, the `web-frontier-wasm` hash.
- `scripts/cq-ownership-check.sh` green.
- The spectator page on the exit stack shows the control fill moving **in vector mode (`?art=0`)**. This is checked by script, with no design judgment: at a bell where `PSFCT1` flags 1, the rendered province's `mapFaction` and the fill colour `provinceFill` returns both change to the new holder (v1.1, R-17: v1.0 checked only the state slice). Art mode (the default) is checked the same way **only after the design chat ships the `sprites.mjs` switch**; until then the demo's moving map is vector mode and `mapmove.svg`, and the exit notes say so.
- The design chat's screenshot matrix is theirs, not an MC gate.

### 13.7 E8 — Measurements owed

- The ResolveFromInputs worst and in-play distribution with the keep;
- the extra keeper transactions per contested province-day and per season (§8.2);
- the fold lag distribution;
- the PSFCT1 size per bell;
- the release `.so` size and `--max-len`.

These go to DESIGN rev 3.2 (CQ5-C) and to the M2 capacity model.

### 13.8 After the exit

Only with the owner's explicit approval: a private devnet conquest playtest (50–200 people, no money, `MC_SEASON_28` or Frontier-7). Every devnet step and every push needs its own approval.

---

## 14. Estimate (honest)

| Wave | Units | Unit engineer-weeks [estimate] | Calendar, human team (6 + integrator) | Main uncertainty |
|---|---|---|---|---|
| CQ1 rules, sim, ABI, formats | 4 | 8.7 (A 2.8, B 3.2, C 1.7, D 1.0) | 1.5 wk + overnight | the K-01 package re-measured with the v1.1 rules and a real-cadence bot profile; a failed `mapmove-gate` forces a tuning amendment (+1–3 days) |
| CQ2 program, keeper, herald, bots | 6 | 14.8 (A 2.6, B 2.1, C 3.3, D 2.3, E 2.0, F 2.5) | 2 wk | the RFI margin (est. 284k of 290k); the `prev_gen` transit path; slots and reservations |
| CQ3 verifier, stack, relay, web, itest | 5 | 10.0 (A 3.2, B 2.3, C 1.0, D 2.0, E 1.5) | 1.5 wk | the first in-process conquest day |
| CQ4 hardening and the rehearsal | 5 | 6.5 (A 2.0, B 1.5, C 1.5, D 1.0, E 0.5) | 1.5 wk | **whether the stack's bots move the map as the simulator's do** |
| CQ5 exit | 3 | 2.5 | 1 wk | re-runs after late fixes (≈ 9 h each) |
| Integrator (5 windows, manifests, gates, staging, regeneration) | 1 | 3.5 | throughout | gate wall time |
| **Total** | 23 + integrator | **≈ 46** | **≈ 8 weeks nominal; 50% by 8.5, 80% by 10.5** | |

v1.1 delta (+3.5 ew): slots and reservations (+0.6), lead host (+0.2), capture credit (+0.3), faction-scoped immunity and Respite (+0.2), V22 and ten tampers (+0.7), version-dispatching readers (+0.6), the extra simulator measurements (+0.7), staging and regeneration (+0.5), web self-feed and hash table (+0.2); minus the raze path and released Free Cities (−0.5).

**At the pace M1 actually ran:**

- M1's 62.5-engineer-week plan ran from the contract (2026-09-27 18:27) to the close (2026-10-01) in about **3.5 days** with parallel agents [measured: `git log`].
- MC is ≈ 68% of that size, plus the same fixed wall times:
  - each 7-day accelerated season ≈ 8.7 h; plan one rehearsal and two exit attempts;
  - overnight doctrine runs;
  - owner-decision latency.
- **Estimate with agents: ≈ 5–8 calendar days; 50% by ≈ 6.5 days, 80% by ≈ 9.5 days** [estimate].
- **The largest risk:** criterion 10 passes in the simulator and fails on the stack. That would be a bot-behaviour gap, as M1's 0.24 Departs per bot per day showed. v1.1 attacks it three ways: thresholds from a simulator profile at the stack bots' real cadence, a bot-activity gate in every nightly, and criterion 7's per-day comparison built for diagnosis. A failure still costs one diagnosis loop of 1–3 days.

**Gate wall time:**

| Gate | Time |
|---|---|
| CQ1 | ≈ 3 h + two overnight lines |
| CQ2 | ≈ 4 h |
| CQ3 | ≈ 5 h (incl. mutate builds and the smoke) |
| CQ4 | three nights + ≈ 9 h rehearsal |
| CQ5 | ≈ 9 h per exit attempt |

**What would cut time, in this order:**

| Cut | Saving | Note |
|---|---|---|
| RetireHost (state the harsh rule instead: the victim's hosts of a captured holding are lost) | −1.0 ew | |
| genesis Free Cities (`free_city_min_ring = 0`; captures then come only from released Free Cities and outposts) | −0.5 ew | needs a CQ1 re-measure |
| the `PSFOV2` overview (control layer only) | −0.5 ew | |
| Herald's Call | −0.3 ew | |

**What must not be cut:** the keep, criterion 10 and its negative controls, the verifier's conquest checks and tampers, the lag and capture-lock properties, P9.

---

## 15. Owner decisions (working defaults in force until decided)

| # | Question | Working default (implementers build to it) | Blocks |
|---|---|---|---|
| OD-1 | **The faction map is coloured by keeps** (one fort per province, held by a faction, owned by no wallet, taken by holding its hex 12 h), instead of by holdings | yes (K-01) | CQ1-A |
| OD-2 | D9: the first holding is never taken (occupied at most) | keep (K-02) | CQ1-A |
| OD-3 | Keeps give the map only: no Dominion, goods or bonus. Dominion stays DESIGN §5.6 from holdings, game points, unofficial | yes (K-03); revisit with a doctrine re-tune | CQ1-A, CQ2-C |
| OD-4 | Holdings 2–3 by outpost tickets at the front (faction < 50% of the province), sealed Settler marches in M3 | yes (K-07) | CQ1-A, CQ2-A |
| OD-5 | Genesis Free Cities: one neutral town per province from ring 4; released dormant homes become plain free sites as in M1, not Free Cities (v1.1) | yes (K-08) | CQ1-A |
| OD-6 | Occupation: Dominion points only (the map colour does not change); 12 h (7-day) / 48 h (28-day) at most; then equal Respite **against the occupier's faction only, and only after expiry or the owner's own liberation** (v1.1); no tribute; no laurels in MC | yes (K-09, K-12, K-16) | CQ1-A |
| OD-7 | Heartlands (rings 2–3 of the own wedge): no sieges and no keep contests until M3; all factions in Rivalry | yes (K-18; `heartland_max_ring` can drop to 2) | CQ1-A |
| OD-8 | Frontier-7 timers for the exit and the demo (24-h shield, **3 days dormant, no release within the 7-day season** (v1.1, R-14; v1.0 had 30 h / 60 h), 36 h Frontier protection) | yes (K-17) | CQ1-C |
| OD-9 | Siege stake 500 Gold; immunity only when the defender broke the siege, **against the besieging faction only** (v1.1); ≤ 2 sieges a day per citizen; **only the hex's largest host may sound the horn** (v1.1, K-24) | yes (K-05, K-06, K-20, K-24) | CQ1-A |
| OD-10 | A captured holding loses its garrison and trained troops; the victim's hosts keep fighting until **the victim** sends them home (v1.1, K-27); a capture lands in a slot reserved at the horn, so there is no raze (K-25); a quick recapture (inside 1 day / 2 days) earns no points and moves no stores (K-26); the refugee kit waits for M2 | yes | CQ2-C |
| OD-11 | Criterion 10 ("the map moved") gates the exit, with §13.4's floors | yes | CQ3-B |
| OD-12 | The Province account grows to 4,736 B (+0.00325 SOL refundable per province; +40.6 SOL float at full map) | yes (K-22) | CQ1-C |
| OD-13 | The replay page (`frontier/demo-replay`) data for the English demo: hand-off only, or an add-on unit in wave 4 | hand-off only | none |
| OD-14 | (v1.1, R-10) **Should keeps score?** Humans have no built-in reason to take keeps yet. Option: March-banner hours at a small Dominion weight (`bannerdom`), measured through the doctrine gate in CQ1 | no (display-only recognition: per-player keep standings and titles); revisit with CQ1's numbers | none (CQ1-B measures either way) |
| OD-15 | (v1.1, R-12) Should the bots use a held keep as a forward base (the chain already allows it)? | no, until CQ1-B's `--forward` rows show no snowball | CQ2-F |
| OD-16 | (v1.1, R-13) If a coordinated (campaign) faction ends above 22% of provinces against lone factions in CQ1, what then: accept, or add a limit (e.g. the arrival cap per faction per province-bell lowered at fronts)? | stop and ask before Wave 2 | Wave 2 |

Every OD here is a game rule visible to players, except OD-12 (cost) and OD-13 (scope). The join-page text (CQ4-E) states OD-2, OD-6, OD-7, OD-8 (how a home can still be lost: dormancy release in long seasons), OD-9 and OD-10 in plain words.

---

## 16. Amendments, sources and links

**Amendments:**

- Only the integrator changes this file. Each change bumps the version line, lists the sections changed and is copied into `DECISIONS.md` part CQ if it resolves a conflict.
- Layout, tag, error-code and log-kind changes after Gate CQ1 require an amendment and regenerated vectors in the same merge.
- An outcome-changing kernel change follows §3.13.

**This contract:**

- `scratchpad/frontier/conquest/CONQUEST-CONTRACT.md`;
- committed by CQ1-D as `docs/frontier/conquest/CONQUEST-CONTRACT.md`;
- the owner's summary: `scratchpad/frontier/conquest/SUMMARY.ja.md`.

**Area designs (informative):** `scratchpad/frontier/conquest/design/{rules,program,offchain,sim-balance,game-design}.md`.

**Labs:**

- `scratchpad/frontier/conquest/lab/simbal/out/{cand,main,sens,snowball,doct_R,doct_Rdom,criterion_R}.md` (the keep model);
- `lab/rules-mapmove/out/*` (holding-weight control, Free Cities, outposts);
- `lab/offchain-mapmove/out/*.md` (field control, base rules);
- `lab/program-conquest/results.txt` (CU).

**Repo (read-only during planning), at `CQ0` = `d11d058`:**

- `docs/frontier/DESIGN.md` (rev 3.1: §3, §4, §5.4–§5.6, §6, §6.6, §8);
- `docs/frontier/m1/M1-CONTRACT.md` (v1.13);
- `docs/frontier/m1/M1-EXIT-NOTES.md`;
- `docs/frontier/DECISIONS.md`;
- the kernels in `permutation-rules/src/frontier/`;
- the program in `permutation-frontier/src/proc/`;
- `frontier-node/configs/m1-exit.toml`.

---

## 17. Revision notes (v1.0 → v1.1, 2026-10-01)

Each review issue was checked against the repo at `CQ0` (`d11d058`), the `frontier/ui-shell` branch and the labs. Verdicts: **accepted** (the issue is real and the contract changed), **accepted, worse than stated** (real, and the check found more), **partly rebutted** (part of the claim is false; the evidence is given). Nothing in the repo was changed.

| # | Severity | Issue | Verdict and evidence | What changed |
|---|---|---|---|---|
| R-01 | blocker | A keep garrison above 30,000 troops freezes a province | **Accepted.** `clash::validate` refuses any garrison above `MAX_HOST_TROOPS` (`clash.rs` 878, `host.rs` 27: 30,000 troops); `HEX_HOST_CAP` = 6 (`host.rs` 38), so v1.0's Σ-of-every-host rule could reach 90,000 and fail every later resolve of the province (and with it DeclareSiege, the hour snapshots, FoldMarch and the herald's PSFCT1 completeness). The simulator takes 50% of a single host and sends it home (`lab/simbal/frontier-sim/src/sim.rs` 3811–3830), so it never passes 15,000. | K-19 and §3.2 step 5 now follow the simulator exactly (one donor, 50%, the donor returns home, a remainder below `MIN_HOST_TROOPS` joins the keep); the cap is asserted by `keep::open/garrison/advance`; CreateSeason bounds the guards; `cq_keep_states_validate`, P13, a six-30,000-host G1 fill and T45; criterion 1 adds "no frozen province". Because the rule now equals the measured one, the K-19 numbers stand. |
| R-02 | major | RetireHost lets anyone time a victim's defenders away | **Accepted.** v1.0's N-class call took effect after the current bell's clash at any bell the caller chose, so it picked outcomes. | K-27: only the victim (wallet or session) may retire during the season; anyone after `end_bell`; a never-retired host keeps fighting. Class P during the season, `NotLead` for others, P11, T42, persona `retire_foreign`; the keeper no longer retires hosts during the season. |
| R-03 | major | DeclareSiege is a first-come race | **Accepted.** Under Rivalry only one faction holds a hex after the clash, so the race is among citizens of that faction, and the first transaction took the capture rights. | K-24, the lead-host rule: only the owner of the hex's largest non-civilian host may declare (`NotLead`, 78). Rosters change only at a bell's settle, so exactly one wallet qualifies at any time. P2 rewritten to test permutations; T46; persona `lead_racer`. |
| R-04 | major | Capture or raze depends on settle timing and order | **Accepted, worse than stated.** Besides the raze flip, v1.0 set `order := holdings_n + 1` at the settle, and order sets the Dominion weight (`laurel::order_quarters`: 4 / 2 / 1 quarters), so settle order also chose the weight; immunity counted from the settle. | K-25: slots 1–3, the lowest free slot reserved at the horn with one `rent(1,280)` escrowed per reservation, counted by FileOutpost, released by SettleSiege on failure or lapse; a capture always lands in its reserved slot; **raze removed**; immunity counted from the completion bell. P2(b), the two-completions fixture, the `capture-settle` hold now spans an outpost SettleTicket. |
| R-05 | major | Respite and SiegeBusy farming by a friendly faction or an alt | **Accepted, worse than stated.** K-06 also let an alt besiege and the owner's own host "break" it for 36 bells of immunity against everyone plus the stake. | Respite only on expiry or the owner's own liberation; immunity and Respite bar **only the recorded faction** (`barred_faction`); post-capture immunity stays global (it needs a completed siege). An alt's siege or occupation still holds the site's single record, but any other faction can end it by taking the hex and declare at once. P10 extended, T44, persona `respite_farmer`. |
| R-06 | major | Sybil capture ping-pong farms Dominion | **Accepted.** v1.0 §1's reason for deferring pair rules ("Dominion does not read them") was wrong: each capture adds 36 control-bells. | K-26: a capture credits `captures_by` (Dominion, standings) and keeps the stores only if the victim held the holding ≥ `capture_credit_min_bells` (144 / 288), from a new `held_since_hour` in the site mirror. The cross-season wallet-pair rules stay in M2; §1 corrected. T47, persona `pingpong_pair`. Released homes no longer become capturable Free Cities (R-07), which also removes the alt-dormancy loot path. |
| R-07 | major | A site's state can change under a live record | **Accepted.** (a) Free City expiry under a siege; (b) ReleaseDormant on a besieged or occupied home; (c) raze leaving the Holding behind a free site; (d) kind-0 bits surviving an owner change. | (a) removed: released homes become plain free sites (M1), genesis Free Cities never expire. (b) ReleaseDormant refuses while a record is live or owes anything, and zeroes the record. (c) removed with raze. (d) `owed_gen` on stakes, record resets on every owner change. New §3.15 (S1–S7), P12, V22's `SiteStateBroken`. |
| R-08 | major | DeclareSiege's 30k CU has no basis for the vigil arithmetic | **Accepted.** `bells_outside_vigil` (`siege.rs` 355–405) does up to 144 + 143 `covers()` calls per segment, and a 3-entry schedule gives up to ~10 segments; `advance_quiet` binary-searches over it (`siege.rs` 508–520); the lab's 6,664 CU did not record its horizon or vigil. | `can_complete_before` is O(1) when ≥ 288 bells remain (a pinned kernel bound) and otherwise a forward scan of ≤ 287 calls; `earliest_completion_bell` leaves the chain (herald and WASM compute the ETA; SIEGE_DECLARED carries the lead host id instead). G1's DeclareSiege fill is pinned (Frontier-28 day 0, a pending change, both paths); a fallback to closed-form arithmetic if G1 still exceeds 30,000. |
| R-09 | major | Verifier gaps | **Accepted, all six.** (6) confirmed: the field step keeps one hostile-free set (`clash.rs` 1466–1471), so a two-faction hold cannot happen under Rivalry. | V22 (outposts, Free Cities, keep placement, S1–S6); capture lock, stake routing, credit and RetireHost legality in V16, V17 and V19; V21 covers sieges, standings, players and call files; T39–T48 (47 required classes); keep pause marked M3-only (code 10 and flags bit 1 reserved); the fixture replaced. |
| R-10 | blocker | Real players have no reason to take keeps | **Partly rebutted, mostly accepted.** True: a keep pays nothing, and the simulator's keep attacks are a per-session dice roll (`sim.rs` 1536–1539) with a value/defence choice of target but no value test of attacking at all. Partly false that movement "measures obedience" only: halving keep interest (`kaggr` 0.5, `out/k1_1k28.md`) moved March changes 3.0 → 2.9 a day and provinces ever changed 42% → 38%. A Dominion payoff is not adopted by default because keep Dominion broke the doctrine band (1 of 6, `doct_Rdom.md`). | Display-only recognition (per-player keep standings, `players.json`, Chronicle titles); `keep_aggr` 0.25 / 0.5 / 1.0 rows in CQ1; the `bannerdom` variant through the doctrine gate (OD-14); the plain statement in §0, §3.2 and the summary. Severity kept as a known product risk, not a build blocker. |
| R-11 | major | The 1k-bot reference assumes far more activity than the stack bots showed | **Accepted.** `Arch::Bot` = 24 sessions a day, aggression 0.5 (`model.rs` 139–148); M1's stack bots made ≈ 0.24 Departs per bot-day (M1-EXIT-NOTES). | `--bot-profile cq` (the planner's cadence) and `m1` (0.24) in CQ1; thresholds derived from `cq`; a stack-side bot-activity gate in the nightlies and the rehearsal; the summary uses the human-mix row (≈ 28 provinces and ≈ 8 March changes a day). |
| R-12 | major | Borders move but the balance of power never does | **Partly rebutted.** Confirmed by the data: leader 16.5–18%, 38–55% re-taken within 3 days, 76% of provinces never change at 10k/28d, 0.0% "vs 7 d before" in every 7-day row, a guild gains nothing. That compression is the intended no-snowball property, not a defect. **False:** "captured keeps cannot be used as a march base": M1's Depart looks the host up in the Province the call names (`permutation-frontier/src/proc/host.rs` Depart step 3; M1-CONTRACT §5.11 step 1, "province = the host's"), so a host on a taken keep can march on. Only the simulator lacks it. | Expectations stated plainly (§0, summary); a 7-day net-movement metric in CQ1 for criterion 10e; `--forward` measured; OD-15 decides whether bots use forward staging. |
| R-13 | major | Snowball resistance measured without coordination | **Accepted.** `sim-balance.md` §6.1 says so itself. | CQ1 runs the 3:1 size stress under `campaign` and a one-campaign-faction run; a stop rule at 22% (OD-16); the summary caveat. |
| R-14 | major | The summary says the first holding is never lost | **Accepted, worse than stated.** v1.0's Frontier-7 dormancy (30 h / 60 h) was harsher than M1's own 7-day preset (`frontier-abi/src/presets.rs` 391–392: 5 days / 10 days), and a released home became capturable loot. | Frontier-7: dormant after 3 days, release after 7 days (never within the season); released homes become plain free sites in both presets; the summary states the one exception (Frontier-28, 10 days without action). The casual-profile home-loss count is reported by CQ1-B. |
| R-15 | major | Two different "controls" per March | **Accepted.** | The Dominion controller is renamed *Dominion lead*, is never a map colour (PSFCT1 fields renamed `points_lead`), and the hand-off says so; the summary says "occupation moves only points; map colour and point ranking can disagree". |
| R-16 | blocker | Gates CQ1 and CQ2 cannot go green | **Accepted, worse than stated.** Confirmed: fclient's twin tests (`frontier-node/crates/fclient/src/abi.rs` 1113, 1141; `PROVINCE = 4_096` at 238). Also: `MAX_GARRISONS` feeds `ruleset_hash_input` (`permutation-rules/src/frontier/mod.rs` 73), the camp rule in `frontier-abi/src/clash_model.rs` 666 and 1157, and `layout/province.rs` 50 asserts it equals `SITES_N`, so v1.0's 12 → 13 would have changed M1's hash and camp outcomes. Correction: `permutation-frontier/src/proc/clash.rs` names it only in a comment. | Staged ABI (§5.1): Wave 1 additive (v2 under new names, `MAX_GARRISONS` stays 12, `MAX_GARRISONS_WITH_KEEP`, `is_heartland_in`); CQ1-A owns every `permutation-rules/tests/frontier_*.rs`; the integrator regenerates generated outputs in the same merge; Wave 2 switches the node crates (CQ2-D owns the twin tests); `codex/frontier` is not fast-forwarded until the exit. |
| R-17 | major | The live map will not visibly move | **Accepted.** `layers.mjs` 73–76 and `sprites.mjs` 203–206 colour from `majorityOwner`; art is on by default (`app.mjs` 31); `fmap.mjs` calls `paintProvince` and `paintStrategic`. | `fcontrol.mapFaction`; `layers.mjs` fill and sigil switched (vector path); an explicit request to the design chat for `sprites.mjs`; E7 checks the rendered faction in vector mode, and art mode only after the design chat ships; the demo's moving map until then is `?art=0` and `mapmove.svg`. |
| R-18 | major | `inproc_cq_day` cannot show its events under Frontier-7 | **Accepted.** | `MC_TEST` preset (§3.12); `inproc_cq_day`, G14-CQ, the nightlies and the smoke run on it with pinned events. |
| R-19 | major | The design chat's footprint is larger than §4.5 lists | **Accepted, worse than stated.** Eight `ui-shell` commits after `CQ0` (`d4a3448`…`246b1fd`) also touch `controller.mjs`, `fstate.mjs`, `fui.mjs`, `onboarding.mjs`, `screens/*` and `web-frontier-march.test.mjs`, besides the herald and gateway files named. | §4.5 extended; MC stops editing `controller.mjs` / `fstate.mjs` (new `fcqstate.mjs`); herald hooks only; ownership check by first-parent history from each branch's fork point, ignoring merged `codex/frontier` commits. |
| R-20 | major | The ruleset pin breaks the design chat's live preview | **Accepted.** `fchainio.mjs` 36–39 `setPin` and `app.mjs` 286 `seasonKernel`. | `RULESET_HASHES {m1, mc}`; spectate accepts both; the v2 WASM carries both rule sets and reports the pinned season's hash; `codex/frontier` keeps v1 until the exit; a fixture herald on 41900. |
| R-21 | major | Shared models frozen in Wave 1, first used in Wave 2 | **Accepted.** | `clash_model` v2 in CQ1-C's brief; `frontier-abi/src/{conquest_model,clash_model}.rs` owned by CQ2-B in Wave 2 and CQ3-E in Wave 3 (layout frozen; vectors in the same merge). |
| R-22 | major | The verifier's M1 fixtures are v1 | **Accepted.** `frontier-node/fixtures/verify/` is 15 MB of v1 recordings used by `tests/{fixtures,camp_clear,tamper}.rs`. | Version-dispatching readers in fclient, herald and verifier; the M1 fixtures stay as v1 regression inputs, never re-recorded (§5.1, CQ3-A). |
| R-23 | major | FoldMarch lag p99 ≤ 2 h conflicts with 4-hour idle skips | **Accepted.** | Target ≤ 5 game hours, zero lost hours as the real gate, plus fold-driven skips (≤ +33% idle skips). |
| R-24 | major | Latency and persona criteria fail on designed holds and races | **Accepted** (two review issues: SettleCapture p99 vs the `capture-settle` hold; criterion 13's zero tolerance). | Hold windows excluded from every latency criterion and reported apart; criterion 13 becomes ≤ 2% of honest attempts, counting only refusals the bot's own send-time check should have caught, with designed race codes listed. |
| R-25 | major | Exit thresholds "4–6× below the simulator" | **Accepted.** `max(floor, ½ × p10)` sits ≈ 2× below the simulator. | Gate on the fixed floors, report `½ × p10`; the reference table shows the human-mix and bot rows; p10 from `--bot-profile cq`. |
| R-26 | major | No diagnosis tool; 10e without evidence | **Accepted.** Criterion 7 was "n.a." in every M1 run (M1-EXIT-NOTES E5). | Criterion 7's per-day comparison in CQ3-B's brief and the report; 10e measured by CQ1-B (R-12) before the floors freeze. |

**Net effect on scope:** raze and released Free Cities leave MC; slots, the lead host, faction-scoped immunity, capture credit and the victim-only RetireHost enter. ABI numbers: errors 62–78, tamper classes T25–T48, checks V14–V22, properties P1–P13. Estimate ≈ 46 engineer-weeks (from 42.5). New owner decisions OD-14…OD-16; OD-5, OD-6, OD-8, OD-9 and OD-10 reworded.
