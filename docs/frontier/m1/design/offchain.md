# M1 "First Bell": off-chain stack design

- **Date:** 2026-09-27. **Area:** off-chain. It covers the keeper, the relay and sponsorship, the herald read path, the bots, verifier v2, and the local stack for the 7-day, 1,000-bot accelerated season.
- **Normative inputs:**
  - DESIGN rev 3.1: §0–§13, §19; in particular §6.2–§6.4, §8.2–§8.8, §9, §11.3, §12.
  - M0-FINAL; SP-V2; SP-FEE (with the m0c corrections).
  - The owner decisions O1–O8, N1–N5 and D23. O8 means no ER lane, no Skirmish and no MagicBlock.
  - Sibling M1 designs, reconciled below: `m1/design/web.md` (the herald and relay contracts the client consumes) and `m1/design/closeout.md` (CL-19/20/21 time and address primitives, CL-23 reveal latch, CL-30/31a escalation classes).
- **Code read** (`codex/frontier` at `d95fa25`):
  - `permutation-gateway/src/{crank,send,cosign,guards,config,app,server}.mjs`, `routes/relay.mjs`, `scripts/{local-stack,sync-web-sdk}.mjs`;
  - `permutation-server` (README, `chainlink.rs`, bots, `bin/verify/*`);
  - `frontier-sim/src/{model,sim}.rs`;
  - `permutation-rules/src/frontier/clash.rs` (API);
  - `permutation-chain/DESIGN.md` (log and verifier conventions);
  - lab code: SP-V2 `host/src/{lab,drand}.rs`, `program/src/seal.rs`; SP-FEE `driver/`; S-TLOCK `js/`, `rs-interop/`.
- **Rules kept:** read-only on the repo; no commits; no servers started; no devnet or mainnet traffic. One lab experiment was run in a copy (§17).
- **Tags:** [measured], [model], [estimate] and [design] are used as in DESIGN.

---

## 0. Decisions in one table

| # | Decision | Why |
|---|---|---|
| F1 | **Rust for everything on the replay path and the keeper**: the keeper, herald, verifier v2, bots, local chain node, drand replay server and stack orchestrator. All live in one new cargo workspace, `frontier-node/` | They share the rules-v10 kernels, the program's ABI and the seal and beacon crypto. Rust tlock opens a seal in **0.92 ms** against 11.3 ms in tlock-js [measured, S-TLOCK `rust-interop.txt`, `js-bench.txt`]. The hash-to-curve hint code already exists in Rust (SP-V2 `host/src/drand.rs`). LiteSVM allows deterministic in-process tests of the whole stack |
| F2 | **Node for the relay only**: `permutation-gateway/src/frontier/`, plus the JS SDK in `permutation-gateway/client/src/frontier/`, synced to the web by `sync-web-sdk.mjs` | Reuses `cosign.mjs` (exact parse, signature check, simulate before signing), `guards.mjs` (RateLimiter, FundsGuard, ReplayCache, TtlCache, clientIp), `send.mjs` (HTTP-only confirmation) and the app and route pattern. The browser signs in JS anyway |
| F3 | **One shared ABI crate, `frontier-abi`** (owned by the program area; requested here), with no Solana dependencies. It holds account layouts, instruction encodings, with-seed seed strings (CL-21) and the PS2 log events. The off-chain side depends on it and on `permutation-rules` | This avoids the Solana-crate version clash that forced `svm-tests` into its own lock. Both languages get one source of truth, with vectors checked in JS |
| F4 | **Every component is a library driven by `ChainPort` / `DrandPort` / `GameClock` traits.** The binaries only add IO | The whole stack (keeper, bots, herald fold, verifier) can run **in one process against in-process LiteSVM with virtual time**. That gives deterministic A/B tests: the program-level lag gate, crash injection and tamper tests |
| F5 | **The accelerated season runs on `frontier-localnet`**: a LiteSVM-backed JSON-RPC node with a scaled clock. Default scale is 20×, so 7 game days take 8.4 h of wall time. It is paired with **`drand-replay`**, which serves *real historical quicknet rounds* gated by the game clock | The production `.so` runs unchanged: quicknet's key is compiled in, there is no test-key build, and the verifier checks real BLS signatures. A real validator's clock cannot be sped up. LiteSVM has the BLS12-381 syscalls; the repo's `solana-test-validator` 3.1.9 does not |
| F6 | **A real-time 24-h soak on Agave ≥ 4.0 `solana-test-validator`** (1,000 bots, live quicknet) as a second exit run, **if the owner approves installing Agave 4.x** | It covers what the local chain node cannot: real slots, the cost tracker, real RPC and real drand latency |
| F7 | Keeper escalation follows the SDK rules (§6.4 defence 2), with the **closeout class split**. Window-closing writes (Reveal, RevealPosture) go up to **P_def 2.0** and are pool-eligible. Delay-only writes (PostAnchor, PostSeed, GatherClash, ResolveFromInputs) go up to **P_delay 0.5** and are never pool-eligible (CL-31a). Every critical write is paid from **≥ 150 rotating fee payers**, which are also its rent payers | O7/N1 and SP-FEE drill (g). No shared writable account ever enters a critical transaction |
| F8 | Owner Reveals without a player signer go through the relay to the **keeper's loopback submit API**, which alone rotates payers and escalates. The escalation engine exists once, in Rust | A Reveal needs no signature from the player (anyone holding the plaintext may send it), so the keeper can re-sign each bid level itself |
| F9 | The herald writes **immutable per-(province, bell) files** holding raw account bytes, slot and `event_head`, plus short-TTL "latest" aliases and a WebSocket diff stream. It is served directly and CDN-ready. It is never a trust root | This is the design §9.3 and web.md §4 contract. It targets ≥ 5,000 concurrent viewers on one origin box, and more behind a CDN |
| F10 | Bots are **new Rust agents** that use frontier-sim's archetype profiles (`model.rs`) through a small shared `frontier-agents` library. The v9 bots (`permutation-server/src/bots`) are **not ported**: they are about tick orders for nations and offices, which the Frontier does not have. A public bot SDK (JS) comes in M3 | The local season then checks the simulator's play assumptions on the real chain path |
| F11 | Verifier v2 is **new** (`frontier-verify`). It keeps v9 `bin/verify`'s module shape and its "the index is not trusted" principle. It replays per entity in parallel, checks against **on-chain heads**, and audits seals with drand's `tlock` crate, which is independent of the on-chain opener | Design §9.1, K2 and K4 |

---

## 1. Components and data flow

```
                      drand quicknet (live)  ─or─  drand-replay (historical rounds, gated by game clock)
                               │ rounds, BLS sigs
                               ▼
 ┌────────────┐  critical writes (≥150 rotating payers, escalation)   ┌──────────────────────────────┐
 │  keeper(s) │ ─────────────────────────────────────────────────────► │ Solana: Agave ≥4 validator   │
 │  Rust      │ ◄──── ingest (sigs+txs+accounts) ──────────────────── │  or frontier-localnet (LiteSVM│
 └─────▲──────┘                                                        │  + scaled Clock)             │
       │ loopback submit (owner reveals, nudges)                       └───────▲───────────┬──────────┘
 ┌─────┴──────┐  co-signed player txs (session key + relay payer)             │           │ ingest
 │ relay      │ ───────────────────────────────────────────────────────────────┘           ▼
 │ Node       │                                                        ┌──────────────────────────────┐
 └─────▲──────┘                                                        │ herald (Rust): fold → files  │
       │ /gw/f/*  (proxied by the herald origin)                       │ /h/* immutable + latest, WS  │
 ┌─────┴──────────────────────┐   reads /h/*, WS /h/ws                 └───────┬──────────────────────┘
 │ web client, bots (1,000)   │ ◄─────────────────────────────────────────────┘
 └────────────────────────────┘
 verifier v2 (Rust): tx archive (own RPC fetch or stack archive) + on-chain heads + quicknet key → PASS/FAIL
```

---

## 2. Reuse vs new

| Existing piece | Fate in M1 | Notes |
|---|---|---|
| `permutation-gateway/src/cosign.mjs`, `guards.mjs`, `send.mjs` (`simulateWire`, HTTP-poll `confirm`), `routes/errors.mjs`, the `app.mjs` two-listener pattern, `config.mjs` (option table, atomic state store) | **Reused** by the Frontier relay (imported from `src/`) | v9 behaviour is untouched: the Frontier relay has its own entry point |
| `client/src/{bytes,base58,sha256,borsh,solana-tx,retry}.mjs` | **Reused** by the JS SDK and the web (already browser-safe) | |
| `routes/x402.mjs`, `faucet.mjs` | x402 **parked until M2** (M1 is free). The faucet is kept for localnet SOL airdrops only | |
| `crank.mjs`, `sealed.mjs`, `ticks.mjs`, `tickscan.mjs`, `seats.mjs`, `season.mjs`, `registry.mjs`, `talk.mjs`, `advisor.mjs`, `agents/*` | **Not used by the Frontier** (they are v9 tick, ER and seat code). `crank.mjs` becomes the Rust keeper, as §11.3 says. `agents/runner.mjs` is a pattern for the M3 JS bot SDK | Stays for v9; `sealed.mjs` is retired per §11.3 |
| `scripts/local-stack.mjs` (ports 18899/17799/16699, `mb-stack`) | **Not used** | These are reserved ports and the MagicBlock stack. A new `frontier-stack` replaces it |
| `permutation-server/src/chainlink.rs` (minimal HTTP/1.1 JSON-RPC) | **Pattern** for `fclient::rpc` (blocking std client for the verifier CLI; async client for the keeper and herald) | |
| `permutation-server/src/play/http.rs` (security headers), `proxy.rs` (`/gw` proxy that forwards no credentials) | **Ported** into the herald's server (same header set; `/gw/*` proxy to the relay's public listener) | web.md §3 serves the page, `/h/*` and `/gw/*` from one origin |
| `permutation-server/src/bin/verify/{main,chain,report}.rs` | **Structure and report format reused**; logic new | v9 replays tick inputs; v2 replays entities |
| `permutation-server/tests/codec_vectors.rs` → gateway JS test | **Pattern reused**: `fclient` tests write `permutation-gateway/test/frontier-vectors.json` | |
| `permutation-server/src/bots`, `driver.rs` | **Not ported** (v9 nation orders) | |
| `frontier-sim/src/model.rs` (Arch, Profile) and its decision heuristics in `sim.rs` | **Extracted** (profiles) into `frontier-agents`; heuristics re-implemented over an `Observation` | A test pins the extracted profiles equal to the simulator's |
| SP-V2 `host/src/drand.rs` (quicknet beacons, xmd, hash-to-curve **hints**, off-chain BLS check), `host/src/lab.rs` (LiteSVM harness, tx shape) | **Promoted** into `fclient::beacon` and `frontier-localnet` | |
| SP-V2 `program/src/seal.rs` constants; S-TLOCK `rs-interop` (Rust `tlock` 0.0.10 opens tlock-js compact-16 seals), `js/march.cjs` (keystream, salt, commit) | **Promoted** into `fclient::seal` (Rust) and `client/src/frontier/seal.mjs` (JS), with shared vectors | The domains are pinned in DESIGN §6.2 |
| SP-FEE `driver/lib.cjs`, `drill.cjs` (priority formula, filler floods), `c4_model.py` | Priority math **ported** to `fclient::fees`. The flood generator becomes the **contention emulator's** adversary scripts. `c4_model.py` is re-run with the measured Reveal CU (CL-26/30) | |
| `permutation-chain/svm-tests` | Pattern only | |

---

## 3. File layout

```
frontier-node/                         # new cargo workspace (own Cargo.lock, rust-toolchain 1.95.0 for LiteSVM 0.16;
│                                      #  excluded from the root workspace like frontier-sim and svm-tests)
├── Cargo.toml
├── crates/
│   ├── fclient/        (lib)  ABI glue over frontier-abi: ids and with-seed/PDA addresses (via permutation-rules
│   │                          frontier::addr, CL-21); instruction builders; account decoders; PS2 event parser
│   │                          and event-head chain; fees.rs (priority ⇄ CU price, cost model); tx.rs (compute
│   │                          budget, loaded-data limit, legacy message); rpc.rs (JSON-RPC client, async + blocking);
│   │                          beacon.rs (drand HTTP client, BLS verify via blstrs, hash-to-curve hints from SP-V2);
│   │                          seal.rs (tlock 0.0.10 IBE + PS-KS body + PS-SALT + commitment); clock.rs (GameClock
│   │                          from the Clock sysvar; tlock_round/seed_round via frontier::beacon, CL-19/20);
│   │                          ports.rs (ChainPort, DrandPort traits; Rpc, InProcess backends); payers.rs (derive,
│   │                          fund, sweep a rotating pool)
│   ├── findex/         (lib)  ingest backends (RpcPoll, LocalnetFeed, later Geyser); tx archive (append-only
│   │                          segments + manifest); SQLite entity index; per-bell work views
│   ├── keeper/         (bin frontier-keeper; lib keeper-core)
│   ├── herald/         (bin frontier-herald; lib herald-fold)
│   ├── verify/         (bin frontier-verify; lib verify-core) + tests/tamper.rs + fixtures/
│   ├── agents/         (lib frontier-agents: profiles, Observation, policies)
│   ├── bots/           (bin frontier-bots)
│   ├── localnet/       (bin frontier-localnet; lib: RPC server over LiteSVM, scaled clock, block builder,
│   │                          contention emulator, account/tx feed)
│   ├── drand-replay/   (bin drand-replay)
│   └── stack/          (bin frontier-stack: orchestrator, ports, chaos, run reports)
├── fixtures/           recorded mini-seasons (archive + final heads) for verifier tests
└── README.md

permutation-gateway/
├── src/frontier/       server.mjs, app.mjs, shapes.mjs, quota.mjs, payers.mjs, invites.mjs, keeperlink.mjs,
│                       routes/{relay,join,reveal,nudge,quota,season,operator}.mjs
├── client/src/frontier/ codec.mjs, addresses.mjs, seal.mjs, fees.mjs, budgets.mjs, shapes.mjs, herald.mjs
└── test/frontier-*.test.mjs (+ frontier-vectors.json written by fclient tests)

(program area)  permutation-frontier/ (program) and frontier-abi/ (no_std, no Solana deps) — requested, §5.1
(web area)      permutation-server/web/frontier/ (web.md) — consumes §7 and §8
```

Ports for the local stack are listed in §11.3. **None of them is on the reserved list.**

---

## 4. Shared foundations

### 4.1 Traits (the testability spine)

```rust
trait ChainPort {            // Rpc(url…) or InProcess(LiteSVM) — same code in tests and binaries
    async fn clock(&self) -> ClockSysvar;                                  // slot, unix_timestamp
    async fn accounts(&self, keys: &[Pubkey], min_slot: u64) -> Vec<Option<Account>>;
    async fn simulate(&self, tx: &[u8]) -> SimResult;                      // logs, CU, post balances
    async fn send(&self, tx: &[u8]) -> Signature;
    async fn statuses(&self, sigs: &[Signature]) -> Vec<Option<Status>>;
    async fn feed(&self, after: Cursor) -> Vec<TxRecord>;                  // program txs incl. failed, in slot order
    async fn blockhash(&self) -> (Hash, u64);
}
trait DrandPort { async fn round(&self, r: u64) -> Option<Beacon>; fn info(&self) -> ChainInfo; }
struct GameClock { /* extrapolates the Clock sysvar; detects the time scale; never reads the wall clock for game rules */ }
```

- `feed` is `getSignaturesForAddress(program)` + `getTransaction` paging on a real RPC. On the local chain node it is a native ordered stream. In-process it is a vector.
- **Every rule time comes from `GameClock`, never from the wall clock.** That covers T(b), A + W, S(b, r), bell_start, the 48-h archive time and dormancy. This is what makes 1× and 20× runs behave the same.

### 4.2 Fees and bids (`fclient::fees`, SP-FEE §3, DESIGN §8.7)

```
cost      = cu_limit + 720·n_sig + 300·n_write_locks + 8·ceil(loaded_limit / 32 KiB)     (64 KiB → +16)
priority  = (priority_fee + 2,500) / cost
fee(p)    = max(0, p·cost − 2,500)  lamports;   cu_price = ceil(fee(p)·1e6 / cu_limit) µlamports/CU
p_tip     = (tip − 2,500) / cost       (the default tip 10,006 gives 0.433 at a 16k Reveal limit: cost = 16,000 + 1,336)
```

- Every keeper transaction sets `SetComputeUnitLimit` from a per-kind table and `SetLoadedAccountsDataSizeLimit(64 KiB)`.
- The CU table is generated from the program's measured maxima + 5% (`budgets.rs` / `budgets.mjs`, the same table web.md uses).

### 4.3 PS2 events and the archive

- `findex` stores **every program transaction, failed ones included**: signature, slot, block time, message, logs, error, CU used.
- Storage is append-only segment files of about 64 MiB, plus a manifest with each segment's sha256.
- It also stores **account snapshots** of program-owned writable accounts after each landed transaction (see §8.2).
- The herald, keeper and verifier all read this archive. **Only the verifier refuses to trust it**: it re-derives every head from on-chain accounts (§10).

---

## 5. Interfaces the off-chain stack needs from the program area

These are requests, to reconcile with the program design. Each one names the off-chain consumer.

| # | Requirement | Consumer |
|---|---|---|
| P1 | **`frontier-abi` crate** (no_std, no Solana deps): account layouts (magic, offsets), instruction tags and encodings, the error-code table, with-seed seed strings (CL-21's `frontier::addr`, which may live in `permutation-rules`), and PS2 event encoding. Versioned; never reorder tags | all |
| P2 | **PS2 log contract v1.** Each account write emits one `sol_log_data` record: `"PS2" ‖ ver u8 ‖ kind u8 ‖ entity 32 ‖ seq u32 ‖ payload` (≤ 128 B, DESIGN §8.3). **The account stores `event_head = sha256(prev_head ‖ record)` and `seq`.** Cross-entity events are chained into every account they touch. Resolve logs `outcome_digest` (`ClashOutcome::digest`) and the clash input digest. Seals and commitments are *not* repeated in logs (they are in the instruction data, which the archive keeps) | verifier (chains, K4), herald (events feed) |
| P3 | A **season genesis record** logged at CreateSeason/ConsumeGenesisSeed: ruleset hash, program version, quicknet chain hash, W, Δ, R_MAX, genesis_ts; plus AnnounceSeason (CL-24) | verifier, herald `/h/season` |
| P4 | **A beneficiary field** (a pubkey in the instruction data, neither signer nor writable) on Reveal, RevealPosture, GatherClash, ResolveFromInputs and ProveBadSeal. It is recorded in the written account, and SettleTransit / close paths pay tips, march fees, prover rewards and rent refunds to it. **Rent is paid by the fee payer** (the rotating key), and refunds go to the beneficiary | keeper (sweeping 150 payers otherwise costs one tx per payer per refund) |
| P5 | **A separate `rent_payer` / `funder` signer** on Join, SettleTicket and Depart/CommitPosture escrows (march fee, seal bond, tip). It may equal the authority. Close paths refund rent to the recorded rent payer | relay (M1 is free: the relay fronts rent and escrows) |
| P6 | **Landing evidence in critical accounts**: landed slot, declared CU price and limit (read from the instructions sysvar, read-only and unlockable), fee payer and beneficiary. ClaimDefence (M2, or M1 if the pool lands early) can then check lateness and bid without looking up transactions | keeper defence claims, verifier liveness report |
| P7 | **A quota-refused arrival must not be routed.** When a Reveal is refused because the four slots already hold larger arrivals (or the citizen already has an arrival there), SettleTransit must accept the plaintext as proof that it was valid and bounce the host home with no loss. The alternative is a program-side "RevealRefused" record | keeper settle duty, verifier |
| P8 | **Deterministic error codes** for the keeper's retry logic: `SlotChanged` (retry with a fresh read), `WindowClosed` (stop), `NotAnchored`, `SeedNotReady`, `NotAllGathered`, `AlreadyDone` (no-op success preferred) | keeper |
| P9 | The **quiet-bell proof** (M1 must choose one) and any batch "resolve quiet run" instruction | keeper catch-up, herald `resolved_through` |
| P10 | Keep the with-seed address tables and `tlock_round` / `seed_round` in `permutation-rules` (CL-19/20/21), so that the keeper, verifier and web use the kernel functions rather than copies | all |

---

## 6. The keeper (`frontier-keeper`, Rust)

### 6.1 Duties

"Crit." marks the class: **W** window-closing (P_def 2.0, pool-eligible), **D** delay-only (P_delay 0.5, own budget), **N** non-critical (priority 0 to market level).

| Duty | Trigger (game time) | Deadline | Transaction(s) | Crit. |
|---|---|---|---|---|
| Genesis seed | `first_round ≥ t_create_min + 660` (CL-24) is published | none (the 7-day Abort guard) | ConsumeGenesisSeed | D |
| Anchor | round T(b) published (0.8–2.0 s after its time [measured, S-TLOCK]) | none (the game waits) | Combined 16-region PostAnchor **and** 16 per-region fallbacks | D |
| Seed caches | S(b, r) = the first round at or after A(b, r) + W + Δ is published | none | PostSeed(region, random unused nonce 0..255); switch nonce if not landed after 2 slots | D |
| BeaconLog | every bell (config) | — | PostBeacon for each of the 16 shards, with the latest round | N |
| Decrypt | T(b) published | before A + W | off chain: open every seal targeted at T(b) (Depart arrivals at b, postures for b) | — |
| Reveal | from T(b) (bell start for owner submissions through the relay) | **A(b, r) + W** (never sent after `A + W − ε`; the CL-23 latch is also respected) | Reveal(plaintext, salt, ct_hash, beneficiary) into the slot index the kernel's `admit_arrival` picks | **W** |
| RevealPosture | T(b) published | A + W | RevealPosture | **W** |
| ProveBadSeal | a seal failed to open (FO failure, bad point, wrong round or commitment mismatch) and THE anchor exists | before SettleTransit is allowed (close + 1 bell) | ProveBadSeal(commit, seal, beneficiary) | N (a cap at tip level; it pays the prover) |
| Gather | Clock ≥ A + W, the province-bell has activity, and every arrival's origin is resolved through its departure bell | none | GatherClash parts (≤ 24 positions; ≤ 10 arrivals with their origin Provinces) | D |
| Resolve | all parts gathered, a SeedCache exists, and the province is resolved through b − 1 | none | ResolveFromInputs (heap frame when the kernel needs it) | D |
| Catch-up | a `nudge` from the relay/web, or residents with hostile neighbours | — | quiet-run resolves (P9) | D |
| Tickets and explore | the bell's seed exists | challenge window (end of b + 1) | SettleTicket per filed ticket in seed order; explore settlement if the program defines a crank | N |
| SettleTransit | after close + 1 bell | — | SettleTransit, including quota-refused arrivals with their plaintext (P7) and unrevealed arrivals (rout) | N |
| Close short-lived accounts | after resolution | — | CloseClashInputs, close PosturePDAs | N |
| ArchiveAnchors | A + 48 h for each region-day | — | ArchiveAnchors (tombstone, then close anchors and caches) | N |
| FoldOccupancy | every bell while joins are open | — | FoldOccupancy (2 txs) | N |
| Rings | the crowding rule holds (from the index) | — | OpenRing → ConsumeRingSeed (first round ≥ t_open + 660) → OpenProvince × 6d | N |
| Dormancy | a first holding idle for 10 days | — | the release crank, if the program has one (otherwise lazy) | N |
| Payer care | balance outside the band | — | fund and sweep (not critical; never inside a critical transaction) | N |

### 6.2 Bid policy (the keeper SDK rules, part of the spec)

- **Classes (CL-31a):** W writes start at `p_start` and double every slot up to **P_def = 2.0**. D writes double up to **P_delay = 0.5**. N writes use a fixed low bid.
- **Resend every slot.** Each bid level is a new signature from a newly drawn random payer. The tight CU limit comes from the budget table, with the 64 KiB loaded-data limit.
- **`p_start`:** the design says "start at the tip level". That leaves a keeper with **zero margin**: the tip exactly pays base fee + priority fee at p_tip. This design keeps it as the default for W writes, so C4 is modelled as written, and exposes `--peace-start` (for example 0.25 × p_tip). **Open question Q3.**
- **Attack detection:** a W or D write not landed within 2 slots at a bid ≥ p_tip marks (bell, region) as **contested**. Then:
  - every W write for that region starts at p_tip;
  - per-region anchor fallbacks are sent every slot;
  - an alert is raised;
  - evidence is journalled for ClaimDefence (P6).
- **Never after the close:** a Reveal is not sent once `GameClock ≥ A + W − ε` (ε = 2 slots). An unlanded valid seal at the close is journalled as a liveness finding, with every attempted signature. The verifier joins this record.
- **Duplicate versions:** only one Reveal version can succeed, because a slot's host is written once. The others fail with AlreadyDone for about 5,000 lamports each. The bound is (resends per write) × (base + fee) ≤ the configured per-write spend cap.

### 6.3 The rotating payer pool (≥ 150, DESIGN §6.4)

- **Keys:** payer_i = ed25519 from `sha256("PS-FRONTIER-PAYER-v1" ‖ master_seed ‖ pool_id ‖ i)`, with i < N and N ≥ 150 enforced at startup (the keeper refuses to start below 150 unless `--dev`). The master seed file is mode 0600.
- **Selection:** a uniform random draw per transaction version (OS CSPRNG). Payers under the balance floor are skipped. **No round-robin.** A test checks uniformity (χ²) and that no fixed payer ever appears in a critical transaction.
- **The payer is also the rent payer** (ArrivalSlot 1.14M lamports, ClashInputs 5.85M [measured, SP-V2]), so no shared funding account is writable in a critical transaction. The float per payer at 50k players is about 20 reveals per payer per bell × 1.14M × about 3 bells before refund ≈ 0.07 SOL, so the pool is **≈ 10 SOL** [model]. At 1,000 players it is ≈ 0.75 SOL [model].
- **Refunds** go to the beneficiary (P4). A background sweeper tops payers up into the band [0.01, 0.05] SOL (configurable) with non-critical transfers.
- Being enumerable is acceptable: 150 × 40M / 60 locks per stream > 100M, so holding every payer still costs a whole block (DESIGN §6.4). What matters is N ≥ 150 and a random draw.

### 6.4 Beacon handling

- Rounds are fetched in parallel from ≥ 2 relays (configurable: `api.drand.sh`, `api2`, `api3`, `drand.cloudflare.com`, or `drand-replay`).
- Each round is verified off chain first (blstrs pairing, as in SP-V2 `drand.rs`), so a bad beacon never costs a 345k-CU transaction.
- The hints (145 B per map) are computed once per round and cached.
- **Combined and fallbacks together:** SP-FEE D5 held the combined form for 9.8 s by holding one region, while a fallback landed in 2.0 s [measured]. In peacetime the keeper may delay fallbacks by 1 slot to save their priority fee; this is charged on the declared 345k limit even for a no-op. The cost of always sending them at p = 0.1 is ≈ 16 × 34k lamports per bell ≈ 0.08 SOL/day [model].
- **Outages:** the keeper keeps polling. Windows are anchored, so nothing expires (A4). The keeper exposes `beacon_lag_seconds`.

### 6.5 Reveal on behalf

1. **Index.** Every Depart and CommitPosture in the archive: host, holding, arrive_bell, commit, seal (165 B from the instruction data), departure mass and tip.
2. **At T(b):**
   - open every seal of bell b (Rust `tlock` IBE + PS-KS body; 0.92 ms/seal/thread [measured]; about 3k seals per bell at 50k ≈ 0.35 s on 8 threads [model]);
   - check `sha256("PS-FRONTIER-MARCH-v1" ‖ plaintext37 ‖ salt) = commit` and `sha256(commit ‖ sha256(seal)) = seal_root`;
   - bad seals go to the ProveBadSeal queue.
3. **Group by (P, Q, b, faction).** Send in **descending departure mass**, so the first four to land are the final four. No displacement churn, and at most 4 Reveals per group create slots.
   - Each Reveal targets the slot index that `admit_arrival` computes from the index's view of the 4 slots.
   - On `SlotChanged`, re-read the slots and retry (at most 4 times).
   - Arrivals refused by quota and second arrivals of one citizen go to the SettleTransit-with-plaintext queue (P7).
4. **Owner submissions** (`POST /v1/reveal` on loopback, fed by the relay's `/f/reveal`):
   - validated (commitment against the logged root);
   - sent at once from the bell start with the same escalation.
   - This covers in-bell self-reveals without exposing a predictable account (web.md §5).
5. **Postures:** the same pipeline for RevealPosture.
6. **Multiple keepers:** a random start jitter of 0–2 slots per group lowers duplicate spend. Tips go to whoever lands, so competition is intended.

### 6.6 The gather and resolve scheduler

- **A dependency DAG:**
  - `resolve(p, b)` needs `resolve(p, b−1)` (or the quiet proof), `gathers(p, b)` and `seed(b, region(p))`;
  - `gather-part(p, b, k)` needs Clock ≥ A + W and, for each arrival in the part, `resolve(origin, depart_bell)` (DESIGN §6.2, 3.1: gathers read origin post-clash values).
- **No deadlines** (delay-only). The scheduler works critical-path-first: the provinces with arrivals, then the ones blocking resident actions (nudges).
- The CU limit comes from the budget table: ResolveFromInputs up to 650k with its heap frame. In test builds the keeper also simulates first and checks the result against the table (a guard against budget drift).
- **Order independence:** gathers commute (SP-V2 `t2b`), so several keepers may gather concurrently.

### 6.7 Crash safety

- **The chain is the state.** Every duty is idempotent and succeeds once per object. So a restart rebuilds the queue from the index (`findex`) plus on-chain reads. The journal speeds this up and keeps evidence; it is not needed for correctness.
- **The journal** is SQLite in WAL mode with `synchronous=FULL` for the attempts table:
  - `attempts(sig, kind, object_key, bell, region, class, payer, bid_priority, cu_limit, first_valid_slot, sent_slot, landed_slot, status)`;
  - `plaintexts(host, bell, plaintext, salt)` (a cache: re-derivable from the logs and drand);
  - `cursor(ingest)`;
  - `claims(evidence…)`;
  - `payers(i, balance_seen)`.
- **Startup order:** take the lock file (one process per keeper identity, `flock`) → reconcile in-flight attempts with `getSignatureStatuses` → catch up the ingest from the cursor → rebuild the queues → resume.
- **Crash-injection test:** kill the process at every journal write point (about 20 points × 3 duty kinds) in the in-process harness. After a restart, the season must end identical (the outcome digests match the uncrashed run), with extra spend ≤ 1 duplicate version per in-flight write.

### 6.8 Anyone can run one

- **Deliverable:** a single binary plus `keeper.toml`:
  - RPC URLs (≥ 2 recommended) and drand relays;
  - `roles = [beacon, reveal, posture, gather, resolve, settle, prove, archive, fold, rings]`;
  - `regions = "0-15"` (sharding across keepers);
  - pool size (≥ 150), payer band, P_def and P_delay (capped by the season's published maxima), daily SOL budget;
  - beneficiary pubkey; metrics port (loopback).
- **No privileged key.** The operator's keeper differs only in its roles: beacon, archive, fold and rings are the operator's duty in §8.8, and nobody is tipped for them.
- **Public keepers:** the default profile is reveal, posture, settle and prove, which the tips pay for.
- **Docs:** "Run a keeper" (economics per bell, float sizing, pool funding, risks).
- **Metrics** (`/metrics`, Prometheus text; `/v1/status` JSON):
  - latencies: round → anchor, S → cache, T(b) → last reveal landed, close → resolve;
  - counts: landed/failed per kind, reveals near the close, contested regions;
  - money: bids and SOL spent/earned per bell, payer balances, beacon lag.

### 6.9 The keeper's loopback API (used by the relay and the stack)

| Route | Body | Answer |
|---|---|---|
| `POST /v1/reveal` | `{holding, transit_slot, plaintext_b64, salt_b64}` or a Reveal/RevealPosture instruction `{ix_b64}` | `{accepted, track}`; 409 if the commitment mismatches; 410 after the close |
| `POST /v1/nudge` | `{province:[P,Q], bell}` | `{queued, blocking:[…]}` |
| `GET /v1/status` | — | duties per bell, lag, pool, spend |
| `GET /metrics` | — | Prometheus text |

Bound to 127.0.0.1 only. The relay authenticates with a shared token file, like the gateway's operator token.

---

## 7. Relay and sponsorship for a free M1 (Node, `permutation-gateway/src/frontier/`)

### 7.1 Who pays what (M1, no money)

| Cost | Payer in M1 | Recovered |
|---|---|---|
| Transaction fees for player actions (Join, FileTicket, Harvest/Build/Train, Muster, Depart, CommitPosture, Explore, SettleTransit by owner) | relay payer pool | no |
| Rent of the Citizen (2.28M lamports) and Holding ((128 + 832) × 5,080 ≈ 4.88M) [derived] | relay payer (as `rent_payer`, P5) | yes, at close (the M2 close paths); the float is ≈ 7.2M lamports/player, so ≈ 1.44 SOL for a 200-person playtest [model] |
| Depart/CommitPosture escrows (march fee 10k, seal bond 20k, tip 10k lamports) | relay payer as `funder` (P5), within the lamport quota | the bond goes back to the holding; the tip and fee go to keepers |
| Reveals (owner or keeper), gathers, resolves, beacons | keeper pools (operator keeper for beacons) | tips and march fees |
| ProvinceFund for rings ≤ 7–10 (162–330 provinces × (128 + 4,096) × 5,080 ≈ 0.0215 SOL each) [derived] | operator | at close |

### 7.2 Quotas and abuse limits

| Limit | Default (M1) | Where |
|---|---|---|
| Joins | **invite code required on devnet** (a one-time HMAC token from `POST /f/operator/invites`); none locally. Per IP 10/h. One Citizen per wallet (on chain) | `invites.mjs`, `guards` |
| Sponsored transactions per Citizen | **40 per game day for days 0–6, 20 after** (D4), burst 60. Refused with `429 QuotaExceeded {retryAt}`; a player with SOL may send directly | `quota.mjs` (SQLite or JSON store, keyed by Citizen PDA, reset at game midnight) |
| Sponsored lamports per Citizen per game day | ≤ 24 Departs' escrow (≈ 0.00096 SOL) plus rent at Join and SettleTicket | `quota.mjs` |
| Per signer | `SIGNER_LIMITS.relay`-style bucket (burst 24, 1/s) | `guards` |
| Per IP (public listener) | `IP_LIMITS`-style: `POST /f/relay` 40 / 2 per s, `POST /f/join` 10 / 0.2 per s, `POST /f/reveal` 40 / 2 per s | `guards` |
| Replay | ReplayCache 30 min; charge only transactions that landed | `guards` |
| Funds breaker | FundsGuard on the relay pool's total (503 OperatorLowFunds) | `guards` |
| **Shape allowlist** | exactly `[SetComputeUnitLimit, SetComputeUnitPrice(≤ relay cap, 0 by default), SetLoadedAccountsDataSizeLimit?, one Frontier player instruction]`; the program id must match; the fee payer is in the relay pool; the authority signature is verified (`signedBy`) | `shapes.mjs` (vectors shared with web.md §5.4) |
| **Drain guard** | simulate with signatures checked and read the relay payer's post-balance: `Δlamports ≤ fee + (rent + escrow allowed for that instruction kind)`; otherwise 400 `RelayRejected` | `app.mjs` over `simulateWire` |
| Local bot profile | loopback clients are exempt from IP limits; quotas still apply (bots act within the 30/h bucket anyway) | config profile `local` |

### 7.3 Routes (public listener; the herald proxies `/gw/*` on the web origin, per web.md)

| Route | Contract |
|---|---|
| `GET /f/season` | program id, season, cluster, relay pool size, quotas, herald URL (mirrors `/h/season`) |
| `GET /f/relay` | `{feePayer, blockhash, lastValidBlockHeight, programId, quota:{left, resetsAt}}`. The fee payer is drawn at random from the relay pool (**default 150 keys**, as web.md assumes; derived with the §6.3 scheme and `pool_id = "relay"`) |
| `POST /f/relay` | `{tx}`: a player-signed legacy transaction → the shape check (§7.2), `signedBy`, drain guard, co-sign, send → `{ok, signature}`. Refusals use the `refusal()` codes of `cosign.mjs` plus `QuotaExceeded` and `InviteRequired` |
| `POST /f/join` | `{tx, invite?}`: the wallet-signed Join (rent_payer = the relay payer) |
| `POST /f/reveal` | `{ix}` or `{tx}` of a Reveal/RevealPosture (no player signer) → forwarded to the keeper's `/v1/reveal` → `{accepted, track}`. **Reconciles web.md §5.1,** which sends a Reveal through `/gw/f/relay`: the relay recognises the shape and forwards it either way |
| `POST /f/nudge` | `{province, bell}` → the keeper's `/v1/nudge` |
| `GET /f/quota?citizen=` | quota left |
| operator only: `POST /f/operator/invites`, `GET /f/operator/pool` | invites; pool balances |

**Crash safety:**
- Quota and invite state go through `createStateStore`-style atomic writes (or SQLite).
- The ReplayCache is in memory; a restart only means a replayed, already-landed signature fails at send ("already processed").
- The payer pool is derived from the seed, so there is nothing to lose.

---

## 8. The herald: fold and cached read path (Rust, `frontier-herald`)

### 8.1 What it serves

This is the web.md §4 contract, adopted and completed.

| Path | Content | Cache |
|---|---|---|
| `GET /h/season` | the season record (raw bytes + decoded), quicknet info, W, Δ, ruleset hash, rings and ring seeds, tip/CU table, quotas, `head_seq`, latest slot and game time | `max-age=30`, ETag |
| `GET /h/overview/{ring}/{bell}.bin` (+ `latest.bin`) | compact per-province records: site owners (12 × 3 bits), host count per faction, clash flag, `resolved_through`, dormant/free flags | immutable once the ring's bell is resolved; `latest` `max-age=5` |
| `GET /h/province/{P},{Q}/{bell}` (+ `/latest`) | Province bytes after the bell's resolve (or its quiet proof), with slot, `event_head` and seq; that bell's ArrivalSlots and ClashInputs bytes (captured before close) | immutable; `latest` `max-age=2` |
| `GET /h/clash/{P},{Q}/{bell}` | clash report: inputs digest, seed, the outcome digest stored on chain, decoded fighters and fates, **recomputed natively with `resolve_clash`** and checked against the on-chain digest (a mismatch is published as a herald alarm) | immutable |
| `GET /h/bell/{bell}/region/{r}` | BellAnchor (A, sig), S, SeedCache nonces and seed, tombstone, per-province resolved flags, archive after 48 h | immutable once seeded |
| `GET /h/me/{wallet}` | Citizen, ≤ 3 Holdings, hosts, transit records, open slots, SealVerdicts, relay quota left | `no-store`; mainly pushed over WS |
| `GET /h/events?after={seq}` | chronicle and PS2 lines, paged by 500 | pages that are full are immutable |
| `WS /h/ws` | `subscribe {provinces:[…≤64], rings:[…], wallet?, bells:true}` → `{seq, kind, key, slot, head, bytes_b64}`; on a seq gap the client resyncs from files. Heartbeat every 15 s; ≤ 64 subscriptions per socket | — |
| `/frontier/*` | the static web client (web.md §3) | long max-age with hashed names |
| `/gw/*` | proxy to the relay's public listener. It forwards only `Content-Type` and adds `X-Forwarded-For`, as `permutation-server/src/play/proxy.rs` does | — |

### 8.2 The fold

1. **Ingest** through `findex`: RpcPoll on validators, the native feed on the local chain node, Geyser later (M4).
2. **Accounts.** For each landed program transaction, take the program-owned writable accounts. Fetch their bytes with `getMultipleAccounts(minContextSlot = tx slot)`, batched per slot, or take exact post-transaction bytes from the local node's feed.
   - Each capture is stamped with (slot, `event_head`, seq), so a snapshot is always identifiable and verifiable against the chain.
   - On a validator without Geyser, the bytes are "state at slot ≥ s". The head says which version it is. That is exact for immutable per-bell files, because each file keys on the head the resolve produced.
3. **Per-bell closing.** When `resolve(p, b)` lands, write `province/{P},{Q}/{b}` and the clash report. When a region's anchor and seed exist, write `bell/{b}/region/{r}`. When every active province in a ring has resolved b, write `overview/{ring}/{b}.bin`.
4. **Writes are atomic** (temp file + rename, plus a precompressed `.br`/`.gz` sibling). The same archive always gives **byte-identical files** (a determinism test).
5. **WS fan-out:** a broadcast channel per province and per ring, with a bounded queue per socket. A slow client is dropped and resyncs from files.

### 8.3 Scale

- **Viewers [model].** web.md assumes ≈ 13 requests per viewer per bell, so 10,000 viewers ≈ 220 req/s, almost all immutable hits. `latest` polling with jitter adds ≈ 1 req per 5 s per viewer, so 2,000 req/s at 10k.
- **One origin** (axum, files in the page cache) targets ≥ 5,000 req/s and **5,000 concurrent WS** on 4 cores [estimate, to measure]. A CDN in front takes the rest.
- The current v9 server breaks at 500–1,000 viewers (DESIGN §9.3). The herald's load test is part of the M1 exit (§12).
- **Ingest [model].** At 50k players ≈ 1.2M tx/day ≈ 14 tx/s × ~2 accounts, so ≈ 3 batched RPC calls/s. At 1,000 players the load is trivial.
- **Storage [model].** Changed provinces per bell × ~2–4 KB. At 50k: ~1,000 × 3 KB × 4,032 bells ≈ 12 GB/season. Retention: keep every bell with a clash or a capture; compact quiet bells older than 48 h into daily files; everything can be rebuilt from the archive.
- **Crash safety:** a checkpoint of (ingest cursor, `head_seq`) every N slots. A restart re-folds from the checkpoint, and deterministic output makes rewrites harmless.

---

## 9. Bots (`frontier-bots` + `frontier-agents`, Rust)

- **Profiles:** `Arch` and `Profile` are extracted from `frontier-sim/src/model.rs` into `frontier-agents::profile`. A test asserts field-for-field equality with the simulator until frontier-sim depends on the crate.
- **The default mix follows the simulator's default** (Idle, Casual, Daily, Skilled, VerySkilled and Bot at the suite's shares; bots 5%). The join schedule is the simulator's (day-0 spike, then a decay to day 21).
- **Observation:** herald files (overview, provinces in view, `/h/me`) plus the bot's own accounts over RPC. Bots use **the same read path as people**, which is how the herald gets loaded realistically.
- **Policy** (heuristics re-implemented from `sim.rs`: `session`, `economy`, `expand`, `war`, `defend`, `camp_raid`, `pick_stance`, over an `Observation`):
  - FileTicket (pair tickets for friend pairs), then SettleTicket through the keeper;
  - Harvest, Build, Train within the 4-item queue; Muster; Explore;
  - Depart with a seal: Rust `tlock::encrypt` to `tlock_round(b)` (CL-20). The plaintext and salt are journalled. The tip is the default; retreat_ratio comes from the profile;
  - CommitPosture when a hostile arrival is due (departures are public);
  - owner in-bell Reveal for a profile-dependent share, through `/f/reveal`;
  - SettleTransit.
- **Pacing:** sessions follow `day_p` and `sessions`, and actions per session stay within the 30/h bucket.
- **Transport:** each bot has a wallet and a session key derived from `--seed`. Every write goes through the relay (the same path as people). Join goes through `/f/join`.
- **Adversarial personas**, each a small share and on by default in the exit run. The expected outcome, checked by the stack report or the verifier, is in brackets:
  - `zero_tip`: seals with tip 0 [may be routed if no keeper reveals; flagged; PASS];
  - `garbage_seal`: a random 165-byte seal with a valid commitment [ProveBadSeal lands; host destroyed at SettleTransit; verifier agrees];
  - `prefunder`: sends lamports to future ArrivalSlot, PosturePDA, SeedCache and anchor addresses [nothing blocked; absent counts as absent];
  - `squatter`: 100-troop hosts into a faction's slots [displaced by larger arrivals];
  - `late_revealer`: sends a Reveal at A + W and later [refused];
  - `forger`: non-canonical anchor, cache or slot addresses [refused];
  - `spammer`: exhausts its bucket and quota [429 or on-chain refusal, no effect on others];
  - `double_arrival`: two arrivals from one citizen to one province-bell [the second is refused and settled without loss, P7].
- **Budget:** 1,000 bots in one process, a tokio task per bot plus a shared tlock worker pool. Target ≤ 2 cores at 20× [estimate].
- **Determinism:** the decisions depend only on (seed, observation). Runs are not bit-reproducible across processes, because landing order varies. Deterministic A/B tests use the in-process harness (§12).
- **Later (M3):** a public bot SDK in JS on top of `client/src/frontier/`, following the `agents/runner.mjs` pattern. The Shade policy will be one of these agents, with its code hash committed.

---

## 10. Verifier v2 (`frontier-verify`, Rust)

### 10.1 Inputs and trust

- **Inputs:**
  - the program id, season id, RPC URL(s), quicknet `info` (the pinned public key, genesis and period), and the ruleset hash expected;
  - optionally `--archive DIR`, which is only a speed-up.
- **Trust:** whatever the archive says, the verifier reads **final on-chain account states** (`event_head`, seq) at a pinned slot and requires the replayed chains to reach them exactly (K4).
- **Fetching:** without an archive it fetches everything itself: `getSignaturesForAddress` over the program and the season accounts, including failed transactions, then `getTransaction`.

### 10.2 Checks (DESIGN §9.1)

| # | Check | Finding codes (FAIL unless marked) |
|---|---|---|
| V1 | **Entity chains:** `head_n = sha256(head_{n−1} ‖ record_n)`, seq contiguous, final head = on-chain head, for every program-owned account (closed accounts: the last head before close is in the closing record) | `ChainGap`, `HeadMismatch`, `DuplicateEvent`, `UnknownEntity` |
| V2 | **Program and rules:** the program id, the `.so` hash per slot range (upgrade history via the ProgramData account), and the ruleset hash in the genesis record | `ProgramMismatch`, `RulesetMismatch` |
| V3 | **Randomness:** every anchor and cache signature re-verified with blstrs against the pinned quicknet key; exactly one anchor per (bell, region), at the canonical address, including after archiving (tombstones); S(b, r) = the first round ≥ A + W + Δ; genesis and ring seeds are their rule-fixed rounds (CL-19/24) | `BeaconSigInvalid`, `DuplicateAnchor`, `NonCanonicalAddress`, `SeedRoundRule`, `GenesisSeedRule`, `RingSeedRule` |
| V4 | **Windows:** every landed Reveal and RevealPosture has Clock < A + W of THE anchor and comes before the first gather (CL-23) | `RevealAfterClose` |
| V5 | **Seal audit:** every Depart and CommitPosture seal is opened with drand's **`tlock` crate** (independent of the on-chain opener) + the PS-KS body + the commitment. Revealed plaintext = opened plaintext. Every SealVerdict agrees (a verdict exists ⇒ the seal is bad by stock decryption; a valid seal never gets one) | `RevealCommitMismatch`, `VerdictDisagreesWithTlock`; **liveness (PASS with a warning):** `ValidSealUnrevealed` (with the attempted signatures from failed transactions and the keeper journal if given), `BadSealUnproven`, `RevealNearClose` |
| V6 | **Quotas:** for every (province, bell, faction) the final slot set = the 4 largest revealed arrivals by (mass, `slot_key`), one per citizen; departure mass = the value recorded at Depart | `QuotaSetMismatch`, `TransitMassMismatch` |
| V7 | **Per-entity replay** (parallel): each province-bell is re-run with `permutation_rules::frontier::clash::resolve_clash` from the logged inputs (reveals, postures, origin post-clash values, garrison mirror, seed) → the outcome digest and the post-state fields must equal the chain's. Holdings, hosts and citizens are re-run through the kernel's lazy accrual, queue and bucket functions. Transits: settle, rout, bounce, destroy | `ClashReplayMismatch`, `HoldingReplayMismatch`, `TransitOutcomeMismatch` |
| V8 | **Lag invariance witness:** every resolve used origin values from the origin's departure-bell resolve (never later ones) | `OriginValueMismatch` |
| V9 | **Accounts and addresses:** every keyed account at its canonical with-seed or PDA address; pre-funded addresses never blocked an init (from the failed-transaction list, informational) | `NonCanonicalAddress` |
| V10 | **Report:** `report.json` (schema below) + `report.md`. Exit 0 PASS, 1 FAIL, 2 cannot verify (data missing). **Fails closed** (K2): any missing transaction or undecodable record is FAIL, never a skip | — |

```json
{"verdict":"PASS|FAIL","season":…,"program":…,"slot_range":[a,b],"entities":n,"bells":n,
 "counts":{"tx":n,"failed_tx":n,"seals":n,"reveals":n,"verdicts":n,"clashes":n},
 "findings":[{"code":"ClashReplayMismatch","severity":"fail|warn","entity":"…","bell":n,"signature":"…","detail":"…"}],
 "liveness":{"valid_unrevealed":[…],"reveals_near_close":n,"max_anchor_delay_s":x}}
```

### 10.3 Tamper tests (must FAIL with the named code)

They run on recorded fixtures: a 1-game-day, 100-bot in-process season, plus the exit season's archive.

- **T1** drop one Depart → `ChainGap`/`HeadMismatch`.
- **T2** flip a Reveal plaintext byte → `RevealCommitMismatch`.
- **T3** shift an anchor's A → `SeedRoundRule`.
- **T4** inject a second anchor for a (bell, region) → `DuplicateAnchor`.
- **T5** swap a cache signature for another round's → `SeedRoundRule`/`BeaconSigInvalid`.
- **T6** `set_account` on a Province after a resolve (a rogue write on the local node) → `ClashReplayMismatch`/`HeadMismatch`.
- **T7** add a SealVerdict for a valid seal → `VerdictDisagreesWithTlock`.
- **T8** change which slot a displacement hit → `QuotaSetMismatch`.
- **T9** alter a departure mass → `TransitMassMismatch`.
- **T10** wrong quicknet key → `BeaconSigInvalid`.
- **T11** wrong ruleset hash → `RulesetMismatch`.
- **T12** truncate the last game day → `HeadMismatch`.
- **T13** move a Reveal's slot past A + W → `RevealAfterClose`.
- **T14** duplicate a transaction → `DuplicateEvent`.
- **T15** use origin values from a later bell → `OriginValueMismatch`.
- **T16** wrong genesis round → `GenesisSeedRule`.

**Checks of the checks:** with each check disabled in turn (a cargo feature `mutate-<check>`), its tamper must PASS, and the test asserts that it does. This mirrors the SP-V2 `t9` mutation discipline.

**Honest-but-adverse fixtures that must PASS:**
- a keeper crash mid-bell;
- a held anchor (contention emulator);
- a lagging origin;
- displacement;
- a bad seal proven;
- a zero-tip rout;
- pre-funded addresses;
- archived anchors and tombstones;
- a quota-refused arrival settled without loss.

### 10.4 Performance [model]

- **7-day, 1,000-bot season:** ≈ 770k transactions; ≈ 60k seals × 0.92 ms; ≈ 32k beacon verifications. **< 10 min on 8 threads** (target).
- **28-day, 50k season:** ≈ 34M transactions and ≈ 12M seals (≈ 3.1 CPU-hours for seals). **< 6 h on 16 cores** (target, M4).

---

## 11. Local stack and the 7-day accelerated season

### 11.1 Two modes

| Mode | Chain | Beacons | Time | Use |
|---|---|---|---|---|
| **A: accelerated** (exit run) | `frontier-localnet`: LiteSVM 0.16 with the mainnet feature set (incl. the SIMD-0388 BLS12-381 syscalls), the **pinned SBPF v2 release `.so`**, rent 5,080 lamports/byte, a 64-lock limit, blocks of ≤ 100M CU with ≤ 40M per account (enforced by the block builder) | `drand-replay` over **real historical quicknet rounds** | Clock.unix_timestamp = G0 + (wall − wall0) × scale, where G0 is a past date (e.g. 2026-08-01T00:00Z) so every round the season needs already exists. **Default scale 20×** (7 days in 8.4 h; the 600-s bell and W take 30 s of wall time). 60–100× for CI smoke | the 7-day, 1,000-bot season, crash and chaos tests, tamper fixtures |
| **R: real time** | Agave ≥ 4.0 `solana-test-validator` (**needs the owner's approval to install**: a download from Anza's releases) | live quicknet (`api.drand.sh`, …, read-only) | 1× | a 24-h, 1,000-bot soak; relay and herald against a real RPC; the keeper against real slot timing |

**Why Mode A is enough for the "7-day local season" exit, and where it falls short:**
- It runs the exact release binary with real quicknet signatures and seals, so the verifier's PASS covers the production crypto path.
- It cannot show real leader scheduling, the fee market or real network timing. Mode R and the M4 soak cover those.
- **Open question Q1:** does the owner accept Mode A as the exit run, with Mode R as a complement?

### 11.2 `frontier-localnet` and `drand-replay`

- **RPC subset.** Enough for `@solana/web3.js` 1.99, `send.mjs` (which confirms over HTTP only) and `fclient`:
  - `getLatestBlockhash`, `sendTransaction`, `simulateTransaction` (with `accounts` post-state), `getAccountInfo`, `getMultipleAccounts`, `getSignatureStatuses`, `getTransaction`, `getSignaturesForAddress`;
  - `getSlot`, `getBlockHeight`, `getBlockTime`, `getBalance`, `getMinimumBalanceForRentExemption`, `getProgramAccounts` (memcmp filters), `requestAirdrop`, `getEpochInfo`, `getVersion`, `getHealth`.
  - WS: `slotSubscribe`, `signatureSubscribe` and `logsSubscribe(mentions)`. This is optional: every component in this design also works by polling.
- **Extensions** (`frontier_*`, loopback only):
  - `frontier_feed(after)`: an ordered transaction feed with exact post-transaction account bytes;
  - `frontier_pause` / `frontier_resume` / `frontier_setScale`;
  - `frontier_snapshot` (all accounts + clock) and `frontier_restore`;
  - `frontier_hold(keys, priority, slots)`, the **contention emulator**: a transaction that writes a held key is deferred unless its priority exceeds the hold's. The priority is computed with the §4.2 formula from its compute-budget instructions.
- **Block builder.** Per game slot (400 ms / scale of wall time), collect transactions, order them by priority as SP-FEE's scheduler does, and execute them in sequence under the block and account caps. Each block is recorded (signatures, logs, CU).
  - Throughput: LiteSVM on this machine ran **≈ 60–90M CU/s single-threaded** [measured, §17].
  - Load [model]: a 1,000-player game day is ≈ 4.2G CU (72k player tx × 20k + 4.6k beacon posts × 333k + about 7k resolves × ~150k + gathers), so ≈ 50–70 s of execution per game day.
  - So at 20× (72 wall-minutes per game day) the chain is about 1–2% busy. 100× is still well under 10%.
- **Crash and resume:** a transaction write-ahead log plus a snapshot every 36 game bells. Restore = snapshot + WAL replay, with the recorded clock values, so the result is deterministic.
- **drand-replay** serves the drand HTTP API subset: `/{chain}/info`, `/{chain}/public/latest`, `/{chain}/public/{round}`.
  - It serves a round only when its time ≤ game_now (read from the localnet Clock) + a configured delay (0.8–2.0 s of game time, as measured for quicknet); otherwise it answers 425.
  - Rounds come from a local archive, prefetched only for the rounds the season will need, or fetched lazily from `api.drand.sh` into the cache. Every round is verified against the pinned key.
  - Rounds needed: ≈ 1,008 T(b) + ≈ 16k S(b, r) + BeaconLog posts ≈ 20k rounds for 7 days [model], at 20 req/s ≈ 17 min to prefetch.
  - **Fetching historical beacons is a read-only download of public data. Q2 asks the owner to confirm it.** The fallback is a `beacon-local` program feature that pins a local test key; the verifier would then run with that key, and the release `.so` would not be the one tested.
- **Caveat:** in Mode A the rounds are already public in reality, so seal secrecy is only enforced by `drand-replay`'s gate for the components pointed at it. That is a property of the test setup, not of the protocol.

### 11.3 Ports (none on the reserved list 4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191; checked free on 2026-09-27)

| Service | Port |
|---|---|
| frontier-localnet JSON-RPC / WS | 38810 / 38811 |
| drand-replay | 38820 |
| relay operator (loopback) / public | 38830 / 38833 |
| herald HTTP + WS (serves `/frontier/`, `/h/*`, `/gw/*`) | 38840 |
| keeper A (operator roles) `/v1`, `/metrics` | 38850 |
| keeper B (public profile: reveal, settle, prove) | 38851 |
| bots control / metrics | 38870 |
| viewer load generator (stats) | 38875 |
| Mode R: Agave test-validator RPC / pubsub (RPC + 1) / gossip / faucet / dynamic range (≥ 25 ports) | 38880 / 38881 / 38885 / 38886 / 38890–38930 |

`frontier-stack` refuses to start if any configured port is reserved or already bound.

### 11.4 `frontier-stack`

```
frontier-stack up   --mode accel --scale 20 --days 7 --bots 1000 --run-id s7a [--chaos] [--viewers 5000]
frontier-stack up   --mode realtime --hours 24 --bots 1000          # Mode R (after approval)
frontier-stack verify --run-id s7a        # frontier-verify over the run's RPC + archive; then the tamper suite on a copy
frontier-stack report --run-id s7a        # run report (below)
frontier-stack down --run-id s7a
```

- **Start order:**
  1. localnet → load `.so`;
  2. drand-replay;
  3. AnnounceSeason / CreateSeason → ConsumeGenesisSeed (keeper A);
  4. relay (airdrops the pool) and keepers A and B (airdrop 150 payers each);
  5. herald;
  6. bots on their join schedule;
  7. the viewer generator for one game-day window.
- **Output:** everything goes under `.local/frontier/<run-id>/` inside `frontier-node/` (git-ignored): logs, archive, herald `out/`, journals, snapshots.
- **Chaos** (`--chaos`): `kill -9` a random component every 2–6 game hours; restart after 0–60 game seconds. Localnet restarts use snapshot + WAL.
- **Adversary schedule:** `frontier_hold` scripts copied from the SP-FEE drill shapes:
  - hold one province's slots through a close, against keeper bids below and above the hold;
  - hold one region's anchor;
  - hold 20 known keeper payers;
  - hold the origin Province and the origin region's anchor past the destination's close. **This is the program-level lag gate: the result must equal the unheld run** (§12, run in-process for exact A/B).
- **Run report** (`report.md` + `report.json`):
  - **CU:** the max and p99 CU per instruction kind, bytes, locks and heap, **against the budget table** (the M1 exit "every instruction under budget", from real play plus the adversarial personas);
  - **the full Reveal CU distribution** (the C4 input; it feeds `c4_model.py` v3, CL-26/30);
  - keeper latencies and spend;
  - herald latency and load-test results;
  - bot outcome statistics against `frontier-sim` for the same agents (clashes per bell, returns by archetype; order-of-magnitude comparison);
  - liveness findings;
  - the verifier verdict and the tamper suite results.

---

## 12. Tests

- **`fclient`:**
  - ABI round trip against the real `.so` in LiteSVM: build an instruction, execute, decode the account;
  - with-seed addresses equal the program's and CL-21's vectors;
  - `fees`: p_tip = 0.433 at a 16k limit, tip 10,006 lamports [model], cost formula against SP-FEE calibration rows;
  - beacon: every fixture quicknet beacon from SP-V2 `beacons/` verifies off chain, and its hints are accepted on chain;
  - seal interop both ways: tlock-js compact-16 → Rust open; Rust seal → JS open (the S-TLOCK q4 vector); commitment domain vectors;
  - `GameClock` scale detection.
- **JS SDK** (`permutation-gateway/test/frontier-*.test.mjs`):
  - codec, address, seal, fee and shape vectors from `frontier-vectors.json`;
  - relay shape allowlist (every allowed shape passes; wrong program, extra instruction, non-pool payer, missing signature, CU price above the cap, and oversize are refused);
  - quota accounting, invites, the drain guard (a simulated payer delta above the allowance is refused), ReplayCache 409, FundsGuard 503;
  - `/f/reveal` forwarding;
  - `sync-web-sdk --check` freshness.
- **keeper-core** (in process, virtual time):
  - the whole bell pipeline for 1, 16 and 64 provinces;
  - reveal ordering by mass (at most 4 slot creations per group);
  - `SlotChanged` retry;
  - never sending after `A + W − ε`;
  - ProveBadSeal for each bad-seal class (SP-V2 `t6` cases);
  - quota-refused settlement (P7);
  - gather dependency on origins;
  - archive at 48 h and tombstones respected;
  - ring opening;
  - **escalation schedule** (×2 per slot, caps by class, a new payer per version);
  - **payer uniformity** (χ², ≥ 150 distinct, no fixed payer in any critical transaction);
  - **contention:** a held slot with the keeper cap above the hold lands before the close; below it, it is routed and flagged;
  - nonce switch on a held cache;
  - combined-anchor fallback;
  - **crash injection** at every journal point (identical outcome digests, bounded extra spend);
  - **the program-level lag gate** (hold the origin Province and the origin anchor past the destination's close: the outcome digests equal the unheld run);
  - **"no Reveal after the close"** as a property test over random anchor delays;
  - duplicate keepers (2 or 3 keepers racing: identical results, bounded waste).
- **herald:**
  - fold determinism (the same archive gives byte-identical files);
  - snapshot head = account head at random slots;
  - clash report digest = on-chain digest;
  - restart idempotency;
  - WS gap resync;
  - **load:** a Rust viewer generator with 5,000 viewers (4,000 polling with jitter, 1,000 WS) for one game day; p99 latency and error rate recorded.
- **verifier:** PASS fixtures, tamper T1–T16, checks of the checks, the honest-but-adverse fixtures, and CLI exit codes.
- **bots:** profile equality with `frontier-sim`; policy determinism given (seed, observation); every adversarial persona's expected outcome asserted in the stack report.
- **localnet:** RPC conformance against web3.js calls used by the relay; block caps (per account ≤ 40M, block ≤ 100M); priority ordering; snapshot + WAL restore gives an identical state hash; the clock scale.
- **CI:**
  - unit and in-process tests on every push (≤ 15 min target);
  - nightly: a 1-game-day, 100-bot Mode A run at 100× with verify and tamper;
  - the 7-day, 1,000-bot run is manual or weekly (8.4 h).

---

## 13. Budgets

| Item | Budget | Tag |
|---|---|---|
| Round published → anchor landed (peacetime) | p99 ≤ 5 s | [design] |
| S round → first SeedCache | p99 ≤ 5 s | [design] |
| T(b) anchor → all valid reveals landed (peacetime, 1,000 players) | p99 ≤ 30 s (window 600 s) | [design] |
| Seal decryption | ≤ 1 ms/seal/thread (0.92 measured); 3k seals ≤ 0.5 s on 8 threads | [measured]/[model] |
| Close → resolve landed (active province) | p99 ≤ 60 s | [design] |
| Keeper memory (50k) | ≤ 1 GB | [estimate] |
| Keeper payer float | ≈ 10 SOL at 50k, ≈ 0.75 SOL at 1k, plus beacon posts ≈ 0.023 SOL/day base fees | [model] |
| Keeper tx per bell (1,000 players) | ≈ 250 (32 beacon posts, ~60 reveals, ~20 posture reveals, ~40 gathers, ~20 resolves, ~60 settles, folds) | [estimate] |
| Relay co-sign latency (excluding confirmation) | p99 ≤ 300 ms | [design] |
| Relay sponsorship | 40 tx/game-day for days 0–6, then 20; ≤ 24 Depart escrows/day; rent ≈ 7.2M lamports per player (refundable) | [design, D4] |
| Herald ingest → file/WS | p99 ≤ 2 s after confirmation | [design] |
| Herald origin | ≥ 5,000 req/s static, 5,000 WS, one 4-core box | [estimate, to measure] |
| Herald storage (50k, 28 days) | ≤ 20 GB with retention | [model] |
| Verifier | 7-day, 1k-bot season < 10 min on 8 threads | [model] |
| Bots | 1,000 bots ≤ 2 cores at 20× | [estimate] |
| Localnet | ≥ 50M CU/s (measured ≈ 60–90M); chain ≤ 5% busy at 20× | [measured]/[model] |
| Exit run wall time | 7 game days = 8.4 h at 20× | [design] |

---

## 14. Plan (M1 weeks; 2 engineers on the off-chain side [estimate])

| Week | Off-chain work |
|---|---|
| 1 (with the M0 close) | Agree P1–P10 with the program area; `frontier-abi` skeleton; `fclient` fees, addresses (CL-21), beacon (port SP-V2 hints), seal (Rust + JS vectors), clock (CL-19/20); localnet MVP (RPC subset, scaled clock, LiteSVM); drand-replay MVP |
| 2 | `findex` ingest and archive; keeper beacon duties (anchor combined + fallback, seed caches, genesis); relay skeleton (shapes, quotas, invites, rent fronting, `/f/reveal` link); JS SDK codec as instructions land |
| 3 | Keeper reveal pipeline (decrypt, mass ordering, slot index, escalation, payer pool), ProveBadSeal, gather/resolve DAG, settle; journal and crash safety; herald fold v0 (province, bell and region files) |
| 4 | Bots v0 (join, economy, marches, postures), a 100-bot one-game-day smoke at 100×; verifier v0 (V1–V5) |
| 5 | Verifier V6–V10 and tamper T1–T16 with checks of the checks; herald WS, overview `.bin`, proxy, load generator; keeper archive, rings, catch-up; contention emulator; chaos |
| 6 | **First 7-day, 1,000-bot run (Mode A 20×)**; fixes; Reveal CU distribution → `c4_model` v3 input; 5,000-viewer load test |
| 7 | Hardening; Mode R 24-h soak (if Q1 is approved); "Run a keeper" docs; playtest runbook (devnet config only; **no devnet step without the owner's approval**) |
| 8 | **Exit run:** 7 days, 1,000 bots, chaos on, verifier PASS, tamper FAIL, report → owner decision on the 50–200-person devnet playtest |

---

## 15. Risks

| # | Risk | Mitigation |
|---|---|---|
| OR1 | **The local chain node differs from a validator** (scheduler, clock, RPC edge cases) and hides bugs | Mode R 24-h soak; the RPC conformance suite; keep production-path code (web3.js, `send.mjs`) unchanged; M4 soak |
| OR2 | **ABI churn** while the program is built in parallel | `frontier-abi` + generated vectors in both languages; a CI freshness check (as `sync-web-sdk --check`) |
| OR3 | **The log contract is not enough for per-entity replay** (P2) | Agree P2 in week 1; the verifier's V7 is prototyped against the in-process harness before the program freezes its events |
| OR4 | Historical drand fetch is not allowed or not available | A pre-fetched archive; a `beacon-local` feature fallback (weaker: a different key) |
| OR5 | **Keeper economics at a tip-level start give zero margin** (p_start = p_tip) | `--peace-start`, Q3; CL-30 pool sizing |
| OR6 | The defence pool and ClaimDefence are M2 in the design, but M1 needs escalation | M1 escalates from the keeper's own budget up to the caps and journals P6 evidence; claims when the pool lands (Q4) |
| OR7 | New dependencies (tokio, axum, rusqlite bundled, blstrs, `tlock` 0.0.10, a young 0.0.x crate) | Pin with `=`, review, and cross-check tlock against tlock-js vectors; the verifier uses `tlock`, the program uses its own opener (independence is intended) |
| OR8 | Devnet playtest RPC limits (public devnet RPC throttles `getTransaction` paging) and devnet SOL for pools (keeper ~1.5 SOL, relay ~2 SOL, ProvinceFund ~3.5 SOL) | A private devnet RPC and funding plan in the runbook; **owner approval required** |
| OR9 | Accelerated time hides latency bugs (a 30-s wall window) | Also run at 1× (Mode R) and at 60–100× (stress); all rule times come from `GameClock` |
| OR10 | The quiet-bell proof is not chosen (P9), so the keeper's catch-up load is unknown | Duties are abstracted; the run report measures catch-up transactions per idle province-day |
| OR11 | Herald snapshot exactness without Geyser | Snapshots are keyed by `event_head`; clash reports are recomputed; Geyser in M4 |
| OR12 | **Scope** (≈ 7 binaries, 2 languages) | The in-process harness first; bots and herald on shared libraries; cut list: WS subscriptions (polling only), the Mode R soak, chaos |
| OR13 | A relay payer (known account) is locked, which delays every sponsored player action | A pool of 150 relay payers drawn at random; player actions are not window-closing; owner reveals go through the keeper |

---

## 16. Open questions and owner decisions

1. **Q1:** accept the **accelerated Mode A run** (LiteSVM-backed local chain node, 20×, real quicknet history) as the M1 exit's "7-day local season", with a real-time 24-h soak as a complement? **Approve installing Agave ≥ 4.0** (download) for Mode R?
2. **Q2:** approve **read-only fetching of historical quicknet rounds** from drand's public HTTP API for `drand-replay`? Otherwise use the `beacon-local` build fallback.
3. **Q3:** the keeper's starting bid. The design says the tip level, which leaves zero margin. Allow a lower peacetime start (e.g. 0.25 × p_tip) with ×2 per slot?
4. **Q4:** is the defence pool (and ClaimDefence) on chain in M1, or is M1 escalation from the keeper's own budget with evidence journalled for M2? This also covers P6's evidence fields.
5. **Q5 (program area):** P4 beneficiary, P5 rent_payer/funder signers, P7 settlement of quota-refused arrivals, P2 log contract.
6. **Q6:** relay sponsorship values for the playtest (D4: 40/20 tx per day), invite codes, and the escrow budget per citizen.
7. **Q7:** where the herald's files and the relay live for the devnet playtest (a CDN or object storage and a tunnel are operator infrastructure) — a decision for the playtest runbook, with owner approval.
8. **Q8:** extract `Arch`/`Profile` from `frontier-sim` into `frontier-agents` now (frontier-sim then depends on it, with its digest unchanged), or copy them with an equality test until M3?
9. **Q9 (reconcile with web.md):** the owner-Reveal route (`/gw/f/reveal` forwarding to the keeper, while `/gw/f/relay` also accepts the shape) and the relay pool size (150).

---

## 17. Lab evidence from this planning pass

**Lab:** `scratchpad/frontier/m1/lab/offchain-svmspeed/`. It is a copy of `m0b/spikes/SP-V2` (without `target/`) with its own `CARGO_TARGET_DIR`. The original lab was not touched.

**Question:** can a LiteSVM-backed local chain node carry a 1,000-bot season at 20×?

**Run:** SP-V2 host tests on the pinned v2 `.so`, `SPV2_ARCH=v2`, on an Apple M4 Max (arm64).

| Test | Work | Wall time | Rate |
|---|---|---|---|
| `t1_post_anchor_cu` | 64 quicknet verifications (≈ 327–336k CU each, ≈ 21M CU) + 32 no-ops + 32 season setups | **0.36 s** | **≈ 60M CU/s** [measured] |
| `t5_gather_and_resolve_worst_case` | 2 builds × 40 adversarial fills × (full gather + hybrid): ≈ 87M CU of gathers and resolves + ≈ 20M CU of reveal slot creation + native kernel cross-checks | **1.18 s** | **≈ 90M CU/s** [measured] |

The numbers reproduced SP-V2's own: worst full-gather resolve 540,891 CU, worst gather 30,323 CU, reveal slot creation 6,603 CU, heap 25,832 B.

**Conclusion [model]:** a 1,000-player game day (≈ 4.2G CU) needs about 50–70 s of LiteSVM execution. At 20× that is about 1–2% of wall time, so the local chain node is not the bottleneck; the bots, keeper and herald are.

**Nothing else was run.** No server was started and no port was opened. No devnet or mainnet transaction was sent. No commit was made.
