# Stack run `m1c-nightly-1`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `2jj24FMp4VHkFFCXMPNUePvNvB1W98GfWML1baeDVieQ` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (9083 transactions, last bell 169)

- **NOT exit-grade**: ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 4.14, p99 6, max 6.25 at bell 67; max per game day [6.25,5.19]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 48 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 132 max 132 (reported) |
| 4 | **pass** | 0 unrevealed inside 7 above-cap hold windows (expected) |
| 5 | **n.a.** | no persona violated, but not every expected outcome was observed: squatter not exercised ({"file_ticket@relay:QuotaExceeded":138,"file_ticket@relay:ok":1,"join@relay:ok":1}) |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; 8 garbage and 4 bad-plaintext transits sent and due, each settled as bad-seal (codes {"bad_plaintext:5":4,"garbage:2":8}) |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 9083 txs, read 0.27 s, verify 0.57 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (30 built from the run; 3.73 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 147.46 ms, error rate 0 (0 errors / 29828 requests + WS sessions), ingest → WS p99 0.52 s (ws-stamp; fold lag p99 4.80 s), WS coverage 1 outside 0 outage windows, stale retries 0 / WS reconnects 0 (outside outages 0 / 0), unavailable 0 (0 ms), generator recovery true → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330796 | 330796 | 330796 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 185 | 0 | 4655 | 338499 | 338499 | 345000 | no | 780 |
| PostAnchorMulti | 524 | 0 | 377072 | 380321 | 380379 | 400000 | no | 1177 |
| PostSeed | 2912 | 0 | 337220 | 339368 | 339691 | 345000 | no | 781 |
| PostBeacon | 2720 | 16 | 330762 | 333103 | 333193 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 143753 | 148635 | 148635 | 220000 | no | 400 |
| FoldOccupancy | 511 | 3 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 100 | 0 | 14211 | 16064 | 16090 | 17000 | no | 575 |
| SettleTicket | 100 | 18 | 20168 | 21615 | 21653 | 40000 | no | 562 |
| Harvest | 201 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 107 | 0 | 11745 | 15948 | 15948 | 22000 | no | 428 |
| Train | 95 | 3 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 49 | 0 | 16067 | 18829 | 18829 | 25000 | no | 466 |
| Explore | 43 | 0 | 14026 | 16308 | 16308 | 20000 | no | 471 |
| SettleExplore | 43 | 1 | 8275 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 53 | 1 | 18268 | 22968 | 22968 | 24500 | no | 712 |
| Reveal | 49 | 77 | 19158 | 21780 | 21780 | 26000 | no | 862 |
| SettleDeparture | 53 | 1 | 6262 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 53 | 12 | 58696 | 61314 | 61314 | 85000 | no | 859 |
| SweepPoolOwed | 1 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 108 | 0 | 15277 | 37093 | 37864 | 49000 | no | 1197 |
| ResolveFromInputs | 48 | 1 | 30507 | 36723 | 36723 | 290000 | no | 465 |
| SkipQuiet | 745 | 26 | 12426 | 53967 | 59405 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 18 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 49 | 1 | 6496 | 6496 | 6496 | 8000 | no | 373 |
| ClaimDefence | 1 | 4 | 12537 | 12537 | 12537 | 25500 | no | 434 |

**Reveal CU distribution** (C4 input): n 49, p50 19158, p90 21602, p99 21780, max 21780.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 1 | 10 | 25 | 2 | 77 | 437 | 5 s |
| s_to_first_cache_slots | 2688 | 1 | 11 | 23 | 2 | 59 | 459 | 5 s |
| anchor_to_last_reveal_slots | 48 | 0 | 13 | 13 | 4 | 0 | 520 | 30 s |
| s_to_resolve_slots | 48 | 2 | 27 | 27 | 8 | 99 | 1099 | 60 s |
| close_to_resolve_slots | 48 | 4 | 29 | 29 | reported | 160 | 1160 | reported |

Round → anchor from the publication instant, in slots: p50 1.93, p99 10.93, max 25.93.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 24 | 1 | 6 | 6 | 0 |
| churned | 31 | 8 | 132 | 132 | 21 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| resident (resident action or nudge, reported) | 0 | – | – | – | 0 |
| all | 55 | 6 | 132 | 132 | – |

Resident actions 146 (landed or refused), keeper-served nudges 457, season-end flush SkipQuiet 0 (not counted).

**ClashInputs:** 0 closed, 48 open: 48 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2720,"BUILD":107,"CAMP":25,"CLASH":48,"CLOSE":67,"DEFENCE_CLAIM":1,"DEPART":53,"DEPARTURE_SETTLED":53,"DIVERT":1,"EXPLORE":43,"EXPLORE_RESULT":43,"FOLD":511,"GATHER":108,"GENESIS_SEED":1,"HARVEST":201,"HOLDING_FINAL":16,"JOIN":100,"MUSTER":49,"POOL_SWEEP":1,"PROVINCE_OPEN":37,"REVEAL":49,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2880,"SETTLE":100,"SKIP":745,"TICKET":100,"TRAIN":95,"TRANSIT_SETTLED":53}
- transits: {"outcome 1 seal 0":21,"outcome 3 seal 0":14,"outcome 4 seal 0":6,"outcome 8 seal 2":8,"outcome 8 seal 5":4}
- departs 53 (due 53), unsettled due: 0
- bad-seal codes: {"2":8,"5":4}
- failed transactions: {"ClaimDefence: NotEligible":4,"CloseArrivalSlot: BadAccount":1,"Depart: TipTooLow":1,"FoldOccupancy: FoldStale":3,"PostBeacon: AlreadyDone":16,"ResolveFromInputs: OutOfOrder":1,"Reveal: AlreadyDone":63,"Reveal: BadAddress":8,"Reveal: TransitState":1,"Reveal: WindowClosed":5,"SettleDeparture: AlreadyDone":1,"SettleExplore: AlreadyDone":1,"SettleTicket: NoTicket":18,"SettleTransit: TransitState":12,"SkipQuiet: OutOfOrder":26,"Train: Insufficient":3}

### Failed transactions by class (reported, not gating)

164 failed: {"expected":62,"redundancy":97,"unclassified":2,"waste":3} (by cause {"a/b race":76,"adversary":8,"bot-policy":3,"bounded duplicate":27,"claim versions":4,"duplicate":17,"persona":6,"race":21}); unclassified 2.

| kind | error | n | class | cause |
|---|---|---|---|---|
| Reveal | AlreadyDone | 63 | redundancy | a/b race |
| SkipQuiet | OutOfOrder | 26 | expected | bounded duplicate |
| SettleTicket | NoTicket | 18 | expected | race |
| PostBeacon | AlreadyDone | 16 | redundancy | duplicate |
| SettleTransit | TransitState | 12 | redundancy | a/b race |
| Reveal | BadAddress | 8 | expected | adversary |
| Reveal | WindowClosed | 5 | expected | persona |
| ClaimDefence | NotEligible | 4 | redundancy | claim versions |
| FoldOccupancy | FoldStale | 3 | expected | race |
| Train | Insufficient | 3 | waste | bot-policy |
| CloseArrivalSlot | BadAccount | 1 | unclassified |  |
| Depart | TipTooLow | 1 | expected | persona |
| ResolveFromInputs | OutOfOrder | 1 | expected | bounded duplicate |
| Reveal | TransitState | 1 | unclassified |  |
| SettleDeparture | AlreadyDone | 1 | redundancy | a/b race |
| SettleExplore | AlreadyDone | 1 | redundancy | duplicate |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 171 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":85,"anchor_latency_slots_p99":10,"archived_bells":0,"bell":170,"contested_bells":12,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20239573903,"n":32},"funders":{"lamports":2035434224114,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52521918987,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":11,"spend_by_day":{"0":222012052,"1":258566660},"sweeps_sent":1,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 171 bells (0 timeouts); last answered status {"alerts":1,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":1,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999800198,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497394746,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":376461,"1":406115},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 12, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632360: 1 keys, 1000 milli, 360 slots, above keeper cap true
- hold slots-below at game 1785640160: 25 keys, 1900 milli, 38 slots, above keeper cap false
- hold slots-below at game 1785642600: 25 keys, 1900 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785643960: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold slots-below at game 1785645000: 25 keys, 1900 milli, 38 slots, above keeper cap false
- hold defence-pool at game 1785645560: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold anchor at game 1785653160: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662360: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785671560: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold relay-payers at game 1785708360: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785717560: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 161–520 | 1 | 0 | 10 | 521 |
| slots-below | false | 356–393 | 25 | 2 | 1 | 417 |
| slots-below | false | 417–454 | 25 | 0 | 1 | 489 |
| slots-above | true | 451–488 | 25 | 0 | 2 | 489 |
| slots-below | false | 477–514 | 25 | 2 | 2 | 546 |
| defence-pool | true | 491–535 | 1 | 0 | 1 | 536 |
| anchor | true | 681–685 | 1 | 0 | 0 | – |
| keeper-payers | true | 911–925 | 20 | 0 | 1 | 926 |
| lag | true | 1141–1170 | 2 | 0 | 11 | 1171 |
| relay-payers | true | 2061–2075 | 20 | 0 | 0 | – |

### ClaimDefence (M1 exit U4)

landed 1, failed 4, refunded 42132 lamports

| keeper | slot | bell | day | slots | refund | partial | fee | beneficiary after | signature |
|---|---|---|---|---|---|---|---|---|---|
| keeper-a | 536 | 27 | 0 | 1 | 42132 | false | 16347 | 999011985 | `C9stdKHxPdo5JzSKMPdS3VeYJPsdS6Rqw5xEyk6JwZey5UfmWw1ufZiFcXG3VQadk7xh2Mfch9BCms6DsWuRaSY` |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 48,
      "closed": 0,
      "open": 48,
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
      "max": 21780.0,
      "n": 49,
      "p50": 19158.0,
      "p90": 21602.0,
      "p99": 21780.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 16",
      "(-3,1) day 0: 9",
      "(-3,2) day 0: 7",
      "(-3,3) day 0: 82",
      "(-2,2) day 0: 9",
      "(-2,3) day 0: 132",
      "(-1,-2) day 0: 30",
      "(-1,3) day 0: 17",
      "(0,-2) day 0: 7",
      "(0,2) day 0: 8",
      "(0,3) day 0: 47",
      "(1,-3) day 0: 21",
      "(1,-2) day 0: 7",
      "(1,1) day 0: 8",
      "(2,-3) day 0: 37",
      "(2,-2) day 0: 8",
      "(2,0) day 0: 10",
      "(2,1) day 0: 124",
      "(3,-3) day 0: 20",
      "(3,-1) day 0: 9",
      "(3,0) day 0: 23"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states), no GATHER or CLASH of the province that day (§13.4 A1), and no resident action (Muster/Dissolve/Garrison/Explore/Depart naming it, landed or refused) or keeper-served nudge of it within the day or the 26 bells before (A1 as amended, v1.13); active = roster unchanged with a GATHER/CLASH (reported); resident = with a resident action or nudge (reported: each nudge is one split); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day; the SkipQuiet reaching end_bell (the season-end flush) is not counted",
    "idle_province_days_over_6": [],
    "nudges_served": 457,
    "province_days_not_judged": 0,
    "province_post_states": 1543,
    "resident_actions": 146,
    "resident_province_days_over_6": [],
    "season_end_flush": 0,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 132.0,
      "n": 31,
      "p50": 8.0,
      "p90": 47.0,
      "p99": 132.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 24,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 132.0,
      "n": 55,
      "p50": 6.0,
      "p90": 30.0,
      "p99": 132.0
    },
    "skip_txs_per_resident_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 520.0,
      "n": 48,
      "p50": 0.0,
      "p90": 40.0,
      "p99": 520.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 13.0,
      "n": 48,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 13.0
    },
    "close_to_resolve_game_secs": {
      "max": 1160.0,
      "n": 48,
      "p50": 160.0,
      "p90": 200.0,
      "p99": 1160.0
    },
    "close_to_resolve_slots": {
      "max": 29.0,
      "n": 48,
      "p50": 4.0,
      "p90": 5.0,
      "p99": 29.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 25.925,
      "n": 2704,
      "p50": 1.925,
      "p90": 1.925,
      "p99": 10.925
    },
    "round_to_anchor_game_secs": {
      "max": 1037.0,
      "n": 2704,
      "p50": 77.0,
      "p90": 77.0,
      "p99": 437.0
    },
    "round_to_anchor_slots": {
      "max": 25.0,
      "n": 2704,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 10.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 939.0,
      "n": 2688,
      "p50": 59.0,
      "p90": 59.0,
      "p99": 459.0
    },
    "s_to_first_cache_slots": {
      "max": 23.0,
      "n": 2688,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 11.0
    },
    "s_to_resolve_game_secs": {
      "max": 1099.0,
      "n": 48,
      "p50": 99.0,
      "p90": 139.0,
      "p99": 1099.0
    },
    "s_to_resolve_slots": {
      "max": 27.0,
      "n": 48,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 27.0
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
        "denominator": 29828.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "file_answered": 15916,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 4.800000000000001,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.524288,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 12912,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_answered_ms": 131.072,
        "p99_file_ms": 147.456,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "recovery_ok": true,
        "requests": 28828.0,
        "stale_retries": 0,
        "stale_retries_outside": 0,
        "summary": "p99 file 147.5 ms (answered only 131.1 ms), ingest->WS p99 0.52 s (ws-stamp; herald share p99 524 ms, delivery p99 7 ms), error rate 0.00000, gaps 0, 404 12912, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 0,
        "unavailable_ms": 0.0,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 0,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_split": {
          "delivery_max_ms": 13.0,
          "delivery_p50_ms": 0.992,
          "delivery_p99_upper_ms": 7.168,
          "herald_max_ms": 523.0,
          "herald_p50_ms": 34.816,
          "herald_p99_upper_ms": 524.288,
          "send_stamped": 830552
        },
        "ws_timed": 830552.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 8,
      "5": 4
    },
    "marchbook": {
      "codes": {
        "bad_plaintext:5": 4,
        "garbage:2": 8
      },
      "due": {
        "bad_plaintext": 4,
        "garbage": 8
      },
      "not_bad_seal": []
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
