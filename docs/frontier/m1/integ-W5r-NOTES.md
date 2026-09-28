# integ-W5r: wave-5 review response and Gate W5 re-run

- **Role:** M1 integrator, wave 5, after the wave-5 review (all five units needs-fix). **Branch:** `frontier/m1-integ` (worktree `.claude/worktrees/m1-integ`), from `36eb803`. **Contract:** v1.8 → **v1.9** (§25). **Decisions:** `DECISIONS.md` part O. **Date:** 2026-09-28.
- **Owner decisions in force:** as in `integ-W5-NOTES.md` (O-M1-01…24 defaults; O-M1-12 items 1–3 approved, Agave ≥ 4.0 not approved; O-M1-17 local; O-M1-18 not approved). Nothing was installed, fetched or pushed; no devnet or mainnet transaction; the main tree, `codex/magicblock-playable`, `codex/v9-security` and `permutation-server/web/session.mjs` untouched. Services only on 41000–41099 (the gate's stack), stopped by its `down` line.
- **Logs:** `(session scratch)/scratchpad/integ-w5r/` — `cu1.log`, `cu2.log` (full-suite `PSF_CU_LOG`), `svm{1,2}.log`, `trace*.log`, `regen.log`, `rec-*.log` (herald re-recordings), `gate/` (script, run output, per-item logs).

## 1. Commits

| Commit | Unit reviewed | What |
|---|---|---|
| `9c86c6e` | W5-A | return-settle scan rework + G1 worst-fill test; SettleDeparture gate 48k; CloseSeason float caps 10/2/10 (on chain + `fclient::ix::close_season_float_all`); `cu-table.py` and MEASURED regenerated; limit ≤ gate + 5 % asserted; heap gate on every trace-build landing + `trace-sweep.sh`; G7 over four cases; 31f1aa1 golden digests and a three-word sort test; ABI tag v1.9 propagated |
| `24e2560` | W5-D | V7 Holding continuity + H1b; fail-closed skips; PANICKED status; `t13_forced`; V5 grace for Reveals; `run_suite_with_fixtures` (CLI `--fallback-fixtures`/`--strict`, stack uses it); `mutate.sh` relink and mutated-binary refusal; fixtures regenerated at the head, the wave-base recording kept as a second one; stronger unit tests |
| `36bdbba` | W5-C | findex index roll-back instead of rebuild; viewer targets on bucket upper bounds; bots control Origin/Host/timeouts; `Diff` equality |
| `9bdf9b4` | W5-E | sheet drag (click suppression window, pointer capture) with CDP touch and mouse tests; region null; two-tone boundaries with a computed contrast test; EN copy; quota in More; H in the map label; placeholder and doubled-word checks; `npm test` runs `logic.screen.mjs` |
| `a104ec7` | W5-B | report criteria verdicts (exit 1 on a failure); in-run viewer verdict; `load` post-play label, WS-stamp ingest, gaps, 404; holds on pending work + hold effects; archive range check and PENDING exit 4; EndSeason retry; bots `--control`; verify resumes; bell_secs; count flags; `--expect-so-sha256` |
| (this commit) | — | contract v1.9 §25, DECISIONS part O, unit-note corrections (W5-A G1 row, W5-C deviation 2), these notes |

## 2. Findings, one line each

(verified = reproduced or read in the code before fixing; "rebut" = not a defect, with the reason)

**W5-A**
- major SettleDeparture budget — verified (18,344 CU in the D8 log; worst fill measured at 25,148 live / 46,758 absent). Fixed: scan matches host id bits (13,240 / 43,083), gate 48k, test `g01_budget_settle_return_worst` at the table limit (O2).
- major CloseSeason float parts at 24 pairs — verified (the test measured only 10). Fixed by caps 10/2/10 enforced on chain; test at the caps with the client profile; arithmetic proof in `close_float_caps` (O3).
- major limits above the gate — verified (eight kinds). Resolved by amending §10.2 (≤ 5 % of the gate above it) with a test (O1).
- minor sorts in shared helpers — verified as a test gap (no defect). Golden digests recorded on the 31f1aa1 kernel; random three-word sort keys.
- minor G7 one scenario — verified. Four cases incl. a second arrival, a bounce, a destruction, real origin resolve.
- minor heap only from scratch logs — verified. Harness heap gate + committed sweep (49 kinds, max 15,320 B).
- minor D8 / R1–R3 — no code action; accepted in O5.
- missing items: G1 return settle, float G1, MEASURED generator, limit assertion, sort check — all added; D8 was done in `8871e2a`.

**W5-B**
- major in-run window never judged — verified. `judge_in_run` writes `load/in-run.verdict.json` with fold lag sampled each second.
- major load on an idle chain — verified. Labelled post-play, not criterion-6 evidence; judge uses WS stamps, gaps, upper bounds; 404 classified (§8.4 per-bell files).
- major report decides nothing — verified. `decide()` per criterion, exit 1 on a failure, nightly shows it.
- major holds not tied to situations — verified. Slot, lag and ticket holds now aim at pending work and the report shows each hold's effect; frontier-fund and defence-pool timing remains open (O11).
- major archive path never exercised — verified. Range check (G0, end) added and tested; an end-to-end archive run was **not** possible (no manifest at 19:11 JST; a test-key archive cannot exercise the quicknet pin or the herald without `--test-key`) → W6 (O11).
- minor EndSeason once — fixed (bounded retry in a task, AlreadyDone accepted).
- minor PENDING-OWNER label for the archive — fixed (PENDING, exit 4).
- minor builder panic as fallback — fixed (PANICKED fails; the stack now runs the library suite; test asserts every class on the run for the program recording).
- minor `.so` trust root — fixed optionally (`--expect-so-sha256`; V2 then checks the pin). Not made mandatory: no committed release hash exists yet (the release `.so` changes with W6-B's Phase B).
- minor small defects (1)–(5) — all fixed.

**W5-C**
- major full re-index after a crash in a group commit — verified (Findex::open rebuilt from zero). Fixed by `Index::truncate_to`; test proves no rebuild and identical tables.
- minor gate filter — already fixed in `ce341f0`; no action.
- minor histogram lower bound — fixed (upper bound for targets; test with a 252-ms p99).
- minor fixture drives no real march — verified (0 planned); re-recorded at bells 60, 120, 140: still 0 unrested marches; **not fixed**, the bell-96 recording kept → W6-C (O11).
- minor bots control — fixed (403 for Origin / foreign Host / no Host; read timeout; accept back-off; test).
- minor `Diff` equality — fixed (content equality; test).
- missing: process-level smoke → the Gate W5 stack run; independence of G14's SKIP check → W5-C notes amended; 24-hour load → W6/W7.

**W5-D**
- major T13 not applicable on other runs — verified on the integ-head recording. `t13_forced` added; suite asserted on two recordings.
- major Holding not protected at non-owner writes — verified by reading `owner_actions`. Continuity check + H1b (FAILs; PASSes under mutate-v7).
- major mutated binary left behind — verified. Relink trap + refusal to run (checked by hand: exit 2, then 0 after relink).
- major stack tamper not using the suite — verified. The stack runs `run_suite_with_fixtures`.
- minors: Reveal grace (fixed, test); fail-open skips (fixed, MissingData); skip.rs bounds and verifier panics (fixed); fixtures regenerated (done, `regen-fixtures.sh` honours `CARGO_TARGET_DIR`); weak assertions (fixed; a missed extra class fails the CLI; message grammar).
- missing: E6 wall time and memory → W6/W7 (O11).

**W5-E**
- major drag swallows the next tap — verified: the new CDP touch test fails on the old code and passes now; the mouse-drag test likewise.
- major "region null" — verified in the bell sheet. Fixed in `bell.mjs` and the controller; the matrix now fails on placeholders.
- major stroke contrast — verified (ink at 55 % gave 1.8:1 on purple; even opaque ink 2.8:1). Two-tone boundary; test computes ≥ 3:1 for every fill, bare and veiled.
- minors: EN plural and "Dormant in | in" — fixed (and a doubled-word check); logic tests without a browser — fixed via `npm test`; quota on phones — fixed with a test; D1/D2 and the aria-label — H added to `index.html` (spectate and practice have no holding for H), D1/D2 recorded as accepted deviations (O10).
- missing: pattern at province LOD and §13.2 determinism — recorded in O10 (open with the goldens, W6-D).

## 3. Measurements (this window)

| What | Result |
|---|---|
| svm `RELEASE_CHECK=1 ./run.sh --release` (after all program changes) | 244 passed, 0 failed, 4 ignored by design |
| `cu-table.py cu2.log --check` | exit 0 (MEASURED = the full-suite maxima) |
| trace sweep (`trace-sweep.sh`) | 26,295 transactions; 0 heap-gate failures; 49 kinds; max 15,320 B |
| release / test-beacon `.so` | 874,120 B / 874,624 B (placeholder 884,736 still covers) |
| return settle worst (48 entries, 45 foreign, last three) | live 13,240 CU; absent 43,083 CU (release and test-beacon) |
| CloseSeason at the caps (distinct recipients, client profile) | part 8: 29,759 CU, 1,177 B; part 9 (2 pairs): 10,656 CU, 649 B; part 10: 35,180 CU, 1,177 B |
| G7 cases | outcome digests identical held/unheld in all four; case 1 = the review's `cbcafdfa…4e7e` |
| kernel golden digests (M1 rules) | EMPTY `679c2abe…163b`, ROOM `de82d520…e98b`, identical on `31f1aa1` and at the head |
| verify tests | 5 + 56 + 8 green; mutate.sh v4, v7 green (full run in the gate) |
| `inproc_day` recording at the head (test-beacon `094dc2d6…`) | 12/12 PASS; 11,471 herald events |
| screens | 51/51 (72 shots with the new checks) |
| gateway `npm test` | 515/515 |

## 4. Gate W5 re-run

Script `gate/gate-w5.sh` (the same as integ-W5's: the §12 preamble, every Gate W1–W4 item, then the nine Gate W5 lines, each logged separately). **Run 1 on `a104ec7` (19:18–20:12 JST): 36/36 items exit 0; `PENDING_OWNER:` empty.** No other run was needed.

| # | Item | Result |
|---|---|---|
| 1–13 | Gate W1 lines | all exit 0 (gateway `npm test` 515/515 with the logic tests; civilization tests; `session.mjs` / `permutation-chain` unchanged since `d95fa25`) |
| 14–20 | Gate W2 lines | all exit 0: release `.so` 874,120 B, `file_sha256 1b1968af…06fc`, both builds identical; `frontier.wasm` 190,979 B, fresh |
| 21–23 | Gate W3 lines | all exit 0 |
| 24–27 | Gate W4 lines | all exit 0 (svm full; `inproc_`, `lag_gate`, `crash_injection`; keeper play on the test-beacon `.so`; verify `tamper_`) |
| 28 | `(cd permutation-frontier/svm-tests && RELEASE_CHECK=1 ./run.sh --release)` | exit 0: 244 passed, 0 failed, 4 ignored by design |
| 29 | `cargo build --release --workspace && crates/verify/mutate.sh` | exit 0: every `mutate-v*` build lets its tampers PASS (H1b under v7, `t13_forced` under v4 included); the default binary relinked at the end |
| 30 | `$S check-ports --config frontier-node/configs/w5-smoke.toml` | exit 0 |
| 31 | `$S up --mode accel --beacon test-key --scale 100 --days 1 --bots 100 --run-id w5-smoke --base-port 41000` | exit 0 (1,073 s) |
| 32 | `$S verify --run-id w5-smoke && $S tamper --run-id w5-smoke` | exit 0, exit 0: verify **PASS** (6,266 txs, 10 failed, no finding); tamper: every class detected; **26 classes judged on the run** (was 20), 3 on the `march-synth` fixture and labelled (T7, T9, T15: the run's three marches were all bad seals); T23b, H1 and H1b now counted |
| 33 | `$S load --run-id w5-smoke --viewers 5000 --game-hours 1` | exit 0, labelled **post-play** (not criterion-6 evidence): p99 file (bucket upper bound) 27.6 ms, ingest → WS p99 0.21 s from the WS stamps, error rate 0, WS gaps 0, 161 × 404 counted apart |
| 34 | `$S down --run-id w5-smoke` | exit 0; nothing listens on 41xxx afterwards |
| 35 | `(cd frontier-node && cargo test --locked --release -p itest -- --include-ignored g14_)` | exit 0 (305 s) |
| 36 | `(cd permutation-gateway/screens && npm ci && node --test *.screen.mjs)` | exit 0: 51/51 (72 shots with the placeholder and doubled-word checks, 8 interaction incl. the touch and mouse drags, 7 logic) |

**Pass conditions (§12, Gate W5):** G1–G14 green with no PENDING test (items 28, 24–26, 35); verify PASS (item 32); all 22 tamper classes FAIL with their codes (item 32, 19 of the 22 on the run itself; items 27 and the two recordings in the verify tests); every `mutate-<check>` build lets its tamper PASS (item 29); herald targets met (item 33 post-play; W5-C's in-process load test in item 18); screenshot matrix green (item 36); Phase B decision recorded (N10). Earlier gates' conditions still hold: RFI max 327,609 CU (≤ 340k), heap max 15,320 B (trace sweep), Reveal 25,155 CU (≤ 26k), wasm 190,979 B.

**Extra (not a gate line):** `frontier-stack report --run-id w5-smoke` on the gate's run, exit 0: criteria 1, 2, 4, 5, 8, 9 pass; 3 n.a. (100×), 6 n.a. (no in-run window in the smoke), 7 n.a. (reported). Output in `extra-report.md`.

## 5. Open (not gate items)

See DECISIONS O11. In short: the archive end-to-end run and real-round smoke (W6, once `manifest.json` exists; the archive held only `partial-32065012-32311012.bin` at 19:11 JST, no fetch started here); ring-opening / claim-grace timing of two holds (W6-C); ClashInputs closability and idle-vs-churned skips in the report (W6-A); the herald recording that drives unrested marches (W6-C); E6 verifier wall time (W6/W7); three nightlies (Gate W6).
