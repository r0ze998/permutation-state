# p-tri-end: plan R3 on a preview of the merged tree (U1 + U2 + U3 + U4)

This is R3 of the w6-s7 triage plan (§3) on a **preview integration tree**. The tree is a scratch worktree `m1-w6t-U4-preview`, detached at U4's `bacc957`. Each unit's changed files were checked out over it from the unit's branch head. No commit was made and no branch was touched.

| unit | head | paths (exclusive per §11) |
|---|---|---|
| U1 W6T-1 | `3676f1a` | permutation-frontier, frontier-abi (release `.so` `d85e1bd7…2281`, test-beacon `b2cef4a7…a4e7`) |
| U2 W6T-2 | `30e791c` | frontier-node/crates/keeper |
| U3 W6T-3 | `176138c` | verify, agents, bots, herald viewers, localnet tests, fclient, gateway relay |
| U4 W6T-4 | `bacc957` | stack, configs, scripts |

The units' paths are disjoint and no manifest changed, so this overlay equals the merge. **It is not the integrator's merge.** The main session's R3 on `frontier/m1-integ` after the merges is still the record of Gate W6.

```
frontier-stack up --mode accel --beacon test-key --scale 100 --days 2 --season-end-at-play-end --bots 300 \
    --run-id p-tri-end --base-port 41200 --chaos --adversary
frontier-stack verify|tamper|report|down --run-id p-tri-end
```

Run record (UTC; load average 1/5/15 min):

```
start         2026-09-29T18:39:02Z  load 3.04 3.67 3.81
up exit 0     2026-09-29T19:11:25Z  load 2.76 3.17 3.36
verify 0, tamper 1, report 0, down 0 (19:11:36Z)
```

A preview nightly (`p-nightly-1`, base 41500) ran at the same time. The per-bell load average was p50 3.49, p99 4.81 and max 5.04.

## R3 pass conditions (plan §3)

| condition | result |
|---|---|
| criterion 1 pass (end_bell, EndSeason, 0 unsettled due, 0 stuck) | **pass**: `end_bell` 288, EndSeason landed, 94 departs due and all settled once, ClashInputs 173 closable and 0 blocked |
| verify PASS | **PASS** (no fail codes) |
| tamper 30/30 | **29/30**: T17 "skip over an arrival" gave ClashReplayMismatch and RevealAfterLatch, not SkipNotQuiet/SkipOverArrival. The verdict is still FAIL, but the class counts as missed. The preview nightly detected T17 (30/30), so this depends on the run. **For W6T-2 (skip batches) and W6T-3 (T17's construction).** |
| 0 DEPART with arrive ≥ end_bell | 0 |
| 0 PostAnchor/Multi BadData | 0 |
| 0 failed CloseArrivalDay/Slot | 0 |
| 0 Reveal WrongStatus, 0 Shielded | 0 and 0 |
| criterion 4 = 0 | pass (0 ValidSealUnrevealed; by rule: none) |
| criterion 5 | pass |
| bots' `NotResident` ≤ the w6-s7 rate | 18 of 16,525 txs (0.11%); w6-s7 had 47 of 177,765 (0.03%). Higher per transaction, but a 300-bot, 2-day run is not comparable, so R5 is the check |
| A/B duplicate settle failures < 5% of landed | 1 (SettleTransit TransitState) of 188 landed settles: 0.5% (U4-tree R3: about 100%). Keeper B ran with `backup_delay_slots = 8`, written because this keeper knows the key |

Holds: **9 of 9 fired**, including `defence-pool` at bell 65, the first time in any U4 run. There is no `hold-skipped`. Row E is not exit-grade only because this is a test-key run without a pin.

Failed transactions: 70 in all. Waste 20 (bot-policy), redundancy 38 (Reveal AlreadyDone 28), expected 12. The U4 tree had 10,612.
