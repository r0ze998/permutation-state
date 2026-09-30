# Stack run `integ-w6t-tri-end`

- phase **complete**, beacon **test-key**, scale 100×, 300 bots, play 288 bells + 26 drain; program `6BZbS5LrGUEvi1jA5na5Zs2cepFGsEszd5ive42tgZev` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (16324 transactions, last bell 313)

- **NOT exit-grade**: ["hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 6.05, p99 27.07, max 27.66 at bell 119; max per game day [27.66,11.89,6.12]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 122 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 39 max 39 (reported) |
| 4 | **pass** | 0 unrevealed inside 7 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes []; 16324 txs, read 0.53 s, verify 1.06 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (29 built from the run; 6.74 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 60 | 0 | 4655 | 337096 | 337096 | 345000 | no | 780 |
| PostAnchorMulti | 865 | 0 | 377348 | 380293 | 380714 | 400000 | no | 1177 |
| PostSeed | 4630 | 0 | 337279 | 339530 | 339646 | 345000 | no | 781 |
| PostBeacon | 5040 | 16 | 330585 | 332736 | 333198 | 340000 | no | 645 |
| ArchiveAnchors | 400 | 0 | 8852 | 12834 | 12834 | 60000 | no | 439 |
| CloseSeedCache | 400 | 0 | 5802 | 5802 | 5802 | 6000 | no | 369 |
| OpenRing | 5 | 0 | 15070 | 15380 | 15380 | 30000 | no | 563 |
| ConsumeRingSeed | 1 | 0 | 331852 | 331852 | 331852 | 345000 | no | 646 |
| OpenProvince | 61 | 8 | 139068 | 145462 | 145462 | 220000 | no | 400 |
| FoldOccupancy | 648 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 300 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 301 | 0 | 14221 | 16101 | 16126 | 17000 | no | 575 |
| SettleTicket | 309 | 9 | 20184 | 21655 | 21687 | 40000 | no | 562 |
| Harvest | 267 | 0 | 12161 | 16686 | 16686 | 17500 | no | 427 |
| Build | 514 | 3 | 11742 | 18144 | 18154 | 22000 | no | 428 |
| Train | 389 | 1 | 12220 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 140 | 10 | 14794 | 18099 | 18101 | 25000 | no | 466 |
| Explore | 51 | 4 | 14099 | 16387 | 16387 | 20000 | no | 471 |
| SettleExplore | 51 | 0 | 8346 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 82 | 6 | 16594 | 22955 | 22955 | 24500 | no | 712 |
| Reveal | 82 | 2 | 20208 | 22809 | 22809 | 26000 | no | 895 |
| SettleDeparture | 82 | 0 | 6262 | 6322 | 6322 | 48000 | no | 331 |
| SettleTransit | 82 | 1 | 62151 | 63104 | 63104 | 85000 | no | 827 |
| SweepPoolOwed | 82 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 204 | 0 | 14270 | 37090 | 37093 | 49000 | no | 1197 |
| ResolveFromInputs | 122 | 0 | 33307 | 44802 | 44883 | 290000 | no | 465 |
| SkipQuiet | 955 | 2 | 41380 | 54155 | 57414 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 46 | 0 | 6018 | 6018 | 6018 | 8000 | no | 371 |
| CloseArrivalSlot | 82 | 0 | 6496 | 6538 | 6538 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 82, p50 20208, p90 21812, p99 22809, max 22809.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 4608 | 1 | 2 | 5 | 2 | 79 | 119 | 5 s |
| s_to_first_cache_slots | 4608 | 1 | 2 | 6 | 2 | 58 | 98 | 5 s |
| anchor_to_last_reveal_slots | 82 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 122 | 2 | 6 | 6 | 8 | 98 | 258 | 60 s |
| close_to_resolve_slots | 122 | 4 | 8 | 8 | reported | 160 | 320 | reported |

Round → anchor from the publication instant, in slots: p50 1.98, p99 2.98, max 5.97.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 43 | 6 | 6 | 6 | 0 |
| churned | 73 | 7 | 39 | 39 | 42 |
| active (GATHER/CLASH, reported) | 2 | 7 | 7 | 7 | 2 |
| all | 118 | 6 | 32 | 39 | – |

**ClashInputs:** 0 closed, 122 open: 122 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":4608,"ANNOUNCE":1,"ARCHIVE":400,"BEACON":5040,"BUILD":514,"CAMP":48,"CLASH":122,"CLOSE":928,"DEPART":82,"DEPARTURE_SETTLED":82,"DIVERT":164,"EXPLORE":51,"EXPLORE_RESULT":51,"FOLD":648,"GATHER":204,"GENESIS_SEED":1,"HARVEST":267,"HOLDING_FINAL":58,"JOIN":300,"MUSTER":140,"POOL_SWEEP":82,"PROVINCE_OPEN":61,"REVEAL":82,"RING_OPEN":5,"RING_SEED":1,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":4630,"SETTLE":309,"SKIP":955,"TICKET":301,"TRAIN":389,"TRANSIT_SETTLED":82}
- transits: {"outcome 1 seal 0":40,"outcome 3 seal 0":24,"outcome 4 seal 0":18}
- departs 82 (due 82), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"Build: QueueFull":3,"Depart: NotResident":6,"Explore: NotResident":4,"Muster: NotResident":10,"OpenProvince: AlreadyDone":8,"PostBeacon: AlreadyDone":16,"Reveal: TransitState":2,"SettleTicket: NoTicket":9,"SettleTransit: TransitState":1,"SkipQuiet: OutOfOrder":2,"Train: Insufficient":1}

### Failed transactions by class (reported, not gating)

62 failed: {"expected":11,"redundancy":25,"unclassified":2,"waste":24} (by cause {"a/b race":1,"bot-policy":24,"bounded duplicate":2,"duplicate":24,"race":9}); unclassified 2.

| kind | error | n | class | cause |
|---|---|---|---|---|
| PostBeacon | AlreadyDone | 16 | redundancy | duplicate |
| Muster | NotResident | 10 | waste | bot-policy |
| SettleTicket | NoTicket | 9 | expected | race |
| OpenProvince | AlreadyDone | 8 | redundancy | duplicate |
| Depart | NotResident | 6 | waste | bot-policy |
| Explore | NotResident | 4 | waste | bot-policy |
| Build | QueueFull | 3 | waste | bot-policy |
| Reveal | TransitState | 2 | unclassified |  |
| SkipQuiet | OutOfOrder | 2 | expected | bounded duplicate |
| SettleTransit | TransitState | 1 | redundancy | a/b race |
| Train | Insufficient | 1 | waste | bot-policy |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 61; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 315 bells (0 timeouts; answer ms p99 0); last answered status {"alerts":0,"anchor_latency_slots_p99":2,"archived_bells":400,"bell":314,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19947703258,"n":32},"funders":{"lamports":2030561838511,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52498705343,"n":150}},"provinces_opened":61,"rings_complete":[0,1,2,3,4],"seed_latency_slots_p99":1,"spend_by_day":{"1":69986367,"2":93190161},"sweeps_sent":26,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 315 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":314,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999840000,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52500000000,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":160000,"1":160000,"2":160000},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 11, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 11, restarts 11, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold anchor at game 1785672280: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold slots-below at game 1785686280: 25 keys, 1500 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785687520: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold keeper-payers at game 1785691080: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785709880: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold frontier-fund at game 1785716880: 2 keys, 1000 milli, 15 slots, above keeper cap true
- hold relay-payers at game 1785785080: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped defence-pool at game 1785804480: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 159–503 | 1 | 0 | 14 | 504 |
| anchor | true | 1159–1163 | 1 | 0 | 0 | – |
| slots-below | false | 1509–1546 | 25 | 1 | 1 | 1571 |
| slots-above | true | 1540–1577 | 25 | 0 | 0 | – |
| keeper-payers | true | 1629–1643 | 20 | 0 | 0 | – |
| lag | true | 2099–2128 | 2 | 0 | 10 | 2129 |
| frontier-fund | true | 2274–2288 | 2 | 0 | 10 | 2289 |
| relay-payers | true | 3979–3993 | 20 | 0 | 2 | 3996 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 122,
      "closed": 0,
      "open": 122,
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
      "max": 22809.0,
      "n": 82,
      "p50": 20208.0,
      "p90": 21812.0,
      "p99": 22809.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(-2,1) day 1: 7",
      "(1,-2) day 1: 7"
    ],
    "churned_province_days_over_6": [
      "(-4,1) day 1: 8",
      "(-4,2) day 1: 20",
      "(-4,3) day 1: 8",
      "(-3,1) day 0: 20",
      "(-3,1) day 1: 14",
      "(-3,2) day 1: 8",
      "(-3,3) day 0: 13",
      "(-3,3) day 1: 21",
      "(-2,-2) day 1: 8",
      "(-2,-1) day 1: 7",
      "(-2,0) day 1: 9",
      "(-2,2) day 1: 7",
      "(-2,3) day 0: 7",
      "(-2,3) day 1: 8",
      "(-1,-3) day 1: 18",
      "(-1,-2) day 0: 24",
      "(-1,-2) day 1: 21",
      "(-1,2) day 1: 7",
      "(-1,3) day 0: 8",
      "(-1,3) day 1: 17",
      "(-1,4) day 1: 7",
      "(0,-4) day 1: 8",
      "(0,-3) day 1: 32",
      "(0,2) day 1: 8",
      "(0,3) day 0: 24",
      "(0,3) day 1: 39",
      "(0,4) day 1: 19",
      "(1,-3) day 1: 8",
      "(1,2) day 0: 15",
      "(1,2) day 1: 23",
      "(1,3) day 1: 7",
      "(2,-3) day 1: 24",
      "(2,-2) day 1: 7",
      "(2,1) day 1: 10",
      "(2,2) day 1: 24",
      "(3,-3) day 1: 22",
      "(3,-2) day 1: 11",
      "(3,-1) day 1: 9",
      "(3,0) day 1: 12",
      "(3,1) day 1: 7",
      "(4,-3) day 1: 15",
      "(4,0) day 1: 13"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 2971,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 2,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 39.0,
      "n": 73,
      "p50": 7.0,
      "p90": 22.0,
      "p99": 39.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 43,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 39.0,
      "n": 118,
      "p50": 6.0,
      "p90": 20.0,
      "p99": 32.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 82,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 82,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 320.0,
      "n": 122,
      "p50": 160.0,
      "p90": 160.0,
      "p99": 320.0
    },
    "close_to_resolve_slots": {
      "max": 8.0,
      "n": 122,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 8.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 5.975,
      "n": 4608,
      "p50": 1.975,
      "p90": 1.975,
      "p99": 2.975
    },
    "round_to_anchor_game_secs": {
      "max": 239.0,
      "n": 4608,
      "p50": 79.0,
      "p90": 79.0,
      "p99": 119.0
    },
    "round_to_anchor_slots": {
      "max": 5.0,
      "n": 4608,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 258.0,
      "n": 4608,
      "p50": 58.0,
      "p90": 58.0,
      "p99": 98.0
    },
    "s_to_first_cache_slots": {
      "max": 6.0,
      "n": 4608,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 2.0
    },
    "s_to_resolve_game_secs": {
      "max": 258.0,
      "n": 122,
      "p50": 98.0,
      "p90": 98.0,
      "p99": 258.0
    },
    "s_to_resolve_slots": {
      "max": 6.0,
      "n": 122,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 6.0
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
