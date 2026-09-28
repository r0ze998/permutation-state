# Stack run `w6a-nightly-3`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `GCbwtntocEEP5uF1599Uh4oXHBYM1YBa29zSo6bWShqx` (`.so` sha256 `094dc2d69a351f2f7289abb70fb5eea8224c611bfef28266bf9f88fda8eb39c5`, 874624 B)
- source: verify input (6740 transactions, last bell 170)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 3 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 35 max 35 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes []; 6740 txs, read 0.21 s, verify 0.50 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (25 built from the run; 3.01 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 53.25 ms, error rate 0, ingest → WS p99 0.28 s (ws-stamp; fold lag p99 4 s) → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328984 | 328984 | 328984 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 25 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 509 | 0 | 377306 | 380242 | 380718 | 400000 | no | 1177 |
| PostSeed | 2720 | 0 | 337317 | 339561 | 339646 | 345000 | no | 781 |
| PostBeacon | 2208 | 51 | 330625 | 332729 | 332873 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 143354 | 147501 | 147501 | 220000 | no | 400 |
| FoldOccupancy | 523 | 1 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 50 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 46 | 0 | 14211 | 16051 | 16051 | 17000 | no | 575 |
| SettleTicket | 46 | 63 | 20160 | 21540 | 21540 | 40000 | no | 562 |
| Harvest | 30 | 0 | 10140 | 12161 | 12161 | 17500 | no | 427 |
| Build | 22 | 0 | 11598 | 15948 | 15948 | 22000 | no | 428 |
| Train | 26 | 0 | 10364 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 5 | 1 | 16109 | 16557 | 16557 | 25000 | no | 466 |
| Explore | 2 | 0 | 12164 | 14103 | 14103 | 20000 | no | 471 |
| SettleExplore | 2 | 0 | 8275 | 8346 | 8346 | 15000 | no | 396 |
| Depart | 3 | 0 | 18395 | 18397 | 18397 | 24500 | no | 712 |
| Reveal | 3 | 1 | 18054 | 20404 | 20404 | 26000 | no | 829 |
| SettleDeparture | 3 | 3 | 6252 | 6272 | 6272 | 48000 | no | 331 |
| SettleTransit | 3 | 3 | 62433 | 62435 | 62435 | 85000 | no | 794 |
| SweepPoolOwed | 3 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 6 | 0 | 14259 | 37074 | 37074 | 49000 | no | 1197 |
| ResolveFromInputs | 3 | 0 | 32759 | 32826 | 32826 | 340000 | no | 465 |
| SkipQuiet | 289 | 35 | 42270 | 54286 | 56153 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 1 | 0 | 5920 | 5920 | 5920 | 8000 | no | 371 |
| CloseArrivalSlot | 3 | 0 | 6461 | 6461 | 6461 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 3, p50 18054, p90 20404, p99 20404, max 20404.

## Keeper latencies (a slot is 40 game s)

Round → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 3 | 5 | 2 | 118 | 158 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 3 | 3 | 2 | 98 | 138 | 5 s |
| anchor_to_last_reveal_slots | 3 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| close_to_resolve_slots | 3 | 6 | 7 | 7 | 8 | 240 | 280 | 60 s |

Round → anchor from the publication instant, in slots: p50 2.95, p99 3.95, max 5.95.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 43 | 1 | 6 | 6 | 0 |
| churned | 29 | 6 | 35 | 35 | 2 |
| all | 72 | 6 | 35 | 35 | – |

**ClashInputs:** 0 closed, 3 open: 3 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2208,"BUILD":22,"CAMP":13,"CLASH":3,"CLOSE":4,"DEPART":3,"DEPARTURE_SETTLED":3,"DIVERT":6,"EXPLORE":2,"EXPLORE_RESULT":2,"FOLD":523,"GATHER":6,"GENESIS_SEED":1,"HARVEST":30,"HOLDING_FINAL":2,"JOIN":50,"MUSTER":5,"POOL_SWEEP":3,"PROVINCE_OPEN":37,"REVEAL":3,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":46,"SKIP":289,"TICKET":46,"TRAIN":26,"TRANSIT_SETTLED":3}
- transits: {"outcome 3 seal 0":2,"outcome 4 seal 0":1}
- departs 3 (due 3), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"FoldOccupancy: FoldStale":1,"Muster: NotResident":1,"PostBeacon: AlreadyDone":51,"Reveal: AlreadyDone":1,"SettleDeparture: AlreadyDone":3,"SettleTicket: NoTicket":63,"SettleTransit: TransitState":3,"SkipQuiet: OutOfOrder":35}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":4,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":30,"floor":500000000,"lamports":17132012045,"n":32},"funders":{"lamports":2039586079405,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52498817599,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":2,"spend_by_day":{"0":182855050,"1":217226511}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 11, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785631680: 1 keys, 1000 milli, 360 slots, above keeper cap true
- hold-skipped slots-below at game 1785638280: – keys, – milli, – slots, above keeper cap –
- hold-skipped slots-above at game 1785647880: – keys, – milli, – slots, above keeper cap –
- hold anchor at game 1785653080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped lag at game 1785675480: – keys, – milli, – slots, above keeper cap –
- hold frontier-fund at game 1785689880: 2 keys, 1000 milli, 15 slots, above keeper cap true
- hold defence-pool at game 1785699080: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold relay-payers at game 1785708280: 20 keys, 3000 milli, 15 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 144–503 | 1 | 0 | 4 | 504 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| frontier-fund | true | 1599–1613 | 2 | 0 | 18 | 1614 |
| defence-pool | true | 1829–1873 | 1 | 0 | 0 | – |
| relay-payers | true | 2059–2073 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 3,
      "closed": 0,
      "open": 3,
      "pending": 0
    },
    "end_season": null,
    "note": "the run stops before end_bell: EndSeason not expected",
    "reaches_end_bell": false,
    "stuck_province_bells": 0,
    "unsettled_due_transits": 0
  },
  "2_cu": {
    "over_budget": [],
    "reveal": {
      "max": 20404.0,
      "n": 3,
      "p50": 18054.0,
      "p90": 20404.0,
      "p99": 20404.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,2) day 0: 9",
      "(0,3) day 0: 35"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 535,
    "skip_txs_per_churned_province_day": {
      "max": 35.0,
      "n": 29,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 35.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 43,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 35.0,
      "n": 72,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 35.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 3,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 3,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 280.0,
      "n": 3,
      "p50": 240.0,
      "p90": 280.0,
      "p99": 280.0
    },
    "close_to_resolve_slots": {
      "max": 7.0,
      "n": 3,
      "p50": 6.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "definition": "round -> anchor and S -> first cache in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W (W5-B F5, pinned by W6-A)",
    "round_to_anchor_from_publication_slots": {
      "max": 5.95,
      "n": 2704,
      "p50": 2.95,
      "p90": 2.95,
      "p99": 3.95
    },
    "round_to_anchor_game_secs": {
      "max": 238.0,
      "n": 2704,
      "p50": 118.0,
      "p90": 118.0,
      "p99": 158.0
    },
    "round_to_anchor_slots": {
      "max": 5.0,
      "n": 2704,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 3.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 138.0,
      "n": 2688,
      "p50": 98.0,
      "p90": 98.0,
      "p99": 138.0
    },
    "s_to_first_cache_slots": {
      "max": 3.0,
      "n": 2688,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 3.0
    },
    "slot_game_secs": 40.0,
    "targets_game_secs_p99": {
      "anchor_to_last_reveal": 30,
      "close_to_resolve": 60,
      "round_to_anchor": 5,
      "s_to_first_cache": 5
    },
    "targets_slots_p99": {
      "anchor_to_last_reveal": 4,
      "close_to_resolve": 8,
      "round_to_anchor": 2,
      "s_to_first_cache": 2,
      "skips_per_idle_day": 6
    }
  },
  "4_liveness": {
    "effective_n_ok": true,
    "keeper_a_min_effective_n": 150
  },
  "5_personas": {
    "violated": []
  },
  "6_herald": {
    "alarms": 0,
    "loads": [
      {
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "ingest_lag_p99_s": 4.0,
        "ingest_lag_samples": 43,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.278528,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 161,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 53.248,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "requests": 28858.0,
        "summary": "p99 file 53.2 ms, ingest->WS p99 0.28 s (ws-stamp), error rate 0.00000, gaps 0, 404 161",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 525376.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {}
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
