# Wylls: M0 status report

- **Date:** 2026-09-27
- **Design:** `openworld/open-world-design.md`, revision 2. The M0 exit criteria are in §12.
- **Code:** branch `codex/frontier` at `cea89be`, worktree `.claude/worktrees/frontier-integ`. The branch holds local commits only; nothing was pushed, so GitHub CI has never run on it.
- **Owner decisions this report follows (2026-09-27):** build the Frontier on Solana base; take bell randomness from drand and verify it on chain; stop the v9 security round without finishing it.

**Tags**

| Tag | Meaning |
|---|---|
| [measured] | measured by us on real code (LiteSVM, solana-test-validator, cargo test, read-only RPC) |
| [sim] | output of the host simulator: the real rules-v10 kernels, driven by assumed player behaviour |
| [model] | arithmetic on assumed inputs, such as the fee-market cost model |
| [estimate] | an extrapolation or guess |

---

## 1. Summary

**M0 is not complete. Of the five exit criteria, one is met, three are partial and one is not met.**

| # | Exit criterion (§12) | Status |
|---|---|---|
| 1 | Spike numbers recorded | **partial** |
| 2 | Stance and doctrine balance in CI | **partial** (stances yes, doctrines no) |
| 3 | C4: block cost ≥ 10× the marginal prize value of a clash at the default tip | **not met** (never measured) |
| 4 | β measured and γ tuned so the herding ratio ≤ 1.0 at 3× size | **met in simulation** |
| 5 | §9.4 property tests green | **partial** (3 of 4 families exist and pass) |

**What went well**
- The rules-v10 kernels exist as `permutation-rules::frontier` behind `RULES_VERSION = 10`: world, clash, siege, terrain, travel, pools, laurels, index and payout.
- v9 behaviour is unchanged. The server golden replays still pass, and `params::RULES_VERSION` is still 9 [measured].
- The test suites pass: 338 rules tests in debug and release, 88 server tests and 157 program host tests [measured].
- The simulator ran 1,575 of 1,575 settlements with every conservation check passing, and it replays bit for bit across threads [measured].
- The crypto path works on Solana:
  - drand beacons verify on chain within budget once the program is built for SBPF v2 [measured].
  - A tlock seal can be opened on chain for about 43k CU [measured]. This lets the chain prove a bad seal, which was not possible before.

**What went badly**
- The reviews found **five blockers.** All are confirmed.
  - The two kernel blockers are fixed in `cea89be`.
  - The three spike blockers are design faults. They need design changes and new probes:
    - anyone can pre-fund an address to block account creation;
    - the BellAnchor is not unique, so a caller can choose between seeds;
    - the S-FEE drill never created contention.
- **C4, the central claim of the fee-market defence, has not been tested.**
  - The S-FEE drill never created contention.
  - The per-account write cap on mainnet is 40M CU, not the 12M the design assumes.
  - At 40M, the cheapest attack prices out whole blocks rather than single accounts. The design's "blast radius of one province" framing therefore does not hold.
- **Doctrines are not balanced.**
  - The draft table fails completely: Lumen wins 99.5% of 600 seasons [sim].
  - The tuned proposal puts 3 of 6 doctrines inside the target band [sim].
- **The bot criterion planned for M3 already fails at low bot shares** [sim]. A staking bot returns 1.07× what it pays at a 1% bot share and 1.05× at 2%. If bots can win paid offices, a staking bot returns 1.52× at 2%.

**Recommendation.** Do not start M1 program work on the contention-sensitive parts yet: Reveal, ResolveClash, the beacon accounts and account initialisation. First, write design revision 3 with the changes in §4. Then re-run S-FEE with real contention and the anchor lock drill, and re-measure CU under a pinned SBPF v2 build. Kernel and client work that does not depend on these parts can continue.

---

## 2. Exit criteria, one by one

### 2.1 Spike numbers recorded: **partial**

Every spike left raw numbers on disk, but not every spike has a report, and some required measurements are missing.

| Spike | State | Main numbers | Where |
|---|---|---|---|
| **S-BEACON** | done; no written report (a harness hook blocked it); corrections prepended to the log | **quicknet** verification: 882–895k CU on SBPF v0, 324–330k on v2. **evmnet:** 139–186k (v0), 79–95k (v2). PostAnchor quicknet 889k (v0) / about 332k (v2). ResolveClash reading a SeedCache: 973 CU, 1,128 B. With inline verification: 1,427 B (evmnet) and 1,476 B (quicknet), **over** the 1,232 B limit. Without hash-to-curve hints: > 1.4M CU. [measured, LiteSVM; reproduced by the reviewer] | `spikes/S-BEACON/results/*.json`, `run-log.txt` |
| S-BEACON anchor lock drill | **not done** | none | to be folded into the redone S-FEE |
| **S-FEE** | done, but the **verdict is withdrawn** | The drill's fill transactions declared 200k–1.4M CU but used about 4k, so no contention formed. The reviewer's corrected drill on test-validator 3.1.9 packed 30 fills of about 1.30M CU per slot on one account (39M CU). With the attacker at 1.0 lamport/CU, a defender bidding 0.01 or 0.5 did not land within 12 s; a defender at 2.0 landed in 459 ms. The attacker spent about 0.085 SOL/s [measured]. | `spikes/S-FEE/REPORT.md` (correction header); reviewer `scratchpad/review-fee/drill3.json` |
| **S-TLOCK / S-TLOCK-V** | done; no written report (hook); corrections prepended | Compact quicknet seal 165 B; Depart 667 B. On-chain opening 33.8k CU without the FO check, 42.7k with it; 488 B. Beacons appear 0.8–2.0 s after their round time. The evmnet tlock round trip works (229 B seal), but only with an unofficial JS library at version 0.0.1. On-chain opening of an evmnet seal was not measured. [measured] | `spikes/S-TLOCK/results/sbf-results.txt`, `q*-result.json`, `e1-evmnet-result.json` |
| **S-SIZE + S-JOIN** | done; `results.txt` with corrections | Adversarial ResolveClash: 247–270k CU, 1,152 B, 31 locks. Reveal 17–26k. Join 18.3k (estimate was 45–60k). SettleTicket 19k new / 11k displacement. FileTicket 4k. 24 absence proofs: 96k CU with PDAs, 12k with with-seed addresses. Postures do not fit: 3 PosturePDAs push the tx to 1,251 B. GatherClash 13–29k per gather; ResolveFromInputs 262k, 391 B. [measured, LiteSVM, SBPF v0] | `spikes/S-SIZE-JOIN/results.txt` |
| **S-VRF-LOCK** | **deferred** | none | see §3 |

**What is missing to call this met**
- A written S-BEACON report and a written S-TLOCK report.
- The anchor lock drill.
- A valid S-FEE run.
- Every CU row re-measured under the SBPF version the Frontier program will actually use. Most rows above are v0, and v2 cuts crypto cost 1.9–2.7× [measured].
- A decision on S-VRF-LOCK.

### 2.2 Stance and doctrine balance in CI: **partial**

- **Stances: met locally.** The test `stance_table_is_in_equilibrium` in `permutation-rules/tests/frontier_world.rs` checks two things: no stance is dominated, and withholding (Disarray) is strictly worse than revealing any stance. The CI job runs `cargo test --locked --release -p permutation-rules`, which includes this test. It passes locally [measured]. It has not run on GitHub, because nothing was pushed.
- **Doctrines: not met.**
  - Doctrine balance lives only in `frontier-sim`. That crate is excluded from the root workspace and from CI. Its one doctrine test checks that settlement works, not balance.
  - The balance results fail. With the draft §4.1 table, 0 of 6 doctrines land in the 16.7% ± 2-point band, and Lumen wins 99.5% of seasons [sim].
  - The tuned proposal removes the direct multipliers on scored facts. With it, 3 of 6 doctrines land in the band (A 18.2%, B 16.8%, D 18.7%; C 20.2%, E 13.8% and F 12.3% miss) [sim].
  - The band is tight because a faction's index varies only about 1.4% between seasons. The binomial standard error of one win rate at 600 seasons is 1.5 points [sim].
- **To close:** tune the doctrine table until all six land in the band. Then add a reduced doctrine run, small enough to finish in minutes, as a CI job.

### 2.3 C4: **not met (not measured)**

- **The drill never tested the defence.** S-FEE's flood released almost all of its declared CU, so the per-account cap never bound. The spike's conclusion that a local validator cannot enforce the cap is wrong. The reviewer's corrected drill shows that it does enforce it [measured].
- **The account cap is 40M CU on mainnet, not 12M.** SIMD-0306 has been active on mainnet since slot 379,296,000 [read-only RPC]. It sets the per-account cap to 40% of the block limit, which is 40M with a 100M block. We did not verify the 100M block limit ourselves; the 30 × 1.30M ≈ 39M packing we observed is consistent with a 40M cap [measured].
- **What that does to the model** [model]:
  - Holding one account costs the attacker p × 40M CU per block, about 3.3× the design's figure.
  - Four ArrivalSlots at 40M each exceed a 100M block. So blocking one faction's 4 slots for one 10-minute bell means pricing out the whole block. At the default tip that costs about 110 SOL gross, or 55 SOL if the keeper's bid is counted net of the 5,000-lamport base fee (about $16.5k / $8.3k at $150 per SOL).
  - The same purchase blocks **every** Frontier reveal in the world that bids below the attacker's price. The design's "one held account delays at most one province" does not hold for this attack.
- **On paper the margin looks large, but nobody has measured it.** $8.3k against 10 × a $5–7 clash is a wide margin. But the $5–7 clash value is a model input, and the attack now hits every clash in that bell, not one. C4 has to be restated (§4.4) and then drilled.
- **To close:**
  - Redo S-FEE with fills that consume their full declared CU, packed to the cap.
  - Measure exclusion against the attacker's and defender's prices for 1, 2 and 4 accounts and for the full block.
  - Compare the measured spend with p × min(k × 40M, 100M) × blocks.
  - Include the anchor lock drill.
  - Keep a devnet or mainnet-fork soak in M4 for Jito and multi-leader behaviour.

### 2.4 β measured and γ tuned: **met in simulation**

- **β is measured in the simulator, not from real play.** The index β is 0.86 at 2× and 3× size [sim]. By path, Dominion is 0.44–0.55 and the others are 0.97–1.00. Dominion is low because small factions spread thinly and hold more Marches per member.
- **The herding ratio at 3× size** (big faction's payout per USDC ÷ the small factions') is 0.910 at γ = 0, 0.786 at γ = 0.3 and 0.680 at γ = 0.6 [sim]. So the criterion holds for every γ ≥ 0 in the simulator.
- **Caveat.** The simulated agents do not coordinate, and real players in a big faction probably will, so real β is likely higher. If real β is 1.2 the minimum γ is 0.26; at 1.4 it is 0.53; at 1.6 it is 0.81 [estimate].
- **Recommendation:** keep γ = 0.6 until M3 measures β with real players; γ = 0.3 is the softer option if real β stays ≤ 1.2. The kernel stores γ as a fraction, so it can be changed per season.

### 2.5 §9.4 property tests green: **partial**

| Family | State | Evidence |
|---|---|---|
| Order independence | **green (kernel level)** | clash: 300 scenarios × 7 shuffles; sharded faction totals built in 7 orders give the same settlement (`incremental_sharded_totals_are_order_independent`) [measured]. Site lottery, Bourse clears and folds have no kernels yet (M1/M3). |
| Roster freeze | **green** | `actions_during_a_bell_do_not_change_its_clash` now drives the kernels' own host, garrison, walls and relations state. Merges, splits, Departs, garrison changes, wall completions and decrees land mid-bell. A control that applies the same actions immediately changes the result [measured]. |
| Lag invariance | **green** | `clash_results_do_not_depend_on_lag` plus the new two-province test `arrivals_carry_the_origin_clash_whatever_the_lag`. The integrator removed the new origin-resolution check and confirmed that the two-province test then fails [measured]. |
| Quota fairness (ArrivalSlot set = the 4 largest same-faction arrivals, whatever the reveal order) | **missing** | There is no Reveal-time slot-ranking kernel and no test. The existing fair-share tests (`fair_share_admits_every_faction`, `allies_cannot_pool_hex_slots`) cover the hex slots inside the clash, which is a different rule. |

All of these are kernel-level tests. Program-level versions come in M1.

---

## 3. S-VRF-LOCK is deferred, and what that means for the genesis co-seed

**Why deferred.** The v9 security round was stopped, so there will be no Skirmish or Arena lane on the MagicBlock ER. S-VRF-LOCK was scoped for exactly those lanes and for the Frontier's genesis co-seed. Nobody measured how the VRF oracle behaves when its callback account or queue is held, or its fulfilment rate over 7 days.

**What the design relied on.** In §8.5, the genesis seed is `H(MagicBlock VRF output ‖ drand round after that output landed)`. This seed places genesis rings 2..g, and its purpose is to stop the operator pre-solving the terrain. §11.2 ports v9's VRF plumbing (WP11) only for this. J1–J5 lists MagicBlock VRF as a trust root.

**What changes now**
1. **There is no evidence for the co-seed path.**
   - The VRF oracle's fee does not escalate, and its queue is a cheap lock target [model, §8.5].
   - `solana-vrf` issue #72 reports legacy requests that were never fulfilled [doc].
   - At genesis, a lock or a miss only delays the season before joins open. It is not a money risk. But it is a non-escalating third party on the path to opening a season, which §8.6 rule 8 otherwise forbids.
2. **The hardened version of WP11 will not exist.** It was due to be finished and regression-checked (scoped `identity_mode` 1) in v9 waves 3–4, which will not happen. Porting it would mean porting unreviewed code.
3. **A drand-only genesis seed gives the property the design wants.**
   - Define the genesis seed as the season network's drand round at CreateSeason's on-chain time plus at least 10 minutes. By then the join parameters, `roster_root` and `policy_code_hash` are fixed.
   - The operator cannot know that round in advance. The only exception is if the operator colludes with a threshold of the League of Entropy, and that is already the trust assumption for every bell seed, ring seed and tlock seal.
   - The VRF co-seed only guarded against that one collusion case, for genesis alone.
4. **Recommended default (owner decision O2):** drop the VRF from the Frontier. Then:
   - The genesis seed is the drand round defined above.
   - Remove WP11 from the §11.2 port list.
   - Remove MagicBlock VRF from J1–J5, from the §8.5 table and from the §8.6 table.
   - The alternative is to keep the co-seed, run S-VRF-LOCK in M1 against the scoped identity mode, and accept a non-escalating dependency before joins open.
5. **Other text that assumed Skirmish is now stale.** §2.8, §11.2 ("stays as the Skirmish program"), §15, D13, D14, and the mitigations for R3 and R15 ("Skirmish as the tutorial", "Skirmish is free"). Also, several v9 fixes the Frontier was going to port (P2 and P3: token checks, solvency, Abort and refunds, roster reveal) belonged to waves that were not finished. They must be reviewed fresh in M2, not treated as a hardened reference.

---

## 4. Design changes the evidence requires

The M0 integrator did not edit the design. Everything below should go into revision 3.

### 4.1 Accounts and initialisation (blockers)
1. **Pre-funding-safe initialisation (§8.6, §10 A1).**
   - Never use System `CreateAccount` or `CreateAccountWithSeed` for an account the program creates. Initialise by transferring up to rent-exempt, then Allocate, then Assign (the `...WithSeed` variants for with-seed addresses), all signed by the program.
   - Absence proofs must reject a pre-funded system account.
   - Add one pre-funded-address LiteSVM test per init path to the M1 gates.
   - Why: today, blocking one faction's 4 slots in one province-bell costs 0.0026 SOL (about $0.39). Pre-funding every anchor of a season costs about 41.9 SOL and freezes the world [measured / derived].
2. **Exactly one BellAnchor per (season, bell, region) (§8.2, §8.5).**
   - Store and check the canonical address on every read.
   - The SeedCache records the anchor it was derived from.
   - Add a forgery test with a non-canonical bump.
   - Why: a second anchor under bump 253 got a later timestamp and a different seed, and Resolve accepted both [measured].
3. **With-seed addresses for ArrivalSlot and PosturePDA,** derived from the Season PDA, with canonical absence proofs. 24 absence proofs cost 12k CU this way against 96k with PDAs; creation costs +1.2k against +9.8k [measured]. Add a test in which a resolve that omits an existing slot fails.

### 4.2 Randomness (§8.4, §8.5)
4. **Drop inline beacon verification from ResolveClash.** It does not fit in one transaction (1,427–1,476 B) [measured].
   - Use keeper-nonce SeedCaches `['seed', season, bell, region, nonce]`, up to 256 per region-bell. They all hold the same seed.
   - ResolveClash reads any valid cache (973 CU).
   - Delete §8.6 rule 8's "or can be bypassed (inline beacon verification)" and the matching §8.4 and §8.6 text.
5. **The BellAnchor is the one lockable critical beacon account.**
   - Correct §8.4: holding the anchor does delay the whole region. What it cannot do is let anyone choose between seeds that are already public.
   - The invariant to test is "nobody can choose between already-published seeds".
   - Defences: anchor addresses that cannot be predicted (seeds include `sha256(sig_T)[..8]`); one combined 16-region anchor transaction per bell, with per-region fallback; keeper escalation priced by the redone S-FEE.
6. **Add a seed-round margin.**
   - Define `S(b, r) = round_at(A + 600 s + M)`, with M ≥ 60 s. S-BEACON proposed about 30 s; the review wants ≥ 60 s. Review M against measured Solana clock skew.
   - Reveal refuses once `Clock ≥ A + 600` or once any round ≥ S is on chain.
   - Add a property test: seed time ≥ close + M.
   - Why: `round_at` rounds down, and beacons appear 0.8–2.0 s after their round time [measured]. So without a margin, the seed can be public before reveals close.
7. **One drand network per season for both anchors and tlock.** §8.5's "anchors from evmnet while tlock opens on quicknet" is wrong: an evmnet anchor proves nothing about whether the quicknet tlock round exists. The recommended default is quicknet with an SBPF v2 build (owner decision O1).
8. **Hash-to-curve hints are part of the PostBeacon and PostSeed instruction format.** Keepers and the SDK compute them; without them, verification exceeds 1.4M CU [measured]. The hints are 65–129 B per map on evmnet and 145 B on quicknet.
9. **Add a non-critical `ArchiveAnchors` crank** (bell → seed time per region-day), so anchors and caches can close after 48 h. A province more than 48 h behind still needs its anchors, and keeping every anchor for a season costs about 62.9 SOL [model].

### 4.3 Sealed orders (§6.2, §6.3, §6.4, §8.2, §8.3)
10. **Seal format.**
    - A 165-B compact tlock block: quicknet, 16-byte key, the standard 128-B IBE block plus a 37-B SHA-256-keystream body, salt = sha256(PS-SALT ‖ k).
    - Pin the scheme, DST and hashes in `frontier::seal`.
    - The envelope is custom cryptography, so it needs a formal spec and an outside review. Use only the binary form, never armored age.
    - In §8.3, the log size is 165 B, not "≈ 300 B", and Depart is 667 B.
11. **Depart stores `seal_root = sha256(commit ‖ sha256(seal))`** in the existing commit field. Reveal passes `ct_hash` (+32 B) and stays free of pairings.
12. **BellAnchor stores the verified round-T(b) signature** (64 → 112 B), so any region's anchor can serve a seal proof.
13. **New permissionless `ProveBadSeal` writing one `SealVerdict` PDA.**
    - Core cost is about 43k CU with the FO check [measured].
    - If a proof ever has to verify the signature inline instead of reading an anchor, it would cost about 330k CU on v2 [measured, S-BEACON]. S-TLOCK's "190–200k" estimate is too low.
    - `SettleTransit` destroys a host whose seal is proven bad and pays the prover the tip, the march fee and a seal bond. It is allowed only after the reveal close + 1 bell.
    - This removes the "trade 50% of a host for information" option that §6.4 left open. Apply the same rule to postures: a bad posture seal becomes Disarray plus forfeiture of the posture tip and bond.
    - **Keep the FO check** (+9k CU). The review argued it is unnecessary because the commitment already authenticates the opening. But without FO the chain would call VALID seals that every stock tlock library rejects. Keepers could then not auto-reveal them, which rebuilds the withholding option.

### 4.4 Contention and C4 (§8.1, §8.6, §8.7, §10 C4)
14. **Set the account cap to 40% of the block limit** (40M today), and track it per season under R14. Add a per-season check that the deployment cluster enforces the cap.
15. **Rewrite the cost model:**
    - excluding k accounts costs p × min(k × 40M, 100M) CU per block;
    - the keeper's bid is net of the base fee;
    - use a 0.4 s slot time;
    - include the world-wide block-pricing case.
    Label every number [model].
16. **Restate C4:** excluding every reveal bid at the default tip for one bell must cost at least 10× the value of all clashes resolving in that bell for the targeted side. Drop "blast radius of one province" for the fee-market attack.
17. **Consider stronger defences:**
    - longer or owner-extendable reveal windows (the attacker's cost grows with window length);
    - keeper bids above the tip, funded by the operator or a faction pool;
    - letting owners reveal during the bell itself, which doubles the window an attacker must cover.

### 4.5 ResolveClash inputs and budgets (§6.3, §8.3, §8.9)
18. **Postures:** add a permissionless `GatherClash`. It writes one `ClashInputs` account per province-bell after the reveal close, and is idempotent and order-independent. Add a `ResolveFromInputs` that requires every position to be gathered [measured: 1–4 gathers plus a 391-B resolve]. Add these transactions to the lock and keeper analysis.
19. **Mirror garrison, walls and owner faction in the Province,** so ResolveClash never reads Holdings. With Holdings the transaction is 1,548 B, over the limit [measured].
20. **Budgets:**
    - ResolveClash about 270k CU (not 230k), which raises the §8.9 capacity tables by about 28%;
    - Reveal 17–26k;
    - Join 18k, SettleTicket 19k, FileTicket 4k;
    - ResolveClash is 31 keys / 1,152 B (the design left out Season, BellAnchor and ComputeBudget).
    All of these are SBPF v0 and must be re-measured under v2.
21. **Pin the Frontier program to SBPF v2** (v3 once platform-tools supports it) in `scripts/build-program.sh`, and keep arkworks field operations `#[inline(never)]`. Inlined, they produce 8–17 KiB stack frames, over the 4 KiB limit [measured].

### 4.6 Joining and settlement (§8.2, §8.3)
22. **SettleTicket displacement also rewrites the displaced citizen's Citizen record and JoinShard** (13 locks, 554 B). Key the Holding PDA by site (`[hold, id, P, Q, site]`). Define when the challenge window finalises MarchRoll counts.
23. **Add a permissionless `FoldOccupancy`** that sums the 48 JoinShards into Frontier. Join's 2% reserve check has no other data source that respects the no-global-counter rule. Size the reserve to cover the joins between folds.

### 4.7 Rules that the kernels now implement but the text does not state
24. **§6.3:**
    - an arrival lands no earlier than its departure bell + 2;
    - its troops and stamina are the host's values after the origin's clash of the departure bell;
    - Reveal ranking and the destination clash wait until the origin has resolved.
25. **§8.4:** resident actions need the province resolved through b − 2, not b − 1. Merges, splits, Depart stamina, garrison changes, walls and decrees issued during bell b take effect after that bell's clash.
26. **§6.1 and §6.3: hex fair share is allocated by side,** not by faction. On a holding's hex the owner's side keeps 3 slots, and the hostile sides share 3.
27. **§6.3 and §8.9:** define a quiet bell (`is_quiet`), meaning a bell a province may skip. A siege advances over a run of quiet bells in closed form, and only while its declarer holds the hex. Update the capacity model.
28. **§3.2:** carve a passable path from every gate and site to the province centre. Before this fix, 10.3% of provinces had gates in different components [measured]. **§3.6:** the 4-province cap limits a straight march to 20–28 hexes, not 32.
29. **§5.4 and D11:**
    - a late stake counts only laurels banked after it;
    - occupations and siege stakes follow the pair rule;
    - a fee-only wallet transfers no laurels;
    - the Works weight is linear and capped at 140 Works per USDC of fee, replacing the square root, which rewarded splitting play across wallets.
30. **Clash combat:** keep both halves of v9's `resolve_engagement`, including City and ranged retaliation at ×0.5, as §11.1 requires.

### 4.8 Open kernel items (small; fix before M1 gates)
- **Bounds:**
  - Walls, production and upkeep deltas have no cap: one large Walls item makes a holding immune to sieges.
  - `duplicate_cost` can overflow.
  - `ProvinceCoord` has no ring bound.
  - `clash::validate` accepts troops above `MAX_HOST_TROOPS`, an unbounded `dealt_bps` and stamina above the cap.
- **Scouts** count for field holding and siege progress although they cannot capture.
- **A vigil change can create one vigil of about 16 hours;** changes should take effect at a UTC midnight.
- `Stamina::set` can silently overwrite a later spend.
- Province caps are not recounted after the hex fair share.
- Faction ids are not limited to 0..=5 plus NEUTRAL.
- **Economy:**
  - there is no closed-form kernel for the Mandate reserve (and the simulator pays non-stakers, against §4.2);
  - some simulator conservation checks are identities rather than bounds;
  - the Ledger is global rather than per faction;
  - there is no `validate()` for the bps and γ parameters, and `permutation-rules` builds without overflow checks in release.
- The v9 `.so` hash changed by +1,416 B after the frontier module landed, though no Frontier symbol is linked (generic instantiations) [measured, K1]. The SBF build and the LiteSVM suite were not re-run after `cea89be`. v9 is stopped, so this matters only for hygiene.

---

## 5. Decisions for the owner

| # | Question | Recommended default | Alternatives and what they cost |
|---|---|---|---|
| **O1** | Which drand network does a season use (one network for both seeds and tlock)? | **quicknet**, with the program built for SBPF v2. Measured PostAnchor about 332k CU; on-chain seal opening measured on quicknet. | evmnet: 3.5–5.7× cheaper to verify, so an attack on the anchor is cheaper to fight off. But evmnet tlock exists only in unreleased Go code and a 0.0.1 JS port, and on-chain opening of evmnet seals is unmeasured (S-TLOCK says it needs SIMD-0650). |
| **O2** | Genesis co-seed now that S-VRF-LOCK is deferred | **drand only:** a round fixed by rule at least 10 minutes after CreateSeason. Drop MagicBlock VRF from the Frontier. | Keep the VRF co-seed and run S-VRF-LOCK in M1 (a non-escalating dependency before joins open; WP11 must be hardened fresh). |
| **O3** | Can bots win paid offices? The bot criterion depends on it. | **Bound officer pay** so an always-online wallet cannot profit from it (for example, cap each officer's pay at a share of what that officer paid, or pay it in laurels from the Mandate budget). | Publish the measured bot multiple instead. [sim]: bots in office return 1.52× at 2% and 1.22× at 5%. |
| **O4** | The M3 bot criterion (< 1.0×) fails at 1–2% bot share even without offices (1.07× / 1.05×) [sim] | **Try, in order:** a lower `WORKS_PER_USDC`; pricing a late stake by expected remaining accrual; order-weighted emission. Remove Relic Site laurels only if needed (no Relic Sites + order-weighted emission gives 0.98 / 0.96 / 0.93× at 2 / 5 / 10%). | Accept and publish that small bot shares earn slightly above 1.0×. |
| **O5** | Doctrines | **Keep asymmetric doctrines, but follow the tuned direction:** no direct multipliers on scored facts. Keep tuning until 6 of 6 are in the band, then gate in CI. | Symmetric factions (the D17 alternative). This closes the criterion immediately but loses faction flavour. |
| **O6** | γ (herding damping) | **0.6** until M3 measures β with real players | 0.3 (softer; covers real β up to about 1.2 [estimate]) |
| **O7** | New C4 wording and the defence budget | **Adopt the restated C4** (§4.4, item 16). Fund keeper bids above the tip from an operator or faction pool. Consider longer reveal windows. | Keep the per-province framing (it is wrong at a 40M cap). |
| **O8** | MagicBlock's role after the v9 stop | **Frontier on base only for Season 1.** No Skirmish or Arena, so no ER lane. X1 (a gated ER) stays a later extension. Rewrite onboarding without Skirmish as the tutorial. | Revive a small Skirmish later as a separate product. |
| **O9** | Capture chain and steward cut (K2 choices) | Confirm the kernel behaviour: a second capture pays 25% of the first transfer; an unused steward cut moves to the citizen pot. | 25% of the victim's banked laurels; the unused steward cut stays in the laurel pool. |
| **O10** | Mandate reserve | **Stakers who completed the Mandate only** (as §4.2 says). Needs a new closed-form kernel. | Every completer (as the simulator does now). |

Every devnet step, including the private M1 playtest, still needs the owner's separate approval.

---

## 6. Suggested path to close M0

1. Write design revision 3 with §4 above and the owner's answers to O1–O10.
2. Patch the spike probes with pre-funding-safe initialisation and a canonical anchor. Re-run S-SIZE R0/R2 and S-BEACON PostAnchor. Re-measure every CU row under SBPF v2.
3. Redo S-FEE with real contention and the anchor lock drill (D1–D6 in the S-BEACON spec). Produce a real C4 pass or fail against the restated criterion.
4. Add the quota-fairness kernel and property test, the seed-margin property test, and the §4.8 bounds.
5. Tune doctrines to 6 of 6 and add a reduced doctrine run to CI.
6. Push to a remote only with the owner's approval, so that GitHub CI runs the gates.

---

## 7. What was run for this report

This report was written from the spike results, the kernel and simulator summaries, the three reviews and the integration notes. For this report I read the design, the branch log (`cea89be` on top of `d089c15`), the test names in both frontier test files, `.github/workflows/ci.yml` and the simulator's RESULTS. I ran no new builds or tests. The measured figures come from:
- **Integration gate at `cea89be`** [measured]: `permutation-rules` `cargo test` debug and release, 338 passed each; `permutation-server` release, 88 passed (golden replays unchanged); `permutation-chain` host, 157 passed; clippy and fmt clean; `frontier-sim` 3 tests passed; `suite --agents 10000 --seeds 5`, 1,575 of 1,575 settlements passed in 642 s on 16 threads; seed-1 digest `c71a8a0eb7641b53` identical across threads; one 10k season takes 2.9 s and 35 MB.
- **Not re-run after `cea89be`:** the SBF build, the LiteSVM suite, and the spike re-runs.

## 8. Links

- Design: `(session scratch)/scratchpad/openworld/open-world-design.md`
- Simulator results: `.../scratchpad/frontier/m0/sim/RESULTS.md`, `suite-output-review.md`
- Spikes: `.../scratchpad/frontier/m0/spikes/S-BEACON/results/run-log.txt`, `S-FEE/REPORT.md`, `S-TLOCK/results/sbf-results.txt`, `S-SIZE-JOIN/results.txt`
- Code: `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-integ` (branch `codex/frontier`, `cea89be`); kernels in `permutation-rules/src/frontier/`, tests in `permutation-rules/tests/frontier_world.rs` and `frontier_economy.rs`, simulator in `frontier-sim/`
- Japanese version: `.../scratchpad/frontier/m0/M0-REPORT.ja.md`
