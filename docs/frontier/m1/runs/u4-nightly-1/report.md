# Stack run `u4-nightly-1`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `3rSx9mB8Sp8jbtGz1U39W6rjW22TtrtZai1NXWZcEKJX` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (8299 transactions, last bell 169)

- **NOT exit-grade**: ["hold-skipped frontier-fund (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 5.61, p99 12.44, max 13.18 at bell 68; max per game day [13.18,4.4]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 52 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 125 max 125 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected); unrevealed by rule: outcome 6 (no reason: pre-W6T-3 verifier) 2 |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped frontier-fund (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 8299 txs, read 0.26 s, verify 0.67 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (29 built from the run; 4.23 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 27.65 ms, error rate 0 (0 errors / 29871 requests + WS sessions), ingest → WS p99 0.22 s (ws-stamp; fold lag p99 0.40 s), WS coverage 0.89 outside 0 outage windows, stale retries – / WS reconnects – (outside outages 0 / 0), unavailable – (– ms), generator recovery false → **fail** (["WS coverage 88.9% < 99% outside the herald outage windows"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 10 | 0 | 338225 | 338225 | 338225 | 345000 | no | 780 |
| PostAnchorMulti | 508 | 0 | 377383 | 380288 | 380714 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337191 | 339530 | 339691 | 345000 | no | 781 |
| PostBeacon | 2256 | 0 | 330498 | 332733 | 332874 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 139838 | 144187 | 144187 | 220000 | no | 400 |
| FoldOccupancy | 510 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 96 | 0 | 14211 | 16090 | 16090 | 17000 | no | 575 |
| SettleTicket | 96 | 10 | 20168 | 21653 | 21653 | 40000 | no | 562 |
| Harvest | 199 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 95 | 2 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 86 | 0 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 44 | 1 | 16063 | 18838 | 18838 | 25000 | no | 466 |
| Explore | 37 | 0 | 14029 | 14129 | 14129 | 20000 | no | 471 |
| SettleExplore | 37 | 0 | 8275 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 59 | 1 | 18271 | 22951 | 22951 | 24500 | no | 712 |
| Reveal | 53 | 25 | 19027 | 21913 | 21913 | 26000 | no | 862 |
| SettleDeparture | 59 | 59 | 6252 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 59 | 68 | 60297 | 62997 | 62997 | 85000 | no | 859 |
| SweepPoolOwed | 56 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 104 | 0 | 14259 | 37094 | 37094 | 49000 | no | 1197 |
| ResolveFromInputs | 52 | 0 | 30069 | 35304 | 35304 | 290000 | no | 465 |
| SkipQuiet | 800 | 6 | 12463 | 53978 | 57163 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 20 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 52 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 53, p50 19027, p90 21548, p99 21913, max 21913.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 5 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 99 | 99 | 5 s |
| anchor_to_last_reveal_slots | 52 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 52 | 4 | 4 | 4 | 8 | 179 | 179 | 60 s |
| close_to_resolve_slots | 52 | 6 | 6 | 6 | reported | 240 | 240 | reported |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 5.97.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 28 | 1 | 6 | 6 | 0 |
| churned | 27 | 8 | 125 | 125 | 19 |
| active (GATHER/CLASH, reported) | 1 | 7 | 7 | 7 | 1 |
| all | 56 | 6 | 125 | 125 | – |

**ClashInputs:** 0 closed, 52 open: 52 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2256,"BUILD":95,"CAMP":23,"CLASH":52,"CLOSE":72,"DEPART":59,"DEPARTURE_SETTLED":59,"DIVERT":107,"EXPLORE":37,"EXPLORE_RESULT":37,"FOLD":510,"GATHER":104,"GENESIS_SEED":1,"HARVEST":199,"HOLDING_FINAL":14,"JOIN":100,"MUSTER":44,"POOL_SWEEP":56,"PROVINCE_OPEN":37,"REVEAL":53,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":96,"SKIP":800,"TICKET":96,"TRAIN":86,"TRANSIT_SETTLED":59}
- transits: {"outcome 1 seal 0":23,"outcome 3 seal 0":17,"outcome 4 seal 0":8,"outcome 6 seal 0":3,"outcome 8 seal 2":4,"outcome 8 seal 5":4}
- departs 59 (due 59), unsettled due: 0
- bad-seal codes: {"2":4,"5":4}
- failed transactions: {"Build: QueueFull":2,"Depart: TipTooLow":1,"Muster: NotResident":1,"Reveal: AlreadyDone":13,"Reveal: BadAddress":5,"Reveal: WindowClosed":7,"SettleDeparture: AlreadyDone":59,"SettleTicket: NoTicket":10,"SettleTransit: TransitState":68,"SkipQuiet: OutOfOrder":6}

### Failed transactions by class (reported, not gating)

172 failed: {"expected":23,"redundancy":140,"unclassified":6,"waste":3} (by cause {"a/b race":140,"adversary":5,"bot-policy":3,"persona":8,"race":10}); unclassified 6.

| kind | error | n | class | cause |
|---|---|---|---|---|
| SettleTransit | TransitState | 68 | redundancy | a/b race |
| SettleDeparture | AlreadyDone | 59 | redundancy | a/b race |
| Reveal | AlreadyDone | 13 | redundancy | a/b race |
| SettleTicket | NoTicket | 10 | expected | race |
| Reveal | WindowClosed | 7 | expected | persona |
| SkipQuiet | OutOfOrder | 6 | unclassified |  |
| Reveal | BadAddress | 5 | expected | adversary |
| Build | QueueFull | 2 | waste | bot-policy |
| Depart | TipTooLow | 1 | expected | persona |
| Muster | NotResident | 1 | waste | bot-policy |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"outcome 6 (no reason: pre-W6T-3 verifier)":2}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 2 of 171 bells (0 timeouts; answer ms p99 8); last answered status {"alerts":4,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"contested_bells":4,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20782343694,"n":32},"funders":{"lamports":2035182699314,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52518180112,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":1,"spend_by_day":{"0":196710157,"1":230894868},"sweeps_sent":56,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 1 of 171 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":169,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23998851612,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52503792422,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":1312546,"1":1355966},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 10, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold slots-below at game 1785640080: 25 keys, 1500 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785643880: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold anchor at game 1785653080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785671480: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold relay-payers at game 1785708280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785718080: – keys, – milli, – slots, above keeper cap –
- hold-skipped defence-pool at game 1785718080: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 159–503 | 1 | 0 | 6 | 504 |
| slots-below | false | 354–391 | 25 | 0 | 1 | 416 |
| slots-above | true | 449–486 | 25 | 0 | 2 | 487 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| lag | true | 1139–1168 | 2 | 0 | 7 | 1169 |
| relay-payers | true | 2059–2073 | 20 | 0 | 1 | 2079 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 52,
      "closed": 0,
      "open": 52,
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
      "max": 21913.0,
      "n": 53,
      "p50": 19027.0,
      "p90": 21548.0,
      "p99": 21913.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(-2,1) day 0: 7"
    ],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 16",
      "(-3,1) day 0: 7",
      "(-3,2) day 0: 40",
      "(-3,3) day 0: 125",
      "(-2,-1) day 0: 7",
      "(-2,2) day 0: 8",
      "(-2,3) day 0: 29",
      "(-1,-2) day 0: 26",
      "(-1,2) day 0: 7",
      "(-1,3) day 0: 21",
      "(0,-3) day 0: 8",
      "(0,-2) day 0: 7",
      "(0,2) day 0: 8",
      "(0,3) day 0: 49",
      "(1,-3) day 0: 27",
      "(1,-2) day 0: 10",
      "(1,2) day 0: 122",
      "(2,-3) day 0: 124",
      "(3,-3) day 0: 31"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 1592,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 1,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 125.0,
      "n": 27,
      "p50": 8.0,
      "p90": 122.0,
      "p99": 125.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 28,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 125.0,
      "n": 56,
      "p50": 6.0,
      "p90": 31.0,
      "p99": 125.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 52,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 52,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 240.0,
      "n": 52,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 6.0,
      "n": 52,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 5.975,
      "n": 2704,
      "p50": 2.975,
      "p90": 2.975,
      "p99": 2.975
    },
    "round_to_anchor_game_secs": {
      "max": 239.0,
      "n": 2704,
      "p50": 119.0,
      "p90": 119.0,
      "p99": 119.0
    },
    "round_to_anchor_slots": {
      "max": 5.0,
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
    "s_to_resolve_game_secs": {
      "max": 179.0,
      "n": 52,
      "p50": 179.0,
      "p90": 179.0,
      "p99": 179.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 52,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 4.0
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
          "coverage_samples": 36,
          "outage_windows": [],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 0,
          "samples_unanswered": 0,
          "stale_retries_inside": 0,
          "stale_retries_outside": 0,
          "ws_coverage": 0.8888888888888888,
          "ws_coverage_ok": false,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 0,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 29871.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "generator_recovery": false,
        "generator_recovery_note": "this frontier-viewers has no recovery (pre-W6T-3 build): a herald kill is counted as errors and drops its WS viewers",
        "ingest_lag_p99_s": 0.4,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.221184,
        "ingest_target_s": 2.0,
        "misses": [
          "WS coverage 88.9% < 99% outside the herald outage windows"
        ],
        "not_found": 654,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 27.648,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "recovery_ok": true,
        "requests": 28871.0,
        "stale_retries": null,
        "stale_retries_outside": 0,
        "summary": "p99 file 27.6 ms, ingest->WS p99 0.22 s (ws-stamp), error rate 0.00000, gaps 0, 404 654, WS coverage 88.9%, recovery outside outages 0",
        "unavailable": null,
        "unavailable_ms": null,
        "ws_coverage": 0.8888888888888888,
        "ws_coverage_ok": false,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": null,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_timed": 692782.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 4,
      "5": 4
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
