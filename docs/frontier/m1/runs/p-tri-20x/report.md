# Stack run `p-tri-20x`

- phase **complete**, beacon **archive**, scale 20×, 1000 bots, play 144 bells + 26 drain; program `ApokMuVPUFpmRjCAUxU7EwexCTWHcHqDJPMtdSCnME12` (`.so` sha256 `d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`, 875824 B)
- source: verify input (10375 transactions, last bell 169)

- **exit-grade environment**
- machine load average (1 min, sampled each bell; the machine is shared): p50 3.42, p99 5.82, max 5.95 at bell 14; max per game day [5.95,4.56]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 36 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 81.00 > 2 slots; anchor_to_last_reveal_slots p99 37.00 > 4 slots; SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 24 max 24 (reported) |
| 4 | **pass** | 0 unrevealed inside 8 above-cap hold windows (expected); unrevealed by rule: bounced 1 |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **fail** | in-run window: ["ingest -> WS p99 Some(2.3592959999999996) s (ws-stamp) > 2 s"] |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **exit-grade** | every adversary hold fired; release .so pinned; real rounds |

## Verdicts

- verify: **PASS** (fail codes []; 10375 txs, read 0.32 s, verify 0.54 s — E6 wall time)
- tamper: 29/30 classes FAIL with their codes (29 built from the run; 3.57 s)
- `.so` pin: expected sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281 (the release build record; V2 checks the deployed program against it)
- load in-run (in-run, 5000 viewers, 12 game h): p99 file 13.82 ms, error rate 0 (0 errors / 1726425 requests + WS sessions), ingest → WS p99 2.36 s (ws-stamp; fold lag p99 2.40 s), WS coverage 1 outside 3 outage windows, stale retries 12000 / WS reconnects 3000 (outside outages 0 / 0), unavailable 5812 (10800896.24 ms), generator recovery true → **fail** (["ingest -> WS p99 Some(2.3592959999999996) s (ws-stamp) > 2 s"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328965 | 328965 | 328965 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 352 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 483 | 0 | 377042 | 380239 | 380481 | 400000 | no | 1177 |
| PostSeed | 2528 | 0 | 337040 | 339491 | 339640 | 345000 | no | 781 |
| PostBeacon | 2736 | 240 | 330644 | 332622 | 332724 | 340000 | no | 645 |
| OpenRing | 6 | 0 | 15052 | 15400 | 15400 | 30000 | no | 563 |
| ConsumeRingSeed | 2 | 0 | 331093 | 331284 | 331284 | 345000 | no | 646 |
| OpenProvince | 91 | 20 | 141122 | 148308 | 148308 | 220000 | no | 400 |
| FoldOccupancy | 337 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 490 | 10 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 452 | 0 | 14234 | 16125 | 16138 | 17000 | no | 575 |
| SettleTicket | 480 | 27 | 20224 | 21709 | 21779 | 40000 | no | 562 |
| Harvest | 155 | 0 | 11996 | 16686 | 16686 | 17500 | no | 427 |
| Build | 349 | 0 | 11598 | 15948 | 18144 | 22000 | no | 428 |
| Train | 361 | 0 | 10545 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 96 | 0 | 14740 | 18840 | 18840 | 25000 | no | 466 |
| Explore | 42 | 0 | 14037 | 16415 | 16415 | 20000 | no | 471 |
| SettleExplore | 42 | 0 | 8346 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 37 | 2 | 16565 | 18484 | 18484 | 24500 | no | 712 |
| Reveal | 36 | 115 | 20353 | 22031 | 22031 | 26000 | no | 895 |
| SettleDeparture | 37 | 0 | 6262 | 6312 | 6312 | 48000 | no | 331 |
| SettleTransit | 37 | 3 | 60362 | 62908 | 62908 | 85000 | no | 891 |
| SweepPoolOwed | 36 | 13 | 5206 | 5216 | 5216 | 8000 | no | 330 |
| GatherClash | 72 | 0 | 16322 | 37100 | 37100 | 49000 | no | 1197 |
| ResolveFromInputs | 36 | 0 | 34389 | 41351 | 41351 | 290000 | no | 465 |
| SkipQuiet | 575 | 6 | 42270 | 47226 | 54371 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 24 | 0 | 6018 | 6018 | 6018 | 8000 | no | 371 |
| CloseArrivalSlot | 36 | 0 | 6496 | 6538 | 6538 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 36, p50 20353, p90 21779, p99 22031, max 22031.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2304 | 1 | 81 | 154 | 2 | 15 | 655 | 5 s |
| s_to_first_cache_slots | 2304 | 1 | 1 | 145 | 2 | 9 | 11 | 5 s |
| anchor_to_last_reveal_slots | 36 | 0 | 37 | 37 | 4 | 0 | 296 | 30 s |
| s_to_resolve_slots | 36 | 2 | 3 | 3 | 8 | 17 | 25 | 60 s |
| close_to_resolve_slots | 36 | 10 | 11 | 11 | reported | 80 | 88 | reported |

Round → anchor from the publication instant, in slots: p50 1.88, p99 81.88, max 154.88.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 17 | 6 | 6 | 6 | 0 |
| churned | 74 | 5 | 24 | 24 | 26 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| all | 91 | 6 | 24 | 24 | – |

**ClashInputs:** 0 closed, 36 open: 36 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2304,"ANNOUNCE":1,"BEACON":2736,"BUILD":349,"CAMP":15,"CLASH":36,"CLOSE":60,"DEPART":37,"DEPARTURE_SETTLED":37,"DIVERT":73,"EXPLORE":42,"EXPLORE_RESULT":42,"FOLD":337,"GATHER":72,"GENESIS_SEED":1,"HARVEST":155,"HOLDING_FINAL":35,"JOIN":490,"MUSTER":96,"POOL_SWEEP":36,"PROVINCE_OPEN":91,"REVEAL":36,"RING_OPEN":6,"RING_SEED":2,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":2384,"SETTLE":480,"SKIP":575,"TICKET":452,"TRAIN":361,"TRANSIT_SETTLED":37}
- transits: {"outcome 1 seal 0":23,"outcome 3 seal 0":6,"outcome 4 seal 0":7,"outcome 6 seal 0":1}
- departs 37 (due 37), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"Depart: TipTooLow":2,"Join: AlreadyDone":10,"OpenProvince: AlreadyDone":20,"PostBeacon: AlreadyDone":240,"Reveal: AlreadyDone":109,"Reveal: WindowClosed":6,"SettleTicket: NoTicket":27,"SettleTransit: TransitState":3,"SkipQuiet: OutOfOrder":6,"SweepPoolOwed: AlreadyDone":13}

### Failed transactions by class (reported, not gating)

436 failed: {"expected":41,"redundancy":395} (by cause {"a/b race":112,"bounded duplicate":6,"duplicate":283,"persona":8,"race":27}); unclassified 0.

| kind | error | n | class | cause |
|---|---|---|---|---|
| PostBeacon | AlreadyDone | 240 | redundancy | duplicate |
| Reveal | AlreadyDone | 109 | redundancy | a/b race |
| SettleTicket | NoTicket | 27 | expected | race |
| OpenProvince | AlreadyDone | 20 | redundancy | duplicate |
| SweepPoolOwed | AlreadyDone | 13 | redundancy | duplicate |
| Join | AlreadyDone | 10 | redundancy | duplicate |
| Reveal | WindowClosed | 6 | expected | persona |
| SkipQuiet | OutOfOrder | 6 | expected | bounded duplicate |
| SettleTransit | TransitState | 3 | redundancy | a/b race |
| Depart | TipTooLow | 2 | expected | persona |

- no landed transaction for ≥ 20 slots after a bell started: 1 windows (187 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
  - slots 815–1028 (214 slots, 155 after bell 9 started; bells 8–11): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":1}
- provinces 91; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 171 bells (0 timeouts; answer ms p99 4); last answered status {"alerts":0,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":18698483934,"n":32},"funders":{"lamports":2038226485551,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52495689384,"n":150}},"provinces_opened":91,"rings_complete":[0,1,2,3,4,5],"seed_latency_slots_p99":1,"spend_by_day":{"0":59089354,"1":77654772},"sweeps_sent":21,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 171 bells (0 timeouts); last answered status {"alerts":2,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":2,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999840000,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497780635,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":300681,"1":300681},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 55, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 7, restarts 7, crashes 0
- hold ticket at game 1789087512: 1 keys, 1000 milli, 1725 slots, above keeper cap true
- hold anchor at game 1789108312: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold slots-below at game 1789110312: 25 keys, 1500 milli, 188 slots, above keeper cap false
- hold frontier-fund at game 1789110312: 2 keys, 1000 milli, 75 slots, above keeper cap true
- hold slots-above at game 1789111512: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold defence-pool at game 1789113312: 1 keys, 1000 milli, 225 slots, above keeper cap true
- hold keeper-payers at game 1789117512: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold lag at game 1789133712: 2 keys, 1000 milli, 150 slots, above keeper cap true
- hold relay-payers at game 1789163512: 20 keys, 3000 milli, 75 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 350–2074 | 1 | 0 | 26 | 2075 |
| anchor | true | 2950–2974 | 1 | 0 | 0 | – |
| slots-below | false | 3200–3387 | 25 | 1 | 1 | 3502 |
| frontier-fund | true | 3200–3274 | 2 | 0 | 14 | 3275 |
| slots-above | true | 3350–3537 | 25 | 0 | 2 | 3538 |
| defence-pool | true | 3575–3799 | 1 | 0 | 2 | 3800 |
| keeper-payers | true | 4100–4174 | 20 | 0 | 0 | – |
| lag | true | 6125–6274 | 2 | 0 | 3 | 6275 |
| relay-payers | true | 9850–9924 | 20 | 0 | 2 | 9925 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 36,
      "closed": 0,
      "open": 36,
      "pending": 0
    },
    "end_season": {
      "attempts": 1,
      "errors": [],
      "ok": true
    },
    "note": "",
    "reaches_end_bell": true,
    "stuck_province_bells": 0,
    "unsettled_due_transits": 0
  },
  "2_cu": {
    "over_budget": [],
    "reveal": {
      "max": 22031.0,
      "n": 36,
      "p50": 20353.0,
      "p90": 21779.0,
      "p99": 22031.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-4,0) day 0: 9",
      "(-4,3) day 0: 10",
      "(-3,1) day 0: 7",
      "(-3,3) day 0: 8",
      "(-2,-3) day 0: 7",
      "(-2,-1) day 0: 9",
      "(-2,4) day 0: 12",
      "(-1,-2) day 0: 14",
      "(-1,4) day 0: 10",
      "(0,-5) day 0: 8",
      "(0,-3) day 0: 8",
      "(0,-2) day 0: 8",
      "(1,-3) day 0: 19",
      "(1,-2) day 0: 8",
      "(1,2) day 0: 10",
      "(2,-3) day 0: 21",
      "(2,1) day 0: 24",
      "(2,3) day 0: 7",
      "(3,-4) day 0: 8",
      "(3,-3) day 0: 8",
      "(3,-1) day 0: 8",
      "(3,0) day 0: 7",
      "(4,-4) day 0: 11",
      "(4,0) day 0: 14",
      "(5,-5) day 0: 9",
      "(5,-2) day 0: 8"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 3095,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 24.0,
      "n": 74,
      "p50": 5.0,
      "p90": 10.0,
      "p99": 24.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 17,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 24.0,
      "n": 91,
      "p50": 6.0,
      "p90": 10.0,
      "p99": 24.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 296.0,
      "n": 36,
      "p50": 0.0,
      "p90": 8.0,
      "p99": 296.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 37.0,
      "n": 36,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 37.0
    },
    "close_to_resolve_game_secs": {
      "max": 88.0,
      "n": 36,
      "p50": 80.0,
      "p90": 88.0,
      "p99": 88.0
    },
    "close_to_resolve_slots": {
      "max": 11.0,
      "n": 36,
      "p50": 10.0,
      "p90": 11.0,
      "p99": 11.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 154.875,
      "n": 2304,
      "p50": 1.875,
      "p90": 1.875,
      "p99": 81.875
    },
    "round_to_anchor_game_secs": {
      "max": 1239.0,
      "n": 2304,
      "p50": 15.0,
      "p90": 15.0,
      "p99": 655.0
    },
    "round_to_anchor_slots": {
      "max": 154.0,
      "n": 2304,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 81.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 1161.0,
      "n": 2304,
      "p50": 9.0,
      "p90": 9.0,
      "p99": 11.0
    },
    "s_to_first_cache_slots": {
      "max": 145.0,
      "n": 2304,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_resolve_game_secs": {
      "max": 25.0,
      "n": 36,
      "p50": 17.0,
      "p90": 25.0,
      "p99": 25.0
    },
    "s_to_resolve_slots": {
      "max": 3.0,
      "n": 36,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 3.0
    },
    "slot_game_secs": 8.0,
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
          "coverage_samples": 1733,
          "outage_windows": [
            [
              1790709751178,
              1790709767954
            ],
            [
              1790709910228,
              1790709924132
            ],
            [
              1790710651363,
              1790710668144
            ]
          ],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 40,
          "samples_unanswered": 0,
          "stale_retries_inside": 12000,
          "stale_retries_outside": 0,
          "ws_coverage": 1.0,
          "ws_coverage_ok": true,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 3000,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 1726425.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 2.4000000000000004,
        "ingest_lag_samples": 1780,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": false,
        "ingest_p99_s": 2.3592959999999996,
        "ingest_target_s": 2.0,
        "misses": [
          "ingest -> WS p99 Some(2.3592959999999996) s (ws-stamp) > 2 s"
        ],
        "not_found": 764945,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 13.824,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "recovery_ok": true,
        "requests": 1725425.0,
        "stale_retries": 12000,
        "stale_retries_outside": 0,
        "summary": "p99 file 13.8 ms, ingest->WS p99 2.36 s (ws-stamp), error rate 0.00000, gaps 0, 404 764945, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 5812,
        "unavailable_ms": 10800896.241,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 3000,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_timed": 11077325.0
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
