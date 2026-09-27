# W2-F keeper-core — notes

- **Unit:** W2-F (wave 2), branch `frontier/m1-W2-F` cut from `frontier/m1-integ` at `ed31438`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.2: §5.1, §5.5, §5.8, §5.9, §8.1, §8.2, §10.1–§10.3, §11 (W2-F brief), §12 Gate W2; I-21, I-23, I-45, I-49, I-50, I-53, I-54; O-M1-09.
- **Owned paths touched:** `frontier-node/crates/{findex,keeper,fclient}/**`, this file. Outside them, by instruction of the task: one row in `docs/frontier/DECISIONS.md` (part H, H1: the rustfmt/clippy install record). Integrator-owned files changed on this branch so it builds (dependency requests, §6): `frontier-node/Cargo.toml` (workspace dependencies), `frontier-node/Cargo.lock`, the dependency sections of `crates/{findex,keeper}/Cargo.toml`.
- **Tags:** [measured] = run on this machine on 2026-09-27; [design] = a rule implemented as the contract states it.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no rustup target, Playwright, drand archive or Agave download. One crates.io download: `rusqlite =0.40.2` (bundled) and its dependencies, which are in the contract's expected set (§3.3). Services started: only the binary smoke on this unit's ad-hoc ports 41720–41722 (§10.3: unit 12 → 41720–41729); all were stopped, and the ports were checked free afterwards. Tests bind `127.0.0.1:0`.

## 1. What landed

### `findex` (ingest, archive, index)

| Module | Content |
|---|---|
| `ingest` | `Source` trait. `LocalnetFeed<P: ChainPort>` reads the ordered feed with the post-state (the node's `frontier_feed` or the in-process vector). `RpcPoll` pages `getSignaturesForAddress(program, before, until = last)` and then calls `getTransaction` oldest first; it resumes at a transaction the node cannot return yet. If a server ignores `before` (as the local node's MVP does), it serves the same page twice; RpcPoll refuses that with `Unsupported` instead of archiving a gap. |
| `archive` | Append-only segment files (64 MiB by default). Each frame is `len ‖ sha256[0..8] ‖ JSON TxRecord`. `manifest.json` is replaced atomically (temp + rename + fsync) and lists each segment's first/last sequence, record count and bytes, the sha256 of every sealed segment, and the source cursor. Frames are synced before the manifest names them, so a torn tail is truncated on open. A segment shorter than its manifest entry refuses to open. `verify()` checks the sealed-segment hashes, the frame hashes and that the sequence numbers are contiguous. |
| `index` | SQLite (WAL): `tx` (failed transactions included), `record` (PS2 records of **successful** transactions only), `link` (tail links), `entity` (latest `(seq, head)` per chained entity), `snapshot`/`account` (post-state of program-owned or closed accounts). A link's address comes from the post-state (the written account whose header carries exactly the link's `seq` and `head`); without post-state it falls back to `frontier_abi::log::chains_of` and `AddrCtx`. Records are decoded with `frontier-abi`'s per-kind lengths (W1-F note 5). |
| `Findex` | Every pull is archived first (together with the source cursor) and then indexed. On open, the index catches up from the archive, or is rebuilt from it; the source resumes from the archived cursor. |

`fclient::rpc` gained `get_signatures_for_address`, `get_transaction` and `parse_transaction` (the RpcPoll primitives, W1-F D6).

### `keeper` (lib `keeper_core`, bin `frontier-keeper`)

| Module | Content |
|---|---|
| `config` | `keeper.toml` (§8.2 keys plus the tuning keys): a flat-TOML parser with no new dependency. Unknown keys and unknown roles are refused. `api` must be a loopback address. |
| `pools` | Reveal pool (≥ 150), delay pool (≥ 32), funders (≥ 4), all derived from one master seed (`fclient::payers`). Each version draws its payer uniformly at random with OsRng among the payers above the floor. W draws from the reveal pool; D and N draw from the delay pool. Reveal floor `F_r` from R99 (0.215 SOL at 4,000/150), ceiling 2 F_r; delay floor 0.5 SOL. Balances are cached and debited optimistically. Payer care is class N at rest: funders top payers up, and payers above the ceiling are swept back to funders. |
| `engine` | One new version per slot while a write has not landed. Each version gets a new signature and a newly drawn payer. W bids start at `p_start` and double to `min(P_def, season defence cap)`; D bids start at 0.1 and double to P_delay 0.5; N stays at 0.1. The CU limit and `L(kind)` come from the budgets table. **Retry ladder (I-50):** `ComputationalBudgetExceeded` → `min(2×, 1.4M)`, then 1.4M; `ProgramFailedToComplete`/heap fault → `RequestHeapFrame(262,144)`; each rung is journalled and alerted. **Contested:** a write not landed 2 slots after a bid ≥ p_tip marks its bell contested. **Keeper error mapping (§5.4):** 52 counts as success; 12/16/53 and 99 end the write; any other code backs off 2, 4, 8, … up to 1,024 slots. `per_write_cap` bounds the versions' fees (§8.2 duplicates bound). `poll` / `send` are split, so a landing is recorded as an outcome before the duties re-read the chain. |
| `journal` | SQLite, WAL, `synchronous=FULL`: `attempts` (§8.2 columns plus heap and code), `plaintexts`, `cursor`, `claims`, `payers`, `alerts`. The `<journal>.lock` process lock uses `File::try_lock`. |
| `rounds` | Verifies each round off chain (blstrs, key pinned against `Season.quicknet_pk_hash`) before any transaction carries it. Hints are computed once per round; a round not yet published is asked for at most once per slot. |
| `beacon` | **Anchors:** once T(b) is published, a `PostAnchorMulti` goes out per fixed chunk of `MULTI_MAX_REGIONS` = 7 regions, plus a per-region `PostAnchor` fallback. The fallback lags 1 slot in peacetime and 0 slots for a contested bell; it is cancelled as soon as the anchor is seen. **Seed caches:** once S(b, A) is published, `PostSeed` goes out at a random nonce. A write not landed within 2 slots is dropped for the next random nonce, and each nonce has its own write key (`seed:b:r:n`), so the journal names every cache address tried. **Restart discovery:** the journal's nonces are read first; for an anchor this process did not see land, all 256 nonces are probed, bounded to 2,048 reads per tick, before a cache is created. **Beacon logs:** one `PostBeacon` per region per bell when the region's log is behind the latest round (class N). |
| `genesis` | `ConsumeGenesisSeed` once the genesis round is published (role `beacon`). Then (role `rings`) `OpenRing(d)` for `d = rings_opened ≤ g`, one at a time, and `OpenProvince` for every province of rings 0..=g. A `NotImplemented` (99) answer (the W2 program; land is W3-A's) ends the ring part with an alert, and the beacon duties keep running. |
| `archive` | `plan_archive` groups every anchor whose A + `archive_after` has passed and whose cache is known, by region-day, ≤ 8 per transaction. Each item carries the anchor's `rent_to`, so the refund goes to the payer that created the anchor (I-49). The duty remembers exactly which bells each batch carries. After a batch lands, the caches of its bells are closed with `CloseSeedCache` (N) to their `rent_to`. |
| `api` | Loopback-only axum server; every route needs `Authorization: Bearer` (compared in constant time). `/v1/status` and `/metrics` are live. `/v1/reveal` checks the shape (400) and the plaintext rules (`seal::validate`, 422 `BadPlaintext`), then queues the request with a track id (202). The on-chain commitment (409) and window (410) checks are W3-C's accept path. `/v1/nudge` queues. `/v1/track/{id}`. |
| `Keeper` | `tick()` runs once per slot. It reads the Clock (every rule time is `unix_timestamp`), refreshes the Season, runs payer care and records the effective N per bell (alert below 150, W writes never stopped), polls, plans genesis/beacon/archive, sends, then routes the outcomes and alerts. `start()` reconciles in-flight attempts with `getSignatureStatuses` and adopts the undecided ones. `p_start = p_tip` by default (O-M1-09); `--peace-start f` and `peace_start` set `f × p_tip`. Budgets default to the canonical `frontier-abi/vectors/budgets.json` (embedded at build time); `budgets_file` overrides it. |
| `main` | `frontier-keeper --config keeper.toml [--dev] [--peace-start f] [--init-seed]`: the master seed file (0600), the journal lock, drand `/info` (retried for 10 s), `RpcPort` + `HttpDrand`, the API, then a tick every 100 ms. |

### Tests

| Test | What it checks |
|---|---|
| `keeper::one_day_beacons` (Gate W2) | One game day over `localnet` in process at 20× (real slot counts, virtual time), with the test-beacon key and the §5.7 season lifecycle: AnnounceSeason, the 24-h lead run at scale 2,000, CreateSeason, InitBeaconLogs. Checks genesis seed and rings 0–3 (37 provinces). For all 144 bells × 16 regions: THE anchor (round T(b), anchored after the round, `rent_to` = the payer of a landed version, a delay-pool key) and one seed cache with the seed of S(b, A). Also: beacon logs; latency p99 ≤ 2 slots; χ² of the delay-pool fee payers; funders filling the reveal pool to effective N 150; a crash with versions in flight followed by a restart (lock released and retaken, in-flight adopted, **exactly one cache per bell-region across the restart**); journal with nothing in flight at the end; findex archiving and indexing the day. |
| `keeper::held_accounts` | An adversary fills region 5's anchor address to 39.9M CU per block at priority 1.0 (above P_delay). The held chunk's other regions land through their fallbacks exactly 1 slot after the unheld chunk; the held region lands when the hold ends; the bell is marked contested with an alert. A held seed-cache nonce is switched away from; the stale version expires unlanded (no second cache). |
| `keeper::archive_returns_rent` | Three game days. Day 0 of all 16 regions is archived (tombstone and archived bits, `{a_off, seed, sig}` equal to the rule values), every anchor and cache is closed, and a walk over **every** transaction shows each ArchiveAnchors crediting each anchor's `rent_to` exactly `rent(144)`. |
| `keeper` unit tests (12) | bid schedule; a W write escalating slot by slot (new payer and signature each version, p_tip → P_def, contested once, spend cap); CU and heap ladder; error mapping and backoff; pool minimums and χ² over the delay pool; journal reopen and exclusive lock; archive batching; config; API auth and shapes; classes of the W2-F tags. |
| `findex` (4 unit + 3 integration) | Archive roll, seal and verify; torn tail; tamper and short segment; index links, heads and snapshots (post-state and log-derived addresses, failed transactions not events, closed accounts, rebuild); feed vs RpcPoll giving the same transactions in the same order, then restart and resume; RpcPoll paging against a server that honours `before`/`until`; refusal against one that ignores `before`. |

### The program in these tests

`permutation-frontier` (W2-A) is built in parallel and is not on this branch. The in-process tests therefore register a **native model** of the beacon-side instructions as a LiteSVM builtin (`crates/keeper/tests/model/mod.rs`). The model uses the contract's account lists, checks, frontier-abi offsets, address grammar, rent from the fee payer (`rent_to`), test-key BLS verification and approximate CU. It models 0x08, 0x01, 0x09, 0x03, 0x10–0x15, 0x20 (d ≤ g) and 0x22; everything else answers 99. It leaves out the upgrade-authority check, the instructions-sysvar evidence, chained logs, terrain and camps. **It is not the program.**

The harness uses the real test-beacon `.so` when `PSF_FRONTIER_SO=<path>` is set: it deploys the `.so` under LoaderV3 with `deploy_max_len` and the authority, and the tests print which program ran. Against W2's program the genesis-ring part is reported as NotImplemented, not failed. The archive test likewise reports NotImplemented until W4-B.

## 2. Measurements [measured, 2026-09-27, release, this machine, native model]

| Item | Result |
|---|---|
| `one_day_beacons` | 10,952 slots (24.3 game hours) in 6.9–7.1 s of wall time for the tick loop; ≈ 30 s for the whole test, including the season set-up and the checks |
| Anchor latency, T(b) available → anchor landed | p50 1, p99 1, max 1 slot (1,168 anchors after the restart). E5 criterion 3 asks p99 ≤ 2 |
| Seed latency, S available → cache landed | p50 1, p99 1, max 1 slot |
| A − bell_end(b) | 15 s for every anchor (round ≤ 3 s after the bell ends, 1 s publication delay, 8-s slots) |
| Fee payers (whole run) | 5,149 D/N versions over 32 delay payers; χ² between 25.1 and 42.7 over 5 runs (31 dof, p = 0.001 critical value 61.10); no D/N write paid by the reveal pool |
| Restart | 19 versions in flight at the crash, 19 adopted, 0 duplicated caches |
| Keeper fees for one day, post-restart half (73 bells) | anchor-multi 9.24M, seed 43.4M, beacon-log 42.2M lamports (D/N at 0.1 in peacetime) |
| Held anchor | unheld chunk at slot 403; the held chunk's 6 other regions via fallbacks at 404; held region at 412 (hold 12 slots from 3 before the round) |
| Held cache | first nonce 233 held; cache landed at nonce 66; the stale version expired unlanded |
| `archive_returns_rent` | 3 game days in 22.5 s; 2,336 anchors archived, 2,336 caches closed, 2,336 refunds checked, 3,227,791,360 lamports of anchor rent back to the payers that paid |
| findex over the day | 5,120 transactions, ANCHOR 2,320, SEED 2,304, BEACON 2,336 records, 2 segments (8-MiB segments), archive verified |
| Binary smoke (41720–41722) | `frontier-localnet` + `drand-replay --test-key` + `frontier-keeper`: `/v1/status` 401 without the token, 200 with it (slot 11, no season, pools 150/32/4); `/metrics` served; a second keeper on the same journal exits with "held by another keeper process"; everything stopped and the ports were free again |

## 3. Gate W2 items that concern these files

| Item | Result |
|---|---|
| `(cd frontier-node && cargo fmt --all -- --check)` | pass |
| `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | pass (toolchain 1.95.0; rustfmt and clippy installed with the owner's OK, DECISIONS H1) |
| `(cd frontier-node && cargo test --locked --workspace)` | pass |
| `(cd frontier-node && cargo test --locked --release --workspace)` | pass |
| "`keeper::one_day_beacons` green" | **pass against the native model. Against the W2-A test-beacon `.so`: not run** — the program is not on this branch. The integrator runs `PSF_FRONTIER_SO=<test-beacon .so> cargo test --release -p keeper --test one_day_beacons` after merging W2-A. |
| "payer χ² test green" | pass (`pools::delay_pool_draws_are_uniform_and_skip_the_broke`, `fclient::payers::draws_are_uniform_over_eligible_payers`, and the χ² over the one-day run) |
| Other Gate W2 lines (root workspace, program, svm-tests, gateway, wasm) | not in this unit's files; not run here. `permutation-gateway/test/frontier-vectors.json` is unchanged (the fclient changes do not touch the vectors). |

## 4. Deviations and choices

| # | What | Why |
|---|---|---|
| D1 | `one_day_beacons` runs against a native model of the program unless `PSF_FRONTIER_SO` is set | W2-A is built in parallel; a test that needs the `.so` could not run here, and a silent skip would hide that. The model follows the contract; running it against the `.so` is the integrator's gate step. |
| D2 | D-class writes start at 0.1 (the low bid) and double to 0.5; W writes start at p_tip | §8.2 fixes W's start (O-M1-09) and D's cap, not D's start. 0.1 is the offchain design's peacetime fallback bid; it reaches ≥ p_tip on the 4th slot, so contention is still detected. |
| D3 | One write key per seed-cache nonce (`seed:b:r:n`) | So the journal names each cache address tried, and a restart neither duplicates caches nor probes 256 nonces for its own. |
| D4 | `fclient::abi` still transcribes the tables (twin-tested against `frontier-abi`); it is not re-exported | integ-W1 closed W1-F's D1 with twin tests. A re-export would change names that `localnet` and `drand-replay` (W2-C, in parallel) use. |
| D5 | RpcPoll refuses a server that ignores `before` | The local node's MVP answers `getSignaturesForAddress` without `before`/`until`. Paging against it would loop or leave gaps. W2-C's RPC conformance should honour both parameters. |
| D6 | `/v1/reveal` stops at "queued" | The brief asks for an API skeleton; the accept path is W3-C's (§11) and the pipeline W4-C's. |
| D7 | A failed write backs off exponentially (2 → 1,024 slots) | A persistent refusal should not cost a transaction every other slot. The 10-minute run that exposed this (a batch bookkeeping bug, fixed) is the evidence. |
| D8 | PostBeacon for a round not above `latest_round`: the model answers `WrongRound` (7) | §5.8 does not name the code; W2-A's choice wins. The keeper only posts when the log is behind. |

## 5. Findings for other units

1. **W2-A:** the keeper and harness assume AnnounceSeason → CreateSeason (11 accounts, `SeasonParams ‖ PayoutParams::REV3` borsh) → InitBeaconLogs; test-beacon `quicknet_pk_hash = sha256(test pk96)` (W1-F D7); PostAnchor/PostSeed on a present account succeed as a no-op; PostAnchorMulti skips present anchors. With the real `.so`, `one_day_beacons` should pass unchanged except for the ring part, which is reported.
2. **W2-C:** `getSignaturesForAddress` should honour `before` and `until` (RpcPoll paging). `frontier_feed` returns at most 10,000 records per call; callers must page, and findex's `LocalnetFeed` does.
3. **W3-C / W4-C:** the engine's `WriteSpec` (key, class, tag, builder of `BuildCtx`, deadline slot for W writes, not-before slot, fixed payer) and the `poll → plan → send` tick are the extension points. `Shared.reveals` holds the queued `/v1/reveal` material.

## 6. Dependency requests (integrator, I-55)

- **R1 — `rusqlite = { version = "=0.40.2", features = ["bundled"] }`** in `frontier-node` `[workspace.dependencies]`: the keeper journal and the findex index (§3.3 expected set). Lock additions: rusqlite 0.40.2, libsqlite3-sys 0.38.2, hashlink 0.12.2, hashbrown 0.16.1, foldhash 0.2.0, fallible-iterator 0.3.0, fallible-streaming-iterator 0.1.9, pkg-config 0.3.34, vcpkg 0.2.15, and the wasm-only rsqlite-vfs 0.1.1 and sqlite-wasm-rs 0.5.5 (not built on this host).
- **R2 — `solana-program-runtime = "=4.2.2"`** in `[workspace.dependencies]`, used only as a dev-dependency of `keeper` (the native model builtin). It is already in the lock through LiteSVM; nothing is downloaded.
- **R3 — `crates/findex/Cargo.toml`:** dependencies `fclient`, `frontier-abi`, `solana-address`, `rusqlite`, `sha2`, `hex`, `base64`, `serde_json`, `tokio`; dev-dependencies `localnet`, `axum`, `solana-keypair`, `solana-signer` (all workspace).
- **R4 — `crates/keeper/Cargo.toml`:** dependencies `fclient`, `frontier-abi`, `permutation-rules`, `solana-{address,hash,instruction,keypair,signer,signature}`, `rusqlite`, `axum`, `tokio`, `rand`, `sha2`, `hex`, `base64`, `serde_json`; dev-dependencies `localnet`, `findex = { path = "../findex" }`, `solana-program-runtime`.

## Post-merge addendum (integrator, integ-W2 window, 2026-09-28)

After the wave-2 review (contract v1.3 §8.2): a write at its version or spend cap whose versions all expired ends (Failed, backoff, `write-expired` alert) and is re-planned with a fresh escalation; the anchor scan window always reaches the newest bell (`anchor-missing` alert); findex deletes unnamed segment files at open and creates new segments empty. Tests: `capped_write_whose_versions_expired_ends_and_restarts`, `anchor_held_past_the_version_cap_lands_after_the_hold`, `a_crash_across_a_segment_roll_reopens_clean`. The archive duty and the native model use half-day archive parts (v1.3). Deferred (DECISIONS I12): every tried seed nonce tracked and closed, write-ahead attempts, Dead backoff, the D-class start bid vs contested detection, idempotent payer care, daily budget, orphan fees, payers table, the startup archive scan, the CU ladder's third rung.
