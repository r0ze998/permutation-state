# p-tri-20x: plan R5 on a preview of the merged tree (U1 + U2 + U3 + U4)

This is R5 of the w6-s7 triage plan (§3) on the same kind of **preview integration tree** as `p-tri-end`. The tree is the scratch worktree `m1-w6t-U4-preview`, with each unit's changed files checked out over U4. It is uncommitted and not the integrator's merge.

| unit | head |
|---|---|
| U1 | `3676f1a` (release `.so` `d85e1bd7…2281`, 875,824 B, equal to W6T-1's recorded build) |
| U2 | `30e791c` |
| U3 | `176138c` |
| U4 | `5e7d896` |

```
frontier-stack up --mode accel --beacon archive --scale 20 --days 1 --season-end-at-play-end --bots 1000 \
    --run-id p-tri-20x --base-port 41300 --chaos --adversary --viewers 5000 --viewer-window-hours 12 \
    --chaos-force herald:2 --chaos-force herald:7 \
    --expect-so-sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281
frontier-stack verify|tamper|report|down --run-id p-tri-20x
```

Run record (UTC; load average 1/5/15 min):

```
start         2026-09-29T19:12:10Z  load 3.90 3.39 3.43
up exit 0     2026-09-29T20:38:31Z  load 3.00 3.57 3.47
verify 0, tamper 1, report 1, down 0 (20:38:38Z)
```

Nothing else of this unit ran. The per-bell load average was p50 3.42, p99 5.82 and max 5.95 at bell 14.

## R5 pass conditions (plan §3)

| condition | result |
|---|---|
| **no `hold-skipped`** | **met: row E "exit-grade environment"**, all 9 holds fired: ticket (bell 2), anchor (36), slots-below (40, a planned arrival at (2,-3)), frontier-fund (40), slots-above (42, (1,-3), another province), defence-pool (45, 1 open claim), keeper-payers (52), lag (79), relay-payers (128). `slots-below` got 1 write inside its window: the keepers escalated past 1.5 |
| criteria 1, 2, 4, 5, 8, 9 | **pass**. 1: `end_bell` 144, EndSeason, 37 departs settled once, 0 stuck. 4: 0 ValidSealUnrevealed; by rule: bounced 1 |
| criterion 3 | **fail**; details in the next table |
| criterion 6 | **fail on ingest → WS only**; details in the next table |
| verify / tamper | PASS / **29 of 30**: T17 missed again, as in `p-tri-end`. The nightly `p-nightly-1` detected it, so it depends on the run. **For W6T-3 / W6T-2** |
| keeper status non-null every bell | yes: A and B 0 of 171 unanswered, 0 timeouts |
| no no-landing window ≥ 20 slots (or explained) | **one**: slots 815–1028 (214 slots; 155 after bell 9 started), bells 8–11. It is the w6-s7 window again (827–1028) and the U4-tree R5 window (828–992). W6T-3 explains it (`e41e241`: three concurrent holds fill the block) but did not fix it |
| load average recorded | yes (per bell in `metrics/loadavg.jsonl`; start and end above) |

Criterion 3:

| measure | result | target | why |
|---|---|---|---|
| S → first cache p99 | **1** slot | 2 | met |
| S → resolve p99 | **3** slots | 8 | met |
| idle province-days over 6 SkipQuiet | **0** (A1; idle max 6, churned max 24) | 0 | met |
| round → anchor p99 | **81** slots (p50 1, max 154) | 2 | **Miss.** The tail is the no-landing window of bells 8–11: 16 regions × 3–4 bells ≈ 64 of 2,304 anchors wait up to 154 slots. The U4 tree (7dcacdf keeper) had 45 |
| anchor → last reveal p99 | **37** slots (n 36) | 4 | **Miss.** One province-bell's last Reveal came 37 slots after its anchor. This record does not identify which one. The likely cause is the above-cap `slots-above` hold on (1,-3) bell 43 (slots 3,350–3,537, no write inside): the hold's design says "reveals wait". Criterion 4 excludes above-cap hold windows; criterion 3 does not |

Criterion 6:

| measure | result | target | why |
|---|---|---|---|
| error rate | **0** (0 / 1,726,425 requests + WS sessions) | < 0.1% | met |
| WS coverage outside the outages | **100%** | ≥ 99% | met. 3 outage windows: 2 forced kills and 1 random herald kill |
| stale retries / WS reconnects | 12,000 / 3,000, all inside the kill windows | none outside | met |
| unavailable | 5,812 requests waited for a restarted herald | – | met |
| file p99 | **13.8** ms | 250 ms | met |
| ingest → WS p99 | **2.36** s (bucket upper bound; p50 0.106 s, max 2.38 s, 11,077,325 messages) | 2 s | **Miss.** This is the first measurement of the WS half over the whole window: 1,000 WS viewers connected for 12 game hours. w6-s7's 0.41 s covered only its first 2.9 hours, and U4-tree R5's 0.44 s about 17% of the window. The fold lag passed 2 s in only 18 of 1,787 one-second samples, none inside an outage, so the tail looks like WS fan-out under full load, not ingest. **For the herald owner and the architect** |

Failed transactions: 436, **0 waste** (the U4 tree had 35,643). Redundancy 395, expected 41.
- Redundancy: PostBeacon AlreadyDone 240, **Reveal AlreadyDone 109 against 36 landed Reveals**, OpenProvince 20, SweepPoolOwed 13, Join 10, SettleTransit TransitState 3.
- The Reveal duplicates are the largest A/B redundancy left. Keeper B's `backup_delay_slots` covers only the settles: **for W6T-2**.
