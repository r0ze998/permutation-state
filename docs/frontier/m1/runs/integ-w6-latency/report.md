# Stack run `w6-latency`

- phase **complete**, beacon **test-key**, scale 2×, 300 bots, play 36 bells + 26 drain; program `6tix8aNC3zvuseR5rMx4prjsQFBPVUmwWvk4GwLUKBNj` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: chain (no verify input) (3467 transactions, last bell 61)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 23 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **pass** | round_to_anchor_game_secs p99 2.00 ≤ 5 s; s_to_first_cache_game_secs p99 2.00 ≤ 5 s; anchor_to_last_reveal_game_secs p99 0.00 ≤ 30 s; s_to_resolve_game_secs p99 3.00 ≤ 60 s; close_to_resolve_game_secs p99 65.00 reported (the resolve waits for S(b, r), public seed_margin after the close); SkipQuiet per idle province-day p99 3 max 3 (0 idle days over 6), per churned day p99 18 max 18 (reported) |
| 4 | **n.a.** | no verify report: ValidSealUnrevealed not read |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **n.a.** | no verify report |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **–** (fail codes –; – txs, read – s, verify – s — E6 wall time)
- tamper: –/– classes FAIL with their codes (– built from the run; – s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 332746 | 332746 | 332746 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchorMulti | 183 | 0 | 377737 | 380337 | 380342 | 400000 | no | 1177 |
| PostSeed | 960 | 0 | 337346 | 339747 | 339747 | 345000 | no | 781 |
| PostBeacon | 992 | 0 | 330658 | 333275 | 333275 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15052 | 15052 | 15052 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 140956 | 147258 | 147258 | 220000 | no | 400 |
| FoldOccupancy | 186 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 86 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 81 | 0 | 14222 | 16114 | 16114 | 17000 | no | 575 |
| SettleTicket | 82 | 0 | 20191 | 21687 | 21687 | 40000 | no | 562 |
| Harvest | 119 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 57 | 0 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 54 | 0 | 12220 | 12402 | 12402 | 17500 | no | 432 |
| Muster | 40 | 0 | 16060 | 18074 | 18074 | 25000 | no | 466 |
| Explore | 52 | 0 | 14035 | 16354 | 16354 | 20000 | no | 471 |
| SettleExplore | 52 | 0 | 8275 | 8431 | 8431 | 15000 | no | 396 |
| Depart | 25 | 1 | 16591 | 20678 | 20678 | 24500 | no | 712 |
| Reveal | 23 | 6 | 20087 | 21911 | 21911 | 26000 | no | 862 |
| SettleDeparture | 25 | 25 | 6252 | 6292 | 6292 | 48000 | no | 331 |
| SettleTransit | 25 | 25 | 59964 | 62886 | 62886 | 85000 | no | 859 |
| SweepPoolOwed | 25 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 46 | 0 | 16318 | 37091 | 37091 | 49000 | no | 1197 |
| ResolveFromInputs | 23 | 0 | 31756 | 38211 | 38211 | 290000 | no | 465 |
| SkipQuiet | 192 | 8 | 16460 | 44759 | 44775 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalSlot | 23 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 23, p50 20087, p90 21408, p99 21911, max 21911.

## Keeper latencies (a slot is 0.80 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 576 | 2 | 2 | 2 | 2 | 2 | 2 | 5 s |
| s_to_first_cache_slots | 560 | 2 | 2 | 2 | 2 | 2 | 2 | 5 s |
| anchor_to_last_reveal_slots | 23 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 22 | 3 | 4 | 4 | 8 | 3 | 3 | 60 s |
| close_to_resolve_slots | 22 | 80 | 81.25 | 81.25 | reported | 64 | 65 | reported |

Round → anchor from the publication instant, in slots: p50 2.50, p99 2.50, max 2.50.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 19 | 2 | 3 | 3 | 0 |
| churned | 18 | 7 | 18 | 18 | 10 |
| all | 37 | 2 | 18 | 18 | – |

**ClashInputs:** 0 closed, 23 open: 23 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":976,"ANNOUNCE":1,"BEACON":992,"BUILD":57,"CAMP":6,"CLASH":23,"CLOSE":23,"DEPART":25,"DEPARTURE_SETTLED":25,"DIVERT":42,"EXPLORE":52,"EXPLORE_RESULT":52,"FOLD":186,"GATHER":46,"GENESIS_SEED":1,"HARVEST":119,"HOLDING_FINAL":13,"JOIN":86,"MUSTER":40,"POOL_SWEEP":25,"PROVINCE_OPEN":37,"REVEAL":23,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":960,"SETTLE":82,"SKIP":192,"TICKET":81,"TRAIN":54,"TRANSIT_SETTLED":25}
- transits: {"outcome 1 seal 0":9,"outcome 3 seal 0":6,"outcome 4 seal 0":2,"outcome 8 seal 2":6,"outcome 8 seal 5":2}
- departs 25 (due 25), unsettled due: 0
- bad-seal codes: {"2":6,"5":2}
- failed transactions: {"Depart: TipTooLow":1,"Reveal: AlreadyDone":3,"Reveal: BadAddress":2,"Reveal: WindowClosed":1,"SettleDeparture: AlreadyDone":25,"SettleTransit: TransitState":25,"SkipQuiet: OutOfOrder":8}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":1,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":62,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":21066943494,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52489309433,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":89453850}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 739, alarms 0
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
      "max": 21911.0,
      "n": 23,
      "p50": 20087.0,
      "p90": 21408.0,
      "p99": 21911.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,0) day 0: 14",
      "(-3,1) day 0: 16",
      "(-3,3) day 0: 15",
      "(-1,3) day 0: 16",
      "(0,-3) day 0: 17",
      "(0,3) day 0: 9",
      "(2,-3) day 0: 12",
      "(2,1) day 0: 7",
      "(3,-2) day 0: 18",
      "(3,0) day 0: 12"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 785,
    "skip_txs_per_churned_province_day": {
      "max": 18.0,
      "n": 18,
      "p50": 7.0,
      "p90": 17.0,
      "p99": 18.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 3.0,
      "n": 19,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 3.0
    },
    "skip_txs_per_province_day": {
      "max": 18.0,
      "n": 37,
      "p50": 2.0,
      "p90": 16.0,
      "p99": 18.0
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
      "n": 22,
      "p50": 64.0,
      "p90": 65.0,
      "p99": 65.0
    },
    "close_to_resolve_slots": {
      "max": 81.25,
      "n": 22,
      "p50": 80.0,
      "p90": 81.25,
      "p99": 81.25
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 2.5,
      "n": 576,
      "p50": 2.5,
      "p90": 2.5,
      "p99": 2.5
    },
    "round_to_anchor_game_secs": {
      "max": 2.0,
      "n": 576,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
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
      "n": 22,
      "p50": 3.0,
      "p90": 3.0,
      "p99": 3.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 22,
      "p50": 3.0,
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
      "5": 2
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
