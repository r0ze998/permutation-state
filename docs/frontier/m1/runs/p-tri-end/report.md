# Stack run `p-tri-end`

- phase **complete**, beacon **test-key**, scale 100×, 300 bots, play 288 bells + 26 drain; program `CiMd6MH3ENijDCw5fNCg2jW2oBCzq8Au4XyJm4f6cvYK` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (16525 transactions, last bell 313)

- **NOT exit-grade**: ["no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 3.49, p99 4.81, max 5.04 at bell 15; max per game day [5.04,4.47,4.44]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 173 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 33 max 33 (reported) |
| 4 | **pass** | 0 unrevealed inside 8 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes []; 16525 txs, read 0.49 s, verify 1.02 s — E6 wall time)
- tamper: 29/30 classes FAIL with their codes (29 built from the run; 6.40 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 14258 | 14258 | 14258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 40 | 0 | 4655 | 337096 | 337096 | 345000 | no | 780 |
| PostAnchorMulti | 866 | 0 | 377348 | 380293 | 380714 | 400000 | no | 1177 |
| PostSeed | 4608 | 0 | 337286 | 339530 | 339646 | 345000 | no | 781 |
| PostBeacon | 5056 | 0 | 330582 | 332736 | 333198 | 340000 | no | 645 |
| ArchiveAnchors | 400 | 0 | 8852 | 12834 | 12834 | 60000 | no | 439 |
| CloseSeedCache | 400 | 0 | 5802 | 5802 | 5802 | 6000 | no | 369 |
| OpenRing | 5 | 0 | 15070 | 15380 | 15380 | 30000 | no | 563 |
| ConsumeRingSeed | 1 | 0 | 331852 | 331852 | 331852 | 345000 | no | 646 |
| OpenProvince | 61 | 8 | 139068 | 145462 | 145462 | 220000 | no | 400 |
| FoldOccupancy | 648 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 300 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 301 | 0 | 14222 | 16101 | 16126 | 17000 | no | 575 |
| SettleTicket | 310 | 9 | 20189 | 21655 | 21687 | 40000 | no | 562 |
| Harvest | 261 | 0 | 12161 | 16686 | 16686 | 17500 | no | 427 |
| Build | 519 | 2 | 11742 | 18144 | 18154 | 22000 | no | 428 |
| Train | 397 | 0 | 12220 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 154 | 8 | 14790 | 18063 | 18101 | 25000 | no | 466 |
| Explore | 55 | 2 | 14037 | 16276 | 16276 | 20000 | no | 471 |
| SettleExplore | 55 | 0 | 8346 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 94 | 8 | 18338 | 20691 | 20691 | 24500 | no | 712 |
| Reveal | 94 | 28 | 20208 | 22907 | 22907 | 26000 | no | 895 |
| SettleDeparture | 94 | 0 | 6272 | 6322 | 6322 | 48000 | no | 331 |
| SettleTransit | 94 | 1 | 60401 | 62979 | 62979 | 85000 | no | 827 |
| SweepPoolOwed | 93 | 1 | 5206 | 5216 | 5216 | 8000 | no | 330 |
| GatherClash | 267 | 0 | 14270 | 37093 | 37096 | 49000 | no | 1197 |
| ResolveFromInputs | 173 | 3 | 34925 | 42310 | 42446 | 290000 | no | 465 |
| SkipQuiet | 953 | 0 | 38706 | 54209 | 57883 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 51 | 0 | 6018 | 6018 | 6018 | 8000 | no | 371 |
| CloseArrivalSlot | 94 | 0 | 6496 | 6538 | 6538 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 94, p50 20208, p90 21753, p99 22907, max 22907.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 4608 | 1 | 1 | 5 | 2 | 79 | 79 | 5 s |
| s_to_first_cache_slots | 4608 | 1 | 1 | 1 | 2 | 58 | 58 | 5 s |
| anchor_to_last_reveal_slots | 94 | 0 | 7 | 7 | 4 | 0 | 280 | 30 s |
| s_to_resolve_slots | 173 | 2 | 3 | 32 | 8 | 98 | 138 | 60 s |
| close_to_resolve_slots | 173 | 4 | 5 | 34 | reported | 160 | 200 | reported |

Round → anchor from the publication instant, in slots: p50 1.98, p99 1.98, max 5.97.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 40 | 6 | 6 | 6 | 0 |
| churned | 76 | 7 | 33 | 33 | 45 |
| active (GATHER/CLASH, reported) | 2 | 7 | 7 | 7 | 2 |
| all | 118 | 6 | 27 | 33 | – |

**ClashInputs:** 0 closed, 173 open: 173 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":4608,"ANNOUNCE":1,"ARCHIVE":400,"BEACON":5056,"BUILD":519,"CAMP":50,"CLASH":173,"CLOSE":945,"DEPART":94,"DEPARTURE_SETTLED":94,"DIVERT":188,"EXPLORE":55,"EXPLORE_RESULT":55,"FOLD":648,"GATHER":267,"GENESIS_SEED":1,"HARVEST":261,"HOLDING_FINAL":62,"JOIN":300,"MUSTER":154,"POOL_SWEEP":93,"PROVINCE_OPEN":61,"REVEAL":94,"RING_OPEN":5,"RING_SEED":1,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":4608,"SETTLE":310,"SKIP":953,"TICKET":301,"TRAIN":397,"TRANSIT_SETTLED":94}
- transits: {"outcome 1 seal 0":49,"outcome 3 seal 0":23,"outcome 4 seal 0":21,"outcome 5 seal 0":1}
- departs 94 (due 94), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"Build: Insufficient":1,"Build: QueueFull":1,"Depart: NotResident":8,"Explore: NotResident":2,"Muster: NotResident":8,"OpenProvince: AlreadyDone":8,"ResolveFromInputs: OutOfOrder":3,"Reveal: AlreadyDone":28,"SettleTicket: NoTicket":9,"SettleTransit: TransitState":1,"SweepPoolOwed: AlreadyDone":1}

### Failed transactions by class (reported, not gating)

70 failed: {"expected":12,"redundancy":38,"waste":20} (by cause {"a/b race":29,"bot-policy":20,"bounded duplicate":3,"duplicate":9,"race":9}); unclassified 0.

| kind | error | n | class | cause |
|---|---|---|---|---|
| Reveal | AlreadyDone | 28 | redundancy | a/b race |
| SettleTicket | NoTicket | 9 | expected | race |
| Depart | NotResident | 8 | waste | bot-policy |
| Muster | NotResident | 8 | waste | bot-policy |
| OpenProvince | AlreadyDone | 8 | redundancy | duplicate |
| ResolveFromInputs | OutOfOrder | 3 | expected | bounded duplicate |
| Explore | NotResident | 2 | waste | bot-policy |
| Build | Insufficient | 1 | waste | bot-policy |
| Build | QueueFull | 1 | waste | bot-policy |
| SettleTransit | TransitState | 1 | redundancy | a/b race |
| SweepPoolOwed | AlreadyDone | 1 | redundancy | duplicate |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 61; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 315 bells (0 timeouts; answer ms p99 2); last answered status {"alerts":0,"anchor_latency_slots_p99":1,"archived_bells":400,"bell":314,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20441004374,"n":32},"funders":{"lamports":2029733238111,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497340819,"n":150}},"provinces_opened":61,"rings_complete":[0,1,2,3,4],"seed_latency_slots_p99":1,"spend_by_day":{"1":73237213,"2":96625379},"sweeps_sent":33,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 315 bells (0 timeouts); last answered status {"alerts":1,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":314,"contested_bells":1,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999840000,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52499728340,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":174827,"1":174827,"2":174827},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 10, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 11, restarts 11, crashes 0
- hold ticket at game 1785632880: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold slots-below at game 1785668280: 25 keys, 1500 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785668280: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold defence-pool at game 1785670080: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold anchor at game 1785672280: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785691080: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785709880: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold frontier-fund at game 1785716880: 2 keys, 1000 milli, 15 slots, above keeper cap true
- hold relay-payers at game 1785785080: 20 keys, 3000 milli, 15 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 174–518 | 1 | 0 | 12 | 519 |
| slots-below | false | 1059–1096 | 25 | 0 | 2 | 1097 |
| slots-above | true | 1059–1096 | 25 | 0 | 2 | 1097 |
| defence-pool | true | 1104–1148 | 1 | 0 | 1 | 1149 |
| anchor | true | 1159–1163 | 1 | 0 | 0 | – |
| keeper-payers | true | 1629–1643 | 20 | 0 | 0 | – |
| lag | true | 2099–2128 | 2 | 0 | 9 | 2129 |
| frontier-fund | true | 2274–2288 | 2 | 0 | 10 | 2289 |
| relay-payers | true | 3979–3993 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 173,
      "closed": 0,
      "open": 173,
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
      "max": 22907.0,
      "n": 94,
      "p50": 20208.0,
      "p90": 21753.0,
      "p99": 22907.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(-1,-1) day 1: 7",
      "(0,-2) day 0: 7"
    ],
    "churned_province_days_over_6": [
      "(-4,1) day 1: 7",
      "(-4,2) day 1: 16",
      "(-4,3) day 1: 8",
      "(-3,0) day 1: 7",
      "(-3,1) day 0: 22",
      "(-3,1) day 1: 25",
      "(-3,2) day 1: 8",
      "(-3,3) day 0: 13",
      "(-3,3) day 1: 20",
      "(-2,-2) day 1: 22",
      "(-2,-1) day 1: 7",
      "(-2,0) day 1: 9",
      "(-2,1) day 1: 7",
      "(-2,2) day 1: 7",
      "(-2,3) day 0: 8",
      "(-2,3) day 1: 10",
      "(-2,4) day 1: 7",
      "(-1,-2) day 0: 22",
      "(-1,-2) day 1: 19",
      "(-1,2) day 1: 7",
      "(-1,3) day 0: 11",
      "(-1,3) day 1: 33",
      "(-1,4) day 1: 7",
      "(0,-3) day 1: 17",
      "(0,2) day 1: 7",
      "(0,3) day 0: 19",
      "(0,3) day 1: 27",
      "(0,4) day 1: 23",
      "(1,-3) day 1: 8",
      "(1,-2) day 1: 7",
      "(1,1) day 1: 7",
      "(1,2) day 0: 16",
      "(1,2) day 1: 17",
      "(1,3) day 1: 8",
      "(2,-3) day 1: 22",
      "(2,-2) day 1: 7",
      "(2,1) day 1: 9",
      "(2,2) day 1: 19",
      "(3,-3) day 1: 24",
      "(3,-2) day 1: 11",
      "(3,-1) day 1: 10",
      "(3,0) day 1: 13",
      "(4,-4) day 1: 7",
      "(4,-3) day 1: 19",
      "(4,0) day 1: 14"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 3094,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 2,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 33.0,
      "n": 76,
      "p50": 7.0,
      "p90": 22.0,
      "p99": 33.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 40,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 33.0,
      "n": 118,
      "p50": 6.0,
      "p90": 19.0,
      "p99": 27.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 280.0,
      "n": 94,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 280.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 7.0,
      "n": 94,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 7.0
    },
    "close_to_resolve_game_secs": {
      "max": 1360.0,
      "n": 173,
      "p50": 160.0,
      "p90": 160.0,
      "p99": 200.0
    },
    "close_to_resolve_slots": {
      "max": 34.0,
      "n": 173,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 5.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 5.975,
      "n": 4608,
      "p50": 1.975,
      "p90": 1.975,
      "p99": 1.975
    },
    "round_to_anchor_game_secs": {
      "max": 239.0,
      "n": 4608,
      "p50": 79.0,
      "p90": 79.0,
      "p99": 79.0
    },
    "round_to_anchor_slots": {
      "max": 5.0,
      "n": 4608,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 59.0,
      "n": 4608,
      "p50": 58.0,
      "p90": 58.0,
      "p99": 58.0
    },
    "s_to_first_cache_slots": {
      "max": 1.0,
      "n": 4608,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_resolve_game_secs": {
      "max": 1298.0,
      "n": 173,
      "p50": 98.0,
      "p90": 98.0,
      "p99": 138.0
    },
    "s_to_resolve_slots": {
      "max": 32.0,
      "n": 173,
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
