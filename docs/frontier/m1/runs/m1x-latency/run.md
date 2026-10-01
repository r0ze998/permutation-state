# m1x-latency: the Gate W6 latency line on the exit commit

```
frontier-stack check-ports --config frontier-node/configs/w6-latency.toml --base-port 41200
frontier-stack up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id m1x-latency --base-port 41200 --chaos
frontier-stack verify --run-id m1x-latency          # extra (the Reveal sample), not a §12 line
frontier-stack report --run-id m1x-latency && frontier-stack down --run-id m1x-latency
```

Base port 41200, not the line's 41000: 41000–41099 is the paused `m1-exit` stack. Tree `1e5701b` (the exit commit), binaries as the exit season used them.

Run record (JST): start 02:31:02; `up` complete at slot 30,412 at ≈ 05:54 (6 game hours of play at 2× plus the 26-bell drain at 20×); verify PASS (3,337 txs, 14 failed) 05:54:28; report && down exit 0 05:54:29. The gate shell that started `up` was detached at 04:33 (see `../m1x-nightlies.md`, runner note); `up` itself ran to completion and the last two lines were run by hand. Per-bell load average p50 4.18, p90 8.91, max 95.73 (bell 0, Gate W7 part A's `frontier-sim` tests); the line overlapped part A, the onboarding run, the W5 smoke and the five nightlies.

Result: criterion 3 **pass** at the game-second targets (§13.4, judged at 2×): round → anchor p99 **1 s**, S → first cache p99 **1 s**, anchor → last valid reveal p99 **0 s**, S → resolve p99 **3 s** (targets 5 / 5 / 30 / 60 s); close → resolve p99 65 s reported; idle province-days SkipQuiet p99 2 (0 over 6). Criteria 1, 2, 4, 8, 9 pass; 5 n.a. (forger never reached its test in 6 game hours); 6 n.a. (no viewer window in this line). 24 departs, 19 Reveals (p50 18,059, max 21,613 whole-transaction CU), 18 CLASH; 1 chaos kill (herald), 1 restart. Failed transactions 14 (expected 10, redundancy 4). Row E not exit-grade by design (test key, no pin, no adversary). The full report is `report.md`.
