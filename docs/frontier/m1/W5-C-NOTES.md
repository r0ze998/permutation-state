# W5-C node-integration: notes

- **Unit:** W5-C (wave 5, hardening). **Branch:** `frontier/m1-W5-C`, cut from `frontier/m1-integ` at `31f1aa1` (contract v1.7). **Date:** 2026-09-28.
- **Brief (contract §11, wave 5):** fixes from the first smoke runs across keeper, herald, findex, fclient, agents, bots and itest; relay and JS SDK fixes; the herald's 5,000-viewer load test (p99 targets, §13.4 criterion 6); **G14** (100 bots × 2 game days through the program in LiteSVM, the native kernel re-run at every bell, test key; verifier PASS; one tampered log → FAIL).
- **Owned paths touched:** `frontier-node/crates/{fclient,findex,herald,itest,keeper,agents,bots}/**`, `permutation-gateway/test/frontier-sdk.test.mjs`, this file. Nothing outside §11's list. No manifest, lock, toolchain file or `.gitignore` changed. **No dependency request.**
- **Not done, by rule:** no push; no devnet or mainnet transaction; no download or install; no service on a fixed port (every test binds `127.0.0.1:0`; nothing in 41000–41999 was bound); `permutation-server/web/session.mjs` untouched; no file of the main tree or of another worktree written. W5-D's `fixtures/verify/march-program.json.gz` is **read** (by two tests), never written.
- **Logs:** `(session scratch)/scratchpad/w5c/` — `g14-run{1,2}.log`, `final-g14.log` and their `*-summary.json` and `*.json.gz` recordings, `load-run{3,4}.{log,json}`, `day-rec.log`, `rec96.log`, `ws-test2.txt`, `final-*.log`.

## 1. What landed

### 1.1 G14 (`frontier-node/crates/itest/tests/g14.rs`, `src/native.rs`, `src/gate.rs`)

`g14_two_game_days` (ignored; Gate W5 runs `cargo test --locked --release -p itest -- --include-ignored g14_`): the `inproc_day` system (the test-beacon program on `localnet` in process at 20×, the keeper with every role, the herald fold, the relay stand-in, 100 bots) over **288 bells of play + the 26-bell drain**, recorded as a verifier input. Pass conditions:

1. every `inproc_day` condition (moved from the test file to `itest::gate::day_conditions`, so the two gates judge a run the same way), plus a new one, **`no-valid-seal-routed`** (below);
2. **`native-kernel-every-bell`** (`itest::native::rerun`, independent of the herald and the verifier): every CLASH re-run with `clash::resolve_clash` from the chain's own bytes (the Province as the last transaction before the resolve left it, the gathered ClashInputs as the resolve read them, THE seed from the ANCHOR's `A` and the SEED naming it) — outcome digest, engagements, packed fates and the §22 input digest must all be the record's; every SKIP **replayed bell by bell** (`fclient::clash_model::skip_replay`: the day's camp check, the settle from the first due bell, `resolved_next`) with `clash::is_quiet` asked at **every** bell (the program asks the kernel at most once per transaction), the Province it leaves equal byte for byte (after the chained header) to the one the SKIP wrote, its camp spawns equal to the CAMP records, the quiet digest the §22 formula; and each Province's records covering every bell once, in order, from its opening to its final `resolved_next`;
3. **`verifier-pass`**: `verify_core::verify` (V1–V13) on the recording;
4. **`tampered-log-fails`**: the middle CLASH's outcome digest altered and the whole archive re-chained (the consistent forger) → FAIL `ClashReplayMismatch`; T1 (a DEPART dropped) on this recording → FAIL `ChainGap`.

`g14_recheck_a_recording` (ignored, `G14_DUMP=<file>`) re-runs 2–4 on a kept recording without replaying the days. `tests/native_rerun.rs` (5 fast tests, not ignored) runs the native re-run over the committed program recording and shows it is not blind: a Province a SKIP left with one byte changed, a CLASH digest altered, two hostile residents on one hex before a skip (loud bells), and a bell resolved twice are each caught.

### 1.2 Fixes from the first smoke runs (the in-process two-day run was the first smoke; W5-B's stack had not started when this unit ran)

| # | Finding [measured] | Fix | Test |
|---|---|---|---|
| F1 | **findex archive: one `F_FULLFSYNC` per record.** Sampling the first two-day run showed the herald's ingest spending most of its time in `sync_data` (macOS `fcntl(F_FULLFSYNC)`), 5–50 ms each under this machine's load; at stack scale the herald would fall behind a busy slot | `Archive::append` writes a batch's frames through one handle and syncs once per segment; **group commit**: `Archive::commit_every` (default 0 = durable on return, the keeper's and old tests' behaviour); the herald sets 1 s (`IngestCfg::archive_commit`) and commits before every fold checkpoint, so a checkpoint never covers a record the archive could lose. The crash rule is unchanged (the durable manifest names only synced frames and holds the older cursor; the source re-delivers) | findex `group_commit_is_durable_only_at_commits`; herald `a_crash_inside_a_group_commit_re_ingests_and_gives_the_same_files` (and the old restart test, run with commit 0) |
| F2 | **Bots asked the relay for ≈ 1,200 Departs per game day that the program refuses `Cooldown`**: `ready_host` checked `ready_bell` but not the stamina Depart charges (`march_stamina(32)` = 74, I-32; the program maps `NoStamina` to `Cooldown`). The committed recorded-herald fixture's two "planned marches" had 27 and 35 stamina: the test asserted marches the program would refuse | `agents::policy::can_depart` (ready and `Stamina::at(bell) ≥ march_stamina(MAX_PATH_STEPS)`); the recorded-herald test asserts every planned march has the stamina, and decides a wallet that plans nothing again with its roster rested (the recording's hosts had just marched); fixture re-recorded (32 wallets + personas, bell 96; `ITEST_RECORD_BELL`) | G14 run 1: `Depart Cooldown` 2,377 refusals; final run: 0 (169 sent) |
| F3 | **Relay stand-in answered a duplicate transaction `RelayRejected`** (the chain's refusal; W4-F finding 5) | replay cache as the gateway's `ReplayCache`: same authority signature (settle: same message) → `409 Duplicate`, claimed before simulation, released on any refusal | `relay_standin` (a refused transaction keeps no replay key); final G14: `Harvest Duplicate` 13 |
| F4 | **The verifier's `ValidSealUnrevealed` counted marches that settled bounced-unranked** (7 in run 1: squatter ×4, forger ×2, ticket_holder ×1 — outranked or the citizen's second arrival, no loss by rule); E5 criterion 4 wants it 0 outside held windows | not the verifier (W5-D's): the itest gate gets the real liveness condition **`no-valid-seal-routed`** (no valid seal settled `ROUTED`); finding for W5-D in §4 | G14, inproc_day |
| F5 | **Herald overview dormant flag (K11)** read only the Holding's cache bit, which the program refreshes on owner writes only (always clear then) | flag 2 by the kernel's rule at the bell's end: `last_owner_action + DORMANT_AFTER ≤ bell_end(b)` (or the cache bit) | herald `the_dormant_flag_follows_the_last_owner_action` |
| F6 | `/h/clash` lacked the Province the resolve read (W4-E R2) | `province_before_b64` in every clash report | herald fold test rebuilds each report's outcome from it with the shared builder |
| F7 | `frontier-viewers` panicked in every viewer when no province or ring was known (a season before genesis) | picks fall back to `/h/season` | first load run |

### 1.3 The herald's 5,000-viewer load test (`frontier-node/crates/herald/tests/load.rs`)

`herald_5000_viewers_meet_criterion_6` (ignored; `cargo test --locked --release -p herald --test load -- --ignored herald_5000`): **4,000 polling + 1,000 WS viewers** against a herald on `127.0.0.1:0` while it ingests **one game hour of the program's own season at 20×** (450 slots of 400 ms, paced by the recorded slots, through `runner::run` with its 200-ms poll), after folding the first 72 bells. The season is W5-D's committed program recording. Pass = §13.4 criterion 6: file p99 ≤ 250 ms, ingest → WS p99 ≤ 2 s, error rate < 0.1%.

To measure it: every WS diff now carries **`t`** (unix ms; the stamp is taken before the pull, so it covers the pull, the archive sync, the index, the fold and the fan-out); the generator records ingest → WS latency per message, reports `errorRate` (failed requests and WS sessions over all), and `viewers::targets` gives the criterion-6 verdict (`targetsMet`, `targetMisses`). `frontier-viewers --gate` exits 1 on a miss (for the stack's `load` step, W5-B).

Two WS fan-out fixes came out of the measurement: each diff's JSON (after `seq`) is serialised and base64-encoded **once** (`Diff::wire`), not once per socket; and a socket writes the diffs already queued in **one** write (≤ 256 diffs / 256 KiB).

| Run | file p50 / p99 / max | ingest → WS p50 / p99 / max | requests, WS messages | errors, gaps | load average |
|---|---|---|---|---|---|
| run 3 (before the fan-out fixes) | 0.26 / **69.6** / 438 ms | 147 / **1,638** / 1,718 ms | 148,186; 1,130,724 | 0, 0 | 65–112 |
| run 4 (after) | 0.26 / **30.7** / 438 ms | 127 / **1,049** / 1,083 ms | 148,227; 1,139,771 | 0, 0 | 73–78 |

[measured, release, generator and herald in one process on this Mac while other wave-5 units were building; 7,022 `404`s are immutable per-bell files for bells not folded yet, not errors.] Both runs meet criterion 6; the stack run (W5-B, `frontier-viewers --gate`) is the gate's own measurement.

### 1.4 Other hardening

- **Keeper restart mid-cohort (K11):** `a_restart_mid_cohort_rebuilds_and_settles_the_rest` — after the first settlement of a six-ticket cohort the keeper is dropped and a new one started from the chain alone; every site ends with the owner an uninterrupted keeper gives it (a second world), no displacement. Passes on the native model and on the test-beacon `.so` (2 tickets open at the restart).
- **Bots control listener (§10.3, K11):** `frontier-bots --control 127.0.0.1:41070` serves `GET /metrics` (the report), `GET /health`, `POST /stop` (the fleet stops at its next step); port rule 41000–41999, not reserved, `127.0.0.1` only; tokio only, no new crate.
- **fclient:** `clash_model::{skip_replay, settle_bell, next_due, finish_bell, quiet_digest, input_digest}` — the program's SkipQuiet per-bell model transcribed next to the shared ClashInput builder (the program stays normative; G14 compares against it at every bell).
- **JS SDK:** test that SettleTransit with its optional 14th account (`citizen`, the camp winner, v1.7 §23) classifies as a settle shape, and a read-only 14th or a 15th account is refused (the allowlist already derived it from the ABI; no SDK code changed).

## 2. Tests and gate lines run (this branch, `frontier-node` at the final tree)

| Command | Result [measured] |
|---|---|
| `cargo fmt --all -- --check` (frontier-node) | exit 0 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (frontier-node) | exit 0 |
| `cargo test --locked --release --workspace` (frontier-node) | exit 0: 271 passed, 0 failed, 10 ignored |
| `cargo test --locked --release -p itest -- --include-ignored g14_two` (**G14**) | **PASS** (below) |
| `cargo test --locked --release -p herald --test load -- --ignored herald_5000` | PASS (run 4 above) |
| `cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection` (Gate W4 line) | exit 0: `inproc_day`, `inproc_smoke_one_bell_with_the_test_key`, `lag_gate_in_process`, `crash_injection_every_journal_point` |
| `PSF_FRONTIER_SO=<test-beacon .so> cargo test --locked --release -p keeper --test play -- --include-ignored` (Gate W4 line) | exit 0: 7 passed |
| `cargo test --locked --release -p verify -- --include-ignored tamper_` (Gate W4 line) | exit 0: 34 passed |
| `PSF_FRONTIER_SO=<test-beacon .so> cargo test … -p keeper --test land a_restart` | PASS (program and model) |
| `(cd permutation-gateway && npm test)` | 505 pass, 0 fail, 0 skipped |
| `node permutation-gateway/scripts/sync-web-sdk.mjs --check` | up to date |

**G14, final run** (program: the test-beacon `.so` built from this tree, 1,032,688 B, sha256 `dc1281c3…09d9`; 23,606 slots, 349 s wall): JOIN 100/100, SETTLE 101, DEPART 168 (77 hosts), REVEAL 152, CLASH 222, SKIP 1,980, TRANSIT_SETTLED 168 one to one; 37 provinces resolved through bell 288; 35 bad seals all BAD_SEAL with the stock code; 13 min-tip marches all revealed by keepers; no valid seal routed (5 settled unrevealed, bounced by rule); herald 22,729 events, every clash report matches; **native kernel: 11,198 province-bells re-run — 222 CLASH matched, 1,980 SKIP over 10,976 bells kernel-quiet at every bell and reproduced byte for byte (326 skipped bells settled something, 21 camp spawns), 0.3 s**; **verifier PASS in 1.2 s** over 17,983 transactions (warnings: `ValidSealUnrevealed` 5, `PrefundedAddress` 1); **tampers: CLASH digest → FAIL `ClashReplayMismatch`, T1 → FAIL `ChainGap`**. Earlier runs on this branch: run 1 failed only the native input-digest check (the digest must be taken over the ClashInputs *before* the resolve; fixed in `native.rs`), run 2 passed (448 s).

**`inproc_day`** (Gate W4) on this tree, while re-recording the fixture: all 13 conditions PASS (100/100 joins, 73 departs settled one to one, 12 bad seals, 227 s).

Not run by this unit (other units' files or lines): the root-workspace, svm-tests, `build-frontier.sh --twice`, `build-wasm` and frontier-sim lines; Gate W5's `RELEASE_CHECK=1` svm line (W5-A), `mutate.sh` (W5-D), the `frontier-stack` lines (W5-B), the screens package (W5-E). **PENDING-OWNER in this unit: none.**

## 3. Deviations and choices

1. **The first smoke run is in process.** W5-B's `frontier-stack` did not exist when this unit ran (its worktree was at the wave base), so the "first smoke run" is G14's own two-day in-process run. The Node relay, the binaries and their CLI glue were not run as processes here; the relay stand-in stays in `itest` (W4-F deviation 1).
2. **`skip_replay` is a second transcription of the program's SkipQuiet model** (after W4's `clash_model::build`), placed in `fclient` so the verifier can use it for its per-bell SKIP check (W5-D's deferred item). G14 compares it with the program at every bell (10,976 bells, byte for byte), so a drift fails the gate. The move of the program's model into `frontier-abi` (W4-A D8) is still the durable fix. **Integrator note (wave-5 review, integ-W5r):** since `8871e2a` (W4-A D8) `fclient::clash_model::skip_replay` runs the program's own `frontier_abi::clash_model`, so G14's byte-for-byte SKIP comparison checks the model against itself; the independent checks are the kernel's per-bell `is_quiet` and `resolve_clash` and the verifier's own `verify::skip` (whose camp and terrain steps also go through the shared model, `skip.rs` note). "A drift fails the gate" no longer holds for the model code.
3. **Group commit trades durability for throughput in the herald only** (≤ 1 s of archived records lost on a crash, re-delivered by the source; the keeper and every other caller keep per-append durability by default).
4. **The recorded-herald test decides a wallet again with its roster rested** when the recording's hosts are too tired to march (they had just marched); the unrested decisions are still checked, and must never plan a march without the stamina.
5. **WS messages gained a field `t`** and the JSON key order changed (`seq` first); §8.4 lists the fields a client reads, all unchanged. **`/h/clash` gained `province_before_b64`** (W4-E R2).

## 4. Findings for other units and the integrator

- **W5-D (verifier):** (a) `ValidSealUnrevealed` warns for valid seals that settled **bounced-unranked** (no loss by rule): E5 criterion 4 needs it restricted to seals that settled `ROUTED` (G14's `no-valid-seal-routed` is that condition). (b) The deferred "SKIP quiet at every bell" can call `fclient::clash_model::skip_replay(province_before, b0, n)` and compare `after[64..]` with the SKIP's post-state, as `itest::native` does.
- **W5-B (stack):** `frontier-viewers --gate` (exit 1 on a criterion-6 miss) and its `ws.ingest_p99_ms` / `errorRate` / `targetsMet` fields; `frontier-bots --control 127.0.0.1:41070`; the herald commits its archive at most once a second (`IngestCfg::archive_commit`, 1 s).
- **Web (W5-E / W6-D):** WS diffs carry `t`; `/h/clash` carries `province_before_b64`.
- **Integrator:** no manifest or lock change. The agents fixture `herald-recorded` was re-recorded (32 wallets + personas at bell 96 of a 100-bell strict run, `ITEST_BELLS=100 ITEST_ALLOW_SMALL=1 ITEST_RECORD_BELL=96 FRONTIER_RECORD_FIXTURES=1`); if W5-A's final `.so` changes the chain, re-record it the same way.
- **Open, not done here:** `--source rpc` post-states (K11, until Geyser); the Node relay inside `itest` (a process-level run is W5-B's); the "DisbandStranded code for a live holding" (K11; the program's, `NotDormant` 46, unchanged).

## Links

- G14: `frontier-node/crates/itest/tests/g14.rs`, `frontier-node/crates/itest/src/native.rs`, `frontier-node/crates/itest/src/gate.rs`, `frontier-node/crates/itest/tests/native_rerun.rs`
- Skip replay: `frontier-node/crates/fclient/src/clash_model.rs`
- Load test: `frontier-node/crates/herald/tests/load.rs`, `frontier-node/crates/herald/src/viewers.rs`, `frontier-node/crates/herald/src/bin/viewers.rs`, WS: `frontier-node/crates/herald/src/ws.rs`
- Archive group commit: `frontier-node/crates/findex/src/archive.rs`, `frontier-node/crates/herald/src/runner.rs`
- Bots: `frontier-node/crates/agents/src/policy.rs` (`can_depart`), `frontier-node/crates/bots/src/control.rs`
- Contract: `docs/frontier/m1/M1-CONTRACT.md` v1.7 §11 (W5-C), §12 Gate W5, §13.3 G14, §13.4 criterion 6
