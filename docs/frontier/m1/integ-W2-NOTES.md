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

## 6. Review response (the wave-2 review, answered in the same window, 2026-09-28)

Every blocker/major and every "missing" item was checked against the code at `d662861`; each is either fixed with a test, or answered in one line with its evidence. Minors: fixed where cheap, otherwise deferred to a named owner. Contract amendments are **v1.3 §19**; decisions are **DECISIONS part I**. Commits: `d6b0e7e` (W2-A/W2-B), `24291fc` (W2-C), `213cf56` (W2-F), `97dbcfd` (W2-D), `a274f5d` (W2-E), `98cb40f` (contract v1.3, decisions), `331a27d` (the contention test after the cost model, found by the first gate run), and this notes commit.

### 6.1 W2-A program-core

| Item | Verdict | Disposition |
|---|---|---|
| InitBeaconLogs/InitShards/AnnounceSeason over §5.5 (major) | **Confirmed** | Budgets amended (25k / 80k / 45k, §5.5, `frontier-abi::budgets`, vectors, fclient mirror, synced JS). New committed `g01_budget_w2a_{season_lifecycle,beacon_writes}` asserts every W2-A instruction against the table (CU, tx bytes, locks, loaded data) and is in Gate W2's svm line. Measured: AnnounceSeason worst of 64 ids **18,570** (id 50; above the old 18k, as the review said), InitBeaconLogs 74,688, InitShards 39,313, CreateSeason 42,161, ConsumeGenesisSeed 329,341, SetWindowSchedule 3,535, PostAnchor 337,164 (archive present), PostAnchorMulti 7 regions 378,439, PostSeed 337,292, PostBeacon 332,172 (release) / 330,679 (test-beacon). The splitting alternative (an ABI change) was not taken |
| SetWindowSchedule fold (major) | **Confirmed** | **One change per season** (a second call is `AlreadyDone`; `reveal_window` is never rewritten, so `W(b)` of a bell never changes), `from_bell < end_bell` else `BadData` (refuses the `u32::MAX` sentinel). Tests `g04_window_schedule_needs_notice_and_range` (second call while pending and 150 bells after the change took effect, `reveal_window` unchanged) and `g04_window_schedule_from_bell_inside_the_season`. §5.7 amended |
| F2 AnchorArchive 12,192 B > 10,240 B CPI limit (major) | **Confirmed** | Contract amended and implemented now: **one archive per region and half day** (`part = bell / 72`, 6,144 B, 72 entries; const assert `SIZE ≤ CPI_ALLOC_MAX`). frontier-abi layout/addr/ix (`ArchiveAnchors.part`), vectors, the program's archive read, fclient (mirror, decoder, every builder), keeper archive duty and native model (`archive_returns_rent` checks both parts of day 0), svm crafted archives, JS seeds (`archivePart`). The "grow in a second instruction" option was rejected (a half-created archive is visible to readers and `rent_to` lies past 10,240 B); shrinking the entry is impossible (seed and sig both needed after the anchor closes, I-44) |
| `close_to` departs from §4.2 (minor) | Confirmed | §4.2 amended to W2-A's split (callers emit `CLOSE` first); the rule is in the `proc/mod.rs` hand-over list for W3-A/W3-B/W4-A/W4-B |
| Seal code 3 never returned (minor) | Confirmed | §5.3/§13.1/§19: code 3 reserved, never emitted; a wrong-round seal is code 1. Hand-over rule in `proc/mod.rs` |
| `program_version` unchecked (minor) | Confirmed | CreateSeason refuses `program_version ≠ PROGRAM_VERSION` (`BadData`); test case in `g13_create_season_window_hash_params_status` |
| Notes describe fixed behaviour (minor) | Confirmed | Post-merge addendum appended to `W2-A-NOTES.md` (drops the Season-PDA deviation, points to `22bb26d`/`d8748b8` and this window, current hash and CU) |
| Weak tests (minor) | Confirmed | `payout_params_hash` pinned by a fixed vector (REV3 borsh `2823…0000` → `91dc5b2b…bb87`, computed independently with `shasum`); CreateSeason `TooEarly`/`Announce`/`BadData` (3 cases) and InitBeaconLogs/InitShards `BadData`/`AlreadyDone` now asserted by code; the window rules as above. PostBeacon `AlreadyDone` and the test-beacon `TooEarly` stay with W5-A's G13 completion |
| Missing: CU gate | Fixed | `g01_budget_w2a_*` (above) |
| Missing: fold branch test | Moot | The fold no longer exists; the one-change rule is tested |
| Missing: `init_funded`, `pay_or_divert`, `close_to` on chain | Deferred (W3-A, W4-B) | No wave-2 instruction calls them; their first on-chain users own the tests (G2/G12) |
| Missing: seal opener on SBF | Deferred (W4-B) | SettleTransit is W4-B's; G10 runs it before and after archive |
| Missing: F1 on the validator; `Kind::spec` table | Not needed now | `build-frontier.sh` refuses any data relocation with a non-zero low word (checked on every build; today's release and feature builds pass it), so the hazard cannot ship; the validator confirmation stays optional (drill ports 41080–41089 when wanted) |
| Missing: amendments pending | Done | v1.3 §19 |

### 6.2 W2-B svm-harness

| Item | Verdict | Disposition |
|---|---|---|
| PostAnchorMulti has no G3 coverage (major) | **Confirmed** | `g03_anchor_and_archive_forged_in_post_anchor_multi`: non-canonical anchors (3) and archives (3), swapped anchor and archive positions, a mask naming other regions → `BadAddress`; empty mask → `BadData`; forged present anchor (owner, magic, season, bell, region) and forged archive (4 forgeries) → `BadAccount`; tombstoning archive → `Archived`; the genuine path skips one present anchor unchanged and creates the others. The `Pending` row in `cover/beacon.rs` now names only `WrongRound`; `g05` 8 regions asserts `BadData` |
| Seal code 3 not produced (major) | **Confirmed as a contract conflict** | Recorded and resolved in the contract (code 3 reserved, §13.1 lists 0, 1, 2, 4, 5); W2-B-NOTES addendum corrects "every seal code 0–5" |
| G13 guard cannot enforce per-(ix, code) (minor) | Confirmed | The false claim is fixed: `Chain::submit` panics on `NotImplemented` (99) under `RELEASE_CHECK=1`. The per-instruction expected-code table is W5-A's (G13 completion) |
| Worst sets not worst (minor) | Confirmed | `g01_loaded_limit_{post_anchor,post_anchor_multi,post_seed}_present` run `check()` on the no-op path with THE anchor/cache present; the `tight + PAGE` tolerance is listed as a deviation in the W2-B addendum until W5-A regenerates `L(kind)` |
| Season forgery narrow (minor) | Confirmed | `g03_season_copy_and_id_flip_in_every_w2a_instruction` (CreateSeason, InitBeaconLogs, InitShards, ConsumeGenesisSeed, PostAnchor, PostAnchorMulti, PostSeed, PostBeacon, SetWindowSchedule: copy and id flip → `BadAddress`); `g03_forged_present_targets_of_the_init_instructions` (Frontier, a ProvinceFund, DefencePool, a BeaconLog, a JoinShard → `BadAccount`) |
| `assert_refused` accepts any failure (minor) | Confirmed | It now requires a program error code; every existing use passed |
| `8 regions` only `is_err` (minor) | Confirmed | `assert_code(…, BadData)` |
| `ChainWatch` link matching (minor) | Confirmed, deferred (W3-A/W3-B, first multi-entity records) | No wave-2 record touches two entities of one kind |
| Deviations unlisted (minor) | Confirmed | W2-B addendum lists the test-beacon build for G2–G5 and the placeholder-size drill; `assert_within`'s silent heap skip is documented (heap is measured on the trace build only; W5-A's G1 fills use it) |

### 6.3 W2-C localnet-complete

| Item | Verdict | Disposition |
|---|---|---|
| Block caps count CU, not cost (major) | **Confirmed** | `Pending.cost` = `tx::priority`'s §10.1 cost, used for the block, account and fits checks and for the filler accounting; module doc corrected; §8.7 amended. `block_caps_count_the_cost_not_the_cu_limit`: 40 transfers of 1,000,000 CU into one account → 39 land (cost 1,001,328 each), 1 deferred; the existing cap test now asserts `28 × 1,401,336` |
| `is_loopback` userinfo bypass (minor) | Confirmed | Authority parsed; `@`, `\`, `%`, whitespace and non-numeric ports are never loopback; 10 bypass cases refused without `--approved O-M1-12` |
| `dataSlice` overflow panic (minor) | Confirmed | `saturating_add` (and `checked_add` for memcmp); unit test `data_slice_past_the_end_is_clipped` |
| CU charged on `execute → None`; filler model (minor) | Confirmed / documented | Cost added only after execute returns `Some`, else counted as dropped. The filler's "largest per-key fill" model is kept and stays the W5 contention unit's call |
| Snapshot binding, airdrop pre-balances, snapshot paths, `--program` mismatch, WS `Lagged`, archive verify-on-serve (minor) | Confirmed, deferred (W5 hardening) | Not gate items; loopback-only exposure; each recorded in DECISIONS I12 with the reviewer's fix |
| Missing items | As above | cost-model test and userinfo and dataSlice tests added; the restart/rewind/WS-lag tests go with the deferred fixes |

### 6.4 W2-D relay-sdk

| Item | Verdict | Disposition |
|---|---|---|
| Settle-quota escape with fresh requester keys (major) | **Confirmed** | `requesterCitizen`: charged to `citizen:<c>` only when the requester signed and is, on chain, the canonical Citizen's wallet or unexpired session key; else the address bucket; the signer limiter keys the same way. Test: session and wallet charge their citizen; 3 fresh keys land in one address bucket (37 left), no `session:` buckets, an expired session and a non-canonical Citizen are anonymous |
| Allowlist not exact (major) | **Confirmed** | `classify` requires the account keys to be exactly {fee payer, instruction accounts, two programs} and every key's writability to be the ABI's. SDK tests: unreferenced writable and read-only keys, Season marked writable |
| Replay race (minor) | Confirmed | Key claimed before any await; test: 3 concurrent identical POSTs over a 5-ms chain → 1 sent, 2 `Duplicate`, 1 charged |
| Quota prune (minor) | Confirmed | Prune only entries equal to a missing entry's view; test |
| `frontierRefusal` attribution (minor) | Confirmed | First failing program decides (System → 503 `OperatorLowFunds`, other → `ProgramError`); test |
| Pooled lamport cap (minor) | Confirmed, kept (DECISIONS I9) | Accepted residual for M1, reported |
| Send failure keeps the charge (minor) | Confirmed | A throwing send refunds, releases the replay key and the invite; test |
| gzip figure (minor) | Confirmed | W2-D addendum: 31,437 B is gzip -9 of the concatenated files; per-file ≈ 36.1–36.2 KB |
| Missing: live relay run on localnet; payer-derivation vector | Pending (W3-D / W3-C) | Not a gate item; not run |

### 6.5 W2-E web-foundation

| Item | Verdict | Disposition |
|---|---|---|
| ChainClock rate guessed on every cluster (major) | **Confirmed** | Rate 1 unless the pinned cluster is localnet (then estimated only from advancing samples, never below 1); `behind()` against the local clock drives the 60-s banner. Tests: 40-s lag then catch-up never runs ahead of the chain; a herald stuck 300 s → behind ≥ 299 s, stale, the chip keeps moving; localnet keeps its rate while stuck |
| `messageProblems` does not recompute accounts (major) | **Confirmed** | `expected` instruction and `blockhash` required; accounts and data compared; `payer` = fee payer; the relay's `classify` run. Tests: swapped accounts, another holding, wrong payer, read-only marked writable, missing blockhash/expected; a SettleTransit shape passes |
| `sealMarch` round not tied to T(arrive) (major) | **Confirmed** | `sealRound(clock, arriveBell)`; `sealMarch` refuses another round (`WrongRound`) or a missing clock (`NoClock`); the worker's audit refuses a round ≠ T(arriveBell) even for a seal consistent with it. Tests |
| Herald key checks, beacon NETWORK byte, `reachable` signature and bounds, `plan_path`, ClashArgs codec, D5 wording, `ps-fui`, measurement commands (minors) | Confirmed, deferred (W3-F web play for herald/ps-fui; W4-A/W4-E for the clash codec and `resolve_from_inputs`; frontier-wasm bounds and `plan_path` with W3-F's first use) | Recorded in DECISIONS I12 and the W2-E addendum |
| Missing: `frontier.wasm` | **PENDING-OWNER** (O-M1-12) | unchanged |

### 6.6 W2-F keeper-core

| Item | Verdict | Disposition |
|---|---|---|
| Stalled write after the version cap (major) | **Confirmed** | Engine ends a write whose versions all expired (Failed, backoff, `write-expired` alert) and the duty re-plans it; the scan window always reaches the newest bell (`anchor-missing` alert). Tests: `capped_write_whose_versions_expired_ends_and_restarts` (unit); `anchor_held_past_the_version_cap_lands_after_the_hold` (240-slot hold at priority 1.0 over localnet: hold ended at slot 638, anchor landed at 640; alerts for `anchor-multi:2:0` and `anchor:2:5`; every newer bell anchored) |
| findex roll not crash-safe (major) | **Confirmed** | `Archive::open` deletes segment files the manifest does not name; a new segment is created empty. Test `a_crash_across_a_segment_roll_reopens_clean` (the reviewer's scenario: roll, crash before the manifest, reopen, re-append: verify 12, read back 1..12) |
| Nonce duplicates, write-ahead journal, Dead backoff, D start bid, payer care, spend/orphan/payers/rescan/ladder gaps (minors) | Confirmed, deferred (W3-C keeper land adds seed-cache closes at every nonce and the write-ahead journal; W5 hardening the rest) | DECISIONS I12 |
| Missing: rings on the real .so | Contract amended | Gate W2 text: the ring part runs on the program from Gate W3 (OpenRing is W3-A's) |
| Missing: `archive_returns_rent` on the .so | Deferred (W4-B) | ArchiveAnchors is W4-B's; the native model now uses half-day parts |
| Missing: long-hold, findex crash tests | Fixed | above |
| Missing: short seed-nonce hold, send/journal crash | Deferred (W3-C) | with the nonce and journal fixes |

## 7. Gate W2 re-run after the review response (contract v1.3 §12), 2026-09-28

Logs: `(session scratch)/scratchpad/integ-w2r/gate/` (`gate-w2.sh`, `gate-final.txt`, `logs/item1..19.log`; the first run is `run1-*`). `M1_PORTS` empty; every test binds `127.0.0.1:0`; no service started, nothing pushed, no devnet/mainnet transaction.

**First run, on `98cb40f`:** items 1–9 and 11–19 exit 0; **item 10 failed** (`localnet` `a_held_key_admits_only_higher_priorities_and_ties_lose`: the filler's expected take was 40M − 50,000, the winner's CU; with the §10.1 cost model it is 40M − 51,336). Fixed in `331a27d` (the test derives the winner's cost and pins 51,336), then the whole gate was run again.

**Second run, on `331a27d`, 02:04–02:22** (the working tree differed from HEAD only by these notes, which no test reads):

| # | Item | Result [measured] |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | exit 0 |
| 2 | clippy `permutation-rules`, `frontier-abi` | exit 0 |
| 3 | `cargo test --locked --release -p permutation-rules` | exit 0 (434) |
| 4 | `cargo test --locked -p frontier-abi` | exit 0 (43) |
| 5 | `abi-vectors -- --check` | exit 0 (9 files fresh) |
| 6 | `cargo test --locked -p permutation-chain` | exit 0 (157) |
| 7 | frontier-sim fmt, clippy, `cargo test --release` | exit 0 (18), 385 s |
| 8 | `criterion … --gate` | exit 0, worst cell 0.985 |
| 9 | `doctrine-gate --controls` | exit 0: kernel 6/6 in band, gap 1.4 points, max \|Δ\| 0.063%; draft (0/6), Knight (−0.294%), A boost (+0.338%) rejected |
| 10 | frontier-node fmt + clippy + `cargo test --locked --workspace` | exit 0 (117 passed, 2 ignored) |
| 11 | gateway `npm ci --ignore-scripts && npm test` | exit 0 (443: 442 pass, 1 skip = PENDING-OWNER `frontier.wasm`) |
| 12 | civilization tests | exit 0 (47/47) |
| 13 | `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| 14 | clippy `permutation-frontier` | exit 0 |
| 15 | `cargo test --locked -p permutation-frontier --no-default-features` | exit 0 (36) |
| 16 | `scripts/build-frontier.sh --twice` | exit 0; both builds `file_sha256 89f86c71…53bb`, `program_hash ee47b556…5256`, e_flags 2, overflow strings 3, `.so` 237,384 B, `--max-len` 299,008 (unchanged page count, so every `L(kind)` is unchanged) |
| 17 | `svm-tests/run.sh --release -- g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_` | exit 0 (54 passed) |
| 18 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (117 passed, 2 ignored) |
| 19 | gateway `npm test && sync-web-sdk.mjs --check` | exit 0 (442/443, 1 skip; web/sdk up to date) |
| — | `build-wasm --check` | **PENDING-OWNER** (wasm32 target, O-M1-12) |

`web3.js conformance NOT RUN` appears 0 times in items 10 and 18; `web3_js_and_send_mjs_against_a_live_node` ran and passed in both.

**Pass conditions:** `one_day_beacons` green (item 18, native model; and on the merged test-beacon `.so` below); payer χ² green; `g01_loaded_limit_*` (13 incl. the 3 new no-op sets and 3 controls) and `g01_budget_*` green on the release `.so`; drill result recorded in W2-B notes (not re-run); `frontier.wasm` PENDING-OWNER.

**Extra checks (not in §12):** `PSF_FRONTIER_SO=<test-beacon .so, 238,232 B> cargo test --locked --release -p keeper --test one_day_beacons --test held_accounts --test archive_returns_rent -- --test-threads 1` exit 0: 24.3 game h, anchor and seed latency p99 1 slot, χ² 31.3 over 32 payers (critical 61.10), 19 adopted at restart, findex ANCHOR 2,320 / SEED 2,304 / BEACON 2,336, alerts only `not-implemented` ×2 (rings W3-A, ArchiveAnchors W4-B); the long-hold test on the program: hold ended at slot 638, anchor landed at 640. Full svm suite `./run.sh --release --no-fail-fast`: 74 passed, 1 ignored (drill). Not re-run in this window: frontier-wasm checks and `vendor-noble --check` (no file of theirs changed).

**PENDING-OWNER:** `build-wasm` only (O-M1-12), as §12 writes it.
