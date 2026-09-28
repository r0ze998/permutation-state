# W5-A program-gates: notes

- **Unit:** W5-A program-gates (wave 5, hardening). **Branch:** `frontier/m1-W5-A`, cut from `frontier/m1-integ` at `31f1aa1` (contract v1.7). **Contract:** M1-CONTRACT v1.7 §3.5, §5.5, §10.2, §11 (W5-A row), §12 (Gate W5), §13.1–§13.3 (G1–G13), I-14, I-45, I-50, O-M1-04. **Date:** 2026-09-28.
- **Owner decisions in force:** O-M1-01…24 working defaults accepted; O-M1-12 items 1–3 approved 2026-09-28 (item 4, Agave ≥ 4.0, not approved). This unit needed none of them: no install, no download, no fetch, no Playwright, no archive read. O-M1-04 (Phase B) is decided here on its working default (§4). O-M1-18 not approved (nothing devnet). The rustfmt/clippy install is already in DECISIONS part A (row of 2026-09-27) and H3: nothing added.
- **Not done, by rule:** no push, no devnet/mainnet transaction, no service or port (every test is in-process LiteSVM or a host test), no file of the main tree, `codex/magicblock-playable` or `codex/v9-security`; `permutation-server/web/session.mjs` untouched.
- **Logs and labs:** `(session scratch)/scratchpad/w5a/` (`final.log` the last full svm run, `cu3.log` its CU log, `heap.log` + `heap-run.log` the trace sweep, `inproc.log`, `fnode.log`, `npmtest.log`, `gate-*.log`); the Phase B lab `(session scratch)/scratchpad/frontier/m1/lab/phaseB-gate/` (`RESULTS.md`, `run.sh`, `logs/`).

## 1. What landed

| Commit | What | Paths |
|---|---|---|
| `3767652` | **One shared sort in the clash kernel** (wave note 3). Every on-chain ordering in `frontier::clash` encodes its key into three `u64` words and goes through one natural merge sort of precomputed keys (`sort_keys`), with an in-place insertion path up to 12 elements and a sorted fast path; the position is the last key word, so the order is exactly `sort_by_key`'s (stable). The host-only reference `resolve_clash_ref` keeps the core sorts, and `frontier_clash_equiv` (4,320 inputs) plus the new `sort_keys_equal_the_core_stable_sort` compare them. **Release `.so` 1,032,184 → 865,640 B**; digests unchanged | `permutation-rules/src/frontier/clash.rs` (W1-A's kernel file; see §6 R1) |
| `7e18265` | **CloseSeason float parts 8–10** (wave note 4, W4-B F3): part 8 RingSeeds → stored `payer`, part 9 AnchorArchives → `rent_to`, part 10 DefenceClaims → `beneficiary`, as `[target w] [recipient w]` pairs in the existing repeat group (no new tag, no ABI field). **`heap::scoped`** (W4-A D9). **`PSF_CU_LOG`** in the harness | `permutation-frontier/src/{proc/season.rs, heap.rs, proc/clash.rs}`, `svm-tests/{src/chain.rs, src/ix/season.rs, tests/lifecycle.rs}` |
| `bf64955` | **G13 complete** (`RELEASE_CHECK=1` green: 0 Pending, every code asserted) and **G7 in LiteSVM**; `g01_loaded_limit` for the 25 kinds that had none | `svm-tests/tests/{g13_complete.rs (new), g07_lag.rs (new), citizen.rs, clash.rs, transit.rs, defence.rs, reveal.rs, coverage.rs, common/mod.rs}`, `svm-tests/src/cover/*.rs`, `src/chain.rs` |
| `bf3ca03` | **The bounced-resident Leave rule**, end to end (DECISIONS M16: test W5-A) | `svm-tests/tests/clash.rs`, `src/cover/host.rs` |
| `2dc0ec8` | **Budgets regenerated**: CU limits = G1 maxima + 5 %; `L(kind)` at the release `.so`; presets `reveal_cu_limit`, `reveal_loaded_limit`; abi-vectors; svm guard `g01_loaded_limit_table_covers_the_release_so` | `frontier-abi/src/budgets.rs`, `frontier-abi/vectors/budgets.json` (owned); `frontier-abi/src/presets.rs`, `vectors/{presets,ix}.json` (§6 R2) |
| `e32e837` | `frontier.wasm` rebuilt for the kernel change (238,985 → 190,979 B raw) | `permutation-server/web/frontier/wasm/frontier.wasm{,.sha256}` (integrator's artefact, §6 R3) |
| (this commit) | these notes; `svm-tests/README.md` (`PSF_CU_LOG`) | `docs/frontier/m1/W5-A-NOTES.md` |

### 1.1 CloseSeason parts 8–10 (v1.8 amendment request, §7)

- Accounts: the ten fixed ones of every part, then 1–24 pairs `[target w] [recipient w]` in the repeat group (`close_counts`; an odd or empty list is `TooManyAccounts`; a part above 10 `BadData`). Each target is found by its **own key fields** (RingSeed `d`; AnchorArchive `(region, part)`; DefenceClaim `(beneficiary, day)`) and must sit at their canonical address (`BadAddress`); the recipient must be the stored `payer` / `rent_to` / `beneficiary` (`BadAccount`); an absent or pre-funded target is skipped (a repeat lands). Each close logs `CLOSE` (short-header key: `le16 d`, `region ‖ le32 part`, `keeper_tag8 ‖ le32 day`) with the recipient.
- Status: part 8 like parts 0–6 (Ended ∧ `now ≥ end + 72 h`, Aborted, or the Closed tombstone). Parts 9 and 10 **only on the tombstone or an Aborted season** (`TooEarly` otherwise): SettleTransit, SettleTicket, SettleExplore read archives and ClaimDefence writes claims while the season is Running or Ended, and a closed claim re-created by ClaimDefence would reset its caps. Re-creation after the close stays refused by status (I-46: ArchiveAnchors and OpenRing need Running/Ended resp. Seeded/Running).
- G1 [measured, release `.so`]: 10 pairs (the most a legacy transaction fits with distinct recipients) — part 8 **28,028 CU**, part 9 **28,589**, part 10 **35,169**, ≤ 1,177 B, ≤ 32 locks, within CloseSeason's 60k. **Loaded data:** `L(CloseSeason)`'s worst set is the 48 JoinShards, so part 9 fits **two** archive pairs at `L(CloseSeason)`; a client closing more archives adds 6,272 B per pair to its loaded-data limit (measured 1,152,231 B for 10). Request: the prologue table could list the group as `Either(JoinShard, AnchorArchive)` so `L(kind)` covers it (§6 R5).
- Float released per 7-day season [computed]: 16 regions × 14 half-days = 224 AnchorArchives × 31,861,760 lamports ≈ **7.1 SOL** (v1.3 estimated ≈ 8.2 SOL), plus the RingSeeds and DefenceClaims (1,300,480 lamports each).

### 1.2 G13 (§13.3) — the 35 Pending rows

Every `Cover::Pending` of wave 4 is now a test that asserts its codes: `tests/g13_complete.rs` (season, beacons, map, the player prologue per instruction, Depart/SettleDeparture, Explore/SettleExplore) and additions to the area files (citizen, transit, defence, clash, reveal). The shape refusals use one mutation per code for every instruction (an account too many or too few, a writable account read-only or a builtin replaced, a signer that did not sign, data one byte short). Rows the wave-4 notes named explicitly: ConsumeRingSeed **G1 over the 32 real quicknet rounds on the release `.so`** (worst **332,701 CU**, round 32,551,846; budget 345k); RFI `Kernel` through a stored roster with a duplicated host id; SettleTicket's archive-entry seed path (see §5 O3); SettleExplore `SeedNotReady` through an absent archive.

**Two codes no M1 path emits** — `SiteTaken` (11: a taken site is the SETTLE outcome `taken`, I-47) and `Aborted` (50: every instruction refuses an Aborted season with `WrongStatus`) — are listed as reserved in `cover::EXEMPT` with their reason, and the exempt guard names them. Amendment request §5.4 (§7).

`RELEASE_CHECK=1 ./run.sh --release`: **243 passed, 0 failed, 4 ignored** (the W2-B drill and W4-A's three `zz_profile_*` diagnostics); the coverage test prints `0 Pending entries`, `0 codes no test asserts yet`.

### 1.3 G7 in LiteSVM (`tests/g07_lag.rs`)

One march (900 troops, faction 0) two provinces east onto a hostile resident, run twice on the test-beacon binary. **Unheld:** the origin resolves, SettleDeparture lands, the origin region's anchor is posted, the march is revealed, the destination gathers and resolves at its close. **Held:** the origin Province, the origin region's anchor and SettleDeparture are held past the destination's close; the reveal still lands (Reveal accepts a departed transit); the destination's gather is refused `DepartureUnsettled` (the hold took effect; lag only waits); three bells later the holds are lifted and the destination gathers and resolves. **Byte-identical:** the ClashInputs records and postures, the CLASH `input_digest` and `outcome_digest` (`cbcafdfa…4e7e`), and the Province's game state (all bytes but the event header and the resolve summary). The arrival is in the inputs and fought (`engagements > 0`).

### 1.4 The bounced-resident Leave rule

`clash_bounced_resident_leaves_and_returns`: a native search over the adversarial kinds finds the first fill whose kernel outcome bounces a **resident** (dense#1, host `0x580010000000d`, 5,049,000 milli-troops); on chain the resolve keeps it as a departed `Leave` entry issued at the bell with its post-clash troops; the return settle (SettleDeparture `0xFF`, repeated past `RETURN_MAX = 3` until `AlreadyDone`) credits `reserve += troops / 1,000` to its live Holding (generation 1) and frees it; with the Holding re-founded the troops are lost (`STRANDED`). The comment in `clash_return_settle_loses_troops_of_a_refounded_holding` that pointed at the missing test is updated.

## 2. Measurements [measured unless marked]

Toolchain: cargo-build-sbf 3.1.9, platform-tools v1.52, SBPF v2, LiteSVM 0.16 (mainnet features, SIMD-0186 enforced by the harness).

### 2.1 Program size (wave note 3)

| Build | `.so` | `max_len` | note |
|---|---:|---:|---|
| v1.7 base `31f1aa1` | 1,032,184 B | 1,290,240 | 58 `core::slice::sort` functions, 186,472 B of them |
| + shared sort (`3767652`) | **865,640 B** | 1,085,440 | 7 sort functions, 4,992 B |
| + CloseSeason parts (branch head) | **869,536 B** | **1,089,536** | `build-frontier.sh --twice`: identical hashes (§3) |
| `frontier.wasm` | 238,985 → **190,979 B** raw (78,358 → 69,192 B gzip) | | sha256 `e3e5405c…b5f6` |

The shared sort is CU-neutral or better: first attempt (a `dyn` comparator merge sort) cost +12.6k CU on RFI and was reworked with the kernel's own CU probes (`cu-trace`, kernel steps K1–K6) until every step was at or below the base: **RFI over the 1,244 fills max 331,843 → 329,863 CU, mean 262,200 → 260,636, min 153,623 → 152,206; heap max 15,080 → 15,320 B; SkipQuiet kernel-quiet roster n = 24 112,410 → 111,436, n = 1 82,338 → 81,364.** 1 MiB for the loaded-data working default would need a `.so` ≤ 838,860 B (30 KB more); not pursued (the budgets table now uses the real length, §2.3).

### 2.2 G1 per kind: CU, heap, bytes, locks

G1 maxima are the largest CU of each kind over every landed transaction of a full `--release` svm run (release and test-beacon `.so`, 26,323 transactions logged with `PSF_CU_LOG`, the §13.1 worst fills included). Heap is the trace build's peak over the same suite run on the trace builds (`--features trace`, and `test-beacon trace` built into scratch for the test-beacon tests; 26,074 transactions). Every heap peak is **≤ 15,320 B** (gate 28,672); the v1.7 paths: SettleTransit 2,488 B (camp citizen, stamped destination), GatherClash 6,064 B (the stamp), SkipQuiet 12,312 B (provisional camp check, stop), ClaimDefence 3,280 B, CloseSeason 6,832 B (parts 8–10 included). "max tx B" and "max locks" are the largest observed, not a proof of the worst.

| Kind | gate CU | G1 max CU | margin | CU limit (max + 5 %) | old limit | heap peak (trace) | max tx B | max locks | landed txs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| AnnounceSeason | 25,000 | 18,608 | 25.6 % | 20,000 | 25,000 | 984 | 375 | 6 | 366 |
| CreateSeason | 70,000 | 42,198 | 39.7 % | 44,500 | 70,000 | 4,394 | 802 | 13 | 290 |
| InitBeaconLogs | 80,000 | 74,690 | 6.6 % | 78,500 | 80,000 | 8,336 | 813 | 21 | 276 |
| InitShards | 45,000 | 39,314 | 12.6 % | 41,500 | 45,000 | 4,418 | 550 | 13 | 1,635 |
| ConsumeGenesisSeed | 345,000 | 329,426 | 4.5 % | 346,000 | 345,000 | 400 | 599 | 4 | 272 |
| EndSeason | 10,000 | 3,298 | 67.0 % | 3,500 | 10,000 | 400 | 252 | 4 | 12 |
| CloseSeason | 60,000 | 37,423 | 37.6 % | 39,500 | 60,000 | 6,832 | 1,177 | 32 | 32 |
| AbortSeason | 20,000 | 4,567 | 77.2 % | 5,000 | 20,000 | 784 | 318 | 6 | 9 |
| SetWindowSchedule | 5,000 | 3,610 | 27.8 % | 4,000 | 5,000 | 400 | 260 | 4 | 17 |
| PostAnchor | 345,000 | 339,142 | 1.7 % | 356,500 | 345,000 | 1,212 | 768 | 8 | 219 |
| PostAnchorMulti | 400,000 | 380,130 | 5.0 % | 399,500 | 400,000 | 4,932 | 1,165 | 20 | 12 |
| PostSeed | 345,000 | 339,178 | 1.7 % | 356,500 | 345,000 | 1,216 | 769 | 8 | 71 |
| PostBeacon | 340,000 | 332,479 | 2.2 % | 349,500 | 340,000 | 592 | 633 | 5 | 20 |
| ArchiveAnchors | 60,000 | 45,441 | 24.3 % | 48,000 | 60,000 | 4,888 | 924 | 23 | 17 |
| CloseSeedCache | 6,000 | 5,590 | 6.8 % | 6,000 | 6,000 | 976 | 357 | 7 | 4 |
| OpenRing | 30,000 | 15,210 | 49.3 % | 16,000 | 30,000 | 2,128 | 551 | 13 | 183 |
| ConsumeRingSeed | 345,000 | 332,701 | 3.6 % | 349,500 | 345,000 | 592 | 634 | 5 | 34 |
| OpenProvince | 220,000 | 148,459 | 32.5 % | 156,000 | 220,000 | 1,168 | 388 | 8 | 442 |
| FoldOccupancy | 30,000 | 28,737 | 4.2 % | 30,500 | 30,000 | 5,344 | 1,078 | 29 | 35 |
| CloseProvince | 10,000 | 5,502 | 45.0 % | 6,000 | 10,000 | 784 | 322 | 6 | 8 |
| Join | 25,000 | 12,495 | 50.0 % | 13,500 | 25,000 | 1,480 | 523 | 9 | 1,086 |
| SetSession | 6,000 | 5,259 | 12.3 % | 6,000 | 6,000 | 712 | 326 | 5 | 63 |
| SetVigil | 6,000 | 5,204 | 13.3 % | 5,500 | 6,000 | 712 | 288 | 5 | 4 |
| FileTicket | 17,000 | 16,096 | 5.3 % | 17,000 | 17,000 | 1,672 | 487 | 10 | 76 |
| SettleTicket | 40,000 | 22,383 | 44.0 % | 24,000 | 40,000 | 2,704 | 649 | 16 | 33 |
| ReleaseDormant | 25,000 | 11,864 | 52.5 % | 12,500 | 25,000 | 1,552 | 450 | 10 | 4 |
| CloseHolding | 15,000 | 6,750 | 55.0 % | 7,500 | 15,000 | 976 | 351 | 7 | 5 |
| CloseCitizen | 10,000 | 6,191 | 38.1 % | 7,000 | 10,000 | 904 | 319 | 6 | 7 |
| Harvest | 17,500 | 16,409 | 6.2 % | 17,500 | 17,500 | 904 | 319 | 6 | 8 |
| Build | 22,000 | 19,145 | 13.0 % | 20,500 | 22,000 | 1,096 | 353 | 7 | 10 |
| Train | 17,500 | 16,641 | 4.9 % | 17,500 | 17,500 | 904 | 324 | 6 | 5 |
| Muster | 25,000 | 20,669 | 17.3 % | 22,000 | 25,000 | 1,096 | 358 | 7 | 6 |
| Dissolve | 25,000 | 18,602 | 25.6 % | 20,000 | 25,000 | 1,096 | 360 | 7 | 10 |
| Garrison | 25,000 | 18,052 | 27.8 % | 19,000 | 25,000 | 1,096 | 360 | 7 | 3 |
| Explore | 20,000 | 18,865 | 5.7 % | 20,000 | 20,000 | 1,096 | 363 | 7 | 7 |
| SettleExplore | 15,000 | 8,281 | 44.8 % | 9,000 | 15,000 | 1,168 | 384 | 8 | 5 |
| DisbandStranded | 12,000 | 5,439 | 54.7 % | 6,000 | 12,000 | 784 | 319 | 6 | 7 |
| Depart | 24,500 | 23,167 | 5.4 % | 24,500 | 24,500 | 1,288 | 604 | 8 | 114 |
| Reveal | 26,000 | 25,155 | 3.2 % | 26,500 | 26,000 | 2,810 | 916 | 20 | 116 |
| SettleDeparture | 15,000 | 10,552 | 29.7 % | 11,500 | 15,000 | 784 | 319 | 6 | 85 |

*Integrator note (wave-5 review, integ-W5r):* the SettleDeparture row above was wrong — the full-suite log already held a return settle at 18,344 CU. The return settle's worst fill is 43,083 CU (Holding absent) / 13,240 CU (live) after the scan rework; the gate is now 48,000 and the limit 45,500 (contract v1.9 §25, DECISIONS O2). The table is regenerated by `svm-tests/cu-table.py`; the float parts' "10 pairs is the most a legacy transaction fits" held only for part 10 with distinct recipients, and the parts are now capped (10 / 2 / 10, O3).
| SettleTransit | 85,000 | 63,281 | 25.6 % | 66,500 | 85,000 | 2,488 | 783 | 13 | 44 |
| SweepPoolOwed | 8,000 | 5,066 | 36.7 % | 5,500 | 8,000 | 784 | 318 | 6 | 5 |
| GatherClash | 49,000 | 46,347 | 5.4 % | 49,000 | 49,000 | 6,064 | 1,218 | 32 | 7,851 |
| ResolveFromInputs | 340,000 | 329,863 | 3.0 % | 346,500 | 340,000 | 15,320 | 453 | 9 | 2,732 |
| ResolveClash | ungated (oracle build) | — | — | 1,400,000 | 1,400,000 | — | — | — | — |
| SkipQuiet | 90,000 + 30,000/bell | 206,048 (24 bells, churn) · 111,436 (kernel-quiet n = 24) | — | 810,000 (unchanged) | 810,000 | 12,312 | 1,148 | 31 | 33 |
| CloseClashInputs | 8,000 | 7,064 | 11.7 % | 7,500 | 8,000 | 904 | 327 | 6 | 6 |
| CloseArrivalDay | 8,000 | 5,622 | 29.7 % | 6,000 | 8,000 | 904 | 327 | 6 | 4 |
| CloseArrivalSlot | 8,000 | 6,120 | 23.5 % | 6,500 | 8,000 | 904 | 329 | 6 | 6 |
| ClaimDefence | 25,500 | 24,111 | 5.4 % | 25,500 | 25,500 | 3,280 | 752 | 19 | 13 |

The gates (`cu_budget`) are unchanged. **Tightest margins** (G1 maximum vs gate): PostAnchor and PostSeed 1.7 %, PostBeacon 2.2 %, ResolveFromInputs 3.0 % (with Phase B: 19.4 % against 340k, 5.5 % against 290k), Reveal 3.2 %, ConsumeRingSeed 3.6 %, FoldOccupancy 4.2 %, ConsumeGenesisSeed 4.5 %, Train 4.9 %. ClaimDefence (W4-B F6) 5.4 % against v1.7's 25,500.

### 2.3 `L(kind)` and the presets (I-45)

- `budgets::PLACEHOLDER_SO_LEN` 1,048,576 → **884,736** (the release `.so`, 869,536 B, rounded up to 16 KiB: ≈ 15 KB of growth headroom); `PLACEHOLDER_PROGRAMDATA_LEN` 1,310,720 → **1,105,920**. `L(kind)` drops by 6–7 pages (196,608–229,376 B) for every kind: e.g. Reveal 1,343,488 → **1,146,880**, GatherClash 1,376,256 → 1,146,880, SkipQuiet 1,474,560 → 1,277,952 (`vectors/budgets.json`). The svm guard `g01_loaded_limit_table_covers_the_release_so` asserts the deployed release `.so`'s programdata ≤ the table's and `loaded_limit(kind) ≥ L(kind)` at the deployed length for every kind (it fails when a later wave grows the `.so` past the table: regenerate then).
- Every kind has a `g01_loaded_limit` check against the release `.so` (lands at `L(kind)`, fails charged one page below the tight limit, `L(kind)` within one page of it) — 25 kinds added here (`common::loaded_check`).
- Presets (`M1_LOCAL_7D`): **`reveal_cu_limit` 26,000 → 26,500** (worst Reveal 25,155 CU, FirstOfBell, + 5 %, pinned to `budgets::budget(Reveal).cu_limit` by a new test) and **`reveal_loaded_limit` 1,343,488 → 1,146,880** (`L(Reveal)`). `tip_min` follows from them (the relay/web pins move: §6 R2).

## 3. What I ran

| Command | Result |
|---|---|
| `(cd permutation-frontier/svm-tests && RELEASE_CHECK=1 PSF_CU_LOG=… ./run.sh --release --no-fail-fast -- --nocapture)` (Gate W5 line 1; builds release, test-beacon, trace, oracle, probe) | **exit 0: 243 passed, 0 failed, 4 ignored** (`final.log`) |
| trace sweep: the same suite with `PSF_SO` = trace build, `PSF_SO_TEST_BEACON` = `test-beacon trace` build (scratch), `PSF_CU_LOG` | heap per kind (§2.2); 234 passed, 8 failed as expected on trace builds (7 CU gates exceeded by the trace markers, 1 build-marker check) and one stale oracle build (rebuilt for the final run) |
| `cargo fmt --all -- --check`; clippy `-p permutation-rules -p frontier-abi` and `-p permutation-frontier` `--all-targets -D warnings` | exit 0 |
| `cargo test --locked --release -p permutation-rules` | exit 0 (436 passed; `sort_keys_equal_the_core_stable_sort`, `frontier_clash_equiv` included) |
| `cargo test --locked -p frontier-abi`; `abi-vectors -- --check`; `cargo test --locked -p permutation-chain` | exit 0 |
| `cargo test --locked -p permutation-frontier` (and `--no-default-features`) | exit 0 |
| `scripts/build-frontier.sh --twice` | exit 0: both builds `file_sha256 a5b77562…d533`, `program_hash 86ceeb2e…94e8`, e_flags 2, `.so` 869,536 B, `max_len` 1,089,536, deployable |
| `scripts/build-wasm.sh --check` | fresh after `e32e837` (190,979 B raw, 69,192 B gzip) |
| svm-tests `cargo fmt -- --check` and `cargo clippy --locked --release --all-targets -- -D warnings` | exit 0 |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| `(cd frontier-node && cargo test --locked --workspace --no-fail-fast)` | **256 passed, 3 failed** — all three from this unit's regenerated budgets, in other units' files (§6 R2, R4): `fclient::budgets::loads_frontier_abis_budgets_json` pins Reveal's `cu_limit` 26,000; `keeper play::duplicate_keepers_race` on the **native model** (its fixed CU charges exceed eight new limits, §6 R4); `localnet server::ports_and_base58` failed once because port 41010 was busy at that moment (re-run: pass) |
| `PSF_FRONTIER_SO=<test-beacon .so> cargo test --locked --release -p keeper --test play -- --include-ignored` (Gate W4 line, keeper on the program) | **7/7 pass** on this branch's program and budgets (incl. `duplicate_keepers_race`, `lag_gate_in_process`, `crash_injection_every_journal_point`) |
| `(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)` (Gate W4 line) | exit 0: `inproc_day` ok (456 s), inproc smoke, lag gate, crash injection |
| `(cd permutation-gateway && npm ci --ignore-scripts && npm test)`; `node scripts/sync-web-sdk.mjs --check` | npm ci exit 0; **501/504**: the 3 failures are the regenerated ABI vectors not yet propagated (frontier-vectors budgets prefix, fixtures, generated client ABI modules); sync `--check` reports the four generated SDK/client ABI files out of date (§6 R2) |
| Phase B lab (`phaseB-gate/run.sh`, §4) and the Phase B program build (clash + g07 tests, then reverted) | §4 |
| `(cd frontier-sim && cargo fmt -- --check && cargo clippy --locked --release --all-targets -- -D warnings && cargo test --locked --release)` (Gate W1 line, this branch's kernel) | exit 0 |
| `(cd frontier-sim && cargo run --release -- doctrine-gate --controls)` (Gate W1 line) | exit 0: kernel table 6/6, largest \|Δ index\| 0.063 %, 360/360 conserve; draft (−1.291 %), Knight (−0.294 %) and A-boost (+0.338 %) controls rejected |
| `(cd frontier-node && cargo test --locked --release -p verify -- --include-ignored tamper_)` (Gate W4 line) | exit 0 (34 tests) |

Not run: Gate W5's `frontier-stack`, `mutate.sh`, herald load, G14, screenshots lines (W5-B/C/D/E's), Gate W1's `criterion --best-response` and the overnight band on this branch's kernel (the kernel change is digest-identical: the equivalence test and every G8 digest equality hold; the band was run for Phase B, §4).

## 4. Phase B evaluation (I-14, O-M1-04) → **adopt, committed by W6-B**

Lab `(session scratch)/scratchpad/frontier/m1/lab/phaseB-gate/` (`RESULTS.md`). Phase B = one `sha256(cs ‖ 3 ‖ "eng" ‖ id)` per engagement for both variances (SP-V2's `phaseB.patch`, applied to `resolve_clash` and the reference `resolve_clash_ref`).

| Gate (I-14) | Phase B | Phase A (paired control) |
|---|---|---|
| Doctrine proxy gate `doctrine-gate --controls` | **exit 0**: kernel table largest \|Δ index\| 0.060 % (±0.2 %), 360/360 conserve; draft, Knight and A-boost controls all rejected | (integ runs: pass) |
| O5 band, 1,500 paired seasons (`--seeds 250 --first-seed 10000`) | **6/6**, largest gap 1.1 points, \|Δ\| 0.050 %, 1,500/1,500 conserve | 6/6, gap 1.1, \|Δ\| 0.069 % |
| Golden digests | 3 pinned-digest tests move (by construction); the equivalence test passes with Phase B in both bodies | — |
| Native = on chain | all 1,244 RFI fills' on-chain digest = native Phase B kernel; G7 identical | — |
| RFI CU (release) | **max 274,007**, mean 228,504 | max 329,863, mean 260,636 |

Every gate re-passes; the O-M1-04 working default ("Phase B before the exit if the gates re-pass") therefore **adopts** it. It is **not committed in W5**: a kernel outcome change mid-wave would invalidate the fixtures and recordings W5-B/C/D/E are producing now (verifier fixtures, herald recordings, `clash_model` cross-vectors, the WASM). §11 already gives the commit to W6-B ("Phase B commit if adopted"): `CLASH_VERSION` 2 → 3, the three golden tests regenerated, the RFI gate 340k → 290k, every outcome vector regenerated by its owner, then the doctrine gate and criterion re-run (DECISIONS O5 rule). Amendment row in §7.

## 5. Status of G1–G14 and open items

| Gate | Status on this branch |
|---|---|
| G1 | every §13.1 row has a test asserting CU, tx bytes, locks and loaded data (release `.so`; heap asserted where a test runs the trace build); the heap of every kind measured by the trace sweep, all ≤ 15,320 B; the budgets table regenerated from the suite (§2.2) |
| G2, G3 | existing tests pass; CloseSeason parts add G3 rows (BadAddress, BadAccount) |
| G4–G6, G8–G12 | existing tests pass (G8 on this branch's kernel: every on-chain digest = native) |
| G7 | **new**: `g07_lag_gate_in_litesvm` (§1.3) |
| G13 | **complete**: `RELEASE_CHECK=1` green (§1.2) |
| G14 | W5-C's |

Open (not W5-A's to close, or deferred with a reason):
- **O1 — W4-A D8 (the clash model into `frontier-abi`): not done by W5-A.** `proc::clash::model` is 1,113 lines tied to the program's error type, layout accessors and trace hooks; moving it means a new `frontier-abi` module (outside W5-A's paths: only `budgets.rs` is W5-A's in `frontier-abi/src`) and switching `fclient::clash_model`'s consumers (W5-C's crates this wave). v1.7 already removed the "three copies, two wrong" problem (one off-chain builder, pinned to the program by the `clash_model` cross-vectors). Proposal: W6-B (owns `frontier-abi/**` and `permutation-frontier/**`) moves it together with the Phase B commit (both touch the same code and vectors), the integrator switching fclient in that window. The wave note lists D8 as a must for wave 5; the integrator decides (§6 R6).
- **O2** — the 1-MiB loaded-data working default is met by no kind (`L(kind)` ≥ 1,114,112 B at the release `.so`): 30 KB more `.so` reduction would bring the smallest kinds under it; not needed for correctness.
- **O3** — SettleTicket's archive-entry seed path is exercised with a **crafted** archive (identical to ArchiveAnchors' output format): a real ArchiveAnchors cannot reach an open ticket (`archive_after` ≥ 48 h, tickets expire after 24 bells and an expired ticket settles without a seed). Consequence for the contract: the path is dead for SettleTicket in M1 unless `archive_after` < 4 h; kept for safety.
- **O4** — CloseSeedCache and CloseArrivalSlot/Day need Running/Ended (CLOSE_STATUS), so caches left after the final CloseSeason part cannot close (a keeper float, not a player's). Not in the wave note; for the architect with F3's successor.
- **O5** — verifier items (SKIP quiet at every bell, V5 MissingData, HoldingReplayMismatch) are W5-D's; the web camp-citizen hand-over is W5-E's.

## 6. Deviations and dependency requests

| # | What | Who |
|---|---|---|
| R1 | **`permutation-rules/src/frontier/clash.rs` edited outside "Phase B only"** (wave note 3 asked for it, coordinating with the integrator): the kernel's sorts share one body. Digest-identical (the host reference keeps the core sorts; the 4,320-input equivalence test and every on-chain = native check pass), `CLASH_VERSION` unchanged, no outcome moves. The file is W1-A's | integrator: accept `3767652` |
| R2 | **`frontier-abi/src/presets.rs`, `vectors/presets.json`, `vectors/ix.json`** changed (the presets are W5-A's task, the file is the integrator's), and the consumers of the regenerated tables must be regenerated in the integration window, as integ-W4 did in `f7eec54`/`e306304`: `(cd frontier-node && cargo test -p fclient writes_frontier_vectors)` (→ `permutation-gateway/test/frontier-vectors.json`), `node permutation-gateway/scripts/sync-web-sdk.mjs` (→ `permutation-server/web/sdk/frontier/abi-{budgets,presets}.mjs`, `permutation-gateway/client/src/frontier/abi-{budgets,presets}.mjs`), `node permutation-gateway/test/fixtures/frontier/make-fixtures.mjs`, and the pins: `fclient::budgets::loads_frontier_abis_budgets_json` (Reveal `cu_limit` 26,000 → 26,500), the web tests' pinned `tip_min`/presets tips (v1.6 `e306304`: 14,472 / 21,708 / 28,944 — they move with `reveal_cu_limit` and `L(Reveal)`), `agents/fixtures/herald-recorded/h/season.json` if it pins the presets. The abi-vectors tag stays `v1.7` (bump to v1.8 with the amendment) | integrator (generated files), W5-C (fclient pin) |
| R3 | `permutation-server/web/frontier/wasm/frontier.wasm{,.sha256}` rebuilt (`e32e837`) so `build-wasm --check` and the gateway's wasm hash test hold on this branch | integrator: keep or re-run `scripts/build-wasm.sh` |
| R4 | **The keeper tests' native model charges fixed CUs above nine of the new limits** (`crates/keeper/tests/model`; a transaction also spends 450 CU on its three ComputeBudget instructions): SettleDeparture 12,000 (limit 11,500), Dissolve 20,000 (20,000), CloseArrivalDay 6,000 (6,000), OpenRing 25,000 (16,000), Join 20,000 (13,500), SettleTicket 30,000 (24,000), ReleaseDormant 20,000 (12,500), SettleExplore 12,000 (9,000), DisbandStranded 8,000 (6,000). On the model the keeper then retries up the CU ladder, and `duplicate_keepers_race`'s waste bound fails (SettleDeparture: 37 duplicates over 11 objects; bisected to that JSON field). On the real program the same test passes. The model should charge at most `limit − 450` (e.g. `budgets::MEASURED`) | W5-C (keeper tests) |
| R5 | CloseSeason parts 8–10: a builder `fclient::ix::close_season_float(a, authority, part, pairs)` (the svm builder `ix::season::close_float` is the reference) and the keeper/stack's season-end close (W5-B's `frontier-stack down`/report may want it); optionally the prologue table's repeat group as `Either(JoinShard, AnchorArchive)` so `L(CloseSeason)` covers archive pairs | W5-C (fclient), W5-B, integrator (prologue table) |
| R6 | W4-A D8 not done (§5 O1) | integrator |
| R7 | No manifest, lockfile, toolchain or `.gitignore` change | — |

## 7. Contract amendment requests (v1.8, for the integrator)

| Section | Change | Evidence |
|---|---|---|
| §5.2, §5.7 CloseSeason | Parts **8** (RingSeeds → payer), **9** (AnchorArchives → `rent_to`), **10** (DefenceClaims → beneficiary): 1–24 `[target w] [recipient w]` pairs in the repeat group; targets by their own key fields at the canonical address (`BadAddress`), recipient the stored one (`BadAccount`), absent targets skipped; parts 9–10 only on the tombstone or Aborted (`TooEarly`); part 8 as parts 0–6. Closes W4-B F3 | `g13_close_season_float_parts`, `…_on_an_aborted_season`, `g01_budget_close_season_float_parts` |
| §5.4 | 11 `SiteTaken` and 50 `Aborted` are reserved in M1 (no path emits them; a taken site is the SETTLE outcome, an Aborted season is `WrongStatus`) | `cover::EXEMPT`, `g13_coverage_exempt_codes_are_reserved_ones` |
| §5.5, §10.2 | CU limits = G1 maxima + 5 % (rounded to 500) per kind, `budgets::MEASURED`; `L(kind)` at the release `.so` (`PLACEHOLDER_SO_LEN` 884,736, programdata 1,105,920); presets `reveal_cu_limit` 26,500, `reveal_loaded_limit` 1,146,880 | §2.2, §2.3 |
| I-14, O-M1-04, §5.5 | **Phase B adopted** (every gate re-passes, §4); committed by W6-B with `CLASH_VERSION` 3, RFI gate 290,000 | `phaseB-gate/RESULTS.md` |
| §13.3 G7 | The program-level lag gate runs in LiteSVM (`g07_lag_gate_in_litesvm`) besides the keeper's in-process run | §1.3 |
| §13.1 | The G1 table is regenerated from a full-suite CU log (`PSF_CU_LOG`), not per-test prints | §2.2 |

## Links

- Contract `docs/frontier/m1/M1-CONTRACT.md` (v1.7); decisions `docs/frontier/DECISIONS.md` (M16: the bounced-resident test; O-M1-04).
- Code: `permutation-rules/src/frontier/clash.rs` (`sort_keys`, `sort_by_key3`), `permutation-frontier/src/proc/season.rs` (`close_float`, `close_counts`), `permutation-frontier/src/heap.rs` (`scoped`), `frontier-abi/src/budgets.rs` (`MEASURED`, `limit_of`), `frontier-abi/src/presets.rs`.
- Tests: `permutation-frontier/svm-tests/tests/{g13_complete.rs, g07_lag.rs, lifecycle.rs, citizen.rs, clash.rs, g01_loaded_limit.rs}`, `svm-tests/src/cover/*.rs`, `svm-tests/tests/common/mod.rs` (`loaded_check`).
- Lab: `(session scratch)/scratchpad/frontier/m1/lab/phaseB-gate/RESULTS.md`.
