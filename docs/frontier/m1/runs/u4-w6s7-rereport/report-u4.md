# Stack run `w6-s7`

- phase **complete**, beacon **archive**, scale 20×, 1000 bots, play 1008 bells + 26 drain; program `GS8ULJMRSgBqVLHxBkDFo6DR1Bdja2g2X4CrUvopa515` (`.so` sha256 `072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b`, 875768 B)
- source: verify input (177765 transactions, last bell 1034)

- **NOT exit-grade**: ["hold-skipped slots-below (nothing to hold before the deadline)","hold-skipped lag (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)"]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **fail** | 9 transits due and unsettled; ClashInputs: 0 closed, 17553 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 6.00 > 2 slots; s_to_first_cache_slots p99 5.00 > 2 slots; s_to_resolve_slots p99 13.00 > 8 slots; idle province-days over 6 SkipQuiet: ["(-7,3) day 6: 7","(-6,5) day 4: 7","(-5,4) day 6: 7","(-5,6) day 4: 7","(-4,1) day 5: 8","(-4,6) day 4: 7","(-3,-2) day 3: 7","(-3,3) day 5: 8","(-2,-4) day 4: 8","(-2,-4) day 5: 8","(-2,0) day 5: 7","(-2,0) day 6: 7","(-1,5) day 4: 8","(0,6) day 4: 9","(0,6) day 5: 9","(2,2) day 4: 7","(2,3) day 4: 7","(2,3) day 5: 8","(2,3) day 6: 7","(3,-3) day 4: 8","(3,1) day 5: 8","(3,3) day 6: 7","(4,-2) day 6: 7","(5,-4) day 6: 7"]; SkipQuiet per idle province-day p99 9 max 9 (24 idle days over 6), per churned day p99 32 max 56 (reported) |
| 4 | **fail** | 27 valid seals unrevealed outside the above-cap hold windows; 0 unrevealed inside 6 above-cap hold windows (expected); unrevealed by rule: outcome 6 (no reason: pre-W6T-3 verifier) 9 |
| 5 | **pass** | no persona violated; no honest march refused by rule |
| 6 | **fail** | in-run window: ["error rate 0.002603790714245831 (requests 3456499)"] |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; the bad-seal personas held |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **not exit-grade** | ["hold-skipped slots-below (nothing to hold before the deadline)","hold-skipped lag (nothing to hold before the deadline)","hold-skipped defence-pool (nothing to hold before the deadline)"] |

## Verdicts

- verify: **FAIL** (fail codes ["CampMismatch","CampMismatch","MissingData","MissingData","MissingData","MissingData","MissingData","MissingData","MissingData","MissingData","MissingData","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","ValidSealUnrevealed","PrefundedAddress"]; 177765 txs, read 5.47 s, verify 6.42 s — E6 wall time)
- tamper: 29/29 classes FAIL with their codes (29 built from the run; 42.26 s)
- `.so` pin: expected sha256 072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b (the release build record; V2 checks the deployed program against it)
- load in-run (in-run, 5000 viewers, 24 game h): p99 file 7.17 ms, error rate 0.00 (9000 errors / – requests + WS sessions), ingest → WS p99 0.41 s (ws-stamp; fold lag p99 2.80 s), WS coverage – outside 0 outage windows, stale retries – / WS reconnects – (outside outages – / –), unavailable – (– ms), generator recovery – → **fail** (["error rate 0.002603790714245831 (requests 3456499)"])

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328965 | 328965 | 328965 | 345000 | no | 611 |
| EndSeason | 1 | 0 | 3448 | 3448 | 3448 | 10000 | no | 264 |
| AnnounceSeason | 1 | 0 | 11258 | 11258 | 11258 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 3497 | 3552 | 4655 | 4655 | 4665 | 345000 | no | 780 |
| PostAnchorMulti | 3075 | 666 | 377322 | 380270 | 380803 | 400000 | no | 1177 |
| PostSeed | 17423 | 0 | 337167 | 339419 | 339936 | 345000 | no | 781 |
| PostBeacon | 16624 | 208 | 330560 | 332764 | 333328 | 340000 | no | 645 |
| ArchiveAnchors | 10176 | 0 | 8842 | 12824 | 12834 | 60000 | no | 439 |
| CloseSeedCache | 10176 | 0 | 5792 | 5802 | 5802 | 6000 | no | 369 |
| OpenRing | 8 | 0 | 15052 | 15370 | 15370 | 30000 | no | 563 |
| ConsumeRingSeed | 4 | 0 | 332325 | 333126 | 333126 | 345000 | no | 646 |
| OpenProvince | 169 | 20 | 141299 | 147924 | 148308 | 220000 | no | 400 |
| FoldOccupancy | 2281 | 1 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 1000 | 10 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 1004 | 0 | 14319 | 16114 | 16162 | 17000 | no | 575 |
| SettleTicket | 1050 | 32 | 20278 | 21719 | 21804 | 40000 | no | 628 |
| Harvest | 3418 | 0 | 12080 | 16691 | 16701 | 17500 | no | 427 |
| Build | 6840 | 2 | 13566 | 18149 | 18162 | 22000 | no | 428 |
| Train | 2123 | 0 | 12220 | 16915 | 16924 | 17500 | no | 432 |
| Muster | 1563 | 125 | 14813 | 18779 | 18916 | 25000 | no | 466 |
| Explore | 1080 | 14 | 13982 | 18607 | 18700 | 20000 | no | 471 |
| SettleExplore | 1080 | 0 | 8275 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 1706 | 40 | 16627 | 20730 | 22998 | 24500 | no | 712 |
| Reveal | 1656 | 476 | 19441 | 22735 | 24064 | 26000 | no | 928 |
| SettleDeparture | 1762 | 1716 | 6272 | 10120 | 10142 | 48000 | no | 331 |
| SettleTransit | 1697 | 1504 | 62052 | 63200 | 63457 | 85000 | no | 859 |
| SweepPoolOwed | 1686 | 0 | 5206 | 5206 | 5216 | 8000 | no | 330 |
| GatherClash | 19181 | 0 | 14270 | 37053 | 38807 | 49000 | no | 1197 |
| ResolveFromInputs | 17553 | 19 | 37021 | 44626 | 54348 | 290000 | no | 465 |
| SkipQuiet | 10740 | 2 | 24500 | 56170 | 60304 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 497 | 25655 | 5979 | 5979 | 5979 | 8000 | no | 379 |
| CloseArrivalSlot | 1643 | 3000 | 6496 | 6496 | 6496 | 8000 | no | 381 |

**Reveal CU distribution** (C4 input): n 1656, p50 19441, p90 21609, p99 22735, max 24064.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 16128 | 2 | 6 | 154 | 2 | 23 | 55 | 5 s |
| s_to_first_cache_slots | 16128 | 2 | 5 | 144 | 2 | 19 | 43 | 5 s |
| anchor_to_last_reveal_slots | 1628 | 0 | 0 | 1 | 4 | 0 | 0 | 30 s |
| s_to_resolve_slots | 17553 | 5 | 13 | 467 | 8 | 41 | 107 | 60 s |
| close_to_resolve_slots | 17553 | 13 | 21 | 475 | reported | 104 | 168 | reported |

Round → anchor from the publication instant, in slots: p50 2.88, p99 6.88, max 154.88.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 152 | 6 | 9 | 9 | 24 |
| churned | 875 | 10 | 32 | 56 | 669 |
| active (GATHER/CLASH, reported) | 13 | 9 | 17 | 17 | 12 |
| all | 1040 | 9 | 30 | 56 | – |

**ClashInputs:** 0 closed, 17553 open: 17553 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":16128,"ANNOUNCE":1,"ARCHIVE":10176,"BEACON":16624,"BUILD":6840,"CAMP":800,"CLASH":17553,"CLOSE":22492,"DEPART":1706,"DEPARTURE_SETTLED":1798,"DIVERT":3322,"EXPLORE":1080,"EXPLORE_RESULT":1080,"FOLD":2281,"GATHER":19181,"GENESIS_SEED":1,"HARVEST":3418,"HOLDING_FINAL":559,"JOIN":1000,"MUSTER":1563,"POOL_SWEEP":1686,"PROVINCE_OPEN":169,"REVEAL":1656,"RING_OPEN":8,"RING_SEED":4,"SEASON_CREATED":1,"SEASON_STATUS":1,"SEED":17279,"SETTLE":1050,"SKIP":10740,"TICKET":1004,"TRAIN":2123,"TRANSIT_SETTLED":1697}
- transits: {"outcome 1 seal 0":747,"outcome 3 seal 0":655,"outcome 4 seal 0":229,"outcome 5 seal 0":9,"outcome 6 seal 0":10,"outcome 7 seal 0":27,"outcome 8 seal 2":15,"outcome 8 seal 5":5}
- departs 1706 (due 1706), unsettled due: 9
- bad-seal codes: {"2":15,"5":5}
- failed transactions: {"Build: QueueFull":2,"CloseArrivalDay: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":12874,"CloseArrivalDay: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":12781,"CloseArrivalSlot: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}":1500,"CloseArrivalSlot: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}":1500,"Depart: NotResident":19,"Depart: TipTooLow":21,"Explore: NotResident":14,"FoldOccupancy: FoldStale":1,"Join: AlreadyDone":10,"Muster: NotResident":14,"Muster: ProvinceFull":111,"OpenProvince: AlreadyDone":20,"PostAnchor: BadData":3552,"PostAnchorMulti: BadData":666,"PostBeacon: AlreadyDone":208,"ResolveFromInputs: OutOfOrder":19,"Reveal: AlreadyDone":350,"Reveal: BadAddress":8,"Reveal: Shielded":61,"Reveal: WindowClosed":30,"Reveal: WrongStatus":27,"SettleDeparture: AlreadyDone":1716,"SettleTicket: AlreadyDone":1,"SettleTicket: NoTicket":31,"SettleTransit: TransitState":1504,"SkipQuiet: NotQuiet":2}

### Failed transactions by class (reported, not gating)

37042 failed: {"expected":112,"redundancy":3809,"waste":33121} (by cause {"a/b race":3570,"adversary":8,"bot-bug":88,"bot-policy":160,"bounded duplicate":19,"duplicate":239,"keeper":32873,"persona":51,"race":34}); unclassified 0.

| kind | error | n | class | cause |
|---|---|---|---|---|
| CloseArrivalDay | {"InstructionError":[3,"ProgramFailedToComplete"]} | 12874 | waste | keeper |
| CloseArrivalDay | {"InstructionError":[4,"ProgramFailedToComplete"]} | 12781 | waste | keeper |
| PostAnchor | BadData | 3552 | waste | keeper |
| SettleDeparture | AlreadyDone | 1716 | redundancy | a/b race |
| SettleTransit | TransitState | 1504 | redundancy | a/b race |
| CloseArrivalSlot | {"InstructionError":[3,"ProgramFailedToComplete"]} | 1500 | waste | keeper |
| CloseArrivalSlot | {"InstructionError":[4,"ProgramFailedToComplete"]} | 1500 | waste | keeper |
| PostAnchorMulti | BadData | 666 | waste | keeper |
| Reveal | AlreadyDone | 350 | redundancy | a/b race |
| PostBeacon | AlreadyDone | 208 | redundancy | duplicate |
| Muster | ProvinceFull | 111 | waste | bot-policy |
| Reveal | Shielded | 61 | waste | bot-bug |
| SettleTicket | NoTicket | 31 | expected | race |
| Reveal | WindowClosed | 30 | expected | persona |
| Reveal | WrongStatus | 27 | waste | bot-bug |
| Depart | TipTooLow | 21 | expected | persona |
| OpenProvince | AlreadyDone | 20 | redundancy | duplicate |
| Depart | NotResident | 19 | waste | bot-policy |
| ResolveFromInputs | OutOfOrder | 19 | expected | bounded duplicate |
| Explore | NotResident | 14 | waste | bot-policy |
| Muster | NotResident | 14 | waste | bot-policy |
| Join | AlreadyDone | 10 | redundancy | duplicate |
| Reveal | BadAddress | 8 | expected | adversary |
| Build | QueueFull | 2 | waste | bot-policy |
| SkipQuiet | NotQuiet | 2 | expected | race |
| FoldOccupancy | FoldStale | 1 | expected | race |
| SettleTicket | AlreadyDone | 1 | redundancy | duplicate |

- no landed transaction for ≥ 20 slots after a bell started: 1 windows (575 quiet stretches inside a bell not listed)
  - slots 827–1028 (202 slots, 155 after bell 9 started; bells 8–11): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"outcome 6 (no reason: pre-W6T-3 verifier)":9}
- provinces 169; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 322 of 1035 bells (0 timeouts; answer ms p99 –); last answered status {"alerts":31,"anchor_latency_slots_p99":4,"archived_bells":448,"bell":1002,"contested_bells":21,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19903380159,"n":32},"funders":{"lamports":1893307850813,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52347615003,"n":150}},"provinces_opened":169,"rings_complete":[0,1,2,3,4,5,6,7],"seed_latency_slots_p99":5,"spend_by_day":{"6":197731718},"sweeps_sent":134,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 14 of 1035 bells (0 timeouts); last answered status {"alerts":9,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":1034,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23970955525,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497783287,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"6":3675550,"7":3888618},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 21, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 43, restarts 43, crashes 0
- hold ticket at game 1789087512: 1 keys, 1000 milli, 1725 slots, above keeper cap true
- hold-skipped slots-below at game 1789094112: – keys, – milli, – slots, above keeper cap –
- hold frontier-fund at game 1789110312: 2 keys, 1000 milli, 75 slots, above keeper cap true
- hold slots-above at game 1789156712: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold anchor at game 1789223512: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold keeper-payers at game 1789290312: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold-skipped lag at game 1789361112: – keys, – milli, – slots, above keeper cap –
- hold relay-payers at game 1789624312: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold-skipped defence-pool at game 1789691712: – keys, – milli, – slots, above keeper cap –

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 350–2074 | 1 | 0 | 19 | 2075 |
| frontier-fund | true | 3200–3274 | 2 | 0 | 14 | 3275 |
| slots-above | true | 9000–9187 | 25 | 0 | 0 | – |
| anchor | true | 17350–17374 | 1 | 0 | 0 | – |
| keeper-payers | true | 25700–25774 | 20 | 0 | 0 | – |
| relay-payers | true | 67450–67524 | 20 | 0 | 3 | 67525 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 17553,
      "closed": 0,
      "open": 17553,
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
    "unsettled_due_transits": 9
  },
  "2_cu": {
    "over_budget": [],
    "reveal": {
      "max": 24064.0,
      "n": 1656,
      "p50": 19441.0,
      "p90": 21609.0,
      "p99": 22735.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(-6,-1) day 6: 8",
      "(-5,2) day 3: 10",
      "(-4,-2) day 5: 16",
      "(-3,2) day 5: 8",
      "(-2,-2) day 5: 12",
      "(-2,0) day 2: 7",
      "(-1,3) day 5: 8",
      "(1,2) day 3: 9",
      "(2,-6) day 6: 17",
      "(3,-7) day 6: 9",
      "(5,-3) day 4: 13",
      "(6,-6) day 5: 9"
    ],
    "churned_province_days_over_6": [
      "(-7,0) day 3: 9",
      "(-7,0) day 4: 12",
      "(-7,0) day 5: 8",
      "(-7,1) day 3: 19",
      "(-7,1) day 4: 19",
      "(-7,1) day 5: 7",
      "(-7,2) day 3: 7",
      "(-7,2) day 4: 12",
      "(-7,2) day 5: 11",
      "(-7,2) day 6: 13",
      "(-7,3) day 3: 8",
      "(-7,3) day 4: 8",
      "(-7,3) day 5: 14",
      "(-7,4) day 3: 12",
      "(-7,4) day 4: 9",
      "(-7,4) day 5: 10",
      "(-7,4) day 6: 12",
      "(-7,5) day 4: 16",
      "(-7,5) day 5: 10",
      "(-7,5) day 6: 33",
      "(-7,6) day 3: 17",
      "(-7,6) day 4: 8",
      "(-7,6) day 5: 7",
      "(-7,6) day 6: 24",
      "(-7,7) day 4: 11",
      "(-7,7) day 5: 10",
      "(-7,7) day 6: 9",
      "(-6,-1) day 4: 7",
      "(-6,-1) day 5: 11",
      "(-6,1) day 1: 10",
      "(-6,1) day 2: 21",
      "(-6,1) day 3: 16",
      "(-6,1) day 4: 10",
      "(-6,1) day 5: 8",
      "(-6,2) day 1: 7",
      "(-6,2) day 2: 18",
      "(-6,2) day 3: 12",
      "(-6,2) day 4: 14",
      "(-6,2) day 6: 14",
      "(-6,3) day 1: 7",
      "(-6,3) day 2: 23",
      "(-6,3) day 3: 22",
      "(-6,3) day 4: 13",
      "(-6,3) day 5: 15",
      "(-6,3) day 6: 19",
      "(-6,4) day 1: 7",
      "(-6,4) day 2: 12",
      "(-6,4) day 3: 14",
      "(-6,4) day 4: 12",
      "(-6,4) day 5: 7",
      "(-6,4) day 6: 13",
      "(-6,5) day 2: 10",
      "(-6,5) day 3: 11",
      "(-6,5) day 5: 7",
      "(-6,5) day 6: 16",
      "(-6,6) day 2: 7",
      "(-6,6) day 3: 9",
      "(-6,6) day 4: 12",
      "(-6,6) day 5: 7",
      "(-6,7) day 3: 21",
      "(-6,7) day 4: 15",
      "(-5,-2) day 4: 14",
      "(-5,-2) day 5: 24",
      "(-5,-2) day 6: 19",
      "(-5,-1) day 2: 10",
      "(-5,-1) day 3: 9",
      "(-5,-1) day 4: 10",
      "(-5,-1) day 5: 15",
      "(-5,-1) day 6: 10",
      "(-5,0) day 0: 9",
      "(-5,0) day 1: 32",
      "(-5,0) day 2: 8",
      "(-5,0) day 3: 13",
      "(-5,0) day 4: 21",
      "(-5,0) day 5: 19",
      "(-5,0) day 6: 12",
      "(-5,1) day 1: 11",
      "(-5,2) day 1: 12",
      "(-5,2) day 2: 13",
      "(-5,2) day 4: 11",
      "(-5,2) day 5: 15",
      "(-5,2) day 6: 16",
      "(-5,3) day 0: 9",
      "(-5,3) day 1: 23",
      "(-5,3) day 2: 16",
      "(-5,3) day 3: 15",
      "(-5,3) day 4: 10",
      "(-5,3) day 5: 23",
      "(-5,3) day 6: 26",
      "(-5,4) day 1: 10",
      "(-5,4) day 2: 12",
      "(-5,4) day 3: 13",
      "(-5,4) day 4: 13",
      "(-5,4) day 5: 10",
      "(-5,5) day 1: 9",
      "(-5,5) day 2: 13",
      "(-5,6) day 1: 7",
      "(-5,6) day 2: 14",
      "(-5,6) day 3: 10",
      "(-5,6) day 5: 15",
      "(-5,6) day 6: 15",
      "(-5,7) day 3: 7",
      "(-5,7) day 4: 14",
      "(-5,7) day 5: 11",
      "(-5,7) day 6: 25",
      "(-4,-3) day 3: 7",
      "(-4,-3) day 4: 9",
      "(-4,-3) day 5: 13",
      "(-4,-2) day 2: 10",
      "(-4,-2) day 3: 12",
      "(-4,-2) day 4: 11",
      "(-4,-2) day 6: 11",
      "(-4,-1) day 0: 10",
      "(-4,-1) day 1: 21",
      "(-4,-1) day 2: 16",
      "(-4,-1) day 3: 12",
      "(-4,-1) day 4: 13",
      "(-4,-1) day 5: 21",
      "(-4,-1) day 6: 8",
      "(-4,0) day 0: 11",
      "(-4,0) day 1: 23",
      "(-4,0) day 2: 16",
      "(-4,0) day 3: 9",
      "(-4,0) day 4: 10",
      "(-4,1) day 1: 7",
      "(-4,2) day 1: 7",
      "(-4,2) day 2: 7",
      "(-4,2) day 3: 9",
      "(-4,2) day 4: 7",
      "(-4,2) day 5: 14",
      "(-4,2) day 6: 10",
      "(-4,3) day 0: 14",
      "(-4,3) day 1: 20",
      "(-4,3) day 2: 21",
      "(-4,3) day 3: 30",
      "(-4,3) day 4: 21",
      "(-4,3) day 5: 17",
      "(-4,3) day 6: 13",
      "(-4,4) day 1: 10",
      "(-4,4) day 2: 18",
      "(-4,4) day 3: 11",
      "(-4,4) day 4: 8",
      "(-4,5) day 0: 16",
      "(-4,5) day 1: 30",
      "(-4,5) day 2: 24",
      "(-4,6) day 1: 9",
      "(-4,6) day 2: 10",
      "(-4,6) day 3: 13",
      "(-4,6) day 6: 16",
      "(-4,7) day 4: 9",
      "(-4,7) day 5: 10",
      "(-4,7) day 6: 14",
      "(-3,-4) day 3: 9",
      "(-3,-4) day 4: 9",
      "(-3,-4) day 5: 7",
      "(-3,-4) day 6: 12",
      "(-3,-3) day 1: 9",
      "(-3,-3) day 2: 26",
      "(-3,-3) day 3: 26",
      "(-3,-3) day 4: 13",
      "(-3,-3) day 5: 17",
      "(-3,-3) day 6: 17",
      "(-3,-2) day 1: 8",
      "(-3,-2) day 2: 12",
      "(-3,-2) day 4: 8",
      "(-3,-2) day 5: 9",
      "(-3,-1) day 1: 9",
      "(-3,-1) day 2: 9",
      "(-3,-1) day 3: 9",
      "(-3,-1) day 5: 8",
      "(-3,-1) day 6: 7",
      "(-3,0) day 1: 14",
      "(-3,0) day 2: 15",
      "(-3,0) day 3: 11",
      "(-3,1) day 0: 9",
      "(-3,1) day 1: 13",
      "(-3,1) day 2: 13",
      "(-3,1) day 3: 13",
      "(-3,1) day 4: 15",
      "(-3,1) day 5: 10",
      "(-3,1) day 6: 15",
      "(-3,2) day 0: 7",
      "(-3,2) day 1: 16",
      "(-3,2) day 2: 18",
      "(-3,2) day 3: 19",
      "(-3,2) day 4: 9",
      "(-3,2) day 6: 14",
      "(-3,3) day 0: 10",
      "(-3,3) day 1: 20",
      "(-3,3) day 2: 15",
      "(-3,3) day 3: 18",
      "(-3,3) day 4: 14",
      "(-3,3) day 6: 9",
      "(-3,4) day 1: 8",
      "(-3,4) day 2: 9",
      "(-3,4) day 5: 7",
      "(-3,5) day 1: 8",
      "(-3,5) day 2: 12",
      "(-3,5) day 3: 11",
      "(-3,5) day 4: 9",
      "(-3,5) day 5: 7",
      "(-3,5) day 6: 10",
      "(-3,6) day 2: 10",
      "(-3,6) day 3: 8",
      "(-3,6) day 4: 9",
      "(-3,6) day 5: 14",
      "(-3,6) day 6: 14",
      "(-3,7) day 4: 9",
      "(-3,7) day 6: 12",
      "(-2,-5) day 4: 8",
      "(-2,-5) day 6: 11",
      "(-2,-4) day 2: 15",
      "(-2,-4) day 3: 12",
      "(-2,-4) day 6: 9",
      "(-2,-3) day 0: 12",
      "(-2,-3) day 1: 19",
      "(-2,-3) day 2: 28",
      "(-2,-3) day 3: 16",
      "(-2,-3) day 4: 17",
      "(-2,-3) day 5: 21",
      "(-2,-3) day 6: 22",
      "(-2,-2) day 1: 7",
      "(-2,-2) day 2: 11",
      "(-2,-2) day 3: 9",
      "(-2,-2) day 4: 8",
      "(-2,-2) day 6: 13",
      "(-2,-1) day 0: 11",
      "(-2,-1) day 1: 14",
      "(-2,-1) day 2: 14",
      "(-2,-1) day 3: 10",
      "(-2,-1) day 4: 13",
      "(-2,-1) day 5: 8",
      "(-2,-1) day 6: 9",
      "(-2,0) day 4: 7",
      "(-2,1) day 6: 7",
      "(-2,2) day 1: 7",
      "(-2,2) day 2: 7",
      "(-2,3) day 0: 8",
      "(-2,3) day 1: 14",
      "(-2,3) day 2: 7",
      "(-2,4) day 0: 7",
      "(-2,4) day 1: 13",
      "(-2,4) day 2: 13",
      "(-2,4) day 3: 8",
      "(-2,4) day 4: 10",
      "(-2,4) day 5: 9",
      "(-2,4) day 6: 19",
      "(-2,5) day 1: 27",
      "(-2,5) day 2: 18",
      "(-2,5) day 3: 13",
      "(-2,5) day 4: 9",
      "(-2,5) day 5: 12",
      "(-2,5) day 6: 25",
      "(-2,6) day 1: 25",
      "(-2,6) day 2: 17",
      "(-2,6) day 3: 16",
      "(-2,6) day 4: 8",
      "(-2,6) day 5: 7",
      "(-2,6) day 6: 9",
      "(-2,7) day 4: 16",
      "(-2,7) day 5: 16",
      "(-2,7) day 6: 9",
      "(-1,-6) day 4: 21",
      "(-1,-6) day 5: 19",
      "(-1,-6) day 6: 22",
      "(-1,-5) day 2: 11",
      "(-1,-5) day 3: 12",
      "(-1,-4) day 1: 9",
      "(-1,-4) day 2: 13",
      "(-1,-4) day 3: 10",
      "(-1,-3) day 1: 7",
      "(-1,-3) day 2: 7",
      "(-1,-3) day 3: 11",
      "(-1,-3) day 4: 7",
      "(-1,-3) day 5: 11",
      "(-1,-3) day 6: 10",
      "(-1,-2) day 0: 20",
      "(-1,-2) day 1: 26",
      "(-1,-2) day 2: 15",
      "(-1,-1) day 1: 7",
      "(-1,-1) day 2: 7",
      "(-1,2) day 2: 7",
      "(-1,2) day 5: 7",
      "(-1,3) day 1: 12",
      "(-1,3) day 2: 12",
      "(-1,3) day 3: 10",
      "(-1,3) day 4: 11",
      "(-1,3) day 6: 11",
      "(-1,4) day 1: 20",
      "(-1,4) day 2: 15",
      "(-1,4) day 3: 16",
      "(-1,4) day 4: 15",
      "(-1,4) day 5: 17",
      "(-1,4) day 6: 13",
      "(-1,5) day 1: 16",
      "(-1,5) day 2: 16",
      "(-1,5) day 3: 13",
      "(-1,5) day 5: 11",
      "(-1,5) day 6: 9",
      "(-1,6) day 1: 18",
      "(-1,6) day 2: 19",
      "(-1,6) day 3: 18",
      "(-1,6) day 4: 9",
      "(-1,6) day 5: 26",
      "(-1,6) day 6: 10",
      "(-1,7) day 3: 9",
      "(-1,7) day 4: 12",
      "(-1,7) day 5: 15",
      "(0,-7) day 3: 7",
      "(0,-7) day 4: 11",
      "(0,-7) day 5: 7",
      "(0,-6) day 1: 21",
      "(0,-6) day 2: 30",
      "(0,-6) day 3: 27",
      "(0,-6) day 4: 19",
      "(0,-5) day 1: 8",
      "(0,-5) day 2: 12",
      "(0,-5) day 3: 11",
      "(0,-4) day 1: 12",
      "(0,-4) day 2: 10",
      "(0,-4) day 3: 12",
      "(0,-4) day 6: 7",
      "(0,-3) day 0: 8",
      "(0,-3) day 1: 13",
      "(0,-3) day 2: 9",
      "(0,-2) day 1: 7",
      "(0,-2) day 2: 7",
      "(0,2) day 2: 7",
      "(0,2) day 5: 7",
      "(0,3) day 0: 8",
      "(0,3) day 1: 21",
      "(0,3) day 2: 14",
      "(0,3) day 3: 11",
      "(0,3) day 5: 9",
      "(0,3) day 6: 10",
      "(0,4) day 0: 15",
      "(0,4) day 1: 23",
      "(0,4) day 2: 21",
      "(0,4) day 3: 32",
      "(0,4) day 4: 28",
      "(0,4) day 5: 11",
      "(0,5) day 1: 12",
      "(0,5) day 2: 20",
      "(0,6) day 2: 13",
      "(0,6) day 3: 9",
      "(0,6) day 6: 9",
      "(0,7) day 4: 8",
      "(0,7) day 5: 8",
      "(0,7) day 6: 7",
      "(1,-7) day 3: 7",
      "(1,-7) day 4: 11",
      "(1,-7) day 5: 15",
      "(1,-7) day 6: 9",
      "(1,-6) day 1: 29",
      "(1,-6) day 2: 24",
      "(1,-6) day 3: 23",
      "(1,-6) day 4: 9",
      "(1,-6) day 5: 17",
      "(1,-6) day 6: 45",
      "(1,-5) day 0: 17",
      "(1,-5) day 1: 21",
      "(1,-5) day 2: 20",
      "(1,-5) day 3: 19",
      "(1,-5) day 4: 10",
      "(1,-5) day 5: 11",
      "(1,-5) day 6: 56",
      "(1,-4) day 1: 16",
      "(1,-4) day 2: 9",
      "(1,-4) day 3: 8",
      "(1,-4) day 4: 8",
      "(1,-4) day 5: 9",
      "(1,-4) day 6: 14",
      "(1,-3) day 1: 13",
      "(1,-3) day 2: 11",
      "(1,-3) day 3: 14",
      "(1,-3) day 4: 12",
      "(1,-3) day 5: 11",
      "(1,-3) day 6: 10",
      "(1,1) day 0: 7",
      "(1,1) day 1: 7",
      "(1,2) day 0: 11",
      "(1,2) day 1: 15",
      "(1,2) day 2: 17",
      "(1,2) day 4: 13",
      "(1,2) day 5: 14",
      "(1,2) day 6: 11",
      "(1,3) day 1: 7",
      "(1,3) day 2: 11",
      "(1,3) day 3: 9",
      "(1,3) day 6: 7",
      "(1,4) day 1: 9",
      "(1,4) day 2: 10",
      "(1,4) day 3: 8",
      "(1,6) day 4: 8",
      "(1,6) day 5: 12",
      "(1,6) day 6: 8",
      "(2,-7) day 3: 11",
      "(2,-7) day 4: 15",
      "(2,-7) day 5: 10",
      "(2,-7) day 6: 9",
      "(2,-6) day 1: 8",
      "(2,-6) day 2: 18",
      "(2,-6) day 3: 11",
      "(2,-6) day 4: 11",
      "(2,-6) day 5: 13",
      "(2,-5) day 1: 9",
      "(2,-5) day 2: 10",
      "(2,-5) day 3: 13",
      "(2,-5) day 4: 16",
      "(2,-5) day 5: 13",
      "(2,-5) day 6: 9",
      "(2,-4) day 1: 11",
      "(2,-4) day 2: 10",
      "(2,-4) day 3: 13",
      "(2,-4) day 4: 12",
      "(2,-4) day 5: 14",
      "(2,-4) day 6: 9",
      "(2,-3) day 1: 8",
      "(2,-3) day 2: 7",
      "(2,-3) day 3: 7",
      "(2,-3) day 4: 8",
      "(2,-3) day 6: 9",
      "(2,-2) day 5: 7",
      "(2,-1) day 2: 7",
      "(2,-1) day 3: 8",
      "(2,0) day 1: 7",
      "(2,0) day 3: 8",
      "(2,1) day 0: 33",
      "(2,1) day 1: 15",
      "(2,1) day 2: 20",
      "(2,1) day 3: 23",
      "(2,1) day 4: 15",
      "(2,1) day 5: 30",
      "(2,1) day 6: 46",
      "(2,2) day 1: 9",
      "(2,2) day 2: 10",
      "(2,2) day 3: 9",
      "(2,2) day 5: 13",
      "(2,2) day 6: 13",
      "(2,3) day 0: 13",
      "(2,3) day 1: 28",
      "(2,3) day 2: 22",
      "(2,3) day 3: 21",
      "(2,4) day 1: 8",
      "(2,4) day 2: 9",
      "(2,4) day 3: 15",
      "(2,4) day 4: 10",
      "(2,4) day 5: 9",
      "(2,4) day 6: 7",
      "(2,5) day 4: 10",
      "(2,5) day 5: 13",
      "(2,5) day 6: 13",
      "(3,-7) day 4: 10",
      "(3,-7) day 5: 10",
      "(3,-6) day 1: 25",
      "(3,-6) day 2: 17",
      "(3,-6) day 3: 22",
      "(3,-6) day 4: 10",
      "(3,-6) day 5: 25",
      "(3,-6) day 6: 26",
      "(3,-5) day 1: 8",
      "(3,-5) day 2: 11",
      "(3,-5) day 3: 12",
      "(3,-5) day 4: 7",
      "(3,-5) day 5: 10",
      "(3,-5) day 6: 9",
      "(3,-4) day 1: 7",
      "(3,-4) day 2: 7",
      "(3,-3) day 1: 12",
      "(3,-3) day 2: 16",
      "(3,-3) day 3: 11",
      "(3,-3) day 5: 10",
      "(3,-3) day 6: 11",
      "(3,-2) day 1: 14",
      "(3,-2) day 2: 17",
      "(3,-2) day 3: 14",
      "(3,-2) day 4: 11",
      "(3,-2) day 5: 14",
      "(3,-2) day 6: 10",
      "(3,-1) day 0: 10",
      "(3,-1) day 1: 16",
      "(3,-1) day 2: 19",
      "(3,0) day 1: 16",
      "(3,0) day 2: 13",
      "(3,0) day 4: 9",
      "(3,0) day 5: 17",
      "(3,0) day 6: 14",
      "(3,1) day 0: 7",
      "(3,1) day 1: 13",
      "(3,1) day 2: 14",
      "(3,1) day 3: 9",
      "(3,1) day 6: 9",
      "(3,2) day 1: 9",
      "(3,2) day 2: 12",
      "(3,2) day 3: 7",
      "(3,2) day 4: 17",
      "(3,2) day 5: 12",
      "(3,3) day 1: 29",
      "(3,3) day 2: 21",
      "(3,3) day 3: 21",
      "(3,3) day 4: 29",
      "(3,3) day 5: 15",
      "(3,4) day 3: 7",
      "(3,4) day 5: 12",
      "(3,4) day 6: 12",
      "(4,-7) day 5: 9",
      "(4,-7) day 6: 8",
      "(4,-6) day 1: 11",
      "(4,-6) day 2: 17",
      "(4,-6) day 3: 11",
      "(4,-6) day 4: 12",
      "(4,-6) day 5: 16",
      "(4,-6) day 6: 13",
      "(4,-5) day 1: 9",
      "(4,-5) day 4: 7",
      "(4,-4) day 0: 16",
      "(4,-4) day 1: 23",
      "(4,-4) day 2: 22",
      "(4,-4) day 3: 14",
      "(4,-4) day 4: 12",
      "(4,-4) day 5: 13",
      "(4,-4) day 6: 15",
      "(4,-3) day 1: 13",
      "(4,-3) day 2: 15",
      "(4,-3) day 3: 10",
      "(4,-3) day 4: 9",
      "(4,-3) day 5: 12",
      "(4,-3) day 6: 11",
      "(4,-2) day 2: 7",
      "(4,-2) day 3: 8",
      "(4,-2) day 4: 10",
      "(4,-1) day 1: 7",
      "(4,0) day 0: 15",
      "(4,0) day 1: 20",
      "(4,0) day 2: 16",
      "(4,0) day 3: 15",
      "(4,0) day 4: 7",
      "(4,1) day 1: 10",
      "(4,1) day 2: 15",
      "(4,1) day 3: 12",
      "(4,1) day 4: 13",
      "(4,1) day 5: 11",
      "(4,1) day 6: 10",
      "(4,2) day 1: 7",
      "(4,2) day 2: 9",
      "(4,2) day 3: 13",
      "(4,2) day 4: 9",
      "(4,2) day 5: 13",
      "(4,2) day 6: 10",
      "(4,3) day 3: 7",
      "(4,3) day 4: 14",
      "(4,3) day 5: 14",
      "(4,3) day 6: 11",
      "(5,-7) day 4: 8",
      "(5,-7) day 5: 23",
      "(5,-7) day 6: 24",
      "(5,-6) day 1: 9",
      "(5,-6) day 6: 8",
      "(5,-5) day 0: 14",
      "(5,-5) day 1: 15",
      "(5,-5) day 2: 16",
      "(5,-5) day 3: 19",
      "(5,-5) day 4: 22",
      "(5,-5) day 6: 10",
      "(5,-4) day 1: 7",
      "(5,-4) day 2: 11",
      "(5,-4) day 3: 9",
      "(5,-4) day 4: 8",
      "(5,-4) day 5: 11",
      "(5,-3) day 1: 13",
      "(5,-3) day 2: 16",
      "(5,-3) day 3: 13",
      "(5,-3) day 5: 16",
      "(5,-3) day 6: 14",
      "(5,-2) day 0: 11",
      "(5,-2) day 1: 18",
      "(5,-2) day 2: 22",
      "(5,-2) day 3: 17",
      "(5,-2) day 4: 22",
      "(5,-2) day 5: 30",
      "(5,-2) day 6: 25",
      "(5,-1) day 1: 12",
      "(5,-1) day 4: 9",
      "(5,-1) day 5: 8",
      "(5,-1) day 6: 10",
      "(5,0) day 1: 11",
      "(5,0) day 2: 17",
      "(5,0) day 3: 9",
      "(5,1) day 1: 7",
      "(5,1) day 2: 38",
      "(5,1) day 3: 10",
      "(5,1) day 4: 12",
      "(5,1) day 5: 9",
      "(5,1) day 6: 23",
      "(5,2) day 3: 8",
      "(5,2) day 4: 11",
      "(5,2) day 5: 14",
      "(5,2) day 6: 9",
      "(6,-7) day 3: 7",
      "(6,-7) day 4: 10",
      "(6,-7) day 5: 10",
      "(6,-7) day 6: 12",
      "(6,-6) day 1: 11",
      "(6,-6) day 2: 15",
      "(6,-6) day 3: 13",
      "(6,-6) day 4: 10",
      "(6,-5) day 1: 12",
      "(6,-5) day 2: 9",
      "(6,-5) day 3: 14",
      "(6,-5) day 4: 8",
      "(6,-5) day 5: 10",
      "(6,-5) day 6: 7",
      "(6,-4) day 1: 24",
      "(6,-4) day 2: 16",
      "(6,-4) day 3: 16",
      "(6,-4) day 4: 29",
      "(6,-4) day 5: 23",
      "(6,-4) day 6: 17",
      "(6,-3) day 2: 17",
      "(6,-3) day 3: 12",
      "(6,-3) day 4: 8",
      "(6,-3) day 5: 13",
      "(6,-3) day 6: 9",
      "(6,-2) day 1: 27",
      "(6,-2) day 2: 18",
      "(6,-2) day 3: 18",
      "(6,-2) day 4: 9",
      "(6,-2) day 5: 23",
      "(6,-2) day 6: 23",
      "(6,-1) day 2: 9",
      "(6,-1) day 3: 11",
      "(6,0) day 1: 17",
      "(6,0) day 2: 46",
      "(6,0) day 3: 22",
      "(6,0) day 4: 25",
      "(6,1) day 3: 8",
      "(6,1) day 4: 14",
      "(6,1) day 5: 11",
      "(6,1) day 6: 12",
      "(7,-7) day 4: 7",
      "(7,-7) day 5: 14",
      "(7,-7) day 6: 8",
      "(7,-6) day 3: 7",
      "(7,-6) day 4: 8",
      "(7,-6) day 5: 9",
      "(7,-6) day 6: 10",
      "(7,-5) day 3: 16",
      "(7,-5) day 4: 25",
      "(7,-5) day 5: 8",
      "(7,-5) day 6: 30",
      "(7,-4) day 3: 9",
      "(7,-4) day 4: 21",
      "(7,-4) day 5: 15",
      "(7,-4) day 6: 12",
      "(7,-3) day 3: 8",
      "(7,-3) day 4: 8",
      "(7,-3) day 5: 9",
      "(7,-3) day 6: 13",
      "(7,-2) day 3: 8",
      "(7,-2) day 4: 8",
      "(7,-2) day 5: 14",
      "(7,-2) day 6: 7",
      "(7,-1) day 3: 12",
      "(7,-1) day 4: 7",
      "(7,-1) day 5: 8",
      "(7,0) day 3: 12",
      "(7,0) day 4: 11",
      "(7,0) day 5: 20",
      "(7,0) day 6: 17"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states) and no GATHER or CLASH of the province that day (§13.4 A1); active = roster unchanged with a GATHER/CLASH (reported); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day",
    "idle_province_days_over_6": [
      "(-7,3) day 6: 7",
      "(-6,5) day 4: 7",
      "(-5,4) day 6: 7",
      "(-5,6) day 4: 7",
      "(-4,1) day 5: 8",
      "(-4,6) day 4: 7",
      "(-3,-2) day 3: 7",
      "(-3,3) day 5: 8",
      "(-2,-4) day 4: 8",
      "(-2,-4) day 5: 8",
      "(-2,0) day 5: 7",
      "(-2,0) day 6: 7",
      "(-1,5) day 4: 8",
      "(0,6) day 4: 9",
      "(0,6) day 5: 9",
      "(2,2) day 4: 7",
      "(2,3) day 4: 7",
      "(2,3) day 5: 8",
      "(2,3) day 6: 7",
      "(3,-3) day 4: 8",
      "(3,1) day 5: 8",
      "(3,3) day 6: 7",
      "(4,-2) day 6: 7",
      "(5,-4) day 6: 7"
    ],
    "province_days_not_judged": 0,
    "province_post_states": 42519,
    "skip_txs_per_active_province_day": {
      "max": 17.0,
      "n": 13,
      "p50": 9.0,
      "p90": 16.0,
      "p99": 17.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 56.0,
      "n": 875,
      "p50": 10.0,
      "p90": 20.0,
      "p99": 32.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 9.0,
      "n": 152,
      "p50": 6.0,
      "p90": 7.0,
      "p99": 9.0
    },
    "skip_txs_per_province_day": {
      "max": 56.0,
      "n": 1040,
      "p50": 9.0,
      "p90": 19.0,
      "p99": 30.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 8.0,
      "n": 1628,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 1.0,
      "n": 1628,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 0.0
    },
    "close_to_resolve_game_secs": {
      "max": 3800.0,
      "n": 17553,
      "p50": 104.0,
      "p90": 136.0,
      "p99": 168.0
    },
    "close_to_resolve_slots": {
      "max": 475.0,
      "n": 17553,
      "p50": 13.0,
      "p90": 17.0,
      "p99": 21.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 154.875,
      "n": 16128,
      "p50": 2.875,
      "p90": 3.875,
      "p99": 6.875
    },
    "round_to_anchor_game_secs": {
      "max": 1239.0,
      "n": 16128,
      "p50": 23.0,
      "p90": 31.0,
      "p99": 55.0
    },
    "round_to_anchor_slots": {
      "max": 154.0,
      "n": 16128,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 6.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 1155.0,
      "n": 16128,
      "p50": 19.0,
      "p90": 27.0,
      "p99": 43.0
    },
    "s_to_first_cache_slots": {
      "max": 144.0,
      "n": 16128,
      "p50": 2.0,
      "p90": 3.0,
      "p99": 5.0
    },
    "s_to_resolve_game_secs": {
      "max": 3737.0,
      "n": 17553,
      "p50": 41.0,
      "p90": 73.0,
      "p99": 107.0
    },
    "s_to_resolve_slots": {
      "max": 467.0,
      "n": 17553,
      "p50": 5.0,
      "p90": 9.0,
      "p99": 13.0
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
        "error_rate": 0.002603790714245831,
        "error_rate_ok": false,
        "error_rate_target": 0.001,
        "errors": 9000.0,
        "ingest_lag_p99_s": 2.8000000000000003,
        "ingest_lag_samples": 3572,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.4096,
        "ingest_target_s": 2.0,
        "misses": [
          "error rate 0.002603790714245831 (requests 3456499)"
        ],
        "not_found": 202350,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_ms": 7.168,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": false,
        "requests": 3456499.0,
        "summary": "p99 file 7.2 ms, ingest->WS p99 0.41 s (ws-stamp), error rate 0.00260, gaps 0, 404 202350",
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_timed": 1942352.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 15,
      "5": 5
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
