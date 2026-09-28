# integ-W4: wave-4 merge, integration window and Gate W4

- **Role:** M1 integrator, wave 4. **Branch:** `frontier/m1-integ` (worktree `.claude/worktrees/m1-integ`), wave base `7289822`. **Contract:** M1-CONTRACT v1.5 → **v1.6** (§22, this window) §3.4, §11 (wave 4), §12 (Gate W4), I-45, I-47, I-55. **Date:** 2026-09-28.
- **Owner decisions in force:** O-M1-01…24 working defaults accepted; **O-M1-12 item 1 (wasm32-unknown-unknown for 1.95.0) approved and installed by the main session on 2026-09-28** (recorded in `DECISIONS.md` part A); items 2–4 (Playwright Chromium, the drand quicknet round archive, Agave ≥ 4.0) **not approved**: no `playwright install`, no drand fetch, no Agave or `rustup` install was run. O-M1-17 fix locally, no push; O-M1-18 not approved. The 1.95.0 `rustfmt`/`clippy` install is already recorded in part A (W2-A's row) and H3; nothing added. Main-session wave note: J2 decided (flate2 applied here), K9 decided (implemented here), the §21 troops return landed with W4-A.
- **Not done, by rule:** no push, no devnet or mainnet transaction, no service on a fixed port (every test binds `127.0.0.1:0`; nothing in 41000–41999 was used), no download, no file of the main tree, `codex/magicblock-playable` or `codex/v9-security` touched, `permutation-server/web/session.mjs` untouched (gate item 13).
- **Logs:** `(session scratch)/scratchpad/integ-w4/` — `gate/gate-w4.sh`, `gate/gate-run1.txt` + `gate/logs-run1/`, `gate/gate-run2.txt` + `gate/logs/item1..26.log`; pre-runs in `pre/`; the pass-condition runs with output in `extra/` (`inproc-final.{log,json}`, `keeper-lag-crash.log`, `rfi.log`); the wasm checkout experiment in `wasmexp/`.

## 1. Merges (contract order W4-A, W4-B, W4-C, W4-D, W4-E, W4-F; all `--no-ff`)

| Step | Unit (head) | Merge commit | Integrator-owned files left out and re-applied |
|---|---|---|---|
| 1 | W4-A program-clash (`86bff0b`) | `ec54fe6` | none changed (5-line hook in W3-B's `proc/host.rs`: accepted, DECISIONS L1) |
| 2 | W4-B program-transit-lifecycle (`7d98f1d`) | `853c292` | none changed |
| 3 | W4-C keeper-play (`ae3bf26`) | `1c48afb` | none changed |
| 4 | W4-D verifier (`65fac0b`) | `b0ee509` | `frontier-node/Cargo.lock`, `crates/verify/Cargo.toml` → `aa85312` |
| 5 | W4-E web-report-practice (`cbf1489`) | `2e1259b` | none changed (R1, the wasm artefact: `eab82a4`) |
| 6 | W4-F integration (`b2dcb2b`) | `9a12ded` | `frontier-node/Cargo.toml`, `Cargo.lock`, `crates/{herald,itest}/Cargo.toml` → `86cc81b` |

No textual conflict. Every unit branch was cut from `7289822`.

## 2. Dependency requests (§3.4, I-55)

| Request | Applied | Check |
|---|---|---|
| W4-D R1: verify `findex`, `frontier-abi`, `permutation-rules`, `solana-address`, `hex`, `tokio`; dev `keeper` (path), `localnet`; features `mutate-v1..v13` (no v10) | yes (`aa85312`), byte-identical to the unit's; no new crate | `cargo check --locked --offline --workspace --all-targets` |
| W4-F R1: itest dependencies (workspace and path crates, no new crate) | yes (`86cc81b`) | as above |
| W4-F R2 = DECISIONS J2 (decided by the main session): workspace `flate2 =1.1.10` (default features off, `rust_backend`), herald `flate2.workspace = true`; lock gains `flate2 1.1.10`, `miniz_oxide 0.9.1`, `crc32fast 1.5.2`, `adler2 2.0.1`, `simd-adler32 0.3.10` | yes (`86cc81b`; W4-F's lock diff applied onto W4-D's lock, both unit edits kept); brotli not added; the stale "waits for the owner" comment rewritten | as above; herald tests |
| W4-E R1: commit `frontier.wasm` + `.sha256` | yes (`eab82a4`, after `dfa5408`) | §3 item 1 |
| W4-A, W4-B, W4-C | nothing requested | — |

## 3. Integration window (one commit per gate item or wave-note item)

1. **`dfa5408` — `build-wasm --check`: checkout-independent bytes.** The `m1-integ` and `m1-W4-E` checkouts built different `frontier.wasm` bytes from the same source (238,985 B both; sha256 `5f15df6f…` vs `16e4eb51…`; 72,359 bytes differ). The RUSTFLAGS text is not the cause (measured: an extra no-op remap gives identical bytes); the absolute path of `permutation-rules`, a path dependency outside frontier-wasm's workspace, enters the crate metadata. The build now runs through one fixed symlink (`/tmp/psf-frontier-wasm-root`, taken under an `mkdir` lock, removed on exit) remapped to `/psf`. After: `--check` fresh in `m1-integ` and in a copy of the tree at another path. Without it, `--check` would pass only in the checkout that wrote the artefact (it would fail in `frontier-integ` and every wave-5 unit worktree).
2. **`eab82a4` — `frontier.wasm` and its `.sha256`** (wave note 1, W4-E R1): **238,985 B raw** (400 KB = 409,600 B gate: pass), 78,358 B gzip (150 KB budget: pass), 28 exports + `alloc`, `free`, `memory`, sha256 `782c9f30…1113`. The gateway's real-module wasm tests now run (503 pass, **0 skipped**; wave 3 had 1 PENDING-OWNER skip).
3. **`ebbfe7b` — DECISIONS K9 in SettleTicket** (wave note 3; `proc/citizen.rs` has no wave-4 owner). **Failing first:** svm `citizen_cohort_fresh_waits_for_an_earlier_open_cohort` failed on the merged program (the later cohort's fresh settle landed, 17,841 CU) [measured], then passes with the rule: a fresh settlement is `TicketState` while an earlier cohort of the Province is open (`earlier_cohort_open`, unit test `earlier_open_cohort_blocks_only_later_bells`). Displacement and `taken` unchanged; the keeper and W4-C's model already followed it.
4. **`ec616d6` — svm SettleTicket worst fill under K9.** W3-A's `displacement_setup` settled fresh while seven filler cohorts were open; it now settles at bell + 23 (fillers expired, records still in the tables). `g01_settle_ticket_displacement_three_provinces`: 22,043 CU (W3: 21,954), 649 B, 16 locks. Program unchanged.
5. **`f7eec54` — `L(kind)` and `reveal_loaded_limit` from the merged `.so` (I-45).** First `inproc_day` run: **every** transaction after InitShards refused `MaxLoadedAccountsDataSizeExceeded` (ConsumeGenesisSeed × 20, JOIN 0/100). The merged release `.so` is 1,021,160 B → programdata 1,277,952 B, above the 1-MiB working default that the budgets table (SP-V2's 540,608-B placeholder) and `M1_LOCAL_7D.reveal_loaded_limit` still asked for. `PLACEHOLDER_SO_LEN` = 1 MiB (≈ 27 KB headroom), `reveal_loaded_limit` = `L(Reveal)` = 1,343,488 (pinned by a test); vectors (tag v1.6) and the JS SDK regenerated. W5-A still regenerates both from the final `.so`.
6. **`0179c1b` — keeper GatherClash as the program reads it.** `inproc_day` then showed GatherClash `TooManyAccounts` × 17 and `BadData` × 13 (province −3,0 stuck at bell 88): W4-C's `gather_parts` set `holdings_bitmap` bit j relative to `start` (the program and §5.11: absolute position k), and the no-arrival fast path sent `n = 0` (the program checks the range first). Fixed in fclient, the keeper and its native model; `gather_parts_respect_the_packet` now fails on the old rule [checked].
7. **`5f43419` — keeper play test tip from the season's presets** (it hard-coded `L` = 1 MiB; `TipTooLow` on the real `.so` after item 5).
8. **`3acc6b1` — herald clash check with the program's input rules.** 56 of 121 CLASH records were herald `MISMATCH` alarms: W3-D's Provisional builder passed the camp in whole troops, fought it with 12 garrisons, skipped the day's camp check, passed a stored `dealt_bps` 0 as 0 and did not cap garrisons (W4-A D8). After: 0 mismatches over 121.
9. **`f316c88` — itest `inproc_day`: a 26-bell drain, JSON clash files only.** After an 8-bell drain 21 idle provinces sat at bells 128–141: the keeper skips idle provinces in 24-bell batches (`SKIP_MAX`), so they were batched, not stuck; 26 bells cover one batch and its close (all 37 then pass bell 144). The herald scan also counted every `.gz` sibling as a `null` mismatch.
10. **`da6034e` — permutation-frontier clippy** (Gate run 1 item 14, exit 101): the `stubs!` macro became unused once W4-A and W4-B had both replaced their stubs; removed (no code generated, program unchanged).
11. **`e306304` — gateway `npm test` with the v1.6 `L(kind)`** (Gate run 1 items 11 and 19: 6 failures). fclient's shape vectors used `Budgets::placeholder` (1 MiB everywhere) while the keeper, bots and JS SDK read the generated table: `Budgets::canonical()` now; `frontier-vectors.json` and the web fixtures regenerated; web tests' pinned `L` and tips from the table/season (tip_min 14,472; presets 14,472 / 21,708 / 28,944).

Commits (first parent, oldest first): `ec54fe6`, `853c292`, `1c48afb`, `b0ee509`, `aa85312`, `2e1259b`, `9a12ded`, `86cc81b` (merges and dependencies); `dfa5408`, `eab82a4`, `ebbfe7b`, `ec616d6`, `f7eec54`, `0179c1b`, `5f43419`, `3acc6b1`, `f316c88`, `da6034e`, `e306304` (window); then the docs commit with these notes, contract v1.6 and DECISIONS parts A/C/J/K/L.

## 4. Gate W4 — exactly as §12 (v1.6) writes it

`M1_PORTS` empty (the preamble checks nothing; every test binds `127.0.0.1:0`). Script `gate/gate-w4.sh`: the preamble, every Gate W1, W2, W3 item, then the three Gate W4 items, each logged separately.

**Run 1** on `f316c88` (08:02–08:31): 23 of 26 exit 0; **item 11 and item 19 exit 1** (gateway, §3 item 11), **item 14 exit 101** (clippy, §3 item 10). Fixed in the window; **run 2** on `e306304` (08:35–09:04):

| # | Item | Result (run 2) [measured] |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | exit 0 |
| 2 | clippy `permutation-rules`, `frontier-abi` | exit 0 |
| 3 | `cargo test --locked --release -p permutation-rules` | exit 0 |
| 4 | `cargo test --locked -p frontier-abi` | exit 0 |
| 5 | `abi-vectors -- --check` | exit 0 |
| 6 | `cargo test --locked -p permutation-chain` | exit 0 |
| 7 | frontier-sim fmt, clippy, `cargo test --release` | exit 0 (420 s) |
| 8 | `criterion --best-response … --gate` | exit 0 |
| 9 | `doctrine-gate --controls` | exit 0 (409 s) |
| 10 | frontier-node fmt + clippy `-D warnings` + `cargo test --locked --workspace` | exit 0 (242 passed, 7 ignored; `web3.js conformance NOT RUN` 0 times) |
| 11 | gateway `npm ci --ignore-scripts && npm test` | exit 0 (503 pass, 0 fail, **0 skipped**) |
| 12 | civilization tests | exit 0 |
| 13 | `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| 14 | `cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings` | exit 0 |
| 15 | `cargo test --locked -p permutation-frontier --no-default-features` | exit 0 |
| 16 | `scripts/build-frontier.sh --twice` | exit 0: both builds `file_sha256 b72024c3…8268`, `program_hash e3b05c07…3ee4`, e_flags 2, `.so` 1,021,160 B, `max_len` 1,277,952, deployable yes |
| 17 | `svm-tests/run.sh --release -- g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_` | exit 0 (96) |
| 18 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (242 passed, 7 ignored) |
| 19 | gateway `npm test && sync-web-sdk.mjs --check` | exit 0 (503/503; web/sdk up to date) |
| 20 | `scripts/build-wasm.sh --check` (the target is installed, so the `pending` branch is not taken) | exit 0: 238,985 B raw, 78,358 B gzip, fresh |
| 21 | `svm-tests/run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_` | exit 0 (89) |
| 22 | `PSF_TRACE=1 svm-tests/run.sh --release -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture` | exit 0 |
| 23 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0 (242 passed, 7 ignored) |
| 24 | `(cd permutation-frontier/svm-tests && ./run.sh --release)` | exit 0 (200 passed, 3 ignored: W4-A's `zz_profile_*`) |
| 25 | `(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)` | exit 0 (inproc_day 186 s, inproc smoke, lag gate, crash injection) |
| 26 | `(cd frontier-node && cargo test --locked --release -p verify -- --include-ignored tamper_)` | exit 0 (25 tamper tests) |

**PENDING-OWNER in this gate: none** (`PENDING_OWNER:` empty). `build-wasm --check` ran (O-M1-12 item 1 installed). Playwright, the round archive and Agave are Gate W5/W6/W7 items and were not needed here.

**Pass conditions beyond exit codes** (the Gate W4 list, then the earlier gates' conditions that still apply)

| Condition | Status [measured, run 2 tree `e306304`; numbers from the `--nocapture` runs in `extra/`] |
|---|---|
| `inproc_day` (100 bots, one game day, in process, test key) ends with **zero stuck province-bells** | **met**: 37 provinces resolved through bell 144 (CLASH 121, SKIP 466); test-beacon `.so` `47bc9138…` built by the test from the merged tree; 12,806 slots, 180 s wall |
| every transit settled or routed by rule | **met**: 46 departs, every due one settled (0 unsettled, 0 stuck in state 1–3) |
| every bad seal destroyed at settlement with the stock `tlock` opener's code | **met**: 8 bad seals marched and due, 8 settled `BAD_SEAL`, 0 seal-code disagreements, 0 survived |
| the other `inproc_day` conditions | all PASS: no stubs; activity (JOIN 100/100, SETTLE 102, DEPART 46, REVEAL 41, TRANSIT_SETTLED 46); min-tip marches revealed by keepers (3/3); no persona violated; keeper writes: no `dead`/`failed` alert (only `reveal-pool-low` × 4 at bells 0–1); herald 10,668 events, no alarm, every clash report matches; relay drain guard never tripped. Failed txs on chain are the personas' expected ones only (Reveal `BadAddress` × 5 forger, `WindowClosed` × 4 late revealer, Depart `TipTooLow` × 1 zero-tip) |
| the in-process lag gate byte-identical | **met** (native model, as the gate line runs it): outcome digests equal (`62f1eb81…c64c`), destination CLASH `d13de72c…826f` equal; close → first resolve 9 slots unheld, 189 held |
| crash injection: outcome digests identical, ≤ 1 duplicate version per in-flight write | **met** (native model): 62 crash points reached (reveal 22, clash 20, settle 20), 62 digests identical; extra versions 0 at 47 points, 1 at 15, never above 1 |
| RFI ≤ 340k CU and heap ≤ 28 KiB over all 1,240 fills with the full write-back | **met** on the merged release `.so` (1,021,160 B): **1,244 fills** (SP-V2's 40 + the 1,200 screen + 4 storage fills), CU min 153,623, mean 262,200, **max 331,870** (gen-1200 wide#235), heap **max 15,080 B** (trace build), tx 453 B; gathers max 32,378 CU |
| verifier V1–V13 PASS on the recorded fixtures and T1–T22 FAIL on them | **met on the committed fixtures** (item 18/23 `fixtures`/`unit` tests, item 26: 25 tamper tests FAIL with their codes). **Caveat:** `land-program.json` is a real recording; `march-synth.json` is W4-D's synthetic season (the wave-3 program had no clash/transit instructions); a march recording from the merged program (W4-D R5) was not made in this window — open, §5 |
| Reveal worst ≤ 26,000 CU, tx ≤ 1,100 B (Gate W3) | met: FirstOfBell **25,155** CU, Named 24,300, Displace 20,350; 916 B, 20 locks; `L(reveal)` at this `.so` 1,310,720 B (formula; measured need 1,299,845 B). Trace heap 2,930 B |
| cohort tests, herald fold determinism, keeper land tests (Gate W3) | met (items 21, 23: the four cohort tests plus the new K9 test; `fold_determinism_*`; `land_season`, `reveal_accept_over_localnet`, `one_day_beacons`) |
| `frontier.wasm` ≤ 400 KB raw (Gate W2, now unblocked) | met: 238,985 B |
| other G1 figures (Gate W3 lines) | OpenProvince worst 148,423 CU; Join worst 12,066 CU; FileTicket 16,060 CU; SettleTicket displace 22,043 CU |

**Extra run, not a gate line (W4-C's request):** the keeper play tests with `PSF_FRONTIER_SO=<test-beacon .so>` against the real program. `play_bell_pipeline` **fails** there: the scenario's marches use a fixed arrival bell (`b0 + 2`) and straight-line paths, which the program's Reveal refuses (`ArrivalBell` 31 for the 900- and 800-troop hosts, `Path` 32 for host 1), and its expected outcomes are the native model's clash. That is W4-C's scenario, not a keeper or program fault (`inproc_day`, where bots plan marches with the kernel, reveals and settles on the program); recorded as an open item (§5). The lag gate and crash injection therefore stand on the native model only, as the gate line runs them.

**Verdict:** Gate W4 green on `e306304`: 26/26 items exit 0 (run 2), every pass condition met as written (the verifier condition on the committed fixtures, with the caveat above), no PENDING-OWNER item. `codex/frontier` fast-forwarded to this branch's head after this notes commit.

## 5. Open items (not gate items; for the wave-4 review, later waves and the architect)

- **CloseClashInputs vs SettleTransit settled bits (W4-A/W4-B, architect)** [code reading, not a failing test]. W4-A's CloseClashInputs requires a `settled_mask` bit for **every** record with a host id; W4-B's SettleTransit sets a bit only for a **present** record (`position` requires `present`). A record written for a slot whose Holding is absent, released or re-founded keeps its host id with `present = 0` and can never be settled (no Holding), and a destroyed-at-origin record is `present = 0` too, so those ClashInputs can never close (their rent stays locked). Not exercised by `inproc_day` (the close grace is 1,008 bells). Needs one rule in v1.6's successor.
- **W4-B F1** (a bad seal's destination is the settler's word; proposed GatherClash stamp in the transit record) and **F3** (CloseSeason cannot close RingSeeds, AnchorArchives, DefenceClaims: ≈ 8.2 SOL archive float per 7-day season) — architect. **F6** ClaimDefence 3.7% CU margin — W5-A.
- **W4-C's play scenarios on the program** (above): make the scenario plan arrivals with the kernel's travel time and path, and derive its expectations from the program's clash, so `PSF_FRONTIER_SO` runs of `play_bell_pipeline`, `lag_gate_in_process` and `crash_injection_*` are meaningful — W4-C/W5-C.
- **A march recording from the program for the verifier** (W4-D R5; W4-F did not do it): `inproc_day` can now produce one; V1–V13 over it will test W4-D's pinned encodings (§3 of its notes) against W4-A/W4-B's real records — W5 (verifier owner with itest).
- **W4-A D8/D9/D10:** move `proc::clash::model` into `frontier-abi` (one builder for the program, herald, WASM `resolve_from_inputs` and verifier; the herald's transcription in `3acc6b1` is a stop-gap), `heap::scoped`, the G13 row of the return settle in `cover/host.rs` — W5-A. **W4-E R2** (`province_before_b64` in `/h/clash`) — herald owner, W5.
- **Program size:** 1,021,160 B release; `L(kind)` now needs ≈ 1.28–1.47 MB. The placeholder has ≈ 27 KB of headroom; a wave-5 change that grows the `.so` past 1 MiB must re-run `abi-vectors` with a new placeholder (or W5-A regenerates from the final `.so`, as planned). W4-A's suggestion (one sort helper in the kernel instead of ≈ 185 KB of monomorphs) is W5-A's.
- **Pre-existing, not gate lines:** svm-tests clippy errors in W3-B's `tests/host.rs:574`, `tests/holding.rs:466` (W4-A notes).
- **Contract text, not amended here:** W4-F's note that §5.10 lists Depart with Dissolve/Explore for `HostInTransit` although Depart checks `NotResident` and `HostBusy` first.

## Links

- Contract: `docs/frontier/m1/M1-CONTRACT.md` (v1.6, §22). Decisions: `docs/frontier/DECISIONS.md` (parts A, C, J, K updated; part L new).
- Unit notes: `docs/frontier/m1/W4-A-NOTES.md` … `W4-F-NOTES.md`.
- Artefact: `permutation-server/web/frontier/wasm/frontier.wasm` (+ `.sha256`); build script `scripts/build-wasm.sh`.
