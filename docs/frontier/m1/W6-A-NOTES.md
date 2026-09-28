# W6-A season-ops — notes (wave 6, pass 1)

- **Unit:** W6-A (wave 6, pass 1: prep and known fixes), branch `frontier/m1-W6-A` cut from `frontier/m1-integ` at `0514b06` (contract v1.9).
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.9 — §3, §8.5, §8.7, §10.3, §11 (W6-A), §12 (Gate W6), §13.4 criteria 1–9, §13.5, §25; DECISIONS O11, N11 (W5-B F5), part A (O-M1-12 items 1–3 approved 2026-09-28; Agave ≥ 4.0 not approved).
- **Wave note (pass 1):** nobody in this pass runs the 7-day w6-s7 season. W6-A: make the real-round path work end to end (archive smoke of about one game day at a high scale, fix what breaks), the O11/W5-B reporting items, `--expect-so-sha256`, the nightly on the test key (three consecutive green if time allows), the scale-2 latency run, and `scripts/m1-run-s7.sh` (written, **not started**).
- **Owned paths touched:** `frontier-node/crates/stack/**`, `frontier-node/configs/**`, `docs/frontier/m1/runs/**`, this file. Also, by the wave note: `scripts/m1-run-s7.sh` (new) and `scripts/m1-nightly.sh` (`--run-id`, a 6-line change; W5-B's file, no wave-6 owner). **Cross-ownership (hand-over, commit `acee4c7`):** `frontier-node/crates/verify/src/tamper.rs` T10 (W6-C's path in wave 6), because it blocked the real-round path — see §3 F-A1.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no install; no drand fetch (the archive was only read); no manifest, lockfile, toolchain or `.gitignore` change (no dependency request); `session.mjs`, the main tree, `codex/magicblock-playable`, `codex/v9-security` untouched; the 7-day w6-s7 run **not started**. Services only on 41500–41599 (nightlies), 41600–41699 (archive smoke) and 41700–41799 (latency run) — other W6 units held 41300–41399 (W6-D's stack at 41310/41320 was up when this unit started), so this unit kept off 41000–41399; every stack stopped with `down` (checked with `lsof`).
- **Machine load during the runs:** load average 35–66 on 16 cores (W6-B's `frontier-sim` doctrine and criterion runs and rustc builds of the other units). The latencies below were measured under that load.

## 1. What landed

### Report (`frontier-stack report`, `crates/stack/src/report.rs`)

| Item | What | Source |
|---|---|---|
| ClashInputs closability (criterion 1) | From the final accounts: every open ClashInputs is **closable after grace** (resolved with every *present* record settled; or `FLAG_NO_ARRIVALS` with `resolved_next > bell`, the program's CloseClashInputs rules a/b), **pending** (unresolved, the Province still owes the bell, within `close + 2` bells of the run's last bell), or **blocked** (a present record never settled; a bell passed without resolve or no-arrival skip; unresolved past `close + 2`). CLOSE records of ClashInputs counted as closed. Any blocked one fails criterion 1 | DECISIONS O11 |
| Idle vs churned catch-up (criterion 3) | SkipQuiet (SKIP records) per province-day, split by the Province's **`roster_epoch`** read from the transactions' post-states: a province-day whose epoch moved is *churned* (a SkipQuiet's own change counts for its `b0`'s day, any other for its landing bell's day). At 20× an **idle** province-day over 6 fails criterion 3; churned days are reported (list and distribution) | O11, W5-B F4 |
| Round → anchor reference (criterion 3, **F5 pinned**) | `round_to_anchor_slots` and `s_to_first_cache_slots` = landing slot − **the first slot whose Clock is at or after `round_time(r) + drand delay`** (the replay's publication, which the drand gate waits for). The slot → Clock map comes from the run's transactions (`SlotClock`: counted back from the first observed slot at or after the instant at `0.4 × scale` s per slot, never before the previous observed slot). Also reported: `*_game_secs` (game seconds from publication to the landing slot's Clock; the **2× targets are judged on these**) and `round_to_anchor_from_publication_slots` (the pre-W6 figure). Criterion 3 judges the slot targets at 20× and the game-second targets at 2×; latencies are computed over play only (a drain at another scale is left out) | N11 (F5) |
| E6 / pins | The report prints the verifier's read and verify wall time (E6), the tamper suite's time, and the `.so` pin (`--expect-so-sha256` and what V2 checks it against) | O11 (E6), wave note |
| Load line | Prints the judged ingest → WS figure (WS stamp) with the fold-lag fallback beside it (the old line printed the fold lag as if it were the judged figure: nightly-1 showed "6.0 s" for a 0.20 s pass) | — |

**Proposed amendment (integrator, §13.4 criterion 3, F5):** "round → anchor p99 ≤ 2 slots" and "S → first cache p99 ≤ 2 slots" are counted **from the first slot whose Clock shows the round public (`round_time + drand delay`) to the landing slot**; the game-second targets of the scale-2 run from the publication instant. Measured from the publication instant, every 100× run lands at 2.95–2.98 slots: the Clock moves once per slot, so the first slot that may show the round comes up to one slot after publication (at 20×, 8-game-second slots, 0.5–0.9 of a slot for a round published 1–4 s after a slot's Clock), and the keepers then land 2 slots later; the old reading cannot pass with today's keepers. Under the pinned reading the keepers land in exactly **2 slots at p50 in every run** (§2) — the target has no headroom (F-A3).

### `up` and configs

- **`drain_scale`** (`--drain-scale S`, TOML `drain_scale`; default `max(scale, 20)`): one bell after play, when the fleet is stopped, the chain goes to the drain's scale (event `drain-scale`, `state.play.drain_scale`), and a localnet restart re-applies the current scale. At 2× the 26-bell keeper-only drain took 2.2 h of wall time; at 20× 13 min. Runs at ≥ 20× are unchanged. The report judges latencies over play only.
- **`eager_bots`** (`--eager-bots` / `--no-eager-bots`, TOML `eager_bots`; default on for runs shorter than one game day): the fleet gets `--day0-share 1 --eager-personas` (the in-process day's pacing, W5-B F2/R3), but only the flags `frontier-bots --help` lists (W6-C is adding them; an unknown flag would crash-loop the fleet) and not when `bots_args` sets them. With today's `frontier-bots` nothing changes; after W6-C merges, the Gate W6 latency line gets marches without new flags (F-A8). One-day and longer runs (nightly, smoke, w6-s7) are unchanged.
- `--expect-so-sha256` was already implemented by integ-W5r (O8): the stack refuses another `.so` and V2 checks the deployed program against the pin. Exercised end to end by the archive smoke (§2) and used by `m1-run-s7.sh`.
- Configs: `real-smoke.toml` = the smoke that ran (one game day at 100×, 100 bots, adversary, base 41600); `w6-latency.toml` `drain_scale = 20`; `w6-s7.toml` comment (archive complete; the Gate W6 line names no `--adversary`).
- Usage text lists `--drain-scale` and `--expect-so-sha256`.

### `scripts/m1-run-s7.sh` (written, not started)

Runs exactly the Gate W6 w6-s7 line — `up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 --run-id w6-s7 --base-port 41000 --chaos --viewers 5000` — plus `--expect-so-sha256 <build record>`, then `verify`, `tamper`, `report` (each runs even if an earlier one failed, so the report is always written). Steps: build (release workspace + `scripts/build-frontier.sh`, deployable), take `file_sha256` from the build record, re-check it with `shasum`, write `<run>/so.sha256`; `check-ports` (w6-s7.toml at 41000); `up` under `caffeinate -i`; verify/tamper/report; summary `<run>/s7.json` (step exits, git head, `.so` sha256) and copies of `report.md`, `s7.json`, `so.sha256` into `docs/frontier/m1/runs/<run-id>/` (not committed by the script). Services stay up (chain paused) for triage unless `--down`. `--dry-run` prints the commands; flags that the Gate line fixes (`--base-port`, `--beacon`, `--scale`, `--days`, `--so`, `--expect-so-sha256`) are refused; other flags pass to `up` (for example `--adversary`). Exit 0 all pass, 4 PENDING, 3 PENDING-OWNER, else 1. Bash 3.2 (macOS `/bin/bash`) checked: `bash -n`, `--dry-run`, the refusal.

### `scripts/m1-nightly.sh`

`--run-id ID` (default `nightly-YYYYMMDD`), so three nights on one date keep three run directories.

## 2. Runs [measured, 2026-09-28, this machine]

Release `.so` 874,120 B, sha256 `1b1968af…d506fc` (= Gate W5's); test-beacon `.so` 874,624 B.

| Run | What | Result |
|---|---|---|
| `w6a-real1` — **real-round smoke** | `up --config real-smoke.toml` shape: `--beacon archive` (the main session's archive, G0 1788998400, rounds from 32,065,012), release `.so` pinned with `--expect-so-sha256`, 100×, 1 game day + 26-bell drain, 100 bots, adversary on, base 41600 | `up` exit 0 (54 s of setup incl. 44.0 s pre-season; 2,678 slots). **verify PASS** (6,729 txs, 5,836 accounts; read 0.9 s, verify 3.9 s; V2 against the pin). **tamper: first run 28/29, T10 missed** (F-A1), after the fix **29/29 FAIL with their codes** (26 on the run, 3 on fixtures). **report exit 0**: criteria 1, 2, 4, 5, 8, 9 pass; 3, 6, 7 n.a. (100×; no in-run viewers). Herald: 0 alarms (quicknet signatures checked without `--test-key`), fold lag p99 10 slots. Keepers A and B: reveal effective N ≥ 150 in every bell of play. Records: 37 provinces, 64 joins, 10 marches (10 REVEAL, 10 CLASH, 10 TRANSIT_SETTLED), 0 bad seals. ClashInputs: 10 open, all closable after grace |
| `w6a-nightly-1` | `scripts/m1-nightly.sh --run-id w6a-nightly-1` (build, test key, 100 × 1 day at 100×, adversary, 41500) | **every step exit 0** (`"pass": true`): verify PASS (6,440 txs), tamper 29/29 (16 on the run: no march), load pass (p99 file 25.6 ms, ingest → WS p99 0.20 s by WS stamp, 0 errors, 0 gaps), report exit 0 |
| `w6a-nightly-2` | same, `--no-build` | **all exit 0**: verify PASS (6,977 txs), tamper 29/29 (26 on the run), load pass (p99 24.6 ms, 0.4 s), report exit 0; 8 marches |
| `w6a-nightly-3` | same, `--no-build` | **all exit 0**: verify PASS (6,740 txs), tamper 29/29 (25 on the run), load pass (p99 53.2 ms, 0.28 s), report exit 0; 3 marches |
| `w6-latency` — **scale-2 latency run** | the Gate W6 line as written (`up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id w6-latency --base-port 41700 --chaos`; base port 41700 instead of 41000 only) | `up` exit 0 (53 s setup; play 3 h of wall time; the drain went to 20× at slot 28,537 and took 13 min; complete at slot 30,412, 23:55 JST, ≈ 3 h 23 min in all). One chaos kill (herald, restarted after 1.2 s). **report exit 0**: criteria 1, 2, 4, 5, 8, 9 pass; **3 n.a.**: round → anchor **p99 2.0 game s** and S → first cache **p99 2.0 game s** (targets 5 s) are met, but anchor → last reveal and close → resolve have **no samples — the fleet made no march in 6 game hours** (61 joins of 300 bots, 13 Trains; F-A8); SkipQuiet ≤ 2 per province-day. Extra (not gate lines): verify PASS (2,649 txs), tamper 29/29 (15 on the run). `down` exit 0 |

**Three consecutive nightlies green** (Gate W6 line 1): nightly-1, -2, -3, back to back on one evening (not three nights; the test key, the nightly config, `--run-id` only).

### Measurements across the runs

| Run | round → anchor slots p50 / p99 / max | from publication, slots p50 / p99 | S → first cache slots p50 / p99 | anchor → last reveal slots | close → resolve slots p50 / p99 | SkipQuiet per idle province-day (n, max) | per churned (n, max) |
|---|---|---|---|---|---|---|---|
| w6a-real1 (100×, archive) | 2 / 2 / 2 | 2.98 / 2.98 | 2 / 2 | 0 | 6 / 6 | 39, 6 | 26, 44 |
| w6a-nightly-1 (100×) | 2 / 2 / 2 | 2.98 / 2.98 | 2 / 2 | – | – | 53, 6 | 20, 11 |
| w6a-nightly-2 (100×) | 2 / 2 / 2 | 2.95 / 2.95 | 2 / 2 | 0 | 6 / 6 | 40, 6 | 30, 42 |
| w6a-nightly-3 (100×) | 2 / **3** / 5 | 2.95 / 3.95 | 2 / **3** | 0 | 6 / 7 | 43, 6 | 29, 35 |
| w6-latency (2×, 0.8 game s per slot) | 2 / 2 / 2 (= **2.0 game s** p99 from publication) | 2.50 / 2.50 | 2 / 2 (= 2.0 game s) | – (no march) | – (no march) | 21, 2 | 16, 2 |

**Reveal CU distribution** (landed Reveals, whole-transaction units incl. ≈ 450 of ComputeBudget; for W6-E's c4 v3): w6a-real1 n 10, p50 20,222, p90 21,650, max 21,868; nightly-2 n 8, p50 18,054, max 21,186; nightly-3 n 3, p50 18,054, max 20,404; w6-latency none (no march). The distribution so far is 21 Reveals; W6-E should say it is small. Largest CU per kind in these runs, all within §5.5: PostAnchorMulti 380,718 / 400,000; PostSeed 339,691 / 345,000; PostBeacon 332,937 / 340,000; FoldOccupancy 28,887 / 30,000; SettleTransit 63,047 / 85,000; GatherClash 37,093 / 49,000; ResolveFromInputs 36,048 / 340,000; SkipQuiet 56,487 / 90,000 + 30,000 per unit.

## 3. Findings and triage (for W6-B/C/D and the triage pass)

- **F-A1 (verifier, W6-C; fixed here as a hand-over, `acee4c7`):** T10 ("wrong drand key") set the quicknet key; on a real-round run the pinned key *is* quicknet's, so T10 changed nothing and was missed — the Gate W6 w6-s7 `tamper` line would have failed on every real-round run. T10 now swaps in the test key when the pinned key is quicknet's. Test `stack::verifyrun::t10_uses_a_key_other_than_the_pinned_one`; `w6a-real1` tamper 28/29 → 29/29; `verify` tests (5 + 56 + 8, and `--include-ignored tamper_`) green. W6-C may prefer its own form; the integrator takes this commit or W6-C's.
- **F-A2 (keeper, W6-C):** two keepers race every settle: in each run SettleTicket `NoTicket` = the number of tickets (63), SettleDeparture `AlreadyDone` and SettleTransit `TransitState` = the number of marches; nightly-3 also had PostBeacon `AlreadyDone` × 51 and SkipQuiet `OutOfOrder` × 35. Each is a paid failed transaction (keeper spend) — reported, not a criterion.
- **F-A3 (keeper, W6-C; criterion 3 at 20×):** under the pinned reading the keepers land THE anchor and the first cache **exactly 2 slots** after the round becomes visible at p50 in every run, and 3 at p99 (max 5) in nightly-3 under machine load. The criterion's 2-slot p99 has no headroom: one slot goes to the keeper learning the round (drand-replay reads the chain clock, the keeper polls drand-replay). If the w6-s7 run misses criterion 3 on these two lines, the keeper's round pickup (subscribe to the replay, or poll within the slot) is the lever, not the program.
- **F-A4 (keeper, W6-C; W5-B F4 confirmed):** idle province-days stay at ≤ 6 SkipQuiet (max 6 in every run), but **churned** province-days take up to 44 (real smoke) — SkipQuiet stops at every roster change and starts a new transaction. Criterion 3 judges idle days only; churned days are reported.
- **F-A5 (bots, W6-C; W5-B F2 still open):** the fleet is thin: 25–66 joins of 100 bots per game day, 0–10 marches; most personas `needs-chain`/`pending` (only `forger` and `spammer` observed; the forger's 6 Reveals refused `BadAddress` in the real smoke are its expected refusals). Criterion 5 passes because nothing is violated, not because every persona was seen. `--day0-share` / `--eager-personas` (W6-C) are the fix; the stack passes them with `--bots-args`.
- **F-A6 (stack/adversary, W6-C holds):** several holds find nothing to hold or change nothing: `slots-below` and `lag` were skipped ("nothing to hold before the deadline"), and the anchor, keeper-payers, defence-pool and relay-payers holds saw 0 writes of their keys inside the window and 0–1 in the window after (the frontier-fund hold delayed 18 writes, the ticket hold 4–6). No hold saw a write inside its window (every hold was above the keeper cap, so nobody outbid it). The ring-opening / claim-grace timing is W6-C's (O11).
- **F-A7 (herald, W6-C):** fold lag p99 10 slots in the real smoke at 100× (bell boundaries), 0 alarms; the WS-stamp ingest → WS p99 0.20–0.40 s in the nightly loads. **But after the latency run's chaos `kill -9` of the herald (bell 23.3, restarted after 1.2 s) its fold lag was 586 and 628 slots at the next two bell samples** (bells 25 and 26, several minutes of wall time behind at 2×), back to 0 at bell 27. During the w6-s7 in-run viewer window a herald kill would put ingest → WS far above 2 s for that long; the report's in-run verdict will show it.
- **F-A8 (bots, W6-C; latency run):** 300 bots in 6 game hours made **no march** (61 joins, 13 Trains, 8 Builds), so criterion 3's anchor → last reveal and close → resolve could not be measured at 2×, and the Gate W6 pass condition "the latency run meets criterion 3's game-second targets" is met only on the two lines that have samples. Fix: W6-C's `--day0-share` / `--eager-personas`; the stack now passes them automatically below one game day (`eager_bots`). The integrator's Gate W6 latency run after the W6-C merge should show REVEAL and CLASH records; if it does not, the bots need a march within the first game hours (a persona or `--bots-args`).

**What the triage pass must check on w6-s7** (in addition to the report's decided criteria): criterion 3 in slots at 20× (F-A3 — the most likely miss; the report shows both readings); idle vs churned SkipQuiet (F-A4); `tamper` 29/29 with T10 on the run (needs F-A1 merged); the in-run viewer verdict (`load/in-run.verdict.json`, criterion 6); ClashInputs "blocked" list (criterion 1); keeper spend from the settle races (F-A2); persona coverage (F-A5); hold effects (F-A6); the verifier's wall time on the 7-day input (E6, printed in the report); the herald's fold lag after each chaos kill of the herald against the in-run viewer window (F-A7); whether the one-day-and-longer fleet marches at all at 1,000 bots (F-A5; `eager_bots` is off for a 7-day run, pass `--eager-bots` to change that). Start command (main session, from the integration worktree after the W6 merge): `scripts/m1-run-s7.sh` (add `--adversary` for the §13.4 hold schedule; `--dry-run` first prints the exact commands).

## 4. Gate items run on this branch

| # | Command | Result |
|---|---|---|
| 1 | `(cd frontier-node && cargo fmt --all -- --check)` exit 0
| 2 | `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` exit 0
| 3 | `(cd frontier-node && cargo test --locked --release --workspace)` exit 0: 336 passed, 0 failed, 10 ignored (stack 38 after the last commit)
| 4 | `(cd frontier-node && cargo test --locked --release -p verify -- --include-ignored tamper_)` | exit 0 (56 + 5 + 8 green) |
| 5 | `scripts/m1-nightly.sh` (Gate W6 line 1) × 3 consecutive (`--run-id w6a-nightly-{1,2,3}`, the first with the build) | exit 0, 0, 0 |
| 6 | `$S up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id w6-latency --base-port 41700 --chaos` exit 0 (§2)
| 7 | `$S report --run-id w6-latency && $S down --run-id w6-latency` exit 0, exit 0 (criterion 3 n.a.: two of four lines unmeasured, F-A8)
| 8 | (not a gate line) real-round smoke `w6a-real1`: up, verify, tamper, report, down | exit 0 each (tamper after F-A1) |
| — | Gate W6 w6-s7 line (+ verify/tamper/report) | **not run** (wave note: the main session runs `scripts/m1-run-s7.sh`) |

Not run by this unit (not its files): the svm tests, `mutate.sh`, `itest g14_`, the screens package, the root-workspace lines, the frontier-sim gates.

## 5. Deviations and choices (for review)

1. **F5 pinned in the report** (§1): the slot targets are judged from the first slot that shows the round, the 2× targets from publication; both readings are reported. Needs the §13.4 amendment above to be normative.
2. **Drain at `max(scale, 20)`** by default (§1): the Gate W6 latency line as written now drains at 20×; latencies are judged over play only. A run that wants its drain at its own scale sets `--drain-scale`.
3. **`m1-run-s7.sh` adds `--expect-so-sha256`** to the Gate W6 line (the release build's record; the run is otherwise the line as written) and leaves the services up for triage unless `--down`. The Gate W6 line has **no `--adversary`**; `configs/w6-s7.toml` and the exit run (§13.4, `m1-exit.toml`) do — pass `--adversary` to the script to include the hold schedule.
4. **Ports:** the latency run used base 41700 (the Gate line says 41000) because other W6 units' stacks were on 41300 and could be on 41000; the stack's ports are all offsets of the base, nothing else changes.
5. **Nightlies back to back:** three consecutive green runs on one evening, not three nights.
6. **`eager_bots` on by default below one game day** (§1): only the flags the fleet lists are passed, so it is a no-op until W6-C's flags exist.
7. **Cross-ownership:** the T10 fix in `crates/verify` (F-A1) and `--run-id` in `scripts/m1-nightly.sh` (W5-B's file).

## 6. Dependency requests

None (no manifest, lockfile, toolchain or `.gitignore` change).
