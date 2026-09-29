# u4-tri-20x: plan R5 on the U4 tree (7dcacdf + W6T-4)

This is R5 of the w6-s7 triage plan (§3): one game day at the exit scale with real rounds. It exercises criterion 3 at 20× under full load, criterion 6 with forced herald kills, and adversary coverage.

The run used `frontier/m1-w6t-U4` at `bacc957` (7dcacdf plus U4, before `4ef5b69`). **U1–U3 are not in this tree**: the keeper, viewer generator, bots and verifier are 7dcacdf's. The main session re-runs R5 with the same line on the merged tree.

```
frontier-stack up --mode accel --beacon archive --scale 20 --days 1 --season-end-at-play-end --bots 1000 \
    --run-id u4-tri-20x --base-port 41300 --chaos --adversary --viewers 5000 --viewer-window-hours 12 \
    --chaos-force herald:2 --chaos-force herald:7 \
    --expect-so-sha256 072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b
frontier-stack verify|tamper|report|down --run-id u4-tri-20x
```

The archive is `.claude/data/drand-archive-quicknet-g0-1788998400` with G0 1788998400. The release `.so` is `072b1205…a98b`, the one 7dcacdf builds.

Run record (UTC; load average 1/5/15 min):

```
start         2026-09-29T17:11:51Z  load 4.52 3.97 4.66
up exit 0     2026-09-29T18:38:12Z  load 2.87 3.74 3.84   (86 min; 53 s of setup)
verify 0, tamper 0, report 1, down 0 (18:38:24Z)
```

Nothing else of this unit ran during play. The per-bell load average was p50 3.45, p99 10.87 and max 11.24 at bell 120.

`report.md` here was regenerated after the run at `4eee1dc`, the commit where the no-landing detector leaves the drain out. The original listed five drain gaps as well; everything else is identical.

## U4's pass conditions

| condition | result |
|---|---|
| CreateSeason with `end_bell` = play bells | `end_bell` 144, `join_close_bell` 108, accepted with the release `.so` and real rounds |
| archive guard on the actual genesis | ok. Setup overran the planned `LEAD_SECS` by 1,452 s, the same as w6-s7, so it is structural. The archive has 544,488 s to spare after `until` = 1,789,191,912 |
| reaches `end_bell`, EndSeason, the drain | yes: EndSeason on the first attempt, then 26 drain bells. Criterion 1 **pass**: 44 departs, all settled once, 0 stuck, ClashInputs 43 closable, 0 blocked |
| forced herald kills in the viewer window | game 3 h and 8 h (`--chaos-force herald:2`, `herald:7` after the viewer start at 1 h), each restarted after 3.0 s wall (60 game s). The random plan also killed the herald once in the window |
| per-second viewer sampling and outage windows | 1,789 samples (1,735 judged, the rest ramp, wind-down or inside an outage); 3 outage windows from `events.jsonl`; 40 samples inside them |
| holds | **8 of 9 fired**: ticket, slots-below and slots-above (bell 22, both on (2,-3): the first planned-arrival targeting), frontier-fund (bell 33, ring 4), anchor (bell 36), keeper-payers (bell 52), lag (bell 67), relay-payers (bell 128). **defence-pool skipped**: no claim opened, so row E is **not exit-grade** |
| keeper status | A: 1 of 171 bells unanswered, 0 timeouts. B: 0 |
| no-landing detector | **one** window: slots 828–992 (165 slots, 119 of them after bell 9 started), bells 8–10. This is w6-s7's window again (827–1028, bells 8–11) at the same point: the ticket hold (slots 350–2074) is active and the viewer window has just started. W6T-3 explains it as concurrent holds filling the block |
| verify / tamper | PASS (no fail codes) / 29 of 29; `.so` pinned (V2 against the build record) |

## What this run found in U4's own code (fixed after it)

- **The slot holds held slots no Reveal needed** (0 writes inside or after the window).
  - Both slot holds took (2,-3). The marchbook had two *sealed* marches to it for bell 23, but both Departs had failed. The one real arrival at bell 23 was at (1,-3).
  - Fixed in `5e7d896`: a planned march counts only with a `sent` line and no `failed` line.
- **`slots-below` and `slots-above` fired in the same probe on the same province.** The 3.0 hold masks the 1.5 hold, whose keeper escalation is the point. Fixed in `4ef5b69`: the second slot hold takes another province.
- **Drain gaps listed as stalls.** Fixed in `4eee1dc`.

## The criteria this tree cannot pass (U1–U3 not merged)

| criterion | this tree | owner |
|---|---|---|
| 3 | **fail**: round → anchor p99 **45** slots (max 118); S → first cache p99 2; anchor → last reveal p99 0; S → resolve p99 4 (all ≤ target). Idle province-days over 6: **0** (A1 applied; churned max 29). The anchor tail comes from chaos: keeper-a kill -9 (1.42 s) and drand-replay kill (2.78 s) with the 7dcacdf keeper's slow first tick (cause C) | U2 |
| 6 | **fail**: error rate 0.0076 (13,137 / 1,729,824 requests + WS sessions); WS coverage **16.8%**. The generator has no recovery (`generator_recovery: false`): each herald kill costs 4,000 poller errors, and its WS viewers never come back after the first kill. p99 file 7.2 ms, ingest → WS p99 0.44 s, gaps 0 | U3 (generator) |
| waste | 35,643 failed transactions: keeper waste 35,315 (CloseArrivalDay 28,080 and CloseArrivalSlot 2,922 ProgramFailedToComplete in the drain; PostAnchor/Multi BadData 4,313 ≥ end_bell), redundancy 286, expected 42 | U1, U2 |
