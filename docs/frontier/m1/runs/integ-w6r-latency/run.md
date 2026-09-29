# integ-w6r-latency: the Gate W6 latency lines on frontier/m1-integ (wave 6 review response)

Lines as §12 writes them (base port 41000), plus one **extra** `verify` between `up` and `report && down` (not a §12 line: the Reveal sample of `m1/c4-v3` reads the verifier input, which only `verify` writes while the services are up). Run on `7b30594`, 11:41–15:04 JST, concurrent with the nightlies 2–3 (41500) and the `--spectator` onboarding run (41300).

```
L-item1 exit=0 (12163 s) 15:03:59 :: $S up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id w6-latency --base-port 41000 --chaos
L-item2 exit=0 (1 s)     15:04:00 :: $S verify --run-id w6-latency            (extra; PASS, 3,477 txs, read 0.12 s, verify 0.21 s)
L-item3 exit=0 (1 s)     15:04:00 :: $S report --run-id w6-latency && $S down --run-id w6-latency
```

Result (`report.md` beside this file): criterion 3 **pass** under v1.10 §13.4 — round → anchor p99 1 game s (2 slots), S → first cache 2 s (2 slots), anchor → last valid reveal 0 s, S → resolve 3 s (4 slots); close → resolve 65 game s (81.25 slots) reported (the structural Δ + delay + cache + resolve, DECISIONS Q3); criteria 1, 2, 4, 5, 8, 9 pass, 6 and 7 n.a.; 26 departs, 23 Reveals (p50 20,148 / p99 22,149 / max 22,149 whole-transaction units), 23 CLASH; idle province-days SkipQuiet p99 2, churned p99 19; chaos: 1 kill (herald), 1 restart, 0 crashes; keeper A's reveal pool floor 215,076,060, effective N 150 throughout; `down` stopped every service (no 410xx listener afterwards).
