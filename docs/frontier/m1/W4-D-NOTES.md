# W4-D verifier — notes

- **Unit:** W4-D (wave 4), `frontier-verify` / `verify-core` (M1 contract §8.5, §11 wave 4, §12 Gate W4 last line, §13.5 E6).
- **Branch:** `frontier/m1-W4-D`, cut from `frontier/m1-integ` at `7289822` (contract v1.5).
- **Owned paths touched:** `frontier-node/crates/verify/**`, `frontier-node/fixtures/**`, this file. Outside them: `frontier-node/Cargo.lock` (the `verify` package's dependency list only; request R1).
- **Date:** 2026-09-28. Nothing pushed; no devnet/mainnet transaction; no service started (all tests in process, `localnet::InProcess`); nothing installed or downloaded (test-beacon key only).

## 1. What landed

| Module | What it does |
|---|---|
| `input.rs` | `Config` (program, season, **pinned drand key**, expected ruleset hash, optional expected `.so` sha256) and `Input` (transactions in feed order incl. failed ones, **final account states at a pinned slot**, observed `.so` hashes). Three sources: a fixture file (`findex` archive JSON per transaction, RPC JSON per account), a `findex` archive directory (`--archive`, a speed-up only) plus finals from a file or RPC, or RPC alone (`frontier_feed` on a local node, `findex::RpcPoll` + `getMultipleAccounts` otherwise). |
| `world.rs` | Parses every transaction: message keys and writability, the program's instructions, **the PS2 bodies the program printed in its own frame** (`fclient::log::bodies_from_logs`), decoded with `frontier_abi::log` (kind widths), post-states indexed per account. A transaction listed twice counts once. Anything that does not decode is kept as a problem (K2). |
| `facts.rs` | Season facts read from the chain: CreateSeason's `SeasonParams`, the clock, `W(b)`, THE anchor and caches per `(bell, region)`, archive entries, every drand signature the transactions carried (verified lazily, memoised), ring seeds, citizens (JOIN), owner history per site (SETTLE/RELEASE), ticket funders, province addresses, departures per host. |
| `checks/v1…v13` | The checks (§2). Each is one module; `checks::run` runs them and a `mutate-v<n>` build leaves one out. |
| `clash_input.rs` | `ClashBuilder` trait + `ContractBuilder`: the §5.11 ResolveFromInputs input rebuilt from the Province before the resolve and the gathered ClashInputs (same semantics as the herald's `Provisional`; W4-A's exported builder plugs in here). |
| `report.rs` | V10: `report.json` (the §8.5 schema + `checks_run`, `provenance`, a `check` per finding, severity `fail`/`warn`/`unverifiable`) and `report.md`. |
| `main.rs` | `frontier-verify` CLI: `--fixture FILE` · `--archive DIR (--finals FILE \| --rpc URL)` · `--rpc URL [--localnet]`; `--test-key` / `--quicknet-pk HEX`, `--ruleset HEX`, `--program-hash HEX`, `--out DIR`, `--json`. Exit 0 PASS, 1 FAIL, 2 cannot verify (usage errors are 2 too). |
| `fixture/` | The synthetic march season generator (§4.2). |
| `tamper.rs` | T1–T22 (+ T6b, V9a) and `rechain`, the consistent forger's tool (§5). |
| `mutate.sh` | The checks of the checks (§6). |
| `tests/` | `fixtures.rs` (PASS on both fixtures, scenario coverage, structural freshness of the synthetic fixture, CLI exit codes and report files), `tamper.rs` (one test per class, mutate-aware), `unit.rs` (rechain identity on both fixtures, fixture round trip, report schema, fail-closed without post-states, undecodable record), `record.rs` (the two recorders, `#[ignore]`), `common/`. |

Verdict rule: any `fail` finding → FAIL; else any `unverifiable` finding (data a check needs is absent, code `MissingData`) → UNVERIFIABLE; else PASS (warnings allowed).

## 2. The checks — what each judges, and what it does not yet

| # | Judges | Not (yet) judged, and why |
|---|---|---|
| V1 | Every record's tail matched to `chains_of` (kind order, optional links: v1.5 SETTLE one-shard, BUILD walls, TRANSIT_SETTLED); every link resolved to an address (key/payload; JOIN facts for `Tag8` and `JoinShardOf`; the owner history for `OfHolding`; the transaction's writable account of that kind for `InTx`, kind by magic); seq contiguous, `head = sha256(prev ‖ le64(seq) ‖ body_without_tail)`; CLOSE's payload = the chain as it stood, and a creation after CLOSE starts a new chain (v1.5); **final head of every open chain = the account's header at the pinned slot**, a closed chain's account gone, a chained program account the archive never explains = `ChainGap`; **K4 for state: the final bytes of every program account = the post-state of the last transaction that wrote it** (a `set_account` shows as `HeadMismatch`); a transaction listed twice = `DuplicateEvent`; an undecodable transaction or record = `Undecodable` (K2) | — |
| V2 | Season owned by the program; the Season account's and SEASON_CREATED's ruleset hash = the expected one; one ANNOUNCE and one SEASON_CREATED, same params hash; **CreateSeason's own data hashes to it** (`presets::params_hash`); creation in `[t_create_min, +7 d)`; program version; every observed `.so` hash = the pinned one (a pin without an observation = `MissingData`) | the `.so` hash in RPC mode (ProgramData read) is not wired: only fixtures/archives carry `program_hashes`. A transaction that does not invoke the program is a warning |
| V3 | The season pins the verifier's key (`quicknet_pk_hash`); every signature (genesis, ring, anchor, cache, beacon) re-verified with blstrs from the **instruction data**; genesis round and `genesis_ts` by rule; genesis ring seeds `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))`; later rings `ring_seed_round(t_open)` with `t_open` the landing Clock; RING_SEED = its RING_OPEN's round, `seed_of`; **one ANCHOR per (bell, region) for the whole season**; anchor round `T(b)`, `A`/`slot` = the landing Clock/slot, written at `an‖(b, r)`; caches after THE anchor, `a = A`, round `S(b, r)`, `seed_of`, at `sd‖…`; ARCHIVE `a_off`, seed and the entry's signature = THE anchor's | `max_anchor_delay_s` = max(A − round time of T(b)) is reported |
| V4 | Every landed Reveal: `Clock < A + W(b)` once THE anchor landed (a Reveal before the anchor, e.g. an owner's in-bell reveal, is allowed); none after its bell was archived; **before the province-bell's first GATHER, CLASH or a SKIP over the bell** (latch) | `RevealNearClose` (last 16 game s) is a warning |
| V5 | Every DEPART's seal opened with the **stock `tlock` crate** (`fclient::seal::judge`, the signature of `T(arrive)` from an anchor transaction, verified), commitment and `Plain::validate`; `seal_root`; every Reveal's plaintext+salt give the commitment and root, a valid seal's revealed plaintext = the opened one; SettleTransit judged DEPART's pair; **seal code > 0 ⇔ stock failure**; **a bad seal settled with another outcome = `BadSealSurvived`**; warnings: valid seal unrevealed (with the failed Reveal attempts' signatures), bad seal unsettled | `--keeper-journal` attempts are not read (deferred) |
| V6 | Every REVEAL replayed through `clash::admit_arrival` in landing order with `SlotEntry{host, citizen_tag of the holding's owner, DEPART dep_mass}`: the logged slot, fill/displace and displaced host = the kernel's decision, refused ones never landed; the ArrivalSlot written carries DEPART's mass; at each CLASH the gathered ClashInputs hold exactly the replayed four per faction with DEPART's masses | — |
| V7 | CLASH re-run with `resolve_clash` via `ContractBuilder` from the Province **as the last transaction before the resolve left it**, the gathered inputs and THE seed: digest, engagements, packed fates; Province resolved through the bell after; `resolved_next` advanced one bell at a time from the opening by CLASH/SKIP; SKIP: no revealed arrival in the run (`SkipOverArrival`), quiet at `b0` (`is_quiet`); TRANSIT_SETTLED outcome by D5 from the logged seal code and the destination's resolved inputs (fate, `bounced-unranked` if `admit_arrival` refuses it against the final four, else `routed`; no resolved inputs → `routed`) and a Stays/Withdrew host's troops; each founded Holding written with the SETTLE's owner/gen/score/cohort/final_ts | the CLASH **input digest** and SKIP **quiet digest** are not judged (formulas not pinned, §3 item 1); quiet is re-checked at a run's first bell only; the lazy accrual/queue replay of Harvest/Build/Train (`HoldingReplayMismatch` beyond the founding) is deferred to W5 |
| V8 | Every DEPARTURE_SETTLED after its origin resolved the departure bell, with the values `Host::apply_clash` (replayed origin clash, if the host fought) → `settle` → `march_values` give; every gathered arrival's troops and stamina = its DEPARTURE_SETTLED values | — |
| V9 | Every program account of the season seen in a post-state or final state sits at the address its own key fields derive (all 17 kinds) | pre-funded addresses: informational only (accounts holding more than rent, excluding funds/pools/holdings/citizens) |
| V11 | SETTLE score = `fclient::land::ticket_score(S(ticket_bell, r_site), …)` (the program's twin-pinned function), `final_ts = round_time(S) + 600`; displacement only inside the holder's cohort, by a better ticket (`land::beats`), naming the holder; a `taken` of the holder's own cohort must not beat it; `expired` only from `ticket_bell + 24`; **cohort counters of every Province post-state = the TICKET/SETTLE tallies**; PROVINCE_OPEN terrain digest = `generate_province` in the program's pinned encoding (`sha256("PSF-TERRAIN-v1" ‖ Province[128..296])`), ring, wedge, region, site count; initial camp and CAMP records = `camp::place` | — |
| V12 | Every EXPLORE_RESULT = `explore::roll(S(explore bell, r), …)` per tile with the citizen's floor spent tile by tile | — |
| V13 | TRANSIT_SETTLED: lamports per recipient (the four `(prefix, amount)` pairs, DIVERT records of the same transaction and `pool_owed_delta`) = the D5 rule for the logged outcome, recipients = SettleTransit's own accounts, the slot's beneficiary and the inputs' resolver read from the chain; DEFENCE_CLAIM amount = Σ `fees::defence_refund` over eligible slots (lateness ≥ `lateness_slots` against THE anchor's slot, the keeper's own, unclaimed), per-bell-region and per-keeper-day caps; partial ≤ | SETTLE escrow refunds; `ConservationGap` (informational in M1) is not computed |

Without post-states (a public RPC archive not enriched by `findex::Enriched`) V6's inputs, V7, V11's cohort counters, V13 are `MissingData` → UNVERIFIABLE, never PASS (test `no_post_states_is_unverifiable`).

## 3. Choices the contract leaves open (candidates for v1.6; W4-A/W4-B please match or say so)

1. **CLASH `input_digest` and SKIP `quiet_digest`** are named in §6 without a formula. The verifier does not judge them; the synthetic fixture writes `sha256("PSF-CLASH-INPUTS-v1" ‖ 24 arrival records ‖ le32(bell))` and `sha256("PSF-QUIET-v1" ‖ P ‖ Q ‖ b0 ‖ n)` (`clash_input::provisional_*`). Request: W4-A pins them (contract amendment) and V7 then checks them.
2. **TRANSIT_SETTLED payments**: V13 compares **sums per recipient prefix**, so W4-B may encode the pairs as it likes (e.g. bad seal as one `reward` of tip+fee+bond, or three pairs to the settler). The fixture uses: fate → tip to the slot beneficiary, fee to the resolver, bond to the rent payer; bad seal → one reward to the settler; routed → `pool_owed_delta`.
3. **TRANSIT_SETTLED troops**: checked only for Stays/Withdrew (= ClashInputs `troops_after`) and bad seal (0). Bounced/Retreated/unranked/routed troops are W4-B's (the fixture writes the arrival troops, the departure troops and `rout_survivors`).
4. **ClashInputs after the resolve**: `fate` 1–5 per position (`faction × 4 + i`), `troops_after`, flag 2, `resolver` = the resolve's beneficiary; CLASH `fates` packed at the same positions. The dest Province's `resolved_next = bell + 1` after the CLASH.
5. **The camp in the clash** is a NEUTRAL garrison with id `u64::MAX − camp.gen` (as the herald's builder); the lazy respawn (I-56) is not modelled — W4-A's builder is normative.
6. **DEFENCE_CLAIM**: key = the keeper; the claimed slots are the `[slot w][anchor r]` pairs from account 5 on; the per-bell-region cap is cumulative over all claims of a `(bell, region)`, the per-keeper-day cap over the keeper's claims of the day.
7. **ARCHIVE key** `region, day` carries the half-day **part** (v1.3); CLOSE of a non-chained account (anchor, cache, slot, day) has no tail link.

## 4. Fixtures (`frontier-node/fixtures/verify/`)

### 4.1 `land-program.json` — recorded from the program (1.6 MB, 333 transactions)

The **test-beacon program** of this branch (`scripts/build-frontier.sh --features test-beacon`: `file_sha256 6555facc418b51cab8fcabdce734b3b83d958fdc77ce07f617e572cc1cee96ca`, the integ-W3 review build) on `localnet::InProcess` at 20×, the season announced and created by rule (24-h lead at scale 2,000), and the **keeper's own W3 duties** (`beacon`, `rings`, `fold`, `tickets`, `keeper_core::Keeper`, test-key drand gated by the chain's Clock). Records: ANNOUNCE, SEASON_CREATED, GENESIS_SEED, RING_OPEN ×4, PROVINCE_OPEN ×37, FOLD ×99, JOIN ×7, TICKET ×7, SETTLE ×9, ANCHOR ×64, SEED ×60, BEACON ×68. The config pins the `.so` hash (V2 checks it). Recorder: `tests/record.rs::record_land_program` (`PSF_FRONTIER_SO=… cargo test --release -p verify --test record -- --ignored record_land_program`). **The verifier's chain resolution reproduces every tail and header the program wrote** (`rechain_is_the_identity_on_honest_archives`), and its independent terrain encoding and digests, ticket scores, cohort counters and canonical addresses agree with the program's on all 37 provinces and 9 settlements.

### 4.2 `march-synth.json` — synthetic (1.3 MB, 190 transactions)

The wave-3 program has no clash, transit, defence or archive instructions (W4-A/W4-B write them in this wave), so their records cannot be recorded from it yet. `verify_core::fixture::march` builds a season that has them **byte for byte in the `frontier-abi` layouts**: real signed transactions from the `fclient::ix` builders, real PS2 records and event chains, the test key's signatures and hints, **stock-`tlock` seals**, and the kernels' `resolve_clash`, `is_quiet`, `admit_arrival`, `Host`, `explore::roll`, `camp::place`, `generate_province`, `ticket_score`, `defence_refund`. The write-back after a resolve (residents' results, Stays arrivals as entries, pending ops settled, camp troops) and the TRANSIT_SETTLED encoding are the generator's documented choices (§3). **This fixture is a regression of the verifier, not evidence about the program**; W4-F's `itest` should re-record the march part from the program once W4-A/W4-B are merged (request R5). `march_fixture_is_fresh` checks the committed file against today's generator structurally (tlock's IBE randomness differs run to run).

### 4.3 Honest-but-adverse scenarios (§8.5) — all PASS

| Scenario | Where | How |
|---|---|---|
| keeper crash mid-bell | land | the keeper dropped half a bell after the cohort filed; a new one (same payer seed, no journal) started over from the chain |
| held anchor | land | the keeper away for 15 slots at a bell start: anchor 135 s late (`max_anchor_delay_s`) |
| pre-funded addresses | land | lamports sent to a future anchor and cache address; both inits landed |
| displacement; ticket cohort with a displacement and an expired ticket | land | a lower score settled first by hand, the keeper's higher one displaced it (gen 2); a ticket nobody settled for 24 bells settled `expired` |
| lagging origin | march | B resolves the departure bell of h3/h10 a bell late; their departures settle then; the destination's gather waited |
| low-tip rout (at `tip_min`, keeper B off) | march | h1 never revealed → `routed`, tip to `pool_owed`; `ValidSealUnrevealed` warning |
| bad seal destroyed: revealed in-bell / unrevealed / settled before and after archive | march | h3 (garbage, owner reveal before the anchor; code 2), h9 (garbage, unrevealed, settled from the archive 2 days later), h10 (valid seal of an invalid plaintext, code 5) |
| quota-refused arrival settled without loss | march | w6's Reveal refused `QuotaRefused` (failed transaction), settled `bounced-unranked` with its departure troops; w4 displaced from its slot, likewise |
| archived anchors and tombstones | march | ArchiveAnchors for bells 0–12 after `archive_after`, anchors and caches closed (CLOSE), settled slots closed |
| SkipQuiet runs | march | 22 skips, runs of 1–3 bells; two no-arrival clashes where the province was not quiet |
| also | march | a defence claim, an explore settled from the seed of its bell, a contested clash (h0 vs the resident h2), a march still in flight at the end |

## 5. Tamper classes (`tests/tamper.rs`, all FAIL with their code in the default build)

| Class | Fixture | Edit | Code | Feature that must let it PASS |
|---|---|---|---|---|
| T1 | march | drop the Depart of the march in flight at the end | `HeadMismatch` (`ChainGap`) | mutate-v1 |
| T2 | march | flip a byte of a Reveal's plaintext (instruction data) | `RevealCommitMismatch` | mutate-v5 |
| T3 | land | shift an anchor's A by 7 s (one no settlement reads) | `SeedRoundRule` | mutate-v3 |
| T4 | land | a second ANCHOR line for one (bell, region) | `DuplicateAnchor` | mutate-v3 |
| T5 | land | a PostSeed signature swapped for another round's | `BeaconSigInvalid`/`SeedRoundRule` | mutate-v3 |
| T6 | march | `set_account` on a Province after its last resolve (final bytes only) | `HeadMismatch` (K4 state) | mutate-v1 |
| T6b | march | a rogue Province write between two resolves; the second CLASH recomputed from it (re-chained) | `ClashReplayMismatch` | mutate-v7 |
| T7 | march | a valid seal's settlement forged as bad seal (code, outcome, payments; re-chained) | `VerdictDisagreesWithTlock` | mutate-v5 |
| T8 | march | the slot `i` of a displacing REVEAL changed | `QuotaSetMismatch` | mutate-v6 |
| T9 | march | a departure mass +1 troop (re-chained) | `TransitMassMismatch` | mutate-v6 |
| T10 | land | the verifier given quicknet's key instead of the test key | `BeaconSigInvalid` | mutate-v3 |
| T11 | land | a wrong expected ruleset hash | `RulesetMismatch` | mutate-v2 |
| T12 | march | the last game day's transactions dropped (finals unchanged) | `HeadMismatch` | mutate-v1 |
| T13 | march | a Reveal's Clock moved 5 s past `A + W` | `RevealAfterClose` | mutate-v4 |
| T14 | land | a transaction listed twice | `DuplicateEvent` | mutate-v1 |
| T15 | march | a gathered arrival's troops −1 troop, the CLASH, `troops_after` and the settlement recomputed (re-chained) | `OriginValueMismatch` | mutate-v8 |
| T16 | land | the genesis seed taken from another round (ConsumeGenesisSeed data, GENESIS_SEED; re-chained) | `GenesisSeedRule` | mutate-v3 |
| T17 | march | a SkipQuiet run extended over the next bell, which had a revealed arrival (re-chained) | `SkipOverArrival` | mutate-v7 |
| T18 | land | a SETTLE score changed, with the Holding's stored score (re-chained) | `TicketScoreMismatch` | mutate-v11 |
| T19 | land | a PROVINCE_OPEN terrain digest bit (re-chained) | `TerrainMismatch` | mutate-v11 |
| T20 | march | a DEFENCE_CLAIM amount +1,000 | `DefenceRefundMismatch` | mutate-v13 |
| T21 | march | an explore find +1 Works (re-chained) | `ExploreRollMismatch` | mutate-v12 |
| T22 | march | h3's bad-seal settlement forged as its clash fate (code 0, troops, payments; re-chained) | `BadSealSurvived` | mutate-v5 |
| V9a | land | an anchor account whose stored region is not its address's (post and final agree) | `NonCanonicalAddress` | mutate-v9 |

"Re-chained" = `tamper::rechain`: every later link, every CLOSE payload, every post-state and final header recomputed, so V1 cannot see the lie and only the check under test can; T6b, T7, T9, T15, T22 also forge the dependent fields a careful program-forger would (digests, payments, the Holding's stored score).

## 6. The checks of the checks

`crates/verify/mutate.sh` builds the verifier once per `mutate-v1 … v13` (no v10: the report) and runs the tamper suite; `tests/tamper.rs` asserts, **per build**, that the classes of the disabled check PASS and every other class still FAILS with its code. Result (debug, then release): all 12 builds pass — v1: T1, T6, T12, T14; v2: T11; v3: T3, T4, T5, T10, T16; v4: T13; v5: T2, T7, T22; v6: T8, T9; v7: T6b, T17; v8: T15; v9: V9a; v11: T18, T19; v12: T21; v13: T20.

## 7. What was run (all in `frontier-node/` of this worktree)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (and `-p verify` with `--features mutate-v1`, `-v5`, `-v13`) | exit 0 |
| `cargo test --locked -p verify` | 34 passed (fixtures 4, tamper 25, unit 5), 2 ignored (the recorders) |
| `cargo test --locked --release -p verify -- --include-ignored tamper_` (Gate W4 line) | 25 passed, 0 failed |
| `crates/verify/mutate.sh` (release; also `MUTATE_PROFILE=" "` debug) | exit 0: all 12 `mutate-` builds, 25/25 each |
| `cargo test --locked --workspace` (Gate W1 `frontier-node` line) | exit 0: 223 passed, 0 failed, 5 ignored |
| `cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection` (Gate W4 line; other units' tests, run to show `verify` does not break it) | 1 passed (the W1 `inproc_smoke`; `inproc_day`, the lag gate and crash injection are W4-C/W4-F's) |
| `PSF_FRONTIER_SO=…/deploy-test-beacon/permutation_frontier.so cargo test --release -p verify --test record -- --ignored record_land_program` | PASS, wrote `land-program.json` (333 txs) |
| `cargo test --release -p verify --test record -- --ignored record_march_synth` | PASS, wrote `march-synth.json` (190 txs) |
| `scripts/build-frontier.sh --features test-beacon` | exit 0, sha256 `6555facc…96ca`, 563,712 B |

Not run by this unit (not its files, or needing other units' merges): the program's svm suite, `build-wasm.sh`, the herald/keeper Gate W4 items.

## 8. Measurements

- `frontier-verify --fixture` (release): **0.58 s** for `land-program` (333 transactions, ≈ 190 BLS verifications: 1.5–3 ms each with hash-to-curve), **0.09 s** for `march-synth` (190 transactions, 12 seals opened with stock tlock). Extrapolated to the 7-day, 1,000-bot season (≈ 32k beacon verifications, ≈ 60k seals): ≈ 1.5–2 min single-threaded for the crypto; the checks are sequential today (post-states and resolutions are indexed, no quadratic scan). The < 10 min target (offchain design §10.4) is [model], to be measured on the W6 archive.
- Fixture sizes: 1.6 MB and 1.3 MB of JSON (post-states of program accounts only). If the integrator prefers them compressed, `flate2` (approved for the herald, J2) would do.

## 9. Deviations

1. **The march fixture is synthetic** (§4.2), not recorded from the program: the wave-3 program has no clash/transit/defence/archive instructions. The land fixture is a real recording. The contract's "T1–T22 on recorded test-beacon mini-seasons" is therefore met by a real recording for T3, T4, T5, T10, T11, T14, T16, T18, T19 and by the synthetic season for the others, until W4-F re-records.
2. **Checks not (yet) complete**, listed in §2's last column: input/quiet digests, holding lazy replay, SETTLE payments, conservation, `--keeper-journal`, `.so` hash from RPC.
3. **Tamper classes are forgeries by construction** for T6b/T7/T9/T15/T16/T17/T18/T19/T21/T22 (re-chained) — stronger than byte flips (a flip would be caught by V1 first and could not pass under its `mutate-` build).
4. **T6** is the rogue write after the last resolve (final bytes only, caught by K4 state); **T6b** adds the between-resolves case (caught by the replay).
5. **T12** truncates the march fixture's last game day (day 2: the archive, the closes and h9's settlement); the land fixture is shorter than a game day.

**Merge risk:** `tests/record.rs` (the land recorder) uses `keeper_core`'s public API (`Keeper::new/start/tick`, `payers`, `seeds.find`, `beacon.anchors`, `tickets.displacing`, `cfg.roles`); a W4-C API change breaks its compilation (not the committed fixture).

## 10. Dependency requests (for the integrator)

- **R1** `crates/verify/Cargo.toml` `[dependencies]`: `findex`, `frontier-abi`, `permutation-rules`, `solana-address`, `hex`, `serde_json`, `tokio` (all already workspace dependencies); `[dev-dependencies]`: `keeper` (path, for the land recorder), `localnet`. `[features]` `mutate-v1 … mutate-v13` (no v10). **No new external crate**; `Cargo.lock` changes only in the `verify` package's dependency list.

## 11. Requests and hand-overs to other owners (K11 items named for W4-D)

- **R2 (integrator, frozen files): the F8/W3-B encodings.** K11 names "F8 and W3-B encodings into frontier-abi/the kernel (W4-D)". `frontier-abi` (outside `layout/`, which is frozen) and `permutation-frontier` are not W4-D's paths. Meanwhile the verifier carries an **independent** copy of the terrain encoding and digest (`checks::v11_land::{encode_terrain, terrain_digest, TERRAIN_DOMAIN}`) and uses `fclient::land::ticket_score`; `land-program.json` pins both to the program (37 digests, 9 scores). Proposal: move `encode_terrain`/`terrain_digest`/`TERRAIN_DOMAIN` and `ticket_score` into `frontier-abi` (one definition) with the program, `fclient` and the verifier calling it, in the integration window.
- **R3 (integrator or the reveal.rs owner): `walk` vs `travel::path_cost`.** `walk` is `permutation-frontier/src/proc/reveal.rs:479`, a file no wave-4 unit owns; the host test belongs next to it. Not done here.
- **R4 (W4-F): herald and findex.** The archive `sig` in the herald's bell-region records and findex's upsert keyed by address alone (K11) are in W4-F's paths this wave. The verifier itself keys chains by address with the v1.5 restart after CLOSE.
- **R5 (W4-F, after W4-A/W4-B merge):** re-record the march part from the program (`itest`), save it with `verify_core::Input::save` as a third fixture, and keep `march-synth.json` as the verifier's own regression.
- **R6 (W4-A):** export the program's ClashInput builder; plug it into `clash_input::ClashBuilder` (V7, V8, the tamper forger). **(W4-A, W4-B):** confirm or amend §3's items 1–7 (v1.6).
- **K9, troops-return:** not in W4-D's files (SettleTicket/cohorts: the integrator; Dissolve's return: W4-A).
- `PENDING-OWNER`: none for this unit (test-beacon key only). Verification with the real quicknet key over real rounds waits for O-M1-12's archive (W6/W7); the verifier's default key is quicknet's, `--test-key` selects the test key.
