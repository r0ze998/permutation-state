# integ-W3: wave-3 merge, integration window and Gate W3

- **Role:** M1 integrator, wave 3. **Branch:** `frontier/m1-integ` (worktree `.claude/worktrees/m1-integ`), wave base `241c52d`. **Contract:** M1-CONTRACT v1.3 → **v1.4** (§20, this window) §3.4, §11 (wave 3), §12 (Gate W3), I-55. **Date:** 2026-09-28.
- **Owner decisions in force (relayed by the M1 workflow, 2026-09-27):** O-M1-01…24 working defaults accepted; **O-M1-12 not approved** (no `rustup target add`, Playwright install, drand archive fetch or Agave install was run); O-M1-17 fix locally, no push; O-M1-18 not approved; ordinary registry dependencies **from the contract's expected set** approved. The 1.95.0 `rustfmt`/`clippy` install is already recorded in `DECISIONS.md` part A (W2-A's row); nothing added.
- **Not done, by rule:** no push, no devnet or mainnet transaction, no service started (every test binds `127.0.0.1:0`; no port of 41000–41999 was used), no download, no file of the main tree, `codex/magicblock-playable` or `codex/v9-security` touched, `permutation-server/web/session.mjs` untouched.
- **Logs:** `(session scratch)/scratchpad/integ-w3/` — `gate/gate-w3.sh`, `gate/gate-run1.txt`, `gate/logs/item1..22.log`, `gate/extra-program.log`; pre-checks in `pre/` (`svm1b.log`, `svm2b.log`, `keeper-real*.log`, `keeper-real-odb.log`).

## 1. Merges (contract order W3-A, W3-B, W3-C, W3-D, W3-E, W3-F; all `--no-ff`)

| Step | Unit (head) | Merge commit | Integrator-owned files left out and re-applied |
|---|---|---|---|
| 1 | W3-A program-land (`7e0b5cc`) | `953be2a` | none changed |
| 2 | W3-B program-holding-march (`2954691`) | `c821089` | none changed |
| 3 | W3-C keeper-land (`8b37821`) | `8a2e89f` | none changed |
| 4 | W3-D herald (`cc92fe6`) | `8695e0d` | `frontier-node/Cargo.toml`, `Cargo.lock`, `crates/herald/Cargo.toml` → `476ba03` |
| 5 | W3-E bots-agents (`57f0350`) | `026a7f6` | `frontier-node/Cargo.lock`, `crates/{agents,bots}/Cargo.toml` → `4891c62` |
| 6 | W3-F web-play (`a7e855c`) | `f786f47` | none changed |

No textual conflict. W3-D's `tests/real_chain.rs` (which includes the keeper tests' harness by path, D10) compiled against W3-C's merged harness without change.

## 2. Dependency requests (§3.4, I-55)

| Request | Applied | Check |
|---|---|---|
| W3-D R1 `findex` workspace path dependency | yes (`476ba03`) | `cargo check --locked --offline --workspace --all-targets` |
| W3-D R2 `hyper =1.11.1`, `hyper-util =0.1.21` | yes (`476ba03`); both already in the lock through `axum`, no new crate | as above |
| W3-D R3 `flate2 =1.1.10` (+ `miniz_oxide`, `crc32fast`, `adler2`, `simd-adler32`) | **not applied**: outside the contract's expected dependency set (§3.3), which is what the owner approved. As W3-D offered, the herald writes **no `.gz` siblings** in wave 3: `files.rs` loses `gzip`/`GZ_LEVEL`/`Out.gz` (a changed write still removes a stale sibling; the server still serves a sibling placed by other means); the gz assertions in `files.rs`, `tests/fold.rs` and `tests/server.rs` became "no sibling written" / opaque-sibling checks. **Owner question (DECISIONS J2):** approve `flate2` for §8.4's `.gz` siblings (and `brotli` for `.br`) | herald release tests 22 passed, 1 ignored |
| W3-D R4 herald dependency sections, `[[bin]] frontier-viewers` | yes, without `flate2` | as above |
| W3-E R1/R2 agents and bots dependency sections, bots `[lib]` | yes (`4891c62`); lock edges only, byte-identical to the unit's lock | `cargo check --locked --offline` |
| W3-A, W3-B, W3-C, W3-F | nothing requested | — |

## 3. Integration window: gate fixes (one commit per gate item)

1. **`05dc69c` — svm g03, foreign ProgramData case gets a loaded limit for two ProgramData** (W3-B F5; W2-B's test). The merged release `.so` is 564–566 KB, so the test's forged ProgramData copy made the transaction load 1,417,762 B against the 1-MiB `L(AnnounceSeason)` and SIMD-0186 refused it before the program ran. The case now sends with `Profile::ladder(..).with_loaded(2 × L)`; `assert_refused` still requires the program's own error code. Program unchanged.
2. **`9e260bb` — OpenProvince ≤ 220k, terrain kernel tables (W3-A F2, Appendix A applied as written).** `geometry::tile_offset`/`tile_index` read const tables built by the same loops; `terrain::canonical` hashes the terrain base once per province. Outcome-identical: W3-A's on-chain check of 331 provinces; a new unit test `tile_tables_equal_the_former_loops` (every index 0..=255, every offset in a 25 × 25 box); `permutation-rules` release tests; and Gate items 8–9 give **exactly** the W2 numbers (criterion worst cell 0.985; doctrine gate kernel 6/6, gap 1.4 points, max |Δ| 0.063%). OpenProvince worst over 324 provinces 284,896 → **148,401 CU**. No `TERRAIN_VERSION`/ruleset change.
3. **`89479f8` + `00d066b` — FileTicket budget 17k (contract v1.4 §20, W3-A F3).** Measured 16,048 CU at the §13.1 fill (player prologue, three canonical Provinces and cohorts, the escrow CPI, the 4-chain `TICKET`); class P, so no fee or tip formula moves. `frontier-abi::budgets`, vectors (contract tag v1.4), fclient mirror, `client/src/frontier` and `web/sdk` resynced; a follow-up commit adds fclient's regenerated `permutation-gateway/test/frontier-vectors.json`. DECISIONS part J.
4. **`9f8c285` — keeper land tests, the site gen rule of the program.** W3-A pinned "every founding bumps the site's gen (first holding gen 1)"; W3-C's native model and `land_season` assumed a first founding has gen 0, so on the program the test read gen 1 as a displacement. The model now bumps on every founding and the test compares with `FIRST_GEN = 1` / `FIRST_GEN + 1`. The keeper reads gen from chain and needed no change.

Commits (first parent, oldest first): `953be2a`, `c821089`, `8a2e89f`, `8695e0d`, `476ba03`, `026a7f6`, `4891c62`, `f786f47` (merges and dependency commits); `05dc69c`, `9e260bb`, `89479f8`, `00d066b`, `9f8c285` (window); then this notes commit.

## 4. Gate W3 — run on `9f8c285`, 2026-09-28 03:59–04:21, exactly as §12 (v1.4) writes it

`M1_PORTS` empty (the preamble checks nothing; every test binds `127.0.0.1:0`). Script: Gate W1 items, Gate W2 items, Gate W3 items, each logged separately.

| # | Item | Result [measured] |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | exit 0 |
| 2 | clippy `permutation-rules`, `frontier-abi` | exit 0 |
| 3 | `cargo test --locked --release -p permutation-rules` | exit 0 (435) |
| 4 | `cargo test --locked -p frontier-abi` | exit 0 (43) |
| 5 | `abi-vectors -- --check` | exit 0 (9 files fresh) |
| 6 | `cargo test --locked -p permutation-chain` | exit 0 (157) |
| 7 | frontier-sim fmt, clippy, `cargo test --release` | exit 0, 416 s |
| 8 | `criterion --best-response … --gate` | exit 0, worst cell 0.985 |
| 9 | `doctrine-gate --controls` | exit 0: kernel 6/6 in band, gap 1.4 points, max \|Δ\| 0.063%; draft (−1.291%), Knight (−0.294%), A boost (+0.338%) rejected |
| 10 | frontier-node fmt + clippy `-D warnings` + `cargo test --locked --workspace` | exit 0 (179 passed, 3 ignored) |
| 11 | gateway `npm ci --ignore-scripts && npm test` | exit 0 (472: 471 pass, 1 skip = the PENDING-OWNER `frontier.wasm` test) |
| 12 | civilization tests | exit 0 (47/47) |
| 13 | `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| 14 | `cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings` | exit 0 |
| 15 | `cargo test --locked -p permutation-frontier --no-default-features` | exit 0 (36) |
| 16 | `scripts/build-frontier.sh --twice` | exit 0; both builds `file_sha256 847d628f…fd17`, `program_hash 8c4dfc10…4add`, e_flags 2, overflow strings 3, `.so` 564,472 B, `max_len` 708,608, deployable yes |
| 17 | `svm-tests/run.sh --release -- g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_` | exit 0 (72) |
| 18 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (179 passed, 3 ignored) |
| 19 | gateway `npm test && sync-web-sdk.mjs --check` | exit 0 (471/472, 1 skip; web/sdk up to date) |
| — | `build-wasm --check` | **PENDING-OWNER** (wasm32 target, O-M1-12) |
| 20 | `svm-tests/run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_` | exit 0 (83) |
| 21 | `PSF_TRACE=1 svm-tests/run.sh --release -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture` | exit 0 (5) |
| 22 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (179 passed, 3 ignored) |

`web3.js conformance NOT RUN` appears 0 times in items 10, 18 and 22. The 3 ignored frontier-node tests are the long/benchmark ones (`viewers_measure` and the two W2 ignores).

**Pass conditions beyond exit codes**

| Condition | Status [measured, item 21 on the merged release `.so`, 564,472 B] |
|---|---|
| Reveal worst ≤ 26,000 CU | **25,140 CU** (FirstOfBell, honest worst; Named 24,291; Displace 20,341); heap on the trace build 2,930 B. The 20k target is not met (W3-B's finding, unchanged) |
| Reveal tx ≤ 1,100 B | **916 B**, 20 locks (3 writable) |
| `L(reveal)` measured | **753,664 B** by the formula over the worst account set at this `.so`; measured loaded need 729,605–730,597 B. W3-B's figure (589,824 B) was for its 420,800-B branch `.so`; W5-A regenerates the preset from the wave-5 release `.so` (the C4 model should take the W5 figure). Presets were not amended in this window |
| Cohort tests green | yes (item 20): `citizen_cohort_displacement_has_no_deadline_and_finality_waits` (displacement without a deadline, a Province held through `final_ts`), `citizen_cohort_expiry_ends_the_ticket`, `citizen_cohort_displacement_within_one_join_shard`, `citizen_cohort_table_full_refuses_a_ninth_bell` |
| Herald fold determinism green | yes (item 22): `fold_determinism_same_archive_same_bytes`, `ingest_restart_and_crash_give_the_same_files`; and `real_chain::folds_the_programs_beacon_records` on the test-beacon program (extra run below) |
| Keeper land tests over `localnet` green | **yes, as the gate runs them** (item 22: `land_season`, `reveal_accept_over_localnet`, `one_day_beacons` over `localnet` with W3-C's native model, test key). **On the merged program** (extra run, test-beacon `.so` `d36820ee…77c9`, 564,984 B): `one_day_beacons` **green with the ring part** (genesis rings 0–3, 37 provinces opened; 24.3 game h; anchor/seed p99 1 slot; χ² 22.7 over 32 payers; 19 adopted at restart; journal 5,303 landed; **0 alerts**) — the v1.3 §12 condition that the ring part runs on the program from Gate W3; `reveal_accept_over_localnet` green (202/409/422/410 as the program's Reveal). `land_season` on the program passes genesis, the cohort (8 settlements, 6 winners, winner landing p99 3 slots, no displacement after fix 4) and **stops at the first resident action** (the keep-alive Muster): `NotResident` (26), because `resolved_next` only advances by SkipQuiet/ResolveFromInputs, which are W4-A's stubs in wave 3. Not a unit bug; the full land scenario on the program belongs to W4 (W4-A resolution, W4-F `itest`) |
| Other lines of W2 kept green | `g01_budget_w2a_*`, `g01_loaded_limit_*` with their controls (item 17); payer χ² (above) |

**PENDING-OWNER in this gate:** `build-wasm` only (O-M1-12), as §12 writes it (a `pending` branch of Gate W2, carried into W3); it does not block wave 3 and blocks W6/W7 while pending. No other item needed an install or download.

**Verdict:** Gate W3 green (22/22 items exit 0, every pass condition met as written, one PENDING-OWNER item the contract allows at this wave). `codex/frontier` fast-forwarded to this branch's head after this notes commit.

## 5. Open items (not gate items; for the wave-3 review, later waves and the owner)

- **Owner:** J2 — approve `flate2` (and optionally `brotli`) for the herald's §8.4 precompressed siblings; until then the herald serves plain files (a CDN can compress).
- **W3-A F1 — `init::init_funded` is refused on chain (`UnbalancedInstruction`)**: no caller uses it now (W3-A uses `map::init_funded_after`); fix or remove it before W4-B (ArchiveAnchors) might call it. Not changed here.
- **Contract, not amended in this window (v1.4 §20 lists them):** W3-A F4 (`chains_of(SETTLE)` with a shared JoinShard), F5 (§5.9 Join's wedge-fund clause; W3-A's D2), F7 (vacuous `ticket_bell ≠ now_bell`), F8 (terrain encoding and ticket score into a shared crate: today the program and `fclient::land::ticket_score` agree byte for byte, checked by reading both, and the keeper test recomputes the winner), F10 (`SiteTaken` has no path); W3-B presets (`reveal_cu_limit` 26,000, `reveal_loaded_limit`, `tip_min`) at W5 from the final `.so`; W3-B's four G1 CU breaches under `RELEASE_CHECK=1` (Harvest, Train, Explore, Depart; Gate W5); W3-C F1 (crowding rule); W3-D D2/D3/D6 (`unchecked`, WS numbering, bell-region caching); W3-F R2 (catalog/rules constants exported from frontier-abi), R3 (doctrine travel bias in `frontier-wasm`), R4 (`reachable`'s signature), D7 (Muster/Garrison troop units: W3-B's program takes whole troops, matching the page).
- **W3-A F6 / W3-C:** the keeper's CU-exhaustion classification by logs (LiteSVM 0.16 reports SBPF v2 CU exhaustion as `ProgramFailedToComplete`); the trigger (OpenProvince over budget) is gone with fix 2, so `one_day_beacons` passes, but the classification is still W3-C's to harden.
- **W4-A:** export the ClashInput builder for the herald (W3-D D1) and the verifier; the land scenario on the program needs its SkipQuiet/ResolveFromInputs.
- **W4-F:** re-record W3-E's synthetic herald fixtures from a real herald; `inproc_day` with 100 bots.
