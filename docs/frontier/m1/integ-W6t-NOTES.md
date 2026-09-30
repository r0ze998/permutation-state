# integ-W6t: the w6-s7 triage merge (U1–U5), Gate W6 re-run, targeted runs R2/R3/R5

Integration window of the `w6-s7` triage (`scratchpad/frontier/m1/triage/PLAN.md`; contract v1.12 §28; DECISIONS part S). Base `frontier/m1-integ` = `codex/frontier` `7dcacdf` (the tree `w6-s7` ran on, release `.so` `072b1205…a98b`). The paused `w6-s7` stack (41000–41099) was only read; every service this pass started used 41100–41999 and was stopped. No push, no install, no download, no devnet or mainnet transaction; `permutation-server/web/session.mjs` unchanged (`git diff --quiet d95fa25` exit 0).

## 1. Merge (PLAN §1 order, `--no-ff`)

| # | unit | branch head | merge | paths |
|---|---|---|---|---|
| 1 | U1 W6-B program-fix (W6T-1) | `3676f1a` | `eb39ad3` | `permutation-frontier/**`, `frontier-abi/**` |
| 2 | U2 W6-C keeper (W6T-2) | `f82d57e` | `fbdd720` | `frontier-node/crates/keeper/**` |
| 3 | U3 W6-C verifier, agents, bots, viewers, localnet, relay (W6T-3) | `176138c` | `ef9b3e0` | the other node crates, fixtures, `permutation-gateway/src/frontier`, its tests |
| 4 | U4 W6-A season-ops (W6T-4) | `99aae98` | `a172783` | `frontier-node/crates/stack/**`, `configs/**`, `scripts/m1-run-s7.sh`, `runs/**` |
| 5 | U5 W6-E docs (W6T-5) | `5cb49f1` | `548a494` | `docs/frontier/**` |

The five path sets are disjoint; every merge was clean. No manifest or lockfile changed in any unit (`git diff 7dcacdf -- '*Cargo.toml' '*Cargo.lock' permutation-gateway/package*.json` empty), so there was no integrator-owned manifest or lock change to apply.

## 2. Integration-window commits

| commit | what | why |
|---|---|---|
| `aeda792` | `node permutation-gateway/scripts/sync-web-sdk.mjs` (its producer): `client/src/frontier/abi-budgets.mjs` and `web/sdk/frontier/abi-budgets.mjs` re-embed `budgets.json` (CloseArrivalDay/Slot `cu_limit` 8,000). Contract §3.2, §10.2, §28: U1's values (release `d85e1bd7…2281` 875,824 B re-built `--twice` on the merged tree, test-beacon `b2cef4a7…a4e7`, worst close 6,018 / 6,538). Docs aligned with the merged code: keeper refusal bodies are `{error, code, detail}` (U2 shipped both fields), the relay's pre-check `400 ArrivalBell {endBell}`, the CU ladder (2×, then 1.4M, from the program feed), `Spend`/`Leave` stay skip-split targets (U2 deviation 2), `unrevealed_by_rule` keys `host_id`/`arrive_bell` plus `unrevealed_by_reason`; `w6t_docs_check.py` follows the shipped body | U1's request; U5 §4's checklist. `w6t_docs_check.py --final`: 0 fail, 0 pending |
| `6512cda` | **T17 and V7.** `tamper.rs` `t17()` builds its forged skip on a skip whose end bell's Reveal precedes it (W4-D's construction) when the run has one; `v7_replay.rs` collects every REVEAL in a pre-pass and flags `SkipOverArrival` for a SKIP over a bell whose arrival is revealed anywhere in the run | U4's finding 1: T17 missed in 2 of 3 preview runs (tamper 29/30). With U2's early skips the skip ending at an arrival bell lands before that bell's Reveal; `t17()` took the first skip and V7 knew only the REVEALs before the SKIP record, so the forgery was caught only as `ClashReplayMismatch` + V4 `RevealAfterLatch`. The program refuses a Reveal once the Province is resolved past its arrival bell (`LatchClosed`), so no honest run has a REVEAL after a SKIP over its bell: the pre-pass has no false positive. On U4's three preview inputs: base PASS, T17 detected with `SkipOverArrival`, 30/30 |
| `36386b0` | `cargo fmt --all` over frontier-node and three clippy lints (a `MarchSetup` type alias in `herald_fixtures.rs`, `let … else` as `?` in `v5_seals.rs`, a string compare without an owned temporary in `verify/tests/fixtures.rs`) | Gate W1's frontier-node line failed on U3's files: U3 had no rustfmt/clippy in its toolchain (W6T-3 §10). No behaviour change |
| (this) | contract §28 integ-W6t rows, V7/T17 text in §8.5, O-M1-28/29 in §15; DECISIONS S22 updated, S23–S25, change log; run records `runs/integ-w6t-*`; these notes | |

## 3. Gate W6 re-run (contract §12 as written, cumulative W1–W5 + the W6 lines except `w6-s7`)

Records: scratch `integ-w6t/gate/{A,L,R3,N,W5,O,R5}-run.txt` and `gate/logs/`; committed summaries under `runs/integ-w6t-*`. `M1_PORTS` empty for the port-free items; `frontier-stack check-ports` for each stack base. `PENDING_OWNER` empty (wasm32 target and Playwright Chromium installed).

**Part A (port-free), 05:49–06:26 JST on `6512cda`/`36386b0`** (load average 1 min in brackets; the frontier-sim tests took it to 75):

| line | result |
|---|---|
| W1 root: fmt, clippy rules+abi, `test --release -p permutation-rules` (437), `-p frontier-abi` (47), `abi-vectors --check`, `-p permutation-chain` (157) | all 0 |
| W1 frontier-sim fmt + clippy + test (320 s); `criterion --best-response --seeds 3 --first-seed 30001 --gate` (worst bot choice **0.985**, as W6-B/W6T-1); `doctrine-gate --controls` (302 s; Knight −0.242 %, A boost +0.326 % rejected as they must be) | 0 / 0 / 0 |
| W1 frontier-node fmt + clippy `--workspace` + `cargo test --workspace` | **exit 1 on `aeda792`/`6512cda`** (rustfmt diffs and three clippy lints in U3's files) → fixed in `36386b0` → **re-run exit 0** (267 s, 402 passed) |
| W1 gateway `npm test` (529/529); civilization tests; `git diff --quiet d95fa25 -- session.mjs permutation-chain/src` | 0 / 0 / 0 |
| W2 clippy + `--no-default-features` test of permutation-frontier; `build-frontier.sh --twice` (release **`d85e1bd7…2281`**, 875,824 B, both builds identical; deployable); svm `g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_` (92); frontier-node `--release` tests (402); `npm test && sync-web-sdk.mjs --check`; `build-wasm.sh --check` | all 0 |
| W3 svm `g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_` (102, incl. U1's `host_depart_arrival_at_or_after_end_bell_refused`); `PSF_TRACE=1` five worst cases | 0 / 0 |
| W4 svm full (248 passed incl. `g01_close_arrival_{day,slot}_ended_paths`); `--include-ignored inproc_ lag_gate crash_injection` (330 s); keeper `--test play` on the test-beacon `.so` (234 s); verify `tamper_` (58) | all 0 |
| W5 `RELEASE_CHECK=1` svm full (248, no G13 pending); `cargo build --release --workspace && crates/verify/mutate.sh` (every `mutate-<check>` build lets its tampers PASS; run with `CARGO_TARGET_DIR` in scratch so the stacks' `frontier-verify` was never a mutated one); itest `g14_` (379 s); screens `node --test *.screen.mjs` (51/51) | all 0 |
| U5's `w6t_docs_check.py --final` (not a §12 line) | 0 (182 pass after this commit) |

`npm ci` was **not** run before `npm test` (the §12 W1 line has it): the lockfile is unchanged since the last `npm ci` (integ-W6r), the live `w6-s7` relay (pid 11554) runs from this worktree's `permutation-gateway`, and a reinstall is an install. Same for the screens' `npm ci`.

**Stack lines** (base ports moved off 41000, which the paused `w6-s7` holds):

| line | base | result |
|---|---|---|
| W5 `check-ports`; `up --scale 100 --days 1 --bots 100 --run-id integ-w6t-w5-smoke`; `verify && tamper`; `load --viewers 5000 --game-hours 1`; `down` | 41600 | all 0: verify PASS (7,206 txs, 0.2 + 0.5 s), tamper base PASS, 27 classes on the run + 3 on fixtures all detected; load p99 file 20.5 ms, ingest → WS p99 0.14 s, error rate 0, WS coverage 100 % |
| W6 `scripts/m1-nightly.sh --no-build` ×3 (`integ-w6t-nightly-1..3`, 06:31–07:27) | 41500 | **three consecutive nights green** (`pass: true`): verify PASS (8,485 / 8,725 / 8,416 txs), tamper **30/30** each, load pass (p99 file 32.8 / 25.6 / 31.7 ms, ingest → WS 0.28 / 0.31 / 0.31 s, error 0, WS coverage 100 %), report 0 (criteria 1, 2, 4, 5, 8, 9 pass; 3, 6, 7 n.a. at 100×), down 0. Failed transactions 98 / 103 / 64 (integ-W6r's nightlies ≈ 200). Row E not exit-grade (test key; `frontier-fund` skipped as U4 expects for a 1-day 100-bot night; night 3 also `defence-pool`) |
| W6 latency `up --scale 2 --game-hours 6 --bots 300 --chaos --run-id integ-w6t-latency` (+ extra `verify`); `report && down` | 41100 | 0 (05:48–09:13): criterion 3 **pass**, round → anchor 1 s, S → first cache 1 s, anchor → last reveal 1 s, S → resolve 3 s (targets 5 / 5 / 30 / 60); idle days p99 2; verify PASS |
| W6 scripted onboarding `run-onboarding.sh --base-port 41700` | 41700 | 0: JA ok (336 s), EN ok (361 s) |
| W6 `w6-s7` line + `verify && tamper && report` | – | **not run** here (the 7-day exit re-run, §6) |

Gate W6 pass conditions beyond exit codes: the latency run meets criterion 3's game-second targets — yes; the c4 v3 report cites the Reveal CU distribution and `L(reveal)` — unchanged from integ-W6r (the `w6-s7` sample fold is still open, S22); scripted onboarding green in JA and EN — yes; real rounds not pending — yes (R5 ran on the archive with the pin). Earlier gates' conditions re-checked: Phase B digests and `RULESET_HASH` `72c6b583…54bd9` unchanged (U1 touched no kernel file; the rules tests pass), G1–G14 with `RELEASE_CHECK=1`, verify PASS and tamper all detected on the smoke, every `mutate-<check>` build lets its tamper PASS, herald load within targets on the smoke and nightlies.

## 4. Targeted runs (PLAN §3), before / after

| run | before | after (merged tree) | pass? |
|---|---|---|---|
| R1 (U1) | 7dcacdf `.so`: the host test lands the Depart; the close tests run out of CUs at 6,000 / 6,500 (W6T-1 §2.2) | Gate part A: svm full ×2 (248 each) incl. the three new tests, `build-frontier.sh --twice` `d85e1bd7…2281` = W6T-1's record | yes |
| R2 (U2) late-season replica, keeper A, 1,500 slots, kill -9 at +700 (41610/41611/41620/41650) | 7dcacdf keeper: p99 9 / 6 / 14 slots, 47 % of slots ticked, first tick 17.9 s, 471 of 544 status samples null (W6T-2 §2) | **merged keeper binary** (`acef8c1a…`; it embeds the new budgets.json): round → anchor p99 **1**, S → first cache **1**, S → resolve **2**; closes phase p99 13 ms, max 69 ms (the passes right after a start), ≤ 62 reads; every slot 70400–71714 ticked (1,314 gaps of 1); first tick after kill -9 **303 ms**; 547 status samples, 0 null, 0 over 1 s (2 refused connects: before the first start and at the kill); load 2.4–4.7 | yes |
| R3 `--scale 100 --days 2 --season-end-at-play-end --bots 300 --chaos --adversary` (41200) | w6-s7: c1 fail, verify FAIL; U4 tree: 3,176 anchor BadData, 7,214 close failures, ≈ 100 % A/B settle duplicates; preview: tamper 29/30 | **all R3 conditions met** (`runs/integ-w6t-tri-end/run.md`): c1 pass (end_bell 288, EndSeason, 82/82 settled once), verify PASS, tamper **30/30**, 0 DEPART ≥ end_bell, 0 anchor BadData, 0 close failures, 0 WrongStatus, 0 Shielded, c4 = 0, c5 pass, A/B settle duplicates 0.6 %; `NotResident` 20 = the U4 tree's at the same scale. `defence-pool` was hold-skipped (no claim opened) | yes (R3's list); see the hold risk below |
| R4 (U3) viewer generator vs a restarting herald | 7dcacdf generator: 9,335 errors | not re-run (the generator source is U3's, only reformatted); U3's result stands: 0 errors, 8,000 stale retries, 2,000 reconnects. The in-run window of R5 confirms it at scale: 1 error in 1.73 M, 12,000 stale retries and 3,000 reconnects all inside the kill windows | yes |
| R5 `--beacon archive --scale 20 --days 1 --season-end-at-play-end --bots 1000 --chaos --adversary --viewers 5000 --viewer-window-hours 12 --chaos-force herald:2 --chaos-force herald:7 --expect-so-sha256 d85e1bd7…` (41300) | w6-s7 (7 days): c1, c3, c4, c6 fail; U4 tree: c3 round → anchor p99 45, c6 error 0.76 %, 35,643 failed txs | `runs/integ-w6t-tri-20x/run.md`: **exit-grade environment (all 9 holds fired)**, criteria 1, 2, 4, 5, 8, 9 pass, verify PASS, tamper 30/30, keeper status answered every bell, 436 failed txs (0 waste but 2). **Criterion 3 fails**: round → anchor p99 81 (bells 8–10's stacked-hold stall: 48 of 2,304 anchors), anchor → last reveal p99 38 (n 42: the one Reveal `slots-below` delays by design); S → cache p99 1, S → resolve p99 3, idle days 0 meet their targets. **Criterion 6 fails on ingest → WS p99 2.49 s** (error rate, WS coverage, recovery, file p99 all met) | **no** (c3, c6) |
| R6 regression | – | §3: every Gate W1–W5 line and the W6 nightly/latency/onboarding lines exit 0 | yes |

The `w6-s7` input itself under the merged verifier (`runs/integ-w6t-w6s7-reverify/`): CampMismatch 2 → 0, ValidSealUnrevealed 27 → 0 (36 by rule: 27 `shielded-own`, 9 `bounced`), MissingData 9 → 9, new `ArrivalAfterEnd` 9 (FAIL, correct for this pre-U1 input); tamper 29/29 → 30/30 (base FAIL on ArrivalAfterEnd).

**What R5 shows (for the owner and the exit run):**

1. **Criterion 3 in a 1-day sample.** Both misses are adversary windows. (a) The `ticket` hold (1.0) and two `ticket_holder` persona holds (0.5 = the keepers' D cap; ties lose to a hold) fill the 100M block from slot 813 to 1027: bells 8, 9 and 10 anchored at slots 1028, 1030, 1035 instead of 875, 950, 1025 (keeper A's journal); every other bell's 16 anchors landed in the bell's first anchor slot except one region the above-cap `anchor` hold delayed 25 slots. The same stall is in every 20× run (w6-s7 827–1028, U4 tree, preview). Over 7 days it is 48 of 16,128 anchors (0.3 %), below the p99: w6-s7 had max 154 and p99 6 (the 6 was the keeper causes R2 removes). (b) One Reveal of 42 was delayed 38 slots by the below-cap `slots-below` hold until the keepers escalated past it; it opened the claim `defence-pool` then held. Criterion 4 excludes above-cap windows, criterion 3 none; over 7 days (w6-s7: 1,628 Reveals, p99 0) one such Reveal is not the p99. Owner question O-M1-29 (default: unchanged).
2. **Criterion 6's ingest → WS tail is burst fan-out.** A 30-s sampler of the generator and herald (scratch `gate/R5-samples.txt`): ≈ 100,000 WS messages per 30 s to 1,000 all-ring sockets, p99 **0.44 s** through bell 24; at bell 25, when the `ticket` hold ended (slot 2074) and ≈ 100 transactions landed in slots 2075–2099, **1.14 M** messages arrived in one 30-s interval and p99 became 2.36 s; three more bursts of 0.5–1.1 M followed (after the herald restart at 1790722376 and at bells ≈ 57 and ≈ 72); final p99 2.49 s. The ingest stamp is taken at the pull, so this is the fan-out of ≈ 1,100 messages per socket at once to 1,000 sockets in one generator process, not ingest. The herald's fold lag also reached 20–25 s three times without a kill (its checkpoint every 150 slots is awaited inside the ingest loop, `runner.rs` `Ingest::step`); those stalls did not move the WS p99 but are worth a herald look. The preview R5 had 2.36 s for the same reason; earlier figures covered ≤ 17 % of the window. Owner question O-M1-28 (default: judged as written).
3. **Hold coverage is not guaranteed.** R5 had 9/9, but R3 and nightly 3 skipped `defence-pool`: `slots-below` delayed no Reveal enough to open a defence claim. The 7-day run has one `slots-below` (armed by 60 % of play); if it opens no claim, row E says "not exit-grade". A stack change (re-arm `slots-below` until a claim opens) would remove the risk; it is W6-A's (not done here).
4. Smaller, reported: Reveal `AlreadyDone` 109 against 42 landed in R5 (27–48 per nightly; integ-W6r nightlies 15–44): keeper B's `backup_delay_slots` covers only the settles (U4 finding 5). Four `CloseArrivalSlot` `BadAccount` in R5: keeper A's versions of `close:slot:1,-3:44:4` queued behind the above-cap `keeper-payers` hold (4100–4174) all landed at 4175 after the slot was closed; the report leaves them unclassified, as it does 1–2 `Reveal TransitState` per run. With `--follow-status` 44 % of the viewer requests are 404s (the live bells' not-yet shapes, as the web client's), not errors.

## 5. The 37,042 failed transactions of w6-s7

As classified by the triage and U4's class table on the `w6-s7` input (DECISIONS S21): waste 33,121 — keeper 32,873 (closes out of CUs in the drain 28,655: CloseArrivalDay 25,655 at `cu_limit` 6,000, CloseArrivalSlot 3,000 at 6,500, each key re-planned ≈ 270 times as a heap fault; anchors ≥ `end_bell` 4,218: PostAnchor 3,552 + PostAnchorMulti 666 BadData), bot policy 160, bot bugs 88 (61 Reveal `Shielded`, 27 `WrongStatus`); redundancy 3,809 (A/B settle races 3,570, duplicates 239); expected 112 (personas, adversary, drain rules). After the merge the waste classes are gone at the same lines: R3 62 failed (24 bot-policy waste, 0 keeper waste), R5 436 (2 waste), nightlies 64–103; what remains is A/B redundancy (PostBeacon and Reveal `AlreadyDone` above all).

## 6. The 7-day exit run (§13.4, W7)

**Ready to start mechanically; not expected to pass as the criteria stand.** The build, pin, archive, scripts and configs are ready (R5 ran the same release `.so`, archive and flags at one day and was exit-grade), and criteria 1, 2, 4, 5, 8, 9, verify and tamper passed on every run of the merged tree. But R5 fails criterion 6's ingest → WS p99 for a reason the 7-day run shares (a burst of ≈ 100 transactions in one slot inside the viewer window: a hold's release, a ring opening, a herald restart's catch-up), and the one `slots-below` may open no claim (not exit-grade). Criterion 3 is expected to pass over 7 days (the hold windows fall below the p99, as in w6-s7) but that is an inference, not a measurement. Decide O-M1-28 (and O-M1-29) first, or start it knowing criterion 6 is at risk.

The command (PLAN §5 and §9 step 4; no flag changes: the archive stays `drand-archive-quicknet-g0-1788998400`, G0 1788998400, which covers g0 … g0 + 738,000 s against the timeline's g0 + 710,460 s; the drain stays 26 bells at 20×; keeper B gets `backup_delay_slots = 8` and the viewers the A3 flags from the stack's defaults; the new run id keeps `w6-s7` as evidence):

```sh
cd /Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/m1-integ
# 1. the w6-s7 evidence is kept (run directory, triage scratch, runs/w6-s7); stop the paused stack (frees 41000-41099)
frontier-node/target/release/frontier-stack down --run-id w6-s7
# 2. build, pin and check: expect ".so pin d85e1bd7…2281; W6T-1 recorded release d85e1bd7…2281: match"
scripts/m1-run-s7.sh --dry-run --adversary --run-id w6-s7b
# 3. the run (≈ 8.4 h of play + the drain, under caffeinate), then verify, tamper, report; services stay up for triage
scripts/m1-run-s7.sh --adversary --run-id w6-s7b
```

Then check (PLAN §5, §13.4 as amended): row E "exit-grade" (all nine holds fired); verify PASS; tamper 30/30; criteria 1–6, 8, 9; keeper A `/v1/status` `pools.reveal.floor` 215,076,060; the no-landing detector and the per-bell load average. Nothing else heavy on the machine.

## 7. Fast-forward

`codex/frontier` fast-forwarded to the head of `frontier/m1-integ` (this commit) with `git -C .claude/worktrees/frontier-integ merge --ff-only frontier/m1-integ`, on the Gate lines being green (the plan's merge condition, R6). R5's two criterion failures are reported above, not hidden; they concern the exit run's criteria, not the merge.

## Links

- Plan and triage write-ups: `scratchpad/frontier/m1/triage/PLAN.md`, `unsettled-transits.md`, `camp-mismatch.md`, `latency.md`, `unrevealed-seals.md`, `herald-errors.md`
- Unit notes: [`W6T-1-NOTES.md`](W6T-1-NOTES.md), [`W6T-2-NOTES.md`](W6T-2-NOTES.md), [`W6T-3-NOTES.md`](W6T-3-NOTES.md), [`W6T-4-NOTES.md`](W6T-4-NOTES.md), [`W6T-5-NOTES.md`](W6T-5-NOTES.md)
- Contract v1.12: [`M1-CONTRACT.md`](M1-CONTRACT.md) §28 (integ-W6t rows), §15 O-M1-28/29; decisions [`../DECISIONS.md`](../DECISIONS.md) part S (S22–S25)
- Runs: `runs/integ-w6t-tri-end/`, `runs/integ-w6t-tri-20x/`, `runs/integ-w6t-latency/`, `runs/integ-w6t-nightly-{1,2,3}/`, `runs/integ-w6t-w5-smoke/`, `runs/integ-w6t-onboarding/`, `runs/integ-w6t-w6s7-reverify/`
- Previous passes: [`integ-W6-NOTES.md`](integ-W6-NOTES.md), [`integ-W6r-NOTES.md`](integ-W6r-NOTES.md)
