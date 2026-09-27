# W2-B svm-harness — notes

- **Unit:** W2-B (wave 2), branch `frontier/m1-W2-B` cut from `frontier/m1-integ` at `ed31438`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.2: §3.5, §4, §5.5–§5.8, §8.7, §10.1, §11 (W2-B brief), §12 Gate W2, §13.1–§13.3.
- **Owned paths touched:** `permutation-frontier/svm-tests/**`, this file. **One file outside them:** a single row in `docs/frontier/DECISIONS.md` part A (the rustfmt/clippy install), because the wave-2 task asked every unit to record it if missing; the integrator may keep or move it.
- **Integrator-owned files created on this branch to build** (dependency requests, §6): `permutation-frontier/svm-tests/{Cargo.toml (dependency section), Cargo.lock, rust-toolchain.toml}`, `permutation-frontier/svm-tests/probe/{Cargo.toml (dependency section), Cargo.lock}`.
- **Tags:** [measured] = run on this machine on 2026-09-27; [design] = the contract's rule as implemented.
- **Not done, by rule:** no push, no devnet or mainnet transaction, no rustup target, Playwright, drand download or Agave install. The only service started was `solana-test-validator 3.1.9` on the drill ports 41080/41081/41085/41086 + 41100–41140 (three short runs, each stopped by its script; ledgers deleted; ports verified free afterwards).

## 1. What landed

`permutation-frontier/svm-tests`: its own workspace, lock and toolchain (1.95.0), linking `frontier-abi`, `fclient` (by path) and `permutation-rules` (`std`).

| Module | Content |
|---|---|
| `chain` | LiteSVM 0.16 with mainnet rent (`rent(n) = (128 + n) × 5,080`); the program binary per build (`PSF_SO`, `PSF_SO_TEST_BEACON`, `PSF_SO_TRACE`, `PSF_SO_ORACLE`, else the newest under the repo) **deployed under LoaderV3 exactly as a `--max-len` deploy lays it out** (ProgramData `45 + round_up(1.25 × .so, 4 KiB)`, upgrade authority for AnnounceSeason, I-51); compute-budget profiles `client` (budget CU limit + `L(kind)`) and `ladder` (1.4M CU + `L(kind)`, used by the functional gates so a G1 CU miss never masks a property); **SIMD-0186 enforcement**: the harness computes the loaded size before execution (Σ(64 + data) of existing accounts, instructions sysvar and absent accounts 0, builtins at the validator's data length, + the unlisted ProgramData of every invoked LoaderV3 program) and fails a transaction above its limit with `MaxLoadedAccountsDataSizeExceeded`, **fee charged**; asserts (`assert_code` checks the Frontier program itself raised the §5.4 code), forks, Clock control, account crafting, forgery helpers |
| `fixtures::beacons` | the 32 SP-V2 quicknet rounds (verified on load), **test-beacon rounds** signed on demand with `fclient::beacon::TestKey` (I-53), hints, seeds, forged signatures, `align(round, Δ)` (a `t_create_min` whose genesis round is a given fixture round) |
| `fixtures::tlock` | the S-TLOCK q3/q4 vectors (q4's 165-B compact form opens with the Rust crate; the previous round fails the FO check) and `SealKit`: seals built with the stock `tlock =0.0.10` for every seal code 0–5 with the stock opener's judgement |
| `records` | PS2 bodies from `Program data:` lines, decoded with `frontier_abi::log` (per-kind widths); `ChainWatch` checks a chained account's header moved exactly along its links (`seq + 1`, `head = sha256(prev ‖ le64(seq) ‖ body_without_tail)`) |
| `budget` | `Need` (CU at the ladder, heap peak from the **trace build's** `sol_log_64(0x4355, tag, heap_peak, 0, 0)` checkpoints (W2-A's `heap.rs`), tx bytes, locks, write locks, loaded size) and the §5.5/§10.2 ceilings (`cu_gate`, 28,672-B heap, `tx_ceiling`, 64 locks, `L(kind)`) |
| `wallets` | CL-28: `wallet(i) = Keypair::new_from_array(sha256("join" ‖ le32 i))`, 1,000 seeded, and an adversarial wallet (see D9) |
| `probe` + `probe/` | a stand-alone SBPF v2 program (never deployable): CreateAccount / CreateAccountWithSeed **signed as the real Season PDA** (G2's regression), a no-op padded to any ELF size (`PSF_PROBE_PAD`, the drill), a CPI forwarder (`NotTopLevel` tests) |
| `world/mod.rs` | `World`: a season through the program's own instructions (`announced`, `created`, `seeded`, `running`), placed so its genesis round is fixture round 32,551,361; beacon helpers (`post_anchor` at `T(b)`, `seed_round` = `S(A)` from THE anchor's stored A and the Season's `W(b)`, `post_seed`, `post_beacon`); `craft_archive` (a region-day AnchorArchive with tombstone/archived bits, as ArchiveAnchors (W4-B) leaves it) |
| `world/{land,holding,clash,transit}.rs` | stubs, handed over to W3-A, W3-B, W4-A, W4-B |
| `ix/{season,beacon}.rs` | W2-A's builders (fclient's, the client's account order) + params hashing, masks, account positions for forgeries; `ix/{map,citizen,holding,host,reveal,clash,transit,defence}.rs` stubs |
| `cover/mod.rs` + 10 area files | the (instruction, error) registry: exhaustive `match` over `Ix`, `Cover::{Test, Pending}`, codes `Err(E::X)`, `Lands`, `Refused` (a refusal the contract pins without a code), `Loaded`; `EXEMPT` (14, 61, 99); `season`/`beacon` filled for W2-A's instructions, the rest `Pending(<unit>)` |
| `run.sh` | builds the release `.so` (`scripts/build-frontier.sh`), the feature builds (`test-beacon trace oracle`) and the probe, sets the variables, runs `cargo test --locked "$@"`; `PSF_SKIP_BUILD=1` tests existing builds |
| `drill/validator-drill.sh`, `tests/drill.rs`, `drill/drill-2026-09-27.txt` | the one-time I-45 validator drill and its record |

### Tests (65 plus the opt-in drill)

| File | Tests | What they assert |
|---|---|---|
| `src/**` unit tests | 10 | fixtures (32 rounds verify, hints = blstrs, test key ≠ quicknet, `align`), every seal case judged with its code, the S-TLOCK vectors, PS2 round trip, heap-peak parsing, ceilings, wallets |
| `harness.rs` | 6 | **`g01_loaded_limit_control_*`** (3): the harness rule equals the drill's numbers on the same account set; one byte below the need fails with 5,000 lamports charged, at the need lands, one page below `round_up(need)` fails; raw LiteSVM (enforcement off) lands a transaction ~680 KB under its need. **`g02_probe_create_account_fails_on_a_prefunded_address`**: at the Frontier program's id, CreateAccount of the Season PDA and CreateAccountWithSeed of the canonical Frontier address land on a fresh address and fail ("already in use") once it holds 1 lamport, rent or 10× rent. Build markers, reproducible keys |
| `g01_loaded_limit.rs` | 10 | `g01_loaded_limit_<kind>` for AnnounceSeason, CreateSeason, InitBeaconLogs, InitShards, ConsumeGenesisSeed, SetWindowSchedule, PostAnchor, PostAnchorMulti (7 regions), PostSeed, PostBeacon on the **release** binary at their worst sets (targets pre-funded, archives present): `need ≤ L(kind)`, lands at `L(kind)`, one page below the tight limit fails charged, `L(kind)` within one page of the tight limit and, when equal, `L(kind) − 32 KiB` fails |
| `g02_prefund.rs` | 10 | pre-funding (1, rent, 10× rent) for Season, Frontier + 6 ProvinceFunds + DefencePool, 16 BeaconLogs, 48 JoinShards, BellAnchor (PostAnchor and PostAnchorMulti), SeedCache: lands, payer pays only the shortfall (escrows: topped up or shortfall + escrow), account shape; re-creation: BellAnchor after archive (`Archived`, single and multi), SeedCache after archive (`NoAnchor`), a used Season id (`Announce`) |
| `g03_forgery.rs` | 9 | non-canonical addresses (`BadAddress`) for Season, ProgramData, Frontier/funds/pool, logs, shards, anchors, archives, caches; owner/magic/season/key-field forgeries at canonical addresses (`BadAccount`) for Season, BeaconLog, archive, anchor (read and written), cache; `Auth` for non-authority signers; `RulesetMismatch`; a forged instructions sysvar refused |
| `g04_reveal_window.rs` | 5 | the anchor side of G4: A = Clock (and slot) at creation over random landing times, `T(b)`, the ANCHOR record; tombstoned bell → `Archived`; BeaconLog forward-only (+ `Crypto`); SetWindowSchedule range, 144-bell notice, `Auth`, one pending change, `W(b)` switching; over random clocks and schedules the stored A, `W(b)` and log give the §5.1 predicate both closing conditions |
| `g05_one_anchor.rs` | 7 | one anchor per (bell, region) (repeats are no-ops with no record, multi skips present ones), multi = singles, > 7 regions refused, every nonce the same seed = `seed_of(S(A))`, `S(A)` enforced (`WrongRound`, `Crypto`, `NoAnchor`), `T(b)` enforced, no anchor after archive, the genesis seed is the announced round's (release, real round), `WrongStatus` on a repeat |
| `g13_season_beacon.rs` | 5 | Announce lead/bond, CreateSeason window, params hash (`Announce`), invalid params, foreign beacon key, `WrongStatus`; InitBeaconLogs/InitShards status, `Auth`, faction 6, repeats; **`NotTopLevel` for the five top-level-only instructions via the probe's CPI**; `Auth` for an unsigned fee payer |
| `season_records.rs` | 1 | ANNOUNCE, SEASON_CREATED, GENESIS_SEED, WINDOW: fields, and the Season's event chain (seq 1–4) checked link by link; Frontier and JoinShard headers consistent with their links |
| `coverage.rs` | 2 | G13 guard: every registry entry names a real, non-ignored test whose body asserts its codes; `RELEASE_CHECK=1` fails on `Pending` and on unasserted codes |
| `drill.rs` | 1 (ignored) | the validator drill (run by the script) |

## 2. Measurements [measured, 2026-09-27]

### The I-45 validator drill (`solana-test-validator 3.1.9`, feature set 1620780344)

Probe: SBPF v2, padded to 540,704 B (target: 540,608 B, SP-V2's kprobe size, the `frontier-abi` placeholder), deployed with `solana program deploy --max-len 679,936` (= round_up(1.25 × .so, 4 KiB)) and again at 544,768. Each need is the smallest passing `SetLoadedAccountsDataSizeLimit`: the prediction passed and prediction − 1 failed.

| Item | Result |
|---|---|
| Base transaction (payer, probe program, ComputeBudget; ProgramData not listed) | **680,295 B** = 64 + (64 + 36) + (64 + 679,981) + (64 + 22): the ProgramData counts in full |
| Same ELF at `max_len` 135,168 B smaller | need exactly 135,168 B smaller |
| + an absent address | **+0** |
| + a pre-funded address (lamports, no data) | **+64** |
| + a 10,000-B account | +10,064 |
| + the instructions sysvar | **+0** |
| + the System program listed | **+78** (its account data is `system_program`, 14 B) |
| ComputeBudget program account data | 22 B |
| One byte below the need | `MaxLoadedAccountsDataSizeExceeded`, **landed in a block, fee 5,000 lamports charged** (payer balance −5,000 exactly) |
| One page below `round_up(need, 32 KiB)` | same, charged |

The harness rule reproduces every row (`g01_loaded_limit_control_harness_rule_matches_the_validator_drill`). **LiteSVM 0.16 stores the System program's data as `solana_system_program` (21 B)**, so the harness uses the validator's 14 B for it (and 22 B for ComputeBudget, equal in both).

### The program under test

The branch has no program (W2-A builds it in parallel). To test the tests, I copied W2-A's **uncommitted** worktree (`permutation-frontier/`, `scripts/build-frontier.sh`, root `Cargo.toml`/`Cargo.lock`) into a scratch tree on top of `ed31438` (read-only use; nothing written to W2-A's worktree), built it there with its script, and ran the whole suite with `PSF_SKIP_BUILD=1`: release `.so` **236,288 B**, sha256 `1946f019…d7796d`, e_flags 2, `max_len` 299,008; `test-beacon` and `trace` builds from the same tree. **62 of 65 pass; 3 fail** — the three contract findings of §3 (items 1 and 2). This is a snapshot of work in progress, not W2-A's merged code; the gate must be run again after the merge.

`g01` at that snapshot (programdata `max_len` 299,008 B): every `L(kind)` equals the tight limit (slack < 1 page):

| Kind | need B | `L(kind)` B |
|---|---|---|
| AnnounceSeason | 299,509 | 327,680 |
| CreateSeason | 302,069 | 327,680 |
| InitBeaconLogs | 302,581 | 327,680 |
| InitShards | 302,069 | 327,680 |
| ConsumeGenesisSeed | 301,479 | 327,680 |
| SetWindowSchedule | 301,479 | 327,680 |
| PostAnchor (archive present) | 313,813 | 327,680 |
| PostAnchorMulti (7 regions, 7 archives) | 387,349 | 393,216 |
| PostSeed | 301,829 | 327,680 |
| PostBeacon | 301,671 | 327,680 |

### Other

- The pre-funding regression: CreateAccount and CreateAccountWithSeed of the Frontier program's own addresses fail once the address holds even **1 lamport** (System: "already in use"); they land on a fresh address.
- `#[used]` on a static makes the linker mark the ELF `EI_OSABI = GNU`; `solana program deploy` (3.1.9) then refuses it with "Incompatible ELF: wrong ABI" (v0 and v2 alike). The probe keeps its padding alive with a volatile read instead. W2-A's markers use `black_box`, which is fine; no `#[used]` should enter the program.

## 3. Findings for W2-A and the integrator (not fixed here: outside W2-B's files)

1. **PostAnchor and PostSeed take any program-owned account at the canonical address as "present"** (W2-A snapshot: `anchor.owner == p` / `cache.owner == p` → success no-op). §4.1: presence is authenticated by owner, magic, season id and key fields; G3 (§13.2) requires `BadAccount` for each forgery "for every account kind any instruction reads or writes". `g03_anchor_and_archive_forged_in_post_anchor` and `g03_anchor_and_cache_forged_in_post_seed` fail on the snapshot for the magic/season/key forgeries. Fix: `prologue::presence(..)` (as PostSeed already does for THE anchor it reads) before the no-op.
2. **SetWindowSchedule accepts a program-owned copy of the Season at another address** (the keeper and authority prologues do not recompute the Season PDA from the stored id and bump; `read_season`'s doc leaves it to the caller). In PostBeacon the log's canonical-address check catches the copy; in SetWindowSchedule nothing does. `g03_season_forged_in_keeper_and_authority_instructions` fails on the snapshot. Fix: `create_program_address(["season", le64(id), [bump]])` against the key in every prologue that trusts the Season (§3.3 "recompute every keyed address a reader trusts").
3. **`L(kind)` over-counts** by < 1 KiB–2 KiB (`frontier_abi::budgets`): it counts the instructions sysvar at its data length (the runtime counts 0) and each builtin at 64 B (System 14, ComputeBudget 22). Safe (never under the need) and tight at today's sizes, but it can put `L(kind)` one page above the tight limit for some future `.so` length, which would break the literal "one page less fails" (`g01` then prints a note and still passes, see the helper). W5-A, when it regenerates `L(kind)` from the release `.so`: count the sysvar as 0 and builtins at 14/22 B.
4. The contract's `solana-test-validator` drill ports and the §10.3 pubsub port: the validator derives pubsub from RPC + 1 (41081), as §10.3 lists; nothing else needed.
5. Gate W2's `./run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_` stops at the first failing test binary (cargo), so with finding 1/2 open the later files (`g04_`, `g05_`, and `harness.rs`'s controls) are not reported; `--no-fail-fast` shows all (used for §2).

## 4. Deviations and their reasons

| # | Deviation | Why | Who resolves |
|---|---|---|---|
| D1 | The harness depends on `fclient` (path) for builders, beacons, the test key, seals, fees and tx encoders | one set of builders (the client's account order, §3.3 "hand-copied constants are a review failure"); `fclient` already had all of it | W2-F owns `fclient` in wave 2: keep `fclient::ix`, `beacon::{TestKey, beacon_arg, hints_bytes, FixtureDrand, seed_of}`, `seal::*`, `tx::*`, `addr::*` source-compatible, or the integrator adapts `svm-tests` at the merge |
| D2 | G4 is the anchor side only | Reveal (W3-B) does not exist in wave 2; every input of the §5.1 predicate W2-A writes is asserted, the Reveal-side property is W3-B's over the same accounts | W3-B |
| D3 | Crafted accounts: AnchorArchives with tombstone bits (ArchiveAnchors is W4-B's), and in `g01` on the release binary a crafted `genesis_ts` and anchor Clock so `T(0)` and `S(A)` are real fixture rounds (the 32 SP-V2 rounds are not 200 rounds apart) | the release binary verifies only real rounds and O-M1-12 (the archive) is not approved | W4-B replaces the crafted archives with ArchiveAnchors; the real-round archive after O-M1-12 |
| D4 | Functional gates send at the ladder profile (1.4M CU, `L(kind)`); the client profile (budget CU limit) is there for G1 | budgets are gates, not liveness limits (I-50); G1 is W5-A's | W5-A |
| D5 | Codes the contract leaves open are asserted as refusals (`Refused`), not by code: PostBeacon of an old round, CreateSeason outside its window or with invalid params, init repeats, faction 6, SetWindowSchedule range/notice/pending, a foreign ProgramData or instructions sysvar | the contract pins no code there; W2-A pins some (e.g. `AlreadyDone`, `TooEarly`, `BadData`) in its notes | W5-A (G13 completion) may tighten them to W2-A's pinned codes |
| D6 | `cover/{season,beacon}.rs` keep one `Pending("W5-A: …")` row each for codes no wave-2 test asserts (BadData ranges, TooManyAccounts, WrongStatus for anchors, TooEarly of the test-beacon build) | G13 is W5-A's; the guard accepts `Pending` unless `RELEASE_CHECK=1` | W5-A |
| D7 | The harness counts builtins at the validator's data length, not LiteSVM's | drill (§2): System 14 B on the validator, 21 B in LiteSVM | none |
| D8 | The drill's probe is padded to SP-V2's 540,608 B (the `frontier-abi` placeholder), not the M1 release size | no release `.so` existed; the drill script pads to the release `.so` automatically once one is found (`PSF_DRILL_SO_LEN` overrides); the rule it measured does not depend on the size | re-run optional after W2-A merges |
| D9 | The adversarial Join wallet is chosen for contention (JoinShard 0) and tag order, not a "long hash path" | M1 Citizens are with-seed addresses (I-01): no bump search exists, Join hashes fixed-length input for every wallet | none (W3-A's Join budget test uses it) |
| D10 | The program id is a fixed harness key (`PSF_PROGRAM_ID` overrides) | the W2-A snapshot derives every address from the id it runs under; no `declare_id!` | integrator, if a fixed id is ever pinned |

## 5. Gate W2 items that concern these files

| Item | Result |
|---|---|
| `(cd permutation-frontier/svm-tests && ./run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_)` | **not runnable on this branch as written** (no `scripts/build-frontier.sh`, no program: W2-A). Run with `PSF_SKIP_BUILD=1` against the W2-A snapshot: g01 10/10, g02 10/10, g03 6/9 (findings 1–2), g04 5/5, g05 7/7; the harness controls `g01_loaded_limit_control_*` 3/3 and `g02_probe_*` 1/1 |
| "`g01_loaded_limit_*` green against the release `.so` with the harness's enforcement control" | green on the snapshot (10/10 + 3 controls) |
| "the validator drill result recorded in W2-B notes" | §2; record in `drill/drill-2026-09-27.txt` |
| `cargo fmt -- --check`, `cargo clippy --locked --all-targets -- -D warnings` (svm-tests, 1.95.0) | pass |
| same for `probe/` (host) | pass |
| `cargo test --locked` without a program (lib, `coverage`, `harness`) | pass (18) |
| wasm32 / Playwright / archive items | not W2-B's; nothing `PENDING-OWNER` here |

## 6. Dependency requests (integrator, I-55)

- **R1 — new workspace files** (created so the branch builds; please adopt or re-create): `permutation-frontier/svm-tests/Cargo.toml` (own `[workspace]`), `Cargo.lock` (seeded from `frontier-node/Cargo.lock`, pruned by cargo; no version differs), `rust-toolchain.toml` (`1.95.0`, minimal, no `components` line), `.cargo/config.toml` (`target-dir = "target"`, so builds stay in `svm-tests/target` rather than the root config's `permutation-chain/target`); `probe/Cargo.toml` (own `[workspace]`) and `probe/Cargo.lock` (seeded from the root lock, pruned; 156 packages).
- **R2 — pinned crates**, all already in `frontier-node`'s or the root lock: `litesvm =0.16.0`; `solana-account =4.3.2`, `solana-address =2.6.1` (`curve25519`, `sha2`, `decode`), `solana-clock =3.1.1`, `solana-hash =4.5.0`, `solana-instruction =3.4.1`, `solana-keypair =3.1.2`, `solana-message =4.4.1`, `solana-rent =4.3.0`, `solana-signature =3.4.1`, `solana-signer =3.0.1`, `solana-transaction =4.1.6` (`serde`, `wincode`, `verify`); `sha2 =0.10.9`, `hex =0.4.3`, `base64 =0.22.1`, `bincode =1.3.3`, `serde_json =1.0.151`; paths `frontier-abi`, `frontier-node/crates/fclient`, `permutation-rules` (`std`). Probe: `solana-program =4.0.0` (the root lock's).
- **R3 — root `Cargo.toml`:** `exclude += "permutation-frontier/svm-tests"` (§3.1) once `permutation-frontier` joins the root workspace, so a root `cargo` never tries to adopt the harness.
- **R4 — CI** (W1-D's `frontier-program` job): run `permutation-frontier/svm-tests/run.sh --release` (it builds the program and the probe); the drill stays manual (one-time, needs the validator).

## 7. For the next units

- **W2-A:** findings 1–2 (§3) are the only failures against your snapshot. `run.sh` calls `scripts/build-frontier.sh` and `--features test-beacon|trace|oracle`, and reads `permutation-frontier/target/deploy{,-<feature>}/permutation_frontier.so`, which is what your script writes.
- **W3-A / W3-B / W4-A / W4-B:** start from `World::running(&mut Chain::test_beacon(), id)` (any round) or `Chain::release()` (real fixture rounds only); area stubs `world/<area>.rs`, `ix/<area>.rs`, `cover/<area>.rs` are yours. Use `assert_code` for pinned codes, `Refused` entries for open ones, `ChainWatch` for chained accounts, `Chain::measure` for G1 numbers (heap from `Build::Trace`), `probe::cpi` for `NotTopLevel`, `fixtures::tlock::SealKit` for seals of every code, `wallets` for Join.
- **W5-A:** `RELEASE_CHECK=1 ./run.sh --release -- g13_` turns the registry into the release gate; finding 3 when regenerating `L(kind)`.

## 8. Commands run (all from `permutation-frontier/svm-tests` unless stated)

```
cargo build --offline --tests                      # first build fetched nothing (lock seeded from frontier-node)
cargo test --offline --lib                         # 10 pass
cargo test --offline --test harness                # 6 pass (probe built with cargo-build-sbf --arch v2)
drill/validator-drill.sh <scratch>/w2b-drill2      # exit 0, see §2 and drill/drill-2026-09-27.txt
PSF_SKIP_BUILD=1 PSF_SO=<snapshot>/deploy/… PSF_SO_TEST_BEACON=… PSF_SO_TRACE=… ./run.sh --release --no-fail-fast   # 62/65, §2
./run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_   # same env: g01 10/10, g02 10/10, g03 6/9
cargo fmt -- --check; cargo clippy --offline --locked --all-targets -- -D warnings      # pass
(cd probe && cargo fmt -- --check && cargo clippy --offline --locked --all-targets -- -D warnings)   # pass
cargo test --offline --test coverage               # 2 pass
```

## 9. Post-merge addendum (integrator, integ-W2 window, 2026-09-28)

Written by the integrator after the wave-2 review; the sections above are kept as submitted.

- **Seal codes.** The kit produces codes 0, 1, 2, 4 and 5; "every seal code 0–5" above is corrected: code 3 (wrong round) is indistinguishable from 1 and contract v1.3 reserves it (§5.3, §13.1 lists 0, 1, 2, 4, 5). `SealCase::WrongRound` → 1 is right.
- **Deviations not listed above:** (1) most G2–G5 tests run on the `test-beacon` build (any round, I-53); the release build runs G1, G2 and the season parts, and G2–G5 move to the release build with real rounds once O-M1-12 allows them (W6); (2) the I-45 drill ran on a probe padded to the 540,608-B placeholder, not the release `.so` size (237,384 B now); the size-independence argument (two `max_len` measured) stands and a re-run stays optional; (3) `check()` accepts `L(kind) == tight + PAGE` (finding 3) until W5-A regenerates `L(kind)`; (4) `assert_within` checks heap only on the trace build (a plain build reports no heap).
- **Added by the integrator:** `g03_anchor_and_archive_forged_in_post_anchor_multi`, `g03_season_copy_and_id_flip_in_every_w2a_instruction`, `g03_forged_present_targets_of_the_init_instructions`, `g01_loaded_limit_*_present` (no-op paths), `g01_budget_w2a_*`, and the tighter `assert_refused` (a program code is required) and `RELEASE_CHECK=1` NotImplemented panic in `chain.rs`. The per-(instruction, code) completeness table stays W5-A's.
