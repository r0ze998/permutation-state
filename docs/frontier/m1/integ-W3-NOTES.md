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

## 6. Review response (the wave-3 review, answered in the same window, 2026-09-28)

Every blocker/major and every "missing" item was checked against the code at `1534014`; each is either fixed with a test, or answered in one line with its evidence. Minors: fixed where cheap, otherwise deferred to a named owner. Contract amendments are **v1.5 §21**; decisions are **DECISIONS part K**. Commits: `abbdf71` (W3-A), `97f677d` (W3-B), `7835196` (W3-C/W3-D), `f42013f` (W3-E), `2acea0e` (vectors v1.5, JS SDK, fclient vectors), `2086c5d` (W3-F), then the docs commit with these notes.

### 6.1 W3-A program-land

| Item | Verdict | Disposition |
|---|---|---|
| Refile makes the new payer the funder of someone else's escrow (major) | **Confirmed** | `ticket_funder = payer` only when the payer tops up; `TICKET.funder` logs the effective funder (§5.9 v1.5). svm `citizen_refile_keeps_the_escrow_funder`: sponsor pays, expiry, self-paid refile (fee only) → funder and, after a fresh settle, `Holding.rent_payer` stay the sponsor; a sponsored refile over a self-funded escrow keeps the player; an empty escrow makes the payer the funder |
| F4 SETTLE with a shared JoinShard vs `chains_of` bounds (major) | **Confirmed** | `chains_of(SETTLE DISPLACE)` marks the displaced JoinShard link optional (§6 v1.5); unit test `settle_displace_admits_one_shared_join_shard` (bounds (5, 8), FRESH (4, 6)); the svm records decoder now checks **every** record's tail against `chains_of(..).bounds()` and its entity kinds, so every svm test is a tail test (`citizen_cohort_displacement_within_one_join_shard` passes it). An ABI vector for the one-shard case was not added: the vectors generator emits one sample per kind; the unit test pins it |
| Displaced triple / ReleaseDormant forgeries (minor + missing) | Confirmed (coverage) | `g03_settle_ticket_refuses_a_forged_displaced_triple` (thief and another citizen as rent payer, another Citizen, another faction's and another shard's JoinShard → `BadAddress`; the genuine one lands and repays); ReleaseDormant Citizen, JoinShard, a DefencePool copy (`BadAddress`) and another season's pool (`BadAccount`) in `citizen_release_dormant_frees_the_site_and_strands_the_gen` |
| Repeated SettleTicket returns `NoTicket` (minor) | Confirmed | Keeper-side: `NoTicket` ends a SettleTicket as done (`fclient::abi::err::is_done`, §5.4 v1.5); the program cannot tell "never filed" from "settled" once the ticket is gone |
| Chain restart after CLOSE (minor + missing) | Confirmed | §6 v1.5 rule (a creation after CLOSE starts a new chain, seq 1); `ChainTrack` assertion added in the release test (first link seq 1 from a zero head). findex's upsert keyed by address alone stays W4-D's to adapt (K11) |
| Sinks that are not Holdings (minor) | Confirmed | `Sink::Never` for CloseProvince and CloseCitizen (a diversion would be refused, never a wrong-typed `pool_owed` write) |
| Notes numbers stale (minor) | Confirmed | Addendum in `W3-A-NOTES.md` |
| F1 `init::init_funded` (minor) | Confirmed | `init::init_funded` now allocates first, then moves the lamports; `map::init_funded_after` removed and both callers use it |
| ConsumeRingSeed G1, D2/D4/D5/F8 (minor + missing) | D2, D4, D5, F7 amended (§21); ConsumeRingSeed G1 and `g01_loaded_limit_*` for the W3-A kinds deferred to W5-A; F8: a twin assertion now pins `fclient::land::ticket_score` to the program's stored score (`citizen_founded_holding_runs_the_holding_actions`); moving the encodings into frontier-abi is W4-D's (K11) |

### 6.2 W3-B program-holding-march

| Item | Verdict | Disposition |
|---|---|---|
| Four G1 CU breaches (major) | **Confirmed** | Cheap trim done: W3-B's player prologue takes W3-A's lean path (one Season check), ≈ 200 CU each. The rest (Holding codec ≈ 2.7k, kernel accrual ≈ 7.3k, Depart's transfer CPI and record) is not reachable without a codec rewrite, so the budgets are amended (§5.5 v1.5): Harvest 17.5k, Train 17.5k, Explore 20k, Depart 24.5k (measured 16,373 / 16,605 / 18,829 / 23,133 + 5%). The print-only mode is removed: `g01_budget_w3b_*` fail on any breach |
| DisbandStranded breaks the roster freeze (major) | **Confirmed** | Pending `Forfeit` issued at `now_bell` for a roster host (state 1 or 2), `NotResident`/`HostBusy` as §21; freed by the resolve of that bell (W4-A; the svm stand-in does it). Tests: `host_disband_stranded_keeps_the_frozen_roster` (disband before vs after the resolve: identical roster), `host_disband_stranded_frees_a_host_of_a_gone_holding`. Keeper skips pending Forfeits; the native model follows |
| Build/tier-up ignore doctrine economy (major) | **Rebutted** | The shipping doctrine set is the kernel table built with `Doctrine::kernel`, whose food/ore/science multipliers are all `BPS_ONE` (`frontier-sim/src/model.rs` l. 274–305, 401); D Verdant 11,000 etc. are the **draft** table. Pinned by `frontier-sim` test `kernel_set_economy_is_neutral` (K7) |
| Troops never return (major) | **Confirmed (design gap)** | Assigned to W4-A by §21: the resolve keeps Leave entries (state 3, op Leave) until a class-D return settle credits `reserve[unit] += troops/1,000`; garrison withdrawals follow the same step. Dissolve stays enabled: no wave-3 path frees a Leave entry (RFI/SkipQuiet are stubs), so no troops can be lost before W4-A's return lands (K6) |
| Reveal headroom 3.4% (minor) | Confirmed, deferred (W5-A) | Presets and the Reveal budget/limit are regenerated from the final `.so` (`reveal_cu_limit` ≥ 26.4k if still needed) |
| Encodings in the program crate (minor) | Deferred (W4-D) | K11 |
| Mirror tier byte (minor) | Documented | Not authoritative until a Province write carries it (W4-A) |
| Reveal step order (minor) | Confirmed | Ended clause before the plaintext; `reveal_refusals` covers Ended + bad plaintext |
| SettleDeparture on a missing entry (minor) | Pinned | §21: the resolve keeps state-3 entries until SettleDeparture (W4-A) |
| `created_day` wording (minor) | Amended | §21 ("wrote the day's bit") |
| Record units (minor) | Pinned | §21 |
| `walk` vs `path_cost` (minor) | Deferred (W4-D) | K11 |
| Missing: G13 pending rows, presets | Deferred (W5-A) | unchanged |
| Missing: P1–P17 not in DECISIONS | Fixed | DECISIONS K10 |
| Missing: a SettleTicket holding through W3-B's actions | Fixed | `citizen_founded_holding_runs_the_holding_actions`: Harvest (the kernel's settle of W3-A's bytes equals the program's result), Train, Build, Muster (with the HOLDING_FINAL flip) |
| Missing: Works day cap | Known gap (contract counts Works only in M1) | unchanged |

### 6.3 W3-C keeper-land

| Item | Verdict | Disposition |
|---|---|---|
| Land index trusts JOIN records (major) | **Confirmed** | JOIN refused unless its wallet's canonical Citizen is the key; `LandIndex::citizen_of` re-checks a cached wallet and else reads the Citizen (displacement, ReleaseDormant, SettleExplore); PS2 bodies only from the program's own frame (below). Unit test with a forged JOIN |
| land_season exercises most duties only on the model (major) | **Confirmed** | The test detects `NotResident` on the first Muster and then skips only explore, keep-alive and the stranded host; sweep skips on the program because SweepPoolOwed is W4-B's stub. **On the review test-beacon `.so`** (sha256 `6555facc…`, the final review build): genesis, cohort (8 settlements, 6 winners, p99 3 slots), fold (bell 5, part 2 at 3 slots), crowding ring 4 at `ring_seed_round(t_open)` (5 rings / 61 provinces, then `NotCrowded`), displacement (gen 2), dormancy (7 releases after ≥ 3,600 s idle), expiry (bell 18 → 42) — all on the program; alerts asserted (only the expected `sweep` not-implemented) |
| `by_site` mixes cohorts (major) | **Confirmed** | Settle order oldest cohort first, then the lottery order; outcome predicted per candidate (a lower-ranked own-cohort ticket displaces with its triple). Unit test `settle_order_puts_the_oldest_cohort_first` |
| Greedy order displaces overlapping fallbacks (major) | **Confirmed** | Overlap rule (≤ 300 slots): unit test `overlap_hold_waits_for_a_better_mover`; localnet test `overlapping_fallbacks_never_displace` (A,B contest X, B falls back to Y, C only Y, D,E contest Z falling back to Y): 0 displacements and the lottery's owners, on the model and on the program |
| Archived cache source (minor) | Confirmed | Remembered caches re-checked every 150 slots; the archive once closed |
| Reveal accept checks (minor) | Confirmed | Season status first, this season's Holding at its canonical address, the Ended clause, the destination must exist; unit test extended |
| Weak assertions (minor) | Partly | (a) alerts asserted; (b) the model's own score stays the keeper's function, and the program run plus the F8 twin assertion cover it; (c) the class test comparing each duty's `WriteSpec` deferred (W5) |
| Crowding-ring cadence (minor) | Confirmed | Keeper policy (§21): d − 1 complete and a whole later fold |
| Missing: cross-cohort rule gap | **Raised** | DECISIONS K9 (architect decision before W4-C) |
| Missing: restart-rebuild and poisoned-feed tests | Partly | Forged JOIN and foreign-frame tests added; restart rebuild while tickets are open deferred (W5) |
| K1 resolved in fact, F8 twin | Fixed | the twin assertion above |
| DisbandStranded code with a live holding | Unchanged | the program answers `NotDormant` (46), as the model |

### 6.4 W3-D herald

| Item | Verdict | Disposition |
|---|---|---|
| Fold and findex accept any program's PS2 lines (major) | **Confirmed** | `fclient::log::bodies_from_logs(logs, program)` tracks the invoke stack and keeps a line only in the program's own frame; findex skips an undecodable body (no batch error, so no wedge on `Findex::open`). Tests: fclient frame cases (foreign top level, nested foreign CPI, failed foreign frame), findex `garbage_and_foreign_records_are_skipped` (no event number taken), herald `a_foreign_programs_ps2_lines_are_ignored` (a forged CLASH in a foreign frame: files identical to the clean archive, no mismatch). The chain-link and SeedCache cross-checks were not added (the frame filter removes the injection path) |
| WS client that never reads (major) | **Confirmed** | Every write bounded by 2 × heartbeat; a slow peer is closed 1013 and counted `dropped_slow`; test `a_client_that_never_reads_is_dropped` (duplex(512), 2,000 diffs, closes within bounded time, `open` back to 0) |
| `--source rpc` post-states inexact (minor) | Deferred (W5-C) | Recorded; the CLI still offers it |
| Durability gap on Same (minor) | Confirmed | A Same write is marked dirty for the next checkpoint sync (unit test) |
| `/h/me` quota bypass (minor) | Confirmed | `X-Forwarded-For: client_ip(peer, headers)` on `/f/quota` (`fclient::http::get_with_headers`) |
| Notes out of date (minor) | Confirmed | Addendum in `W3-D-NOTES.md` (no `.gz` siblings in the merged tree; §8.4 siblings open as J2) |
| Archive `sig`, Provisional respawn, blocking I/O, overview dormant flag (minor) | Deferred (W4-D, W4-A, W5-C, W5-C) | K11 |
| Missing items | As above; the ClashInput builder is W4-A's; the 5,000-viewer run is W5-C's |

### 6.5 W3-E bots-agents

| Item | Verdict | Disposition |
|---|---|---|
| SettleTransit from the latest envelope (major) | **Confirmed** | The bot fetches `/h/province/{P},{Q}/{arrive}` for every march whose destination resolved past it (`Observation.province_bells`, `province_at`); `settle_args` waits without it. The fixture serves such a per-bell envelope (slot index 2, beneficiary, resolved inputs with a resolver) and the settle-racer test asserts accounts 5, 8, 9 (it failed before) |
| Joins after `join_close_bell` (major) | **Confirmed** | `Mix::for_season_days` sets `join_close_bell = days × 108` (756 for 7 days), late joins uniform over `[144, close)`; `frontier-bots` re-deals with the season's own value; roster test asserts `join_bell < 756` and that late days are used |
| Journal/transport recovery, verdict codes, mock plumbing labels, `Hold` slot length, path search, 1,000-bot measurement (minors) | Deferred (W4-F) / notes | K11; the fleet tests stay labelled plumbing tests in these notes |
| `seed_source` ignores `present` (minor) | Confirmed | archive once archived, else a present cache of round S; unit test |
| `MeView` fails on `null` (minor) | Confirmed | skipped; unit test (and the web client too) |
| Ports and loopback prefix (minor) | Confirmed | §10.3 ports in the usage; authority parsed; unit test with 8 bypasses |
| Missing: control port 41070 | Deferred (W5 stack) | K11 |
| Missing: agents readers against the real herald | Partly | the per-bell envelope shape now matches the herald's; the committed cross-check with a running herald is W4-F's |

### 6.6 W3-F web-play

| Item | Verdict | Disposition |
|---|---|---|
| Stale earliest bell at send (major) | **Confirmed** | `DEPART_MARGIN_SECS` (90 s) in the composer's earliest bell; `sendMarch` recomputes it (`earliestAtSend`) and refuses `ArrivalBell` before sealing; the composer shows the new window. Test in `web-frontier-march.test.mjs` |
| Chronicle from event 1 (major) | **Confirmed** | `chronicleStart(headSeq)` = head − 500; up to four `full` pages per refresh (the herald client reports `full`) |
| Warnings at infantry pace (major) | **Confirmed** | `WARNING_UNIT` = Horseman (cavalry pace); the road-optimistic bound stays R4 (frontier-wasm owner) |
| Remote hosts invisible (major) | **Confirmed** | `hostProvinces(/h/me)` → their latest envelopes are loaded, so the Hosts tab and `hostRow` see them |
| Invite field never shown (major) | **Confirmed** | `inviteRequired(season)` and an `InviteRequired` refusal set the flag; the join screen also reads the season's gate |
| SettleExplore never enabled, `caches[0]` (major) | **Confirmed** | `FS.exploreSeedReady` from the explore bell's record; `seedSourceOf` (present cache of round S and THE anchor's A, or the archive) |
| Hidden page polls faster (major) | **Confirmed** | `pollDelay`: hidden → reveal-only tick every 60 s with backoff; `visibilitychange` refreshes |
| Minors | Confirmed and fixed: failed-send revival by the live transit (marchbook), SettleTransit only with the arrival bell's own envelope, cards for transits without an entry, garrison increases only; deferred: adjacent-wedge tickets (W4-E), the kernel for warnings now loads at start; the raw-size figure is restated in the W3-F addendum |
| Missing: no controller test | Fixed | `web-frontier-controller.test.mjs`: the controller's `refresh()` against a fake herald (remote provinces, invite flag, explore seed, chronicle start and paging) plus its pure rules |
| Missing: §9.4 landed-transaction check | Amended | §21 records D10 (relay-signature check) |
| PENDING-OWNER `frontier.wasm` / Playwright | unchanged | O-M1-12 |

## 7. Gate W3 re-run after the review response (contract v1.5 §12), 2026-09-28

Logs: `(session scratch)/scratchpad/integ-w3r/gate/` (`gate-w3.sh`, `run1-gate.txt` + `run1-logs/`, `gate-run2.txt` + `logs/item1..23.log`). `M1_PORTS` empty; every test binds `127.0.0.1:0`; no service started, nothing pushed, no devnet/mainnet transaction, nothing installed or downloaded by this session.

**First run, on `2086c5d` (05:23–05:43):** every item exit 0 except **item 10** (debug `frontier-node` tests): the new `overlapping_fallbacks_never_displace` used `wallet(200..205)`, whose `0x40 + i` overflows `u8` in the debug build (the release items passed). Fixed in `6be7848` (labels 150–154); the whole gate was run again.

**Second run, on `6be7848` (05:44–06:06)** (the working tree differed from HEAD only by these docs):

| # | Item | Result [measured] |
|---|---|---|
| 1–2 | fmt; clippy rules/abi | exit 0 |
| 3 | `permutation-rules` release tests | exit 0 (435) |
| 4 | `frontier-abi` tests | exit 0 (44) |
| 5 | `abi-vectors -- --check` | exit 0 (contract tag v1.5) |
| 6 | `permutation-chain` | exit 0 (157) |
| 7 | frontier-sim fmt, clippy, tests | exit 0 (415 s; includes `kernel_set_economy_is_neutral`) |
| 8 | criterion `--gate` | exit 0, worst cell **0.985** (unchanged) |
| 9 | `doctrine-gate --controls` | exit 0: kernel 6/6 in band, gap 1.4 points, max \|Δ\| 0.063%; draft 0/6, Knight 4/6, A boost 3/6 rejected (unchanged) |
| 10 | frontier-node fmt + clippy + debug tests | exit 0 (190 passed, 3 ignored) |
| 11 | gateway `npm ci && npm test` | exit 0 (481: 480 pass, 1 skip = `frontier.wasm`) |
| 12 | civilization | exit 0 (47/47) |
| 13 | session.mjs / permutation-chain untouched since `d95fa25` | exit 0 |
| 14 | clippy `permutation-frontier` | exit 0 |
| 15 | `permutation-frontier --no-default-features` | exit 0 |
| 16 | `build-frontier.sh --twice` | exit 0; both `file_sha256 e94141da…79ad`, `program_hash 7795667e…9209`, `.so` 563,200 B, `max_len` 704,512 |
| 17 | svm `g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_` | exit 0 (73) |
| 18 | frontier-node release tests | exit 0 (190 passed, 3 ignored) |
| 19 | gateway `npm test && sync-web-sdk --check` | exit 0 |
| 20 | `build-wasm` | **exit 1 — reported as PENDING-OWNER (O-M1-12), see below** |
| 21 | svm `g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_` | exit 0 (87) |
| 22 | `PSF_TRACE=1` g01 W3 lines | exit 0 (5) |
| 23 | frontier-node release tests | exit 0 (190 passed, 3 ignored) |

`web3.js conformance NOT RUN` appears 0 times in items 10, 18 and 23.

**Item 20 (build-wasm).** In the first run the `wasm32-unknown-unknown` target was absent and the item was `PENDING-OWNER` as §12 writes it. During the second run the target **appeared** in the 1.95.0 toolchain (its `rustlib` directory is dated 2026-09-28 05:46:52, two minutes after this run started); **this session did not install it** (no `rustup` command was run), and the brief relayed to this window says O-M1-12 is not approved. §12's condition then ran `scripts/build-wasm.sh --check`, which built the crate in a scratch directory (238,985 B raw, 78,373 B gzip, sha256 `5f15df6f…1113`, 28 exports + alloc/free/memory — under the 400-KB bound) and exited 1 only because no `web/frontier/wasm/frontier.wasm` is committed (none has ever been: producing it is the O-M1-12 step). Not knowing whether the install was approved, this window did **not** build or commit `frontier.wasm`; the item is reported as the PENDING-OWNER item the contract allows at wave 3. If the owner did approve O-M1-12: run `scripts/build-wasm.sh`, commit `web/frontier/wasm/frontier.wasm{,.sha256}`, and re-run the item (it then also un-skips the gateway's wasm test).

**Pass conditions:** Reveal worst **25,156 CU** ≤ 26,000 (FirstOfBell; Named 24,301, Displace 20,351), tx **916 B** ≤ 1,100, `L(reveal)` **753,664 B** at this `.so` (formula over the worst account set; unchanged); cohort tests green (item 21, now also `citizen_refile_keeps_the_escrow_funder` and the displaced-triple forgeries); herald fold determinism green (item 23, now with foreign-frame and slow-reader tests); keeper land tests over `localnet` green (items 10, 18, 23 on the native model). OpenProvince worst 148,423 CU, Join 12,066, FileTicket 16,060 (budget 17k), SettleTicket 22,035.

**Extra checks on the review build (not in §12):** the full svm suite (`./run.sh --release --no-fail-fast`): 144 passed, 1 ignored (the drill) — every record of every test passes the new `chains_of` tail check. On the test-beacon `.so` (sha256 `6555facc…`): `keeper` `one_day_beacons` (24.3 game h, journal 5,302 landed, 0 alerts), `reveal_accept_over_localnet`, `land_season` (all but the resident parts and SweepPoolOwed, which are W4-A's and W4-B's: genesis, cohort, fold, crowding ring 4, displacement gen 2, 7 releases, expiry) and `overlapping_fallbacks_never_displace` green; `herald` `real_chain` green (128 bell-region records, 476 events).

**Verdict:** Gate W3 green on `6be7848`: 22 of 23 items exit 0, and the one that did not is `build-wasm`, the PENDING-OWNER item the contract allows at wave 3 (circumstances above). `codex/frontier` is fast-forwarded to this branch's head after this notes commit.
