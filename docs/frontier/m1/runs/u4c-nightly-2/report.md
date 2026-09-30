# Stack run `u4c-nightly-2`

- phase **complete**, beacon **test-key**, scale 100×, 100 bots, play 144 bells + 26 drain; program `EfphXGdQaNiubMWtkHsg2TDRL1Nb4dAGSEqey2bZhBF5` (`.so` sha256 `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7`, 876328 B)
- source: verify input (8651 transactions, last bell 169)

- **NOT exit-grade**: ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 4.66, p99 7.70, max 8.28 at bell 84; max per game day [8.28,5.63]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 50 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **n.a.** | criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is 100x (figures reported); SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 126 max 126 (reported) |
| 4 | **pass** | 1 unrevealed inside 7 above-cap hold windows (expected); unrevealed by rule: bounced 1 |
| 5 | **n.a.** | no persona violated, but not every expected outcome was observed: squatter not exercised ({"file_ticket@relay:QuotaExceeded":138,"file_ticket@relay:ok":1,"join@relay:ok":1}) |
| 6 | **n.a.** | no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence) |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; 8 garbage and 4 bad-plaintext transits sent and due, each settled as bad-seal (codes {"bad_plaintext:5":4,"garbage:2":8}) |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped frontier-fund (nothing to hold before the deadline)","no release .so pin (--expect-so-sha256)","beacon test-key (not real rounds)"] |

## Verdicts

- verify: **PASS** (fail codes ["ValidSealUnrevealed","PrefundedAddress"]; 8651 txs, read 0.28 s, verify 0.59 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (30 built from the run; 3.70 s)
- `.so` pin: expected sha256 – (none: V2 checks the deployed file's own hash, not exit-grade)
- load load-1 (post-play, 5000 viewers, 1 game h): p99 file 327.68 ms, error rate 0 (0 errors / 29832 requests + WS sessions), ingest → WS p99 0.75 s (ws-stamp; fold lag p99 4.80 s), WS coverage 1 outside 0 outage windows, stale retries 0 / WS reconnects 0 (outside outages 0 / 0), unavailable 0 (0 ms), generator recovery true → **fail** (["file p99 327.68 ms > 250 ms"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 330796 | 330796 | 330796 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 114 | 0 | 4655 | 338499 | 338499 | 345000 | no | 780 |
| PostAnchorMulti | 515 | 0 | 377121 | 380321 | 380379 | 400000 | no | 1177 |
| PostSeed | 2736 | 0 | 337222 | 339368 | 339691 | 345000 | no | 781 |
| PostBeacon | 2720 | 0 | 330756 | 333103 | 333193 | 340000 | no | 645 |
| OpenRing | 4 | 0 | 15070 | 15070 | 15070 | 30000 | no | 563 |
| OpenProvince | 37 | 0 | 143753 | 148635 | 148635 | 220000 | no | 400 |
| FoldOccupancy | 512 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 100 | 1 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 99 | 0 | 14211 | 16090 | 16090 | 17000 | no | 575 |
| SettleTicket | 99 | 18 | 20167 | 21653 | 21653 | 40000 | no | 562 |
| Harvest | 197 | 0 | 12161 | 14426 | 14426 | 17500 | no | 427 |
| Build | 104 | 2 | 11745 | 15948 | 15948 | 22000 | no | 428 |
| Train | 95 | 3 | 10545 | 14658 | 14658 | 17500 | no | 432 |
| Muster | 50 | 2 | 16063 | 18838 | 18838 | 25000 | no | 466 |
| Explore | 42 | 1 | 14030 | 16394 | 16394 | 20000 | no | 471 |
| SettleExplore | 42 | 0 | 8275 | 8657 | 8657 | 15000 | no | 396 |
| Depart | 58 | 2 | 18249 | 22938 | 22938 | 24500 | no | 712 |
| Reveal | 52 | 77 | 19224 | 21926 | 21926 | 26000 | no | 895 |
| SettleDeparture | 58 | 0 | 6262 | 6322 | 6322 | 48000 | no | 331 |
| SettleTransit | 58 | 5 | 58805 | 62254 | 62254 | 85000 | no | 891 |
| SweepPoolOwed | 6 | 1 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 104 | 0 | 15277 | 37864 | 37958 | 49000 | no | 1197 |
| ResolveFromInputs | 50 | 0 | 29573 | 38622 | 38622 | 290000 | no | 465 |
| SkipQuiet | 589 | 12 | 16350 | 56557 | 65575 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 19 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 52 | 0 | 6496 | 6496 | 6496 | 8000 | no | 373 |
| ClaimDefence | 1 | 4 | 12537 | 12537 | 12537 | 25500 | no | 434 |

**Reveal CU distribution** (C4 input): n 52, p50 19224, p90 21483, p99 21926, max 21926.

## Keeper latencies (a slot is 40 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2704 | 1 | 2 | 10 | 2 | 77 | 117 | 5 s |
| s_to_first_cache_slots | 2688 | 1 | 2 | 8 | 2 | 59 | 99 | 5 s |
| anchor_to_last_reveal_slots | 50 | 0 | 13 | 13 | 4 | 0 | 520 | 30 s |
| s_to_resolve_slots | 50 | 2 | 9 | 9 | 8 | 99 | 379 | 60 s |
| close_to_resolve_slots | 50 | 4 | 11 | 11 | reported | 160 | 440 | reported |

Round → anchor from the publication instant, in slots: p50 1.93, p99 2.92, max 10.93.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 25 | 1 | 6 | 6 | 0 |
| churned | 28 | 8 | 126 | 126 | 20 |
| active (GATHER/CLASH, reported) | 1 | 7 | 7 | 7 | 1 |
| resident (resident action or nudge, reported) | 0 | – | – | – | 0 |
| all | 54 | 6 | 126 | 126 | – |

Resident actions 155 (landed or refused), keeper-served nudges 315, season-end flush SkipQuiet 0 (not counted).

**ClashInputs:** 0 closed, 50 open: 50 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2704,"ANNOUNCE":1,"BEACON":2720,"BUILD":104,"CAMP":25,"CLASH":50,"CLOSE":71,"DEFENCE_CLAIM":1,"DEPART":58,"DEPARTURE_SETTLED":58,"DIVERT":5,"EXPLORE":42,"EXPLORE_RESULT":42,"FOLD":512,"GATHER":104,"GENESIS_SEED":1,"HARVEST":197,"HOLDING_FINAL":15,"JOIN":100,"MUSTER":50,"POOL_SWEEP":6,"PROVINCE_OPEN":37,"REVEAL":52,"RING_OPEN":4,"SEASON_CREATED":1,"SEED":2736,"SETTLE":99,"SKIP":589,"TICKET":99,"TRAIN":95,"TRANSIT_SETTLED":58}
- transits: {"outcome 1 seal 0":22,"outcome 3 seal 0":14,"outcome 4 seal 0":8,"outcome 6 seal 0":1,"outcome 7 seal 0":1,"outcome 8 seal 2":8,"outcome 8 seal 5":4}
- departs 58 (due 58), unsettled due: 0
- bad-seal codes: {"2":8,"5":4}
- failed transactions: {"Build: Insufficient":1,"Build: QueueFull":1,"ClaimDefence: NotEligible":4,"Depart: NotResident":1,"Depart: TipTooLow":1,"Explore: NotResident":1,"Join: AlreadyDone":1,"Muster: NotResident":1,"Muster: ProvinceFull":1,"Reveal: AlreadyDone":44,"Reveal: BadAddress":6,"Reveal: SlotMoved":12,"Reveal: TransitState":6,"Reveal: WindowClosed":9,"SettleTicket: NoTicket":18,"SettleTransit: TransitState":5,"SkipQuiet: OutOfOrder":12,"SweepPoolOwed: AlreadyDone":1,"Train: Insufficient":3}

### Failed transactions by class (reported, not gating)

128 failed: {"expected":46,"redundancy":51,"unclassified":22,"waste":9} (by cause {"a/b race":49,"adversary":6,"bot-policy":9,"bounded duplicate":12,"duplicate":2,"persona":10,"race":18}); unclassified 22.

| kind | error | n | class | cause |
|---|---|---|---|---|
| Reveal | AlreadyDone | 44 | redundancy | a/b race |
| SettleTicket | NoTicket | 18 | expected | race |
| Reveal | SlotMoved | 12 | unclassified |  |
| SkipQuiet | OutOfOrder | 12 | expected | bounded duplicate |
| Reveal | WindowClosed | 9 | expected | persona |
| Reveal | BadAddress | 6 | expected | adversary |
| Reveal | TransitState | 6 | unclassified |  |
| SettleTransit | TransitState | 5 | redundancy | a/b race |
| ClaimDefence | NotEligible | 4 | unclassified |  |
| Train | Insufficient | 3 | waste | bot-policy |
| Build | Insufficient | 1 | waste | bot-policy |
| Build | QueueFull | 1 | waste | bot-policy |
| Depart | NotResident | 1 | waste | bot-policy |
| Depart | TipTooLow | 1 | expected | persona |
| Explore | NotResident | 1 | waste | bot-policy |
| Join | AlreadyDone | 1 | redundancy | duplicate |
| Muster | NotResident | 1 | waste | bot-policy |
| Muster | ProvinceFull | 1 | waste | bot-policy |
| SweepPoolOwed | AlreadyDone | 1 | redundancy | duplicate |

- no landed transaction for ≥ 20 slots after a bell started: 0 windows (0 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":1}
- provinces 37; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 170 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":37,"anchor_latency_slots_p99":2,"archived_bells":0,"bell":170,"contested_bells":8,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19927129540,"n":32},"funders":{"lamports":2035955193301,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52518894363,"n":150}},"provinces_opened":37,"rings_complete":[0,1,2,3],"seed_latency_slots_p99":2,"spend_by_day":{"0":211341472,"1":247925986},"sweeps_sent":6,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 170 bells (0 timeouts); last answered status {"alerts":1,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":1,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999840000,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52499238939,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":337924,"1":367578},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 12, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 0, restarts 0, crashes 0
- hold ticket at game 1785632360: 1 keys, 1000 milli, 360 slots, above keeper cap true
- hold slots-below at game 1785640800: 25 keys, 1900 milli, 38 slots, above keeper cap false
- hold slots-below at game 1785643200: 25 keys, 1900 milli, 38 slots, above keeper cap false
- hold slots-above at game 1785643960: 25 keys, 3000 milli, 38 slots, above keeper cap true
- hold defence-pool at game 1785645560: 1 keys, 1000 milli, 45 slots, above keeper cap true
- hold anchor at game 1785653160: 1 keys, 1000 milli, 5 slots, above keeper cap true
- hold keeper-payers at game 1785662360: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold lag at game 1785671560: 2 keys, 1000 milli, 30 slots, above keeper cap true
- hold relay-payers at game 1785708360: 20 keys, 3000 milli, 15 slots, above keeper cap true
- hold-skipped frontier-fund at game 1785717560: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 161–520 | 1 | 0 | 10 | 521 |
| slots-below | false | 372–409 | 25 | 2 | 4 | 432 |
| slots-below | false | 432–469 | 25 | 3 | 2 | 489 |
| slots-above | true | 451–488 | 25 | 0 | 3 | 489 |
| defence-pool | true | 491–535 | 1 | 0 | 2 | 536 |
| anchor | true | 681–685 | 1 | 0 | 0 | – |
| keeper-payers | true | 911–925 | 20 | 0 | 0 | – |
| lag | true | 1141–1170 | 2 | 0 | 13 | 1171 |
| relay-payers | true | 2061–2075 | 20 | 0 | 0 | – |

### ClaimDefence (M1 exit U4)

landed 1, failed 4, refunded 42132 lamports

| keeper | slot | bell | day | slots | refund | partial | fee | beneficiary after | signature |
|---|---|---|---|---|---|---|---|---|---|
| keeper-a | 536 | 27 | 0 | 1 | 42132 | false | 16347 | 999044651 | `hcywzT89UTZpr3cUrDqZgnYtszdohY6MpgKkKSAvcWXjtxiHf4YuzjXhT8LKurts2feLXySSg9kJsdL9qrRYXuD` |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 50,
      "closed": 0,
      "open": 50,
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
      "max": 21926.0,
      "n": 52,
      "p50": 19224.0,
      "p90": 21483.0,
      "p99": 21926.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(2,-2) day 0: 7"
    ],
    "churned_province_days_over_6": [
      "(-3,0) day 0: 12",
      "(-3,1) day 0: 7",
      "(-3,2) day 0: 8",
      "(-3,3) day 0: 84",
      "(-2,2) day 0: 9",
      "(-2,3) day 0: 24",
      "(-1,-2) day 0: 26",
      "(-1,2) day 0: 8",
      "(-1,3) day 0: 14",
      "(0,2) day 0: 7",
      "(0,3) day 0: 38",
      "(1,-3) day 0: 24",
      "(1,-2) day 0: 8",
      "(1,1) day 0: 9",
      "(1,2) day 0: 8",
      "(2,-3) day 0: 18",
      "(2,0) day 0: 8",
      "(2,1) day 0: 126",
      "(3,-3) day 0: 21",
      "(3,0) day 0: 10"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states), no GATHER or CLASH of the province that day (§13.4 A1), and no resident action (Muster/Dissolve/Garrison/Explore/Depart naming it, landed or refused) or keeper-served nudge of it within the day or the 26 bells before (A1 as amended, v1.13); active = roster unchanged with a GATHER/CLASH (reported); resident = with a resident action or nudge (reported: each nudge is one split); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day; the SkipQuiet reaching end_bell (the season-end flush) is not counted",
    "idle_province_days_over_6": [],
    "nudges_served": 315,
    "province_days_not_judged": 0,
    "province_post_states": 1400,
    "resident_actions": 155,
    "resident_province_days_over_6": [],
    "season_end_flush": 0,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 1,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 126.0,
      "n": 28,
      "p50": 8.0,
      "p90": 38.0,
      "p99": 126.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 25,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 126.0,
      "n": 54,
      "p50": 6.0,
      "p90": 24.0,
      "p99": 126.0
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
      "max": 520.0,
      "n": 50,
      "p50": 0.0,
      "p90": 40.0,
      "p99": 520.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 13.0,
      "n": 50,
      "p50": 0.0,
      "p90": 1.0,
      "p99": 13.0
    },
    "close_to_resolve_game_secs": {
      "max": 440.0,
      "n": 50,
      "p50": 160.0,
      "p90": 160.0,
      "p99": 440.0
    },
    "close_to_resolve_slots": {
      "max": 11.0,
      "n": 50,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 11.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 10.925,
      "n": 2704,
      "p50": 1.925,
      "p90": 1.925,
      "p99": 2.925
    },
    "round_to_anchor_game_secs": {
      "max": 437.0,
      "n": 2704,
      "p50": 77.0,
      "p90": 77.0,
      "p99": 117.0
    },
    "round_to_anchor_slots": {
      "max": 10.0,
      "n": 2704,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 2.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 339.0,
      "n": 2688,
      "p50": 59.0,
      "p90": 59.0,
      "p99": 99.0
    },
    "s_to_first_cache_slots": {
      "max": 8.0,
      "n": 2688,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 2.0
    },
    "s_to_resolve_game_secs": {
      "max": 379.0,
      "n": 50,
      "p50": 99.0,
      "p90": 99.0,
      "p99": 379.0
    },
    "s_to_resolve_slots": {
      "max": 9.0,
      "n": 50,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 9.0
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
        "coverage": {
          "coverage_samples": 28,
          "outage_windows": [],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 0,
          "samples_unanswered": 0,
          "stale_retries_inside": 0,
          "stale_retries_outside": 0,
          "ws_coverage": 1.0,
          "ws_coverage_ok": true,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 0,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 29832.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "file_answered": 15879,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 4.800000000000001,
        "ingest_lag_samples": 44,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.753664,
        "ingest_target_s": 2.0,
        "misses": [
          "file p99 327.68 ms > 250 ms"
        ],
        "not_found": 12953,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_answered_ms": 344.064,
        "p99_file_ms": 327.68,
        "p99_file_ok": false,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "recovery_ok": true,
        "requests": 28832.0,
        "stale_retries": 0,
        "stale_retries_outside": 0,
        "summary": "p99 file 327.7 ms (answered only 344.1 ms), ingest->WS p99 0.75 s (ws-stamp; herald share p99 754 ms, delivery p99 10 ms), error rate 0.00000, gaps 0, 404 12953, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 0,
        "unavailable_ms": 0.0,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 0,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_split": {
          "delivery_max_ms": 14.0,
          "delivery_p50_ms": 0.992,
          "delivery_p99_upper_ms": 10.24,
          "herald_max_ms": 725.0,
          "herald_p50_ms": 28.672,
          "herald_p99_upper_ms": 753.664,
          "send_stamped": 754004
        },
        "ws_timed": 754004.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 8,
      "5": 4
    },
    "marchbook": {
      "codes": {
        "bad_plaintext:5": 4,
        "garbage:2": 8
      },
      "due": {
        "bad_plaintext": 4,
        "garbage": 8
      },
      "not_bad_seal": []
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
