# Stack run `integ-w6t-rv-racer`

- phase **complete**, beacon **test-key**, scale 20×, 300 bots, play 72 bells + 26 drain; program `GGDzgskAhX7KZBZb8N1pr1qs8NisKH7fAR2Y3HH6Lkjp` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (8961 transactions, last bell 97)

- **NOT exit-grade**: ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 5.14, p99 6.65, max 6.65 at bell 13; max per game day [6.65]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 84 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 232.00 > 2 slots; s_to_first_cache_slots p99 223.00 > 2 slots; anchor_to_last_reveal_slots p99 62.00 > 4 slots; s_to_resolve_slots p99 18.00 > 8 slots; SkipQuiet per idle province-day p99 4 max 4 (0 idle days over 6), per churned day p99 61 max 61 (reported) |
| 4 | **pass** | 0 unrevealed inside 7 above-cap hold windows (expected); unrevealed by rule: bounced 4 |
| 5 | **n.a.** | no persona violated, but not every expected outcome was observed: forger refused with a code its rule does not name ({"build@relay:ok":15,"depart@relay:ok":12,"explore@relay:ok":13,"file_ticket@relay:ok":3,"harvest@relay:ok":34,"join@relay:ok":3,"muster@relay:ok":9,"reveal@direct:BadAddress":11,"reveal@keeper:ok":11,"settle_explore@relay:ok":3,"settle_transit@relay:BadAddress":1,"settle_transit@relay:TooEarly":2,"settle_transit@relay:ok":1,"train@relay:ok":15}); late_revealer refused with a code its rule does not name ({"build@relay:ok":16,"depart@relay:ok":11,"explore@relay:ok":19,"file_ticket@relay:ok":3,"harvest@relay:ok":30,"join@relay:ok":3,"muster@relay:ok":12,"reveal@direct:AlreadyDone":2,"reveal@direct:CommitMismatch":1,"reveal@direct:WindowClosed":7,"reveal@keeper:CommitMismatch":1,"reveal@keeper:WindowClosed":7,"reveal@keeper:ok":2,"settle_explore@relay:ok":2,"train@relay:ok":10}) |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; 11 garbage and 7 bad-plaintext transits sent and due, each settled as bad-seal (codes {"bad_plaintext:5":7,"garbage:2":11}) |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 8961 txs, read 0.26 s, verify 0.42 s — E6 wall time)
- tamper: –/– classes FAIL with their codes (– built from the run; – s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 332489 | 332489 | 332489 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 826 | 0 | 4655 | 4655 | 338910 | 345000 | no | 780 |
| PostAnchorMulti | 423 | 0 | 345835 | 380319 | 380501 | 400000 | no | 1177 |
| PostSeed | 1920 | 0 | 337043 | 339494 | 339551 | 345000 | no | 781 |
| PostBeacon | 1568 | 438 | 330537 | 332716 | 332716 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15052 | 15052 | 15052 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 141402 | 148405 | 148405 | 220000 | no | 400 |
| FoldOccupancy | 308 | 1 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 155 | 14 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 128 | 0 | 14210 | 16114 | 16126 | 17000 | no | 575 |
| SettleTicket | 134 | 32 | 20177 | 21670 | 21674 | 40000 | no | 562 |
| Harvest | 337 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 166 | 31 | 11745 | 15948 | 15948 | 22000 | no | 428 |
| Train | 138 | 24 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 105 | 2 | 16135 | 18104 | 18114 | 25000 | no | 466 |
| Explore | 105 | 1 | 14033 | 16275 | 16334 | 20000 | no | 471 |
| SettleExplore | 105 | 6 | 8275 | 8431 | 8502 | 15000 | no | 396 |
| Depart | 105 | 3 | 16628 | 22968 | 22975 | 24500 | no | 712 |
| Reveal | 94 | 187 | 19014 | 22538 | 22538 | 26000 | no | 895 |
| SettleDeparture | 105 | 26 | 6272 | 6322 | 6322 | 48000 | no | 331 |
| SettleTransit | 105 | 34 | 60372 | 62942 | 63054 | 85000 | no | 859 |
| SweepPoolOwed | 101 | 34 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 168 | 0 | 18366 | 37996 | 38029 | 49000 | no | 1197 |
| ResolveFromInputs | 84 | 0 | 33081 | 43151 | 43151 | 290000 | no | 465 |
| SkipQuiet | 632 | 163 | 12728 | 48775 | 63308 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalSlot | 94 | 8 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 94, p50 19014, p90 21373, p99 22538, max 22538.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 1552 | 1 | 232 | 232 | 2 | 15 | 1863 | 5 s |
| s_to_first_cache_slots | 1536 | 1 | 223 | 223 | 2 | 9 | 1785 | 5 s |
| anchor_to_last_reveal_slots | 84 | 0 | 62 | 62 | 4 | 0 | 496 | 30 s |
| s_to_resolve_slots | 84 | 2 | 18 | 18 | 8 | 17 | 145 | 60 s |
| close_to_resolve_slots | 84 | 10 | 26 | 26 | reported | 80 | 208 | reported |

Round → anchor from the publication instant, in slots: p50 1.88, p99 232.88, max 232.88.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 10 | 4 | 4 | 4 | 0 |
| churned | 26 | 14 | 61 | 61 | 16 |
| active (GATHER/CLASH, reported) | 1 | 4 | 4 | 4 | 0 |
| resident (resident action or nudge, reported) | 0 | – | – | – | 0 |
| all | 37 | 5 | 61 | 61 | – |

Resident actions 321 (landed or refused), keeper-served nudges 422, season-end flush SkipQuiet 0 (not counted).

**ClashInputs:** 0 closed, 84 open: 84 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":1552,"ANNOUNCE":1,"BEACON":1568,"BUILD":166,"CAMP":17,"CLASH":84,"CLOSE":94,"DEPART":105,"DEPARTURE_SETTLED":105,"DIVERT":188,"EXPLORE":105,"EXPLORE_RESULT":105,"FOLD":308,"GATHER":168,"GENESIS_SEED":1,"HARVEST":337,"HOLDING_FINAL":33,"JOIN":155,"MUSTER":105,"POOL_SWEEP":101,"PROVINCE_OPEN":37,"REVEAL":94,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":1648,"SETTLE":134,"SKIP":632,"TICKET":128,"TRAIN":138,"TRANSIT_SETTLED":105}
- transits: {"outcome 1 seal 0":39,"outcome 3 seal 0":30,"outcome 4 seal 0":14,"outcome 6 seal 0":4,"outcome 8 seal 2":11,"outcome 8 seal 5":7}
- departs 105 (due 105), unsettled due: 0
- bad-seal codes: {"2":11,"5":7}
- failed transactions: {"Build: Insufficient":6,"Build: QueueFull":25,"CloseArrivalSlot: BadAccount":8,"Depart: HostBusy":1,"Depart: TipTooLow":2,"Explore: HostBusy":1,"FoldOccupancy: FoldStale":1,"Join: AlreadyDone":14,"Muster: Insufficient":1,"Muster: ProvinceFull":1,"PostBeacon: AlreadyDone":438,"Reveal: AlreadyDone":167,"Reveal: BadAddress":11,"Reveal: CommitMismatch":1,"Reveal: WindowClosed":8,"SettleDeparture: AlreadyDone":26,"SettleExplore: AlreadyDone":6,"SettleTicket: NoTicket":32,"SettleTransit: TransitState":34,"SkipQuiet: OutOfOrder":163,"SweepPoolOwed: AlreadyDone":34,"Train: Insufficient":24}

### Failed transactions by class (reported, not gating)

1004 failed: {"expected":217,"redundancy":719,"unclassified":11,"waste":57} (by cause {"a/b race":227,"adversary":11,"bot-policy":57,"bounded duplicate":163,"duplicate":492,"persona":10,"race":33}); unclassified 11.

| kind | error | n | class | cause |
|---|---|---|---|---|
| PostBeacon | AlreadyDone | 438 | redundancy | duplicate |
| Reveal | AlreadyDone | 167 | redundancy | a/b race |
| SkipQuiet | OutOfOrder | 163 | expected | bounded duplicate |
| SettleTransit | TransitState | 34 | redundancy | a/b race |
| SweepPoolOwed | AlreadyDone | 34 | redundancy | duplicate |
| SettleTicket | NoTicket | 32 | expected | race |
| SettleDeparture | AlreadyDone | 26 | redundancy | a/b race |
| Build | QueueFull | 25 | waste | bot-policy |
| Train | Insufficient | 24 | waste | bot-policy |
| Join | AlreadyDone | 14 | redundancy | duplicate |
| Reveal | BadAddress | 11 | expected | adversary |
| CloseArrivalSlot | BadAccount | 8 | unclassified |  |
| Reveal | WindowClosed | 8 | expected | persona |
| Build | Insufficient | 6 | waste | bot-policy |
| SettleExplore | AlreadyDone | 6 | redundancy | duplicate |
| Depart | TipTooLow | 2 | expected | persona |
| Depart | HostBusy | 1 | unclassified |  |
| Explore | HostBusy | 1 | unclassified |  |
| FoldOccupancy | FoldStale | 1 | expected | race |
| Muster | Insufficient | 1 | waste | bot-policy |
| Muster | ProvinceFull | 1 | waste | bot-policy |
| Reveal | CommitMismatch | 1 | unclassified |  |

- no landed transaction for ≥ 20 slots after a bell started: 2 windows (98 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
  - slots 1112–1252 (141 slots, 79 after bell 13 started; bells 12–14): **unexplained**
  - slots 1420–1552 (133 slots, 79 after bell 17 started; bells 16–18): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":4}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 99 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":220,"anchor_latency_slots_p99":232,"archived_bells":0,"bell":98,"contested_bells":18,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":18607869264,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52492337223,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":223,"spend_by_day":{"0":181347747},"sweeps_sent":101,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 99 bells (0 timeouts); last answered status {"alerts":3,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":98,"contested_bells":2,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999251186,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497922121,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":704733},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 65, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785631512: 1 keys, 1000 milli, 1725 slots, above keeper cap true
- hold slots-below at game 1785636912: 25 keys, 1900 milli, 188 slots, above keeper cap false
- hold slots-above at game 1785638312: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold defence-pool at game 1785639912: 1 keys, 1000 milli, 225 slots, above keeper cap true
- hold slots-below at game 1785640512: 25 keys, 1900 milli, 188 slots, above keeper cap false
- hold anchor at game 1785642712: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold keeper-payers at game 1785647112: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold lag at game 1785651512: 2 keys, 1000 milli, 150 slots, above keeper cap true
- hold relay-payers at game 1785669112: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785674112: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 350–2074 | 1 | 0 | 8 | 2075 |
| slots-below | false | 1025–1212 | 25 | 1 | 0 | – |
| slots-above | true | 1200–1387 | 25 | 0 | 1 | 1388 |
| defence-pool | true | 1400–1624 | 1 | 0 | 9 | 1625 |
| slots-below | false | 1475–1662 | 25 | 2 | 2 | 1784 |
| anchor | true | 1750–1774 | 1 | 0 | 0 | – |
| keeper-payers | true | 2300–2374 | 20 | 0 | 4 | 2375 |
| lag | true | 2850–2999 | 2 | 0 | 21 | 3000 |
| relay-payers | true | 5050–5124 | 20 | 0 | 6 | 5125 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 84,
      "closed": 0,
      "open": 84,
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
      "max": 22538.0,
      "n": 94,
      "p50": 19014.0,
      "p90": 21373.0,
      "p99": 22538.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 38",
      "(-3,1) day 0: 34",
      "(-3,3) day 0: 37",
      "(-2,-1) day 0: 52",
      "(-2,0) day 0: 8",
      "(-1,3) day 0: 61",
      "(0,-3) day 0: 21",
      "(0,3) day 0: 56",
      "(1,-3) day 0: 14",
      "(1,2) day 0: 46",
      "(2,-3) day 0: 28",
      "(2,1) day 0: 27",
      "(3,-3) day 0: 26",
      "(3,-2) day 0: 60",
      "(3,-1) day 0: 14",
      "(3,0) day 0: 19"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states), no GATHER or CLASH of the province that day (§13.4 A1), and no resident action (Muster/Dissolve/Garrison/Explore/Depart naming it, landed or refused) or keeper-served nudge of it within the day or the 26 bells before (A1 as amended, v1.13); active = roster unchanged with a GATHER/CLASH (reported); resident = with a resident action or nudge (reported: each nudge is one split); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day; the SkipQuiet reaching end_bell (the season-end flush) is not counted",
    "idle_province_days_over_6": [],
    "nudges_served": 422,
    "province_days_not_judged": 0,
    "province_post_states": 1859,
    "resident_actions": 321,
    "resident_province_days_over_6": [],
    "season_end_flush": 0,
    "skip_txs_per_active_province_day": {
      "max": 4.0,
      "n": 1,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 4.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 61.0,
      "n": 26,
      "p50": 14.0,
      "p90": 56.0,
      "p99": 61.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 4.0,
      "n": 10,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 4.0
    },
    "skip_txs_per_province_day": {
      "max": 61.0,
      "n": 37,
      "p50": 5.0,
      "p90": 52.0,
      "p99": 61.0
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
      "max": 496.0,
      "n": 84,
      "p50": 0.0,
      "p90": 8.0,
      "p99": 496.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 62.0,
      "n": 84,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 62.0
    },
    "close_to_resolve_game_secs": {
      "max": 208.0,
      "n": 84,
      "p50": 80.0,
      "p90": 88.0,
      "p99": 208.0
    },
    "close_to_resolve_slots": {
      "max": 26.0,
      "n": 84,
      "p50": 10.0,
      "p90": 11.0,
      "p99": 26.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 232.875,
      "n": 1552,
      "p50": 1.875,
      "p90": 1.875,
      "p99": 232.875
    },
    "round_to_anchor_game_secs": {
      "max": 1863.0,
      "n": 1552,
      "p50": 15.0,
      "p90": 15.0,
      "p99": 1863.0
    },
    "round_to_anchor_slots": {
      "max": 232.0,
      "n": 1552,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 232.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 1785.0,
      "n": 1536,
      "p50": 9.0,
      "p90": 9.0,
      "p99": 1785.0
    },
    "s_to_first_cache_slots": {
      "max": 223.0,
      "n": 1536,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 223.0
    },
    "s_to_resolve_game_secs": {
      "max": 145.0,
      "n": 84,
      "p50": 17.0,
      "p90": 25.0,
      "p99": 145.0
    },
    "s_to_resolve_slots": {
      "max": 18.0,
      "n": 84,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 18.0
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
    "loads": []
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 11,
      "5": 7
    },
    "marchbook": {
      "codes": {
        "bad_plaintext:5": 7,
        "garbage:2": 11
      },
      "due": {
        "bad_plaintext": 7,
        "garbage": 11
      },
      "not_bad_seal": []
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
