# W3-D herald — notes

- **Unit:** W3-D herald (wave 3), branch `frontier/m1-W3-D` cut from `frontier/m1-integ` at `241c52d`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.3 — §8.4 (herald), §9.2 (envelope), §9.3 (overview binary), §6 (PS2 records), §5.3 (layouts), §8.3 (the relay behind `/gw/*`), §10.3 (ports), §11 (brief and ownership), §12 (Gate W3), §3.5 (tests). Area design `design/offchain.md` §8 and `design/web.md` §4, §12.
- **Owned paths touched:** `frontier-node/crates/herald/**`, `frontier-node/crates/findex/**`, this file. Integrator-owned files changed on this branch so it builds (dependency requests, §6 below): `frontier-node/Cargo.toml` (workspace dependencies), `frontier-node/Cargo.lock`, the dependency sections of `crates/herald/Cargo.toml`. Nothing else in the repo was changed; `docs/frontier/DECISIONS.md` already records the rustfmt/clippy install (part A), so it was not touched.

## 1. What landed

### `herald` (lib `herald_fold`, bins `frontier-herald`, `frontier-viewers`)

| module | what |
|---|---|
| `fold` | archived transactions in archive order → captures `(slot, archive seq, bytes)` of every program account of the season, per-bell files, WS diffs. Failed transactions change nothing. Deterministic: ordered maps, no clock, no randomness |
| `clash` | clash reports recomputed natively with `clash::resolve_clash` through a `ClashBuilder` trait; `Provisional` builds the §5.11 ResolveFromInputs input over the `frontier-abi` entry codec (see D1) |
| `overview` | the §9.3 binary (24-B records, the JS decoder's bit order) |
| `records` | PS2 bodies decoded field by field from the `frontier-abi::log` kind table; account keys (`pv:`, `ar:`, `ad:`, `ci:`, `ho:`, `an:`, `sd:`, `aa:`, …); record → province routing |
| `views` | the live answers: `/h/season`, `/h/province/{P},{Q}/latest`, `/h/me/{wallet}` |
| `files` | atomic writes (temp + rename) with deterministic `.gz` siblings (mtime 0, level 6); batched durability (D4) |
| `checkpoint` | the fold's whole state, hashed, saved atomically; a damaged or foreign checkpoint is refused |
| `runner` | findex (archive, then index) → fold → files and diffs → checkpoint; restart = load checkpoint + re-fold the archive after it (streamed per segment) |
| `server` | axum origin: `/h/*`, `WS /h/ws`, `/frontier/*`, `/gw/*`, security headers, the two CSPs, caching, gzip |
| `ws` | RFC 6455 over hyper's upgrade (own frames, SHA-1 for the accept key), subscriptions, per-socket sequence, lag → gap → drop, heartbeat; a small client (viewers, tests) |
| `viewers` | the load generator: HTTP/1.1 keep-alive polling viewers with the web client's request mix and jitter, WS viewers, a log-bucket latency histogram |
| `fixture` | a synthetic mini-season archive built byte for byte from the layouts, with real event chains (tests; see D7) |

**Files** (under `<data>/files/h/…`, served at the same path):

| file | written when | cache |
|---|---|---|
| `province/{P},{Q}/{b}.json` | the Province's `resolved_next` passes `b` (ResolveFromInputs, or each bell of a SkipQuiet run): §9.2 envelope with the Province bytes after that transaction, `seq`/`head` from its header, and that bell's ArrivalSlots, ArrivalDay and ClashInputs as captured then (before they close) | immutable |
| `clash/{P},{Q}/{b}.json` | a CLASH record: `{v, key, bell, province, slot, inputs_b64, seed, anchor {key, A, round}, cache {key, nonce} \| archive {key}, outcomeDigest, inputDigest, decoded {fighters, engagements, fates[24]}, heraldCheck, builder, recomputedDigest, checkError}` | immutable |
| `overview/{d}/{b}.bin` | every province of ring `d` opened by bell `b` has resolved `b`; each record from the province's bytes **at its own resolve of `b`** | immutable |
| `bell/{b}/region/{r}.json` | THE anchor (or its archive entry) and a seed of `S(b, A)` exist; rewritten while `resolved`, `present`, tombstone and archive state change: `{v, bell, region, anchor {key, address, bytes_b64, slot, A, round, present}, archive {key, address, aOff, seed}, S, caches [{key, address, nonce, round, seed, A, present}], tombstoned, archived, resolved [[P,Q]…], final}` | immutable once `final` (archived and resolved by every province of the region), else `max-age=5` |

**Live answers:** `/h/season` (`max-age=30`, ETag, 304) with every field of §8.4 plus `seasonAddress`, `windowNext`, `windowFromBell`, `revealLoadedLimit`, `slot`; `latest` province (`max-age=2`) = the current bytes with `bell = resolved_next` and that bell's slots/day/inputs as they stand; `overview/{d}/latest.bin` (`max-age=5`) from the current bytes, header bell = the last bell all of the ring resolved; `/h/me/{wallet}` (`no-store`): `citizen`, `holdings[]` (address, slot, seq, head, `bytes_b64`), `hosts` (every entry of the citizen's holdings in any Province), `transits`, open `slots` (by citizen tag), `seals` (TRANSIT_SETTLED outcome and seal code per host), `quota` (the relay's `GET /f/quota?citizen=<Citizen address>` within 2 s, else null); `/h/events?after=` pages of 500 `{seq, slot, sig, kind, bell, tx, body_b64, decoded}` with `next` and `full` (a full page is immutable); `/h/status` (fold position, alarms, WS counters). The shapes are the ones W2-E's fixtures (`permutation-gateway/test/fixtures/frontier/*.json`) read, as supersets.

**WS `/h/ws`:** `{"op":"sub","provinces":[[P,Q]…≤64],"rings":[…],"wallet":"b58"?,"bells":bool}` (replaces the previous one; acknowledged `{"op":"subscribed",…}`, refused `{"op":"error","code"}`); messages `{seq, kind:"acct|bell|event", key, slot, head, bytes_b64}` numbered per socket from 1. `acct` = an account's new bytes (empty = closed; `head` for chained accounts), routed to its province, or to the wallet for a Citizen or Holding, or to `bells` for anchors, caches, archives and beacon logs; `bell` = a per-bell file was written (`key` = its `/h/…` path, no bytes: the client fetches it); `event` = a PS2 record (`key` = `NAME:fields`, bytes = the body) routed by its province. A socket that lags the 4,096-deep broadcast skips one `seq` (the client resyncs from the files); past 3 lags it is closed 1013. Ping every 15 s; closed after 3 silent heartbeats. ≤ 8,192 sockets.

**`/frontier/*`** from `--web DIR`: `index.html` for `/frontier/`, `/` → 302; names with a hex content hash of ≥ 8 digits are `immutable`, every other file `no-cache` + ETag/304; `.gz` sibling when present; path allowlist (no `..`, no dot-files, no `%`).

**`/gw/*`** → `--relay host:port` (the relay's public listener, 41033): as `permutation-server/src/play/proxy.rs` — GET/POST/OPTIONS, safe path and query, body ≤ 64 KiB (413), only `Content-Type` forwarded, `X-Forwarded-For` = the peer (or, from a loopback peer, the last entry of its `X-Forwarded-For`), 90-s answer limit, only status, content type and body returned (`no-store`).

**Headers on every answer:** `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy` = web design §12 for `/frontier/*` (`default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; frame-ancestors 'none'`), `default-src 'none'; frame-ancestors 'none'` elsewhere; `/h/*` adds `Access-Control-Allow-Origin: *` (public data; the page's loopback `?herald=` override reads it cross-origin).

**`frontier-herald`:** `--data DIR --program B58 --season ID --rpc URL [--listen 127.0.0.1:41040] [--source localnet|rpc] [--cluster localnet] [--web DIR] [--relay 127.0.0.1:41033] [--test-key | --drand-info FILE] [--checkpoint-slots 150] [--poll-ms 200] [--quotas JSON]`. Refuses a listen port outside 41000–41999 or on the reserved list (0 allowed for tests). `--source localnet` reads `frontier_feed` (exact post-state); `--source rpc` polls `getSignaturesForAddress`/`getTransaction` and fetches the post-state (below). Ctrl-C checkpoints. `--test-key` serves the test beacon's chain info in `/h/season.drand` (I-53).

**`frontier-viewers`:** `--herald host:port [--viewers 4000] [--ws 1000] [--seconds 600] [--think-ms 5000] [--rings] [--provinces] [--bells] [--seed] [--stats 127.0.0.1:41075]`; prints `{requests, errors, notFound, p50_ms, p99_ms, max_ms, rps, ws {connected, messages, gaps, errors}}`; `--stats` serves the running counters at `GET /stats` (port checked like the herald's).

### `findex` (owned from wave 3)

- `index`: an `event` table numbers the PS2 records of landed transactions 1, 2, … in archive order (the `/h/events` numbering, equal to the fold's `events`, asserted); `Index::{events_after, last_event}`; `index::Reader` opens a second, read-only connection (WAL) for the HTTP handlers. Schema 2: an index written before it is emptied at open and rebuilt from the archive.
- `archive`: `for_each_after` streams records one segment at a time; `read_after` uses it; `Findex::open` catches the index up in batches of 2,000 (a season's archive does not fit in memory as records).
- `ingest`: `Enriched<S>` fills the post-state of a source without it (a public RPC) before the batch is archived: the message's writable accounts (fee payer and program ids excluded) read with `getMultipleAccounts(minContextSlot = slot)`, so every later fold of the archive sees the same bytes.

## 2. Tests (§3.5)

| test | what |
|---|---|
| `herald` unit (9) | gzip determinism and atomic writes, batched-durability bookkeeping, path allowlist; checkpoint round trip and refusal of another season/program, a flipped byte, truncation; PS2 decoding and routing; SHA-1 vectors and the RFC 6455 accept example; frames masked/unmasked at 0, 125, 126, 65,535, 65,536 B; subscriptions; histogram quantiles; ports |
| `tests/fold.rs` (6) | **fold determinism** (gate W3): one archive folded twice → byte-identical trees (with `.gz` siblings checked by decompression); a fold checkpointed, restored and fed an overlapping remainder twice → the same bytes and the same state, zero rewrite alarms. **Restart and crash** through findex: steps without a checkpoint, drop (crash), reopen (resumes the source from the archived cursor, re-folds from the checkpoint written at open), finish → the same tree as a one-pass fold; archive verifies; `/h/events` numbering = the fold's. Per-bell files: the envelope (keys, slot, day, inputs, `seq`/`head` = the Province header), a SkipQuiet run of two bells, every clash `match` with the recomputed fighters, overviews in (P, Q) order with owners, site states, host counts, clash and dormant flags, the bell-region record (A, S by rule, cache, archived, tombstoned, final), `/h/me`, `latest`. A tampered outcome digest → `MISMATCH` and one alarm; with an inexact (RPC) post-state → `unchecked`, no alarm. Diffs carry heads and scopes; a failed transaction's records never appear |
| `tests/server.rs` (5) | over 127.0.0.1:0: every `/h/*` route, its cache header, ETag/304, gzip sibling (decompressed = plain), 400/404s, security headers and both CSPs, CORS; `/frontier/*` (redirects, ETag/304, gzip, hashed = immutable, traversal refused); `/gw/*` against a recording fake relay (query and body passed, `Cookie`/`Authorization` dropped, `X-Forwarded-For` rule, `Set-Cookie` and extra headers not returned, 405, 400, 413); WS subscribe, refusal, diffs in sequence with no gap while the rest of the season folds, routing by subscription; in memory on one thread: a socket lagging a 4-deep channel sees `seq` skip once, then is closed 1013; the heartbeat pings three times and closes a silent socket |
| `tests/viewers.rs` (1 + 1 ignored) | 100 polling + 20 WS viewers for 3 s while half the season folds: 0 errors, WS messages with no gaps; `viewers_measure` (ignored) is the measurement below |
| `tests/real_chain.rs` (1) | **over the chain the keeper drives** (the keeper tests' harness included by path, `localnet` in process at 20×, test key): 8 game bells of the program's own AnnounceSeason/CreateSeason/InitBeaconLogs, genesis seed, anchors, caches and beacon logs are ingested by `LocalnetFeed` and folded; `/h/season` from the Season account; 128 bell-region records checked against the chain (A, T(b), the cache's round and seed, S(b, A) by rule); index numbering = the fold's; a second fold of the archive is byte-identical |
| `findex` | + events numbering and rebuild in the index unit test; `enriched_poll_archives_post_state` (RpcPoll over the local node's JSON-RPC: landed transactions get the recipient's post-state, failed ones none, the archive keeps it) |

`real_chain` runs against the keeper's native model by default and against the program when `PSF_FRONTIER_SO` names the test-beacon `.so`; both were run (below).

## 3. Measurements

| what | result | how |
|---|---|---|
| 4,000 polling viewers (think 5 s ± 50%) + 1,000 WS viewers, 30 s, while the fixture season's second half folds | 24,202 requests, **0 errors**, p50 0.19 ms, **p99 43 ms**, max 225 ms, 648 req/s; 1,000 WS connected, 183,401 messages, **0 gaps**, 0 WS errors | `HERALD_SECONDS=30 cargo test --locked --release -p herald --test viewers viewers_measure -- --ignored --nocapture` (release; generator and herald in one process on this Mac, 8 runtime workers) [measured] |
| saturated origin: 500 polling viewers, think 1 ms, 10 s | 1,059,422 requests, **0 errors**, ≈ **106k req/s**, p50 1.9 ms, p99 5.6 ms (4% of them 404s for bells not yet folded) | `HERALD_VIEWERS=500 HERALD_WS=0 HERALD_SECONDS=10 HERALD_THINK_MS=1 … viewers_measure …` [measured] |
| per-file `fsync` on this Mac (APFS, `F_FULLFSYNC`) | ≈ 10–12 ms per file: the first fold test took 19.5 s with a sync per write, 1.7 s with batched durability (D4) | `cargo test -p herald --test fold` before/after [measured] |
| live binary smoke | `frontier-localnet --port 41760` + `frontier-herald --listen 127.0.0.1:41761 --test-key --web permutation-server/web/frontier`: `/h/season` 503 (no season on that chain), `/h/status` with the live Clock, `/frontier/` 200 with the web CSP, `/gw/*` 503 without `--relay`; `--listen 127.0.0.1:4190` refused (exit 2); SIGINT → checkpoint written, both stopped, ports 41760–41769 free afterwards | ad-hoc ports of unit 16 (§10.3: 41600 + 10·16) [measured] |

The contract's herald scale targets (≥ 5,000 req/s and 5,000 WS on one 4-core box, design §8.3) are for the exit run on the stack; the numbers above are an in-process check, not that run.

## 4. Deviations and decisions (for review)

| # | What | Why | Who |
|---|---|---|---|
| D1 | **Clash recomputation uses a provisional builder** (`clash::Provisional`: residents in state 1 with `from_bell ≤ b` at `Host::values_at`, garrisons from the site mirror at `GarrisonState::at` with walls = `walls_committed > 0` or a wall item effective ≤ b, garrison id = the holding key, the camp as a NEUTRAL garrison with id `u64::MAX − gen`, arrivals = present records, terrain from the compact arrays in borsh enum order, `Occupancy` from the entry states); it does **not** model the lazy camp respawn at the day's first resolve. The report names its builder (`"builder":"provisional-w3d"`) | The program's builder is W4-A's (`proc/clash.rs` is a stub in wave 3; the WASM `resolve_from_inputs` also waits for it). A builder mismatch would publish false `MISMATCH` alarms | **W4-A**: export the program's `ClashInput` builder as a pure function (e.g. in `frontier-abi` or `permutation-rules`) so the herald, WASM and verifier share it; then the herald's `FoldCfg.builder` takes it (one line in `runner::IngestCfg::new`) and `Provisional` is deleted |
| D2 | `heraldCheck` has a third value, **`unchecked`** (with `checkError`), when the input cannot be rebuilt (no seed of THE anchor's S known, Province/inputs not captured, a builder refusal) or when the source's post-state is not exact (`--source rpc`) and the digest differs | §8.4 lists `match|MISMATCH`; a herald that cannot recompute must not claim either | contract note at the next amendment |
| D3 | **WS `seq` is per socket**; a gap is signalled by skipping one number after a lag, and the socket is dropped after 3 lags; a `sub` is acknowledged by a message without `seq`; `bell` messages carry the file's path, not its bytes | §8.4 leaves the numbering open; the client's `DiffStream` (web `herald.mjs`) resyncs on any gap and ignores messages without `seq`; files are CDN-cacheable, pushing their bytes to every socket is not | W3-F uses these semantics |
| D4 | **Durability is batched**: files are atomic (temp + rename, readers never see a partial file) but not synced one by one; the paths written since the last checkpoint are synced (files, then directories) off the runtime's workers before the checkpoint is saved | one `fsync` ≈ 10 ms on this Mac; a bell of a few hundred files synced one by one would stall the fold (measured above). A power loss can only lose files written after the last checkpoint, and the restart re-folds exactly those | — |
| D5 | Only **`.gz`** siblings, no `.br` | brotli is another three crates outside the contract's expected set; gzip is a pure-Rust `flate2` (R3). A CDN in front can add br | integrator/owner: add `brotli` if wanted |
| D6 | `bell/{b}/region/{r}` is **rewritten** until final (`max-age=5`), immutable only once archived and resolved everywhere | §8.4 says "immutable once seeded", but the same record carries the per-province resolved flags and the tombstone/archive state, which change after seeding | contract note |
| D7 | The province, clash, transit and land records in the tests come from a **synthetic fixture** (`herald_fold::fixture`, layouts byte for byte, real event chains, the resolve digest recomputed by the same builder) | the program's land and clash instructions are W3-A/W4-A's (stubs on this branch). The beacon side is tested against the real program (`real_chain`) | W4-F (`itest::inproc_day`) runs the herald over the whole program |
| D8 | Overview choices where §9.3 is silent: sites at or beyond `site_count` → owner 7, state 3 (never shown as free); site state 2 (the unused camp state) → owner 6, state 2; released and reserved → state 3; `hosts_by_faction` counts roster entries (state 1) and a present camp as one neutral host; flag 2 reads the Holding's dormant-flag cache (flags bit 0); flag 4 when `opened_bell == b` (a pre-genesis opening counts as bell 0) | the binary has no other place for them | W3-F reads them |
| D9 | `/h/me` quota = the relay's `GET /f/quota?citizen=<Citizen address>` (the relay route's own parameter), `null` when no relay is configured or it does not answer within 2 s | §8.4 lists "relay quota" without a source | — |
| D10 | `tests/real_chain.rs` includes the keeper tests' harness (`keeper/tests/{common,model}/mod.rs`) by path and adds `keeper` as a herald dev-dependency | one harness for the in-process chain; W3-C owns it this wave | if W3-C changes `world()`/`keeper_config`/`fund`, the integrator adapts this test at the W3 merge (W3-C merges before W3-D) |

Not done in wave 3 (not in the brief's gate, recorded for later waves): retention (compacting quiet bells older than 48 h into daily files, design §8.3), Geyser ingest (M4), per-province broadcast channels (one channel with per-socket filtering is enough at the measured load), the 5,000-viewer one-game-day load run against the stack (W6/W7 with `frontier-viewers`).

## 5. Gate W3 — the parts that concern these files

Run in this worktree (`frontier-node`, toolchain 1.95.0, rustfmt/clippy installed with the owner's OK — DECISIONS part A):

| command | result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --workspace` (debug) | pass (exit 0; 41 test binaries; herald 9 + 6 + 5 + 1 + 1, findex 5 + 5) |
| `cargo test --locked --release --workspace` | pass (exit 0; 41 test binaries, keeper `one_day_beacons` included) |
| `PSF_FRONTIER_SO=<m1-integ test-beacon .so, sha256 145b9ca1…, 238,232 B> cargo test --locked --release -p herald --test real_chain -- --nocapture` | pass: the program's records, 128 bell-region records checked, 435 events |
| herald fold determinism (pass condition) | green (`tests/fold.rs`, `real_chain`) |

Not run here (other units' files): the root-workspace, `permutation-frontier/svm-tests`, gateway and web lines of Gates W1–W3. `build-wasm` stays **PENDING-OWNER** (wasm32 target, O-M1-12); nothing of this unit needs it. No devnet or mainnet transaction; no download beyond crates already in the local cargo cache (`flate2` and its three dependencies were resolved offline).

## 6. Dependency requests (integrator, I-55)

- **R1** — `frontier-node/Cargo.toml` `[workspace.dependencies]`: `findex = { path = "crates/findex" }` (the herald reads the archive and index).
- **R2** — `hyper = { version = "=1.11.1", features = ["http1", "server"] }`, `hyper-util = { version = "=0.1.21", features = ["tokio"] }` as workspace dependencies: the WebSocket upgrade under axum (`hyper::upgrade::on`, `TokioIo`) without axum's `ws` feature (which would pull tungstenite, not in the lock or the local cache). Both already in the lock through axum; no lock change of their own.
- **R3** — `flate2 = { version = "=1.1.10", default-features = false, features = ["rust_backend"] }`: the `.gz` siblings (§8.4). **Outside the contract's expected set**; pure Rust. Lock additions: `flate2 1.1.10`, `miniz_oxide 0.9.1`, `crc32fast 1.5.2`, `adler2 2.0.1`, `simd-adler32 0.3.10` (all from the local registry cache). If refused, the siblings can be switched off (`Out.gz = false`) and the gz tests dropped.
- **R4** — `crates/herald/Cargo.toml`: dependencies `fclient`, `findex`, `frontier-abi`, `permutation-rules`, `solana-address`, `sha2`, `hex`, `base64`, `serde_json`, `tokio`, `axum`, `hyper`, `hyper-util`, `flate2` (workspace); dev-dependencies `keeper = { path = "../keeper" }`, `localnet`, `solana-program-runtime`, `solana-instruction` (workspace; the `real_chain` harness). Second `[[bin]]` `frontier-viewers` (`src/bin/viewers.rs`).

## 7. For the next units

- **W3-F (web):** envelopes, overviews, bell-region, me and events are as §9.2/§9.3 and W2-E's fixtures, plus the extra fields in §1; WS per D3 (subscribe, `bell` messages name the file to refetch after the 0–15 s jitter, gap → resync). `/frontier/*` is served with the web CSP; hashed names get a year, others revalidate.
- **W4-A:** the shared `ClashInput` builder (D1).
- **W4-F (itest), W5 (stack):** run `frontier-herald --source localnet` on 41040 with `--relay 127.0.0.1:41033 --web permutation-server/web/frontier` (and `--test-key` for in-process/nightly runs); `frontier-viewers --stats 127.0.0.1:41075` for the load part of E5/E7. `Ingest`/`runner::run` and `server::serve` are the in-process entry points.
- **W4-D (verifier):** `findex::Enriched` gives archive post-state from a public RPC; the verifier still re-derives heads from the chain (K4).

## 8. Links

- Code: `frontier-node/crates/herald/src/{lib,fold,clash,overview,records,views,files,checkpoint,runner,server,ws,viewers,fixture,main}.rs`, `frontier-node/crates/herald/src/bin/viewers.rs`; `frontier-node/crates/findex/src/{index,archive,ingest,lib}.rs`
- Tests: `frontier-node/crates/herald/tests/{fold,server,viewers,real_chain}.rs`, `frontier-node/crates/findex/tests/ingest.rs`
- Contract §8.4, §9.2, §9.3: `docs/frontier/m1/M1-CONTRACT.md`; web client read path: `permutation-server/web/frontier/herald.mjs`, `permutation-gateway/client/src/frontier/herald.mjs`
