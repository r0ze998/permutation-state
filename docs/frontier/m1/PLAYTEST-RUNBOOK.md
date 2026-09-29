# M1 private playtest runbook (devnet configuration only)

> **Status: not approved, not run.** The private devnet playtest (50–200 people, no money) is owner decision **O-M1-18, not approved** (DECISIONS part A). This runbook is the configuration and the order of steps the playtest would use; **no step of it has been executed**, no devnet account was created, funded or read, and nothing here authorises one. **Every devnet step — deploying, funding or airdropping, announcing a season, starting a keeper, relay or herald against devnet, even a read-only probe of a devnet RPC — needs its own owner approval at the time**, and so does any push (contract §13.8, DESIGN §12). Written by unit W6-E (contract v1.9 §11, wave 6).

Related: [`RUN-A-KEEPER.md`](RUN-A-KEEPER.md) (keeper operation), [`M1-CONTRACT.md`](M1-CONTRACT.md) §8 (services), §10.3 (ports), §13.8 (after the exit), [`../DESIGN.md`](../DESIGN.md) §22 (the final C4 model, D18 and payer band).

---

## 0. Preconditions (all required before step 1)

| # | Precondition | Where it stands (2026-09-28) |
|---|---|---|
| P1 | The M1 exit passed (Gate W7: the exit season on the release `.so` with real rounds, verify PASS, 22 tamper classes FAIL, web smoke) | wave 7; the 7-day real-round season is started by the main session after wave 6 |
| P2 | Owner approves O-M1-18: the playtest itself, its size, the sponsorship values (relay quotas 40 transactions per citizen per game day for days 0–6, then 20), invites, hosting for the herald and relay, and who operates the keepers | not approved |
| P3 | Owner approves each devnet step below as it comes (§3 marks them **[approval]**) | none given |
| P4 | The gaps of §2 closed, merged and gated | open |
| P5 | Test SOL for §5's budget obtained by a route the owner approves (devnet faucets are rate-limited [unverified]) | open |

## 1. What runs

| Component | Binary / script | Where | Notes |
|---|---|---|---|
| Program | `permutation-frontier` release `.so` built by `scripts/build-frontier.sh` (SBPF v2; refuses the `oracle`, `trace` and `test-beacon` markers); sha256 recorded | devnet, new program id | deploy with `--max-len = round_up(1.25 × .so, 4 KiB)` (the script prints it); the **upgrade authority** is the only key that can `AnnounceSeason` (I-51): keep it offline between steps |
| Season | preset **`M1_PLAYTEST`** = `M1_LOCAL_7D` with `join_gate` = the relay's gate public key (`frontier_abi::presets::m1_playtest(gate)`; the unset constant `PLAYTEST_GATE_UNSET` is not a valid key, so a season created without the operator's gate key refuses every Join) | on chain | 7 real days (`end_bell` 1,008), joins until bell 756 (day 5.25), genesis ring 3, `r_max` 16, reveal window 600 s, `tip_min` 14,668 lamports |
| drand | quicknet, live, over HTTPS | public endpoints | the keeper and herald need a TLS path (§2 G1) |
| Keeper A (operator roles) | `frontier-keeper` | operator host | every role (§2 of the keeper guide); ≥ 150 reveal payers, ≥ 32 delay payers, ≥ 4 funders |
| Keeper B (public profile) | `frontier-keeper` | a second host or operator | roles `reveal`, `settle-departure`, `settle`, `claims`; its own master seed |
| Relay | `node permutation-gateway/src/frontier/server.mjs` | operator host, **loopback listeners only** | 150-key payer pool (`pool_id = "relay"`), invites, join-gate co-signing, quotas, drain guard, `/f/reveal` → keeper A |
| Herald | `frontier-herald --source rpc` | operator host behind a TLS reverse proxy | serves `/frontier/*` (the web client), `/h/*`, `WS /h/ws` and `/gw/*` (the relay's public routes); the only public origin |
| Web client | `permutation-server/web/frontier/` (static) | served by the herald | JA/EN; the page reads `/h/season` for the chain info and presets |

## 2. Gaps that block a devnet run (P4)

Each is a code change for a later unit (none is W6-E's), listed so the owner can see the size of the work. The local stack does not need any of them.

| # | Gap | Evidence | Suggested fix |
|---|---|---|---|
| G1 | **No TLS client in `frontier-node`.** `fclient::http` speaks plain `http://` only; the keeper, herald and bots cannot reach `https://api.devnet.solana.com` or the public drand endpoints. (`drand-replay prefetch` shells out to `curl` for HTTPS.) | `fclient/src/http.rs` header; W1-F R4 | an integrator dependency request (`rustls`-based client), or a loopback TLS-terminating forward proxy per host (configuration only, but every request then trusts that proxy) |
| G2 | **The keeper reads the transaction feed through `frontier_feed`**, a `frontier-localnet` extension (`RpcPort::localnet` in `keeper/src/main.rs`); a public RPC has no such call | `fclient/src/rpc.rs` `feed()` returns `Unsupported` for a public RPC | reuse `findex::RpcPoll` (the herald's `--source rpc` path) in the keeper |
| G3 | **No live drand source for the keeper**: `drand-replay` serves an archive or the test key; the keeper's `drand` URLs must answer `/{chain}/public/{round}` over plain HTTP | `drand-replay` modes | G1's TLS client pointed at the public quicknet endpoints, with the keeper's blstrs verification (it already verifies every round) |
| G4 | **No operator tool** for AnnounceSeason, CreateSeason, InitBeaconLogs and the six InitShards against a public RPC: these steps exist only inside `frontier-stack` (`stack/src/setup.rs`) against the local node | `setup.rs` | a small `frontier-operator` binary (or `frontier-stack operator --rpc`), signing with the upgrade authority from a file or hardware wallet |
| G5 | **No address listing** for keeper and relay payers and funders | keeper CLI | `frontier-keeper --print-addresses`; the relay's `GET /f/operator/pool` already lists its pool |
| G6 | **BLS12-381 syscalls on devnet** are assumed (DESIGN §8.5 cites them for mainnet, Agave 4.0) but were never checked on devnet; without them PostAnchor, PostSeed, ConsumeGenesisSeed and ConsumeRingSeed fail and the season cannot start | M1 ran quicknet verification only in LiteSVM and `frontier-localnet` | a read-only feature-gate check on devnet **[approval]** before anything else |
| G7 | **Rent on devnet** may differ from the 5,080 lamports per byte the model uses (SIMD-0437 on mainnet) | DESIGN §8.2 | read `getMinimumBalanceForRentExemption` during G6's check **[approval]**; §5 gives both rates |
| G8 | **Payer floors for a small season:** the keeper's defaults are sized for 50k wallets (reveal R99 4,000 → 0.215 SOL per payer; delay floor 0.5 SOL per payer) | `fclient::payers` | configuration only: `r99_reveals = 150`, `delay_floor = 50000000` (§4) |

## 3. Steps (each **[approval]** needs the owner's OK at the time)

**Phase A — preparation (local, no devnet contact)**

1. Build and record: `scripts/build-frontier.sh` (release, twice-identical with `--twice`), record the `.so` sha256 and `max_len`; `cargo build --locked --release --workspace` in `frontier-node`; `npm ci` in `permutation-gateway`; `scripts/build-wasm.sh --check`.
2. Keys (offline, on the operator host, mode 0600): program keypair; **upgrade authority**; relay master seed (`--master-seed-file`), relay invite secret (`--invite-secret-file`), **relay gate key** (`--gate-key-file`; its public key goes into `join_gate`); keeper A and keeper B master seeds (`frontier-keeper --init-seed`), each keeper's beneficiary key; the operator token (`FRONTIER_OPERATOR_TOKEN`) and keeper API tokens.
3. Season parameters: `m1_playtest(<gate pubkey>)`; **`quicknet_pk_hash` stays quicknet's** (never the test key's); decide `pfund_initial` and `dpool_initial` for the playtest size (§5 has a smaller option; changing a preset value is a `frontier-abi` change, not configuration). Compute `params_hash` and `t_create_min` (≥ 24 h after the announcement).
4. Rehearse the whole sequence on the local stack with the same release `.so`, the same parameters and the archive beacon (`frontier-stack up --config <a copy of configs/w6-s7.toml with M1_PLAYTEST>`), including a forced keeper restart and an Abort drill on a throw-away season id.

**Phase B — devnet setup**

5. **[approval]** Read-only devnet checks: BLS12-381 syscall feature gates (G6), rent per byte (G7), the current `solana-test-validator`-equivalent limits used by the cost model (per-account 40M, block 100M).
6. **[approval]** Fund the deployer and authority; deploy the program with the recorded `--max-len`; verify the on-chain ProgramData hash equals the recorded sha256.
7. **[approval]** `AnnounceSeason(id, params_hash, t_create_min, bond = 1 SOL)` signed by the upgrade authority. The id is single-use; a pre-join abort after the genesis round is public **burns** the bond (I-09).
8. **[approval]** Fund keeper A's and keeper B's funders, the relay pool (§5). Start keeper A (`beacon` role first, so ConsumeGenesisSeed lands), keeper B, the relay and the herald (§4). Check `/v1/status`: reveal effective N ≥ 150 for both keepers.
9. **[approval]** After `t_create_min`: `CreateSeason` (authority; funds 6 ProvinceFunds and the DefencePool), `InitBeaconLogs`, `InitShards` × 6. Keeper A posts the genesis seed (`ConsumeGenesisSeed`) and opens rings 0..3 and their provinces; `genesis_ts = round_time(genesis_round) + 600`.
10. **[approval]** Issue invites (`POST /f/operator/invites`, operator token) and send them to the invited players out of band.

**Phase C — play (7 days)**

11. Players open the herald's URL, connect a wallet, redeem an invite (Join co-signed by the relay's gate key), file a site ticket, and follow the onboarding card (first holding ≈ 11–21 minutes after filing, first clash report ≈ 31–41 minutes after the first march departs [model, DESIGN §2.3]).
12. Watch (§6). Nightly: run `frontier-verify` over devnet (`--rpc`) into a dated report; any FAIL stops new invites until triaged.
13. The season ends by rule at bell 1,008: anyone sends `EndSeason`; keepers drain (every province resolved or skipped, every transit settled, cohorts closed).

**Phase D — wind-down**

14. **[approval]** After `end + 72 h`: CloseClashInputs/ArrivalDay/ArrivalSlot (keepers), CloseHolding and CloseCitizen (anyone; rent to each account's rent payer, escrows to ticket funders), CloseProvince, `CloseSeason` parts 0–10 (RingSeeds → payer, AnchorArchives → `rent_to`, DefenceClaims → beneficiary; ≤ 10 pairs per transaction for parts 8 and 10, ≤ 2 for part 9) — the Season shrinks to a 128-B tombstone.
15. Final verifier run and report; sweep the keepers' and relay's pools back to the operator; archive the herald's data directory, the keepers' journals and the verifier report.

## 4. Configuration

**Relay** (flags over `FRONTIER_*` environment variables over defaults; `permutation-gateway/src/frontier/config.mjs`):

```sh
FRONTIER_OPERATOR_TOKEN=<secret> node permutation-gateway/src/frontier/server.mjs \
  --rpc https://api.devnet.solana.com \            # Node's fetch speaks TLS; G1 is the Rust side
  --program <PROGRAM_ID> --season <ID> \
  --port 41030 --public-port 41033 \               # loopback only; the herald proxies /gw/*
  --herald http://127.0.0.1:41040 \
  --keeper http://127.0.0.1:41050 --keeper-token-file keeper-a/keeper.token \
  --pool-size 150 --master-seed-file relay/relay-master.seed \
  --gate-key-file relay/gate.key --invite-secret-file relay/invite.secret \
  --state-file relay/relay-state.json --min-pool-sol 1
```

Quotas (contract §8.3, defaults): 40 sponsored transactions per citizen per game day for days 0–6, then 20; burst 60; per-IP limits on `/f/relay` (40 burst, 2/s), `/f/join` (10, 0.2/s), `/f/reveal` (40, 2/s); sponsored Depart tips only at 14,668 / 22,002 / 29,336 lamports; `SetComputeUnitPrice` 0 on every sponsored shape. `X-Forwarded-For` is trusted only from the herald's loopback peer.

**Keeper A** (`keeper.toml`, see the keeper guide §5 for every key):

```toml
program = "<PROGRAM_ID>"
season = <ID>
beneficiary = "<operator beneficiary>"
rpc = ["<devnet RPC over the G1/G2 path>"]
drand = ["<quicknet over the G1/G3 path>"]
roles = ["beacon","reveal","settle-departure","gather","resolve","skip","settle","tickets","explore","archive","close","fold","rings","dormancy","claims","sweep"]
reveal_pool = 150
delay_pool = 32
funders = 4
r99_reveals = 150          # W6-E: any value ≤ 150 gives the same floor (0.0080 SOL); the default 4,000 is 27× that
delay_floor = 50000000     # 0.05 SOL, ≈ 4× the F_d formula for a 200-player season (3 × ≈ 0.12 SOL of delay-class rent per bell / 32) [model]
p_def_milli = 2000
p_delay_milli = 500
daily_budget_sol = 5
api = "127.0.0.1:41050"
token_file = "keeper-a/keeper.token"
master_seed_file = "keeper-a/keeper.seed"
journal = "keeper-a/keeper.journal.sqlite"
beneficiary_key_file = "keeper-a/beneficiary.key"
```

Keeper B: the same with its own seed, journal, token, beneficiary, `api = "127.0.0.1:41051"`, `roles = ["reveal","settle-departure","settle","claims"]` and `race_jitter_slots = 2`.

**Herald:**

```sh
frontier-herald --data herald/ --program <PROGRAM_ID> --season <ID> \
  --rpc <devnet RPC over the G1 path> --source rpc --cluster devnet \
  --listen 127.0.0.1:41040 --web permutation-server/web --relay 127.0.0.1:41033 \
  --drand-info quicknet-info.json --checkpoint-slots 150 --poll-ms 400
```

A TLS reverse proxy (hosting is the owner's decision, P2) forwards the public origin to `127.0.0.1:41040` and nothing else; the herald's security headers and CSP apply as on the local stack. No other port is public.

**Ports** stay in 41000–41999 on every host, never 4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185 or 5191.

## 5. Budget in test SOL [model]

At 5,080 lamports per byte (mainnet since SIMD-0437); **devnet's rate is unverified (G7)**: at the older 6,960 multiply every rent line by 1.37. "Float" is returned at close.

| Item | M1_PLAYTEST as it stands | Smaller option (a preset change, owner's choice) | Kind |
|---|---|---|---|
| Program ProgramData (`max_len` from the release build record, `round_up(1.25 × .so, 4 KiB)`: 1,097,728 B for the merged Phase B release `.so` of 875,768 B; 1,093,632 B for the W6-base 874,120 B) | ≈ 5.6 SOL (5.58 at 1,097,728 B) | same | float (until the program is closed) |
| Creation bond | 1 SOL | 1 SOL | returned after a normal season |
| ProvinceFunds (`pfund_initial`, all 817 provinces within ring 16) | 18 SOL | ≈ 2.7 SOL (rings ≤ 6: 127 provinces × 0.0215 SOL) | float |
| DefencePool (`dpool_initial`) | 20 SOL | 2 SOL (at 200 players the model's p99 attacked bell costs < 0.002 SOL, §22.3) | returned if unspent |
| Season, Frontier, 48 JoinShards, 16 BeaconLogs | ≈ 0.13 SOL | same | float |
| AnchorArchives (16 regions × 16 half-days) | ≈ 8.2 SOL | same | float, returned at CloseSeason part 9 |
| Keeper A and B reveal pools (150 payers each, R99 150) | 2 × 1.2–2.4 SOL | same | float |
| Keeper A and B delay pools (32 payers each) | 2 × 16–32 SOL at the default 0.5-SOL floor | 2 × 1.6–3.2 SOL at `delay_floor` 0.05 | float |
| Relay pool (150 payers ≥ 0.05 SOL) | ≥ 7.5 SOL | same | float |
| Player rent, sponsored (Citizen + Holding, 200 players) | ≈ 1.95 SOL | same | float, returned at close |
| Fees and tips over 7 days (≤ 40 sponsored tx per player-day, ≤ 24 marches per day) | ≤ ≈ 2.5 SOL | same | spent (tips go to keepers) |
| **Total to hold at the start** | **≈ 100–135 SOL** | **≈ 37–43 SOL** | |

## 6. Watching, stopping, and what counts as a failure

- **Watch:** keeper `/v1/status` (effective N ≥ 150 for both keepers in every bell; alerts `contested`, `write-expired`, `anchor-missing`, `retry-ladder`, `payer-care`), the relay's `GET /f/operator/pool` (pool ≥ `--min-pool-sol`, else it answers 503 `OperatorLowFunds`), the herald's fold lag and error rate, and the nightly verifier.
- **A failure that stops new invites:** a verifier FAIL; a province-bell stuck more than 2 bells after its seed; a transit unsettled 2 bells after `close + 600`; reveal effective N < 150 in any bell; a `retry-ladder` alert (a CU or heap budget breach on a real cluster).
- **Stopping early:** once the season is Running, M1 has no operator Abort (AbortSeason works only before `genesis_ts`, or for Announced/Created seasons 7 days after `t_create_min`). Stopping the keepers makes the game **wait** (lag only delays); communicate a pause, fix, and resume. A program upgrade mid-season is possible with the upgrade authority but changes the `.so` hash for a slot range (the verifier's V2 reports it) and must keep `PROGRAM_VERSION` and every layout; anything else needs a new season id.
- **Privacy:** wallets and every game action are public on chain; the relay keeps per-IP counters in memory and invite redemptions in its state file; no other personal data is collected. Tell players this in the invite.

## 7. What this runbook does not decide

The owner decides: whether the playtest happens (O-M1-18), its size and dates, who is invited and how, the hosting and domain for the herald, who runs keeper B, the preset values of §5 (the smaller option is a `frontier-abi` change for W7-B), and whether the playtest waits for the M4 fee-market soak (it need not: C4 is a mainnet property; devnet's fee market is not representative).
