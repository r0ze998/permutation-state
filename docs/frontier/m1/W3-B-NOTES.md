# W3-B program-holding-march: notes

**Unit:** W3-B program-holding-march (wave 3). **Branch:** `frontier/m1-W3-B`, cut from `frontier/m1-integ` at `241c52d`. **Contract:** M1-CONTRACT v1.3 §3, §4, §5.1, §5.3–§5.6, §5.10, §5.11 (Depart, Reveal, SettleDeparture), §6, §10.1, §11 (W3-B row), §12 (Gate W3), §13.1–§13.3. **Date:** 2026-09-28.

## 1. What landed

| Path | Content |
|---|---|
| `permutation-frontier/src/proc/holding.rs` | **Harvest, Build, Train, Explore, SettleExplore**; the shared player prologue over `AccountInfo`s (`player`: Season at its recomputed PDA, Running, ruleset; citizen and actor through `frontier_abi::prologue::player_prologue`; bucket and end bell; `last_action_ts`), `owned_holding`, `own_province`, the lazy finality flip with `HOLDING_FINAL`; the **Holding ↔ kernel `holding::Holding` codec**; the holding touch; the bell-seed reader (`seedcache\|archive`, `anchor\|archive`) |
| `permutation-frontier/src/proc/host.rs` | **Muster, Dissolve, Garrison, DisbandStranded, Depart, SettleDeparture**; `resident_host` (the §5.6 step-6 / §5.10 resident checks) |
| `permutation-frontier/src/proc/reveal.rs` | **Reveal** (class W, top-level only), with the compile-time step table, the specialised account-flag check, the key-cached quota and the direct REVEAL record (§3 below), each with a host equivalence test against the ABI/kernel function it replaces |
| `permutation-frontier/svm-tests/src/world/holding.rs` | crafted Province/Citizen/Holding (W3-A's instructions are the same wave), the Holding codec for tests, hosts, anchors, slots, days, resolves stood in for (`resolve_through`), marches and seals (`plan_march`, `depart_ix`, `reveal_ix`), paths (`trace`, `padded_line`, `open_path`, `find_path`) |
| `permutation-frontier/svm-tests/src/{ix,cover}/{holding,host,reveal}.rs` | builders (fclient's) and forgery positions; the G13 registry rows |
| `permutation-frontier/svm-tests/tests/{holding,host,reveal}.rs` | 36 program tests (§5 below) incl. `g01_reveal_worst`, `g01_budget_w3b_{holding,host}`, G2/G3/G4/G9 for Reveal |

No manifest, lock, toolchain or `.gitignore` change: **no dependency request**. No file outside §11's W3-B row was edited.

## 2. Pinned here (the contract leaves these open; please copy to DECISIONS if accepted)

| # | Item | Choice |
|---|---|---|
| P1 | Holding codec | `tier` u8 = `holding::Tier` in declaration order (0 Hamlet … 3 Stronghold); queue item `kind` 0 free, 1 `Production{resource = arg, delta}`, 2 `Upkeep{resource = arg, delta}`, 3 `TierUp`, 4 `Walls{delta}`; every other kernel field one to one at its §5.3 offset; `shield_until` = kernel `shield_until()`, flags bit 0 = `is_dormant(now)` at the last write. **Request:** the integrator lifts `queue_kind::*` into `frontier-abi` (herald, verifier, web decode the same bytes). |
| P2 | Holding touch (every owner action) | kernel `settle` split at each finished `TierUp`, adding `base_production(new) − base_production(old)` from the tier-up's completion (the simulator's `apply_tier_bonus`) and re-applying rates (`set_upkeep(t, Food, same)`, the simulator's idiom); then `touch_owner(now)`; then `commit_walls(now)`. The verifier replays the same sequence. |
| P3 | Build items | 0–5 `catalog::BUILDINGS`, 6 walls, **7 tier-up** (`catalog::tier_up_item`, one queued at a time, else `QueueFull`). A building's copy number `n = 1 + (production[r] − base_production(tier)[r]) / per_hour + queued copies` (the six kinds produce six distinct resources). **Request:** `ITEM_TIER_UP = 7` as a `frontier-abi` constant (web build panel). |
| P4 | Walls in the site mirror | `{effective_bell = bell_at(done_at) + 1, delta}` in a free item after folding items with `effective_bell ≤ resolved_next` into `walls_committed` (capped 1,200); both busy → `QueueFull`. |
| P5 | Units | `reserve` whole troops; entries, site mirror, transit and slots `MilliTroops`; Muster/Garrison/Train data whole troops. |
| P6 | Muster | tile < 61 and passable (`BadData`); caps over states 1–2: 48 total and 8 per faction, then a free entry, all `ProvinceFull`; kernel bounds (100–30,000) `Kernel` (sub-codes 0x20/0x21); entry `dealt_bps` = faction doctrine, Hold, not arriving; `n_entries` counts non-free entries (Muster +1, SettleDeparture and DisbandStranded −1). |
| P7 | Garrison | draws on `reserve[Spearman]` (the unit the simulator's garrison purchase prices). **Positive deltas only** (`BadData` otherwise): the program has no step that returns a withdrawal's post-clash troops to the Holding, and crediting them at issue would let a withdrawal issued during bell b keep troops that die in b's clash. Open item for the contract (§6 F4). |
| P8 | Depart | seal byte 0: compression flag set and infinity flag clear (`BadData`); `march_stamina(32)` = 74 charged (I-32); transit `dealt_bps` = doctrine, Hold, arriving (the sealed stance's multiplier goes into the slot at Reveal); `payer.lamports ≥ tip + fee + bond` else `Insufficient`; the escrow moves by System transfer into the Holding (`escrow +=`). |
| P9 | SettleDeparture | a transit in state 2 or 3 is `AlreadyDone` (keeper idempotency), state 0 `TransitState`; the entry must be `departed` (`NotResident` otherwise); `troops_after < 500` milli → state 3 and 0 troops; `ready_bell_off = ready_bell − depart_bell`. |
| P10 | DisbandStranded | an entry whose Holding is live (state 1–2) with the id's generation is `NotDormant` (46, "not stranded"); a free entry `BadData`; `STRANDED.troops_lost` in milli. |
| P11 | Explore | tiles within one hex of the host's tile (`BadData`), distinct; a single tile records `[t, NO_TILE]` whatever `tiles[1]` says (fclient's builder sends 0; §6 F2); the holding's explore record busy → `HostBusy`; a tile already in `explored_mask` → `Explored`. |
| P12 | SettleExplore | a cleared record is `AlreadyDone`; each tile is one exploration and the floor (`explores_floor_left`) is spent tile by tile; `explores += tiles`; `EXPLORE_RESULT.works_per_tile` = two u32. |
| P13 | Reveal path | from the transit's origin tile, `hex::DIRECTIONS` steps; the supplied path provinces must be exactly the distinct non-destination provinces the steps enter, in first-entered order (extra, missing or reordered: `Path`); `path_len = 0` is `Path`; cost from `passable_mask`/`rough_mask`/`road_mask` (bit = tile index; `travel::hex_secs` exactly), then the doctrine's travel bias, then `travel::check_arrival_bell` (`ArrivalBell`). |
| P14 | Reveal shield | destination tile = a site whose mirror shows another faction's holding with `shield_until_bell > arrive` → `Shielded`; or the host's holding is shielded at `bell_start(arrive)` (not dormant) and the destination is another faction's holding site → `Shielded`. |
| P15 | Reveal accounts | `target_i < 4` else `BadData`; exactly slot `target_i` writable (`BadAccount`); an anchor/archive/BeaconLog key that is a present account of its kind elsewhere is `WrongRegion`, any other wrong key `BadAddress`; a wrong-key Holding (absent elsewhere) `BadAccount`. |
| P16 | Reveal slot | `dealt_bps` = doctrine in the sealed stance, arriving; flags bit 2 (`created_day`) when this Reveal wrote the day's bit (cleared on displacement, with `claimed = 0`, `rent_to` kept); evidence = the ix-sysvar bid and the landing slot. |
| P17 | Records | `HOLDING_FINAL` precedes the action's record when the lazy flip happens (resident actions carrying the holding's own Province); `TRAIN.done_at` = now; `MUSTER.troops` whole troops; `DISSOLVE.delta` = the host's troops (milli); `GARRISON.delta` milli. |

## 3. Reveal: the worst-case measurement (Gate W3; C4 input, CL-22)

Release `.so` of this branch: **420,800 B**, `max_len` 528,384, ProgramData 528,429 B, SHA-256 `0f12ef61…ed85a82` (`build-frontier.sh --twice`: identical). Measured in LiteSVM 0.16 with the harness's SIMD-0186 accounting (`g01_reveal_worst`, `PSF_TRACE=1` prints the per-step profile from the trace build).

Fill (§13.1 Reveal row): **32 steps over 4 provinces** (a 22-step straight line that enters its fourth province on the last straight step, then 5 back-and-forth pairs across that border: a province re-location every pad step), Knight host of 30,000, stance Flank, retreat 25,000, THE anchor present (crafted as PostAnchor leaves it: the release build verifies only the 32 fixture rounds), BeaconLog read, adversarial slots (four of other citizens at 29,000 each, ranked by their slot keys).

| Fill | CU (release) | heap (trace) | tx | locks (writable) | loaded |
|---|---|---|---|---|---|
| **Named row** (displace the smallest of 4 full slots **and** create the ArrivalDay on a pre-funded address; not reachable in honest play, bounds both) | 24,291 | 2,680 B | 916 B | 20 (3) | 550,277 B |
| **First of the province-bell** (fill a slot, create the day; honest worst) | **25,140** | 2,930 B | 916 B | 20 (3) | 549,381 B |
| Displacement, day present (honest) | 20,341 | 2,176 B | 916 B | 20 (2) | 550,373 B |

- **Gate W3:** worst **25,140 ≤ 26,000 CU**, tx 916 ≤ 1,100 B, heap ≤ 28,672 B. The **20k target is not met** (+5.1k), after both cuts.
- **`L(reveal)`** at this `.so`: formula over the worst account set (`budgets::loaded_limit_for`) **589,824 B** (18 pages); measured need 550,277 B → **557,056 B** (17 pages); SIMD-0186 control: lands at 557,056, fails charged one page below (in the test).
- **Requested presets (amendment via the integrator):** `reveal_cu_limit = 26,000` (measured max 25,140 + 5% = 26,397 is above the budget, so the budget caps it; margin 3.4%); `reveal_loaded_limit = 589,824` at this `.so` (the formula; W5-A regenerates it from the release `.so` of wave 5, which will be larger: W3-A and wave 4 add code). **CL-22 tip:** `tip_min = ⌈0.433 × (26,000 + 1,320 + 8 × 18)⌉ + 2,500` = **14,392 lamports** (14,441 at the 1-MiB default; 14,389 at the measured 17 pages).
- **Profile of the honest worst** (trace build, CU incl. ≈ 230 per marker): entry + flags + decode 990; Clock + keeper prologue 1,061; holding, transit, plaintext 966; commitment 544; region + window (3 addresses, anchor, log, clock) 1,839; latch (2 addresses) 1,179; destination + 3 path provinces 2,044; walk 2,982 (≈ 90 per step); arrival bell 275; quota (4 slot addresses, reads, admit) 1,498 + 482; ArrivalDay 779; evidence 526; **the three CPIs of the two creations ≈ 5.3k** (one System transfer carrying both rents, two `AllocateWithSeed` signed by the Season PDA at ≈ 2.1k each: the PDA signer derivation is charged); slot fields 304; REVEAL record 714.
- **Cuts applied** (program-side, no layout change; each proved equal to the ABI/kernel function it replaces by a host test): (a) the per-province neighbour gates of program design §10 as one compile-time `STEP[61][6]` table (walk 15.0k → 3.0k; `step_table_is_locate`); (b) the salt instead of `k` was already in v1.1; plus a specialised account-flag check (−1.7k; `flags_are_check_flags`), key-cached `admit_arrival` (−1.1k on a full faction; `admit_is_the_kernels`), one-buffer addresses with the slot index patched (−1k; `addresses_are_the_abis`), an O(1) plaintext check (−0.9k; `plain_ok_is_validate`), the REVEAL body written directly (−1.2k; `reveal_record_is_the_abis`), one Rent read, and **three CPIs instead of four** for a first-of-bell Reveal (the transfer carries both rents to the slot; the day is allocated, then funded from the slot by lamport arithmetic; the payer pays exactly the two shortfalls, G2 test). Before the cuts the named fill measured 42,413 CU.
- **What remains above 20k:** ≈ 5.3k of CPIs (unavoidable without CreateAccount, which §3.3 forbids) and ≈ 4k of 13 canonical-address hashes (§3.3 requires each). A further ≈ 1k might come from a leaner walk; nothing structural is left inside W3-B's files.

## 4. G1 for W3-B's other instructions (§13.1 fill: full queue, 48-entry Province, max resources)

Measured on the release `.so` (`g01_budget_w3b_holding`, `g01_budget_w3b_host`, `--nocapture`). "Full queue": a City holding with three items (two due), stores of 10⁶ units; the host in entry 55 of a 48-used Province.

| Instruction | CU | budget | tx | Note |
|---|---|---|---|---|
| Harvest (full queue) | 16,578 | 12,000 | 319 | **over**; quiet holding 11,790 |
| Build (4th slot) / walls | 18,080 / 19,299 | 22,000 | 320 / 353 | |
| Train 30,000 (full queue) | 16,810 | 15,000 | 324 | **over**; quiet 12,014 |
| Explore (2 tiles) | 19,847 | 18,000 | 363 | **over** |
| SettleExplore (2 tiles) | 8,160 | 15,000 | 384 | |
| Muster / Dissolve / Garrison | 20,827 / 18,767 / 18,205 | 25,000 | 358–360 | |
| Depart (full queue) | 23,320 | 15,000 | 604 | **over**; quiet holding 16,595 (still over) |
| SettleDeparture | 6,611 | 15,000 | 319 | |
| DisbandStranded | 5,337 | 12,000 | 319 | |

- **Cause** (trace profile of Harvest, full queue): the kernel `holding::Holding::settle` ≈ 7.3k (`Accrual::settle` does an i128 `div_euclid` and `rem_euclid` per store per event: 8 stores × 3 events), the generic `check_accounts` ≈ 1.2k, the player prologue ≈ 2.2k, the Holding codec ≈ 1.5k read + 1.2k write, a record with 2–3 chains ≈ 1.6k. Depart adds the System transfer CPI (≈ 1.5k), the 165-B seal hash and the 278-B DEPART body hashed into three chains.
- **Status:** the tests assert every other ceiling (tx, locks, loaded, heap) and print these four CU breaches; `RELEASE_CHECK=1` (W5-A's release gate) asserts them too. **Recommendation** (integrator / owner): an i64 fast path in `Accrual::settle` (kernel owner; outcome-identical when `|rate × dt| < 2⁶³`) and a Depart budget amendment (15k cannot hold the prologue, the holding touch, a CPI and the DEPART record: 23,320 CU at the named fill, 16,595 with a quiet holding).

## 5. Tests

| File | Tests |
|---|---|
| `tests/holding.rs` | Harvest (kernel-equal stores, digest, chains, provisional allowed, released refused); Build (copy numbers, quadratic cost, `QueueFull`, production after completion, walls mirror item, tier-up and the new tier's base production); Build/Train/Explore refusals; Explore + SettleExplore (floor finds, works, record cleared, `SeedNotReady` before THE anchor and before the cache, `AlreadyDone`); a non-floor roll = `explore::roll`; `g01_budget_w3b_holding` |
| `tests/host.rs` | Muster (entry, host_seq, roster_epoch, n_entries, record, joins at the next resolve) and 9 refusals; the lazy finality flip + `HOLDING_FINAL`; Dissolve (`Leave`, `HostBusy`, `NotOwner`, `NotResident`, `HostInTransit`); Garrison (mirror pending, reserve, refusals); DisbandStranded (live/re-founded/closed, free entry); Depart (escrow, transit, Spend, DEPART record with commit and seal, ≤ 800 B) + SettleDeparture (`TooEarly`, `BadAddress`, values, entry freed, `AlreadyDone`, `TransitState`, destroyed at origin); Depart refusals (11 codes); player-prologue refusals (7 codes); `g01_budget_w3b_host` |
| `tests/reveal.rs` | fill + day creation (rents, fields, evidence, record, `AlreadyDone`); nothing else written; displacement / `SlotMoved` / `QuotaRefused`; **G9** order-free quota over 8 orders; **G4** (close at `A + W`, BeaconLog ≥ `S(A)` crafted and through PostBeacon, latch by present inputs and by `resolved_next`, pre-funded inputs address absent, tombstone, and a 24-case property over window lengths, anchor times, Clocks and log rounds); **G2** pre-funded slot and day (1, rent, 10× rent, one of them) paying only the shortfalls, pre-funded slots absent in the quota; **G3** forgeries of every account (wrong address, copies elsewhere, owner, magic, season, key fields, another region's anchor and log, path provinces missing/extra/reordered/copied/foreign-owned, impassable step, ix sysvar); refusals (11 codes); `ArrivalBell`; `NotTopLevel` by CPI; a garbage seal reveals (judged at SettleTransit, I-44); `g01_reveal_worst` |

Program host tests (`cargo test -p permutation-frontier`): 55 (15 in W3-B's files: codec round trip, tier-up bonus, copy numbers, explore adjacency, garrison mirror, seal syntax, walk vs `locate`, step table, flags, admit, addresses, plaintext check, REVEAL record).

## 6. Findings for other units and the integrator

- **F1 (W3-A, integrator): `init::init_funded` unbalances its CPI.** It moves lamports from the fund to the target by arithmetic and then calls `AllocateWithSeed` without listing the fund, so the runtime sees the target's gain but not the fund's loss: `UnbalancedInstruction` [measured here with a slot funding a day; OpenProvince uses the same pattern]. Fix: allocate first, then move (what Reveal does), or list the fund in the CPI's account infos.
- **F2 (W3-C): `fclient::ix::explore` sends `tiles[1] = 0` for one tile**, where the ABI says 0xFF. The program records `NO_TILE` either way (P11); the builder should follow the ABI.
- **F3 (W3-A): founding a Holding.** For the economy to run from the found, SettleTicket should found as the simulator does: `Holding::found(now, day, 1)`, `production = base_production(Hamlet)`, `set_upkeep(now, Food, 0)` (applies the rates), `STARTER_KIT` credited, in the P1 codec (tier byte 0, empty queue).
- **F4 (W4-A, contract): the Holding side of a settle.** Dissolve's `Leave` and garrison withdrawals must return post-clash troops to `Holding.reserve`, which ResolveFromInputs does not write; the contract names no step for it (P7 refuses withdrawals meanwhile). Troop food upkeep (the simulator's `refresh_upkeep`) is not modelled by any M1 instruction either.
- **F5 (integrator, W2-B's file): `g03_forgery::g03_announce_season_addresses_and_authority` fails once the release `.so` exceeds ≈ 418 KB:** the test adds a forged copy of the ProgramData, so the transaction loads two ProgramData (1,057,314 B) against the 1-MiB limit (`MaxLoadedAccountsDataSizeExceeded` before the program runs). This branch's `.so` is 420,800 B. Fix in the test: send that one case with `Profile::ladder(..).with_loaded(2 × L)`. Every other W2 test passes on this branch (full `run.sh --release --no-fail-fast`).
- **F6 (kernel owner): `Accrual::settle` CU** (§4).

## 7. Deviations

- Reveal does not call `prologue::check_accounts` or `clash::admit_arrival` or `AddrCtx` or `events::emit`: it calls equivalent specialised code (§3), each pinned to the original by a host test. The rule stays the kernel's (`quota_set`, verifier).
- Garrison refuses negative deltas (P7). Build accepts item 7 (P3). Codes chosen where §5.10 names none: P6, P10–P12.
- The four G1 CU breaches of §4 are reported, not fixed.

## 8. Commands run (this branch, 2026-09-28)

```
scripts/build-frontier.sh --twice                                  # same sha256 0f12ef61…, so 420,800 B, deployable yes
scripts/build-frontier.sh --features {test-beacon,trace}           # deployable no
cargo fmt --all -- --check                                         # pass
cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings   # pass
cargo test --locked -p permutation-frontier --no-default-features  # 36 pass
cargo test --locked -p permutation-frontier                        # 55 pass
(svm-tests) cargo fmt -- --check; cargo clippy --offline --locked --all-targets -- -D warnings   # pass
(svm-tests) ./run.sh --release --no-fail-fast                      # all pass except F5 (g03_announce_season_addresses_and_authority)
(svm-tests) ./run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_   # Gate W3 line: all pass except F5; g06_/map_/citizen_ match no test on this branch
(svm-tests) PSF_TRACE=1 ./run.sh --release -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture   # g01_reveal_worst pass (§3); the others are W3-A's
(svm-tests) cargo test --test coverage                             # G13 registry guard pass
```

Not run: `(cd frontier-node && cargo test --locked --release --workspace)` — no `frontier-node` file and no crate it depends on changed (it does not link `permutation-frontier` or load its `.so`). Nothing `PENDING-OWNER` in W3-B's items; no download, install, service or port used.
