# W5-D verifier-tamper — notes

- **Unit:** W5-D (wave 5), `frontier-verify` / `verify-core` (M1 contract v1.7: §8.5, §11 wave 5 "T1–T22 on stack runs, `mutate.sh` over every check, fixtures regenerated from the nightly", §12 Gate W5 lines 2 and 5, §13.5 E6).
- **Branch:** `frontier/m1-W5-D`, cut from `frontier/m1-integ` at `31f1aa1` (contract v1.7).
- **Owned paths touched:** `frontier-node/crates/verify/**`, `frontier-node/fixtures/verify/**`, this file. Nothing outside them; **no dependency request** (no manifest, lock or toolchain file changed).
- **Also done here (wave note item 4, the verifier items the W4 integrator deferred to W5):** SKIP quiet at every bell (V7), V5 `MissingData` for an absent `T(arrive)` signature, `HoldingReplayMismatch` (the lazy holding replay).
- **Tags:** [measured] = run on this machine on 2026-09-28 (load average 40–75 from the other wave-5 units: wall times are noisy, CPU times are given); [design] = a rule as implemented.
- **Not done, by rule:** no push; no devnet/mainnet transaction; nothing installed or downloaded; no service started on a port (every test is in process, `localnet::InProcess`); the main tree, `session.mjs`, `codex/magicblock-playable` and `codex/v9-security` untouched. The rustfmt/clippy install is already recorded in `docs/frontier/DECISIONS.md` part A (and H3), so nothing was added there.

## 1. What landed

| Area | What |
|---|---|
| `tamper.rs` — T1–T22 on **any** run | Every class builder now returns `Result<Case, String>` (a run that lacks what a class needs says so instead of panicking). `tamper::classes()` lists the 22 required classes and six extra checks of the checks (T1b, T6b, T23, **T23b**, **H1**, V9a), each with its builders tried in order: the fixture's selection first, then a **run fallback** (below). `tamper::run_suite(&Input, jobs)` verifies the base run, builds and verifies every class on its own copy of the run (`jobs` at a time, a panicking builder counts as not applicable), and returns `SuiteReport` (`json()`, `markdown()`, `all_detected()`, `exit_code()`) |
| run fallbacks (new builders) | **T8 `t08_fill`**: the run has no displacement → the slot `i` a filling Reveal logged is changed (`QuotaSetMismatch`). **T13 `t13_moved`**: every Reveal of the program's runs lands *before* THE anchor (in-bell reveals; the integ-W4 review found T13 "not applicable" to the program recording) → one Reveal is moved to after its anchor with a Clock past `A + W`, just before the first transaction at or after the close, only where no transaction in between touches an account the Reveal wrote (`RevealAfterClose`; with V4 disabled the run PASSES, so the move is invisible to every other check). **T14 `t14_any`**: no FOLD → any landed program transaction duplicated. **T20 `t20_injected`**: the run has no defence claim → a ClaimDefence transaction of a keeper with no eligible slot and an amount of 50,000 lamports is appended (DEFENCE_CLAIM is not chained; `DefenceRefundMismatch`; PASSES with V13 disabled). `pick()` makes the fixture selections that index (`REVEAL[1]`, `ANCHOR[3]`, `PROVINCE_OPEN[5]` …) fall back to the last record of a shorter run |
| new checks of the checks | **T23b** a SkipQuiet's write-back forged (+25 troops to a resident in the Province the skip wrote and every later state; the skip's and every later skip's quiet digest recomputed; re-chained) → `ClashReplayMismatch`, only V7's new skip replay sees it. **H1** a Harvest's Holding forged (+1 food in the store it wrote and every later state; every later HARVEST digest recomputed; re-chained) → `HoldingReplayMismatch`, only V7's new holding replay sees it. T22's forger now keeps `pool_owed_delta` = the transaction's DIVERTs (the program recording has them), so T22 is a consistent forgery on the program's run too |
| `frontier-verify tamper` (CLI) | `frontier-verify tamper <the verify source options> [--jobs N] [--out DIR] [--json]` runs the suite over a fixture, a findex archive (+ finals or RPC) or an RPC/localnet run and writes `tamper.json` + `tamper.md`. Exit **0** every required class FAILS with its code; **1** a required class was missed (its tampered run PASSED or FAILED with other codes); **2** the base run does not PASS, or a required class could not be built on the run. `--save FILE` (verify and tamper) also writes the input it read as a fixture (`.gz`: gzip) — the path by which a stack or nightly run becomes a fixture |
| `skip.rs` + V7 — **SKIP quiet at every bell** | An independent transcription of what the program's SkipQuiet does bell by bell (`proc/clash.rs` v1.7): the day's camp check first, then the **kernel's `is_quiet` at every bell** (the program asks it once per transaction and relies on `trivially_quiet` ⇒ `is_quiet`), `settle_bell` from `next_due` (forfeits, musters, Leave, Spend with its march stamina, splits/merges through the kernel, garrison changes), `finish_bell` (`resolved_next`, `roster_epoch`, `n_entries`). V7 replays every SKIP run: the first bell that is not quiet is `SkipNotQuiet` (named with its bell), and the Province the skip wrote must equal the replay byte for byte except the chain header (`ClashReplayMismatch`, "skip write-back"). Before: quiet was checked at the run's first bell only and the write-back not at all |
| `holding.rs` + V7 — **HoldingReplayMismatch** | An independent transcription of W3-B's pinned Holding codec and owner touch (`proc/holding.rs`: tier-split settle with the new tier's base production, `touch_owner`, `commit_walls`; Build copy numbers; walls item into the site mirror; Train credits whole troops) over the kernel's `holding::Holding` and `catalog`. V7 replays **every Harvest, Build and Train** from the Holding the last transaction before it left, at the transaction's Clock: the Holding written (every byte but the chain header), the record's payload (HARVEST stores digest; BUILD item, cost digest, `done_at`; TRAIN unit, n, time) and a walls Build's site-mirror item must be the replay's; a kernel refusal of an action that landed is a FAIL. A transaction where another program instruction also writes that Holding is not replayed (none in M1's clients). The SETTLE founding check stays |
| V5 — **MissingData** | A Reveal whose seal the verifier cannot open (no transaction carried a verified signature of `T(arrive)`) is now `MissingData` (it passed silently); an unsettled march whose arrival bell ended a whole bell before the archive's last transaction, with no such signature, too (a bad seal could hide there). Settlements were already `MissingData` |
| fixtures (regenerated from the current program) | `march-program.json.gz` re-recorded by `itest::inproc_day` (strict, 100 bots × 144 bells + 26 drain bells, test key — the nightly's shape, in process) with `VERIFY_DUMP` on the wave-5 base program (test-beacon `.so` sha256 `dc1281c3…09d9`, built by `scripts/build-frontier.sh --features test-beacon` on this branch); `land-program.json` re-recorded from the same `.so` (`record_land_program`); `march-synth.json` regenerated (its skips now use the program's `settle_bell`/`finish_bell`, see §4) |
| `regen-fixtures.sh` | One command to regenerate all three fixtures from the current program and to check them (every fixture PASSES, the tamper suite detects every class on the program recording); `NIGHTLY_ARGS="<frontier-verify source options>"` also saves a stack or nightly run as `fixtures/verify/nightly.json.gz` — only if it PASSES and the suite detects every class on it |
| `mutate.sh` | Unchanged loop over all twelve `mutate-v*` features (every check but V10, the report); it now also runs the suite test on the program recording in each build (`VERIFY_RUN=<fixture>` runs it over another run) and documents `MUTATE_CHECKS` |
| tests | `tests/tamper.rs` 53 tests (was 34): the fixture classes, **every class on the program recording** (18 new `tamper_program_*`, T10 as codes-only, see §3), T23b, H1, the four fallbacks, and `tamper_suite_on_the_program_recording` (all 22 required classes built and detected; in a `mutate-vN` build exactly the classes of VN are missed). `tests/unit.rs` +2: `unopenable_seal_is_missing_data`, `skip_replay_checks_every_bell` (a camp that spawns at the first bell of a day on a resident's hex: the run's first bell is quiet, the second is not — the old first-bell-only check passed it). `tests/fixtures.rs`: the program recording must hold ≥ 100 HARVEST, ≥ 50 BUILD, ≥ 50 TRAIN |

## 2. The suite on the program's run [measured]

`frontier-verify tamper --fixture fixtures/verify/march-program.json.gz --test-key --ruleset 1ac11f85…` (the committed recording, 9,624 transactions): base **PASS**; **all 22 required classes detected**, every extra class detected; exit 0; 29.5 s CPU, 8.3 s wall with 8 jobs. (The first recording of this session, 9,641 transactions, gave the same table.)

| class | variant used on the program run | FAIL codes of the tampered run |
|---|---|---|
| T1 | drop the last Depart | ChainGap, HeadMismatch (+ the V5/V6/V7/V8 consequences) |
| T2 | flip a Reveal plaintext byte | RevealCommitMismatch |
| T3 | shift an unused anchor's A | SeedRoundRule |
| T4 | inject a second anchor | DuplicateAnchor |
| T5 | swap a cache signature | BeaconSigInvalid |
| T6 | set_account on a Province | HeadMismatch |
| T7 | valid seal settled as bad | VerdictDisagreesWithTlock |
| T8 | **fill slot** (the run has no displacement) | QuotaSetMismatch |
| T9 | departure mass | TransitMassMismatch |
| T10 | wrong drand key | BeaconSigInvalid |
| T11 | wrong ruleset hash | RulesetMismatch |
| T12 | truncate the last game day | ChainGap, HeadMismatch |
| T13 | **Reveal moved past A + W** (every Reveal lands before its anchor) | RevealAfterClose |
| T14 | duplicate a FOLD | DuplicateEvent |
| T15 | origin values | OriginValueMismatch |
| T16 | wrong genesis round | GenesisSeedRule, RingSeedRule |
| T17 | skip over an arrival | SkipOverArrival, ClashReplayMismatch |
| T18 | SETTLE score | TicketScoreMismatch |
| T19 | terrain digest | TerrainMismatch |
| T20 | **claim injected** (the run has no defence claim) | DefenceRefundMismatch |
| T21 | explore find | ExploreRollMismatch |
| T22 | bad seal logged as surviving | BadSealSurvived, VerdictDisagreesWithTlock |
| T1b, T6b, T23, T23b, H1, V9a (extra) | as named | ChainGap; ClashReplayMismatch ×3; HoldingReplayMismatch; NonCanonicalAddress |

## 3. The checks of the checks (`crates/verify/mutate.sh`)

Twelve builds (`mutate-v1 … v13`, no v10). In each: every class of the disabled check **PASSES** on the fixture it targets, every other class still FAILS with its code, and the suite over the program recording misses exactly the classes of the disabled check. New PASS-under-mutation cases: T23b and H1 (v7), T8-fill (v6), T13-moved (v4), T20-injected (v13), T22 on the program run (v5), and the program-run copies of T2, T3, T4, T5, T6, T7, T9, T11, T12, T15, T16, T17, T18, T19, T21, V9a, T1b.

**T10 on the program run is codes-only:** a wrong drand key also leaves every seal of the run unopenable, so with V3 disabled V5 answers `MissingData` (UNVERIFIABLE), not PASS — there is no consistent forgery of a wrong key on a run with seals. The test asserts it FAILS with `BeaconSigInvalid` in the default build and that the code is gone with `mutate-v3`; T10's full check of the check stays on the land fixture (no seals). T1 on the program run is likewise not a consistent forgery (dropping a Depart there breaks its Reveal, gather and settlement too); the suite still requires it to FAIL with `ChainGap`/`HeadMismatch`, and T1's check of the check stays on the synthetic fixture (a march still in flight at the end).

## 4. Fixtures (`frontier-node/fixtures/verify/`)

| file | source | size | verify [measured] |
|---|---|---|---|
| `land-program.json` | `record_land_program` on the test-beacon `.so` `dc1281c3…` (the W3 recording was of `6555facc…`, the integ-W3 build) | 333 txs | PASS |
| `march-synth.json` | `record_march_synth` (the verifier's own regression; skips now written by `skip::settle_bell`/`finish_bell` — before, the generator bumped `roster_epoch` on every skipped bell and never recounted `n_entries`, which the new skip replay rejects; the generator, not the program, was wrong: the program recordings agree with the replay on every skip) | 190 txs | PASS |
| `march-program.json.gz` | `itest::inproc_day` strict with `VERIFY_DUMP`, `.so` `dc1281c3…`, written by `regen-fixtures.sh` (the inproc test asserts all 11 of its conditions: zero stuck province-bells, every depart settled one to one, every bad seal destroyed with the stock code, …; the first recording of this session printed them: 64 departs, 12 bad seals, 11/11 PASS) | 9,624 txs (77 failed), 6.7 MB | PASS: 1,624 skips replayed bell by bell, 202 HARVEST + 110 BUILD + 94 TRAIN replayed, 79 clashes, 12 bad seals; 5 `ValidSealUnrevealed` warnings (liveness) |

`frontier-verify --fixture march-program.json.gz`: ≈ 1.15 s CPU both before and after this unit's checks (the skip and holding replays add < 0.1 s on 9.6k transactions).

**"Regenerated from the nightly":** `scripts/m1-nightly.sh` and `frontier-stack` are W5-B's this wave and did not exist on the base. The fixture of the nightly's shape (test key, 100 bots × 1 game day) was recorded in process by `itest::inproc_day`; a real nightly run becomes a fixture with `NIGHTLY_ARGS=… crates/verify/regen-fixtures.sh` (or `frontier-verify … --save`) once W5-B's stack exists.

## 5. For W5-B (`frontier-stack verify/tamper`)

- `frontier-stack verify --run-id X` = `frontier-verify --program <id> --season <n> --rpc http://127.0.0.1:<rpc port> --localnet --test-key [--ruleset <hex>] --out <run>/verify` (the localnet feed carries post-states; a plain public-RPC archive does not, and V6/V7/V11/V13 then say UNVERIFIABLE by design).
- `frontier-stack tamper --run-id X` = the same source options with `frontier-verify tamper … --out <run>/tamper` (or `--save <run>/run.json.gz` once, then `tamper --fixture <run>/run.json.gz`, which avoids a second RPC read). Exit 0 = all 22 classes FAIL with their codes; the per-class table is `tamper.md`/`tamper.json` (`classes[].status` ∈ detected / missed / not-applicable, `variant`, `fail_codes`). Library: `verify_core::tamper::run_suite(&Input, jobs)`.
- Memory: each class verifies its own copy of the run; `--jobs` bounds the copies in flight (default: cores, ≤ 8). For the 7-day exit run use `--jobs 2`–`4` unless the host has the RAM for 8 copies [design; not measured at that scale].

## 6. What was run (in `frontier-node/` of this worktree)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked -p verify --all-targets -- -D warnings` (and with `--features mutate-v1`, `mutate-v7`, `mutate-v13`) | exit 0 |
| `cargo test --locked --release -p verify` | exit 0: fixtures 5, tamper 53, unit 7 passed; 2 ignored (the recorders) |
| `cargo test --locked --release -p verify -- --include-ignored tamper_` (Gate W4 line) | exit 0 (53 tamper tests) |
| `cargo build --locked --release --workspace && crates/verify/mutate.sh` (Gate W5 line 2) | exit 0: all 12 `mutate-` builds 53/53 each (≈ 4.3 min) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace` (Gate W1 `frontier-node` line) | fmt, clippy: exit 0. `cargo test --locked --workspace --no-fail-fast`: 279 passed over 55 test binaries, **1 failed outside this unit**: `localnet::server::tests::ports_and_base58` asserts `check_port(41_010).is_ok()`, and W5-B's `frontier-localnet --port 41010` (pid 10745, from `worktrees/m1-W5-B`) was listening on 41010 during the run — environmental (a fixed port in a unit test), reported to the integrator, not changed here (not W5-D's file) |
| `crates/verify/regen-fixtures.sh` (builds the test-beacon `.so`, re-records all three fixtures, checks them) | exit 0 in 5 min 23 s (257 s CPU): `.so` `dc1281c3…` (incremental build), land 333 txs, synth 190 txs, program recording 9,624 txs; `cargo test -p verify` green; tamper suite on the recording: base PASS, every required class detected. (Its output lines were missing `--nocapture`; fixed after the run.) |
| `VERIFY_DUMP=… cargo test --locked --release -p itest --test inproc_day -- --include-ignored inproc_day` (first recording) | exit 0, 11/11 conditions PASS, 307 s wall |
| `frontier-verify tamper --fixture fixtures/verify/march-program.json.gz --test-key --ruleset 1ac11f85…` | exit 0 (§2) |

**Not run (not this unit's, or not yet possible):** the Gate W5 `frontier-stack` lines (`check-ports/up/verify/tamper/load/down`: W5-B's binary, not on the base), the svm `RELEASE_CHECK=1` suite (W5-A), `itest g14_` (W5-C), the screens package (W5-E). No `PENDING-OWNER` item for this unit (test key only; the real-round archive is W5-B's/W6's).

## 7. Deviations and open points

1. **T1–T22 "on stack runs"** are implemented for any run and run on the program's in-process run (`inproc_day`), not yet on a `frontier-stack` run: the stack is W5-B's and lands in the same wave (merge order W5-A, **W5-D**, W5-B). The stack's `tamper` subcommand only has to call `frontier-verify tamper` (§5).
2. **Fallback variants** (T8 fill, T13 moved, T14 any, T20 injected) are used when a run lacks the fixture's selection; each is a consistent forgery (its `mutate-` build PASSES on the program run). T8's contract wording is "change which slot a **displacement** hit": the fill variant is the same V6 judgement on a Reveal that filled; the displacement variant still runs on the synthetic fixture. A run with the `squatter` persona active should produce a displacement and use the original variant.
3. **Transcriptions, not shared code:** `verify::skip` and `verify::holding` transcribe the program's `model::{settle_bell, finish_bell, camp_check, next_due}` and `proc::holding::{read_holding, write_holding, settle_tiered, touch, copy_number}` independently (the verifier should not share the program's code). If W5-A moves the clash model into `frontier-abi` (W4-A D8), the verifier's copies should stay and a cross-vector test (program model = verifier transcription on the recorded Provinces) is the cheap guard; today the program recording is that test (1,624 skips and 406 owner actions agree on the committed recording, 1,615 and 401 on the first).
4. The holding replay skips a transaction in which another program instruction also writes the same Holding (none in M1's clients; a future batched client would make those unjudged, not failed).

## 8. Dependency requests

None (no manifest, lock, toolchain or `.gitignore` change).
