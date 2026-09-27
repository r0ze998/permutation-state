# W2-A program-core: notes

**Unit:** W2-A program-core (wave 2). **Branch:** `frontier/m1-W2-A`, cut from `frontier/m1-integ` at `ed31438`. **Contract:** M1-CONTRACT v1.2 §3, §4, §5.1–§5.8, §5.12, §6, §10.1, §11 (W2-A row), §12 (Gate W2), §13.1–§13.2. **Date:** 2026-09-27.

## 1. What landed

A new crate `permutation-frontier/` (root workspace member, cdylib + lib) and `scripts/build-frontier.sh`.

| Path | Content |
|---|---|
| `src/lib.rs` | features (`program`, `custom-heap` default; `no-entrypoint`, `trace`, `oracle`, `test-beacon`), `dispatch` for **all 50 tags** (ResolveClash only in the `oracle` build, `BadData` otherwise), the `cfg(target_os = "solana")` entrypoint, `RULESET_HASH` (= `frontier_abi::presets::RULESET_HASH`), `QUICKNET_PK_HASH` (the binary's beacon key) |
| `src/error.rs` | `Error` = a `FrontierError` code or a runtime `ProgramError` passed through; `Crypto` (9) and `Kernel` (15) log a sub-code with `sol_log_64("PSFE", code, sub)` |
| `src/ix.rs` | re-exports `frontier_abi::{ix, tags}`; `mask_regions`; `legacy_tx_len` (byte-exact legacy transaction size) |
| `src/addr.rs` | the kernel/ABI address grammar re-exported; Season PDA seeds; absence rule |
| `src/clock.rs` | `SeasonClock` binds the kernel `frontier::beacon` functions (T(b), W(b), close, S(A), genesis round and `genesis_ts`) to the Season's stored fields |
| `src/events.rs` | PS2 records with per-entity event heads (`record` pure, `emit` logs); tail sorted by entity kind, stable in account order; `WIDTHS`, a compile-time integer table of every kind's key/payload widths, and the program's own body writer (see finding F1) |
| `src/evidence.rs` | allocation-free parser of the instructions sysvar: `SetComputeUnitLimit`, `SetComputeUnitPrice`, `SetLoadedAccountsDataSizeLimit`, `RequestHeapFrame` (absent = 0) + the landing slot |
| `src/heap.rs` | the 256-KiB-aware upward bump allocator (I-50) as a host-tested `Bump` plus the SBF `#[global_allocator]`; `peak()`; `trace_checkpoint` (trace builds only) |
| `src/markers.rs` | `PSF_TRACE_BUILD`, `PSF_ORACLE_BUILD`, `PSF_TEST_BEACON_BUILD`, kept alive by `black_box` on paths every instruction / every beacon verification runs |
| `src/layout/{mod,world,player,land,clash,beacon}.rs` | bounds-checked `Ro`/`Rw` accessors over the `frontier-abi` offsets, headers, event chains, typed `SeasonCore`, `Anchor`, `Cache`, archive bits and entries. Frozen after wave 2 (§11): later waves use `Ro`/`Rw` with the ABI constants. `layout/text_check.rs` is the text-driven §5.3 test (below) |
| `src/crypto/{sys,field,xmd,quick,seal}.rs` + `consts/*.bin` | SP-V2 port: SIMD-0388 syscalls, field codecs with `#[inline(never)]` mul/square, RFC 9380 `expand_message_xmd`, hinted quicknet verification, seeds; `seal::{open, judge}` = the tlock opener with the FO check returning the contract's seal codes (for SettleTransit, W4-B); feature `test-beacon` pins the local test key (I-53) |
| `src/init.rs` (`program`) | `init_pda` (Season), `init_with_seed` (top-up + `AllocateWithSeed` signed by the Season PDA), `init_funded` (shortfall from a program-owned fund that stays rent-exempt), `move_lamports`, `pay_or_divert` with sinks `PoolOwed(holding)` / `DefencePool(dpool)` and the `DIVERT` record, `close_to`; never `CreateAccount*` |
| `src/prologue.rs` (`program`) | the thin wrapper over `frontier_abi::prologue`: `check_accounts` (exact count, signers `Auth`, writability `BadAccount`, fixed ids), `top_level`, `season`, `keeper`, `presence`/`present`, `expect_key`, `now` |
| `src/proc/{season,beacon}.rs` | **implemented:** AnnounceSeason, CreateSeason, InitBeaconLogs, InitShards, ConsumeGenesisSeed, SetWindowSchedule; PostAnchor, PostAnchorMulti, PostSeed, PostBeacon. Stubs (`NotImplemented`, handed to W4-B): EndSeason, AbortSeason, CloseSeason, ArchiveAnchors, CloseSeedCache |
| `src/proc/{map,citizen,holding,host,reveal,clash,transit,defence}.rs` | stubs returning `NotImplemented` (99), one function per instruction, handed to W3-A, W3-B, W4-A, W4-B (table in `proc/mod.rs`) |
| `scripts/build-frontier.sh` | `solana-cargo-build-sbf 3.1.9`, `--tools-version v1.52 --arch v2`; refuses unless `e_flags == 2`, the overflow panic strings are present, **no test marker** is present (a `--features trace\|oracle\|test-beacon` build must carry its own marker and none other, goes to `target/deploy-<feature>/`, "deployable no"), and **no data relocation carries a non-zero addend** (finding F1); prints `file_sha256`, `program_hash`, `e_flags`, `so_len`, `max_len` (`round_up(1.25 × .so, 4 KiB)`), `programdata` (45 + max_len); `--twice` builds in two fresh target dirs and fails on a hash mismatch |

Root manifest (integrator-owned, changed on this branch to build; see §6): `members += permutation-frontier`; `exclude += permutation-frontier/svm-tests, frontier-wasm`; `[profile.release.package.{permutation-frontier,frontier-abi}] overflow-checks = true` (the integ-W1 addendum left both profile entries for W2-A).

## 2. The implemented instructions (behaviour pinned where §5.7/§5.8 leave it open)

| Instruction | Checks in order | Pinned by W2-A |
|---|---|---|
| AnnounceSeason 0x08 | accounts; authority = upgrade authority from the LoaderV3 Program → ProgramData (`Auth`; an immutable program has none → `Auth`); Season PDA found once (`BadAddress`), absent else `Announce` (id used); `t_create_min ≥ now + 86,400` and `bond ≥ 1 SOL` else `Announce` | the Season holds rent + bond (pre-funded lamports count); `window_from_bell = u32::MAX` from the start |
| CreateSeason 0x01 | accounts; Season status Announced (`WrongStatus`); stored authority signed (`Auth`); `now < t_create_min` → `TooEarly`, `now ≥ t_create_min + 7 d` → `Announce`; params hash (`Announce`); `SeasonParams::validate`, `quicknet_pk_hash == the binary's`, `PayoutParams` borsh + `validate_for_season` (`BadData`); targets canonical (`BadAddress`) and absent | `genesis_round = genesis_seed_round(t_create_min, Δ)`, `genesis_ts = round_time + 600`; each wedge fund gets `pfund_initial / 6` (remainder unfunded) above rent, `funded_total` = that; DefencePool gets `dpool_initial` above rent and both caps; `payout_params_hash = sha256(PayoutParams borsh)`; `window_next = reveal_window`; ruleset, rules/program version, `reveal_loaded_limit`, `join_gate` stored; `SEASON_CREATED` |
| InitBeaconLogs 0x09 / InitShards 0x02 | accounts; status ∈ {Created, Seeded, Running}; ruleset; authority; faction ≤ 5 (`BadData`); each canonical (`BadAddress`) | a present target is `AlreadyDone` (52), the whole transaction fails (no partial init); no log (§6) |
| ConsumeGenesisSeed 0x03 | accounts; top-level; keeper prologue, status Created; `round == genesis_round` (`WrongRound`); verify (`Crypto`) | `GENESIS_SEED` with the current bell (`u32::MAX` before genesis) |
| SetWindowSchedule 0x07 | accounts; status ∈ {Created, Seeded, Running}; authority; `600 ≤ window ≤ 1,800` (`BadData`); `from_bell ≥ now_bell + 144` (`TooEarly`) | "no pending change": `TooEarly` until the previous change has been in effect for 144 bells, then `reveal_window := window_next` before the new schedule (only bells more than a day old can see their `W` re-read; §5.1 needs `W(b)` stable while a bell can still be anchored, so a stricter rule may be wanted: owner/integrator) |
| PostAnchor 0x10 / PostAnchorMulti 0x11 | accounts (Multi: counts `[1, k, k, 1]` from `mask`, `1 ≤ k ≤ 7`); top-level; keeper prologue, status ∈ {Running, Ended}; region < 16, `bell < end_bell` (`BadData`); per anchor: canonical (`BadAddress`), present → skip (no-op), archive canonical and tombstoned → `Archived`; `round == T(bell)` (`WrongRound`); one verification | anchor `{bell, region, net 2, round, A = Clock, slot, sig48, rent_to = fee payer, ev_price, ev_limit}`; `ANCHOR` per created anchor; Multi with every anchor present is a no-op before any verification |
| PostSeed 0x12 | accounts; top-level; keeper prologue {Running, Ended}; THE anchor canonical and present (`NoAnchor`), key fields match (`BadAccount`); cache canonical, present → no-op; `round == S(A)` with `W(bell)` (`WrongRound`); verify | cache `{…, seed = seed_of(round, sig96), anchor_key, A, slot, rent_to}`; `SEED` |
| PostBeacon 0x13 | accounts; top-level; keeper prologue {Seeded, Running, Ended}; region; log canonical and present; `round > latest_round` else **`AlreadyDone`**; verify | `BENEFICIARY` = the fee payer (the data carries none) |

`test-beacon` builds only: a round the chain's Clock has not reached is `TooEarly` in every beacon instruction, because the test key can be derived by anyone (a real quicknet round cannot exist before its time, so the release build does not carry the check).

## 3. Measurements [measured, 2026-09-27]

**Build** (`scripts/build-frontier.sh --twice`, platform-tools v1.52, SBPF v2): `file_sha256 f50612177cccd58a4ce41348da3d3f109bd2dd09b24ee7b2404e903b57358e1f`, `program_hash 0ad356b2d264c683e7e82f7aaae8265e5286d601c1dbee7e9d44179ffaff98c8`, `e_flags 2`, 3 overflow panic strings, **.so 236,288 B, `--max-len` 299,008, ProgramData 299,053 B**; both builds and the in-place build hash the same. Feature builds: test-beacon 237,136 B, trace 238,536 B, oracle 236,328 B, each with its marker only.

**On chain** — a scratch LiteSVM harness (not committed; `(session scratch)/scratchpad/w2a/smoke/`) over `frontier-localnet`'s `Chain` (LiteSVM 0.16, mainnet features incl. SIMD-0388, rent 5,080/B, SIMD-0186 loaded-data check at 1 MiB, the program deployed under LoaderV3 with an upgrade authority) and `fclient`'s instruction builders. Two runs, 0 failures each:
- **release `.so` with real quicknet rounds** (SP-V2 fixtures): ConsumeGenesisSeed on round 32,551,361; the Season's `genesis_ts` then patched 600 s earlier (the only non-program write) so `T(0)` = 32,551,561, and THE anchor posted at `A = round_time(32,551,846) − 660` so `S(A)` = 32,551,846 (a fixture round);
- **test-beacon `.so`** with fclient's `TestKey` for every round.

| Instruction (worst path run) | CU release | CU test-beacon | Budget §5.5 | Tx B | Heap peak (trace) |
|---|---|---|---|---|---|
| AnnounceSeason (pre-funded PDA) | 11,220–15,720 (bump search) | 11,220 | 18k | 387 | 984 |
| CreateSeason (Frontier pre-funded: 8 transfers + 9 allocations) | 40,300 | 40,228 | 70k | 814 | 4,246 |
| InitShards (8 shards) | 39,197 | 39,125 | 40k | 562 | 4,418 |
| **InitBeaconLogs (16 logs)** | **74,564** | 74,420 | **60k** | 825 | 8,336 |
| ConsumeGenesisSeed | 329,224 | 329,272 | 345k | 611 | 400 |
| PostAnchor (anchor pre-funded) | 337,444 | 336,964 | 345k | 780 | 1,212 |
| PostAnchor, present (no-op) | 4,271 | 4,271 | — | 780 | 1,168 |
| PostAnchorMulti, 7 regions, 1 present | 372,412 | 371,885 | 400k | **1,177** | 4,552 |
| PostAnchorMulti, all present (no verify) | 11,116 | 11,116 | — | 1,177 | — |
| PostSeed | 338,712 | 338,245 | 345k | 781 | 1,216 |
| PostSeed, present (no-op) | 4,776 | 4,776 | — | 781 | 1,168 |
| PostBeacon | 328,654 | 331,697 | 340k | 645 | 592 |
| SetWindowSchedule | 3,386 | 3,386 | 5k | 272 | 400 |

A PostAnchorMulti with 8 regions is **1,243 B** on the wire (1,232 max): `MULTI_MAX_REGIONS = 7` confirmed on the wire and pinned by the host test `multi_max_regions_is_the_largest_that_fits_a_packet` (1,177 / 1,243 B from the ABI account table and a byte-exact legacy size).

Refusals exercised on chain (code as returned): AnnounceSeason by a non-authority 4, lead < 24 h 56, bond < 1 SOL 56, id used 56; CreateSeason before `t_create_min` 13, params hash mismatch 56, non-authority 4, twice 5; InitShards again 52, faction 6 → 1; ConsumeGenesisSeed wrong round 7, another round's signature 9, again 5, before its round (test key) 13; PostAnchor wrong round 7, region 16 → 1, bell ≥ `end_bell` 1, non-canonical archive 3; PostSeed without THE anchor 8, wrong round 7, non-canonical anchor 3, before S is due (test key) 13; PostBeacon same round 52, non-canonical log 3, future round (test key) 13; SetWindowSchedule notice < 144 bells 13, window 1,801 → 1, while pending 13; EndSeason and OpenRing stubs 99. Pre-funding: Season PDA (authority paid only the shortfall), Frontier at 10× rent (kept its lamports), BellAnchor at half rent (topped up to rent). The Season's event chain replayed from the PS2 records (ANNOUNCE, SEASON_CREATED, GENESIS_SEED, WINDOW) equals the account's seq and head; the ANCHOR/SEED/BEACON records decode with `frontier_abi::log::decode`; the anchor's evidence (price 1,234 µlamports, limit 400,000) and `rent_to` are stored.

**`L(kind)` at this `.so`** (`frontier_abi::budgets::loaded_limit_for(ix, 299,008)`): largest SkipQuiet 622,592, PostAnchorMulti 393,216, Reveal / GatherClash / RFI / SettleTicket / SettleExplore 360,448, every other kind 327,680 (the placeholder at SP-V2's 540,608-B `.so` gives 688,128–983,040). They grow with every wave's code; W5-A regenerates them from the release `.so`.

## 4. Findings for the integrator and later units

- **F1 — SBPF v2 drops the addend of data relocations [measured].** Platform-tools v1.52 with `--arch v2` writes a pointer into the *middle* of a static object stored in `.data.rel.ro` (here the switch table LLVM makes of `frontier_abi::log::Kind::spec()`, i.e. `&SPECS[i]`) as `R_SBF_64_RELATIVE` with value `(object_base << 32) | offset`; the loader (solana-sbpf 0.21.1 in LiteSVM 0.16: for a non-text relocation it reads only the high word) resolves every entry to the object's start, so on chain `Kind::spec()` returned `SPECS[0]` (ANNOUNCE) for every kind and `log::write_body` refused CreateSeason's record (seen as `BadAccount`). Pointers to the start of an object (addend 0: the prologue's account tables) are unaffected. The program therefore never calls `Kind::spec()`, `log::write_body`, `log::decode` or `log::field` on chain: `events::WIDTHS` is a compile-time integer table and `events::write_body` writes the same bytes (host test `bodies_are_the_abis` over all 41 kinds). **`build-frontier.sh` now refuses any artefact with such a relocation** (the pre-fix builds had 40, the fixed one 0), so a later wave that links a pointer-table kernel path finds out at build time. Requests: (a) the integrator / frontier-abi owner may change `Kind::spec()` to index a const integer table (host behaviour unchanged); (b) W2-B's validator drill can confirm whether `solana-test-validator 3.1.9` loads it the same way (not measured here; the loader crate is the same family).
- **F2 — AnchorArchive (12,192 B) cannot be created by one instruction [measured].** A CPI can allocate at most 10,240 B (`MAX_PERMITTED_DATA_INCREASE`): a probe program's CPI `Allocate(10,240)` lands, `Allocate(10,241)` and `Allocate(12,192)` fail `InvalidRealloc` in LiteSVM 0.16. ArchiveAnchors (W4-B) needs an amendment: e.g. two half-day archives (72 entries, 6,144 B), or create at ≤ 10,240 B and grow in a second instruction, or a smaller entry.
- **F3 — budgets.** InitBeaconLogs measures 74.6k against 60k: each created account costs ≈ 3.7k CU in its two CPIs (System Transfer ≈ 1.8k, `AllocateWithSeed` signed by the Season PDA ≈ 1.9k), so 16 accounts cannot fit 60k; proposal **80k** (or two calls of 8, an ABI change). InitShards 39.2k is within 40k with little margin (proposal 45k). AnnounceSeason's cost depends on the Season PDA's bump search (+≈ 1.5k per extra try; 11.2k and 15.7k measured); 18k is exceeded when the bump is found only after ≥ 5 tries (≈ 3% of ids): proposal 25k. The no-op re-posts cost 4.3–4.8k (SP-V2's 1.75k had no account-table and keeper-prologue checks). All four are O/N-class or idempotent; W5-A regenerates the table.
- **F4 — evidence when a ComputeBudget instruction is absent** is stored as 0 (`ev_limit = 0`, `ev_loaded = 0`); keeper transactions always carry all three.
- **F5 — `payout_params_hash`** is pinned as `sha256(PayoutParams borsh)` (the contract names the field only); the verifier (W4-D) recomputes it from CreateSeason's data.
- **F6 — CreateSeason requires `SeasonParams.quicknet_pk_hash` to be the binary's key hash** (`BadData` otherwise), so a test-beacon season must set `faa6e379…b9ef` (fclient `beacon::pk_hash(TestKey::new().pk96)`); `M1_LOCAL_7D` names quicknet.

## 5. Tests and gate items run (all exit 0)

```
cargo fmt --all -- --check
cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings
cargo clippy --locked -p permutation-frontier --no-default-features --all-targets -- -D warnings
cargo clippy --locked -p permutation-frontier --all-targets --features trace,oracle,test-beacon -- -D warnings
cargo test --locked -p permutation-frontier --no-default-features      # 35 host tests
cargo test --locked -p permutation-frontier                            # 39 (with the program layer)
cargo test --locked -p permutation-frontier --features test-beacon,oracle,trace   # 39
scripts/build-frontier.sh --twice                                      # hashes above
scripts/build-frontier.sh --features trace | oracle | test-beacon      # markers checked
cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings
cargo test --locked --release -p permutation-rules
cargo test --locked -p frontier-abi
cargo run --locked -p frontier-abi --bin abi-vectors -- --check        # 9 files fresh
cargo test --locked -p permutation-chain
git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src
```
Host tests of note: the tlock opener's steps against all 19 `seal-vectors-v1.json` cases stock tlock opens (`k = W ⊕ H4(σ)`, `H3(σ, k)·G2 = U` with ark, body and commitment, the validation verdict); RFC 9380 `expand_message_xmd` vectors; the embedded G2 keys are the compressed keys the Season pins (x, sign bit, on curve, in the subgroup) for quicknet and the test key; the program's rounds equal the kernel's over every drand phase; the §5.3 **text-driven offsets test** (`layout::text_check`, 332 stated offsets and every stated size checked against the `frontier-abi` layouts, with controls that a shifted offset or size in the text fails); PS2 records advance chains in tail order; the evidence parser over serialized sysvars.

Not run by W2-A (other units' files or blocked): `permutation-frontier/svm-tests/run.sh` g01/g02–g05 (W2-B writes them against this program), `frontier-node` workspace tests (W1-F/W2-C/W2-F files; no dependency on this crate), `permutation-gateway` `npm test` and `sync-web-sdk --check` (W2-D/W2-E), frontier-sim gate lines (unchanged files), civilization tests. **`build-wasm.sh --check` is PENDING-OWNER (wasm32 target, O-M1-12)**. Nothing was downloaded or installed; no port was bound (the harness is in-process); no chain transaction; no push.

## 6. Dependency requests (integrator-owned files changed on this branch to build)

- Root `Cargo.toml`: `members += "permutation-frontier"`; `exclude += "permutation-frontier/svm-tests", "frontier-wasm"` (§3.1); `[profile.release.package.permutation-frontier] overflow-checks = true` and `[profile.release.package.frontier-abi] overflow-checks = true`.
- Root `Cargo.lock`: **20 packages added, none changed or removed** (`ark-bls12-381`, `ark-ec`, `ark-ff`, `ark-ff-asm`, `ark-ff-macros`, `ark-poly`, `ark-serialize`, `ark-serialize-derive`, `ark-std` 0.5.0 and their deps `allocator-api2`, `educe`, `either`, `enum-ordinalize(-derive)`, `fnv`, `hashbrown`, `itertools`, `paste`, `zeroize_derive`, plus `permutation-frontier`), resolved offline from the registry cache; `cargo metadata --locked` passes (the I-37 probe predicted exactly this).
- `permutation-frontier/Cargo.toml` `[dependencies]`: `solana-program =4.0.0` (already in the lock: entrypoint, CPI, sysvars), `ark-ff`/`ark-ec`/`ark-bls12-381 =0.5.0` default-features off (hinted hash-to-curve, SP-V2), `borsh =1.6.1` default-features off (CreateSeason decodes `PayoutParams`; already in the lock), path deps `permutation-rules`, `frontier-abi`.
- `docs/frontier/DECISIONS.md` (not a W2-A path; written because the wave-2 brief asked for it): one part-A row recording that the 1.95.0 `rustfmt`/`clippy` components were installed with the owner's explicit OK.

## 7. Hand-over notes

- **W2-B (svm-harness):** build variants with `scripts/build-frontier.sh [--features test-beacon|trace|oracle]` (outputs in `permutation-frontier/target/deploy*/`); deploy under LoaderV3 with an upgrade authority (AnnounceSeason reads it; `localnet::Chain::deploy` does this); a test-beacon season must name the test key's hash (F6); `round_is_due` makes test-key rounds `TooEarly` before their time. The scratch smoke (`scratchpad/w2a/smoke/src/main.rs`) shows the account lists, the genesis-round alignment trick for real fixture rounds and the refusal codes; g02–g05 and `g01_loaded_limit_*` can start from its sequence.
- **W3/W4 units:** use `init::{init_with_seed, init_funded, pay_or_divert, close_to}` (the last three are not exercised on chain by any wave-2 instruction), `events::emit` with `Chained` accounts in instruction order, `layout::{Ro, Rw}` with the ABI offsets, `prologue::{check_accounts, keeper, season, presence}`, `clock::SeasonClock`, `evidence::read`, `crypto::seal::judge` (SettleTransit). Avoid static tables of interior pointers on chain (F1): `build-frontier.sh` will refuse them.
- **W4-B:** ArchiveAnchors cannot create the 12,192-B archive in one instruction (F2).
