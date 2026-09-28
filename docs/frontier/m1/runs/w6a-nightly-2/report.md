# Stack run `w6a-nightly-2`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `35m4zDk9p1q349GWoJuvH38Q1aW8qTPp6KpgbgKYAnvo` (`.so` sha256 `094dc2d69a351f2f7289abb70fb5eea8224c611bfef28266bf9f88fda8eb39c5`, 874624 B)
- source: verify input (6977 transactions, last bell 169)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 8 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 42 max 42 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes []; 6977 txs, read 0.22 s, verify 0.50 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (26 built from the run; 3.12 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (5000 viewers, 1 game h): p99 file 24.58 ms, error rate 0, ingest lag p99 0.40 s → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328984 | 328984 | 328984 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 16 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 507 | 0 | 377341 | 380242 | 380718 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337316 | 339561 | 339646 | 345000 | no | 781 |
| PostBeacon | 2352 | 0 | 330638 | 332650 | 332902 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 143354 | 147501 | 147501 | 220000 | no | 400 |
| FoldOccupancy | 522 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 66 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 66 | 0 | 14211 | 16089 | 16089 | 17000 | no | 575 |
| SettleTicket | 66 | 63 | 20160 | 21631 | 21631 | 40000 | no | 562 |
| Harvest | 51 | 0 | 11996 | 14426 | 14426 | 17500 | no | 427 |
| Build | 40 | 1 | 11598 | 15948 | 15948 | 22000 | no | 428 |
| Train | 50 | 0 | 10364 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 10 | 1 | 16103 | 17966 | 17966 | 25000 | no | 466 |
| Explore | 4 | 0 | 12236 | 14103 | 14103 | 20000 | no | 471 |
| SettleExplore | 4 | 0 | 8346 | 8502 | 8502 | 15000 | no | 396 |
| Depart | 8 | 0 | 16538 | 18397 | 18397 | 24500 | no | 712 |
| Reveal | 8 | 0 | 18054 | 21186 | 21186 | 26000 | no | 862 |
| SettleDeparture | 8 | 8 | 6252 | 6272 | 6272 | 48000 | no | 331 |
| SettleTransit | 8 | 8 | 22656 | 62792 | 62792 | 85000 | no | 859 |
| SweepPoolOwed | 8 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 16 | 0 | 14259 | 37074 | 37074 | 49000 | no | 1197 |
| ResolveFromInputs | 8 | 0 | 29231 | 34209 | 34209 | 340000 | no | 465 |
| SkipQuiet | 327 | 0 | 42270 | 54341 | 56487 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 4 | 0 | 5920 | 5920 | 5920 | 8000 | no | 371 |
| CloseArrivalSlot | 8 | 0 | 6461 | 6461 | 6461 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 8, p50 18054, p90 21186, p99 21186, max 21186.

## Keeper latencies (a slot is 40 game s)

Round → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 2 | 2 | 118 | 118 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 97 | 98 | 5 s |
| anchor_to_last_reveal_slots | 8 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| close_to_resolve_slots | 8 | 6 | 6 | 6 | 8 | 240 | 240 | 60 s |

Round → anchor from the publication instant, in slots: p50 2.95, p99 2.95, max 2.95.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 40 | 1 | 6 | 6 | 0 |
| churned | 30 | 6 | 42 | 42 | 5 |
| all | 70 | 6 | 42 | 42 | – |

**ClashInputs:** 0 closed, 8 open: 8 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2352,"BUILD":40,"CAMP":15,"CLASH":8,"CLOSE":12,"DEPART":8,"DEPARTURE_SETTLED":8,"DIVERT":12,"EXPLORE":4,"EXPLORE_RESULT":4,"FOLD":522,"GATHER":16,"GENESIS_SEED":1,"HARVEST":51,"HOLDING_FINAL":3,"JOIN":66,"MUSTER":10,"POOL_SWEEP":8,"PROVINCE_OPEN":37,"REVEAL":8,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":66,"SKIP":327,"TICKET":66,"TRAIN":50,"TRANSIT_SETTLED":8}
- transits: {"outcome 3 seal 0":1,"outcome 4 seal 0":3,"outcome 8 seal 2":4}
- departs 8 (due 8), unsettled due: 0
- bad-seal codes: {"2":4}
- failed transactions: {"Build: QueueFull":1,"Muster: NotResident":1,"SettleDeparture: AlreadyDone":8,"SettleTicket: NoTicket":63,"SettleTransit: TransitState":8}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":4,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":27,"floor":500000000,"lamports":18127390330,"n":32},"funders":{"lamports":2038555173182,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52498743464,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":188838558,"1":223487846}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 10, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
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
| ticket | true | 159–503 | 1 | 0 | 6 | 504 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| frontier-fund | true | 1599–1613 | 2 | 0 | 18 | 1614 |
| defence-pool | true | 1829–1873 | 1 | 0 | 1 | 1916 |
| relay-payers | true | 2059–2073 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 8,
      "closed": 0,
      "open": 8,
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
      "max": 21186.0,
      "n": 8,
      "p50": 18054.0,
      "p90": 21186.0,
      "p99": 21186.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,1) day 0: 9",
      "(0,3) day 0: 42",
      "(1,1) day 0: 12",
      "(1,2) day 0: 9",
      "(2,1) day 0: 30"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 702,
    "skip_txs_per_churned_province_day": {
      "max": 42.0,
      "n": 30,
      "p50": 6.0,
      "p90": 9.0,
      "p99": 42.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 40,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 42.0,
      "n": 70,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 42.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 8,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 8,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 240.0,
      "n": 8,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 6.0,
      "n": 8,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "definition": "round -> anchor and S -> first cache in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W (W5-B F5, pinned by W6-A)",
    "round_to_anchor_from_publication_slots": {
      "max": 2.95,
      "n": 2704,
      "p50": 2.95,
      "p90": 2.95,
      "p99": 2.95
    },
    "round_to_anchor_game_secs": {
      "max": 118.0,
      "n": 2704,
      "p50": 118.0,
      "p90": 118.0,
      "p99": 118.0
    },
    "round_to_anchor_slots": {
      "max": 2.0,
      "n": 2704,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 98.0,
      "n": 2688,
      "p50": 97.0,
      "p90": 98.0,
      "p99": 98.0
    },
    "s_to_first_cache_slots": {
      "max": 2.0,
      "n": 2688,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
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
        "ingest_lag_p99_s": 0.4,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.155648,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 289,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 24.576,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "requests": 28868.0,
        "summary": "p99 file 24.6 ms, ingest->WS p99 0.16 s (ws-stamp), error rate 0.00000, gaps 0, 404 289",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 560000.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 4
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
