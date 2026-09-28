# W6-B program-fix: notes

- **Unit:** W6-B program-fix (wave 6, pass 1: prep and known fixes). **Branch:** `frontier/m1-W6-B`, cut from `frontier/m1-integ` at `0514b06` (contract v1.9). **Contract:** M1-CONTRACT v1.9 §3.2, §3.3, §3.5, §5.5, §5.7, §5.8, §5.11 (closes), §8.2, §10.2, §11 (W6-B row), §12 (Gates W1, W2, W5 lines on these paths), §13.1, §13.3; I-14, I-15, I-49, I-50, O-M1-04; amendments v1.8 §24 (Phase B row, the §8.2 open item) and v1.9 §25. **Date:** 2026-09-28.
- **Owner decisions in force:** O-M1-01…24 working defaults accepted; O-M1-12 items 1–3 approved (item 4, Agave ≥ 4.0, not approved). This unit used none of the approvals: no install, no download, no archive read, no Playwright. O-M1-04 (Phase B) was decided at the W5 gate (DECISIONS N10); this unit commits it. The 1.95.0 rustfmt/clippy install is already in DECISIONS part A: nothing added (DECISIONS is not this unit's file).
- **Wave note (pass 1):** the 7-day w6-s7 season is not run here (the main session runs it after this pass); no run triage list exists yet, so the "fixes from the runs" part of the brief is empty in this pass. What this pass owes: the Phase B commit and the program-side item deferred to W6 (contract v1.8 §24 §8.2, W5-A O4).
- **Not done, by rule:** no push; no devnet/mainnet transaction; no service and no port (every test is in-process LiteSVM or a host test); nothing in the main tree, `codex/magicblock-playable` or `codex/v9-security`; `permutation-server/web/session.mjs` untouched (`git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` exit 0). No manifest, lockfile, toolchain or `.gitignore` change.
- **Logs:** `(session scratch)/scratchpad/w6b/logs/` — `svm1.log` (full suite, Phase B only), `svm2.log`/`svm3.log` (full suite with the keeper-float change; run 2 had the two failures fixed in §3, run 3 is the green one), `cu1.log`/`cu3.log` (their `PSF_CU_LOG`), `trace-sweep.txt` + `trace.log`, `doctrine-gate-phase{A,B}.txt`, `criterion-phase{A,B}.txt`, `band-1500-phaseB.txt`, `node-tests.log`, `npm-test.txt`; `frontier-vectors.regen.diff` (what `fclient`'s vector writer produced on this branch, reverted: not this unit's file).

## 1. What landed

| Commit | What | Paths |
|---|---|---|
| `3c021bc` | **Phase B** (I-14, O-M1-04): one `sha256(cs ‖ 3 ‖ "eng" ‖ id)` per engagement gives both variance dice (bytes 8..16 attacker, 16..24 defender, mod the span), in `resolve_clash` and the reference `resolve_clash_ref` (shared helper `engagement_variances`). `CLASH_VERSION` 2 → **3**; `RULESET_HASH` `1ac11f85…` → **`72c6b5835ded6418ed98b0c00b2ae45ce4c4b082d9614447dbce2c9d2e654bd9`**; the golden digests regenerated (§2.2); RFI gate 340,000 → **290,000**, `MEASURED` 271,673, limit 285,500; `vectors/{budgets,presets}.json` regenerated | `permutation-rules/src/frontier/clash.rs`, `permutation-rules/tests/frontier_{clash_bounds,clash_equiv,shared,world}.rs`, `frontier-abi/src/{presets,budgets}.rs`, `frontier-abi/vectors/{budgets,presets}.json` |
| `4c41518` | **Keeper float closes on the tombstone** (W5-A O4, §24's §8.2 open item): CloseSeedCache, CloseClashInputs, CloseArrivalDay and CloseArrivalSlot close at once on the Closed tombstone; CloseClashInputs and CloseArrivalDay no longer read the Province once the Season has been Ended for 72 h (or on the tombstone). Test `clash::keeper_float_closes_on_the_tombstone` (failing first); `citizen_g13_file_ticket_forgery_shape_loaded` given loaded room for its shape probe; `MEASURED` of the four close kinds regenerated | `permutation-frontier/src/proc/{clash,beacon,season}.rs`, `permutation-frontier/svm-tests/tests/{clash,citizen}.rs`, `frontier-abi/src/budgets.rs`, `frontier-abi/vectors/budgets.json` |
| (this commit) | these notes | `docs/frontier/m1/W6-B-NOTES.md` |

## 2. Phase B

### 2.1 The kernel change

W5-A's lab diff (`m1/lab/phaseB-gate/phaseB-on-w5a.diff`) wrote the formula inline in both bodies; the commit puts it in one private helper both bodies call, `engagement_variances(rules, cs, id) -> (Bps, Bps)`. The preimage `cs ‖ 3 ‖ "eng" ‖ id` is exactly `rng::rand(cs, "eng", id)`'s, so Phase B is "the engagement id's hash, read as three words" (Phase A used word 0 as an id and hashed twice more through `combat::variance`). `combat::variance` stays for v9. The `rand` and `variance` imports left the clash kernel.

**Same kernel as the one the W5 gate adopted:** every regenerated digest (§2.2) came out identical on W5-A's independent lab tree (`phaseB-gate/tree-b`, Phase B applied to the kernel before W5-A's shared sort), so the doctrine gate, band and on-chain = native results of `phaseB-gate/RESULTS.md` are results for this commit, and the new `m1_rules_keep_the_phase_b_digests` still guards the shared sort (tree-b has the old sorts).

### 2.2 The golden tests (regenerated)

| Test | Before (Phase A) | After (Phase B) | Cross-check |
|---|---|---|---|
| `frontier_clash_bounds::clash_bounds_do_not_change_honest_outcomes` (300 honest clashes) | `c2c75bd7…` (recorded at `d95fa25`) | `4fe17f5e2f47f94a…e573567409` | = lab tree-b (W5-A's log and re-run) |
| `frontier_clash_equiv::occupancy_empty_keeps_the_phase_b_digests` (was `…_d95fa25_digests`; 4,320 fills without M1 rules) | `c3946cb0…` (d95fa25) | `6b0869a2125a7559…b7caf1d6f0` | = lab tree-b |
| `frontier_clash_equiv::m1_rules_keep_the_phase_b_digests` (was `…_31f1aa1_digests`; EMPTY / ROOM) | `679c2abe…` / `de82d520…` (31f1aa1) | `523069855d2df8d7…` / `615354a0878d281e…` | = lab tree-b (the test file run there) |
| `frontier_world::engagements_keep_the_v9_retaliation_rules` | dice restated from `combat::variance` | dice restated for Phase B (independently of the kernel helper) | the halving check's tolerance was 1 and held only for Phase A's dice: `without − 2 × with` is bounded by the two floors of `combat::damage` (the `/2` of an odd `x` and the variance scaling ≤ 1.1), so ≤ 3; Phase B's dice give 2. Now asserted as `≤ 3` with the derivation in the test |
| `frontier_shared::ruleset_hash_binds_versions_and_catalog` + `frontier_abi::presets::ruleset_hash_is_the_kernels` | `1ac11f85…` | `72c6b583…` | `presets.json` regenerated |

Each old value is kept in the test's doc comment. The two renamed tests are named in the contract (§12 Gate W1 pass condition "`Occupancy::EMPTY` digests identical to `d95fa25`", §25 `m1_rules_keep_the_31f1aa1_digests`): amendment request §6.

The equivalence tests (Phase A body = reference over 4,320 inputs, with occupancy, the refund corner, 5,000 tie/edge fills) pass unchanged: both bodies carry Phase B.

### 2.3 RFI budget [measured]

Full svm suite, release + test-beacon `.so`, `PSF_CU_LOG` (2,744 RFI transactions; 2,490 on the release build), `cu-table.py`:

| | Phase A (`budgets::MEASURED` at `0514b06`) | Phase B (this branch) |
|---|---:|---:|
| ResolveFromInputs G1 max | 327,609 | **271,673** (−55,936, −17.1 %) |
| mean (release) | — | 225,731 |
| gate | 340,000 | **290,000** (6.3 % margin) |
| CU limit (max + 5 %, rounded to 500) | 344,000 | **285,500** |
| heap max (trace sweep) | 15,320 B | 15,320 B |

W5-A's lab measured 274,007 on the lab tree; the 2,334 CU difference is integ-W5's clash-model move (RFI −2,254, DECISIONS N8). SkipQuiet's kernel-quiet roster is not a clash with engagements (no change). Every other kind's G1 maximum is unchanged by Phase B (`cu1.log` against the committed table).

### 2.4 Doctrine proxy gate and bot criterion (DECISIONS O5 rule) [sim, measured]

Paired: Phase A = a detached checkout of the wave base `0514b06` (removed afterwards), Phase B = this branch; same binary options, same seeds, run side by side.

**Doctrine proxy gate** `frontier-sim doctrine-gate --controls` (360 seasons, 60 gate seeds × 6 rotations, 10k wallets, Season-1 economy): both **exit 0**.

| | Phase A (base) | Phase B | Δ | m0c (`dk_final_gate_controls.md`, pre-D23 economy) |
|---|---|---|---|---|
| kernel table largest \|Δ index\| (bound ±0.2 %) | 0.063 % | **0.060 %** | −0.003 | 0.072 % |
| in band (proxy, 2.0-point SE) | 6/6, gap 1.4 | 4/6 (E 19.2 %, F 13.6 %), gap 3.1 | the win-rate band is not the proxy's pass condition (16.7 ± 10) | 4/6 (integ-W1 re-run) |
| conservation | 360/360 | 360/360 | | |
| control draft (must fail) | −1.291 % rejected | −1.319 % rejected | −0.028 | −2.06 % |
| control Knight (must fail) | −0.294 % rejected | **−0.242 %** rejected | +0.052 | −0.219 % |
| control A boost (must fail) | +0.338 % rejected | +0.326 % rejected | −0.012 | +0.384 % |

Identical to W5-A's lab figures for Phase B (0.060 %, −1.319 %, −0.242 %, +0.326 %). **Watch:** the Knight control is still rejected but only 0.042 points past the ±0.2 % bound (it was 0.094 with Phase A's stream); this is the same "Knight edge sits at the limit" the DESIGN §3.1 notes already record — the 1,500-season band below is the real check.

**O5 band** `doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate` on Phase B: **exit 0, 6/6 in 16.7 % ± 2**, largest gap **1.1 points**, largest |Δ index| **0.050 %** (SE ≈ 0.017), 1,500/1,500 conserve, 657 s. Win rates A 16.7, B 15.6, C 16.7, D 17.1, E 17.3, F 16.7 %. Paired control (Phase A on the same seeds, W5-A lab and integ-W1): 6/6, gap 1.1, 0.069 %. m0c: 6/6, gap 1.1, 0.043 %.

**Bot criterion** `criterion --best-response --seeds 3 --first-seed 30001 --gate`: both **exit 0**; worst bot choice **0.985 (Phase B) = 0.985 (Phase A)** (bots in office, 1 %, days 1–7 with stake). Per cell the Phase B − Phase A deltas are within ±0.002 (e.g. no-office 1 %: 0.983 vs 0.984; office 2 %: 0.981 vs 0.979; office 10 %: 0.955 vs 0.956). m0c: 0.980 worst on the pre-D23 economy (DESIGN §1 item 9); with D23 the m0c figure was 0.967 (bots in office), and the Season-1 economy the gate now runs gives 0.985 with either stream. Margin under 1.0: +1.5 points (unchanged).

**Verdict:** the outcome-changing kernel commit re-passes the doctrine gate (and its controls), the O5 band and the criterion, with deltas within sampling noise.

## 3. The keeper float (W5-A O4; contract v1.8 §24 §8.2 open item)

**Problem (code reading, then failing test):** CloseSeedCache needed Running/Ended (`ARCHIVE_STATUS`) and an archived bell; CloseClashInputs, CloseArrivalDay and CloseArrivalSlot needed Running/Ended/Aborted (`CLOSE_STATUS`). After CloseSeason's final part the Season is the 128-B Closed tombstone, so any cache, inputs, day or slot still open then (keeper float, test SOL in M1) could never close. A second hole on the same path: CloseClashInputs and CloseArrivalDay required the **Province** present even in their "Ended ≥ 72 h" fallback, while CloseProvince (anyone, from end + 72 h) closes it — so a CloseProvince that ran first locked that province's inputs and days (until, now, the tombstone).

**Fix (`4c41518`):**
- `season::float_tombstone` (the tombstone check CloseHolding/CloseCitizen already use) and `clash::FloatSeason` {`Live(hdr)`, `Tomb(id)`}: on the tombstone the close's own timing rules do not apply — every reader of these accounts (Reveal, GatherClash, ResolveFromInputs, SkipQuiet, SettleTransit, ClaimDefence, PostSeed consumers) needs Running or Ended, so nothing can read them again.
- CloseClashInputs, CloseArrivalDay: with the Season Ended ≥ 72 h or on the tombstone the Province is **not read** (its canonical address is still checked: `BadAddress`); it may be absent. Before that point the rules are unchanged (Province present, `InputsOpen`/`TooEarly`).
- CloseArrivalSlot: on the tombstone case (b) applies; in (b) and on the tombstone the optional anchor is not read (before, a wrong anchor address in case (b) was `BadAddress`; now it is ignored).
- CloseSeedCache: on the tombstone the archive is not read (part 9 may have closed it; its canonical address is still checked).
- Unchanged: the target at its canonical address and present (`BadAddress`, absent cache `AlreadyDone`), `rent_to` = the stored one, `CLOSE` logged (bell `NO_BELL` on the tombstone, as CloseSeason logs there), rent to `rent_to`.

**Test** `svm-tests/tests/clash.rs::keeper_float_closes_on_the_tombstone` (crafted fill + gather; the tombstone crafted as the final part leaves it — first 128 bytes, status Closed; the real parts are run by `lifecycle::close_holding_and_citizen_on_the_tombstone`): Ended ≥ 72 h with the Province removed → inputs and day close (a wrong Province address is `BadAddress`); Running → `InputsOpen`/`TooEarly` for all four; tombstone with Province and THE anchor removed and no archive → all four kinds close, each pays its rent to `rent_to` and logs one `CLOSE` at `NO_BELL` with those lamports; wrong cache and wrong slot addresses `BadAddress`; a repeat cache close `AlreadyDone`. **Failing first** on the pre-fix `.so` (`PSF_SKIP_BUILD=1`, the run-1 build): `BadAccount` on the Ended/Province-closed close and on the tombstone.

**Side effect fixed:** the release `.so` grew 873,504 → **875,768 B** (+2,264), which moved `max_len` up one 4-KiB page and consumed the **267-B** loaded-data margin `citizen_g13_file_ticket_forgery_shape_loaded` relied on to reach the program's `TooManyAccounts` with a fourth Province (the runtime refused it first, `MaxLoadedAccountsDataSizeExceeded`). The probe now requests `L(FileTicket)` + one Province + one page; `loaded_check` of the real three-Province shape is unchanged. `g01_loaded_limit_table_covers_the_release_so` passes: 875,768 B against the table's `PLACEHOLDER_SO_LEN` 884,736 (8,968 B left).

## 4. Measurements [measured]

| What | Value |
|---|---|
| Release `.so` (`build-frontier.sh --twice`, identical) | **875,768 B**, `file_sha256 072b1205f92a16131d4c29753807de5720344e38a99ec24bb83e04a5409da98b`, `program_hash 57637b98…96aa`, e_flags 2, `max_len` 1,097,728, deployable. Phase B alone: 873,504 B (`24fb3073…`) |
| Test-beacon / trace / oracle `.so` | 876,272 / 887,416 / 880,832 B |
| `MEASURED` changes (full suite, `cu3.log`) | ResolveFromInputs 327,609 → 271,673; CloseSeedCache 5,590 → 5,642; CloseClashInputs 7,064 → 7,187 (limit 7,500 → **8,000**, = the gate); CloseArrivalDay 5,622 → 5,637; CloseArrivalSlot 6,120 → 6,155; every other kind unchanged. `cu-table.py --check` exit 0 |
| Heap (trace sweep, 49 kinds, 26,483 transactions) | 0 heap-gate failures; max 15,320 B (ResolveFromInputs); the four closes 904–976 B. The 4 failures are the by-design ones on trace builds (two CU sends at the table's limit, the `.so` size guard, the build-marker check) |
| `frontier.wasm` built from this branch (`build-wasm.sh --check`, not written) | 190,768 B raw, 69,086 B gzip, sha256 `1166e114…336e7d` — **stale** against the committed artefact (W6-D, §5) |

## 5. What I ran

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings`; same `-p permutation-frontier` | exit 0 / exit 0 |
| `cargo test --locked --release -p permutation-rules` | **437 passed, 0 failed** (before the regeneration: 5 failed as a rules change must — the three goldens, `m1_rules_keep_the_31f1aa1_digests` and the ruleset-hash pin — plus the retaliation test's tolerance) |
| `cargo test --locked -p frontier-abi`; `cargo run --locked -p frontier-abi --bin abi-vectors -- --check` | 47 passed; `9 files fresh` |
| `cargo test --locked -p permutation-chain` | 157 passed |
| `cargo test --locked -p permutation-frontier` (and `--no-default-features`) | 83 + 37 passed |
| `(cd permutation-frontier/svm-tests && RELEASE_CHECK=1 PSF_CU_LOG=… ./run.sh --release --no-fail-fast)` — Phase B only (run 1) | exit 0: **244 passed**, 0 failed, 4 ignored |
| the same with the keeper-float change (run 2, rebuilt) | 243 passed, **2 failed** (my test's expected code for a wrong cache address, §3; the loaded margin, §3) → fixed |
| the same, `PSF_SKIP_BUILD=1` on the run-2 builds (run 3; the program did not change after run 2) | exit 0: **245 passed**, 0 failed, 4 ignored |
| `svm-tests` `cargo fmt -- --check`, `cargo clippy --locked --release --all-targets -- -D warnings` | exit 0 |
| `svm-tests/trace-sweep.sh` | exit 0, 0 heap-gate failures (§4) |
| `svm-tests/cu-table.py cu3.log --write`, then `--check` | written; exit 0 |
| `scripts/build-frontier.sh --twice` | exit 0, identical hashes (§4) |
| `(cd frontier-sim && cargo test --locked --release)` | 16 + 4 passed (10 min 49 s) |
| `doctrine-gate --controls` (Phase A and Phase B), `criterion --best-response --seeds 3 --first-seed 30001 --gate` (both), `doctrines … --seeds 250 --first-seed 10000 --set kernel --gate` (Phase B) | all exit 0 (§2.4) |
| `(cd frontier-node && cargo test --locked --release --workspace --no-fail-fast)` | **325 passed, 7 failed, 10 ignored** — every failure is a consumer of the Phase B outcomes or the RFI gate, in W6-C's paths (§6 R1). The run also rewrote `permutation-gateway/test/frontier-vectors.json` (fclient's vector writer); reverted, diff kept in the scratch logs |
| `(cd permutation-gateway && npm ci --ignore-scripts && npm test)`; `node scripts/sync-web-sdk.mjs --check` | npm ci exit 0 (registry cache); **512/515** — the 3 failures are generated-file freshness (§6 R1, R2); sync `--check` lists the four ABI modules (budgets, presets) out of date |
| `scripts/build-wasm.sh --check` | stale (§4), expected: the kernel changed |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |

Not run (not this unit's in pass 1): Gate W6's `frontier-stack` lines (W6-A / integrator; w6-s7 by the main session), `m1-nightly.sh`, `mutate.sh`, the Gate W4 `--include-ignored inproc_`/keeper-play lines (they depend on the regenerated fixtures of R1), the screens line.

## 6. Dependency requests and outcome vectors to regenerate (by their owners)

Phase B changes every clash outcome with an engagement and the ruleset hash; the budgets moved. The files below are not this unit's; I did not change them (DECISIONS N10: "every outcome vector regenerated by its owner"). Merge order puts W6-B first, so the other wave-6 units (cut from the same base) will see the failures only after the merge: **the integrator should run the regenerations in the window, or hand them to W6-C/W6-D.**

| # | What (failing today on this branch) | Owner | Command / change |
|---|---|---|---|
| R1a | `fclient::abi::twin_tests::instructions_are_frontier_abis` (ResolveFromInputs 340,000 vs 290,000) | W6-C | `frontier-node/crates/fclient/src/abi.rs:178` → `290_000`; check `fclient/src/vectors.rs:316`'s cost row `(340_000, 1, 3, …)` and `keeper/src/engine.rs:969`'s `Some(340_000)` (not failing; confirm they are not RFI) |
| R1b | `permutation-gateway/test/frontier-vectors.json` (clash_model digests, budgets) | W6-C (generated) | `(cd frontier-node && cargo test --locked -p fclient writes_frontier_vectors)` |
| R1c | `verify` `march_program_passes`, `march_synth_passes` (V7 `ClashReplayMismatch`), `tamper_suite_on_the_program_recording`, `tamper_suite_on_a_second_recording` ("the base run must PASS"), `stack::verifyrun::tests::the_suite_on_the_committed_program_recording`, `itest native_rerun::every_resolved_and_skipped_bell_reruns_natively` (39 of 94 clashes' digests differ) | W6-C | re-record `frontier-node/fixtures/verify/march-program.json.gz`, `march-program-dc1281c3.json.gz` (the wave-base second recording must now be a second **Phase B** recording), `march-synth.json`, `land-program.json` (ruleset hash) and the itest recording on the Phase B test-beacon `.so`; the herald-recorded agents fixture `frontier-node/crates/agents/fixtures/herald-recorded/h/season.json` pins the ruleset hash (O11's re-record for an unrested march can be the same recording) |
| R1d | `permutation-server/web/sdk/frontier/abi-{budgets,presets}.mjs`, `permutation-gateway/client/src/frontier/abi-{budgets,presets}.mjs` (npm freshness tests) | W6-C (generated) | `node permutation-gateway/scripts/sync-web-sdk.mjs` |
| R2a | `permutation-server/web/frontier/abi.mjs` freshness (npm) | W6-D | regenerate from the ABI vectors |
| R2b | `permutation-server/web/frontier/wasm/frontier.wasm{,.sha256}`, `frontier-wasm/vectors/wasm-vectors.json` (ruleset hash) | W6-D | `scripts/build-wasm.sh` (190,768 B) and the wasm vectors; `web-frontier-practice.test.mjs` reads the clash_model vectors of R1b |
| R2c | `permutation-gateway/test/fixtures/frontier/season.json` (ruleset hash) | W6-D | `node permutation-gateway/test/fixtures/frontier/make-fixtures.mjs` |
| R3 | The release `.so` hash changes (`072b1205…` on this branch; the integ head's differs) | W6-A | `scripts/m1-run-s7.sh` records the `.so` sha256 at run time; nothing pinned should carry an older one |
| R4 | c4 v3 and DESIGN: the RFI distribution is Phase B's (§2.3) | W6-E | cite §2.3 |
| R5 | Keeper: the float can now be closed on the tombstone and after CloseProvince (§3); the keeper's season-end closer may drop any ordering it kept for that reason | W6-C (optional) | — |

No manifest, lockfile, toolchain or `.gitignore` change.

## 7. Contract amendment requests (for the integrator)

| Section | Change | Evidence |
|---|---|---|
| I-14, O-M1-04, §5.5, §10.2 | Phase B **committed** (`CLASH_VERSION` 3); ResolveFromInputs gate **290,000** in force (the "340k (290k with Phase B)" row becomes 290k), G1 max 271,673, limit 285,500; CloseClashInputs limit 8,000 | §2.3, §4 |
| §3.2 | `RULESET_HASH` = `72c6b5835ded6418ed98b0c00b2ae45ce4c4b082d9614447dbce2c9d2e654bd9` | §2.2 |
| §12 Gate W1 pass conditions, §25 §7 row | "`Occupancy::EMPTY` digests identical to `d95fa25`" → "identical to the Phase B recording (W6-B)"; tests renamed `occupancy_empty_keeps_the_phase_b_digests`, `m1_rules_keep_the_phase_b_digests` (old values in their doc comments; the new ones cross-checked on W5-A's independent Phase B lab tree) | §2.2 |
| §5.8 CloseSeedCache, §5.11 CloseClashInputs / CloseArrivalDay / CloseArrivalSlot, §8.2 (closes §24's open item) | On the Closed tombstone each closes at once (canonical target, `rent_to`, `CLOSE` at `NO_BELL`); CloseSeedCache does not read the archive there; with the Season Ended ≥ 72 h or on the tombstone CloseClashInputs and CloseArrivalDay do not read the Province (canonical address only, may be absent); CloseArrivalSlot does not read the anchor in case (b) or on the tombstone | §3; `keeper_float_closes_on_the_tombstone` |
| DECISIONS | a row for §3 (closes N11's "keeper float after the final CloseSeason part → architect") | §3 |

## 8. Open

- **W5-A O3** (SettleTicket's archive-entry seed path is dead in M1 unless `archive_after` < 4 h) — an architect item; no program change here (the path stays, crafted-archive test green).
- **Knight control margin** in the proxy gate is 0.042 points (§2.4): the gate still rejects it; worth watching in the nightly band, not a failure.
- The program-side triage of the 7-day run belongs to the fix pass after the main session's w6-s7 run.

## Links

- Branch `frontier/m1-W6-B` in `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/m1-W6-B` (commits `3c021bc`, `4c41518`, this notes commit)
- Kernel: `permutation-rules/src/frontier/clash.rs` (`engagement_variances`, `CLASH_VERSION`)
- Program: `permutation-frontier/src/proc/clash.rs` (`FloatSeason`, the three closes), `permutation-frontier/src/proc/beacon.rs` (`close_seed_cache`), `permutation-frontier/src/proc/season.rs` (`float_tombstone`)
- Test: `permutation-frontier/svm-tests/tests/clash.rs::keeper_float_closes_on_the_tombstone`
- Phase B lab: `(session scratch)/scratchpad/frontier/m1/lab/phaseB-gate/RESULTS.md`
- Logs: `(session scratch)/scratchpad/w6b/logs/`
