# Stack run `w6-latency`

- phase **complete**, beacon **test-key**, scale 2×, 300 bots, play 36 bells + 26 drain; program `6tix8aNC3zvuseR5rMx4prjsQFBPVUmwWvk4GwLUKBNj` (`.so` sha256 `094dc2d69a351f2f7289abb70fb5eea8224c611bfef28266bf9f88fda8eb39c5`, 874624 B)
- source: verify input (2649 transactions, last bell 61)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 0 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | no samples for anchor_to_last_reveal_game_secs, close_to_resolve_game_secs; SkipQuiet per idle province-day p99 2 max 2 (0 idle days over 6), per churned day p99 2 max 2 (reported) |
| 4 | **pass** | 0 unrevealed inside 0 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes []; 2649 txs, read 0.11 s, verify 0.18 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (15 built from the run; 1.97 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 332746 | 332746 | 332746 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchorMulti | 183 | 0 | 377737 | 380337 | 380342 | 400000 | no | 1177 |
| PostSeed | 960 | 0 | 337299 | 339747 | 339747 | 345000 | no | 781 |
| PostBeacon | 992 | 0 | 330694 | 333275 | 333275 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15052 | 15052 | 15052 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 140956 | 147258 | 147258 | 220000 | no | 400 |
| FoldOccupancy | 186 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 61 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 59 | 0 | 14210 | 16076 | 16076 | 17000 | no | 575 |
| SettleTicket | 59 | 0 | 20173 | 21647 | 21647 | 40000 | no | 562 |
| Harvest | 3 | 0 | 11996 | 11996 | 11996 | 17500 | no | 427 |
| Build | 8 | 0 | 11598 | 11598 | 11598 | 22000 | no | 428 |
| Train | 13 | 0 | 10364 | 12220 | 12220 | 17500 | no | 432 |
| SkipQuiet | 74 | 0 | 42270 | 42465 | 42465 | 90000 + 30000/unit | no | 1160 |

**Reveal CU distribution** (C4 input): n 0, p50 –, p90 –, p99 –, max –.

## Keeper latencies (a slot is 0.80 game s)

Round → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 576 | 2 | 2 | 2 | 2 | 2 | 2 | 5 s |
| s_to_first_cache_slots | 560 | 2 | 2 | 2 | 2 | 2 | 2 | 5 s |
| anchor_to_last_reveal_slots | 0 | – | – | – | 4 | – | – | 30 s |
| close_to_resolve_slots | 0 | – | – | – | 8 | – | – | 60 s |

Round → anchor from the publication instant, in slots: p50 2.50, p99 2.50, max 2.50.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 21 | 2 | 2 | 2 | 0 |
| churned | 16 | 2 | 2 | 2 | 0 |
| all | 37 | 2 | 2 | 2 | – |

**ClashInputs:** 0 closed, 0 open: 0 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":976,"ANNOUNCE":1,"BEACON":992,"BUILD":8,"FOLD":186,"GENESIS_SEED":1,"HARVEST":3,"JOIN":61,"PROVINCE_OPEN":37,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":960,"SETTLE":59,"SKIP":74,"TICKET":59,"TRAIN":13}
- transits: {}
- departs 0 (due 0), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":0,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":62,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":21284599595,"n":32},"funders":{"lamports":2039994062942,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52500000000,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":85258792}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 628, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 1, restarts 1, crashes 0

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|

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
    "churned_province_days_over_6": [],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 355,
    "skip_txs_per_churned_province_day": {
      "max": 2.0,
      "n": 16,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 2.0,
      "n": 21,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "skip_txs_per_province_day": {
      "max": 2.0,
      "n": 37,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
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
      "max": 2.5,
      "n": 576,
      "p50": 2.5,
      "p90": 2.5,
      "p99": 2.5
    },
    "round_to_anchor_game_secs": {
      "max": 2.0,
      "n": 576,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "round_to_anchor_slots": {
      "max": 2.0,
      "n": 576,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 2.0,
      "n": 560,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "s_to_first_cache_slots": {
      "max": 2.0,
      "n": 560,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "slot_game_secs": 0.8,
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
    "loads": []
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
