# integ-W6t review: the nightlies and the W5 smoke

`scripts/m1-nightly.sh --no-build --run-id <id>` (test key, 100 bots, 1 game day at 100×, adversary, base 41500), one at a time, on each tree of this pass. Row E is never exit-grade on a test-key night; `frontier-fund` is hold-skipped in every one-day night (no ring opening). Criterion 5 reads n.a. when a persona is not exercised: at 100 bots each persona has one bot and one sponsored ticket a game day.

| run | tree | night | txs | verify | tamper | `defence-pool` at bell | c5 | c8 | failed txs | load p50 / max |
|---|---|---|---|---|---|---|---|---|---|---|
| `integ-w6t-rv-nightly-1` | `dcfece9` | green | 8,888 | PASS | 30/30 | 24 (1 re-arm) | n.a. | pass | 103 | 3.6 / 5.52 |
| `integ-w6t-rv-nightly-2` | `dcfece9` | green | 8,679 | PASS | 30/30 | 24 (1 re-arm) | n.a. | pass | 113 | 4.22 / 5.73 |
| `integ-w6t-rv-nightly-3` | `dcfece9` | green | 8,775 | PASS | 30/30 | 24 (1 re-arm) | n.a. | pass | 105 | 4.12 / 14.54 |
| `integ-w6t-rv2-nightly-1` | `d66fa99` | green | 8,720 | PASS | 30/30 | 67 (10 re-arms) | n.a. | n.a. | 117 | 4.23 / 5.77 |
| `integ-w6t-rv2-nightly-2` | `d66fa99` | green | 8,998 | PASS | 30/30 | 25 (1 re-arm) | n.a. | pass | 137 | 5.06 / 10.03 |
| `integ-w6t-rv2-nightly-3` | `d66fa99` | green | 9,067 | PASS | 30/30 | 24 (2 re-arms) | n.a. | pass | 114 | 4.17 / 5.95 |
| `integ-w6t-rv3-nightly-1` | `6b9f078` | green | 9,094 | PASS | 30/30 | 40 (5 re-arms) | n.a. | pass | 139 | 4.53 / 5.63 |
| `integ-w6t-rv3-nightly-2` | `6b9f078` | green | 8,320 | PASS | 30/30 | 18 (0 re-arms) | n.a. | n.a. | 63 | 4.32 / 6.8 |
| `integ-w6t-rv3-nightly-3` | `6b9f078` | green | 8,506 | PASS | 30/30 | 18 (0 re-arms) | n.a. | n.a. | 95 | 5.14 / 8.58 |
| `integ-w6t-rv4-nightly-1` | `629d007` | green | 8,988 | PASS | 30/30 | 24 (2 re-arms) | pass | pass | 129 | 5.35 / 7.67 |
| `integ-w6t-rv4-nightly-2` | `629d007` | green | 8,826 | PASS | 30/30 | 24 (2 re-arms) | n.a. | n.a. | 119 | 4.78 / 8.0 |
| `integ-w6t-rv4-nightly-3` | `629d007` | green | 8,482 | PASS | 30/30 | 18 (0 re-arms) | n.a. | n.a. | 72 | 5.99 / 7.59 |

The `dcfece9` nights ran `slots-below` at 1,500 (1 re-arm each); `d66fa99` and later at 1,900. At 100× a bell is 15 slots and the keepers' Reveals often landed before the anchor's recorded slot, so a window opens a claim less often: `rv2-nightly-1` needed 10 re-arms and `rv3-nightly-1` 5; the others ≤ 2. At 20× the rehearsal's first window opened one.

**W5 smoke on `629d007`** (`integ-w6t-rv2-w5-smoke`, base 41600, 16:45–17:05 JST, load 6.5 → 5.1): `check-ports`, `up --mode accel --beacon test-key --scale 100 --days 1 --bots 100`, `verify` (PASS, 7,206 txs), `tamper` (base PASS; required classes detected: 29 on the run, 1 on a fixture), `load --viewers 5000 --game-hours 1` (pass: file p99 12.3 ms, answered 14.3 ms; ingest → WS p99 0.29 s with herald share 0.30 s and delivery 0.008 s; error rate 0; WS coverage 100 %), `down`: all exit 0. An earlier smoke (`integ-w6t-rv-w5-smoke`, 11:22–11:41) was also all 0.

**Scripted onboarding** (`permutation-gateway/screens/live/run-onboarding.sh --base-port 41700`, 11:41–11:55): JA ok, EN ok.
