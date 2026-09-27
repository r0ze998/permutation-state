# PERMUTATION STATE at scale: Banners, Realms and the Commons

The single scale design for thousands to tens of thousands of players. It is synthesized from designs A to D and the three judges. **Revision 2** answers the stress test (15 issues). Every issue was checked against the code and re-measured; see **Revision notes** at the end.

- Synthesizer, 2026-09-27. Repo read-only at HEAD `96a3464` (branch `codex/magicblock-playable`). No commits, no servers, no devnet or mainnet transactions.
- Inputs:
  - designs `scale/design/{A-parallel-worlds, B-one-world-externalized-members, C-representation-tiers, D-free-hybrid}.md`;
  - judges `scale/judge/{limits-security, game-player, feasibility-deadline}.md`;
  - understanding docs `scale/understand/{rules-members, chain-members, game-intent, platform}.md`;
  - the security contract `fix/contract.md` (§0, §1, §1.8, §3, §4.3, §6) and `fix/contract-amendments.md`;
  - the stress test's labs `scale/lab/stress/{jury.py, txsize.mjs}`.
- Labs for this document:
  - `scale/lab/synth/synth.py` (revision 1: capacity, rent and the first jury model; its jury model is superseded, see §7.4);
  - `scale/lab/revise/capture2.py` → `capture2-results.txt` (revision 2: jury rules, recall thresholds, citizen-share economics);
  - `scale/lab/revise/txsize2.mjs` → `txsize2-results.txt` (revision 2: serialized sizes, legacy and v0 + lookup table);
  - `scale/lab/revise/rev2-numbers.py` → `rev2-numbers.txt` (revision 2: rent with 4 KiB Commons, time model, abort economics).
- Every number carries a label:
  - **[measured]**: from a lab run, with its source named;
  - **[derived]**: arithmetic on measured numbers;
  - **[estimate]**: code reading plus platform constants;
  - **[uncertain]**: needs a spike.

---

## 要約（日本語）

- **選んだ形**：**A（並行する世界）を骨組み**にします。その上に **D の「コモンズ」**（国民はベース層で遊ぶ）を載せ、社会の単位として **C の「セル」**（32 人の班）を加えます。B は採りません（誰でも無料でシーズンを止められる道と、選挙の結果を変えられる道があるため）。
- **世界（ER）に入る人数は、いつも最大 48 席**です。48 席の数字（1,136k CU / 191 KiB）は、**WP07 の試作で測ったもの**です。統合したビルドでの合同ゲート（§4.3）は wave 4 の仕事で、**まだ通っていません**。つまり、48 席の土台そのものも、まだ最終確認前です。
- **段階 1（10/12 まで。プログラム変更なし）**：
  - v9 の世界（48 席）を「評議会レルム」として、いくつか同時に動かします。6 つの国は、全レルム共通の 6 つの「**旗**」です。
  - **レルムはプレイヤーが自由に選びます**（前版の「乱数の振り分け」はやめました。直接参加で迂回でき、鍵を作り直せば狙ったレルムに入れるため、約束できないからです）。
  - 作るのは、レルム一覧、起動スクリプト、旗ランキング、負荷試験です。すべて新しいファイルです。
  - devnet で見せるのは 2〜3 レルムです。v9 が 10/7 に間に合わなければ、複数レルムはローカルだけで見せ、そう明記します。
  - **10/12 の時点では「数千人が遊べる」とは言いません。**言えるのは、測った数字だけです。
- **段階 2（10/12 の後、約 3〜4 週間）**：登録を締め切ってから VRF で振り分ける「コホート」です。受け取られていない賞金は、勝手に没収しません。
- **段階 3（約 8〜10 週間＋再監査）**：「**コモンズ・レルム**」です。1 つの地図に最大 2,400 人（1 国 400 人）が入ります。
  - 世界にいるのは 48 席の「卓」だけです。国民はベース層で、約 13 分ごと（60 秒ティックの場合）の「窓」ごとに出仕します。
  - 窓は**実時間**で区切ります。卓が遅れたときは、卓が追いつくまで窓の結果を待たせます。全シーズン分（18 窓）を保存するので、上書きで壊れることはありません。
  - 役職は、推薦の上位 3 人から、護民官 4 人の陪審が選びます。3 票以上が集まらなければ、その期は空席（代行）です。
- **正直な限界**：
  - ER では、**誰でも無料で書き込みロックをかけられます**。1 回のトランザクションで止まるのは「1 レルム」ではなく、**同じ ER 検証者にいるレルムの最大 32 個（ルックアップ表を使えば最大 64 個）**です。5 万人（21 レルム）なら、1 本のトランザクションで全員が止まります。
  - 止まったシーズンは中止になり、プレイヤーには全額が返ります。ただし運営は、20% の取り分（5 万人で 10 万 USDC）、保証金、SOL を失います。攻撃者は、自分の財布 1 つあたり参加費の 1.5〜4% を得ます。
  - このため、**専用の ER（または入口で選別する ER）を用意するまでは、デモ以外の大規模キャンペーンは開きません**（F13 が前提条件です）。
  - 投票率が低いとき、いつも活動する 1 割の集団は、かなりの確率で役職を取れます（投票率 30% で 1 期あたり 21〜27%）。「乗っ取りは割に合わない」とは、もう書きません。お金の面の上限だけを約束します。

---

## 0. Decision in ten lines

1. **Chosen: A's parallel worlds as the backbone, D's Commons as the scale tier, C's cells as the social unit.** B is rejected.
2. **The ER world never holds more than 48 seats.** 48 is the population the v9 round targets: WP03 quota 96/48 = 2; WP07 T-bound 76.7 of 77.6 KB; 1,136k CU and 191 KiB at 48 adversarial **[measured on the WP07 lab prototype, WP07 §7.7]**. **The joint capacity gate on the integrated build (§4.3, unit T, wave 4) has not run**; wave 2 has not started. So the 48-seat base is designed and prototyped, not yet gated. If the gate fails, C1's fallback (36/24/18) lowers the Table cap and every realm number below scales down with it.
3. **Horizontal scale comes from realms under six banners.** A realm is one Season and one ER world. The six nations are six global banners with fixed indices, names and lore.
4. **Phase 1 ships by 2026-10-12 with no program change**, on the v9 program if gate 4 is green on 10-07, else **local-only** on the HEAD program:
   - Council realms (v9 at 48 = 47 people + 1 AI);
   - **open realms**: players choose a realm and nation in that realm's existing lobby; a directory lists the realms (no randomized placement is claimed);
   - a multi-realm launcher; a banner leaderboard; a measured multi-realm soak and a local lock-grief spike (S1-local);
   - 2–3 realms on devnet, only on the v9 program.
5. **Phase 2 (about 3–4 weeks after the deadline) is A's on-chain cohort:** VRF deal after close, tickets, Admit, ReportWorld, AbortCohort, RefundTicket, CloseMember (after claim only), SweepDust (dust only). It is the first phase with placement nobody chooses, and it is the dealing machinery Phase 3 reuses.
6. **Phase 3 (about 8–10 weeks plus a re-audit, gated by spikes S1, S6, S7, S8 and by F13) is Commons realms:** a 48-seat Table on the ER and up to 2,400 citizens per realm on **base**, in their own Member PDAs, in cells of 32, and in per-nation Commons accounts.
   - Citizens check in with a labour pledge, vote for a cell voice once per term, endorse, petition for recall and sit on juries.
   - Four tribune seats per nation per term are drawn by VRF from cells, after every eligibility input is sealed.
   - Officers are chosen by a tribune jury from the top 3 endorsed, with a quorum; no quorum → caretaker.
   - Payouts are closed forms computed at claim time.
7. **Per-action and per-tick costs are constant in N:**
   - base: CheckIn 8.0k CU, Endorse ≤ 23.6k, claim math 5.4k, sortition 4.4k per try **[measured, D lab]**;
   - ER: ≤ 28.1M CU per realm per tick at the v9 gate ceiling **[derived]**.
   - At 50k people: 21 realms, ≈ 105 ER tx/s of **protocol-bounded load** (every game-legal transaction at its cap, 30 s ticks), and ≤ 230 base tx/s at 60 s ticks and 800 s windows (≈ 1.4 % of base capacity) **[derived]**. **Free spam on the ER is not bounded by the protocol at all** (§12.3); that is what F13 is for.
8. **Five rules keep the audit guarantees under adversarial load:**
   - **G1, citizens never write ER accounts.** Griefing their accounts becomes a base fee auction that the defender wins for micro-SOL.
   - **G2, barriers, not fallbacks, and no deadlock.** The barrier sits in CloseCommits, before revealing starts. No lock can produce "the incumbents continue" or a partial tally, and no ordering of permissionless calls can wedge a realm.
   - **G3, no journal or ledger can make a ResolveTick part fail.** Outgoing seat rows go to a fixed 8-row area in the Nation, emptied before the next change.
   - **G4, every record the ER consumes is reconciled on base.** One digest chain per nation, folded once per window when the window is *ready*, over exactly the bytes LoadCommons copies, seats included. A mismatch voids the season and refunds everyone.
   - **G5, storage never wraps.** Every per-window and per-term record has a slot for the whole season (18 windows, 6 terms), so a lagging ER can always catch up.
9. **Honest residual: a free ER write-lock freezes every realm that one transaction names.** A legacy transaction can write-lock 32 chunk-0 accounts (1,226 B **[measured]**), a v0 transaction with a lookup table up to the runtime's per-transaction lock limit (64 **[estimate]**). All realms on one ER validator can be frozen by one looping transaction. Players are refunded in full; **the operator loses its 20 % revenue, its escrow and its SOL** (§11.4). Only an ingress-filtered or dedicated ER (F13) removes it, so **F13 is a precondition for any campaign beyond the demo realms.**
10. **What the owner gets on 10-12:** a verifiable multi-realm game under six banners, with measured numbers and no claim of "thousands". The design reaches "thousands on one shared map" in Phase 3, behind F13, without re-opening any v9 proof except those listed in the delta (§14).

---

## 1. Why this architecture (weighing fatal flaws first)

| Design | Fatal or blocking flaw found by the judges | Verdict |
|---|---|---|
| B one world, externalized members | 1. A **free season freeze**: MeritLog overflow makes ResolveTick depend on ApplyMerit over lockable shards, so it fills in about 11 ticks. 2. **Free entrenchment**: an ElectionScan stall keeps the incumbents; 8 recall lanes can be locked in one transaction. 3. A **board-squatting bug**: `EVICT_FLOOR = 3` is a constant, so about 60 USDC silences a nation's board. 4. The officer pay cap is optional. 5. The ordinary member has nothing that matters. | **Rejected.** We keep only its closed-form claim (0 mismatches against `settle_with` **[measured, B lab]**), CloseMember/SweepDust, and its turnout-based recall idea. |
| C one world, cells, assembly | 1. **Integrity**: CloseElection and TallyRecall proceed "when every page is stamped or the deadline passed", so a free page lock excludes cells from a tally or keeps the incumbents. 2. One world is one free lock target for 50k. 3. 1,566 page undelegations can be stalled to void the season. 4. DelegateAct writes the critical-path NationAccount. 5. Nothing ships by 10-12. | **Not the backbone.** We keep its best game ideas (cells of 32, the member → voice → seat ladder, team stake), its "journal never fails a tick" rule, and eviction by weight. We move all of it to base (G1). |
| D Table + Commons | No proven freeze or fund-lock path. Gaps: 1. S6 (read-only base clones on the ER) is unverified. 2. The ledger row bound says 60 while 72 is possible. 3. Phase 11 credit voiding is unmeasured. 4. Low-turnout capture pays without a jury. 5. The Phase 1 gateway work collides with G1/W files. | **Adopted as the scale tier.** Revision 2 also restores the property D's beacon gave (windows cannot outrun the Table) with a barrier instead of a beacon (§6.1), and keeps seats inside the reconciled digest (G4). |
| A parallel worlds | No fatal flaw. 1. Capital and ER load are linear in players (817 SOL locked at 50k). 2. Phase 0 placement is trusted to the operator, and (revision 2) it is also steerable by users through each realm's public join. 3. Thousands never share one map. | **Backbone and Phase 1–2.** Phase 1 drops the placement claim (open realms); the VRF deal in Phase 2 is the first placement nobody chooses. Phase 3 answers "one shared map" and cuts capital about 6×. |

What the judges agreed on, and how each point is applied here:
- **All three:**
  - ship v9 at 48 by 10-12;
  - lift the pause on WP03/06/07/10 now;
  - A's shape is the one that ships;
  - long ticks are needed for mass cohorts;
  - a CDN state cache and header polling;
  - B's and C's measured tables go into SUBMISSION as prototype evidence.
- **Security judge.** Run S1 first (revision 2: S1-local runs before 10-12, in X4). Never let a non-critical lock trigger an incumbents fallback. Put citizen writes on base.
- **Game judge.** A alone repackages the rejected 48 cap, so the plan must reach a shared map with a small social unit (cells) and a per-window action that moves the map (mobilisation from day one of Phase 3). It also asks for:
  - sortition with a jury;
  - AI that scales with cells;
  - a K = 1 small-N mode (here, the Council preset).
- **Feasibility judge.**
  - Build D's goal the A way: separate processes, new files, no edits to G1/W files before wave 4 merges. Revision 2 honours this by **not** building a placement router, because closing its bypass would require editing G1's `routes/x402.mjs`.
  - One play server per realm.
  - A paid or dedicated RPC.
  - Cut line on 10-07.

---

## 2. Architecture

```
                         ┌──────────────────────────── BASE (Solana) ─────────────────────────────┐
 Campaign / Cohort       │ Phase 1: off-chain realm directory (open realms, no placement claim)    │
 (six banners)           │ Phase 2: Cohort, CohortBanner×6·lanes, banner vaults, Ticket (A §4.2)    │
                         │          JoinCohort → CloseCohort(VRF) → SpawnWorld×R → Admit → Report   │
                         ├──────────────────────────────────────────────────────────────────────────┤
 Realm r (one Season)    │ Season (4 KiB) · vault · Roster · Member PDA per person (320 B)          │
                         │ Phase 3 Commons (not delegated): Commons×6 (4 KiB) · Cell×(6·cells)      │
                         │   (256 B) · CitizenIndex pages (4 KiB/127) · StewardTable×6 (1 KiB)      │
                         │   citizens: CheckIn · CellPick · Stand · Endorse · JuryVote · Recall     │
                         │   anyone: SealCells · SealWindow · DrawTerm(VRF) · SeatTerm · Settle…    │
                         └───────────────┬───────────────────────────────────────▲─────────────────┘
                       delegate 26 (+6)  │                  read-only clone of   │ commit / undelegate
                                         ▼                  READY Commons (S6)   │
                         ┌───────────────────────────── ER (MagicBlock) ──────────────────────────┐
 The Table (≤ 48 seats)  │ 20 world chunks · 6 Nation · (Phase 3: 6 TermLedger)                    │
                         │ v9 tick loop: CloseCommits(barrier G2) → reveal → FreezeTick → Resolve  │
                         │ Phase 3: LoadCommons(window j) before CloseCommits of tick a_j          │
                         │          ExportLedger(civ) empties the Nation's outgoing-seat area      │
                         └────────────────────────────────────────────────────────────────────────┘
 Off-chain: realm directory · launcher/crank fleet · relays (operator's and anyone's) · play-server
            state cache per (realm,nation,tick) · verifier per realm + verify-cohort + verify-commons
            · talk per cell/nation/realm/banner
```

Design rules. Each is a checkable property.

- **R1. The world's population is at most 48 seats, forever.**
  - Council realms (Phases 1–3): seats = members.
  - Commons realms (Phase 3): seats = 4 officers + 4 tribunes per nation.
  - `SEASON_MEMBER_CAP` and `SEAT_CAP` stay 48, with the C1 fallback (36/24/18) if the gate fails.
- **R2. No account is shared by two realms, and no instruction spans realms.** This does **not** isolate realms from the ER lock residual: a transaction that is not ours can name accounts of many realms at once (§12.3). Isolation against that comes only from F13 or from spreading realms across validators (O8 pin).
- **R3. Meta and citizen instructions do O(1) work per ticket, citizen or cell, and O(1) per realm.**
  - The only per-realm vector is Phase 2's `reported` bitmap (≤ 512 B).
  - The only per-nation vectors on base are bounded: ≤ 24 cells, 18 window records, 6 term seat tables, ≤ 72 ledger rows, top-3 lists.
- **R4. Assignment to a realm or a cell never claims more randomness than it has.**
  - Phase 1: players choose their realm and nation (open realms). Nothing random is claimed.
  - Phases 2–3: the VRF after registration closes; every input to a draw is sealed before the draw's VRF request (the WP11 pattern).
- **R5. Money stays per realm.** WP12 applies per vault. Phase 2's optional banner levy is a separate, separately proven path; it is off by default.
- **G1. Citizens never write an ER account.** Every per-citizen write lands on base, in the citizen's own Member, their own Cell (bound on chain, §4.3) and their nation's Commons. The ER sees citizens only as sealed, fixed-size records and as ≤ 48 seats.
- **G2. Barriers, not fallbacks, and no deadlock.**
  - At the apply tick `a_j` of window j, **CloseCommits** refuses with `SnapshotPending` until `loaded_window ≥ j`. LoadCommons is allowed exactly while the tick is in its commit phase (`!frozen && !revealing`), so whichever of the two lands first, the other can still land: there is no state in which both are refused. FreezeTick repeats the check as defence in depth.
  - No code path turns a missing tally or missing seats into "the incumbents continue" or a partial tally. SeatTerm refuses (never vacates) when the caller omits an account a draw needs.
  - The verifier fails any season that shows a skipped window.
- **G3. No journal or ledger can make a ResolveTick part fail.**
  - Outgoing seats go to a fixed area of 8 rows in the Nation (≤ 8 seats change at once). LoadCommons refuses to load a window with seat changes while that area is non-empty (`PendingExport`), and ExportLedger moves the rows to the TermLedger outside the tick. So the area can never overflow.
  - TermLedger rows are bounded by construction: 8 seats × 6 terms + ≤ 1 recall per office per term = 72 rows (recalls in revision 2 vacate, so this is an upper bound), and the account is sized for 72.
- **G4. Base is authoritative for citizen data; the ER's copy is reconciled.**
  - A window is **ready** when it is sealed and, if it is a seat window (3m − 1), when the 9th SeatTerm of term m has written the term's seats (`seats_ready`).
  - At the moment a window becomes ready, base folds `Commons.snap_digest = sha256(prev ‖ record_bytes(j))`, where `record_bytes(j)` is exactly what LoadCommons copies: the aggregates, the recall vacancies and, at seat windows, the 8 seats of term m.
  - LoadCommons folds the same bytes into `Nation.snap_digest`. The last ResolveTick copies the six Nation digests into the world's chunk-0 header.
  - FinishSeason compares the six chunk-0 copies with the six Commons digests over all 18 windows. A mismatch leads to `WorldRolledBack` and then the WP14 refund path: void, never mis-pay.
- **G5. Storage never wraps.** Commons and Cell keep one record per window for the whole season (18) and one seat table per term (6). CheckIn writes the record of the current window index directly; nothing is indexed modulo a ring. A Table that lags by any number of windows can still load every one of them.

---

## 3. Realm and population sizes

| Quantity | Phase 1–2 Council realm | Phase 3 Commons realm |
|---|---|---|
| Seats in the ER world | ≤ 48 (47 people + 1 AI by default) | 48 = 6 × (4 officers + 4 tribunes) |
| People per realm | ≤ 47 | `realm_cap` default **2,400** (400 per nation); hard maximum 4,608 (24 cells × 32 × 6, from the Commons layout) |
| Cells per nation | – | ⌈citizens_n / 32⌉ ≤ 24 (13 at the default) |
| Realms at 1k / 10k / 50k people | **22 / 213 / 1,064** [derived] | **1 / 5 / 21** [derived] |
| Small seasons | – | Below about 200 people a realm runs as a Council realm (everyone seated). This is the "K = 1" mode the game judge asked for, and it keeps V5 exactly. |

Why 2,400 per Commons realm is the default (owner decision F7; alternatives 1,000 and 4,600):
- **Game.** A nation of 400 has 13 cells. The 24 tribune-terms per season give about a 6 % direct chance per citizen, and about 31 % per term for a cell's voice (§8). One map is one shared story.
- **Security (revision 2).** Realm size does **not** bound the blast radius of the ER write-lock: one transaction reaches every realm on the validator (§12.3). The size is chosen for the game and for base-side limits only.
- **Capture.** Bigger nations do not change capture odds much at a fixed bloc share (§7.4: 400 and 833 per nation give the same numbers within noise).

---

## 4. Accounts and sizes

### 4.1 Unchanged per realm (v9 contract §1.4)

| Account | Size | Count | Note |
|---|---|---|---|
| Season `PSSEASN8` | 4,096 | 1 | Phase 2 appends block 8, Phase 3 appends block 9 (§4.3). Both are ≤ 400 B, inside 4 KiB. |
| World chunks `PSWORLD6` | 20 × 4,096 | 20 | T-bound holds at 48 seats on the WP07 prototype. Phase 3 adds 60 B of mobilisation and 192 B of chunk-0 digest copies (§4.3); both must be re-asserted in CT(a). |
| Nation `PSNATN08` | 8,192 | 6 | Phase 3 appends ≈ 640 B: seat table 8 × `{cit u32, key [u8;32], cell u16}` = 304 B, outgoing-row area 8 × 26 B = 208 B, `loaded_window u16`, `snap_digest [u8;32]`, `pending` (the loaded record awaiting its apply tick, ≤ 80 B), spare. Whether the v9 Nation body leaves this much room is **[estimate; checked in CT(a)]**. |
| Vault | 165 | 1 | – |
| Roster `PSROSTR2` | 21 + 46·a | 1 | a = AI count (1 in Council, ≤ 64 in Commons) |
| Member `PSMEMBR6` | 320 | 1 per person | v9 record ≤ 206 B. Phase 2 tail: `rent_payer [u8;32]`. Phase 3 tail: 45 B (§4.3). Total ≤ 283 B, so no realloc and no re-registration. |

Rent per Council realm is 0.707 SOL of accounts plus 0.061 SOL of delegation deposits, so **0.768 SOL locked**. 48 Member PDAs cost another 0.109 SOL, never reclaimed in v9. All figures at 5,080 lamports/B; multiply by 1.37 at 6,960 **[derived, synth]**.

### 4.2 Phase 2 (cohort; A §4.2, adopted verbatim unless noted)

| Account | Seeds | Size | Count |
|---|---|---|---|
| Cohort `PSCOHRT1` | `["cohort", cohort_id]` | 2,048 | 1 |
| CohortBanner `PSCOBNR1` | `["cobanner", cohort_id, b, lane]` | 64 | 6 × S (S ≤ 8 lanes) |
| Banner vault | ATA of the CohortBanner | 165 | 6 × S |
| Ticket `PSTICKT1` | `["ticket", cohort_id, wallet]` | 160 | 1 per player, closed at Admit |

- **Change from A:** the Cohort gains `mode u8` (Council or Commons) and `realm_cap u32`. W becomes `R = max(⌈(N + A)/realm_cap⌉, max_b ⌈N_b/nation_cap⌉, 1)`. The same Feistel deal applies.
- **Commons mode:** Admit also assigns `cit_index` (the ticket's position in its realm-nation), writes `Member.cell` (§8.1) and appends to the CitizenIndex.
- **Season id:** `2^63 | cohort_id << 16 | r` (A §4.2). Phase 1 keeps the gateway's own ids (`season.mjs:207`, `BigInt(now())`); the launcher staggers starts so two realms never share a millisecond (§9.1).

### 4.3 Phase 3 (Commons)

**Base, not delegated:**

| Account | Seeds | Size | Written by | Fields |
|---|---|---|---|---|
| Season block 9 | – | ≈ 320 B | OpenGovernment, ConsumeTermSeed, FinishSeason, Claim | `realm_cap u32`, `citizens [u32;6]`, `gov_open_at i64`, `window_seconds u32` (= 10 · T_eff, §6.1), `term_seed [u8;32]`, `term_seed_state`, and per nation `{counted, rate u64, units_people u64, steward_rows u16}` × 6, `claimed_count u32` |
| Member tail | same PDA | 45 B | its citizen (and candidates' `tally`) | `cell u16` (**bound at Admit/RegisterCitizen, never changes**), `windows u32` (checked-in window bits), `pledge u8`, `pick_term u8`, `endorsed u32` (term × office bits), `stand u8`, `stand_term u8`, `tally_term u8`, `tally [u32;4]`, `recalled u32`, `seated u8` (term bits), `cit_index u32`, `jury u16` |
| Commons | `["commons", season, civ]` | **4,096** zero-copy | CheckIn is **not** a writer; Endorse, Stand, RecallPetition, JuryVote, SealCells, SealWindow, SeatTerm | `citizens`, `cells`, `units_total`; **`win[18]`** of `{sealed u8, ready u8, cells_done u32, checkins u32, pledges [u32;4], term_active u32, recall [u16;4], vacate u8}` (≈ 40 B × 18 = 720 B); `top3[4]` of `{cit, tally, tiekey u64}`; `jury[4][3] u8`, `jury_voted u8` (seat bits, current term); `candidates [u32;4]`; **`seats[6][8]`** of `{cit u32, key [u8;32], cell u16}` (1,824 B), `seats_ready u8` (term bits), `seat_try [8] u8` (resumable cursor); `snap_digest [u8;32]` |
| Cell | `["cell", season, civ, cell]` | 256 | CheckIn, CellPick, SealCells (voice), SettleStewards (pot) | `size u8`, **`win[18]` of `{checkins u8, pledges [u8;4]}`** (90 B), `term_active [u8;6]`, `picks [u8;32]` + `picks_term u8` (picks of the current term's endorse window only), `voice {slot u8, term u8}`, `cell_pot u64`, `digest [u8;32]` (≈ 180 B used) |
| CitizenIndex page | `["citidx", season, civ, page]` | 4,096 = 32 + 127 × 32 | Register / Admit (append) | wallet per ordinal, so a draw maps an ordinal to a Member in O(1) |
| StewardTable | `["steward", season, civ]` | 1,024 | SettleStewards (once) | ≤ 72 rows × 12 B `(cit_index u32, amount u64)`, sorted |

**ER, delegated:**

| Account | Size | Written by | Note |
|---|---|---|---|
| TermLedger ×6 | 2,048 (72 × 26 B + 32 B header = 1,904 B, within the 2.5 KB dense-commit limit) | **ExportLedger{civ}** (revision 2; not ResolveTick) | Row `{cit u32, seat u8, from_tick u8, merit [u32;5]}`. It takes bits 26–31 of `Season.delegated: u32`: 20 + 6 + 6 = 32 targets exactly (D §4.2). |

**World (rules v10, Commons mode only):**
- `members` = the 48 seats (unchanged type), plus `mob [u16;4]` and `commons_window u16` per nation: 60 B in total.
- Chunk-0 header: the six Nation `snap_digest` copies (192 B), written by the ResolveTick of the last tick (G4).
- `ballots` stays empty; the election is on base.
- The world holds **no citizen ids**. Seat → citizen lives in the Nation seat table and the TermLedger.

Rent per Commons realm at the default of 2,400 **[derived, rev2-numbers.py]**:
- Table: 0.853 SOL, including 32 delegation deposits, 6 ledgers and a 20-AI roster.
- Base Commons (now 4 KiB), cells, index pages and steward tables: 0.83 SOL.
- **Float: about 1.7 SOL per realm.**
- Member rent: 0.00228 SOL per citizen, reclaimable by CloseMember after the claim.

---

## 5. Instructions

### 5.1 Phase 1 (by 10-12): none

- Every realm is an ordinary season of whichever program is deployed (v9 if gate 4 is green; otherwise the HEAD program, local only).
- **Revision 2.** Revision 1 said O14 makes a placement router the only entrance. That is false: every realm gateway serves `POST /x402/join` on its public listener (`app.mjs:42-46` PUBLIC_ROUTES, `routes/x402.mjs:139`), each realm's play server proxies that listener under `/gw` (`play/mod.rs:163-174`, `play/proxy.rs`), and the gateway co-signs any valid Register for its own season, so `fee_payer ∈ {admin, crank}` passes. At HEAD there is no O14 at all. Phase 1 therefore makes no placement claim: **players pick a realm in the directory and a nation in that realm's own lobby**, as the per-realm lobby already allows. The per-realm, per-nation cap is the chain's (24 in v9).

### 5.2 Phase 2 tags 37–51 (append-only after v9's 36; errors 45–48)

A §5.2 is adopted, with these changes:

| Tag | Instruction | Change from A |
|---|---|---|
| 37 | CreateCohort | + `mode`, `realm_cap`; AI tickets ≤ 1 per realm on average (§10) |
| 38 | AllocCohortBanner | – |
| 39 | JoinCohort {banner, lane, name, kind, stand} | WP17 checks moved here verbatim (the session signs; `session == payer` is refused; the wallet owns the source). The player pays **before** any placement exists, so there is nothing to grind. |
| 40 | CloseCohort | `R` as in §4.2; VRF request last, with `caller_seed = sha256(cohort_id ‖ counts ‖ ai_tickets ‖ rules_hash)` |
| 41 / 42 | ConsumeCohortSeed / RetryCohortSeed | the WP11 season-seed pattern (O3) |
| 43 | SpawnWorld {r} | pins `rules_hash` (I-C5); validator = `validators[r mod E]` (O8), **with at most `K_v` realms per validator** (F13 interim, §12.3); bond pool sized for the worst deal: `bond_floor(max AI per realm)` × R |
| 44 | Admit {r, n ≤ 6} | + Commons mode: `cit_index`, `Member.cell`, CitizenIndex append, cell counters (cell = Feistel after close, §8.1) |
| 45 | ReportWorld {r} | – |
| 46 / 47 | AbortCohort / RefundTicket | – |
| 48 | CloseMember | **revision 2:** only after the member has claimed (or, if it never can, after its refund), or when the member's own wallet signs. Lamports go to `Member.rent_payer`. It never destroys an unclaimed claim record. |
| 49 | RevealCohortRoster | A's WP09 row (batches of 8 against the cohort commitment) |
| 50 | **SweepDust** | **revision 2:** moves only `vault − outstanding` (rounding dust) to operations, any time after FinishSeason. It never touches `outstanding`, so every unclaimed prize stays owed (WP12). |
| 51 | ForfeitUnclaimed (optional, F11b, **off by default**) | Only if the owner opts in: after a claim deadline disclosed in the lobby and V5 §7, moves `outstanding` to the named destination (default: the next season's pool of the same campaign, never operations) and logs every forfeited member. Its own conservation test. |

Errors: 45 `CohortPhase`, 46 `NotDealt`, 47 `TicketState`, 48 `NotReclaimable`.

Changed v9 instructions:
- Register: I-C4, so no direct join into a cohort season.
- FinishSeason: writes `nation_points`.
- Claim: dispatches on `Season.mode` (0 = v9 table, 1 = Commons closed form).

### 5.3 Phase 3 tags 52–69 (errors 49–57)

Sizes in the last column are serialized transaction sizes **[measured, lab/revise/txsize2]** unless marked.

| Tag | Instruction | Layer | Accounts (s = signer, w = writable) | Checks and effects | CU / heap / tx |
|---|---|---|---|---|---|
| 52 | **CheckIn {window, pledge}** | base | session (s), member (w), cell (w), season (r) | `session == member.session`; **cell PDA == `["cell", season, civ, member.cell]`** (else `WrongCell`); `window == ⌊(now − gov_open_at)/window_seconds⌋ ≤ 17` (else `WindowClosed`); bit unset; set bit; `cell.win[window].checkins += 1`, pledge counter; first check-in of the term → `cell.term_active[term] += 1`; log `PS_CIT`. **Any fee payer** may pay (§8.4). | **8.0k [measured, D]** + cell ≈ 1–2k → **≤ 12k [estimate]**; heap 392 B; ≈ 422 B tx [estimate] |
| 53 | **CellPick {slot}** | base | session (s), member (w), cell (w) | only in the endorse window of the term; the picker checked in during that window; one pick per term; cell PDA bound as in CheckIn; `slot < size`; resets `picks` on the first pick of a new term | ≈ 7k [estimate] |
| 54 | **Stand {offices}** | base | session (s), member (w), commons (w) | checked in during this or the last window; `stand_term = term + 1` | ≈ 6k [estimate] |
| 55 | **Endorse {mask}** | base | session (s), endorser (w), commons (w), 1–4 candidate members (w) | only in the endorse window of the term (§6.2); endorser checked in this window; one bit per (term, office); candidate standing, same season and nation; `tally += 1`; incremental top-3 by `(tally, tiekey)`, with eviction **only by a higher key**; `tiekey = sha256(season_seed ‖ term ‖ office ‖ cit_index)` | **≤ 23.6k (4 offices, top-8 worst case) [measured, D]**; 522 B |
| 56 | **JuryVote {office, pick ∈ 0..2}** | base | session (s), member (w), commons (w) | only in the jury window; caller ∈ the current term's seats as a tribune (full 32-B key match); one bit per office; `jury[o][pick] += 1`; sets the juror's bit in `jury_voted` | ≈ 8k [estimate] |
| 57 | **RecallPetition {office}** | base | session (s), member (w), commons (w) | checked in during the current window; one bit per (term, office); `win[j].recall[o] += 1` | ≈ 7k [estimate] |
| 58 | **SealCells {civ, window, from}** | base | commons (w), season (r), ≤ 12 cells (r; w at endorse windows for the voice) | permissionless; `now ≥ window_end(window)`; adds each cell's `win[window]` and `term_active` into `Commons.win[window]` once (bit in `cells_done`); at endorse windows sets each cell's voice = top pick if its picks > ½ × the cell's check-ins in that window, else none | ≈ 5k + 3k per cell ≤ 45k [estimate]; **767 B** (12 cells + 4 members), so a 24-cell nation needs 2 calls |
| 59 | **SealWindow {civ, window}** | base | commons (w), season (r) | permissionless; all cells counted (`cells_done` full); sets `sealed`. At endorse windows freezes top-3. At the recall window (the term's second window, §7.2) evaluates recalls: office o is **vacated** (caretaker) if `Σ recall[o]` over the term's first two windows > ½ × the term's distinct active citizens so far **and** ≥ citizens_n / 8, and the office was not already vacated this term. If the window is not a seat window, sets `ready` and folds `snap_digest` (G4); log `PS_SEAL` | ≈ 20k [estimate]; < 300 B |
| 60 | **DrawTerm {term}** | base | payer (s,w), season (w), 6 commons (r), identity PDA, VRF_QUEUE_BASE (w), VRF program, SlotHashes | **revision 2:** all six **jury** windows (3m − 1) sealed, so every eligibility input (cell voices sealed at 3m − 2, check-ins of 3m − 1) is fixed before the request; `caller_seed = sha256(season ‖ term ‖ 6 Commons digests ‖ 6 win[3m−1] records)`; VRF request last (WP11 pattern) | 40k + CPI |
| 61 / 62 | ConsumeTermSeed / RetryTermSeed | base | VRF identity (s), season (w) | WP11 identity check (`WrongOracle`); retry every 60 s; **no fallback** (O3 policy): after 1 day the realm is abortable | 5k / 40k |
| 63 | **SeatTerm {civ, slot 0..8}** | base | commons (w), season (r), slot < 4: officer-elect member (r); slot ≥ 4: the one cell of the current try + ≤ 4 index pages + ≤ 4 members (r) | the jury window is sealed and the term seed is present. **Officers (rule J4, §7.4):** quorum ≥ 3 jury votes cast; strict plurality of cast votes; ties → lottery among the tied by `H(seed ‖ civ ‖ term ‖ office)`; quorum not met → vacant (caretaker). **Tribunes:** one cell try per call, cursor in `seat_try[slot]`: `c = H(seed ‖ civ ‖ term ‖ slot ‖ try) mod cells`; cell quorum ≥ 3 check-ins in the jury window; seat = the cell voice if eligible, else a lottery member of that cell via the inverse Feistel, 4 member tries; eligible = checked in during the jury window, not an officer-elect, not a tribune last term. After 4 cell tries the seat is vacant for the term (logged). **If an account needed by the current try is missing → `TryAccountMissing`; the cursor does not move and nothing is vacated.** The 9th finished slot sets `seats_ready[m]`, marks window 3m − 1 `ready` and folds `record_bytes(3m − 1)` (aggregates + the 8 seats) into `snap_digest` | ≈ 30k + **4.4k per try [measured, D sortition]**; **535 B** per call (legacy); revision 1's 4 × 4 tries in one call measured **1,426 B (over)** |
| 64 | **LoadCommons {window}** | ER | chunk 0 (r), 6 nation (w), 6 commons (read-only base clones), sysvars | `!frozen && !revealing` (WP01); **alone** rule; `window == loaded_window + 1` for every nation; every clone has `win[window].ready`; **if the record vacates or changes seats, every nation's outgoing area must be empty (`PendingExport`)**; copies `record_bytes(window)` into `Nation.pending`, sets `loaded_window`, folds `snap_digest` (G4); log `PS_COMMONS_IN` | ≈ 40–60k [estimate] |
| 65 | **ExportLedger {civ}** | ER | nation (w), term ledger (w), chunk 0 (r) | alone rule; moves the Nation's outgoing rows to the TermLedger and clears the area; permissionless | ≈ 20k [estimate]; < 400 B |
| 66 | **SettleStewards {civ}** | base | season (w), 6 TermLedgers (base copies, after undelegation), steward (w), roster (r), cells (w) | after FinishSeason's world part; merit split of the steward pot over ≤ 72 rows with the per-person cap; **a tribune row whose seat has no `jury_voted` bit for its term gets 0**; tribune rows share τ of their pay with their cell (`cell_pot`); **AI rows are voided at reveal and their amount joins the people pot** (§11.3); overflow to the people pot | **2.1k per row [measured, D]** → **≈ 0.155M at 72 rows [derived]** |
| 67 | CloseCommons | base | commons, cells, index, steward, rent receiver | after every claim or refund is paid, or after ForfeitUnclaimed if F11b is on (extends CloseSeasonAccounts) | small |
| 68 | RegisterCitizen | base | Register + commons (w) + index page (w) + cell (w) | Commons mode without a cohort (devnet or small campaigns); all WP17 checks; writes `Member.cell`; `+2.7k [measured, D IndexAppend]` | v9 Register + 3–5k |
| 69 | ApplySeatsGenesis {civ} | base | season, nation (w, before delegation), commons | term-0 tribunes drawn at seating from the season seed (cells, lottery member); officers vacant for term 0 (the V5 caretaker) | ≈ 30k + tries |

Errors: 49 `WindowClosed`, 50 `AlreadyActed`, 51 `NotSealed` (also "not ready"), 52 `SnapshotPending` (the G2 barrier), 53 `LedgerFull` (unreachable by construction), 54 `NotJuror`, 55 `WrongCell`, 56 `TryAccountMissing`, 57 `PendingExport`.

Changed v9 instructions, **Commons mode only**; Council mode stays byte-for-byte v9:
- **CloseCommits:** the G2 barrier. If the open tick is `a_j` for some window j (§6.1), it requires `loaded_window ≥ j` for every nation, else `SnapshotPending`.
- **FreezeTick:** repeats the check (defence in depth; unreachable once CloseCommits passed).
- **SubmitGov:** the roll = the 8 seat keys (full key in the seat table; key8 fingerprint in the roll as in v9). Refused during the apply tick of a seat window.
- **ResolveTick phase 11:**
  - apply `Nation.pending`: mobilisation shares; seat changes and vacancies. Each outgoing seat's merit row goes to the Nation's outgoing area (never to the TermLedger, so the ResolveTick account list stays v9's). **Void pending `Credit`s of changed seats**, a bounded scan over cities, civs, envoy shares and proposals; update the roll;
  - at the last tick, copy the six Nation digests into the chunk-0 header.
- **FinishSeason:** reads the season, 20 chunks, vault, roster and the 6 Commons (**1,127 B [measured]**; with the 6 Nations as revision 1 implied it was 1,325 B, over the limit); checks the G4 digests over 18 windows; writes the per-nation scalars.
- **Claim:** the closed form (§11.3).
- **RevealRoster:** also records each AI's units, so people-only aggregates are exact.
- **Abort:** refunds per citizen at claim, O(1).

**Transaction sizes, revision 2** **[measured, txsize2-results.txt]**: ResolveTick with v9's accounts, the CU-limit and heap-frame instructions = 1,145 B; with 6 TermLedgers it would be 1,343 B, which is why ExportLedger exists. A v0 transaction with an address lookup table brings every instruction here to 230–325 B, but whether the ER accepts v0 + ALT is **[uncertain, spike S6b]**; nothing above depends on it.

---

## 6. Time: windows, terms and the tick of application

### 6.1 Wall-clock windows anchored on base, paced against the Table

**What revision 1 got wrong.** At HEAD a tick is not T seconds long. ResolveTick sets the next commit deadline to `now + tick_seconds` (`processor/play.rs:402`), CloseCommits then opens a reveal window of `reveal_seconds(T) = max(2, T/6)` (`play.rs:174-178`, `state.rs:386`), and the VRF callback and about 13 crank transactions follow. An honest tick therefore takes about 1.2–1.25 T. Windows of exactly 10 T drift ahead of the Table by about 2 ticks per window; with rings of 4, slot j was overwritten around window 16–17, and the last 30–40 minutes of play had no window at all.

**Revision 2:**
- **Window length.** `window_seconds = 10 · T_eff` with `T_eff = T + reveal_seconds(T) + slack`, `slack` = 10 s by default, re-set per preset from the X4 soak's p50 tick time **[estimate until measured]**. At T = 60 s: T_eff = 80 s, a window is 800 s (≈ 13 min) and a season of 18 windows is 4.0 h. At T = 30 s: 45 s, 450 s, 2.25 h **[derived, rev2-numbers]**.
- **Window j** = [`gov_open_at + j · window_seconds`, `gov_open_at + (j + 1) · window_seconds`), j = 0 … 17.
- **Apply tick** `a_j = min(10(j + 1) + 1, 179)`: windows 0 … 16 apply at ticks 11, 21, …, 171, and **window 17 applies at tick 179**, the last tick (ticks run 0 … 179). Every window is loaded, so both digest chains cover exactly 18 records.
- **If the Table runs ahead** (ticks faster than T_eff), CloseCommits of `a_j` waits for window j to become ready and be loaded (G2). The Table can never outrun the windows, which is the property D's beacon provided.
- **If the Table runs behind** (drift, a degraded step, VRF retries, an ER outage or a lock), windows keep closing on base and wait in their own records (G5). LoadCommons reads by index, never modulo, so any lag is recoverable; the season simply ends later than 18 windows. **"If the ER lags, windows queue on base" is now true.**
- **What a player sees.** The window clock is wall-clock; the Table's tick is shown beside it with "your pledge applies at tick a_j". In an honest season the two stay within about one tick of each other because the barrier paces the Table. Under a long stall they diverge and the UI says so.
- **Why the barrier cannot deadlock (G2).** At `a_j` the tick is in its commit phase. LoadCommons is allowed there (`!frozen && !revealing`), CloseCommits is refused until it lands. Anyone may send either. No interleaving of permissionless calls moves the tick to revealing without the snapshot, so the revision-1 wedge (CloseCommits first → LoadCommons refused as revealing, FreezeTick refused as pending) cannot occur.
- **Clock skew** between base and the ER is a few seconds, against T ≥ 30 s.

### 6.2 The term calendar (18 windows, 6 terms, 180 ticks)

| Term m | Seats applied (phase 11 of) | New officers first commit | Endorse window | Jury window (jurors = tribunes of term m−1) | DrawTerm after |
|---|---|---|---|---|---|
| 0 | tribunes at seating (ApplySeatsGenesis); officers vacant | – | – | – | – |
| 1…5 | tick 30m + 1 (`a_{3m−1}`) | tick 30m + 2 | 3m − 2 | 3m − 1 | SealWindow{3m − 1} ×6 |

- Officers serve ticks 30m + 2 … 30(m + 1) + 1: one tick later than revision 1 and two later than V5's tick-30k elections (V5 §5.3). A documented change.
- Term 0 has no officers (caretaker), because no jury exists before the first tribunes have served. v9 AI seasons already start with vacant offices (O22).
- The draw for term m happens only after the jury window is sealed. The VRF round trip, 9 SeatTerm calls per nation (up to 4 per tribune slot) and 6 LoadCommons fit in the ≈ 1 T_eff between the end of window 3m − 1 and CloseCommits of tick 30m + 1 **[estimate; measured in CT]**; if not, the barrier waits.

### 6.3 Snapshot flow per window (all logged, all replayable)

1. Citizens send CheckIn, RecallPetition (and CellPick, Stand, Endorse or JuryVote in their windows) as base transactions, through any relay (§8.4).
2. After the window's end, anyone sends `SealCells{civ, j, from}` (1–2 per nation) and `SealWindow{civ, j}` ×6.
3. At a seat window 3m − 1: anyone sends DrawTerm, waits for the callback, then `SeatTerm{civ, slot}` until all nine slots of every nation are done (`seats_ready`).
4. Anyone sends `LoadCommons{j}` on the ER when every Commons record j is ready (with S6 it reads the base clones; without S6, see §6.6). If seats changed, `ExportLedger{civ}` follows before the next seat-changing window.
5. CloseCommits of tick `a_j` passes the barrier. The tick input carries the snapshot; PS_INPUT and `input_hash` cover it (C3, A13).

### 6.4 What happens under lock griefing (G2)

| Target | Cost to the attacker | Effect |
|---|---|---|
| A Cell, Commons or Member on base | a base fee auction: to hold one account the attacker buys 12M CU per block (≈ 2.4 SOL per 800 s window at 0.1 lamport/CU, ≈ 240 SOL at 10) **[derived, synth's formula at 800 s]**; an honest 12k-CU check-in outbids it for 1.2–120 µSOL | Delays only while paid. The crank and relays add priority fees automatically. The barrier keeps the result correct, so the tick waits instead of running on a partial tally. |
| Chunk 0 or a Nation on the ER | **free** (platform §2.3) | Freezes the Table of **every realm whose accounts the transaction names**: up to 32 per legacy transaction (1,226 B), up to the lock limit (64 [estimate]) with v0 + ALT if the ER accepts it. Windows keep closing on base (G5); if the lock is held until `running_deadline` (≈ 34 h at 60 s ticks) anyone may Abort, and players are refunded in full while the operator loses its revenue, escrow and SOL (§11.4). **The v9 residual, multiplied by realms per validator.** |
| A Commons clone on the ER | none: clones are read-only, and a clone lock only delays LoadCommons, which is the same as the chunk-0 case | same as above |

No lock of any account can change who is seated, what a tally says or what anyone is paid. A lock can still void a season, and the whole cost of that falls on the operator. That is why F13 gates campaigns (§12.3).

### 6.5 Why the Table's input stays inside the v9 gate

- **Tick input.** A snapshot is ≤ 6 × 48 B = 288 B per apply tick. Seat changes are ≤ 48 × 38 B = 1.8 KB at a seat window's apply tick.
- **Seat apply ticks refuse SubmitGov.** The input's governance part (up to 96 slots × 48 B = 4.6 KB) is therefore replaced by ≤ 1.8 KB, so that input is **smaller** than WP03's worst case.
- **World body.** +60 B of mobilisation fields and +192 B of chunk-0 digest copies, out of WP07's 0.9 KB slack. The T-bound must be re-asserted in CT(a).
- **Phase 11 additions** (seat changes, outgoing rows, credit voiding) are **unmeasured**. WP07's worst part is 1,136k against a 1.2M ceiling, so the margin is 64k; WP07 §7.7 already says nothing may be added to phase 0. The Commons additions go to phase 11, but whether they fit is **[uncertain until CT(a)]**. If they do not, the seat application moves to its own alone instruction before CloseCommits of the apply tick.
- **D13 envoy shares.** Credits name seats, so distinct (officer, proposer) pairs are bounded by 48 seats. The v9 rider (§14, R-D13) caps envoy shares at 8 per city-state in any case.

### 6.6 If spike S6 fails (no read-only base clones on the ER)

- `LoadCommons` becomes **crank-only** and takes `record_bytes(j)` as instruction data (≤ 2 KB at seat windows; the ER may allow larger transactions, platform #14 **[uncertain]**; otherwise send it in 2 parts).
- The ER cannot check the data, but it folds exactly those bytes into `Nation.snap_digest`, and base folded the same record, **seats included**, when the window became ready (G4). FinishSeason compares them.
- A crank that feeds false aggregates **or a false seat table** can therefore only void the season (refund; the operator's escrow is forfeited under O6). It cannot change a seat that survives to payout, a TermLedger row that is paid, or a SettleStewards amount, because none of them is paid when the digests differ.
- Outsiders cannot send LoadCommons, so they cannot grief this path.
- This is the v9 trust model: the operator is trusted for liveness only (§0.11). Citizens stay on base, so D's fallback of moving the Commons onto the ER (free griefing) is **never** taken.

---

## 7. Game rules that change (V5 references)

### 7.1 Phase 1–2 Council realms

The V5/v9 rules apply unchanged inside a realm. Changes for the player:

| V5 | Change |
|---|---|
| §3 シーズンの流れ, §4 国と国民 (D1: pick a nation) | Phase 1: you pick a **realm** in the directory and a nation (your **banner**) in its lobby. Phase 2: you pick a banner, pay, and the VRF deal picks your realm after registration closes. |
| §4 D1, no nation cap | v9 caps apply: ≤ 24 per nation per realm. No soft cap is enforced in Phase 1 (the directory shows fill per nation). |
| §5.3 選挙 | No pre-season votes (O22), because every realm has an AI; the first term is caretaker as in v9 AI seasons |
| §13, §18.2 AI | 1 hidden AI per realm by default, in a nation nobody knows in advance |
| §17.5 歴史の層 | A banner standing across realms (the leaderboard) is added above the per-realm history |

### 7.2 Phase 3 Commons realms

| V5 section | V5 today | Commons realm |
|---|---|---|
| §4 国と国民 | every member is in the world | **Citizens** on base, dealt into **cells** of ≤ 32 by the VRF and bound to that cell on chain; **seats** at the Table (4 officers + 4 tribunes per nation) |
| §5.1 役職と出せる命令 | 4 offices | 4 offices, unchanged, **plus 4 tribune seats**. Tribunes issue no orders; they propose, support, consent and sit on the jury. |
| §5.2 命令の枠 | officer batches | unchanged (only officers commit and reveal; WP04) |
| §5.3 選挙 | plurality among members every 30 ticks | Citizens **endorse** (plurality top-3 per office, window 3m − 2). The sitting tribunes act as a **jury** (window 3m − 1): quorum ≥ 3 votes cast, plurality of cast votes, ties by lottery, no quorum → caretaker (rule J4, §7.4). Seated at tick 30m + 1. |
| §5.4 献策 | any member proposes and supports | Tribunes and officers propose and support on the ER (WP03 quota 2). Citizens propose through their cell voice and the cell channel. |
| §5.5 リコール | a majority of recently active members | Petitions in the term's first two windows **vacate** the office (caretaker until the next term) if they exceed ½ of the citizens active in the term so far **and** citizens_n / 8. At most 1 recall per office per term. Revision 1's "next candidate takes over" is removed: a recall can only remove, never install. |
| §5.6 宣戦・国庫 | a second officer | unchanged (I4), **plus** treasury spends above the threshold need consent from ≥ 1 seated tribune (R6), **plus R7**: in Commons mode, consented Transfer/Exchange spending is capped at κ = 25 % of the treasury at term start per term, and Transfers to another nation never exceed the no-consent allowance (WP10 R5) per term, consent or not. |
| §6 達成 | points by 4 paths | unchanged, **plus mobilisation** (§7.3) |
| §7.3 国の中の配分 | 20 % equal + 80 % merit | citizen share (weighted by capped attendance, max 1.25×) + steward (capped) + cell share (§11.3) |
| §7.5 市場 | officers | unchanged, within R7 |
| §9 ずるの防止 | "every vote costs a fee" | fee per wallet + VRF cells bound on chain + jury with quorum + steward cap + R7 (§7.4) |
| §13, §18 AI | ≤ 2 per nation | AI citizens, about 1 per 4 cells (§10) |
| §18.3 住む都市 (D19) | home city per member | home city = f(`cit_index`) mod the nation's cities, deterministic; bounties (D20) unchanged for ≤ 64 AIs |

### 7.3 Mobilisation (rules v10, Commons mode)

- **Share per effort.** `share_e = pledges_e(window) / max(1, citizens_n)` in bps, where the denominator counts **all registered citizens**. Sybils raise the numerator and the denominator alike.
- **Bonus.** `bonus_e = B_e × min(1, share_e / 2,500 bps)`: concave and capped.
  - Starting values: production +20 %, research +20 %, city defence +25 %, influence +20 % at saturation. Simulation tuning and a golden re-pin are needed.
  - A more engaged nation is stronger; a bigger one is not.
- **No merit and no pay from pledges** (V5 §7.3 anti-farming). Attendance pay is capped separately (§11.3).
- **Residual.** A rival can dilute a nation by joining it and never checking in. That costs f per wallet and has a 1/n effect.

### 7.4 Office capture at scale (V5 §9 at 400 per nation)

**Revision 1's table was wrong.** `synth.py` counted capture only when the bloc held ≥ 3 of 4 jurors, while the rule it described ("ties → endorsement tally; no votes → endorsement rank 1") hands the office to an always-active bloc, which is endorsement rank 1 at low turnout, whenever its jurors are at least as many as the honest jurors who vote. The bloc cost column was hard-coded (`synth.py:88`, `wallets × 0.32`). The stress re-run (`lab/stress/jury.py`) and this revision's `capture2.py` agree.

**Revision 2 rules:**
1. **Jury J4.** Quorum ≥ 3 votes cast of 4 jurors; strict plurality of cast votes; ties → a lottery among the tied from the term seed; no quorum → caretaker for the term. A tribune who casts no jury vote gets no steward pay for that term, so honest jurors are expected to vote (v ≈ 0.9 in the table; v = 0.6 shown as sensitivity).
2. **Draw after the jury window seals** (§5.3 DrawTerm), so the bloc cannot see the seed and adapt check-ins or break a cell's quorum on purpose.
3. **Recall only vacates**, measured against the term's distinct active citizens with a floor of n/8 (§7.2).
4. **R7 treasury caps** (§7.2): no inter-nation Transfer above R5 per term, consented spend ≤ 25 % of the treasury per term.

**Capture probability per office-term** (n = 400; bloc share p always active; honest per-window turnout t; the columns at 833 per nation match within ±0.01) **[measured, capture2.py, 6,000 trials per cell]**:

| p | t | rev 1 table (synth) | rev 1 rule as written (spec) | **J4, v = 0.9** capture / vacancy | J4, v = 0.6 capture / vacancy |
|---|---|---|---|---|---|
| 0.05 | 0.1 | 0.16 | 0.53–0.57 | **0.37–0.45 / 0.02** | 0.40–0.45 / 0.25 |
| 0.05 | 0.3 | 0.011 | 0.03–0.07 | **0.07–0.10 / 0.04** | 0.08–0.10 / 0.42 |
| 0.10 | 0.3 | 0.071 | 0.33–0.37 | **0.21–0.27 / 0.03** | 0.23–0.30 / 0.32 |
| 0.10 | 0.5 | 0.021 | 0.04–0.10 | **0.10–0.14 / 0.04** | 0.11–0.16 / 0.39 |
| 0.20 | 0.3 | 0.44 | 0.80–0.83 | **0.65–0.74 / 0.01** | 0.68–0.72 / 0.13 |

(Ranges: honest jurors united on one candidate, or split over two.)

**Recall by a bloc alone** (no honest petitions) **[derived, capture2.py]**: revision 1 needed p > t/(1 + t), i.e. 0.09 at t = 0.1 and 0.23 at t = 0.3. Revision 2 needs p > 0.16 at t = 0.1, 0.34 at t = 0.3 and 0.43 at t = 0.5, and even then it only vacates the office.

**What capture can gain, and what it costs** (revision 2 economics, §11.3; f = 10, equal nation scores) **[derived, capture2.py]**:
- **Steward pay:** ≤ 0.5 f per captured office-term; officers exist in terms 1–5, so ≤ 10 f per nation-season.
- **Treasury:** R1/R2/R5 plus R6 and R7: at most 25 % of the treasury per term with a tribune's consent, and no more than the R5 allowance to another nation per term. Capture can still waste a nation's treasury inside the nation (bad trades), within those caps.
- **Score:** captured officers can throw their own nation's score. That shifts the inter-nation pool toward the other five nations; a bloc that also holds wallets there gains a fraction of that shift. This is not bounded by a cap; it is the game's politics, and it is visible in the public logs.
- **Cost of a Sybil bloc:** each extra wallet pays f and gets back at most 1.25 × the passive citizen rate: 8.2 USDC at n = 400, p = 0.1, t = 0.3 (−18 %), 8.7 at t = 0.1 (−13 %). A 40-wallet bloc nets −72 USDC (−7.2 f) at t = 0.3 and −51 USDC at t = 0.1.
- **Verdict.** With steward pay alone, capture does not pay in expectation at t ≥ 0.3 and p ≤ 0.1 (expected gain ≤ 10 f × 0.27 ≈ 2.7 f against 7.2 f of cost). **At low honest turnout (t ≈ 0.1) it can pay**: gain up to 10 f × 0.8 ≈ 8 f against about 5 f of cost at p = 0.1. Revision 1's "capture never pays" is withdrawn. What the design promises is narrower: monetary gain from capture is capped (steward cap, R6, R7), and the probability is published per realm by verify-commons.
- **Bribery.** Drawn voices are unknown until the seed lands after the jury window, and seating then follows deterministically with no further choice. Sitting tribunes (future jurors) are known for their whole term, like officers in V5; bribing them is a residual.

---

## 8. Player experience

### 8.1 Cells (from C, moved to base)

- **Dealing.** A citizen's cell is `⌊π_{seed,civ}(cit_index) / 32⌋`, where π is A's keyed Feistel permutation with the post-close VRF seed. No choice and no packing. The result is written to `Member.cell` at Admit (or RegisterCitizen), and every cell instruction checks the Cell PDA against it.
  - Cell sizes differ by ≤ 1.
  - The inverse permutation maps (cell, slot) back to an ordinal, then the CitizenIndex maps the ordinal to a Member, which is what the lottery uses.
  - Friends are not co-dealt yet; parties of ≤ 3 per cell come later (A §3.2).
- **What a cell is:**
  - a chat of about 31 people (off-chain, rate-limited, scoped per cell);
  - a **voice**, voted once per term in the endorse window by members who checked in that window, and sealed with it;
  - a team stake: a tribune shares τ = 50 % of their steward pay equally with the members of their cell who checked in during the tribune's term;
  - a unit of the tribune draw.

### 8.2 One season for an ordinary citizen (Commons realm, 60 s ticks, ≈ 4 h)

| When | What they do | On chain |
|---|---|---|
| Before | Pick a banner, pay f (USDC; no SOL), get a ticket. At close: "Realm 3, Astel, cell 7 (31 citizens, some may be AI)" | JoinCohort → Admit |
| Every window (≈ 13 min of wall-clock; the Table covers about 10 ticks of it) | One tap: **check in** with a labour pledge (production, research, defence or influence). The nation's mobilisation meter moves at the window's apply tick. | CheckIn |
| Every term (3 windows) | Stand for office (optional). In the endorse window, vote for your cell's voice and endorse candidates. Petition to recall a failing officer in the term's first two windows. | Stand, CellPick, Endorse, RecallPetition |
| When drawn | **Summoned to the Table** as a tribune for a term: propose, support, consent to large treasury spends, earn steward merit; **vote on the jury** for the next officers (no vote, no pay) | SubmitGov on the ER (seat key); JuryVote on base |
| After | Claim `citizen share + steward + cell share + treasury refund` in one transaction, with a breakdown per pot (I9) | Claim |

### 8.3 Chances and voice

At 400 citizens per nation and 13 cells **[derived]**:
- **Voice.** About 1 in 31 per term, by the cell's vote in the endorse window.
- **A drawn cell.** 4 of 13 cells are drawn per term, so about 31 %.
- **Direct seat.** 20 tribune-terms (terms 1–5) plus 4 at genesis over 400 citizens is about a 6 % chance per season, plus the officer race by endorsement.
- **Every window.** A citizen moves 1/400 of a nation's mobilisation share.

### 8.4 Spectating, relays and scale off-chain

These apply in every phase:
- a cached state JSON per (realm, nation, tick) behind a CDN, because fog is per nation;
- per-member data through a small authenticated call;
- header-slice crank polling;
- talk per cell, nation, realm and banner.

The current play server breaks at about 500–1,000 viewers in every design (chain-members §4.2), so this cache is required.

**Relays (revision 2).** Citizen instructions check the session key's signature, never the fee payer, so **anyone may relay**. The client ships with a list of relays (the operator's, community ones) and sends directly when the citizen's wallet holds SOL. Relay code is published. The residual is in §12.3.

---

## 9. Phase 1: what ships by 2026-10-12

### 9.1 Scope

Everything is in **new files**. Nothing edits files that wave-4 units G1, G2 or W own.

| Unit | Files (new) | What | Effort |
|---|---|---|---|
| **X1 realm directory** | `permutation-gateway/realms/{directory,registry}.mjs`, `realms/test/*.test.mjs` | `GET /realms` (realm → origin, season id, program id, fill per nation, phase), `GET /standings`. **No join path**: each entry links to its realm's own play-server origin, whose existing lobby handles the wallet, the nation choice and the x402 Register for that season. | 1 d |
| **X2 launcher** | `permutation-gateway/realms/launch.mjs` | N × (gateway `--state <dir>/realm-r.json --port --public-port --ai 1 --tick-seconds` + play server `--gateway-proxy`); ports from a base (for example 5301+); **one play server per realm** (one origin per season). All realms share the checkout's key directory, which the gateway already supports ("run several seasons side by side", `server.mjs:10-11`); starts are **staggered and awaited** so that `BigInt(now())` season ids (`season.mjs:207`) never collide. | 1.5–2 d |
| **X3 banner board** | `permutation-server/web/realms/{index.html,banners.mjs}` | Reads `/standings`. Banner score `S_b = 6 × mean_r share_{r,b}` over realms that are Finalized **and** VERIFIED (the verifier report JSON); links each realm's verifier report | 1–2 d |
| **X4 soak, S1-local and devnet runbook** | `permutation-gateway/realms/{soak.mjs,lockgrief.mjs,SOAK.md}` | 10–20 realms with bots on the local stack (own non-owner ports, for example 38899/37799/36699). Records per-realm tick p50/p99 (which sets `slack`, §6.1), ER CU/s, crank RPC/s, stalls. **S1-local:** one looping transaction write-locking k chunk-0 accounts on the local ER, k = 1, 8, 32; records how long every named realm stalls and whether honest ticks resume. Local only, no devnet. | 1.5 d |
| Docs text | handed to unit DOCS (wave 5) | SUBMISSION/PITCH/README (§9.4) and the residual list (§14.1) | 0.5 d |

**Total: 5.5–7 engineer-days, parallel to the v9 waves.** Revision 1's X1 router and X5 verify-wave are dropped (Revision notes, issues 5 and 11).

### 9.2 Placement in Phase 1: open realms (replaces revision 1's rule P1)

- **Rule.** Players choose. The directory orders realms by fill and suggests the least-full one per banner, but any open realm and any nation below the chain cap can be joined.
- **Why not a randomized router now.**
  - Every realm's public listener serves `POST /x402/join`, and the play server proxies it (`app.mjs:42-46`, `play/proxy.rs`); a router could be skipped by posting there.
  - Through a router, an x402 client must learn the season before signing Register, so the 402 answer reveals the placement before payment. Fresh keypairs are free, so a user can probe until a wallet lands in the target realm (≈ R probes, slowed only by `POST /x402/join`'s 0.2/s per-IP limit, `guards.mjs:52`).
  - Closing both needs a router-signed admission check in `routes/x402.mjs` (G1's file) or a filtering reverse proxy in front of every realm, plus reservations, which is 3–5 more days on the critical path.
- **What this gives up.** Nine friends can join the same realm-nation. That is V5 §9's accepted "coordinated group" case at 8 per nation; it gains nothing that a V5 season does not already allow.
- **Placement nobody chooses** arrives with Phase 2 (pay first, deal after close).

### 9.3 Timeline and cut line

**State of the v9 critical path on 09-27 [measured, git]:** `codex/v9-security` has wave 1 merged (units A, B, C) plus follow-ups. Waves 2–5 (14 units), the §4.3 gate, local E2E, the devnet steps and D3 all remain. At HEAD (`96a3464`) there is no O14 and none of the 85 fixes.

| Dates | v9 critical path (unchanged contract) | Scale track (X1–X4) |
|---|---|---|
| 09-27 → 09-28 | owner decisions F1–F4, F13, F17; **lift the pause** on WP03/06/07/10 (amendment A14, §14) | – |
| 09-28 → 10-03 | waves 2–3 (D1, R with the R-D13 rider, P1–P5) | X1, X3 against HEAD's read APIs; X2 against HEAD's CLI flags |
| 10-04 → 10-07 | wave 4 (G1, G2, W, S, V, T) + §4.3 gate | X4 soak and S1-local on the HEAD build (mechanics) |
| **10-07** | **cut line.** If gate 4 is green: the scale track proceeds on the v9 build. **If not: no multi-realm run on devnet at all** (the HEAD program has none of the 85 fixes); the multi-realm demo is local only, and SUBMISSION says so. | – |
| 10-08 → 10-10 | local E2E (§4.7), devnet deploy steps (§5.3, owner-approved) | X2 rework for wave-4 CLI changes (0.5 d budgeted); soak re-run on the v9 build; devnet 2–3 realms (6 AIs each so the maps are lively, F5) with a **dedicated RPC** (public devnet allows 40 calls per method per 10 s per IP, and one crank polls about 33 per 10 s) |
| 10-11 | D3 (Squads or `--final`), DOCS, RELEASE records the soak and S1-local numbers | – |
| 10-12 | submit | – |

### 9.4 The honest SUBMISSION line

> One realm = one verifiable world of 48 seats. Several realms run side by side under six banners: load-tested to **X** realms locally and **N** on devnet. A free write-lock on the rollup can still stall realms and void their seasons (players are refunded in full; the operator bears the loss), so larger public campaigns wait for a dedicated or ingress-filtered rollup. Designed path to thousands on shared maps (Commons realms): measured per-action costs of 5–24k CU, flat in N. Labelled as prototype measurements, not shipped.

- X and N are measured values. If the 10-07 cut line triggers, N is 0 and the sentence says "locally only".
- **The word "thousands" is used only for the design path**, never as something that runs, until S1 (on the target ER) and F13 are done.
- If the v9 gate lowers the Table cap (C1), "48" becomes the gated number.

---

## 10. AI members at scale

| Phase | Default | Why | Operator economics (f = 10) |
|---|---|---|---|
| 1 (demo) | 6 AIs per realm (1 per nation) on devnet; 1 per realm otherwise | keeps "is there one among us?" alive; O14 (in v9) keeps registration atomic in AI seasons | net ops per realm: +86 USDC at a = 1, +36 at a = 6, −24 at a = 12 (A §9.2) |
| 2 | AI tickets ≈ 1 per realm, **dealt by the VRF** like everyone else, so the operator cannot place AIs | stronger than WP09, where the operator picks nations | the bond pool is sized for the worst deal (§5.2) |
| 3 | AI citizens ≈ **1 per 4 cells** (≈ 20 per realm at 2,400), ≤ `MAX_AI` 64 | "Is our tribune an AI?" is the scaled hidden-AI hook; AIs can become voices, tribunes, jurors and officers on equal terms (D14–D17) | AI units are removed from the people's denominators at reveal; AI steward rows and AI cell shares are voided and join the people pot (§11.3). About 20 × f = 200 USDC of cost against 4,800 USDC of operations share per realm |

- **Commit and reveal:**
  - WP09 roster commit before registration (per realm in Phases 1 and 3; per cohort in Phase 2);
  - reveal after the season;
  - `bond_floor = ⌈(a·P0 + N·π)/(N − a)⌉`, which tends to π at large N (A7 holds).
- **Before the reveal,** AI citizens count in `term_active`, cell quorums and the cell share denominator exactly like people (nobody can tell them apart). At the reveal their own claims become 0 and their units leave `units_people`.
- **Forfeited roster** (no reveal): as WP09 D6. The bond is split over every member, and every wallet, including the operator's AI wallets, claims as a person. No new rule.
- **Agents play through the same relays and instructions as people.** No operator-only path exists.

---

## 11. Economics

### 11.1 Who pays

- **Players** pay only the entry fee f in USDC: 80 % to the realm pool, 20 % to operations. No SOL is needed, as today.
- **The operator** pays every SOL cost out of the 20 % share:
  - rent float, reclaimed by CloseSeasonAccounts, CloseCommons and CloseMember;
  - delegation fees;
  - base relay fees (for citizens who use the operator's relay);
  - VRF;
  - priority fees under contention.
- **Option F10:** a refundable SOL rent deposit in the entry, trading away "no SOL needed".

### 11.2 Totals

SOL = 150 USD, f = 10 USDC, 5,080 lamports/B; ×1.37 at 6,960 **[derived, synth and rev2-numbers]**.

| | 1,000 | 10,000 | 50,000 |
|---|---|---|---|
| **Phase 1–2 Council realms** | 22 realms | 213 | 1,064 |
| Locked during the season | 17 SOL | 164 SOL | **817 SOL** |
| Member rent (lost in v9; reclaimable from Phase 2) | 2 | 23 | 116 |
| Burned (delegation, commits, VRF, fees ≈ 0.032 per realm) | 0.7 | 6.8 | 34 |
| **Phase 3 Commons realms (2,400)** | 1 realm | 5 | 21 |
| Realm float (Table + Commons, reclaimable) | 1.3 SOL | 7.7 SOL | **35 SOL** |
| Member rent (reclaimable by CloseMember after the claim) | 2.3 | 22.8 | 114 |
| Burned (realm burn + ≈ 30 relayed base txs × 5,000 lamports per citizen) | ≈ 0.2 | ≈ 1.7 | ≈ 8 |
| Operations income (20 %) if every season finishes | 2,000 USDC | 20,000 | 100,000 |

- Phase 1 is practical to the low thousands in capital terms; it is not safe at that size on a public ER (§12.3).
- Phase 3 cuts the float about 23× (and total capital about 6×, counting Member rent).
- SIMD-0437's later steps (696 lamports/B) would cut every rent row about 7×.

### 11.3 Payout inside a nation (Commons; revision 2)

With pool `P_n` (the nation's share of 0.8 × paid-in, by points as in V5 §7.2):
- **Steward pot.**
  - `S_n = min(σ·P_n, Σ_rows c_term·f)`, with σ = 15 % and `c_term` = 0.5 f.
  - Split by merit over ≤ 72 rows, with a person cap of 2 f per season. A tribune row with no jury vote for its term is 0.
  - Tribune rows give τ = 50 % to `cell_pot`.
  - AI rows are voided at reveal; their amount and any overflow join the people pot.
- **Citizen share (replaces revision 1's equal and service pots).**
  - Units: `u_i = 1 + β · min(w_i, 12)/12`, where `w_i` is the citizen's checked-in windows and β = 0.25. Every registered person has at least 1 unit; attendance adds at most 25 %, and only the first 12 of 18 windows count (6 windows of grace).
  - `rate_n = (P_n − S_paid − cell_paid) / Σ_people u_i`; each person gets `u_i × rate_n`.
  - **Bound:** with equal nation scores, a wallet that only checks in gets back at most `1.25 × (0.8 f − S_n/n)` < 1.0 f (0.93 f at n = 400, 0.96 f at n = 833, reached only if nobody else ever checks in **[derived]**), whatever anyone else does. Scripted attendance never recovers the fee in a nation that scores at the average; in a nation that scores above average, everyone in it gains, which is the game's intent.
  - R4's intent (AI shares must not concentrate on a few people) holds by construction: AI shares join a pot that pays everyone within a factor of 1.25.
- **Cell share.** `cell_pot / cell_active`, where `cell_active` = members of that cell with ≥ 1 check-in during the tribune's term (people and, before reveal, AIs; AI parts are voided at reveal and join the people pot).
- **Treasury refund.** Per member, as v9 (WP10 C2/C4, WP12 C26).
- **Worked shape** (equal nations; f = 10) **[derived, capture2.py]**:

| n | pool | steward | passive (u = 1) | casual (t = 0.3, ≈ 5 windows) | always-on (u = 1.25) |
|---|---|---|---|---|---|
| 400, no bloc | 3,200 | 240 | 6.65 USDC | 7.40 | 8.31 (−17 %) |
| 400, 10 % bloc, t = 0.1 | 3,200 | 240 | 6.99 | 7.25 | 8.74 (−13 %) |
| 833, no bloc | 6,664 | 240 | 6.93 | 7.71 | 8.67 (−13 %) |

  Officers get up to +5 per term and tribunes +0–5 per term, capped. Compare revision 1, where a script wallet got about 23 USDC per 10 USDC fee at t = 0.3 (the stress test's figure, which this revision confirms for the revision-1 formula), and V5 at 10k, where officers get about 100× the fee and ordinary members −84 % (game-intent §2.3).
- **Rounding and conservation.**
  - Σ floors ≤ each pot, with dust < 1 base unit per citizen per term **[measured, D topc.py: worst 0.935]**.
  - `outstanding` is set to Σ allocations and decremented exactly (WP12).
  - Conservation: `Σ claims + outstanding + swept_dust + ops + refunds == paid_in` at every moment; with F11b on, `+ forfeited`. Unclaimed prizes stay in `outstanding` forever unless the owner turns F11b on.

### 11.4 What an abort costs whom (new in revision 2)

Under O6/C9 (WP14 §3.3), an abort from Running pays each member `fee + shares + ⌊escrow / N⌋`, and the operator's 20 % is never paid, because fees are refunded in full.

| Realm | Escrow (bond floor + bounties, b = 10 per AI) | Per member from escrow | Operator loses |
|---|---|---|---|
| Council, N = 48, a = 1 | 9 + 10 = 19 USDC | 0.40 USDC (4.0 % of f) | 96 USDC of revenue + 19 of escrow + the realm's SOL burn |
| Commons, N = 2,400, a = 20 | 164 + 200 = 364 USDC | 0.15 USDC (1.5 % of f) | 4,800 USDC of revenue + 364 of escrow + SOL |
| 50k in 21 Commons realms | ≈ 7.6k USDC | – | 100k USDC of revenue + ≈ 7.6k of escrow + SOL |

**[derived, rev2-numbers.py]**

- Players are always made whole. An attacker who holds wallets gains 1.5–4 % of f per wallet, so the motive is griefing, not profit; the loss falls on the operator.
- The operator cannot be credited with "the stall was not our fault": an ER lock and an operator stall look the same on base, and crank heartbeats are signed by the operator. A rule that let the operator keep its 20 % on an unattributed abort would give it a free stall option and cost players 20 % on every ER failure. It is rejected (F17).
- In Phase 2, forfeited escrow on a Running abort can go to a carry-forward pool (the next season of the same campaign) instead of the members. That removes the attacker's small profit and still never returns the escrow to the operator (F17 alternative, with its own solvency proof).
- This residual goes into DOCS (§14.1).

---

## 12. Budgets: per action and per tick, honest and adversarial

### 12.1 Per action (constant in N)

| Action | Layer | CU | Heap | Tx bytes | Per wallet bound |
|---|---|---|---|---|---|
| JoinCohort | base | 45–60k [estimate, A] | < 32 KiB | ≤ 1,232 | pays f |
| Admit (6 tickets) | base | 60–90k; worst 250 walks ≈ +110k [estimate, A] | 32 KiB | – | permissionless, deterministic |
| CheckIn | base | 8.0k [measured, D] (≤ 12k with the cell) | 392 B | ≈ 422 [estimate] | 1 per window |
| CellPick | base | ≈ 7k [estimate] | < 1 KiB | ≈ 390 B | 1 per term |
| Endorse (4 offices) | base | ≤ 23.6k [measured, D] | 872 B | 522 B | 1 per term per office |
| JuryVote / RecallPetition / Stand | base | 6–8k [estimate] | < 1 KiB | ≈ 400 B | tribunes only / 1 per office per term / 1 per term |
| SealCells (12 cells) + SealWindow | base | ≤ 45k + ≈ 20k [estimate] | < 8 KiB | **767 B** [measured] / < 300 B | permissionless, once |
| SeatTerm (one cell try) | base | 30k + 4.4k per member try [measured, D] | < 4 KiB | **535 B** [measured] | permissionless, ≤ 4 calls per slot |
| LoadCommons | ER | 40–60k [estimate] | < 16 KiB | < 600 B (S6) | permissionless (crank-only without S6), once per window |
| ExportLedger | ER | ≈ 20k [estimate] | < 4 KiB | < 400 B | permissionless, after a seat change |
| SettleStewards (72 rows) | base | ≈ 0.155M [derived from measured 2.1k/row] | ≈ 6 KiB | 286 B | once per nation |
| Claim (Commons) | base | v9 Claim + 5.4k [measured, D] + cell read | 392 B | ≈ 400 B | once |
| FinishSeason (Commons) | base | v9 + digest check [estimate] | v9 | **1,127 B** [measured] | once |
| Heaviest ResolveTick part | ER | ≤ 1,136k [measured on the WP07 prototype; integrated gate pending; Commons phase 11 **uncertain until CT(a)**] | ≤ 191 KiB | **1,145 B** [measured, v9 accounts + CU limit + heap frame] | v9 gate |

### 12.2 Per tick and per season at 1k / 10k / 50k **[derived, synth]**

ER CU per realm per tick: honest ≈ 2.8M, **protocol-bounded** ≤ 28.1M (12 × 1.2M + 8 × 1.2M + 24 × 60k + 96 × 25k + 0.3M). ER transactions per realm per tick: honest ≈ 40, protocol-bounded ≈ 150. "Protocol-bounded" means every game-legal transaction at its cap. **It is not an adversarial bound**: free ER spam is unbounded by the protocol, and the platform's FCFS backpressure reportedly sets in at about 1,024 pending transactions **[reported by the stress test; uncertain, S1]**.

**Phase 1–2 (Council):**

| | 1,000 (22) | 10,000 (213) | 50,000 (1,064) |
|---|---|---|---|
| ER CU/s at 30 s: honest / protocol-bounded | 2.1M / 20.6M | 19.9M / 200M | 99M / **997M** |
| ER CU/s at 120 s: honest / protocol-bounded | 0.5M / 5.2M | 5.0M / 49.9M | 24.8M / 249M |
| ER tx/s at 120 s: honest / protocol-bounded | 7 / 28 | 71 / 266 | 355 / 1,330 |
| Per-tick VRF requests/s at 120 s | 0.2 | 1.8 | 8.9 [oracle throughput **uncertain**] |
| Delegations (and undelegations) | 572 | 5,538 | 27,664 (stagger; MagicBlock's 41k burst needed a fix) |

- **Verdict.** 1k fits one ER at any tick length. 10k needs 120 s ticks and probably 1–2 ER validators. 50k needs 120–300 s ticks and 3–10 validators (unmeasured; spike S7). **None of it is safe on a public ER** (§12.3).
- **Phase 1 claim.** By 10-12 it claims only the measured soak.

**Phase 3 (Commons, 2,400 per realm):**

| | 1,000 (1 realm) | 10,000 (5) | 50,000 (21) |
|---|---|---|---|
| ER CU/s at 30 s: honest / protocol-bounded | 0.09M / 0.94M | 0.47M / 4.7M | 2.0M / **19.7M** |
| ER tx/s at 30 s: honest / protocol-bounded | 1.3 / 5 | 6.7 / 25 | 28 / **105** |
| Base CheckIns/s, all active (60 s ticks, 800 s windows) | 1.3 | 12.5 | 63 (0.75M CU/s) |
| Base worst case: every wallet at every bitmask limit (60 s ticks, 800 s windows) | 4.5 tx/s | 46 tx/s | **≤ 230 tx/s, ≤ 3.5M CU/s** (≈ 1.4 % of 250M CU/s base; revision 1 said 306 at 600 s windows) |
| Hottest base account (Commons: Endorse + Recall + Seal) | < 1 tx/s | < 1 tx/s | ≈ 1.1 tx/s against about 500 per block at 12M CU per account |
| Delegations | 32 | 160 | 672 |
| Registration chain time (≈ 250 per block per realm, realms in parallel) | 1.6 s | 3.2 s | 3.8 s |

- **On base, the adversary cannot exceed the "everyone maximally active" line.** Each wallet paid f and is bitmask-bounded; SealCells, SealWindow, DrawTerm, SeatTerm and LoadCommons are once per window or term.
- **Failing spam through a relay is refused in simulation, for free.** Direct-to-RPC spam pays 5,000 lamports per signature.
- **On the ER, only the Table's legal load is bounded.** See §12.3.

### 12.3 Blast radius (the one residual that grows)

| Attack | Phase 1–2 | Phase 3 | Outcome |
|---|---|---|---|
| Free ER write-lock on chunk 0 | **Every realm the transaction names, on the same validator:** 32 per legacy transaction (1,226 B [measured]); 21 fit in 863 B [measured]; up to the lock limit (64 [estimate]) with v0 + ALT if the ER accepts it. 1,064 realms (50k) need ≈ 34 legacy transactions, ≈ 17 with ALT. | **21 realms (50k) in one transaction.** | Ticks stop; windows keep closing on base and wait (G5). If held until `running_deadline` (≈ 31 h at 30 s ticks, 34 h at 60 s [derived]), anyone may Abort: players refunded in full, operator loses revenue, escrow and SOL (§11.4). An attacker who stops earlier leaves a realm that resumes and finishes (G5; gate test CT(k)). |
| Relay flooding (HTTP or simulation budget) | – | Citizens using that relay miss windows | Motive: attendance raises one's share. Bounded by the citizen share's shape: blocking **every** honest check-in for a whole season raises the remaining wallets' rate by ≈ 10 % (n = 400, p = 0.1, t = 0.3) **[derived]**. Missed windows up to 6 cost nothing (grace). Mitigations: anyone may relay, direct send with own SOL, several relays (§8.4). The operator can also censor its own relay; the others remain. |
| Stalling undelegation past `running_deadline` | 26 targets per realm | 32 targets per realm (not 1,566 pages as in C) | abort and refund (A2 residual) |

**What follows from the first row:**
1. **F13 is a precondition for any campaign beyond the demo realms.** Options, in order of preference **[all uncertain; spike S8]**: an ER validator that filters ingress (only transactions whose fee payer is on an allowlist, or that call our program, may write-lock our delegated accounts); a dedicated validator; a self-hosted ER (the MagicBlock validator is published under BSL 1.1). Each has trust and licensing questions that S8 must answer.
2. **Until then, spread realms across validators (O8 pin) with at most `K_v` realms per validator** (default 4), so one transaction reaches few realms. On public devnet there are only a few regional validators, so this caps a campaign at a few dozen realms, not thousands.
3. **Run S1 on the target ER before any "thousands" claim.** S1-local (X4) gives the mechanics before 10-12; S1 on the real ER needs owner approval.

---

## 13. Verification

- **Per realm:** the WP16 verifier, unchanged (`--er`, `input_hash`, seals, PS_TICK v9, solvency, program pin).
- **Phase 1:** no placement to verify (open realms). The directory lists each realm's program id and verifier report.
- **verify-cohort (Phase 2, A §11):**
  1. tickets against CohortBanner counts;
  2. recompute `realm(t)` for every ticket from `PS_COHORT_SEED`;
  3. realm membership equals the dealt set;
  4. standings recomputed from `nation_points`.
- **verify-commons (Phase 3):**
  1. rebuild every window aggregate from `PS_CIT` base logs (`--recount`), with every check-in's cell equal to `Member.cell`, and compare with `PS_SEAL`;
  2. recompute each `record_bytes(j)`, **seats included**, and both digest chains: the base fold (at readiness) and the ER fold (`PS_COMMONS_IN`), in order 0 … 17, with none skipped (G2, G4);
  3. check that DrawTerm's request came after every jury window of its term was sealed, and recompute every SeatTerm try from the term seed, the cell deal and the check-ins;
  4. recompute SettleStewards from the committed TermLedgers and `jury_voted`;
  5. every claim equals the closed form;
  6. publish per realm the jury outcomes (capture risk is visible, §7.4).
  - Base logs are durable.
  - ER retention matters only for the Table: 48 seats, the same exposure as v9.
- **A player's own check:** their realm's replay, their cell (O(1)), their claim breakdown.

---

## 14. Delta to the security contract

This is the exact delta to `fix/contract.md` and `fix/contract-amendments.md`. "Kept" means the v9 text applies unchanged per realm.

### 14.1 Now (by 10-12): amendments only, no change to §1

| Item | Change |
|---|---|
| **A14 (new amendment), "Member scale" resolved** | The pause on WP03, WP06, WP07 and WP10 is **lifted**. Those WPs ship as written. C1/O9 (`SEASON_MEMBER_CAP = 48`, fallback 36/24/18) is kept **permanently as the per-world (Table) cap**. **The 48 figure is measured on the WP07 prototype; the joint gate on the integrated build is pending (wave 4).** The product answer to scale is realms (Phases 1–3 of this document). A6 stays. No v9 unit is superseded. |
| **Unit R, rider R-D13** | Cap `CityState.envoys` at 8 distinct entries per city-state. Beyond 8, merge into the entry of the same officer, else refuse the envoy. Test `envoy_shares_are_capped`. T re-asserts the T-bound with it. |
| Units D1, P1–P5, G1, G2, W, S, V, T, RELEASE | **Unchanged.** Scale units must not edit their files before wave 4 merges. |
| Unit DOCS (wave 5) | + the SUBMISSION/PITCH/README text of §9.4. + residuals: **the free ER write-lock reaches every realm on a validator per transaction**; **an abort from Running refunds players in full and costs the operator its 20 % revenue, its escrow and its SOL, and a third party can cause it for free** (§11.4); Phase 1 placement is players' choice, not random; operator capital per realm; one play server per realm; dedicated devnet RPC. |
| Unit RELEASE | + records the X4 soak (realms, tick p50/p99, ER CU/s, crank RPC/s) and S1-local (stall per k locked accounts); + the devnet realm run, or "local only" if the cut line triggered. |
| **New units X1–X4** | As §9.1. New files only (`permutation-gateway/realms/**`, `permutation-server/web/realms/**`). They run parallel to waves 2–4 and are integrated after gate 4. |
| **New tests (Phase 1)** | `realms/test/directory.test.mjs`: lists only registered realms, never exposes a gateway's operator port, reports program id and fill. `realms/test/launch.test.mjs`: 2 realms on free ports with distinct season ids when started in the same second; shared key directory. S1-local script output checked into RELEASE. |
| **New gate: Realm soak (release note, not a program gate)** | ≥ 10 realms concurrently on the local ER for ≥ 30 ticks; no stall; per-realm tick p99 ≤ 2 × `tick_seconds`; every realm VERIFIED. SUBMISSION may quote only measured numbers. |
| §6 owner decisions | O9 reworded: "48 per world is the Table cap; scale is by realms". O6: re-confirmed for now with its operator-loss residual disclosed; revisited in Phase 2 (F17). O26: continuous realms need scheduled maintenance gaps on devnet. |

### 14.2 Phase 2 (after 10-12): cohort

| WP / unit | Status | Change |
|---|---|---|
| WP01, WP02, WP03, WP04, WP05, WP06, WP07, WP08, WP10, WP13 | **Kept** per realm | Admit and SpawnWorld act only in Registering; the AI flush and SeasonFull side channel (O17) vanish in cohort realms |
| WP09 roster | **Changed** | Cohort `ai_commit` before joining opens; AIs dealt by the VRF; RevealCohortRoster (tag 49); bond pool for the worst deal |
| WP11 randomness | **Added** | Cohort seed with the season-seed pattern (O3); per-realm season seeds stay separate |
| WP12 solvency | **Added** | I-C1 cohort solvency (checked decrement of `outstanding`); I-C2 exact fee + deposit at Admit; **SweepDust (tag 50) moves only `vault − outstanding`**; ForfeitUnclaimed (tag 51) only if F11b, with its own conservation test |
| WP14 escape | **Added / changed** | AbortCohort, RefundTicket, CloseMember **after claim only** (fixes the "Member rent never reclaimed" residual without destroying claims). If F17 picks carry-forward: forfeited escrow of a Running abort goes to the campaign's next-season pool (new conservation proof) |
| WP15 upgrade | **Added** | I-C5 rules pin at SpawnWorld; mainnet uses per-version program ids (O30) |
| WP16 verifier | **Added** | verify-cohort |
| WP17 registration | **Moved** | Its checks move verbatim into JoinCohort; Register refuses direct joins into cohort seasons (I-C4) |
| §1.1 / §1.2 / §1.4 | Appended | Tags 37–51, errors 45–48, Season block 8 (`cohort_id u64, world u16, mode u8, realm_cap u32, nation_points [u64;8]`), Member tail `rent_payer` |
| WP18 tests | Added | Coverage pairs for 37–51; admit-equivalence (a cohort realm ≡ a Register realm byte for byte after seating); Feistel bijection and ±1 balance on SBF vs host; cohort solvency property test; AbortCohort/RefundTicket for every stuck state; **SweepDust never lowers `outstanding`; CloseMember refused before the claim**; ≥ 20-realm local soak |
| **Cohort gate** | New | JoinCohort ≤ 60k; Admit (6 tickets, 250-walk worst) ≤ 200k; CloseCohort at R = 4,096 ≤ 100k; ReportWorld ≤ 20k; all under the Light/Medium profiles (§4.2) |
| Gateway | After wave 4 merges | Crank fleet (supervisor, ≤ 100 realms per worker, header-slice polls, event-driven deadlines); the season id convention; the CDN state cache; `K_v` realms per validator |

### 14.3 Phase 3: Commons realms (Council realms stay exactly Phase 2)

| WP | Status (Commons mode) | Why it is at least as strong |
|---|---|---|
| WP01 play gate | **Changed** | LoadCommons is allowed only in the commit phase (`!frozen && !revealing`) and before the end; the alone rule; the G2 barrier in **CloseCommits** (repeated in FreezeTick); SubmitGov refused at seat apply ticks. No permissionless interleaving deadlocks a tick (CT(l)). |
| WP02 undelegation | **Changed** | +6 TermLedger targets (bits 26–31), `undelegation_order = [1..=19, ledgers, nations, 0]`, chunk 0 last; ledgers ≤ 5 commits each; ExportLedger must have emptied every outgoing area before the last tick completes |
| WP03 inbox / auth | **Changed** | Kept verbatim for the 48 seats (quota 2; the roll = seat keys). Citizens bypass the inbox. Their actions are base transactions on their own Member **and their own Cell (PDA bound to `Member.cell`)**, authenticated by the full 32-B session key plus a runtime signature, with per-window/term/office bitmasks. An unforgeable per-member quota with no shared pool, and no way to act in another cell. |
| WP04 batch caps | Kept | – |
| WP05 patrol | Kept | – |
| WP06 election CU | **Replaced** | No O(voters) step anywhere: Endorse is O(3) per office on base; SeatTerm is O(1 cell try) per call; the tie key is sha256 over the post-close VRF seed; eviction only by a higher key; the tally comes from base-sealed state (G2) |
| WP07 world / heap | **Changed** | 48 seats; +60 B mobilisation and +192 B digest copies within the 0.9 KB slack; the seat apply input is smaller than WP03's worst case; ResolveTick's account list stays v9's (ExportLedger); **Commons phase-11 cost is unmeasured and must fit the 64k margin, CT(a)** |
| WP08 market | Kept | New money code uses the same checked u128 `mul_div` |
| WP09 roster | **Changed** | ≤ 64 AI citizens; RevealRoster records AI units; AI steward rows and cell shares voided to the people pot; forfeit as WP09 D6 |
| WP10 treasury | **Changed** | R1, R2, R5 and C1–C5 kept. **+R6** tribune consent above the threshold. **+R7** consented spend ≤ κ (25 %) of the term-start treasury per term; inter-nation Transfer ≤ the R5 allowance per term even with consent. R4's intent holds because AI shares join a near-equal pot. |
| WP11 randomness | **Added** | Term seed on base (DrawTerm/ConsumeTermSeed/RetryTermSeed, O3 policy, no fallback, abortable after 1 day); **the request follows the sealing of every input (jury window)**; cells dealt with the season seed after close |
| WP12 solvency | **Changed** | `outstanding = Σ allocations` at FinishSeason; exact decrements; SweepDust dust-only; the floors-≤-pot proof and fuzz (D §12.2) re-run with the citizen share and the cell pot |
| WP13 preset | Kept | Blitz with 60 s ticks (O25); `window_seconds` per preset from the soak |
| WP14 escape | **Changed** | Barriers (G2) are backstopped by abort; CloseCommons and CloseMember after claims; per-citizen O(1) abort refund; the G4 mismatch goes to the refund path; the operator-loss residual is disclosed (§11.4) |
| WP15 upgrade | **Changed** | `RULES_VERSION` 10 (Commons mode), `CHAIN_LOGIC_VERSION` 2, pinned hashes for both modes; Council realms verify with the v9 arm |
| WP16 verifier | **Extended** | verify-commons (§13) |
| WP17 registration | Kept | Register/JoinCohort checks unchanged; +2 accounts; `Member.cell` written |
| WP18 tests | **Extended** | Coverage pairs for tags 52–69 and errors 49–57; the Commons gate below |

**New units (Phase 3):**
- **CM1** chain Commons instructions;
- **CR** rules v10 (mobilisation, seat changes and credit voiding, outgoing-row area, R6, R7, golden re-pin, simulation tuning);
- **CG** gateway: a published relay for citizens with automatic priority fees, the SealCells/SealWindow/Draw/SeatTerm/LoadCommons/ExportLedger crank loop, and a cell-talk service;
- **CW** citizen web UI (check-in tap, window clock and apply tick, cell view, endorse and jury screens, claim breakdown, relay list);
- **CV** verify-commons;
- **CT** Commons gate;
- then an external **re-audit** of 52–69 and rules v10.

**Commons capacity gate CT (release blocker for Phase 3):**
- (a) The seat apply tick with 48 seat changes + the largest sendable batches + the LoadCommons input + credit voiding at the WP07 caps: ≤ 1.2M CU / 224 KiB per part, no degraded step; world T-bound re-asserted with the 252 B of additions.
- (b) Every base citizen instruction with 50k-scale counter fixtures: ≤ 50k CU, heap ≤ 8 KiB, **serialized tx ≤ 1,232 B with a priority-fee instruction**. Negative tests: **CheckIn and CellPick with another cell's PDA → `WrongCell`**; CellPick outside the endorse window or without a check-in → refused.
- (c) SealCells at 12 cells + 4 members: ≤ 300k CU, tx ≤ 1,232 B; SealWindow refuses until every cell is counted.
- (d) SettleStewards at 72 rows: ≤ 0.2M; a tribune without a jury vote gets 0.
- (e) SeatTerm per call ≤ 100k and tx ≤ 1,232 B; **a missing try account → `TryAccountMissing`, cursor unchanged, no vacancy**.
- (f) Commons Claim within the Light profile (≤ 170k).
- (g) FinishSeason with the G4 digest check from the chunk-0 copies: tx ≤ 1,232 B.
- (h) Recall-bound test: a second recall of an office in a term is refused on base, and the outgoing area and the ledger never fill (G3).
- (i) Barrier and digest tests: a missing snapshot gives `SnapshotPending`, never incumbents or a partial tally; **LoadCommons of a seat window before `seats_ready` → `NotSealed`**; a false crank snapshot **and a false crank seat table** (S6 fallback) each give a void and refund at FinishSeason; **the digests never differ in an honest season with recalls and seat changes**.
- (j) Spikes S1 (lock grief p50/p99 on chunk 0 and on a clone, 1 and 32 accounts per transaction), S6 (read-only base clones of program-owned accounts, freshness after readiness), S6b (does the ER accept v0 + ALT), S7 (realms per ER validator) and S8 (ingress filter / dedicated / self-hosted ER) recorded.
- (k) **Drift and stall:** full seasons at T = 30 s and T = 60 s with degraded steps and VRF retries finish with equal digests; **the ER stalled for 60 ticks, then resumed, finishes the season with equal digests** and no overwritten record.
- (l) **Race:** CloseCommits sent before a late LoadCommons at an apply tick → `SnapshotPending`, then LoadCommons lands, then CloseCommits succeeds; no deadlock in any order.
- (m) **Seed timing:** the DrawTerm request slot is after the SealWindow of every jury window of its term (asserted from logs).

---

## 15. Later phases and their gates

| Phase | Starts | Duration (judges' estimates, not the designs') | Gate |
|---|---|---|---|
| 2 cohort + crank fleet + CloseMember/SweepDust | 10-13 | 3–4 engineer-weeks + review; fleet 1–2 weeks | cohort gate; 20-realm soak; devnet cohort of ≥ 4 realms with ≤ `K_v` realms per validator |
| Spikes S1, S6, S6b, S7, S8 (during Phase 2) | 10-13 | S1 ½ day, S6 1 day (devnet, owner approval), S6b ½ day, S7 2 days, S8 2–3 days incl. talking to MagicBlock | results decide the Commons read path (clone or crank-carried, §6.6), whether ALTs are available, the default realm size, and **which F13 option exists** |
| **F13 in place** | before any public campaign beyond the demo realms | depends on S8 | an ingress-filtered or dedicated ER on which S1 shows that an outside transaction cannot stall our realms |
| 3 Commons realms | about 11-17 | 8–10 engineer-weeks + external re-audit (revision 2 adds SealCells, ExportLedger, the full-season records, the jury quorum and R7) | the CT gate; a local Commons season with ≥ 2,000 synthetic citizens; a devnet Commons realm |
| Later | 2027 | – | banner levy and pool (A §3.5, own solvency proof, off by default); championship finals (A §3.4); parties of ≤ 3; liquid endorsement delegation (depth 1); guild charters; ZK-compressed Member registry |

---

## 16. Owner decisions (recommended default first)

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| F1 | Is "realms under six banners now, shared-map Commons realms of thousands next, behind F13" the answer to 何千〜何万人? | **Yes** | One world with thousands (B/C, rejected on integrity grounds); worlds of 48 only (A alone) |
| F2 | Lift the pause on WP03/06/07/10 at 48 now (amendment A14)? | **Yes, today** | Keep waiting (blocks v9 for no gain; every design keeps them) |
| F3 | Phase 1 placement | **Open realms: players choose; no randomness claimed** | Revision 1's committed-seed router (bypassable and grindable before payment; closing that edits G1 files and adds 3–5 days) |
| F4 | Phase 1 scope cut line | **10-07: if v9 is not green, no devnet multi-realm run; local-only demo, stated in SUBMISSION** | Devnet multi-realm on the HEAD program (none of the 85 fixes; not recommended) |
| F5 | AI per realm | **6 in devnet demo realms; 1 per realm otherwise; ≈ 1 per 4 cells in Commons** | 12 per realm (operator loses 24 USDC per realm) |
| F6 | Tick length | **30 s for the demo; 120 s for mass Council waves; 60 s for Commons realms (window ≈ 13 min)** | 300 s everywhere |
| F7 | Commons realm size | **2,400 (400 per nation)**, chosen for the game only | 1,000 (more seats per person, more realms); 4,600 (larger maps) |
| F8 | Officer selection in Commons | **Endorsement top-3 + tribune jury J4 (quorum 3, plurality, ties by lottery, no quorum → caretaker), jurors paid only if they vote** | Revision 1's rule (ties and no votes → endorsement rank 1: capture 0.33–0.71 at p = 0.1, t = 0.3); plurality only |
| F9 | Tribune source | **Drawn cells' voices (voted once per term); else a lottery member of the cell** | Pure lottery among citizens (D) |
| F10 | Rent | **The operator pays (no SOL for players)** | Refundable SOL deposit in the entry |
| F11 | Dust sweep | **SweepDust: only `vault − outstanding`, any time after finish** | Revision 1's "sweep after 180 days" (which was a forfeiture) |
| F11b | Forfeit unclaimed prizes? | **Never** | After a disclosed deadline (for example 180 days), to the next season's pool of the campaign (never to operations) |
| F12 | Payout tunables | **σ 15 %, `c_term` 0.5 f, person cap 2 f, β 0.25 with 12 counted windows, τ 50 %** | V5 80/20 inside Commons (capturable at scale); β 0.5 (more reward for attendance; scripts reach 1.1 f at the average score) |
| F13 | Ingress-filtered or dedicated ER | **Precondition for any public campaign beyond the demo realms; choose the option after S8** | Public ER with ≤ `K_v` realms per validator only (caps campaigns at a few dozen realms) |
| F14 | Dedicated or paid RPC for the devnet multi-realm demo | **Yes** | Public RPC with 2 realms at a reduced poll rate |
| F15 | R-D13 envoy cap in unit R now | **Yes** | Phase 2 |
| F16 | Banner levy | **Off**; revisit after Phase 3 | 5 % with a per-person cap (A §3.5) |
| F17 | O6 for aborts from Running, now that a third party can cause them for free | **Keep O6 now (players refunded in full, operator bears the loss), disclose it; in Phase 2 move forfeited escrow of Running aborts to a carry-forward pool** | Cap the forfeit at the bond; keep the 20 % when the operator "was not the cause" (rejected: unprovable on chain, and players would lose 20 % on every ER failure) |
| F18 | R7 treasury caps in Commons | **κ = 25 % per term; inter-nation Transfer ≤ R5 allowance per term** | κ = 50 %; forbid inter-nation Transfer entirely |

---

## 17. Player story (Japanese, plain language)

（段階 3 の完成形です。1 万人のシーズンを、1 人の目で見た話です。10 月 12 日の時点では、48 席の評議会レルムがいくつかあり、プレイヤーが自由にレルムを選び、6 つの旗でつながる形です。）

あなたは 10 USDC を払って、6 つの旗の中から「アステル」を選びます。SOL は要りません。登録が締め切られると、乱数で振り分けが決まり、「レルム 3・アステル・第 7 班（31 人。AI が混ざっているかもしれない）」と通知が来ます。どのレルム、どの班に入るかは、あなたも運営も選べません。約 13 分ごとの「窓」に 1 回、スマホで 1 回タップして「出仕」し、生産・研究・防衛・外交のどれかに労働を出します。その結果は、画面に出る「適用ティック」で地図に反映され、国の参加率が上がるほど国が強くなります。班のチャットでは、「今期は研究に寄せよう」と相談し、期ごとの推薦の窓で、班の代表を投票で決めます。役職の候補も推薦し、今期の護民官 4 人が、推薦の上位 3 人から役職者を選びます（3 票以上が集まらなければ、その期は代行です）。ある期、あなたの班がくじで選ばれ、代表のあなたが護民官として「卓」に座ります。その間、献策と支持をし、国庫の大きな支出に同意するかどうかを決め、次の役職者を選ぶ陪審で投票します。怠けている将軍がいれば、国民の請願で解任でき、その期の残りは代行になります。シーズンが終わると、1 回の受け取りで「国民の取り分（出仕が多いほど最大 1.25 倍）＋護民官の功績（半分は班に）」が戻り、どこから来たお金なのかも表示されます。最後に AI の一覧が公開され、「うちの班のあの人は AI だったのか」と分かり、レルムの結果は旗のランキングにも加わります。

---

## Revision notes

Revision 2, 2026-09-27, after the stress test. Every issue was checked against HEAD `96a3464` and the contract; new measurements are in `scale/lab/revise/`.

| # | Stress issue (severity) | Verdict | Evidence | Change |
|---|---|---|---|---|
| 1 | Free ER write-lock hits every realm on the validator; aborts cost the operator (blocker) | **Accepted**, with one correction | 21 chunk-0s serialize to 863 B, 32 to 1,226 B, 33 to 1,259 B (over) [measured, txsize.mjs, txsize2.mjs]; with v0 + ALT, 32 fit in 270 B. WP14 §3.2/§3.3: Running abort needs `now ≥ running_deadline ∧ !finishable`; Claim pays `fee + shares + ⌊escrow/N⌋`; WithdrawOps pays nothing of the 20 %. `running_deadline` ≈ 34.2 h at 60 s ticks [derived]. **Correction:** the operator's 20 % is revenue never received (fees are refunded), not money transferred to members; the transfer is the escrow, 1.5–4 % of f per member (§11.4). The "≈ 40 ticks via the ring bug" path is gone with G5. | §0.9, §2 R2, §6.4, §9.4, §11.4 (new), §12.2 ("protocol-bounded"), §12.3, §14.1 DOCS, §15 (F13 gate, S8), F13, F17, 要約. "Keep ops unless the operator caused the stall" rejected as unprovable (§11.4). |
| 2 | LoadCommons before SeatTerm ×9; seats outside the digest (blocker) | **Accepted** | §5.3 rev 1: LoadCommons required only `sealed`; SeatTerm folded nothing. | `seats_ready[m]` set by the 9th SeatTerm; window 3m − 1 becomes ready only then and the fold covers the seats (G4); LoadCommons requires `ready`; SeatTerm refuses on a missing account (`TryAccountMissing`); CT(i) tests. |
| 3 | 4-deep rings and a single `next_seats` break "windows queue" (major) | **Accepted** | §4.3 rev 1: `ring[4]`, `next_seats[8]`. | G5: Commons `win[18]`, `seats[6][8]` (Commons grows to 4 KiB, +0.06 SOL per realm); Cell `win[18]`; reads by index; CT(k) stall test. |
| 4 | Office capture much cheaper; "capture never pays" unsupported; recall, treasury, seed timing (major) | **Accepted** | Re-run with the rule as written: 0.33–0.71 at p = 0.1, t = 0.3 [measured, stress jury.py; reproduced by capture2.py]; `synth.py:88` hard-codes cost. WP10 R5: consent lifts the per-term limit without a cap. | Jury J4 with quorum and paid-only-if-voting; DrawTerm after the jury window; recall vacates, measured over the term's distinct actives with an n/8 floor; R7 treasury caps; §7.4 rewritten with measured tables; "never pays" withdrawn; low-turnout capture stated as a residual. |
| 5 | Phase 1 router bypassable and grindable (major) | **Accepted** | `app.mjs:42-46` PUBLIC_ROUTES includes `POST /x402/join`; `routes/x402.mjs:139-170` answers the 402 with `seasonId` before payment; `play/mod.rs:163-174` requires the proxy to use the public listener; `guards.mjs:52` limits joins to 0.2/s per IP. | Randomized placement dropped from Phase 1 (open realms, F3); X1 is a directory, X5 removed; placement nobody chooses arrives with Phase 2. |
| 6 | Cell membership not bound on chain (major) | **Accepted** | Member tail rev 1 had no `cell`; CheckIn/CellPick took no Season. | `Member.cell` bound at Admit/RegisterCitizen; Cell PDA checked (`WrongCell`); CheckIn reads Season for the window clock; picks only by members who checked in during the endorse window; CT(b) negative tests. |
| 7 | Operator relay is the only path; pot is zero-sum (major) | **Accepted** | Citizen instructions check the session signature, not the fee payer (§5.3), so other relays are possible but rev 1 never said so. | Anyone may relay; direct send with own SOL; published relay code (§8.4); attendance counted to 12 of 18 windows and worth at most 25 % (§11.3); residual with its ≈ 10 % motive (§12.3). |
| 8 | Wall-clock windows vs drifting ER ticks (blocker) | **Accepted** | `processor/play.rs:402` sets `deadline = now + tick_seconds` after each tick; `play.rs:174-178` adds `reveal_seconds` = T/6 (`state.rs:386`). | Option (b), strengthened: windows of 10 · T_eff, full-season records (G5), the barrier paces the Table so windows cannot outrun it; apply tick for window 17 = 179; §8.2 and §17 now say wall-clock windows; CT(k) drift test at 30 s and 60 s. |
| 9 | G2 barrier can deadlock with CloseCommits (major) | **Accepted** | Rev 1: LoadCommons needed `!revealing`; barrier in FreezeTick, which only acts while revealing. | Barrier moved to CloseCommits (kept in FreezeTick as defence); CT(l) race test. |
| 10 | Term seed revealed before tribune eligibility is fixed (major) | **Accepted** | Rev 1 DrawTerm needed only the endorse windows. | DrawTerm after the jury windows seal; `caller_seed` covers those records; CT(m) and verify-commons check the order. |
| 11 | Phase 1 scope misses the join path; keys; season ids; timeline (major) | **Partly accepted, partly rebutted** | **Rebutted:** per-realm key directories are not needed. The gateway already runs "several seasons side by side" from one checkout (`server.mjs:10-11`, `--port P --state file.json`); revision 1's "one crank key per realm" was the design's own over-requirement. **Accepted:** `season.mjs:207` uses `BigInt(now())`, so simultaneous starts collide; `codex/v9-security` has only wave 1 merged; HEAD has no O14. With open realms the join path is each realm's existing lobby, so no X6 is needed. | Launcher staggers and awaits starts; X2 rework budgeted on 10-08; cut line: no devnet multi-realm on the HEAD program (F4); §9.3 states the branch status. |
| 12 | The document overstates what is proven (major) | **Accepted** | WP07 §7.7 and §834: 1,136k CU and 191 KiB are lab-prototype measurements; contract C1 and gate 4 put the joint gate in wave 4. | 要約, §0.2, §12.1, A14 reworded; Commons ResolveTick marked uncertain until CT(a); the 64k margin stated (§6.5). |
| 13 | Several Phase 3 transactions exceed 1,232 B (major) | **Accepted** | [measured, txsize2]: ResolveTick + 6 ledgers 1,343 B; FinishSeason + 6 Nations 1,325 B; SeatTerm 4 × 4 tries 1,426 B; SealWindow 24 cells with a priority fee 1,211 B (rev 1 said 1,080). | ExportLedger (ResolveTick keeps v9's 1,145 B); Nation digests copied into chunk 0 (FinishSeason 1,127 B); SeatTerm one cell try per call (535 B); SealCells ≤ 12 cells (767 B); ALTs optional pending S6b; size checks in CT(b), (c), (e), (g). |
| 14 | G4 underspecified: window 17 and seats (major) | **Accepted** | Ticks 0 … 179; rev 1 applied window j at 10(j + 1) + 1, so window 17 had no tick. | Apply tick `a_17 = 179`; both digests cover windows 0 … 17 with seats folded at readiness; verify-commons checks both; CT(i) false-seat-table case. |
| 15 | Economics reward scripted attendance; AI shares; `cell_active` (major) | **Accepted** | Rev 1 formula: ≈ 23 USDC back per 10 USDC fee for an always-on script at t = 0.3 (stress figure; rev 1 payout has no per-person cap on service). | Citizen share with units `1 + 0.25 · min(w, 12)/12`: a check-in-only wallet gets < 1.0 f at average score (≤ 0.93 f at n = 400, 8.2–8.7 USDC in the simulated turnouts) [derived, capture2.py]; AI rows and shares voided to the people pot (R4's intent by construction); `cell_active` and AI handling defined (§10, §11.3). |
| 16 | Dust sweep is a forfeiture of unclaimed winnings (major) | **Accepted** | Rev 1: SweepDust "after the claim deadline" + CloseMember "after the claim window" + conservation with `swept`. | SweepDust moves only `vault − outstanding`; CloseMember only after the claim; ForfeitUnclaimed is a separate, off-by-default owner decision (F11b) whose destination is never operations. |

What did not change: the architecture choice (A + D + C cells, B rejected), R1 (48 seats per world), G1 (citizens on base), Phase 2's cohort, and the per-action costs. The schedule for Phase 3 grows by about a week (8–10 weeks), and the Phase 1 scale track shrinks by about 2 days.

---

## Links

- This design: `(session scratch)/scratchpad/scale/scale-design.md`
- Revision 1 (kept for comparison): `.../scratchpad/scale/lab/revise/scale-design.rev1.md`
- Revision 2 labs: `.../scratchpad/scale/lab/revise/{capture2.py, capture2-results.txt, txsize2.mjs, txsize2-results.txt, rev2-numbers.py, rev2-numbers.txt}`. Run `python3 capture2.py` (≈ 20 s), `node txsize2.mjs` (uses the gateway's `@solana/web3.js`), `python3 rev2-numbers.py`.
- Stress test labs: `.../scratchpad/scale/lab/stress/{jury.py, txsize.mjs}`
- Revision 1 lab: `.../scratchpad/scale/lab/synth/synth.py` and `synth-results.txt` (its jury section is superseded).
- Designs: `.../scratchpad/scale/design/{A-parallel-worlds,B-one-world-externalized-members,C-representation-tiers,D-free-hybrid}.md`
- Judges: `.../scratchpad/scale/judge/{limits-security,game-player,feasibility-deadline}.md`
- Measured inputs:
  - `.../scale/lab/free-hybrid/sbf-results.txt` (CheckIn, Endorse, ClaimCalc, Sortition, StewardSettle);
  - `.../scale/lab/A-parallel-worlds/{assign,costs}-results.txt`;
  - `.../fix/design/WP07-world-heap-capacity.md` §7.5–§7.7;
  - `.../fix/design/WP14-escape-hatches.md` §3.2–§3.3 (abort rules and money);
  - `.../fix/design/WP10-treasury-governance.md` §3 (R1–R5);
  - `.../scale/lab/B-one-world/` (closed-form claim equivalence).
- Contract: `.../scratchpad/fix/contract.md` (§0, §1.1–§1.8, C1, C9, §3 waves, §4.3, §5.3, §6 O6/O14), `.../scratchpad/fix/contract-amendments.md`.
- Code checked (read-only):
  - `permutation-chain/src/processor/play.rs:157-178, 298, 402` (commit deadline, reveal window, next deadline = now + T);
  - `permutation-chain/src/state.rs:386` (`reveal_seconds`);
  - `permutation-chain/src/instruction.rs:85-100` (ResolveTick and FinishSeason accounts);
  - `permutation-gateway/src/app.mjs:42-55` (PUBLIC_ROUTES, COSIGN_ROUTES);
  - `permutation-gateway/src/routes/x402.mjs:139-200` (402 answer reveals the season before payment);
  - `permutation-gateway/src/guards.mjs:52` (join rate limit);
  - `permutation-gateway/src/server.mjs:10-11` (several seasons side by side);
  - `permutation-gateway/src/config.mjs:14-16, 237` (KEYS_DIR);
  - `permutation-gateway/src/season.mjs:199-207` (season id = time);
  - `permutation-server/src/play/mod.rs:163-174`, `play/proxy.rs` (`/gw` proxy to the public listener).
- Public docs: [MagicBlock validator (BSL 1.1)](https://github.com/magicblock-labs/magicblock-validator/), [Private Ephemeral Rollups](https://www.magicblock.xyz/blog/private-ephemeral-rollup), [PER quickstart](https://docs.magicblock.gg/pages/private-ephemeral-rollups-pers/how-to-guide/quickstart).
