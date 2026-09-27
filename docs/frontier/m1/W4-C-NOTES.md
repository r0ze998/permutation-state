# W4-C keeper-play — notes

- **Unit:** W4-C (wave 4), branch `frontier/m1-W4-C` cut from `frontier/m1-integ` at `7289822`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.5: §5.1, §5.3, §5.4, §5.5, §5.11, §5.12, §6, §8.1, §8.2, §11 (W4-C brief), §12 Gate W4, §13.3 G7/G12, §13.4 criteria 1, 3, 4, 8; §21 (the troops return); I-07, I-11, I-12, I-21, I-24, I-44, I-46, I-47, I-49, I-50, I-52; offchain design §6.5–§6.7. Main-session wave note items 2 (J2) and 3 (K9) and 4 (§21 returns).
- **Owned paths touched:** `frontier-node/crates/{keeper,fclient}/**` and this file. No manifest, lockfile, toolchain file or `.gitignore` changed; **no dependency requested** (§6).
- **Tags:** [measured] = run on this machine on 2026-09-28; [design] = a rule implemented as the contract states it; [model] = against the native model of the program.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no `rustup`, Playwright, drand archive or Agave install; no crate or npm download. No service started: every test runs `localnet` in process and binds nothing (the API unit test binds `127.0.0.1:0`). `permutation-server/web/session.mjs` untouched. The rustfmt/clippy install for 1.95.0 is already recorded in `docs/frontier/DECISIONS.md` part A (the 2026-09-27 row), so nothing was added there (and `DECISIONS.md` is not this unit's file).

## 1. What landed

### Keeper duties (`keeper_core::play`, feed index `keeper_core::playindex`)

| Duty (role) | Trigger | Write | Class |
|---|---|---|---|
| **Decrypt** (`reveal`, `settle`) | round T(arrive) published | — the seal opened off chain with the stock `tlock` opener (`fclient::play::open_march` = `seal::judge` + the salt of `k`); valid plaintexts and salts journalled (`plaintexts`), so a restart does not re-open them | — |
| **Reveal** (`reveal`) | an opened seal, or owner material from `/v1/reveal` (from the bell start, before T(b)) | Reveals grouped by `(P, Q, arrive, faction)`: the group's **final set** (`quota_set` over the slots read and every candidate) is sent **at once in the kernel's rank order** (troops, slot key, host), each at the index `admit_arrival` gives when the ones ranked above it land first, with a 1-milli bid bonus per rank so a block lands them in that order (no displacement churn, ≤ 4 slot creations per group); arrivals outside the final set are **never revealed** and go to the settle queue (D5 bounce); `SlotMoved` → re-read and retry (≤ 4, then settle queue); `QuotaRefused` → settle queue; **never at or after `A + W − 2 slots`** (the engine's deadline slot plus a Clock check every tick that cancels a pending version and records `ValidSealUnrevealed`); nothing once the latch is closed (ClashInputs present or the destination resolved past `arrive`). Owner tracks move `queued → sent → landed / refused / expired` | **W** |
| **SettleDeparture** (`settle-departure`) | the origin resolved past `depart_bell` (read) and the transit in state 1 | SettleDeparture | D |
| **Return** (`settle-departure`, §21) | a `Leave` entry (state 3) whose bell resolved | **W4-A's shape:** SettleDeparture with `transit_slot = 0xFF` per (province, Holding) (`fclient::ix::{RETURN_SLOT, settle_return}`, read from W4-A's work in progress, `proc/clash.rs` doc; the integrator checks it at merge) | D |
| **Gather** (`gather`) | window closed (THE anchor or the archive entry), the ArrivalDay bit set (or a contested roster), every present arrival's transit in state ≥ 2 (else it waits: the lag rule) | GatherClash parts over the positions not in the mask: ≤ 10 Holdings, ≤ 21 slot keys + Holdings a part (1,232-B packet), `holdings_bitmap` bit j = position `start + j` present; a clear bit (contested roster, or a skip refused `NotQuiet`) → one no-arrival gather | D |
| **Resolve** (`resolve`) | mask full, `resolved_next == b`, the seed of THE anchor (the W3-C `SeedFinder`: cache, archive) | ResolveFromInputs | D |
| **Skip** (`skip`) | closed windows with clear bits from `resolved_next` (bells < `end_bell`) | SkipQuiet over ≤ 24 bells: **at once** when the province has pending changes, a `Leave` to return, a departure to settle, a known arrival ahead (or a settlement judged against it), a nudge, or the season's end within 24 bells; **otherwise once 24 bells are due** (≤ 6 transactions per idle province-day). CU limit `60k + 30k × bells with a pending change` (I-50, capped at 1.4M); a committed prefix (the program's CU-aware or work-bound stop) is re-planned from the new `resolved_next`. Provinces with residents of ≥ 2 factions skip the skip (they fight every bell): gather + resolve at once | D |
| **Settle** (`settle`) | `close + 600` passed and the destination resolved past `arrive` | SettleTransit for **every** transit — revealed, refused, outranked, unrevealed, bad — with the **logged** commitment and seal from its DEPART record (the proof runs inside, I-44). Destination: the valid plaintext's, else the revealed one (REVEAL records), else the origin (a bad seal nobody revealed; see F1). Slot index = the slot holding the host (W4-B's P4), else 0; resolver = the resolved inputs' (W4-B P5), else the keeper's beneficiary; anchor or archive by presence | D |
| **Closes** (`close`) | every 4 slots | CloseArrivalSlot (settled flag and `claimed` or past `close + 6 bells`), CloseArrivalDay (`resolved_next ≥ 144 (day + 1)`), CloseClashInputs (resolved, every present arrival settled, past `clash_close_grace`); `rent_to` read from the account | N |
| **Claims** (`claims`) | this keeper's slots (`beneficiary == claim key`) with `ev_slot − anchor.slot ≥ lateness_slots`, `fees::defence_refund > 0`, before `close + 6 bells` | ClaimDefence ≤ 6 slots, signed by the beneficiary key (`beneficiary_key_file`; `Keeper::set_claim_key`); a refused claim frees its slots for a new write (W4-B P16: `day == day(now_bell)` at landing) | D |

**Nudges** (`/v1/nudge`): a nudged province is caught up at once and stays urgent until it is resolved through the nudge's bell − 2; the answer's `blocking` lists what holds the province (its lag, play writes in flight on it, departures from it waiting for its resolve), as of the last tick.

**Multiple keepers:** `race_jitter_slots` (keeper.toml, default 0) delays every play write by a random 0..=n slots (offchain design §6.5); the operator's keeper keeps 0 (its latency is what E5 criterion 3 measures).

**Crash safety:** the journal's `plaintexts` table is read before opening a seal; `Journal::crash` (`CrashAt`) is the crash-injection hook (a panic at the n-th write of a point: the write does not happen, as a `kill -9` there); the play index is rebuilt from feed cursor 0 at start (as W3-C's land index; persisting the cursor stays W5 hardening).

### `fclient`

- `play` (new): `open_march`, `rank`, `reveal_order`, `target` (→ `Fill`/`Displace`/`Already`/`Refused`), `in_final_set`, `path_provinces` (the provinces a Reveal names: every province a step enters, first-entered order, destination left out, `None` beyond 3 or for a path that does not end on the plaintext's tile), `position`, `gather_parts`, `bounces_unranked`.
- `ix`: `RETURN_SLOT`, `settle_return` (§21).
- `abi::err`: play codes (13, 26, 30, 33, 36, 38, 40–43, 55); `is_done` also ends as done a SettleDeparture/SettleTransit refused `TransitState` (the record moved on) and a ResolveFromInputs/SkipQuiet refused `OutOfOrder` (the province moved past the bell).
- `land::earlier_cohort_open` (DECISIONS K9, §3).

### Engine and plumbing

`Engine::set_cu_limit` (a duty's own CU size: SkipQuiet), `Engine::set_bid_bonus` (reveal rank order), `Payers::extra` (the claim key as a fixed payer), `KeeperConfig::{beneficiary_key_file, claim_key, race_jitter_slots}`, `Shared::blocking`, play counters in `/v1/status` (`play`) and `/metrics` (`frontier_keeper_play_*_total`, close → resolve p99), the play refusals that re-plan (`NotQuiet`, `SlotMoved`, `QuotaRefused`, `TooEarly`, `DepartureUnsettled`) not alerted, and a play role answered `NotImplemented` turned off with one alert (as W3-C's land roles).

## 2. The native model (`crates/keeper/tests/model/play.rs`)

W4-A (clash) and W4-B (transit) are built in parallel, so the tests run against a **native model** registered as a LiteSVM builtin (as W2-F/W3-C did): Dissolve, Depart, Reveal (quota by the kernel's `admit_arrival`, window, BeaconLog guard, archive tombstone, latch, ArrivalDay, evidence parsed from the instructions sysvar), SettleDeparture (+ the §21 return with `transit_slot = 0xFF`), GatherClash, ResolveFromInputs, SkipQuiet, SettleTransit (the proof with `seal::judge`, every D5 branch, payments from the escrow, `pool_owed`), the three closes and ClaimDefence (`fees::defence_refund`). Its left-outs are listed at the top of the file; the important one: **ResolveFromInputs is a model clash** (the strongest faction's arrivals stay, the others are destroyed, losing residents halve; deterministic in the inputs, the Province and the seed), not the kernel's `resolve_clash`. The model also implements DECISIONS K9 in SettleTicket (`TicketState`). **It is not the program.** With `PSF_FRONTIER_SO=<test-beacon .so>` the tests deploy the program instead and report "not applicable" while it answers `NotImplemented` for the play instructions (the integration branch's W3 program); once W4-A and W4-B are merged the same tests run against it.

## 3. DECISIONS K9 (main-session wave note 3)

The program rule (a fresh settlement waits while an earlier cohort of the same Province is open) belongs to SettleTicket (`permutation-frontier/src/proc/…`), which **no wave-4 unit owns**: per the wave note, the integrator implements it in the program with its failing-first test and records it in the contract and DECISIONS. This unit made the **keeper** consistent with it, failing-first: `fclient::land::a_fresh_settlement_waits_for_an_earlier_open_cohort` was written against a stub that returned `false` and failed [measured], then `earlier_cohort_open` was implemented and it passed; the ticket duty now holds a fresh settlement while an earlier cohort of its Province is open (the oldest-cohort-first order of W3-C settles that cohort first), and the model refuses it. `keeper::land` (`land_season`, `overlapping_fallbacks_never_displace`) stays green with the rule on [measured].

## 4. Tests

| Test | What it checks [model, measured] |
|---|---|
| **`keeper::play_bell_pipeline`** | One province-bell, eleven marches and a dissolve (fixture Citizens and final Holdings; Muster, Depart, Dissolve as player transactions): the faction-0 group of five plus a citizen's second arrival → **4 slot creations, no displacement, revealed 900 → 800 → 700 → 600**; the outranked (500) and the second arrival (400) **never revealed, settled bounced without loss** (outcome 6); an **owner self-reveal** landed before T(b) + 30 s; a **garbage seal** revealed by its owner (valid commitment) and a **bad-plaintext** seal (never revealed) **both settled bad-seal with the stock opener's code** (code 5 for the plaintext); the other arrivals Stays / Destroyed by the model clash; every DEPART logged the committed pair; **no Reveal landed at or after `A + W`**; T(b) anchor → last keeper Reveal **0 slots** (≤ 4); SettleDeparture before the gather (10); one CLASH at (D, arrive); the §21 return credited 150 troops to the reserve; every slot of the bell closed after the claim grace; the ClashInputs closed after `clash_close_grace` (2 bells in this run); every province within its 24-bell batch and the destination at the head; owner tracks `landed`; no `dead` or `not-implemented` alert |
| **`keeper::lag_gate_in_process`** (Gate W4 "program-level lag gate in process", G7) | The same scenario twice; the second holds the origin Province and the origin region's anchor of the departure bell (priority 1,000 = 1,000 × P_delay, above every keeper cap) past the destination's close: the destination's CLASH digest and **every outcome digest are byte-identical** to the unheld run, THE destination anchor's `A` equal; close → first resolve 9 slots unheld, **189 held** (the destination waited for SettleDeparture, never read pre-settlement values) |
| **`keeper::crash_injection_every_journal_point`** (ignored; Gate W4 runs it) | The keeper killed at the n-th journal write of each point × duty kind (reveal: `attempt`, `status`, `plaintext` × 1–7/10; clash = gather/resolve/skip and settle = settle-transit/settle-departure/return: `attempt`, `status` × 1–10), restarted from its journal in the same slot: **62 crash points reached (reveal 22, clash 20, settle 20), outcome digests identical in all 62; extra duplicate versions 0 in 47 runs and 1 in 15** (the version sent but not journalled), never above the in-flight bound (adopted + 1) |
| **`keeper::duplicate_keepers_race`** | Three keepers (the extra two with a 2-slot race jitter) on one chain: **identical outcome digest**; per instruction tag duplicates ≤ (keepers − 1) × objects: Reveal, GatherClash, ResolveFromInputs and SettleTransit **0**; SettleDeparture 22 over 11, SkipQuiet 94 over 48, closes 14 over 7 (≈ one losing version per extra keeper); beacon logs 576 over 288 (every keeper had the beacon role) |
| **`keeper::reveals_stop_at_close_under_hold`** | The faction-0 slots of the destination bell held above the keeper's cap from T(arrive) for 150 slots: no Reveal lands at or after `A + W` (4 pending versions cancelled at `A + W − ε`, `ValidSealUnrevealed` recorded for each), the held arrivals settle routed or bounced |
| **`keeper::claim_after_late_reveal`** | The slots held at 1.9 (below P_def 2.0) for 10 slots, the keeper at `--peace-start 0.25`: the Reveals land ≥ `lateness_slots` after THE anchor at a price above the tip level; the keeper's ClaimDefence lands (162,844 lamports, the model's `defence_refund`) and the slots close after it |
| unit (keeper) | `play_writes_have_their_contract_class_and_a_budget` (§5.5 classes of the 10 play tags, a budget row each, key → role); `api` nudge `blocking` |
| unit (fclient) | `play::{reveal_order_fills_without_displacement, gather_parts_respect_the_packet, path_provinces_follow_locate}`; `land::a_fresh_settlement_waits_for_an_earlier_open_cohort` |

## 5. Gate W4 items that concern these files [measured, 2026-09-28, this machine]

| Item | Result |
|---|---|
| `(cd frontier-node && cargo fmt --all -- --check)` | pass |
| `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` (debug and release) | pass (toolchain 1.95.0) |
| `(cd frontier-node && cargo test --locked --release --workspace)` | pass — see the final counts in §9 |
| `(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)` | pass: `lag_gate_in_process`, `crash_injection_every_journal_point` (62 points) and W4-F's current `inproc_smoke_one_bell_with_the_test_key` stub |
| `(cd frontier-node && cargo test --locked --workspace)` (Gate W1 debug line) | pass — see §9 |
| "the in-process lag gate byte-identical" | **green against the native model** |
| "crash injection outcome digests identical with ≤ 1 duplicate version per in-flight write" | **green against the native model** (62/62 identical; ≤ 1 extra) |
| Against the program | **not run**: W4-A/W4-B are not on this branch; with `PSF_FRONTIER_SO` the play tests report "not applicable" on the W3 program. The integrator's step after merging W4-A and W4-B: `PSF_FRONTIER_SO=<test-beacon .so> cargo test --locked --release -p keeper --test play -- --include-ignored --nocapture` |
| `itest::inproc_day`, the svm-tests run, `verify tamper_` | W4-F's, W4-A/B's and W4-D's files; not run here |
| `build-wasm`, Playwright, drand archive | not this unit's; nothing installed |

## 6. Dependency requests (integrator, I-55)

None. Every crate used (`rand`, `base64`, `hex`, `serde_json`, `sha2`, `rusqlite`, `tokio`, LiteSVM through `localnet`, `solana-program-runtime` for the model) was already a dependency or dev-dependency of `keeper`/`fclient`.

## 7. Deviations and choices

| # | What | Why |
|---|---|---|
| D1 | A group's final set is sent in one slot with a rank bid bonus (1 milli per rank) instead of one Reveal at a time | §8.2 "sent in descending departure mass, targeting the index `admit_arrival` computes": one-at-a-time costs a slot per arrival (≥ 4 slots for a full group, E5 criterion 3 is ≤ 4); a block orders by priority, so the bonus lands them in rank order; any other order is a `SlotMoved` and the group is re-read |
| D2 | Arrivals outside their group's final set are not revealed at all | §8.2 sends them to the SettleTransit queue on `QuotaRefused`; knowing the candidates, the keeper skips the refused Reveal (its fee) and the result is the same D5 bounce (verified: outcome 6) |
| D3 | A bad seal nobody revealed is settled against its **origin** Province | Its destination cannot be read; W4-B's program uses the supplied Province (its P3, F1). The keeper always uses the REVEAL record's destination when there is one, so an honest keeper cannot be led to settle a revealed bad seal elsewhere |
| D4 | Contested provinces (residents of ≥ 2 factions) are gathered and resolved each bell instead of skipped | They fight every bell (no quiet proof); a skip would only be refused `NotQuiet` |
| D5 | Idle provinces are skipped in 24-bell batches | Criterion 3's "≤ 6 SkipQuiet per idle province-day"; players who want to act nudge (the web's `/f/nudge`), and every province the keeper needs (arrivals, departures, pending changes, returns, season end) is kept current |
| D6 | `is_done` extended for the play tags (`TransitState`, `OutOfOrder`) | §5.4's mapping names `AlreadyDone` only; these refusals can only mean "another version or keeper did it", and treating them as failures made racing keepers alert and back off |
| D7 | The play feed index is re-read from cursor 0 at start | As W3-C's land index (D2 there); W5 hardening can persist the cursor in the journal |
| D8 | Tests start from fixture Citizens and final Holdings (not tickets) and use the model clash | Land is W3's and tested there; the model clash is deterministic in the seed, which is what the lag gate and crash injection need; the real kernel runs once W4-A's program is merged (§5) |

## 8. Findings for other units and the integrator

1. **W4-A / integrator — return settle.** The keeper sends W4-A's shape (SettleDeparture, `transit_slot = 0xFF`, one per (province, Holding)); `DEPARTURE_SETTLED.destroyed = 2` marks a return (the keeper's feed index ignores it). If W4-A's committed shape differs, only `fclient::ix::settle_return` changes.
2. **W4-A — `holdings_bitmap`.** The keeper sets bit j ⇔ position `start + j` present, Holdings listed in that order (one per present position; the same Holding may repeat). W4-A's `BadData` rule (a present slot without a Holding or the reverse) matches.
3. **W4-B — bad-seal `settled_mask` bit.** A revealed bad seal is in the resolved records; unless its SettleTransit sets its `settled_mask` bit (the model does), CloseClashInputs can never close those inputs (≈ 7.15M lamports locked per case). W4-B's code sets the bit through `position`; please confirm it is set in the bad-seal branch too.
4. **W4-B F1 (bad seal's destination)** is real for dishonest settlers; the keeper mitigates it for honest ones (D3). A program-side stamp (W4-B's option (a)) would let the keeper drop the origin fallback.
5. **W4-B P16 (ClaimDefence `day == day(now_bell)`)**: handled (a refused claim frees its slots for a new write with the new day).
6. **Contract (§8.2 / §5.4 keeper mapping)**: record D6's done codes, D1's rank bonus and the §21 return in v1.6.
7. **Integrator — K9** is the program's (SettleTicket); the keeper and the model already follow it (§3).
8. **W4-F** — `keeper_core::play` exposes `stats` (reveal latency per bell, close → resolve slots), `findings` (`ValidSealUnrevealed`) and `index` for `inproc_day`'s criteria (zero stuck province-bells, every transit settled, bad seals with the stock code).

## 9. Final run record [measured, 2026-09-28, the committed tree, toolchain 1.95.0]

| Command (from `frontier-node/`) | Result | Wall |
|---|---|---|
| `cargo fmt --all -- --check` | pass | — |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | pass | — |
| `cargo clippy --locked --release --workspace --all-targets -- -D warnings` | pass | — |
| `cargo test --locked --release --workspace` | **200 passed, 0 failed, 4 ignored** (W3: 130 / 2) | 134 s |
| `cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection` | **3 passed** (`lag_gate_in_process`, `crash_injection_every_journal_point`, W4-F's `inproc_smoke_one_bell_with_the_test_key`) | 113 s |
| `cargo test --locked --workspace` (debug, Gate W1 line) | **200 passed, 0 failed, 4 ignored** | 178 s |
| `cargo test --locked --release -p keeper --test play -- --include-ignored --nocapture` | 6 passed; one scenario ≈ 1–4 s of wall, the 62-run crash injection 175 s | — |

The build started from an APFS clone of the integration worktree's `frontier-node/target` (git-ignored) to skip a cold LiteSVM build; it changes nothing in the tree.
