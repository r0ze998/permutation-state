# integ-W6t review response: idle days, defence-pool coverage, the WS tail, persona evidence

A review of integ-W6t raised one blocker and three majors. Each was checked against the code and the `w6-s7`, R3 and R5 data before it was fixed; all four were confirmed (no rebuttal). Fixed on `frontier/m1-integ` (code `dcfece9`, docs and run records in the commit after it); contract v1.13 (§29), DECISIONS S26–S29. The paused `w6-s7` stack (41000–41099) was only read; every service this pass started used 41100–41999 and was stopped. No push, no install or download, no devnet/mainnet transaction; `permutation-server/web/session.mjs` unchanged.

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

**Fix.** The hold decisions moved into one pure function, `adversary::decide_holds` (the supervisor places the fired holds and writes the events). While `defence-pool` waits and no live claim is open, a `slots-below` whose window ended a bell ago is re-armed on the next planned arrivals: at most 12 times, and only while a new window, a claim and the defence hold fit before the end of play. A re-armed hold that finds nothing before its deadline is `hold-rearm-expired` (reported), not `hold-skipped`, since the kind has fired. Chosen over a harness-made claim: a claim is a late Reveal's refund, which the harness could only make by sending a player transaction of its own.

**Tests** (`stack/src/adversary.rs`): `defence_pool_fires_when_only_a_later_slots_below_opens_a_claim` runs the whole schedule over 1, 3 and 7 days against a world where only the third `slots-below` window's Reveal is late enough (a few Reveals): `slots-below` fires three times (two re-arms), `defence-pool` fires once, nothing is skipped, and nothing is re-armed after `defence-pool`; with the first window's claim there is no re-arm. `slots_below_rearms_are_bounded`: with no claim ever, 12 re-arms then `defence-pool` is still reported skipped (not exit-grade, correctly); a short play stops re-arming when the hold no longer fits. The rehearsal (§5) and the nightlies (§6) ran the new schedule.

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

## 5. Rehearsal: 3 game days at 20× on the fixed tree

RESULTS_PLACEHOLDER

## 6. Gate items re-run

GATE_PLACEHOLDER

## 7. The exit run

EXIT_PLACEHOLDER

## Links

- Review targets: `scratchpad/frontier/m1/review-target/w6t/` (`idle_splits.out`, `r5_splits.out`, the example source)
- Contract v1.13 [`M1-CONTRACT.md`](M1-CONTRACT.md) §29, §13.4, §15 O-M1-28; [`../DECISIONS.md`](../DECISIONS.md) S26–S29
- Runs: `runs/integ-w6t-rv-3d/`, `runs/integ-w6t-rv-nightly-{1,2,3}/`, `runs/integ-w6t-rv-w5-smoke/`, `runs/integ-w6t-rv-onboarding/`, R5's limits in `runs/integ-w6t-tri-20x/run.md`
- Previous pass: [`integ-W6t-NOTES.md`](integ-W6t-NOTES.md)
