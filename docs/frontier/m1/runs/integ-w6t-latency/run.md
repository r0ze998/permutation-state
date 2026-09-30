# integ-w6t-latency: the Gate W6 latency line on the merged `frontier/m1-integ`

```
frontier-stack up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id integ-w6t-latency --base-port 41100 --chaos
frontier-stack verify --run-id integ-w6t-latency          # extra (the Reveal sample), not a §12 line
frontier-stack report --run-id integ-w6t-latency && frontier-stack down --run-id integ-w6t-latency
```

Base port 41100, not the line's 41000: 41000–41099 is the paused `w6-s7` stack. Tree `aeda792` at the start (the keeper, bots and stack binaries built then; the later integration commits changed only the verifier's T17/V7, rustfmt layout and clippy lints, and `report`/`verify` ran from the rebuilt `36386b0` binaries).

Run record (JST): start 05:48:49 (load 5.90 3.88 3.52), up exit 0 09:13:13 (load 2.77 4.19 4.45), verify PASS (3,316 txs), report && down exit 0 09:13:15. Per-bell load average p50 4.61, max 74.29 at bell 0 (Gate part A's frontier-sim tests); the line overlapped part A, R3, the nightlies, the W5 smoke, the onboarding run and R5.

Result: criterion 3 **pass** — round → anchor p99 1 game s (1 slot), S → first cache 1 s, anchor → last reveal 1 s, S → resolve 3 s (targets 5 / 5 / 30 / 60 s); close → resolve 65 s reported; idle province-days p99 2 SkipQuiet (0 over 6). Criteria 1, 2, 4, 5, 8, 9 pass; 24 departs, 19 Reveals, 17 CLASH; 14 failed transactions (expected 9, redundancy 4, unclassified 1); 1 chaos kill (herald), 1 restart. Integ-W6r's run on 7dcacdf: 1 / 2 / 0 / 3 s.
