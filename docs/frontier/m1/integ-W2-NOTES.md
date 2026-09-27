# integ-W2: wave-2 merge, integration window and Gate W2

- **Role:** M1 integrator, wave 2. **Branch:** `frontier/m1-integ` (worktree `.claude/worktrees/m1-integ`), wave base `ed31438`. **Contract:** M1-CONTRACT v1.2 §3.4, §11 (wave 2), §12 (Gate W2), I-55. **Date:** 2026-09-28.
- **Owner decisions in force (relayed by the M1 workflow, 2026-09-27):** O-M1-01…24 working defaults accepted; **O-M1-12 not approved** (no wasm32 target, Playwright, drand archive or Agave install was run); O-M1-17 fix locally, no push; O-M1-18 not approved. The 1.95.0 `rustfmt`/`clippy` components were installed by the main session with the owner's OK: recorded once in `DECISIONS.md` part A (W2-A's row).
- **Not done, by rule:** no push, no devnet or mainnet transaction, no service started (every test binds `127.0.0.1:0`; the I-45 validator drill was not re-run), no file of the main tree, `codex/magicblock-playable` or `codex/v9-security` touched, `permutation-server/web/session.mjs` untouched.
- **Logs:** `(session scratch)/scratchpad/integ-w2/` — `gate/gate-w2.sh`, `gate/gate-final.txt`, `gate/logs/item1..19.log`; the pre-checks (`pre-*.log`) and the CU comparison (`smoke-*-fixed.txt`).

## 1. Merges (contract order W2-A, W2-B, W2-C, W2-F, W2-D, W2-E; all `--no-ff`)

| Step | Unit (head) | Merge commit | Integrator-owned files left out and re-applied |
|---|---|---|---|
| 1 | W2-A program-core (`199f0bb`) | `f8cd4c5` | root `Cargo.toml`/`Cargo.lock` → `42b766c` |
| 2 | W2-B svm-harness (`da4d83f`) | `5c5aa47` | `svm-tests/{Cargo.lock, rust-toolchain.toml, .cargo/config.toml}`, `svm-tests/probe/Cargo.lock` → `f672e32` |
| 3 | W2-C localnet-complete (`6fa1667`) | `e61cfd2` | `localnet`/`drand-replay` dependency edges, `frontier-node/Cargo.lock` → `e19abf2` |
| 4 | W2-F keeper-core (`e863867`) | `6166270` | `frontier-node/Cargo.toml`, `findex`/`keeper` dependency sections, `frontier-node/Cargo.lock` → `c7a4c24` |
| 5 | W2-D relay-sdk (`71fa75c`) | `3c64df0` | none requested |
| 6 | W2-E web-foundation (`c151efa`) | `fbae8dc` | `frontier-wasm/{Cargo.lock, rust-toolchain.toml, .cargo/config.toml}` → `adcad69` |

`DECISIONS.md`: all six units added the same fact (the rustfmt/clippy install). W2-A's part-A row is kept; the other five copies (W2-F's as a new part H) were dropped at their merges (two textual conflicts, W2-C and W2-D, resolved that way).

## 2. Dependency requests (§3.4, I-55)

| Request | Applied | Check |
|---|---|---|
| W2-A: root members += `permutation-frontier`; exclude += `permutation-frontier/svm-tests`, `frontier-wasm`; `overflow-checks` for `permutation-frontier` and `frontier-abi`; 20 lock packages (ark 0.5.0 family and deps) | yes (`42b766c`) | lock regenerated offline, byte-identical to the unit's; `cargo metadata --locked` |
| W2-B R1/R2: svm-tests and probe workspace files, pinned set (litesvm =0.16.0 …) | yes (`f672e32`) | `cargo metadata --locked --offline` on both locks unchanged |
| W2-B R3 (root exclude) | yes, with W2-A's | — |
| W2-B R4 (CI runs `svm-tests/run.sh --release`) | already in `ci.yml` (W1-D's `frontier-program` job) | — |
| W2-C: `sha2`, `hex` edges in `localnet` and `drand-replay` | yes (`e19abf2`) | lock byte-identical to the unit's |
| W2-C gate ordering (`npm ci` first, `PSF_REQUIRE_WEB3=1`) | not applied to §12 (the gate is run as written); instead the gate logs were checked: `web3.js conformance NOT RUN` appears **0 times** in items 10 and 18, and `web3_js_and_send_mjs_against_a_live_node` ran and passed in both (the gateway `node_modules` existed) | grep of the logs |
| W2-F R1–R4: `rusqlite =0.40.2` (bundled), `solana-program-runtime =4.2.2` (keeper dev-dep), findex/keeper dependency sections | yes (`c7a4c24`) | lock = W2-F's lock + W2-C's edges; `cargo check --locked --offline --workspace --all-targets` |
| W2-D | nothing requested | — |
| W2-E R1/R2 | yes (`adcad69`, R2 with W2-A's) | `cargo metadata --locked --offline` |
| W2-E R3 (direct devDependency `@noble/curves 1.9.7`) | **not applied** (optional): the version is locked transitively through `@solana/web3.js` and `vendor-noble.mjs` refuses any other | — |
| W2-E R4 (frontier-wasm fmt/clippy/test and `vendor-noble --check` in the gate/CI) | run as extra checks (§4); not added to §12 or CI in this window | — |
| W2-A F1 (a): `frontier_abi::log::Kind::spec()` over an integer table | **not applied** in this window (not a gate item; the program never calls it on chain and `build-frontier.sh` refuses any data relocation with a non-zero addend). Open for the next integration window | — |

## 3. Integration window: gate fixes (one commit per gate item)

1. **`22bb26d` — svm g03, authenticated presence (W2-A bug, W2-B finding 1).** PostAnchor/PostAnchorMulti and PostSeed took any program-owned account at the canonical address as "present" and returned a success no-op. §4.1 authenticates presence by owner, magic, season id and key fields; §13.2 G3 wants `BadAccount`. Fix: `prologue::presence` plus the stored `(bell, region)` / `(bell, region, nonce)` before the no-op. The tests were not changed.
2. **`d8748b8` — svm g03, Season PDA recomputed (W2-A bug, W2-B finding 2).** The season and keeper prologues did not recompute the Season's address (W2-A's deviation "relies on the presence rule"); §3.3 and `read_season`'s own contract ("the caller verifies the PDA address with the stored bump") require it; a program-owned copy was accepted by SetWindowSchedule. Fix: `addr::season_pda(id, bump, program)` over `sol_sha256` (the runtime's PDA hash without the off-curve test, which AnnounceSeason's `find_program_address` already guarantees), checked in `prologue::{season, keeper}` after `read_season` and before the status (`BadAddress`). Host test against `Pubkey::find_program_address`. The tests were not changed.
3. **`98eaaa6` — frontier-node release tests, findex paging (not a unit bug).** W2-F's `rpc_poll_refuses_a_server_that_ignores_before` asserted that the local node ignores `before` (the W1 MVP); W2-C's node now honours it, as W2-F asked. The refusal is now tested against the test's fake RPC with an `ignore_before` switch, and a new `rpc_poll_pages_the_local_node` checks that RpcPoll pages W2-C's node (page 2, 5 transactions, same signatures as the feed, oldest first). `RpcPoll` code unchanged.

**CU cost of fixes 1–2** [measured, W2-A's scratch LiteSVM smoke re-run on the new release and test-beacon `.so`, `smoke-*-fixed.txt`]: every one of the 46/49 smoke outcomes unchanged; +265…+303 CU on each instruction that reads the Season, +383/+405 on the no-op re-posts, +1,091 on the all-present PostAnchorMulti. Against §5.5: SetWindowSchedule 3,682 / 5k, InitShards 39,463 / 40k (tight), CreateSeason 40,603 / 70k, ConsumeGenesisSeed 329,491 / 345k, PostAnchor 337,737 / 345k, PostSeed 339,032 / 345k, PostBeacon 328,919 (test-beacon 331,962) / 340k, PostAnchorMulti (7 regions) 372,963 / 400k; AnnounceSeason unchanged. InitBeaconLogs 74,838 stays over its 60k budget (W2-A F3, pre-existing).

Release `.so` after the fixes: 237,376 B, `file_sha256 6b668a93…b886`, `program_hash 55e8192e…8956`, e_flags 2, `--max-len` 299,008 (unchanged page count, so every `L(kind)` of W2-B's table is unchanged).

## 4. Gate W2 — run on `98eaaa6`, 2026-09-28 00:19–00:37, exactly as §12 writes it

`M1_PORTS` empty (the preamble checks nothing; every test binds `127.0.0.1:0`).

| # | Item | Result [measured] |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | exit 0 |
| 2 | clippy `permutation-rules`, `frontier-abi` | exit 0 |
| 3 | `cargo test --locked --release -p permutation-rules` | exit 0 (434 passed) |
| 4 | `cargo test --locked -p frontier-abi` | exit 0 (43) |
| 5 | `abi-vectors -- --check` | exit 0 (9 files fresh) |
| 6 | `cargo test --locked -p permutation-chain` | exit 0 (157) |
| 7 | frontier-sim fmt, clippy, `cargo test --release` | exit 0 (18), 390 s |
| 8 | `criterion --best-response … --gate` | exit 0, worst cell 0.985 |
| 9 | `doctrine-gate --controls` | exit 0: kernel 6/6 in band, gap 1.4 points, largest \|Δ\| 0.063%; draft (−1.291% A), Knight (−0.294% F), A boost (+0.338%) all rejected |
| 10 | frontier-node fmt + clippy `-D warnings` + `cargo test --locked --workspace` | exit 0 (112 passed, 2 ignored) |
| 11 | gateway `npm ci --ignore-scripts && npm test` | exit 0 (438: 437 pass, 1 skip = the PENDING-OWNER `frontier.wasm` test) |
| 12 | civilization tests | exit 0 (47/47) |
| 13 | `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| 14 | `cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings` | exit 0 |
| 15 | `cargo test --locked -p permutation-frontier --no-default-features` | exit 0 (36) |
| 16 | `scripts/build-frontier.sh --twice` | exit 0, both builds `6b668a93…b886` |
| 17 | `svm-tests/run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_` | exit 0 (45: g01 10, g02 10, g03 9, g04 5, g05 7, harness controls 3 + probe 1) |
| 18 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (112 passed, 2 ignored) |
| 19 | gateway `npm test && node scripts/sync-web-sdk.mjs --check` | exit 0 (437/438, 1 skip as item 11; sdk up to date) |
| — | `build-wasm --check` | **PENDING-OWNER** (wasm32 target, O-M1-12) |

**Pass conditions beyond exit codes**

| Condition | Status |
|---|---|
| `keeper::one_day_beacons` green (genesis, rings, anchors, fallbacks, caches over `localnet`, test key) | **green.** In item 18 against W2-F's native model (the test's default); **also run against the merged program**: `PSF_FRONTIER_SO=permutation-frontier/target/deploy-test-beacon/permutation_frontier.so cargo test --locked --release -p keeper --test one_day_beacons --test archive_returns_rent -- --nocapture` exit 0: 24.3 game hours, every bell × region anchored and cached (findex: ANCHOR 2,320, SEED 2,304, BEACON 2,336), anchor/seed latency p99 1 slot, restart adopted 19 in-flight versions, journal 5,262 landed / 4 failed. **Rings:** OpenRing is W3-A's stub (`NotImplemented` 99) in wave 2 by the contract's own unit split, so the ring part is reported by the test, not exercised; ArchiveAnchors likewise (W4-B) |
| payer χ² test green | green: keeper `pools` unit test, and one_day_beacons on the real program χ² 26.0 over 32 delay payers (critical 61.10) |
| `g01_loaded_limit_*` green against the release `.so` with the enforcement control | green (item 17: 10 kinds + 3 `g01_loaded_limit_control_*`) |
| validator drill result recorded in W2-B notes | yes (W2-B-NOTES §2, `svm-tests/drill/drill-2026-09-27.txt`; probe padded to the 540,608-B placeholder; the optional re-run at the real 237,376-B size was not done) |
| `frontier.wasm` ≤ 400 KB raw once unblocked | **PENDING-OWNER** (O-M1-12 item 1) |

**PENDING-OWNER in this gate:** `build-wasm` (wasm32 target) only; §12 writes it as a `pending` branch of Gate W2 and the size condition as "once unblocked", so it does not block wave 2. It blocks W2-E's size figure until approved, and the web smoke of W5/W7.

**Extra checks (not in §12):** full svm suite `./run.sh --release --no-fail-fast` 65 passed + 1 ignored drill; `cargo test --locked -p permutation-frontier` (default features) 40; frontier-wasm `cargo fmt -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --release` exit 0 (14); `node scripts/vendor-noble.mjs --check` fresh (17 files).

## 5. Open items for the owner, the contract and later waves

- **Contract amendments requested, not made in this window** (they change budgets or a layout path and deserve their own review): W2-A F2 — AnchorArchive (12,192 B) exceeds the 10,240-B CPI allocation limit, so ArchiveAnchors (W4-B) needs a new creation path or size before wave 4; W2-A F3 — budgets InitBeaconLogs 60k → ≈ 80k (measured 74.8k now), InitShards 40k → 45k (39.5k now), AnnounceSeason 18k → 25k; W2-A's SetWindowSchedule "no pending change" rule (may re-read `W` for bells older than a day).
- **W2-A F1 on `solana-test-validator 3.1.9`**: not confirmed on the validator (the drill used a probe, not the program).
- **W5-A:** W2-B finding 3 (`L(kind)` over-counts the instructions sysvar and builtins), G13 completion.
- **W3-A** (player prologue on chain): call the same Season address check as `prologue::season` (`addr::season_pda`).
- **CI (`ci.yml`)**: `frontier-wasm` has no host test step and `build-wasm.sh --check` will fail until a module is committed; `frontier-node` runs the tests without `npm ci` (the web3.js test would print NOT RUN there). Not changed here; nothing is pushed.
- **W2-D pending:** a live relay run against `frontier-localnet` with the real program (first possible now; not a gate item, not run).
