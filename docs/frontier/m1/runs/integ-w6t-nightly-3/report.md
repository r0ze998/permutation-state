# Stack run `integ-w6t-nightly-3`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `GxpfhbDiYg18Gf8TapZeZ8YrPJmf7JmoRTfE7a23chjJ` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (8416 transactions, last bell 169)

- **NOT exit-grade**: ["hold-skipped frontier-fund (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 3.68, p99 5.25, max 5.37 at bell 148; max per game day [5.25,5.37]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 49 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 104 max 104 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected); unrevealed by rule: bounced 2 |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped frontier-fund (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 8416 txs, read 0.26 s, verify 0.58 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (28 built from the run; 3.67 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 31.74 ms, error rate 0 (0 errors / 29850 requests + WS sessions), ingest → WS p99 0.31 s (ws-stamp; fold lag p99 0.40 s), WS coverage 1 outside 0 outage windows, stale retries 0 / WS reconnects 0 (outside outages 0 / 0), unavailable 0 (0 ms), generator recovery true → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328984 | 328984 | 328984 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 15758 | 15758 | 15758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 25 | 0 | 4655 | 337206 | 337206 | 345000 | no | 780 |
| PostAnchorMulti | 509 | 0 | 377284 | 380242 | 380718 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337333 | 339479 | 339992 | 345000 | no | 781 |
| PostBeacon | 2720 | 0 | 330614 | 332736 | 333198 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 143354 | 147501 | 147501 | 220000 | no | 400 |
| FoldOccupancy | 510 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 96 | 0 | 14211 | 16090 | 16090 | 17000 | no | 575 |
| SettleTicket | 96 | 9 | 20168 | 21655 | 21655 | 40000 | no | 562 |
| Harvest | 169 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 98 | 0 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 84 | 0 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 41 | 1 | 14739 | 18838 | 18838 | 25000 | no | 466 |
| Explore | 47 | 0 | 14033 | 16308 | 16308 | 20000 | no | 471 |
| SettleExplore | 47 | 0 | 8346 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 56 | 1 | 18246 | 22956 | 22956 | 24500 | no | 712 |
| Reveal | 50 | 37 | 19059 | 22268 | 22268 | 26000 | no | 895 |
| SettleDeparture | 56 | 0 | 6262 | 6312 | 6312 | 48000 | no | 331 |
| SettleTransit | 56 | 12 | 60391 | 63002 | 63002 | 85000 | no | 859 |
| SweepPoolOwed | 54 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 98 | 0 | 15277 | 37918 | 37918 | 49000 | no | 1197 |
| ResolveFromInputs | 49 | 0 | 31028 | 36802 | 36802 | 290000 | no | 465 |
| SkipQuiet | 585 | 4 | 16541 | 56312 | 61179 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 17 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 50 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 50, p50 19059, p90 21641, p99 22268, max 22268.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 1 | 1 | 6 | 2 | 78 | 78 | 5 s |
| s_to_first_cache_slots | 2688 | 1 | 1 | 1 | 2 | 57 | 57 | 5 s |
| anchor_to_last_reveal_slots | 49 | 0 | 2 | 2 | 4 | 0 | 80 | 30 s |
| s_to_resolve_slots | 49 | 2 | 3 | 3 | 8 | 97 | 137 | 60 s |
| close_to_resolve_slots | 49 | 4 | 5 | 5 | reported | 160 | 200 | reported |

Round → anchor from the publication instant, in slots: p50 1.95, p99 1.95, max 6.95.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 30 | 1 | 6 | 6 | 0 |
| churned | 27 | 8 | 104 | 104 | 18 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| all | 57 | 6 | 104 | 104 | – |

**ClashInputs:** 0 closed, 49 open: 49 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2720,"BUILD":98,"CAMP":25,"CLASH":49,"CLOSE":67,"DEPART":56,"DEPARTURE_SETTLED":56,"DIVERT":106,"EXPLORE":47,"EXPLORE_RESULT":47,"FOLD":510,"GATHER":98,"GENESIS_SEED":1,"HARVEST":169,"HOLDING_FINAL":14,"JOIN":100,"MUSTER":41,"POOL_SWEEP":54,"PROVINCE_OPEN":37,"REVEAL":50,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":96,"SKIP":585,"TICKET":96,"TRAIN":84,"TRANSIT_SETTLED":56}
- transits: {"outcome 1 seal 0":26,"outcome 3 seal 0":17,"outcome 4 seal 0":7,"outcome 6 seal 0":2,"outcome 8 seal 5":4}
- departs 56 (due 56), unsettled due: 0
- bad-seal codes: {"5":4}
- failed transactions: {"Depart: TipTooLow":1,"Muster: NotResident":1,"Reveal: AlreadyDone":27,"Reveal: BadAddress":5,"Reveal: TransitState":1,"Reveal: WindowClosed":4,"SettleTicket: NoTicket":9,"SettleTransit: TransitState":12,"SkipQuiet: OutOfOrder":4}

### Failed transactions by class (reported, not gating)

64 failed: {"expected":23,"redundancy":39,"unclassified":1,"waste":1} (by cause {"a/b race":39,"adversary":5,"bot-policy":1,"bounded duplicate":4,"persona":5,"race":9}); unclassified 1.

| kind | error | n | class | cause |
|---|---|---|---|---|
| Reveal | AlreadyDone | 27 | redundancy | a/b race |
| SettleTransit | TransitState | 12 | redundancy | a/b race |
| SettleTicket | NoTicket | 9 | expected | race |
| Reveal | BadAddress | 5 | expected | adversary |
| Reveal | WindowClosed | 4 | expected | persona |
| SkipQuiet | OutOfOrder | 4 | expected | bounded duplicate |
| Depart | TipTooLow | 1 | expected | persona |
| Muster | NotResident | 1 | waste | bot-policy |
| Reveal | TransitState | 1 | unclassified |  |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":2}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 171 bells (0 timeouts; answer ms p99 0); last answered status {"alerts":5,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"contested_bells":5,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20284045883,"n":32},"funders":{"lamports":2035688094260,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52528424557,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":1,"spend_by_day":{"0":208892068,"1":245629697},"sweeps_sent":54,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 171 bells (0 timeouts); last answered status {"alerts":2,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":2,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999761997,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52499698724,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":384367,"1":428876},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 12, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785631680: 1 keys, 1000 milli, 360 slots, above keeper cap true
- hold slots-below at game 1785640080: 25 keys, 1500 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785643880: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold anchor at game 1785653080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785671480: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold relay-payers at game 1785708280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785717480: – keys, – milli, – slots, above keeper cap –
- hold-skipped defence-pool at game 1785717480: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 144–503 | 1 | 0 | 6 | 504 |
| slots-below | false | 354–391 | 25 | 1 | 2 | 415 |
| slots-above | true | 449–486 | 25 | 0 | 1 | 487 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| lag | true | 1139–1168 | 2 | 0 | 8 | 1169 |
| relay-payers | true | 2059–2073 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 49,
      "closed": 0,
      "open": 49,
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
      "max": 22268.0,
      "n": 50,
      "p50": 19059.0,
      "p90": 21641.0,
      "p99": 22268.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 15",
      "(-3,2) day 0: 20",
      "(-3,3) day 0: 51",
      "(-2,1) day 0: 8",
      "(-2,2) day 0: 8",
      "(-2,3) day 0: 29",
      "(-1,-2) day 0: 32",
      "(-1,-1) day 0: 8",
      "(-1,3) day 0: 19",
      "(0,-3) day 0: 7",
      "(0,2) day 0: 7",
      "(0,3) day 0: 44",
      "(1,-3) day 0: 104",
      "(1,2) day 0: 8",
      "(2,-3) day 0: 26",
      "(2,1) day 0: 36",
      "(3,-3) day 0: 22",
      "(3,0) day 0: 7"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 1366,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 104.0,
      "n": 27,
      "p50": 8.0,
      "p90": 44.0,
      "p99": 104.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 30,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 104.0,
      "n": 57,
      "p50": 6.0,
      "p90": 29.0,
      "p99": 104.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 80.0,
      "n": 49,
      "p50": 0.0,
      "p90": 40.0,
      "p99": 80.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 2.0,
      "n": 49,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 2.0
    },
    "close_to_resolve_game_secs": {
      "max": 200.0,
      "n": 49,
      "p50": 160.0,
      "p90": 160.0,
      "p99": 200.0
    },
    "close_to_resolve_slots": {
      "max": 5.0,
      "n": 49,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 5.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 6.95,
      "n": 2704,
      "p50": 1.95,
      "p90": 1.95,
      "p99": 1.95
    },
    "round_to_anchor_game_secs": {
      "max": 278.0,
      "n": 2704,
      "p50": 78.0,
      "p90": 78.0,
      "p99": 78.0
    },
    "round_to_anchor_slots": {
      "max": 6.0,
      "n": 2704,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 59.0,
      "n": 2688,
      "p50": 57.0,
      "p90": 57.0,
      "p99": 57.0
    },
    "s_to_first_cache_slots": {
      "max": 1.0,
      "n": 2688,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_resolve_game_secs": {
      "max": 137.0,
      "n": 49,
      "p50": 97.0,
      "p90": 97.0,
      "p99": 137.0
    },
    "s_to_resolve_slots": {
      "max": 3.0,
      "n": 49,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 3.0
    },
    "slot_game_secs": 40.0,
    "targets_game_secs_p99": {
      "anchor_to_last_reveal": 30,
      "round_to_anchor": 5,
      "s_to_first_cache": 5,
      "s_to_resolve": 60
    },
    "targets_slots_p99": {
      "anchor_to_last_reveal": 4,
      "round_to_anchor": 2,
      "s_to_first_cache": 2,
      "s_to_resolve": 8,
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
        "coverage": {
          "coverage_samples": 28,
          "outage_windows": [],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 0,
          "samples_unanswered": 0,
          "stale_retries_inside": 0,
          "stale_retries_outside": 0,
          "ws_coverage": 1.0,
          "ws_coverage_ok": true,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 0,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 29850.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 0.4,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.311296,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 12968,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 31.744,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "recovery_ok": true,
        "requests": 28850.0,
        "stale_retries": 0,
        "stale_retries_outside": 0,
        "summary": "p99 file 31.7 ms, ingest->WS p99 0.31 s (ws-stamp), error rate 0.00000, gaps 0, 404 12968, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 0,
        "unavailable_ms": 0.0,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 0,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_timed": 756934.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "5": 4
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
