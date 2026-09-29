# PERMUTATION STATE: The Sixfold Frontier (open-world design, revision 3.1)

One civilization, six factions, one shared hex map that grows as people arrive. Anyone can join, at any time until late in the season, in any number up to the season's stated map capacity.

- **Status:** **revision 3.1**, 2026-09-27. It applies the second M0 pass (`scratchpad/frontier/m0b/`: the redone spikes SP-V2 and SP-FEE, and the K3 simulator work `sim/ECONOMY.md`, `sim/DOCTRINES.md`), the K3 integration on `codex/frontier` (`360bc85`), and the 22 confirmed review issues of revision 3 with the fixes measured in `scratchpad/frontier/m0c/` (§0.6, **§20 maps every issue to its fix**). Revision 3 is kept as `open-world-design.rev3.md`. Revision 3 itself: revision 3, 2026-09-27. It applies the owner's decisions O1–O10 of 2026-09-27, every design change in the M0 first-pass report (`scratchpad/frontier/m0/M0-REPORT.md` §4, items 1–30 and §4.8), every spike `design_impact` and every confirmed review issue in `scratchpad/frontier/m0/result.json`. **§19 maps each of those inputs to the section that now carries it.** Revision 2 is kept as `open-world-design.rev2.md`, revision 1 as `open-world-design.rev1.md`; §18 (the revision-2 notes) is unchanged.
- **Owner decisions in force (2026-09-27):** Season 1 runs on Solana base only, with no Skirmish, no Arena and no ER lane (O8; X1 stays a later extension); the v9 security round is stopped; randomness and sealed orders use drand **quicknet** only, verified on chain, with the program built for **SBPF v2** (O1); the genesis seed comes from drand alone, and MagicBlock VRF is no longer used anywhere in the Frontier (O2). O3–O10 are applied in §4–§6 and §8 and listed in §14. **Standing rule:** every devnet step and any push needs the owner's separate approval.
- **Code (revision 3.1):** `codex/frontier` carries the K3 kernels (`frontier::doctrine`, `frontier::mandate` with the m0c share floor, `payout::office_ceiling_bps`) and the m0c simulator additions (`frontier-sim criterion --best-response`, `frontier-sim c4`, the doctrine gate's negative controls and the scheduled 1,500-season workflow `doctrine-balance.yml`) at **`bcc6382`** (on top of the K3 merges `86a466b`, `360bc85`); local commits only, nothing pushed, so GitHub CI has not run. **Revision 3's code line:** rules v10 kernels exist on branch `codex/frontier` at `cea89be` (worktree `.claude/worktrees/frontier-integ`): `permutation-rules/src/frontier/` (world, clash, siege, terrain, travel, pools, laurels, index, payout) and the host simulator `frontier-sim/`. v9 is untouched (`params::RULES_VERSION` is still 9; the v9 golden replays pass). Local commits only; nothing was pushed, so GitHub CI has not run. This revision changed no code.
- **Evidence for this revision:** the M0 spikes in `scratchpad/frontier/m0/spikes/` (S-BEACON, S-FEE, S-TLOCK/S-TLOCK-V, S-SIZE+S-JOIN, each with the reviewers' corrections prepended), the simulator results `scratchpad/frontier/m0/sim/RESULTS.md`, and a small revision-3 cost model `scratchpad/openworld/lab/rev3/rev3.py` → `rev3-results.txt` (fee market at the 40M per-account cap, restated C4, defence pool, base load with the M0 budgets). Earlier labs: `lab/rev2/rev2.py` → `rev2-results.txt` (§A3 lock costs are **superseded** by `rev3-results.txt`), `lab/synth/model.py` → `model-results.txt` (growth, rent, travel; still valid), `lab/redteam/rt.py`, and the E1–E4 labs.
- **M1 wave-1 amendments (2026-09-27, unit W1-D of the M1 integration contract v1.1, `m1/M1-CONTRACT.md`):** the text-only parts of the M0 closeout (CL-19…CL-27, CL-31, CL-37), the ticket-cohort and onboarding text (I-47, I-33, O-M1-13) and the measured choices of the first M1 week are applied in place, each marked **(M1, CL-xx)**, and listed in **§21**. Where this document and the M1 contract differ on anything M1 builds, the contract wins; the rest of the M1 decisions (ProveBadSeal removed, the lock table, cohorts in full) reach this text in wave 6 (unit W6-E). Owner decisions after revision 3.1 are logged in **`DECISIONS.md`**.
- **M1 wave-6 amendments (2026-09-28, unit W6-E, M1 contract v1.9):** the M1 decisions that change this design are applied in place, each marked **(M1, I-xx)**, and listed in **§22**: settlement is the seal proof (ProveBadSeal and SealVerdict removed, §6.2), the quiet-bell proof (ArrivalDay + SkipQuiet, §6.3), departure values in the Holding (SettleDeparture, §6.2–§6.3), rank-based transit outcomes, the storage-aware clash room, ticket cohorts in full (§2.2), the M1 account set and lock table (§8.2, §8.6), the measured M1 instruction costs (§8.3), and the **final C4 model v3, D18 table and keeper payer band with the measured Reveal CU and `L(reveal)`** (§22.2–§22.4; §21.2–§21.3 are superseded). Two operator documents go with it: `m1/RUN-A-KEEPER.md` and `m1/PLAYTEST-RUNBOOK.md` (devnet configuration only; no devnet step is approved).
- **Paths:** `scratchpad/` means `(session scratch)/scratchpad/`.

**Confidence tags**

| Tag | Meaning |
|---|---|
| **[measured]** | measured on real code: SBF builds under LiteSVM, `solana-test-validator`, `cargo test`, or read-only RPC (file cited). **Revision 3.1: the CU, byte and heap rows come from SP-V2 (the real rules-v10 kernel on SBPF v2)**; M0's rows came from a stand-in kernel on v0 and are kept only where marked |
| **[measured-prior]** | an earlier spike of this project (VRF spike, scale study, v9 round) |
| **[sim]** | output of the M0 host simulator (`frontier-sim`): the real rules-v10 kernels driven by **assumed** player behaviour. A model of play, not a measurement of play |
| **[doc]** | public documentation or source code, URL in §17 |
| **[model]** | arithmetic over stated assumptions in `lab/rev3`, `lab/rev2` or `lab/synth`, **not** a measurement |
| **[estimate]** | a stated assumption or extrapolation, to be replaced by a measurement |
| **[design]** | a choice this document makes |

---

## 0. The decision

### 0.1 What was chosen

**The game is E4, "The Sixfold Frontier". The chassis underneath it is E3's: the world lives on Solana base, not on a public ER. The contention rules are E1's. Revision 2 replaced the money design and the randomness source** (§0.4). **Revision 3 makes Season 1 base-only (no MagicBlock component at all), pins drand quicknet and SBPF v2, and rebuilds the account, beacon and clash-input layer on the M0 evidence** (§0.5).

In one paragraph: players own holdings and armies (hosts) on a shared hex map of provinces arranged in rings around a neutral centre, the Concord. The economy is lazy, as in Eternum: nothing ticks, values are settled when touched. The one thing that stays simultaneous is conflict. **Departures are public, destinations are sealed, the defender's roster is frozen at the start of the arrival bell, and everything that arrives at a province in the same 10-minute bell resolves together** from pre-clash counts, with a seed from a drand beacon round that is fixed by rule and becomes public at least 60 seconds after every reveal for that bell is in. Factions govern themselves in three tiers (local Wardens, a faction Assembly, four Ministers) and steer thousands of members with Mandates. The map opens a new ring whenever the open land is crowded enough, up to a capacity the season states at creation. USDC comes in as a small **citizen fee** plus an optional **laurel stake**, and goes out as prizes from two separate pools; nothing inside the game is bought with money. Hidden operator AIs (Shades) play among the people under a policy committed at genesis, and are revealed and replayed after the season. Every write extends a per-entity hash chain, and anyone can replay the season from base-chain logs.

### 0.2 Why this one: fatal flaws first

| Design | Game (judge 1) | Security and scale (judge 2) | Buildability (judge 3) | Fatal or near-fatal flaw | Verdict |
|---|---|---|---|---|---|
| E2 Provinces (regional ticks) | 5.0 | 4.5 | **7.5** | **Two fatal flaws.** (1) The skew rule is a distributed global tick: one stuck province freezes 39% of the map in 6 h and 100% in 24 h (`lab/E2/sim-results.txt`), which breaks A1. (2) It is not an open world: 2 seats per faction per province, no cross-border combat, dealt seats. | Rejected as the base. Grafted: idle recycling, cohorts, beacon book cost, determinism property test, "ship a playable early". |
| E3 Open Frontier (async + sealed battles) | 7.0 | **8.0** | 7.0 | None fatal. Its game repeats Eternum Season 0's joinable, timed, multi-round battles, which Eternum abandoned. | **Its architecture is chosen**, and in revision 2 also **its per-capita faction scoring**. Its battle model is not. |
| E1 Six Banners (async entities on ER) | 7.5 | 7.0 | 6.0 | Public scale needs an "F13-strict" gated ER that MagicBlock does not document. | **Its contention rules are chosen** (escrowed operator share as the bond, parimutuel split, "never read a shared account on the hot path"), plus Mandates, the accusation game, occupation. Its daily merit cap is **dropped** in revision 2 (it made scripted wallets profitable, §18 issue 14). |
| E4 Sixfold Frontier (bells, gated ER) | **8.5** | 6.0 | 5.0 | No fatal *game* flaw. Its architectural flaws (custom ingress gate, CensusBook overflow, Court mode, ~65k delegations, stall changes outcomes) disappear on E3's base-layer chassis. | **Its game is chosen.** |

### 0.3 Grafts, and where each one comes from

| Graft | From | What it fixes |
|---|---|---|
| World on Solana base; a gated ER lane only as later extension X1 (revision 3: no Skirmish, Arena or other ER lane in Season 1) | E3 §8 | No free lock; no gate dependency; no operator ingress; durable logs |
| Operator's 20% escrowed as the bond; parimutuel faction split; closed-form claims | E1 §9 | Solvency on abort, withholding never pays, O(1) settlement |
| **Per-capita faction index, clamped, with a herding damping** | E3 §5.3 (revision 2) | Joining a big faction never raises expectation (§5.6) |
| No hot-path read of a faction-wide account on any future ER lane | E1 §4, §12.1 | Safe path to the ER lane later |
| Delegated command and tlock keeper reveal | E3 §4.4, §6.5 | Removes the offline "lost in the fog" tax |
| Mandates for Ministers and Wardens (paid from a bounded laurel budget in revision 2) | E1 §8.3 | How a few officers steer thousands |
| Accusation game | E1 §10 | Hidden-AI hunting during the season |
| Live "prize per citizen if the season ended now" | E1 §9.3 | Honest anti-herding signal |
| First holding can only be occupied, never taken | E1 §7.3 | Paying players are never evicted |
| Civilisation Share (Engine builders) | E3 §5.3 | "One civilization" becomes money, not only flavour |
| Companies of ≤ 32 (revision 3: the ER Arena is replaced by an off-chain practice mode that runs the same clash kernel in the client, §2.8) | E3 §2.3, §4.4 | Onboarding; the social unit below the March |
| Idle holdings go dormant; frontier cohorts; friend pairs | E2 §2.6, §4.6 | Ghost towns; late joiners; social clustering |
| Roads, Waystones | E1 §2.4, §2.6 | Rim players can reach the Concord |
| Catch-up kit pegged to the 40th-percentile holding tier | E1 §2.6 | Late-joiner calibration |
| Small tuned stance table (3-cycle + Hold, equilibrium checked in CI) | E3 §6.2 | A tactical read without multi-round battles |
| Site lottery per bell | E3 §3.5, E1 §2.2 | Bots cannot snipe spawns |
| Per-faction arrival quotas, **allocated by mass, one arrival per citizen** | E4 G-e, Dark Forest (revision 2) | No faction, and no spy inside a faction, can crowd others out |
| No outcome-changing fallback; lag only waits | security judge | A4 |
| Scheduler-determinism property test | E2 lab | Order independence proven |
| Herald-style read path (snapshot + diffs, CDN per province per bell) | Eternum §0.7 | The current server breaks at 500–1,000 viewers |
| ~~v9's 48-seat game kept as **Skirmish**~~ (revision 3: dropped; the v9 round is stopped and there is no ER lane, O8) | E4 §2.7, E3 §15.4 | — |

**Explicitly rejected:** E2's skew rule and USDC war chest; E4's single CensusBook, ingress gate as a precondition and Court mode; E3's joinable multi-round battles and its DegradedBeacon; E1's six Throne gates; any USDC inside play; **(revision 2)** a per-bell MagicBlock VRF callback on the Frontier's critical path, the daily honor cap as the payout driver, share-of-totals tier scoring, and Bourse volume in scores; **(revision 3)** MagicBlock VRF anywhere in the Frontier, including the genesis co-seed (O2); Skirmish and Arena as Season-1 products (O8); inline beacon verification inside ResolveClash (does not fit a transaction); two drand networks in one season; System `CreateAccount` for any program-created account (pre-funding blocks it); caller-supplied bumps on any address a reader trusts; doctrine multipliers on scored facts (O5); the square-root Works weight.

### 0.4 What revision 2 changed, in one list

(Kept as history. Items marked *superseded* were replaced in revision 3, §0.5.)

1. **Randomness:** bell and ring seeds come from drand beacon rounds fixed by rule, verified on chain by anyone (BLS12-381 syscalls, or BN254 for drand's evmnet), with per-region anchors. No oracle callback, no shared third-party queue, no predictable account that one lock can hold. *Superseded in part:* MagicBlock VRF no longer co-seeds genesis and there is no Skirmish or Arena (O2, O8); the network is quicknet only (O1); inline verification is dropped.
2. **Lag never changes outcomes:** arrivals live in per-(province, bell, faction, slot) PDAs; reveals are never refused because a province is behind; windows are anchored to when the beacon actually appeared; the 24-hour VRF Abort is gone (§8.4, §8.10).
3. **Sealed simultaneity inside a bell:** the combat roster freezes at the start of the arrival bell; tlock encrypts to the round at the *end* of the bell; postures close before the bell; unrevealed arrivals are routed (50% loss); keepers are paid tips (§6.2–§6.4).
4. **Money:** a citizen fee (4 USDC at day 0) plus an optional laurel stake (6 USDC). Two pools. Laurels are zero-sum emissions, not minted by activity; the daily honor cap is gone. A scripted wallet as strong as an above-average human earns 0.86× what it pays [model] (§5.4). *Superseded:* the M0 simulator shows 1.00–1.07× at low bot shares; revision 3's plan is in §5.4.
5. **Faction score:** per-capita index, smooth, clamped, with herding damping; no Bourse volume; contiguous March control (§5.6).
6. **Shades:** policy committed at genesis and replayed by the verifier; no LLM in actions; contributions removed from faction score; 0.5% of players; genesis rings seeded after joins are fixed; a Shade Auditor can trigger forfeiture (§7).
7. **Capacity stated honestly:** a per-season map capacity (default `R_MAX` 64, about 100k–130k citizens), Join refuses without charging when land runs out, fee escrow until a site is settled, per-wallet action bucket; adversarial load re-run (§3.4, §8.9).
8. **Human-scale combat:** sieges take at least 6 hours outside the defender's chosen 8-hour vigil; fronts protected for new cohorts (§6.6).
9. **Governance and identity:** Assembly weight by ballots cast, recall needs Assembly weight, March-level hostility, six asymmetric faction doctrines, first holding in one's own wedge (§4).
10. ~~**Skirmish is stake-free** until a gated ER passes the lock drill~~ *Superseded:* there is no Skirmish (O8).

### 0.5 What revision 3 changed, in one list

1. **Base only (O8).** Season 1 has no ER lane, no Skirmish and no Arena. Onboarding happens inside the Frontier (guided first bells against barbarian camps) plus an off-chain practice mode that runs the rules-v10 clash kernel in the browser (§2.8). X1 (a gated ER) stays a later extension.
2. **One drand network, quicknet, and an SBPF v2 build (O1, O2).** Bell seeds, ring seeds, the genesis seed and tlock seals all use quicknet. The genesis seed is a drand round fixed by rule at least 10 minutes after `CreateSeason`; MagicBlock VRF is gone from the Frontier. Quicknet verification costs 324–330k CU on SBPF v2 against 882–895k on v0 [measured, S-BEACON], so the program is pinned to v2 (v3 once platform-tools supports it) (§8.5).
3. **Pre-funding-safe account creation (blocker).** No program-created account uses System `CreateAccount`/`CreateAccountWithSeed`. The program tops up to rent-exempt, then Allocates and Assigns (WithSeed variants for with-seed addresses), so lamports sent to an address in advance cannot block it. Every init path gets a pre-funded-address test (§8.6 rule 10).
4. **Canonical addresses, checked on every read (blocker).** Exactly one BellAnchor per (season, bell, region); ArrivalSlots, PosturePDAs, anchors, seed caches and clash inputs use with-seed addresses from the Season PDA, so no caller-supplied bump can create a second copy (§8.2).
5. **Beacon layer rebuilt.** Inline verification in ResolveClash is dropped (it does not fit a transaction). Seeds live in nonce-keyed SeedCaches, up to 256 per region-bell, which anyone may create and which all hold the same seed. The seed round has a margin: `S(b, r)` = the first quicknet round scheduled at or after `A(b, r) + 600 s + Δ` (M1, CL-19: revision 3 wrote `round_at`, which rounds down), Δ ≥ 60 s, and Reveal refuses once `Clock ≥ A(b, r) + 600 s`. An `ArchiveAnchors` crank lets anchors close after 48 h (§8.4, §8.5).
6. **Sealed orders become provable.** A pinned 165-byte quicknet seal is mandatory on every Depart and CommitPosture; the chain can open it (48.0k CU with the FO check on SBPF v2 [measured, SP-V2]; revision 3 said 43k; revision 3.1 also puts the 32-byte commitment itself in Depart's instruction data, §6.2). A new permissionless `ProveBadSeal` writes a `SealVerdict`; a proven bad seal destroys the host and pays the prover. This closes the "garbage ciphertext for 50% of a host" last-look option that revision 2 left open (§6.2, §6.4).
7. **Clash inputs gathered, not crammed.** Postures do not fit in one ResolveClash transaction, so a permissionless, idempotent `GatherClash` writes a `ClashInputs` account and `ResolveFromInputs` resolves from it. The Province mirrors each site's garrison, walls and owner, so the resolver never reads Holdings. *Corrected in 3.1:* with the real kernel a worst-case clash costs 367k–633k CU, and the full-gather `ResolveFromInputs` is the primary path, budgeted at 650k per worst-case clash (§6.3, §8.3).
8. **The fee market at its real parameters (O7).** The per-account write cap is 40M CU (40% of the 100M block, SIMD-0306), not 12M. C4 is restated for a whole bell and a targeted side, and a **defence pool** lets keepers bid above the tip during an attack (§6.4, §8.6, §8.7). *Corrected in 3.1:* revision 3 said that holding three or more accounts costs a whole block; SP-FEE measured that one multi-lock stream holds 4, 20 or more known accounts at one account's price. Only a side whose reveals go to addresses and from fee payers the attacker cannot predict forces whole-block pricing (§8.6, §8.7).
9. **Kernel rules now written down.** An arrival lands no earlier than its departure bell + 2, with the host's values after the origin's clash (*3.1:* read by GatherClash, not by Reveal, §6.2); changes issued during a bell take effect after that bell's clash; resident actions need the province resolved through b − 2; hex fair share is allocated by side; quiet bells; sieges progress only while the besieging faction holds the site; passable paths are carved from every gate and site (§3, §6, §8.4).
10. **Economy clarified and bounded (O3, O4, O9, O10).** A late stake counts only laurels banked after it; occupations and siege stakes follow the pair rule; fee-only wallets transfer no laurels; Works are weighted linearly with a cap per USDC of fee; officer pay is bounded so an always-online wallet cannot profit from office; the Mandate reserve pays stakers who completed only, in closed form; the bot criterion has an ordered plan (§4.2, §4.3, §5.4).
11. **Doctrines without direct multipliers on scored facts (O5)**, tuned to 6 of 6 in the 16.7% ± 2-point band and gated in CI; γ = 0.6 (O6) (§4.1, §5.6).
12. **Joining at scale.** SettleTicket displacement also rewrites the displaced citizen's records; Holding PDAs are keyed by site; a permissionless `FoldOccupancy` feeds Join's land-reserve check (§3.4, §8.3).
13. **Milestones re-planned** around the M0 result: M0 is not complete; M1 has no Skirmish (§12).

### 0.6 What revision 3.1 changed, in one list

Inputs: the redone spikes SP-V2 and SP-FEE (`scratchpad/frontier/m0b/spikes/`), the K3 simulator work (`m0b/sim/ECONOMY.md`, `DOCTRINES.md`), the K3 integration, and the 22 confirmed issues of the review of revision 3 with the m0c measurements (`scratchpad/frontier/m0c/`). §20 maps every issue.

1. **The BellAnchor address is predictable again** (`an ‖ hex(bell, region)`), so Reveal can prove "no anchor yet = window open". Revision 3's `sha256(sig_T)` seed would have let a late revealer omit the anchor and reveal after the seed round was public. Closing an anchor after 48 h no longer reopens its window: `ArchiveAnchors` tombstones the bell first, and Reveal and PostAnchor refuse a tombstoned bell (lab test `t9`, §8.4) [measured, m0c].
2. **The seal is bound to its commitment on chain.** Depart and CommitPosture carry the 32-byte commitment and the 165-byte seal in instruction data; the program computes `seal_root` itself; ProveBadSeal takes the commitment. The 37-byte plaintext and the commitment domain are pinned (§6.2).
3. **Origin lag no longer changes outcomes.** Reveal no longer reads the origin Province; it ranks by the departure mass stored in the Holding's transit record. GatherClash reads the origin's post-clash values and has no deadline (§6.2, §8.4).
4. **Every CU, byte and heap figure is SP-V2's** (real rules-v10 kernel, SBPF v2): a worst-case clash 367k–633k CU (budget 650k), ResolveFromInputs 541k, heap 25.9 KB of 32 KiB, ProveBadSeal 48.0k / 588 B, PostAnchor 709 B with the archive key. The capacity point is restated: about 60,000 players at the adversarial 10% line with the 30/h bucket, 75,000 with 20/h [model, `lab/rev31`] (§8.3, §8.9).
5. **The fee-market model is rewritten from SP-FEE** (§8.6, §8.7): one province costs p × 40M per block for up to 20 (legacy) or 60 (lookup table) known accounts; a side-wide attack costs a whole block **only if the side's fee payers are unpredictable** — drill (g) held a Reveal into an unknown address by locking its keeper's fee payer at one account's price [measured, m0c]. Keepers pay from ≥ 150 rotating payers.
6. **C4 is reported as NOT MET at the default tip [model].** The defence pool at cap 2.0 passes in the model on every valuation and capacity case **only if** keepers pay from ≥ 150 rotating fee payers; with a known payer it depends on the leader execution rate. The M1 Reveal CU and the M4 soak decide; the value side now also comes from the simulator (`frontier-sim c4`), and the M0 exit criterion is restated (§6.4, §12).
7. **The defence pool is specified**: refunds are a non-critical claim from per-keeper shards, eligibility is on-chain lateness evidence with per-bell and per-keeper caps, and the keeper SDK rules are part of the spec (§6.4, §8.8).
8. **Officer pay is K3's 95% ceiling**, `steward_i ≤ max(0, 0.95 × paid_i − rest_i)`; the office laurel rows (D21) are dropped (§4.3, §5.4).
9. **The bot criterion is taken as the maximum over the bot's own choices** (join window × stake × office): worst 0.980 at 1% (days 1–7, staking), a 2-point margin, not 4–5 [sim, m0c]. **It fails when bot officers steer Mandates** to always-online tasks (1.04 at 1% if humans then complete a quarter as often) [sim, m0c]; a Mandate share floor (kernel) bounds the worst case, and Mandates come from a fixed menu of single-action tasks with ≥ 24-h windows (§4.2, §5.4). Bots' share of office-terms is reported (61% at 1% bots) [sim].
10. **The costs of the K3 economy to honest players are stated**: late stakers lose 10–18 points against M0, and the skill premium over a bot is gone (§2.6, §2.7).
11. **Doctrines**: the table is the kernel's, **provisional** until the unsimulated mechanisms land; F's heavy cavalry gets its own variant weight so it no longer fields B's army; the CI gate is a 180-season proxy with three negative controls, and the O5 band itself runs on 1,500 paired seasons in a scheduled workflow (§4.1, §9.4).

---

## 1. Vision

**The fantasy.** You are one citizen of a civilization split into six rival factions. You own a hamlet on your faction's frontier, you grow it into a stronghold, you raise hosts and send them out under sealed orders, and you watch the bell to see what met them. Around you, thousands of others do the same. Your faction is a real polity: your neighbours elect a Warden, the Wardens elect Ministers, the Ministers post Mandates and make peace, and you decide whether to follow. At the centre stands the Permutation Engine, which the whole civilization builds together. Somewhere among you are the operator's Shades, playing by a policy locked before the season began. At the end, everyone can check every result from the chain.

**Design goals, in priority order.**
1. **Anyone, any number, any time.** No seat cap below the season's stated map capacity. Joining is open until day 21 of 28, and the price falls with the days left.
2. **Every player moves the map** (pillar P9).
3. **Sealed simultaneity where it matters** (P2): "where is that host going?"
4. **Politics that scale into a career.**
5. **Nobody, the operator included, can steer the result, and anyone can replay it** (P8).
6. **Human-scale time.** A 15–30 minute daily session is enough. A night's sleep costs nothing if you set your vigil hours.
7. **Money is honest.** The join page says what an average player gets back; automation and extra wallets do not pay by themselves.

**What it is not.** It is not a copy of Eternum. Eternum's combat is instant and first-come, which on a fee-free first-come sequencer turns into a latency race that bots win. Its tribes pay the leader 30% off the top, and whales buy realms. Here clashes are sealed and simultaneous, officer pay is capped, and no money buys power.

**Eternum mapping, for orientation.**

| Eternum | Sixfold Frontier |
|---|---|
| Realm, Village | Holding (Hamlet → Town → City → Stronghold); ≤ 3 per wallet |
| Army with stamina | Host with stamina; sealed marches |
| Instant combat between adjacent units | Everything arriving at a province in one bell resolves together |
| Hyperstructures and victory points | The Permutation Engine plus four faction paths |
| Tribes | Companies (≤ 32, no treasury) under a March and a faction |
| Bank (AMM, 15% + 15% fees) | The Bourse: a batch auction per bell, 1% + 1% fees |
| Settlement rings grown by demand | Province rings opened by a crowding rule |
| Blitz (60-minute lobbies) | None in Season 1 (revision 3: Skirmish dropped, O8); an off-chain practice mode for learning the clash |

---

## 2. The game loop

### 2.1 Time scales

| Unit | Length | What happens |
|---|---|---|
| **Bell** | 10 min (600 s) | The combat roster of every province freezes at the bell's start. Sealed arrivals land. Reveals open at the bell's end (tlock round) and close 10 minutes after the beacon for that round is first anchored on chain. The bell's seed is the beacon round at that close **plus a margin of at least 60 s** (revision 3). Clashes, explorations and market clears resolve. **Pinned (M1, CL-20):** `bell_start(b) = genesis_ts + 600 b`, `bell_end(b) = bell_start(b + 1)`, and the tlock round `T(b)` is the first quicknet round scheduled at or after `bell_end(b)`. The roster freeze and the posture close are the wall-clock boundary `bell_start(b)`, never derived from `T(b)`, so the drand phase cannot move them (kernel `frontier::beacon`, §21.1). |
| **Day** | 24 h = 144 bells | An active day is counted; faction folds run hourly; each holding's 8-hour vigil applies |
| **Term** | 4 days | Warden and Minister elections; per-term caps reset |
| **Season** | 28 days = 7 terms | Joins until day 21 ("Last Call"); the Reckoning at day 28 |

### 2.2 Joining (about two minutes)

1. Connect a wallet, or join through x402 via the gateway (reused). **Every join, human or Shade, goes through the same relay path by default:** the relay fronts rent and a fee quota, so funding graphs do not separate people from Shades (§7.4).
2. **Faction picker.** For each faction: members, its doctrine (§4.1), free land in its homeland wedge, its per-capita index, and a live **"prize per citizen if the season ended now"** computed from the smooth per-capita formula (§5.6), so it has no cliffs. A faction below the average size shows a **Frontier Grant** (+50% starter resources and one extra settler).
3. **Pay.** The **citizen fee** `c(day) = 4 × max(0.25, (28 − day)/28)` USDC (4.00 / 3.00 / 2.00 / 1.00 at days 0/7/14/21). Optionally add the **laurel stake** `l(day) = 6 × max(0.25, (28 − day)/28)` USDC, which enters you into the laurel pool (§5.4); it can be added later until day 21 at the price of that day, and **a late stake counts only laurels banked after it** (revision 3). The join page shows the expected return of an average player for each choice (§2.7). **The fee is escrowed until your first holding is settled.** If no site is settled within 24 hours, you may withdraw 100% (§3.4).
4. **Choose where to live.** Your first holding must be in **your faction's wedge** (or, if the wedge is full, in the overflow ring of an adjacent wedge). The map highlights free sites, Colonist invitations from your Wardens, and sites next to friends. You submit up to three preferred sites as a **site ticket**; all tickets filed in the same bell are settled together by the bell's seed (§4.5 of E3), so speed wins nothing. Friends can file a **pair ticket**. **Ticket cohorts (M1, I-47; replaces revision 3's "challenge window until the end of bell b + 1").** Every ticket filed at bell b for sites in a province joins that province's **cohort for bell b**. A settled site stays **provisional** until both (1) 10 minutes after the bell's seed round `S(b, r)` and (2) its cohort is closed — every ticket filed there at bell b has been settled, or 24 bells have passed. Inside the cohort a higher-ranked ticket may displace the holding **at any time**, with no deadline, and the displacement rewrites the displaced citizen's Citizen record and JoinShard too (holding and fee escrow revert); the displaced ticket ends, and the client offers a one-tap refile. While provisional a holding may Harvest, Build and Train (lost if displaced) but not Muster, Dissolve, Garrison, Explore or Depart (I-29). The holding appears (provisional) about 11–21 minutes after the ticket is filed [model, M1 web design §6.3], and with keepers settling the cohort promptly it becomes final soon after; to steal a site an attacker must hold the Province against delay-class keepers for 24 bells [model, M1 contract I-47]. MarchRoll counts and Warden eligibility use only final holdings.
   **Cohorts in full (M1, I-47, O-M1-13/O-M1-20; contract §5.9, DECISIONS K9/L4).** (a) *Filing.* `FileTicket` names ≤ 3 sites whose Provinces must already exist (ring ≥ 2; rings 0–1 are reserved), writes each of those Provinces' cohort record for the filing bell (a Province keeps 8 cohort records; a full set is `CohortFull`), and moves the Holding's rent (`rent(1,280 B)`) into an **escrow in the Citizen**, paid by whoever funds the ticket (the relay in sponsored play); a later ticket over an escrow left by an expired or exhausted one keeps the original funder. (b) *Settling.* Keepers settle every ticket of a cohort (`SettleTicket`, class D, a delay-only write) **in descending lottery score in the first slots after the seed round `S(b, r)`**; the site score is `rand(S, "site", P‖Q‖site‖citizen)`. Outcomes: *fresh* (the Holding is created from the escrow), *displace* (a provisional holding of the same cohort with a lower score is rewritten in place; the winner's escrow pays the Holding's rent back to the displaced ticket's funder, the displaced citizen's ticket ends, and the client offers a one-tap refile), *taken* (the next preference follows, in the same transaction's success), *exhausted* (all preferences taken: the player files again, the escrow is kept) or *expired* (24 bells after filing). (c) *Ordering between cohorts.* A fresh settlement **waits** while an earlier cohort of the same Province is still open, so a later cohort can never make an earlier winner lose its site and the 24-bell bound holds (DECISIONS L4). (d) *Finality.* A holding is final at `round_time(S) + 600` **and** once its cohort is closed (every ticket filed there at that bell settled, or 24 bells passed); the flip is lazy, in the next owner action or settlement. (e) *Price of theft.* Holding the Province (a class-D account) for 24 bells costs ≈ $59k–163k on §8.7's model instead of revision 3's ≈ 0.6 SOL against a fixed low bid [model, contract §8.8]. (f) *Invites (M1 playtest, I-51).* When the season sets `join_gate`, Join needs the gate key's co-signature, which the relay gives only for a valid one-time invite; open seasons leave it zero.
5. The holding appears at the next bell with a **48-hour Shield** (72 h for joiners after day 7), then 7 days of **Frontier protection** (§6.6).
6. You choose your **vigil hours** (an 8-hour window, default 00:00–08:00 in your browser's time zone), during which siege progress against your holdings pauses (§6.3).

### 2.3 The first ten minutes

- **Minute 1.** Queue a Farm and a Lumber Camp.
- **Minute 2.** Train a Scout host, muster it, **Explore** two hexes. Results land at the next bell. The first three explorations of a new holding have a floor reward (Works points, not laurels; **M1 gives no goods**: the simulator, which is normative for the M1 kernels, has none, so the goods part of the floor waits, I-56).
- **Minute 4.** The March panel: your March is the 7-province district around you. The faction's pinned **Mandate** shows.
- **Minute 5.** Optional **practice clash** in the browser against a rule bot (revision 3: runs the rules-v10 clash kernel locally; nothing on chain, nothing staked).
- **Minute 6.** First sealed march at a barbarian camp: "arrives at bell 1,034, destination sealed". The client always posts a timelock seal of the orders and **the minimum reveal tip** (M1: stated in priority terms; a zero tip is impossible, §6.4) so that any keeper can reveal them even if you close the tab (§6.2, §6.4).
- **Minute 10+.** The bell ends, the reveal closes, the seed lands: combat report, loot, Works points. The Chronicle records your founding.
- **Reachable times, restated (M1, I-33).** The minutes above are the order of the first actions, not their clock times. With a site ticket (settled by the bell's seed, final once its cohort closes, §2.2), the earliest arrival bell (departure bell + 2) and the reveal window, the first holding appears about **11–21 minutes** after its ticket is filed and the first clash report arrives about **31–41 minutes** after the first march departs [model, M1 web design §6.3]. The rules are unchanged; the onboarding card and the practice mode fill the wait.

**Progressive disclosure.** A newcomer sees Holding, Host, Bell, March and Mandates. The Warden vote appears on day 2. Assembly and Ministers are visible only in the news feed until the player stands for office.

### 2.4 A daily session (15–30 minutes)

- **Collect and plan.** Harvest is implicit in any action. Refill the 4-item queue. Upgrade the holding tier.
- **Army.** Train, muster and merge hosts. Send sealed marches toward Mandate targets or your Company's rally point. Commit a sealed **posture** before a bell in which a hostile arrival is due (§6.3).
- **Economy.** Bourse orders; caravans; Engine, research and March pledges.
- **Politics.** Decrees, the war horn, Warden votes, petitions, accusations.
- **Delegate.** Hand hosts to a Company captain or marshal for the night (§6.5).
- **Automation.** Standing behaviours (auto-harvest, auto-queue, auto-reveal, auto-reinforce) run in your client or a relay you choose. The public bot SDK gives everyone the same tools (P6). The payout rules are built so that automation alone does not return more than you pay (§5.4).

### 2.5 Long-term goals

- **Personal:** grow to City and Stronghold; earn **laurels** (if you staked) and **Works**; Chronicle titles.
- **Faction:** four paths, each measured **per active member and relative to the civilization average** (§5.6): *Dominion* (March control and captures), *Prosperity* (production), *Knowledge* (techs and science), *Concord* (Engine share, treaties kept).
- **Civilization:** the **Permutation Engine** has 5 stages. Each raises everyone's tech ceiling, advances the Era, grows the Civilisation Share and spawns six **Relic Sites**, one per wedge, **three rings inside the current rim** (not on it; §6.6).
- **Political:** Warden, then Minister.
- **Shade hunting:** accuse suspected Shades; capture their holdings.

### 2.6 Late joiners and the end

- **Joins until day 21.** Fee and stake scale with the days left (floor 25%). Late joiners file tickets in their wedge's outermost ring by default (a **frontier cohort**), get a 72-hour Shield and 7 days of Frontier protection, and a **catch-up kit** to the civilization's 40th-percentile holding tier (capped at 3× the day-0 kit).
- **Honest expectation, published on the join page:** the goal is that the expected return *per USDC paid* is roughly the same on day 0 and day 21 for the same play. Revision 2's formula model said it was [model: `rev2-results.txt` §C3, 0.63–0.68× for a regular who stakes]. The M0 simulator said it was not: day-0 stakers got 0.67× their stake back from the laurel pool, joiners on days 1–14 got 1.07–1.08× [sim, M0 `RESULTS.md` §A.3].
- **Revision 3.1, on the K3 economy** (order-weighted emission, the stake priced by the accrual still to come with ramp 2.0, no Relic laurels) [sim, `m0b/sim/ECONOMY.md` A.3, B.2; 5 seeds × 10k wallets, 5% bots]: the laurel-pool claim per staked USDC by join bucket (day 0 / days 1–7 / 8–14 / 15–21) is **0.77 / 0.90 / 0.79 / 0.70**, against 0.67 / 1.08 / 1.08 / 0.95 in M0. Days 1–7 are still best (late first holdings carry 1.6–1.75× their province's mean weight).
- **The cost of that, stated plainly** [sim, m0c `payout_ramp20000.md` vs `payout_m0.md`, same seeds]: **late stakers lose 10–18 points** against M0. A staker joining on days 15–21 gets back: casual 0.67 → **0.57**, daily 0.90 → **0.76**, skilled 1.01 → **0.83**, very skilled 1.10 → **0.87**. The dearer late stake is what closes the late-stake bot exploit (§5.4).
- **Option for the owner (D22):** a flatter ramp. At ramp 1.0 the days-15–21 stakers get casual 0.60, daily 0.79, skilled 0.87, very skilled 0.90, which is no worse than M0's day-0 cells (0.59 / 0.75 / 0.83; M0's very skilled day-0 1.68 was officer pay), while the bot criterion still passes with a thinner margin: worst bot choice 0.985 against 0.980 at ramp 2.0 [sim, m0c `best_ramp10000.md`]. Ramp 1.5 lands in between. The working default stays 2.0.
- The join page publishes the measured table by join day, whatever it shows. Late joiners cannot win the season's top laurel ranks.
- **The Reckoning (day 28, bell 4,032):** the last bells resolve; a 72-hour **banking window** credits everything earned before T_end at its closed-form value (§8.4); faction results fold; Shades are revealed and replayed; claims open; the Chronicle root is written; rent returns to whoever paid it.

### 2.7 Player archetypes and what each gets

**Revision 3.1 uses the K3 economy** (§5.4: officer-pay ceiling 95%, 105 Works per USDC, the stake priced by the accrual still to come, order-weighted emission, no Relic laurels, the Mandate reserve to staking completers) [sim: `scratchpad/frontier/m0b/sim/ECONOMY.md` §1 and appendix A; `frontier-sim suite --agents 10000 --seeds 5`; bots 5% of wallets, γ = 0.6]. The simulator runs the real rules-v10 kernels (holdings, hosts, every clash, sieges, reward indices, index, pools, claims) but **player behaviour and in-game costs are assumptions** (`frontier-sim/src/model.rs`). M0's (revision 3's) numbers are in brackets.

| Archetype | Typical play (sim assumption) | Citizen fee only (× fee) | With laurel stake (× fee + stake) |
|---|---|---|---|
| Idle / passive Sybil | pays, never plays | 0.51 [0.51] | 0.24 [0.24] |
| Casual | 1 session on 3 days of 7 | 0.80 [0.80] | 0.64 [0.64] |
| Daily | 1 session a day | 0.88 [0.87] | 0.82 [0.81] |
| Skilled | 2 sessions a day | 0.93 [1.00] | 0.89 [0.90] |
| Very skilled | 4 sessions a day | 0.96 [2.87] | 0.95 [1.58] |
| **Scripted bot wallet** | online every hour, SDK default | 0.96 [1.02] | **0.93** [1.00] |

- **The average player loses money**, and the join page says so. The citizen fee is a participation price: a casual player gets back about four fifths of it. The laurel stake is a contest with a 20% rake: only players better than the average staker win money.
- **Officer pay no longer makes anyone profitable** (O3, §4.3): M0's very-skilled row (2.87 / 1.58) was almost all steward rows; under the 95% ceiling it is 0.96 / 0.95.
- **The skill premium over a script is gone** [sim]: a very skilled human and an SDK-default bot now return the same (0.96 fee only; 0.95 against 0.93 with a stake). Automation neither wins nor loses much against the best humans; the criterion output now prints this gap (m0c).
- **The bot criterion** [sim, K3 + m0c]: at the SDK default mix a staking bot returns **0.949 / 0.946 / 0.935 / 0.912** at 1 / 2 / 5 / 10% bots, and **0.962 / 0.959 / 0.948 / 0.926** when bots stand for office. **Taken as the bot's best choice** (its join window, stake or not, office or not; m0c `criterion --best-response`), the worst cell is **0.980** (1% bots, all joining on days 1–7 and staking): the margin under 1.0 is **about 2 points**, not the 4–5 of the default mix. **With bot officers steering Mandates** to tasks only always-online wallets can do, it **fails**: 1.04 at 1% if humans then complete a quarter as often (§4.2, §5.4).
- A passive Sybil gets back about half its fee. A scripted wallet that stakes competes only with other stakers for a fixed, zero-sum laurel emission, so a farm of bots mostly splits the pot among itself.
- Whales get mastery and leadership (commander, Minister) but cannot buy outcomes: there is no USDC path to resources; laurels come from a fixed emission split by contest; the wallet cap is 5× what the wallet paid.

### 2.8 No Skirmish or Arena in Season 1 (revision 3)

- **The owner stopped the v9 security round and chose a base-only Season 1 (O8).** Today's 48-seat v9 game is not revived as "Skirmish", and there is no Arena on the ER. The Frontier has no MagicBlock dependency in Season 1.
- **Onboarding without Skirmish.** (1) The first bells are guided: a newcomer's first holding has barbarian camps within reach, the first three explorations have a floor reward, and the tutorial panel walks through one sealed march, one reveal and one clash report. (2) An **off-chain practice mode** in the web client runs the rules-v10 clash kernel (compiled to WebAssembly) against a rule bot: the same stances, retreat ratios and fair-share rules, nothing written on chain, nothing staked. (3) Progressive disclosure as in §2.3.
- **Tournaments** are not part of Season 1. A short-format game could return later as a separate product (the O8 alternative); it would need its own design and review.
- **X1** (the Frontier on a gated ER) remains a later extension (§12).

---

## 3. The map and how it grows

### 3.1 Geometry (E4)

- **Tiles:** axial hexes from `permutation-rules::hex`, bounded to |q|, |r| ≤ 2¹⁶ at every entry point (closes WP08 `hex-coord-i32-overflow`).
- **Provinces:** radius-4 hexagons of 61 tiles with 12 settlement sites each, tiling the plane on the lattice generated by (9, −4) and its rotations; any tile's province id is O(1).
- **Rings:** ring 0 is the **Concord** (neutral). Ring 1 holds the **Six Seats** (never capturable; spawn camps). Ring d ≥ 2 holds 6d provinces.
- **Wedges:** six 60° wedges are the factions' homelands. **A citizen's first holding must be in their own wedge** (revision 2; overflow to the adjacent wedges' outer ring only when the own wedge has no free site). Holdings 2–3 may be founded anywhere open. So each wedge has a solid core and a front at its seams and rim, while second and third holdings create salients and mixed Marches.
- **Marches:** a fixed cluster of 7 provinces (84 sites), the unit of local governance and of Dominion scoring (§5.6).
- **Heartlands:** rings 2–3 of a faction's own wedge. No sieges there without a War decree.

### 3.2 Terrain: symmetric by construction

- A province in wedge k at ring d is generated from the canonical province rotated back to wedge 0, from the ring seed `R_d`. All six wedges of a ring have identical terrain. *Every faction's wedge offers the same land at the same ring at the same moment.*
- Value-noise terrain with threshold classification; 12 sites chosen by score, at least 2 apart. `mapgen`'s noise and rotation are reused.
- **Connectivity by rule:** each province edge keeps ≥ 3 passable border tiles, and (revision 3) **a passable path is carved from every gate and every site to the province centre** (for the Concord, the union of its six rotations). Without the carve, 10.3% of provinces had gates in different components [measured, K1 review]; the kernel test now finds every site and gate in one component over 24 seeds out to ring 6 [measured, `cea89be`].
- **Genesis rings (2..g) are seeded after the season's join parameters are fixed and before joins open**, from the drand quicknet round fixed by rule at `CreateSeason`'s on-chain time + 600 s + Δ (revision 3, O2; §8.5). By then the join parameters, `roster_root` and `policy_code_hash` are fixed. The operator cannot know that round in advance unless it colludes with a threshold of the League of Entropy, which is already the trust assumption for every other seed. MagicBlock VRF is no longer involved.
- **Cost:** `OpenProvince` = **167.7k CU** [measured: `lab/E4-free/sbf-results.txt`].

### 3.3 Fog, vision and discovery

Three things are genuinely hidden: (1) **destinations and postures** of sealed marches until the arrival bell's end; (2) **discoveries**, rolled per (hex, bell) from the bell seed; (3) **Shade identities** until the Reckoning. Client-side fog is presentation only. A MagicBlock Private ER that gates *reads* of arrival slots remains a later option (X2).

### 3.4 Growth, capacity and what happens when land runs out

`OpenRing(d+1)` is permissionless and succeeds when all of these hold:
- occupied sites in rings ≤ d ≥ θ × open sites **in any one wedge** (the crowded faction's wedge drives growth; small factions get empty land, E3's underdog subsidy), with θ = 55% for 72 h and 65% afterwards;
- at least one bell since the last opening;
- `d + 1 ≤ R_MAX` and the **ProvinceFund** holds the rent for the ring's 6(d+1) provinces.

**Season capacity is stated, not hidden (revision 2).**
- `R_MAX` is fixed at `CreateSeason`; **default 64, program hard maximum 128.** Provinces are created only when their ring opens, so a large `R_MAX` costs nothing until it is used.
- The operator escrows the **ProvinceFund** at `CreateSeason` for rings up to `R_fund` (sized as twice the pre-registrations; default 40 → 4,914 provinces, ≈ 105 SOL at 5,080 lamports/byte). Anyone may top it up; it is refunded at close. The join page shows the season's capacity in citizens.
- Capacity [model: `rev2-results.txt` §F]:

| `R_MAX` | Provinces | Sites | Citizens at 1.15 holdings | If every wallet founds 3 | Province rent float at full size |
|---|---|---|---|---|---|
| 24 | 1,794 | 21,528 | 18,720 | 7,176 | 38 SOL |
| 32 | 3,162 | 37,944 | 32,995 | 12,648 | 68 SOL |
| 48 | 7,050 | 84,600 | 73,565 | 28,200 | 151 SOL |
| **64 (default)** | 12,474 | 149,688 | 130,163 | 49,896 | 268 SOL |
| 96 | 27,930 | 335,160 | 291,443 | 111,720 | 599 SOL |

  (Revision 1 said the hard maximum was 64 while its own growth table needed ring 70 at 100k players. The hard maximum is now 128.)
- **Land cannot be exhausted by founding extra holdings:** a second or third holding may be founded only while free sites are ≥ 20% of open sites. With that gate the default map holds ≈ 104k citizens even if every wallet tries to found three.
- **Paid joins never end up without land.** `Join` is refused, with nothing charged, when free sites fall below a 2% reserve and no ring can open (at `R_MAX` or with an empty ProvinceFund). The fee sits in escrow (in the citizen's vault shard, flagged pending) until `SettleTicket` places the first holding; if 24 hours pass without a site, `WithdrawJoin` refunds 100%.
- **Where Join reads occupancy from (revision 3).** The no-global-counter rule (§8.6 rule 4) leaves Join no place to read the free-site count. A permissionless **`FoldOccupancy`** (two transactions) sums the 48 JoinShards into the Frontier account; Join reads that folded value. The reserve is sized to cover the joins that can land between folds (the fold runs at least every bell while joins are open) [design; S-SIZE-JOIN finding].
- **Dormant first holdings are released:** a first holding with no owner action for 10 days becomes a Free City site; the owner keeps citizenship and may re-found with a refugee kit on return.

Simulated growth [model: `lab/synth/model-results.txt` §1]:

| Players | Final ring | Provinces | Sites | Joiners ever without a site | Final fill |
|---|---|---|---|---|---|
| 1,000 | 7 | 162 | 1,944 | 0 | 59% |
| 10,000 | 22 | 1,512 | 18,144 | 0 | 63% |
| 50,000 | 50 | 7,644 | 91,728 | 0 | 63% |
| 100,000 | 70 | 14,904 | 178,848 | 0 | 64% (needs `R_MAX` ≥ 70) |

### 3.5 Keeping the map alive

- **Dormant holdings (E2).** No owner action for 5 days → *Dormant*: Shield lapses, production halves, no laurel emission; raidable, occupiable (first holding) or capturable (2–3). After 10 days a first holding's site is released (§3.4).
- **Relic Sites** pull players outward at each Engine stage.
- **Free Cities** and **barbarian camps** are always attackable.

### 3.6 Travel

| Terrain or mode | Time per hex |
|---|---|
| Plains, foot | 2 min |
| Hills, forest | 3 min |
| Road (built by March Levies) | ×0.5 |
| Cavalry | ×0.5 |
| Mountains, water | impassable except passes and fords |

A march's arrival is rounded up to the next bell and is **never earlier than the departure bell + 2** (revision 3, `MIN_ARRIVAL_LEAD_BELLS`; §6.2). A player may pick a later arrival bell (up to 72 bells ahead). One march covers ≤ 32 path steps over ≤ 4 provinces; because of the 4-province cap, a straight march reaches **20–28 hexes, not 32** [measured, K1 review].

Rim-to-Concord distance [model: `model-results.txt` §6]:

| Players | Rim ring | Hexes | On foot | On roads or cavalry |
|---|---|---|---|---|
| 1,000 | 7 | 55 | 1.8 h | 0.9 h |
| 10,000 | 22 | 172 | 5.7 h | 2.9 h |
| 50,000 | 50 | 392 | 13 h | 6.5 h |
| 100,000 | 70 | 548 | 18 h | 9 h |

At 100k the rim is 18 hours on foot from the centre. **Waystones** (City-built; 3 bells between two of the faction's Waystones, arriving only into a friendly holding's hex, destination public) keep the Concord reachable within a session. **Supply:** hosts more than 3 provinces from any friendly holding lose 1% of troops per bell.

### 3.7 What persists across seasons

Terrain does not persist. The **Chronicle** persists: a Merkle root of every founding, capture, Engine stage and title, chained season to season.

---

## 4. Factions and governance

### 4.1 Identity

- **One civilization:** the Concord, the Permutation Engine, shared Eras and a shared tech ceiling, and a Civilisation Share of the prize.
- **Six factions with asymmetric doctrines (revision 2; rule fixed in revision 3, O5).** Each faction keeps today's id and display name from HEAD `96a3464` and gets one **doctrine**: a unit variant, a stance or movement edge and a civic power.
- **Rule (revision 3): no doctrine multiplies a scored fact or growth directly** (production, science, Knowledge weight, cheaper expansion, bigger grants). Such a bonus wins almost every season, because a faction's per-capita index varies only about 1.4% between seasons: with revision 2's draft table **Lumen won 99.5% of 600 seasons and 0 of 6 doctrines were in the band** [sim, `RESULTS.md` §E1]. Removing those multipliers brings every doctrine's mean index within ±0.27% and puts **3 of 6** win rates in the band [sim, §E2].
- **Criterion (O5):** every doctrine's win rate in the balance simulation lies in **16.7% ± 2 points**, 6 of 6, **on at least 1,500 paired seasons** (revision 3.1). At 600 seasons a win rate's binomial SE is 1.5 points and even a perfectly balanced table passes 6/6 only about 30% of the time; at 1,500 paired seasons the SE is 1.0 [sim, K3 DOCTRINES.md].
- **The table is the kernel's, `permutation_rules::frontier::doctrine::DOCTRINES`, and it is provisional** (revision 3.1) until the mechanisms the simulator does not play land (M1–M3); those parts are **balanced by argument only**, kept small and inside the kernel's bounds (combat ×1.00–1.15, time and cost ×0.5–1, …):

| Doctrine | Tech bias | Unit variant | Civic power | Not simulated yet |
|---|---|---|---|---|
| A: Wardens of Stone | walls −5% cost (3.1; was −10%) | Pikes, drilled in Brace ×1.10 | heartland sieges need 12 more bells against it | heartland sieges (need War) |
| B: Tide | caravans ×0.8 time; Bourse fee 1% | light cavalry (Horseman) | +1 Waystone per City | **all of B's kit** (caravans, Bourse fee, Waystones); in the simulator B is a plain Horseman line |
| C: Ember | +5% damage on the arrival bell | shock infantry, drilled in Assault ×1.05 | war horn 24 bells | the war horn (no War in the simulator) |
| D: Verdant | Foraging: half the supply attrition | Rangers, drilled in Flank ×1.05 | Long supply lines: supply range +1 province | — |
| E: Lumen | Cartography: marches ×0.75 time (the proxy for the Engineers' roads) | Engineers, roads ×2 | Survey charters: Explore reveals the adjacent tiles | road building itself; survey charters |
| F: Iron | troop upkeep −10% | **heavy cavalry: the Horseman line with a variant weight ×1.10 in every stance** (3.1) | captured Free Cities keep their walls | — |

- **What changed in 3.1.** In K3's table B and F fielded the same army (a plain Horseman line; F differed only by upkeep and walls, and `validate_table`'s "no two identical" check passed only through B's unsimulated knobs). The kernel now has a `variant_bps` field and **`validate_table` refuses two doctrines with the same army** (unit line, drill, variant weight). F's Knight line stays out: with Knights F sat 0.17–0.30% of index below the mean in every K3 run [sim]. Tuned on paired seeds 20001–20150 (900 seasons; largest |Δ index| 0.045% with F ×1.10 and A's walls at −5%) and **confirmed on the untouched set 10001–10250: 6 of 6 in the band** — A 15.6%, B 15.7%, C 16.9%, D 17.4%, E 17.4%, F 17.0%; largest gap 1.1 points; largest |Δ index| 0.043% (SE 0.016); 1,500 of 1,500 seasons conserved [sim, m0c `dk_final_paired250.md`, K3 economy].
- **Gates (revision 3.1).** Per push, `frontier-sim`'s CI job runs a **180-season proxy** (30 paired seeds × 6 rotations at 10,000 wallets): every mean index within ±0.2% of the mean (≈ 4 SE) and every win rate within 16.7 ± 10; **three negative controls must fail it**, and do: the draft table (A −2.06%, Lumen +8%), F on the Knight line (F −0.219% on the gate seeds, which fails the ±0.2% bound only just) and an A boost that stays inside every bound (A +0.384%); the kernel table passes with its largest |Δ| at 0.072% [sim, m0c `dk_final_gate_controls.md`]. The gate catches an edge of ≈ 0.25% of index or more; the Knight edge (≈ 0.22%) sits at its limit, and edges under ≈ 0.15% pass it — which is why the nightly 1,500-season run (SE 0.016%) is the real check. The O5 band itself runs in the scheduled workflow `doctrine-balance.yml` (nightly and on demand): 6/6 on the fixed confirmation set, and the mean-index bound (±0.12%) on a fresh seed set each run, so a table tuned to the public CI seeds is caught. A gate row is added for each doctrine mechanism when it starts being simulated.

- **Homeland pull:** the first holding must be in the own wedge (§3.1), and the Heartland gives siege protection only there.
- **Membership is fixed for the season.**

### 4.2 Three tiers (E4), with Mandates (E1)

| Tier | Who | Chosen by | Powers | Scale bound |
|---|---|---|---|---|
| Citizen | every paid wallet | — | own holdings and hosts; vote for the Warden of each March where they hold a site; petitions; recall support; accusations | — |
| **Warden** | 1 per (March, faction) with ≥ 3 of that faction's holdings in the March | approval vote of those citizens per term; ≤ 8 candidates per tally; candidacy deposit; evictable by support after 24 h (H5) | March rally point; Colonist invitations; Levies; one local Mandate; **March-level truce or hostility** (below); a seat in the Assembly | ≤ 84 voters per tally |
| **Assembly** | all seated Wardens of the faction | — | elect Ministers; ratify or veto decrees within 24 bells; co-sign Minister recalls | **Warden weight = min(ballots cast in that Warden's last election, 40)** (revision 2). A Warden seat carries Assembly weight only with ≥ 5 distinct voters and ≥ 3 active days of tenure |
| **Ministers (4)** | General, Steward, Scholar, Envoy | Assembly approval vote per term, top-8 list | General: heartland war with a second Minister's consent; Steward: tax 0–15% of member production into the Granary, Granary spending ≤ 25% per term with a second consent; Scholar: research focus; Envoy: peace, NAP, alliance | 4 seats, O(1) tallies |

**Mandates** (E1 §8.3), revision 2:
- The faction's Mandate board has 8 slots; each Warden may post one local Mandate. Mandates are typed, on-chain-checkable objectives with an expiry, announced ≥ 6 bells before they apply.
- **A Mandate pays from a bounded budget, not a multiplier.** Each Mandate carries a laurel budget taken from the faction's **Mandate reserve** (10% of the faction's laurel emission in the Mandate's window), split among stakers who completed it. A wallet can earn at most 2× the average holding's term emission from Mandates per term. So officers redirect attention and a bounded share of laurels, never money beyond that bound, and cannot post a Mandate that pays their own bots beyond it.
- **Who the reserve pays (revision 3, O10): stakers who completed the Mandate, and nobody else.** Non-stakers who complete a Mandate earn Works (citizen pool) instead. **Kernel (K3, `frontier::mandate`):** `CloseTerm` moves the reserve balance into the term's budget; each staking completer claims `min(cap, budget × s_i / D)` in O(1); `SweepTerm` returns cap cuts, rounding and unclaimed shares to the reserve. Conservation is exact: `deposited = balance + Σ open (budget − paid) + paid + burned` [measured: property test over 200 random seven-term histories].
- **Share floor (revision 3.1, kernel `mandate::share_floor`).** The divisor is `D = max(S, ½ × the faction's stakers active in the term)`. If officers pick a Mandate that few can complete, each completer still gets at most `budget / D`, and the part the missing completers would have earned is **burned** (not returned, or it would swell the next term's budget until the few completers hit the cap). In an ordinary term more than half of the active stakers complete, so the floor does not bind [sim: the default seasons are bit-identical with and without it].
- **Mandates come from a fixed menu (revision 3.1) [design].** Every Mandate type is completable by **one qualifying action at any time in a window of at least 24 hours** (a march to a named province, a pledge, holding one hex at one bell of the holder's choosing, an exploration, a caravan). No type may require presence in more than one bell a day or a streak of bells. The menu is part of the ruleset hash, so officers choose *which* objective, never *how online* one must be.
- **Why (m0c review).** At a 1% bot share bots hold 61% of office-terms in the simulator (77% at 2%, 93% at 5%) because the most engaged wallet wins elections there [sim]. If bot Ministers could pick always-online tasks, a staking bot returned **1.04× at 1% when humans then complete a quarter as often, 1.64× when they cannot complete at all**; the share floor cuts the latter to 1.22× [sim, m0c `crit_floor_botmand*.md`]. The fixed menu removes the lever; the floor bounds what is left. Farms voting for each other are not modelled beyond "the most engaged wallet wins", which already hands bots most seats.

**Decrees** (war, peace, NAP, alliance, law): proposed by a Minister, consented by a second, ratifiable or vetoable by the Assembly within 24 bells, applied by a permissionless `ApplyDecree`. **War takes effect 36 bells after application** (the war horn).

**Relations (revision 2):**
- *Rivalry* (default): field clashes, raids and **sieges outside heartlands**. Dominion no longer needs a War decree, so a pacifist or captured Minister pair cannot stall a faction's Dominion path.
- *War*: adds heartland sieges.
- *Peace / NAP*: no attacks between the two factions; Granary bonds forfeited on breach.
- *Alliance*: allied hosts are friendly and may reinforce each other.
- *March truce / March hostility*: a Warden, with the consent of one other Warden of an adjacent March of the same faction, may declare a truce (no sieges inside the March for one term) or hostility (the March counts as at war for heartland rules, for one term) toward one faction. Ministers can override with a faction-wide treaty.

**The Granary** holds in-game resources only. It buys faction techs, March works (roads, walls), Frontier Grants for newcomers and diplomatic bonds. It can never be converted to USDC or paid to a person.

**Companies** (E3): ≤ 32 citizens of one faction, a captain, a channel, no treasury.

### 4.3 Anti-capture rules (H1–H5 at scale)

1. **No money to capture.** Granaries hold goods only; two Ministers and a 25%-per-term cap (H1, H4).
2. **No pre-season capture (H2).** Eligibility needs ≥ 1 active day in the last 3; the first term is a caretaker.
3. **Capped officer pay (H4), bounded against bots (O3; revision 3.1 adopts K3's ceiling).** The **steward pot** is 5% of each faction's two pools. Rows: 3 USDC per Minister-term; 0.5 USDC per Warden-term, paid only to Wardens whose election had ≥ 12 ballots; a person cap of 10 USDC per season. **If the rows exceed the pot, every row is paid pro rata** (factor = pot / Σ rows); **any unused part of the pot moves to the faction's citizen pool** (O9, as the K2 kernel does).
   - **Why a bound:** in the M0 simulator, when scripted wallets can win paid offices, a staking bot returned 1.52× at a 2% bot share and 1.22× at 5% [sim, M0 `RESULTS.md`]. Officer pay was the largest single source of the edge.
   - **The ceiling (K3, kernel `payout::office_ceiling_bps` = 9,500):** officer pay may raise a wallet's claim to **at most 95% of what the wallet paid**: `steward_i = min(rows_i × factor, max(0, 0.95 × paid_i − rest_i))`, where `rest_i` is the rest of the closed-form claim (§5.4). No wallet ends a season in profit because of an office, however many offices a farm wins; what the ceiling cuts is swept to the next season's pools (1.6–2.2% of the prize [sim]). **Officer pay is compensation for time, not income:** an officer who already gets back 95% or more is paid nothing more.
   - **Measured alternatives (K3, bots standing for office; bot + stake at 1 / 2 / 5 / 10%) [sim, ECONOMY.md §2]:** no bound 1.625 / 1.390 / 1.144 / 1.019; rows capped at 25% of what the officer paid 1.164 / 1.135 / 1.059 / 0.989; **laurels from the Mandate budget only (revision 3's office laurel rows) 1.110 / 1.046 / 0.982 / 0.939 — fails**; a break-even (100%) ceiling 0.989 / 0.986 / 0.966 / 0.937 (passes thinly, and it turns office into loss insurance: a losing bot officer is topped up to exactly 1.0×); **95% ceiling 0.962 / 0.959 / 0.948 / 0.926 (chosen)**. Revision 3's office laurel rows (D21) are therefore **dropped**.
   - **Who holds office.** Bots hold 61% of office-terms at a 1% bot share in the simulator (77% at 2%, 93% at 5%, 98% at 10%) [sim, m0c]; the ceiling bounds their USDC, the Mandate menu and share floor (§4.2) bound their steering. **Decided (D23, 2026-09-27): at most one office-term per wallet per season** (Minister or paid Warden; a by-election term and a term cut short by recall count; the caretaker first term (H2) does not [design, flagged for the owner]). It cuts the bots' share to 9.5% at 1% (72% at 10%) with the criterion still passing (0.967 worst, bots in office) [sim, m0c `crit_termlimit1.md`]; it also rotates human officers every term. **Vacancy rule:** if no eligible wallet stands, the seat stays vacant for the term (no pay, no Assembly weight), which small seasons (the M1 playtest, the 1,000-bot season) need. The simulator's default is now the limit (`Config::office_term_limit = 1`, vacancies counted), the kernel gets `office::GovernanceParams { office_terms_per_wallet: 1 }` with `may_stand`, and the Citizen reserves `office_terms_used` (M1, CL-31; re-run in §21.4).
4. **Recall (revision 2).** A Minister is vacated only with support from ≥ 10% of the faction's term-active citizens **and** Assembly co-signatures carrying ≥ 1/3 of the weight cast in that Minister's election. A Warden is vacated with support from ≥ 50% of the ballots cast in its last election. **One recall per office per term.** Recall never installs anyone; a by-election refills the office.
5. **Bounded boards with deposits (H5).**
6. **Proposer credit.** A petition adopted by a Minister credits the proposer and the adopter with Works points (not laurels). Voting earns nothing.

**Residual, stated honestly.** A well-organised guild can win its own faction's offices at low turnout; the scale study measured a 10% always-active bloc capturing 21–27% of office-terms at 30% turnout. What it wins is capped in money (steward rows), bounded in laurels (the Mandate reserve) and bounded in power. Sybil wallets cannot buy Assembly weight: weight follows ballots cast by distinct paid wallets, a March with one voter carries weight 1, and a recall now costs Assembly weight as well as citizens. The capture simulation is re-run with the real seat counts in M3.

### 4.4 Why it scales

- Each tally has ≤ 8 candidates; each vote is O(1) (**2.7k CU** [measured: `lab/E3-hybrid/e3-results.txt`]).
- Eligibility is read from the voter's own Holding and a per-(March, faction) roll on base. No census proof.
- At 50k players there are about 1,092 Marches and ≈ 255 Warden seats per faction [model: §H], so ordinary daily players can hold office.
- **A first-time player's civic acts** go beyond a no-stake vote: joining a Company, taking a Mandate, flagging a Colonist site, supporting a petition.

---

## 5. Economy

### 5.1 In-game resources and holdings (E4)

- **Resources (8):** Food, Wood, Stone, Ore, Horses, Gold, Science, Influence. Science and Influence can only be pledged.
- **Holding tiers:** Hamlet → Town → City → Stronghold (worked radius 1, 2, 2, 3).
- **Costs:** Eternum's quadratic duplicate rule `base × (1 + 0.5 (n−1)²)`.
- **Upkeep:** troops eat food and gold (`economy::unit_upkeep` reused).
- **Lazy evaluation:** `(stock, rate, cap, last_settled)` per resource; harvest = **7.0k CU** [measured: E4].

### 5.2 Moving goods

- **Caravans:** the recipient **pulls** on arrival. **Inbound cap:** 2× the recipient's own daily production per day.
- Caravans can be raided only at their destination province's bell.

### 5.3 The Bourse: a batch auction per bell (E4)

- Six pools at the Concord, each with **16 order shards** of 64 geometric price levels.
- `PlaceOrder` escrows goods or gold and adds quantity to **any shard the trader chooses** (revision 2; was `hash(holding) mod 16`), so locking one shard cannot keep a given trader out.
- `ClearPool(pool, b)` is permissionless after the bell: **206.6k CU** [measured: E4]. Fills settle lazily.
- Uniform price per bell means no sandwiching and no ordering MEV. **Residual:** the book is public while the bell is open, so a last-second order sees it; the uniform price and the cap of one order per holding per bell (size ≤ 2× daily production) bound what that is worth. Sealed Bourse orders using the march tlock machinery are extension X4.
- **Bourse volume counts for nothing** in scores or payouts (revision 2), so wash trading earns only its 2% fees back as a loss.
- Fees: 1% to the Engine fund and 1% burned.

### 5.4 Money: entry, pools, escrow and prize (revision 2)

**Why the money design changed.** Revision 1 paid 70% of each faction pot by honor up to a daily cap. Honor came from scriptable sources (explore, barbarians, damage, pledges), so a scripted wallet reached the cap every day and earned 1.87× its fee; farms stay profitable until they are ≈ 55% of wallets, pushing regular humans from 1.20× to 0.71× [model: `rev2-results.txt` §C1, reproducing the reviewer's `lab/redteam/rt.py`]. Any single pool that pays activity above the average transfers casual players' fees to whoever is most active, and automation is always most active. The fix is structural: separate the participation price from the contest, and make the contest a fixed, zero-sum emission.

**Entry.**
- **Citizen fee** `c(day)` (4 USDC at day 0, falling with days left to a 25% floor) — mandatory. Funds the **citizen pool**.
- **Laurel stake** `l(day)` (6 USDC at day 0, same schedule) — optional, addable until day 21. Funds the **laurel pool** and makes the wallet a **staker**.
- Both go into **8 vault shards** (SPL token accounts owned by the Season PDA; 6-decimal mint without freeze authority; owner and mint checked, from v9 WP12/WP17). `Join` writes vault shard `hash(wallet) mod 8` and JoinShard `(faction, hash(wallet) mod 8)` (C3). Payments are **escrowed** until the first holding settles (§3.4).
- 80% of each payment goes to its pool. **The operator's 20% is escrowed** and paid only after a successful Finish with every Shade revealed. On Abort the escrow funds full refunds. If any Shade leaf is unrevealed after 24 h, the escrow goes to the pools (§7.3).
- **Conservation invariant** at every step: `Σ vault shards == Σ paid + Σ operator escrows − Σ claims − Σ refunds − ops_paid − swept` and `vault ≥ outstanding + ops_unpaid` (F1). Checked on chain at every out-flow and by the verifier.

**Citizen pool** `C` (80% of citizen fees):
- **Civilisation Share** `c_civ = 5% + 2% per completed Engine stage` (5–15%) of `C`, split **in proportion to the citizen fee paid** among wallets whose Engine pledges passed a threshold (the builders). Fee-proportional, so it cannot be farmed by extra wallets.
- The rest is split among factions by the per-capita index with a square-root softening, `C_k ∝ n_k × s_k^{1/2}` (§5.6), so a winning faction's casual members earn more without the leader becoming a place for late passive Sybils (index clamp [0.5, 2] → multiplier 0.71–1.41; a passive Sybil in the leading faction still gets back < 0.8× its fee).
- Inside a faction, after the steward share: **90% by fee paid × tenure units** `u_i = 1 + 0.25 × min(1, active_days_i / (0.43 × days_available_i))`, and **10% by Works** (explore finds, barbarian loot, pledges, Levies, adopted petitions), capped per day. **Revision 3: the Works weight is linear and capped per USDC of fee**, `w_i' = min(Works_i, WORKS_PER_USDC × fee_i)` with **`WORKS_PER_USDC` = 105** (K3; was 140). Revision 2's square root rewarded splitting the same play across several wallets. In the simulator every bot reaches this fee cap [sim].

**Laurel pool** `L` (80% of laurel stakes):
- Split among factions `L_k ∝ Σ_stakers-in-k stake × s_k` (full per-capita index).
- Inside a faction, after the steward share, among **stakers only**, in proportion to **counted laurels** `λ_i`.
- **A late stake counts only laurels banked after it** (revision 3; K2 `counted_laurels`, test `late_stakes_do_not_reach_back`). Before the fix a wallet could play fee-only, stake on day 21 and have the whole season's laurels count; bots doing that returned 1.61× in the review's lab [sim].
- **The stake is priced by the laurel accrual still to come** (K3, O4 step 2: `pools::EntrySchedule::SEASON1`, ramp 2.0 fitted to the simulated world emission, a season parameter validated to ≤ 10.0): a stake bought while accrual is rising costs more. Bots that stake on day 21 now return 0.86–0.88×, against 1.04–1.14× in M0 [sim]. Its cost to honest late stakers is in §2.6 (D22).

**Laurels are a fixed, zero-sum emission (not minted by activity).**
1. **Holding emission (K3: order-weighted, O4 step 3).** Every non-dormant, non-occupied holding emits 1/12 laurel per bell × its order factor (first 1, second ½, third ¼) into its province (`laurel::emission_quarters`). The province's emission is split among the holdings in it by **strength weight** = tier weight (1, 1.3, 1.6, 2.0) × garrison factor (1 + garrison/2,000, capped at 1.5) × holding-order factor (first 1, second 0.5, third 0.25). Being stronger than your neighbours takes a larger share of *their* emission, not more emission. A farm of wallets in one province splits that province's emission among itself.
2. **Occupation.** An occupier takes 50% of the occupied first holding's share while the occupation lasts (§6.3). **Revision 3:** occupations follow the same pair rule as captures, and a fee-only (non-staking) or tied wallet transfers nothing: occupying it yields 0 laurels.
3. **Captures** (holdings 2–3 and Free Cities) transfer the holding and **25% of the victim's counted laurels** to the captor. Only the first capture between a pair of wallets pays in full; **the second pays 25% of the first transfer (6.25% of the counted laurels), then nothing** (O9, as the K2 kernel does). Nothing transfers, and no refugee kit is minted, when the pair has a prior history (earlier captures, caravans, delegation or shared Company) — so trading captures between alts yields nothing. Only counted laurels ever move between wallets (test `only_counted_laurels_move_between_wallets`).
4. **Sieges stake laurels.** Declaring a siege escrows 5 laurels; a failed siege pays them to the defender; a successful one returns them. Defence is paid, and siege spam costs. **Revision 3:** siege stakes follow the pair rule too, and a stake made from uncounted laurels is burned rather than paid to the defender.
5. **Relic Sites mint no laurels** (K3, O4 step 4, `laurel::RELIC_EMISSION_PER_BELL = 0`): restoring their revision-2 emission (1 laurel a bell to the holder) alone puts bots back at 1.04× at 2% [sim, ECONOMY.md §3] because bots are online every bell. They keep their Works and Dominion value; giving them another role is open (§14 D24).
6. **Mandate reserve:** 10% of each faction's emission, paid to **staking** Mandate completers in closed form with the share floor (§4.2, O10). No office laurel rows (dropped in 3.1).
- **No laurels for damage**, explorations or barbarians (those give Works in the citizen pool), so fighting your own alts mints nothing.
- Laurels are credited lazily with a per-province, per-faction **reward index** (the staking-reward pattern): the province stores cumulative laurels per unit of weight; each holding stores its weight and last index; any touch credits `weight × Δindex` to the Citizen and to FactionShard `hash(citizen) mod 16`. Weights change only on province writes (settle, muster, clash, capture). At T_end the indices freeze, and the banking window credits the rest (§8.4).

**Per-wallet cap:** 5× what the wallet paid (revision 2; was 10 f). Excess is swept to the next season's pools, never to the operator.

**Closed-form claim (no loop at the end).** Every credit is added when it happens to the Citizen and a FactionShard. `FinalizeFaction` sums 16 shards; `RevealShade` subtracts each Shade's recorded contributions (§7.3):

```
rest_i  =  C_k' · [0.9 · fee_i·u_i / Σ_k'(fee·u) + 0.1 · w_i' / Σ_k' w']   (citizen pool; w' = min(Works, WORKS_PER_USDC × fee))
         + C_civ · fee_i·[builder_i] / Σ'(fee·builder)                      (Civilisation Share)
         + L_k' · λ_i·[staker_i] / Λ_k'                                      (laurel pool; λ = counted laurels)
steward_i = min( rows_i × factor_k , max(0, 0.95 · paid_i − rest_i) )       (officer pay, ceiling 95% of paid, §4.3)
claim_i  = min( 5 × paid_i , rest_i + steward_i )
```

Primes mark values after Shade voiding. Claim cost [estimate ≤ 20k CU with the token transfer]. Every term is still O(1) per claim and `settle` is O(6) with no per-member loop [measured, K2 review].

**What the M0 simulator said** [sim, M0 `RESULTS.md` §D]: a scripted staking wallet at the SDK default returned 1.07× at a 1% bot share, 1.05× at 2%, 1.00× at 5%, 0.95× at 10%; the edge came from the Works pot, Relic Sites and the late-join effect; a "tuned" script was no better than the default.

**The O4 plan, as K3 ran it** [sim, `m0b/sim/ECONOMY.md` §3]: the four steps were tried in the owner's order and measured after each; **only step 4 passed** (step 1 alone, 70 Works per USDC: 1.021 barred / 1.101 in office; + step 2: 1.021 / 1.088; + step 3: 1.060 / 1.101; + step 4: 0.928 / 0.951). Backing off to spare honest players, **the shipped set is steps 2–4 with `WORKS_PER_USDC` = 105** (0.949 barred / 0.962 in office); steps 2–4 with 140 also pass, with 2–3 points of margin.

**Acceptance criterion for M3 (revision 3.1, sharpened again):** a scripted wallet returns **< 1.0× at 1%, 2%, 5% and 10% bot shares, taking the maximum over the bot's own choices** — its join window (day 0, 1–7, 8–14, 15–21), stake or not, office or not (`frontier-sim criterion --best-response --gate`, every bot of a season making the same choice so each cell holds the whole bot share) — **and in the "bot officers steer Mandates" variant**. The join page publishes the measured table either way.

**Where it stands** [sim, m0c, 3 seeds × 10k wallets per cell]:
- Default mix: **passes** (0.949 / 0.946 / 0.935 / 0.912 barred; 0.962 / 0.959 / 0.948 / 0.926 in office).
- Best response: **passes with a 2-point margin**. Worst cells: 0.980 (1%, days 1–7, staking, barred), 0.978 (the same in office); at 2–10% the best choice is a fee-only wallet joining on days 15–21 (0.969–0.974). The margin is thin enough that M3 must re-measure it on the final economy.
- Bot officers steering Mandates: **fails** without the menu rule (1.040 / 1.029 / 1.001 / 0.965 at 1 / 2 / 5 / 10% if humans complete a quarter as often; 1.22 / 1.20 / 1.14 / 1.06 if none can, with the share floor). The fixed Mandate menu (§4.2) is what makes this variant moot; it is argued, not simulated, until M3 models Mandate types.
- Cost to honest players: see §2.6 (late stakers) and §2.7 (skill premium).

**Money boundary.** USDC in: citizen fees, laurel stakes, the operator's escrows. USDC out: prizes, refunds, the operator's released 20%, Shade bounties. Nothing else.

### 5.5 Sinks and faucets

- **Faucets:** production, exploration finds, barbarian loot, catch-up kits, Frontier Grants. Rule-minted, never bought.
- **Sinks:** construction, upkeep, training, Bourse burns, storage decay, the Engine, Levies, forfeited bonds, siege repairs.
- The Engine's stage thresholds scale with settled holdings.

### 5.6 Faction scores (revision 2: per-capita, smooth, herding-damped)

Revision 1 scored each path as the faction's *share of civilization totals* with tier cliffs at 18/22/26/30/35%. That rewards size: six equal factions all sit on the floor, one faction 2 points above average jumps a tier, and the per-capita payout of a faction with 3× the members was 1.08× (or 1.83× if Knowledge is also share-scored) of a small faction's, not the 0.47× revision 1 claimed; that number was hard-coded in `model.py`, not derived [model: `rev2-results.txt` §D].

**New index.** `FoldFaction(f)` (hourly, permissionless) reads 16 FactionShards and writes `FactionState`. For each path p, the faction's **per-active-member** value is divided by the civilization's per-active-member value:
- *Dominion*: **March-control bells** (a faction controls a March in a bell when it holds ≥ 50% of the March's strength weight; `FoldMarch(m)` hourly reads the March's 7 provinces and adds to FactionShard `hash(m) mod 16`), plus captures. This rewards contiguous control rather than raw site counts.
- *Prosperity*: production.
- *Knowledge*: techs researched and science pledged.
- *Concord*: Engine contribution and treaty-days kept. **No Bourse volume, no trade.**

`s_k = clamp(mean_p(ratio_p), 0.5, 2) × h_k`, with herding damping `h_k = min(1, (1/6)/share_k)^0.6`. **Crisis** (V5) remains: from day 14 the top-2 factions by index pay +10% upkeep.

**Herding ratio** (per-capita payout of a faction with m× the members of each other faction, relative to a small one) [model: `rev2-results.txt` §D; β = economies of scale in per-capita performance, 1.0 = none, 1.4 = strong, e.g. Lanchester effects]:

| m | Revision 1 tiers | New, β = 1.0 | New, β = 1.2 | New, β = 1.4 |
|---|---|---|---|---|
| 1.2 | 0.83 | 0.91 | 0.95 | 0.98 |
| 1.5 | 1.00 (1.33) | 0.82 | 0.89 | 0.97 |
| 2 | 1.00 (1.50) | 0.72 | 0.83 | 0.95 |
| 3 | 1.08 (1.83) | 0.61 | 0.77 | 0.95 |

Joining a bigger faction never raises expectation in the new index, and the picker's live number moves smoothly.

**M0 result and decision (revision 3, O6).**
- **β measured in the simulator: 0.86** at 2× and 3× size [sim, `RESULTS.md` §C]. By path: Dominion 0.44–0.55, the others 0.97–1.00. Dominion favours small factions because rings open when the crowded wedge fills, so small factions spread thinly and control more Marches per member.
- **Herding ratio at m = 3:** 0.910 at γ = 0, 0.786 at γ = 0.3, **0.680 at γ = 0.6** [sim]. So the criterion (≤ 1.0 at 3× size) holds for every γ ≥ 0 in the simulator.
- **Caveat:** simulated agents do not coordinate; Companies, Mandates and delegated command are exactly the coordination that could raise real β. The minimum γ is 0.26 if real β is 1.2, 0.53 at 1.4 and 0.81 at 1.6 [estimate].
- **Decided: γ = 0.6** until M3 measures β with real players. The kernel stores γ as a fraction, so it can be changed per season (fixed at `CreateSeason`). `index::pow_frac` documents `num ≤ den` while γ > 1 behaves correctly; the doc or an assert must state which is intended (kernel note).

---

## 6. Combat and fairness

### 6.1 Hosts

- A host is up to 30,000 troops of one unit type (the 8 types of `units.rs` minus Settlers, plus the faction's doctrine variant). Stamina (cap 120, lazy), a battle cooldown.
- **Minimum host size 100 troops**; each march costs stamina. **Scouts** explore but do not fight: a civilian host (Scout, Settler) never engages, contests a hex, holds the field or counts for siege progress in either direction (it neither advances a siege nor pauses one as a defender), and it withdraws when its faction loses the hex or when it is hostile to the side that holds it (revision 3; kernel CL-10, M1 W1-A D3).
- **Hex capacity:** ≤ 6 hosts per hex. **Fair-share rule, allocated by side (revision 3):** a *side* is a connected group of mutually non-hostile factions (allies, or factions at Peace/NAP) present on or arriving at a hex. At resolve each side is guaranteed `min(its hosts, ⌊6 / sides present⌋)` slots, and the rest are filled by mass. On **a holding's hex the owner's side always keeps 3 slots**, with the owner's own hosts first, and the hostile sides together share at most 3. Allocating by faction (revision 2's wording) let two allied factions take two shares against a third; by side they share one (kernel test `allies_cannot_pool_hex_slots`). Bounced hosts return home with no loss.
- ≤ 48 hosts per province, ≤ 8 per faction per province (residents). Province caps are recounted after the hex fair share, in **one pass**: arrivals the caps refused are re-admitted in mass order only where the recounted room and a free hex slot allow, never displacing a host the fair share admitted, so the counts are stable after it (kernel CL-10, M1 W1-A D1). A garrison never exceeds 30,000 troops either: a reinforcement past it is refused (M1 integ-W1).
- A marching host is stored in its owner holding's transit slot (4 per holding).

### 6.2 Marching with a sealed destination

- **`Depart(host, commit, seal, arrive_bell, tip)` (revision 3.1):** writes the owner holding, the origin province and the Citizen. The instruction data carries the **32-byte commitment and the 165-byte seal**, so both are public in the log, and **the program computes `seal_root = sha256(commit ‖ sha256(seal))` itself** (a client-supplied root would let the logged seal differ from the committed one). The commitment stays hiding: its salt comes from the seal's 16-byte key `k`, which nobody knows before `T(b)`. The arrival bell is public; the direction is not. **5.0k CU** [measured: E4] plus the seal; the transaction is ≈ 699 B [estimate: 667 B measured in S-TLOCK + 32 B]. The seal is a tlock encryption of the plaintext to the drand quicknet round **at the end of the arrival bell** `T(b)`. The Holding's transit record also stores the host's **departure mass** (troops at Depart), which the destination's quota ranking uses (below). Depart escrows in the holding a **march fee** of 10,000 lamports, a **seal bond** (default 20,000 lamports [design]) and a **reveal tip of at least `tip_min`** (M1, CL-22 / I-08: the minimum is stated in priority terms and Depart refuses less with `TipTooLow`, §6.4; players may pay more). **M1 as built (contract §5.11):** the instruction data is 219 B (host, commitment, seal, arrival bell, tip, transit slot); Depart charges the **maximum** march stamina `march_stamina(32)`, since the path is sealed (I-32); it refuses a host that appears in any unsettled transit of its Holding (`HostInTransit`, I-44), and so do Dissolve and Explore; worst **23,167 CU** on the release `.so` (limit 24,500) [measured, G1].
- **The seal (revision 3; pinned in rules v10 `frontier::seal`).**
  - **Mandatory** on every Depart and every CommitPosture, so after `T(b)` any keeper can reveal any order; the owner can no longer keep a valid order back by not handing it out.
  - **Format:** a 165-byte compact block: quicknet, 16-byte key, tlock's standard 128-byte IBE block (U 96 B, V 16 B, W 16 B) plus a 37-byte SHA-256-keystream body, `salt = sha256("PS-SALT" ‖ k)`. The scheme, DST and hashes are pinned in `frontier::seal`; only the binary form is accepted, never armored age.
  - **The 37-byte plaintext (pinned, revision 3.1) [design]:** `version u8 = 1 | host_id u64 | arrive_bell u32 | dest P i16 | dest Q i16 | dest tile u8 | stance u8 | retreat_bps u16 | path_len u8 | path 12 B (≤ 32 steps × 3-bit hex directions) | reserved 3 B = 0`. A posture's plaintext uses the same length with `version = 2 | host or garrison slot | bell | stance | reserved`.
  - **Commitment (pinned):** `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plaintext37 ‖ salt)` (postures: `"PS-FRONTIER-POSTURE-v1"`), as the SP-V2 opener checks it [measured, SP-V2 `seal.rs`]. Revision 3's field list (`domain ‖ host_id ‖ dest … ‖ path ‖ salt`) is replaced by this fixed layout, which is what fits the 37-byte body. Depart stores `seal_root` in the existing 32-byte commit field (no account growth). Reveal passes the plaintext, the salt and `ct_hash = sha256(seal)` (+32 B) and stays free of pairings.
  - **Custom cryptography:** the compact envelope is ours, not tlock's stock format. It needs a formal specification and an outside review before M4, and a property test that the on-chain verdict equals stock tlock decryption plus the commitment opening.
- **`retreat_ratio`** is a conditional order everyone can use: "withdraw without fighting if the frozen defending strength on my target hex exceeds r × mine". It is evaluated against the roster frozen at the start of the bell (§6.3), so the sophisticated player's abort option is available to every player in advance.
- **Arrival timing (revision 3, kernel `MIN_ARRIVAL_LEAD_BELLS`).** An arrival lands **no earlier than its departure bell + 2**. Its troops and stamina are **the host's values after the origin's clash of the departure bell** (`Host::march_values`), so a host cannot fight at its origin in bell b and again at its destination in bell b + 1 with the same troops. **Revision 3.1: only GatherClash and the resolve wait for the origin, never Reveal.** Revision 3 had Reveal read the origin's result; since Reveal refuses after `A + W`, an origin Province (or the origin region's anchor or caches, possibly in another region) held past the destination's close would have routed every arrival from it — an outcome change priced at one account (≈ $3.9k per 10 min at the default tip [model, SP-FEE]). Now: the origin keeps each departed host's post-clash values in that host's resident entry (flagged *departed*) until `SettleTransit` of that transit frees it, so the values survive any destination lag; `GatherClash`, which has no deadline, reads them for each gathered arrival (≤ 10 arrivals and their ≤ 10 origin Provinces per gather, so ≤ 3 arrival gathers per province-bell), and `ResolveFromInputs` refuses until every arrival's values are gathered. Lag at the origin then only waits. The kernel test `arrivals_carry_the_origin_clash_whatever_the_lag` fails if the check is removed [measured]; the program-level lag test holds the origin Province and the origin region's anchor past the destination's close and checks the result is unchanged (M1 gate, §9.4). **M1 moves the values into the Holding (I-11, D4):** after the origin resolves the departure bell, the class-D **`SettleDeparture`** copies the host's post-clash troops and stamina from the origin's departed entry into the Holding's transit record and frees the entry; `GatherClash` then reads **Holdings** (≤ 10 per gather), not origin Provinces, and refuses an arrival whose departure is unsettled (`DepartureUnsettled`). The keeper's order is resolve(origin, departure bell) → SettleDeparture → gather(destination). The lag gate G7 holds the origin Province, its anchor and the SettleDeparture past the destination's close and finds the destination's result byte-identical (`g07_lag_gate_in_litesvm`, four cases) [measured, M1 Gate W5].
- **`Reveal`:** anyone holding the plaintext may call it from the start of bell b (owner) or from `T(b)` (anyone, via the seal) until the **reveal close** `A(b, r) + W`, where `A(b, r)` is when the beacon for `T(b)` was first anchored on chain for the province's region r (§8.5). Reveal **refuses once `Clock ≥ A(b, r) + W`**, and also once the region's BeaconLog shows any round ≥ the bell's seed round `S(b, r)`. **One-way latch (M1, CL-23 / I-07):** Reveal also takes the province-bell's canonical ClashInputs address read-only and refuses (`LatchClosed`) unless it is absent, and refuses once the destination Province has resolved or skipped past the bell. So "no Reveal after the first gather" is structural, not only a consequence of the Clock (Solana's Clock is a stake-weighted estimate); the check is one absence proof (≈ 0.5k CU, +32 B). **Revision 3.1:** the BellAnchor's address is predictable (§8.2), so when THE anchor is absent Reveal proves it and treats the window as open — **unless the region-day AnchorArchive tombstones the bell**, which it does once the anchor was archived and closed (§8.4). Reveal checks the commitment against `seal_root`, walks the path (≤ 32 steps over ≤ 4 provinces, read-only), **takes the ranking mass from the Holding's transit record** (the departure mass; it never reads the origin Province), and **writes exactly one account: an ArrivalSlot** at the with-seed address derived from the Season PDA and `(province, b, faction, i)`, `i ∈ 0..3` (§8.2). The Holding, the BellAnchor (or the AnchorArchive) and the other slots are read, never written. **17–26k CU** including slot creation [measured, S-SIZE, SBPF v0]; the slot-creation part with the anchor and archive checks is 6.6k on v2 [measured, SP-V2 m0c]; the full Reveal is measured in M1. **M1 as built (contract §5.11, I-06, I-10):** Reveal takes the plaintext, the **salt** (not the seal key) and `ct_hash`; it writes the ArrivalSlot and, **on the first reveal of a province-bell in its day, one bit of that day's `ArrivalDay` bitmap** (creating the 96-B account if absent), which is what lets a quiet bell be skipped without per-bell absence proofs (§6.3). **Measured on the release `.so` (G1, the §13.1 fill: 32 steps over 4 provinces, displacement of the smallest of 4 full slots, first reveal of the province-bell): worst 25,155 CU** (named slot 24,300, displacement 20,350), 916–928 B, `L(reveal)` = 1,146,880 B; the requested limit is 26,500 (worst + 5 %). In play the 18 Reveals measured so far (two test-key runs and the first archive run with real rounds) used 17.4k–21.4k CU of the program's own units, and the svm suite's 126 Reveals 14.7k–25.2k units per transaction (distribution and sources in §22.2) [measured].
- **Quotas by mass, not arrival order (revision 2).** Each faction has 4 ArrivalSlots per (province, bell). **One arrival per citizen per province per bell.** A reveal into a full set may **displace the smallest same-faction arrival if it is larger** (one displacement per reveal; the displaced host bounces home with no loss). So a spy inside a faction cannot squat its slots with 100-troop hosts, and the result does not depend on who revealed first (A3: quotas are settled at reveal; the final set is the 4 largest). **The mass is the departure mass** (revision 3.1), known at Reveal without the origin. Kernel (K3): `clash::admit_arrival` / `quota_set`, rank = (mass, `slot_key`); property test `arrival_slots_are_the_four_largest_regardless_of_reveal_order` [measured, 400 cases × 8 orders].
- **Not revealed by the reveal close:** the host is **routed** — it returns home, **loses 50% of its troops**, its stamina and its tip. With a mandatory seal and a tip, this only happens when no keeper reveals (M1 removes the zero tip: every Depart pays at least `tip_min`, §6.4). The verifier decrypts every seal after its round and **flags every valid seal that was not revealed** as a keeper-liveness finding. **Quota-refused and displaced arrivals are not routed (M1, I-12, D5):** at settlement an arrival that is not in the final set is ranked by `(departure mass, slot_key)` against its faction's recorded final set; if the fourth arrival outranks it, or its citizen already has a higher-ranked arrival there, it **bounces home with no loss**; only an arrival that would have made the final set had it been revealed is routed.
- **A bad seal is proven, not priced (revision 3).** Revision 2 could not tell a valid ciphertext from garbage on chain, so a script could post garbage and "trade 50% of a host for information".
- **Settlement is the seal proof (M1, I-44, O-M1-19; replaces revision 3.1's ProveBadSeal and SealVerdict below).** Revision 3.1 let `SettleTransit` run once the reveal close + 1 bell had passed with or without a verdict, and `ProveBadSeal` refused archived bells, so an attacker who held the proof (one account, at the keepers' fixed low bid) past settlement kept a garbage-sealed host alive; and nothing stopped a host whose transit was unsettled from marching again. In M1:
  - **`SettleTransit(holding, transit_slot, commit, seal, beneficiary)`** (class D, anyone) takes the logged commitment and seal, checks `sha256(commit ‖ sha256(seal)) = seal_root` (so only the logged pair can be judged), reads the round-`T(arrive)` signature from THE BellAnchor or, once archived, from the region-half-day **AnchorArchive, which now stores each bell's signature**, opens the seal with the FO check, recomputes the commitment and validates the plaintext (`Plain::validate`). Any failure is a **bad seal** with a code (1 FO failure, which includes a seal to another round; 2 a U point that does not decode; 4 commitment mismatch; 5 invalid plaintext, wrong host or arrival bell included; 3 "wrong round" is reserved and never emitted, since it cannot be told from 1); code 0 is a valid seal whose plaintext is the march's order, revealed or not. The proof cannot be skipped, raced or held separately, and a late settlement (after archive) still judges. Cost: the opener ≤ 48.0k inside a budget of 85k; **worst 63,281 CU** on the release `.so`, tx ≈ 990 B [measured, G1]; the verifier's G10 compares every code with the stock `tlock` crate before and after archive [measured, M1 Gate W4/W5].
  - **A bad seal destroys the host whatever happened at the destination:** a host that stayed gets the pending op `Forfeit` (its troops are lost at the settle of the current bell; past clashes are never re-run); a bounced or retreated host is not returned; **tip, march fee and bond go to whoever settles** (the prover's reward of revision 3, now paid to the settler).
  - **Hosts in transit cannot act (I-44):** Depart, Dissolve and Explore refuse a host that appears in any transit record of its Holding in state 1–3 (`HostInTransit`); a host that stayed at its destination can act once its settlement landed.
  - Tag 0x53, the `sv` seed tag, the `PSF1SVRD` magic and error 14 are reserved; **17 account kinds** remain (§8.2). The bullets below record revision 3.1's design, which M1 does not build.
  - *(revision 3.1, superseded)* **`ProveBadSeal(holding, transit_slot, commit, seal, anchor)`** (revision 3.1: takes the commitment) is permissionless. It checks `sha256(commit ‖ sha256(seal)) = seal_root` in the transit record (so only the logged pair can be judged), reads the round-`T(b)` signature stored in any region's BellAnchor for that bell (the tlock round is the same for every region), opens the seal on chain **with the FO check**, compares the opened plaintext's commitment with `commit`, and writes exactly one **`SealVerdict`** account (96 B, with-seed address per (host, departure bell)). Cost **≤ 48.0k CU, 588-byte transaction** [measured, SP-V2 v2: 44.4k for a valid seal, refused; revision 3 said 43k / 488 B]. Without the commitment in the instruction, a garbage seal (the case the instruction exists for) could never be proven, because nobody but the owner would know `commit`. If no anchor exists, verifying the signature inline would cost ≈ 330k CU on SBPF v2 [measured, S-BEACON], so the proof waits for the anchor.
  - **The FO check stays** (+9k CU; M1 keeps it inside SettleTransit). Without it the chain would call some seals valid that every stock tlock library rejects; keepers could not auto-reveal them, which would rebuild the withholding option.
  - *(revision 3.1, superseded by settlement as the proof)* **`SettleTransit` reads the SealVerdict:** a proven bad seal **destroys the host** (revealed or not) and pays the prover the tip, the march fee and the seal bond. Past clash results are never changed. SettleTransit is allowed only after the reveal close + 1 bell, which gives provers at least 20 minutes from the anchor.
  - Postures use the same seal and proof: a bad posture seal becomes **Disarray plus forfeiture of the posture tip and bond** (postures are M3; M1 builds none, I-16).

### 6.3 The clash at the bell

**Roster freeze (revision 2; semantics made exact in revision 3 to match the kernels).** The combat roster of province p for bell b is the set of hosts and garrisons present **at the start of bell b**. Muster, merge, split, Depart (including its stamina), Dissolve, garrison changes, finished Walls items and decrees issued during bell b are **pending changes that take effect after bell b's clash** (`Host::settle`, `settle_merge`, `GarrisonState`, `Holding::walls_at(bell_start)` / `commit_walls`, `RelationsLog`): a departing host still fights at b, a mustered host joins at b + 1, a wall finished mid-bell counts from b + 1, peace takes effect at b + 1 and war after its 36-bell horn. A host holds at most one pending change and a garrison at most two pending bells. So seeing revealed destinations during the bell changes nothing about that bell's clash (test `actions_during_a_bell_do_not_change_its_clash`, which a control that applies the same actions immediately fails [measured]). Postures for bell b must be committed before bell b starts.

**What the resolver reads (revision 3).** The Province **mirrors each site's garrison, walls and owner faction** (written by Muster, garrison changes, wall completion, capture and settle), so resolution never reads Holding accounts: with 12 Holdings the transaction is 1,548 B, over the 1,232-B limit [measured, S-SIZE A9]. Postures do not fit either: 24 ArrivalSlots plus 3 PosturePDAs is 1,251 B, and a v0 lookup table overflows the 64-lock limit at 36 postures [measured, S-SIZE A11–A12], while up to 48 defending hosts may post one. So resolution has two steps:
- **`GatherClash(p, b, part)`** — permissionless, after the region's reveal close, with no deadline. It reads a batch of the bell's ArrivalSlots (with, from revision 3.1, each arrival's origin Province for its post-clash values, §6.2) and PosturePDAs (with canonical absence proofs for the empty ones) and writes them into **one `ClashInputs` account** (1,024 B) per province-bell. It is idempotent and order-independent: gathering a part twice, or parts in any order, gives the same ClashInputs [measured, SP-V2 `t2b`: reverse order then all again, repeats are no-ops at 6.4k–10.1k CU]. **≤ 30.3k CU, 1,187 B, 32 locks per gather** [measured, SP-V2 v2, 24 positions per gather]; with origin Provinces in the arrival gathers, arrivals go ≤ 10 per gather. **M1 as built (I-11, I-46, contract §5.11):** a gather reads ≤ 24 slot keys and ≤ 10 **Holdings** (the departure values SettleDeparture put there, §6.2), writes only ClashInputs (1,280 B), stamps each gathered transit with its destination (so a bad seal settles only there), refuses a bell the Province has already resolved or skipped (`LatchClosed`: the tombstone rule for inputs), and takes a **one-transaction fast path when the ArrivalDay bit of the bell is clear** (no arrivals); a full province-bell needs ≤ 3 gathers. **Worst 46,551 CU** (budget 49,000), ≤ 1,232 B [measured, G1, release `.so`].
- **`ResolveFromInputs(p, b)` — the primary path (revision 3.1).** Permissionless once every position is gathered and the bell seed `S(b, r)` is in a SeedCache (§8.5). It **refuses unless every position is gathered**, reads ClashInputs, the Province, a SeedCache and **THE BellAnchor** (added in 3.1 for defence in depth: the resolve then checks the cache's A against the anchor itself, +32 B), and **writes only the Province**. **Worst found 540.9k CU; 358 B and 7 locks without the anchor key, ≈ 390 B with it** [measured, SP-V2 v2, full gather over 52 adversarial fills; M0's stand-in kernel said 262k]. The digest equals the native kernel's in every case. **M1 as built (I-13, I-14, I-15, I-43, I-56; contract §5.11):** ResolveFromInputs writes the Province **and the same province-bell's ClashInputs** (its fate table, delay-only, same stream: D6); it runs the **Phase A** clash kernel (digest-identical to the reference, 4,320-input equivalence) with the full M1 write-back, the camp as a neutral garrison with its daily respawn check, and the **storage-aware room**: arrivals are admitted only while residents + pending musters < 48, < 8 per faction and within the free entries of the 56-entry Province, the rest bouncing without loss, so a Province can never overflow (R1 of the M1 review). **Worst 327,609 CU over all 1,240 adversarial fills with the full write-back** (budget 340,000), heap peak 15,320 B of the 28-KiB gate [measured, G1, release `.so`]. **Phase B** (one hash per engagement, `CLASH_VERSION` 3) was adopted at the wave-5 gate after the doctrine proxy gate, the 1,500-season band (6/6) and on-chain = native over 1,244 fills re-passed: RFI max **274,007 CU**, gate 290,000 (committed by W6-B). The worst-case clash budget is **410k** from contract v1.10 (≤ 3 gathers × 40k + ResolveFromInputs 290k; 460k was the Phase A bound), not revision 3.1's 650k.
- **Demoted:** the single-transaction **`ResolveClash(p, b)`** (24 ArrivalSlots in the transaction, postures through GatherClash) costs **367k–633k CU, 1,150 B (1,182 B with the anchor key), 31 locks** [measured, SP-V2 v2; worst found 632.6k] and leaves 82 B of headroom and a 31-account lock surface on the critical resolve. It is kept only as a test oracle (it must give the same result as the two-step path), not as a keeper path.
- **Budget: 650k CU per worst-case clash** (four gathers ≤ 30.3k + 540.9k ≈ 662k measured on the full-gather path; the worst case is the highest of 40 hand-built and 12 screened adversarial fills, not a proven bound). **Heap peak 25,872 B of the default 32 KiB, a 21% margin** [measured, SP-V2]; any kernel growth needs a `RequestHeapFrame` (+5 B). **M1 kernel optimisation targets** [measured shares, SP-V2 `t5b`]: the field-holding step (39% of kernel CU, 116k–216k; its withdraw search scans withdrawing × 6 neighbours × units) and the engagements (42%, ≈ 1.8k CU each: three sha256 plus `resolve_engagement`); per-faction strength sums.
- **Absence proofs** use the canonical with-seed address of every slot and posture position; a resolve that omits an existing slot must fail (test, §9.4). With with-seed addresses 24 absence proofs cost ≈ 12k CU against ≈ 96k with PDA derivation [measured, S-SIZE A5].
- **No inline beacon verification** (revision 3). With 24 slots and the beacon in instruction data the transaction is 1,427 B (evmnet) or 1,476 B (quicknet), over the limit [measured, S-BEACON]. Seeds come only from SeedCaches, which anyone can create (§8.5).

Resolution steps (unchanged):
1. Merge arrivals by the hex fair-share rule (by side, §6.1), in `tie_key(S, host_id)` order for ties.
2. Apply `retreat_ratio` orders against the frozen roster.
3. On every hex with hostile hosts, each host engages each hostile host with `combat::resolve_engagement` (reused unchanged), **from pre-clash counts**, damage split evenly over its targets. **Both halves of `resolve_engagement` are kept** (revision 3 fix): the garrison (`Combatant::City`) never attacks but retaliates at ×0.5, and a ranged defender retaliates at ×0.5, as in v9.
4. Apply all damage at once. Hosts below 0.5 troops are destroyed.
5. **One side holds the field;** the other withdraws to an adjacent friendly hex or bounces home.
6. **Damage-ratio refund:** a side whose damage ratio is ≥ 10 pays no stamina.

**Quiet bells (revision 3; kernel `is_quiet`).** A bell is *quiet* for a province when it has no arrivals and no hostile combatants share a hex (and no hex is over its fair share). Resolving a quiet bell changes nothing, and a siege counts a run of quiet bells in closed form (`Siege::advance_quiet`, property-tested equal to bell-by-bell). Hostile residents sharing a hex (besiegers next to a garrison with troops) fight every bell, so such a bell is never quiet. **Open item for M1:** the chain still has to prove "no arrivals" for each skipped bell, which means absence proofs for that bell's slot addresses (≈ 73k CU per bell at 24 absent slots with residents [measured, S-SIZE A5], one bell per transaction). A province nobody touched for a day therefore needs up to 144 such resolves before a resident can act (≈ 10.5M CU, ≈ $0.11 of base fees [model, `rev3-results.txt` §G]). M1 must pick a cheaper proof (for example, a per-origin record of departures arriving at each bell, read from the few provinces a march can come from) or accept and budget this cost. **Resolved in M1 (I-10, O-M1-02; contract §5.11):** the first Reveal of a province-bell sets one bit in that province's **`ArrivalDay`** bitmap for the day (96 B, 144 bits), so "no arrivals at bell b" is one bit, not 24 absence proofs. **`SkipQuiet(b0, n ≤ 24)`** advances a Province over a run of bells whose bits are clear and whose roster is quiet by the kernel's `is_quiet`, applying the pending changes of each bell; it is a work-bounded instruction (at most one full kernel quiet test per transaction; a later bell that is not trivially quiet ends the run and commits the prefix, and the day's camp check is provisional at the stop), budget 90,000 + 30,000 CU per recomputed bell. Equivalence gate G11: a skip equals gathering and resolving each bell, byte for byte, including rosters changed at every bell [measured, M1 Gate W5]. In the test-key nightly (100 bots, 1 game day) SkipQuiet used p50 42k / max 56k CU and an idle province-day needed a median of 6 skip transactions (max 40 on churned provinces) [measured, `nightly-20260928`]. A province untouched for a day therefore costs ≈ 6 transactions of ≤ 90k CU instead of ≈ 144 of 50–73k.

Outcomes are recorded in the Province's result ring; `SettleTransit` (the owner, or anyone, after the reveal close + 1 bell) later reads it and any SealVerdict, updates the Holding, pays the tip and march fee to the revealer and resolver (or to the prover for a bad seal), closes the ArrivalSlot (rent back to the revealer) and **frees the departed host's resident entry in the origin Province** (revision 3.1, §6.2; non-critical, so it may write three accounts). `ClashInputs` is closed by a later non-critical crank, rent back to the gatherers. **M1 as built (I-12, I-44, I-48, I-49, I-52):** outcomes are the fate table in ClashInputs; SettleTransit (class D) **opens and judges the seal itself** (§6.2), settles by rank against the final arrival set (bounce without loss, or route), pays the tip to the slot's beneficiary and the march fee to the resolver, and sends the bond back to the Holding's rent payer; a payment that would leave a drained recipient below rent goes to the Holding's `pool_owed`, swept later to the DefencePool by the class-N `SweepPoolOwed`, so **no settlement writes the DefencePool**. A slot whose evidence is claim-eligible stays open for a **6-bell claim grace** (ClaimDefence, §6.4); every other slot closes to the fee payer that paid its rent. The origin entry is freed earlier, by SettleDeparture. ClashInputs close once every present record is settled and the grace (`clash_close_grace` bells) has passed, rent to the fee payer that created them. **No postures in M1** (I-16): every resident fights at Hold, and the posture area of ClashInputs is reserved.

**Stances:**

| Matchup | Effect |
|---|---|
| Assault > Flank > Brace > Assault | the winner deals +20% damage |
| Hold (default doctrine, public) | no modifier either way |
| Disarray (a posture committed but not revealed) | deals ×0.6, takes ×1.25 |

A posture is committed into its own **PosturePDA** (one per defending host or holding; with-seed address from the Season PDA and `(province, b, host_slot)`, revision 3) before bell b, sealed to `T(b)` with the same 165-byte seal as a march, with a tip and a bond, and revealed like an arrival. **Disarray stays** (revision 2 rebuttal): defaulting an unrevealed posture to Hold would give every script a free last-look option worth +6.7% of damage (reveal a winning stance, withhold a losing one), while Disarray makes withholding strictly worse than revealing a losing stance [model: `rev2-results.txt` §B]. The kernel test `stance_table_is_in_equilibrium` checks that no stance is dominated and that Disarray is strictly worse than revealing any stance [measured; runs in the CI job]. What the review rightly found is that Disarray was *cheap to force* by locking a shared account; per-host accounts, tips and region-anchored windows make it expensive (§6.4). A bad posture seal is proven like a bad march seal (§6.2).

**Bounded by construction:** ≤ 48 hosts, ≤ 6 per hex, ≤ 240 ordered engagements, ≤ 24 arrivals. The real rules-v10 kernel alone costs **316k–510k CU** over 40 adversarial fills, the whole single-transaction resolve **367k–633k** [measured, SP-V2 v2; revision 3's 247–270k came from M0's stand-in kernel]; honest **18.5k CU** and refused before the seed at **1.7k CU** [measured: S-SIZE and E4, SBPF v0, not re-measured]. `clash::validate` must also refuse troops above `MAX_HOST_TROOPS`, an unbounded `dealt_bps` and stamina above the cap (kernel fix before the M1 gates).

**Sieges and capture (revision 2: slow and alertable).**
- A holding's garrison and walls fight as a virtual host (`Combatant::City`).
- A **siege** is declared publicly (the siege horn, visible to the owner, the March and the faction) and stakes 5 laurels. Siege progress rises by 1 per **resolved or quiet** bell **only while the besieging (declaring) faction holds the holding's hex** and no defending host is present (revision 3: before the fix any hostile faction's presence advanced another faction's siege; kernel `GarrisonResult.holders`, `BellReport.holds`), **except during the owner's vigil hours**, when progress pauses. Completion needs **36 + walls/50 bells** of progress: at least 6 hours outside the defender's vigil, so an attack launched at bedtime cannot complete before the defender wakes.
- **Vigil hours** are an 8-hour daily window chosen at join. **Revision 3:** a change takes effect at the first UTC midnight at least 24 h after the request (at most one change a week). Changing it at an arbitrary time could create a single vigil of about 16 hours [measured, K1 review].
- **Walls are bounded** (revision 3): walls, production and upkeep deltas get caps, so one large Walls item cannot make a holding immune to sieges (kernel fix before the M1 gates).
- **Auto-reinforce:** a standing order lets friendly holdings in the same March send up to 25% of their garrison to a besieged holding automatically. The kernel returns every eligible donor while a faction has 4 arrival slots per province-bell; the rule is that **the 4 largest shares go** (as the simulator does), and the kernel or program must say so.
- **Raids** (no siege) loot at most 10% of a holding's stock, at most once per 6 hours per holding.
- On completion the attacker may:
  - **Occupy a player's first holding:** takes 50% of its laurel emission share and 20% of its production as tribute while keeping a host in the province; the owner keeps playing and can **Liberate** by winning a clash there. First holdings are never transferred.
  - **Capture holdings 2–3 and Free Cities:** the holding and 25% of the victim's banked laurels transfer (subject to the pair rules of §5.4); the previous owner gets a refugee kit unless the pair has a history.
- Field clashes keep their 10-minute bells; only captures are slow.

### 6.4 Front-running, last-look, blocking and latency

| Vector | Design answer | Residual |
|---|---|---|
| Latency race inside a bell | Order-independent resolution; per-bell lottery for sites, explorations and trades | — |
| Defender reacts to revealed destinations | **Roster frozen at the bell start**; postures close before it | — |
| Seeing the enemy's orders before committing | Commitments with 32-byte salts; tlock opens only at the bell's end | — |
| Withholding a reveal after seeing others (last look) | Mandatory seal, so any keeper reveals a valid order after `T(b)`; `retreat_ratio` for everyone; **a bad seal is judged on chain and destroys the host** (M1, I-44: `SettleTransit` opens every logged seal, also after archive; revision 3.1's `ProveBadSeal` is removed); an unrevealed valid seal is routed at 50% (arrivals) or Disarray (postures) and flagged | **M1: no zero tip** (Depart refuses a tip below `tip_min`, CL-22), so "hope nobody reveals" needs every keeper to ignore a paid tip. A bad-seal host keeps fighting until `SettleTransit` destroys it (past clashes are never re-run). X2 (Private-ER reads) would close the remaining option |
| Quota squatting by spies or allies | Mass-based displacement, one arrival per citizen, hex fair share **by side**, owner's side keeps 3 slots on its holding's hex | — |
| Offline player | Seal with keeper tips, delegated command, Hold doctrine, vigil hours, auto-reinforce | — |
| Choosing randomness | drand quicknet rounds fixed by rule at least Δ ≥ 60 s after every input closes; **exactly one BellAnchor per (bell, region)**, addresses checked on every read; no oracle, no blockhash, no operator seed | drand League of Entropy is a named trust root |
| Choosing between two anchors or seeds (revision 3; address corrected in 3.1) | With a caller-supplied bump, a second anchor under bump 253 got a later timestamp and a different seed, and Resolve accepted both [measured, review of S-BEACON]. Now anchors, caches and slots use **predictable** with-seed addresses (`an ‖ hex(b, r)`), every reader recomputes the address, the first post wins, and an archived bell is tombstoned so no second anchor can follow a closed one (18 forgery and consistency checks pass, plus `t9` [measured, SP-V2]) | — (the same tests become M1 gates) |
| Blocking accounts by pre-funding (revision 3) | Today's `CreateAccount` fails once an address holds lamports: blocking one faction's 4 slots in one province-bell cost 0.0026 SOL (≈ $0.39), and pre-funding every anchor of a season ≈ 41.9 SOL froze the world [measured / derived, reviews]. Now every init path tops up, Allocates and Assigns (§8.6 rule 10) | — (pre-funded-address test per init path) |
| Operator sees sealed orders | No operator deposit path; salts in the client, a chosen relay, or seals | drand threshold collusion could decrypt early |
| Spawn sniping by bots | Site tickets settled per bell by the seed | — |
| **Blocking reveals by buying blockspace** (rewritten in 3.1 from SP-FEE) | The scheduler ranks by **priority = (priority fee + 2,500) / cost** (cost = CU limit + 720 per signature + 300 per write lock + loaded-data cost); a keeper is excluded exactly when its priority is below the attacker's [measured, SP-FEE]. **Known accounts** (one province's slots, one anchor, a Province, a keeper's fee payer) cost **p × 40M per block** for up to ~20 (legacy) or ~60 (lookup table) of them in one stream; **only writes the attacker cannot enumerate** (sealed destinations paid from ≥ 150 rotating fee payers) need the whole block, p × 100M, minus organic traffic bidding above p (13% of mainnet non-vote cost at p = 0.433 [measured, read-only]). At the default tip a 16k-CU Reveal has priority 0.433 (0.27 at 26k CU); the defence pool raises it to a cap of 2.0 | Priced, not impossible; C4 **not met at the default tip** (below) |
| **Locking the keepers' fee payers** (new in 3.1) | Every fee payer is a writable account. Drill (g): an attacker listing the keeper's payer as a writable key in its own fillers held a Reveal into an address it did not know for 11 s at 0.040 SOL per slot (one account's price); 20 known payers in one 21-key stream were all held; a fresh payer landed in 0.89 s [measured, m0c]. **Rule:** keepers pay each Reveal from a payer drawn at random from ≥ 150 funded keys (≥ 150 × 40M / 60 > 100M, so even enumerated payers cost a whole block); owners who self-reveal avoid a predictable wallet | Owners paying from their own wallet are exposed at one account's price; the in-bell reveal and a keeper's tip cover them |
| Operator's keeper withholds selectively | Any keeper earns the tip, so the operator's keeper is not privileged; every unrevealed valid seal is flagged | — |

**Absolute criterion C4, restated (revision 3, O7).** Revision 2 asked that blocking cost ≥ 10× "the marginal prize value of the clash it would change", per province. **C4 now reads: excluding every reveal (and every anchor and seed post) bid at the default keeper price for one bell must cost at least 10× the value of all clashes resolving in that bell for the targeted side.**

**Status (revision 3.1): C4 is NOT MET at the default tip [model].** **M1 final (§22.2), with the measured Reveal (worst 25,155 CU, limit 26,500, `L(reveal)` 1,146,880 B, in play p50 19,772 / max 21,418 CU over 18 Reveals): unchanged** — at the minimum tip (priority 0.433, 0.428 on a first reveal) C4 fails at 600 s (2.3–10.1× with ≥ 150 rotating payers); at the pool cap 2.0 with ≥ 150 rotating payers it passes, **11.7–47.9× at 600 s and 23.4–95.9× at 1,200 s** on valuation (a), relic stress case included [model]. The rest of this block is revision 3.1's record. [SP-FEE `results/c4-model.txt` v2; drills (a)–(g) measured on a local validator; value from `frontier-sim c4`, 50k wallets, 3 seeds.]
- **Measured mechanism.** A keeper is excluded exactly when its priority is below the attacker's; while excluding, the attacker pays 1.0–1.16 × p × (the binding cap) per slot; a keeper that outbids lands in 1–2 slots [measured]. The local validator cannot show Jito bundles, leader discretion, multi-leader ingress, Firedancer's pack or execution time in a 265-ms slot.
- **Attack cost for one 600-s bell, side-wide** [model]: default tip, 16k-CU Reveal (priority 0.433): **$5.1k–14.0k** if the side's payers are unpredictable, **$2.1k–5.9k if they are known** (payer lock); with a 26k-CU Reveal (priority 0.274): $3.2k–8.8k / $1.4k–3.7k. At the defence-pool cap 2.0: $26.5k–67.4k / $9.9k–27.2k. Ranges span three capacity cases: execution-bound [estimate: 55M CU/s per account, 150M CU/s per block], the design's 100M per 0.4 s, and 100M per 0.265-s slot if the limits are not rescaled (unverified); organic traffic above p is subtracted [measured, read-only mainnet].
- **Value of one bell** — three valuations, all reported: (a) the design's placeholder, $7 × every clash the busiest faction takes part in: p99 $1,372, max $1,456 (plus 5 relic clashes: $2,206); (b) $7 × only the clashes with its own arrivals, which are all an exclusion changes besides committed postures: p99 $322, max $455 [sim: p99 44–46, max 62–65 over 3 seeds]; (c) the simulator's counterfactual, re-playing the seed with that faction's reveals excluded in that bell: the target's loss of claims is −$105 to +$164 per attacked bell (mean +$35 over 12 attacks), which is inside the re-play noise (a 100-troop exclusion moved one seed by $91); the conservative envelope (the larger of the target's loss and any other faction's gain) is **$125–164** [sim].
- **The tail is one synchronized episode per seed**, not independent busy bells: in all three seeds the p99 and max bells fall in one run of ~30–40 bells around day 10.6–11.0, with ~200 clashes of which only 2–29 involve the busiest faction's arrivals (the rest are defence-only fights). It coincides with the release of day-0 idle wallets' first holdings after 10 idle days, which the simulator's 60% day-0 join spike synchronizes [estimate: timing match, not isolated]. Real join timing would spread it.
- **Verdicts** (attack ÷ value, PASS ≥ 10×): at the **default tip**, valuation (a) **fails** (1.0–9.6×; the one exception is the p99 bell with unpredictable payers if the limits are not rescaled, 10.2×); valuation (b) passes only with unpredictable payers and a 16k Reveal (11–44×); valuation (c) passes except with a known payer and a 26k Reveal on the execution-bound case (8.3×). At the **defence-pool cap 2.0 with ≥ 150 rotating payers it passes on every valuation and capacity case** (12–540×, worst: max bell plus relics on valuation (a)); with one known payer, valuation (a) passes only if leaders execute full 40M-CU account budgets (4.5–20×).
- **What would change it:** the leader execution rate and bundle behaviour (M4 mainnet-fork or devnet soak, owner approval per step); the full Reveal's CU (M1); the payer rule holding in the keeper SDK; and which valuation the owner accepts.
- **Defences (O7):**
  1. **Defence pool, specified (revision 3.1) [design]; re-sized by write class (M1, CL-30/CL-31a, I-21; owner to confirm, D18).** Keepers bid in priority terms: start at the tip level, **×2 per slot, resend every slot**. Writes are split by whether lateness **changes an outcome**: **window-closing writes (class W: Reveal; RevealPosture in M3)** escalate to **P_def = 2.0** and are the only pool-eligible writes; **delay-only writes (class D: PostAnchor, PostSeed, ConsumeGenesisSeed, GatherClash, ResolveFromInputs, SkipQuiet, SettleDeparture, SettleTransit, ArchiveAnchors, SettleTicket, OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy, ClaimDefence)** escalate to **P_delay = 0.5** from the keeper's own budget and are never pool-eligible, because holding them only makes a region or province wait (§8.4). Revision 3.1 listed every critical write as eligible; counting every delayed write of a whole-block attack puts the pool's spend at ≈ 0.5–2 SOL per attacked bell, so 20 SOL would cover only 10–40 bells. The measured table is in §21.3. **Refunds never touch the critical transaction** (rule 2): a keeper later calls a non-critical `ClaimDefence` against its own per-keeper claim shard, citing the landed transaction. **Eligibility is on-chain evidence of lateness:** the write landed ≥ N slots (default 4) after it first became valid (the bell start for owner reveals, `T(b)`'s anchor for keeper reveals, `A + W` for gathers, the seed round for resolves), and paid a priority above the tip level. **Caps:** per bell and region (the escalation schedule's total), per keeper per day, and per-faction sub-pools pay only that faction's writes. Peacetime writes land in 1–2 slots and so never qualify; a keeper that deliberately waits N slots to qualify earns at most the capped refund of its own overbid, never more than it paid. Priority fees go to the leader, so a validator-keeper could recapture them; the per-keeper cap bounds that. **M1 as built (I-21, I-22, I-52; contract §5.12):** only **ArrivalSlot evidence** is claimable (landing slot, CU price, CU limit and loaded-data limit read from the instructions sysvar at Reveal); lateness is measured against THE anchor's slot (`lateness_slots` = 4); the refund is `min(fee paid, P_def·cost − 2,500) − (tip_min − 2,500)`; claims go through one `DefenceClaim` account per keeper-day with per-bell-region and per-keeper-day caps; and **SettleTransit keeps an eligible, unclaimed slot open for a 6-bell claim grace**, after which CloseArrivalSlot closes it (a displaced reveal's evidence resets and is not claimable). Faction sub-pools are M2.
  2. **Keeper SDK (part of the spec)** [measured basis, SP-FEE]: `SetLoadedAccountsDataSizeLimit` on every transaction (without it a bid's priority halves: 1.1 → 0.596) **at the per-kind limit `L(kind)`** (M1, I-45: SIMD-0186 counts the program's ProgramData, and SP-V2's program is 480–541 KB, so SP-FEE's 64 KiB, measured with a 13,600-B probe, would make every Frontier transaction fail and still pay its fee; `L(kind) = round_up(programdata_len + 45 + Σ(account data + 64), 32 KiB)` from the release build, 1 MiB until measured), a tight CU limit, bids stated in priority terms, ×2 escalation per slot up to the cap, resend every slot, **fee payers drawn at random from ≥ 150 funded keys**, and per-region anchor fallbacks sent alongside the combined 16-region anchor. **M1 final:** `L(kind)` is generated from the release program (programdata 1,105,920 B for the 884,736-B placeholder that bounds the release `.so`): 1,114,112–1,277,952 B per kind, `L(reveal)` = 1,146,880 B (§8.3); the combined anchor carries ≤ 7 regions per transaction (`MULTI_MAX_REGIONS`), so a bell takes three combined posts plus the fallbacks. The keeper's operating guide is `m1/RUN-A-KEEPER.md`.
  3. **Longer reveal windows.** The attacker's cost grows linearly with the window. The window `W` (600 s by default) becomes a season parameter set at `CreateSeason` in 600–1,800 s, and the season owner may raise it for **future** bells only, announced ≥ 144 bells ahead, never for a bell that has started (a longer window can only delay, never pick a seed, §8.5).
  4. **In-bell reveals by owners** already exist: an owner may reveal from the start of bell b, which doubles the time an attacker must cover for that arrival.
- **The default tip is stated in priority terms (revision 3.1), and it is now the minimum (M1, CL-22 / I-08):** `tip_min = ⌈p_min × (reveal_cu_limit + 1,320 + 8 × ⌈L_reveal / 32,768⌉)⌉ + 2,500` lamports with `p_min = 0.433` (season parameter `min_reveal_priority_milli` = 433), kernel `fees::min_tip_lamports`. With the working Reveal limit 26k and a loaded-data limit `L_reveal` of 1 MiB until the release program is measured (I-45): **14,441 lamports** (10,111 at a 16k limit) [model]. Revision 3.1's `+ 1,336` assumed a 64-KiB loaded-data limit, which the real program cannot use (§8.7). The value is final after the M1 Reveal measurement (wave 3). **Final for M1 (W5-A regeneration, contract v1.8; §22.2):** `reveal_cu_limit` = 26,500 (the worst Reveal 25,155 + 5 %), `L_reveal` = 1,146,880 → **`tip_min` = 14,668 lamports**; the relay's three sponsored presets are 14,668 / 22,002 / 29,336 (I-51). The relic tip (100,000 lamports) stays a separate minimum for Relic Sites (M3). Zero tips are gone: the "hope nobody reveals" option of §6.4's table no longer exists. Players may still raise it for decisive battles.

**Seal details (revision 3).** The client encrypts `(plaintext ‖ salt)` to the quicknet round `T(b)` in the pinned 165-byte format (§6.2) and logs it in Depart. After that round anyone decrypts off chain and sends `Reveal`; the chain checks only the hash. The chain can also open the seal (the mainnet BLS12-381 syscalls expose the pairing result), which is what revision 3.1's `ProveBadSeal` used [measured, S-TLOCK-V: 33.8k CU without the FO check, 42.7k with it]; **in M1 `SettleTransit` runs the opener for every transit** (§6.2, I-44). Quicknet beacons appear 0.8–2.0 s after their round time, so a seal becomes openable about then [measured, S-TLOCK]. Sources: https://docs.drand.love/docs/timelock-encryption/ , https://github.com/drand/tlock , https://github.com/drand/tlock-js .

### 6.5 Delegated command (E3)

`DelegateCommand(host_mask, delegate, until)` hands hosts to a Company captain, a faction marshal (≤ 16) or any member of the same faction. The delegate may Depart, Reveal, set postures and settle transits; never move goods, dissolve hosts or transfer holdings. **The delegate earns 20% of the laurels those hosts win through captures and Relic Sites** (not a multiplier on a capped counter). Revocable at any time.

### 6.6 Protection and newcomer fairness

- Shield: 48 h for every new or re-founded holding (72 h after day 7). The newcomer tutorial of §2.8 happens inside this Shield.
- **Frontier protection (revision 2):** for 7 days after its Shield, a holding founded after day 2 can be besieged only by factions with a holding within 2 provinces of it. Distant veteran armies cannot farm new cohorts.
- **Relic Sites** sit three rings inside the current rim, and relic odds from exploration are flat beyond ring 10, so veterans are not drawn to the newcomer band.
- Vigil hours; slow sieges; auto-reinforce (§6.3).
- Heartland rings: no sieges without War. Seats: never capturable. First holdings: occupied, never taken.
- Frontier Grant for small factions; catch-up kit for late joiners.

---

## 7. Hidden AI players (Shades)

### 7.1 Count and behaviour (revision 2)

- `a = ⌈N_expected / 200⌉` roster leaves (**0.5%**; was 1%), the same number in every faction. At 50k: 250 Shades, one per four Marches.
- **Per-faction cap 0.75%** of the faction's citizens; excess leaves are revealed as unused.
- Shades join through the same relay path as people (x402, relay-fronted rent and fees, §2.2), pay with real operator USDC, found holdings, fight, trade and explore through the same instructions, quotas and buckets (E6).
- **Committed policy.** Each Shade runs a **deterministic policy**: open-source bot code whose hash is committed in the Season at genesis, plus a per-Shade parameter vector and seed committed inside its roster leaf. Inputs are only public chain state and drand beacons. Decisions are taken at bell boundaries; join times are drawn from the policy's committed schedule. **No LLM decides any action**; an LLM may only write chat flavour, which has no game effect.
- **Political rights (D5, revised):** Shades **vote** in Warden elections according to the committed policy (they never hold a seat, so they never vote in the Assembly) (so "has ever voted" is not a tell), but **never stand for office, petition, consent, recall or accuse**.

### 7.2 Roster commitment

- At genesis the Season stores `roster_root`, a Merkle root over `a` **blinded** leaves `H(roster_tag(season, wallet, salt) ‖ policy_params ‖ shade_seed ‖ blind)` and `policy_code_hash` (fixes WP09). Dummy leaves pad the count. The root is fixed before any join.

### 7.3 Reveal, replay, voiding and the bond

- `RevealShade(leaf opening)` after the Reckoning: operator's roster key only, in leaf order, within 24 h (G2). It voids the Citizen and **subtracts its recorded contributions** from its faction: citizen-pool units, Works, laurels, Civilisation-Share weight, **and its per-path score facts** (March-control bells from its holdings, captures, production, techs and science, Engine contribution). Each Citizen record carries these per-path fact counters for exactly this purpose. FactionResult is recomputed from the reduced facts; people's claims use the reduced sums.
- **Replay.** The verifier re-runs every Shade's policy from public state at each bell and compares it with the Shade's actual transactions. Any divergence is a **Shade violation**. What a Shade did to other factions (a capture, a raid) cannot be undone, which is why the policy is committed before the season rather than audited only afterwards.
- **Forfeiture.** An unrevealed leaf after 24 h forfeits the whole escrow to the pools on chain. A policy violation found by the verifier is published; on chain, a **Shade Auditor** (a named independent multisig set at `CreateSeason`, owner decision D15) may trigger the same forfeiture within 7 days of the reveal. The Auditor can only move the escrow to the pools, never to anyone else.
- **The bond now holds (G3).** The most withholding could gain is every active Shade at the wallet cap: `0.0075 × N × 5 × paid ≤ 0.0375 N f` against an escrow of `0.2 × N × average paid ≥ 0.07 N f` even if the average payment is only 35% of the day-0 price [model: `rev2-results.txt` §G]. Revision 1's `0.15 N f` against `0.11 N f` failed.
- **Bounties:** the operator escrows 2 USDC per leaf. Half goes to the first captor of that Shade's holding; half is split among correct accusers **whose season accusations were at least one-third correct**. Unclaimed bounty goes to the pools.

### 7.4 The accusation game, and the tells we cannot remove

- Each citizen may `Accuse` up to 3 holdings per week, staking in-game gold that doubles with each further accusation in the same week.
- **Known tells, stated honestly:** chain forensics (funding paths, session-key registration timing, join timing, transaction timing patterns, never standing for office) can raise suspicion. Revision 2 removes the largest ones: everyone is funded through the relay by default, Shades vote, join times follow a committed schedule drawn from the observed join process. What remains is behavioural, and many humans run SDK bots, so behaviour alone does not separate them. The fantasy "is my neighbour an AI?" is weaker than revision 1 implied, and the summary says so.

---

## 8. Architecture

### 8.1 Layers, and why the world is on base

| Lane | Layer | Contents |
|---|---|---|
| **Frontier (Season 1, the only lane)** | **Solana base**, program built for **SBPF v2** (v3 once platform-tools supports it) | Everything: Season, vault shards, Citizens, Holdings, Provinces, ArrivalSlots, PosturePDAs, ClashInputs, SealVerdicts, faction shards and states, tallies, decrees, pools, beacon anchors, seed caches and archives (M1 builds 17 kinds, §8.2; no SealVerdict, no PosturePDA) |
| Randomness and seals | **drand quicknet** (BLS12-381), verified on chain; one network per season for seeds and seals (O1) | Bell seeds, ring seeds, the genesis seed (O2), tlock seals; posted by anyone |
| *Frontier on ER (X1), later* | Dedicated, gated MagicBlock ER | The same program, per season, once the gate spike passes. **Not in Season 1** (O8) |

Revision 3 removes the Skirmish, Arena and ER-VRF rows: the v9 round is stopped and Season 1 has no MagicBlock component (O8, O2).

**The argument for base:**
1. **The free write lock (audit C1) has no program-level fix on a public ER.** ER transactions are free, and the engine orders by the `is_writable` flag of every key before checking delegation, so even read-only clones can be locked (`magicblock-engine` @ 6993f9d, `processor/src/sequencer/order.rs`).
2. **The ER fix is an ingress gate that MagicBlock does not document**, and whoever runs it can censor.
3. **On base, the same lock costs money.** Revision 3 corrects the parameter: the per-account write cap is **40% of the block limit, 40M CU with today's 100M block** (SIMD-0306, active on mainnet since slot 379,296,000 [read-only RPC]; revision 2 used the old 12M). SP-FEE measured 39.9M CU per account per slot and 98.8–99.9M per block on the local validator, and SIMD-0286 (100M block) is active on mainnet [measured, read-only]. Consequences (revision 3.1, corrected from SP-FEE): holding one account costs p × 40M CU per block (3.3× revision 2's figure); **one multi-lock stream holds up to ~20 (legacy) or ~60 (lookup table) known accounts at that same price** [measured: 4 slots at 0.0403 SOL per slot, the same as one; 20 fee payers held by one 21-key stream]; only an attack on writes the attacker cannot enumerate (sealed destinations paid from unpredictable fee payers) needs the whole 100M block, which also delays every other low-bid Frontier write in the world (§8.6, §8.7). Readers are not blocked. Every critical write is sent by players' keepers who bid up to a tip and, during an attack, up to the defence-pool price; no critical path waits on a third party's non-escalating transaction (§8.5, §8.6 rule 8).
4. **Every player action is small** (4–26k CU measured for player actions and reveals); the heaviest steps are a worst-case clash (ResolveFromInputs ≤ 541k, budget 650k with its gathers) and beacon posts (≈ 332k on v2) [measured, SP-V2].
5. **Base logs are durable**; no ER archive, no mass undelegation.
6. **The operator has no ingress.**
7. **MagicBlock in Season 1: none** (revision 3). A gated Frontier lane on an ER (X1) and Private-ER reads (X2) remain later extensions.

**Honest costs of base:** players pay fees (≈ $0.50 per season; sponsorable); ~1 s confirmations; rent is a real, refundable float; how leaders enforce local fee markets (Jito bundles, multiple leaders) is only partly known, so lock prices are orders of magnitude until the redone S-FEE drill and the M4 soak measure them.

**Toolchain (revision 3).** The Frontier program is pinned to SBPF v2 in `scripts/build-program.sh`: quicknet verification is 324–330k CU on v2 against 882–895k on v0 [measured, S-BEACON]. arkworks field operations (mul, square, inverse, sqrt) stay `#[inline(never)]`; inlined they produce 8–17 KiB stack frames, over the 4 KiB SBF limit [measured]. `cargo-build-sbf` from platform-tools v1.52 cannot build SBPF v3, and its bundled cargo 1.84 cannot parse edition-2024 transitive crates; builds use `RUSTUP_TOOLCHAIN=1.95.0` for `cargo metadata` [measured, S-FEE notes]. Local drills that verify quicknet need a validator with the BLS12-381 syscalls (Agave ≥ 4.0); the repo's `solana-test-validator` 3.1.9 has none [measured].

**BOLT ECS is not used** for core accounts (1 KiB return-data cap, 15–99k CU per apply, `add_entity` locks a global World account).

### 8.2 Accounts

Rent = (128 + size) × lamports per byte (5,080 on mainnet since SIMD-0437 step 2).

**Addressing (revision 3).** Accounts that a permissionless reader must trust to be *the* account for a key — ArrivalSlot, PosturePDA, ClashInputs, SealVerdict (M1: removed), BellAnchor, SeedCache, AnchorArchive — use **with-seed addresses** (`create_with_seed(Season PDA, text seed, program id)`, text seed ≤ 32 bytes encoding the key). There is no bump, so exactly one address exists per key, and **every read recomputes and checks the address**. Absence proofs use the same derivation (≈ 0.5k CU each against ≈ 4k for `find_program_address` [measured, S-SIZE]). Player-owned accounts keep canonical PDAs with the bump found at creation and stored. **Every account the program creates is initialised pre-funding-safely** (§8.6 rule 10).

**Seed grammar, pinned (M1, CL-21 / I-01 / I-02; kernel `frontier::addr`).** In M1 **every program account except the Season PDA** is a with-seed address of the Season PDA (the player PDAs above become with-seed too; the Season's bump is found once and stored). `seed = tag (2 ASCII bytes) ‖ lowercase hex of the key fields`, the key fields **little-endian and fixed width** (coordinates as i32, days as u32, bells as u32), at most 15 raw bytes, so every seed is ≤ 32 bytes and no two kinds can collide (distinct tags, fixed widths). This is SP-V2's `acct.rs` grammar byte for byte (the closeout draft's "big-endian" is withdrawn). The longest seeds are ArrivalSlot `ar ‖ P, Q, bell, faction, i` (30 B) and Citizen `ct ‖ sha256("PSF-CIT" ‖ wallet)[0..15]` (32 B). The full table (18 kinds, including the reserved `sv` and `po`) is §4.1 of the M1 contract; vectors `addr-vectors-v1.json` are shared by the program, keeper, verifier and web client.

| Account | Address key | Size | Written by | Paid by |
|---|---|---|---|---|
| Season | PDA `["season", id]` | 2,048 B | lifecycle; `outstanding` per claim | operator |
| VaultShard ×8 | SPL, owner Season | 165 B | Join, Claim, Refund, WithdrawJoin, ops release | operator |
| ProvinceFund | PDA `["pfund", id]` | 128 B | CreateSeason, top-ups, OpenProvince | operator (refunded) |
| DefencePool, FactionDefencePool ×6, and per-keeper claim shards (revision 3.1) | PDA `["dpool", id]`, `["dpool", id, f]`; `["dclaim", id, keeper, s]` | 128 B | CreateSeason and top-ups; **never written by a critical transaction**: refunds are a later claim (§8.8) | operator; faction members (refunded if unspent) |
| JoinShard ×48 | PDA `["join", id, faction, s]` | 256 B | Join, SettleTicket (incl. displacement) | operator |
| **Citizen** | PDA `["cit", id, wallet]` | 320 B | every own action (action bucket), banking, votes, claim; per-path score fact counters; `counted_laurels` and the stake point | player (refundable) |
| **Holding** | PDA `["hold", id, P, Q, site]` (revision 3: keyed by site, so SettleTicket displacement rewrites it in place) | 832 B | owner or delegate; captor/occupier; SettleTransit | player (refundable) |
| **Province** | PDA `["prov", id, P, Q]` | 4,096 B | SettleTicket, Muster, Depart (origin), ResolveClash / ResolveFromInputs, Explore, sieges; per-faction reward indices; result ring; **mirror of each site's garrison, walls and owner faction** (revision 3) | ProvinceFund |
| **ArrivalSlot** | with-seed `(P, Q, b, f, i)` | 96 B | Reveal (create or displace); closed by SettleTransit | revealer (refunded) |
| **PosturePDA** | with-seed `(P, Q, b, host_slot)` | 96 B | CommitPosture, RevealPosture; closed after the bell | defender (refunded) |
| **ClashInputs** (new) | with-seed `(P, Q, b)` | **1,024 B** [measured, SP-V2] | GatherClash (idempotent parts); closed after resolution | gatherers (refunded) |
| ~~SealVerdict~~ (revision 3.1; **removed in M1**, I-44) | with-seed `(host, depart_bell)` | 96 B [measured, SP-V2] | ProveBadSeal (once) | prover (refunded at SettleTransit) |
| BellAnchor ×16 per bell | with-seed **`an ‖ hex(b, r)`** — exactly one per (season, bell, region), **predictable** (revision 3.1; revision 3's `sha256(sig_T)[..8]` in the seed is dropped, §8.4) | **112 B** (stores the verified 48-byte round-`T(b)` signature) | PostAnchor (first post of round T(b); refused for a tombstoned bell), or the combined 16-region PostAnchor | keeper (closed after archiving) |
| SeedCache, ≤ 256 per region-bell | with-seed `(b, r, nonce)`, nonce 0..255 | **112 B** [measured, SP-V2] | PostSeed (round S(b, r)); records THE anchor's address and A | keeper (closed after archiving) |
| AnchorArchive (new) | with-seed `aa ‖ hex(r, day)` | ≈ 2.4 KB [estimate] (the lab's tombstone-only form is 64 B) | ArchiveAnchors: bell → (A, S, seed) for one region-day **and a tombstone bit per bell**, set before that bell's anchor is closed | operator |
| BeaconLog ×16 | PDA `["blog", id, s]` | 256 B | PostBeacon (latest round per shard; outage evidence; Reveal's "seed round already on chain" check) | operator |
| FactionShard ×96 | PDA `["fsh", id, f, s]` | 512 B | banking, active days, pledges, founding, capture, March-control deltas | operator |
| FactionState ×6 | PDA `["fac", id, f]` | 4,096 B | FoldFaction, ApplyDecree, Mandate posts | operator |
| MarchState | PDA `["mst", id, m]` | 128 B | FoldMarch (hourly) | operator |
| CivState | PDA `["civ", id]` | 1,024 B | FoldCiv (hourly) | operator |
| Frontier, RingSeed ×R_MAX | PDA `["frontier", id]`, `["ring", id, d]` | 512 B, 64 B | OpenRing, ConsumeRingSeed, ConsumeGenesisSeed, **FoldOccupancy** (revision 3) | operator |
| MarchRoll, MarchTally, MinisterTally, Decree, PetitionBoard, Company, Pool/PoolShard/PoolBook, Accusation/AccuseCount, FactionResult | as revision 1 | | | |

**Per-player rent:** 1 Citizen + 1.15 Holdings ≈ **0.0079 SOL at 5,080** [derived: (448 + 1.15 × 960) B × 5,080]. ArrivalSlots, PosturePDAs, ClashInputs and SealVerdicts (M1: ArrivalSlots and ClashInputs; ArrivalDays live a day) are short-lived (minutes to an hour) and refunded. **Anchors and caches:** keeping every anchor and cache of a 28-day season would cost ≈ 62.9 SOL [model, S-BEACON]; `ArchiveAnchors` lets them close after 48 h (§8.5).

**The M1 account set (M1, I-01, I-10, I-31, I-44, I-47, I-48, I-49; normative in contract §5.2).** M1 builds 17 kinds, every one a with-seed address of the Season PDA except the Season itself; the table above stays as the full-game design (M2/M3 add vault, faction, March and governance accounts).

| Account | Seed | Size | Created by / paid by | Closed by → refund |
|---|---|---|---|---|
| Season | PDA `["season", id]` | 2,048 B | AnnounceSeason / the program's upgrade authority (+ 1 SOL creation bond) | never; CloseSeason shrinks it to a 128-B tombstone |
| Frontier | `fr` | 512 B | CreateSeason | CloseSeason |
| ProvinceFund ×6 (one per wedge) | `pf‖w` | 128 B + fund | CreateSeason | CloseSeason |
| JoinShard ×48 | `js‖f,s` | 256 B | InitShards | CloseSeason |
| BeaconLog ×16 | `bl‖r` | 128 B | InitBeaconLogs | CloseSeason |
| DefencePool | `dp` | 256 B + escrow | CreateSeason (20 SOL default; test SOL in M1) | CloseSeason |
| RingSeed | `rs‖d` | 128 B | OpenRing / caller | CloseSeason part 8 → payer |
| Citizen | `ct‖tag(wallet)` | **384 B** (ticket escrow) | Join / the relay (`payer`) | CloseCitizen → rent payer (escrow → ticket funder) |
| Holding | `ho‖P,Q,site` | **1,280 B** | SettleTicket, from the Citizen's escrow | ReleaseDormant, CloseHolding → rent payer |
| Province | `pv‖P,Q` | 4,096 B | OpenProvince / its wedge's ProvinceFund | CloseProvince → ProvinceFund |
| ArrivalSlot | `ar‖P,Q,b,f,i` | 160 B | Reveal / the fee payer | SettleTransit or CloseArrivalSlot → the fee payer that paid |
| **ArrivalDay** (new in M1) | `ad‖P,Q,day` | 96 B | first Reveal of a province-bell in its day | CloseArrivalDay → fee payer |
| ClashInputs | `ci‖P,Q,b` | 1,280 B | first GatherClash | CloseClashInputs → fee payer |
| BellAnchor | `an‖b,r` | 144 B | PostAnchor(Multi) | ArchiveAnchors → fee payer |
| SeedCache | `sd‖b,r,n` | 144 B | PostSeed | CloseSeedCache → fee payer |
| AnchorArchive | `aa‖r,part` (half a day: `part = bell / 72`) | 6,144 B (72 entries with A, seed and the round-T(b) signature, tombstone bits) | first ArchiveAnchors of the region-half-day | CloseSeason part 9 → payer |
| **DefenceClaim** (new in M1) | `dc‖keeper,day` | 128 B | first ClaimDefence of a keeper-day | CloseSeason part 10 → beneficiary |

Per-player refundable rent in M1: Citizen + Holding = **9,753,600 lamports ≈ 0.0098 SOL** (+24% over the revision-3.1 figure, because the Holding carries four 96-B transit records and the Citizen the ticket escrow). **Rent returns to the fee payer that paid it** (I-49), so a keeper's payer pool refills as slots, days, inputs and anchors close. The AnchorArchives of a 7-day season hold ≈ 8.2 SOL of refundable float (16 regions × 16 half-days × 31.9M lamports).

### 8.3 Instructions

"Measured" rows are SBF builds under LiteSVM. **M0 rows are SBPF v0 unless marked v2** and are re-measured under the pinned v2 build; crypto rows fall 1.9–2.7× on v2 [measured, S-BEACON]. **Every other row is an estimate** and gets the same gate in M1.

**Player instructions (session key or wallet; every one also debits the Citizen's action bucket)**

| Instruction | Writable (max) | CU | Source |
|---|---|---|---|
| Join(faction, stake?) | Citizen (init), VaultShard, JoinShard; reads Frontier (occupancy) | **14.4–25.0k** (the Citizen PDA's bump search dominates the spread); 750 B, 15 locks | SP-V2 measured, v2, pre-funding-safe init |
| AddStake / WithdrawJoin | Citizen, VaultShard | est. ≤ 30k | |
| FileTicket(≤ 3 sites, pair?) | Citizen | **4k** | S-JOIN measured |
| SettleTicket(site) | Holding (init, keyed by site), Province, JoinShard, MarchRoll; **on displacement also the displaced citizen's Citizen and JoinShard** (holding and escrow revert): 13 locks, 554 B | **19k fresh / 11k displacement** | S-JOIN measured; releases the fee escrow |
| Harvest / Build / Queue / Train | Holding, Citizen | **7.0k** + ≤ 12k | E4 measured |
| Muster / Dissolve / Bank | Holding, Province (incl. the garrison mirror), Citizen, (FactionShard) | est. ≤ 25k | takes effect after the bell's clash |
| Depart(host, commit, seal, arrive_bell, tip) (revision 3.1: commitment and 165-B seal in the data; the program computes `seal_root`) | Holding, origin Province, Citizen | **5.0k** + est. 3k; tx ≈ 699 B [estimate] | E4, S-TLOCK measured (667 B without the commitment) |
| CommitPosture / RevealPosture | PosturePDA (+ Citizen on commit) | est. ≤ 8k | same seal as Depart |
| Explore / Occupy / Capture / Liberate / CollectTribute / DeclareSiege | Province, Holding(s), Citizen | est. 8–20k | |
| SendCaravan / Unload | Holding(s), Citizen | est. 8k | |
| PlaceOrder(shard) | Holding, PoolShard (any), Citizen | est. 8k | |
| Pledge | Holding, FactionShard, Citizen | est. 8k | |
| DelegateCommand / Revoke / SetVigil | Holding, Citizen | est. 5k | vigil change effective at a UTC midnight ≥ 24 h later |
| Votes, candidacies, decrees, Mandates, petitions, recalls | as revision 1, + Citizen | est. ≤ 12k | E3 vote 2.7k measured analogue |
| Accuse | Citizen, Accusation, AccuseCount | est. 10k | |
| Claim | Citizen, payout VaultShard, Season | est. ≤ 20k | closed form (§5.4) |

**Permissionless and crank instructions (no bucket; each succeeds once per object, so volume is bounded by game objects)**

| Instruction | Writable | CU | Source |
|---|---|---|---|
| Reveal (anyone with plaintext) | **ArrivalSlot only** (reads Holding, BellAnchor or, when it is absent, the region-day AnchorArchive, BeaconLog; **no origin Province**, revision 3.1; **M1, CL-23: also the province-bell's ClashInputs address, read-only, which must be absent** — the one-way latch, +32 B) | **17–26k** incl. with-seed creation [S-SIZE, v0]; the slot-creation part 6.6k on v2 | S-SIZE, SP-V2 measured; full Reveal in M1 |
| PostAnchor(round T(b), sig, hints, region) | **one BellAnchor** (reads the AnchorArchive) | **331.8–335.5k (v2)** / 889–900k (v0); tx **709 B** with the archive key (676 B without) | SP-V2 measured, quicknet, 32 real beacons |
| PostAnchor, combined 16-region form | 16 BellAnchors (one verification); falls back to the per-region form if any anchor is held | ≈ 332k + 16 writes [estimate] | |
| PostSeed(round S(b, r), sig, hints, region, nonce) | **one SeedCache** | **333.6k (v2)** / 894.5k (v0); tx 710 B (keeper pays the fee), 806 B with a separate keeper signer | SP-V2 measured |
| PostBeacon (BeaconLog) | one BeaconLog shard | ≈ 332k (v2) [estimate from PostAnchor] | |
| GatherClash(p, b, part) | **ClashInputs only** (reads slots, postures and, for arrivals, their origin Provinces) | **≤ 30.3k** per gather, 1,187 B, 32 locks; 4–6 gathers | SP-V2 measured (24 positions per gather) |
| **ResolveFromInputs(p, b) — primary** | **Province only** (reads THE anchor, a SeedCache, ClashInputs) | **≤ 540.9k** worst case; ≈ 390 B, 8 locks | SP-V2 measured, v2, real kernel |
| ResolveClash(p, b) (single-tx; test oracle only) | **Province only** | **367k–633k** adversarial, 31 locks / 1,150 B (1,182 B with the anchor key); 18.5k honest [E4] | SP-V2 measured |
| ~~ProveBadSeal~~ (removed in M1, I-44) | one SealVerdict | ≤ 48.0k with the FO check; 588 B | SP-V2 measured, v2; M1 runs the opener inside SettleTransit |
| SettleTransit(arrival) | Holding, ArrivalSlot (close), origin Province (frees the departed entry) | est. 12k | non-critical; reads the SealVerdict; pays tip, march fee (and seal bond). **M1: opens and judges the seal itself, 63,281 CU worst (budget 85k), class D; the origin entry is freed by SettleDeparture (§6.2)** |
| ArchiveAnchors(r, day) | one AnchorArchive, then closes that region-day's anchors and caches | est. ≤ 30k (the lab's tombstone-and-close step: 6.2k, 361 B) | non-critical; the tombstone is written before any close (m0c lab test `t9`) |
| CloseAnchor / CloseSeedCache / CloseClashInputs | the closed account | est. 5k | after archiving / after resolution |
| FoldOccupancy | Frontier | est. 2 × 15k (reads 48 JoinShards in 2 txs) | new; feeds Join's reserve check |
| ClearPool(pool, b) | Pool, PoolBook | **206.6k** | E4 measured |
| OpenRing(d) / ConsumeRingSeed / ConsumeGenesisSeed / OpenProvince(p) | Frontier, RingSeed / Province (init), ProvinceFund | est. 60k / 8k / 8k / **167.7k** + 20k | E4 measured (province) |
| FoldMarch(m) | MarchState, one FactionShard | est. 20k | reads 7 provinces |
| FoldFaction(f) | FactionState | **11.7k** | E3 measured |
| FoldCiv, SeatWarden, SeatMinister, ApplyDecree | as revision 1 | est. ≤ 15k | |
| BankAfterEnd(holding) | Citizen, FactionShard, Holding | est. 15k | banking window only |
| FinalizeFaction(f) | FactionResult | est. 20k | after the banking window |
| RevealShade | Citizen, FactionResult | est. 20k | operator roster key only |
| FinishSeason / Abort / Refund / SweepDust / AuditorForfeit | as revision 1 | est. ≤ 40k | |

**Transaction size (measured, revision 3.1, SP-V2).** The primary resolve (ResolveFromInputs) is ≈ 390 B with THE anchor key; the single-transaction ResolveClash with 24 ArrivalSlots is 1,150 B / 31 locks (1,182 B with the anchor key, 50 B of headroom), which is why it is only a test oracle. Gathers are 1,187 B / 32 locks. What does *not* fit: inline beacon verification (1,427–1,476 B), reading 12 Holdings (1,548 B), 3 or more PosturePDAs in one resolve (1,251 B) [measured, S-SIZE]. The program needs no v0 message or lookup-table support. Transaction V1 (SIMD-0385; 159 of 1,255 transactions in one sampled mainnet block were v1 [measured, read-only]) could carry more keys; it is a later option, not a dependency.

**Heap and stack:** worst-case clash heap peak **25,872 B of 32 KiB** [measured, SP-V2; revision 3's 4,480 B came from the stand-in kernel]; ClearPool ~6 KiB heap [measured: E4]; beacon verification heap 720–840 B [measured, S-BEACON]; scratch arrays on the heap; arkworks field operations out of line (§8.1).

**Logs:** one `PS2` event per write, ≤ 128 B, plus the **165-byte** seal on Depart and CommitPosture (revision 2 said ≈ 300 B); < 1 KB per transaction.

**Keeper instruction formats (revision 3).** PostAnchor, PostSeed and PostBeacon carry **hash-to-curve hints** (inverse, square-root and non-square witnesses; 145 B per map on quicknet) computed by keepers and the SDK. Without hints verification exceeds 1.4M CU [measured: 2.8–3.6M on v2].

**M1 as built: measured costs (release SBPF v2 `.so`, 874,120 B at the W6 base; G1 table `frontier-abi::budgets::MEASURED`, each kind at its contract §13.1 adversarial fill) [measured].** Gate = the CU the M1 gate asserts; limit = what clients and keepers request (the G1 maximum + 5 %, rounded up to 500, at most 5 % of the gate above it); `L(kind)` = the loaded-data limit every transaction sets (SIMD-0186 counts the program's ProgramData, I-45).

| Instruction | Class | Writes | Worst CU | Gate / limit | `L(kind)` |
|---|---|---|---|---|---|
| Join | P | Citizen (init), JoinShard | 12,495 | 25,000 / 13,500 | 1,114,112 |
| FileTicket (≤ 3 sites) | P | Citizen (+ escrow), ≤ 3 Provinces (cohorts) | 16,096 | 17,000 / 17,000 | 1,146,880 |
| SettleTicket | D | Holding, Province, Citizen, JoinShard, cohort Provinces (+ displaced Citizen, JoinShard) | 22,383 | 40,000 / 24,000 | 1,146,880 |
| Harvest / Build / Train | P | Holding, Citizen (+ Province for walls) | 16,409 / 19,145 / 16,641 | 17,500 / 22,000 / 17,500 gates; limits 17,500 / 20,500 / 17,500 | 1,114,112 / 1,146,880 / 1,114,112 |
| Muster / Dissolve / Garrison / Explore | P | Holding, Province, Citizen | 20,669 / 18,602 / 18,052 / 18,865 | gates 25,000 (Explore 20,000); limits 22,000 / 20,000 / 19,000 / 20,000 | 1,146,880 |
| Depart | P | Holding, Province, Citizen | 23,167 | 24,500 / 24,500 | 1,146,880 |
| **Reveal** | **W** | ArrivalSlot (+ ArrivalDay bit) | **25,155** | 26,000 / **26,500** | **1,146,880** |
| SettleDeparture (incl. the return settle) | D | Holding, origin Province | 43,083 | 48,000 / 45,500 | 1,114,112 |
| GatherClash | D | ClashInputs | 46,551 | 49,000 / 49,000 | 1,146,880 |
| ResolveFromInputs | D | Province, ClashInputs | 274,007 (Phase B, committed by W6-B; Phase A 327,609) | 290,000 / 285,500 (Phase A: 340,000 / 344,000) | 1,146,880 |
| SkipQuiet (≤ 24 bells) | D | Province | scales with recomputed bells | 90,000 + 30,000 per bell | 1,277,952 |
| SettleTransit (with the seal proof) | D | Holding, ClashInputs, Provinces, slot, recipients | 63,281 | 85,000 / 66,500 | 1,146,880 |
| ClaimDefence (≤ 6 slots) | D | DefencePool, DefenceClaim, slots | 24,111 | 25,500 / 25,500 | 1,114,112 |
| PostAnchor / PostAnchorMulti (≤ 7 regions) / PostSeed | D | one or ≤ 7 BellAnchors / one SeedCache | 339,142 / 380,130 / 339,178 | gates 345,000 / 400,000 / 345,000; limits 356,500 / 399,500 / 356,500 | 1,146,880 / 1,179,648 / 1,114,112 |
| ArchiveAnchors (≤ 8 bells) | D | AnchorArchive, closes ≤ 8 anchors | 45,441 | 60,000 / 48,000 | 1,146,880 |
| OpenProvince | D | Province, RingSeed, ProvinceFund(w) | 148,459 | 220,000 / 156,000 | 1,114,112 |

`L(kind)` = `round_up(programdata 1,105,920 + 45 + Σ(account data + 64), 32 KiB)` at the kind's worst account set; every kind lands at 34–39 pages (+272 to +312 cost units). A transaction that still exceeds its limit is re-sent by the keeper at 2× then 1.4M CU, and a heap fault with a 256-KiB heap frame (I-50): a fill worse than the worst found costs money, never a frozen province. The measured distribution of Reveal in play is in §22.2.

### 8.4 Time and lag (revision 2: lag only waits, never changes outcomes)

- `genesis_ts` and `BELL = 600 s` are fixed; `bell(now) = (now − genesis_ts) / 600`.
- **Arrivals are stored per (province, bell, faction, slot),** so any number of unresolved bells can queue at a province without a buffer ring wrapping, and **a Reveal is never refused because the destination is behind** (revision 1's `ProvinceBehind` refusal of reveals is gone). **Revision 3.1: a Reveal never waits for, or reads, the origin either.** The arrival's post-clash values are read by GatherClash, which has no deadline; the origin keeps them until SettleTransit (§6.2). So a held origin Province, origin anchor or origin cache delays the destination's resolve and changes nothing. (M1: the values move from the origin's departed entry into the Holding at SettleDeparture, which gathers read; same property, gate G7.)
- **Windows are anchored to the beacon, not to the wall clock.** Bell b's reveal close is `A(b, r) + W` (W = 600 s by default, §6.4), where `A(b, r)` is the first on-chain anchoring of the round `T(b)` for region r. If drand stalls, or someone holds a region's anchor, the window stays open and the seed round (defined relative to the anchor) stays in the future. Nothing expires; the bell waits.
- **What lag can and cannot do (revision 3 wording).** Holding the BellAnchor does delay the whole region, and it changes *which future round* becomes the seed — but that round is always at least W + Δ in the future when the anchor lands. The invariant, tested in §9.4, is: **nobody can choose between seeds that are already public.**
- **Combat timers count resolved (or quiet) bells:** siege progress, stamina refills for combat and occupation tenure advance per resolved bell of the province, so a stalled province is a paused province.
- **Resident actions** that write a province (Muster, Depart, Dissolve, Explore, Settle, Capture) require it to be **resolved through bell b − 2** (revision 3; revision 2 said b − 1, but bell b − 1 cannot resolve before its reveal close, which falls in bell b + 1). Their effects are pending until after the current bell's clash (§6.3). The client bundles the needed resolves first. They wait only when the province itself cannot be resolved (seed missing, the Province account held, or quiet-bell proofs outstanding, §6.3).
- **Regions:** provinces are grouped into 16 regions by `hash(province) mod 16`. Each region has one BellAnchor per bell and up to 256 SeedCaches per bell. **The BellAnchor is the one lockable critical beacon account** (inline verification does not bypass it, because the seed round is derived from the anchor's time). **Its address is predictable** (`an ‖ hex(b, r)`, revision 3.1). Revision 3 put `sha256(sig_T)[..8]` into it so nobody could pre-hold it, but then Reveal could not prove "no anchor yet", and a late revealer could omit the anchor and claim the window was open after the seed round was public (review blocker; SP-V2 did not adopt it). Holding a predictable anchor only delays: the window and the seed round both move later with A, so nobody can choose among published seeds; SP-FEE's D6 race (keeper first in 5 of 6 at a tenth of the attacker's bid) measured the attacker's cold-start ramp in the harness, not a lasting defence. Defences: one combined 16-region anchor transaction per bell **always sent alongside per-region fallbacks** (holding region 7 blocked the combined form for 9.8 s while region 3's fallback landed in 2.0 s [measured, SP-FEE D5]); keeper escalation from the defence pool. Holding one anchor for a bell costs p × 40M per block: $2.1k–5.9k per 10 min at p = 0.433, $9.9k–27k at p = 2.0 [model, SP-FEE `c4-model.txt` §6], and it only delays that region.
- **Property (revision 3.1, M1 gate):** no Reveal lands at or after `A + W` of THE anchor, whatever anchor or archive account is supplied or omitted (the lab covers the forged-anchor, forged-archive, absent-anchor and tombstoned cases: SP-V2 `t3`, `t8`, `t9`).
- **Anchors after 48 hours.** `ArchiveAnchors` writes the region-day's bell → (A, S, seed) table **and a tombstone bit per bell**, then closes the anchors and caches. **A closed anchor never reopens its window:** once the anchor is absent, Reveal and PostAnchor consult the tombstone and refuse the bell (a second anchor would carry a new A, hence a new seed round); GatherClash, PostSeed and ProveBadSeal need the anchor itself and refuse too (M1: gathers, resolves and SettleTransit of an archived bell read the archive entry, which stores A, the seed and the round-T(b) signature; PostSeed still needs the anchor). Tested in the lab: `t9` (close at A + 48 h over a pre-funded archive address; Reveal refused at A + 48 h + 5 s and at A + 30 days; PostAnchor refused; a never-anchored bell of the same region-day still open; a forged archive address refused) [measured, SP-V2 m0c]; with the tombstone check removed, the late Reveal lands (mutation check). A tombstone rather than a time limit, because a drand outage longer than 48 h must still only delay (A4). A province more than 48 h behind reads the archive instead.
- Production and other economic timers use wall time (symmetric for everyone).
- **Season end and the banking window.** At `T_end` gameplay instructions refuse. All reward indices, production and score terms are **frozen at their closed-form value at T_end**. For 72 hours anyone may call `BankAfterEnd(holding)` to credit a holding's accruals to its Citizen and FactionShard (bounded: once per holding). Because every value is fixed at T_end, when it is banked does not matter; a lock on a FactionShard or Holding only delays banking within a 72-hour window defended by escalating keepers. `FinalizeFaction` runs after the window; anything still unbanked goes to the faction's general split instead of being lost. There is no cliff at T_end.

### 8.5 Randomness (revision 3: quicknet only, no VRF, no inline path)

| Draw | Source | When it becomes known | Used for |
|---|---|---|---|
| Bell seed `S(b, r)` | the first quicknet round scheduled **at or after** `A(b, r) + W + Δ` (M1, CL-19), **Δ ≥ 60 s** | at least Δ after the region's reveal close | clash variance, tie keys, explore outcomes, site lottery, market tie-breaks |
| Ring seed `R_d` | the first quicknet round at or after `t_open + 600 s + Δ`, `t_open` = the on-chain time `OpenRing(d)` succeeded (kernel `beacon::ring_seed_round`) | after the ring's opening is fixed | terrain and sites of ring d |
| **Genesis seed** (revision 3, O2; M1, CL-24) | the first quicknet round at or after `t_create_min + 600 s + Δ`, where **`t_create_min` is fixed by `AnnounceSeason`** ≥ 24 h before `CreateSeason` (not the actual creation time, so an operator cannot re-roll it, §8.10); posted and checked by `ConsumeGenesisSeed` (kernel `beacon::genesis_seed_round`) | ≥ 10 min after the join parameters, `roster_root` and `policy_code_hash` are fixed; before joins open | genesis rings 2..g (Shade policy seeds are *not* derived from it; they are committed in leaves) |

**Why no MagicBlock VRF (revision 2 for bells; revision 3 for genesis too).** In revision 1, `BellSeed(b)` was a predictable PDA written once by the oracle's callback, and the base queue `Cuj97ggrhhidhbu39TijNVqE74xvKJ69gDervRUXAxGh` is shared by every VRF user [doc: MagicBlock VRF technical details]; the request told an attacker which account to hold, and the oracle does not escalate its fee [model: `rev2-results.txt` §A1]. Genuine misses happen too (`magicblock-labs/solana-vrf` issue #72). Revision 2 still used one VRF output to co-seed genesis. The spike that was to measure that path (S-VRF-LOCK) was deferred when the v9 round stopped, the hardened VRF plumbing (v9 WP11) will not be finished, and the co-seed only guarded against drand threshold collusion for genesis alone — the same collusion every other seed and every seal already trusts. So the owner dropped it (O2): **the Frontier uses no MagicBlock VRF**.

**How seeds are posted.** drand quicknet produces a beacon every 3 seconds whether anyone asks or not. The round for each draw is fixed by rule in advance, is unknown to everyone until it is produced, and anyone can relay it. Solana mainnet has BLS12-381 group and pairing syscalls (Agave 4.0) [doc: solana.com/upgrades/new-cryptography].
- `PostAnchor(T(b), sig, hints, r)` verifies the round-`T(b)` signature and creates the region's **one** BellAnchor, storing `A(b, r)` (the Clock at creation) and the 48-byte signature. A combined form writes all 16 regions' anchors with one verification.
- `PostSeed(S(b, r), sig, hints, r, nonce)` reads the canonical BellAnchor, recomputes `S(b, r)` from its `A`, verifies that round's signature and creates **SeedCache `(b, r, nonce)`** recording the anchor address. **Anyone may create a cache at any unused nonce 0..255**, and all of them hold the same seed [measured: a second nonce gives the same seed], so an attacker cannot pre-hold "the" cache: blocking 256 caches means pricing out whole blocks for as long as keepers keep bidding.
- `ResolveFromInputs` / `ResolveClash` read any valid SeedCache and its anchor (≈ 1k CU) [measured: 973 CU, SBPF v0].

**Rules:**
- A seed is always a round **at least Δ ≥ 60 s after** every input it affects is sealed (revision 3). Without the margin, a round that rounds down could fall before the close, beacons appear 0.8–2.0 s after their round time [measured, S-TLOCK], and Solana's Clock can lag, so the seed could be public 0–3 s before the reveal close [measured, review of S-BEACON]. Every draw is therefore **the first round scheduled at or after** its time (M1, CL-19: `beacon::first_round_from`; revision 3's `round_at` rounded down). **Δ is reviewed against measured Solana clock skew** before each season. *Correction (M1, CL-27):* SP-FEE's figure labelled "Clock drift" measured **confirmation lag** (beacon → confirmed transaction), not Clock-vs-drand skew; the skew is still unmeasured. Reveal refuses once `Clock ≥ A + W`, and once the region's BeaconLog holds any round ≥ S. Property test: seed time ≥ close + Δ.
- **Exactly one BellAnchor per (season, bell, region)**, with its address checked on every read, and every SeedCache names the anchor it came from. A forgery test with a non-canonical anchor address is an M1 gate.
- **One drand network per season, quicknet, for anchors, seeds and seals (O1).** Revision 2's option "anchors from evmnet while tlock opens on quicknet" was wrong: an anchor for `T(b)` exists to prove that round `T(b)`'s seals can be decrypted, and an evmnet anchor proves nothing about the quicknet round. Evmnet is 3.5–5.7× cheaper to verify [measured] but its tlock exists only in unreleased Go code and a 0.0.1 JavaScript port, and on-chain opening of evmnet seals is unmeasured (it would need SIMD-0650) [measured / doc, S-TLOCK].
- No blockhash, slot hash, transaction hash or operator seed anywhere. **No per-bell fallback between sources.**
- **No inline verification path** (revision 3; it does not fit a transaction, §6.3). Revision 2's "or can be bypassed (inline beacon verification)" is deleted from §8.6 rule 8.
- **Cost:** keepers post ≈ 16 anchors (or one combined post) and ≥ 16 seed caches per bell, ≈ 4,600 transactions a day at ≈ 332k CU each on v2 ≈ 1.5G CU a day, ≈ 0.007% of base block compute, plus ≈ 0.023 SOL a day of base fees [derived]. No VRF fees.

**Attack costs** [model, SP-FEE `c4-model.txt` v2 §6; supersedes `rev3-results.txt` §E]: holding one region's BellAnchor costs p × 40M per block ($2.1k–5.9k per 10 min at p = 0.433, $9.9k–27k at p = 2.0) and only delays one region; holding the SeedCaches means holding all 256 nonces, which is 5 ALT streams or more, i.e. whole blocks (the drill's 10-stream attempt did not fill blocks and the keeper landed [measured, SP-FEE (d)]). Faking a 7-day beacon outage (the Abort trigger) means keeping all 16 BeaconLog shards and every Abort-blocking post out of blocks for a week, i.e. whole blocks for ≈ 1.5M slots: ≈ 1,500 SOL at 0.01 lamport/CU and ≈ 150,000 SOL at 1 lamport/CU [model].

### 8.6 Contention defences

**Rules every instruction obeys:**
1. A player instruction writes only that player's accounts (Citizen, Holding) plus at most one Province, one sharded aggregate, one tally, one board or one pool shard. (SettleTicket's displacement also writes the displaced citizen's Citizen and JoinShard; it is bounded to one displacement.)
2. **A permissionless instruction on the critical path writes exactly one account** (Reveal → ArrivalSlot; GatherClash → ClashInputs; ResolveFromInputs → Province; PostAnchor → one BellAnchor; PostSeed → one SeedCache; ProveBadSeal → one SealVerdict). **M1 (I-10, I-13, I-44, I-48):** Reveal → the ArrivalSlot, plus the day's ArrivalDay bit on the first reveal of a province-bell; ResolveFromInputs → the Province and the same province-bell's ClashInputs (one stream); SkipQuiet → the Province; SettleTransit (class D, not critical) writes the Holding, ClashInputs, the destination and home Provinces, the slot and the named recipients but **never the DefencePool** (routed tips and diverts go to `Holding.pool_owed`, swept by the class-N SweepPoolOwed); ProveBadSeal is gone. The combined 16-region PostAnchor is an optimisation with a one-account fallback. **Defence-pool refunds are never part of a critical transaction** (revision 3.1: they are later claims against per-keeper shards, §8.8), so no shared pool account is ever on the critical path.
3. Shared configuration (Season, FactionState, CivState, anchors, caches, archives) is **read only** on the hot path.
4. **No global counter.** Ids derive from the owner or the target. Join's land-reserve check reads a folded value (`FoldOccupancy`).
5. **Quotas are settled at submission** (A3): arrival slots at Reveal (by mass), slot quotas at Muster and Settle, board capacity at Petition.
6. Every instruction validates its exact account list, owners, magics and signer within its first ~1k CU.
7. **An action bucket per wallet** (in the Citizen: 30 actions per hour, burst 60), not per holding (revision 2).
8. **No critical path waits on a transaction our side cannot escalate.** Every critical write is sent by our players' keepers, who bid up to a tip and, during an attack, up to the defence-pool price (revision 3: the "or can be bypassed by inline verification" clause is deleted).
9. **For any future ER lane (X1):** assume every named account can be locked, read-only clones included.
10. **Pre-funding-safe initialisation (revision 3, blocker).** No program-created account uses System `CreateAccount` or `CreateAccountWithSeed`, which fail once the address holds lamports. The program transfers only the shortfall to rent-exempt, then **Allocate** and **Assign** (the `...WithSeed` variants for with-seed addresses), all signed by the program. A system-owned address holding lamports counts as **absent** in absence proofs and never blocks initialisation. **Test per init path** (Citizen, Holding, ArrivalSlot, PosturePDA, ClashInputs, SealVerdict, BellAnchor, SeedCache, AnchorArchive, Province; **M1: 17 account kinds over 19 creation paths**, gate G2, green at Gate W5): pre-fund the address, then initialise successfully; and pre-fund an address that is never revealed, then resolve with it counted absent.
11. **Canonical addresses on every read (revision 3, blocker).** Every reader recomputes the address of each keyed account it trusts (§8.2) and refuses any other; nobody passes a bump.
12. **Cluster parameters checked per season (revision 3).** Before each season, a read-only check that the deployment cluster enforces the per-account cost cap at the value the cost model uses (40% of the block limit today), tracked under R14.

**Per-account analysis on base (50k players; per-account cap 40M CU, block 100M CU)**

| Account | Honest writers and rate | Attack | Effect if held | Blast radius |
|---|---|---|---|---|
| Holding, Citizen | owner, delegate, captor; ≪ 1/s | priced lock (one account, p × 40M/block) | the owner is delayed | one player |
| ArrivalSlot (×4 per faction per province-bell) | one revealer | priced lock against the tip; **all 24 slots of a province-bell (or more) in one multi-lock stream at one account's price, p × 40M per block** [measured, SP-FEE (b)] | that arrival waits; if still held at `A + W`, it is routed (the priced attack C4 covers) | one province-bell |
| **Keeper fee payer** (new in 3.1) | one keeper | priced lock: listed as a writable key in the attacker's fillers, ~20–60 payers per stream at one account's price [measured, m0c drill (g)] | every write that payer pays for waits | everything that payer pays for; hence ≥ 150 rotating payers per keeper fleet (§6.4) |
| PosturePDA | one defender | priced lock against the tip | that posture waits (Disarray if never revealed) | one host |
| ClashInputs | gatherers | priced lock | that province-bell's resolution waits | one province (delay only) |
| Province | residents, resolvers, gatherers of arrivals from it | priced lock | resolution and resident actions wait; timers paused; reveals unaffected (Reveal no longer reads the origin, 3.1); destinations gathering its departed hosts wait | one province and the resolves waiting on it (delay only) |
| BellAnchor (×16 per bell) | keepers | priced lock against escalating keepers (predictable address since 3.1; pre-holding it only delays) | that region's windows stay open and its seed round moves later; nobody can choose among published seeds | one region, delay only |
| SeedCache (≤ 256 per region-bell) | keepers, any nonce | would need all nonces: whole blocks | a keeper uses another nonce | none |
| ~~SealVerdict~~ (removed in M1) | — | — | M1: the proof is inside SettleTransit, so there is nothing separate to hold | — |
| FactionShard | ~520 citizens each | priced lock | 1/16 of a faction's credits delayed (values fixed, §8.4) | 1/16 of one faction |
| FactionState, CivState, MarchState | hourly folds | priced lock | stale by an hour | one faction or March |
| PoolShard | traders pick any shard | priced lock | traders use another shard | none |
| JoinShard, VaultShard, Frontier (occupancy) | joins, folds | priced lock | joins hashed to that shard wait; the reserve check uses the last fold | 1/8 of one faction's joins |
| Tallies, boards | voters, petitioners | priced lock | votes wait; terms are 4 days | one office |
| Season.`outstanding` | claims | priced lock | claims wait | claims |
| *Any address, by pre-funding* | — | sending lamports in advance | **nothing** (rule 10) | none |

**No account is free to lock, and no held account changes an outcome except by holding reveals past their close**, which is the priced attack C4 covers (revision 3.1: a held origin no longer does; §6.2). What revision 3.1 corrects in revision 3's framing: holding three or more accounts does **not** force whole-block pricing. One multi-lock stream holds up to ~20 (legacy) or ~60 (lookup table, **[unverified]**: the v0 + lookup-table path never landed locally, M1 CL-27) *known* accounts at one account's price [measured for 20, SP-FEE (b) and m0c (g)]; whole-block pricing, with its world-wide delay of every low-bid Frontier write, is forced only when the attacker cannot enumerate the writes: sealed destinations **and** unpredictable fee payers.

**The M1 lock table (M1, I-48, I-21; normative in contract §8.8).** M1 removed every write of a world-wide account from the reveal, gather, resolve and settle paths: revision 3.1 would have written the DefencePool in every SettleTransit and Frontier and one ProvinceFund in N-class openings, so one account at a fixed low bid could have stalled every settlement in the world. The accounts M1 writes, who defends them and what holding them does (price per 10 minutes on §8.7's model at the keeper cap of the writer's class; only writes count against the per-account cap):

| Account (writers) | Keeper class, cap | Effect while held | Price per 10 min | Blast radius |
|---|---|---|---|---|
| ArrivalSlots of a province-bell (Reveal) | W, 2.0 | reveals wait; routed if still held at the close | $9.9k–27.2k | one province-bell |
| Keeper fee payer (all keeper writes) | W 2.0 / D 0.5 | that payer's writes wait | whole block while ≥ 150 rotating reveal payers are funded | none while the effective N ≥ 150 (I-49) |
| Province (players P; FileTicket P; gathers, resolves, skips, settles, SettleTicket D) | D, 0.5 | resolution, settlement and ticket finality wait; a lottery winner changes only if held for the cohort's 24 bells | $2.5k–6.8k; 24 bells ≈ $59k–163k | one province (delay) |
| ClashInputs (GatherClash, ResolveFromInputs, SettleTransit) | D, 0.5 | that resolution waits | $2.5k–6.8k | one province-bell (delay) |
| BellAnchor, SeedCache nonce | D, 0.5 | windows stay open longer, the seed comes later, keepers switch nonce | $2.5k–6.8k | one region (delay) |
| Frontier (OpenRing, FoldOccupancy) | D, 0.5 | ring openings and folds wait; Join uses the last fold | $2.5k–6.8k | world-wide ring growth (delay) |
| ProvinceFund(w) (OpenProvince, CloseProvince) | D, 0.5 | openings in that wedge wait | $2.5k–6.8k | one wedge (delay) |
| RingSeed(d) (ConsumeRingSeed, OpenProvince) | D, 0.5 | ring d's seed and provinces wait | $2.5k–6.8k | one ring (delay) |
| JoinShard (Join P, SettleTicket D, ReleaseDormant N) | P / D 0.5 | joins hashed there and ticket settlement wait | player writes ≈ free (priority 0, as §8.6's Holding row accepts); SettleTicket $2.5k–6.8k | 1/8 of one faction's joins |
| Citizen, Holding (player P; SettleTicket, SettleDeparture, SettleTransit D) | P / D 0.5 | that player waits | P ≈ free; D $2.5k–6.8k | one player |
| DefencePool (ClaimDefence D; SweepPoolOwed, dormancy and closes N) | D 0.5 / N | claims and sweeps wait; a claim expires after its 6-bell grace | $2.5k–6.8k, ≈ $15k–41k to void one grace | the refunds of one bell-region |
| *Any address, by pre-funding* | — | nothing (rule 10) | — | none |

No W or D instruction except ClaimDefence writes the DefencePool, and none writes a world-wide counter other than Frontier (class D, priced above). The prices are this document's model, not measurements; the local stack's adversary schedule (M1 E5) holds each of these accounts and reports what the hold delayed.

### 8.7 What an attack costs [model: SP-FEE `results/c4-model.txt` v2 (m0c); supersedes `rev3-results.txt` §A–§E and `rev2-results.txt` §A]

Inputs: SOL = $150. **Priority = (priority fee + 2,500) / (CU limit + 720 per signature + 300 per write lock + loaded-data cost)** [measured, SP-FEE]; a keeper spending its whole tip has priority `(tip − 2,500) / (limit + 1,336)` with a 64-KiB loaded-data limit: **0.433 at a 16k-CU Reveal, 0.35 at 20k, 0.27 at 26k**. *M1 correction (I-45):* the real program is 480–541 KB and SIMD-0186 counts its ProgramData, so the loaded-data term is `8 × ⌈L(kind)/32,768⌉` with `L` ≈ 1 MiB (+256 instead of +16); the priorities move by < 1%. **M1 final (§22.2) [measured inputs]:** the Reveal limit is 26,500 CU and `L(reveal)` 1,146,880 B (+280), so a Reveal costs **28,100** cost units (**28,400** on the first reveal of a province-bell, which also writes the ArrivalDay); a keeper that spends exactly `tip_min` = **14,668** lamports bids **0.433** (0.428 on a first reveal). The 0.27 rows below belong to revision 3.1's fixed 10,000-lamport tip, which M1 replaced by the minimum in priority terms. Known accounts: **p × 40M CU per block for up to ~20 (legacy) or ~60 (lookup table, [unverified], CL-27) accounts per stream**; unenumerable writes: **p × 100M per block minus organic cost bidding above p** (3.8M at p = 0.433, 0.7M at p = 2.0 [measured, read-only mainnet, 24 blocks]). Three capacity cases: execution-bound [estimate: 55M CU/s on one account, 150M CU/s per block, 0.265-s slots], the design's 100M per 0.4 s **[model]**, and 100M per 0.265-s slot if the limits are not rescaled for 250-ms slots **[model]** (the 250-ms feature is active on mainnet; the rescale is not verified, M1 CL-25). Every 0.4-s row in this section, in §8.9 and in the C4 tables is a model row.

| Target (600-s window) | Keeper priority | Attacker pays |
|---|---|---|
| One province-bell's slots, one anchor, one Province, or **one known fee payer** (one stream) | 0.433 (default tip, 16k Reveal) | **$2.1k–5.9k** |
| same | 0.27 (default tip, 26k Reveal) | $1.4k–3.7k |
| same | 2.0 (defence-pool cap) | $9.9k–27.2k |
| **A whole side** (sealed destinations, ≥ 150 rotating payers) | 0.433 | **$5.1k–14.0k** |
| same | 0.27 | $3.2k–8.8k |
| same | 2.0 | **$26.5k–67.4k** |
| Fake a 7-day beacon outage (Abort) | keepers 0.01–1 | ≈ 1,500–150,000 SOL [model, `rev3-results.txt`, unchanged method] |
| *Revision 3's rows* ("4 slots = whole block", $4.3k–8.3k per bell at 0.19–0.37 lamport/CU net of the base fee, 0.4-s slots only) | — | **withdrawn**: wrong bid metric, wrong multi-account pricing |

**Measured on the local validator** [SP-FEE drills (a)–(g), `results/drill-summary.md`]: spend per slot 1.01–1.16 × p × the binding cap; a multi-lock stream held 4 slots for 0.0403 SOL per slot (one slot's price); 20 known fee payers held by one stream; a keeper that outbids lands within 1–2 slots against one account and in 3.7 s against whole blocks. **Not measurable locally:** Jito bundles, leader discretion, stake-weighted ingress, Firedancer's pack, execution time in a 265-ms slot, whether leaders fill 100M per slot; mainnet blocks carry 10–34M CU [measured, read-only]. The M4 soak decides.

### 8.8 Costs and who pays [model: `model-results.txt` §2–§4; `rev2-results.txt` §E; SOL = $150]

| Players | Players' refundable deposits | ProvinceFund (operator, refundable) | Operator keeper (beacons, March and faction folds, pools) [estimate] | Sponsoring every player's fees | Operator income (20%; upper bound at day-0 prices, 30% of wallets staking) |
|---|---|---|---|---|---|
| 1,000 | 7.9 SOL | 3.5 SOL | ≈ 1 SOL | 3.4 SOL | ≤ $1,160 |
| 10,000 | 79 SOL | 32 SOL (105 SOL escrowed at `R_fund` 40) | ≈ 1.5 SOL | 34 SOL ($5.0k) | ≤ $11,600 |
| 50,000 | 395 SOL | 164 SOL ($24.6k) | ≈ 4.5 SOL | 168 SOL ($25.2k) | ≤ $58,000 |
| 100,000 | 790 SOL | 320 SOL ($48k) | ≈ 8 SOL | 336 SOL ($50.4k) | ≤ $116,000 |

- **A player** locks ≈ 0.008 SOL (M1: 0.0098 SOL, §8.2) of refundable rent and spends ≈ 0.0034 SOL (≈ $0.50) of fees per season at 40 tx/active day; a heavy automated player at 120 tx/day spends ≈ $1.50.
- **Resolution is paid by the arriving side:** each march escrows a 10,000-lamport march fee, paid to the resolver at `SettleTransit`, so an attacker who forces every province to resolve every bell (154 SOL a season at 50k) pays for it; the operator does not.
- **Default (D4):** the relay fronts rent (recovered at close) and sponsors 40 transactions a day for 7 days, then 20. Sponsoring everything would cost ≈ 45% of operator income at the revision-2 prices (less income per player than revision 1); at 1,000 players sponsorship plus keeping would exceed half of income.
- The operator also pays Shade entry (not recovered) and the RPC and indexer.
- **Beacon keeping (revision 3):** ≈ 4,600 beacon posts a day at ≈ 332k CU each on SBPF v2: ≈ 0.023 SOL a day of base fees, plus priority fees of ≈ 0.15 SOL a day if keepers bid 0.1 lamport/CU (≈ 0.7–5 SOL a season) [derived]. Anchors and caches are closed after archiving, so their rent is recycled.
- **Defence pool (revision 3, O7; specified in 3.1):** the operator escrows 20 SOL at `CreateSeason` by default (refunded if unspent); faction members may top up their faction's sub-pool. Keepers bid up to priority 2.0 and later claim the part above the tip from per-keeper claim shards, only for writes that landed late by on-chain evidence, within per-bell and per-keeper caps (§6.4). The subsidy is ≈ 9.5k–30k lamports per defended Reveal and ≤ 0.02 SOL per attacked bell [model, SP-FEE], so 20 SOL covers ≈ 1,000 attacked bells (**revision 3.1's figure counted only the target side's reveals; M1's re-size by write class is in §21.3, final with the measured Reveal in §22.3**); one block of attack at p = 2.0 costs the attacker what ≈ 3,600–5,400 defended Reveals cost the pool. Keepers also fund ≥ 150 rotating fee payers well before each bell.

### 8.9 Capacity: how many can play, and why (revision 3 numbers)

There is **no seat cap below the season's stated map capacity** (§3.4). Every instruction's work is independent of the number of players and the map size (B2). What grows is accounts (rent) and transaction rate.

[model: `lab/rev31/rev31-results.txt` (revision 3.1), same method as `rev3-results.txt` §F with the **SP-V2 budgets**: 100M CU per 400-ms block **[model; 250-ms slot rescale unverified, M1 CL-25]**; 20k CU per player instruction, 26k per Reveal, **650k per worst-case clash** (full gather: four gathers ≤ 30.3k + ResolveFromInputs ≤ 540.9k measured) — revision 3 used 270k from M0's stand-in kernel]

| Players | Honest peak (60% active, 120 tx/day, 3× peak) | Adversarial (every wallet at the bucket + reveals + every province resolving a worst-case clash every bell) | Resolve burst after a seed |
|---|---|---|---|
| 10,000 | 25 tx/s, 0.20% | 1.8% (1.4% with a 20/h bucket) | 9.8 blocks of CU, spread over the next bell |
| 50,000 | 125 tx/s, 1.0% | **8.8%** (7.0% with a 20/h bucket) | 50 blocks |
| 100,000 | 250 tx/s, 2.0% | **17.5%** (13.8% with a 20/h bucket) | 97 blocks |

Revision 3's figures (6.9% / 13.7%) used the stand-in kernel's 270k; the adversarial resolve share rose 2.4×. At 0.265-s slots with unscaled limits every share falls by a third. The quiet-bell backlog (§6.3) is not in this table: a province untouched for a day needs up to 144 absence-proof resolves (≈ 10.5M CU) before a resident can act [model, §G]; M1 must shrink or budget it.

- **Unbucketed paths** are bounded by game objects (a Reveal, a Resolve, a Settle each succeed once per object). The remaining floor is failing transactions, priced like any Solana spam at 5,000 lamports each (1 SOL buys about 4 blocks of CU).
- **Binding limits, in order:** (1) the season's `R_MAX` and ProvinceFund; (2) base block share, which passes 10% at **about 60,000 players with the 30/h bucket and 75,000 with 20/h** under the adversarial assumption [model, `rev31`] (revision 3 said 75k–100k on the stand-in kernel); (3) travel (18 h rim-to-centre on foot at 100k); (4) the read path, which is **not built yet** (the current server breaks at 500–1,000 viewers).

**Comfortable design point, restated (revision 3.1):** thousands to about **50,000** in one world (adversarial 8.8% of block CU with the 30/h bucket), and up to about **75,000 with a 20/h bucket**, on the SP-V2 numbers; the M1 kernel optimisation targets (field-holding step 39%, engagements 42% of kernel CU) are what would move it back up. Still to settle: the full Reveal's CU, the quiet-bell proof, and the M4 fee-market soak. Beyond that: the ER lane (X1, a later extension) or continents (X3).

### 8.10 Lifecycle and escape hatches

Season states: Open → Running (joins until day 21) → Ended (T_end) → Banking (72 h) → Revealing (≤ 24 h) → Finalized → Claims (90 days) → Closed.

| Trigger | Result |
|---|---|
| No `FinishSeason` by T_end + 10 days | anyone may `Abort` |
| **No beacon round newer than 7 days in any of the 16 BeaconLog shards, and no newer valid round presented in the Abort transaction itself** (the seed-relay check: anyone can stop a false Abort by posting one recent round) | anyone may `Abort`. Revision 1's "24 h without a seed → Abort" is removed; during a real outage the game waits |
| **The genesis seed round is not posted within 7 days of `CreateSeason`** (revision 3: drand-only genesis) | anyone may `Abort` before joins open; nothing has been paid by players yet |
| **Genesis re-roll guard (M1, CL-24 / I-09)**: `AnnounceSeason(id, params_hash, t_create_min)` must precede `CreateSeason` by ≥ 24 h (M1: signed by the program's upgrade authority, I-51); it fixes the genesis round from `t_create_min`; the season id is single-use (the Season PDA stays as a tombstone after Abort) | a **pre-join Abort after the genesis round is public forfeits the creation bond** (default 1 SOL [design]; test SOL in M1) — so creating, seeing the seed and aborting to try again costs the bond and a new public id; the verifier lists every announced season and its fate |
| **Mandate claim deadline (M1 text for CL-15; kernel `MandateTerm::claim(now, …)`, `Reserve::final_sweep`)** | a term's Mandate claims close at the earlier of the term's end + one term and `T_end + 72 h`; the last term closes at `T_end`; every open term is swept **before** `FinalizeFaction`, and the reserve left at the sweep joins the faction's laurel-pool general split (not carried forward) |
| The season never meets its start conditions | anyone may `Abort` |
| A paid join with no site after 24 h | the citizen may `WithdrawJoin` (100%) |
| Any Shade leaf unrevealed after 24 h, or an Auditor-confirmed policy violation | the escrow goes to the pools; claims proceed |
| **On Abort** | every citizen reclaims **100% of what they paid**, funded by the escrowed 20% (I1, F17). The operator loses its share. |

No delegation in the Frontier, so no DLP escape is needed (I2 applies only to a future ER lane, X1). Every per-player and per-arrival account has a close path (I4).

---

## 9. Verification

### 9.1 What is checked

1. **Per-entity hash chains** (`event_head = sha256(prev_head ‖ event)`), cross-entity events chained into every account they touch (K5). The verifier replays per entity in parallel.
2. **Durable data:** base transaction logs; ≈ 1.2M transactions a day and 8–10 GB per season at 50k [derived].
3. **Rules:** one `permutation-rules` crate; ruleset hash bound at `CreateSeason` (J2).
4. **Randomness:** every anchor and seed cache is re-verified against the quicknet public key; **every anchor is the canonical one for its (bell, region)**; every seed round is checked to lie **at least Δ after** its bell's reveal close; the genesis and ring seeds are checked against their rule-fixed rounds.
5. **Sealed orders:** every seal is decrypted after its round; **every on-chain SealVerdict is re-checked** against stock tlock decryption plus the commitment opening (M1: every `TRANSIT_SETTLED` seal code is re-checked, and a bad seal whose transit settled any other way is `BadSealSurvived`, a FAIL); **every valid seal that was not revealed is flagged** as a keeper-liveness finding, with the transactions that tried to reveal it.
6. **Money:** every vault-shard flow reconciled; program id and upgrade history pinned (F7, K3).
7. **Settlement:** every fold, reward index, banking credit, Mandate-reserve split, office bound, Shade subtraction and claim closed form recomputed.
8. **Shades:** every Shade's actions replayed from its committed policy; divergences reported (§7.3).
9. **Fail closed** (K2); walk entity chains from on-chain heads (K4).

### 9.2 Light verification

A player can verify their own entities, the provinces they fought in and their claim from the browser.

### 9.3 Read path (not verification, but needed for scale; **not yet built**)

- A herald-style fold server serves a snapshot plus sequence-numbered diffs over WebSocket.
- A CDN snapshot per (province, bell); clients poll only on-screen provinces.
- Never a trust root: any snapshot can be checked against the account and its event head.

### 9.4 Determinism, fairness and safety tests (CI)

Kernel-level status on `codex/frontier` after the K3 merges and the m0c fixes [measured]; lab status from SP-V2; program-level versions come in M1.

| Test family | What it checks | Status |
|---|---|---|
| **Order independence** | every cross-entity step (arrival merge, displacement, site lottery, pool clears, folds, claims, GatherClash parts): 7+ random schedulers give identical state hashes (E2's method) | **green for the clash** (300 scenarios × 7 shuffles) and for sharded faction totals built in 7 orders; site lottery, Bourse clears, folds and GatherClash wait for their kernels (M1/M3) |
| **Roster freeze** | defender actions issued during bell b (merges, splits, Departs, garrison changes, walls, decrees) leave the bell-b clash unchanged | **green** (`actions_during_a_bell_do_not_change_its_clash`, with a control that fails) |
| **Lag invariance** | any delay pattern (0–50 bells of lag, reveals anywhere in their window) gives identical results, including two-province marches | **green** at kernel level (`clash_results_do_not_depend_on_lag`, `arrivals_carry_the_origin_clash_whatever_the_lag`, which fails without the origin check). **Program level (3.1, M1 gate):** hold the origin Province and the origin region's anchor past the destination's close; the result must equal the unheld run. **M1: green** (G7 in process and in LiteSVM, `g07_lag_gate_in_litesvm`: four cases, holds of 1–5 bells, stays, bounces and a destruction, every pair byte-identical) |
| **Quota fairness** | the final ArrivalSlot set equals the 4 largest same-faction arrivals whatever the reveal order | **green** (K3: `arrival_slots_are_the_four_largest_regardless_of_reveal_order`, 400 cases × 8 orders; a mutation that displaces the first slot fails it) |
| **Seed margin** (new) | seed time ≥ reveal close + Δ for every bell and region; Reveal refuses at `A + W` | **green** (K3 kernel test over 60,000 anchors; SP-V2 `seed_round_margin` and `t8`: A + 599 lands, A + 600 refused) |
| **No Reveal after the close** (new in 3.1) | no Reveal lands at or after `A + W` of THE anchor, whatever anchor or archive account is supplied or omitted; a closed anchor never reopens its window | **green in the lab** (SP-V2 `t3`, `t8`, `t9`; `t9` fails with the tombstone check removed); M1 gate as a property test; **M1: green** (G4 over random A, Clock, window schedules and supplied, omitted or forged anchors and archives) |
| **No choice among published seeds** (new) | whatever the anchor delay, no two different seeds for one (bell, region) are ever accepted | **green in the lab** (SP-V2 `t3`: 18 forgery and consistency checks; `t9`: no second anchor after archiving); M1 gate; **M1: green** (G5, G3 forgery per keyed account) |
| **Absence** (new) | a resolve that omits an existing ArrivalSlot or PosturePDA fails; a pre-funded, never-revealed address counts as absent | new (M1 gate); **M1: green** (G6, G2) |
| **Pre-funding** (new) | for every init path, pre-funding the address does not block initialisation | new (M1 gate), one LiteSVM test per path; **M1: green**, 17 kinds over 19 creation paths, plus a re-creation test per closable kind (G2, G3) |
| **Reveal latch** (M1, CL-23) | once a province-bell's ClashInputs exists (any gather ran) or its Province resolved or skipped past the bell, no Reveal for that bell lands, even with the Clock warped back before `A + W`; a forged ClashInputs address is refused | new (M1 gate G4: `reveal_refused_after_first_gather`); **M1: green** |
| **Seal verdict** (new) | the on-chain verdict equals stock tlock decryption plus commitment opening, for valid, garbage, wrong-round and tampered seals; **3.1:** a seal whose logged bytes differ from the committed ones, and a missing or forged commitment, are refused (the root does not match) | lab: SP-V2 `t6` covers valid, garbage, tampered V, bad U, wrong round and a seal differing from the logged one; the stock-tlock comparison and the forged-commitment case are M1. **M1: green** — G10 (`g10_settle_transit_seal_codes_match_the_stock_opener`: SettleTransit's code = the stock `tlock` crate + commitment + `Plain::validate`, before and after ArchiveAnchors) and G12 (settle races the proof, a Stays host cannot act before settlement) [measured, Gate W5] |
| **Gather equivalence** (new) | GatherClash + ResolveFromInputs gives the same result as single-transaction ResolveClash when both fit | new (M1); **M1: green** (G8: gathers in random order with repeats = the oracle build = the native kernel; the storage fill never exceeds 56 entries or 8 per faction) |
| **Quiet bells** | skipping a run of quiet bells equals resolving them one by one (`Siege::advance_quiet`) | **green** (kernel property test); **M1 program level green**: G11, SkipQuiet over random rosters, with and without pending ops and with rosters changed at every bell, is byte-identical to gathering and resolving each bell |
| **Stance equilibrium** | no stance dominated; Disarray strictly worse than revealing any stance | **green** (`stance_table_is_in_equilibrium`, in the CI job) |
| **Doctrine balance** (O5) | every doctrine's win rate in 16.7% ± 2 points over **≥ 1,500 paired seasons** (at 600 seasons the SE is 1.5 points and a perfectly balanced table passes 6/6 only ~30% of the time) | **6/6 on 1,500 paired seasons** (largest gap 1.1 points) [sim]. Per push (CI): a 180-season proxy, every mean index within ±0.2%, plus three negative controls that must fail (draft table, F on the Knight line, an in-bounds A boost); nightly and on demand (`doctrine-balance.yml`): the ± 2 band on the fixed confirmation set and the index bound on a fresh seed set |
| **Bot criterion** (new in 3.1) | max over the bot's join window × stake × office < 1.0 at 1/2/5/10% | `frontier-sim criterion --best-response --gate` [sim]; passes (worst 0.980); the Mandate-steering variant fails without the menu rule (§5.4) |
| **Kernel bounds** (new) | walls, production and upkeep deltas capped; `duplicate_cost` checked; `ProvinceCoord` ring-bounded; `clash::validate` refuses troops above `MAX_HOST_TROOPS`, unbounded `dealt_bps`, stamina above cap; `Stamina::set` refuses an earlier bell; faction ids in 0..=5 plus NEUTRAL; parameter `validate()` for bps and γ; overflow checks on in the Frontier release profile | in M1 wave 1 (units W1-A, W1-B; `M0-CLOSE.md` maps each to its commit) |
| **Economy conservation** | vault, claims, laurel totals; simulator checks written as bounds, not identities; the Ledger kept per faction | **M1 (CL-07, CL-08, simulator side): bounds** — each claim ≤ its rule-computed entitlement and ≤ 5× paid, claims + swept ≤ prize, each faction's claims ≤ its own pots, laurels credited ≤ emitted − orphaned, laurels held ≤ sources, Mandate paid + burned + balance ≤ deposited — **with mutation controls** (1 extra unit paid to one wallet, one laurel credited twice, one Mandate unit paid twice, a claim above 5×, a cross-faction over-claim) that each fail their bound; the identities stay as a second check [measured, `frontier-sim` tests]. The kernel `payout::SeasonLedger` is W1-B's |

---

## 10. Audit lessons, one by one

| # | Requirement | How this design meets it |
|---|---|---|
| A1 | No permissionless path to an unfinishable state | No global tick; every state machine advances by permissionless, idempotent calls; windows anchored to the beacon; every state has a timeout. **Revision 3:** pre-funding cannot block any account creation (§8.6 rule 10; before the fix, ≈ 41.9 SOL of pre-funding froze a season [derived]); the genesis seed has an Abort timeout (§8.10). |
| A2 | Lifecycle and layer checks | Every instruction checks season status. There is one lane in Season 1 (base). |
| A3 | Third-party data never breaks the critical path | Quotas settled at Reveal by mass; fixed-size result rings; boards with deposits; ClashInputs gathered idempotently. |
| A4 | No outcome-changing fallback | **Lag only waits** (per-slot arrivals, anchored windows, timers in resolved bells); no Abort on a 24-hour seed gap; **one network (quicknet) for anchors, seeds and seals**, no per-bell switch, **no inline verification path**; exactly one anchor per (bell, region); banking values fixed at T_end. |
| A5 | Ordered, idempotent wind-up | Close instructions idempotent (ArrivalSlot, PosturePDA, ClashInputs, SealVerdict, anchors and caches after archiving). **M1:** every one of the 17 account kinds has a close path (CloseArrivalSlot/Day, CloseClashInputs, ArchiveAnchors, CloseSeedCache, CloseProvince, CloseHolding, CloseCitizen, ReleaseDormant, and CloseSeason parts 0–10 for the float: RingSeeds, AnchorArchives, DefenceClaims); SealVerdict no longer exists (I-44). |
| A6 | Every creatable configuration tested end to end | Presets with golden end-to-end tests only. |
| B1 | Worst-case CU and heap measured | **SP-V2 re-measured on SBPF v2 with the real kernel:** clash 367k–633k (budget 650k), ResolveFromInputs ≤ 541k, heap 25.9 KB of 32 KiB, PostAnchor/PostSeed ≈ 332–336k, ProveBadSeal ≤ 48.0k, Join 14.4–25.0k (§8.3). Still estimates, gated in M1: the full Reveal, CommitPosture/RevealPosture, SettleTransit, ArchiveAnchors. **M1 (G1 on the release `.so`, every instruction at its §13.1 adversarial fill):** Reveal 25,155, SettleTransit (with the seal opener) 63,281, ArchiveAnchors 45,441, ResolveFromInputs 327,609 over 1,240 fills (Phase B 274,007), heap ≤ 15,320 B of the 28-KiB gate; every one of the 49 kinds within its gate (§8.3, §22.1). |
| B2 | Work independent of players and map | ≤ 48 hosts + 24 arrivals per clash, ≤ 4 gathers per province-bell, ≤ 16 shards per fold, ≤ 48 JoinShards per occupancy fold, ≤ 7 provinces per March fold. |
| B3 | Free actions count against caps | Per-wallet action bucket; each permissionless step succeeds once per object. |
| B4 | No on-chain search | Paths submitted and verified; addresses recomputed, never searched (with-seed, no bump search). |
| B5 | Splittable work | Per province, per March, per faction, per citizen; clash inputs split into gathers. |
| B6 | Bytes and logs | ≤ 1,232 B: ResolveFromInputs ≈ 390 B, gathers 1,187 B, PostAnchor 709 B, PostSeed 710–806 B, ProveBadSeal 588 B, Join 750 B [measured, SP-V2]; Depart ≈ 699 B with the commitment [estimate]; the single-tx ResolveClash (1,182 B) is only a test oracle; logs < 1 KB. **M1:** every kind within its tx ceiling and 1,232 B (Reveal 916–928 B, SettleTransit ≈ 990 B, GatherClash ≤ 1,232 B, ResolveFromInputs ≤ 472 B), ≤ 64 locks, and a per-kind loaded-data limit `L(kind)` from the release program (I-45). |
| C1 | No free lock of a shared critical account | World on base; critical writes defended by escalating keepers and the defence pool; no critical path through a non-escalating third party; **no MagicBlock VRF anywhere** (O2). |
| C2 | Player writes to own or sharded accounts | §8.6 rule 1. |
| C3 | Shard hot base accounts | 8 vault shards, 48 JoinShards, 96 FactionShards, 96 PoolShards, 16 regions, ≤ 256 SeedCaches per region-bell. |
| C4 | Blast radius | **Restated (O7):** excluding every reveal and beacon post bid at the default keeper price for one bell must cost ≥ 10× the value of all clashes resolving in that bell for the targeted side. **Not met at the default tip [model]**; met at the defence-pool cap 2.0 with ≥ 150 rotating fee payers on every valuation [model]; unverified until the M1 Reveal CU and the M4 soak (§6.4). **M1: re-run with the measured Reveal CU and `L(reveal)`; the verdicts stand (§22.2); the M4 soak still decides.** Known accounts (one province, one anchor, one payer) cost p × 40M per block for up to 20–60 of them per stream. |
| D1–D3 | Authentication, session keys, token checks | Ported from v9 and **reviewed fresh in M2** (the v9 waves that hardened them were not finished). |
| D4 | No lockouts or side channels from capacity | Capacity stated at creation; Join refuses without charging when full (folded occupancy); paid joins get a site or a refund. |
| D5 | Relays can pay but not forge | Session key checked, not the fee payer. |
| E1 | Operator cannot know or choose randomness | drand quicknet rounds fixed by rule after inputs close, with a ≥ 60 s margin; genesis from drand alone; one canonical anchor; anyone relays. |
| E2 | Commitments closed at deadlines | Marches commit at departure; postures before the bell; reveals until the anchored close, refused at `A + W`. |
| E3 | Operator never sees salts | Salts in the client, a chosen relay or the seal. |
| E4 | No free withholding option | Mandatory seal with keeper tips; the commitment and seal are both in Depart's data and the program computes the root, so **a bad seal is judged and destroys the host** (3.1: ProveBadSeal; **M1: SettleTransit itself opens every seal**, I-44); 50% rout for unrevealed valid arrivals, Disarray for postures; `retreat_ratio` for everyone. Residual: a zero-tip owner keeps a priced option (§6.4). |
| E5 | Randomness gaps visible | Anchors and seeds re-verified; outage → wait; late anchors visible in the logs; archived anchor tables. |
| E6 | No privileged operator-AI path | Same instructions; committed policy; replay. |
| F1–F7 | Money | Conservation checked at every out-flow; fee escrow until settled; wallet cap 5× paid; officer pay within a ceiling of 95% of what the wallet paid; dust, ceiling cuts and cap excess swept to the next season. |
| G1–G6 | Hidden roster | Blinded leaves with policy commitments; bond exceeds the maximum withholding gain; score facts voided. |
| H1–H5 | Governance | Two Ministers for the Granary; caretaker first term; pro-rata steward pot with the 95% ceiling; Mandates from a fixed menu with a share floor; weight by ballots cast; recall needs Assembly weight; deposits everywhere. |
| I1 | Timeout → Abort → full refund | §8.10. |
| I2–I5 | Escape, validator, rent, sponsored commits | No delegation in Season 1, so I2 applies only to X1; rent close paths for every short-lived account (I4). |
| I6 | Operator delay bounded | Every step permissionless except the Shade reveal (bonded) and the Auditor (forfeiture only). |
| J1–J5 | Upgrades and trust roots | Named trust roots: **drand League of Entropy** (randomness, genesis and seal secrecy), **Solana leaders** (censorship) and **the cluster's cost-tracker parameters** (the account cap, checked per season, R14), **the Shade Auditor** (forfeiture only). MagicBlock is no longer a trust root in Season 1. |
| K1–K6 | Verification | §9. |
| L1–L2 | Tests | One LiteSVM test per (instruction, error), incl. pre-funded addresses and non-canonical addresses; golden replays; §9.4 tests. |

---

## 11. Reuse of the current code

### 11.1 `permutation-rules`

| Keep as is | Refactor into entity kernels | Replace |
|---|---|---|
| `fixed`, `hex` (+ bound), `rng`, `hash`, `decision`, `units`, `buildings`, `tech`, `combat` (**linked unchanged in the E3 and E4 SBF probes; the Frontier clash keeps both halves of `resolve_engagement`**), `scoring::{milestone, tiers, era}` (tiers now used only for display), `markets::clear_amm` price math, `roster::{roster_tag, roster_link}`, `params`, `RULES_VERSION` binding | `economy`, `tick::production`, `battle`, `diplomacy`, `checks`, `merit` → laurel reward index and Works, `mapgen`, `invariants`, `preview`, `history` | `state::WorldState`, `tick/*`, `gov/*` layout, on-chain `movement` A*, `standing`, `genesis`, `map::Map`, `scoring::facts_all` and share-of-totals `score_of` (replaced by the per-capita index), `payout::settle_with` loops, unblinded `roster_chain` |

**Status (revision 3.1).** K3 added `frontier::doctrine` (the table, bounds and `validate_table`, now with a variant weight and the same-army check), `frontier::mandate` (the closed-form reserve, now with the share floor and its burn), `payout::office_ceiling_bps`, order-weighted emission, the stake ramp, `clash::admit_arrival` / `quota_set` and the seed-margin functions; the simulator gained the best-response criterion, the C4 counterfactual and the gate controls. **Status (revision 3).** The `frontier` module tree exists on `codex/frontier` at `cea89be`: `geometry`, `terrain`, `holding`, `host`, `stance`, `clash`, `siege`, `travel` (K1) and `pools`, `laurel`, `index`, `payout` (K2), with 47 world and 21 economy integration tests; 338 `permutation-rules` tests pass in debug and release [measured]. `params::RULES_VERSION` is still 9 and the v9 golden replays pass, so v9 behaviour is unchanged. Still to add: `frontier::seal` (the pinned seal format), the Reveal-time slot ranking, the Mandate-reserve closed form, order-weighted emission, and the §9.4 bounds. Kernel notes to settle: `Holding.order` is 1-based while `laurel::strength_weight` takes a 0-based order (the simulator passes `order − 1`); `laurel::Tier` duplicates `holding::Tier`; `RewardIndex`'s boolean `emits` cannot express order-weighted emission. The v9 `.so` grew by 1,416 B after the module landed although no Frontier symbol is linked (generic instantiations) [measured]; v9 is stopped, so this matters only for hygiene.

### 11.2 `permutation-chain` (v9) → new `permutation-frontier` program

- **v9 stays as it is** on `codex/v9-security` (`4ec9e8d`, after wave 2 of 5). The round was stopped by the owner on 2026-09-27; v9 is neither finished nor revived as Skirmish (O8).
- **Ported as reference code, reviewed fresh in M2:** `token.rs` with owner and mint checks; vault, claim, `outstanding`, `WithdrawOps` (WP12); Abort and refunds (WP14); the blinded roster (WP09); the upgrade guard and ruleset binding (WP15); error-code discipline; log format and "alone" rule; the svm-tests harness (WP18). Several of these fixes (P2 token checks and session keys, P3 solvency, Abort and refunds, roster reveal) were in waves 3–4 that will not happen, so **none of them is treated as hardened**.
- **Not ported:** the VRF plumbing (WP11) — the Frontier no longer uses MagicBlock VRF (O2). Account creation helpers that call System `CreateAccount` are replaced by the pre-funding-safe init (§8.6 rule 10).
- **Promoted from the spikes (revision 3.1: from SP-V2, which already has the fixes):** the hinted quicknet verifier (v2 build), the on-chain opener with the FO check, the pre-funding-safe init paths, the canonical with-seed addresses and absence rule, GatherClash / ResolveFromInputs around the real kernel, and the anchor tombstone (`scratchpad/frontier/m0b/spikes/SP-V2/program/src/`).
- **Dropped for the Frontier:** world chunks, `ResolveTick`, `LogTickInput`, nation accounts, seating, world delegation.
- **New program id** (J1), built for **SBPF v2**.

### 11.3 Gateway, server, web

| Component | Fate |
|---|---|
| Gateway x402 join | kept (Join), default path for everyone including Shades |
| Gateway relay with co-sign shapes | kept, plus per-citizen sponsorship quotas and relay-fronted rent |
| Gateway crank | becomes a keeper: beacon anchors (combined and per region), seed caches (any nonce), `ArchiveAnchors`, GatherClash, clashes, transits (M1: SettleTransit is the seal proof; `ProveBadSeal` removed), pools, folds (incl. `FoldOccupancy`), rings, seal reveals, and escalation from the defence pool. Anyone may run one; tips make it worth running |
| Gateway `sealed.mjs` | retired (no Skirmish) |
| Gateway talk anchoring | kept, per channel |
| Server bots | port to entity actions; become the committed Shade policy and the public bot SDK |
| Server replay verifier | per-entity parallel verifier (v2) with Shade replay, seal audit and SealVerdict re-check (M1: `frontier-verify`, V1–V13; the settlement seal code is re-checked against the stock `tlock` crate, `BadSealSurvived` fails) |
| Server state API | the herald fold with a CDN cache per province per bell |
| Web client | hex renderer, panels, i18n and wallet flow reused; new map, holding panel, march/posture UI with tip and retreat ratio, vigil setting, Bourse, governance ladder, join page with the published payout table and capacity; **off-chain practice mode** running the clash kernel in WebAssembly (§2.8) |

### 11.4 Lab probes to promote

- `lab/E4-free/e4probe` → clash, province, zero-copy layouts, CU gate.
- `lab/E3-hybrid/e3probe` → governance and folds.
- `lab/E1-async-entities/e1probe` → account-validation prologue.
- `frontier/m0/spikes/S-BEACON/program` → PostAnchor/PostSeed with hints; **`S-TLOCK/tlockv`** → ProveBadSeal (M1: the opener inside SettleTransit); **`S-SIZE-JOIN/probe`** → Reveal, GatherClash, ResolveFromInputs, Join, SettleTicket. Each needs the pre-funding-safe init and canonical-address checks before promotion.

---

## 12. Milestones

The deadline is ignored, as the owner asked. Weeks are from the start of work [estimate], assuming the team that ran the v9 round. **There is no Skirmish, so there is no live game between now and the M1 playtest** (O8). Every devnet step and any push needs the owner's separate approval.

**M0 status (M1 wave 1): closed in M1's first week and a half** (owner decision N5: M1 started at once and the remaining M0 items were closed inside it; the closeout took 1.5 weeks, not 1). `m0/M0-CLOSE.md` maps every M0-FINAL §5 item to its task, commit and test. **M0 status (revision 3.1): not complete.** The second pass (m0b, m0c) redid SP-V2 and SP-FEE, ran K3, and fixed the review's findings; exit (c) is restated below because, as written, it fails at the default tip. **Earlier status (2026-09-27, first pass):** 1 of 5 exit criteria met, 3 partial, 1 not met (`scratchpad/frontier/m0/M0-REPORT.md`). The kernels, the simulator and four spikes exist; the reviews found five blockers: two fixed in the kernels at `cea89be`, two fixed by this revision's design (pre-funding, anchor uniqueness), and one (the S-FEE drill never created contention) that needs the drill redone. The table below restates M0's exit and adds the work to close it, so later milestones move by about two weeks.

| Milestone | Weeks | Scope | Exit criteria |
|---|---|---|---|
| **M0 Foundations and spikes** (first and second pass done) | 0–7 | **Done:** rules v10 kernels (K1, K2); host simulator; S-BEACON, S-TLOCK/S-TLOCK-V, S-SIZE+S-JOIN; a first S-FEE (verdict withdrawn). **To close M0:** (1) this revision; (2) patch the spike probes with pre-funding-safe init, with-seed addresses and the canonical anchor, and re-run S-SIZE R0/R2 and S-BEACON PostAnchor; (3) **re-measure every CU row under a pinned SBPF v2 build**; (4) **redo S-FEE** with fills that consume their full declared CU (the local validator needs a dynamic port range of ≥ 25 ports and puts pubsub on RPC port + 1), for 1, 2 and 4 accounts and the whole block, plus the **anchor lock drill** (S-BEACON D1–D6: pre-held anchor at several keeper caps, held SeedCache with nonce switch, held Province, combined 16-region anchor with fallback, unpredictable anchor address, Clock-vs-drand skew), on ports outside the reserved list; (5) the Reveal ranking kernel with the quota-fairness test, the seed-margin test, the §9.4 bounds and the Mandate-reserve closed form; (6) doctrine tuning to 6 of 6 and the reduced doctrine CI job; (7) the bot plan steps 1–3 in the simulator; (8) written S-BEACON and S-TLOCK reports. S-VRF-LOCK is **cancelled** (O2). | (a) Spike numbers recorded, **CU rows on SBPF v2** — met (SP-V2); (b) stance **and doctrine** balance: the CI proxy gate with its negative controls, and 6/6 in 16.7% ± 2 on ≥ 1,500 paired seasons in the scheduled workflow (§4.1); (c) **restated (3.1): C4 must pass at the p99 and max bells of `frontier-sim c4` with the defence pool at cap 2.0 and ≥ 150 rotating keeper fee payers, on the valuation the owner accepts** — **met in the model on valuation (a)** (N2, the owner's choice) and on every other valuation [model], **unverified until the M1 Reveal CU and the M4 soak** (M1, CL-37); **the default-tip shortfall is a recorded result** (C4 not met at the default tip); the anchor drill shows no choice among published seeds (met: predictable anchor, tombstone, delay only); (d) β measured and γ set — **met in simulation** (β 0.86–0.88, ratio 0.684 at γ = 0.6); (e) §9.4 kernel tests green, including quota fairness and seed margin — met at kernel level. **GitHub CI green** once the owner approves a push |
| **M1 "First Bell": the first playable** | 7–15 | Program core on base (no money), SBPF v2: Join (free), Citizen, Holding (keyed by site), Province with the garrison mirror, ArrivalSlots and PosturePDAs at with-seed addresses, site tickets with displacement, FoldOccupancy, Harvest/Build/Train, Muster, Depart/Reveal with the mandatory seal and tips, **GatherClash / ResolveFromInputs / ResolveClash**, **ProveBadSeal / SealVerdict** (**as built, M1 contract v1.9: no PosturePDAs (M3); ProveBadSeal and SealVerdict replaced by SettleTransit as the seal proof; ArrivalDay + SkipQuiet as the quiet-bell proof; SettleDeparture; ticket cohorts; ClaimDefence with a claim grace; Mode A 7-day local season on LiteSVM with real 400-ms slots; §22**), SettleTransit, Explore, **PostAnchor / PostSeed / ArchiveAnchors** and the genesis and ring seeds from quicknet, OpenRing/OpenProvince, dormancy, logs, verifier v2 with seal audit, the quiet-bell proof chosen and budgeted. Web: map, holding panel, march UI, **guided first bells and the off-chain practice mode** (no Skirmish). Herald fold and keeper with defence-pool escalation. | Every instruction under budget on SBPF v2 with adversarial fill; **a pre-funded-address test per init path and a non-canonical-address forgery test per keyed account**; a 7-day local season with 1,000 bots (on ports outside the reserved list, with a validator that has the BLS12-381 syscalls); verifier PASS, FAIL on tampering. **Then, only with the owner's approval, a private devnet playtest with 50–200 people and no money.** |
| **M2 Money and lifecycle** | 13–19 | Citizen fee and laurel stake with the schedule and escrow (late stakes counted from the stake point), vault shards, escrowed operator share, FactionShards, reward indices, banking window, FinalizeFaction, closed-form Claim with the 5× cap and the office bound, Abort/Refund/WithdrawJoin (incl. the genesis-seed timeout), ProvinceFund, defence pool, rent close paths, Shade roster with policy commitments, reveal, replay and bounties, relay sponsorship. **Fresh review of every v9 module ported** (token checks, solvency, Abort and refunds, roster reveal). | Conservation property tests and fuzzing; one test per (instruction, error); WP01–WP18 repros ported where they apply; abort from every state; a full season end to end in LiteSVM |
| **M3 Society** | 17–25 | Wardens, Assembly (ballot weights), Ministers with bounded pay, decrees, March truce/hostility, Mandates with the staker-only laurel reserve, petitions and recall, Companies, delegation, postures, sieges with vigil and auto-reinforce, occupation/capture with pair rules, caravans, the Bourse, the Engine, Eras, Civilisation Share, Relic Sites, doctrines, crisis, accusations, Chronicle; public bot SDK. (No Arena.) | 10k-agent simulation: **a scripted wallet returns < 1.0× at 1/2/5/10% bot shares as the maximum over its join window, stake and office choices** (`criterion --best-response --gate`), **with Mandate types modelled and bot officers choosing them**; bots' share of offices reported; measured payout table by archetype and join day; β re-measured with real players in the M1/M3 playtests and γ confirmed; capture simulation with real seat counts; determinism tests green |
| **M4 Hardening and beta** | 25–32 | Adversarial soak (locks on slots, provinces, anchors, caches, shards, **keeper fee payers**; whole-block pricing; drand outage replay; Abort drill), **a devnet or mainnet-fork soak for Jito, multi-leader behaviour, the per-account cap and the leader execution rate in a 265-ms slot — the deciding test for C4** (owner approval per step); external audit, **including an outside review of the seal envelope**; **legal review of the fee and prize model**; devnet beta with test USDC, 500–3,000 people and bots, `R_MAX` 12 (owner approval for every devnet step). | No open critical or high finding; C4 holds in the soak; beta completes, every claim paid, verifier PASS; costs within ±30% of the model; legal sign-off |
| **M5 Mainnet Season 1** | 32–36 | Frontier-28 preset, citizen fee 4 USDC + stake 6 USDC, Shades 0.5%, `R_MAX` 64 with `R_fund` sized from pre-registrations, γ = 0.6, quicknet, SBPF v2, per-season cluster-parameter check. | Season completes with verifier PASS; solvency monitored; ≥ 99% of claims within 7 days |

**Extensions after Season 1:**

| Extension | What | Gate |
|---|---|---|
| **X1 Frontier on a gated ER** | Same program on a dedicated MagicBlock ER behind E4's ingress rules; clients publish signed transactions to a public log | MagicBlock dedicated node with an ingress filter, or a licensed self-hosted validator; S1 lock drill; mass delegation rehearsal (validator#1603); a fresh randomness review (no VRF on the critical path) |
| **X2 Private-ER sealed reads** | Arrival slots readable only after the bell: removes the remaining zero-tip withholding option | MagicBlock PER read permissions |
| **X3 Larger worlds and continents** | `R_MAX` up to 128; beyond ~75k–100k players, continents joined by sea lanes | Travel and load tests |
| **X4 Sealed Bourse orders, regional markets** | Sealed orders with the same seal format; capital and rim markets | Balance |
| **X5 Chronicle features** | Cross-season monuments and titles (history, not power) | Design review |
| **X6 A short-format game** | A separate quick game (the O8 alternative to a revived Skirmish) | Its own design and review |

---

## 13. Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | **The fee market behaves worse than modelled** (C4 not met at the default tip [model]; known accounts, including keepers' fee payers, cost one account's price; leader execution and bundles unknown) | high / high | Defence pool with the refund rules; ≥ 150 rotating keeper payers; priority-term tips; longer windows as a season parameter; M4 soak decides; X1 later |
| R2 | **Scope.** A new program and ~60% new rules code; ~8.5 months to mainnet | high / high | M1 playable at week 15; probes and kernels as seeds |
| R3 | **Complexity** drives casuals away, and there is no Skirmish tutorial | medium / high | Progressive disclosure; 5-noun newcomer UI; guided first bells; off-chain practice mode; delegation; playtests at M1 and M3 |
| R4 | **Operator economics** (smaller income per player than revision 1; sponsorship ≈ 45% of income; at 1k players costs exceed half of income; the defence pool is an extra escrow) | medium / medium | Quotas; players pay beyond them; 14-day preset for small seasons; the defence pool is refunded if unspent |
| R5 | Rent float (ProvinceFund ≈ 105 SOL escrowed at `R_fund` 40; anchors ≈ 62.9 SOL a season if never closed) | certain / low | Refundable; ArchiveAnchors closes anchors after 48 h; SIMD-0437's later steps would cut rent ~7× |
| R6 | **drand outage or League of Entropy compromise** — now the single randomness root, including genesis | low / medium | Game waits (windows anchored); 7-day Abort with seed-relay check; genesis timeout Abort before joins; disclosed trust root |
| R7 | tlock trust (early decryption by threshold collusion) | very low / medium | Manual reveal option; disclosed |
| R8 | **Bots and multi-wallet whales** — the criterion passes in simulation with a ~2-point margin at the bot's best choice, **fails if bot officers can steer Mandates** to always-online tasks, and bots hold most offices at low shares [sim] | likely / medium | 95% officer-pay ceiling (O3); K3 economy (O4); fixed Mandate menu and share floor; **one office-term per wallet (D23, decided), with vacant seats in small seasons**; publish the measured multiple and the bots' office share |
| R9 | Governance capture at low turnout | medium / low | Weight by ballots; recall needs Assembly weight; capped and bounded pay |
| R10 | **Balance** of a lazy economy with bells and six doctrines — doctrine win rates are very sensitive (a 0.25% index edge moves a win rate by ~4 points [sim]); **much of the table is balanced by argument only** until the unsimulated mechanisms land | high / medium | No multipliers on scored facts (O5); the table is provisional; CI proxy gate with negative controls plus the nightly 1,500-season check; a gate row per mechanism as it becomes simulated |
| R11 | Owner and player expectation of MagicBlock's role | low / medium | Season 1 is base-only (O8); X1 and X2 as later, gated paths |
| R12 | **Read path at scale — not built** | high / high if ignored | Herald fold and CDN in M1 |
| R13 | **Regulatory: USDC entry plus prizes** may be treated as gambling or a prize competition in some jurisdictions; the laurel stake makes the contest element explicit | varies / high | **Legal review before any money on mainnet (M4 exit criterion)**; geo and age gates; the citizen-only path without a stake |
| R14 | Platform churn (fee-market parameters such as the per-account cap, syscalls, SBPF versions, SDKs) | medium / medium | Per-season cluster-parameter check (§8.6 rule 12); pinned toolchain and SBPF v2; spikes re-run before each season |
| R15 | **Players dislike that average players lose money** | medium / medium | Honest join page; low citizen fee; the stake is optional; the practice mode is free |
| R16 | **Shade forensics** reduce the social-deduction fantasy | medium / low | Relay-default funding, voting Shades, committed join schedule; honest list of tells |
| R17 | **Custom seal cryptography** (the 165-byte compact envelope) has a flaw | low / high | Pinned spec in `frontier::seal`; property test against stock tlock; outside review before M4; binary form only |
| R18 | **Account-initialisation and address bugs** (the pre-funding and duplicate-anchor class the M0 reviews found) | medium / high | §8.6 rules 10–11; one pre-funded-address and one forgery test per keyed account as M1 gates |
| R19 | **Quiet-bell backlog** makes idle provinces slow or costly to reopen | medium / medium | M1 picks a cheaper "no arrivals" proof or budgets up to 144 resolves per idle day (§6.3) |
| R20 | **Local tooling gaps** (the repo's test-validator 3.1.9 lacks BLS12-381 syscalls; platform-tools cannot build v3; edition-2024 crates need a newer cargo) | high / low | Agave ≥ 4.0 for local drills; `RUSTUP_TOOLCHAIN=1.95.0`; pinned v2 |

---

## 14. Owner decisions

**Decided on 2026-09-27** (the owner accepted every recommended default):

| # | Question | Decision | Where applied |
|---|---|---|---|
| O1 | Which drand network a season uses | **quicknet** for bell seeds, ring and genesis seeds and tlock seals; the Frontier program is built for **SBPF v2** | §8.1, §8.5, §6.2 |
| O2 | Genesis co-seed | **drand only**: a round fixed by rule ≥ 10 min after `CreateSeason`; **MagicBlock VRF is dropped from the Frontier**; S-VRF-LOCK cancelled; WP11 not ported | §3.2, §8.5, §10 J1–J5, §11.2 |
| O3 | Bots and paid offices | **Bound officer pay**. Applied (K3, revision 3.1): a ceiling of 95% of what the wallet paid on the claim; no office laurel rows (they failed the criterion) | §4.3, §5.4 |
| O4 | The bot criterion | **Try in order:** a lower `WORKS_PER_USDC`; late stakes priced by expected remaining accrual; order-weighted emission; remove Relic Site laurels only if needed | §5.4, §12 |
| O5 | Doctrines | **Asymmetric, with no direct multipliers on scored facts**; tuned to 6/6 within 16.7% ± 2 points and gated in CI | §4.1, §9.4 |
| O6 | γ (herding damping) | **0.6** until M3 measures β with real players | §5.6 |
| O7 | C4 and the defence budget | **Adopt the restated C4** (all clashes of the targeted side in a bell, whole-block pricing at the 40M per-account cap) and a **defence budget** (keeper bids above the tip from an operator or faction pool); longer or owner-extendable reveal windows considered and made a season parameter | §6.4, §8.6, §8.7 |
| O8 | MagicBlock's role | **Season 1 on Solana base only**; no Skirmish or Arena ER lane; X1 stays a later extension; onboarding without Skirmish | §2.8, §8.1, §12 |
| O9 | K2 kernel choices | **Keep:** a second capture pays 25% of the first transfer; an unused steward cut moves to the citizen pot | §4.3, §5.4 |
| O10 | Mandate reserve | **Pays stakers who completed only**, via a closed-form kernel | §4.2 |
| — | The v9 security round | **Stopped** (not finished, not frozen as Skirmish) | §11.2, §15 |
| — | Where the world runs; bell randomness | **Solana base; drand verified on chain** | §8 |
| — | Standing rule | Every devnet step and any push needs separate owner approval | §12 |

**Decided after revision 3.1 (2026-09-27; the log is `DECISIONS.md`):**

| # | Decision | Where applied |
|---|---|---|
| N1 | C4 is tested with keeper bids above the tip from a defence pool capped at priority 2.0, paid from ≥ 150 rotating keeper fee payers; the default-tip failure is a recorded known result | §6.4, §8.6–§8.8, §12 (M0 exit c) |
| N2 | D25 decided: valuation **(a)**, $7 × every clash the targeted side takes part in | §6.4, §21.2 |
| N3 | (fact) one push of `codex/frontier` was approved and made; GitHub CI ran once (run 36303403992) | §12 |
| N4 | SP-V2 `RESULTS.md` replaces separate S-BEACON and S-TLOCK reports: **M0 close item 8 is closed** | §12 |
| N5 | M1 starts now; the remaining M0 items close inside M1's first week (it took 1.5 weeks, `m0/M0-CLOSE.md`) | §12 |
| D23 | **Decided: at most one office-term per wallet per season**, with vacant seats allowed | §4.3, R8, §21.4 |

**Revision-2 decisions D1–D17 and their status:**

| # | Question | Status | Current answer |
|---|---|---|---|
| D1 | Where the Frontier runs in Season 1 | **Decided** (O8) | Solana base only; X1 later |
| D2 | Season length and join window | Open — working default | 28 days, joins until day 21 |
| D3 | Entry | Open — working default | Citizen fee 4 USDC + optional laurel stake 6 USDC, both falling with days left to a 25% floor; 80% to each pool, 20% operator, escrowed |
| D4 | Rent and fees | Open — working default | Relay fronts rent and sponsors 40 tx/day for 7 days, then 20; operator escrows the ProvinceFund (and, new, the defence pool) |
| D5 | Shades' political rights | Open — working default | Vote by committed policy; never stand, petition, consent, recall or accuse |
| D6 | Shade ratio | Open — working default | 0.5%, equal per faction, ≤ 0.75% of any faction |
| D7 | Placement | Open — working default | First holding in own wedge; site tickets per bell; friend pairs; Colonist invitations |
| D8 | Seal and auto-reveal | Open — working default, **changed in revision 3** and **in M1** (I-08, O-M1-06) | The seal is mandatory (§6.2); the reveal tip is **at least `tip_min`** in priority terms (14,441 lamports at a 26k Reveal with L = 1 MiB; **final for M1: 14,668 at the measured 26.5k limit and `L(reveal)` 1,146,880 B**; zero tips removed; §6.4); 100,000 lamports at Relic Sites |
| D9 | Losing a holding | Open — working default | First holding occupied, never taken; 2–3 capturable; sieges ≥ 36 bells outside an 8-hour vigil, progressing only while the besieging faction holds the site |
| D10 | Travel pace | Open — working default | 2 min per plains hex, roads and cavalry ×0.5, Waystones |
| D11 | Payout shape | Open — working default, **changed in revision 3** | Citizen pool 90% fee×tenure / 10% Works (linear, capped per USDC of fee); laurel pool by counted laurels (late stakes count from the stake point); Civilisation Share 5–15%; wallet cap 5× paid; office pay bounded (O3) |
| D12 | Map capacity | Open — working default | `R_MAX` 64, ProvinceFund for rings ≤ 40, hard maximum 128 |
| D13 | Skirmish stakes | **Moot** (no Skirmish, O8) | — |
| D14 | The v9 security round | **Decided**: stopped | — |
| D15 | Shade Auditor | Open — working default | An independent multisig that can only move the escrow to the pools on a published replay violation |
| D16 | Randomness | **Decided** (O1, O2) | drand quicknet verified on chain; no MagicBlock VRF anywhere in the Frontier |
| D17 | Faction doctrines | **Decided** (O5) | Asymmetric, no multipliers on scored facts, 6/6 band gate |

**New questions from revision 3 (working defaults; the owner may change them):**

| # | Question | Working default | Alternatives |
|---|---|---|---|
| D18 | Defence pool size and escalation cap | **Re-sized in M1 (CL-30/CL-31a, owner to confirm, O-M1-08 accepted as relayed):** only window-closing writes (Reveal) are pool-eligible, at **P_def 2.0**; delay-only writes escalate to **P_delay 0.5** from the keepers' own budget. Measured on the simulator's write counts: the pool pays **0.0105 SOL per attacked bell at the p99 bell at 50k wallets**, so **20 SOL covers ≈ 1,900 p99 attacked bells** (keep 20 SOL); with every critical write eligible it would pay 0.80 SOL and cover 25 [model on sim, §21.3]. Refunds by later claim on lateness evidence, capped; faction sub-pools. **Final (M1 wave 6, §22.3):** priced at the measured budgets table, the pool pays **0.0102 SOL per attacked p99 bell at 50k** (≤ 42,132 lamports per defended Reveal), so 20 SOL covers ≈ 1,950 p99 attacked bells; the per-bell-region cap (0.2 SOL) and per-keeper-day cap (2 SOL) are adequate as final values [model] | Larger pool; a higher cap; every critical write eligible (≈ 80× the spend) |
| D19 | Reveal window | 600 s, season parameter 600–1,800 s, raisable for future bells only | Fixed 600 s |
| D20 | Seal bond | 20,000 lamports per Depart and CommitPosture, paid to the prover of a bad seal | Larger bond; no bond (the prover gets only tip + march fee) |
| D21 | Office laurel rows | **Dropped (3.1):** they failed the bot criterion in K3 (1.110× at 1% bots in office) | — |
| D22 (new, 3.1) | Stake ramp (price of a late stake) | 2.0 (K3); **re-measured with D23 on (M1, CL-32):** ramp 1.0 lifts days-15–21 stakers by 3–4 points, but its worst bot choice is 0.983 on seeds 201–203 and **0.989 on the held-out seeds** (the rule asks ≤ 0.985) [sim, §21.4]; **recommendation: keep 2.0** unless the owner accepts a ≈ 1-point margin | 1.0 (the owner decides before M2) |
| D23 (new, 3.1) | Office term limit | **Decided 2026-09-27: one office-term per wallet per season**, vacant seats allowed; bots' office share at 1% falls from 60% to 9% [sim, M1 re-run] (§4.3, §21.4) | — |
| D24 (new, 3.1) | Relic Sites' role now that they mint no laurels | Works and Dominion only | pay their emission into the holder faction's Mandate reserve: **measured in M1 (CL-33): the criterion still passes (worst 0.977 against 0.979 without relics)** [sim, §21.4]; the owner decides before the M3 relic work |
| D25 (new, 3.1) | Which C4 valuation the owner accepts | **Decided (N2): valuation (a)**, $7 × every clash the targeted side takes part in | — |

---

## 15. The v9 security round (stopped)

**Decision (owner, 2026-09-27): the v9 security round is stopped.** `codex/v9-security` stays at `4ec9e8d` ("Wave 2 review fixes"), after waves 1–2 of 5. Revision 2's advice (finish v9 and freeze it as a stake-free Skirmish) is withdrawn.

**What that means for the Frontier:**
- **No Skirmish and no Arena** (O8). There is no live game until the M1 playtest; onboarding is rebuilt inside the Frontier (§2.8).
- **No hardened reference.** The v9 fixes the Frontier was going to port (P2 token checks and session-key registration, P3 solvency, `outstanding`, Abort and refunds, roster reveal and forfeiture) belonged to waves that were not finished. They are ported as reference code and **reviewed fresh in M2**.
- **No VRF plumbing.** WP11 and the `identity_mode` regression check are not needed; the Frontier uses no MagicBlock VRF (O2).
- **v9 behaviour is preserved in the code.** `params::RULES_VERSION` is 9 and the v9 golden replays pass on `codex/frontier` [measured], so the recorded v9 seasons stay replayable.
- Every devnet or mainnet deployment and any push remain owner-approved steps.

---

## 16. Labs: what was computed, and how to rerun it

```
cd scratchpad/openworld/lab/rev31 && python3 rev31.py > rev31-results.txt  # < 1 s (revision 3.1: base load and quiet bells with the SP-V2 budgets)
cd scratchpad/frontier/m0b/spikes/SP-FEE/driver && python3 c4_model.py > ../results/c4-model.txt   # revision 3.1 fee-market and C4 model (v2)
cd scratchpad/openworld/lab/rev3 && python3 rev3.py > rev3-results.txt    # < 1 s (revision 3: fee market at 40M, C4, defence pool, base load; §A–§F superseded in 3.1)
cd scratchpad/openworld/lab/rev2 && python3 rev2.py > rev2-results.txt    # < 1 s
cd scratchpad/openworld/lab/synth && python3 model.py                        # ~50 s (growth, rent, travel)
cd scratchpad/openworld/lab/redteam && python3 rt.py                         # the reviewer's model, reproduced
```

| `rev2-results.txt` section | Content |
|---|---|
| §A1–A3 | Lock costs: revision 1's VRF seed; beacon shards and the 7-day Abort; per-slot reveal blocking by tip |
| §B | Posture last-look option value: Hold default vs Disarray |
| §C1 | Revision 1's payouts with scripted bots (reproduces the review) |
| §C2 | Revision 2's two-pool payouts by archetype; bot strength 1.0/1.5/2.0; bot-share equilibrium |
| §C3 | Late joiners' expected return per USDC by join day |
| §D | Herding ratio: revision 1's tiers vs the per-capita index for β 1.0–1.4 |
| §E | Base load with a per-wallet bucket, reveals, resolve bursts and who pays the crank |
| §F | Map capacity by `R_MAX` |
| §G | Shade withholding bound |
| §H | Steward pot vs officer rows |

`model-results.txt` §5 (revision 1's single-account lock costs) and §7 (revision 1's payouts, including the hard-coded 0.47 herding line) are **superseded** by `rev2-results.txt`. In revision 3, `rev2-results.txt` §A (12M-cap lock costs), §C2 (formula payouts) and §E (pre-M0 budgets) are **superseded** by `rev3-results.txt` §A–§F and by the simulator.

| `rev3-results.txt` section | Content |
|---|---|
| §A | Keeper bid per CU for a Reveal, gross and net of the base fee |
| §B | Cost to exclude 1 account or the whole block for one reveal window, by tip |
| §C | Restated C4: whole-block cost vs 10× the value of a bell's clashes at 10k/50k/100k |
| §D | Defence pool: attacker cost vs defender cost at P_def 1/2/5 |
| §E | Holding one BellAnchor |
| §F | Base load with the M0 budgets |
| §G | Quiet-bell backlog |

**M0 artefacts (code on `codex/frontier` @ `cea89be`, worktree `.claude/worktrees/frontier-integ`; spikes in `scratchpad/frontier/m0/`):**

```
cd permutation-rules && cargo test && cargo test --release                   # 338 tests each [measured]
cd frontier-sim && cargo build --release && cargo test --release             # 3 tests
./target/release/frontier-sim suite --agents 10000 --seeds 5 --out <file>    # 642 s on 16 threads; every table of RESULTS.md
./target/release/frontier-sim run --agents 10000 --seed 1                    # digest c71a8a0eb7641b53, 2.9 s, 35 MB
```

| Artefact | Content |
|---|---|
| `frontier/m0/M0-REPORT.md` (+ `.ja.md`) | M0 status, exit criteria, design changes, owner decisions O1–O10 |
| `frontier/m0/sim/RESULTS.md` | Simulator model, payouts, bots, herding, doctrines, conservation |
| `frontier/m0/spikes/S-BEACON/results/` | quicknet/evmnet verification CU on v0 and v2, anchor/seed/resolve flows, sizes (review corrections at the top of `run-log.txt`) |
| `frontier/m0/spikes/S-FEE/REPORT.md` | first fee drill (verdict withdrawn) and the correction header with the reviewer's drill |
| `frontier/m0/spikes/S-TLOCK/results/` | seal sizes, round trips, on-chain opening with and without FO (`sbf-results.txt`) |
| `frontier/m0/spikes/S-SIZE-JOIN/results.txt` | ResolveClash sizes and CU, absence proofs, postures, gather split, Join/Settle/FileTicket |
| `frontier/m0/result.json` | spike design impacts, kernel and simulator summaries, review verdicts, integration gate |

---

## 17. Sources

**Project inputs**
- `scratchpad/openworld/research/{eternum,onchain-mmo-patterns,current-game}.md`
- `scratchpad/openworld/design/{E1-async-entities,E2-regional-tick-worlds,E3-hybrid-async-plus-battles,E4-free}.md`
- `scratchpad/openworld/judge/{game-design,security-scale,buildability}.md`
- `scratchpad/fix/contract.md` (v9 waves), `scratchpad/scale/scale-design.md`
- `scratchpad/fix/vrf-spike/REPORT.md` and `vrf-src/program/src/{request_randomness,provide_randomness}.rs` (queue PDA seeds, callback writes, request id derivation)
- Repo (read-only): `codex/magicblock-playable` @ `96a3464`, `codex/v9-security` @ `4ec9e8d`; Frontier kernels and simulator: `codex/frontier` @ `cea89be`
- M0 (revision 3 inputs): `scratchpad/frontier/m0/{M0-REPORT.md, M0-REPORT.ja.md, result.json, sim/RESULTS.md, spikes/*}`

**Public sources**
- MagicBlock engine scheduler: https://github.com/magicblock-labs/magicblock-engine/blob/6993f9d7f38bae691f0785c9dcf7059bd8e9b985/processor/src/sequencer/order.rs
- MagicBlock VRF technical details (shared queues): https://docs.magicblock.gg/pages/verifiable-randomness-functions-vrfs/introduction/technical-details.md
- MagicBlock VRF program: https://github.com/magicblock-labs/solana-vrf ; issue #72 (legacy requests never fulfilled after the 2026-09-18/19 upgrade): https://github.com/magicblock-labs/solana-vrf/issues/72
- MagicBlock runtime limits, pricing, Private ER authorization: https://docs.magicblock.gg/pages/ephemeral-rollups-ers/introduction/runtime-limits.md , https://docs.magicblock.gg/pages/overview/additional-information/pricing.md , https://docs.magicblock.gg/pages/private-ephemeral-rollups-pers/introduction/authorization.md
- Mass delegation stall: https://github.com/magicblock-labs/magicblock-validator/issues/1603
- Solana local fee markets: https://www.helius.dev/blog/solana-local-fee-markets
- SIMD-0306 (per-account CU limit raised to 40% of the block limit; feature `htsptAwi2yRoZH83SKaUXykeZGtZHgxkS2QwW1pssR8`, active on mainnet since slot 379,296,000, checked by read-only RPC on 2026-09-27): https://github.com/solana-foundation/solana-improvement-documents
- SIMD-0385 (transaction V1, later option), SIMD-0388 (BLS12-381 syscalls), SIMD-0650 (needed for on-chain opening of evmnet seals): https://github.com/solana-foundation/solana-improvement-documents
- SIMD-0286 block limit 100M CU, activated 2026-07-29 (the article's "per-account 12M unchanged" is out of date: SIMD-0306 later raised it to 40%): https://cryptoslate.com/solana-just-boosted-block-capacity-by-a-massive-66-but-the-one-bottleneck-infuriating-traders-hasnt-budged/
- Solana BLS12-381 and BN254 syscalls (Agave 4.0): https://solana.com/upgrades/new-cryptography ; SIMD-0302 BN254 G2: https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0302-bn254-g2-syscalls.md ; SIMD-0334 pairing fix: https://parameter.io/solana-implements-key-alt_bn128_pairing-fix-with-simd-0334-update/
- Rent reduction (SIMD-0437 step 2): https://solanacompass.com/news/simd-0437-step-2-goes-live-on-solana-mainnet-rent-drops-to-5080-lamports-per-byte
- drand: timelock https://docs.drand.love/docs/timelock-encryption/ , https://github.com/drand/tlock , https://github.com/drand/tlock-js ; specification https://docs.drand.love/docs/specification/ ; quicknet and evmnet (BN254) on-chain verification https://docs.drand.love/blog/2025/08/26/verifying-bls12-on-ethereum/ ; OpenVRF (evmnet consumer) https://github.com/Robinhood-OSS/OpenVRF
- Eternum source @ a375655 and docs: https://github.com/BibliothecaDAO/eternum/tree/a375655955d08b96201e40500a52ddc6a9278b48 , https://docs.realms.world/eternum/key-concepts
- Dark Forest (arrival quotas): https://github.com/darkforest-eth/eth

---

## 18. Revision notes (revision 2, 2026-09-27)

Each of the 23 review issues was checked against the revision-1 text and, where numbers were involved, re-computed (`lab/rev2/rev2.py`; the reviewer's `lab/redteam/rt.py` was re-run and reproduced exactly). Verdicts: **Confirmed** (the issue is real as stated), **Confirmed, magnitude corrected** (real, but a number in the review is off), **Partly rebutted** (the proposed fix would make things worse; a different fix is used).

| # | Issue (review severity) | Verdict and evidence | What changed |
|---|---|---|---|
| 1 | One global VRF account per bell; blast radius; 24 h kill switch (blocker) | **Confirmed.** `BellSeed(b)` was a predictable PDA written by the oracle's callback; the VRF program writes the shared queue and the requested callback accounts (`provide_randomness.rs`); the oracle's fee does not escalate; §8.6 of revision 1 wrongly said "one bell". rt.py reproduced: 20 min $20–74, 24 h $1.5k–5.3k. solana-vrf #72 confirms genuine misses. | Bell and ring seeds from drand rounds fixed by rule, verified on chain, cached per region or verified inline (§8.5); per-slot arrivals and anchored windows so lag never changes outcomes (§8.4); 24 h Abort removed, 7-day Abort with seed-relay check (§8.10); S-BEACON and S-VRF-LOCK in M0 |
| 2 | Sealed simultaneity broken inside a bell (blocker) | **Confirmed.** Revision 1 encrypted to the bell's start and let Depart/Muster/Dissolve write the province during the bell; withholding cost 5%. | Roster frozen at the bell's start; tlock to the bell's end; postures before the bell; `retreat_ratio` for everyone; unrevealed arrivals routed at 50%; roster-freeze property test (§6.2–§6.3, §9.4) |
| 3 | Quota squatting by in-faction Sybils, spies, Shades (major) | **Confirmed.** First-come quotas within a faction; 6-slot hexes shared by allies. | Mass-based displacement, one arrival per citizen per province-bell, hex fair-share, owner faction's 3 slots on its holding's hex (§6.1–§6.2) |
| 4 | Shades as an operator steering wheel; G3 bond not proven (major) | **Confirmed.** No policy commitment; RevealShade voided only h/u/g; genesis ring seed unspecified; G3 recomputed: gain 0.15 N f vs bond 0.11 N f → fails. | Committed deterministic policy with replay; no LLM actions; per-path score facts voided; genesis seeded after parameters are fixed; 0.5% Shades, 0.75% cap, wallet cap 5× → gain 0.0375 N f < bond 0.07 N f (§7) |
| 5 | Bots and Sybils profitable (major) | **Confirmed**, reproduced exactly (1.87× at b = 0; 1.05× at b = 0.5). | Two pools; laurels as zero-sum emission; no damage honor; capture pair rules; fee sized so a strength-1.5 bot returns 0.86× (§5.4) |
| 6 | Bourse wash trading; deterministic shard (major) | **Confirmed** (Bourse volume in FactionState; Concord counted trade). Book visibility is inherent to a public batch auction and is stated as a residual. | Volume removed from all scoring; trader chooses any shard; sealed orders as X4 (§5.3, §5.6) |
| 7 | T_end cliff (major) | **Confirmed.** Unbanked honor lost; lazy credits only on touch. | Values frozen at T_end in closed form; 72-hour permissionless banking window; nothing lost (§8.4) |
| 8 | `R_MAX` a hidden seat cap; land exhaustion (major) | **Confirmed**, numbers reproduced (21,528 / 37,944 / 149,688 sites). Also found: revision 1's hard maximum 64 contradicted its own ring-70 row at 100k. | Capacity stated at creation (default 64, hard max 128), funded ProvinceFund, fee escrow until settled, Join refused without charge when full, 2nd/3rd-holding gate, dormant first holdings released (§3.4) |
| 9 | Governance cheap to capture or DoS; Mandates move money (major) | **Confirmed.** Recall cost ≈ 375 wallets reproduced; Mandate multipliers moved honor-based money. | Assembly weight = ballots cast (min voters, tenure); recall needs Assembly weight, one per office per term; Mandates from a bounded laurel reserve with a per-wallet bound (§4.2–§4.3) |
| 10 | Reveal liveness depends on the operator's keeper; locks force Disarray (major) | **Partly rebutted.** Real: no keeper reward, a Province lock blocked reveals and postures. But defaulting a valid posture to Hold would hand scripts a free +6.7% last-look option (§B), and the chain cannot tell a valid ciphertext from garbage. | Per-slot ArrivalSlots and PosturePDAs, keeper tips, verifier flags valid unrevealed ciphertexts; Disarray kept (§6.3–§6.4) |
| 11 | Capacity numbers optimistic (major) | **Confirmed.** Per-holding bucket, missing reveals/resolves, resolve burst 16/31 blocks, forced crank 154/300 SOL all reproduced. | Per-wallet bucket; §8.9 re-run (5.7% at 50k, 11.3% at 100k); resolution paid by a march fee; comfortable point restated as ~50k pending M0 (§8.8–§8.9) |
| 12 | Real-money Skirmish on a public ER (major) | **Confirmed.** | Skirmish stake-free (D13); stakes only after S1 on a gated ER or an owner sign-off (§2.8, §15) |
| 13 | Faction scoring rewards size; 0.47× not derived (blocker) | **Confirmed, magnitude corrected.** 0.47 was hard-coded in `model.py`. Under revision 1's actual score function the 3×-size ratio is 1.08× (1.83× if Knowledge is also share-scored), not "about 2× or more"; the cliffs are the bigger problem (0.83 at 1.2×). | E3's per-capita index grafted, smooth, clamped, herding damping γ = 0.6; ratio 0.61–0.95 at 3× (§5.6); 0.47 claim removed from both documents |
| 14 | Daily honor cap plus automatable PvE makes bots profitable (blocker) | **Confirmed.** | Daily honor cap removed as a payout driver; PvE goes to capped Works (10% of the citizen pool); contested zero-sum laurels; scripted-wallet archetype and acceptance criterion (§5.4, M3) |
| 15 | Operator steers the faction split via Shades (major) | **Confirmed** (same root as 4). | Per-path facts voided; committed policy; replay; Auditor (§7.3) |
| 16 | Accusation becomes chain forensics (major) | **Confirmed.** | Relay-default funding for all; Shades vote by policy; committed join schedule; precision-gated bounties; tells listed honestly (§7.4) |
| 17 | Sleeping is punished (major) | **Confirmed.** 30–60-minute sieges and 72-bell scheduling allowed 4 a.m. captures. | Sieges ≥ 36 bells outside an 8-hour vigil, public siege horn, auto-reinforce, raid cap (§6.3) |
| 18 | Late joiners cannot compete; rim exposure (major) | **Confirmed.** | Fee and stake ∝ days left; fee-weighted citizen share with tenure normalised to days available; Frontier protection; Relic Sites moved inside the rim; expected return by join day published (§2.6, §6.6) |
| 19 | "Any number" capped; paid joins without land (major) | **Confirmed** (same root as 8). | As 8; summary states the cap |
| 20 | Governance decorative at scale; War a single-point veto (major) | **Confirmed.** Steward-pot pro-rata rule was missing. Revision 1's figure (165 f vs 63 f) depended on its seat counts; with first holdings in the own wedge and ≥ 12-ballot pay, rows fit (factor 1.00). | Pro-rata rule defined; Mandates from the laurel reserve; sieges outside heartlands need no decree; March truce/hostility; Granary uses specified (§4.2–§4.3) |
| 21 | The six factions barely matter (major) | **Confirmed.** | Six asymmetric doctrines; first holding in own wedge; Dominion by March control (§4.1, §5.6) |
| 22 | Money buys defensive wins by blocking reveals (major) | **Confirmed.** Revision 1 priced one province at $45 per window. | Per-slot reveals with tips: $148 / $7.2k / $72k per province-bell; absolute criterion C4 against clash value; mentioned in the summary's money section (§6.4, §8.7) |
| 23 | The Japanese summary overstates certainty and omits risks (major) | **Confirmed.** "すべて実測" was false (Join, Settle, votes, Claim, OpenRing and more were estimates); the payout table was an input-driven model; losses, legal risk and sponsorship cost were missing. | Summary rewritten with 実測/見積もり labels, the average-player loss, legal review, sponsorship cost, the capacity cap and late-joiner expectation |

**Not changed, and why:** the world stays on Solana base; MagicBlock stays in the stack (ER lanes, VRF for the ER lanes and genesis); "one civilization, six factions", player governance, hidden Shades, chain verifiability and USDC entry with prizes are all kept. The biggest departures from the KEEP list are (a) bell randomness moved from MagicBlock VRF to drand for the Frontier, argued in §8.5 with the review's evidence and the VRF program source, and (b) the entry split into a citizen fee and an optional stake, argued in §5.4 with the payout model. Both are owner decisions (D16, D3).

---

## 19. Revision 3 notes (2026-09-27)

**Inputs.** The owner's decisions O1–O10 and the stop of the v9 round (2026-09-27); `scratchpad/frontier/m0/M0-REPORT.md` §4 (items 1–30 and the §4.8 kernel list); the spike `design_impact` lists and review verdicts in `scratchpad/frontier/m0/result.json`; the reviewers' corrections prepended to the spike files; `sim/RESULTS.md`. Every review issue that reached the integrator was confirmed (24 of 24; none rebutted), and the kernel ones are fixed at `cea89be`. This revision changed no code. New arithmetic is in `lab/rev3/rev3.py` → `rev3-results.txt`.

### 19.1 Owner decisions

| # | Applied in |
|---|---|
| O1 quicknet for seeds and seals; SBPF v2 | §0.5 (2), §8.1, §8.5, §6.2, §12 M0 close (v2 re-measure) |
| O2 genesis from drand only; no MagicBlock VRF | §3.2, §8.5 table, §8.10 (genesis timeout), §10 J1–J5, §11.2 (WP11 not ported), §12 (S-VRF-LOCK cancelled) |
| O3 bound officer pay | §4.3 (break-even USDC bound, office laurel rows — *both replaced in 3.1 by the 95% ceiling*), §5.4 claim formula, M3 criterion |
| O4 bot criterion plan | §5.4 (four ordered steps), §2.7, §12 M0 close and M3 |
| O5 doctrines without direct multipliers; 6/6 band; CI | §4.1, §9.4, §12 |
| O6 γ = 0.6 | §5.6 |
| O7 restated C4; defence budget; longer windows | §6.4, §8.6, §8.7, §8.8, §10 C4, §14 D18–D19 |
| O8 base only; no Skirmish/Arena; X1 later; onboarding without Skirmish | §0.3, §0.5, §1, §2.3, §2.8, §3.7, §8.1, §11.3, §12, §13 R3/R11/R15, §15 |
| O9 second capture 25% of the first transfer; unused steward cut to the citizen pot | §5.4 item 3, §4.3 |
| O10 Mandate reserve to staking completers; closed-form kernel | §4.2, §5.4 item 6, §12 M0 close |
| v9 stopped; standing approval rule | §11.2, §12, §15 |

### 19.2 M0-REPORT §4 design changes

| Item | Change | Where |
|---|---|---|
| 1 | Pre-funding-safe init; absence proofs; test per init path | §8.6 rule 10, §8.2, §9.4, §12 M1 |
| 2 | Exactly one BellAnchor per (season, bell, region); address checked on every read; SeedCache records its anchor; forgery test | §8.2, §8.5, §8.6 rule 11, §9.4 |
| 3 | With-seed ArrivalSlot and PosturePDA addresses; test that omitting an existing slot fails | §8.2, §6.3, §9.4 |
| 4 | No inline verification; nonce-keyed SeedCaches (≤ 256); rule 8 bypass deleted | §6.3, §8.5, §8.6 rule 8 |
| 5 | BellAnchor is the one lockable beacon account; invariant "no choice between published seeds"; unpredictable address (*withdrawn in 3.1*: it broke Reveal's absence proof), combined anchor, escalation | §8.4, §8.5, §8.6, §9.4 |
| 6 | Seed margin Δ ≥ 60 s; Reveal refuses at A + W or once a round ≥ S is on chain; property test | §2.1, §6.2, §8.5, §9.4 |
| 7 | One drand network per season (quicknet) | §8.5 |
| 8 | Hash-to-curve hints in the instruction format | §8.3 |
| 9 | ArchiveAnchors | §8.2, §8.3, §8.4 |
| 10 | 165-B seal format pinned; spec and outside review; binary only; log size 165 B; Depart 667 B | §6.2, §8.3, §12 M4, §13 R17 |
| 11 | `seal_root` in Depart; `ct_hash` in Reveal | §6.2 |
| 12 | BellAnchor stores the round-T(b) signature (112 B) | §8.2 |
| 13 | ProveBadSeal → SealVerdict; FO check kept; SettleTransit destroys the host and pays the prover; only after close + 1 bell; bad posture seal → Disarray + forfeiture; ≈ 330k inline estimate | §6.2, §6.3, §6.4, §8.3 |
| 14 | Account cap 40% of the block (40M); per-season cluster check | §8.1, §8.6 rule 12, §13 R14 |
| 15 | Cost model p × min(k × 40M, 100M); net-of-base-fee bids; 0.4 s slots; world-wide case; [model] labels | §8.7, `rev3-results.txt` |
| 16 | Restated C4; per-province blast radius dropped for this attack | §6.4, §10 C4 |
| 17 | Longer / owner-extendable windows; pooled keeper bids; in-bell reveals | §6.4 (defences 1–3), §14 D18–D19 |
| 18 | GatherClash → ClashInputs; ResolveFromInputs; in the lock and keeper analysis | §6.3, §8.2, §8.3, §8.6 |
| 19 | Province mirrors garrison, walls and owner | §6.3, §8.2 |
| 20 | Budgets: ResolveClash ≈ 270k, Reveal 17–26k, Join 18k, SettleTicket 19k, FileTicket 4k, 31 keys / 1,152 B; re-measure on v2 | §6.2, §6.3, §8.3, §8.9 |
| 21 | Pin SBPF v2; arkworks `#[inline(never)]` | §8.1 |
| 22 | SettleTicket displacement rewrites the displaced Citizen and JoinShard; Holding keyed by site; challenge window finalises MarchRoll counts | §2.2, §8.2, §8.3 |
| 23 | FoldOccupancy; reserve sized for joins between folds | §3.4, §8.3 |
| 24 | Arrival ≥ departure + 2 bells; post-clash values; Reveal ranking and destination wait for the origin (*3.1: only GatherClash and the resolve wait; Reveal ranks by departure mass*) | §3.6, §6.2, §8.4 |
| 25 | Resident actions need b − 2; mid-bell changes take effect after the bell's clash | §6.3, §8.4 |
| 26 | Hex fair share by side | §6.1 |
| 27 | Quiet bells; siege over quiet runs in closed form, only while the declarer holds the hex; capacity model | §6.3, §8.9 |
| 28 | Carved paths from gates and sites; straight march 20–28 hexes | §3.2, §3.6 |
| 29 | Late stakes; pair rule for occupations and siege stakes; fee-only wallets transfer nothing; linear Works capped at 140 per USDC | §2.2, §5.4 |
| 30 | Keep both halves of `resolve_engagement` | §6.3, §11.1 |
| §4.8 | Bounds (walls/production/upkeep, `duplicate_cost`, ring, `clash::validate`); scouts; vigil at UTC midnight; `Stamina::set`; cap recount; faction ids; Mandate closed form; bounds-style conservation checks; per-faction Ledger; `validate()`; overflow checks; v9 `.so` hygiene | §6.1, §6.3, §9.4, §11.1 |

### 19.3 Spike design impacts and review corrections

| Source | Finding | Verdict here | Where |
|---|---|---|---|
| S-BEACON | PostBeacon 94k (evmnet v2) / 332k (quicknet v2); recommends evmnet | Numbers adopted; **evmnet recommendation overridden by O1** (quicknet tlock tooling is mature, evmnet on-chain opening unmeasured) | §8.3, §8.5 |
| S-BEACON | SBPF v2 build; inline(never); hints; inline verification does not fit; nonce SeedCaches; anchor is the lockable account; seed margin; reword "lag never changes outcomes"; ArchiveAnchors; local validator lacks BLS syscalls | Adopted (margin raised from the spike's ~30 s to the review's ≥ 60 s) | §8.1–§8.5, §13 R20 |
| S-BEACON | Mixed-network anchoring rule (evmnet anchor after quicknet round) | Moot under O1 (one network) | §8.5 |
| S-BEACON | Anchor lock drill D1–D6 | Folded into the S-FEE redo | §12 M0 close |
| S-FEE | "A local validator cannot enforce the account cap; C4 cannot be closed locally" | **Rebutted by the review**: the drill's fills used ≈ 4k of their declared CU; the corrected drill shows the cap is enforced locally. The M4 soak stays for Jito and multi-leader behaviour | §6.4, §8.7, §12 |
| S-FEE | Cost model "validated, no change needed" at 12M | **Superseded**: the cap is 40M; model rewritten | §8.7 |
| S-FEE | Per-season check that the cluster enforces the cap; keep escalating keepers and tips; throughput is no defence; port and toolchain notes | Adopted | §8.1, §8.6 rule 12, §8.7, §12 |
| S-TLOCK | 165-B seal; seal_root; ProveBadSeal/SealVerdict with FO; SettleTransit rule; anchor stores sig_T; posture seals; verifier reports verdicts; port the opener | Adopted | §6.2–§6.4, §8.2, §8.3, §9.1, §11.2 |
| S-TLOCK | "Inline proof ≈ 190–200k CU" | **Corrected** to ≈ 330k on v2 (S-BEACON measured) | §6.2 |
| S-TLOCK | Keep tlock on quicknet even if anchors move to evmnet | Superseded by O1 (both on quicknet) | §8.5 |
| S-SIZE-JOIN | Province mirror; GatherClash; with-seed addresses; pre-funding; budgets; no v0/LUT needed; displacement writes; FoldOccupancy; V1 as a later option | Adopted; the v0 + lookup-table alternative (≈ 3.2M lamports of table rent per province) rejected in favour of the mirror | §6.3, §8.2, §8.3 |
| S-SIZE-JOIN | Row A2 "inline beacon bytes fit (1,175 B)" | **Corrected by the review**: it left out the beacon hints and the BLS check; the real inline transaction is 1,427–1,476 B | §6.3, §8.3 |
| Review B1/C1 | Pre-funding blocks creation (blocker) | Confirmed; fixed in the design | §8.6 rule 10 |
| Review B2 | Non-unique BellAnchor lets a caller choose seeds (blocker) | Confirmed; fixed in the design | §8.2, §8.5 |
| Review S-FEE | Drill never created contention (blocker) | Confirmed; redo in M0 close | §12 |
| Review B3/T3 | Seed round public before the close | Confirmed; Δ ≥ 60 s | §8.5 |
| Review B4/C2 | Inline verification too large | Confirmed; dropped | §6.3 |
| Review B5/C5 | CU depends on the SBPF version | Confirmed; pin v2, re-measure | §8.1, §8.3 |
| Review C3 | Postures missing from ResolveClash's budget | Confirmed; GatherClash | §6.3 |
| Review C4 | CU for the design (270k, absence proofs, creation cost) | Confirmed | §8.3 |
| Review T1/T2 | evmnet seal opening unmeasured; mixed networks wrong | Confirmed; one network | §8.5 |
| K1 review (9 majors) | Two clashes one bell apart; values not frozen; siege advanced by any hostile faction; allies pooling slots; terrain islands; split overflow; dropped retaliation; cadence; plus the b − 2 correction | Confirmed; fixed in the kernels at `cea89be`; now stated in the text | §3.2, §6.1–§6.3, §8.4 |
| K2 review (4 majors) | Late-stake backdating; self-dealing via occupation and siege stakes; √Works rewarding wallet splitting; bot officers | Confirmed; the first three fixed in the kernels; bot officers bounded by O3 | §4.3, §5.4 |

### 19.4 Judgement calls made in this revision (flagged for review)

1. **Absence of a pre-funded address.** M0-REPORT item 1 says absence proofs "must reject a pre-funded system account". Read literally, a pre-funded but never-revealed slot would then make resolution impossible, which is a new freeze. This revision reads the intent as: a pre-funded address can never block initialisation (so it can never hide a real arrival), and it counts as *absent* in absence proofs; both cases are tested (§8.6 rule 10).
2. **The seal is mandatory** on every Depart and CommitPosture. The report's `seal_root` design implies it, and it is what makes `ProveBadSeal` close the last-look option.
3. **Officer pay mechanism (O3)** — *superseded in 3.1 by K3's 95% ceiling without laurel rows (§4.3)*. The owner chose "bound officer pay" with two examples. This revision uses both: USDC office pay only up to break-even (so no wallet profits in USDC from office) plus bounded office laurel rows for stakers. The O9 rule for an unused pot remainder is kept; USDC withheld by the break-even bound is swept to the next season, because it is only known at claim time.
4. **Defence pool numbers** (20 SOL, 2 lamport/CU) and the **seal bond** (20,000 lamports) are working defaults from `rev3-results.txt`, to be tuned after the S-FEE redo (§14 D18–D20).
5. **Anchor and cache addresses use with-seed derivation** from the Season PDA rather than PDAs with a stored canonical bump; both satisfy the report, and with-seed is cheaper to check.
6. **Genesis-seed timeout Abort** (7 days, before joins open) is new; it follows from making drand the only genesis source.

### 19.5 New open item found while writing this revision

**Quiet-bell proofs.** The kernels define a quiet bell, but on chain "no arrivals this bell" still needs absence proofs for that bell's slot addresses, one bell per transaction. A province nobody touched for a day needs up to 144 such resolves (≈ 10.5M CU, ≈ $0.11 of base fees [model]) before a resident can act. Revision 2's "a province needs a resolution only in bells with arrivals" was therefore too optimistic. M1 must choose a cheaper proof or budget the cost (§6.3, §8.9, R19).

---

## 20. Revision 3.1 notes (2026-09-27)

**Inputs.** The second M0 pass in `scratchpad/frontier/m0b/` (SP-V2 and SP-FEE redone; K3 `sim/ECONOMY.md` and `sim/DOCTRINES.md`), the K3 integration on `codex/frontier` (`360bc85`), and the 22 issues of the review of revision 3. Every issue was checked before it was fixed; **all 22 were confirmed** (magnitudes are given where the check found a different number). The fixes that needed code or measurements are in `scratchpad/frontier/m0c/` (simulator runs), `m0b/spikes/SP-V2/` (lab program, test `t9`), `m0b/spikes/SP-FEE/` (drill (g), the model v2, the mainnet parser) and on `codex/frontier` (kernels, simulator, CI).

| # | Issue (severity) | Verdict and evidence | What changed |
|---|---|---|---|
| 1 | Unpredictable anchor address breaks the Reveal-window check (blocker) | **Confirmed.** With `sha256(sig_T)` in the seed Reveal cannot prove "no anchor yet"; SP-V2 built the predictable form and flagged this. Also found: closing an anchor after 48 h reopened its window (issue 16) | Predictable `an ‖ hex(b, r)`; tombstone before close; property "no Reveal after `A + W`" (§8.2, §8.4, §6.4, §9.4) |
| 2 | ProveBadSeal cannot bind the seal (blocker) | **Confirmed.** Depart took a client `seal_root`; ProveBadSeal took no `commit`, and SP-V2's prover needs it | Depart and CommitPosture carry commit + seal, program computes the root; ProveBadSeal takes the commitment; 37-B plaintext and domain pinned; new §9.4 cases (§6.2) |
| 3 | Origin lag changes outcomes (blocker) | **Confirmed.** Reveal read the origin and refused after `A + W`; a held origin routed its arrivals | Reveal ranks by departure mass and never reads the origin; GatherClash reads post-clash values with no deadline; the origin keeps them until SettleTransit; program-level lag test in M1 (§6.2, §8.4) |
| 4 | Stale measured numbers (major) | **Confirmed**: SP-V2 figures differ by ~2.3× for the clash | Every row replaced from SP-V2; 650k budget; full gather primary; anchor key in the resolve; capacity re-run (`lab/rev31`): 10% of block CU at ~60k (30/h) / ~75k (20/h) (§6.3, §8.3, §8.9) |
| 5 | "≥ 3 accounts = whole block" false (major) | **Confirmed** (SP-FEE (b): 4 slots at one slot's price; m0c (g): 20 payers in one stream) | §8.1, §8.6, §8.7 rewritten from the model v2 with the priority formula and slot-time cases; Japanese summary fixed |
| 6 | C4 status misreported (major) | **Confirmed** | C4 reported as not met at the default tip; exit (c) restated; `frontier-sim c4` gives the per-bell metric and the counterfactual value; D18 set from SP-FEE (§6.4, §12, §14) |
| 7 | Defence pool unspecified and exploitable (major) | **Confirmed** | Refund by later claim from per-keeper shards on lateness evidence, with caps; keeper SDK rules in the spec (§6.4, §8.2, §8.8) |
| 8 | O3 implementation contradicted by K3 (major) | **Confirmed** (break-even returns exactly 1.0×; laurel rows 1.110×) | 95% ceiling, laurel rows dropped, §2.7 / §5.4 / R8 updated from K3 (§4.3, §5.4) |
| 9 | Doctrine CI gate as specified fails most runs; stale table (major) | **Confirmed** (SE 1.5 points at 600 seasons) | Kernel table in §4.1 (provisional); CI proxy with negative controls; 1,500-season scheduled check (§4.1, §9.4) |
| 10 | Japanese summary overstates certainty (major) | **Confirmed** | Summary rewritten to revision 3.1 (`summary-ja.md`) |
| 11 | SP-FEE: fee-payer locks never modelled (major) | **Confirmed by measurement** (drill (g): held 11 s at one account's price; 20 payers held; a fresh payer landed in 0.89 s) | Model path `min(p × 40M × ⌈payers/60⌉, p × 100M)`; ≥ 150 rotating payers rule (§6.4, §8.6) |
| 12 | SP-FEE: priority rests on an unmeasured Reveal size (major) | **Confirmed** (0.433 → 0.35–0.27 at 20–26k) | Reveal CU is a model input; the tip is stated in priority terms (§6.4, §8.7) |
| 13 | SP-FEE: capacity not a lower bound; mainnet priority sampling stubbed (major) | **Confirmed** (the parser appended 0; v1 transactions failed) | Parser fixed (priority from `meta.costUnits`, v1 accepted): 13% of mainnet non-vote cost bids ≥ 0.433 [measured]; capacity is a three-case range with execution-bound [estimate] (§8.7) |
| 14 | SP-FEE: value side has no firm basis; the tail rests on one episode (major) | **Confirmed**: the tail is one day-10.6–11.0 episode in each of 3 seeds, mostly defence-only clashes | Three valuations reported, including the simulator's counterfactual (§6.4) |
| 15 | SP-FEE and SP-V2 contradict on the anchor address (major) | **Confirmed** | Predictable address adopted; D6 and "keep unpredictable" withdrawn from SP-FEE's impacts (§8.4) |
| 16 | SP-V2: absent anchor = window open with no limit; archiving reopens it (major) | **Confirmed by a mutation check** (without the tombstone, a Reveal at A + 30 days lands) | Tombstone in the AnchorArchive; Reveal and PostAnchor refuse a tombstoned bell; lab test `t9` (§8.4) |
| 17 | SP-FEE verdict overstated (major) | **Confirmed** | Headline "C4 not met at the default tip [model]" with the sensitivity table carried into §6.4 and §8.7 |
| 18 | Bot criterion on the default join mix only (major) | **Confirmed, magnitude measured**: worst best-response cell 0.980 (margin 2 points), not 1.0 or more, so no retune was needed | `criterion --best-response [--gate]`, the M3 criterion restated as the max over the bot's choices (§5.4) |
| 19 | CI gate does not enforce O5 and its power is overstated (major) | **Confirmed** (the K3 gate let the Knight table through on 2 of 5 seed sets). On the new gate's seeds with the final table: Knight −0.219%, an in-bounds A boost +0.384%, both rejected | Gate 30 seeds, ±0.2%, three negative controls; scheduled 1,500-season workflow with a fixed and a fresh seed set (§9.4) |
| 20 | Doctrines balanced by argument; B and F field the same army (major) | **Confirmed** | Table marked provisional in rules v10 and here; `variant_bps` gives F a heavy-cavalry weight; `validate_table` refuses two doctrines with the same army; re-tuned and re-confirmed (§4.1) |
| 21 | Bots hold most offices; steering Mandates unmodelled (major) | **Confirmed, and it fails**: 61% of office-terms at 1% bots; steering to always-online tasks gives 1.04× (humans at 25%) to 1.64× (humans at 0) | Mandate share floor with burn (kernel); fixed Mandate menu [design]; office share reported; term-limit option D23 (§4.2, §4.3) |
| 22 | Harm to honest players understated (major) | **Confirmed** (late stakers −10 to −18 points; skill premium gone) | Stated in §2.6 and §2.7; ramp option D22; the criterion prints the human-vs-bot gap |

**Not done in this revision, and why.** The full Reveal, CommitPosture/RevealPosture, SettleTransit and ArchiveAnchors are still not built as program instructions (M1). The Mandate menu is argued, not simulated. The C4 valuation is an owner decision (D25). The M4 soak needs owner approval for every devnet or mainnet-fork step. Nothing was pushed and no devnet or mainnet transaction was sent; the local validator ran on ports 39970–40025 and was stopped, its ledger deleted.

---

## 21. M1 wave-1 amendments (2026-09-27)

Written by unit W1-D of the M1 integration contract (`m1/M1-CONTRACT.md` v1.1, §11). Text-only parts of the M0 closeout and the measured choices of M1's first week and a half. The contract is normative for what M1 builds; this section records where the design text changed and what was measured. Unit report: `m1/W1-D-NOTES.md`; lab files under `scratchpad/frontier/m1/lab/{c4-v3,d18,d22,d24}/`.

### 21.1 Text changed in place

| Where | Change | Task |
|---|---|---|
| §0.5 item 5, §8.5 | every seed round is **the first quicknet round scheduled at or after** its time (`round_at` rounded down) | CL-19 |
| §2.1 | `bell_start`, `bell_end`, `T(b)` pinned; roster freeze and posture close at `bell_start(b)` | CL-20 |
| §8.2 | seed grammar: every account but the Season is a with-seed address of the Season PDA; `tag ‖ lowercase hex of little-endian fixed-width fields`, ≤ 32 B, SP-V2 byte for byte | CL-21, I-01, I-02 |
| §6.2, §6.4, §2.3, §14 D8 | the minimum tip in priority terms (`fees::min_tip_lamports`), zero tips removed | CL-22, I-08 |
| §6.2, §8.3, §9.4 | the one-way reveal latch (ClashInputs absence, `resolved_next`) | CL-23, I-07 |
| §8.5, §8.10 | AnnounceSeason, `t_create_min`, single-use id, creation bond | CL-24, I-09 |
| §8.7, §8.9 | 0.4-s rows tagged [model]; the 250-ms rescale unverified | CL-25 |
| §8.5, §8.6, §8.7, §6.4, `m0/SPIKE-SP-FEE.md` | "Clock" figure = confirmation lag; lookup-table lock count [unverified]; loaded-data limit `L(kind)` from the release program, not 64 KiB | CL-27, I-45 |
| §6.4, §8.8, §14 D18 | write classes W / D; pool pays window-closing writes only | CL-30, CL-31a, I-21 |
| §4.3, R8, §14 D23 | D23 decided; vacancy rule | CL-31 |
| §8.10 | Mandate claim deadline and its order to the Reckoning | CL-15 (text) |
| §2.2, §2.3 | ticket cohorts; the reachable onboarding times (11–21 min to a first holding, 31–41 min to a first clash report) | I-47, I-33, O-M1-13 |
| §12, §14 | M0 exit (c) met in the model on valuation (a); N block; D25 decided | CL-37 |

### 21.2 C4 model v3 (CL-26) [model on sim]

*Superseded by §22.2 (the final model with the measured Reveal CU and `L(reveal)`); kept as the wave-1 record.*

`c4_model_v3.py` (a copy of SP-FEE's v2 driver) with the M1 cost formula (`L` = 1 MiB), the minimum tip at priority 0.433, 1,200-s windows next to 600-s, the relic tip, and valuation (a) (N2) from `frontier-sim c4` v3 at the W1-D head (50k wallets, seeds 1–3, D23 on): **p99 bell $1,407, max bell $1,505** (the busiest faction takes part in 195–201 and 206–215 clashes), $2,185–2,255 with five relic clashes at $150.

| Keeper side (attack on a whole side) | 600-s window | 1,200-s window |
|---|---|---|
| minimum tip (p = 0.433), ≥ 150 rotating payers | 2.3–10.1× — **fails** at the max bell | 4.7–20.1× — depends on capacity |
| **pool cap P_def = 2.0, ≥ 150 rotating payers** | **11.8–47.9× — passes** (relic stress case included) | **23.5–95.9× — passes** |
| pool cap 2.0, one known payer | 4.4–19.3× — depends on capacity | 8.8–38.6× |

A relic reveal at the 100,000-lamport relic tip has priority 3.5 (26k Reveal) to 5.5 (16k), above the pool cap, so excluding one relic province for 600 s costs $17.5k–75.4k against a relic clash valued at $150 (117–502×). The table is final only with the measured Reveal CU distribution (wave 3; W6-E re-runs the model).

### 21.3 D18 re-sized (CL-30, CL-31a) [model on sim]

*Superseded by §22.3 (priced at the measured budgets table); kept as the wave-1 record.*

`frontier-sim c4` now counts the world-wide keeper writes of every bell (reveals, gather parts, resolves, transit and departure settlements, relic reveals; 32 beacon posts). `d18_model.py` prices a whole-block attack at the budgets of the M1 contract (§5.5) with `L` = 1 MiB. Pool spend per attacked bell:

| Wallets | Reveals per bell p50 / p99 / max | (A) every critical write eligible at 2.0: p50 / p99 / max SOL | 20 SOL covers (p99) | (B) Reveal only at 2.0: p50 / p99 / max SOL | 20 SOL covers (p99) | keepers' own spend in (B), p99 |
|---|---|---|---|---|---|---|
| 10,000 | 2 / 49 / 69 | 0.034 / 0.168 / 0.194 | 119 | 0.0001 / 0.0021 / 0.0030 | ≈ 9,400 | 0.041 SOL |
| 50,000 | 9 / 243 / 321 | 0.070 / 0.802 / 0.867 | 25 | 0.0004 / 0.0105 / 0.0139 | ≈ 1,900 | 0.196 SOL |

The reveal subsidy is 43,212 lamports per defended Reveal (priority 2.0 against the tip's 0.433 on a 27,576-unit Reveal); a delay-only resolve at 2.0 would cost 681k lamports, at 0.5 168k. **Recommendation (the closeout's rule): keep 20 SOL**, since (B) covers ≈ 1,900 p99 attacked bells at 50k (≥ 100); the per-bell cap is `R_bell × 43,212` lamports from a folded reveal count. What the pool no longer pays, keepers pay: ≈ 0.2 SOL of their own per attacked p99 bell at 50k. The owner confirms (D18). **R99** (p99 reveals per bell, world-wide) is **49 at 10k and 243 at 50k [sim]**, far below the contract's working default of 4,000, which sizes the keeper reveal-payer band at 0.215 SOL per payer; at the simulated R99 the band would be 0.008–0.016 SOL. The default stays until M1 measures reveals in play (E5, W6-E).

### 21.4 Simulator re-runs with D23 (CL-16, CL-31, CL-32, CL-33) [sim]

Bot criterion as the maximum over the bot's join window × stake × office (`criterion --best-response`, 3 seeds × 5 windows × 4 shares × 2 office choices = 120 seasons of 10,000 wallets):

| Run | Worst bot choice | Margin | Bots' office-terms at 1% / 10% bots (default mix) |
|---|---|---|---|
| seeds 201–203, no term limit (m0c economy) | 0.980 | 2.0 pt | 60% / 98% |
| seeds 201–203, **D23 on** | **0.979** | 2.1 pt | 9% / 71% |
| **held-out seeds 30002–30004, D23 on** (`--first-seed 30001`, CL-16) | **0.985** | 1.5 pt | 9% / 72% |
| D23 on, stake ramp 1.0 (D22), seeds 201–203 | 0.983 | 1.7 pt | 9% / 71% |
| D23 on, stake ramp 1.0 (D22), held-out 30002–30004 | 0.989 | 1.1 pt | 9% / 72% |
| D23 on, Relic Sites paying into the Mandate reserve (D24 variant) | 0.977 | 2.3 pt | 9% / 72% |
| **integ-W1: D23 with the caretaker first term exempt (H2)**, seeds 201–203 | **0.979** | 2.1 pt | 16% / 80% |
| **integ-W1: same, held-out 30002–30004** (the Gate W1 run) | **0.985** | 1.5 pt | 16% / 80% |

The rows above the integ-W1 rows counted the caretaker first term against D23 (a stricter rule than §4.3); the integ-W1 simulator exempts it, as the kernel's `office::counts_toward_limit` does. The worst bot choice is unchanged (0.979 / 0.985); bots hold more office-terms in the default mix (16% at 1%, 80% at 10%; the caretaker term is open to them again), 11% / 15% at the bot's best window [sim, integ-W1 lab `sim/crit-*.log`]. The D22/D24 rows were not re-run with the exemption.

Late stakers (days 15–21, with stake) at ramp 2.0 → 1.0: casual 0.57 → 0.60, daily 0.76 → 0.79, skilled 0.83 → 0.87, very skilled 0.86 → 0.89 (bots 0.85 → 0.89) [sim, `suite --only payout`, 3 seeds]. The D22 rule (pick 1.0 if its worst cell ≤ 0.985 with D23 on) passes on seeds 201–203 (0.983) and **fails on the held-out seeds (0.989)**, where ramp 2.0 gives 0.985: the measurement does not support switching, so the recommendation is **keep 2.0**, with 1.0 open to the owner at a ≈ 1-point margin. **Doctrine gate with D23:** the kernel table passes the 180-season proxy (largest |Δ index| 0.152%) and the draft control is rejected, but the Knight control moves to −0.178%, inside the ±0.2% bound; W1-D kept the per-push proxy on the pre-D23 economy; **the integ-W1 window re-calibrated it instead**: the proxy runs the Season-1 economy (D23, caretaker exempt) on 60 gate seeds (360 seasons), bound ±0.2% unchanged: kernel table largest |Δ index| 0.063%, Knight control −0.294%, A boost +0.338%, draft +8.77% (all three controls rejected). O5 band with D23 on: W1-D **6 of 6, largest gap 1.0 point**; integ-W1 (caretaker exempt, kernel at the integ-W1 head) **6 of 6 in 16.7% ± 2 on 1,500 paired seasons, largest gap 1.1 points, largest |Δ index| 0.069%, 1,500/1,500 conserve** (m0c: 1.1) [sim].

---

## 22. M1 wave-6 amendments (2026-09-28)

Written by unit W6-E of the M1 integration contract (`m1/M1-CONTRACT.md` v1.9, §11 wave 6): the M1 decisions that change this design, applied in place and listed here, and the final C4 model, D18 table and keeper payer band with the **measured** Reveal (CL-26, CL-30, I-49; exit item E8). The contract stays normative for what M1 builds. Unit report: `m1/W6-E-NOTES.md`; the committed model and its output: `m1/c4-v3/` (scripts and `c4-model-v3-final.txt`, `reveal-cu.txt`); lab copies and inputs under `scratchpad/frontier/m1/lab/c4-v3/w6e/`.

### 22.1 Text changed in place

| Where | Change | M1 source |
|---|---|---|
| Status | wave-6 bullet; pointers to §22 and the two operator documents | — |
| §2.2 item 4 | ticket cohorts in full: filing, the escrow in the Citizen, settlement order and outcomes, a fresh settlement waits for an earlier open cohort, finality, the price of theft, invites on chain | I-47, O-M1-13/20, DECISIONS K9/L4, I-51 |
| §6.2 | Depart as built (219-B data, maximum stamina, `HostInTransit`, 23,167 CU); departure values moved into the Holding by **SettleDeparture**, gathers read Holdings; Reveal as built (salt, ArrivalDay bit, measured CU); quota-refused and displaced arrivals bounce by rank; **settlement is the seal proof** (ProveBadSeal and SealVerdict removed; seal codes; `Forfeit`; tip, fee and bond to the settler) | I-11, I-12, I-32, I-44, I-06, I-10, O-M1-19 |
| §6.3 | GatherClash as built (Holdings, stamp, latch, the no-arrival fast path, 46,551 CU); ResolveFromInputs as built (Province + ClashInputs, Phase A, storage room, camps, 327,609 CU over 1,240 fills; Phase B 274,007, gate 290k; worst-case clash 410k from v1.10, 460k under Phase A); **the quiet-bell proof: ArrivalDay + SkipQuiet** (open item closed); settlement, `pool_owed`, claim grace, closes; no postures in M1 | I-10, I-13, I-14, I-15, I-43, I-46, I-48, I-52, I-56, I-16 |
| §6.4 | the "withholding" row; the defence pool as built (ArrivalSlot evidence only, the refund formula, DefenceClaim per keeper-day, the 6-bell claim grace); final `L(kind)` and `MULTI_MAX_REGIONS`; **final `tip_min` 14,668** and the relay presets | I-21, I-22, I-45, I-51, I-52 |
| §8.1–§8.4 | 17 kinds; SealVerdict and ProveBadSeal rows struck; **the M1 account set** (sizes, seeds, who pays, who gets the refund; per-player rent 0.0098 SOL); **the M1 measured cost table** (worst CU, gate, limit and `L(kind)` per kind); archived bells read the archive's signature | I-01, I-31, I-44, I-45, I-47, I-48, I-49 |
| §8.6 | rule 2 and rule 10 as built; **the M1 lock table** (who defends each written account, the effect and the price of holding it) | I-48, I-21 |
| §8.7, §8.8 | the final Reveal cost and keeper priority; the M1 per-player rent | I-45, I-31 |
| §9.1, §9.4 | the settlement seal code re-checked by the verifier (`BadSealSurvived`); the program-level gates G2–G12 recorded green | I-44, I-58 |
| §10, §11, §12, §14 D8 | audit rows B1/B6/E4/A5/C4 and the reuse rows restated for M1; the M1 milestone row "as built"; D8's final tip | — |
| §21.2, §21.3 | marked superseded by §22.2, §22.3 | — |

### 22.2 C4 model v3, final (CL-26, E8) [measured inputs; model]

**Measured Reveal inputs** [measured]:

| Source | Reveals | Program CU p50 / p99 / max | Notes |
|---|---|---|---|
| G1 worst fill (svm suite, release `.so`): first reveal of a province-bell, 32 steps over 4 provinces, displacement of the smallest of 4 full slots | — | — / — / **25,155** | named slot 24,300, displacement 20,350 (whole-transaction units, `budgets::MEASURED`) |
| svm suite, every landed single-Reveal transaction (release + test-beacon `.so`, W6-E run) | 126 | 20,172 / 25,155 / 25,155 (units per transaction) | the test-beacon build's 116: p50 19,694, max 22,922; the release build's 10 are the G1 fills (p50 24,300) |
| **In play:** `nightly-20260928` (W5-B, test key, 100 bots × 1 game day at 100×) | 5 | 19,826 / 20,985 / 20,985 | pre-wave-5 `.so` (1,032,688 B, limit 26,000) |
| **In play:** `w5-smoke` (integ-W5 Gate W5 run, test key) | 3 | 17,402 / 19,752 / 19,752 | limit 26,500, `L` 1,146,880 |
| **In play, real rounds:** `w6a-real1` (W6-A archive smoke, **release `.so` 874,120 B, sha256 `1b1968af…06fc`**, quicknet archive from G0 1788998400, 100 bots × 1 game day at 100×) | 10 | 19,911 / 21,418 / 21,418 | path 2–15 steps, ≤ 1 path province; verify PASS |
| **In play:** W6-A's `w6a-nightly-2`, `w6a-nightly-3` (test key, `.so` `094dc2d6…`, W6 base) | 8 + 3 | 19,891 / 20,736 / 20,736; 17,604 / 19,954 / 19,954 | `w6a-nightly-1` landed no Reveal |
| **In play, Phase B `.so` (876,272 B, `797675b4…16f7`), eager fleet:** integ-W6's `w5-smoke`, nightly 1 (`nightly-20260928`), `integ-w6-nightly-2`, `integ-w6-nightly-3` (test key, 100 bots × 1 game day at 100×) | 10 + 53 + 56 + 56 | 19,891 / 20,732 / 20,732; 18,694 / 21,463 / 21,463; 18,694 / 22,207 / 22,207; 17,982 / 22,207 / 22,207 | up to 4 reveals per arrival bell; nightly 3 holds the first **later** reveal of a province-bell (2 write locks): 17,550 CU |
| **In play, integ-W6r's Gate W6 re-run (Phase B `.so`, final tree):** `nightly-20260929`, `integ-w6r-nightly-2`, `integ-w6r-nightly-3` (100 bots × 1 game day at 100×) and the scale-2 latency run `w6-latency` (300 bots, 6 game hours, chaos; its `verify` input written by the extra `verify` of the re-run) | 58 + 66 + 68 + 23 | 18,776 / 22,207 / 22,207; 18,769 / 22,350 / 22,350; 18,673 / 22,355 / 22,355; 19,698 / 21,699 / 21,699 | the latency run's 23 are all first-of-bell (3 locks); the nightlies hold the lowest Reveals so far (13,861 CU) |
| **In play, pooled (integ-W6r)** | **416** | **18,743 / 22,207 / 22,355** | over eleven run directories with Reveals (thirteen read); whole-transaction max 22,805; every one ≤ the 26,500 limit with ≥ 15 % headroom (W6-E's 18: 19,772 / 21,418 / 21,418) |

Program CU is the Frontier program's own `consumed` line; each in-play transaction adds ≈ 450 units for its three ComputeBudget instructions. **Sample (integ-W6r):** 416 in-play Reveals over thirteen run directories (`m1/c4-v3/reveal-cu.txt`, rows in `reveal-rows.json`; run directories under `.local` are mutable — integ-W6's `w5-smoke` was overwritten by a Phase B run, so the JSON is the record), the scale-2 latency run included (its 23 Reveals needed a `verify` input, which the Gate W6 line does not write; the integ-W6r re-run added one). **Not yet included:** the 7-day 1,000-bot real-round season (w6-s7, the main session's run after wave 6). The wave-6 eager fleet (W6-C) raised the sample from 18 to 416 without moving p50 (19,772 → 18,743) or the maximum beyond the sensitivity case (22,355 < 23,500); the distribution is recomputed with one command from any run directory (`m1/c4-v3/README` in the committed copy).

**`L(reveal)`** = **1,146,880 B** (35 pages of 32 KiB): the budgets table's formula over Reveal's worst account set at the placeholder programdata (1,105,920 B, for a `.so` up to 884,736 B); at the W6-base release `.so` (programdata 1,093,632 B) the need is 1,124,234 B, which rounds to the same 35 pages. **`reveal_cu_limit`** = **26,500** (G1 worst + 5 %).

**What the measurement changes** [model]: a Reveal costs **28,100** cost units (2 write locks) and **28,400** on the first reveal of a province-bell (3: the ArrivalDay is written; every in-play Reveal so far was one); `tip_min` = ⌈0.433 × 28,100⌉ + 2,500 = **14,668** lamports. The scheduler prices the *requested* limit, so the in-play distribution moves no C4 figure: a keeper spending exactly `tip_min` bids **0.433** (2 locks) or **0.428** (3 locks, −1.1 %); at P_def 2.0 the priority fee is 53,700 / 54,300 lamports. A keeper that requested the in-play maximum + 5 % (22,500) would bid 0.505 at `tip_min` (+17 %), but a fill above its request fails and re-sends (I-50), so **the season keeps the G1 limit** (not adopted). The relic tip (100,000 lamports) bids 3.47.

**Value of one bell, valuation (a)** [sim, `frontier-sim c4` re-run at the W6-E base, D23 on, before Phase B's variance stream]: 50k wallets, seeds 1–3: the busiest faction takes part in p99 195–201, max 210–216 clashes per bell (with its own arrivals: p99 44–45, max 56–62) → **p99 bell $1,407, max bell $1,512**, $2,262 with five relic clashes at $150; with Relic Sites on (seed 1) the busiest faction's p99 is 191 and max 211 ($2,227 with five relic clashes), and relic provinces see p99 1, max 7 reveals per bell (W1-D: $1,407 / $1,505 / $2,255 / $2,185).

**Verdicts** (attack ÷ value, PASS ≥ 10×; ranges run from the lowest capacity case against the largest value case to the highest capacity case against the p99 bell) [model]:

| Keeper side (attack on a whole side) | 600-s window | 1,200-s window |
|---|---|---|
| minimum tip (0.433; 0.428 on a first reveal), ≥ 150 rotating payers | 2.3–10.1× — **fails** (passes only against the p99 bell with unscaled limits) | 4.5–20.1× — depends on capacity |
| minimum tip, one known payer | 0.9–4.2× — fails | 1.9–8.4× — fails |
| **pool cap P_def 2.0, ≥ 150 rotating payers** | **11.7–47.9× — passes** (relic stress case included) | **23.4–95.9× — passes** |
| pool cap 2.0, one known payer | 4.4–19.3× — depends on capacity | 8.8–38.6× — depends on capacity |

Excluding one relic province (known slots, one stream) at the relic tip for 600 s costs $17.2k–47.1k, 115–314× a $150 relic clash. **C4's status is unchanged by the measurement:** not met at the minimum tip (a recorded known result, O7/N1); met at the defence-pool cap with ≥ 150 rotating keeper payers on valuation (a) (N2) [model]; the M4 fee-market soak decides.

### 22.3 D18, final (CL-30, CL-31a) [model on sim counts, priced at the measured budgets table]

`c4_model_v3_final.py` §6 prices a whole-block attack at the budgets table's **CU limits, write locks and `L(kind)`** (what keepers request, on the merged Phase B table — integ-W6r; the W6-E base priced ResolveFromInputs at 344,000 → 345,900 and a provisional Phase B limit 288,000 → 289,900, output in git history at `46a3cfa`: GatherClash 49,000 → 53,600 cost units, ResolveFromInputs 285,500 → 287,400 (gate 290,000), SettleTransit 66,500 → 70,800, SettleDeparture 45,500 → 47,092, PostAnchor and PostSeed 356,500 → ≈ 358,100), and the pool's refund per defended Reveal by the program's formula `min(fee paid, 2.0 × cost − 2,500) − (tip_min − 2,500)` = **≤ 42,132 lamports** (41,532 on a 2-lock Reveal).

| Wallets | Reveals per bell p50 / p99 / max [sim] | (A) every critical write eligible at 2.0, p99 SOL | **(B) Reveal only: p50 / p99 / max SOL** | 20 SOL covers (B, p99 bells) | keepers' own spend in (B), p99 SOL |
|---|---|---|---|---|---|
| 10,000 | 2 / 49 / 75 | 0.153 (W6-E base: 0.175) | 0.0001 / **0.0021** / 0.0032 | ≈ 9,700 | 0.037 (W6-E base: 0.043 / 0.038) |
| 50,000 | 9 / 243 / 323 | 0.717 (0.835) | 0.0004 / **0.0102** / 0.0136 | ≈ 1,950 | 0.175 (0.204 / 0.176) |

**Recommendation (the closeout's rule, final): keep the 20-SOL pool** — (B) covers ≈ 1,950 p99 attacked bells at 50k, far above 100. **Caps:** the preset's `per_bell_region_cap` (0.2 SOL = 20 SOL / 100 bells) never binds an honest p99 bell (0.0102 SOL world-wide, even if every reveal fell in one region), and `per_keeper_day_cap` (2 SOL) covers one keeper defending every bell of a day at the 50k p99 (144 × 0.0102 ≈ 1.47 SOL): **both placeholders are adequate as final values** for M1 and Season 1 at ≤ 50k wallets [model]. The owner confirms D18.

### 22.4 The keeper reveal-payer band, final (I-49) [model]

A reveal payer fronts per reveal the ArrivalSlot rent (1,463,040), the ArrivalDay rent (1,137,920) and the fee at P_def (54,300): 2,655,260 lamports, returned as the accounts close. `F_r = 3 × ⌈R99 / 150⌉ × 2,655,260`:

| R99 source | R99 | F_r per payer | 150 payers hold (floor – ceiling) |
|---|---|---|---|
| in play so far (≤ 300 bots, 416 Reveals, integ-W6r) [measured] | 3 (max 4 in a bell) | 0.0080 SOL | 1.2 – 2.4 SOL |
| 10,000 wallets [sim] | 49–50 | 0.0080 SOL | 1.2 – 2.4 SOL |
| 50,000 wallets [sim] | 243–246 | 0.0159 SOL | 2.4 – 4.8 SOL |
| the code's default (`R99_DEFAULT`) | 4,000 | 0.2151 SOL | 32 – 65 SOL |

`F_r` is flat for every R99 ≤ 150 and steps by 0.0080 SOL per further 150 reveals per bell; the simulator gives ≈ 4.9 reveals per bell per 1,000 wallets at p99. **The keeper prices this floor at the Reveal it requests** (integ-W6r, W6-E F1: the budgets table's 26,500 CU and `L` 1,146,880 B with the first-of-bell 3 write locks, 2,655,260 lamports per reveal, so `F_r` = 215,076,060 lamports at the code default; before, the §5.5 gate at the 1-MiB placeholder gave 214,942,572). **Recommendation: configure `r99_reveals = 150` for seasons up to ≈ 30,000 wallets (the M1 local season and the playtest included) and 300 up to 50,000**; re-measure from the w6-s7 and exit seasons. The code's default of 4,000 is a keeper configuration value, not a rule, and stays until the owner accepts this (DECISIONS P3). Effective N ≥ 150 was met in every bell of every M1 run so far.

### 22.5 Still open after wave 6

1. The in-play Reveal distribution at scale: the 7-day, 1,000-bot real-round season (w6-s7). Run `frontier-stack verify` before `down`, then `m1/c4-v3/reveal_cu_from_runs.py` and `c4_model_v3_final.py` on its run directory (416 in-play Reveals so far, integ-W6r); only §22.2's in-play rows and §22.4's measured R99 can move (the verdicts use the requested limit).
2. §22.3 is priced at the merged Phase B budgets table (integ-W6r); the W6-E base output (Phase A column) is in git history.
3. C4 is a mainnet property: the M4 soak (leader execution rate, bundles, the 265-ms slot) decides it. Devnet cannot.
4. What a public-network keeper and the playtest still need is listed in `m1/RUN-A-KEEPER.md` §9 and `m1/PLAYTEST-RUNBOOK.md` §2 (TLS, a public-RPC feed for the keeper, an operator tool, address listing, BLS syscalls on devnet).
