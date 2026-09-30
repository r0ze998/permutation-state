# Stack run `integ-w6t-rv-3d`

- phase **complete**, beacon **archive**, scale 20×, 1000 bots, play 432 bells + 26 drain; program `9JY3cmQyXjV1hF2QCgyfQoYAA9f97v9roo6UjrrtHJqj` (`.so` sha256 `d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`, 875824 B)
- source: verify input (47129 transactions, last bell 458)

- **exit-grade environment**
- machine load average (1 min, sampled each bell; the machine is shared): p50 4.76, p99 7.94, max 10.03 at bell 94; max per game day [10.03,7.72,8.68,7.81]

## §13.4 criteria decided

| criterion | status | why |
|---|---|---|
| 1 | **pass** | the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs: 0 closed, 2018 closable after grace, 0 pending, 0 blocked |
| 2 | **pass** | every kind within its §5.5 budget (Reveal distribution reported) |
| 3 | **fail** | round_to_anchor_slots p99 4.00 > 2 slots; SkipQuiet per idle province-day p99 6 max 6 (0 idle days over 6), per churned day p99 27 max 33 (reported) |
| 4 | **pass** | 0 unrevealed inside 8 above-cap hold windows (expected); unrevealed by rule: bounced 2 |
| 5 | **n.a.** | no persona violated, but not every expected outcome was observed: settle_racer pending (never reached its test) |
| 6 | **pass** | in-run window: "p99 file 9.2 ms (answered only 10.2 ms), ingest->WS p99 0.72 s (ws-stamp; herald share p99 688 ms, delivery p99 35 ms), error rate 0.00000, gaps 0, 404 1533361, WS coverage 100.0%, recovery outside outages 0" |
| 7 | **n.a.** | reported, not gating (§13.4) |
| 8 | **pass** | no bad seal survived; 3 garbage and 4 bad-plaintext transits sent and due, each settled as bad-seal (codes {"bad_plaintext:5":4,"garbage:2":3}) |
| 9 | **pass** | every cohort closed within 24 bells |
| E | **exit-grade** | every adversary hold fired; release .so pinned; real rounds |

## Verdicts

- verify: **PASS** (fail codes ["PrefundedAddress"]; 47129 txs, read 1.42 s, verify 2.43 s — E6 wall time)
- tamper: 30/30 classes FAIL with their codes (30 built from the run; 15.48 s)
- `.so` pin: expected sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281 (the release build record; V2 checks the deployed program against it)
- load in-run (in-run, 5000 viewers, 24 game h): p99 file 9.22 ms, error rate 0 (0 errors / 3454703 requests + WS sessions), ingest → WS p99 0.72 s (ws-stamp; fold lag p99 2.80 s), WS coverage 1 outside 4 outage windows, stale retries 16000 / WS reconnects 4000 (outside outages 0 / 0), unavailable 7603 (13044233.11 ms), generator recovery true → **pass**

## CU per instruction kind (whole-transaction units)

| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |
|---|---|---|---|---|---|---|---|---|
| CreateSeason | 1 | 0 | 42348 | 42348 | 42348 | 70000 | no | 814 |
| InitShards | 6 | 0 | 39464 | 39464 | 39464 | 45000 | no | 562 |
| ConsumeGenesisSeed | 1 | 0 | 328965 | 328965 | 328965 | 345000 | no | 611 |
| AnnounceSeason | 1 | 0 | 12758 | 12758 | 12758 | 25000 | no | 387 |
| InitBeaconLogs | 1 | 0 | 74840 | 74840 | 74840 | 80000 | no | 825 |
| PostAnchor | 531 | 0 | 4655 | 336327 | 336327 | 345000 | no | 780 |
| PostAnchorMulti | 1446 | 0 | 377283 | 380337 | 380803 | 400000 | no | 1177 |
| PostSeed | 7704 | 0 | 337198 | 339485 | 339936 | 345000 | no | 781 |
| PostBeacon | 7360 | 304 | 330589 | 332732 | 332837 | 340000 | no | 645 |
| ArchiveAnchors | 2704 | 0 | 8842 | 12824 | 12824 | 60000 | no | 439 |
| CloseSeedCache | 2704 | 0 | 5792 | 5792 | 5792 | 6000 | no | 369 |
| OpenRing | 8 | 0 | 15052 | 15400 | 15400 | 30000 | no | 563 |
| ConsumeRingSeed | 4 | 0 | 330729 | 331211 | 331211 | 345000 | no | 646 |
| OpenProvince | 169 | 20 | 141122 | 147581 | 147976 | 220000 | no | 400 |
| FoldOccupancy | 1389 | 7 | 28733 | 28887 | 28887 | 30000 | no | 1090 |
| Join | 1000 | 10 | 12438 | 12438 | 12438 | 25000 | no | 534 |
| FileTicket | 1004 | 0 | 14297 | 16126 | 16174 | 17000 | no | 575 |
| SettleTicket | 1050 | 28 | 20279 | 21709 | 21866 | 40000 | no | 562 |
| Harvest | 1301 | 0 | 12080 | 16691 | 16696 | 17500 | no | 427 |
| Build | 2949 | 5 | 11745 | 18149 | 18159 | 22000 | no | 428 |
| Train | 1571 | 0 | 12220 | 16915 | 16915 | 17500 | no | 432 |
| Muster | 939 | 11 | 14815 | 18060 | 18936 | 25000 | no | 466 |
| Explore | 357 | 4 | 14035 | 18636 | 18703 | 20000 | no | 471 |
| SettleExplore | 357 | 0 | 8346 | 8658 | 8658 | 15000 | no | 396 |
| Depart | 538 | 14 | 16631 | 20725 | 20748 | 24500 | no | 712 |
| Reveal | 532 | 55 | 20076 | 22929 | 23648 | 26000 | no | 928 |
| SettleDeparture | 540 | 3 | 6272 | 6322 | 10129 | 48000 | no | 331 |
| SettleTransit | 538 | 32 | 60433 | 63015 | 63148 | 85000 | no | 859 |
| SweepPoolOwed | 536 | 9 | 5206 | 5206 | 5206 | 8000 | no | 330 |
| GatherClash | 2552 | 2 | 14270 | 37090 | 37952 | 49000 | no | 1197 |
| ResolveFromInputs | 2018 | 1 | 36679 | 42717 | 50976 | 290000 | no | 465 |
| SkipQuiet | 4010 | 22 | 34392 | 57519 | 65932 | 90000 + 30000/unit | no | 1160 |
| CloseArrivalDay | 245 | 0 | 5979 | 5979 | 5979 | 8000 | no | 371 |
| CloseArrivalSlot | 532 | 4 | 6496 | 6496 | 6496 | 8000 | no | 373 |

**Reveal CU distribution** (C4 input): n 532, p50 20076, p90 21780, p99 22929, max 23648.

## Keeper latencies (a slot is 8 game s)

Round → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).

| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |
|---|---|---|---|---|---|---|---|---|
| round_to_anchor_slots | 7312 | 1 | 4 | 153 | 2 | 15 | 39 | 5 s |
| s_to_first_cache_slots | 7296 | 1 | 1 | 144 | 2 | 9 | 11 | 5 s |
| anchor_to_last_reveal_slots | 524 | 0 | 1 | 4 | 4 | 0 | 8 | 30 s |
| s_to_resolve_slots | 2018 | 2 | 3 | 1720 | 8 | 17 | 25 | 60 s |
| close_to_resolve_slots | 2018 | 10 | 11 | 1728 | reported | 80 | 88 | reported |

Round → anchor from the publication instant, in slots: p50 1.88, p99 4.88, max 153.88.

### Catch-up (SkipQuiet transactions per province-day)

| province-days | n | p50 | p99 | max | over 6 |
|---|---|---|---|---|---|
| idle (roster unchanged) | 62 | 6 | 6 | 6 | 0 |
| churned | 414 | 8 | 27 | 33 | 269 |
| active (GATHER/CLASH, reported) | 2 | 7 | 7 | 7 | 2 |
| resident (resident action or nudge, reported) | 2 | 1 | 6 | 6 | 0 |
| all | 480 | 7 | 27 | 33 | – |

Resident actions 1863 (landed or refused), keeper-served nudges 1259, season-end flush SkipQuiet 0 (not counted).

**ClashInputs:** 0 closed, 2018 open: 2018 closable after grace, 0 pending, 0 blocked.

## Play

- records: {"ANCHOR":7312,"ANNOUNCE":1,"ARCHIVE":2704,"BEACON":7360,"BUILD":2949,"CAMP":395,"CLASH":2018,"CLOSE":6185,"DEPART":538,"DEPARTURE_SETTLED":542,"DIVERT":1067,"EXPLORE":357,"EXPLORE_RESULT":357,"FOLD":1389,"GATHER":2552,"GENESIS_SEED":1,"HARVEST":1301,"HOLDING_FINAL":385,"JOIN":1000,"MUSTER":939,"POOL_SWEEP":536,"PROVINCE_OPEN":169,"REVEAL":532,"RING_OPEN":8,"RING_SEED":4,"SEASON_CREATED":1,"SEED":7494,"SETTLE":1050,"SKIP":4010,"TICKET":1004,"TRAIN":1571,"TRANSIT_SETTLED":538}
- transits: {"outcome 1 seal 0":284,"outcome 3 seal 0":175,"outcome 4 seal 0":70,"outcome 6 seal 0":2,"outcome 8 seal 2":3,"outcome 8 seal 5":4}
- departs 538 (due 538), unsettled due: 0
- bad-seal codes: {"2":3,"5":4}
- failed transactions: {"Build: Insufficient":4,"Build: QueueFull":1,"CloseArrivalSlot: BadAccount":4,"Depart: NotResident":8,"Depart: TipTooLow":6,"Explore: NotResident":4,"FoldOccupancy: FoldStale":7,"GatherClash: LatchClosed":2,"Join: AlreadyDone":10,"Muster: NotResident":11,"OpenProvince: AlreadyDone":20,"PostBeacon: AlreadyDone":304,"ResolveFromInputs: OutOfOrder":1,"Reveal: AlreadyDone":38,"Reveal: BadAddress":3,"Reveal: WindowClosed":14,"SettleDeparture: AlreadyDone":3,"SettleTicket: NoTicket":28,"SettleTransit: TransitState":32,"SkipQuiet: NotQuiet":1,"SkipQuiet: OutOfOrder":21,"SweepPoolOwed: AlreadyDone":9}

### Failed transactions by class (reported, not gating)

531 failed: {"expected":81,"redundancy":416,"unclassified":6,"waste":28} (by cause {"a/b race":73,"adversary":3,"bot-policy":28,"bounded duplicate":22,"duplicate":343,"persona":20,"race":36}); unclassified 6.

| kind | error | n | class | cause |
|---|---|---|---|---|
| PostBeacon | AlreadyDone | 304 | redundancy | duplicate |
| Reveal | AlreadyDone | 38 | redundancy | a/b race |
| SettleTransit | TransitState | 32 | redundancy | a/b race |
| SettleTicket | NoTicket | 28 | expected | race |
| SkipQuiet | OutOfOrder | 21 | expected | bounded duplicate |
| OpenProvince | AlreadyDone | 20 | redundancy | duplicate |
| Reveal | WindowClosed | 14 | expected | persona |
| Muster | NotResident | 11 | waste | bot-policy |
| Join | AlreadyDone | 10 | redundancy | duplicate |
| SweepPoolOwed | AlreadyDone | 9 | redundancy | duplicate |
| Depart | NotResident | 8 | waste | bot-policy |
| FoldOccupancy | FoldStale | 7 | expected | race |
| Depart | TipTooLow | 6 | expected | persona |
| Build | Insufficient | 4 | waste | bot-policy |
| CloseArrivalSlot | BadAccount | 4 | unclassified |  |
| Explore | NotResident | 4 | waste | bot-policy |
| Reveal | BadAddress | 3 | expected | adversary |
| SettleDeparture | AlreadyDone | 3 | redundancy | a/b race |
| GatherClash | LatchClosed | 2 | unclassified |  |
| Build | QueueFull | 1 | waste | bot-policy |
| ResolveFromInputs | OutOfOrder | 1 | expected | bounded duplicate |
| SkipQuiet | NotQuiet | 1 | expected | race |

- no landed transaction for ≥ 20 slots after a bell started: 1 windows (295 quiet stretches inside a bell and 0 drain gaps after end_bell not listed)
  - slots 876–1027 (152 slots, 79 after bell 10 started; bells 9–11): **unexplained**
- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {"bounced":2}
- provinces 169; stuck province-bells: 0; cohorts open past 24 bells: 0

## Keepers, herald, bots

- keeper A: min reveal effective N in play 150 (≥ 150: true); status unanswered in 0 of 459 bells (0 timeouts; answer ms p99 7); last answered status {"alerts":0,"anchor_latency_slots_p99":1,"archived_bells":2704,"bell":458,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":19569235454,"n":32},"funders":{"lamports":2014498352460,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52491425539,"n":150}},"provinces_opened":169,"rings_complete":[0,1,2,3,4,5,6,7],"seed_latency_slots_p99":1,"spend_by_day":{"1":85417209,"2":402363343,"3":461648105},"sweeps_sent":374,"tickets_open":0}
- keeper B: min reveal effective N in play 150; status unanswered in 0 of 459 bells (0 timeouts); last answered status {"alerts":0,"anchor_latency_slots_p99":null,"archived_bells":0,"bell":458,"contested_bells":0,"pools":{"delay":{"effective_n":32,"floor":500000000,"lamports":23999635012,"n":32},"funders":{"lamports":2040000000000,"n":4},"reveal":{"effective_n":150,"floor":215076060,"lamports":52497170961,"n":150}},"provinces_opened":0,"rings_complete":[],"seed_latency_slots_p99":null,"spend_by_day":{"2":0,"3":59308},"sweeps_sent":0,"tickets_open":0}
- herald: fold lag slots p99 25, alarms 0
- personas violated: []

## Chaos and adversary

- chaos kills 19, restarts 19, crashes 0
- hold ticket at game 1789087512: 1 keys, 1000 milli, 1725 slots, above keeper cap true
- hold frontier-fund at game 1789108512: 2 keys, 1000 milli, 75 slots, above keeper cap true
- hold slots-below at game 1789110312: 25 keys, 1900 milli, 188 slots, above keeper cap false
- hold defence-pool at game 1789112112: 1 keys, 1000 milli, 225 slots, above keeper cap true
- hold slots-above at game 1789118312: 25 keys, 3000 milli, 188 slots, above keeper cap true
- hold anchor at game 1789146712: 1 keys, 1000 milli, 25 slots, above keeper cap true
- hold keeper-payers at game 1789175112: 20 keys, 3000 milli, 75 slots, above keeper cap true
- hold lag at game 1789203512: 2 keys, 1000 milli, 150 slots, above keeper cap true
- hold relay-payers at game 1789317112: 20 keys, 3000 milli, 75 slots, above keeper cap true

### Hold effects (held keys written inside the window / in as long again after it)

| kind | above cap | slots | keys | inside | after | first after |
|---|---|---|---|---|---|---|
| ticket | true | 350–2074 | 1 | 0 | 28 | 2075 |
| frontier-fund | true | 2975–3049 | 2 | 0 | 14 | 3050 |
| slots-below | false | 3200–3387 | 25 | 1 | 1 | 3502 |
| defence-pool | true | 3425–3649 | 1 | 0 | 3 | 3650 |
| slots-above | true | 4200–4387 | 25 | 0 | 0 | – |
| anchor | true | 7750–7774 | 1 | 0 | 0 | – |
| keeper-payers | true | 11300–11374 | 20 | 0 | 1 | 11377 |
| lag | true | 14850–14999 | 2 | 0 | 11 | 15000 |
| relay-payers | true | 29050–29124 | 20 | 0 | 4 | 29125 |

## §13.4 criteria (what this run decides)

```json
{
  "1_complete": {
    "clash_inputs": {
      "blocked": [],
      "closable_after_grace": 2018,
      "closed": 0,
      "open": 2018,
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
      "max": 23648.0,
      "n": 532,
      "p50": 20076.0,
      "p90": 21780.0,
      "p99": 22929.0
    }
  },
  "3_catch_up": {
    "active_province_days_over_6": [
      "(-1,2) day 2: 7",
      "(2,-2) day 0: 7"
    ],
    "churned_province_days_over_6": [
      "(-7,1) day 1: 9",
      "(-7,1) day 2: 14",
      "(-7,2) day 1: 9",
      "(-7,2) day 2: 11",
      "(-7,3) day 2: 11",
      "(-7,4) day 1: 7",
      "(-7,4) day 2: 12",
      "(-7,5) day 2: 7",
      "(-7,6) day 2: 12",
      "(-7,7) day 2: 27",
      "(-6,-1) day 2: 18",
      "(-6,0) day 1: 7",
      "(-6,0) day 2: 10",
      "(-6,1) day 2: 10",
      "(-6,2) day 2: 12",
      "(-6,3) day 1: 7",
      "(-6,3) day 2: 14",
      "(-6,4) day 2: 10",
      "(-6,5) day 2: 10",
      "(-6,6) day 1: 8",
      "(-6,6) day 2: 7",
      "(-6,7) day 1: 10",
      "(-6,7) day 2: 13",
      "(-5,-2) day 2: 8",
      "(-5,-1) day 1: 7",
      "(-5,-1) day 2: 10",
      "(-5,1) day 1: 7",
      "(-5,1) day 2: 9",
      "(-5,2) day 1: 8",
      "(-5,2) day 2: 9",
      "(-5,3) day 1: 8",
      "(-5,3) day 2: 10",
      "(-5,4) day 1: 8",
      "(-5,4) day 2: 13",
      "(-5,5) day 1: 18",
      "(-5,5) day 2: 7",
      "(-5,6) day 2: 10",
      "(-5,7) day 2: 13",
      "(-4,-2) day 1: 7",
      "(-4,-2) day 2: 9",
      "(-4,-1) day 1: 7",
      "(-4,-1) day 2: 10",
      "(-4,0) day 2: 7",
      "(-4,1) day 0: 11",
      "(-4,1) day 1: 21",
      "(-4,1) day 2: 17",
      "(-4,2) day 1: 7",
      "(-4,2) day 2: 9",
      "(-4,3) day 1: 9",
      "(-4,3) day 2: 7",
      "(-4,4) day 1: 8",
      "(-4,5) day 1: 14",
      "(-4,5) day 2: 17",
      "(-4,6) day 1: 7",
      "(-4,6) day 2: 13",
      "(-3,-4) day 2: 20",
      "(-3,-3) day 2: 14",
      "(-3,-2) day 1: 18",
      "(-3,-2) day 2: 15",
      "(-3,-1) day 1: 8",
      "(-3,-1) day 2: 11",
      "(-3,0) day 1: 11",
      "(-3,1) day 0: 8",
      "(-3,1) day 1: 12",
      "(-3,1) day 2: 14",
      "(-3,2) day 0: 8",
      "(-3,2) day 1: 13",
      "(-3,2) day 2: 23",
      "(-3,3) day 1: 10",
      "(-3,3) day 2: 10",
      "(-3,4) day 1: 7",
      "(-3,4) day 2: 9",
      "(-3,5) day 0: 10",
      "(-3,5) day 1: 32",
      "(-3,5) day 2: 16",
      "(-3,6) day 1: 9",
      "(-3,6) day 2: 12",
      "(-3,7) day 2: 7",
      "(-2,-5) day 2: 17",
      "(-2,-4) day 1: 7",
      "(-2,-4) day 2: 12",
      "(-2,-3) day 0: 13",
      "(-2,-3) day 1: 23",
      "(-2,-3) day 2: 24",
      "(-2,-2) day 0: 9",
      "(-2,-2) day 1: 17",
      "(-2,-2) day 2: 22",
      "(-2,-1) day 0: 16",
      "(-2,-1) day 1: 27",
      "(-2,-1) day 2: 26",
      "(-2,0) day 1: 7",
      "(-2,0) day 2: 7",
      "(-2,2) day 1: 7",
      "(-2,3) day 0: 7",
      "(-2,3) day 1: 16",
      "(-2,3) day 2: 12",
      "(-2,4) day 1: 12",
      "(-2,4) day 2: 10",
      "(-2,5) day 1: 7",
      "(-2,5) day 2: 7",
      "(-2,6) day 1: 7",
      "(-2,6) day 2: 9",
      "(-2,7) day 2: 9",
      "(-1,-6) day 2: 9",
      "(-1,-5) day 1: 8",
      "(-1,-4) day 2: 8",
      "(-1,-3) day 1: 7",
      "(-1,-3) day 2: 7",
      "(-1,-2) day 0: 7",
      "(-1,-2) day 1: 7",
      "(-1,-1) day 2: 8",
      "(-1,3) day 1: 8",
      "(-1,3) day 2: 10",
      "(-1,4) day 1: 13",
      "(-1,4) day 2: 13",
      "(-1,5) day 1: 11",
      "(-1,5) day 2: 9",
      "(-1,6) day 1: 13",
      "(-1,6) day 2: 20",
      "(-1,7) day 2: 9",
      "(0,-7) day 2: 7",
      "(0,-6) day 1: 8",
      "(0,-6) day 2: 14",
      "(0,-5) day 0: 10",
      "(0,-5) day 1: 17",
      "(0,-5) day 2: 27",
      "(0,-4) day 1: 12",
      "(0,-4) day 2: 10",
      "(0,-3) day 0: 8",
      "(0,-3) day 1: 15",
      "(0,-3) day 2: 13",
      "(0,-2) day 0: 8",
      "(0,2) day 1: 9",
      "(0,3) day 0: 7",
      "(0,3) day 1: 11",
      "(0,3) day 2: 14",
      "(0,4) day 0: 18",
      "(0,4) day 1: 23",
      "(0,4) day 2: 33",
      "(0,5) day 1: 7",
      "(0,5) day 2: 8",
      "(0,6) day 2: 11",
      "(0,7) day 2: 23",
      "(1,-7) day 2: 7",
      "(1,-6) day 1: 7",
      "(1,-6) day 2: 9",
      "(1,-5) day 2: 9",
      "(1,-4) day 1: 11",
      "(1,-4) day 2: 8",
      "(1,-3) day 0: 20",
      "(1,-3) day 1: 25",
      "(1,-3) day 2: 23",
      "(1,-2) day 0: 8",
      "(1,-2) day 1: 8",
      "(1,2) day 0: 25",
      "(1,2) day 1: 21",
      "(1,2) day 2: 22",
      "(1,3) day 0: 15",
      "(1,3) day 1: 21",
      "(1,3) day 2: 20",
      "(1,4) day 1: 10",
      "(1,4) day 2: 8",
      "(1,5) day 2: 11",
      "(1,6) day 2: 23",
      "(2,-7) day 1: 10",
      "(2,-7) day 2: 18",
      "(2,-6) day 1: 10",
      "(2,-6) day 2: 9",
      "(2,-5) day 0: 11",
      "(2,-5) day 1: 29",
      "(2,-5) day 2: 18",
      "(2,-4) day 0: 8",
      "(2,-4) day 2: 9",
      "(2,-3) day 0: 22",
      "(2,-3) day 1: 22",
      "(2,-3) day 2: 17",
      "(2,-2) day 2: 7",
      "(2,-1) day 2: 7",
      "(2,0) day 1: 7",
      "(2,1) day 0: 8",
      "(2,1) day 1: 13",
      "(2,1) day 2: 16",
      "(2,2) day 1: 12",
      "(2,2) day 2: 7",
      "(2,3) day 1: 9",
      "(2,3) day 2: 9",
      "(2,4) day 1: 13",
      "(2,4) day 2: 17",
      "(2,5) day 2: 8",
      "(3,-6) day 1: 8",
      "(3,-6) day 2: 14",
      "(3,-5) day 1: 7",
      "(3,-5) day 2: 8",
      "(3,-4) day 1: 9",
      "(3,-4) day 2: 9",
      "(3,-3) day 0: 7",
      "(3,-3) day 1: 9",
      "(3,-3) day 2: 8",
      "(3,-2) day 1: 9",
      "(3,-2) day 2: 21",
      "(3,-1) day 0: 8",
      "(3,-1) day 1: 13",
      "(3,-1) day 2: 14",
      "(3,0) day 1: 8",
      "(3,1) day 1: 12",
      "(3,1) day 2: 15",
      "(3,2) day 1: 9",
      "(3,2) day 2: 12",
      "(3,3) day 1: 7",
      "(3,3) day 2: 12",
      "(3,4) day 2: 8",
      "(4,-7) day 2: 7",
      "(4,-6) day 1: 7",
      "(4,-6) day 2: 11",
      "(4,-4) day 1: 10",
      "(4,-4) day 2: 10",
      "(4,-3) day 1: 11",
      "(4,-3) day 2: 12",
      "(4,-2) day 0: 11",
      "(4,-2) day 1: 10",
      "(4,-2) day 2: 18",
      "(4,-1) day 1: 9",
      "(4,0) day 1: 7",
      "(4,0) day 2: 10",
      "(4,1) day 0: 7",
      "(4,1) day 1: 18",
      "(4,1) day 2: 12",
      "(4,2) day 1: 18",
      "(4,3) day 2: 8",
      "(5,-6) day 1: 7",
      "(5,-6) day 2: 8",
      "(5,-5) day 0: 9",
      "(5,-5) day 1: 20",
      "(5,-5) day 2: 14",
      "(5,-4) day 1: 8",
      "(5,-4) day 2: 8",
      "(5,-3) day 1: 10",
      "(5,-3) day 2: 12",
      "(5,-2) day 0: 8",
      "(5,-2) day 1: 20",
      "(5,-2) day 2: 9",
      "(5,-1) day 1: 7",
      "(5,-1) day 2: 11",
      "(5,0) day 1: 7",
      "(5,1) day 1: 9",
      "(5,1) day 2: 9",
      "(5,2) day 2: 11",
      "(6,-6) day 2: 8",
      "(6,-5) day 1: 13",
      "(6,-5) day 2: 21",
      "(6,-4) day 1: 11",
      "(6,-4) day 2: 11",
      "(6,-3) day 1: 30",
      "(6,-3) day 2: 14",
      "(6,-2) day 1: 18",
      "(6,-2) day 2: 10",
      "(6,-1) day 1: 7",
      "(6,-1) day 2: 9",
      "(6,0) day 1: 12",
      "(6,0) day 2: 18",
      "(6,1) day 2: 9",
      "(7,-6) day 2: 7",
      "(7,-5) day 2: 9",
      "(7,-4) day 2: 10",
      "(7,-3) day 2: 9",
      "(7,-2) day 2: 8",
      "(7,-1) day 2: 8",
      "(7,0) day 1: 9",
      "(7,0) day 2: 15"
    ],
    "definition": "idle = the Province's roster_epoch unchanged over the day (post-states), no GATHER or CLASH of the province that day (§13.4 A1), and no resident action (Muster/Dissolve/Garrison/Explore/Depart naming it, landed or refused) or keeper-served nudge of it within the day or the 26 bells before (A1 as amended, v1.13); active = roster unchanged with a GATHER/CLASH (reported); resident = with a resident action or nudge (reported: each nudge is one split); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day; the SkipQuiet reaching end_bell (the season-end flush) is not counted",
    "idle_province_days_over_6": [],
    "nudges_served": 1259,
    "province_days_not_judged": 0,
    "province_post_states": 14459,
    "resident_actions": 1863,
    "resident_province_days_over_6": [],
    "season_end_flush": 0,
    "skip_txs_per_active_province_day": {
      "max": 7.0,
      "n": 2,
      "p50": 7.0,
      "p90": 7.0,
      "p99": 7.0
    },
    "skip_txs_per_churned_province_day": {
      "max": 33.0,
      "n": 414,
      "p50": 8.0,
      "p90": 18.0,
      "p99": 27.0
    },
    "skip_txs_per_idle_province_day": {
      "max": 6.0,
      "n": 62,
      "p50": 6.0,
      "p90": 6.0,
      "p99": 6.0
    },
    "skip_txs_per_province_day": {
      "max": 33.0,
      "n": 480,
      "p50": 7.0,
      "p90": 17.0,
      "p99": 27.0
    },
    "skip_txs_per_resident_province_day": {
      "max": 6.0,
      "n": 2,
      "p50": 1.0,
      "p90": 6.0,
      "p99": 6.0
    }
  },
  "3_latency_slots": {
    "anchor_to_last_reveal_game_secs": {
      "max": 32.0,
      "n": 524,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 8.0
    },
    "anchor_to_last_reveal_slots": {
      "max": 4.0,
      "n": 524,
      "p50": 0.0,
      "p90": 0.0,
      "p99": 1.0
    },
    "close_to_resolve_game_secs": {
      "max": 13824.0,
      "n": 2018,
      "p50": 80.0,
      "p90": 80.0,
      "p99": 88.0
    },
    "close_to_resolve_slots": {
      "max": 1728.0,
      "n": 2018,
      "p50": 10.0,
      "p90": 10.0,
      "p99": 11.0
    },
    "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
    "round_to_anchor_from_publication_slots": {
      "max": 153.875,
      "n": 7312,
      "p50": 1.875,
      "p90": 1.875,
      "p99": 4.875
    },
    "round_to_anchor_game_secs": {
      "max": 1231.0,
      "n": 7312,
      "p50": 15.0,
      "p90": 15.0,
      "p99": 39.0
    },
    "round_to_anchor_slots": {
      "max": 153.0,
      "n": 7312,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 4.0
    },
    "rounds_without_a_mapped_slot": 0,
    "s_to_first_cache_game_secs": {
      "max": 1153.0,
      "n": 7296,
      "p50": 9.0,
      "p90": 9.0,
      "p99": 11.0
    },
    "s_to_first_cache_slots": {
      "max": 144.0,
      "n": 7296,
      "p50": 1.0,
      "p90": 1.0,
      "p99": 1.0
    },
    "s_to_resolve_game_secs": {
      "max": 13761.0,
      "n": 2018,
      "p50": 17.0,
      "p90": 17.0,
      "p99": 25.0
    },
    "s_to_resolve_slots": {
      "max": 1720.0,
      "n": 2018,
      "p50": 2.0,
      "p90": 2.0,
      "p99": 3.0
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
          "coverage_samples": 3502,
          "outage_windows": [
            [
              1790735918013,
              1790735934810
            ],
            [
              1790736076494,
              1790736090396
            ],
            [
              1790736817609,
              1790736834383
            ],
            [
              1790739380911,
              1790739396026
            ]
          ],
          "recovery_outside_at_wall_ms": [],
          "samples_in_outages": 53,
          "samples_unanswered": 0,
          "stale_retries_inside": 16000,
          "stale_retries_outside": 0,
          "ws_coverage": 1.0,
          "ws_coverage_ok": true,
          "ws_coverage_target": 0.99,
          "ws_reconnects_inside": 4000,
          "ws_reconnects_outside": 0,
          "ws_viewers": 1000
        },
        "denominator": 3454703.0,
        "error_rate": 0.0,
        "error_rate_ok": true,
        "error_rate_target": 0.001,
        "errors": 0.0,
        "file_answered": 1920342,
        "generator_recovery": true,
        "generator_recovery_note": "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)",
        "ingest_lag_p99_s": 2.8000000000000003,
        "ingest_lag_samples": 3560,
        "ingest_measure": "ws-stamp",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "ingest_ok": true,
        "ingest_p99_s": 0.720896,
        "ingest_target_s": 2.0,
        "misses": [],
        "not_found": 1533361,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "p99_file_answered_ms": 10.24,
        "p99_file_ms": 9.216,
        "p99_file_ok": true,
        "p99_file_target_ms": 250.0,
        "pass": true,
        "recovery_ok": true,
        "requests": 3453703.0,
        "stale_retries": 16000,
        "stale_retries_outside": 0,
        "summary": "p99 file 9.2 ms (answered only 10.2 ms), ingest->WS p99 0.72 s (ws-stamp; herald share p99 688 ms, delivery p99 35 ms), error rate 0.00000, gaps 0, 404 1533361, WS coverage 100.0%, recovery outside outages 0",
        "unavailable": 7603,
        "unavailable_ms": 13044233.111,
        "ws_coverage": 1.0,
        "ws_coverage_ok": true,
        "ws_gaps": 0,
        "ws_gaps_ok": true,
        "ws_reconnects": 4000,
        "ws_reconnects_outside": 0,
        "ws_sessions": 1000.0,
        "ws_split": {
          "delivery_max_ms": 99.0,
          "delivery_p50_ms": 1.984,
          "delivery_p99_upper_ms": 34.816,
          "herald_max_ms": 699.0,
          "herald_p50_ms": 34.816,
          "herald_p99_upper_ms": 688.128,
          "send_stamped": 24905957
        },
        "ws_timed": 24905957.0
      }
    ]
  },
  "8_bad_seals": {
    "BadSealSurvived": false,
    "bad_seal_codes": {
      "2": 3,
      "5": 4
    },
    "marchbook": {
      "codes": {
        "bad_plaintext:5": 4,
        "garbage:2": 3
      },
      "due": {
        "bad_plaintext": 4,
        "garbage": 3
      },
      "not_bad_seal": []
    }
  },
  "9_tickets": {
    "cohorts_open_past_24_bells": []
  }
}
```
