# W6T-4 (U4, W6-A season-ops): stack, configs, scripts, run records

Fix unit U4 of the w6-s7 triage (`PLAN.md` §1 U4), on `frontier/m1-w6t-U4`, cut from `frontier/m1-integ` = 7dcacdf. Owned paths only: `frontier-node/crates/stack/**`, `frontier-node/configs/**`, `scripts/m1-run-s7.sh`, `docs/frontier/m1/runs/**`, this file. `scripts/m1-nightly.sh` is unchanged (no flag was renamed). No manifest or lockfile change, so there is no dependency request.

Commits:

| commit | what |
|---|---|
| `9516505` | failing-first: 19 new tests against stubs that keep the 7dcacdf behaviour (all 19 fail, the 38 old ones pass) |
| `2090947` | the fixes (58 pass, clippy clean) |
| `88e905d` | WS coverage judged over the generator's own window (found by the first U4 nightly, §4) |
| `af315d9` | the slot holds aim at planned arrivals; `slots-above` armed over play; the honest-seal detail of A2 (found by the U4 nightlies and R3, §4) |
| `bacc957` | two failure classes seen unclassified in R3 |
| `4ef5b69` | the two slot holds of one probe take different provinces (found by R5) |
| `0d45e47` | `m1-run-s7.sh` reads W6T-1's release sha from its build table |
| `4eee1dc` | the no-landing detector leaves the drain out (found by R5) |
| `5e7d896` | planned arrivals count only marches whose Depart was sent (found by R5) |
| (last) | run records, these notes |

`cargo test --release --locked -p stack`: **59 passed** (38 before). `cargo clippy --release --locked -p stack --all-targets`: clean. `cargo fmt -p stack`: clean.

## 1. What changed, per brief item

### (1) Season end at play end

- New config key `season_end_at_play_end` (bool) and flags `--season-end-at-play-end` / `--no-season-end-at-play-end`.
- With it, CreateSeason gets `end_bell = play bells` (`ceil(play_secs / 600)`). `join_close_bell` is scaled from the preset's 756/1,008 and kept in `1..end_bell` (`setup::join_close_for`, `setup::season_params`). Examples: 1 day gives 144/108, 2 days 288/216, 6 game hours 36/27.
- The params are validated (§5.7) **before** AnnounceSeason, so a bad length fails before the bond is posted. `StackConfig::check` refuses fewer than 2 or more than 4,032 play bells when the flag is on.
- For 7 days the params equal the preset byte for byte, so the params hash does not change (tested).
- The EndSeason and drain logic did not need to change: `up` already sends EndSeason once `now ≥ genesis + end_bell · 600` and drains after play.
- The `created` event records `season_end_at_play_end`.
- Test: `setup::tests::short_season_params_validate`.

### (2) Archive guard after CreateSeason

- `up::archive_guard` re-checks the archive after CreateSeason against the actual `genesis_ts`: `archive_until = play_end + drain_bells · 600 + 3,600`, where `play_end` counts from the actual genesis.
- The pre-spawn check only knew the planned `g0 + LEAD_SECS`. w6-s7's setup overran that by 1,452 s.
- A short archive now fails the run before the bots start (`archive guard after CreateSeason: …`).
- `state.json` gains `archive_guard {until, last_round, spare_secs, setup_overrun_secs}`, and an `archive-guard` event is written.
- Test: `up::tests::archive_guard_uses_actual_genesis`. Its archive covers exactly the planned need. It passes as planned and refuses the same run 1,452 s late.

### (3) Adversary coverage

- `slots-below` and `lag` are now armed up to the end of play, the same as `ticket`, `frontier-fund` and `defence-pool`.
- `slots-below` starts after the first hour. If play is shorter than 100 minutes it starts at 60% of play, so it is always armed by 60% of play.
- The `lag` hold takes its origin from **Holding transit records in state 1 or 2**:
  - `adversary::probe_in_flight` reads every Holding (`getProgramAccounts`, dataSize 1,280), but only while a lag hold is waiting.
  - `in_flight_of` keeps states 1 and 2.
  - `origin_in_flight` picks the opened origin Province with the most transits.
  - Previously it looked for Province entries in state 3, and SettleDeparture clears those within slots.
- **Beyond the brief (`af315d9`, `4ef5b69`, `5e7d896`):**
  - `slots-above` is armed to the end of play too. It needs arrivals just as `slots-below` does, and the U4 R3 run skipped it after its one hour: the 300-bot fleet's first arrivals came at bell 76.
  - Both slot holds now aim at the destination with the most marches the fleet sent to arrive at the held bell.
    - The source is the bots' `marchbook-*.jsonl`, read only while a slot hold waits. A seal hides the destination on chain, so only the harness knows it.
    - A march `(host, depart_bell)` counts once, from its `sealed` line, and only if it also has a `sent` line and no `failed` line. The `sealed` line comes before the send: in U4 R5 the first pick had only failed Departs.
  - Without a marchbook they fall back to the old "ArrivalDay exists today" guess. In both U4 nightlies that guess held slots no Reveal needed (0 writes inside or after the window), so no claim could open for `defence-pool`.
  - When both slot holds come due in one probe, the second takes another province (`Pending.held`). In U4 R5 both took one province, so the 3.0 hold masked the 1.5 one.
  - Test: `slot_holds_aim_at_planned_arrivals`.
- Any `hold-skipped` makes the run **not exit-grade**. This shows as report row **E**, together with the missing `.so` pin and test-key rounds. Row E is reported and does not change the exit code, so a nightly that skips `frontier-fund` stays green.
- Tests: `adversary::tests::plan_arms_every_hold_over_play`, `lag_uses_transits_in_flight`, `report::tests::hold_skipped_is_not_exit_grade`. The old `picks` assertion was replaced: a state-3 entry is only the fallback when no situation is required.

### (4) Viewer verdict (`load.rs`)

- `ViewerPlan` / `viewer_args` pass these to `frontier-viewers`:
  - `--retry-budget-ms` = `chaos.restart_max_secs / scale · 1000 + 2000` (5,000 at 20×, 2,600 at 100×; overridable with `viewers.retry_budget_ms` / `--viewer-retry-budget-ms`);
  - `--think-ms` (`viewers.think_ms` / `--viewer-think-ms`, default 5,000);
  - `--follow-status http://127.0.0.1:<herald>`, the herald's base URL (`viewers.follow_status`, default on; `--no-viewer-follow-status`);
  - `--provinces p,q;p,q;…`, the opened Provinces at spawn;
  - `--rings 0,1,…,R`, every ring up to the outermost opened one, as a comma list.
- The arguments are written to `load/<tag>.args.json`. Today's generator ignores flags it does not know, so the stack works with it and with W6T-3's.
- `judge` changes:
  - It uses the generator's denominator: requests + WS sessions (`ws.connected + ws.errors`).
  - It fails when WS viewers are connected for less than 99% of the window outside the herald outage windows. The window is the generator's own `[spawn, spawn + --seconds]`, less a 5 s ramp and a 3 s wind-down.
  - It fails when stale-connection retries or WS reconnects are counted at a sample outside every outage window.
- The data behind these checks:
  - `load::probe` samples the herald's `/h/status` (`ws.open`, fold lag) and the generator's `/stats` (`staleRetries`, `ws.reconnects`, errors) once a second. The samples go to `load/<tag>.samples.jsonl` for both `up --viewers` and `load`.
  - `outage_windows` builds the windows from `events.jsonl`: each chaos kill of the herald, until its restart plus the budget, 1.5 think times and 1 s.
  - The verdict reports `generator_recovery` (whether this `frontier-viewers` has the W6T-3 recovery), `stale_retries`, `unavailable`, `unavailable_ms` and `ws_reconnects`.
- `--chaos-force herald:<game hours after the viewer start>` is repeatable, and `chaos.force = "herald:2,herald:7"` sets the same in a config. Each forced kill:
  - is merged into the chaos plan (`chaos::forced`, `chaos::merge`);
  - restarts after the chaos maximum;
  - applies even without `--chaos`.
- Tests:
  - `load::tests::denominator_includes_ws_sessions`: the w6-s7 numbers give 0.0026023, the generator's own figure, not 0.0026038;
  - `ws_open_12pct_fails` (also covers the generator window bound);
  - `stale_retries_inside_kill_window_pass`;
  - `stale_retries_outside_flagged`;
  - `outage_windows_from_the_chaos_log`;
  - `viewer_args_carry_the_plan`;
  - `chaos::tests::forced_herald_kills_fall_in_the_viewer_window`;
  - `config::tests::w6t4_keys_and_flags`.

### (5) Report

- **Criterion 3, idle day (A1).** An idle day now needs an unchanged `roster_epoch` **and** no GATHER or CLASH record of the province that day. Days with such a record are **active**. They are reported with their own row and are not judged against 6.
  - The logic is in `classify_days`.
  - Test: `day_with_clash_is_not_idle`.
- **Criterion 4, by-rule reasons, and the honest-march rule (A2).**
  - Criterion 4 lists V5's `unrevealed_by_rule` by `reason`. An old verifier without `reason` is shown as `outcome N (no reason: pre-W6T-3 verifier)`.
  - A §5.11 step-6 refusal (`shielded-own`, `shielded-dest`, `path`, `arrival-bell`; not `bounced`) of an honest march fails **criterion 5**.
  - A march counts as honest when the bots' `unrevealed[]` gives no persona, `honest` or `arch:*` and an honest seal (W6T-3's `seal` field). A march missing from that list is treated as honest.
  - Test: `honest_rule_refusal_is_persona_violation`, including W6T-3's JSON shape.
- **Failed-transaction class table.** `classify_failure` and `failure_classes` sort each (kind, error) into:
  - `expected`: persona, adversary, race or drain refusals;
  - `redundancy`: A/B races and duplicates;
  - `waste`: keeper, bot-policy and bot-bug failures;
  - `unclassified`.
  The table is reported and does not gate. Test: `failed_tx_classes_follow_the_triage`, which reproduces the triage's totals.
- **Keeper status.**
  - The report reads the last *answered* sample and the nested `duties.*` fields.
  - It counts unanswered bells (`status_unanswered`, `status_timeouts`, the bell list) and the answer-time distribution.
  - The sampler now writes a line for every unanswered or timed-out request (`status: null` plus `error`). Before, a timeout left no line at all.
  - Test: `keeper_status_from_last_non_null_sample`.
- **Per-bell load average.** The machine's 1/5/15-minute load average is sampled once a bell into `metrics/loadavg.jsonl` (`sysctl -n vm.loadavg`, or `/proc/loadavg`). The report gives p50/p99/max, the worst bell, the maximum per game day and the series. Test: `loadavg_per_bell`.
- **No-landing detector.** `no_landing_windows` lists every stretch with no landed transaction for ≥ 20 slots **after a bell start**. Each bell start owes 16 anchors, so such a stretch is a stall.
  - Quiet stretches inside a bell are only counted. Without this rule the w6-s7 input gives 576 "windows", all of them quiet mid-bell stretches of the early, sparse bells.
  - Bell starts from `end_bell` on owe no anchor, so drain gaps are counted separately (`4eee1dc`).
  - A chaos kill or crash of localnet inside a window is named as its explanation.
  - Test: `no_landing_window_detected`.

### (6) Configs and scripts

- Keeper B gets `backup_delay_slots = 8`: the `keeper_b_backup_delay_slots` config key, default 8.
  - It is written **only when the `frontier-keeper` binary knows the key** (`up::keeper_accepts`, which looks for the key's name in the binary). The keeper's parser refuses unknown keys, so a pre-W6T-2 keeper would not start with it.
  - `state.json` records the value or the reason it was left out.
  - The keeper-toml test asks the keeper's own parser to read the key once `keeper/src/config.rs` has it, so it turns on by itself when W6T-2 merges.
  - Test: `keeper_b_gets_backup_delay_slots`.
- `configs/w6-s7.toml` and `configs/m1-exit.toml` gain `keeper_b_backup_delay_slots = 8` and `[viewers] think_ms = 5000`, `follow_status = true`. The budget is left to its computed default of 5,000 ms at 20×. `m1-run-s7.sh` passes flags, not `--config`, and the defaults are the same values.
- `scripts/m1-run-s7.sh` prints the pinned `.so` sha256 against the W6T-1 record: `match`, `MISMATCH` (warning, not fatal) or `no record`.
  - The record comes from `docs/frontier/m1/W6T-1-NOTES.md`: the first `file_sha256 <hex>`, else the first 64-hex sha on a line naming the release build. W6T-1's build table gives `d85e1bd7…2281` (`0d45e47`). `--recorded-sha256 HEX` or `S7_RECORDED_SO_SHA256` overrides it.
  - `--dry-run` was checked in all three cases (match, MISMATCH, no record).
  - `s7.json` gains `so_sha256_recorded` and `so_sha256_vs_record`.
- The `frontier-stack` usage lists the new flags.

## 2. Failing-first

At `9516505` the stubs keep the 7dcacdf behaviour. `cargo test --release --locked -p stack` gave `38 passed; 19 failed`. The failing tests were:

```
adversary::tests::plan_arms_every_hold_over_play      adversary::tests::lag_uses_transits_in_flight
chaos::tests::forced_herald_kills_fall_in_the_viewer_window
load::tests::denominator_includes_ws_sessions          load::tests::ws_open_12pct_fails
load::tests::stale_retries_inside_kill_window_pass     load::tests::stale_retries_outside_flagged
load::tests::outage_windows_from_the_chaos_log         load::tests::viewer_args_carry_the_plan
report::tests::day_with_clash_is_not_idle              report::tests::honest_rule_refusal_is_persona_violation
report::tests::failed_tx_classes_follow_the_triage     report::tests::keeper_status_from_last_non_null_sample
report::tests::loadavg_per_bell                        report::tests::no_landing_window_detected
report::tests::hold_skipped_is_not_exit_grade          setup::tests::short_season_params_validate
up::tests::archive_guard_uses_actual_genesis           up::tests::keeper_b_gets_backup_delay_slots
```

At `2090947` all 58 pass.

## 3. Repro before/after on the w6-s7 evidence (read-only)

The w6-s7 evidence was cloned into the scratchpad: `state.json`, `events.jsonl`, `metrics/`, `load/`, `verify/{input.json.gz, summary.json, report.json}`, `tamper/report.json`, `bots/report.json`. `frontier-stack report --runs-dir <copy>` was then run twice, once with the integration tree's 7dcacdf binary and once with this branch's binary. The live stack was not touched: the report reads the verify input, not the chain. Load average then was 39.4 / 25.2 / 14.1.

| item | 7dcacdf report | U4 report |
|---|---|---|
| criterion 3: idle province-days over 6 SkipQuiet | 36 (idle p99 16, max 17) | **24** (idle p99 9, max 9); 12 days are active (GATHER/CLASH, max 17). The triage expected 23; the one-day difference was not traced. The keeper fix (U2.9) must bring the 24 to ≤ 6; preview R5 (one day) had 0. |
| keeper A status | `last`: every field null (the last sample was null) | last answered bell 1002: anchor p99 4, seed p99 5, 169 provinces, 448 archived bells; **322 of 1,035 bells unanswered or null**. The old sampler wrote no line on a timeout, so the timeouts show up only as nulls. |
| failed transactions | a flat map | 37,042 = waste 33,121 (keeper 32,873, bot-policy 160, bot-bug 88) + redundancy 3,809 (A/B race 3,570, duplicates 239) + expected 112; unclassified 0 |
| no-landing | not reported | **one** window: slots 827–1028 (202 slots, 155 of them after bell 9 started), bells 8–11, unexplained (no localnet kill). This is the one U3.8 investigated (W6T-3 `e41e241`: concurrent holds fill the block). |
| row E | – | **not exit-grade**: hold-skipped `slots-below`, `lag`, `defence-pool` |
| criterion 4 notes | 27 outside the above-cap windows | the same, plus the by-rule reasons (old verifier: `outcome 6 (no reason: pre-W6T-3 verifier)` 9) |
| criterion 6 (unit-test figures on the w6-s7 numbers) | error rate 0.0026038 = 9,000 / 3,456,499 | 0.0026023 = 9,000 / (3,456,499 + 2,000 WS sessions). WS coverage computed with the same rule from the 144 per-bell `ws.open` samples: **11.8%**, a fail (1,000 open until bell 24, then 0) |

The w6-s7 in-run window has no per-second samples: they did not exist then. The 11.8% figure comes from `metrics/herald.jsonl` (one sample per bell) with the same windows and exclusions (a python re-implementation of `coverage_check`, kept in the scratchpad).

## 4. Runs

When the runs started, the integration tree had not merged U1–U3. So the first runs are on **the U4 tree = 7dcacdf + U4**. They check U4's own pass conditions: the season reaches `end_bell` and drains, every hold fires or is marked, the viewer verdict is measured, and the report has its new sections. They also give the before-picture for the U1–U3 conditions.

By the end of this session U1–U3 had their fixes on their branches: U1 `3676f1a`, U2 `30e791c`, U3 `176138c`. So R3 and R5 also ran on a **preview of the merged tree**: a scratch worktree `m1-w6t-U4-preview`, with each unit's changed files checked out over U4. The units' paths are disjoint and no manifest changed, so the overlay equals the merge. It is uncommitted and has no branch. It is **not** the integrator's merge; the main session's R3/R5 on `frontier/m1-integ` remain the Gate W6 record.

Every record is under `docs/frontier/m1/runs/<id>/` (`report.md`, plus `run.md` or `nightly.json`, and `tamper.md` for the preview runs). The preview worktree was removed after the runs. Its run directories (verify inputs, events, metrics) and the overlay's file list are kept in the session scratchpad under `u4/preview-runs/`. The U4-tree run directories stay in this worktree's `frontier-node/.local/frontier/`. The gateway's `node_modules` in this worktree is a symlink to `m1-integ`'s (same lockfile; nothing was installed), excluded by `.git/info/exclude`.

| id | tree | line | result | load avg (1 min) |
|---|---|---|---|---|
| `u4-nightly-1` | U4 at `2090947` | `m1-nightly.sh` (base 41500) | **load step failed**, the rest passed. WS coverage 88.9%: the generator closes its WS at `--seconds` while its pollers run 7 s more. That was a U4 bug, fixed in `88e905d` | 3.9–13 |
| `u4-nightly-2` | U4 at `88e905d` | the same | **green** (`pass: true`): verify PASS, tamper 29/29, load pass (WS coverage 100%) | p50 4.2, max 11.3 |
| `u4-tri-end` (R3) | U4 at `88e905d` | R3 line | up, verify, tamper, report and down all 0. `end_bell` 288 reached, EndSeason, drain; criterion 1 pass. 7 of 9 holds (slots-above one-hour armed in that build; defence-pool without a claim). U1/U2 conditions not met yet: 3,176 anchor BadData, 7,214 close failures, A/B settle duplicates ≈ 100% | p50 3.95, max 10.6 |
| `u4-tri-20x` (R5) | U4 at `bacc957` | R5 line | verify PASS, tamper 29/29, report 1. c1, 2, 4, 5, 8, 9 pass. c3 fails (round → anchor p99 45). c6 fails (the 7dcacdf generator: error rate 0.76%, WS coverage 16.8%). 8 of 9 holds (no claim for defence-pool). Found three U4 bugs, fixed in `4ef5b69`, `4eee1dc` and `5e7d896` | p50 3.45, max 11.2 |
| `p-nightly-1` | preview (U4 `bacc957`) | `m1-nightly.sh` | **green**: tamper 30/30; load pass with `generator_recovery: true` | p50 3.7, max 5.0 |
| `p-tri-end` (R3) | preview (U4 `bacc957`) | R3 line | **all R3 conditions met except tamper 29/30 (T17)**: c1 pass, 0 anchor BadData, 0 close failures, 0 DEPART ≥ end, 0 Shielded or WrongStatus, c4 = 0, c5 pass, A/B settle duplicates 0.5% (keeper B `backup_delay_slots = 8` written). 9 of 9 holds; 70 failed txs (10,612 on the U4 tree) | p50 3.5, max 5.0 |
| `p-tri-20x` (R5) | preview (U4 `5e7d896`) | R5 line | **exit-grade environment: all 9 holds fired**, including defence-pool on a real claim. c1, 2, 4, 5, 8, 9 pass. Keeper status answered in every bell; 0 waste. Tamper 29/30 (T17). c3: S → cache p99 1, S → resolve p99 3, idle days 0, but round → anchor p99 **81** (the bells 8–11 no-landing window) and anchor → last reveal p99 **37** (one late Reveal, likely behind the above-cap slots-above hold). c6: error rate 0, WS coverage 100%, 12,000 stale retries and 3,000 reconnects all inside the kill windows, but ingest → WS p99 **2.36 s** > 2 s | p50 3.42, max 5.95 |

Findings for the other units and the integrator, from the preview runs:

1. **T17 "skip over an arrival" is missed in 2 of 3 preview runs.**
   - The verdict is still FAIL, but with ClashReplayMismatch and RevealAfterLatch instead of SkipNotQuiet/SkipOverArrival, so tamper gives 29/30.
   - It is new with the merged keeper and verifier: the U4-tree runs had 29/29 before T24.
   - For W6T-3 (T17's construction) and W6T-2 (skip batches carrying pending operations).
2. **The no-landing window at bells 8–11 is still there.**
   - It appears in every 20× run: w6-s7 827–1028, U4 R5 828–992, preview R5 815–1028.
   - W6T-3 explains it (three concurrent holds fill the block) but did not fix it.
   - It is what keeps criterion 3's round → anchor p99 above target at 20×. It needs a fix, or a decision on how criterion 3 treats a hold-filled block.
3. **Criterion 3 does not exclude above-cap hold windows; criterion 4 does.**
   - Anchor → last reveal p99 37 comes from one Reveal the design makes wait.
   - For the architect or U5: exclude the above-cap windows (anchor, slots-above) from criterion 3 as criterion 4 does, or keep them and move the holds apart.
4. **Criterion 6 ingest → WS p99 is 2.36 s once the WS viewers are really connected all window.**
   - This is new evidence, not a regression: the earlier figures covered ≤ 17% of the window.
   - For the herald owner and the architect: WS fan-out under 5,000 viewers, or the target.
5. **Reveal AlreadyDone 109 against 36 landed Reveals in preview R5.** `backup_delay_slots` covers only the settles. For W6T-2.

## 5. Interfaces for the other units and the integrator

- **W6T-3 viewers:**
  - The stack passes `--retry-budget-ms N`, `--think-ms N`, `--follow-status http://127.0.0.1:<herald>` (the **base URL**; the generator appends `/h/status`), `--provinces p,q;p,q` and `--rings 0,1,2` (a **comma list**, as the current parser reads it, not `a-b`).
  - It reads `staleRetries`, `unavailable`, `unavailable_ms`, `ws.reconnects` and `errorSeconds` from the report, and the same counters live from `/stats`, **cumulative**.
  - Checked against W6T-3 `197ca6b`: its `parse_rings` takes both `0,1,2` and `0-7`, and `--follow-status` takes the base URL (`host_of`). Preview R5 shows the counters arriving (`generator_recovery: true`).
- **W6T-3 verifier and bots:** `liveness.unrevealed_by_rule[].reason` ∈ {`shielded-own`, `shielded-dest`, `path`, `arrival-bell`, `bounced`}. The bots' `unrevealed[]` gives `persona` (null for the fleet), `seal`, `host` (a decimal string) and `arrive`. Both shapes are tested.
- **W6T-2 keeper:** the key name is `backup_delay_slots`. The stack writes it for keeper B once the binary carries the name. `/v1/status` is read with a 5 s timeout; `status_unanswered` / `status_timeouts` in the report show whether the snapshot fix holds.
- **W6T-5 (contract §12 Gate W6 notes and §13.4 A1–A3):**
  - flags: `--season-end-at-play-end`, `--chaos-force herald:H`, `--viewer-think-ms`, `--viewer-retry-budget-ms`, `--no-viewer-follow-status`, `--keeper-b-backup-delay-slots`;
  - keys: `season_end_at_play_end`, `keeper_b_backup_delay_slots`, `viewers.{think_ms, follow_status, retry_budget_ms}`, `chaos.force`;
  - report row **E** is "not exit-grade" on any `hold-skipped`, no `.so` pin, or test-key rounds;
  - A3's WS coverage is measured as described in §1 (4).

## 6. Open points

- **`defence-pool` needs a claim.**
  - On the U4 tree no below-cap slot hold created one. `slots-below` fired in both nights (bells 15 and 16) and in U4 R3 (bell 76), but it held slots no Reveal needed.
  - With the planned-arrival targeting on the preview tree, `defence-pool` fired: in preview R3 at bell 65 (`af315d9` targeting) and in preview R5 at bell 45 (`5e7d896`). In preview R5 `slots-below` held a real Reveal and the keepers escalated past it (1 write inside the window).
  - The exit run should confirm it over 7 days. A skip still shows as not exit-grade.
- **`frontier-fund` is skipped in short runs.** A 100-bot, 1-day nightly opens rings 0–3 in its first hour, so there is no ring opening left to hold. In w6-s7 it fired (bell 40). This is expected for a test-key nightly, which is not exit-grade anyway.
- **The keeper A status nulls in w6-s7 (322 bells) are `null` bodies the keeper answered**, not timeouts: the old sampler wrote a line only on an HTTP 200 whose body parsed, and `null` parses. Why the snapshot was empty belongs to W6T-2. The new sampler also writes timeouts and errors (`error`), so the two can be told apart. In the preview runs every bell was answered.
