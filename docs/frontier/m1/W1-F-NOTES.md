# W1-F node-foundation — notes

- **Unit:** W1-F (wave 1), branch `frontier/m1-W1-F` cut from `frontier/m1-integ` at `d9d32ec`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.1, §8.1 (`fclient`), §8.7 (`localnet`, `drand-replay`), §10.1, §10.3, §11 (W1-F brief), §12 Gate W1.
- **Owned paths touched:** `frontier-node/crates/**`, `frontier-node/README.md`, `permutation-gateway/test/frontier-vectors.json`, this file.
- **Integrator-owned files created on this branch to build** (dependency requests, below): `frontier-node/Cargo.toml`, `frontier-node/Cargo.lock`, `frontier-node/rust-toolchain.toml`, `frontier-node/.cargo/config.toml`.
- **Tags:** [measured] = run on this machine on 2026-09-27; [design] = the contract's rule as implemented.
- **Not done, by rule:** no push, no devnet or mainnet transaction, no `rustup` target or component install, no Playwright, no drand download, no Agave install. The only services started were test servers on `127.0.0.1:0` and a smoke run on the unit's ad-hoc ports 41660–41661 (§10.3: unit 6 → 41660–41669), all stopped.

## 1. What landed

### `frontier-node` workspace (11 crates)

`fclient`, `findex`, `keeper` (lib `keeper_core`, bin `frontier-keeper`), `herald` (lib `herald_fold`, bin `frontier-herald`), `verify` (lib `verify_core`, bin `frontier-verify`), `agents` (lib `frontier_agents`), `bots` (bin `frontier-bots`), `localnet` (bin `frontier-localnet`), `drand-replay` (bin `drand-replay`), `stack` (bin `frontier-stack`), `itest`. Package names equal the directory names, so the later gates' `-p verify`, `-p itest` and `frontier-node/target/release/frontier-stack` resolve. The skeletons compile, carry one test each, and hold the first piece their next owner needs (feed pull and PS2 split; the §8.2 bid schedule; the §9.3 overview header; the §8.5 report schema and exit codes; the 13 adversarial personas of §8.6; the port rule as `frontier-stack check-ports --ports …`; an in-process smoke).

### `fclient` (complete for §8.1)

| Module | Content |
|---|---|
| `abi` | all 50 tags (49 + oracle ResolveClash) with class, CU budget, tx max and top-level flag; magics; sizes; rent; the 62 error codes and the keeper mapping; 41 PS2 kinds; entity kinds; seal codes; **every §5.3 offset**; the layout end/tiling checks as `const` assertions |
| `addr` | SP-V2 seed grammar byte for byte (`tag ‖ lowercase-hex(LE raw)`, i32 coordinates, u32 days/bells, ≤ 15 raw B), `create_with_seed`, the Season PDA, LoaderV3 ProgramData, citizen/keeper tags, JoinShard of a wallet, `host_id` and its inverse, `Addresses` for every account kind |
| `ix` | **one builder per tag**, account order and data as §5.7–§5.12, canonical addresses recomputed inside |
| `decode` | decoders for all 17 account kinds (incl. transit, entry, site mirror, arrival record, archive entry), absence rule, effective season status and `W(b)` |
| `log` | PS2 encode/decode, `head = sha256(prev ‖ le64(seq) ‖ body_without_tail)`, `Program data:` extraction, a multi-entity chain checker (gap / head / final) |
| `fees`, `budgets` | §10.1 formulas incl. `L(kind)` (`loaded_limit`), `min_tip_lamports`, priority ⇄ CU price, `defence_refund`, `deploy_max_len`; per-kind CU and loaded limits (placeholder = §5.5 budget and 1 MiB; loads a `{"budgets":[…]}` table), the I-50 retry ladder |
| `tx` | ComputeBudget encoders (limit, price, loaded-data limit, heap frame), legacy messages, signing, wire, shape (bytes/locks/writes/sigs), the reverse parse, priority and fee of a message |
| `http`, `rpc` | loopback HTTP/1.1 client; JSON-RPC client; `RpcPort: ChainPort` (feed via `frontier_feed` on localnet) |
| `ports` | `ChainPort`, `DrandPort` and their types (`Result`-returning, `Send` futures) |
| `beacon` | quicknet info, blstrs verification, SP-V2 hash-to-curve hints (ported unchanged), `seed_of`, RFC 9380 `expand_message_xmd`, **the test key** (I-53), `HttpDrand`, `FixtureDrand`, drand JSON codecs |
| `seal` | the 37-B plaintext (DESIGN §6.2), `validate` (I-28), salt/commit/root/keystream, seal with the stock `tlock =0.0.10`, the stock opener (FO panic caught), `judge` → seal codes 0–5 |
| `clock` | rule times over `clash::BeaconClock` (`T(b)`, `S`, ring/genesis rounds, windows, settle/archive times); `GameClock` (scale and slot-rate detection, extrapolation, monotone) |
| `payers` | derivation `sha256("PS-FRONTIER-PAYER-v1" ‖ master ‖ pool_id ‖ le32 i)`, pools `reveal` ≥ 150 / `delay` ≥ 32 / `relay` ≥ 150 (refused below unless `dev`), uniform draw over payers above the floor, effective N, ≥ 4 funders, payer care (top-up / sweep), `reveal_floor` |
| `vectors` | writes `permutation-gateway/test/frontier-vectors.json` (codec, addresses, seal, fees, shapes, beacon, clock) |

### `localnet` (MVP)

LiteSVM 0.16 with its mainnet feature set (incl. `enable_bls12_381_syscall` and SBPF v2), mainnet rent (5,080 lamports/B incl. 128 B), **real 400-ms slots at every scale with `0.4 × scale` game seconds per slot** (I-54; `frontier_setScale` applies at the next slot), a block builder ordering by §10.1 priority under the 100M block / 40M per-writable-account caps, a 150-slot blockhash queue, **SIMD-0186 loaded-data enforcement done by the node itself with the fee charged** (I-45), an ordered feed incl. failed transactions with post-state of written accounts, the §8.7 RPC subset, `frontier_feed/pause/resume/setScale/status`, `frontier_setAccount` behind `--allow-tamper`, LoaderV3 deploys with `max_len` padding and an upgrade authority, and `ChainPort::InProcess` on virtual time.

### `drand-replay` (MVP)

`/{chain}/info`, `/{chain}/public/latest`, `/{chain}/public/{round}` (and the chain-less paths) over the 32 SP-V2 fixture rounds (verified on load) or **`--test-key`**; gated by the game clock (fixed, wall-scaled, or the chain's Clock sysvar polled through a `ChainPort` into a `GameClock`); `425` before publication, `404` outside the archive.

## 2. Measurements [measured, 2026-09-27]

| Item | Result |
|---|---|
| `cargo test --locked --workspace` (1.95.0) | all green: fclient 39, localnet 1 + 6 (1 ignored, run separately below), drand-replay 1 + 3, findex 1, keeper 1, herald 1, verify 1, agents 1, stack 1, itest 1 |
| `cargo test --locked --release --workspace` | all green (same counts) |
| **SIMD-0186 control** (`loaded_data_control_one_page_below_fails_at_the_need_passes`) | SPL Memo's ELF deployed under LoaderV3 with `max_len` 602,112 (the size SP-V2's 480,512-B `.so` deploys at): need **602,471 B**; a limit of 589,824 B (one page below) fails `MaxLoadedAccountsDataSizeExceeded` **with the 5,050-lamport fee charged**; 622,592 B passes |
| LiteSVM alone (`litesvm_alone_undercounts_programdata`) | raw LiteSVM 0.16 **accepts** the one-page-below transaction: it counts the listed accounts but not the ProgramData, and a load failure charges no fee — so the node checks before execution, as §8.7 foresees |
| **BLS syscalls, real program** (`spv2_bls_program_verifies_a_fixture_round`, `PSF_SPV2_SO=…/SP-V2/program/out/plain-v2/spv2.so`, `--ignored`) | SP-V2 `Verify` of a fixture quicknet round with `fclient`'s hints lands on `localnet` at **327,438 CU** (SP-V2: 326,188–329,920) and returns exactly `fclient::beacon::seed_of`; loaded limit of that transaction **622,592 B** for a 480,512-B `.so` |
| Real slots (`rpc_subset_over_http_with_real_400ms_slots`) | slots advance at 400 ms of real time (±1.5 slots over 2 s); `unix_timestamp` advances exactly 8 s per slot at 20×; `GameClock` detects ≈ 20×; pause freezes the Clock; scale 2 applies at the next slot |
| Seal interop, JS → Rust | S-TLOCK q4 (tlock-js 0.9 compact-16) opens with the Rust `tlock` crate; commitment matches; the previous round's signature fails the FO check |
| Seal interop, Rust → JS | the `rust_to_js` vector (sealed by `tlock =0.0.10` to quicknet round 32,556,350) was opened by tlock-js 0.9 (`ibe.decryptOnG2`, the S-TLOCK `js/` install) with a scratch script: `k`, plaintext and commitment all match. The script is not committed; W2-D's `frontier-*.test.mjs` should read the same vector |
| Seal codes | valid → 0; wrong round → 1 (FO); garbage U → 2; flipped body → 4; invalid plaintext in a valid seal → 5 |
| Fixture beacons | all 32 verify with blstrs; hinted `H(round)` equals blstrs's for the sampled rounds |
| Fees | `tip_min` 14,441 (26k, 1 MiB) and 10,111 (16k, 1 MiB) as §10.1; 10,007 at 16k with 64 KiB; `F_r` = 214,942,572 lamports ≈ 0.215 SOL (§8.2) |
| Payer draw | 29,800 draws over 149 eligible payers: χ² below the p = 0.001 critical value; a payer under the floor is never drawn |
| `permutation-gateway` `npm ci --ignore-scripts && npm test` | 327 / 327 pass (the new JSON file is not a test) |
| `frontier-vectors.json` | 53,783 B; deterministic (a second build over the written file is byte-identical) |

Transaction shapes (legacy, with the three-instruction compute-budget prefix, fee payer + signers) [measured]:

| Shape | Bytes | §5.5 tx max | Locks |
|---|---|---|---|
| Join (wallet + relay payer) | 534 | 700 | 9 |
| FileTicket, 3 sites in 3 provinces (session + relay) | **575** | 560 | 11 |
| Harvest (session + relay) | **427** | 320 | 7 |
| Muster (session + relay) | **466** | 380 | 8 |
| Depart (session + relay) | 712 | 800 | 9 |
| Reveal, 3 path provinces (keeper) | 928 | 1,100 | 20 |
| SettleTransit (relay) | 762 | 1,100 | 12 |
| PostAnchor (keeper) | 780 | 800 | 8 |
| PostAnchorMulti, k = 1…8 regions | 781, 847, 913, 979, 1,045, 1,111, 1,177, **1,243** | 1,232 | — |
| FoldOccupancy part 1 (24 shards + 6 funds) | **1,288** | 1,200 | 36 |

## 3. Findings for the contract (not fixed here; outside W1-F's files)

1. **FoldOccupancy part 1 does not fit a packet** [measured]: 24 JoinShards + 6 ProvinceFunds + Frontier + Season + payer + programs = 1,288 B with the compute-budget prefix every transaction must carry (I-45), 56 B over 1,232. Without any compute-budget instruction it would be 1,228 B, but FoldOccupancy is class D (it needs `SetComputeUnitPrice` to escalate). Options: three parts of 16 shards, the 6 funds read in their own part, or a v0 transaction with a lookup table. Pinned by `every_tag_has_a_builder_that_fits_a_packet` (the only allowed overflow; any other is a regression).
2. **`MULTI_MAX_REGIONS` ≤ 7** [measured]: with one anchor and one region-day archive per region, 8 regions are 1,243 B. The contract says "8 or 16"; 7 is the most that fits. W1-E/W2-A should pin 7 (or drop the per-region archive keys when no anchor of the day can be archived yet).
3. **§5.5 "Tx B max" understates sponsored player shapes** [measured]: FileTicket 575 vs 560, Harvest 427 vs 320, Muster 466 vs 380 — the column seems to leave out the second signature (relay fee payer + session key) and the compute-budget prefix. Everything is still ≤ 1,232 B; the column (and `budgets.rs`'s tx byte ceiling) should be regenerated from measured shapes.
4. **drand-replay gating wording** (§8.7 "only when `round_time ≤ game_now + delay`"): read literally it serves rounds up to `delay` early. Implemented as `round_time + delay ≤ game_now` (the 0.8–2.0 s is quicknet's publication latency, applied after the round time), which never releases a round early. Please confirm and amend the text.
5. **PS2 tails need the per-kind lengths.** A tail is `n ‖ n × 41 B` at the end of the body; without the kind's key+payload length a payload byte can impersonate a tail (the parser refuses such a body as `Ambiguous` rather than guess; for random payloads the chance is ≈ 10⁻⁴ per record, so a season will hit it). `fclient::log::BodyLens` takes the table; W2-F must feed it `frontier-abi::log`'s lengths.

## 4. Deviations and their reasons

| # | Deviation | Why | Who resolves |
|---|---|---|---|
| D1 | `fclient::abi` is a transcription of §4–§6, not `pub use frontier_abi::*` | `frontier-abi` (W1-E) is built in parallel and is not on this branch; W1-E merges before W1-F | W2-F (owns `fclient` in wave 2) switches to the re-export; `abi_tables_match_the_contract` and the vector file must stay green |
| D2 | `addr`, `fees`, `seal` (`Plain`, `validate`, `salt_of`, `commit`, `seal_root`, `body_xor`) and `clock` implement the kernel formulas locally | the new `permutation-rules::frontier::{addr, fees, seal, beacon}` (W1-C) are built in parallel; `clock` already calls the existing `clash::BeaconClock` | W2-F points them at the kernel; the SP-V2 seed strings, the §10.1 figures and the seal vectors are the cross-check |
| D3 | `ChainPort::InProcess` lives in `localnet` (`localnet::InProcess`), not in `fclient::ports` | `fclient` would otherwise link LiteSVM and `localnet` depends on `fclient` (cycle) | none (recorded) |
| D4 | `ChainPort`/`DrandPort` methods return `Result` | the §8.1 sketch leaves errors implicit; keepers must tell "not landed" from "RPC down" | none |
| D5 | `fclient::http` is plain `http://` only | no TLS client is in the contract's expected dependency set; every M1 service is on loopback. A live drand (api.drand.sh) or public RPC needs HTTPS | dependency request R2 (Mode R / playtest) |
| D6 | `RpcPort::feed` over a public RPC returns `Unsupported` | the `getSignaturesForAddress` + `getTransaction` pager is `findex`'s RpcPoll (W2-F) | W2-F |
| D7 | Test-key definition pinned here: `sk = OS2IP(expand_message_xmd_sha256("PSF-TEST-BEACON-v1", DST "PSF-TEST-BEACON-KEYGEN-v1", 48)) mod r`; `pk` = `sk · G2` compressed; `chain_hash = sha256("PSF-TEST-BEACON-v1" ‖ pk96)`; `QUICKNET_PK_HASH = sha256(pk96)` for both quicknet and the test key; clock = quicknet's genesis and period | the contract names `hash_to_field("PSF-TEST-BEACON-v1")` without the DST, length or hash of the pk; W2-A's `test-beacon` feature must pin the same `pk` | W2-A reads `beacon.test_key` in `frontier-vectors.json` |
| D8 | Fixture rounds and tlock vectors live in `frontier-node/crates/fclient/fixtures/` | `frontier-node/fixtures/` (§3.1) is not in W1-F's owned paths | the integrator may move them |
| D9 | `localnet` MVP answers `-32601` for `frontier_snapshot/restore/hold` and WS subscriptions | W2-C's brief | W2-C |
| D10 | The archive for `drand-replay` is the 32 SP-V2 rounds; the prefetch tool and the ≈ 250k-round archive are not built | O-M1-12 not approved; W2-C's brief | W2-C, then O-M1-12 |

## 5. Gate W1 items that concern these files

Run from the worktree on 2026-09-27 [measured]:

| Gate item | Result |
|---|---|
| `(cd frontier-node && cargo test --locked --workspace)` | **pass** |
| `(cd frontier-node && cargo fmt --all -- --check)` | **PENDING-OWNER**: the installed 1.95.0 toolchain has no `rustfmt` (nor `clippy`); installing components is a toolchain download not covered by the 2026-09-27 approvals. Equivalent run: `cargo +1.89.0 fmt --all -- --check` **pass** (the code was formatted with rustfmt 1.89) |
| `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | **PENDING-OWNER** (same reason). Partial equivalent: `cargo +1.89.0 clippy --locked --ignore-rust-version -p fclient -p findex -p keeper -p herald -p verify -p agents -p bots -p stack --all-targets -- -D warnings` **pass**; `localnet`, `drand-replay` and `itest` link LiteSVM, which does not build on 1.89, so they are unlinted (they build on 1.95.0 with no rustc warning) |
| "`localnet`'s loaded-data control and `drand-replay --test-key` tests green" | **pass** |
| `(cd permutation-gateway && npm ci --ignore-scripts && npm test)` | **pass** (327/327) |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | untouched by this unit |
| `cargo test --locked --release --workspace` (a W2 gate item, run early) | **pass** |
| Binary smoke on 41660/41661 (unit 6's ad-hoc range) | `frontier-localnet --scale 20` served `frontier_status`; `drand-replay --test-key --clock chain:…41660` served `/info` (test chain hash `b420d636…`), `latest` 200, a future round 425; `frontier-stack check-ports` refused 4185 and 38810 and passed 41662; both stopped, ports free again |

## 6. Dependency requests (integrator, I-55)

- **R1 — new workspace files** (created here so the branch builds; please adopt or re-create): `frontier-node/Cargo.toml` (workspace, members, `[workspace.dependencies]`, `opt-level = 3` for dependencies in dev/test profiles — the pairing and LiteSVM are unusable unoptimised), `frontier-node/Cargo.lock` (seeded from the SP-V2 host lock, so LiteSVM and the Solana crates resolve to the versions M0 measured), `frontier-node/rust-toolchain.toml` (`channel = "1.95.0"`, `profile = "minimal"`, deliberately **no `components` line** — listing rustfmt/clippy would make rustup download them on first use), `frontier-node/.cargo/config.toml` (`target-dir = "target"`: without it the repository root's `.cargo/config.toml` sends the build to `permutation-chain/target`, and W5's `frontier-node/target/release/frontier-stack` would not exist). The root `Cargo.toml` needs no change (`frontier-node` has its own `[workspace]`); §3.1 lists it under `exclude` — harmless to add.
- **R2 — pinned crates** (all from the expected set or already in the SP-V2 lock): `litesvm =0.16.0`; `solana-account =4.3.2`, `solana-address =2.6.1` (features `curve25519`, `sha2`, `decode`), `solana-clock =3.1.1`, `solana-hash =4.5.0`, `solana-instruction =3.4.1`, `solana-keypair =3.1.2`, `solana-message =4.4.1`, `solana-signature =3.4.1`, `solana-signer =3.0.1`, `solana-transaction =4.1.6` (features `serde`, `wincode` for signing, `verify` for sigverify at submit), `solana-rent =4.3.0`, `agave-feature-set =4.2.2` (LiteSVM's own); `blstrs =0.7.1`, `group =0.13.0`, `ark-ff/ark-ec/ark-bls12-381 =0.5.0` (SP-V2 hints); `tlock =0.0.10`; `sha2 =0.10.9`, `hex =0.4.3`, `bincode =1.3.3`, `base64 =0.22.1`, `serde_json =1.0.151`, `rand =0.8.8`; `tokio =1.53.1` (rt-multi-thread, macros, net, time, sync, io-util, signal); `axum =0.8.9` (default features off; http1, json, tokio, query). `permutation-rules` by path with `std`.
- **R3 — toolchain components** (owner approval, same class as O-M1-12): `rustup component add rustfmt clippy --toolchain 1.95.0`, so the Gate W1 `frontier-node` fmt/clippy line can run as written.
- **R4 — later, for Mode R / the playtest only:** a TLS HTTP client (e.g. `reqwest` with rustls, pinned) for live drand and a public RPC.

## 7. For the next units

- **W2-F:** switch `fclient::abi` to `frontier_abi`, the kernel twins to `permutation_rules::frontier::*`, pass `frontier-abi::log` lengths as `BodyLens`; build the RpcPoll feed in `findex`. `payers::Pool::draw_os` and `keeper_core::bid_milli` are ready for the escalation engine.
- **W2-C:** `localnet::chain::Chain` has the hooks (`produce_block`, `set_scale`, `set_paused`, `loaded_size`, `deploy`); snapshot/WAL, `frontier_hold` and WS remain. `drand_replay::Source` takes a new archive format beside the fixture directory.
- **W2-A:** the test-beacon `pk` and `QUICKNET_PK_HASH` are in `frontier-vectors.json` → `beacon.test_key`; `MULTI_MAX_REGIONS` ≤ 7 (finding 2).
- **W2-D:** `frontier-vectors.json` carries codec offsets, addresses, the seal both ways, fees and signed shapes; its sections are documented in `crates/fclient/src/vectors.rs`.
