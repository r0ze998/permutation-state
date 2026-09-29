# W6T-2 (fix unit U2): keeper latency, end-of-season waste, reveal acceptance

Triage of the 7-day `w6-s7` season (`PLAN.md` §1 U2; contract v1.11). Branch `frontier/m1-w6t-U2`, cut from `frontier/m1-integ` = `7dcacdf`; worktree `.claude/worktrees/m1-w6t-U2`. Paths touched: `frontier-node/crates/keeper/**` and this file only. No manifest or lockfile change, no dependency added. Nothing pushed.

Commits (oldest first):

| commit | what |
|---|---|
| `4feb113` | the failing-first tests, with the behaviour-neutral plumbing they compile against (the `backup_delay_slots` key parsed, `Engine::note_tx_meta` a no-op, the test kit `src/testkit.rs`) |
| `81df890` | the nine plan items |
| `6e7a587` | R2 finding 1: the beacon duty's anchors and seed caches are sent right after it plans them |
| `30e791c` | R2 finding 2: one feed read for the land and play indexes |
| `c823efd` | R2 finding 3: a status snapshot from the process start |

## 1. The nine items

Every plan test failed on `4feb113` (the `7dcacdf` behaviour) for the w6-s7 reason, and passes from `81df890` on. The failing output is in scratch (`w6t-U2/failing-first.log`, `failing-first-latency.log`) and quoted in the last column.

| # | change | test | failing output on `4feb113` |
|---|---|---|---|
| 1 | **Closes (cause A).** `PlayDuty::closes` reads ArrivalSlots, ArrivalDays and ClashInputs with one `getMultipleAccounts` per 100. Each key gets a recheck time: its grace end once it is settled or resolved and settled (the claim grace for a slot, `resolved_ts + clash_close_grace` for inputs), otherwise one bell later. Closes and claims are now `PlayDuty::housekeeping`. `Keeper::tick` plans them after the tick's send and sends them in a second send (`plan()` still runs both, for callers and tests). | `play::w6t_tests::closes_do_not_reread_inputs_inside_grace`: 5,000 inputs and 1,000 slots inside their grace, 10 closes passes | `[6000, 6000, …]` calls per pass. Now the first pass makes ≤ 60 calls of ≤ 100 keys, and the other passes 0 |
| 2 | **In-slot drand retry (cause B).** `Rounds::take_missed(slot)`. On an idle tick (same slot), a round asked for and not served in this slot is asked again: the beacon plans and its writes are sent. | `tests/latency.rs::round_425_in_publication_slot_still_anchors_in_slot`: drand answers "not yet" to the first request for every round; the keeper ticks twice per slot | every bell 2–10 anchored one slot late: `(2, 402, 403) … (10, 1002, 1003)` |
| 3 | **Journal prefix (cause C).** `object_keys_with_prefix` is a range on the `attempts_object` index (`p ≤ key < succ(p)`; empty prefix and the `char::MAX` edge are handled) | `journal::prefix_tests::object_keys_with_prefix_matches_scan`: 100,000 rows; 11 prefixes (ASCII, `ü`, empty, none) identical to the `substr` scan; the 288 × 16 calls of a restart | `took 26.728426s` (load 38–48, the shared machine was busy). Now < 1 s |
| 4 | **`/v1/status`.** The tick no longer takes the API's `Shared` state for the play duty. It takes only the queued `reveals` and `nudges`, and merges track updates and `blocking` back. `status`, `metrics`, `tracks` and `reveal_keys` stay served. Before, the whole struct was `mem::take`n for the play plan, so status was `null`, a track answered 404, and the same material could be queued twice. The snapshot is published after the sends. | `tests/latency.rs::status_answers_during_a_long_tick`: every chain read delayed 150 ms, an 8.5-s tick, status polled every 100 ms | `76 of 81 status samples not answered with a status: [(true, 399µs, "null"), …]` |
| 5 | **End of season.** `anchors_plan`: `last = min(b_now − 1, end_bell − 1)`. `play.rs` `reveals`: an arrival `≥ end_bell` is skipped | `beacon::tests::anchors_plan_stops_at_end_bell` (`b_now = end_bell + 5`); `play::w6t_tests::reveals_skip_arrivals_at_or_after_end_bell` (arrivals `end − 1`, `end`, `end + 2`) | anchors planned for bells 20–24 (end 20); `arrive 300 (end_bell 300): reveal:…:297` planned |
| 6 | **Close waste.** `engine.rs`: a `ProgramFailedToComplete` whose units consumed ≥ the version's CU limit, or whose logs say "exceeded CUs meter", is CU exhaustion. It climbs the CU ladder (`Budgets::retry_cu`: 2×, then 1.4M), not the heap ladder, and SkipQuiet treats it as `NotQuiet` as before. The units and logs come from the program feed: `PlayIndex` records failed PFTC transactions (`failed_meta`) and the keeper hands them to `Engine::note_tx_meta`. While the keeper reads the feed (`expect_tx_meta`), a PFTC status waits up to `META_WAIT_SLOTS` (3) for them. `play.rs`: a close key that ended Dead is not planned again until its account (lamports, sha256 of data) changes; it is re-read once a bell. | `engine::tests::cu_meter_pftc_climbs_cu_not_heap` (units = limit; meter log; a genuine fault below the limit still takes the heap rung); `play::w6t_tests::dead_close_key_not_replanned` | `left: (Some(6000), Some(262144)) right: (Some(12000), None)`; `re-planned at pass 2` |
| 7 | **reveal_accept.** `409 ArrivalBell` when the transit arrives at or after `end_bell`, Running or Ended (replaces the Ended-only `410 WindowClosed`). `409 Shielded` for the §5.11 step-6 shield rules, as the program judges them (`reveal.rs` 857–870): the destination tile is another faction's holding site shielded past the arrival bell, or the host's own Holding is shielded at `bell_start(arrive)`, is not dormant, and the destination is another faction's holding site. It reads no new account. Every refusal body is now `{"error", "code", "detail"}` (`error` is the relay's field; `code` stays). | `reveal_accept::tests::reveal_accept_refuses_own_shield_war_target`, `reveal_accept_refuses_arrival_at_end_bell`, `api::tests::refusal_bodies_name_the_error` | `called unwrap_err() on an Ok value: Accepted {…}`; `status 3 end 40: accepted`; `left: Null right: "Shielded"` |
| 8 | **Redundancy.** New `keeper.toml` key `backup_delay_slots` (u32, default 0). SettleDeparture and SettleTransit wait that many slots after they first become eligible, then re-read the Holding (batched) and are planned only while the transit is still in the state they need (SettleDeparture: 1, departed; SettleTransit: 2 or 3). Reveals are unchanged. One Reveal write per `(host, arrive)`, whatever the source, and a W write never gets a second version in the chain slot its last version went out in. | `play::w6t_tests::backup_delay_rereads_before_settle`; `play::w6t_tests::reveal_sources_deduped_by_host_arrive` (both sources, late ticks) | `slot 100: sent early`; `at most one Reveal of the march per slot: {51: 2, 52: 0, 53: 2, 54: 0}` |
| 9 | **Skip splits (part E).** `skip_target` splits a batch only for a nudge, an arrival here, a departure from here to settle, a pending `Spend` or `Leave`, or a `Leave` entry. The last two are what SettleDeparture and the §21 return settle wait on; see deviation 2. Other pending resident changes (musters, splits, merges, forfeits) ride whole 24-bell batches. The `dest_of(..).unwrap_or(origin)` fallback is gone; the origin is the target only once the seal is judged bad. | `play::w6t_tests::idle_day_with_pending_ops_is_one_batch_per_24_bells` (5 pending ops in a day) | `one SkipQuiet per 24 bells: [(432, 6), (438, 24), (462, 1), (463, 1), (464, 24), (488, 22), (510, 23), (533, 24), (557, 24)]`: 9 batches. Now 6 |

Existing tests changed with the behaviour:

- `skips_wait_for_the_target_bell`: a pending change no longer targets its bell, and an unknown destination targets nothing until the seal is judged bad.
- `the_accept_path_answers_like_the_program`: Ended with `arrive == end_bell` is `ArrivalBell`.
- `parses_the_documented_file`: `backup_delay_slots`.

## 2. R2: the late-season replica [measured, 2026-09-30, this machine]

Setup, as in `triage/latency.md` §3 and PLAN R2. `r2.sh <variant> <keeper> <kill9> 1500` in scratch `w6t-U2/r2/`:

- `frontier-localnet` from `snap-000000070210.bin` with the WAL cut at 584,357,446 B, on 41610/41611 (`state 960625ee…` on every start);
- `drand-replay` on the w6-s7 archive, on 41620;
- keeper A's `keeper.toml`, seed and beneficiary key, with the journal cut at slot 70210 (`sqlite3 .backup` of the triage copy, attempts and alerts after 70210 deleted). The API is on 41650, `FRONTIER_KEEPER_TICK_LOG=1`.
- A status sampler (the stack's 5-s timeout) runs once a second.
- 1,500 slots each. The fix variants kill -9 the keeper at slot +700 and restart it at once.
- Then `lat.py <journal> 70300 1008` and `ticks.py <log> 70400` (the triage's scripts), plus `anch.py` and `sres.py` (per-bell breakdowns, in `r2/`).
- Only these three services ran; everything was stopped after each variant (ports 41610/41611/41620/41650 checked free).

| variant (keeper sha256) | load avg (start → end) | round → anchor p50 / p99 | S → first cache p50 / p99 | S → resolve p50 / p99 | closes (per tick) | slots with a tick | first tick after (re)start | status samples |
|---|---|---|---|---|---|---|---|---|
| base: 7dcacdf + the triage's tick timers (`b6e0d2c3…`) | 4.7 → 7.9 | 3 / **9** | 1 / **6** | 5 / **14** | 1,385 ms p50, 15,361 reads | 614 of 1,312 (47 %) | 17,852 ms (beacon 17,406) | **471 of 544 null** |
| `81df890` (`096dcfaa…`) | 6.8 → 3.9 | 1 / **2** | 1 / 1 | 2 / 2 | steady 3–17 ms, 1–2 reads; ≤ 71 ms in the passes after a start | 1,313 of 1,315 | 518 ms | 0 null, 0 > 1 s |
| `6e7a587` (`b0cf1e50…`) | 5.7 → 2.9 | 1 / 1 | 1 / 1 | 2 / **3** | same | 1,313 of 1,315 | 508 ms | 1 null (after the kill) |
| `30e791c` (`f3e104ee…`) | 3.9 → 3.6 | 1 / 1 | 1 / 1 | 2 / 2 | same | 1,314 of 1,314 | 321 ms | 1 null (after the kill) |
| **`c823efd` final** (`5494cdce…`) | 6.7 → 3.1 (3.7–6.9 during) | 1 / **1** | 1 / **1** | 2 / **2** | steady p99 14 ms over 1,500 passes, 1–2 reads; 52–80 ms in the passes right after a start | **every slot** 70400–71712 (1,313 gaps of 1; slot 70913 ticked by both processes) | **315 ms** (beacon 75 ms) | **0 null of 546**, 0 > 1 s (2 connection refusals: before the first start, and at the kill) |

Reading the rows:

- **Base** reproduces the real run's late half, and is worse than the triage's replica (7 / 5 / 13) at a similar load. The closes duty read every open ClashInputs with its own RPC every 4th slot. After each closes tick the keeper skipped 2–4 slots (slot gaps: 295 × 1, 259 × 3, 45 × 4).
- **`81df890`** meets the targets except round → anchor p99 2. The late bell was 942, published 12 slots after the kill -9 at 70912: 16 region-anchors sent in the publication slot and landed 2 slots after it. Its tick ran about 0.45 s, because the restarted keeper re-reads the program feed from cursor 0, and the land and play indexes each read 8 pages (≈ 0.2 s each). The anchor was planned early in the tick but sent only after them.
  - `6e7a587` sends the beacon duty's anchors and seed caches right after it plans them (`Engine::send_kinds`), which fixed the anchor tail.
  - Bell 941's resolves then took 3 slots, published 21 slots after the kill, still inside the catch-up. Their seed cache landed at S + 1, but the resolve was sent one slot late.
  - `30e791c` reads each feed page once for both indexes (while their cursors agree, which they always do from a start), so a catch-up tick is ≈ 0.3 s.
- **`30e791c`** gives 1 / 1 / 2 with the kill -9, every slot ticked, and a first tick after the restart of 321 ms.
  - Its one "null" status sample was the restarted process's API answering before its first tick had published. The old keeper had the same gap after every start.
  - `c823efd` starts the snapshot as `{"starting": true}` and publishes a full status in `start()`.
- "Status samples": 1 s cadence. The sample that fails to connect while the process is dead (the kill -9 itself, and the start) is not counted.

The PLAN "done when" for R2: p99 ≤ 1 / 1 / 2, closes tick < 50 ms, 100 % of slots ticked, a first tick after kill -9 < 1 s, every status sample answered.

- The final row meets all of them, with one qualification on the closes tick (§5 item 7).
- The steady-state closes phase is 0–14 ms (p99 14 ms over 1,500 ticks), with 1–2 `getMultipleAccounts` calls, against 1,385 ms and 15,361 reads before.
- The 4–5 closes passes right after a (re)start read every key once while the index refills: 13–62 batches, 47–80 ms.
- Every tick of the final run, the restart included, stayed under one slot: other ticks p99 114 ms and max 315 ms; closes ticks max 308 ms.

## 3. Tests run

- `cargo test --release -p keeper` (scratch `CARGO_TARGET_DIR`): **59 passed, 0 failed, 1 ignored** (the Gate W4 crash-injection sweep). Unit tests 43 (31 before), plus `archive_returns_rent` 1, `held_accounts` 2, `land` 3, `latency` 2 (new), `one_day_beacons` 1, `play` 6, and `reveal_accept` 1. Load averages 5–7 during the final runs, 38–48 during the failing-first run.
- `cargo test --release -p keeper --test play -- --include-ignored crash_injection` (the Gate W4 line): **ok** on `c823efd` (74.7 s, load 3.2–3.8), and on `81df890` (94.6 s).
- `cargo clippy --release -p keeper --all-targets -- -D warnings`: clean. `cargo fmt -p keeper -- --check`: clean.
- Not run here: the other crates' tests (nothing outside `keeper` changed), R3 and R5 (U4, on the integration tree after the merges), and the nightly.

## 4. Keeper guide (for U5's `RUN-A-KEEPER.md`)

- `backup_delay_slots` (u32, default 0). For a backup keeper, e.g. keeper B with 8 (U4's config): its SettleDeparture and SettleTransit wait that many slots after they become eligible, then re-read the transit. The operator's keeper keeps 0, because its latency is what the exit criteria measure.
- `/v1/reveal` refusals: `409 {"error":"ArrivalBell"}` (the arrival is at or after the season's end bell, any status) and `409 {"error":"Shielded"}` (the §5.11 step-6 shield rules). Every refusal body is `{error, code, detail}`.
- `/v1/status` answers from a snapshot that the tick publishes after its sends. It is never taken away while a tick runs, and it exists from the process start (`{"starting": true}` until `start()` has read the Clock). The latency fields are under `duties` (`anchor_latency_slots_p99`, `seed_latency_slots_p99`).
- `FRONTIER_KEEPER_TICK_LOG=1` prints one `TICK slot=… chain_at_send=… sent=… total=…ms <phase>=<ms>ms/<reads>r …` line per tick to stderr. The phases are `season+care`, `poll`, `beacon`, `send0`, `land`, `play`, `send`, `closes` (with its `getMultipleAccounts` calls), and `send2`. An idle tick that sent (the in-slot drand retry) prints an `IDLE` line. The triage's `ticks.py` reads it.
- A close whose every rung failed stays unplanned until its account changes. The CU ladder answers a CU-meter `ProgramFailedToComplete`. U1 raising the `cu_limit` of CloseArrivalDay and CloseArrivalSlot to 8,000 removes the first failed version too.

## 5. Deviations and findings

1. **Notes file name.** The unit owns `docs/frontier/m1/W6T-2-NOTES.md` (PLAN §1: `W6T-<n>-NOTES.md`). The computed SETUP line substituted the unit's whole title into the branch, worktree and notes names; the branch is `frontier/m1-w6t-U2` and the worktree `m1-w6t-U2`.
2. **Skip targets keep Spend and Leave.** The plan lists nudge, arrival and departure to settle. Letting a pending `Leave` ride a 24-bell batch held the §21 return settle, and `tests/play.rs` (all five scenarios) did not finish (`returns 0`). A pending `Spend` or `Leave` and a `Leave` entry are what a SettleDeparture waits on, so they stay targets; musters, splits, merges and forfeits ride the batch. `ProvState::settle_bells` carries them; `pending_bells` still sizes the SkipQuiet CU.
3. **"Keeper A dedupes its two reveal sources" was not what happened in w6-s7.** In keeper A's journal, every same-slot pair is one write key (`reveal:<host>:<depart>`) with versions at slots s and s + 1 that landed in the same block. A tick ran late, and the next tick's escalation version joined the first. Example: `reveal:1909855992414208:546` sent at 41600 and 41601, both landing at 41602. The keys are per `(host, depart_bell)`, so the two sources could never make two writes. The fix does both:
   - an explicit per-`(host, arrive)` guard in the grouping;
   - no second W version in the chain slot the last one went out in, which is the actual mechanism.

   The test covers both sources and late ticks.
4. **Where the CU meta comes from.** `ChainPort` has no `getTransaction`, and `fclient` is U3's, so the meta comes from the program feed the play index already reads (localnet's feed carries units and logs, verified on the live w6-s7 localnet: `computeUnitsConsumed 6000`, "exceeded CUs meter at BPF instruction"). A keeper without play roles has no meta and keeps the old heap rung after the 3-slot wait. A public-RPC keeper, which has no feed, would need a `ChainPort::transaction` in fclient; that is a request, below.
5. **The CU ladder goes to 1.4M**, the existing I-50 ladder (2×, then 1.4M), not only up to the budgets row: there is no "budget" cap in the engine, and a close at 12,000 CU is a small cost next to 270 failed versions.
6. **Three R2 follow-ups beyond the plan** (§2: `6e7a587`, `30e791c`, `c823efd`), each found by R2 against a "done when" condition. They carry test coverage (the status test's restart and shared-cursor asserts) but no separate failing-first commit.
7. **Closes right after a start.** Steady state is 0–14 ms; the passes right after a start read every key once (≤ 80 ms in R2). Spreading that first read over several ticks would keep even those under 50 ms. It is not done: it only happens after a restart and costs no slot.
8. The one-time journal cost of a restart is now small: beacon 67–79 ms in the first tick, against 17.4 s before.

## 6. Requests and hand-offs

- **U4 (stack):**
  - keeper B `backup_delay_slots = 8`;
  - the report reads the last non-null status and the nested `duties.*`;
  - optionally set `FRONTIER_KEEPER_TICK_LOG=1` for the keepers in R3 and R5 and keep their stderr (it gives "slots with a tick" directly).
- **U3 (relay):** pass the keeper's `409` bodies through. The `error` field is `ArrivalBell` or `Shielded`, and `code` carries the same value.
- **U3 (fclient), optional:** a `ChainPort::transaction(sig)` (or units and logs in `Status`) would let a public-RPC keeper judge the CU meter without the feed.
- **U1:** the close `cu_limit`s, as planned. The keeper now climbs the ladder, but the first version at 6,000 or 6,500 still fails on the Ended path.
- **U5 (contract §28, §8.2 rows):**
  - anchors < `end_bell`, and no Reveal for an arrival ≥ `end_bell`;
  - the reveal_accept 409s;
  - the CU-meter rule and the Dead-close rule;
  - `backup_delay_slots`, and one W version per chain slot;
  - the skip-split rule (deviation 2);
  - the status snapshot;
  - the in-slot drand retry and the early beacon send;
  - one feed read for both indexes.

## 7. Repro

Scratch root `…/scratchpad/frontier/m1/w6t-U2/`.

- Failing-first: `git checkout 4feb113`, then `cargo test --release -p keeper` (the lib tests stop the run; then `--test latency`). The logs are `failing-first.log` and `failing-first-latency.log`.
- R2:
  - `r2/r2.sh base r2/bin/frontier-keeper-base 0 1500`;
  - `r2/r2.sh final r2/bin/frontier-keeper-final 1 1500`;
  - then `python3 ../triage/latency/lat.py r2/<v>/keeper/keeper.journal.sqlite 70300 1008`, `python3 ../triage/latency/ticks.py r2/<v>/keeper.log 70400`, `python3 r2/anch.py …` and `python3 r2/sres.py …`.
  - The status samples are in `r2/<v>/status.log` (`epoch http time slot|null|err`).
- Binaries:
  - `frontier-localnet` `1deaec4b…` and `drand-replay` `a63d8fbb…`, copied from the integration tree's release build of `7dcacdf`;
  - the keepers as in the table.
