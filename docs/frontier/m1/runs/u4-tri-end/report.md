# Stack run `u4-tri-end`

- phase **complete**, beacon **test-key**, scale 100×, 300 bots, play 288 bells + 26 drain; program `Axs9K1QK9GrK8TT67j3G29R8pri4m9Qvnv66EXoC1yun` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (25368 transactions, last bell 314)

- **NOT exit-grade**: ["hold-skipped slots-above (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 3.95, p99 9.39, max 10.56 at bell 123; max per game day [10.56,6.88,4.1]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 107 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 8 max 8 (1 idle days over 6), per churned day p99 36 max 36 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped slots-above (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes []; 25368 txs, read 0.67 s, verify 0.99 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (28 built from the run; 6.41 s)
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
| PostAnchor | 16 | 2672 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 864 | 504 | 377373 | 380293 | 380714 | 400000 | no | 1177 |
| PostSeed | 4608 | 0 | 337199 | 339413 | 339560 | 345000 | no | 781 |
| PostBeacon | 3472 | 0 | 330677 | 332774 | 333103 | 340000 | no | 645 |
| ArchiveAnchors | 400 | 0 | 8852 | 12834 | 12834 | 60000 | no | 439 |
| CloseSeedCache | 400 | 0 | 5802 | 5802 | 5802 | 6000 | no | 369 |
| OpenRing | 5 | 0 | 15070 | 15380 | 15380 | 30000 | no | 563 |
| ConsumeRingSeed | 1 | 0 | 331852 | 331852 | 331852 | 345000 | no | 646 |
| OpenProvince | 61 | 8 | 139068 | 145462 | 145462 | 220000 | no | 400 |
| FoldOccupancy | 648 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 300 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 301 | 0 | 14222 | 16101 | 16126 | 17000 | no | 575 |
| SettleTicket | 310 | 8 | 20188 | 21655 | 21687 | 40000 | no | 562 |
| Harvest | 272 | 0 | 12161 | 16686 | 16686 | 17500 | no | 427 |
| Build | 518 | 1 | 11742 | 18144 | 18154 | 22000 | no | 428 |
| Train | 387 | 1 | 12220 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 143 | 9 | 14797 | 18085 | 18099 | 25000 | no | 466 |
| Explore | 50 | 7 | 14103 | 16383 | 16383 | 20000 | no | 471 |
| SettleExplore | 50 | 0 | 8346 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 89 | 6 | 18356 | 22941 | 22941 | 24500 | no | 712 |
| Reveal | 89 | 0 | 20205 | 23041 | 23041 | 26000 | no | 895 |
| SettleDeparture | 91 | 91 | 6272 | 8283 | 8283 | 48000 | no | 331 |
| SettleTransit | 89 | 89 | 60558 | 62936 | 62936 | 85000 | no | 859 |
| SweepPoolOwed | 89 | 0 | 5206 | 5216 | 5216 | 8000 | no | 330 |
| GatherClash | 196 | 0 | 14280 | 37093 | 37096 | 49000 | no | 1197 |
| ResolveFromInputs | 107 | 0 | 34915 | 43787 | 45719 | 290000 | no | 465 |
| SkipQuiet | 1092 | 2 | 26023 | 54155 | 57405 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 9 | 7066 | 5979 | 5979 | 5979 | 8000 | no | 379 |
| CloseArrivalSlot | 88 | 148 | 6496 | 6496 | 6496 | 8000 | no | 381 |

**Reveal CU distribution** (C4 input): n 89, p50 20205, p90 21672, p99 23041, max 23041.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 4608 | 2 | 2 | 2 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 4608 | 2 | 2 | 4 | 2 | 99 | 99 | 5 s |
| anchor_to_last_reveal_slots | 89 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 107 | 4 | 4 | 5 | 8 | 179 | 179 | 60 s |
| close_to_resolve_slots | 107 | 6 | 6 | 7 | reported | 240 | 240 | reported |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 2.98.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 43 | 6 | 8 | 8 | 1 |
| churned | 73 | 8 | 36 | 36 | 46 |
| active (GATHER/CLASH, reported) | 2 | 7 | 7 | 7 | 2 |
| all | 118 | 6 | 34 | 36 | – |

**ClashInputs:** 0 closed, 107 open: 107 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":4608,"ANNOUNCE":1,"ARCHIVE":400,"BEACON":3472,"BUILD":518,"CAMP":47,"CLASH":107,"CLOSE":897,"DEPART":89,"DEPARTURE_SETTLED":91,"DIVERT":178,"EXPLORE":50,"EXPLORE_RESULT":50,"FOLD":648,"GATHER":196,"GENESIS_SEED":1,"HARVEST":272,"HOLDING_FINAL":58,"JOIN":300,"MUSTER":143,"POOL_SWEEP":89,"PROVINCE_OPEN":61,"REVEAL":89,"RING_OPEN":5,"RING_SEED":1,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":4608,"SETTLE":310,"SKIP":1092,"TICKET":301,"TRAIN":387,"TRANSIT_SETTLED":89}
- transits: {"outcome 1 seal 0":45,"outcome 3 seal 0":24,"outcome 4 seal 0":20}
- departs 89 (due 89), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"Build: QueueFull":1,"CloseArrivalDay: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":3533,"CloseArrivalDay: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":3533,"CloseArrivalSlot: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":74,"CloseArrivalSlot: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":74,"Depart: NotResident":6,"Explore: NotResident":7,"Muster: NotResident":7,"Muster: ProvinceFull":2,"OpenProvince: AlreadyDone":8,"PostAnchor: BadData":2672,"PostAnchorMulti: BadData":504,"SettleDeparture: AlreadyDone":91,"SettleTicket: NoTicket":8,"SettleTransit: TransitState":89,"SkipQuiet: OutOfOrder":2,"Train: Insufficient":1}

### Failed transactions by class (reported, not gating)

10612 failed: {"expected":8,"redundancy":188,"unclassified":3,"waste":10413} (by cause {"a/b race":180,"bot-policy":23,"duplicate":8,"keeper":10390,"race":8}); unclassified 3.

| kind | error | n | class | cause |
|---|---|---|---|---|
| CloseArrivalDay | {"InstructionError":[3,"ProgramFailedToComplete"]} | 3533 | waste | keeper |
| CloseArrivalDay | {"InstructionError":[4,"ProgramFailedToComplete"]} | 3533 | waste | keeper |
| PostAnchor | BadData | 2672 | waste | keeper |
| PostAnchorMulti | BadData | 504 | waste | keeper |
| SettleDeparture | AlreadyDone | 91 | redundancy | a/b race |
| SettleTransit | TransitState | 89 | redundancy | a/b race |
| CloseArrivalSlot | {"InstructionError":[3,"ProgramFailedToComplete"]} | 74 | waste | keeper |
| CloseArrivalSlot | {"InstructionError":[4,"ProgramFailedToComplete"]} | 74 | waste | keeper |
| OpenProvince | AlreadyDone | 8 | redundancy | duplicate |
| SettleTicket | NoTicket | 8 | expected | race |
| Explore | NotResident | 7 | waste | bot-policy |
| Muster | NotResident | 7 | waste | bot-policy |
| Depart | NotResident | 6 | waste | bot-policy |
| Muster | ProvinceFull | 2 | waste | bot-policy |
| SkipQuiet | OutOfOrder | 2 | unclassified |  |
| Build | QueueFull | 1 | waste | bot-policy |
| Train | Insufficient | 1 | unclassified |  |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 61; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 2 of 315 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":10390,"anchor_latency_slots_p99":1,"archived_bells":400,"bell":314,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19954999280,"n":32},"funders":{"lamports":2030592960040,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52453964100,"n":150}},"provinces_opened":61,"rings_complete":[0,1,2,3,4],"seed_latency_slots_p99":1,"spend_by_day":{"1":65337083,"2":246615272},"sweeps_sent":30,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 315 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":314,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23998334150,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52500000000,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":384967,"1":1656299,"2":1665850},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 9, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 11, restarts 11, crashes 0
- hold ticket at game 1785632880: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold-skipped slots-above at game 1785657480: – keys, – milli, – slots, above keeper cap –
- hold anchor at game 1785672280: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold slots-below at game 1785676680: 25 keys, 1500 milli, 38 slots, above keeper cap false
- hold keeper-payers at game 1785691080: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785710280: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold frontier-fund at game 1785716880: 2 keys, 1000 milli, 15 slots, above keeper cap true
- hold relay-payers at game 1785785080: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped defence-pool at game 1785804480: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 174–518 | 1 | 0 | 19 | 519 |
| anchor | true | 1159–1163 | 1 | 0 | 0 | – |
| slots-below | false | 1269–1306 | 25 | 0 | 0 | – |
| keeper-payers | true | 1629–1643 | 20 | 0 | 0 | – |
| lag | true | 2109–2138 | 2 | 0 | 3 | 2139 |
| frontier-fund | true | 2274–2288 | 2 | 0 | 10 | 2289 |
| relay-payers | true | 3979–3993 | 20 | 0 | 2 | 3997 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 107,
      "closed": 0,
      "open": 107,
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
      "max": 23041.0,
      "n": 89,
      "p50": 20205.0,
      "p90": 21672.0,
      "p99": 23041.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(0,-2) day 0: 7",
      "(1,-2) day 1: 7"
    ],
    "churned_province_days_over_6": [
      "(-4,0) day 1: 7",
      "(-4,1) day 1: 12",
      "(-4,2) day 1: 31",
      "(-4,3) day 1: 8",
      "(-3,0) day 1: 11",
      "(-3,1) day 0: 32",
      "(-3,1) day 1: 25",
      "(-3,2) day 0: 7",
      "(-3,2) day 1: 13",
      "(-3,3) day 0: 16",
      "(-3,3) day 1: 24",
      "(-2,-2) day 1: 31",
      "(-2,-1) day 0: 7",
      "(-2,-1) day 1: 17",
      "(-2,0) day 0: 7",
      "(-2,0) day 1: 8",
      "(-2,1) day 1: 8",
      "(-2,3) day 0: 7",
      "(-2,3) day 1: 12",
      "(-1,-3) day 1: 8",
      "(-1,-2) day 0: 21",
      "(-1,-2) day 1: 17",
      "(-1,2) day 1: 7",
      "(-1,3) day 0: 15",
      "(-1,3) day 1: 36",
      "(-1,4) day 1: 8",
      "(0,-3) day 1: 32",
      "(0,2) day 1: 8",
      "(0,3) day 0: 22",
      "(0,3) day 1: 29",
      "(0,4) day 1: 23",
      "(1,-4) day 1: 7",
      "(1,1) day 1: 8",
      "(1,2) day 0: 15",
      "(1,2) day 1: 27",
      "(2,-3) day 1: 34",
      "(2,-2) day 1: 7",
      "(2,1) day 1: 11",
      "(2,2) day 1: 27",
      "(3,-3) day 1: 29",
      "(3,-2) day 1: 14",
      "(3,-1) day 1: 12",
      "(3,0) day 1: 15",
      "(3,1) day 1: 7",
      "(4,-3) day 1: 20",
      "(4,0) day 1: 15"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [
      "(1,-3) day 1: 8"
    ],
    "province_days_not_judged": 0,
    "province_post_states": 3130,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 2,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 36.0,
      "n": 73,
      "p50": 8.0,
      "p90": 29.0,
      "p99": 36.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 8.0,
      "n": 43,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 8.0
    },
    "skip_txs_per_province_day": {
      "max": 36.0,
      "n": 118,
      "p50": 6.0,
      "p90": 24.0,
      "p99": 34.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 89,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 89,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 280.0,
      "n": 107,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 7.0,
      "n": 107,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 2.975,
      "n": 4608,
      "p50": 2.975,
      "p90": 2.975,
      "p99": 2.975
    },
    "round_to_anchor_game_secs": {
      "max": 119.0,
      "n": 4608,
      "p50": 119.0,
      "p90": 119.0,
      "p99": 119.0
    },
    "round_to_anchor_slots": {
      "max": 2.0,
      "n": 4608,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 179.0,
      "n": 4608,
      "p50": 99.0,
      "p90": 99.0,
      "p99": 99.0
    },
    "s_to_first_cache_slots": {
      "max": 4.0,
      "n": 4608,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "s_to_resolve_game_secs": {
      "max": 219.0,
      "n": 107,
      "p50": 179.0,
      "p90": 179.0,
      "p99": 179.0
    },
    "s_to_resolve_slots": {
      "max": 5.0,
      "n": 107,
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
