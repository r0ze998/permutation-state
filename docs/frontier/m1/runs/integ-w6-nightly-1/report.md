# Stack run `nightly-20260928`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `poxXBscv9uNi3SMot8eANobZXLUw2VNKAo448ZScwn3` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (7858 transactions, last bell 169)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 53 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 7 max 7 (1 idle days over 6), per churned day p99 125 max 125 (reported) |
| 4 | **pass** | 0 unrevealed inside 5 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 7858 txs, read 0.25 s, verify 0.52 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (28 built from the run; 3.39 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 34.82 ms, error rate 0, ingest → WS p99 0.23 s (ws-stamp; fold lag p99 0.80 s) → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 16 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 507 | 0 | 377393 | 380288 | 380714 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337247 | 339646 | 339691 | 345000 | no | 781 |
| PostBeacon | 1920 | 0 | 330473 | 332976 | 332998 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 139838 | 144187 | 144187 | 220000 | no | 400 |
| FoldOccupancy | 510 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 96 | 0 | 14211 | 16090 | 16090 | 17000 | no | 575 |
| SettleTicket | 96 | 9 | 20168 | 21655 | 21655 | 40000 | no | 562 |
| Harvest | 179 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 98 | 1 | 11742 | 15948 | 15948 | 22000 | no | 428 |
| Train | 83 | 0 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 42 | 1 | 16049 | 18047 | 18047 | 25000 | no | 466 |
| Explore | 35 | 1 | 14027 | 16307 | 16307 | 20000 | no | 471 |
| SettleExplore | 35 | 0 | 8275 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 59 | 1 | 18271 | 22951 | 22951 | 24500 | no | 712 |
| Reveal | 53 | 38 | 19144 | 21913 | 21913 | 26000 | no | 862 |
| SettleDeparture | 59 | 59 | 6252 | 6302 | 6302 | 48000 | no | 331 |
| SettleTransit | 59 | 67 | 61751 | 63070 | 63070 | 85000 | no | 859 |
| SweepPoolOwed | 57 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 106 | 0 | 14259 | 37093 | 37148 | 49000 | no | 1197 |
| ResolveFromInputs | 53 | 0 | 29866 | 40119 | 40119 | 290000 | no | 465 |
| SkipQuiet | 702 | 4 | 15168 | 53978 | 56728 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 20 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 53 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 53, p50 19144, p90 21544, p99 21913, max 21913.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 2 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 98 | 99 | 5 s |
| anchor_to_last_reveal_slots | 53 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 53 | 4 | 4 | 4 | 8 | 178 | 179 | 60 s |
| close_to_resolve_slots | 53 | 6 | 6 | 6 | reported | 240 | 240 | reported |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 2.98.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 30 | 1 | 7 | 7 | 1 |
| churned | 27 | 8 | 125 | 125 | 19 |
| all | 57 | 6 | 125 | 125 | – |

**ClashInputs:** 0 closed, 53 open: 53 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":1920,"BUILD":98,"CAMP":23,"CLASH":53,"CLOSE":73,"DEPART":59,"DEPARTURE_SETTLED":59,"DIVERT":112,"EXPLORE":35,"EXPLORE_RESULT":35,"FOLD":510,"GATHER":106,"GENESIS_SEED":1,"HARVEST":179,"HOLDING_FINAL":14,"JOIN":100,"MUSTER":42,"POOL_SWEEP":57,"PROVINCE_OPEN":37,"REVEAL":53,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":96,"SKIP":702,"TICKET":96,"TRAIN":83,"TRANSIT_SETTLED":59}
- transits: {"outcome 1 seal 0":24,"outcome 3 seal 0":17,"outcome 4 seal 0":12,"outcome 6 seal 0":2,"outcome 8 seal 5":4}
- departs 59 (due 59), unsettled due: 0
- bad-seal codes: {"5":4}
- failed transactions: {"Build: Insufficient":1,"Depart: TipTooLow":1,"Explore: NotResident":1,"Muster: NotResident":1,"Reveal: AlreadyDone":28,"Reveal: BadAddress":5,"Reveal: WindowClosed":5,"SettleDeparture: AlreadyDone":59,"SettleTicket: NoTicket":9,"SettleTransit: TransitState":67,"SkipQuiet: OutOfOrder":4}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":3,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20019291021,"n":32},"funders":{"lamports":2035950446434,"n":4},"reveal":{"effective_n":150,"floor":214942572,"lamports":52522660630,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":185270930,"1":217223771}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 8, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold-skipped slots-below at game 1785638880: – keys, – milli, – slots, above keeper cap –
- hold slots-above at game 1785643880: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold anchor at game 1785653080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped lag at game 1785675480: – keys, – milli, – slots, above keeper cap –
- hold relay-payers at game 1785708320: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785718080: – keys, – milli, – slots, above keeper cap –
- hold-skipped defence-pool at game 1785718080: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 159–503 | 1 | 0 | 6 | 504 |
| slots-above | true | 449–486 | 25 | 0 | 1 | 487 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 0 | – |
| relay-payers | true | 2060–2074 | 20 | 0 | 0 | – |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 53,
      "closed": 0,
      "open": 53,
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
      "max": 21913.0,
      "n": 53,
      "p50": 19144.0,
      "p90": 21544.0,
      "p99": 21913.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,0) day 0: 16",
      "(-3,1) day 0: 7",
      "(-3,2) day 0: 33",
      "(-3,3) day 0: 124",
      "(-2,-1) day 0: 7",
      "(-2,1) day 0: 8",
      "(-2,2) day 0: 9",
      "(-2,3) day 0: 27",
      "(-1,-2) day 0: 24",
      "(-1,2) day 0: 7",
      "(-1,3) day 0: 21",
      "(0,-3) day 0: 8",
      "(0,-2) day 0: 7",
      "(0,3) day 0: 45",
      "(1,-3) day 0: 26",
      "(1,-2) day 0: 10",
      "(2,-3) day 0: 125",
      "(2,1) day 0: 38",
      "(3,-3) day 0: 31"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [
      "(0,2) day 0: 7"
    ],
    "province_days_not_judged": 0,
    "province_post_states": 1490,
    "skip_txs_per_churned_province_day": {
      "max": 125.0,
      "n": 27,
      "p50": 8.0,
      "p90": 45.0,
      "p99": 125.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 7.0,
      "n": 30,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 7.0
    },
    "skip_txs_per_province_day": {
      "max": 125.0,
      "n": 57,
      "p50": 6.0,
      "p90": 31.0,
      "p99": 125.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 53,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 53,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 240.0,
      "n": 53,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 6.0,
      "n": 53,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
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
      "p50": 98.0,
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
    "s_to_resolve_game_secs": {
      "max": 179.0,
      "n": 53,
      "p50": 178.0,
      "p90": 179.0,
      "p99": 179.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 53,
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
    "loads": [
      {
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "ingest_lag_p99_s": 0.8,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.229376,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 370,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 34.816,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "requests": 28864.0,
        "summary": "p99 file 34.8 ms, ingest->WS p99 0.23 s (ws-stamp), error rate 0.00000, gaps 0, 404 370",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 739422.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "5": 4
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
