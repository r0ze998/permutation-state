# m1-exit: the M1 exit season (E5, contract v1.13 §13.4)

The 7-day, 1,000-bot, real-round Mode A season on the exit commit, run by the main session with `scripts/m1-run-s7.sh --no-build --adversary --run-id m1-exit` (the §13.4 line plus the `.so` pin; `--viewer-window-hours 24`). Tree `frontier/m1-integ` = `codex/frontier` at **`1e5701b`** (code identical to `629d007`, the integ-W6t review head: the two later commits touch only `docs/`). Release `.so` **`d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`** (875,824 B), pinned with `--expect-so-sha256` and matched against W6T-1's release build record; V2 checks the deployed program against the pin. Real quicknet rounds from the approved archive (G0 1788998400, rounds 32065012..=32311012, `.claude/data/drand-archive-quicknet-g0-1788998400`; the stack's archive guard: last round needed well inside the archive, 26,079 s spare).

```
frontier-stack check-ports --config frontier-node/configs/w6-s7.toml --base-port 41000
caffeinate -i frontier-stack up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 --run-id m1-exit \
    --base-port 41000 --chaos --viewers 5000 --expect-so-sha256 d85e1bd7…2281 --adversary --viewer-window-hours 24
frontier-stack verify --run-id m1-exit
frontier-stack tamper --run-id m1-exit
frontier-stack report --run-id m1-exit
```

Run record (JST; the services stay up with the chain paused on 41000–41099 for the owner to spectate):

```
start          2026-09-30 17:45:23   head 1e5701b; .so pin match
check-ports    exit 0 (1 s)
up             exit 0 (31,136 s = 8 h 39 min; setup 55 s; 1,008 play bells + 26 drain; complete at slot 77,751)
verify         exit 0 (41 s)   PASS, 144,300 transactions (784 failed), read 4.6 s + verify 6.4 s
tamper         exit 0 (44 s)   base PASS; 30/30 classes FAIL with their codes, all built from the run
report         exit 0 (2 s)    criteria 1-6, 8, 9 pass; 7 n.a. (reported, not gating); row E exit-grade
finished       2026-10-01 02:25:47   s7.json "pass": true
```

Machine load average (1 min, sampled each bell): p50 4.73, p99 8.76, max 11.24 (bell 489). The machine was shared (browsers, other sessions).

## Environment as §13.4 requires

| requirement | as run |
|---|---|
| release SBPF v2 `.so`, pinned | `d85e1bd7…2281`, match (V2 checks the deployed program) |
| real quicknet rounds, ≈ 250k contiguous | the approved archive, 246,001 rounds, G0 1788998400 |
| scale, days, bots | 20×, 7 game days (1,008 bells + 26 drain), 1,000 bots with the 13 personas (5 bots each) |
| keepers A and B, ≥ 150 reveal payers, ≥ 32 delay payers, ≥ 4 funders | both: reveal effective N 150 (floor 215,076,060 lamports), delay 32, funders 4; keeper B with `backup_delay_slots = 8` |
| chaos: `kill -9` a random component every 2–6 game hours, restart after 0–60 game seconds | 43 kills, 43 restarts, 0 crashes: keeper-b 10, localnet 8, bots 6, herald 6, keeper-a 6, relay 4, drand-replay 3 |
| every adversary hold fires (v1.12) | all 9 kinds fired (14 windows): ticket, frontier-fund, slots-below ×6 (1,900 milli; 5 re-arms before a claim opened), defence-pool, slots-above, anchor, keeper-payers, lag, relay-payers; none `hold-skipped`. `slots-above` and `anchor` found no pending write (0 writes of their keys inside or after the window; `criteria.md` note 2, F-A6), so they fired but held nothing observable |
| 5,000 viewers (4,000 polling, 1,000 WS) for 24 game hours | simulated by the `frontier-viewers` load generator (not people or browsers); from game hour 1 to 25 (≈ 1 h 12 min real); 2 herald kills inside the window (2 outage windows) |
| ports 41000–41999 | 41000–41099 (localnet 41010/41011, drand-replay 41020, relay 41030/41033, herald 41040, keepers 41050/41051, bots 41070, viewers 41075) |

## §13.4 criteria

The table the report decided (`criteria.md` has it with the numbers; the full report is `report.md`).

| criterion | status |
|---|---|
| 1 season complete | **pass** |
| 2 CU per kind within §5.5 | **pass** |
| 3 keeper latencies and catch-up | **pass** |
| 4 liveness | **pass** |
| 5 personas | **pass** |
| 6 herald | **pass** |
| 7 bots vs `frontier-sim` | **n.a.** (reported, not gating; the stack report does not compute it) |
| 8 bad seals | **pass** |
| 9 tickets | **pass** |
| E environment | **exit-grade** |

## Personas (criterion 5; the fleet's 7 lifetimes merged)

Observed locally: forger, late_revealer, settle_racer, spammer, zero_tip. Judged by the chain (verify and criteria 4, 8, 9), each exercised: bad_plaintext (9 Departs, 18 `BadPlaintext` Reveal refusals, 9 transits settled bad-seal code 5), double_arrival (14 Departs), garbage_seal (4 Departs), min_tip (17 Departs; all 17 revealed by the keepers and settled, `criteria.md` note 1), prefunder, self_tip, squatter, ticket_holder. Violated: none. No honest march refused by rule.

## Reported, not gating

- **Failed transactions** 784 of 144,300: redundancy 589 (PostBeacon `AlreadyDone` 240, A/B races on Reveal 119, SettleDeparture 96, SettleTransit 95), expected 162 (SettleTicket `NoTicket` 66, persona refusals 74, adversary 8, SkipQuiet races 14), bot-policy waste 24, unclassified 9 (CloseArrivalSlot `BadAccount` 8, Reveal `TransitState` 1).
- **No-landing window:** one, slots 815–1029 (bells 8–11), listed by the detector as unexplained. It is the known stall of every 20× run: the `ticket` hold (above cap, 1,800 slots) and the persona holds fill the block at bells 8–10 (W6T-3 §8, O-M1-29). The latency p99s stay within target over 7 days (round → anchor max 154 slots, p99 1).
- **Largest single delays:** round → anchor max 154 slots (the same stall); S → resolve max 1,720 slots, consistent with the `ticket` hold, which holds one Province for 1,800 slots (a held Province cannot resolve).
- **Catch-up:** 122 idle province-days (p99 6, max 6, none over 6); churned 847 (p99 28, max 32), active 16 (max 13), resident 17 (max 9) reported; 2,861 keeper-served nudges and 4,459 resident actions attributed; 126 season-end flush SkipQuiet not counted.
- **Bots' herald reads:** 674 `observe: /h/season` errors in the fleet (its own reads during herald kills; the viewers' error rate is criterion 6's).
- **Keeper A status** answered in all 1,035 bells (p99 2 ms); keeper B likewise.

## Files

`report.md` (the full stack report, identical to the report section of `run-log.txt`), `criteria.md` (the decided table with the numbers), `verify.md` and `verify-summary.json`, `tamper.md`, `load-verdict.json` (criterion 6), `s7.json`, `so.sha256`, `run-log.txt` (the harness log, 48 KB). Not copied: `verify/input.json.gz` (122 MB), the herald checkpoint, ledgers and component logs; they stay in `frontier-node/.local/frontier/m1-exit/`.
