# W4-A program-clash: notes

Unit W4-A of wave 4 (M1 contract v1.5 §11): GatherClash, ResolveFromInputs
(Phase A kernel, the `Occupancy` room, the full M1 write-back, fates, camps,
evidence), ResolveClash (`oracle`), SkipQuiet (with a CU-aware stop),
CloseClashInputs, CloseArrivalDay, CloseArrivalSlot, and the §21 item: the
return of troops after Dissolve. Gates G6, G8 (+ the storage fill), G9 (the
clash side), G11 (+ churned rosters), G1 (RFI over all 1,240 fills with the
full write-back, heap ≤ 28 KiB), G2/G3 rows of ClashInputs, and the G13 rows of
the clash area.

Branch `frontier/m1-W4-A` from `frontier/m1-integ` at `7289822`. Nothing
pushed, no server, no chain transaction; every test runs in LiteSVM.

## 1. What landed

| Path | What |
|---|---|
| `permutation-frontier/src/proc/clash.rs` | the seven instructions, the return settle, and `clash::model`: the clash of one province-bell as a pure function of account bytes (the `ClashInput` builder, the write-back, the settle of a bell's pending changes, the camp's daily check, the trivial quiet test, the digests), with host unit tests |
| `permutation-frontier/src/proc/host.rs` | **5 lines outside W4-A's files** (see §4 D1): SettleDeparture with `transit_slot = 0xFF` dispatches to `clash::settle_return` |
| `permutation-frontier/svm-tests/src/world/clash.rs` | the lab's fills ported to M1 accounts (`Fill::adversarial`, kinds 0–5), the I-43 storage fill (`Fill::storage`), crafting of destination Provinces, slots, transit Holdings, anchors and SeedCaches, gather/resolve/skip builders over chain state |
| `permutation-frontier/svm-tests/src/ix/clash.rs` | builder bundles (fclient's), the oracle's account list, the return settle, account positions for forgeries |
| `permutation-frontier/svm-tests/src/cover/clash.rs` | the G13 registry of the clash area |
| `permutation-frontier/svm-tests/tests/clash.rs` | 22 tests (+ 3 ignored `zz_profile_*` diagnostics that print the trace build's CU checkpoints) |

### Behaviour pinned here (v1.6 amendment requests, §4)

- **Inputs.** Residents = entries in state 1 with `from_bell ≤ b` at
  `Host::values_at(b)`, at Hold, stored `dealt_bps`. Garrisons = site
  mirrors in state 1 among `site_count` at `GarrisonState::at(b)`, walls when
  committed or an item with `delta > 0` is effective by b, id = the holding
  key (`host_id(P, Q, site, gen, 0)`). The **camp** (present) is a NEUTRAL
  garrison, id `u64::MAX − gen`, troops `× 1,000` (the Province stores whole
  troops), no walls, when fewer than 12 garrisons stand (12 holdings and a
  camp: the camp sits the bell out). Arrivals = records with `present = 1`.
  The kernel receives residents and arrivals sorted by id (a merge for its
  own sort; the write-back walks the outcome in step).
- **Camp check (I-56)** at the first resolve or skip of a day
  (`day(b) ≥ next_check_day`): `camp::place(camp_seed, P, terrain, day,
  has_holding, false)` with `camp_seed = sha256("PSF-CAMP-v1" ‖
  province[TERRAIN ..= SITE_COUNT])` (the ring seed is in neither account list;
  this block is fixed at OpenProvince from the ring seed, so it is exactly as
  public). A spawn replaces a present camp (the simulator's daily respawn),
  `gen += 1`, logs `CAMP`; `next_check_day = day + 1` either way.
- **Write-back.** Residents: post-clash troops, stamina, cooldown only when
  something changed (a quiet bell leaves the lazy stamina clock alone, so a
  skip and a resolve leave identical bytes: G11); `Withdrew` moves the tile;
  `Destroyed` frees the entry unless a departure (`Spend`), a `Leave` or a
  `Forfeit` is pending; `Bounced` sends the host home as a `Leave` issued at b
  (troops back through the return settle). Arrivals that stay or withdraw
  become entries (state 1, `from_bell = b + 1`, `ready_bell = b + 2` engaged
  else `b + 1`, doctrine dealt at Hold not arriving). Garrisons take their
  post-clash troops. The camp is cleared when it keeps less than one whole
  troop or hostile hosts hold its hex; `WORKS_CAMP` goes to the positions in
  `camp_mask` (the arrivals of the holding factions that stay on its tile),
  stored in ClashInputs' reserved word at offset 76 and logged by the `CAMP`
  record of the clear (`troops = 0`).
- **Settle of bell b** (resolve and skip alike): musters with
  `from_bell ≤ b + 1` join; `Spend` → state 3 with the march stamina paid
  (SettleDeparture next); `Leave` → state 3, op kept (return settle next);
  `Forfeit` → freed; splits/merges by the kernel (no M1 instruction issues
  them); garrison changes of bells ≤ b. `roster_epoch` moves and `n_entries`
  is recounted only when something changed.
- **Return settle (§21)** = SettleDeparture with `transit_slot = 0xFF`, the
  same accounts `[payer s] [season] [province w] [holding w]`: every state-3
  `Leave` entry of that Holding in that Province is freed; `reserve[unit] +=
  troops / 1,000` when the Holding is live with the host's generation
  (`DEPARTURE_SETTLED` with `destroyed = 2`, chained Holding + Province),
  else the troops are lost (`STRANDED`, Province). Nothing to return:
  `AlreadyDone`. Class D, keeper duty (W4-C: `fclient::ix::settle_departure(…,
  0xFF)`).
- **Gather records.** A slot whose Holding is absent, released, re-founded
  or has no matching transit gathers as not present; a transit still in
  state 1 is `DepartureUnsettled`; one destroyed at the origin (state 3) is
  recorded with `present = 0`, `fate = Destroyed`. The arrival's stamina is
  `stamina_after` refilled from `depart_bell + 1` to the arrival bell
  (`Stamina::at`). A present slot listed without its Holding, or an absent
  one with a Holding, is `BadData` (G6). Gather's `beneficiary` is accepted
  and unused (a gather pays nothing).
- **Bells ≥ `end_bell`** are never gathered, resolved or skipped
  (`WrongStatus`).
- **SkipQuiet.** Quietness is re-tested at the first bell and after any
  change (in-transaction cache; `Province.quiet_ok` stays 0: nothing records
  the epoch it would hold for). The test is `model::trivially_quiet` (every
  occupied hex one faction's residents within `HEX_HOST_CAP`, a garrison of
  the same faction or none, no one on the camp's hex, caps held: no
  engagement, bounce or withdrawal can occur; host property test against the
  kernel over 300 random rosters) and, **only at the transaction's first
  bell**, the kernel's `is_quiet` (heap scoped). A later bell that is not
  trivially quiet ends the transaction, which commits the bells before it.
  Settles run only from the first bell something is due (`model::next_due`).
- **CU-aware stop (I-50) is a work bound**: `sol_remaining_compute_units` is
  **not active on mainnet** (LiteSVM 0.16's mainnet feature list of
  2026-08-24; calling it fails "unsupported BPF instruction"), so SkipQuiet
  runs at most one kernel quiet test per transaction (`SKIP_KERNEL_TESTS`)
  plus cheap bells; it also stops before a bell once the heap has handed out
  20 KiB.
- **Digests.** CLASH `outcome_digest` = `ClashOutcome::digest` (encoded into
  a stack buffer; host test equal to the kernel's); `input_digest =
  sha256("PSF-CLASH-INPUT-v1" ‖ le32(b) ‖ seed ‖ province[SITE_MIRROR ..
  TICKET_COHORTS] before ‖ inputs[ARRIVALS .. POSTURES])`; SKIP
  `quiet_digest = sha256("PSF-QUIET-v1" ‖ le32(b0) ‖ n ‖
  province[SITE_MIRROR .. TICKET_COHORTS] after)`.
- **Closes.** CloseClashInputs: flag 2, a `settled_mask` bit for **every
  recorded host** (`host_id ≠ 0`, destroyed-at-origin records included:
  W4-B's SettleTransit must set the bit of every transit it settles from the
  records), `resolved_ts + clash_close_grace × 600 ≤ now − genesis_ts`;
  `InputsOpen` otherwise; `rent_to` must match (`BadAccount`).
  CloseArrivalDay: `resolved_next ≥ 144 (day + 1)` else `TooEarly`.
  CloseArrivalSlot (a): `settled` flag and (claimed, or `now ≥ close + 6
  bells` from THE anchor; THE anchor **absent** at its canonical address
  means archived, the grace long past); (b): Ended and `now ≥ end + 72 h`;
  `TooEarly` otherwise. Short-header closes log `CLOSE` with seq 0 and a zero
  head.

## 2. Measurements [measured, LiteSVM 0.16, cargo-build-sbf 3.1.9, platform-tools v1.52, SBPF v2]

Release `.so` 918,320 B, sha256 `8a11a3dc…4462` (`build-frontier.sh --twice`:
identical), programdata 1,151,021 B (`--max-len` 1,150,976). Trace build
929,616 B.

| Item | Result | Gate |
|---|---|---|
| **ResolveFromInputs, 1,244 fills** (SP-V2's 40 + the 1,200 screen + 4 storage fills), full gather (3 × 8) and the full M1 write-back | CU min 153,627, mean 262,204, **max 331,874** (gen-1200 wide#235); **heap max 15,080 B** (spv2-40 random#1, trace build); tx 453 B; every on-chain digest = the native kernel's | ≤ 340,000 CU, ≤ 28,672 B: **PASS** (margin 8,126 CU) |
| RFI profile of wide#235 (trace) | kernel 281.9k; builder 16.8k (terrain 2.0k, roster + sort 12.7k, garrisons 1.6k, arrivals 3.8k); write-back 12.9k; settle 3.3k; digest 5.3k; prologue/seed/evidence/input digest 8.3k; records 3.2k | — |
| GatherClash, 8 positions / 8 Holdings (every fill) | mean 32,184, max 32,381 CU | ≤ 40,000 |
| GatherClash, 12 positions / 10 Holdings, 2 absent (1 pre-funded), first gather (creates the inputs) | **38,725 CU**, 1,218 B, 32 locks | ≤ 40,000, ≤ 1,232 B |
| SkipQuiet, 24 idle bells, 48 residents, crossing a day (camp check) | **56,407 CU** (2 quiet tests), 1,148 B | ≤ 60k + 30k × 2 |
| SkipQuiet, 24 bells, 48 residents, a change at every bell | **205,948 CU** (24 re-tests, all trivial) | ≤ 60k + 30k × 24 |
| Kernel `is_quiet`, 48 residents (trace) | ≈ 58.6k CU + 16k builder ≈ 75k | the budget's "30k per recomputed bell" assumed 10–30k: see §4 D6 |
| CloseClashInputs / CloseArrivalDay / CloseArrivalSlot | 6,923 / 5,552 / 6,099 CU | ≤ 8,000 |
| Return settle (SettleDeparture 0xFF) | 8,653 CU, 319 B | ≤ 15,000 |
| `L(kind)` at programdata 1,150,976 | GatherClash 1,212,416 (need 1,173,797); RFI 1,179,648 (need 1,159,367); SkipQuiet 1,310,720 (need 1,162,919); CloseClashInputs 1,179,648; CloseArrivalDay 1,179,648; CloseArrivalSlot 1,179,648 — each lands at L, fails charged one page under the tight limit | I-45 |

**The `.so` grew from 563,200 B (integ-W3) to 918,320 B**: the clash kernel
is linked for the first time; of it ≈ 185 KB are monomorphized
`core::slice::sort` bodies (the Phase-A kernel's `sort_by_key` calls),
≈ 56 KB the kernel itself, ≈ 80 KB `proc::clash`. Consequence: programdata
(1.15 MB) now exceeds the 1-MiB working default of
`reveal_loaded_limit` (which only feeds `tip_min`) and every `L(kind)` is
≈ 1.18–1.31 MB; the presets are regenerated from the final `.so` at W5-A
anyway. A kernel-side reduction (one sort helper instead of a dozen
monomorphs) is W1-A's file: suggestion for W5-A.

## 3. What I ran

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` (root) | exit 0 |
| `cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings` (and `--features oracle`) | exit 0 |
| `cargo test --locked -p permutation-frontier --no-default-features` | 36 passed |
| `cargo test --locked -p permutation-frontier` | 74 passed (11 new in `proc::clash`) |
| `scripts/build-frontier.sh --twice` | identical hashes, e_flags 2, overflow panics present, deployable |
| `(cd permutation-frontier/svm-tests && ./run.sh --release --no-fail-fast)` (Gate W4 line 1; builds release, test-beacon, trace, oracle, probe) | **166 passed, 0 failed**, 4 ignored (the W2-B drill and my 3 `zz_profile_*` diagnostics) |
| `(cd permutation-frontier/svm-tests && cargo fmt -- --check)` | exit 0 |
| `(cd permutation-frontier/svm-tests && cargo clippy --locked --release --all-targets -- -D warnings)` | my files clean; **2 pre-existing errors in W3-B's `tests/host.rs:574` and `tests/holding.rs:466`** (`empty_line_after_doc_comments`; untouched, not in a gate) |

Not run: the frontier-node lines of Gate W4 (`inproc_`, `lag_gate`,
`crash_injection`, `verify` tampers): no file of mine is in `frontier-node`,
and no frontier-node test loads the program binary; they are W4-C/W4-D/W4-F's.
No Playwright, drand archive, Agave or rustup install was used; no port was
bound.

## 4. Deviations and requests

| # | What | Why | Who |
|---|---|---|---|
| D1 | **`proc/host.rs` edited (5 lines)**: SettleDeparture dispatches `transit_slot = 0xFF` to `clash::settle_return`. `host.rs` is W3-B's (frozen in wave 4) | §21 lets W4-A choose "a new tag, or a SettleDeparture variant"; the variant needs no tag, account list, budget row, vector or builder change (a new tag would touch `frontier-abi` tags/ix/prologue/budgets, the vectors, `fclient`, `lib.rs` and the JS SDK) | integrator: confirm; amendment v1.6 §5.11/§6 (`DEPARTURE_SETTLED.destroyed = 2` = returned) |
| D2 | ClashInputs offset 76 (`RSV_76`) holds `camp_mask` | "WORKS_CAMP credited to the winning faction's arrivals in the fate table": the 40-B records have no spare byte | integrator: rename in `frontier-abi::layout::clash` (v1.6 §5.3); W4-B/verifier credit `Citizen.works` from it or from the `CAMP` record |
| D3 | Camp seed from the Province's terrain block, not the ring seed; camp as a garrison only when < 12 garrisons; clear rule; spawn replaces | the ring seed is not in RFI's or SkipQuiet's accounts (§1 above) | v1.6 §5.11 |
| D4 | SkipQuiet's stop is a work bound (one kernel quiet test per transaction, trivial tests otherwise) | `sol_remaining_compute_units` is not active on mainnet | v1.6 §5.11 / I-50 text |
| D5 | `Province.quiet_ok` is never written | a one-byte cache cannot say which `roster_epoch` it holds for, and other instructions change the roster without clearing it | v1.6 §5.3 (reserve it) |
| D6 | The kernel's `is_quiet` costs ≈ 75k CU at 48 residents (not 10–30k) | it runs the whole `resolve_clash`; the trivial test answers every M1 quiet roster I built, so the budget formula holds in practice (§2) | informational; the formula can stay |
| D7 | Bells ≥ `end_bell`: `WrongStatus` for gather, resolve and skip | no arrival can exist there; DisbandStranded frees at once once `resolved_next ≥ end_bell` | v1.6 §5.11 |
| D8 | The herald's `Provisional` builder (W3-D D1) passes the camp's troops unscaled and does not run the camp check; `clash::model::build` is the program's builder | the Province stores whole troops (W3-A's pinned encoding) | integrator: move `proc::clash::model` (pure: `frontier-abi`, `permutation-rules`, the program's pure layer) into `frontier-abi` and point the herald (`runner::IngestCfg::new`), WASM `resolve_from_inputs` and the verifier at it |
| D9 | `heap_scoped` (resets the bump allocator's `next` word around the quiet test) lives in `clash.rs` | `heap.rs` is W2-A's | integrator: move as `heap::scoped` |
| D10 | G13 row for the return settle | SettleDeparture's registry row is in `cover/host.rs` (W3-B's) | integrator: add `clash::clash_dissolve_returns_troops_to_the_reserve` `[Lands("cix::settle_return("), Err(E::AlreadyDone)]` |
| D11 | Oracle ResolveClash transactions are ≈ 1.9 KB (24 slots + up to 24 Holdings) and ask for a 4-MiB loaded-data limit | test-only; LiteSVM does not enforce the packet size | none (not gated) |
| D12 | Garrison withdrawals stay refused (positive deltas only) | §21: "until the same step returns withdrawals"; the return settle returns hosts only | W5 if wanted |

Pending (G13 rows marked `Pending("W5-A: …")`): `NotTopLevel` of the three
top-level clash instructions (needs a CPI through the probe),
`RulesetMismatch`, RFI's `Kernel` (an inconsistent stored roster).

Not W4-A's (from the wave note): **K9** (SettleTicket's cross-cohort rule)
is in `proc/citizen.rs`, which no wave-4 unit owns: the integrator's.
**DECISIONS.md** entries (O-M1-12 item 1, J2, K9, the rustfmt/clippy
install) are the integrator's file.

**Dependency requests:** none (no manifest, lockfile or toolchain change).
