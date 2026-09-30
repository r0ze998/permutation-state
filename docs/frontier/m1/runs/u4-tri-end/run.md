# u4-tri-end: plan R3 on the U4 tree (7dcacdf + W6T-4)

This is R3 of the w6-s7 triage plan (§3): the season end, the drain, rule refusals and waste. It ran on `frontier/m1-w6t-U4` at `88e905d`, which is 7dcacdf plus U4. **U1–U3 are not in this tree**, so the U1–U3 pass conditions of R3 show the *before* picture. The main session re-runs R3 with the same line on the merged integration tree.

```
frontier-stack up --mode accel --beacon test-key --scale 100 --days 2 --season-end-at-play-end --bots 300 \
    --run-id u4-tri-end --base-port 41200 --chaos --adversary
frontier-stack verify|tamper|report|down --run-id u4-tri-end
```

Run record (UTC; load average 1/5/15 min):

```
start         2026-09-29T16:38:25Z  load 7.69 7.06 9.73
up exit 0     2026-09-29T17:10:47Z  load 3.36 3.74 4.64   (32 min 22 s, 56 s of setup)
verify 0, tamper 0, report 0, down 0 (17:11:00Z)
```

The per-bell load average during the run (`metrics/loadavg.jsonl`) was p50 3.95, p99 9.39 and max 10.56 at bell 123. A nightly (`u4-nightly-2`, base 41500) ran at the same time.

## U4's pass conditions

| condition | result |
|---|---|
| CreateSeason with `end_bell` = play bells | `end_bell` 288, `join_close_bell` 216, accepted |
| reaches `end_bell`, EndSeason, the drain | yes: EndSeason landed on the first attempt; 26-bell drain; last bell 314 |
| criterion 1 | **pass**: 0 stuck province-bells; 89 departs due, all settled exactly once; ClashInputs 107 closable, 0 blocked |
| verify / tamper | PASS (no fail codes) / 29 of 29 |
| holds | 7 of 9 fired: ticket, anchor, slots-below (bell 76), keeper-payers, lag (bell 132, origin of 1 transit in flight), frontier-fund (bell 143, ring 4), relay-payers. Skipped: **slots-above** (still one-hour armed in this build; fixed in `af315d9`) and **defence-pool** (no claim opened) → row E **not exit-grade** |
| report sections | failed-tx classes, per-bell load, keeper status (A: 2 of 315 bells unanswered, 0 timeouts), no-landing (0 windows), row E |
| chaos | 11 kills, 11 restarts, 0 crashes |

## The U1–U3 conditions (before the merges)

| R3 condition | this tree |
|---|---|
| 0 PostAnchor/Multi BadData | **3,176** (2,672 + 504; anchors ≥ end_bell, U2.5) |
| 0 failed CloseArrivalDay/Slot | **7,214** (ProgramFailedToComplete in the drain, U1.2 / U2.6) |
| 0 DEPART with arrive ≥ end_bell | 0 in this run (no march happened to arrive past 288; U1.1 makes it impossible) |
| 0 Reveal WrongStatus, 0 Shielded | 0 and 0 |
| criterion 4 = 0 | pass (0 ValidSealUnrevealed) |
| criterion 5 | pass |
| A/B duplicate settle failures < 5% of landed | 180 (SettleDeparture AlreadyDone 91 + SettleTransit TransitState 89) against 89 departs: about 100%. Keeper B's `backup_delay_slots = 8` needs the W6T-2 keeper; this keeper does not know the key, so it was not written |

Failed transactions: 10,612 = waste 10,413 (keeper 10,390, bot-policy 23) + redundancy 188 + expected 8 + unclassified 3. The 3 (SkipQuiet OutOfOrder 2, Train Insufficient 1) are classified from `bacc957` on.
