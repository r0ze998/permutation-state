# integ-W6t review response: idle days, defence-pool coverage, the WS tail, persona evidence

A review of integ-W6t raised one blocker and three majors. Each was checked against the code and the `w6-s7`, R3 and R5 data before it was fixed; all four were confirmed (no rebuttal). Fixed on `frontier/m1-integ` (code `dcfece9`, `6685252`, `d66fa99`, `6b9f078`, `33f77cc`, `de9bb3d`, `629d007`; docs and run records in the last commit); contract v1.13 (§29), DECISIONS S26–S29. The paused `w6-s7` stack (41000–41099) was only read; every service this pass started used 41100–41999 and was stopped. No push, no install or download, no devnet/mainnet transaction; `permutation-server/web/session.mjs` unchanged.

## 1. Blocker: criterion 3's idle province-days (S26)

**Verified.** The reviewer's trace (`review-target/w6t/idle_splits.out`): on the 24 `w6-s7` days still over 6 under A1 there are 58 short SkipQuiet batches, none after a pending entry op, muster, Leave, departure or arrival; 49 are followed within 15 game minutes by an EXPLORE landing in the province; 6 are the season-end flush at bell 1007; 3 unexplained. The cause is in the code as merged: `agents::policy::residency_gate` replaces a resident action on a province that is not resolved through `b − 2` by a nudge, and the keeper's `skip_target` returns the live edge for a nudged province (U2 kept it: a nudge is the latency-relevant target), so every nudge cuts one batch. U3 did not change when bots nudge or Explore. R5's "idle days 0 over 6" is day 0 with 40 EXPLOREs.

I also traced the 3 unexplained splits, all of province (-2,0) (my script over `verify/input.json.gz`, account keys decoded from the wire): the SkipQuiet transactions landed at bells 737.5, 781.8 and 953.6 and end at bells 735, 779 and 951, exactly the landing bell − 2, which is what a nudge asks for (`resident_ok` needs the province resolved through `b − 2`); no DEPART, REVEAL or marchbook march involves those bells. That is a nudge whose bot never acted afterwards. `w6-s7`'s keeper did not record nudges, so this attribution is by signature.

**Fix (review option a).** A day on which a player acted in the province is not idle: §13.4 criterion 3's A1 is amended (v1.13) so an idle province-day also had no resident action (Muster, Dissolve, Garrison, Explore, Depart naming the Province, landed **or refused**: the Province is found by its account key, learnt from the post-states of other transactions) and no keeper-served nudge on that day or in the 26 bells before it (the batch a nudge can cut, `report::split_days`); such days are reported as `resident`. The SkipQuiet that reaches `end_bell` (the season-end flush) is not counted. The keeper now publishes the nudges it takes (`/v1/status` `play.nudges_recent`, the last 288 bells) and the report unions them over every per-bell sample of both keepers, so a nudge with no action after it is attributed too. Option b (bots nudge less) was not taken: the web client nudges the same way, and making players wait for the keeper's 24-bell batch is the latency the nudge exists to remove.

**Evidence.**

- `w6-s7` re-reported with the fixed report on a copy of its run directory (`frontier-stack report --runs-dir <copy>`; the live run directory untouched): idle province-days over 6 **24 → 1**, (-2,0) day 5 (the nudge signature above); idle p99 6, max 7; resident days 31 (16 over 6, max 9, reported); resident actions 4,528; season-end flush 137 SkipQuiet not counted.
- The 3-day rehearsal on the fixed tree (§5), which has the keeper's nudge list: see §5.
- Tests: `report::tests::nudged_or_resident_day_is_not_idle`, `season_end_flush_and_refused_resident_actions` (on the program recording: a refused resident action with no post-state still names its Province), keeper `play::review_tests::nudges_taken_are_published_for_two_days`.

## 2. Major: adversary hold coverage (S27)

**Verified.** `defence-pool` fires only on a live defence claim, and a claim comes only from a Reveal landing ≥ `lateness_slots` after its anchor with a refund (`fclient::play::open_claim`): the below-cap `slots-below` window must delay one that long. `runs/integ-w6t-tri-end/run.md` (R3): `slots-below` held 25 keys for 38 slots, 1 write inside, no claim, `defence-pool` hold-skipped; `integ-w6t-nightly-3`: `defence-pool` and `frontier-fund` skipped. The plan had exactly one `slots-below`.

**Root cause found in this pass.** A claim needs the Reveal to land ≥ `lateness_slots` (4) after its anchor. The keepers bid a Reveal 433, 866, 1,732, then 2,000 (their cap, P_def) milli-lamports per CU in the slots after their first version (`keeper_core::engine::EngineParams::bid`), and the first version goes out a slot after the anchor lands at the earliest. At the old price 1,500 the third version won and the Reveal landed 3 slots after the anchor, one short: a claim opened only when a keeper happened to start a slot late, which is why coverage was a coin toss.

**Fix, in two steps.**

1. `adversary::decide_holds` (one pure function for the hold decisions; the supervisor places the fired holds and writes the events) re-arms `slots-below` while `defence-pool` waits and no live claim is open: a bell after a window ends, at most 12 times, while a new window, a claim and the defence hold fit before the end of play; a re-armed hold that finds nothing is `hold-rearm-expired` (reported), not `hold-skipped`. **This alone was not enough:** the first 3-day rehearsal of this pass (at 1,500) re-armed 12 windows over bells 40–88 without a claim, most of them on the "arrivals are due there today" guess with no Reveal in the held bell; it was stopped at bell ≈ 130 (`frontier-node/.local/frontier/integ-w6t-rv-3d-aborted-1500`).
2. `slots-below` is priced **1,900** (`SLOTS_BELOW_MILLI`): above the keepers' third bid, below their cap, so the delayed Reveal lands on the fourth version, ≥ 4 slots after the anchor, and opens a claim; it still lands (criterion 4 unaffected). A re-armed `slots-below` fires only on a bell the fleet sealed a march to (the marchbook). Chosen over a harness-made claim: a claim is a late Reveal's refund, which the harness could only make by sending a player transaction of its own.

**Tests** (`stack/src/adversary.rs`): `slots_below_delays_a_reveal_past_lateness` (on the keeper's own bid ladder: 1,900 lands on the fourth version, ≥ `lateness_slots`; 1,500 one slot short; below the cap and the season's defence cap); `a_rearmed_slots_below_needs_a_planned_arrival`; `defence_pool_fires_when_only_a_later_slots_below_opens_a_claim` runs the whole schedule over 1, 3 and 7 days against a world where only the third window's Reveal is late enough: `slots-below` fires three times (two re-arms), `defence-pool` once, nothing skipped, nothing re-armed after `defence-pool`; `slots_below_rearms_are_bounded`: with no claim ever, 12 re-arms, then `defence-pool` is still reported skipped (not exit-grade, correctly). The rehearsal (§5) and the final nightlies (§6) ran the final schedule.

## 3. Major: criterion 6's ingest → WS tail (S28, O-M1-28)

**Verified,** and the tail's owner found. Every WS message now carries the herald's send stamp `s` (unix ms when that socket's batch is handed to the socket; `ws::message_sent`), so ingest → WS splits into the herald's share `s − t` and the delivery share receipt − `s`. The bench `herald/tests/burst.rs` replays R5's recording at its real pace (400 ms a slot) from slot 2040 to 2300 in a herald in the test process, with `frontier-viewers` as a **separate process** (4,000 pollers, 1,000 WS viewers on every ring and every opened province): 247 transactions, 72 of them in slots 2075–2076 (the `ticket` hold's release), 1.41 M WS messages. Same herald, same burst:

| generator | ingest → WS p50 / p99 (upper) / max | herald share `s − t` p50 / p99 / max | delivery p50 / p99 / max | file p99 (answered) | load (1 min) |
|---|---|---|---|---|---|
| pre-fix (`aa87235`, built in scratch) | 1.38 s / **3.28 s** / 3.32 s | – (ignores `s`) | – | 49.2 ms (–) | 5.15 → 4.48 |
| fixed | 0.59 s / **0.75 s** / 0.73 s | 0.56 s / 0.69 s / 0.67 s | 0.013 s / 0.086 s / 0.124 s | 8.2 ms (9.2 ms) | 4.44 → 3.94 |

So R5's 2.49 s (and 3.28 s here) was the generator's own backlog: its WS client read each frame unbuffered (3–4 read calls a frame) and parsed every 5.5-KB Province diff into a `serde_json::Value` to read one number. The fixed generator reads its sockets through a 256-KB buffer and reads `seq`, `s` and `t` off the wire without parsing the payload (`viewers::stamps`, with a full-parse fallback for any other shape), and takes the receipt time before any work on the message. The herald's share (0.69 s p99 for a 1.1 M-message burst to 1,000 all-ring sockets) is within the 2-s target without a fan-out change, so neither herald coalescing nor generator sharding was needed.

Also fixed: the herald saves its checkpoints on a blocking worker and returns to the ingest at once, one save in flight (R5's 20–25 s fold stalls: `Ingest::step` awaited the save every 150 slots); `checkpoints_save_in_the_background` holds each save 1.5 s and checks 5+ steps finish in < 1.4 s and a restart from those checkpoints gives the same files. The generator reports the file p99 of the answered (200/304) requests separately (`answered`), and the stack's load verdict prints it and the WS split (reported, not judged).

**O-M1-28** stays with the owner (contract §15, updated): the evidence says the criterion is met as written on R5's burst, so the default (a) needs no amendment; the owner confirms it before the exit run.

## 4. Major: R5's limits; criteria 5 and 8 decided only when exercised (S29)

**Verified** (R5's `report.json`): JOIN 490, TICKET 452 (`w6-s7` 1,000 / 1,004); DEPART 42, REVEAL 42, EXPLORE 40; `bad_seal_codes` `{}`; personas `needs-chain` or `pending` with ≈ 130 `file_ticket` `QuotaExceeded` each. Why only 490 joined: `--season-end-at-play-end` scales `join_close_bell` to 108 (`setup::join_close_for(144)`) while the bots' one-day mix deals the non-day-0 joins from bell 145 (`Mix::for_season_days(1)`), so only day-0 joiners could join. R5's run record now has a **Limits** section: not evidence for criteria 4, 5, 8, criterion 3's idle days, or criterion 6 at the exit load.

**Fix.** The report decides criterion 5 only when every persona is `observed` or, when only the chain can judge it, exercised (a landed Depart for min_tip, garbage_seal, bad_plaintext, squatter, self_tip; two for double_arrival; a prefund; a ticket or hold); a `pending` persona, one never exercised, or a locally checkable one refused with a code its rule does not name leaves 5 **n.a.** Criterion 8 reads the bots' marchbooks: every garbage / bad-plaintext transit sent (not failed) and due must have settled `BAD_SEAL` (bad plaintext with code 5); with none of either kind 8 is **n.a.**; one that settled otherwise fails it. `w6-s7`'s `bots/report.json` covered only the last of four fleet lifetimes (2,107 steps: chaos kills restart `frontier-bots`, which overwrote the report), so a restarted fleet now keeps the earlier one as `report-life-<n>.json` and the report merges them (strongest verdict, counts added, unrevealed marches once each). A late_revealer's `AlreadyDone` (its march was revealed in its window before the late try) is a refusal.

**Evidence.** Re-reported on copies of their run directories: R5 gives 5 **n.a.** and 8 **n.a.** (was pass/pass); `w6-s7` gives 8 **pass** (15 garbage and 5 bad-plaintext transits due, codes 2 and 5) and 5 **n.a.** (its single-lifetime report). Tests `criteria_5_and_8_need_their_personas_exercised`, `bots_lifetimes_merge`, bots `a_restart_keeps_the_previous_report`.

**Persona fixes found by the runs of this pass** (all in `frontier-node/crates/{agents,bots}`; they change no rule, only how the persona bots test theirs and how their verdicts read):

- `settle_racer` never reached its test at 20× (the rehearsal: 5 racers, no re-depart; `w6-s7` and R5: pending). Its window is the few slots between the destination's resolve of the arrival bell and the keepers' SettleTransit, and the bot ran its duties 0–20 game seconds into each bell and waited to see the resolve through the herald. Now, from the bell after its arrival bell (the resolve needs the close `A + W` and the seed round after it) for `RACE_BELLS` (4) bells, it polls every 8 game seconds (`fleet::settle_racer_polls`) and tries its re-depart at each poll without waiting; the relay's simulation refuses the early tries (`NotResident`, `HostBusy`: nothing sent, nothing charged; `RateLimited` when the signer's bucket runs dry) and they neither count as evidence nor end the race (`33f77cc`, `de9bb3d`; tests `the_settle_racer_redeparts_from_the_arrival_bell`, `the_settle_racer_polls_through_its_race`, `a_settle_racer_try_before_the_resolve_is_no_evidence`). A first try of this (resident gate + nudge, `6685252`) still missed the window in the final-tree nightlies. The 20× check `runs/integ-w6t-rv-racer/` observed it (§5).
- A forged SettleTransit refused `TransitState`/`AlreadyDone` (a keeper settled first) never reached the account check and is left out of the forger's verdict (`6b9f078`); a late Reveal refused `TransitState` or `AlreadyDone` is the late_revealer's expected refusal.
- Remaining at nightly scale: in a one-day 100-bot nightly each persona has one bot and one sponsored ticket a game day (the relay's lamport quota covers one Holding escrow a day, `quota.mjs`); a persona whose ticket loses loops on `file_ticket` `QuotaExceeded` until the next game day and is not exercised, so the nightlies' criterion 5 is n.a. At the exit run's 5 bots per persona and 7 game days, the rehearsal exercised every persona but the racer in 3 days.

## 5. Rehearsal: 3 game days at 20× on the fixed tree

`runs/integ-w6t-rv-3d/run.md` (tree `d66fa99`; 11:28–15:19 JST, load p50 4.76, p99 7.94, max 10.03). **Exit-grade environment** (all 9 holds; `slots-below` at 1,900 opened a claim on its first window, `defence-pool` held it at bell 43); verify PASS (47,129 transactions), tamper 30/30; criteria 1, 2, 4, 6, 8, 9 pass; criterion 5 n.a. (settle_racer only, fixed after, see §4); criterion 3 fails only on round → anchor p99 4 slots.

- **Criterion 3's idle days (the blocker): 0 over 6** on days 0–2 of a 1,000-bot, 20× season with 1,259 keeper-served nudges and 1,863 resident actions attributed; idle p99 6, max 6.
- **Round → anchor p99 4**: 65 of 7,312 anchors more than 2 slots late, all in four places: the bells 8 and 10 stall of every 20× run (the stacked `ticket` holds, O-M1-29), bell 148 (the above-cap `keeper-payers` hold), bell 93 (coincides with the machine's load peak, 10.03) and one at 196 (`lag`). None follows a chaos kill. Over 7 days the same fixed windows are ≈ 0.4 % of 16,128 anchors, below the p99 (O-M1-29 unchanged).
- **Criterion 6 at exit load: pass.** ingest → WS p99 0.72 s (herald share 0.69 s, delivery 0.035 s) over 24.9 M messages with 4 herald kills in the window; error rate 0 of 3.45 M; WS coverage 100 %; file p99 9.2 ms (answered 10.2 ms).
- Failed transactions 531 of 47,129: redundancy 416 (PostBeacon `AlreadyDone` 304), expected 81, bot-policy waste 28, unclassified 6.

**The settle racer at 20×** (`runs/integ-w6t-rv-racer/run.md`: 300 bots, 3 per persona, 12 game hours, test key, adversary; binaries of `de9bb3d`): observed, `HostInTransit` after 59 `NotResident` and 24 `RateLimited` tries refused in simulation. The first 20× observation of this persona in any run. The same run showed two more verdicts spoilt by tries that never reached their check (forged settle `TooEarly`, late Reveal `CommitMismatch`), left out since `629d007`.

## 6. Gate items re-run

Every line on 127.0.0.1 ports 41100–41999; the paused `w6-s7` stack untouched. Load averages are in each run record.

| line | tree | result |
|---|---|---|
| W1 frontier-node: `cargo fmt --all -- --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --release --workspace` | `dcfece9`, `6b9f078`, **`629d007`** | exit 0 each; **418 passed** on `629d007` (16:43–16:45, load 8.4 → 7.6) |
| agents/bots/stack/herald/keeper unit and integration tests of the new items | each commit | pass (listed in §1–§4) |
| `w6t_docs_check.py --final` | final | 0 fail |
| W5 smoke: `check-ports`; `up --scale 100 --days 1 --bots 100` (41600); `verify && tamper`; `load --viewers 5000 --game-hours 1`; `down` | `629d007` (and earlier `dcfece9`) | all 0: verify PASS (7,206 txs), tamper base PASS and every required class detected (29 on the run, 1 on a fixture), load pass: file p99 12.3 ms (answered 14.3), ingest → WS p99 0.29 s (herald share 0.30 s, delivery 0.008 s), error rate 0, WS coverage 100 % |
| W6 nightlies ×3 (`m1-nightly.sh --no-build`, 41500) | **`629d007`** (`integ-w6t-rv4-nightly-1..3`, 16:45–17:41) | **three green** (`pass: true`): verify PASS (8,988 / 8,826 / 8,482 txs), tamper 30/30, load pass, report 0; `defence-pool` fired every night (bells 24, 24, 18; ≤ 2 `slots-below` re-arms); criterion 5 **pass** on night 1 (every persona observed or exercised, the settle racer included), n.a. on nights 2–3 (garbage_seal, self_tip: one bot and one ticket a day each, §4); `frontier-fund` hold-skipped (no ring opening in a one-day 100-bot night, as before; test-key nights are never exit-grade). Also three green on `dcfece9` and three on `d66fa99`/`6b9f078` (`integ-w6t-rv-nightly-*`, `integ-w6t-rv2-nightly-*`, `integ-w6t-rv3-nightly-*`) |
| W6 scripted onboarding (`run-onboarding.sh --base-port 41700`) | `d66fa99` binaries (the herald's `s` stamp and checkpoint change; nothing web-side changed after) | 0: JA ok, EN ok (11:41–11:55) |
| W6 latency line (2×, 300 bots, 6 game hours) | not re-run | the keeper changed only by the nudge list in its status; criterion 3's game-second targets are unaffected (integ-W6t's run stands) |
| Rehearsal and racer check | `d66fa99`, `de9bb3d` | §5 |

**Fast-forward:** `codex/frontier` fast-forwarded to `frontier/m1-integ` (this commit) with `git -C .claude/worktrees/frontier-integ merge --ff-only frontier/m1-integ`.

## 7. The exit run

**Ready to start.** Every review finding is fixed and was checked at the exit scale: the idle-day rule (the w6-s7 replay 24 → 1, the rehearsal 0), an exit-grade environment on the first `slots-below` window, criterion 6 at the exit load (0.72 s), criteria 5 and 8 decided on evidence (the settle racer observed at 20×).

**Before starting (owner):** confirm O-M1-28's default (a), now backed by the burst replay and the rehearsal, and O-M1-29's default (criterion 3 unchanged, judged over 7 days).

**Known risk, not fixed here:** criterion 3's round → anchor p99. The 3-day rehearsal had p99 4 slots from fixed windows: the stacked ticket holds at bells 8–10 of every 20× run, the above-cap `keeper-payers` hold, and a load peak. Over 7 days these are ≈ 0.4 % of anchors, which should put the p99 at ≤ 2, but that is an estimate (O-M1-29). Criterion 5 needs each persona exercised: the rehearsal exercised all but the settle racer in 3 days, and the racer is fixed. Keep the machine otherwise idle; the per-bell load is recorded.

The command (no flag changes from integ-W6t §6; the archive stays G0 1788998400; the new run id keeps `w6-s7` as evidence):

```sh
cd /Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/m1-integ
# 1. the owner stops the paused w6-s7 stack (frees 41000-41099; its run directory stays)
frontier-node/target/release/frontier-stack down --run-id w6-s7
# 2. build, pin and check: expect ".so pin d85e1bd7…2281; W6T-1 recorded release d85e1bd7…2281: match"
scripts/m1-run-s7.sh --dry-run --adversary --run-id w6-s7b
# 3. the run (≈ 8.4 h of play + the drain, under caffeinate), then verify, tamper, report; services stay up for triage
scripts/m1-run-s7.sh --adversary --run-id w6-s7b
```

Then check: row E "exit-grade"; verify PASS; tamper 30/30; criteria 1–6, 8, 9 **pass**, not n.a. (5 and 8 are n.a. only if a persona was never exercised); the load verdict's `ws_split` (herald share against delivery); `resident_province_days_over_6`, reported; keeper A `pools.reveal.floor` 215,076,060; the no-landing detector and the per-bell load average.

## Links

- Review targets: `scratchpad/frontier/m1/review-target/w6t/` (`idle_splits.out`, `r5_splits.out`, the example source)
- Contract v1.13 [`M1-CONTRACT.md`](M1-CONTRACT.md) §29, §13.4, §15 O-M1-28; [`../DECISIONS.md`](../DECISIONS.md) S26–S29
- Runs: `runs/integ-w6t-rv-3d/`, `runs/integ-w6t-rv-racer/`, `runs/integ-w6t-rv-burst/`, `runs/integ-w6t-rv4-nightlies.md` (the final-head nightlies and W5 smoke), R5's limits in `runs/integ-w6t-tri-20x/run.md`; run directories under `frontier-node/.local/frontier/integ-w6t-rv*`
- Previous pass: [`integ-W6t-NOTES.md`](integ-W6t-NOTES.md)
