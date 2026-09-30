# Stack run `integ-w6t-tri-20x`

- phase **complete**, beacon **archive**, scale 20×, 1000 bots, play 144 bells + 26 drain; program `DocUguvZBi8WFzvgrf5TyTzus7deUXsS2j2o7KXmjVLS` (`.so` sha256 `d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`, 875824 B)
- source: verify input (10468 transactions, last bell 169)

- **exit-grade environment**
- machine load average (1 min, sampled each bell; the machine is shared): p50 4.54, p99 8.09, max 8.98 at bell 60; max per game day [8.98,7.02]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 42 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 81.00 > 2 slots; anchor_to_last_reveal_slots p99 38.00 > 4 slots; SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 30 max 30 (reported) |
| 4 | **pass** | 0 unrevealed inside 8 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **fail** | in-run window: ["ingest -> WS p99 Some(2.490368) s (ws-stamp) > 2 s"] |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **exit-grade** | every adversary hold fired; release .so pinned; real rounds |

## Verdicts

- verify: **PASS** (fail codes []; 10468 txs, read 0.34 s, verify 0.57 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (29 built from the run; 3.75 s)
- `.so` pin: expected sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281 (the release build record; V2 checks the deployed program against it)
- load in-run (in-run, 5000 viewers, 12 game h): p99 file 13.82 ms, error rate 0.00 (1 errors / 1726501 requests + WS sessions), ingest → WS p99 2.49 s (ws-stamp; fold lag p99 2.80 s), WS coverage 1 outside 3 outage windows, stale retries 12000 / WS reconnects 3000 (outside outages 0 / 0), unavailable 5749 (10689498.41 ms), generator recovery true → **fail** (["ingest -> WS p99 Some(2.490368) s (ws-stamp) > 2 s"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330988 | 330988 | 330988 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 14258 | 14258 | 14258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 362 | 0 | 4655 | 337304 | 337304 | 345000 | no | 780 |
| PostAnchorMulti | 486 | 0 | 377018 | 380300 | 380592 | 400000 | no | 1177 |
| PostSeed | 2528 | 0 | 337037 | 339402 | 339420 | 345000 | no | 781 |
| PostBeacon | 2736 | 240 | 330656 | 332622 | 332724 | 340000 | no | 645 |
| OpenRing | 6 | 0 | 15052 | 15400 | 15400 | 30000 | no | 563 |
| ConsumeRingSeed | 2 | 0 | 331093 | 331284 | 331284 | 345000 | no | 646 |
| OpenProvince | 91 | 20 | 141017 | 148308 | 148308 | 220000 | no | 400 |
| FoldOccupancy | 337 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 490 | 10 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 452 | 0 | 14248 | 16114 | 16150 | 17000 | no | 575 |
| SettleTicket | 484 | 17 | 20226 | 21725 | 21804 | 40000 | no | 562 |
| Harvest | 176 | 0 | 11996 | 16686 | 16686 | 17500 | no | 427 |
| Build | 345 | 0 | 11598 | 15948 | 18144 | 22000 | no | 428 |
| Train | 365 | 0 | 12220 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 100 | 1 | 14737 | 18819 | 18840 | 25000 | no | 466 |
| Explore | 40 | 1 | 14030 | 16415 | 16415 | 20000 | no | 471 |
| SettleExplore | 40 | 0 | 8346 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 42 | 2 | 16566 | 20691 | 20691 | 24500 | no | 712 |
| Reveal | 42 | 115 | 20227 | 23201 | 23201 | 26000 | no | 895 |
| SettleDeparture | 42 | 0 | 6272 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 42 | 8 | 60509 | 63001 | 63001 | 85000 | no | 859 |
| SweepPoolOwed | 42 | 18 | 5206 | 5216 | 5216 | 8000 | no | 330 |
| GatherClash | 84 | 0 | 16322 | 37100 | 37100 | 49000 | no | 1197 |
| ResolveFromInputs | 42 | 0 | 34564 | 41904 | 41904 | 290000 | no | 465 |
| SkipQuiet | 580 | 0 | 42270 | 48859 | 54225 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 23 | 0 | 6018 | 6018 | 6018 | 8000 | no | 371 |
| CloseArrivalSlot | 42 | 4 | 6496 | 6538 | 6538 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 42, p50 20227, p90 21553, p99 23201, max 23201.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2304 | 1 | 81 | 154 | 2 | 13 | 653 | 5 s |
| s_to_first_cache_slots | 2304 | 1 | 1 | 145 | 2 | 10 | 11 | 5 s |
| anchor_to_last_reveal_slots | 42 | 0 | 38 | 38 | 4 | 0 | 304 | 30 s |
| s_to_resolve_slots | 42 | 2 | 3 | 3 | 8 | 18 | 26 | 60 s |
| close_to_resolve_slots | 42 | 10 | 11 | 11 | reported | 80 | 88 | reported |

Round → anchor from the publication instant, in slots: p50 1.62, p99 81.62, max 154.62.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 17 | 6 | 6 | 6 | 0 |
| churned | 74 | 5 | 30 | 30 | 24 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| all | 91 | 6 | 30 | 30 | – |

**ClashInputs:** 0 closed, 42 open: 42 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2304,"ANNOUNCE":1,"BEACON":2736,"BUILD":345,"CAMP":12,"CLASH":42,"CLOSE":65,"DEPART":42,"DEPARTURE_SETTLED":42,"DIVERT":84,"EXPLORE":40,"EXPLORE_RESULT":40,"FOLD":337,"GATHER":84,"GENESIS_SEED":1,"HARVEST":176,"HOLDING_FINAL":36,"JOIN":490,"MUSTER":100,"POOL_SWEEP":42,"PROVINCE_OPEN":91,"REVEAL":42,"RING_OPEN":6,"RING_SEED":2,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":2384,"SETTLE":484,"SKIP":580,"TICKET":452,"TRAIN":365,"TRANSIT_SETTLED":42}
- transits: {"outcome 1 seal 0":23,"outcome 3 seal 0":9,"outcome 4 seal 0":10}
- departs 42 (due 42), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"CloseArrivalSlot: BadAccount":4,"Depart: TipTooLow":2,"Explore: NotResident":1,"Join: AlreadyDone":10,"Muster: NotResident":1,"OpenProvince: AlreadyDone":20,"PostBeacon: AlreadyDone":240,"Reveal: AlreadyDone":109,"Reveal: WindowClosed":6,"SettleTicket: NoTicket":17,"SettleTransit: TransitState":8,"SweepPoolOwed: AlreadyDone":18}

### Failed transactions by class (reported, not gating)

436 failed: {"expected":25,"redundancy":405,"unclassified":4,"waste":2} (by cause {"a/b race":117,"bot-policy":2,"duplicate":288,"persona":8,"race":17}); unclassified 4.

| kind | error | n | class | cause |
|---|---|---|---|---|
| PostBeacon | AlreadyDone | 240 | redundancy | duplicate |
| Reveal | AlreadyDone | 109 | redundancy | a/b race |
| OpenProvince | AlreadyDone | 20 | redundancy | duplicate |
| SweepPoolOwed | AlreadyDone | 18 | redundancy | duplicate |
| SettleTicket | NoTicket | 17 | expected | race |
| Join | AlreadyDone | 10 | redundancy | duplicate |
| SettleTransit | TransitState | 8 | redundancy | a/b race |
| Reveal | WindowClosed | 6 | expected | persona |
| CloseArrivalSlot | BadAccount | 4 | unclassified |  |
| Depart | TipTooLow | 2 | expected | persona |
| Explore | NotResident | 1 | waste | bot-policy |
| Muster | NotResident | 1 | waste | bot-policy |

- no landed transaction for ≥ 20 slots after a bell started: 1 windows (184 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
  - slots 813–1027 (215 slots, 154 after bell 9 started; bells 8–11): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 91; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 171 bells (0 timeouts; answer ms p99 24); last answered status {"alerts":0,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":18390233521,"n":32},"funders":{"lamports":2038490354713,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52495615277,"n":150}},"provinces_opened":91,"rings_complete":[0,1,2,3,4,5],"seed_latency_slots_p99":1,"spend_by_day":{"0":59436444,"1":77991862},"sweeps_sent":24,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 171 bells (0 timeouts); last answered status {"alerts":3,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":3,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999762297,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497721305,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":310202,"1":310202},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 63, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 7, restarts 7, crashes 0
- hold ticket at game 1789086912: 1 keys, 1000 milli, 1800 slots, above keeper cap true
- hold anchor at game 1789108312: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold slots-below at game 1789110312: 25 keys, 1500 milli, 188 slots, above keeper cap false
- hold frontier-fund at game 1789110312: 2 keys, 1000 milli, 75 slots, above keeper cap true
- hold slots-above at game 1789112112: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold defence-pool at game 1789113912: 1 keys, 1000 milli, 225 slots, above keeper cap true
- hold keeper-payers at game 1789117512: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold lag at game 1789126712: 2 keys, 1000 milli, 150 slots, above keeper cap true
- hold relay-payers at game 1789163512: 20 keys, 3000 milli, 75 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 275–2074 | 1 | 0 | 22 | 2075 |
| anchor | true | 2950–2974 | 1 | 0 | 0 | – |
| slots-below | false | 3200–3387 | 25 | 1 | 1 | 3501 |
| frontier-fund | true | 3200–3274 | 2 | 0 | 14 | 3275 |
| slots-above | true | 3425–3612 | 25 | 0 | 2 | 3613 |
| defence-pool | true | 3650–3874 | 1 | 0 | 3 | 3875 |
| keeper-payers | true | 4100–4174 | 20 | 0 | 1 | 4175 |
| lag | true | 5250–5399 | 2 | 0 | 9 | 5400 |
| relay-payers | true | 9850–9924 | 20 | 0 | 3 | 9925 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 42,
      "closed": 0,
      "open": 42,
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
      "max": 23201.0,
      "n": 42,
      "p50": 20227.0,
      "p90": 21553.0,
      "p99": 23201.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-4,0) day 0: 9",
      "(-4,3) day 0: 11",
      "(-3,1) day 0: 7",
      "(-3,3) day 0: 8",
      "(-2,-3) day 0: 7",
      "(-2,-1) day 0: 9",
      "(-2,4) day 0: 10",
      "(-1,-2) day 0: 14",
      "(-1,4) day 0: 11",
      "(0,-5) day 0: 8",
      "(0,-3) day 0: 7",
      "(0,3) day 0: 7",
      "(1,-4) day 0: 7",
      "(1,-3) day 0: 30",
      "(1,-2) day 0: 8",
      "(1,2) day 0: 9",
      "(2,-3) day 0: 22",
      "(2,1) day 0: 24",
      "(2,3) day 0: 7",
      "(3,-1) day 0: 8",
      "(4,-4) day 0: 11",
      "(4,0) day 0: 14",
      "(5,-5) day 0: 9",
      "(5,-2) day 0: 8"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 3162,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 30.0,
      "n": 74,
      "p50": 5.0,
      "p90": 11.0,
      "p99": 30.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 17,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 30.0,
      "n": 91,
      "p50": 6.0,
      "p90": 9.0,
      "p99": 30.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 304.0,
      "n": 42,
      "p50": 0.0,
      "p90": 8.0,
      "p99": 304.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 38.0,
      "n": 42,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 38.0
    },
    "close_to_resolve_game_secs": {
      "max": 88.0,
      "n": 42,
      "p50": 80.0,
      "p90": 80.0,
      "p99": 88.0
    },
    "close_to_resolve_slots": {
      "max": 11.0,
      "n": 42,
      "p50": 10.0,
      "p90": 10.0,
      "p99": 11.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 154.625,
      "n": 2304,
      "p50": 1.625,
      "p90": 1.625,
      "p99": 81.625
    },
    "round_to_anchor_game_secs": {
      "max": 1237.0,
      "n": 2304,
      "p50": 13.0,
      "p90": 13.0,
      "p99": 653.0
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
      "max": 1162.0,
      "n": 2304,
      "p50": 10.0,
      "p90": 10.0,
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
      "max": 26.0,
      "n": 42,
      "p50": 18.0,
      "p90": 18.0,
      "p99": 26.0
    },
    "s_to_resolve_slots": {
      "max": 3.0,
      "n": 42,
      "p50": 2.0,
      "p90": 2.0,
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
          "coverage_samples": 1736,
          "outage_windows": [
            [
              1790721476159,
              1790721492933
            ],
            [
              1790721634364,
              1790721648268
            ],
            [
              1790722376159,
              1790722392937
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
        "denominator": 1726501.0,
        "error_rate": 5.792061516326953e-7,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 1.0,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 2.8000000000000003,
        "ingest_lag_samples": 1783,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": false,
        "ingest_p99_s": 2.490368,
        "ingest_target_s": 2.0,
        "misses": [
          "ingest -> WS p99 Some(2.490368) s (ws-stamp) > 2 s"
        ],
        "not_found": 764142,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 13.824,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "recovery_ok": true,
        "requests": 1725501.0,
        "stale_retries": 12000,
        "stale_retries_outside": 0,
        "summary": "p99 file 13.8 ms, ingest->WS p99 2.49 s (ws-stamp), error rate 0.00000, gaps 0, 404 764142, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 5749,
        "unavailable_ms": 10689498.408,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 3000,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_timed": 11070204.0
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
