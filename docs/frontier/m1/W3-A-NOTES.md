# W3-A program-land — notes

- **Unit:** W3-A (wave 3), branch `frontier/m1-W3-A` cut from `frontier/m1-integ` at `241c52d`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.3: §4, §5.1–§5.6, §5.9, §6, §11 (W3-A brief), §12 Gate W3, §13.1–§13.3.
- **Owned paths touched (all of them, nothing else):** `permutation-frontier/src/proc/{map,citizen}.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{map,citizen}.rs`; `permutation-frontier/svm-tests/src/world/land.rs`; `permutation-frontier/svm-tests/tests/{map,citizen}.rs`; this file.
- **Integrator-owned files:** none changed. **Dependency requests:** none (no new crate).
- **Tags:** [measured] = run on this machine on 2026-09-28 (LiteSVM 0.16, the harness's SIMD-0186 enforcement, `solana-cargo-build-sbf 3.1.9`, SBPF v2); [design] = the contract's rule as implemented; [estimate] as stated.
- **Not done, by rule:** no push, no devnet or mainnet transaction, no rustup target, Playwright, drand download or Agave install; no service started (every test runs in LiteSVM in process).

## 1. What landed

### Program (`permutation-frontier/src/proc/`)

| Instruction | File | Summary |
|---|---|---|
| 0x20 OpenRing | `map.rs` | status Seeded/Running (I-46); `d == rings_opened ≤ r_max`; `d ≤ g` → RingSeed seeded at once with `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))` (I-30); `d > g` → Running, a bell since the last opening, a wedge at `θ` on the folded values, every wedge fund ≥ `d × rent(4,096)` above rent; RingSeed requested with `ring_seed_round(now)`; `RING_OPEN` (Frontier) |
| 0x21 ConsumeRingSeed | `map.rs` | K prologue, top-level; status 1 → verify → `seed_of`, status 2; `RING_SEED` (no chain, W1-E) |
| 0x22 OpenProvince | `map.rs` | status Seeded/Running; `ProvinceCoord::checked`; THE RingSeed seeded; the wedge fund (wedge 0 for the Concord, G10); Province created from the fund; `terrain::generate_province` encoded (§4 below); rings 0–1 sites `reserved`; ring ≥ 2 initial camp `camp::place(…, initial)`; fund counters (`open_sites`, `provinces_opened`, `provinces_funded`, `spent_total`); RingSeed `provinces_created`; **no Frontier write** (I-48); `PROVINCE_OPEN` (Province) |
| 0x23 FoldOccupancy | `map.rs` | three parts (v1.2): 24 shards (factions 0–2), 24 shards (3–5), 6 funds; `FoldStale` across bells or out of order; `FOLD` (Frontier) |
| 0x24 CloseProvince | `map.rs` | Ended ∧ `now ≥ end + 72 h`, or Aborted; `CLOSE` then close into the wedge fund; `provinces_opened −= 1` |
| 0x30 Join | `citizen.rs` | Running, `bell < join_close_bell`; join gate (I-51); faction, session expiry; Citizen absent (present: `AlreadyDone`); JoinShard `(faction, sha256(wallet)[0] mod 8)`; capacity on the folded Frontier; Citizen init (payer funds, `rent_payer`, bucket full, 3 floor explorations, no ticket); `members += 1`; `JOIN` (Citizen, JoinShard) |
| 0x31 SetSession, 0x32 SetVigil | `citizen.rs` | the on-chain **player prologue** ([`player`], §5.6 steps 1–4 through `frontier_abi::prologue` plus the Season PDA recompute of H2); SetSession wallet only; SetVigil `start_min < 1,440`, effective at `siege::change_effective_at(now)` (CL-09), weekly rule (§4); `SESSION`, `VIGIL` |
| 0x33 FileTicket | `citizen.rs` | no first holding, no open ticket (`TicketState`); per site: coordinates, ring ≥ 2 (`ReservedSite`), ring < `rings_opened`, site index < `site_count`, no duplicate, the wedge rule, its Province present; cohort for `now_bell` in each distinct Province (`CohortFull`); escrow top-up to `rent(1,280)` by the payer (I-47); `TICKET` (Citizen + m Provinces) |
| 0x34 SettleTicket | `citizen.rs` | fresh / displace / taken / expired (I-47, §5.9); seed from THE anchor's cache (any nonce) or the archive entry; pinned score; fresh funded from the Citizen's escrow; displacement rewrites the Holding in place (`gen + 1`), pays the new escrow to the displaced rent payer (`pay_or_divert`, sink `pool_owed`), reverts the displaced Citizen and its JoinShard; cohorts marked when the ticket ends; `SETTLE` |
| 0x35 ReleaseDormant | `citizen.rs` | first holding, `release_after` since the last owner action, no transit in state 1–3; site released-free (gen kept), refugee Citizen, JoinShard `holdings −= 1`, `released += 1`; `pool_owed` → DefencePool (`POOL_SWEEP`); `RELEASE`, `CLOSE`; Holding closed to its rent payer |
| 0x36 CloseHolding, 0x37 CloseCitizen | `citizen.rs` | season-end closes (Ended + 72 h, or Aborted); `pool_owed` → DefencePool; Citizen escrow → `ticket_funder` (listed exactly when there is escrow); `CLOSE` before every close (v1.3) |

Trace checkpoints (free outside `--features trace`): 200–202 around the terrain kernel in OpenProvince, 300–304 in FileTicket (the §2 profiles). Helpers kept `pub(crate)` for later units: `citizen::player` (the player prologue), `citizen::seed_of_bell` (THE cache or the archive entry of `(bell, region)`), `map::{present_at, emit_close, season_end_close, init_funded_after}`. Pure helpers with host tests: `map::{encode_terrain, terrain_digest, genesis_ring_seed, ring_occupancy_met}`, `citizen::{ticket_score, displaces, wedge_allowed, cohort_file, cohort_settle, founded_holding}`.

### Tests (`permutation-frontier/svm-tests`)

| File | Tests | What they assert |
|---|---|---|
| `tests/map.rs` | 11 | genesis rings (seeds, records, Frontier chain, order, `AlreadyDone`, `BadData`); OpenProvince writes exactly the kernel's land for all 12 ring-2 provinces (terrain block, digest, camp = `camp::place`, masks, fund counters, RingSeed count, `PROVINCE_OPEN` fields, Province chain), the Concord and a Seat; ring beyond `g` (`TooEarly`, `Capacity`, `Insufficient`, `ring_seed_round`, ConsumeRingSeed `WrongRound`/`TooEarly`/`Crypto`/`AlreadyDone`); status sets (Seeded before genesis, Created, Ended, Aborted); the three fold parts and `FoldStale`; CloseProvince (`TooEarly`, Aborted at once, `CLOSE` payload and chain); **G2** RingSeed both paths, Province funded path; **re-creation** of Province and RingSeed after close (`WrongStatus`); **G3** forgeries of Frontier, RingSeed, ProvinceFund, Province, JoinShard; **G1** `g01_open_province_worst_of_rings_2_to_10` |
| `tests/citizen.rs` | 19 | Join (every refusal incl. gate, capacity, ruleset, before genesis, after `join_close_bell`); sessions and vigils through the player prologue (`Auth`, `SessionExpired`, `Cooldown`, `Bucket` at the 61st action, `WrongStatus` after `end_bell`); G3 of the player prologue (non-canonical Citizen and Season copies, forged magic/season/owner, another wallet's Citizen); a fresh settlement end to end (escrow, Holding fields, founded kit and production, final_ts, mirror, cohorts, JoinShard, SETTLE, chains, fold counts it); **cohort tests**: displacement with no deadline while the Province is held through `final_ts` (finality waits, then is due), taken and exhausted, refile, `AlreadyDone`; displacement inside one JoinShard; expiry; a full cohort table (`CohortFull`); SettleTicket forgeries; ReleaseDormant (transit, pool sweep, gen kept, **re-creation of the Holding** by a new ticket with gen 2, refugee refile); season-end closes (**re-creation of the Citizen refused**); **G2** Citizen (Join), Holding (SettleTicket fresh); **G1** Join (1,001 wallets), FileTicket (3 provinces, full cohort tables), SettleTicket (displacement, 14 accounts), and every other W3-A instruction |
| `src/world/land.rs` | — | land builders (rings, provinces, folds, citizens, tickets, seeds, settlements), the pinned encodings recomputed natively (`terrain_block`, `terrain_digest`, `ticket_score`), crafted season status and trace-build seeds, and **`ChainTrack`**: follows a chained account by link continuation (a SETTLE with a displacement carries two Citizen links; `records::ChainWatch` takes the first link of a kind — the "ChainWatch link matching" item integ-W2 deferred to W3-A/W3-B) and accepts a closed account whose last record is `CLOSE` |
| `src/ix/{map,citizen}.rs` | — | re-exports of `fclient::ix` builders and account positions for forgeries |
| `src/cover/{map,citizen}.rs` | — | G13 registry for the 13 instructions; codes not reached in wave 3 stay `Pending("W5-A: …")` |

Host tests in the program crate: 10 new (`proc::map`, `proc::citizen`).

## 2. Measurements [measured, 2026-09-28]

Release `.so` of this branch: **405,832 B** (was 237,384 B at the wave-2 merge: terrain, camp, holding and catalog kernels now linked), `max_len` 507,904; `build-frontier.sh --twice` reproducible: `file_sha256 f06132de34d88d16f6619c32c56e8ec4931dd2370c93548e46f644ae13b215e9`, `program_hash 0f0fa3c924d10a598f285740fc79da1d1913a724c25147f4d94a62e43b86aed8`.

| Instruction (worst fill) | CU | Budget | tx B | locks | loaded B (L) | Verdict |
|---|---|---|---|---|---|---|
| **OpenProvince** (worst of all 324 provinces of rings 2–10 × 6 wedges: (−4, 9), 12 sites, camp) | **284,896** | 220,000 | 388 | 8 | 514,933 (1 MiB) | **over budget (F2)** |
| OpenProvince, same sweep, **with the kernel patch of F2** (scratch build) | **148,401** | 220,000 | 388 | 8 | 510,837 | within |
| Join (1,000 seeded wallets + the adversarial one) | 12,066 | 25,000 | 426 | 8 | 515,445 | within |
| **FileTicket** (3 sites, 3 provinces, 7 open cohorts in each) | **16,048** | 14,000 | 467 | 10 | 528,053 | **over budget (F3)** |
| SettleTicket (displacement, 3 ticket provinces, displaced triple: 14 accounts) | 21,954 | 40,000 | 649 | 16 | 530,389 | within |
| OpenRing (ring 4, the `d > g` path) | 15,176 | 30,000 | 551 | 13 | 516,277 | within |
| FoldOccupancy part 0 / 1 / 2 | 28,548 / 28,702 / 10,365 | 30,000 | 1,078 / 1,078 / 484 | 29 / 29 / 11 | ≤ 522,727 | within (5% margin on parts 0–1) |
| SetSession / SetVigil | 5,223 / 5,130 | 6,000 | 326 / 288 | 5 | 514,919 | within |
| ReleaseDormant | 10,690 | 25,000 | 450 | 10 | 521,127 | within |
| CloseHolding / CloseCitizen (escrow) / CloseProvince | 6,101 / 6,092 / 5,430 | 15,000 / 10,000 / 10,000 | ≤ 351 | ≤ 7 | ≤ 518,823 | within |

CU profile of OpenProvince on the trace build (province (1, 1)): `terrain::generate_province` alone **245,243 CU**; everything else (checks, addresses, funded init, encoding, digest, record) ≈ 21k. Profile of FileTicket (trace, the worst fill): dispatch ≈ 2.1k, player prologue ≈ 3.6k, Frontier ≈ 0.6k, sites ≈ 1.0k, three Provinces with their cohorts ≈ 4.0k (≈ 3.7k after the single-pass cohort table), escrow CPI ≈ 2.2k, Citizen writes and the 4-chain `TICKET` ≈ 3.5k. Heap peaks on the trace build (`PSF_TRACE=1`): OpenProvince 1,168 B, Join 1,312 B, FileTicket 1,672 B, SettleTicket 2,704 B (≤ 28,672 B). Trace-build CU: OpenProvince 285,990, Join 13,162, FileTicket 17,546, SettleTicket 22,469 (the trace markers add ≈ 1k).

## 3. Findings for the integrator and other units

- **F1 — `init::init_funded` fails on chain (blocker for its callers, fixed on my side).** It moves the shortfall out of the fund by direct lamport arithmetic *before* its AllocateWithSeed CPI; the target's credit is synced into the CPI but the fund's debit is not (the fund is not a CPI account), so the runtime refuses the caller with `UnbalancedInstruction` ("sum of account balances before and after instruction do not match") [measured, every OpenProvince before the fix]. W3-A's two callers (OpenProvince, SettleTicket fresh) use `map::init_funded_after` instead: the CPI first on untouched accounts, the lamports afterwards between two program-owned accounts. **Request:** fix or remove `init::init_funded` (W2-A's `init.rs`, not mine) so no later unit calls it.
- **F2 — OpenProvince exceeds its 220k budget: 284,896 CU [measured]; the cost is the terrain kernel, and an outcome-identical kernel change brings it to 148,401 CU [measured].** `generate_province` spends most of its time in `geometry::tile_offset` / `tile_index` (linear scans called thousands of times by the site search, the gate paths and the wedge turn) and hashes the same `frontier/terrain` base 61 times. The patch in Appendix A tabulates `tile_offset`/`tile_index` as const tables and hoists the base hash out of the tile loop. **Verified bit-identical** on chain against the unpatched kernel for all 331 provinces of rings 0–10 (the scratch-built `.so` against the harness's own kernel), `permutation-rules` host tests all green in the scratch copy. `permutation-rules/src/frontier/{geometry,terrain}.rs` are not W3-A's files: **request** the integrator (or the kernel owner) applies it; no `TERRAIN_VERSION` bump is needed (outcomes unchanged). Without it the gate item "OpenProvince ≤ 220k" fails, and the keeper cannot open genesis provinces (F6).
- **F3 — FileTicket exceeds its 14k budget: 16,048 CU [measured] at the §13.1 fill** (3 provinces, full cohort tables). The single-pass cohort table saved ≈ 300 CU; the rest is the player prologue, three canonical Province addresses, the escrow's System CPI (≈ 2.2k) and the 4-chain record, none of which can go. **Request (amendment):** FileTicket 14k → **17k** (measured + 5%, as the budgets table defines the keeper/client limit). FileTicket is class P (relay-sponsored, no liveness role).
- **F4 — SETTLE's chain list when both JoinShards are one account.** Same-faction displacement is the common case and the two citizens share a JoinShard with probability 1/8; the program then chains that JoinShard **once** (one account, one write; `holdings` nets out). `frontier_abi::log::chains_of(SETTLE)` lists `JoinShardOf(displaced)` as non-optional, so the verifier (W4-D) must accept one JoinShard link when both resolve to the same address. **Request:** mark that link optional in `chains_of` (frontier-abi) or document the rule for W4-D. Test: `citizen_cohort_displacement_within_one_join_shard`.
- **F5 — Join cannot read the wedge funds.** §5.9's capacity clause "or a ring can still open (`rings_opened ≤ r_max` and every wedge fund covers it)" names funds Join does not list (§5.9 account list, `frontier-abi` table). Implemented as `rings_opened ≤ r_max`; the fund condition is left out (the class-D OpenRing checks it). Amendment request: drop the fund clause from §5.9 Join or add the funds (6 more read locks per Join).
- **F6 — keeper (W3-C) misreads CU exhaustion.** With this program, `keeper::one_day_beacons` against the test-beacon `.so` fails at "every genesis province" (1 of 37 opened) [measured, 24.3 game h run]: OpenProvince exceeds the keeper's 220k limit and LiteSVM 0.16 reports SBPF v2 CU exhaustion as `InstructionError(_, ProgramFailedToComplete)` with "exceeded CUs meter" only in the logs, so `engine::is_cu_exceeded` misses it and `is_heap_fault` takes the heap rung and then gives up. For W3-C: classify by the logs too. F2's patch removes the trigger for OpenProvince.
- **F7 — the contract's FileTicket check `ticket_bell ≠ now_bell` is vacuous** once `ticket_bell == u32::MAX` is required: a ticket can only end after its bell's seed, so no ticket ends in the bell it was filed. Nothing implemented; noted in the doc comment.
- **F8 — encodings the verifier and herald must share (request to move them into `frontier-abi` or the kernel):** the Province terrain block and the PROVINCE_OPEN digest, and the ticket score (§4). They live as pure functions in `proc/{map,citizen}.rs` (host-tested) and are recomputed independently in `svm-tests/src/world/land.rs`; `proc` is `program`-feature code, so `frontier-node` cannot link them.
- **F9 — for W3-B (resident prologue):** the lazy provisional → final flip and its `HOLDING_FINAL` record are the resident actions' (§5.6 step 5); SettleTicket does not flip (it never needs to: a displaceable holding is by definition in an open cohort). `citizen::player` is the player prologue on chain, ready to call.
- **F10 — `SiteTaken` (11) has no path**: a taken site is a success with `ticket_next += 1` (§5.9 note). Candidate for the G13 exempt list or a reserved code (W5-A).

## 4. Rules and encodings pinned here (the contract leaves them open)

| Item | Rule |
|---|---|
| Province terrain block | `terrain[i] = Terrain as u8` (Grassland 0 … Water 5); `resource[i] = 0` or `1 + TileResource as u8`; `sites`, `site_count` as the kernel's; `passable_mask` bit i = `is_passable`; `rough_mask` bit i = `defense_bps < BPS_ONE` (the clash kernel's rough test: Forest, Hills); `road_mask`, `explored_mask` 0 |
| PROVINCE_OPEN digest | `sha256("PSF-TERRAIN-v1" ‖ province[128..296])` (the block above); `camp_tile = 0xFF` when none; `reserved = 1` for rings 0–1 |
| Concord | stored `wedge = 6` (none); funded from, and closed into, wedge 0's fund (G10) |
| Camp | `troops` whole troops (kernel `Camp`); `next_check_day = day + 1`; `gen = 1` for the initial camp |
| Site mirror at open | state free (rings ≥ 2) or reserved (rings 0–1), faction NEUTRAL (6), pending-garrison bells `u32::MAX` |
| Ticket score | `rng::rand(S, "site", le32(P) ‖ le32(Q) ‖ site ‖ le64(citizen_tag))`; tie → lower `citizen_tag` |
| Site generation | every founding bumps `gen` (first holding of a site: gen 1; released and displaced sites bump again), so old hosts are stranded |
| Founded holding | `Holding::found(now, day, 1)`, `production = catalog::base_production(Hamlet)` (the sim's `found`; no doctrine production multiplier exists, O5), food upkeep 0, `catalog::starter_kit()` credited; queue written as zeros; mirror garrison 0; `shield_until_bell` = first bell starting at or after `shield_until` |
| Ticket site count | the leading non-zero `ticket_sites` entries (all-zero = the Concord's site 0, never a ticket site) |
| SettleTicket account count | 9 + other provinces (0–2) + 3 × displaced (0–1): the count decides both groups uniquely; the displaced triple must be present exactly for a displacement (`TooManyAccounts` otherwise); the other provinces are the ticket's distinct Provinces except site k's, first-seen order, on every call |
| Seed source | account 6/7 = the archive `aa‖(r, part(b))` → the archive entry (archived bit, `A = bell_end(b) + a_off`, `S = S(A)`); otherwise a SeedCache naming THE anchor with THE anchor's `A` and round `S(A)` (any nonce); missing anchor `NoAnchor`, missing cache or entry `SeedNotReady` |
| Vigil weekly rule | the Citizen stores no request time: refused (`Cooldown`) until `vigil_from_ts + 6 d`, never earlier than the kernel's `last_request + 7 d`; a change in force is folded in first |
| Ring occupancy | a wedge qualifies only with open sites: `open > 0 ∧ occupied × 10,000 ≥ θ × open` (with `0 ≥ θ·0` an unfolded Frontier would open rings at once) |
| FOLD payload | occupied fields = the accumulators after part 0, the folded values after parts 1–2; open fields = the folded values |
| CLOSE payload | `final_seq`, `final_head` = the chain before the CLOSE record; the record's tail link is the chain's last advance |
| ReleaseDormant | clears Citizen flags 4 and 8 (the refugee may file again), sets 16 |
| Refusal codes | present target → `AlreadyDone`; `d > rings_opened`, a bell too soon, before `end + 72 h` → `TooEarly`; ring seed absent or unseeded → `SeedNotReady`; `θ` not met → `Capacity`; fund short → `Insufficient`; FileTicket site rules → `BadData`; ticket/holding state → `TicketState`; release preconditions → `NotDormant` (46); vigil weekly → `Cooldown` |

## 5. Gate W3 items that concern these files

| Item | Result |
|---|---|
| `(cd permutation-frontier/svm-tests && ./run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_)` (full build of the four variants and the probe) | **exit 0, 51 passed, 0 failed** (W3-A's `map_`, `citizen_`, `g02_`, `g03_` and wave 2's `g02_`/`g03_`; `g06_`, `holding_`, `host_`, `reveal_` select nothing on this branch: W3-B/W4-A) |
| `(cd permutation-frontier/svm-tests && PSF_TRACE=1 ./run.sh --release -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture)` | **fails**: `g01_join`, `g01_settle_ticket` pass; **`g01_open_province` (284,896 > 220,000 CU) and `g01_file_ticket` (16,048 > 14,000 CU) fail on their budgets (F2, F3)**; `g01_reveal_worst` selects nothing (W3-B's). As written the command stops at the first failing binary (W2-B finding 5); run with `--no-fail-fast` to see both |
| Pass: cohort tests (displacement without a deadline, Province held through `final_ts`, expiry) | green: `citizen_cohort_displacement_has_no_deadline_and_finality_waits`, `citizen_cohort_expiry_ends_the_ticket` (+ `…_within_one_join_shard`, `…_table_full_…`) |
| Gate W2 for the program: `cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings`, `cargo test --locked -p permutation-frontier --no-default-features`, root `cargo fmt --all -- --check` | pass |
| `scripts/build-frontier.sh --twice` | exit 0, identical hashes (§2) |
| svm-tests `cargo fmt -- --check`, `cargo clippy --locked --all-targets -- -D warnings` | pass |
| Full svm suite `./run.sh --release --no-fail-fast` (existing wave-2 tests unchanged) | every existing file green (g01_budget_w2a, g01_loaded_limit 13, g02_prefund 10, g03_forgery 12, g04 6, g05 7, g13 5, harness 6, season_records 1, coverage 2, lib 10); new files: all green except the two budget tests |
| `keeper::one_day_beacons` against this program (not a W3-A gate line; the ring part is exercisable from Gate W3) | fails at "every genesis province" (F6, caused by F2) |
| `(cd frontier-node && cargo test --locked --release --workspace)` | not W3-A's files; not re-run here (W3-C/W3-D/W3-E change that tree in this wave) |
| `PENDING-OWNER` items | none in W3-A's scope |

## 6. Deviations

| # | Deviation | Why | Who resolves |
|---|---|---|---|
| D1 | OpenProvince and SettleTicket create with `init_funded_after`, not `init::init_funded` | F1 | integrator (fix `init.rs`) |
| D2 | Join's capacity clause without the fund condition | F5 | contract amendment |
| D3 | SETTLE chains a shared JoinShard once | F4 | frontier-abi / W4-D |
| D4 | Vigil weekly rule measured from `vigil_from_ts` | no request time in the frozen Citizen layout (§4) | owner/contract, if the exact kernel rule is wanted (needs a Citizen field) |
| D5 | Ring occupancy needs `wedge_open > 0` | §4 (an unfolded Frontier would pass `0 ≥ 0`) | contract text |
| D6 | Crafted state in tests: the Season's status byte (EndSeason/AbortSeason are W4-B's), the Frontier's folded `wedge_occupied` (instead of ≈ 55% of a wedge settled), RingSeeds of rings 4–10 for the budget sweep, a fund's lamports, a Holding's transit record and `pool_owed`, an anchor + cache on the trace build | the creating instructions belong to other waves, or the state needs hundreds of settlements; each crafted account is written in the frozen layouts and named in its test | W4-B replaces the status crafts once EndSeason/AbortSeason land |
| D7 | Loaded-limit tests (`g01_loaded_limit_<kind>`) for the 13 W3-A kinds are not added (the `check` helper lives in W2-B's `tests/g01_loaded_limit.rs`); every W3-A measurement asserts `loaded ≤ L(kind)` instead | ownership | W5-A (G1 completion) |

## 7. Commands run (from the worktree root unless stated)

```
cargo build -p permutation-frontier; cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings   # pass
cargo test --locked -p permutation-frontier            # 48 pass (10 new)
cargo test --locked -p permutation-frontier --no-default-features   # 36 pass
cargo fmt --all -- --check                             # pass
scripts/build-frontier.sh [--features test-beacon|trace]            # release 405,832 B deployable; test-beacon, trace
(cd permutation-frontier/svm-tests && cargo fmt -- --check && cargo clippy --locked --all-targets -- -D warnings)   # pass
(cd permutation-frontier/svm-tests && PSF_SKIP_BUILD=1 ./run.sh --release --no-fail-fast)   # all green except g01_open_province, g01_file_ticket
(cd permutation-frontier/svm-tests && PSF_SKIP_BUILD=1 ./run.sh --release --test citizen g01_land_other -- --nocapture)   # §2 table
(cd frontier-node && PSF_FRONTIER_SO=<abs>/deploy-test-beacon/permutation_frontier.so cargo test --locked --release -p keeper --test one_day_beacons -- --nocapture)   # F6
# F2 experiment (scratch copy of the root tree, patch of Appendix A):
cargo test --locked --release -p permutation-rules      # all pass (scratch)
scripts/build-frontier.sh; scripts/build-frontier.sh --features test-beacon   # scratch
PSF_SO=<scratch> PSF_SO_TEST_BEACON=<scratch> cargo test --locked --release --test map   # 11/11, worst 148,401 CU
(temporary test, deleted) 331 provinces of rings 0–10 opened by the scratch .so == the unpatched kernel's block
# Gate runs:
scripts/build-frontier.sh --twice                      # exit 0, §2 hashes
(cd permutation-frontier/svm-tests && ./run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_)   # exit 0, 51 passed
(cd permutation-frontier/svm-tests && PSF_TRACE=1 PSF_SKIP_BUILD=1 ./run.sh --release --no-fail-fast -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture)   # 2 pass, 2 fail (F2, F3)
```

## Appendix A — the kernel patch of F2 (for the integrator; not applied here)

Outcome-identical (verified on 331 provinces on chain and by the kernel's own tests). `permutation-rules/src/frontier/geometry.rs`: `tile_offset` and `tile_index` read two const tables built by the same loops; `permutation-rules/src/frontier/terrain.rs`: `canonical` computes `rand(seed, "frontier/terrain", [])` once and passes it to a private `tile_at_base`, `tile_at` keeps its signature.

```diff
--- permutation-rules/src/frontier/geometry.rs
+++ permutation-rules/src/frontier/geometry.rs
 pub fn tile_offset(idx: u8) -> Option<Hex> {
+    let (q, r) = *TILE_OFFSETS.get(idx as usize)?;
+    Some(Hex::new(q as i32, r as i32))
+}
+
+const TILE_OFFSETS: [(i8, i8); 61] = {
     const R: i32 = PROVINCE_RADIUS;
-    let mut i = idx as i32;
-    for q in -R..=R {
-        let r_min = (-R).max(-q - R);
-        let r_max = R.min(-q + R);
-        let n = r_max - r_min + 1;
-        if i < n {
-            return Some(Hex::new(q, r_min + i));
+    let mut t = [(0i8, 0i8); 61];
+    let mut n = 0;
+    let mut q = -R;
+    while q <= R {
+        let r_min = if -R > -q - R { -R } else { -q - R };
+        let r_max = if R < -q + R { R } else { -q + R };
+        let mut r = r_min;
+        while r <= r_max {
+            t[n] = (q as i8, r as i8);
+            n += 1;
+            r += 1;
         }
-        i -= n;
+        q += 1;
+    }
+    t
+};
+
+const TILE_INDEX: [[u8; 9]; 9] = {
+    let mut t = [[255u8; 9]; 9];
+    let mut i = 0;
+    while i < 61 {
+        let (q, r) = TILE_OFFSETS[i];
+        t[(q + 4) as usize][(r + 4) as usize] = i as u8;
+        i += 1;
     }
-    None
-}
+    t
+};

 /// Tile index of an offset from the province centre.
 pub fn tile_index(o: Hex) -> Option<u8> {
-    const R: i32 = PROVINCE_RADIUS;
-    if o.radius() > R as u32 {
+    if o.q < -4 || o.q > 4 || o.r < -4 || o.r > 4 {
         return None;
     }
-    let mut base = 0;
-    for q in -R..o.q {
-        base += R.min(-q + R) - (-R).max(-q - R) + 1;
-    }
-    Some((base + o.r - (-R).max(-o.q - R)) as u8)
+    let i = TILE_INDEX[(o.q + 4) as usize][(o.r + 4) as usize];
+    (i != 255).then_some(i)
 }
--- permutation-rules/src/frontier/terrain.rs
+++ permutation-rules/src/frontier/terrain.rs
 pub fn tile_at(seed: &Seed, c: Hex) -> (Terrain, Option<TileResource>) {
-    let base = rand(seed, b"frontier/terrain", &[]);
+    tile_at_base(seed, rand(seed, b"frontier/terrain", &[]), c)
+}
+
+fn tile_at_base(seed: &Seed, base: u64, c: Hex) -> (Terrain, Option<TileResource>) {
     let at = |salt: u64, cell: i64| {
@@ fn canonical
+    let base = rand(seed, b"frontier/terrain", &[]);
     for i in 0..PROVINCE_TILES as u8 {
         let mut g = pc.tile(i).unwrap_or(Hex::ORIGIN);
         if concord {
             g = g.turned(g.sextant()); // six-fold symmetric by itself
         }
-        let (t, r) = tile_at(seed, g);
+        let (t, r) = tile_at_base(seed, base, g);
```
