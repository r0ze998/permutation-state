# Stack run `w6a-nightly-1`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `7twEjKqChqAhUqz9x9ucfcqj7MZtuNrTdahVJN2zCW1h` (`.so` sha256 `094dc2d69a351f2f7289abb70fb5eea8224c611bfef28266bf9f88fda8eb39c5`, 874624 B)
- source: verify input (6440 transactions, last bell 169)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 0 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 11 max 11 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes []; 6440 txs, read 0.21 s, verify 0.49 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (16 built from the run; 3.15 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (5000 viewers, 1 game h): p99 file 25.60 ms, error rate 0, ingest lag p99 6 s → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 16 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 507 | 0 | 377406 | 380288 | 380714 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337223 | 339646 | 339691 | 345000 | no | 781 |
| PostBeacon | 2240 | 0 | 330596 | 332650 | 332733 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 139838 | 144187 | 144187 | 220000 | no | 400 |
| FoldOccupancy | 522 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 25 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 23 | 0 | 14211 | 16051 | 16051 | 17000 | no | 575 |
| SettleTicket | 23 | 63 | 20177 | 21540 | 21540 | 40000 | no | 562 |
| Harvest | 8 | 0 | 12162 | 14425 | 14425 | 17500 | no | 427 |
| Build | 4 | 0 | 11598 | 11745 | 11745 | 22000 | no | 428 |
| Train | 5 | 0 | 10364 | 10545 | 10545 | 17500 | no | 432 |
| Muster | 2 | 0 | 14651 | 16103 | 16103 | 25000 | no | 466 |
| SkipQuiet | 263 | 0 | 42270 | 53806 | 54022 | 90000 + 30000/unit | no | 1160 |

**Reveal CU distribution** (C4 input): n 0, p50 –, p90 –, p99 –, max –.

## Keeper latencies (a slot is 40 game s)

Round → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 2 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 99 | 99 | 5 s |
| anchor_to_last_reveal_slots | 0 | – | – | – | 4 | – | – | 30 s |
| close_to_resolve_slots | 0 | – | – | – | 8 | – | – | 60 s |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 2.98.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 53 | 1 | 6 | 6 | 0 |
| churned | 20 | 6 | 11 | 11 | 1 |
| all | 73 | 6 | 11 | 11 | – |

**ClashInputs:** 0 closed, 0 open: 0 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2240,"BUILD":4,"CAMP":8,"FOLD":522,"GENESIS_SEED":1,"HARVEST":8,"HOLDING_FINAL":1,"JOIN":25,"MUSTER":2,"PROVINCE_OPEN":37,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":23,"SKIP":263,"TICKET":23,"TRAIN":5}
- transits: {}
- departs 0 (due 0), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"SettleTicket: NoTicket":63}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":4,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":29,"floor":500000000,"lamports":18201883934,"n":32},"funders":{"lamports":2038543956350,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52500000000,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":182660475,"1":217313578}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 13, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold-skipped slots-below at game 1785638880: – keys, – milli, – slots, above keeper cap –
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
| ticket | true | 159–503 | 1 | 0 | 4 | 504 |
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
      "closable_after_grace": 0,
      "closed": 0,
      "open": 0,
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
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(1,2) day 0: 11"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 404,
    "skip_txs_per_churned_province_day": {
      "max": 11.0,
      "n": 20,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 11.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 53,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 11.0,
      "n": 73,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 11.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "anchor_to_last_reveal_slots": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "close_to_resolve_game_secs": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "close_to_resolve_slots": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "definition": "round -> anchor and S -> first cache in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W (W5-B F5, pinned by W6-A)",
    "round_to_anchor_from_publication_slots": {
      "max": 2.975,
      "n": 2704,
      "p50": 2.975,
      "p90": 2.975,
      "p99": 2.975
    },
    "round_to_anchor_game_secs": {
      "max": 119.0,
      "n": 2704,
      "p50": 119.0,
      "p90": 119.0,
      "p99": 119.0
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
      "max": 99.0,
      "n": 2688,
      "p50": 99.0,
      "p90": 99.0,
      "p99": 99.0
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
        "ingest_lag_p99_s": 6.0,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.196608,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 161,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 25.6,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "requests": 28865.0,
        "summary": "p99 file 25.6 ms, ingest->WS p99 0.20 s (ws-stamp), error rate 0.00000, gaps 0, 404 161",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 576000.0
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
