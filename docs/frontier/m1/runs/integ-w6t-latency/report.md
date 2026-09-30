# Stack run `integ-w6t-latency`

- phase **complete**, beacon **test-key**, scale 2×, 300 bots, play 36 bells + 26 drain; program `BPMWSqvY72vboB3CyMay2uRE8CRSjSv4Pr968hkYRk3E` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (3316 transactions, last bell 61)

- **NOT exit-grade**: ["no adversary schedule","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 4.61, p99 74.29, max 74.29 at bell 0; max per game day [74.29]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 17 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **pass** | round_to_anchor_game_secs p99 1.00 ≤ 5 s; s_to_first_cache_game_secs p99 1.00 ≤ 5 s; anchor_to_last_reveal_game_secs p99 1.00 ≤ 30 s; s_to_resolve_game_secs p99 3.00 ≤ 60 s; close_to_resolve_game_secs p99 65.00 reported (the resolve waits for S(b, r), public seed_margin after the close); SkipQuiet per idle province-day p99 2 max 2 (0 idle days over 6), per churned day p99 17 max 17 (reported) |
| 4 | **pass** | 0 unrevealed inside 0 above-cap hold windows (expected); unrevealed by rule: bounced 1 |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["no adversary schedule","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes []; 3316 txs, read 0.11 s, verify 0.21 s — E6 wall time)
- tamper: –/– classes FAIL with their codes (– built from the run; – s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 332746 | 332746 | 332746 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchorMulti | 183 | 0 | 377737 | 380337 | 380342 | 400000 | no | 1177 |
| PostSeed | 960 | 0 | 337405 | 339747 | 339747 | 345000 | no | 781 |
| PostBeacon | 992 | 0 | 330696 | 333275 | 333275 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15052 | 15052 | 15052 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 140956 | 147258 | 147258 | 220000 | no | 400 |
| FoldOccupancy | 186 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 86 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 81 | 0 | 14223 | 16101 | 16101 | 17000 | no | 575 |
| SettleTicket | 82 | 0 | 20191 | 21687 | 21687 | 40000 | no | 562 |
| Harvest | 106 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 56 | 0 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 54 | 0 | 12220 | 12402 | 12402 | 17500 | no | 432 |
| Muster | 40 | 0 | 16060 | 18856 | 18856 | 25000 | no | 466 |
| Explore | 41 | 0 | 14030 | 16313 | 16313 | 20000 | no | 471 |
| SettleExplore | 41 | 0 | 8275 | 8431 | 8431 | 15000 | no | 396 |
| Depart | 24 | 1 | 16557 | 20678 | 20678 | 24500 | no | 712 |
| Reveal | 19 | 5 | 18247 | 21341 | 21341 | 26000 | no | 862 |
| SettleDeparture | 24 | 0 | 6252 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 24 | 0 | 58991 | 62886 | 62886 | 85000 | no | 827 |
| SweepPoolOwed | 23 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 34 | 0 | 18265 | 37934 | 37934 | 49000 | no | 1197 |
| ResolveFromInputs | 17 | 0 | 31427 | 36392 | 36392 | 290000 | no | 465 |
| SkipQuiet | 159 | 8 | 20411 | 49109 | 50759 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalSlot | 19 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 19, p50 18247, p90 20416, p99 21341, max 21341.

## Keeper latencies (a slot is 0.80 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 576 | 1 | 1 | 1 | 2 | 1 | 1 | 5 s |
| s_to_first_cache_slots | 560 | 1 | 1 | 1 | 2 | 1 | 1 | 5 s |
| anchor_to_last_reveal_slots | 17 | 0 | 1.25 | 1.25 | 4 | 0 | 1 | 30 s |
| s_to_resolve_slots | 17 | 2 | 3 | 3 | 8 | 2 | 3 | 60 s |
| close_to_resolve_slots | 17 | 80 | 81.25 | 81.25 | reported | 64 | 65 | reported |

Round → anchor from the publication instant, in slots: p50 1.25, p99 1.25, max 1.25.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 18 | 2 | 2 | 2 | 0 |
| churned | 19 | 3 | 17 | 17 | 7 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| all | 37 | 2 | 17 | 17 | – |

**ClashInputs:** 0 closed, 17 open: 17 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":976,"ANNOUNCE":1,"BEACON":992,"BUILD":56,"CAMP":6,"CLASH":17,"CLOSE":19,"DEPART":24,"DEPARTURE_SETTLED":24,"DIVERT":38,"EXPLORE":41,"EXPLORE_RESULT":41,"FOLD":186,"GATHER":34,"GENESIS_SEED":1,"HARVEST":106,"HOLDING_FINAL":13,"JOIN":86,"MUSTER":40,"POOL_SWEEP":23,"PROVINCE_OPEN":37,"REVEAL":19,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":960,"SETTLE":82,"SKIP":159,"TICKET":81,"TRAIN":54,"TRANSIT_SETTLED":24}
- transits: {"outcome 1 seal 0":10,"outcome 3 seal 0":2,"outcome 4 seal 0":2,"outcome 6 seal 0":1,"outcome 8 seal 2":6,"outcome 8 seal 5":3}
- departs 24 (due 24), unsettled due: 0
- bad-seal codes: {"2":6,"5":3}
- failed transactions: {"Depart: TipTooLow":1,"Reveal: AlreadyDone":4,"Reveal: SlotMoved":1,"SkipQuiet: OutOfOrder":8}

### Failed transactions by class (reported, not gating)

14 failed: {"expected":9,"redundancy":4,"unclassified":1} (by cause {"a/b race":4,"bounded duplicate":8,"persona":1}); unclassified 1.

| kind | error | n | class | cause |
|---|---|---|---|---|
| SkipQuiet | OutOfOrder | 8 | expected | bounded duplicate |
| Reveal | AlreadyDone | 4 | redundancy | a/b race |
| Depart | TipTooLow | 1 | expected | persona |
| Reveal | SlotMoved | 1 | unclassified |  |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (102 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":1}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 63 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":1,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":62,"contested_bells":1,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":21111310205,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52494767748,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":1,"spend_by_day":{"0":88560200},"sweeps_sent":23,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 63 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":62,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999840000,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52498802902,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":219178},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 65, alarms 0
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
      "closable_after_grace": 17,
      "closed": 0,
      "open": 17,
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
      "max": 21341.0,
      "n": 19,
      "p50": 18247.0,
      "p90": 20416.0,
      "p99": 21341.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 17",
      "(-3,1) day 0: 16",
      "(-1,3) day 0: 9",
      "(0,-3) day 0: 14",
      "(2,-3) day 0: 12",
      "(3,-2) day 0: 14",
      "(3,0) day 0: 8"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 729,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 17.0,
      "n": 19,
      "p50": 3.0,
      "p90": 16.0,
      "p99": 17.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 2.0,
      "n": 18,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "skip_txs_per_province_day": {
      "max": 17.0,
      "n": 37,
      "p50": 2.0,
      "p90": 14.0,
      "p99": 17.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 1.0,
      "n": 17,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 1.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 1.25,
      "n": 17,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 1.25
    },
    "close_to_resolve_game_secs": {
      "max": 65.0,
      "n": 17,
      "p50": 64.0,
      "p90": 65.0,
      "p99": 65.0
    },
    "close_to_resolve_slots": {
      "max": 81.25,
      "n": 17,
      "p50": 80.0,
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
      "max": 1.0,
      "n": 576,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 1.0,
      "n": 560,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_first_cache_slots": {
      "max": 1.0,
      "n": 560,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_resolve_game_secs": {
      "max": 3.0,
      "n": 17,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 3.0
    },
    "s_to_resolve_slots": {
      "max": 3.0,
      "n": 17,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 3.0
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
