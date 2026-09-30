# integ-w6t-tri-20x: plan R5 on the merged `frontier/m1-integ`

R5 of the w6-s7 triage plan (§3): all units at the exit scale for one game day, real rounds from the G0 archive, the release `.so` pinned. Tree: `frontier/m1-integ` at `36386b0` (the five merges and the integration commits). Release `.so` `d85e1bd7…2281` (875,824 B).

```
FRONTIER_KEEPER_TICK_LOG=1 \
frontier-stack up --mode accel --beacon archive --scale 20 --days 1 --season-end-at-play-end --bots 1000 \
    --run-id integ-w6t-tri-20x --base-port 41300 --chaos --adversary --viewers 5000 --viewer-window-hours 12 \
    --chaos-force herald:2 --chaos-force herald:7 \
    --expect-so-sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281
frontier-stack verify|tamper|report|down --run-id integ-w6t-tri-20x
```

Run record (JST; load average 1/5/15 min):

```
start      2026-09-30 07:27:35  load 4.20 4.04 4.09
up exit 0  2026-09-30 08:53:56  load 4.62 4.87 4.62
verify 0 (PASS, 10,468 txs), tamper 0 (30/30), report 1, down 0 (08:54:03)
```

Per-bell load average p50 4.54, p99 8.09, max 8.98 (bell 60). Nothing else heavy ran: the only other stack was the Gate W6 latency line (41100, 2×, 300 bots) and the paused `w6-s7` stack.

## R5 pass conditions (plan §3)

| condition | result |
|---|---|
| **no `hold-skipped`** | **met: row E "exit-grade"**. All 9 holds fired: ticket (slots 275–2074), anchor (2950–2974), slots-below (3200–3387, 1 write inside: a late Reveal opened a claim), frontier-fund (3200–3274), slots-above (3425–3612), defence-pool (3650–3874, on a real claim), keeper-payers (4100–4174), lag (5250–5399), relay-payers (9850–9924) |
| criteria 1, 2, 4, 5, 8, 9 | **pass**. 1: `end_bell` 144, EndSeason landed, 42 departs settled once, 0 stuck, 42 ClashInputs closable, 0 blocked. 4: 0 ValidSealUnrevealed, none by rule. 5: no persona violated, no honest march refused by rule |
| criterion 3 | **fail** (table below): the two misses are the stacked-hold stall of bells 8–10 and one designed late Reveal; the keeper's own figures meet the targets |
| criterion 6 | **fail on ingest → WS only** (table below) |
| verify / tamper | **PASS / 30 of 30** (29 built from the run; T17 detected) |
| keeper status non-null in every bell | **yes**: keeper A and B 0 of 171 bells unanswered, 0 timeouts (answer p99 24 ms). w6-s7: 322 of 1,035 null |
| no no-landing window ≥ 20 slots (or explained) | **one, explained**: slots 813–1027 (215 slots; bells 8–11). The report says "unexplained" because its detector knows only localnet kills. It is W6T-3 §8's mechanism: the adversary `ticket` hold (1.0 on one key, slots 275–2074) plus the `ticket_holder` persona's holds (0.5, 3 bells when provisional) fill the 100M block; keepers' D bids cap at 0.5 and ties lose to a hold. Same window as w6-s7 (827–1028), U4-tree R5 (828–992) and the preview (815–1028) |
| load average recorded | yes (`metrics/loadavg.jsonl`, the report's series) |

### Criterion 3

| measure | result (p50 / p99 / max, slots) | target p99 | reading |
|---|---|---|---|
| round → anchor | 1 / **81** / 154 | 2 | **miss.** From keeper A's journal: bells 8, 9 and 10 anchored at slots 1028, 1030, 1035 instead of 875, 950, 1025 (the stall above), 48 of 2,304 anchors = 2.1 % of the sample, so they are the p99. In every other bell all 16 regions' anchors landed in the same slot as the bell's first one, except one region held 25 slots by the above-cap `anchor` hold (2950–2974) |
| S → first cache | 1 / **1** / 145 | 2 | met (the max is the same stall) |
| anchor → last reveal | 0 / **38** / 38 (n 42) | 4 | **miss.** One Reveal, the one the below-cap `slots-below` hold (3200–3387) delayed until the keepers escalated past it; it is the Reveal that opened the claim `defence-pool` then held. The hold's design is "reveals land late, one opens a claim"; criterion 4 excludes above-cap windows, criterion 3 excludes none (U4 finding 3). n = 42, so one Reveal is the p99 |
| S → resolve | 2 / **3** / 3 | 8 | met |
| idle province-days over 6 SkipQuiet (A1) | **0** (17 idle days, p99 6, max 6); churned max 30 (reported) | 0 | met |

Against the same line elsewhere: U4 tree (7dcacdf keeper) round → anchor p99 45, S → cache p99 2, S → resolve p99 4; the preview 81 / 1 / 3 / 38-class 37; `w6-s7` (7 days) 6 / 5 / 13 with the **same** stall (round → anchor max 154) diluted below the p99 by 16,128 anchors, anchor → last reveal p99 0 over 1,628 Reveals.

### Criterion 6 (in-run window, 5,000 viewers, 12 game hours)

| measure | result | target |
|---|---|---|
| error rate (A3) | 1 of 1,726,501 (requests + WS sessions) = 0.00006 %: one request failed at the herald kill of 1790722376 (wall ms 1790722376996) inside its outage window | < 0.1 % — met |
| WS coverage outside the 3 outage windows | 100 % | ≥ 99 % — met |
| stale retries / WS reconnects | 12,000 / 3,000, all inside the kill windows (0 / 0 outside) | met |
| file p99 | 13.82 ms (404 share 764,142 of 1,725,501 requests, the not-yet shapes of the live bells, as the web client's) | ≤ 250 ms — met |
| ingest → WS p99 | **2.49 s** (bucket upper bound; p50 0.090 s, max 2.487 s; 11,070,204 messages) | ≤ 2 s — **miss** |

Where the ingest → WS tail comes from (a 30-s sampler of the generator's `/stats` and the herald's `/h/status`, scratch `integ-w6t/gate/R5-samples.txt`): the WS messages grow by ≈ 100,000 per 30 s (1,000 sockets, all rings subscribed) and p99 was **0.44 s** through bell 24 (2.11 M messages). In the next 30 s, bell 25, **1.14 M** messages arrived at once and p99 went to 2.36 s: the `ticket` hold ended at slot 2074 and ≈ 100 transactions landed in slots 2075–2099. Three more bursts of 0.5–1.1 M messages came after the herald kill-restart of 1790722376 (3.0 s) and at bells ≈ 57 and ≈ 72. So the tail is the fan-out of bursts (≈ 1,100 messages per socket at once, to 1,000 sockets in one generator process), not steady-state latency and not ingest: the stamp is taken at the pull. The herald's fold lag also reached 20–25 s three times without a kill (the inline checkpoint every 150 slots, `runner.rs` `step` awaits `checkpoint_async`); those stalls did not move the WS p99. The preview R5 gave 2.36 s the same way; the earlier figures (w6-s7 0.41 s, U4 tree 0.44 s) covered ≤ 17 % of the window because the old generator never reconnected.

### Other figures

- Failed transactions: 436 (the U4 tree 35,643; `w6-s7` 37,042 over 7 days): redundancy 405 (PostBeacon AlreadyDone 240, **Reveal AlreadyDone 109 against 42 landed**, OpenProvince 20, SweepPoolOwed 18, Join 10, SettleTransit TransitState 8), expected 25, waste 2 (Explore/Muster NotResident, 1 each), unclassified 4 (CloseArrivalSlot `BadAccount`: keeper A's four versions of `close:slot:1,-3:44:4` were queued behind the above-cap `keeper-payers` hold, 4100–4174, and all landed at slot 4175 after the slot was closed; an A/B race the report does not class yet).
- Bots' `NotResident`: 2 of 186 Depart/Explore/Muster attempts (1.1 %; w6-s7 47 of 4,396, 1.1 %).
- Keeper A status at the end: anchor p99 1, seed p99 1 slot; reveal pool effective N 150, floor 215,076,060.
- Chaos: 7 kills, 7 restarts, 0 crashes (herald ×4 incl. the two forced, drand-replay, bots, keeper A).

## Limits (integ-W6t review, added 2026-09-30)

R5 is a one-day sample at the exit scale; several of its passes carry no evidence, and it is **not** evidence for criteria 4, 5 or 8, nor for criterion 3's idle days or criterion 6 at the exit run's load:

- **Population.** With `--days 1 --season-end-at-play-end` the season's `join_close_bell` is scaled to 108 (`setup::join_close_for(144)`), while `frontier-bots`' one-day mix deals the joins that are not on day 0 from bell 145 (`Mix::for_season_days(1)`, `with_join_close` keeps ≥ 145). Only day-0 joiners could join: JOIN 490, TICKET 452 (`w6-s7`: 1,000 / 1,004). The persona and archetype bots that did join looped on `file_ticket` `QuotaExceeded` (≈ 130 each).
- **Activity.** 42 Departs, 42 Reveals, 40 EXPLOREs (`w6-s7`: 1,706 DEPART, 1,656 REVEAL and 1,080 EXPLORE records). Its 10,468 transactions match `w6-s7`'s day 0 (10,356), not days 4–6 (25–27k a day).
- **Criteria 4, 5, 8 passed vacuously.** `bad_seal_codes` was `{}`: no garbage_seal, bad_plaintext or settle_racer transit existed, so criterion 8's "the bad-seal personas held" and criterion 4's "0 of 42" say nothing; criterion 5 passed with personas only `needs-chain` or `pending`. The report now decides 5 and 8 only when exercised (contract v1.13 §13.4, §29): re-reported on a copy of the run directory with the fixed report, R5 gives 5 and 8 **n.a.** (and 4, 3, 6, E unchanged)
- **Criterion 3's idle days** ("0 over 6") are day 0 only, with 40 EXPLOREs; `w6-s7`'s idle-day failures were all on days 2–6. The idle-day rule was amended (A1, v1.13) and checked on `w6-s7` and on the 3-day rehearsal `runs/integ-w6t-rv-3d/`.
- **Criterion 6's ingest → WS tail** was the generator's, not the herald's (contract §15 O-M1-28, v1.13).

