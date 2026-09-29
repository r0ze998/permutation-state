# Stack run `w6-latency`

- phase **complete**, beacon **test-key**, scale 2×, 300 bots, play 36 bells + 26 drain; program `6tix8aNC3zvuseR5rMx4prjsQFBPVUmwWvk4GwLUKBNj` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (3477 transactions, last bell 61)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 23 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **pass** | round_to_anchor_game_secs p99 1.00 ≤ 5 s; s_to_first_cache_game_secs p99 2.00 ≤ 5 s; anchor_to_last_reveal_game_secs p99 0.00 ≤ 30 s; s_to_resolve_game_secs p99 3.00 ≤ 60 s; close_to_resolve_game_secs p99 65.00 reported (the resolve waits for S(b, r), public seed_margin after the close); SkipQuiet per idle province-day p99 2 max 2 (0 idle days over 6), per churned day p99 19 max 19 (reported) |
| 4 | **pass** | 0 unrevealed inside 0 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 3477 txs, read 0.12 s, verify 0.21 s — E6 wall time)
- tamper: –/– classes FAIL with their codes (– built from the run; – s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330736 | 330736 | 330736 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchorMulti | 183 | 0 | 377323 | 380893 | 380898 | 400000 | no | 1177 |
| PostSeed | 960 | 0 | 337060 | 339232 | 339232 | 345000 | no | 781 |
| PostBeacon | 992 | 0 | 330720 | 332895 | 332895 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15052 | 15052 | 15052 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 141673 | 145861 | 145861 | 220000 | no | 400 |
| FoldOccupancy | 186 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 86 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 81 | 0 | 14223 | 16114 | 16114 | 17000 | no | 575 |
| SettleTicket | 82 | 0 | 20191 | 21687 | 21687 | 40000 | no | 562 |
| Harvest | 117 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 57 | 0 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 54 | 0 | 12220 | 12402 | 12402 | 17500 | no | 432 |
| Muster | 40 | 0 | 16162 | 18055 | 18055 | 25000 | no | 466 |
| Explore | 54 | 0 | 14035 | 16317 | 16317 | 20000 | no | 471 |
| SettleExplore | 54 | 0 | 8275 | 8431 | 8431 | 15000 | no | 396 |
| Depart | 26 | 1 | 16558 | 20708 | 20708 | 24500 | no | 712 |
| Reveal | 23 | 6 | 20148 | 22149 | 22149 | 26000 | no | 895 |
| SettleDeparture | 26 | 26 | 6252 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 26 | 26 | 59358 | 62870 | 62870 | 85000 | no | 827 |
| SweepPoolOwed | 26 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 46 | 0 | 16315 | 37083 | 37083 | 49000 | no | 1197 |
| ResolveFromInputs | 23 | 0 | 31398 | 37212 | 37212 | 290000 | no | 465 |
| SkipQuiet | 193 | 9 | 16389 | 44775 | 44805 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalSlot | 23 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 23, p50 20148, p90 21410, p99 22149, max 22149.

## Keeper latencies (a slot is 0.80 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 576 | 2 | 2 | 2 | 2 | 1 | 1 | 5 s |
| s_to_first_cache_slots | 560 | 2 | 2 | 2 | 2 | 2 | 2 | 5 s |
| anchor_to_last_reveal_slots | 23 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 23 | 4 | 4 | 4 | 8 | 3 | 3 | 60 s |
| close_to_resolve_slots | 23 | 81.25 | 81.25 | 81.25 | reported | 65 | 65 | reported |

Round → anchor from the publication instant, in slots: p50 1.25, p99 1.25, max 1.25.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 18 | 2 | 2 | 2 | 0 |
| churned | 19 | 3 | 19 | 19 | 9 |
| all | 37 | 2 | 19 | 19 | – |

**ClashInputs:** 0 closed, 23 open: 23 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":976,"ANNOUNCE":1,"BEACON":992,"BUILD":57,"CAMP":5,"CLASH":23,"CLOSE":23,"DEPART":26,"DEPARTURE_SETTLED":26,"DIVERT":43,"EXPLORE":54,"EXPLORE_RESULT":54,"FOLD":186,"GATHER":46,"GENESIS_SEED":1,"HARVEST":117,"HOLDING_FINAL":13,"JOIN":86,"MUSTER":40,"POOL_SWEEP":26,"PROVINCE_OPEN":37,"REVEAL":23,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":960,"SETTLE":82,"SKIP":193,"TICKET":81,"TRAIN":54,"TRANSIT_SETTLED":26}
- transits: {"outcome 1 seal 0":8,"outcome 3 seal 0":8,"outcome 4 seal 0":1,"outcome 8 seal 2":6,"outcome 8 seal 5":3}
- departs 26 (due 26), unsettled due: 0
- bad-seal codes: {"2":6,"5":3}
- failed transactions: {"Depart: TipTooLow":1,"Reveal: AlreadyDone":3,"Reveal: BadAddress":2,"Reveal: WindowClosed":1,"SettleDeparture: AlreadyDone":26,"SettleTransit: TransitState":26,"SkipQuiet: OutOfOrder":9}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":1,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":62,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":21068817436,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52489171513,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":89500127}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 668, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 1, restarts 1, crashes 0

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 23,
      "closed": 0,
      "open": 23,
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
      "max": 22149.0,
      "n": 23,
      "p50": 20148.0,
      "p90": 21410.0,
      "p99": 22149.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,0) day 0: 18",
      "(-3,1) day 0: 14",
      "(-3,3) day 0: 14",
      "(-1,3) day 0: 17",
      "(0,-3) day 0: 17",
      "(0,3) day 0: 8",
      "(2,-3) day 0: 10",
      "(3,-2) day 0: 18",
      "(3,0) day 0: 19"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 790,
    "skip_txs_per_churned_province_day": {
      "max": 19.0,
      "n": 19,
      "p50": 3.0,
      "p90": 18.0,
      "p99": 19.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 2.0,
      "n": 18,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "skip_txs_per_province_day": {
      "max": 19.0,
      "n": 37,
      "p50": 2.0,
      "p90": 17.0,
      "p99": 19.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 23,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 23,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 65.0,
      "n": 23,
      "p50": 65.0,
      "p90": 65.0,
      "p99": 65.0
    },
    "close_to_resolve_slots": {
      "max": 81.25,
      "n": 23,
      "p50": 81.25,
      "p90": 81.25,
      "p99": 81.25
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 1.25,
      "n": 576,
      "p50": 1.25,
      "p90": 1.25,
      "p99": 1.25
    },
    "round_to_anchor_game_secs": {
      "max": 1.0,
      "n": 576,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "round_to_anchor_slots": {
      "max": 2.0,
      "n": 576,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 2.0,
      "n": 560,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "s_to_first_cache_slots": {
      "max": 2.0,
      "n": 560,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "s_to_resolve_game_secs": {
      "max": 3.0,
      "n": 23,
      "p50": 3.0,
      "p90": 3.0,
      "p99": 3.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 23,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 4.0
    },
    "slot_game_secs": 0.8,
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
      "2": 6,
      "5": 3
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
