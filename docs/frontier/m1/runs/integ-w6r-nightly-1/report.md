# Stack run `nightly-20260929`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `4vckXPMyVUrXj5iLPnxC4fKqj6KDYoTjeUpL1LXNsySb` (`.so` sha256 `797675b41ad2349197ae9604a01d44e7a06f92782316a4f4b3c2ce3e6c5916f7`, 876272 B)
- source: verify input (7798 transactions, last bell 169)

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 57 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 125 max 125 (reported) |
| 4 | **pass** | 0 unrevealed inside 6 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 7798 txs, read 0.26 s, verify 0.54 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (29 built from the run; 3.89 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 55.30 ms, error rate 0, ingest → WS p99 0.23 s (ws-stamp; fold lag p99 2.40 s) → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330659 | 330659 | 330659 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 16 | 0 | 4655 | 4655 | 4655 | 345000 | no | 780 |
| PostAnchorMulti | 507 | 0 | 377398 | 380288 | 380714 | 400000 | no | 1177 |
| PostSeed | 2688 | 0 | 337222 | 339368 | 339691 | 345000 | no | 781 |
| PostBeacon | 1792 | 0 | 330575 | 332549 | 332568 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 139838 | 144187 | 144187 | 220000 | no | 400 |
| FoldOccupancy | 510 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 0 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 96 | 0 | 14211 | 16114 | 16114 | 17000 | no | 575 |
| SettleTicket | 96 | 8 | 20167 | 21670 | 21670 | 40000 | no | 562 |
| Harvest | 204 | 0 | 12161 | 14426 | 14427 | 17500 | no | 427 |
| Build | 103 | 0 | 11745 | 15948 | 15948 | 22000 | no | 428 |
| Train | 92 | 0 | 12220 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 50 | 2 | 16153 | 18058 | 18058 | 25000 | no | 466 |
| Explore | 42 | 0 | 14010 | 16393 | 16393 | 20000 | no | 471 |
| SettleExplore | 42 | 2 | 8275 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 63 | 1 | 18279 | 22949 | 22949 | 24500 | no | 712 |
| Reveal | 58 | 67 | 19219 | 22657 | 22657 | 26000 | no | 895 |
| SettleDeparture | 63 | 63 | 6252 | 6292 | 6292 | 48000 | no | 331 |
| SettleTransit | 63 | 69 | 62576 | 63228 | 63228 | 85000 | no | 859 |
| SweepPoolOwed | 63 | 4 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 114 | 0 | 15277 | 37081 | 37911 | 49000 | no | 1197 |
| ResolveFromInputs | 57 | 0 | 29661 | 36544 | 36544 | 290000 | no | 465 |
| SkipQuiet | 628 | 4 | 16536 | 55289 | 56617 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 22 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 58 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 58, p50 19219, p90 21684, p99 22657, max 22657.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 2 | 2 | 2 | 2 | 119 | 119 | 5 s |
| s_to_first_cache_slots | 2688 | 2 | 2 | 2 | 2 | 99 | 99 | 5 s |
| anchor_to_last_reveal_slots | 57 | 0 | 12 | 12 | 4 | 0 | 480 | 30 s |
| s_to_resolve_slots | 57 | 4 | 4 | 4 | 8 | 179 | 179 | 60 s |
| close_to_resolve_slots | 57 | 6 | 6 | 6 | reported | 240 | 240 | reported |

Round → anchor from the publication instant, in slots: p50 2.98, p99 2.98, max 2.98.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 25 | 1 | 6 | 6 | 0 |
| churned | 28 | 9 | 125 | 125 | 22 |
| all | 53 | 6 | 125 | 125 | – |

**ClashInputs:** 0 closed, 57 open: 57 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":1792,"BUILD":103,"CAMP":24,"CLASH":57,"CLOSE":80,"DEPART":63,"DEPARTURE_SETTLED":63,"DIVERT":114,"EXPLORE":42,"EXPLORE_RESULT":42,"FOLD":510,"GATHER":114,"GENESIS_SEED":1,"HARVEST":204,"HOLDING_FINAL":15,"JOIN":100,"MUSTER":50,"POOL_SWEEP":63,"PROVINCE_OPEN":37,"REVEAL":58,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2688,"SETTLE":96,"SKIP":628,"TICKET":96,"TRAIN":92,"TRANSIT_SETTLED":63}
- transits: {"outcome 1 seal 0":15,"outcome 3 seal 0":27,"outcome 4 seal 0":9,"outcome 8 seal 2":7,"outcome 8 seal 5":5}
- departs 63 (due 63), unsettled due: 0
- bad-seal codes: {"2":7,"5":5}
- failed transactions: {"Depart: TipTooLow":1,"Muster: NotResident":2,"Reveal: AlreadyDone":44,"Reveal: BadAddress":5,"Reveal: SlotMoved":13,"Reveal: TransitState":2,"Reveal: WindowClosed":3,"SettleDeparture: AlreadyDone":63,"SettleExplore: AlreadyDone":2,"SettleTicket: NoTicket":8,"SettleTransit: TransitState":69,"SkipQuiet: OutOfOrder":4,"SweepPoolOwed: AlreadyDone":4}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true), last status {"alerts":5,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":20251797306,"n":32},"funders":{"lamports":2035696468824,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52519361539,"n":150}},"provinces_opened":37,"seed_latency_slots_p99":1,"spend_by_day":{"0":179784888,"1":211696854}}
- keeper B: min reveal effective N in play 150
- herald: fold lag slots p99 18, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632280: 1 keys, 1000 milli, 345 slots, above keeper cap true
- hold-skipped slots-below at game 1785638880: – keys, – milli, – slots, above keeper cap –
- hold slots-above at game 1785643880: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold defence-pool at game 1785645480: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold anchor at game 1785653080: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped lag at game 1785675480: – keys, – milli, – slots, above keeper cap –
- hold relay-payers at game 1785708280: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785718080: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 159–503 | 1 | 0 | 6 | 504 |
| slots-above | true | 449–486 | 25 | 0 | 5 | 487 |
| defence-pool | true | 489–533 | 1 | 0 | 9 | 534 |
| anchor | true | 679–683 | 1 | 0 | 0 | – |
| keeper-payers | true | 909–923 | 20 | 0 | 1 | 929 |
| relay-payers | true | 2059–2073 | 20 | 0 | 1 | 2074 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 57,
      "closed": 0,
      "open": 57,
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
      "max": 22657.0,
      "n": 58,
      "p50": 19219.0,
      "p90": 21684.0,
      "p99": 22657.0
    }
  },
  "3_catch_up": {
    "churned_province_days_over_6": [
      "(-3,0) day 0: 15",
      "(-3,2) day 0: 10",
      "(-3,3) day 0: 34",
      "(-2,-1) day 0: 7",
      "(-2,2) day 0: 9",
      "(-2,3) day 0: 32",
      "(-1,-2) day 0: 24",
      "(-1,2) day 0: 7",
      "(-1,3) day 0: 29",
      "(0,-3) day 0: 8",
      "(0,-2) day 0: 7",
      "(0,2) day 0: 7",
      "(0,3) day 0: 47",
      "(1,-3) day 0: 26",
      "(1,-2) day 0: 9",
      "(2,-3) day 0: 28",
      "(2,-2) day 0: 7",
      "(2,0) day 0: 8",
      "(2,1) day 0: 125",
      "(3,-3) day 0: 34",
      "(3,-1) day 0: 9",
      "(3,0) day 0: 40"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 1462,
    "skip_txs_per_churned_province_day": {
      "max": 125.0,
      "n": 28,
      "p50": 9.0,
      "p90": 40.0,
      "p99": 125.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 25,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 125.0,
      "n": 53,
      "p50": 6.0,
      "p90": 32.0,
      "p99": 125.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 480.0,
      "n": 57,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 480.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 12.0,
      "n": 57,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 12.0
    },
    "close_to_resolve_game_secs": {
      "max": 240.0,
      "n": 57,
      "p50": 240.0,
      "p90": 240.0,
      "p99": 240.0
    },
    "close_to_resolve_slots": {
      "max": 6.0,
      "n": 57,
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
    "s_to_resolve_game_secs": {
      "max": 179.0,
      "n": 57,
      "p50": 179.0,
      "p90": 179.0,
      "p99": 179.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 57,
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
        "ingest_lag_p99_s": 2.4000000000000004,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.229376,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 407,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 55.296,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "requests": 28862.0,
        "summary": "p99 file 55.3 ms, ingest->WS p99 0.23 s (ws-stamp), error rate 0.00000, gaps 0, 404 407",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 770000.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 7,
      "5": 5
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
