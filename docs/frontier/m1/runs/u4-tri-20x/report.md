# Stack run `u4-tri-20x`

- phase **down**, beacon **archive**, scale 20×, 1000 bots, play 144 bells + 26 drain; program `7iyFdsv9TEsutDVzguD6BokjqUes12SMDKnA1gmS2ibu` (`.so` sha256 `072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b`, 875768 B)
- source: verify input (45746 transactions, last bell 170)

- **NOT exit-grade**: ["hold-skipped defence-pool (nothing to hold before the deadline)"]
- machine load average (1 min, sampled each bell; the machine is shared): p50 3.45, p99 10.87, max 11.24 at bell 120; max per game day [11.24,8.12]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | EndSeason landed; no stuck province-bell; every due transit settled exactly once; ClashInputs: 0 closed, 43 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 45.00 > 2 slots; SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 29 max 29 (reported) |
| 4 | **pass** | 0 unrevealed inside 7 above-cap hold windows (expected) |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **fail** | in-run window: ["error rate 0.007594414229424497 (requests 1727824)","WS coverage 16.8% < 99% outside the herald outage windows"] |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped defence-pool (nothing to hold before the deadline)"] |

## Verdicts

- verify: **PASS** (fail codes []; 45746 txs, read 1.01 s, verify 0.62 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (27 built from the run; 4.85 s)
- `.so` pin: expected sha256 072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b (the release build record; V2 checks the deployed program against it)
- load in-run (in-run, 5000 viewers, 12 game h): p99 file 7.17 ms, error rate 0.01 (13137 errors / 1729824 requests + WS sessions), ingest → WS p99 0.44 s (ws-stamp; fold lag p99 2.80 s), WS coverage 0.17 outside 3 outage windows, stale retries – / WS reconnects – (outside outages 0 / 0), unavailable – (– ms), generator recovery false → **fail** (["error rate 0.007594414229424497 (requests 1727824)","WS coverage 16.8% < 99% outside the herald outage windows"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328965 | 328965 | 328965 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 298 | 3632 | 4655 | 336881 | 336881 | 345000 | no | 780 |
| PostAnchorMulti | 480 | 681 | 377042 | 380239 | 380481 | 400000 | no | 1177 |
| PostSeed | 2528 | 0 | 336972 | 339314 | 339541 | 345000 | no | 781 |
| PostBeacon | 2736 | 156 | 330716 | 332732 | 332824 | 340000 | no | 645 |
| OpenRing | 6 | 0 | 15052 | 15400 | 15400 | 30000 | no | 563 |
| ConsumeRingSeed | 2 | 0 | 330936 | 331269 | 331269 | 345000 | no | 646 |
| OpenProvince | 91 | 20 | 140708 | 146133 | 146133 | 220000 | no | 400 |
| FoldOccupancy | 342 | 0 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 490 | 4 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 464 | 0 | 14234 | 16125 | 16138 | 17000 | no | 575 |
| SettleTicket | 501 | 39 | 20216 | 21716 | 21748 | 40000 | no | 562 |
| Harvest | 160 | 0 | 11996 | 16686 | 16686 | 17500 | no | 427 |
| Build | 362 | 0 | 11598 | 15948 | 18144 | 22000 | no | 428 |
| Train | 366 | 0 | 10545 | 16910 | 16910 | 17500 | no | 432 |
| Muster | 101 | 0 | 14740 | 18049 | 18840 | 25000 | no | 466 |
| Explore | 52 | 0 | 14057 | 16415 | 16415 | 20000 | no | 471 |
| SettleExplore | 52 | 0 | 8501 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 44 | 2 | 16574 | 18477 | 18477 | 24500 | no | 712 |
| Reveal | 44 | 11 | 20301 | 22667 | 22667 | 26000 | no | 895 |
| SettleDeparture | 44 | 44 | 6262 | 6312 | 6312 | 48000 | no | 331 |
| SettleTransit | 44 | 45 | 60307 | 62962 | 62962 | 85000 | no | 859 |
| SweepPoolOwed | 44 | 0 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 86 | 0 | 16322 | 37922 | 37922 | 49000 | no | 1197 |
| ResolveFromInputs | 43 | 0 | 33321 | 39963 | 39963 | 290000 | no | 465 |
| SkipQuiet | 671 | 7 | 35914 | 44574 | 45057 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 0 | 28080 | – | – | – | 8000 | no | 379 |
| CloseArrivalSlot | 41 | 2922 | 6496 | 6496 | 6496 | 8000 | no | 381 |

**Reveal CU distribution** (C4 input): n 44, p50 20301, p90 21712, p99 22667, max 22667.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 2304 | 2 | 45 | 118 | 2 | 23 | 367 | 5 s |
| s_to_first_cache_slots | 2304 | 2 | 2 | 108 | 2 | 19 | 19 | 5 s |
| anchor_to_last_reveal_slots | 43 | 0 | 0 | 0 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 43 | 4 | 4 | 4 | 8 | 35 | 35 | 60 s |
| close_to_resolve_slots | 43 | 12 | 12 | 12 | reported | 96 | 96 | reported |

Round → anchor from the publication instant, in slots: p50 2.88, p99 45.88, max 118.88.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 14 | 6 | 6 | 6 | 0 |
| churned | 77 | 6 | 29 | 29 | 32 |
| active (GATHER/CLASH, reported) | 0 | – | – | – | 0 |
| all | 91 | 6 | 29 | 29 | – |

**ClashInputs:** 0 closed, 43 open: 43 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":2304,"ANNOUNCE":1,"BEACON":2736,"BUILD":362,"CAMP":17,"CLASH":43,"CLOSE":41,"DEPART":44,"DEPARTURE_SETTLED":44,"DIVERT":88,"EXPLORE":52,"EXPLORE_RESULT":52,"FOLD":342,"GATHER":86,"GENESIS_SEED":1,"HARVEST":160,"HOLDING_FINAL":36,"JOIN":490,"MUSTER":101,"POOL_SWEEP":44,"PROVINCE_OPEN":91,"REVEAL":44,"RING_OPEN":6,"RING_SEED":2,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":2416,"SETTLE":501,"SKIP":671,"TICKET":464,"TRAIN":366,"TRANSIT_SETTLED":44}
- transits: {"outcome 1 seal 0":26,"outcome 3 seal 0":10,"outcome 4 seal 0":8}
- departs 44 (due 44), unsettled due: 0
- bad-seal codes: {}
- failed transactions: {"CloseArrivalDay: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":14040,"CloseArrivalDay: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":14040,"CloseArrivalSlot: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":1461,"CloseArrivalSlot: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":1461,"Depart: TipTooLow":2,"Join: AlreadyDone":4,"OpenProvince: AlreadyDone":20,"PostAnchor: BadData":3632,"PostAnchorMulti: BadData":681,"PostBeacon: AlreadyDone":156,"Reveal: AlreadyDone":8,"Reveal: WindowClosed":3,"SettleDeparture: AlreadyDone":44,"SettleTicket: AlreadyDone":9,"SettleTicket: NoTicket":30,"SettleTransit: TransitState":45,"SkipQuiet: OutOfOrder":7}

### Failed transactions by class (reported, not gating)

35643 failed: {"expected":42,"redundancy":286,"waste":35315} (by cause {"a/b race":97,"bounded duplicate":7,"duplicate":189,"keeper":35315,"persona":5,"race":30}); unclassified 0.

| kind | error | n | class | cause |
|---|---|---|---|---|
| CloseArrivalDay | {"InstructionError":[3,"ProgramFailedToComplete"]} | 14040 | waste | keeper |
| CloseArrivalDay | {"InstructionError":[4,"ProgramFailedToComplete"]} | 14040 | waste | keeper |
| PostAnchor | BadData | 3632 | waste | keeper |
| CloseArrivalSlot | {"InstructionError":[3,"ProgramFailedToComplete"]} | 1461 | waste | keeper |
| CloseArrivalSlot | {"InstructionError":[4,"ProgramFailedToComplete"]} | 1461 | waste | keeper |
| PostAnchorMulti | BadData | 681 | waste | keeper |
| PostBeacon | AlreadyDone | 156 | redundancy | duplicate |
| SettleTransit | TransitState | 45 | redundancy | a/b race |
| SettleDeparture | AlreadyDone | 44 | redundancy | a/b race |
| SettleTicket | NoTicket | 30 | expected | race |
| OpenProvince | AlreadyDone | 20 | redundancy | duplicate |
| SettleTicket | AlreadyDone | 9 | redundancy | duplicate |
| Reveal | AlreadyDone | 8 | redundancy | a/b race |
| SkipQuiet | OutOfOrder | 7 | expected | bounded duplicate |
| Join | AlreadyDone | 4 | redundancy | duplicate |
| Reveal | WindowClosed | 3 | expected | persona |
| Depart | TipTooLow | 2 | expected | persona |

- no landed transaction for ≥ 20 slots after a bell started: 1 windows (164 quiet stretches inside a bell and 5 drain gaps after end_bell not listed)
  - slots 828–992 (165 slots, 119 after bell 9 started; bells 8–10): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}
- provinces 91; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 1 of 171 bells (0 timeouts; answer ms p99 1); last answered status {"alerts":35282,"anchor_latency_slots_p99":1,"archived_bells":0,"bell":170,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19062435101,"n":32},"funders":{"lamports":2037465583392,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52460821022,"n":150}},"provinces_opened":91,"rings_complete":[0,1,2,3,4,5],"seed_latency_slots_p99":1,"spend_by_day":{"0":60361292,"1":401824015},"sweeps_sent":25,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 171 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":170,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999102562,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52499925995,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"0":971443,"1":971443},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 45, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 7, restarts 7, crashes 0
- hold ticket at game 1789087512: 1 keys, 1000 milli, 1725 slots, above keeper cap true
- hold slots-below at game 1789099512: 25 keys, 1500 milli, 188 slots, above keeper cap false
- hold slots-above at game 1789099512: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold frontier-fund at game 1789106112: 2 keys, 1000 milli, 75 slots, above keeper cap true
- hold anchor at game 1789108312: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold keeper-payers at game 1789117512: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold lag at game 1789126712: 2 keys, 1000 milli, 150 slots, above keeper cap true
- hold relay-payers at game 1789163512: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold-skipped defence-pool at game 1789173312: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 350–2074 | 1 | 0 | 13 | 2075 |
| slots-below | false | 1850–2037 | 25 | 0 | 0 | – |
| slots-above | true | 1850–2037 | 25 | 0 | 0 | – |
| frontier-fund | true | 2675–2749 | 2 | 0 | 14 | 2750 |
| anchor | true | 2950–2974 | 1 | 0 | 0 | – |
| keeper-payers | true | 4100–4174 | 20 | 0 | 0 | – |
| lag | true | 5250–5399 | 2 | 0 | 9 | 5400 |
| relay-payers | true | 9850–9924 | 20 | 0 | 2 | 9934 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 43,
      "closed": 0,
      "open": 43,
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
      "max": 22667.0,
      "n": 44,
      "p50": 20301.0,
      "p90": 21712.0,
      "p99": 22667.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [],
    "churned_province_days_over_6": [
      "(-5,0) day 0: 12",
      "(-4,2) day 0: 15",
      "(-3,1) day 0: 11",
      "(-3,3) day 0: 8",
      "(-3,4) day 0: 15",
      "(-3,5) day 0: 14",
      "(-2,-1) day 0: 19",
      "(-1,-4) day 0: 9",
      "(-1,-3) day 0: 11",
      "(-1,-2) day 0: 7",
      "(-1,4) day 0: 7",
      "(0,-3) day 0: 9",
      "(0,-2) day 0: 9",
      "(0,2) day 0: 7",
      "(0,3) day 0: 7",
      "(1,-3) day 0: 28",
      "(1,-2) day 0: 8",
      "(1,1) day 0: 7",
      "(1,2) day 0: 26",
      "(1,3) day 0: 20",
      "(1,4) day 0: 12",
      "(2,-4) day 0: 7",
      "(2,-3) day 0: 29",
      "(2,-2) day 0: 7",
      "(2,1) day 0: 11",
      "(3,-5) day 0: 10",
      "(3,-3) day 0: 7",
      "(3,-1) day 0: 10",
      "(3,0) day 0: 9",
      "(4,-2) day 0: 16",
      "(5,-5) day 0: 18",
      "(5,-2) day 0: 11"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [],
    "province_days_not_judged": 0,
    "province_post_states": 3312,
    "skip_txs_per_active_province_day": {
      "max": null,
      "n": 0,
      "p50": null,
      "p90": null,
      "p99": null
    },
    "skip_txs_per_churned_province_day": {
      "max": 29.0,
      "n": 77,
      "p50": 6.0,
      "p90": 15.0,
      "p99": 29.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 14,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 29.0,
      "n": 91,
      "p50": 6.0,
      "p90": 14.0,
      "p99": 29.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 0.0,
      "n": 43,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 0.0,
      "n": 43,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 96.0,
      "n": 43,
      "p50": 96.0,
      "p90": 96.0,
      "p99": 96.0
    },
    "close_to_resolve_slots": {
      "max": 12.0,
      "n": 43,
      "p50": 12.0,
      "p90": 12.0,
      "p99": 12.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 118.875,
      "n": 2304,
      "p50": 2.875,
      "p90": 2.875,
      "p99": 45.875
    },
    "round_to_anchor_game_secs": {
      "max": 951.0,
      "n": 2304,
      "p50": 23.0,
      "p90": 23.0,
      "p99": 367.0
    },
    "round_to_anchor_slots": {
      "max": 118.0,
      "n": 2304,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 45.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 867.0,
      "n": 2304,
      "p50": 19.0,
      "p90": 19.0,
      "p99": 19.0
    },
    "s_to_first_cache_slots": {
      "max": 108.0,
      "n": 2304,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 2.0
    },
    "s_to_resolve_game_secs": {
      "max": 35.0,
      "n": 43,
      "p50": 35.0,
      "p90": 35.0,
      "p99": 35.0
    },
    "s_to_resolve_slots": {
      "max": 4.0,
      "n": 43,
      "p50": 4.0,
      "p90": 4.0,
      "p99": 4.0
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
    "loads": [
      {
        "coverage": {
          "coverage_samples": 1735,
          "outage_windows": [
            [
              1790702532024,
              1790702548789
            ],
            [
              1790702691141,
              1790702705074
            ],
            [
              1790703432050,
              1790703448819
            ]
          ],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 40,
          "samples_unanswered": 0,
          "stale_retries_inside": 0,
          "stale_retries_outside": 0,
          "ws_coverage": 0.16829971181556197,
          "ws_coverage_ok": false,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 0,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 1729824.0,
        "error_rate": 0.007594414229424497,
        "error_rate_ok": false,
        "error_rate_target": 0.001,
        "errors": 13137.0,
        "generator_recovery": false,
        "generator_recovery_note": "this frontier-viewers has no recovery (pre-W6T-3 build): a herald kill is counted as errors and drops its WS viewers",
        "ingest_lag_p99_s": 2.8000000000000003,
        "ingest_lag_samples": 1782,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.442368,
        "ingest_target_s": 2.0,
        "misses": [
          "error rate 0.007594414229424497 (requests 1727824)",
          "WS coverage 16.8% < 99% outside the herald outage windows"
        ],
        "not_found": 198109,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 7.168,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "recovery_ok": true,
        "requests": 1727824.0,
        "stale_retries": null,
        "stale_retries_outside": 0,
        "summary": "p99 file 7.2 ms, ingest->WS p99 0.44 s (ws-stamp), error rate 0.00759, gaps 0, 404 198109, WS coverage 16.8%, recovery outside outages 0",
        "unavailable": null,
        "unavailable_ms": null,
        "ws_coverage": 0.16829971181556197,
        "ws_coverage_ok": false,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": null,
        "ws_reconnects_outside": 0,
        "ws_sessions": 2000.0,
        "ws_timed": 1586287.0
      }
    ]
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
