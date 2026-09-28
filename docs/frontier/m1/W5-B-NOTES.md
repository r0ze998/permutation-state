# W5-B stack — notes

- **Unit:** W5-B (wave 5), branch `frontier/m1-W5-B` cut from `frontier/m1-integ` at `31f1aa1` (contract v1.7).
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.7 — §3.3–§3.5, §8.2, §8.3, §8.6, §8.7, §8.8, §10.1, §10.3, §11 (W5-B brief), §12 (preamble, Gate W5, Gate W6 lines that name `frontier-stack`), §13.4 (E5), §13.5; I-25, I-26, I-45, I-48, I-49, I-51, I-53, I-54, I-55. Offchain design §11 (the local stack). Owner decisions 2026-09-27/28 as relayed by the workflow: O-M1-12 items 1–3 approved (item 3: the archive is fetched by the main session; this unit starts no fetch), Agave ≥ 4.0 not approved, O-M1-18 not approved.
- **Owned paths touched:** `frontier-node/crates/stack/**`, `frontier-node/configs/**` (new), `scripts/m1-nightly.sh` (new), this file. `localnet` and `drand-replay` needed no change. Integrator-owned files changed on this branch so it builds and stays clean (requests §6): `frontier-node/crates/stack/Cargo.toml` (dependency sections), `frontier-node/Cargo.lock` (edges of the `stack` package only), `.gitignore` (one line).
- **Tags:** [measured] = run on this machine on 2026-09-28; [design] = a rule implemented as the contract states it.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no drand fetch (the archive directory was only read for `manifest.json`); no Agave, rustup or Playwright install; no new crate (every dependency was already in the lock). `npm ci --ignore-scripts` was run once in this worktree's `permutation-gateway` (its lockfile, for the relay the stack starts; `node_modules` is git-ignored). Services were started only on the M1 ports of this unit's runs (41000–41099, 41500–41599, and 41200 for refusal probes that bind nothing), every one stopped with `down` afterwards (checked: nothing of this unit listens). `permutation-server/web/session.mjs`, the main tree, `codex/magicblock-playable` and `codex/v9-security` untouched. The 1.95.0 rustfmt/clippy install is already recorded in `docs/frontier/DECISIONS.md` part A (and H3); DECISIONS is not this unit's file, nothing added.

## 1. What landed

### `frontier-stack` (crate `stack`, lib `frontier_stack`, bin `frontier-stack`)

```text
frontier-stack check-ports (--config FILE [--base-port P] | --ports P1,P2,...)
frontier-stack up [--config FILE] [--mode accel|realtime] [--beacon test-key|archive] [--scale S]
                  [--days D | --game-hours H] [--bots N] [--run-id ID] [--base-port P] [--chaos]
                  [--adversary] [--viewers N] [--viewer-window-hours H] [--archive DIR] [--g0 UNIX]
                  [--so PATH] [--seed N] [--keep-running] [--chaos-targets a,b] [--bots-args "..."]
frontier-stack verify --run-id ID
frontier-stack tamper --run-id ID [--strict]
frontier-stack load   --run-id ID [--viewers 5000] [--game-hours 1]
frontier-stack report --run-id ID
frontier-stack down   --run-id ID
```
Exit codes: 0 pass, 1 fail, 2 bad arguments / cannot run, **3 PENDING-OWNER** (Mode R; real rounds before the archive has its `manifest.json`).

| module | what |
|---|---|
| `config` + `toml` | the stack TOML (flat keys, `[ports]`, `[paths]`, `[chaos]`, `[adversary]`, `[viewers]`, `[pools]`; unknown keys refused) and the `up` flags (they win). Every port is `base_port + offset` (§10.3 table); an archive run defaults to G0 = 1788998400 and the main session's archive directory (`../../data/drand-archive-quicknet-g0-1788998400` from a worktree, or `FRONTIER_DRAND_ARCHIVE`) |
| `ports` | the §12 rule over **every** port a config names: reserved list, 41000–41999, busy (`fclient::ports::port_in_use`, the `lsof` rule), and no port named twice; never requires the reserved ports to be idle (I-26) |
| `run` | `frontier-node/.local/frontier/<run-id>/`: `state.json` (config, ports, program, `.so` sha256/len/max_len, pids, phase, season, play window), `events.jsonl` (supervisor log), `logs/<component>.log`, per-component directories, `metrics/`, `verify/`, `tamper/`, `load/`, `report.{json,md}`; secrets (operator key, keeper master seeds and tokens, relay master seed, beneficiary keys) mode 0600 from `/dev/urandom` |
| `procs` | components in their own process group (they outlive `up`; a terminal Ctrl-C does not reach them), SIGINT then SIGKILL, reaping, `kill -9` for chaos; `down` signals a recorded pid only if `ps` still shows that component's program |
| `chain` | `frontier-localnet` over JSON-RPC: `frontier_status/setScale/pause/resume/hold/holds`, airdrops, operator transactions (sent and waited for, with the program's logs on refusal), the season, every Province (`getProgramAccounts` + dataSize), the newest program transaction's slot |
| `setup` | the operator (§5.7, I-09, I-54): AnnounceSeason **at the run's scale** with 20 slots of margin (at 2,000 a slot is 800 game s, so the lead fell under the program's minimum before landing — `Announce` 56 on the first run [measured]), the 24-h lead at scale 2,000 (**43.9 s** of wall time [measured]), back to the run's scale, CreateSeason, InitBeaconLogs, InitShards × 6. Season parameters = `M1_LOCAL_7D`, with the test key's `quicknet_pk_hash` for a test-key run |
| `up` | the start order (below), then the supervisor |
| `chaos` | the kill schedule: a kill every U(2, 6) game hours over the play window, restart after U(0, 60) game seconds (converted to wall time at the run's scale; the Clock stops while the chain itself is down), seeded xorshift, `--chaos-targets` to restrict |
| `adversary` | the §13.4 / §8.8 hold schedule (table below) |
| `verifyrun` | `verify` (verifier input from the run's chain, trust roots pinned from the run) and `tamper` (every class of `verify_core::tamper` on the run) |
| `load` | `frontier-viewers` with 4/5 polling and 1/5 WS viewers for `--game-hours` (wall = game / scale), stats on the viewers port, fold lag sampled every second, the §13.4 criterion-6 targets |
| `report` | the run report |

**Start order** (offchain design §11.4, contract §8.7): (1) `frontier-localnet` with the `.so` at `--max-len = round_up(1.25 × .so, 4 KiB)` and the operator key as its upgrade authority (`--data-dir` = WAL + snapshots); (2) `drand-replay` (`--test-key`, or `--archive DIR`) with `--clock chain:` the localnet; (3) the operator's season (`setup`); (4) airdrops — both keepers' **150 reveal payers** (0.35 SOL each, see F1), 32 delay payers and 4 funders, the relay's 150 payers — then keeper A (every role of `keeper::config::ROLES`, API 41050) and keeper B (public profile `reveal, settle-departure, settle, claims`, API 41051, `race_jitter_slots = 2`), each from a `keeper.toml` the stack writes (checked with the keeper's own parser in a test), then the Node relay (`permutation-gateway/src/frontier/server.mjs`, operator 41030 / public 41033, the keeper link with keeper A's token, `FRONTIER_OPERATOR_TOKEN`); (5) the herald (`--web permutation-server/web`, `--test-key` for a test-key run), waiting for `/h/season`; (6) the bots (`--rpc` for the personas' own transactions, journal and report in the run directory); (7) the in-run viewer window when `viewers.count > 0` (E5: 5,000 for 24 game hours from `start_hours`).

**Supervisor:** reaps exits (an unexpected one is a `crash` event and a restart after 2 s; a restarted localnet gets `frontier_setScale` again because a recovered header may carry another scale; a restarted bot fleet gets its remaining play hours, and is not restarted after play), chaos, the adversary holds, EndSeason once `end_bell` is over (any signer; no keeper duty sends it), per-bell samples of both keepers' `/v1/status` and the herald's `/h/status`, stops the fleet one bell after play if it has not stopped by itself (F3), and after play + drain pauses the chain (so verify, tamper and the report read one fixed state; `--keep-running` turns it off), records `complete` and exits 0 **leaving the services up**. Ctrl-C of `up` stops everything (`interrupted`, exit 130). `down` stops the supervisor if it still runs, then every recorded component in reverse start order, and any viewer generator.

**Refusals before anything starts:** Mode R → PENDING-OWNER (Agave ≥ 4.0, O-M1-12 item 4); ports; archive without `manifest.json` → PENDING-OWNER ("still being fetched" when a `partial-*.bin` is there); `.so` missing, a test-key run on a build without `PSF_TEST_BEACON_BUILD`, an archive run on a test-beacon build, any oracle/trace build; missing sibling binaries; the gateway's `node_modules` missing; a previous run with the same id still up.

**Adversary schedule** (`--adversary`, `[adversary] enabled`): one hold per kind, spread over play, keys chosen from the chain when the hold starts, each written to `events.jsonl` with its keys, price, slots and whether it is above the keeper cap (so criterion 4's expected `ValidSealUnrevealed` windows are listed):

| kind | keys | milli | length | cap |
|---|---|---|---|---|
| `slots-below` / `slots-above` | the busiest Province's next-bell ArrivalSlots (6 factions × `transit_slots`) + ArrivalDay (25 keys) | 1,500 / 3,000 | 1,500 game s (through the close) | W 2.0: below / above |
| `anchor` | that province's region's next BellAnchor | 1,000 | 200 s | D 0.5: above |
| `keeper-payers` | 20 of keeper A's reveal payers | 3,000 | 1 bell | above |
| `lag` | an origin Province + its region's next anchor | 1,000 | 2 bells | above |
| `ticket` | a Province with an open ticket cohort (armed from the first bell, fires at the first open cohort) | 1,000 | 2 bells | above |
| `frontier-fund` | Frontier + the newest province's wedge ProvinceFund | 1,000 | 1 bell | above |
| `defence-pool` | the DefencePool | 1,000 | 3 bells (half the claim grace) | above |
| `relay-payers` ("payer holds") | 20 of the relay pool's payers | 3,000 | 1 bell | above |

**verify / tamper.** `verify` pauses the chain if it is running, reads every program transaction (`frontier_feed`), every program account of the season and the deployed ELF's sha256 (ProgramData), pins the drand key of the run's beacon (test key or quicknet), `RULESET_HASH` of this build and the `.so` sha256 the stack deployed (V2), saves the input as `verify/input.json.gz`, writes `verify/report.{json,md}` and `summary.json`; exit = the verifier's. `tamper` builds **every class of `verify_core::tamper`** (T1–T22 plus T1b, T6b, T23, V9a: 26) from the run's input under `catch_unwind`; a class the run cannot express (its builder finds nothing to edit — e.g. no REVEAL in a land-only run, T13's late Reveal that the program never lets land) is judged on the committed fixture it was written for and labelled `fixture`; exit 0 when all 26 FAIL with one of their codes, `--strict` also fails on any fixture fallback. (W5-D owns the verifier and may add builders for stack runs; the stack calls the builders by name.)

**report** (`report.json` + `report.md`): per instruction kind landed / failed / p50 / p99 / max CU (whole-transaction units: the ComputeBudget instructions add ≈ 450) against the §5.5 budget and tx bytes; the **Reveal CU distribution**; keeper latencies in slots from the records (round → anchor, S → first cache, anchor → last valid reveal, close → resolve, SkipQuiet per province-day); failures by (kind, error name); records by kind; transit outcomes and seal codes; unsettled due transits; stuck province-bells and cohorts open past 24 bells from the final Provinces; keeper samples (min reveal effective N in play, alerts, spend); herald fold lag and alarms; chaos kills, restarts, crashes; holds; personas; verify, tamper and load verdicts; the §13.4 criteria it can decide.

### Configs (`frontier-node/configs/`)

`w5-smoke.toml` (= the Gate W5 `up` flags and the `up` defaults), `nightly.toml` (base 41500, adversary on), `w6-latency.toml` (scale 2, 6 game hours, 300 bots, chaos), `w6-s7.toml` (archive, release `.so`, 20×, 7 days, 1,000 bots, chaos, adversary, 5,000 viewers from day 1 for 24 game hours), `w6-s7-rehearsal.toml` (the same on the test key, "not exit-grade"), `m1-exit.toml` (E5), `real-smoke.toml` (archive, 3 game hours at 100×, 20 bots, base 41500). A test parses every committed config and checks its ports statically and that archive configs use the archive's G0.

### `scripts/m1-nightly.sh`

Builds (`cargo build --locked --release --workspace`, `build-frontier.sh --features test-beacon`; `--no-build` skips), `check-ports --config configs/nightly.toml`, `up` (run id `nightly-YYYYMMDD`, extra args passed to `up`), `verify`, `tamper`, `load --viewers 5000 --game-hours 1`, `report`, and `down` always (trap); writes `<run>/nightly.json`; exit 0 all pass, 3 when a step was PENDING-OWNER, else 1. Bash 3.2 compatible (macOS `/bin/bash`).

## 2. Runs [measured, 2026-09-28, this machine; test-beacon `.so` 1,032,688 B, sha256 `dc1281c3…09d9`]

| run | what | result |
|---|---|---|
| `w5-smoke` (Gate W5 lines, §3) | 100 bots × 1 game day at 100×, 41000–41099 | `up` complete in 1,108 s wall (57 s setup incl. 43.9 s pre-season), 2,768 slots; **verify PASS** (6,538 txs, 5 failed, 5,841 accounts; read 0.2 s, verify 0.5 s); **tamper 26/26 FAIL with their codes** (12 built from the run, 14 on fixtures: the run has no march); **load pass**: 5,000 viewers (4,000 polling + 1,000 WS) for 1 game hour (36 s), 28,912 requests, 0 errors, p99 file 59.4 ms, fold lag p99 1.2 s, 589,821 WS messages, 0 gaps; `down` 0. No CU over budget (PostAnchorMulti max 380,714 / 400,000; PostSeed 339,691 / 345,000; FoldOccupancy 28,887 / 30,000); keeper A reveal effective N ≥ 150 through play, 0 alerts |
| `nightly-20260928` (`scripts/m1-nightly.sh`, concurrently with the smoke) | 100 bots × 1 day at 100×, 41500–41599, adversary on | every step exit 0 (`"pass": true`): all 9 holds placed; verify PASS (7,153 txs); tamper 26/26 (**21 from the run**); load pass (p99 59→115 ms with two stacks on the machine, fold lag p99 2.0 s — at the limit); JOIN 63, DEPART 5, REVEAL 5, CLASH 5, TRANSIT_SETTLED 5 (3 bad seals destroyed with stock code 2, 1 Stays, 1 Retreated); Reveal CU p50 20,276, max 21,435 |
| `dev2` | 60 bots, 4 game hours, chaos (0.4–0.8 h gaps) on every component + adversary | kills of herald × 2, drand-replay, bots, keeper-a, each restarted; verify PASS, tamper 26/26 |
| `dev3` | 30 bots, 3 game hours, chaos on **localnet and relay**: localnet `kill -9` × 4 | each restart recovered from snapshot + WAL ("recovered: slot 355, … 1130 history entries"); verify PASS on the recovered chain |
| `dev4` | 30 bots, 2 game hours, chaos on the bots × 4 | each restarted fleet kept the play end (`until` 1785638279…320); fleet exited at play end; verify PASS |
| refusal probes | `up --beacon archive`, `up --config configs/real-smoke.toml`, `up --mode realtime` | exit 3 `PENDING-OWNER` (the archive directory holds `partial-32065012-32311012.bin`, no `manifest.json`, at 12:04–13:30 JST; Mode R needs Agave) |

**Real-round smoke: PENDING** — the main session's archive (`.claude/data/drand-archive-quicknet-g0-1788998400`) had no `manifest.json` during this unit's window; the stack refuses with exit 3 until it has one, and `configs/real-smoke.toml` is the command to run then (`frontier-stack up --config frontier-node/configs/real-smoke.toml`, then verify/tamper/report/down). The archive path pins quicknet (herald without `--test-key`, verifier with the quicknet key, drand-replay `--archive`, localnet `--g0 1788998400`, release `.so` required).

## 3. Gate items that concern these files (run on this branch)

| # | Command | Result |
|---|---|---|
| 1 | `(cd frontier-node && cargo fmt --all -- --check)` | exit 0 |
| 2 | `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | exit 0 |
| 3 | `(cd frontier-node && cargo test --locked --release --workspace)` | exit 0: 287 passed, 0 failed, 7 ignored (stack: 29 tests); `web3.js conformance NOT RUN` 0 times |
| 4 | `(cd frontier-node && cargo test --locked --workspace)` (debug, Gate W1 line) | exit 0: 287 passed, 0 failed, 7 ignored |
| 5 | `cargo build --locked --release --workspace` (Gate W5 line, first half) | exit 0 (nightly `build-node`) |
| 6 | `$S check-ports --config frontier-node/configs/w5-smoke.toml` | exit 0 |
| 7 | `$S up --mode accel --beacon test-key --scale 100 --days 1 --bots 100 --run-id w5-smoke --base-port 41000` | exit 0 |
| 8 | `$S verify --run-id w5-smoke && $S tamper --run-id w5-smoke` | exit 0, exit 0 |
| 9 | `$S load --run-id w5-smoke --viewers 5000 --game-hours 1` | exit 0 |
| 10 | `$S down --run-id w5-smoke` | exit 0 |
| 11 | `scripts/m1-nightly.sh` (Gate W6's first line, one night) | exit 0 |

Not run by this unit (not its files): `svm-tests RELEASE_CHECK=1` (W5-A), `crates/verify/mutate.sh` (W5-D), `itest g14_` (W5-C), the screens package (W5-E), the root-workspace lines.

## 4. Findings for other units (from the runs)

- **F1 (keeper, W5-C):** with only the delay pool and funders airdropped, keeper A's **reveal pool had effective N = 0 for the first 21 bells** at 100× (payer care runs at rest every 150 slots = 10 bells at 100×; `reveal-pool-low` alerts 1 → 22) [measured, run `dev1`]. The stack now airdrops the 150 reveal payers at start (offchain design §11.4 step 4 says so); the keeper's care cadence is in slots, so fast runs starve longer — worth a bell-based cadence.
- **F2 (bots, W5-C):** in the stack the fleet is far less active than in `inproc_day`: 25 of 100 bots joined in the smoke's game day (63 in the nightly), no march in the smoke; persona verdicts mostly `needs-chain`/`pending`. `inproc_day` uses `day0_share = 1.0` and `eager_personas`, which `frontier-bots` has no flag for. **Request:** `frontier-bots --day0-share F --eager-personas`; the stack passes them with `bots_args = "…"` / `--bots-args "…"` (implemented; empty by default so an unknown flag cannot crash-loop the fleet).
- **F3 (bots, W5-C):** a fleet started with `--days 1` printed `until game time 1785717439` but was still running 6 bells after play + drain (smoke); SIGINT stopped it and it wrote its report. The stack now stops the fleet one bell after play end.
- **F4 (keeper, W5-C):** `SkipQuiet` refused `OutOfOrder` 3 (smoke), 23 (nightly), 28 (`dev2`) times; SkipQuiet transactions per province-day p99 13–40 against criterion 3's "≤ 6 per idle province-day" (the report cannot yet tell idle from churned days).
- **F5 (E5 criterion 3 definition, W6-A):** measured from the round's publication (`round_time + drand delay`) to the landing slot's Clock, round → anchor is a constant **≈ 3 slots at 100×** (S → first cache ≈ 2.5), while the keeper's own `anchor_latency_slots_p99` (from when it observed the round) says 1. With the drand gate at `round_time + 1 s ≤ Clock` and a Clock that moves once per slot, the first slot that may show the round is the one after the round time, the keeper sees it there and lands one slot later: ≥ 2 slots structurally. Criterion 3's "p99 ≤ 2 slots" needs its reference point pinned (publication vs observation) before the 20× run judges it.
- **F6 (herald, W5-C):** the fold lag sampled at bell boundaries (where anchors, seeds and beacon logs land together) has p99 12 slots (smoke) / 30 slots (nightly, two stacks on one machine); during the 1-s-sampled load it was 1.2 s / 2.0 s. The load's ingest → WS figure is this fold lag; the WS fan-out after the fold is not in it (the generator does not timestamp events).
- **F7 (verifier, W5-D):** 14 of 26 tamper classes cannot be built from a land-only run and 5 from the nightly (no late Reveal ever lands on the program: T13; no Explore settle with a roll to edit when none happened, etc.); the stack judges those on the committed fixtures and says so. A march-rich stack run (F2) would move most of them onto the run.

## 5. Deviations and choices (for review)

1. `up` returns when the run is complete and leaves the services up (the Gate W5 sequence runs `verify`, `tamper`, `load`, `down` afterwards); the chain is **paused at completion** so the verified state is the reported state; `load` resumes the chain for its window (the herald needs live blocks) and pauses it again.
2. The operator's AnnounceSeason is sent at the run's scale, then the pre-season runs at 2,000 (I-54's intent; the announce itself cannot land at 2,000).
3. The stack is the operator: it holds the upgrade authority and sends AnnounceSeason, CreateSeason, InitBeaconLogs, InitShards and EndSeason (no keeper role sends EndSeason).
4. Keeper B's roles are `reveal, settle-departure, settle, claims` ("reveal, prove, settle": proving is settlement since I-44).
5. Tamper classes the run cannot express fall back to the committed fixtures, labelled; `--strict` refuses that.
6. Ingest → WS is measured as fold lag (§1 `load`).
7. Chaos restarts are converted to wall time at the run's scale (the game clock stops with the chain).

## 6. Dependency requests (integrator, I-55)

- **R1 `stack`:** dependencies `fclient`, `frontier-abi`, `permutation-rules` (workspace), `verify` (path), `tokio`, `serde_json`, `sha2`, `hex` (workspace); dev-dependency `keeper` (path; its own `KeeperConfig::from_toml` checks the `keeper.toml` the stack writes). No new external crate; `Cargo.lock` gains edges of the `stack` package only.
- **R2 `.gitignore`:** `/frontier-node/.local/` (run directories: logs, WAL, snapshots, test keys).
- **R3 (W5-C, bots CLI):** `--day0-share` and `--eager-personas` on `frontier-bots` (F2).
