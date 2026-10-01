# M1 exit: the Gate W6 nightlies on the exit commit

`scripts/m1-nightly.sh --no-build --run-id m1x-nightly-<n>` (test key, 100 bots, 1 game day at 100×, adversary, base 41500), one at a time, on `frontier/m1-integ` at the exit commit `1e5701b` (binaries as the exit season used them). 2026-10-01 03:36–05:11 JST, concurrent with the Gate W6 latency line (41200) and, until 03:57, the onboarding run (41700). Row E is never exit-grade on a test-key night; `frontier-fund` is hold-skipped in every one-day night (no ring opening). Criterion 5 reads n.a. at 100 bots (a persona is not exercised when its one ticket a day loses).

| run | night | txs | verify | tamper | `defence-pool` at bell | c5 | c8 | failed txs | load p50 / max | post-play load (1 game hour, 5,000 viewers) |
|---|---|---|---|---|---|---|---|---|---|---|
| `m1x-nightly-1` | green | 8,985 | PASS | 30/30 | 25 (1 re-arm) | n.a. | pass | 156 | 6.37 / 8.77 | file p99 7.2 ms, ingest → WS p99 0.08 s, errors 0 |
| `m1x-nightly-2` | **red** (the `load` step) | 8,694 | PASS | 30/30 | 24 (1 re-arm) | n.a. | pass | 107 | 6.53 / 8.93 | **file p99 524.3 ms > 250 ms** (answered 557.1 ms); ingest → WS p99 0.95 s (herald share 0.92 s); errors 0; WS coverage 100 % |
| `m1x-nightly-3` | green | 8,937 | PASS | 30/30 | 24 (2 re-arms) | n.a. | pass | 148 | 6.11 / 8.7 | file p99 29.7 ms, ingest → WS p99 0.26 s, errors 0 |
| `m1x-nightly-4` | green | 8,844 | PASS | 30/30 | 43 (6 re-arms) | n.a. | pass | 103 | 3.51 / 5.42 | file p99 180.2 ms, ingest → WS p99 0.59 s, errors 0 |
| `m1x-nightly-5` | green | 8,521 | PASS | 30/30 | 18 (0 re-arms) | n.a. | n.a. (none due) | 53 | 3.44 / 6.11 | file p99 12.8 ms, ingest → WS p99 0.29 s, errors 0 |

**Gate W6's "three consecutive nights green": met by nights 3, 4 and 5**; night 2 was red. Its only failing step was the post-play `load` (36 s of wall time at 100× after the chain paused): file p99 524 ms against 250 ms, with the herald's own share of the WS path at 0.92 s. The generator's one-second samples show a 1.4-s gap in that window (a stall of the machine or of the generator), no error and no outage. Night 4's post-play file p99 was 180 ms at a load average of 3.5. So this short post-play measurement is noisy on the shared machine (7–524 ms over these five nights). It is not criterion-6 evidence: criterion 6 is judged in-run over 24 game hours, and the exit season's figure is 8.7 ms (`m1-exit/criteria.md`). Nights 4 and 5 were added because night 2 was red (the gate asks for three consecutive).

Note on the runner: the gate script was edited (two nights appended) while it ran, so its shell went on into the new lines after night 3; a second runner that was started for the same two nights was refused by `check-ports` (ports 41510–41520 busy) before it started anything, and was stopped. Each night ran once, in order.
