# Stack run `w6a-real1`

- phase **complete**, beacon **archive**, scale 100×, 100 bots, play 144 bells + 26 drain; program `FSL2S85e9UUrQAFci8VzsPxpWmQYq7GNVVkAkmtx2mku` (`.so` sha256 `1b1968af5bedca0ebd86ccf5c287bc0107b56b7870928fcab6ce483d1dd506fc`, 874120 B)
- source: verify input (6729 transactions, last bell 169)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 10 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 44 max 44 (reported) |
| 4 | **pass** | 0 unrevealed inside 7 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes []; 6729 txs, read 0.93 s, verify 3.91 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (26 built from the run; 28.48 s)
- `.so` pin: expected sha256 1b1968af5bedca0ebd86ccf5c287bc0107b56b7870928fcab6ce483d1dd506fc (the release build record; V2 checks the deployed program against it)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 331468 | 331468 | 331468 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 16 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 507 | 0 | 377603 | 380361 | 380486 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337171 | 339325 | 339620 | 345000 | no | 781 |
| PostBeacon | 2048 | 0 | 330472 | 332762 | 332937 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 142531 | 146254 | 146254 | 220000 | no | 400 |
| FoldOccupancy | 522 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 64 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 63 | 0 | 14207 | 16061 | 16061 | 17000 | no | 575 |
| SettleTicket | 63 | 63 | 20167 | 21593 | 21593 | 40000 | no | 562 |
| Harvest | 44 | 0 | 10140 | 14426 | 14426 | 17500 | no | 427 |
| Build | 42 | 0 | 11598 | 15948 | 15948 | 22000 | no | 428 |
| Train | 46 | 1 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 17 | 2 | 16114 | 18048 | 18048 | 25000 | no | 466 |
| Explore | 5 | 1 | 12265 | 16386 | 16386 | 20000 | no | 471 |
| SettleExplore | 5 | 0 | 8346 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 10 | 0 | 16558 | 18447 | 18447 | 24500 | no | 712 |
| Reveal | 10 | 6 | 20222 | 21868 | 21868 | 26000 | no | 862 |
| SettleDeparture | 10 | 10 | 6272 | 6282 | 6282 | 48000 | no | 331 |
| SettleTransit | 10 | 10 | 62654 | 63047 | 63047 | 85000 | no | 827 |
| SweepPoolOwed | 10 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 20 | 0 | 14259 | 37093 | 37093 | 49000 | no | 1197 |
| ResolveFromInputs | 10 | 0 | 29807 | 36048 | 36048 | 340000 | no | 465 |
| SkipQuiet | 358 | 0 | 42270 | 53873 | 56409 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 7 | 0 | 5920 | 5920 | 5920 | 8000 | no | 371 |
| CloseArrivalSlot | 10 | 0 | 6461 | 6461 | 6461 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 10, p50 20222, p90 21650, p99 21868, max 21868.

## Keeper latencies (a slot is 40 game s)

Round → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 2 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 99 | 99 | 5 s |
| anchor_to_last_reveal_slots | 10 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| close_to_resolve_slots | 10 | 6 | 6 | 6 | 8 | 240 | 240 | 60 s |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 2.98.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 39 | 1 | 6 | 6 | 0 |
| churned | 26 | 6 | 44 | 44 | 9 |
| all | 65 | 6 | 44 | 44 | – |

**ClashInputs:** 0 closed, 10 open: 10 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2048,"BUILD":42,"CAMP":11,"CLASH":10,"CLOSE":17,"DEPART":10,"DEPARTURE_SETTLED":10,"DIVERT":20,"EXPLORE":5,"EXPLORE_RESULT":5,"FOLD":522,"GATHER":20,"GENESIS_SEED":1,"HARVEST":44,"HOLDING_FINAL":6,"JOIN":64,"MUSTER":17,"POOL_SWEEP":10,"PROVINCE_OPEN":37,"REVEAL":10,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":63,"SKIP":358,"TICKET":63,"TRAIN":46,"TRANSIT_SETTLED":10}
- transits: {"outcome 1 seal 0":3,"outcome 3 seal 0":5,"outcome 4 seal 0":2}
- departs 10 (due 10), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"Explore: NotResident":1,"Muster: NotResident":2,"Reveal: BadAddress":6,"SettleDeparture: AlreadyDone":10,"SettleTicket: NoTicket":63,"SettleTransit: TransitState":10,"Train: Insufficient":1}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":4,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":31,"floor":500000000,"lamports":17864868950,"n":32},"funders":{"lamports":2038814231031,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52496437970,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":181710959,"1":212675751}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 10, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1789088280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold-skipped slots-below at game 1789094880: – keys, – milli, – slots, above keeper cap –
- hold slots-above at game 1789103280: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold anchor at game 1789109080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1789118280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped lag at game 1789131480: – keys, – milli, – slots, above keeper cap –
- hold frontier-fund at game 1789145880: 2 keys, 1000 milli, 15 slots, above keeper cap true
- hold defence-pool at game 1789155080: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold relay-payers at game 1789164280: 20 keys, 3000 milli, 15 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 159–503 | 1 | 0 | 5 | 504 |
| slots-above | true | 534–571 | 25 | 0 | 0 | – |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| frontier-fund | true | 1599–1613 | 2 | 0 | 18 | 1614 |
| defence-pool | true | 1829–1873 | 1 | 0 | 0 | – |
| relay-payers | true | 2059–2073 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 10,
      "closed": 0,
      "open": 10,
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
      "max": 21868.0,
      "n": 10,
      "p50": 20222.0,
      "p90": 21650.0,
      "p99": 21868.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,0) day 0: 12",
      "(-1,-2) day 0: 17",
      "(-1,3) day 0: 24",
      "(0,-3) day 0: 44",
      "(0,-2) day 0: 9",
      "(0,2) day 0: 9",
      "(1,-3) day 0: 9",
      "(1,2) day 0: 29",
      "(2,1) day 0: 9"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 732,
    "skip_txs_per_churned_province_day": {
      "max": 44.0,
      "n": 26,
      "p50": 6.0,
      "p90": 24.0,
      "p99": 44.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 39,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 44.0,
      "n": 65,
      "p50": 6.0,
      "p90": 9.0,
      "p99": 44.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 10,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 10,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 240.0,
      "n": 10,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 6.0,
      "n": 10,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "definition": "round -> anchor and S -> first cache in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W (W5-B F5, pinned by W6-A)",
    "round_to_anchor_from_publication_slots": {
      "max": 2.975,
      "n": 2704,
      "p50": 2.975,
      "p90": 2.975,
      "p99": 2.975
    },
    "round_to_anchor_game_secs": {
      "max": 119.0,
      "n": 2704,
      "p50": 119.0,
      "p90": 119.0,
      "p99": 119.0
    },
    "round_to_anchor_slots": {
      "max": 2.0,
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
    "slot_game_secs": 40.0,
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
