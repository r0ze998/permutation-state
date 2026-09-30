# integ-w6t-tri-end: plan R3 on the merged `frontier/m1-integ`

R3 of the w6-s7 triage plan (§3), on the integration tree after the five `--no-ff` merges (U1 `3676f1a`, U2 `f82d57e`, U3 `176138c`, U4 `99aae98`, U5 `5cb49f1`) and the integration commits up to `36386b0`. Test-beacon `.so` `b2cef4a7…a4e7` (876,328 B).

```
frontier-stack up --mode accel --beacon test-key --scale 100 --days 2 --season-end-at-play-end --bots 300 \
    --run-id integ-w6t-tri-end --base-port 41200 --chaos --adversary
frontier-stack verify|tamper|report|down --run-id integ-w6t-tri-end
```

Run record (JST; load average 1/5/15 min):

```
start      2026-09-30 06:03:54  load 13.82 25.77 24.79   (Gate part A was running: sim tests, node tests)
up exit 0  2026-09-30 06:37:55  load 4.67 5.70 8.50
verify 0 (PASS, 16,324 txs), tamper 0 (30/30), report 0, down 0 (06:38:07)
```

Per-bell load average: p50 6.05, p99 27.07, max 27.66 (bell 119). The run overlapped Gate part A for its first 22 minutes, the `integ-w6t-latency` line (41100) throughout and the first nightly (41500) at the end.

## R3 pass conditions (plan §3)

| condition | result | w6-s7 / U4 tree (`u4-tri-end`, 7dcacdf keeper) |
|---|---|---|
| criterion 1 pass (end_bell, EndSeason, 0 unsettled due, 0 stuck) | **pass**: `end_bell` 288, EndSeason landed (1 attempt), 82 departs due and all settled once, 0 stuck, ClashInputs 122 closable, 0 blocked | w6-s7: 9 unsettled (fail) |
| verify PASS | **PASS** (no fail codes) | w6-s7: FAIL (CampMismatch ×2, MissingData ×9) |
| tamper 30/30 | **30/30** (29 built from the run; T17 detected: the integ-W6t fix, §2 of the notes) | preview `p-tri-end`: 29/30 (T17 missed) |
| 0 DEPART with arrive ≥ end_bell | 0 (no `ArrivalAfterEnd`, no `Depart: ArrivalBell` failure: the bots clamp) | w6-s7: 9 |
| 0 PostAnchor/Multi BadData | 0 | U4 tree: 3,176 |
| 0 failed CloseArrivalDay/Slot | 0 (46 + 82 landed; worst 6,018 / 6,538 CU of 8,000) | U4 tree: 7,214 |
| 0 Reveal WrongStatus, 0 Shielded | 0 and 0 (2 `Reveal: TransitState`, a reveal after the settle; unclassified by the report) | w6-s7: 27 / 61 |
| criterion 4 = 0 | **pass** (0 ValidSealUnrevealed; by rule: none) | w6-s7: 27 |
| criterion 5 | **pass** (no persona violated; no honest march refused by rule) | |
| bots' `NotResident` ≤ the w6-s7 rate | 20 (Depart 6, Explore 4, Muster 10) of 293 such actions, 6.8 %; the U4 tree at the same scale 20 (6 / 7 / 7), the preview 18. w6-s7 (20×) 47 of 4,396, 1.1 %. At 100× a bell is 6 wall s, so the bots meet unresolved provinces more often; the same-scale comparison is unchanged, and R5 (20×) has 2 | |
| A/B duplicate settle failures < 5 % of landed | **1** (SettleTransit TransitState) of 164 landed settles, 0.6 % | U4 tree: ≈ 100 % |

Holds: 8 of 9 fired. **`defence-pool` was `hold-skipped`** ("nothing to hold before the deadline"): `slots-below` held 25 keys at 1.5 for 38 slots and the keepers wrote inside it (1 write), but no Reveal was late enough to open a defence claim. The preview R3 had 9/9; R5 below had 9/9. Row E is "not exit-grade" for that skip, the test key and the missing pin (a test-key run is never exit-grade). This is not an R3 condition; it is the open risk of §4 in the notes (a 7-day run whose one `slots-below` opens no claim is not exit-grade).

Failed transactions: 62 (w6-s7 37,042 over 7 days; U4 tree 10,612 for this line): redundancy 25 (PostBeacon AlreadyDone 16, OpenProvince AlreadyDone 8, SettleTransit TransitState 1), waste 24 (bot policy: Muster/Depart/Explore NotResident 20, Build QueueFull 3, Train Insufficient 1), expected 11 (SettleTicket NoTicket 9, SkipQuiet OutOfOrder 2), unclassified 2 (Reveal TransitState).

Catch-up (100×, reported): idle province-days 43, SkipQuiet p99 6, max 6, 0 over 6. Latencies at 100× (n.a.): round → anchor p99 2, S → first cache p99 2, S → resolve p99 6 slots. No no-landing window ≥ 20 slots.
