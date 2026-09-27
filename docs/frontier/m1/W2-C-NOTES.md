# W2-C localnet-complete: notes

- **Unit:** W2-C (wave 2), branch `frontier/m1-W2-C` cut from `frontier/m1-integ` at `ed31438`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.2: §8.7 (local chain node and drand replay), §10.1, §10.3, §11 (W2-C brief), §12 Gate W2, §13.4 (E5 environment), I-45, I-53, I-54.
- **Owned paths touched:** `frontier-node/crates/localnet/**`, `frontier-node/crates/drand-replay/**`, this file. Outside them: `frontier-node/Cargo.lock` (two dependency edges, see the requests below) and one row in `docs/frontier/DECISIONS.md` part A (the workflow asked every wave-2 unit to record the rustfmt/clippy install if it was missing; see deviation D10).
- **Date:** 2026-09-27. No push, no devnet or mainnet transaction, no download, no install. Services ran only on unit 9's ad-hoc ports 41690–41693 (§10.3: W2-C is the 9th unit) and were stopped; tests bind `127.0.0.1:0`.

## 1. What landed

### `frontier-localnet` (`crates/localnet`)

| Brief item | What it is | Where |
|---|---|---|
| Block builder with the 100M/40M caps and priority ordering | Kept from the W1-F MVP (priority order by §10.1, ties by arrival; requested CU counted against 100M per block and 40M per writable account; read-only accounts uncapped), now tested at the caps: 30 × 1.4M on one account → 28 land, 2 wait; 75 × 1.4M on distinct accounts → 71 land, 4 wait. A deferred transaction is dropped when its blockhash passes 150 slots, and a new submission with an expired blockhash is refused | `chain.rs` `produce_block`; tests `block_caps_40m_per_writable_account_and_100m_per_block`, `a_deferred_transaction_expires_with_its_blockhash` |
| Contention emulator (`frontier_hold`) | `frontier_hold(keys ≤ 64, priority_milli, slots)` models an attacker's filler stream: in each held slot, when the builder reaches priority `p` in its order, the filler takes every held key's remaining 40M account budget and the same CU of the block budget. A transaction writing a held key lands only if its priority **exceeds** `p` (a tie loses); readers are unaffected; the filler's CU and notional price (`Σ filled CU × p`) are reported. Also `frontier_release(id)` and `frontier_holds` | `chain.rs` (`hold`, `release`, `fill_hold`); tests `a_held_key_admits_only_higher_priorities_and_ties_lose`, `the_filler_takes_block_room_and_multi_key_holds_fill_every_key`, `frontier_hold_over_rpc_delays_a_low_bid_until_the_hold_ends` |
| Snapshot + WAL restore | `--data-dir`: a ledger WAL (`ledger.wal`: every block with its slot, game clock, scale and executed transactions with their full outcomes, in execution order; airdrops; deploys; tamper writes; framed with a truncated sha256) plus snapshots (`snap-<slot>.bin`: every account, slot, clock, scale, blockhash window, ledger offset, history length; sha256 trailer) every 36 game bells (`--snapshot-every-bells`), newest 3 kept. A block's record is written, flushed and `fsync`ed before its results are visible. Restore = newest valid snapshot + the ledger after it **re-executed with the recorded clock values; every re-executed transaction must reproduce its recorded outcome** (error, units, logs, return data, post-state, balances) or the restore fails. A torn last record is cut off. `frontier_snapshot [path]`, `frontier_restore path` (a rewind: state, history and ledger go back to the snapshot; later snapshots are deleted), `frontier_stateHash`. After a restart the chain resumes at its last recorded slot and game time | `persist.rs`, `chain.rs` (`open`, `snapshot`, `restore`, `state_hash`); tests `restart_from_snapshot_and_ledger_is_identical` (snapshot + ledger, ledger alone, torn tail), `a_ledger_that_replays_differently_is_refused`, `rewind_to_a_snapshot_truncates_history_and_ledger`, `in_memory_snapshots_need_a_path`, unit tests in `persist.rs` |
| Full RPC subset + conformance for the calls `send.mjs`/web3.js use | All of §8.7's list plus `isBlockhashValid` and `getGenesisHash`; errors in Agave's JSON form (`{"InstructionError":[3,{"Custom":1}]}` with the real instruction index, from the runtime's own serde form); `getTransaction` in `json` (web3.js's default, which the W1 MVP did not serve) and `base64`, with pre/post balances; `getSignaturesForAddress` with `before`/`until`/`limit`; `dataSlice`, `base58` encoding, `withContext`; `simulateTransaction` with post-state accounts and `returnData`. **WebSocket** on the RPC port + 1 (41011): `slotSubscribe`, `signatureSubscribe` (one-shot, immediate if already landed), `logsSubscribe` (`all` / `mentions`) and the unsubscribes, Agave's notification shapes, so web3.js's `confirmTransaction` and `onLogs` work. **Conformance:** `tests/conformance.rs` checks the shapes directly and then runs `tests/web3/conformance.mjs`, which drives **`@solana/web3.js` 1.99, `permutation-gateway/src/send.mjs` (`send`, `sendWire`, `simulateWire`, `confirm`) and `tickscan.mjs` (`scanRecords`)** against a live node: 13 checks | `server.rs`, `ws.rs`; tests `response_shapes_the_clients_rely_on`, `websocket_subscriptions_follow_agave_shapes`, `web3_js_and_send_mjs_against_a_live_node` |
| `frontier_setScale` at slot boundaries | Kept (applies at the next `produce_block`); now answers `{atSlot}`, is recorded per block in the ledger, and `frontier_status` shows `pendingScale` | `chain.rs`; tests `virtual_time_scales_the_clock_per_slot`, `rpc_subset_over_http_with_real_400ms_slots`, the restart tests (scale changes mid-ledger) |

CLI additions: `--ws-port`, `--data-dir`, `--snapshot-every-bells`, `--keep-snapshots`, `--no-fsync`, `--start-paused` (no block until `frontier_resume`, so a stack can check a recovered state or deploy before time moves). `--program` deploys are skipped for a program the recovered chain already has.

### `drand-replay` (`crates/drand-replay`)

| Brief item | What it is | Where |
|---|---|---|
| Archive format | A directory with `info.json` (the chain's drand info), `manifest.json` (`psf-drand-archive-v1`: segments with first round, count, sha256) and one file per contiguous run, `r<first>-<last>.bin` = `"PSFDRND1" ‖ chain_hash ‖ le64 first ‖ le64 count ‖ count × sig48`. The exit's ≈ 246,000 contiguous rounds are one ≈ 11.8 MB segment. The manifest is written last, so a half-written archive is never loaded. The SP-V2 `{round}.json` fixture directory still loads | `archive.rs`; `Source::open` / `Source::Packed` |
| Verification | Every round is verified with blstrs against the **pinned** key when fetched or packed. Load checks `info.json` against the pinned chain (quicknet compiled in, or the test key, or `--info`), each segment's sha256 and chain hash, and re-verifies the first, last and 16 sampled rounds per segment; `drand-replay verify --archive DIR --all` re-verifies every round on all cores | `archive.rs` (`load`, `verify_all`); tests `load_checks_the_chain_the_hashes_and_a_sample`, `prefetch_verifies_resumes_packs_and_serves` |
| Gating | Unchanged rule (v1.2 §8.7: `round_time + delay ≤ game_now`, `game_now` = the last observed chain Clock), now over packed archives too; a published round is served `Cache-Control: immutable`, `latest` `no-store` | `lib.rs`; tests above and the W1 `gates_on_the_game_clock`, `chain_clock_never_extrapolates`, `gates_on_a_live_localnet_clock` |
| Prefetch tool (the download waits for O-M1-12) | `drand-replay plan` (the season's contiguous range from G0: pre-season + 7 days + margin) and `drand-replay prefetch`: one round per request, ≤ `--rate` requests/s per endpoint over several endpoints, each round verified as it arrives, a lying endpoint's answers rejected and the round retried elsewhere, resumable partial file (re-verified on resume), packed on completion. **Public endpoints are refused before any request** unless `--approved O-M1-12`; `https://` needs `--fetcher curl` (no TLS crate in the workspace); public endpoints are capped at 20 requests/s | `prefetch.rs`, `main.rs`; tests `prefetch_verifies_resumes_packs_and_serves`, `the_curl_fetcher_and_the_public_endpoint_guard`, `public_endpoints_wait_for_the_owner`, `the_exit_plan_is_about_250k_rounds` |
| Test-key round service | Kept (`--test-key`), with a signature cache (the same rounds are asked for many times); `drand-replay pack --from-dir … --test-key` and `prefetch --test-key` work with it, so archive-path runs can be rehearsed without any download | `lib.rs` (`test_key_sig`) |

## 2. Tests run (all on this branch, toolchain 1.95.0)

| Command | Result |
|---|---|
| `(cd frontier-node && cargo fmt --all -- --check)` | pass |
| `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | pass (rustfmt/clippy installed for 1.95.0 with the owner's OK, as relayed) |
| `(cd frontier-node && PSF_REQUIRE_WEB3=1 cargo test --locked --workspace)` | pass: localnet 5 unit + 8 (1 ignored, the SP-V2 program probe needing `PSF_SPV2_SO`) + 3 conformance + 3 contention + 4 persist (1 ignored measurement); drand-replay 6 unit + 3 archive + 3 replay; the other crates unchanged and green |
| `(cd frontier-node && PSF_REQUIRE_WEB3=1 cargo test --locked --release --workspace)` (the Gate W2 line) | pass: 91 tests in 32 binaries, 0 failed |
| `cargo test --release -p localnet --test persist -- --ignored --nocapture measure` | the measurements below |
| Binary smoke on 41690–41693 (`frontier-localnet --data-dir`, `kill -9`, restart with `--start-paused`; `drand-replay --test-key --clock chain:`; `plan`; `prefetch` from it; `prefetch` to `https://api.drand.sh` (refused before any request); `verify --all`; serve the packed archive) | state hash before `kill -9` = after restart (`fd21671e…`, slot 9: snapshot at slot 5 + ledger); airdrop status survived; a future round 425; 200 rounds prefetched, packed, verified (PASS) and served; public fetch refused with exit 1; all four ports free afterwards |

The Node half of the conformance test needs `node` and `permutation-gateway/node_modules`. This worktree had none, so its `node_modules` was cloned from the integration worktree's (web3.js 1.99.0, the lockfile's version); no `npm install` ran. Without them the test prints `web3.js conformance NOT RUN` and passes, **unless `PSF_REQUIRE_WEB3=1`**, which I used for every run above. Gate W1 runs the frontier-node tests before `npm ci` in `permutation-gateway`; the integrator may want to move `npm ci` first and export `PSF_REQUIRE_WEB3=1` for the frontier-node lines (a request, §4).

## 3. Measurements [measured, this machine, release build]

| Quantity | Value |
|---|---|
| 20,000 memo transactions in 400 blocks (50 per block), ledger on, fsync off | 1.41 s wall (building, signing and executing) |
| Ledger size | 22.5 MB, ≈ 1,125 B per transaction (wire, logs, post-state, balances); an empty block ≈ 41 B, so the exit run's ≈ 75,600 slots of empty blocks add ≈ 3 MB |
| Snapshot of that chain (≈ 270 accounts, mostly LiteSVM's builtin programs and sysvars) | 1.40 MB in 0.015 s |
| Restart from snapshot + history reload (22.5 MB ledger) | 0.068 s |
| Restart from the ledger alone (re-executing all 20,000) | 0.71 s |
| `verify --all` of 200 test-key rounds | 0.031 s on all cores (≈ 40 s projected for 246k rounds) |
| Exit archive plan (G0 2026-08-01, 1.5 + 7 days + 1 h) | 246,001 rounds, 11.8 MB packed, 1.14 h over 3 endpoints at 20 requests/s |

## 4. Dependency requests (integrator, §3.4 / I-55)

| Crate | Change | Reason |
|---|---|---|
| `localnet` | `sha2.workspace = true`, `hex.workspace = true` | ledger/snapshot checksums, state hash, deterministic blockhashes and airdrop keys; hex for `frontier_stateHash` and the run id |
| `drand-replay` | `sha2.workspace = true`, `hex.workspace = true` | segment sha256 in the manifest; chain-hash hex |

Both are existing workspace dependencies already in `frontier-node/Cargo.lock` at `=0.10.9` / `=0.4.3`: the lock change on this branch is two dependency edges per package, no new crate and no version change. `cargo build --locked` passes with the branch's lock.

Gate request (not a dependency): run `npm ci --ignore-scripts` in `permutation-gateway` before the frontier-node test lines and export `PSF_REQUIRE_WEB3=1`, so the web3.js conformance can never be skipped silently in a gate.

## 5. Deviations and decisions

| # | What | Why | Owner |
|---|---|---|---|
| D1 | Airdrops mint directly and are served as a **synthetic, deterministically signed** system transfer from a fixed faucet key (`localnet::chain::faucet()`), with a unique blockhash per history entry. LiteSVM's own faucet (a random key per process) is removed | Determinism: a restart must rebuild the same state hash and signatures; web3.js `confirmTransaction` and `tickscan.scanRecords` need `getTransaction` to return every listed signature. Airdrops are never in `frontier_feed` | none (recorded) |
| D2 | Blockhashes are the node's own hash chain (`sha256(prev ‖ slot)`), not LiteSVM's; LiteSVM's `RecentBlockhashes` sysvar stays at genesis | Restore determinism (LiteSVM's blockhash is private state a snapshot cannot set without the `persistence-internal` feature); the node checks blockhash validity itself, LiteSVM's check was already off | none |
| D3 | The WebSocket side is a small hand-written RFC 6455 server (SHA-1 handshake, text/ping/close, masking, fragments) on the RPC port + 1, not axum's `ws` | The workspace's axum is built without `ws` and the contract's expected dependency set has no WebSocket crate; web3.js defaults to port + 1 | none |
| D4 | Holds (`frontier_hold`) are not persisted: after a restart the stack's adversary schedule re-applies them. The ledger records what landed, not why | Holds only change block composition, which the ledger records exactly | W5/W6 stack |
| D5 | After a restart (or a pause) the game clock resumes at the last recorded value; wall time while the node was down does not advance it | §8.7's Clock is `G0 + Σ_slots 0.4 × scale`; a restart is a pause | none |
| D6 | `frontier_restore` is a **rewind** (history and ledger truncated to the snapshot, later snapshots deleted); feed sequence numbers restart from the snapshot's history length, so a `findex` reading the feed must reset its cursor after a rewind | W6's "one re-run from snapshot allowed" | W2-F / W6 |
| D7 | `https://` prefetch uses a `curl` subprocess | No TLS crate in the workspace; the download is not approved anyway (O-M1-12). The curl path is tested on loopback | O-M1-12 |
| D8 | The node keeps the whole history in memory (as the W1 MVP did): at ≈ 1.1 KB per memo transaction, and more for Frontier transactions with Province post-states, a 7-day 1,000-bot run (order of 10⁶ transactions) needs several GB. Not a W2 gate item; W5 should measure it with real Frontier transactions and, if needed, keep `post` only in the ledger | recorded risk | W5 |
| D9 | The web3.js conformance test passes with a `NOT RUN` message when `node` or the gateway's `node_modules` is missing, unless `PSF_REQUIRE_WEB3=1` | Gate W1 runs the frontier-node tests before `npm ci`; see the gate request in §4 | integrator |
| D10 | One row added to `docs/frontier/DECISIONS.md` part A (rustfmt and clippy for 1.95.0 installed with the owner's OK) | The workflow asked each wave-2 unit to record it if missing; the file is outside W2-C's paths, so the integrator should keep one copy if several units add it | integrator |

## 6. Gate W2 items that concern these files

| Item | Status |
|---|---|
| `(cd frontier-node && cargo test --locked --release --workspace)` | **pass** (with `PSF_REQUIRE_WEB3=1`) |
| Gate W1 `frontier-node` fmt / clippy / test lines | **pass** |
| W1 pass condition "`localnet`'s loaded-data control and `drand-replay --test-key` tests green" | **pass** (unchanged tests still green after the rework) |
| Real quicknet rounds for `drand-replay` (the ≈ 250k archive) | **PENDING-OWNER (O-M1-12)**: the prefetch tool is built and tested on loopback only; no public endpoint was contacted. W6's real-round run and the exit need it |
| `keeper::one_day_beacons` over `localnet` with the test key | W2-F's; the node side it needs (`InProcess`, scaled Clock, test-key rounds) is unchanged in API |

## 7. For the next units

- **W2-F / keeper, findex:** `localnet::InProcess` and `Chain::{new, airdrop, deploy, submit, produce_block}` keep their signatures; `Config` gained `data_dir`, `snapshot_every_secs`, `keep_snapshots`, `fsync` (use `..Config::default()`). New: `Chain::{hold, release, holds, snapshot, restore, state_hash, subscribe, open}`. Feed records now exclude airdrops by a flag rather than by an empty wire.
- **W5 stack:** start `frontier-localnet --port 41010 --data-dir <run>/chain [--start-paused]` (WS on 41011), restart it with the same `--data-dir` after `kill -9`; `drand-replay --archive <dir>` loads a packed archive pinned to quicknet (or `--test-key`); `drand-replay plan --g0 … --days 7 --pre-days 1.5` sizes the download; `prefetch` needs `--approved O-M1-12` for any public endpoint.
- **W6:** a rewind (`frontier_restore`) truncates history; re-point `findex` after it (D6).

## Post-merge addendum (integrator, integ-W2 window, 2026-09-28)

After the wave-2 review: the block and account caps count the §10.1 cost, not the CU limit (the module doc's "Agave's cost tracker counts the requested limit" was wrong; contract v1.3 §8.7), and a transaction not executable at block time gives its cost back; `is_loopback` refuses userinfo and other crafted authorities; `dataSlice` saturates. The other review minors (snapshot/ledger binding, airdrop pre-balances in rebuilt history, snapshot path restriction, `--program` mismatch on a recovered chain, WS `Lagged`, verify-on-serve for the archive) are deferred to W5 hardening (DECISIONS I12). Contention and capacity numbers measured before this change were optimistic by ≈ 11% per account for a 26k-CU Reveal (review estimate).
