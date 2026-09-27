# The Sixfold Frontier: M0 final report

- **Date:** 2026-09-27
- **Design:** `openworld/open-world-design.md`, **revision 3.1** (revision 3 kept as `.rev3.md`, revision 2 as `.rev2.md`). The M0 exit criteria are in §12. This report grades them as revision 3.1 states them. Where 3.1 changed a criterion that the owner had decided (C4, under O7), the report says so.
- **Code:** branch `codex/frontier` at **`bcc6382`** (on top of `360bc85` and `cea89be`), worktree `.claude/worktrees/frontier-integ`. The worktree is clean. All commits are local; nothing was pushed, so **GitHub CI has never run on this branch**.
- **Earlier report:** `frontier/m0/M0-REPORT.md` (first pass: 1 criterion met, 3 partial, 1 not met).
- **Inputs to this report:** design revision 3 and 3.1; SP-V2 and SP-FEE (m0b); K3-economy and K3-doctrines; the m0c integration (`bcc6382`); the three reviews (22 major and blocker issues confirmed and fixed, plus 21 minor issues).

**Tags:** [measured] means measured on real code (LiteSVM, solana-test-validator, cargo test, read-only mainnet RPC). [sim] means the host simulator running the real rules-v10 kernels with assumed player behaviour. [model] means arithmetic on assumed inputs. [estimate] means an extrapolation or a guess.

---

## 1. Verdict

**M0 is not complete.** Of six exit items, three are met, two are partial and one is not met. The one not met is blocked only by the owner's approval to push.

| # | Exit criterion (design §12, revision 3.1) | Status | One-line reason |
|---|---|---|---|
| (a) | Spike numbers recorded, CU rows on SBPF v2 | **met** | SP-V2 re-measured every critical-path instruction that exists, on v2, with the real kernel. The full Reveal does not exist yet (an M1 item) |
| (b) | Stance and doctrine balance: the CI proxy gate with negative controls, and 6/6 in 16.7% ± 2 on ≥ 1,500 paired seasons | **met locally** | 6/6 on 1,500 paired seasons, largest gap 1.1 points [sim]. The gate and its 3 controls pass locally. They have not run on GitHub (see the last row) |
| (c) | C4 (restated in 3.1): passes at the p99 and max bells with the defence pool at cap 2.0 and ≥ 150 rotating keeper fee payers, on the valuation the owner accepts; no choice among published seeds | **partial** | Passes **only in the model**, 12–540× [model]. It **fails at the default tip**, which is the point O7's wording names. Moving the test point to the pool cap needs the owner's confirmation. The anchor part is met [measured, lab] |
| (d) | β measured and γ set | **met in simulation** | β 0.86–0.88; herding ratio 0.684 at 3× faction size with γ = 0.6 [sim] |
| (e) | §9.4 kernel tests green, including quota fairness and seed margin | **partial** | Quota fairness and seed margin are green [measured]. The §9.4 **kernel-bounds family is still open** (the §4.8 list), and conservation checks are still identities, not bounds |
| — | GitHub CI green | **not met** | Never pushed. A push needs the owner's approval (standing rule) |

**What closing M0 takes:** about **one week of small work** [estimate] (§5), plus four owner actions (§6: N1–N4). None of the remaining work changes the architecture. **Recommendation:** start M1 program work now, and close M0 in parallel during M1's first week.

---

## 2. The exit criteria, one by one

### (a) Spike numbers recorded, CU rows on SBPF v2: **met**

SP-V2 built one lab program for SBPF v2 (ELF e_flags 2) with cargo-build-sbf 3.1.9. It combines the beacon, seal-proof and Join probes, and it runs the **real** rules-v10 `resolve_clash` instead of M0's stand-in. On-chain results match the native kernel's digest. A reviewer re-ran the suite in a copy and got byte-identical results, except Join, whose CU depends on the random wallets [measured].

| Instruction (SBPF v2, LiteSVM, mainnet feature set) | Result | Tag |
|---|---|---|
| PostAnchor (quicknet verify, safe create, store the 48-B signature) | 331,253–334,985 CU (v0: 888,981–899,738); 709 B after the tombstone fix | [measured] |
| PostSeed (keeper-nonce SeedCache) | 333,637 CU; 710 B (806 B with a separate keeper signer) | [measured] |
| ResolveClash, hybrid, worst of 52 adversarial fills | **367,243–632,641 CU**; 1,150 B; 31 locks; heap 25,872 B of 32 KiB | [measured] |
| ResolveFromInputs (full gather), worst of 40 fills | 540,892 CU; 358 B; 7 locks | [measured] |
| GatherClash, worst | 30,323 CU; 1,187 B; 32 locks | [measured] |
| ProveBadSeal with the FO check | ≤ 47,999 CU; 588 B | [measured] |
| Join | 14,404–24,968 CU (the spread comes from the Citizen bump search); 750 B; 15 locks | [measured] |
| Pre-funding-safe creation of 6 account kinds | The old CreateAccount path fails on a pre-funded address (Custom(0)); the new path succeeds, for about +1.6k CU | [measured] |
| Anchor uniqueness and address checks | 18/18 forgeries refused; a tombstoned bell refuses Reveal and PostAnchor (`t9`; the test fails when the check is removed) | [measured] |

**Caveats. None of them blocks this criterion.**
- **The clash is about 2.3× the old budget.** The design now budgets 650k CU per worst-case clash. This moves the comfortable design point to about **50,000 players** (adversarial load 8.8% of block CU), with the 10% line at about 60,000 (75,000 with a 20/h action bucket) [model, `lab/rev31`]. Engagements are 42% of kernel CU and the field-holding step is 39%; both are M1 optimisation targets [measured].
- **The full Reveal, CommitPosture/RevealPosture, SettleTransit and ArchiveAnchors are not built.** The full Reveal's CU (17–26k on v0 [measured, S-SIZE]) is an input to C4 (§2(c)) and is an M1 measurement.
- **Non-crypto rows (SettleTicket 19k, FileTicket 4k) are still v0 numbers.** SP-V2 found v0 and v2 within 0.5% for non-crypto code, so these rows stand [measured].
- **There are no separate written S-BEACON and S-TLOCK reports** (close item 8). Their v2 numbers are written up in SP-V2 `RESULTS.md`, and the v0 logs carry correction headers. I recommend accepting this substitution (N4).

### (b) Stance and doctrine balance: **met locally**

- **Stances.** `stance_table_is_in_equilibrium` passes in the rules suite: 354/354 in release, re-run for this report [measured].
- **Doctrines.** On the final table (commit `bcc6382`), 1,500 paired seasons at 10k wallets (seeds 10000–10249 × 6 rotations) put **6 of 6 in the band**: A 15.6, B 15.7, C 16.9, D 17.4, E 17.4, F 17.0%. The largest gap is 1.1 points, the largest |Δ index| is 0.043% (SE about 0.016), and 1,500 of 1,500 seasons were conserved [sim, `m0c/dk_final_paired250.md`].
- **The per-push CI proxy** runs 30 paired seeds, requires every mean index within ±0.2%, and has three negative controls that must fail. The kernel table passes (largest |Δ| 0.072%). All three controls are rejected: the draft table (A −2.06%), the Knight table (F −0.219%) and an in-bounds A boost (+0.384%) [sim]. The `frontier-sim` tests run 11 of 11 in 309 s on a loaded 16-thread machine [measured].
- **The scheduled workflow** `.github/workflows/doctrine-balance.yml` runs the O5 band nightly on the fixed seed set, plus an index bound on a fresh seed set each run.

**Caveats.**
- **The Knight control fails the gate only just** (0.219% against a 0.2% bound).
- **The table is provisional.** Doctrine parts whose mechanisms are not simulated yet (Bourse fee, caravans, Waystones, war horn, heartland sieges, survey) are balanced by argument only. The table must be re-tuned as those mechanisms land in M1–M3.
- **Not checked:** doctrines combined with unequal faction sizes.
- **Nothing here has run on GitHub.** On a 2–4-core GitHub runner, the per-push job may take about 15–40 minutes and the nightly job about 1–2 hours [estimate, scaled from the local CPU time].

### (c) C4, the fee-market criterion: **partial**

**What was measured** (SP-FEE, own solana-test-validator 3.1.9 on ports 39899–40025, since stopped and deleted; mainnet read-only):
- The cost tracker enforces 39.9M CU per account per slot and 98.8–99.9M per block, and the fee payer is capped too. SIMD-0306 (40% account cap) and SIMD-0286 (100M block) are active on mainnet. Mainnet slots are 0.263–0.275 s. Whether the 100M block limit was rescaled for 250-ms slots is **unverified** [measured].
- The scheduler ranks by (priority fee + 2,500) / cost. A keeper is excluded exactly when its priority is below the attacker's (16/16 well-packed floods, held for 8.9–14.4 s). A keeper that outbids lands in 0.53–2.1 s. While excluding, the attacker pays 1.01–1.16 × p × cap per slot [measured].
- **Known accounts cost one account's price.** One multi-lock stream held 4 slots for the price of 1. Drill (g) held a keeper's fee payer for 11.2 s at 0.0395 SOL per slot, and held 20 known payers with one stream; a fresh payer landed in 0.89 s [measured]. Only writes the attacker cannot list in advance (sealed destinations paid from unpredictable payers) force whole-block pricing.
- On mainnet, 13.2% of non-vote cost bids at priority ≥ 0.433. The attacker gets that part of each block without paying for it [measured, 24 blocks].

**Model result for one 600-s bell, side-wide** (`SP-FEE/results/c4-model.txt` v2):

| Keeper setting | Attack cost [model] | Verdict against 10× value |
|---|---|---|
| Default tip (priority 0.433 at a 16k Reveal; 0.274 at 26k), known payer | $1.4k–5.9k | **fails** on valuations (a) and (b); (c) passes except with a 26k Reveal on the execution-bound case (8.3×) |
| Default tip, ≥ 150 rotating payers | $3.2k–14.0k | **fails** on (a) (1.0–9.6×; one exception, 10.2× at the p99 bell if limits are not rescaled); (b) passes only with a 16k Reveal (11–44×); (c) passes |
| **Defence pool cap 2.0, ≥ 150 rotating payers** | **$26.5k–67.4k** | **passes on every valuation and capacity case, 12–540×** |
| Defence pool cap 2.0, one known payer | $9.9k–27.2k | on (a), passes only if leaders execute full 40M-CU account budgets |

The cost ranges span three capacity cases: execution-bound [estimate], 100M per 0.4 s, and 100M per 0.265 s if the limits were not rescaled.

**Value of the targeted side's bell, three valuations** [sim, `frontier-sim c4`, 50k wallets, 3 seeds]:
- **(a) $7 × every clash the side takes part in:** p99 $1,372, max $1,456 ($2,206 with relics).
- **(b) $7 × clashes with the side's own arrivals:** p99 $322, max $455.
- **(c) The counterfactual re-play:** the target's loss is −$105 to +$164, which is inside re-play noise. The conservative envelope is **$125–164**.
- The p99 and max bells are **one synchronized episode per seed** (days 10.6–11.0), made mostly of defence-only clashes.

**Why partial, not met:**
1. **The criterion was moved.** O7, as the owner decided it, restates C4 *at the default tip*, and there it fails on valuation (a). Revision 3.1 re-points the M0 exit to the defence-pool cap and records the default-tip shortfall as a known result. That is a reasonable reading of O7, since O7 also adopted the defence budget, but it changes a decided criterion. **The owner has to confirm it (N1).**
2. **The owner has not chosen a valuation (D25).** The pool configuration passes on all three, so this does not change the model verdict. It does change what "C4 holds" will mean at M4.
3. **The pass rests on parts that exist only on paper:** the ≥ 150-rotating-payer rule, the defence pool and its refund rules, and the keeper SDK rules. None is built.
4. **What a local validator cannot show:** Jito bundles, leader discretion, stake-weighted multi-leader ingress, Firedancer's pack, and execution time in a 265-ms slot. The M4 devnet or mainnet-fork soak decides these, and it needs owner approval for each step.

**The anchor part is met** [measured, lab]. The anchor address is predictable (`an ‖ hex(b, r)`), and there is exactly one per (bell, region). A second anchor, a forged anchor, a wrong seed round and a tombstoned bell are all refused. The seed is published 60 s after the reveal close. Holding an anchor only delays; a keeper that outbids lands in 0.53 s.

### (d) β measured and γ set: **met in simulation**

- β is 0.86–0.88 [sim]. The herding ratio at γ = 0.6 is 0.862 / 0.781 / 0.684 at 1.5× / 2× / 3× faction size (max 0.692 at 3×) [sim].
- γ = 0.6 (O6) is the default, and season parameters are now validated at creation.
- **Caveat:** simulated agents do not coordinate. Real β is likely higher, and M3 re-measures it with players. If β were 1.2, 1.4 or 1.6, the minimum γ would be 0.27, 0.54 or 0.81 [estimate].

### (e) §9.4 kernel tests: **partial**

| Family | State |
|---|---|
| Quota fairness | **green**: 400 cases × 8 reveal orders; a mutation of the slot rule fails it [measured] |
| Seed margin | **green**: kernel test over 60,000 anchors; SP-V2 on chain, where A + 599 lands and A + 600 is refused [measured] |
| Order independence, roster freeze, lag invariance, quiet bells, stance equilibrium | **green** at kernel level [measured] |
| No Reveal after the close; no choice among published seeds | **green in the lab** (SP-V2 `t3`, `t8`, `t9`); program-level tests are M1 gates [measured] |
| **Kernel bounds** | **open.** A spot check at `bcc6382`: `clash::validate` still accepts troops above `MAX_HOST_TROOPS`, an unbounded `dealt_bps` and stamina above the cap; `holding::duplicate_cost` is an unchecked multiply; `Stamina::set` can still lower a value. The design's §9.4 table marks the family "open" |
| Economy conservation | green as identities in every m0c run [sim]. Rewriting them as bounds, and keeping the Ledger per faction, is still open |
| Absence, pre-funding, gather equivalence, seal verdict | lab coverage in SP-V2; program-level tests are M1 gates (by design, not an M0 item) |

### GitHub CI green: **not met**

Local gate at `bcc6382` [measured]:
- `permutation-rules`: 354 passed, debug and release (I re-ran the release suite for this report: 354/0). fmt and clippy are clean.
- `permutation-server`: 88 passed.
- `permutation-chain`: 157 passed.
- `frontier-sim`: fmt, clippy and 11 tests pass.

None of this has run on GitHub. Pushing needs the owner's approval (N3).

---

## 3. The "to close M0" work list (design §12)

| # | Item | State |
|---|---|---|
| 1 | Design revision 3 | **done**, and revision 3.1 on top (the 22 major and blocker review issues fixed; §20) |
| 2 | Probes patched: pre-funding-safe init, with-seed addresses, canonical anchor; re-run ResolveClash and PostAnchor | **done** (SP-V2) |
| 3 | Every CU row re-measured on pinned SBPF v2 | **done** for every existing critical-path instruction; the full Reveal is not built (M1) |
| 4 | S-FEE redone with real contention plus the anchor lock drill D1–D6 | **done**, drills (a)–(g). D4 is not applicable (inline verification was dropped). D6 is withdrawn (predictable address). Clock-vs-drand skew was measured only as confirmation lag. The v0 + lookup-table 64-lock path never landed locally |
| 5 | Reveal ranking kernel with quota test, seed-margin test, §9.4 bounds, Mandate closed form | **3 of 4 done**. The kernel bounds are open |
| 6 | Doctrines 6/6 and the reduced CI job | **done locally** |
| 7 | Bot plan in the simulator | **done**: steps 1–4 measured in order; best-response gate at 0.980 [sim] |
| 8 | Written S-BEACON and S-TLOCK reports | **substituted** by SP-V2 `RESULTS.md` plus the correction headers (N4) |

---

## 4. What the second pass found, in brief

**Now established:**
- drand quicknet verifies on chain in about 332k CU on SBPF v2 [measured].
- A pre-funded address can no longer block account creation, and anchors are unique [measured, lab].
- A bad seal can be proven on chain for 48k CU [measured].
- The officer-pay ceiling works: officer pay can lift a claim to at most 95% of what the wallet paid.
- The staking-bot criterion passes as a maximum over the bot's own choices: worst cell 0.980 [sim].
- 6/6 doctrines are in the band [sim].
- Conservation holds in every run [sim].

**New or larger than thought:**
- **The real clash costs 2.3× the old budget** [measured]. Capacity falls to about 50k players comfortable [model]. Heap margin is 21%.
- **C4 fails at the default tip** [model], and **keepers' fee payers are a cheap lock target** [measured]. The defence pool and ≥ 150 rotating payers are required.
- **Bots hold most offices in the simulator:** 61% of office-terms at a 1% bot share, 77% at 2%, 93% at 5% [sim].
- **The criterion fails if bot officers can steer Mandates** to always-online tasks. With the share floor it is 1.040 at 1% when humans complete a quarter as often, and 1.22 when they cannot complete at all [sim]. A one-term limit does not fix this either (1.013 worst) [sim, `m0c/crit_floor_tl1_bm025.md`]. **Only the fixed Mandate menu, a design argument, removes the lever.** M3 must simulate it.
- **Honest late stakers lose 10–18 points**, and the skill premium over a script is about 0 to −0.03 [sim]. Option D22 softens this.
- **The quiet-bell backlog:** a province untouched for a day needs up to 144 absence-proof resolves, about 10.5M CU or about $0.11 [model]. M1 must choose a cheaper proof.

---

## 5. What remains to close M0

Ordered. The estimates are for the team that ran v9 [estimate].

1. **Kernel bounds (§9.4 row; the M0 §4.8 list), about 1.5–2 days:**
   - Walls, production and upkeep caps. `clash::validate` must refuse troops above `MAX_HOST_TROOPS`, `dealt_bps` above `COMBAT_MAX_BPS` and stamina above the cap.
   - Checked `duplicate_cost`; a `ProvinceCoord` ring bound; `Stamina::set` monotone; faction ids limited.
   - Conservation checks written as bounds; a per-faction Ledger.
2. **Code-review minors still open at `bcc6382`, about 1 day:**
   - `PayoutParams::validate` must refuse `office_ceiling_bps > 10_000`. Today it accepts up to 5× paid, which would allow a profitable office.
   - Remove the `CitizenRecord::weights()` compile-time Works-rate trap.
   - Closed form for `EntrySchedule::accrual_left`, which today loops up to 366 terms per Join.
   - Bound the product of combat multipliers.
   - Add `criterion --first-seed` and re-state the criterion on held-out seeds.
   - Specify the Mandate claim deadline and its order relative to the Reckoning snapshot.
   - Decide where the officer-ceiling sweep goes in Season 1, which has no successor season.
3. **Design minors not applied in 3.1, about 1 day:**
   - §8.5 still writes every seed round as `round_at(…)`, which rounds down. The kernel and the lab round up; the text must say "the first round at or after", for bell, ring and genesis seeds.
   - `T(b)` has the same rounding issue, which moves the posture and roster cutoffs.
   - Pin the with-seed grammar and length bounds for every account type.
   - A minimum reveal tip, in priority terms.
   - A one-way reveal latch: Reveal refuses once ClashInputs exists.
   - An operator re-roll guard on the genesis seed (announce the season id in advance, or make a pre-join Abort cost something).
   - Mark the 0.4-s slot rows as [model].
4. **Spike minors, about 1 day:**
   - **Pool sizing:** D18's "≤ 0.02 SOL per attacked bell" counts only the target's reveals. A whole-block attack also delays every PostAnchor, PostSeed, gather and resolve, which is on the order of 1–2 SOL per war bell [model, review]. Resize D18 and consider escalating only the writes that close a window.
   - Report 1,200-s windows next to 600-s ones, and price relic clashes at the relic tip.
   - Relabel the Clock-drift figure as confirmation lag. Mark the lookup-table lock count as unverified.
   - Seed Join's wallet RNG. Run the 1,200-fill worst-case search on ResolveFromInputs, which is now the primary path.
5. **Push and GitHub CI**, after owner approval (N3). Then fix whatever differs on the runner (toolchain and time limits).

Total: **about 5–6 working days** [estimate], which fits inside M1's first week if M1 starts now.

---

## 6. Owner decisions

**New decisions needed to close M0:**

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| **N1** | Accept revision 3.1's restated exit (c): C4 is tested at the defence-pool cap 2.0 with ≥ 150 rotating keeper payers, and the default-tip failure is recorded as a known result | **Accept.** It follows O7's defence budget. Peacetime cost stays at the default tip, and the pool pays only during an attack | Keep O7's default-tip wording and **raise the default tip** to priority ≈ 2.0 (about 37k–57k lamports per reveal instead of 10k, paid on every reveal whether or not anyone attacks) [model] |
| **N2** | D25: which C4 valuation counts | **(a), the most conservative** ($7 × every clash). The pool configuration passes it (12× at the worst case) [model] | (b) arrivals only; (c) the simulator's counterfactual |
| **N3** | Approve a push of `codex/frontier` so GitHub CI runs (which remote and branch) | Approve a push of the branch only (not `main`, not `codex/magicblock-playable`) | Accept the local gate as M0 evidence and push at M1 |
| **N4** | Accept SP-V2 `RESULTS.md` in place of separate S-BEACON and S-TLOCK reports | Accept | Ask for the two reports (about half a day) |
| **N5** | Start M1 before M0 formally closes | **Yes.** The §5 items are small and do not change the architecture | Wait about one week |

**Still-open working defaults:**

| # | Default now | Note |
|---|---|---|
| D18 | Defence pool 20 SOL, cap 2.0, refund by later claim | **Needs re-sizing** (§5 item 4) |
| D19 | Reveal window 600 s, season parameter 600–1,800 s | A 1,200-s window doubles the attacker's cost [model] |
| D20 | Seal bond 20,000 lamports | — |
| D22 | Stake ramp 2.0 | 1.0 gives late stakers M0's day-0 level; the bot margin falls to about 1.5 points [sim] |
| D23 | No office term limit | One term per wallet cuts bots' office share at 1% from 61% to 9.5%, and the criterion still passes (0.967) [sim]. **Worth adopting**, though it does not fix Mandate steering |
| D24 | Relic Sites mint no laurels | Their role is unsettled |
| D2–D12, D15 | Revision-2 working defaults | unchanged |

The standing rule is unchanged: every devnet step and any push needs separate owner approval.

---

## 7. M1 "First Bell": what it builds, and how long

**Plan:** design weeks 7–15, so **about 8 weeks** after M0 [estimate]. With M0's closing week overlapping, M5 (mainnet Season 1) stays at weeks 32–36, **about 8.5 months** from the start [estimate]. There is no live game before the M1 playtest (O8).

**What M1 builds:**
- **The Frontier program on Solana base, SBPF v2, no money yet:**
  - Join, Citizen, Holding (keyed by site), and the Province with the garrison mirror.
  - Site tickets with displacement, and FoldOccupancy.
  - Harvest, Build, Train and Muster.
  - Depart with the mandatory seal, Reveal, GatherClash, ResolveFromInputs (primary) and ResolveClash.
  - ProveBadSeal and SealVerdict, and SettleTransit.
  - Explore, PostAnchor, PostSeed and ArchiveAnchors with tombstones.
  - The genesis and ring seeds from quicknet, and OpenRing/OpenProvince.
  - Dormancy, logs, and verifier v2 with the seal audit.
- **The keeper with defence-pool escalation**, following the SDK rules: loaded-data limit, bids in priority terms, ×2 per slot, and ≥ 150 rotating payers.
- **The Herald read path.** The current server breaks at 500–1,000 viewers.
- **The web client:** map, holding panel, march UI, guided first bells, and an off-chain practice mode.

**M1 exit:**
- Every instruction is under budget on v2 with adversarial fill.
- Each init path has a pre-funding test, and each keyed account has a forgery test.
- Program-level lag and "no Reveal after close" property tests pass.
- A 7-day local season with 1,000 bots runs, and the verifier passes (and fails on tampering).
- **Only then, and only with the owner's approval,** a private devnet playtest with 50–200 people and no money.

**Measurements M1 owes M0's open questions:**
1. The full Reveal's CU. It sets the default tip's priority and one C4 input.
2. The quiet-bell proof and its cost.
3. Clash kernel optimisation: the field-holding step and engagements, with the heap margin kept.
4. The Reveal ArrivalSlot tie-break computed at Reveal time.

**Main M1 risks:**
- Scope: a new program plus about 60% new rules code.
- Heap and CU growth in the clash.
- The read path.
- **Doctrines will drift from balance** as new mechanisms land, and must be re-tuned each time.

---

## 8. What I ran for this report

- **Read:** design revision 3.1 (§0, §6.2–6.4, §8.3–8.9, §9.4, §12–§14, §19–§20); the first-pass M0 report; the m0c outputs (`gate_best.md`, `gate_crit.md`, `dk_final_paired250.md`, `dk_final.log`, `c4_50k_s3.md`, the `crit_*` Mandate runs); SP-FEE `c4-model.txt`; the branch log and CI workflows.
- **Checked in code at `bcc6382`:**
  - `clash::validate` and `holding::duplicate_cost` are still unbounded.
  - `PayoutParams::validate` accepts `office_ceiling_bps` up to 5× paid.
  - `accrual_left` still loops.
- **Re-ran:** `cargo test --release -p permutation-rules`: 354 passed, 0 failed [measured]. The worktree stayed clean.
- **Not done:** no commits, no push, no servers, no devnet or mainnet transactions.

## 9. Links

- This report: `(session scratch)/scratchpad/frontier/m0b/M0-FINAL.md`
- Japanese version: `(session scratch)/scratchpad/frontier/m0b/M0-FINAL.ja.md`
- Design (revision 3.1): `(session scratch)/scratchpad/openworld/open-world-design.md`
- Japanese summary: `(session scratch)/scratchpad/openworld/summary-ja.md`
- First-pass report: `(session scratch)/scratchpad/frontier/m0/M0-REPORT.md`
- SP-V2: `(session scratch)/scratchpad/frontier/m0b/spikes/SP-V2/RESULTS.md`
- SP-FEE: `(session scratch)/scratchpad/frontier/m0b/spikes/SP-FEE/RESULTS.md` and `.../SP-FEE/results/c4-model.txt`
- Economy and doctrines: `(session scratch)/scratchpad/frontier/m0b/sim/ECONOMY.md`, `.../m0b/sim/DOCTRINES.md`
- m0c runs: `(session scratch)/scratchpad/frontier/m0c/`
- Code: `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-integ` (branch `codex/frontier`, `bcc6382`). Kernels: `permutation-rules/src/frontier/`. Simulator: `frontier-sim/`. CI: `.github/workflows/ci.yml`, `.github/workflows/doctrine-balance.yml`
