# M1 "First Bell": the on-chain program `permutation-frontier` (design)

- **Date:** 2026-09-27. **Area:** program. **Status:** design for review; no repo change, no commit.
- **Normative inputs:** `docs/frontier/DESIGN.md` rev 3.1 (§0–§13, §19, §20), `docs/frontier/m0/M0-FINAL.md`, `SPIKE-SP-V2.md`, `SPIKE-SP-FEE.md`, the SP-V2 lab program (`scratchpad/frontier/m0b/spikes/SP-V2/program/src/*`), S-SIZE-JOIN and E4 probe results, the kernels on `codex/frontier` at `d95fa25` (`permutation-rules/src/frontier/*`, resolve body identical to the SP-V2 copy at `cea89be`).
- **Owner decisions applied:** O1 (quicknet, SBPF v2), O2 (drand-only genesis), O7/N1 (defence pool, ≥ 150 rotating payers, cap priority 2.0), O8 (base only, no Skirmish/Arena/ER), N5 (M1 now), and the standing rules (no devnet step, no push without approval).
- **New evidence produced for this document** (lab copy `scratchpad/frontier/m1/lab/clash-opt/`, own `CARGO_TARGET_DIR`s, SP-V2 harness, SBPF v2, LiteSVM 0.16, mainnet feature set):
  - **Clash kernel rewrite, no rules change (Phase A):** worst ResolveFromInputs **540,891 → 274,798 CU** (−49%) over SP-V2's 40 adversarial fills (baseline re-run in the copy: 540,891, identical to SP-V2); worst single-tx ResolveClash of the 1,200-fill screen **632,643 → 322,736 CU**; heap peak **25,832 → 21,024 B**; engagement step **1,843 → 1,137 CU** each; digest equal to the original kernel on **4,320 inputs** (6 fill kinds × 120 seeds × 6 variants, all wedges, symmetric and raw asymmetric relations) [measured].
  - **Phase B (rules change: one sha256 per engagement):** worst ResolveFromInputs **227,652 CU**, screened worst hybrid 267,261 CU, engagement 815 CU [measured].
  - Capacity re-run of `lab/rev31` with the M1 budgets: the adversarial 10% line moves from ~60k to **~65k players (30/h bucket; ~85k at 20/h)** [model, `m1/lab/rev31-m1/m1-results.txt`].

Tags as in the design: [measured], [sim], [model], [estimate], [design]. "Change" marks a deviation from DESIGN rev 3.1 that needs review.

---

## 0. Summary of decisions

| # | Decision | Why | Change vs rev 3.1? |
|---|---|---|---|
| D1 | New crate **`permutation-frontier`** (root workspace member), own program id, SBPF v2, no MagicBlock, no token code in M1 | O8, J1 | no |
| D2 | **Every program-created account except the Season is a with-seed address of the Season PDA** (Citizen, Holding, Province included); presence is authenticated by owner + magic + key fields, absence by canonical address + "System-owned and empty" | flat ~0.5k CU per address check, no bump search (Join's 14.4–25.0k spread was the Citizen bump search [measured, SP-V2]) | **change** (§8.2 kept PDAs for player accounts) |
| D3 | **Quiet-bell proof = `ArrivalDay` bitmap + `SkipQuiet`**: the first Reveal into a province-bell sets one bit in a per-(province, day) account; a bell with the bit clear and its reveal window closed is provably arrival-free; `SkipQuiet` advances ≤ 24 bells per transaction | 144 idle bells cost ≈ 6 transactions × ≤ 60k CU instead of 144 × 50–73k [estimate vs model] | **change** (new account, Reveal may write 2 accounts on the first reveal of a province-bell) |
| D4 | **`SettleDeparture`** moves a departed host's post-clash values from the origin Province into the Holding's transit record; `GatherClash` reads Holdings, never origin Provinces | bounds Province entries (a departed entry otherwise lives up to 72+ bells); lag still only waits | **change** (3.1 kept values in the origin until SettleTransit) |
| D5 | **Transit outcome by rank against the final arrival set** stored in `ClashInputs` (admitted → fate; outranked or citizen-has-larger → bounce, no loss; otherwise → routed), so refused and displaced reveals need no write | a refused Reveal fails and leaves no trace; 3.1's "revealed or not" could not be decided on chain | **change** (fills a gap) |
| D6 | `ResolveFromInputs` writes **Province + ClashInputs** (fate table) | a Province result ring overflows under SettleTransit lag (A1) | **change** (rule 2 "exactly one account"; the second account is the same province-bell's and delay-only) |
| D7 | **Adopt clash Phase A now** (no rules change); **adopt Phase B before the M1 exit** if the doctrine gate and golden replays re-pass | Phase A is digest-identical; Phase B changes every variance draw | Phase B is a rules change (v10 not deployed; gates re-run) |
| D8 | Budget per worst-case clash **460k CU** (Phase A: ≤ 3 gathers × 40k + ResolveFromInputs ≤ 340k), 410k with Phase B | measured 274.8k + M1 write-back and margin | lowers 3.1's 650k |
| D9 | **No postures in M1** (M3 scope); every resident fights at `Hold`; `ClashInputs` reserves the posture area | M1 scope list; fewer gathers | no (M3 list) |
| D10 | **First holding only; no pair tickets; no MarchRoll, sieges, captures, diplomacy** in M1 | M1 scope; M3 builds them | no |
| D11 | `T(b)` = **first** quicknet round at or after the end of bell b; `S(b,r)` = first round at or after `A + W + Δ` | M0-FINAL §5 item 3 (round up everywhere) | pins a minor |
| D12 | Ticket challenge window closes at `round_time(S(b,r)) + 600 s`, not "end of bell b + 1" | the seed does not exist before ≈ end of b + 1 + 60 s | **change** (correction) |
| D13 | **Pay-or-divert** for every lamport payment to a third party (tip, march fee, bond, rent refund): if the recipient would stay below rent exemption, the amount goes to the DefencePool and is logged | a drained revealer wallet would otherwise make SettleTransit fail forever (A1) | new rule |
| D14 | Critical writes record **fee evidence** (slot, compute-unit price, limit, payer) read from the instructions sysvar; `ClaimDefence` refunds from it later | O7 defence pool, rule 2 (no pool on the critical path) | specifies 3.1 §6.4 |
| D15 | Anchors and caches record their **payer**; closing refunds the payer | 3.1 did not say who gets the rent | fills a gap |

---

## 1. Scope boundary of the program in M1

**In:** Season lifecycle (create, genesis seed, run, end, close, abort before joins), rings and provinces (OpenRing, ring seeds, OpenProvince with barbarian camps), FoldOccupancy, Join (free), session keys, site tickets with displacement and challenge window, Harvest/Build/Train, Muster/Dissolve/Garrison, Explore, dormancy release, Depart with mandatory seal and tip, Reveal (quota ranking, displacement, ArrivalDay), SettleDeparture, ProveBadSeal, GatherClash, ResolveFromInputs, SkipQuiet, SettleTransit, beacons (PostAnchor single and combined, PostSeed, PostBeacon), ArchiveAnchors and closes, DefencePool with ClaimDefence, event chains and logs. ResolveClash (single transaction) is built **only with the `oracle` feature** for tests.

**Out (later milestones, layouts reserve room):** USDC, vaults, fees, stakes, claims, FactionShards, reward indices, banking (M2); postures, sieges, occupation, capture, holdings 2–3, pair tickets, Wardens/Assembly/Ministers, Mandates, decrees and relations, caravans, Bourse, Engine, Relic Sites, delegation (M3); Shades and the roster (M2).

---

## 2. Crate layout, dependencies and build

### 2.1 Workspace

Root `Cargo.toml`:
```toml
[workspace]
members = ["permutation-chain", "permutation-rules", "permutation-frontier"]
exclude = [ ..., "permutation-frontier/svm-tests" ]

[profile.release.package.permutation-frontier]
overflow-checks = true          # program arithmetic aborts (WP08); the rules crate stays unchecked for CU
```
`permutation-frontier/Cargo.toml`:
```toml
[lib] crate-type = ["cdylib", "lib"]
[features]
default   = ["custom-heap", "program"]
program   = []                     # entrypoint + processor; without it: layouts, addresses, pure helpers (svm-tests, gateway, verifier)
no-entrypoint = []
custom-heap   = []                 # upward bump allocator with peak (from SP-V2 heap.rs / v9 heap.rs)
trace         = []                 # CU checkpoints + heap peak in logs (measurement builds only)
oracle        = []                 # ResolveClash single-tx, test builds only; build-frontier.sh refuses it
[dependencies]
solana-program = "=4.0.0"
ark-ff  = { version = "=0.5.0", default-features = false }
ark-ec  = { version = "=0.5.0", default-features = false }
ark-bls12-381 = { version = "=0.5.0", default-features = false, features = ["curve"] }
permutation-rules = { path = "../permutation-rules" }
```
No borsh in hot paths (fixed-offset zero-copy accessors); borsh stays in the rules crate only.

### 2.2 Source tree and where each piece comes from

| Module | Contents | Source |
|---|---|---|
| `lib.rs` | entrypoint, `dispatch(tag)`, `#[repr(u32)] FrontierError` | SP-V2 `lib.rs` pattern; v9 error discipline |
| `error.rs` | error codes (§7) | v9 `error.rs` discipline (codes stable, one test per code) |
| `ix.rs` | tags, fixed-layout data decoders, `AccountIter` with `signer()/writable()/readonly()` | new |
| `addr.rs` | `SeedStr`, `with_seed`, every seed grammar (§3.1), `is_absent` | SP-V2 `acct.rs` (extended grammar) |
| `init.rs` | `init_with_seed` (top-up + AllocateWithSeed, signer = Season PDA), `init_funded_from(program-owned)`, `close_to(payer)`, `pay_or_divert` | SP-V2 `acct.rs` mode 0; legacy mode 1 **not** ported (regression test keeps it in svm-tests as a stand-alone probe program) |
| `layout/*.rs` | one file per account: const offsets, `magic`, typed getters/setters over `&[u8]` / `&mut [u8]` | new (SP-V2 offsets for anchor, cache, archive, slot, inputs, verdict) |
| `clock.rs` | bell model, `T(b)`, `S(A)`, window schedule (§4) | SP-V2 `Season::{seed_round, reveal_close}`; kernel `clash::{BeaconClock, seed_round}` |
| `events.rs` | per-entity hash chains, `PSF` log records (§8) | v9 `sol_log_data` records |
| `crypto/{quick,field,xmd,sys}.rs` | hinted quicknet verification, BLS syscalls | SP-V2 unchanged (`#[inline(never)]` field ops) |
| `crypto/seal.rs` | on-chain tlock opener with FO check, commitment domains | SP-V2 `seal.rs` (+ posture domain for M3) |
| `evidence.rs` | read ComputeBudget instructions from the instructions sysvar; top-level check | new |
| `heap.rs` | bump allocator, `peak()` | SP-V2 / v9 |
| `proc/season.rs` | CreateSeason, InitShards, ConsumeGenesisSeed, EndSeason, CloseSeason, AbortSeason, SetWindowSchedule | new (v9 lifecycle ideas) |
| `proc/beacon.rs` | PostAnchor, PostAnchorMulti, PostSeed, PostBeacon, ArchiveAnchors, CloseSeedCache | SP-V2 `beacon.rs` + archive table + payer |
| `proc/map.rs` | OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy | new; kernel `terrain::generate_province` (E4 probe) |
| `proc/citizen.rs` | Join, SetSession, SetVigil, FileTicket, SettleTicket, ReleaseDormant | S-SIZE-JOIN probe, SP-V2 `join.rs` without token CPI |
| `proc/holding.rs` | Harvest, Build, Train, Explore, SettleExplore | E4 probe + kernel `holding` |
| `proc/host.rs` | Muster, Dissolve, Garrison, DisbandStranded, Depart, SettleDeparture | kernel `host` |
| `proc/reveal.rs` | Reveal | SP-V2 `reveal_slot` + E4 commit/path + kernel `admit_arrival` |
| `proc/clash.rs` | GatherClash, ResolveFromInputs, SkipQuiet, CloseClashInputs, CloseArrivalDay, (oracle) ResolveClash | SP-V2 `clash.rs` |
| `proc/transit.rs` | ProveBadSeal, SettleTransit | SP-V2 `seal.rs`, new |
| `proc/defence.rs` | ClaimDefence | new |

**Ported from v9 (as reference code, reviewed fresh):** error discipline, log-record format and the `records.rs` reader, heap allocator, the svm-tests harness structure (§12), `scripts/build-program.sh` (as `build-frontier.sh`). **Not ported in M1:** `token.rs` (M2), vault/claim/outstanding (M2), roster (M2), VRF (never, O2), anything MagicBlock.

### 2.3 SBPF v2 build: `scripts/build-frontier.sh`

1. Require `solana-cargo-build-sbf 3.1.9` (as `build-program.sh`); `export RUSTUP_TOOLCHAIN=1.95.0` only for `cargo metadata` if an edition-2024 crate enters the graph [measured, S-FEE notes].
2. `cargo-build-sbf --manifest-path permutation-frontier/Cargo.toml --tools-version v1.52 --arch v2 --sbf-out-dir permutation-frontier/target/deploy -- --locked`.
3. Refuse the artefact unless: ELF `e_flags == 2` (SBPF v2); the overflow-panic strings are present (overflow checks on for the program crate); the strings of the `oracle` and `trace` features are **absent** (marker symbols `PSF_ORACLE_BUILD`, `PSF_TRACE_BUILD`).
4. Print `file_sha256`, `program_hash` (trailing zeros stripped) and `e_flags`. Two builds must give the same hash (SP-V2 showed deterministic builds [measured]).
5. The program embeds `RULESET_HASH` (sha256 over the rules crate's `frontier` sources' version constants, as v9's binding) and `QUICKNET_PK_HASH`; `CreateSeason` stores both, every instruction compares `Season.ruleset_hash` with the binary's (J2).

---

## 3. Conventions every instruction follows

### 3.1 Addresses (with-seed grammar, pinned)

`addr(seed) = sha256(season_pda ‖ seed ‖ program_id)` (System `create_with_seed`). A seed is a 2-letter tag plus lowercase hex of the little-endian key fields; every seed is ≤ 32 bytes. Province coordinates are stored and hashed as **i16** (`|P|,|Q| ≤ 256` for rings ≤ 128; `OpenProvince` refuses others).

| Account | Tag | Key fields (LE) | Seed length |
|---|---|---|---|
| Season | PDA `["season", id u64]`, bump stored | — | — |
| Frontier | `fr` | — | 2 |
| RingSeed | `rs` | d u16 | 6 |
| ProvinceFund | `pf` | — | 2 |
| JoinShard | `js` | faction u8, shard u8 | 6 |
| BeaconLog | `bl` | region u8 | 4 |
| DefencePool | `dp` | — | 2 |
| Citizen | `ct` | `sha256("PSF-CIT" ‖ wallet)[0..15]` | 32 |
| Holding | `ho` | P i16, Q i16, site u8 | 12 |
| Province | `pv` | P i16, Q i16 | 10 |
| ArrivalSlot | `ar` | P, Q, bell u32, faction u8, i u8 | 22 |
| **ArrivalDay** (new) | `ad` | P, Q, day u16 | 14 |
| ClashInputs | `ci` | P, Q, bell u32 | 18 |
| SealVerdict | `sv` | host u64, bell u32 | 26 |
| BellAnchor | `an` | bell u32, region u8 | 12 |
| SeedCache | `sd` | bell u32, region u8, nonce u8 | 14 |
| AnchorArchive | `aa` | region u8, day u16 | 8 |
| DefenceClaim | `dc` | `sha256("PSF-KPR" ‖ keeper)[0..8]`, day u16 | 22 |

Rules (DESIGN §8.6 rules 10–11, restated for the implementation):
- **Presence** of a keyed account is authenticated by: owner = program, magic, `season_id`, and the stored key fields equal to the expected key. Only the program writes program-owned data and it initialises only at the canonical address, so a present account with the right key fields *is* the canonical one. Readers that need **the** instance (anchors, slots, inputs, verdicts, marks) also recompute the address (one sha256, ≈ 0.5k CU [measured, S-SIZE]).
- **Absence** is authenticated by: address = canonical address **and** owner = System **and** data length 0. Lamports are ignored (pre-funded = absent).
- Nobody passes a bump. The only PDA is the Season; its bump is found once in `CreateSeason` and stored.

### 3.2 Initialisation and closing

- `init_with_seed(payer, target, seed, space)`: require `is_absent(target)`; transfer `max(0, rent(space) − target.lamports)` from `payer` (System Transfer CPI; payer is a signer); `AllocateWithSeed{base: season, seed, space, owner: program}` signed by the Season PDA. Never `CreateAccount*`.
- `init_funded(fund, target, ...)`: as above but the shortfall is moved by direct lamport arithmetic from a **program-owned** fund (ProvinceFund for Provinces, the RingSeed payer for none) — used by `OpenProvince`.
- `close_to(target, recipient)`: `resize(0)`, `assign(System)`, move all lamports with `pay_or_divert`. Closed addresses are absent again; every account type whose re-creation could change an outcome has a **tombstone** rule (anchors: AnchorArchive bits; slots and marks: the reveal window is closed for good; inputs: the Province has resolved past the bell).
- `pay_or_divert(from, to, amount)`: if `to.lamports + amount < rent_exempt(to.data_len)` and `to` is not program-owned, credit the DefencePool instead and log `DIVERT`. Applied to tips, march fees, bonds and rent refunds.

### 3.3 Common header and event chains

Chained entities (Season, Frontier, JoinShard, Citizen, Holding, Province, ClashInputs) start with a 64-B header **H**:

| Off | Size | Field |
|---|---|---|
| 0 | 8 | magic (`PSF1SEAS`, `PSF1FRNT`, `PSF1JSHD`, `PSF1CITZ`, `PSF1HOLD`, `PSF1PROV`, `PSF1CLIN`) |
| 8 | 8 | season_id u64 |
| 16 | 2 | layout_version u16 (= 1) |
| 18 | 6 | reserved |
| 24 | 8 | event_seq u64 |
| 32 | 32 | event_head = `sha256(prev_head ‖ record)` over every PSF record that touched this entity |

Short accounts (slots, marks, anchors, caches, archives, verdicts, logs, pools, claims) start with a 16-B header: magic (8) + season_id (8); they are immutable or append-only and are verified by content against the logs.

### 3.4 Player prologue (every player instruction)

Accounts `[0] actor (s)`, `[1] payer (s,w)` (may equal actor; the relay fronts fees and rent, D5 of the audit list), `[2] season (r)`, `[3] citizen (w)` then instruction-specific accounts. Checks in order, all within the first ≈ 1.5k CU (rule 6):
1. `season` owner/magic/id, `status == Running`, `ruleset_hash == RULESET_HASH`;
2. `citizen` owner/magic/season; `actor == citizen.wallet`, or `actor == citizen.session && now < session_expiry`;
3. **action bucket** (30 per hour, burst 60, season parameters): tokens refill at `rate × Δt`, debit 1,000 milli-tokens, else `Bucket`;
4. `now < end_ts` except for SettleTransit/SettleExplore/Dissolve after the end (M1 has no banking).

### 3.5 Top-level rule and fee evidence

Critical and heavy instructions (PostAnchor*, PostSeed, PostBeacon, Reveal, GatherClash, ResolveFromInputs, SkipQuiet, ProveBadSeal, ConsumeGenesisSeed, ConsumeRingSeed, OpenProvince) require `get_stack_height() == 1` (`NotTopLevel`). Those that are defence-pool eligible (Reveal, PostAnchor*, PostSeed, GatherClash, ResolveFromInputs) read the instructions sysvar (`Sysvar1nstructions…`, read-only, +32 B) and record **evidence** in the account they create or update: landing slot u64, `SetComputeUnitPrice` micro-lamports u64 (0 if absent), `SetComputeUnitLimit` u32, `SetLoadedAccountsDataSizeLimit` u32, and the fee payer key. Parsing costs ≈ 1–2k CU [estimate].

---

## 4. Clock and bell model

All times are the Clock sysvar's `unix_timestamp`. Integers are u32 bells and i64 seconds.

| Quantity | Definition | Code |
|---|---|---|
| bell of time t | `b(t) = ⌊(t − genesis_ts) / 600⌋` for `t ≥ genesis_ts` | `clock::bell_at` = kernel `travel::bell_at` |
| bell start / end | `start(b) = genesis_ts + 600 b`, `end(b) = start(b) + 600` | `travel::bell_start` |
| tlock round of bell b | **`T(b) = first round with round_time ≥ end(b)`** (D11) | new kernel fn `clash::anchor_round`; clients seal to it |
| anchor time | `A(b, r)` = Clock at creation of THE BellAnchor `(b, r)` | stored in anchor and archive |
| window | `W(b)` = `window_next` if `b ≥ window_from_bell` else `reveal_window` (600–1,800 s; a change needs ≥ 144 bells of notice and never touches a started bell) | `clock::window` |
| reveal close | `close(b, r) = A(b, r) + W(b)` | kernel `reveal_close` generalised to W |
| seed round | **`S(b, r) = first round with round_time ≥ close + Δ`**, `Δ = seed_margin ≥ 60 s` | kernel `seed_round` |
| day | `day(b) = b / 144` (region-day for archives, province-day for ArrivalDay) | — |
| resolved | `Province.resolved_next` = first bell not resolved or skipped | — |
| resident actions | allowed at bell b iff `resolved_next + 1 ≥ b` (resolved through `b − 2`); effects pending until after the clash of b (kernel `Host::check_issue`) | kernel `host` |
| arrival bell | `arrive ∈ [earliest_arrival_bell(depart_ts, secs), depart_bell + 72]`, `earliest ≥ depart_bell + 2` | kernel `travel::check_arrival_bell` |
| reveal open | anchor present: `now < close ∧ BeaconLog(r).latest_round < S(A)`; anchor absent: not tombstoned in AnchorArchive `(r, day)` | kernel `reveal_open` + tombstone |
| gather/resolve allowed | `now ≥ close` (anchor or archive entry); resolve additionally needs a SeedCache for `S` or the archive's seed | — |
| settle transit | `now ≥ close(arrive, r_dest) + 600` and destination resolved past `arrive` (or ClashInputs closed, §6.5) | — |
| ticket final | `now ≥ round_time(S(b_ticket, r_site)) + 600` (D12) | — |
| archive | `now ≥ A + archive_after (172,800 s)`; the tombstone bit is set before the anchor closes | SP-V2 `t9` |

Economic timers (production, dormancy, shields, vigil) use wall time; combat timers (stamina refill, cooldown) count resolved bells (kernel `Stamina`, `ready_bell`).

---

## 5. Accounts

### 5.1 Summary

Rent = `(128 + size) × 5,080` lamports.

| Account | Address | Size | Rent (lamports) | Created by / paid by | Written by | Closed by / refund to |
|---|---|---|---|---|---|---|
| Season | PDA | 2,048 | 11,054,080 | CreateSeason / operator | lifecycle, ConsumeGenesisSeed, SetWindowSchedule | CloseSeason / operator |
| Frontier | `fr` | 512 | 3,251,200 | CreateSeason / operator | OpenRing, FoldOccupancy | CloseSeason |
| RingSeed | `rs‖d` | 128 | 1,300,480 | OpenRing (d ≥ g+1) or CreateSeason (d ≤ g) / caller | ConsumeRingSeed, OpenProvince (count) | CloseSeason / payer |
| ProvinceFund | `pf` | 128 | 1,300,480 + funds | CreateSeason / operator; top-ups by anyone | OpenProvince (debit) | CloseSeason / operator |
| JoinShard ×48 | `js‖f,s` | 256 | 1,950,720 each | InitShards / operator | Join, SettleTicket, ReleaseDormant | CloseSeason |
| BeaconLog ×16 | `bl‖r` | 128 | 1,300,480 | CreateSeason / operator | PostBeacon | CloseSeason |
| DefencePool | `dp` | 256 | 1,950,720 + escrow | CreateSeason / operator (20 SOL default, D18) | pay_or_divert credits, ClaimDefence debits | CloseSeason / operator |
| Citizen | `ct‖tag` | 320 | 2,275,840 | Join / payer (relay) | own player instructions, SettleTicket (displacement), ReleaseDormant, SettleExplore | CloseSeason crank `CloseCitizen` (M2) / rent payer |
| Holding | `ho‖P,Q,site` | 1,280 | 7,152,640 | SettleTicket / citizen's payer | owner actions, SettleTicket, SettleDeparture, SettleTransit, SettleExplore, ReleaseDormant | ReleaseDormant, CloseSeason / rent payer |
| Province | `pv‖P,Q` | 4,096 | 21,457,920 | OpenProvince / ProvinceFund | SettleTicket, Build (walls), Muster/Dissolve/Garrison, Depart, Explore, SettleDeparture, SettleTransit (returns), ResolveFromInputs, SkipQuiet, ReleaseDormant | CloseSeason / ProvinceFund |
| ArrivalSlot | `ar‖…` | 160 | 1,463,040 | Reveal / revealer | Reveal (displacement overwrites) | SettleTransit, `CloseArrivalSlot` after ClashInputs closed / rent payer |
| **ArrivalDay** | `ad‖P,Q,day` | 96 | 1,137,920 | first Reveal of the day / revealer | Reveal (bit set) | CloseArrivalDay once the Province resolved past the day / payer |
| ClashInputs | `ci‖P,Q,b` | 1,280 | 7,152,640 | first GatherClash / gatherer | GatherClash, ResolveFromInputs (fates), SettleTransit (settled mask) | CloseClashInputs (§6.5) / first gatherer |
| SealVerdict | `sv‖host,b` | 96 | 1,137,920 | ProveBadSeal / prover | — | SettleTransit / prover |
| BellAnchor | `an‖b,r` | 144 | 1,381,760 | PostAnchor / keeper | — | ArchiveAnchors / keeper (payer field) |
| SeedCache | `sd‖b,r,n` | 144 | 1,381,760 | PostSeed / keeper | — | CloseSeedCache after archive / keeper |
| AnchorArchive | `aa‖r,day` | 5,280 | 27,473,280 | first ArchiveAnchors of the region-day / caller | ArchiveAnchors | CloseSeason / payer |
| DefenceClaim | `dc‖keeper,day` | 128 | 1,300,480 | first ClaimDefence of the keeper-day / keeper | ClaimDefence | CloseSeason / keeper |

**Per-player rent:** Citizen 2.28M + Holding 7.15M = **9.43M lamports ≈ 0.0094 SOL** (design 0.0079 at an 832-B Holding; +19% from the four 96-B transit records and full `Accrual`s). Refundable.

### 5.2 Byte layouts

All integers little-endian. "rsv" = reserved zero. Offsets are exact; each layout file has a const assertion that its last field ends at or before the size.

**Season (2,048 B)** — H(0..64) then:

| Off | Field | Off | Field |
|---|---|---|---|
| 64 | status u8 (0 Created, 1 Seeded, 2 Running, 3 Ended, 4 Closed, 5 Aborted) | 65 | bump u8 |
| 66 | regions u8 (16) | 67 | genesis_ring g u8 |
| 68 | r_max u16 (≤ 128) | 70 | rsv u16 |
| 72 | authority [32] | 104 | ruleset_hash [32] |
| 136 | rules_version u16 (10) | 138 | program_version u16 |
| 140 | bell_secs u32 (600, fixed in M1) | 144 | genesis_ts i64 (start of bell 0) |
| 152 | created_ts i64 | 160 | join_close_bell u32 (3,024) |
| 164 | end_bell u32 (4,032) | 168 | drand_genesis i64 (1,692,803,367) |
| 176 | drand_period u32 (3) | 180 | network u8 (2 = quicknet), rsv[3] |
| 184 | quicknet_pk_hash [32] | 216 | reveal_window u32 |
| 220 | seed_margin u32 (≥ 60) | 224 | window_next u32 |
| 228 | window_from_bell u32 (u32::MAX = none) | 232 | genesis_round u64 |
| 240 | genesis_seed [32] | 272 | archive_after u32 (172,800) |
| 276 | min_lead u8 (2), max_lead u8 (72), transit_slots u8 (4), postures_enabled u8 (0) | 280 | march_fee u64 (10,000) |
| 288 | seal_bond u64 (20,000) | 296 | tip_min u64 (10,006 = 2,500 + 0.433 × (16,000 + 1,336)) |
| 304 | bucket_rate_per_h u16 (30), bucket_burst u16 (60) | 308 | defence_cap_milli u32 (2,000 = priority 2.0) |
| 312 | lateness_slots u8 (4), rsv[3] | 316 | theta_early_bps u16 (5,500), theta_late_bps u16 (6,500) |
| 320 | theta_switch_secs u32 (259,200) | 324 | reserve_bps u16 (200), extra_free_bps u16 (2,000) |
| 328 | clash_close_grace u32 (1,008 bells) | 332 | camp_regrow_bells u32 (144) |
| 336..1,024 | reserved for M2/M3 (mint, vaults, roster_root, policy_code_hash, γ, prices) | 1,024..2,048 | reserved |

**Frontier (512 B)** — H then: 64 rings_open u16 · 66 rsv · 68 last_ring_open_bell u32 · 72 last_ring_open_ts i64 · 80 fold_bell u32 · 84 fold_part u8, rsv[3] · 88 open_sites u32 · 92 occupied_sites u32 · 96 provinces_opened u32 · 100 wedge_open [6]u32 · 124 wedge_occupied [6]u32 · 148 acc_occupied u32 · 152 acc_wedge [6]u32 · 176..512 rsv.

**RingSeed (128 B)** — 16-B header · 16 d u16 · 18 status u8 (1 requested, 2 seeded) · 19 rsv · 20 opened_bell u32 · 24 t_open i64 · 32 round u64 · 40 seed [32] · 72 provinces_created u16 · 74 rsv[6] · 80 payer [32] · 112..128 rsv. Genesis rings (d ≤ g): `seed = sha256("PSF-RING" ‖ genesis_seed ‖ d)`, status 2 at creation.

**JoinShard (256 B)** — H · 64 faction u8 · 65 shard u8 · 66 rsv[2] · 68 members u32 · 72 first_holdings u32 · 76 holdings u32 · 80 holdings_by_wedge [6]u32 · 104 released u32 · 108..256 rsv (M2: fee and stake sums).

**BeaconLog (128 B)** — 16-B header · 16 region u8 · 17 rsv[7] · 24 latest_round u64 · 32 posted_ts i64 · 40 posted_slot u64 · 48 sig48 [48] · 96 payer [32].

**DefencePool (256 B)** — 16-B header · 16 paid_total u64 · 24 diverted_total u64 · 32 per_bell_region_cap u64 (lamports) · 40 per_keeper_day_cap u64 · 48 claims u64 · 56..256 rsv. Its lamports above rent are the escrow.

**Citizen (320 B)** — H then:

| Off | Field |
|---|---|
| 64 | wallet [32] |
| 96 | session [32] (zero = none) |
| 128 | session_expiry i64 |
| 136 | faction u8 · 137 flags u8 (1 joined, 4 has_first_holding(final), 8 provisional_holding, 16 refugee) · 138 holdings_n u8 · 139 explores_floor_left u8 (3) |
| 140 | join_bell u32 |
| 144 | join_shard u8 · 145 rsv · 146 vigil_start_min u16 · 148 vigil_next_min u16 · 150 rsv u16 |
| 152 | vigil_from_ts i64 |
| 160 | bucket_milli u32 · 164 bucket_t u32 (seconds since genesis_ts) |
| 168 | holding [3] × {P i16, Q i16, site u8, gen u8} (18 B) |
| 186 | rsv[2] |
| 188 | ticket_bell u32 (u32::MAX none) · 192 ticket_sites [3] × {P i16, Q i16, site u8} (15 B) · 207 ticket_next u8 (next preference to try) |
| 208 | citizen_tag u64 (= first 8 B of the Citizen address; the quota's "citizen" id) |
| 216 | last_action_ts i64 |
| 224 | works u64 (counted only in M1) |
| 232 | explores u32 · 236 arrivals u32 |
| 240 | rent_payer [32] |
| 272..320 | rsv (M2: fee, stake, stake bell, counted laurels, path facts) |

**Holding (1,280 B)** — H then:

| Off | Size | Field |
|---|---|---|
| 64 | 2+2+1+1 | P i16, Q i16, site u8, gen u8 (bumped on each re-founding; part of host ids) |
| 70 | 1 | tile u8 |
| 71 | 1 | state u8 (0 none, 1 provisional, 2 final, 3 released) |
| 72 | 32 | owner_citizen (Citizen address) |
| 104 | 8 | ticket_score u64 (rank among tickets of `ticket_bell`) |
| 112 | 1+1+1+1 | faction, order (1), tier, flags |
| 116 | 4 | ticket_bell u32 |
| 120 | 8 | founded_ts i64 |
| 128 | 4 | founded_day u32 |
| 132 | 4 | host_seq u32 |
| 136 | 8 | last_owner_action i64 |
| 144 | 8 | shield_until i64 |
| 152 | 320 | stores [8] × Accrual {value i64, rate i64, cap i64, t0 i64, frac i64} (kernel `holding::Accrual`) |
| 472 | 64 | production [8] i64 |
| 536 | 64 | upkeep [8] i64 |
| 600 | 96 | queue [4] × {done_at i64, kind u8, arg u8 (resource or unit), rsv[6], delta i64} |
| 696 | 4+4 | walls u32, rsv |
| 704 | 8 | walls_committed_before i64 |
| 712 | 8 | food_shortfall i64 |
| 720 | 32 | reserve [8] u32 (trained troops by unit, milli) |
| 752 | 32 | delegate [32] (M3; zero) |
| 784 | 384 | transit [4] × 96 B (below) |
| 1168 | 24 | explore {bell u32, P i16, Q i16, tiles [2] u8, host u64, state u8, rsv[5]} |
| 1192 | 8 | escrow u64 (tips + fees + bonds held) |
| 1200 | 32 | rent_payer |
| 1232..1280 | 48 | rsv |

Transit record (96 B): 0 state u8 (0 free, 1 departed, 2 values_settled, 3 closed) · 1 unit u8 · 2 faction u8 · 3 origin_tile u8 · 4 origin P i16 · 6 origin Q i16 · 8 host_id u64 · 16 depart_bell u32 · 20 arrive_bell u32 · 24 depart_ts i64 · 32 dep_mass u32 · 36 march_stamina u16 · 38 dealt_bps u16 · 40 troops_after u32 · 44 stamina_after u16 · 46 ready_bell_off u16 · 48 seal_root [32] · 80 tip u64 · 88 flags u8 (1 fee, 2 bond, 4 values from origin clash applied) · 89..96 rsv.

**Host id (pinned):** `host_id = (province_index(P,Q) as u64) << 44 | site << 40 | gen << 32 | seq` (province index < 2^20 for R ≤ 128 [kernel `ProvinceCoord::index`], site < 16, gen u8, seq u32). The owner Holding's address is derivable from any host id; a stale `gen` marks a stranded host.

**Province (4,096 B)** — H then:

| Off | Size | Field |
|---|---|---|
| 64 | 2+2+2+1+1 | P i16, Q i16, ring u16, wedge u8, region u8 |
| 72 | 4 | resolved_next u32 |
| 76 | 4 | opened_bell u32 |
| 80 | 8 | relations u64 (0 in M1: all factions hostile; M3 writes it only through `Relations::set_peaceful`, so it stays symmetric) |
| 88 | 32 | last outcome digest |
| 120 | 1+1+1+1 | n_entries, n_sites_used, quiet_ok (cached `is_quiet` for `roster_epoch`), rsv |
| 124 | 4 | roster_epoch u32 (bumped by every roster, garrison or wall change) |
| 128 | 61 | terrain class per tile (kernel `Terrain` as u8) |
| 189 | 61 | resource per tile (0 none, 1 + `TileResource`) |
| 250 | 12+1+1 | sites [12] tile u8, site_count u8, rsv |
| 264 | 8×4 | passable_mask u64, rough_mask u64, road_mask u64 (M3), explored_mask u64 |
| 296 | 768 | site mirror [12] × 64 B (below) |
| 1064 | 2,688 | entries [56] × 48 B (below): ≤ 48 in the roster (≤ 8 per faction), the rest musters pending or departed awaiting SettleDeparture |
| 3752 | 32 | last resolve summary {bell u32, engagements u32, arrivals u8, destroyed u8, bounced u8, rsv, resolver [8]…} |
| 3784..4096 | 312 | rsv |

Site mirror (64 B): 0 state u8 (0 free, 1 holding, 2 camp, 3 released-free) · 1 faction u8 (owner or NEUTRAL = 6) · 2 order u8 · 3 tier u8 · 4 gen u8 · 5 rsv[3] · 8 garrison u32 · 12 pend0_bell u32 · 16 pend0_delta i64 · 24 pend1_bell u32 · 28 rsv · 32 pend1_delta i64 (kernel `GarrisonState`) · 40 walls_committed u32 · 44 wall_item0 {effective_bell u32, delta u32} · 52 wall_item1 · 60 shield_until_bell u32 (camps: regrow_bell).

Entry (48 B): 0 id u64 · 8 faction u8 · 9 unit u8 · 10 tile u8 · 11 state u8 (0 free, 1 roster, 2 muster_pending, 3 departed) · 12 troops u32 · 16 stamina_value u16 · 18 dealt_bps u16 · 20 stamina_bell u32 · 24 ready_bell u32 · 28 from_bell u32 (presence) · 32 pend_bell u32 · 36 pend_op u8 (0 none, 1 Spend, 2 Split, 3 Absorb, 4 AbsorbedInto, 5 Leave(dissolve)) · 37 op_a u8 · 38 op_b u16 · 40 op_troops u32 · 44 op_ref u32 (split: new seq; merge: partner entry). This is the kernel's `Host` (id, faction, unit, troops, `Stamina`, `ready_bell`, `pending`) plus presence; `owner` is derived from `id`.

**ArrivalSlot (160 B)** — 16-B header · 16 P i16 · 18 Q i16 · 20 bell u32 · 24 faction u8 · 25 i u8 · 26 unit u8 · 27 stance u8 · 28 tile u8 · 29 flags u8 · 30 retreat_bps u16 (0 = none) · 32 host_id u64 · 40 citizen_tag u64 · 48 dep_mass u32 · 52 dealt_bps u16 · 54 rsv · 56 revealer [32] · 88 rent_payer [32] · 120 ev_slot u64 · 128 ev_price u64 · 136 ev_limit u32 · 140 ev_loaded u32 · 144..160 rsv.

**ArrivalDay (96 B)** — 16-B header · 16 P i16 · 18 Q i16 · 20 day u16 · 22 rsv[2] · 24 bits [18] (bit `b mod 144` set = at least one ArrivalSlot of `(P,Q,b)` was ever created) · 42 rsv[22] · 64 payer [32].

**ClashInputs (1,280 B)** — H then: 64 P i16 · 66 Q i16 · 68 bell u32 · 72 arrivals_mask u32 (24 bits gathered) · 76 rsv · 80 posture_mask u64 (M3) · 88 flags u8 (1 no-arrivals proven by ArrivalDay, 2 resolved) · 89 n_present u8 · 90 rsv[2] · 92 settled_mask u32 · 96 arrivals [24] × 40 B · 1056 postures [60] × 2 B (M3; zero) · 1176 resolver [32] · 1208 ev_slot u64 · 1216 ev_price u64 · 1224 ev_limit u32 · 1228 resolved_ts u32 (seconds since genesis) · 1232 first_gatherer [32] · 1264..1280 rsv.

Arrival record (40 B): 0 host_id u64 · 8 citizen_tag u64 · 16 dep_mass u32 · 20 troops u32 (after origin clash and march) · 24 stamina u16 · 26 retreat u16 · 28 dealt u16 · 30 faction u8 · 31 unit u8 · 32 tile u8 · 33 stance u8 · 34 present u8 · 35 fate u8 (0 none, 1 Stays, 2 Withdrew, 3 Bounced, 4 Retreated, 5 Destroyed) · 36 troops_after u32.

**SealVerdict (96 B)** — SP-V2 layout: 16-B header · 16 host u64 · 24 bell u32 · 28 code u8 · 29 rsv · 32 prover [32] · 64 slot u64 · 72..96 rsv.

**BellAnchor (144 B)** — SP-V2 layout to 112 (16 bell u32 · 20 region u8 · 21 net u8 · 24 round T u64 · 32 A i64 · 40 slot u64 · 48 sig48 [48]) then 96 payer [32] · 128 ev_price u64 · 136 ev_limit u32 · 140 rsv.

**SeedCache (144 B)** — SP-V2 layout to 112 (21 nonce · 24 round S · 32 seed [32] · 64 anchor key [32] · 96 A i64 · 104 slot u64) then 112 payer [32].

**AnchorArchive (5,280 B)** — 16-B header · 16 region u8 · 17 rsv · 18 day u16 · 20 rsv[4] · 24 tombstone bits [18] · 42 archived bits [18] · 60 rsv[4] · 64 entries [144] × {a_off u32 (A − end(b), s), seed [32]} (36 B) · 5,248 payer [32].

**DefenceClaim (128 B)** — 16-B header · 16 keeper [32] · 48 day u16 · 50 rsv · 56 claimed u64 · 64 count u32 · 68..128 rsv.

---

## 6. Instructions

### 6.1 Tag map and budgets

"Budget" is the CU ceiling the M1 gate asserts at adversarial fill (the client's CU limit is set to measured + 5%). "Basis" says where the number comes from. Heap: every instruction must stay ≤ 28 KiB of the default 32 KiB (4 KiB reserve); none requests a heap frame.

| Tag | Instruction | Kind | Writes | Budget CU | Basis | Tx B (est.) |
|---|---|---|---|---|---|---|
| 0x01 | CreateSeason | operator | Season, Frontier, ProvinceFund, DefencePool, BeaconLog ×16 (two txs: 0x01 + 0x02) | 60k | estimate | 900 |
| 0x02 | InitShards(f) | operator | 8 JoinShards | 40k | estimate (8 × safe init 3k) | 600 |
| 0x03 | ConsumeGenesisSeed | anyone | Season | 345k | PostAnchor measured 331.8–335.5k | 700 |
| 0x04 | EndSeason / 0x05 CloseSeason / 0x06 AbortSeason | anyone / operator | Season (+ closes) | 20k | estimate | 300 |
| 0x07 | SetWindowSchedule | operator | Season | 5k | estimate | 200 |
| 0x10 | PostAnchor | keeper, **critical** | 1 BellAnchor | 345k | 331.8–335.5k [measured] | 709 + 48 (evidence sysvar key, payer) ≈ 760 |
| 0x11 | PostAnchorMulti | keeper, critical | ≤ 16 BellAnchors | 400k | 332k + 16 × 3k [estimate] | ≈ 1,150 |
| 0x12 | PostSeed | keeper, critical | 1 SeedCache | 345k | 333.6k [measured] | 742 |
| 0x13 | PostBeacon | keeper | 1 BeaconLog | 340k | estimate from PostAnchor | 700 |
| 0x14 | ArchiveAnchors(r, day, ≤ 8 bells) | anyone | AnchorArchive, closes ≤ 8 anchors | 45k | 6.2k per close [measured, `t9`] | 1,100 |
| 0x15 | CloseSeedCache | anyone | closes 1 cache | 6k | estimate | 300 |
| 0x20 | OpenRing(d) | anyone | Frontier, RingSeed (init) | 25k | estimate | 400 |
| 0x21 | ConsumeRingSeed(d) | anyone | RingSeed | 345k | quicknet verify | 700 |
| 0x22 | OpenProvince(P, Q) | anyone | Province (init, funded), RingSeed (count), ProvinceFund | 220k | 167.7k [measured, E4 v0; non-crypto v0≈v2] + camps | 350 |
| 0x23 | FoldOccupancy(part) | anyone | Frontier | 2 × 25k | estimate | 900 |
| 0x30 | Join(faction) | player (no bucket before init) | Citizen (init), JoinShard | 20k | SP-V2 Join minus the Citizen bump search and TransferChecked [estimate] | 450 |
| 0x31 | SetSession | player | Citizen | 6k | estimate | 300 |
| 0x32 | SetVigil | player | Citizen | 6k | estimate | 250 |
| 0x33 | FileTicket(≤ 3 sites) | player | Citizen | 8k | 3.6k [measured, S-JOIN v0] | 420 |
| 0x34 | SettleTicket(citizen, k) | anyone | Holding (init or rewrite), Province, Citizen, JoinShard (+ displaced Citizen, JoinShard) | 35k | 19k fresh / 11k displace [measured v0] + seed cache read | 620 |
| 0x35 | ReleaseDormant | anyone | Holding (close), Province, Citizen, JoinShard | 25k | estimate | 450 |
| 0x40 | Harvest | player | Holding | 12k | 7.0k [measured, E4] | 320 |
| 0x41 | Build(item) | player | Holding (+ Province for walls) | 22k | estimate | 360 |
| 0x42 | Train(unit, n) | player | Holding | 20k | estimate | 330 |
| 0x43 | Muster(unit, troops, tile) | player, resident | Holding, Province | 25k | estimate | 380 |
| 0x44 | Dissolve(host) / 0x45 Garrison(delta) | player, resident | Holding, Province | 25k | estimate | 380 |
| 0x46 | Explore(host, ≤ 2 tiles) | player, resident | Holding, Province | 18k | estimate | 380 |
| 0x47 | SettleExplore | anyone | Holding, Citizen | 15k | estimate | 380 |
| 0x48 | DisbandStranded(host) | anyone | Province | 12k | estimate | 300 |
| 0x50 | Depart(host, commit, seal, arrive, tip, slot) | player, resident | Holding, Province, Citizen | 15k | 5.0k [measured, E4] + 2 sha256 over 197 B + log 165 B | ≈ 700 |
| 0x51 | **Reveal** | anyone, **critical** | 1 ArrivalSlot (+ ArrivalDay on the first reveal of the province-bell) | **26k** (target 20k) | 6.6k slot part [measured, SP-V2]; rest §10 | ≈ 780 |
| 0x52 | SettleDeparture | anyone | Holding, origin Province | 15k | estimate | 400 |
| 0x53 | ProveBadSeal | anyone | 1 SealVerdict | 50k | ≤ 48.0k [measured, SP-V2] | 588 + 32 |
| 0x54 | SettleTransit | anyone | Holding, ClashInputs, (home Province), ArrivalSlot (close), recipients | 30k | estimate | 650 |
| 0x60 | GatherClash(part) | anyone, critical | 1 ClashInputs | 40k | ≤ 30.3k [measured, SP-V2, 24 positions] + Holding reads | ≤ 1,200 |
| 0x61 | **ResolveFromInputs** | anyone, critical | Province, ClashInputs | **340k** (Phase A) / 290k (A+B) | 274.8k / 227.7k [measured, lab] + write-back | ≈ 430 |
| 0x62 | ResolveClash (feature `oracle`) | tests | Province | — | 322.7k [measured, lab, screened worst] | 1,182 |
| 0x63 | **SkipQuiet(b0, n ≤ 24)** | anyone | Province | 60k | estimate (§9) | ≤ 1,150 |
| 0x64 | CloseClashInputs / 0x65 CloseArrivalDay / 0x66 CloseArrivalSlot | anyone | closes | 8k | estimate | 300 |
| 0x70 | ClaimDefence(evidence…) | keeper | DefencePool, DefenceClaim, evidence accounts (claimed bit) | 25k | estimate | 700 |

### 6.2 Lifecycle, rings and provinces

**CreateSeason (0x01)** — accounts: `[authority s,w] [season w] [frontier w] [pfund w] [dpool w] [blog×16 w] [system]` (split into 0x01 without the 16 BeaconLogs and a follow-up 0x01 variant `InitBeaconLogs` if over 1,232 B). Data: id u64, parameters (the Season table's configurable fields), `genesis_ring g ≤ 4`, `r_max ≤ 128`, `reveal_window ∈ [600, 1800]`, `seed_margin ≥ 60`. Checks: parameters validated (kernel `IndexParams::validate`-style, plus `ruleset_hash == RULESET_HASH`); every target absent at its canonical address. Effects: Season (status Created, `created_ts = now`, `genesis_round = first round ≥ now + 600 + Δ`, `genesis_ts ≥ round_time(genesis_round) + 600`), Frontier, ProvinceFund (operator's escrow transferred), DefencePool (default 20 SOL), BeaconLogs. Log `SEASON_CREATED`.

**InitShards (0x02)** — `[authority s,w] [season] [js×8 w] [system]`, data faction; creates the 8 JoinShards of one faction (pre-funding-safe). Six calls.

**ConsumeGenesisSeed (0x03)** — `[payer s,w] [season w]`, data: round u64, compressed sig 48, hints. Checks: status Created; round == `genesis_round`; quicknet verify (hinted, `crypto::quick::verify`). Effects: `genesis_seed = seed_of(round, sig)` (SP-V2 `seed_of`, domain `PSF-SEED-v1`), status Seeded; RingSeeds 0..g become derivable. Log `GENESIS_SEED`. Status Running starts lazily when `now ≥ genesis_ts` (every instruction computes it).

**AbortSeason (0x06)** — anyone, if status Created and `now ≥ created_ts + 7 days` (genesis timeout) or status Seeded/Running and the beacon-outage rule of §8.10 holds (M2 completes the outage evidence). M1 effect: status Aborted; every instruction except closes refuses.

**OpenRing (0x20)** — `[payer s,w] [season] [frontier w] [ringseed w] [pfund] [system]`, data d. Checks: `d == frontier.rings_open + 1 ≤ r_max`; ≥ 1 bell since `last_ring_open_bell`; some wedge has `wedge_occupied ≥ θ × wedge_open` (θ early/late by `now − genesis_ts`), read from the **folded** values; `pfund.lamports − rent ≥ 6d × rent(4096)`. Effects: RingSeed(d) status requested, `t_open = now`, `round = first round ≥ now + 600 + Δ`; Frontier `rings_open = d` (provinces creatable after the seed). Log `RING_OPEN`.

**ConsumeRingSeed (0x21)** — `[payer s,w] [season] [ringseed w]`, data round, sig, hints; verify; `seed = seed_of`; status seeded. Log `RING_SEED`.

**OpenProvince (0x22)** — `[payer s,w] [season] [ringseed w] [pfund w] [province w] [frontier w] [system]`, data P i16, Q i16. Checks: ring of (P,Q) = d, RingSeed seeded, `|P|,|Q| ≤ 256`; province absent at `pv‖P,Q`. Effects: `init_funded` from ProvinceFund; kernel `terrain::generate_province(ring_seed, p)` → compact arrays and masks; sites; **barbarian camps**: kernel `camp::place(ring_seed, p, ring) → ≤ 2 site indices, troops` (new small kernel) as NEUTRAL site-mirror entries (state camp); `resolved_next = current bell`; region = kernel `region_of`; RingSeed `provinces_created += 1`; Frontier `open_sites += site_count − camps`, `provinces_opened += 1`, `wedge_open[w] +=`. Log `PROVINCE_OPEN` with the terrain digest. Budget 220k (E4: terrain 75k + sites 88.6k [measured]).

**FoldOccupancy (0x23)** — `[payer s] [season] [frontier w] [js×24]`, data part 0/1. Part 0: `acc = Σ` of 24 shards, `fold_bell = now_bell`, `fold_part = 1`. Part 1 (same bell as part 0, else `FoldStale`): add the other 24, write `occupied_sites`, `wedge_occupied`, `fold_part = 0`. Log `FOLD`.

### 6.3 Citizens and land

**Join (0x30)** — `[wallet s] [payer s,w] [session s?] [season] [frontier] [citizen w] [joinshard w] [system]`. Data: faction u8, session_expiry i64. Checks: status Running and `bell < join_close_bell`; faction < 6; citizen absent at `ct‖tag(wallet)`; JoinShard is `(faction, sha256(wallet)[0] mod 8)`; **capacity**: `(open_sites − occupied_sites) × 10,000 ≥ reserve_bps × open_sites` or a ring can still open (`rings_open < r_max` and ProvinceFund covers it) else `Capacity` (nothing charged, nothing created). Effects: Citizen init (`citizen_tag = first 8 B of its address`, bucket full, `explores_floor_left = 3`, vigil default 0 min UTC), `members += 1`. Log `JOIN`. No token CPI in M1.

**FileTicket (0x33)** — prologue + data ≤ 3 × (P, Q, site). Checks: citizen has no provisional or final first holding; each site's province is open and in the citizen's wedge (or, when the own wedge has no free site per the folded Frontier, the adjacent wedges' outermost ring) — the program checks the wedge rule arithmetically (kernel `ProvinceCoord::wedge`, `ring`); Effects: ticket stored with `ticket_bell = now_bell`, `ticket_next = 0`. Log `TICKET`.

**SettleTicket (0x34)** — `[payer s,w] [season] [citizen w] [holding w] [province w] [joinshard w] [seedcache] [anchor|archive] [displaced_citizen w?] [displaced_joinshard w?] [system]`, data k (preference index). Checks: `k == citizen.ticket_next`; the site (P,Q,site) from the ticket; the site's region seed `S(ticket_bell, r)` from a valid SeedCache or archive; score `= rand(S, "site", P‖Q‖site‖citizen_tag)` (kernel `rng::rand`); Province site state: free → **fresh**; provisional with `ticket_bell` equal and lower score and its challenge window still open → **displace** (the displaced Citizen's `flags`, `holding[0]` revert and `ticket_next += 1`; its JoinShard counters revert); otherwise `SiteTaken` and `ticket_next += 1` so the next preference can be tried. Effects: Holding init (or rewrite in place, `gen += 1`) with kernel `Holding::found(now, day, 1)`, shield `shield_secs(day)`; Province site mirror (state holding, faction, gen, `shield_until_bell`), `roster_epoch += 1`; Citizen provisional; JoinShard `first_holdings += 1`, `holdings_by_wedge`. Finality is lazy: the first owner action after `round_time(S) + 600` flips provisional → final (D12). Log `SETTLE` (fresh / displace, score).

**ReleaseDormant (0x35)** — anyone; Holding order 1 with `now ≥ last_owner_action + RELEASE_AFTER` (kernel `is_released`) and no transit in state 1–2; effects: Province site → released-free (`open`), Citizen flag refugee, JoinShard `released += 1`, Holding closed to its rent payer. Hosts of that holding in any Province become **stranded** (their `gen` no longer matches); `DisbandStranded` removes them.

### 6.4 Holdings and resident actions

All: player prologue, then `[holding w]` (owner = citizen, state ≥ 1). Every one calls kernel `Holding::settle(now)` first (harvest is implicit) and `touch_owner(now)`.

- **Harvest (0x40):** only the settle. Log `HARVEST` (stores digest).
- **Build (0x41)** `[province w?]`: data item id; kernel catalog `frontier::catalog::building(item, tier, n) → (cost, Effect, secs)` (new; costs by `duplicate_cost`), `pay`, `enqueue`. A Walls item also writes the Province site mirror's wall item `{effective_bell = bell_at(done_at) + 1, delta}` so the resolver computes `walls_at(start(b))` without reading the Holding. Log `BUILD`.
- **Train (0x42):** catalog `train(unit, n) → cost, secs`; the queue item's effect is `Effect::Troops{unit, n}` (new kernel variant) crediting `reserve[unit]` at completion.
- **Muster (0x43)** `[province w]`: data unit, troops, tile. Checks: province = holding's province; resident rule (`resolved_next + 1 ≥ now_bell`); `reserve[unit] ≥ troops`; kernel `Host::muster` bounds (100–30,000 troops, not Settler); roster caps after the change (kernel caps: 48 and 8 per faction counted over entries in state 1 or 2); a free entry (else `ProvinceFull`, delay only). Effects: `reserve -= troops`; entry state 2 (muster_pending, `from_bell = now_bell + 1`, `dealt_bps` from kernel `doctrine::of_faction(f)` at resolve time), `host_id` from `host_seq`; `roster_epoch += 1`. Log `MUSTER`.
- **Dissolve (0x44)** / **Garrison (0x45)**: pending op Leave (troops back to reserve after the clash) / kernel `GarrisonState::change(b, delta, resolved_next)` on the site mirror with reserve moved accordingly.
- **Explore (0x46)** `[province w]`: data host_id, tiles ≤ 2 (within 1 hex of the host's tile). Checks: host is the caller's, in roster, unit Scout; tiles not in `explored_mask`; holding's explore record free. Effects: set `explored_mask` bits (first claim wins), Holding explore record {bell, tiles, host}. Log `EXPLORE`.
- **SettleExplore (0x47)** `[holding w] [citizen w] [seedcache|archive]`: after `S(b, r_province)`; kernel `explore::roll(S, P, Q, tile, host) → Find` (new) with the floor reward while `explores_floor_left > 0`; credits goods (kernel `Holding::credit`) and `works`. Log `EXPLORE_RESULT`.

### 6.5 Marches, reveals and transits

**Depart (0x50)** — prologue + `[holding w] [province w]`. Data: host_id u64, `commit [32]`, `seal [165]`, arrive_bell u32, tip u64, transit_slot u8. Checks (in order):
1. `seal[0..]` length 165, first byte of U a valid compressed-G2 flag (cheap syntax check only; validity is ProveBadSeal's job);
2. host is the caller's (id → holding), in the roster of this province (state 1), no pending op; kernel `Host::depart(b, march_stamina(hexes), resolved_next)` — **the path is not known here** (sealed), so the stamina charged is the **maximum** march (`march_stamina(32)`) and Reveal refunds nothing (pinned: marching costs the full march stamina; simpler and leaks nothing) [design];
3. `arrive_bell ∈ [now_bell + 2, now_bell + 72]` (the exact earliest bell is checked by Reveal against the path);
4. `tip ≥ tip_min`; transit slot free; payer lamports ≥ tip + march_fee + seal_bond.
Effects: `seal_root = sha256(commit ‖ sha256(seal))` computed by the program; transit record {state 1, host, unit, faction, origin, depart_bell, arrive_bell, depart_ts, dep_mass = troops, march_stamina, dealt_bps, seal_root, tip}; escrow += tip + fee + bond (System transfer payer → holding); Province entry pending op Spend (kernel), flagged to become `departed` at settle; Citizen `arrivals += 1`. Log `DEPART` with the full 165-B seal and the commitment (< 1 KB).

**Reveal (0x51)** — critical; anyone with the plaintext. Accounts (in order):
`[revealer s,w (fee payer)] [season] [holding] [anchor (dest region)] [archive (dest region, day)] [beaconlog (dest region)] [arrivalday w?] [slot0..slot3 of (dest, arrive, faction); exactly one w] [path_province × 1–4] [instructions sysvar] [system]`.
Data: transit_slot u8, `plain [37]`, `k [16]`, `ct_hash [32]`, target i u8.
Checks in order (cheap first):
1. season Running (or Ended with `arrive_bell < end_bell`); holding owner/magic; transit state ∈ {1, 2} and `plain.host_id == transit.host_id`, `plain.arrive_bell == transit.arrive_bell`, `plain.version == 1`;
2. commitment: `salt = sha256("PS-SALT" ‖ k)`, `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain ‖ salt)`, `sha256(commit ‖ ct_hash) == transit.seal_root` else `CommitMismatch`;
3. destination `(P, Q, tile)` from `plain`; region `r = region_of(P,Q)` (the last path province carries it, checked); **window**: THE anchor at `an‖(arrive, r)` present → `now < A + W(arrive)` and `beaconlog.latest_round < S(A)` else `WindowClosed`; absent → AnchorArchive `aa‖(r, day(arrive))` must not tombstone `arrive` else `Archived` (SP-V2 `t8`, `t9`);
4. path: ≤ 32 steps of 3-bit directions from the origin tile's global hex; every step's province is one of the supplied path Provinces (owner/magic/key; ≤ 4 distinct, in first-entered order), every step passable (`passable_mask`), the last hex = destination tile; kernel `travel::path_cost` → `secs`; kernel `check_arrival_bell(genesis_ts, depart_ts, secs × doctrine travel_bps, arrive)`; destination tile is not the site of a **shielded holding of another faction** (site mirror `shield_until_bell > arrive`), and a host of a shielded holding may not target another faction's holding site (`Shielded`);
5. quota: read the 4 slots of `(dest, arrive, faction)` at their canonical addresses (absent or present), build `FactionSlots`, `SlotEntry{host_id, citizen_tag, troops: dep_mass}` and kernel `admit_arrival(p, arrive, &slots, x)`: `Refuse(_)` → `QuotaRefused` (fails, writes nothing); `Fill{i}` / `Displace{i}` → `i` must equal the target's index and that key must be the writable one (`SlotMoved` otherwise, the keeper re-reads and retries);
6. ArrivalDay: if bit `arrive mod 144` is clear, the ArrivalDay account must be writable (`NeedArrivalDay`); pre-funding-safe init if absent; set the bit.
Effects: slot created (pre-funding-safe, `rent_payer = revealer`) or overwritten on displacement (`rent_payer` unchanged, `revealer` replaced); fields from plain + transit (unit, faction, dealt_bps, dep_mass, citizen_tag from the owner's Citizen — carried in the Holding's `owner_citizen` → first 8 B); **fee evidence** (§3.5). **Nothing else is written**: not the Holding, not any Province (DESIGN §6.2, 3.1 corrections). Log `REVEAL` (host, dest, stance, retreat, fill/displace, displaced host id).

**SettleDeparture (0x52)** — anyone. `[payer s] [season] [origin province w] [holding w]`, data transit_slot. Checks: transit state 1; `province.resolved_next > depart_bell` (the origin's clash of the departure bell is applied; kernel `Host::march_values(depart_bell, resolved_next)`); the entry (state departed, id matches). Effects: transit `troops_after`, `stamina_after` = kernel `march_values`, state 2; entry freed; `roster_epoch += 1` only if roster membership changed (it did at the resolve). Log `DEPARTURE_SETTLED`. If the host was **destroyed** at its origin clash, the transit is marked state 3 with troops 0; Reveal of such a transit still works (it ranks by `dep_mass`, as 3.1 pins) and GatherClash records it with 0 troops (the kernel destroys a 0-troop arrival without effect) [design; keeps Reveal free of the origin].

**GatherClash (0x60)** — critical, idempotent. `[gatherer s,w] [season] [province (dest)] [anchor|archive] [arrivalday] [inputs w] [instructions sysvar] [system] [slot_k … ] [holding_k …]`, data bell u32, start u8, n u8, holdings bitmap u32 (which slots are expected present). Checks: province present; window closed (`now ≥ A + W`, from anchor or archive); inputs absent → init (`first_gatherer`), present → owner/magic/key; ArrivalDay bit clear → set `flags |= 1`, `arrivals_mask = 0xFFFFFF`, done (one gather proves "no arrivals"); else for each position k in `start..start+n` not yet in `arrivals_mask`: slot at its canonical address, absent → record not present; present → owner/magic/key fields; its host's Holding (address from host id) must be supplied, transit state 2 or 3 with matching `host_id` and `arrive_bell` (else `DepartureUnsettled`: lag waits for SettleDeparture); record = slot fields + `troops_after`, `stamina_after`. Positions commute and repeat as no-ops (SP-V2 `t2b`). Evidence recorded for the first gather of each part (per-position evidence would not fit; the part index and the landing slot are in the log). Log `GATHER`.

Per gather: ≤ 24 slot keys, ≤ 10 Holdings, ≤ 32 keys, ≤ 1,232 B; a full province-bell needs ≤ 3 gathers (24 present arrivals → 3 × 8) [design; SP-V2 measured 30.3k for 24 positions].

**ResolveFromInputs (0x61)** — critical, primary. `[resolver s,w] [season] [province w] [inputs w] [seedcache | archive] [anchor | archive] [instructions sysvar]`, data bell. Checks: `bell == province.resolved_next`; inputs complete (`arrivals_mask == 0xFFFFFF`; postures ignored in M1); seed from a SeedCache that records THE anchor's address and `round == S(A)` with `A` equal to THE anchor's (defence in depth, 3.1) — or from the archive entry for bells > 48 h old. Effects:
1. build `ClashInput`: residents = entries in state 1 whose presence covers the bell (`from_bell ≤ bell`), values `Host::values_at(bell)`, `dealt_bps = doctrine::of_faction(f).dealt_bps(Hold, false)`; garrisons = site mirror entries (holdings and camps) with `GarrisonState::at(bell)` and `walls = walls_committed + items effective ≤ bell > 0`; arrivals = present records (`dealt_bps` for arrivals from the doctrine with `arrival = true`); terrain from the compact arrays; relations `Relations{peaceful: province.relations}`;
2. kernel `frontier::clash::resolve_clash` (Phase A);
3. write back: residents' troops/stamina/`ready_bell` (kernel `Host::apply_clash`), `Stays`/`Withdrew` arrivals become roster entries (state 1, `from_bell = bell + 1`), garrisons (`GarrisonState::apply_clash`), camp cleared → site free and `camp_regrow_bell`; then settle every pending op of this bell (kernel `Host::settle`, `settle_merge`, `GarrisonState::settle`): musters join (state 1), departures → state 3 with post-clash values, splits create entries;
4. ClashInputs fate table (`fate`, `troops_after` per arrival), `flags |= 2`, resolver + evidence;
5. Province `resolved_next = bell + 1`, digest, `roster_epoch += 1`, event chain.
Log `CLASH` (bell, digest, engagements, fates compact). Budget 340k (Phase A: 274.8k measured worst kernel+load over 40 fills; screened worst hybrid 322.7k ≈ 302k full-gather [estimate]; +≈ 25k write-back and settles [estimate]).

**SkipQuiet (0x63)** — see §9. `[payer s] [season] [province w] [arrivalday(s) 1–2] [anchor|archive × n]`, data b0 (= `resolved_next`), n ≤ 24.

**ProveBadSeal (0x53)** — SP-V2 unchanged except accounts: `[prover s,w] [season] [holding] [anchor (any region, bell = arrive_bell)] [verdict w] [system]`, data transit_slot, `commit [32]`, `seal [165]`. Checks: `sha256(commit ‖ sha256(seal)) == transit.seal_root`; THE anchor of `(arrive_bell, r)` present (after archive: refused, `NoAnchor`); verdict absent; opener with FO check (`crypto::seal::open`); code 0 → `SealValid`. Effects: SealVerdict. Log `BAD_SEAL`. Budget 50k (≤ 48.0k measured).

**SettleTransit (0x54)** — anyone (the owner's client normally). `[payer s,w] [season] [holding w] [dest province w?] [inputs w | absent-proof] [slot w?] [verdict?] [home province w?] [revealer w?] [resolver w?] [prover w?] [dpool w]`, data transit_slot, `plain [37]`, `k [16]`, `ct_hash [32]` (anyone can decrypt the seal after `T(arrive)`). Checks: transit state 2 or 3; `now ≥ close(arrive, r) + 600`; plaintext opens `seal_root` (as Reveal) → destination; destination `resolved_next > arrive`. Outcome (D5, order-free):
- **SealVerdict present** (canonical `sv‖host,arrive`) → host destroyed (entry removed from the destination if it stayed; a stays-fate host is removed by SettleTransit writing the destination Province), prover gets tip + march fee + bond (`pay_or_divert`).
- else **ClashInputs present**: host in the arrival records → fate from the table (Stays/Withdrew: host is now a destination resident, transit closed; Bounced/Retreated: returns home, no loss; Destroyed: gone); revealer (from the slot) gets the tip, resolver gets the march fee, owner gets the bond back; slot closed to its rent payer; `settled_mask` bit set. Host not in the records → compare its `SlotEntry{host, citizen_tag, dep_mass}` with the faction's recorded final set: outranked by the 4th, or its citizen has a higher-ranked arrival → **bounce, no loss**, tip and bond back to the owner, march fee to the resolver; otherwise → **routed** (kernel `rout_survivors`, stamina 0, tip → DefencePool, bond back, fee to the resolver).
- else ClashInputs **closed** (absent; destination resolved past `arrive`; `CloseClashInputs` only runs after every admitted arrival was settled) → the host was not admitted: **routed** (the grace of 1,008 bells for owners to settle a refused arrival is the price of closing inputs; clients settle within a bell) [design].
Returning hosts rejoin the home Province as a muster-pending entry (writes the home Province) if an entry and the caps allow; otherwise their troops go to the Holding's `reserve`. Log `TRANSIT_SETTLED` (outcome, payments).

### 6.6 Beacons and archives

**PostAnchor (0x10)** — SP-V2 `post_anchor` plus: `[payer s,w] [season] [anchor w] [archive] [instructions sysvar] [system]`; data region, bell, round, `0 ‖ sig48`, hints. Order of checks: canonical address; already present → no-op (first post wins, 1.75k CU [measured]); tombstoned → `Archived`; `round == T(bell)`; verify; init; store A = Clock, slot, sig48, payer, evidence. Log `ANCHOR`.

**PostAnchorMulti (0x11)** — the combined 16-region form: one verification, up to 16 anchors, **present ones skipped** (so a held region costs the transaction nothing but that region's key); keepers always send per-region fallbacks alongside (SP-FEE D5 [measured]).

**PostSeed (0x12)** — SP-V2 `post_seed` plus payer and evidence; any nonce 0..255; `round == S(A)` from THE anchor. Log `SEED`.

**PostBeacon (0x13)** — `[payer s,w] [season] [beaconlog w]`, data round (must exceed `latest_round`), sig, hints; verify; store. Log `BEACON`. Keepers post it with every anchor round for the region (it closes Reveal early if the Clock is slow).

**ArchiveAnchors (0x14)** — `[payer s,w] [season] [archive w] [system] ( [anchor w] [anchor_payer w] [seedcache] ) × ≤ 8`, data region, day, bells. For each bell: `now ≥ A + archive_after`; a valid cache of THE anchor gives the seed; write entry `{A − end(b), seed}`, set `archived` and **`tombstone` bits first**, then close the anchor to its payer (`pay_or_divert`). Log `ARCHIVE`. SeedCaches of archived bells are closed individually (`CloseSeedCache`, refund to their payer). SP-V2 `t9`'s tombstone-then-close order is kept, and every "anchor present" check in §4 accepts the archive entry instead.

### 6.7 Defence pool

**ClaimDefence (0x70)** — `[keeper s,w] [season] [dpool w] [claim w] [evidence account w × ≤ 8] [anchor|archive per evidence]`. For each evidence account (ArrivalSlot, BellAnchor, SeedCache, ClashInputs; the account records `payer == keeper`, not yet claimed): lateness = `ev_slot − reference_slot ≥ lateness_slots` where the reference is THE anchor's slot for Reveal/PostSeed/gathers (the anchor of `T(b)` or the close), and the seed cache's slot for resolves; priority paid `p = (ev_price × ev_limit / 10⁶ + 2,500) / (ev_limit + 720 + 300 × writes + loaded/32KiB × 8)` must exceed the tip level and is capped at `defence_cap`; refund = `(min(p, cap) − p_tip) × cost` lamports; caps per bell-region and per keeper-day (`claim`). Effects: set the evidence account's claimed flag, pay the keeper from the pool (pool empty → partial, logged). Non-critical by construction (rule 2). Log `DEFENCE_CLAIM`.

### 6.8 Remaining instructions (compact)

| Tag | Accounts (in order; s = signer, w = writable) | Data | Checks (in order) | Effects / log |
|---|---|---|---|---|
| 0x04 EndSeason | `[any s] [season w]` | — | status Running, `now_bell ≥ end_bell` | status Ended / `SEASON_END` |
| 0x05 CloseSeason | `[authority s,w] [season w] [frontier w] [pfund w] [dpool w] [js×n w] [blog×n w]` | — | status Ended/Aborted; M1: all Provinces closed by `CloseProvince` crank first (Province count 0 in Frontier) | closes to authority / `SEASON_CLOSED` (M2 adds claims before close) |
| 0x07 SetWindowSchedule | `[authority s] [season w]` | window u32, from_bell u32 | `600 ≤ window ≤ 1800`; `from_bell ≥ now_bell + 144`; no pending change | `window_next`, `window_from_bell` / `WINDOW` |
| 0x31 SetSession | prologue (actor = wallet only) | session [32], expiry i64 | expiry ≤ now + 30 days | Citizen session / `SESSION` |
| 0x32 SetVigil | prologue | start_min u16 | kernel `Vigil::request_change` (≥ 24 h notice, ≤ 1 per 7 days, effective at a UTC midnight) | Citizen vigil fields / `VIGIL` |
| 0x48 DisbandStranded | `[any s] [season] [province w] [holding? (absent or other gen)]` | entry index | entry's host id → Holding address; Holding absent or `gen` ≠ id's gen | entry freed (troops lost), `roster_epoch += 1` / `STRANDED` |
| 0x15 CloseSeedCache | `[any s] [season] [cache w] [archive] [payer w]` | bell, region, nonce | archive `archived` bit of bell set; `payer == cache.payer` | close to payer (`pay_or_divert`) |
| 0x64 CloseClashInputs | `[any s] [season] [province] [inputs w] [first_gatherer w]` | P, Q, bell | `flags & resolved`; every present arrival's `settled_mask` bit set; `resolved_ts + clash_close_grace × 600 ≤ now` | close to first gatherer |
| 0x65 CloseArrivalDay | `[any s] [season] [province] [day w] [payer w]` | P, Q, day | `province.resolved_next > 144 × (day + 1) − 1` (every bell of the day resolved or skipped) | close to payer |
| 0x66 CloseArrivalSlot | `[any s] [season] [inputs (absent)] [province] [slot w] [rent_payer w]` | P, Q, bell, f, i | ClashInputs of (P,Q,bell) closed and Province resolved past bell (the slot's transit was routed or never settled) | close to rent payer |
| 0x24 CloseProvince | `[any s] [season] [province w] [pfund w] [frontier w]` | P, Q | season Ended/Aborted and `now ≥ end + 72 h` | close into ProvinceFund; `provinces_opened −= 1` |
| 0x36 CloseHolding / 0x37 CloseCitizen | `[any s] [season] [holding|citizen w] [rent_payer w]` | — | season Ended/Aborted, `now ≥ end + 72 h`, no escrow left (transits settled or refunded to the owner in the same instruction) | close to rent payer (M2 inserts claims before this) |

Every one of these is permissionless except the authority rows, idempotent (a second call finds the account absent and fails with `BadAccount`), and outside the critical path.

---

## 7. Error codes

`Custom(code)`; codes are stable across versions; one svm test per (instruction, code) pair (L1).

| Code | Name | Code | Name |
|---|---|---|---|
| 1 | BadData | 26 | NotResident (province not resolved through b − 2) |
| 2 | BadAccount (owner/magic/season/key) | 27 | ProvinceFull (entries or caps) |
| 3 | BadAddress (not canonical) | 28 | HostBusy (pending op) |
| 4 | NotSigner / Auth | 29 | Cooldown / NoStamina (kernel `HostError`) |
| 5 | WrongStatus (season) | 30 | TransitBusy / TransitState |
| 6 | RulesetMismatch | 31 | ArrivalBell (kernel `TravelError::TooEarly/TooLate`) |
| 7 | WrongRound | 32 | Path (not adjacent, impassable, too long, > 4 provinces, wrong end) |
| 8 | NoAnchor | 33 | CommitMismatch |
| 9 | BadHint / BadPoint / PairingFailed (crypto) | 34 | QuotaRefused (kernel `SlotRefusal`) |
| 10 | Capacity | 35 | SlotMoved (target index differs from the decision) |
| 11 | SiteTaken | 36 | NeedArrivalDay |
| 12 | WindowClosed | 37 | Shielded |
| 13 | TooEarly (gather, resolve, archive, settle) | 38 | DepartureUnsettled |
| 14 | SealValid | 39 | NotGathered |
| 15 | Kernel (rules error, with a sub-code in the log) | 40 | OutOfOrder (resolve bell ≠ resolved_next) |
| 16 | Archived | 41 | NotQuiet |
| 17 | Bucket | 42 | InputsOpen (close before settles/grace) |
| 18 | NotTopLevel | 43 | AlreadyClaimed / NotEligible (defence) |
| 19 | Overflow (explicit checked arithmetic) | 44 | FoldStale |
| 20 | NotOwner (host/holding) | 45 | TicketState |
| 21 | Insufficient (resources or reserve) | 46 | NotDormant / HasTransits |
| 22 | QueueFull | 47 | Explored |
| 23 | NoTicket | 48 | SessionExpired |
| 24 | NotFinal | 49 | WrongRegion |
| 25 | TooManyAccounts | 50 | Aborted |

---

## 8. Log records (verifier and herald input)

One `sol_log_data` call per write: fields `["PSF1", record]`, `record = kind u8 ‖ version u8 ‖ bell u32 ‖ entity key ‖ payload ‖ new_event_head[32]` (≤ 128 B except DEPART, which appends the 165-B seal and the 32-B commitment). The event head written is `sha256(prev_head ‖ record_without_head)` of **each chained entity** the record touches (K5); a record touching two chained entities carries both heads.

| Kind | Record | Payload (beyond key) | Chains |
|---|---|---|---|
| 1 SEASON_CREATED | season | parameters digest, genesis_round, genesis_ts | Season |
| 2 GENESIS_SEED / 3 RING_OPEN / 4 RING_SEED | season / ring | round, seed | Season / Frontier |
| 5 PROVINCE_OPEN | P,Q | ring, region, terrain digest, sites, camps | Province, Frontier |
| 6 FOLD | — | occupied, wedge occupancy | Frontier |
| 10 JOIN | citizen | faction, shard, bell | Citizen, JoinShard |
| 11 TICKET / 12 SETTLE / 13 RELEASE | citizen / holding | sites; score, displaced citizen | Citizen, Holding, Province, JoinShard |
| 20 HARVEST / 21 BUILD / 22 TRAIN | holding | stores digest; item; unit, n | Holding |
| 23 MUSTER / 24 DISSOLVE / 25 GARRISON / 26 EXPLORE / 27 EXPLORE_RESULT / 28 STRANDED | host / holding | values | Holding, Province (, Citizen) |
| 30 DEPART | host | origin, depart/arrive bell, dep_mass, tip, `seal_root`, **commit[32], seal[165]** | Holding, Province, Citizen |
| 31 REVEAL | slot | host, dest tile, stance, retreat, i, fill/displace, displaced host, evidence | (slot content) |
| 32 DEPARTURE_SETTLED | host | troops_after, stamina_after | Holding, Province |
| 33 BAD_SEAL | host, bell | code, prover | — |
| 34 TRANSIT_SETTLED | host | outcome, troops, payments | Holding, ClashInputs (, Province) |
| 40 GATHER | P,Q,b | positions, mask, no-arrivals flag | ClashInputs |
| 41 CLASH | P,Q,b | outcome digest, engagements, fates | Province, ClashInputs |
| 42 SKIP | P,Q | b0, n, quiet digest | Province |
| 50 ANCHOR / 51 SEED / 52 BEACON / 53 ARCHIVE | b, r | round, A, slot, payer (seed) | — |
| 60 DIVERT / 61 DEFENCE_CLAIM | recipient | amount, reason | — |

The herald folds these per province and bell; the verifier re-verifies every anchor, seed and beacon against the quicknet key, decrypts every seal after `T(b)`, re-checks every SealVerdict, re-runs every clash natively from its gathered inputs and compares digests, re-runs every SkipQuiet (is the run quiet?), and walks every entity chain from its on-chain head (DESIGN §9).

---

## 9. Quiet bells: options and recommendation

**Problem (DESIGN §6.3, R19).** A resident may act at bell b only when its Province is resolved through b − 2. A bell with no arrivals still needs proof of "no arrivals": with 24 per-slot accounts that is 24 absence keys per bell, one bell per transaction; a province idle for a day needs up to 144 transactions (≈ 10.5M CU at S-SIZE's 73k, ≈ 7.2M at 50k [model, `rev31` §G]) before its resident can act.

**Facts that constrain the choice.** Destinations are sealed until Reveal, so nothing can be written at the destination before Reveal; Reveal is the only write that knows the destination; every critical write is a lock target at p × 40M per block per known account, and one multi-lock stream holds 20–60 known accounts at one account's price [measured, SP-FEE (b), m0c (g)] — so consolidating a province-bell's writes behind fewer accounts does **not** make them cheaper to attack. Closure of bell b's window needs THE anchor of (b, r) or its archive entry (anchors are not monotone in b).

| Option | Proof per quiet bell | Tx per idle day | Critical-path change | Verdict |
|---|---|---|---|---|
| Q0 budget it (3.1 default) | 24 slot absences + anchor | 144 | none | 144 sequential Province writes before a resident acts; minutes of latency; rejected |
| Q1 ArrivalMark per province-bell | 1 mark absence + anchor | ≈ 72 at 2 keys/bell … 12–14 bells per tx ≈ 11 | first Reveal of a province-bell writes the mark | works; 2 keys per bell |
| **Q2 ArrivalDay bitmap per province-day (recommended)** | 1 bit + anchor | **≈ 6** (≤ 24 bells per tx) | first Reveal of a province-bell sets one bit (writes the ArrivalDay) | **1 key per bell + 1 per day** |
| Q3 consolidated ArrivalBook (all 24 slots in one account) | 1 absence + anchor | ≈ 11 | Reveal always writes the shared book; rent and refunds per entry get awkward (realloc, displaced revealers) | same lock profile as Q2, more code; rejected |
| Q4 per-origin arrival histogram | read ≤ 37 origin provinces | — | Depart writes an arrival-bell counter | cannot prove absence for a specific destination without leaking it; rejected |
| Q5 let resident actions skip quiet bells without proof | none | 0 | changes the roster-freeze semantics (pending ops under lag) | reopens the kernel's `check_issue` contract; rejected |
| Q6 monotone anchors (anchor b needs anchor b − 1) + Q2 | 1 bit per bell, 1 anchor per run | ≈ 3 | PostAnchor reads the previous anchor | holding one anchor then pauses the whole region, not one bell; rejected for M1 |

**Recommendation: Q2.**
- **ArrivalDay** `ad‖(P, Q, day)`, 96 B. Reveal reads it; if the bell's bit is clear the transaction must list it writable, the program creates it pre-funding-safely if absent and sets the bit. Lock analysis: holding the ArrivalDay blocks only the **first** Reveal of each bell into that province (later ones read it); it is a known account of one province, priced like the province-bell's slots (one stream) — no new cost class and no side-wide effect. A pre-funded ArrivalDay is absent = all bits clear; since only the program can allocate it and every slot creation sets the bit in the same transaction, "bit clear ⇒ no slot of (P,Q,b) was ever created" holds by construction.
- **SkipQuiet(b0 = resolved_next, n ≤ 24)** writes only the Province: for each bell b in order: (1) window closed (`now ≥ A + W`, THE anchor or the archive entry); (2) ArrivalDay bit of b clear (ArrivalDay absent counts as clear); (3) apply the pending ops that take effect at b (musters join, departures leave — the same `settle` path as the resolve); (4) quiet test: kernel `clash::is_quiet(rules, inp)` with no arrivals — **cached** by `(roster_epoch, relations)`: while neither changes the answer is the same, so one kernel run covers the whole run; (5) on a non-quiet bell stop with `NotQuiet` (the client then gathers and resolves that bell normally). Effects: `resolved_next = b0 + k`, stamina clocks and camp regrowth advance lazily (kernel closed forms), `SKIP` log with the quiet digest. Sieges (M3) use `Siege::advance_quiet` in the same step.
- **Cost** [estimate from measured primitives]: per bell ≈ 2k (anchor address + read ≈ 1.5k, bit ≈ 0.1k, settle ≈ 0.3k); one `is_quiet` kernel run on residents only ≈ 10–30k with Phase A (the kernel's K1+K3+K6 steps with no engagements); total ≤ 60k for 24 bells, **≈ 6 transactions and ≤ 0.36M CU per idle day** against 144 transactions and 7–10M CU. Tx size: 24 anchor keys + 2 ArrivalDays + 4 fixed ≈ 30 keys ≈ 1,150 B.
- **GatherClash gains too:** a province-bell whose bit is clear is gathered in one step (no 24 slot keys).
- **Gate (M1):** `skip_equals_resolve` — for random rosters and runs with and without arrivals, SkipQuiet over a run leaves the Province byte-identical to gathering and resolving each bell; a Reveal landing into a skipped bell is impossible (window closed); a pre-funded ArrivalDay counts clear; a forged ArrivalDay (wrong key fields) is refused.

---

## 10. The full Reveal CU (C4 input)

Reveal is not built yet; its CU sets the default tip's priority (0.433 at 16k, 0.35 at 20k, 0.27 at 26k [model, SP-FEE]). Estimate from measured parts (SBPF v2, non-crypto v0 ≈ v2 within 0.5% [measured, SP-V2]):

| Part | CU | Basis |
|---|---|---|
| prologue, season, holding, transit | 1.5k | S-SIZE `checked` 0.86k + Holding read |
| commitment (3 sha256 over ≤ 101 B) | 1.2k | E4 `commit_ok` 1.13k [measured] |
| anchor + archive + BeaconLog checks | 2.5k | SP-V2 slot part includes anchor/archive (6.6k total with creation) |
| path walk, 32 steps over ≤ 4 provinces | 4–9k | E4: 2.8k (8 steps/2 provinces), 5.0k (16 steps) [measured]; bitmask lookups instead of terrain decode |
| quota: 4 slot addresses + reads + `admit_arrival` (≤ 4 `slot_key` sha256) | 4k | 4 × 0.5k address + 4 × 0.3k hash + compare |
| slot creation (safe init) + write | 4k | SP-V2 6.6k includes the anchor check [measured] |
| ArrivalDay (first reveal only: address + safe init + bit) | 3k | as slot init |
| evidence (instructions sysvar) + log | 2.5k | estimate |
| **Total** | **≈ 20–27k** (first reveal of a province-bell), **≈ 17–24k** otherwise | [estimate] |

**Plan.** Build Reveal in M1 week 2; measure it under LiteSVM at the worst case (32 steps, 4 provinces, displacement, first-of-bell ArrivalDay init) and report it to the C4 model the same week. **Budget 26k**; if the measurement exceeds 20k, two optional cuts keep it near 20k: (a) move the path's province lookups to precomputed per-province neighbour gates (a 6-entry gate table in each Province replaces `province_of` per step); (b) pass `salt` instead of `k` (saves one sha256, +16 B). The tip in priority terms (`tip = 2,500 + 0.433 × (limit + 1,336)`) follows whatever is measured.

---

## 11. Kernel calls and new kernel work (rules side)

**Called unchanged:** `clash::{resolve_clash (Phase A), is_quiet, admit_arrival, apply_slot, quota_set (tests), slot_key, seed_round, reveal_open, BeaconClock}`, `host::{Host::{muster, values_at, depart, apply_clash, settle, march_values, route}, settle_merge, GarrisonState, rout_survivors, Presence, effective_bell}`, `holding::{Holding::{found, settle, touch_owner, enqueue, pay, credit, walls_at, shielded, is_released}, duplicate_cost, shield_secs}`, `travel::{path_cost, check_arrival_bell, earliest_arrival_bell, march_stamina, bell_at, bell_start}`, `terrain::generate_province`, `geometry::{ProvinceCoord::{ring, wedge, index, from_index}, region_of, tile_index, locate}`, `doctrine::of_faction(f).{dealt_bps, travel_secs, upkeep, wall_cost}`, `rng::{rand, tie_key}`, `combat` via the clash.

**New or changed kernel items M1 needs** (owned by the rules area; listed so the program design is complete):
1. **Clash Phase A** (the lab patch, `resolve_clash` rewritten; the original kept as a host-only test oracle `resolve_clash_ref` with the equivalence test) — no rules change.
2. **Clash Phase B** (optional, rules change): one sha256 per engagement; requires re-running the doctrine CI proxy, the 1,500-season band and the golden digests.
3. `clash::anchor_round(clock, genesis_ts, bell)` = first round ≥ `end(b)` (D11).
4. `holding::Effect::Troops{unit, n}` and `frontier::catalog` (buildings, training costs and times).
5. `frontier::camp` (placement, troops by ring, loot, regrowth) and `frontier::explore::roll`.
6. The §9.4 kernel-bounds family (M0-FINAL §5 item 1): `clash::validate` refusing troops > `MAX_HOST_TROOPS`, `dealt_bps` > cap, stamina > cap — **the program also enforces these on every write**, so the kernel checks are a second line.
7. Relations stay symmetric: the program never writes `Province.relations` except through `Relations::set_peaceful` (M3); Phase A is exact even for asymmetric matrices [measured, equivalence test].

---

## 12. Clash-kernel optimisation: what was measured and what to adopt

**Where the CU went** (SP-V2 `t5b`, 40 adversarial fills, SBPF v2 [measured]): engagements 42% (≈ 1.84k CU each), the field-holding step 39% (116k–216k: its withdraw search rescans every unit for each withdrawing host × 6 neighbours, and hashes a field tie key per faction per tile), fair share 7%, validate/units 6%.

**Phase A (implemented in the lab, digest-identical):**
1. units bucketed by tile once (tiles do not change before step 5), so retreat sums, fair share, engagements and the field step read one hex's units instead of scanning all ≤ 84;
2. damage dealt/taken per (tile, faction) as per-tile arrays; the stamina refund decided when the tile's engagements end (no linear `bump` search per engagement);
3. withdraw search reads an occupancy table (hosts per tile, faction mask per tile) updated as hosts withdraw, and a compile-time 61 × 6 neighbour table (`NB0`, wedge rotation by index);
4. the field tie key `rand_id(cs, "field", f)` does not depend on the tile: hashed once per faction;
5. garrison results from a per-tile mask of staying hosts.

**Measured** (lab `m1/lab/clash-opt`, SBPF v2, LiteSVM 0.16, same fills and harness as SP-V2):

| Metric | SP-V2 baseline | Phase A | Phase A + B |
|---|---|---|---|
| ResolveFromInputs, worst of 40 fills | 540,891 | **274,798** | **227,652** |
| ResolveClash single-tx, worst of 40 | 561,713 | 295,620 | 248,490 |
| ResolveClash single-tx, worst of 12 screened from 1,200 | 632,643 | **322,736** | 267,261 |
| Kernel alone, min–max over 40 | 316,201–510,307 | 167,255–244,213 | 144,952–197,083 |
| Engagement step, CU per engagement | 1,843 | 1,137 | 815 |
| Field step (K6), max | 215,505 | 39,887 | — |
| Heap peak (trace build) | 25,832 B | **21,024 B** | 21,024 B |
| Gather (unchanged) | 30,323 | 30,323 | 30,323 |
| Equivalence with the original kernel | — | **4,320 / 4,320 identical** (outcome and digest; 14,189 withdrawals, 335,180 engagements exercised); mutation check: 5 of 7 mutants caught; survivors: one equivalent (the faction-mask update after a withdrawal, which can only add a bit already set) and one untested corner (the refund's `d > 0` guard, reachable only when an engaged faction both deals and takes 0 damage on a hex) — the rules property test must add that corner | n/a (rules change) |

After Phase A the engagements are 55% of kernel CU and three sha256 per engagement dominate them; Phase B cuts that to one. K1 (validate + 72 `tie_key` hashes, ≈ 25k) is next if more is needed (lazy tie keys give nothing at the adversarial worst case, where every mass ties).

**Recommendation.** Adopt Phase A in `permutation-rules` at the start of M1 (patch `m1/lab/clash-opt/phaseA.patch`, with `host/tests/equiv.rs` promoted to a rules property test against the kept reference). Budget ResolveFromInputs at **340k** and a worst-case clash at **460k** (≤ 3 arrival gathers × 40k + 340k). Take Phase B before the M1 exit only if the doctrine proxy gate and the scheduled 1,500-season band pass on the new variance stream (then 290k / 410k). Heap margin rises from 21% to 36% of 32 KiB; keep the ≤ 28 KiB gate. **Capacity effect** [model, `m1/lab/rev31-m1`]: at 50k players the adversarial load falls from 8.8% to 7.8% of block CU (26k Reveal) or 7.3% (20k Reveal); the 10% line moves from ~60k to **~65–70k (30/h bucket)** and ~85–90k (20/h). The bucket and reveals now dominate; further clash work buys little capacity.

---

## 13. Critical path and lock surface (M1)

| Write | Accounts written | Deadline? | Held by an attacker → |
|---|---|---|---|
| Reveal | ArrivalSlot (+ ArrivalDay once per province-bell) | yes (`A + W`) | the priced attack C4 covers; ArrivalDay is the same province's known account (one stream) |
| PostAnchor / Multi | 1 / ≤ 16 BellAnchors (present skipped) | no (delay only) | region windows stay open; seed later |
| PostSeed | 1 SeedCache (any nonce) | no | keepers use another nonce |
| GatherClash | ClashInputs | no | resolve waits |
| ResolveFromInputs | Province + ClashInputs | no | resolve waits (same province-bell accounts) |
| SkipQuiet | Province | no | resident actions wait |
| ProveBadSeal | SealVerdict | before archive (48 h) | proof waits |
| Keeper fee payers | the payer | — | ≥ 150 rotating payers (SDK rule) |
| DefencePool | never on a critical transaction | — | claims wait |

Reads are not blocked by write-lock cost filling (the per-account cost limit counts writable accounts) [measured mechanism, SP-FEE]; so Reveal's read-only Holding, anchor, archive, BeaconLog, sibling slots and path Provinces add no cost class.

---

## 14. LiteSVM harness and the M1 gates

### 14.1 Harness (`permutation-frontier/svm-tests`, port of `permutation-chain/svm-tests` + SP-V2 `host`)

- Own `[workspace]`, lock and toolchain **1.95.0** (LiteSVM 0.16 does not build on 1.89); `litesvm = "=0.16.0"`, mainnet feature set **with SIMD-0388 BLS12-381 syscalls**, 64 account locks, rent 5,080 lamports/byte (SP-V2 `lab.rs`); the program crate without `program` for layouts (no mirrors); `permutation-rules` with `std` for native oracles.
- `run.sh` builds with `scripts/build-frontier.sh` first (plain) and a `trace` build; tests take `PSF_SO` / `PSF_SO_TRACE`; prints binary hash and age (v9 README behaviour).
- `src/chain.rs` (`Chain`: send with the client's budget profile incl. `SetLoadedAccountsDataSizeLimit` 64 KiB, `send_as`, clock control, `fork`, `assert_program_err(code)`), `src/ix/*.rs` (one builder per instruction; account order equals the gateway client), `src/fixtures/beacons.rs` (real quicknet beacons with hints: SP-V2 `beacons/` + a fetched pack for a 7-day window, read-only download), `src/fixtures/tlock.rs` (valid, garbage, tampered, wrong-round seals from S-TLOCK vectors), `src/world.rs` (seasons, provinces, adversarial fills from SP-V2 `Scenario`), `src/records.rs` (PSF reader), `src/budget.rs` (`need()` = CU, heap peak from the trace build, tx bytes, locks; ceilings from §6.1), `src/cover/*.rs` (the (instruction, error) coverage table with a PENDING ledger; `RELEASE_CHECK=1` fails on PENDING).
- Time: LiteSVM clock set to the recorded beacons' round times (seasons whose `genesis_ts` falls in the fixture window), so every seal, anchor and seed is a real quicknet value.

### 14.2 Gate tests (M1 exit, all on the SBPF v2 build)

| # | Gate | What it asserts |
|---|---|---|
| G1 | Budgets | every instruction ≤ its §6.1 ceiling at its adversarial fill (ResolveFromInputs over the 40 SP-V2 fills + the 1,200-fill screen on the **full-gather** path, M0-FINAL §5 item 4; Reveal at 32 steps/4 provinces/displacement/ArrivalDay init; gathers at 24 positions + 10 Holdings); heap ≤ 28 KiB; tx ≤ 1,232 B; locks ≤ 64 |
| G2 | Pre-funding, per init path | Season, Frontier, RingSeed, ProvinceFund, JoinShard, BeaconLog, DefencePool, Citizen, Holding, Province, ArrivalSlot, ArrivalDay, ClashInputs, SealVerdict, BellAnchor, SeedCache, AnchorArchive, DefenceClaim: pre-fund (rent, 10× rent), initialise successfully, payer pays only the shortfall; a stand-alone probe shows `CreateAccount` failing on the same address (regression) |
| G3 | Forgery, per keyed account | wrong address, wrong owner, wrong magic, wrong season, wrong key fields each refused with the pinned code; absence at a non-canonical address refused |
| G4 | No Reveal after the close | property over random A, Clock, window schedule: supplied/omitted/forged anchor or archive, tombstoned bell: no Reveal lands at `now ≥ A + W` or once BeaconLog ≥ S (SP-V2 `t3`, `t8`, `t9` generalised) |
| G5 | No choice among published seeds | one anchor per (b, r); every cache nonce gives the same seed; no second anchor after archive; ResolveFromInputs refuses a cache whose A differs from THE anchor |
| G6 | Absence | a resolve with a present slot omitted fails; a pre-funded never-revealed slot and ArrivalDay count absent; a bell with a clear bit gathers in one step |
| G7 | Lag invariance (program level) | hold the origin Province, the origin region's anchor and SettleDeparture past the destination's close: the destination's result is byte-identical to the unheld run |
| G8 | Gather equivalence | gathers in random order/repeats + ResolveFromInputs == ResolveClash (oracle build) == native kernel digest |
| G9 | Quota fairness | 8 reveal orders × random arrivals: same final slots = kernel `quota_set`; SettleTransit outcomes (admitted/bounced/routed) identical across orders |
| G10 | Seal verdict | valid (refused 14), garbage, tampered V/U, wrong round, logged-vs-sent mismatch, forged commitment; on-chain verdict equals stock tlock decryption + commitment opening (S-TLOCK js vectors) |
| G11 | Quiet skip | `skip_equals_resolve` (§9) |
| G12 | Transit settlement | every outcome branch incl. bad seal before/after resolve, closed inputs, stranded hosts; pay-or-divert with a drained recipient |
| G13 | Coverage | one test per (instruction, error code); `RELEASE_CHECK=1` passes |
| G14 | Bot season (CI size) | 100 bots × 2 simulated days in LiteSVM through the program, native kernel re-run every bell (v9 `BotSeason` pattern); verifier PASS; one tampered log → FAIL |

The **1,000-bot, 7-day local season** (the M1 exit) runs on a local validator with the BLS12-381 syscalls (Agave ≥ 4.0; the repo's 3.1.9 has none [measured]) in real time with live quicknet, on ports outside the reserved list (e.g. RPC 39970 range as in SP-FEE m0c), with the keeper fleet (≥ 150 payers) and the herald; that run belongs to the season/keeper areas and uses this program unchanged.

---

## 15. Build order inside M1 (program only) [estimate, one engineer-week units]

| Week | Program work | Gates reached |
|---|---|---|
| 1 | crate, build script, addr/init/layout/events, Season/Frontier/shards, beacons ported from SP-V2 (+ payer, evidence, archive table), Phase A kernel patch | G2, G3, G5 (beacons), G4 (anchor side) |
| 2 | Join, tickets, OpenRing/OpenProvince, Holding actions, Muster; **Reveal** built and measured (C4 input) | G1 (Reveal), G6 |
| 3 | Depart, SettleDeparture, GatherClash, ResolveFromInputs, oracle ResolveClash | G7, G8, G9 |
| 4 | SettleTransit, ProveBadSeal, SkipQuiet, closes, ArchiveAnchors | G10, G11, G12 |
| 5 | Explore, dormancy, ClaimDefence, coverage table, budgets on all instructions | G1, G13 |
| 6 | bot season in LiteSVM, verifier hooks, hardening; hand-off to the 1,000-bot local run | G14 |

---

## 16. Risks and open questions

**Risks**
1. **Holding size and rent** (1,280 B, +19% per-player rent) — trim `Accrual.cap` (derivable from tier) and transit fields if the owner prefers the 832-B figure.
2. **SettleTransit's closed-inputs fallback routes** an unsettled refused arrival after 1,008 bells; clients must auto-settle. Alternative: keep a 6-faction threshold record in the Province (no grace), at +≈ 200 B of Province per recent bell — rejected for size.
3. **Depart charges the maximum march stamina** (the path is sealed). This penalises short marches' stamina; an alternative is charging at Reveal via a pending refund applied by SettleTransit.
4. **Phase B changes every variance draw**; doctrine balance must be re-confirmed (gates exist).
5. The **ArrivalDay write** makes the first Reveal of a province-bell a 2-account write; the argument that it adds no cost class rests on SP-FEE's multi-lock measurement (one stream holds 20–60 known accounts at one account's price).
6. **Evidence parsing** trusts the ComputeBudget instructions of the same transaction; fees are charged on the declared limit, so the recorded (price, limit) is what the keeper paid [measured, SP-FEE fee rule]; the signature count is assumed 1.
7. The **1,000-bot local season needs a validator with BLS syscalls** and a real-time week.

**Open questions for review / owner**
1. Accept D2 (with-seed addresses for player accounts instead of PDAs)?
2. Accept D3 (ArrivalDay + SkipQuiet) as the quiet-bell proof?
3. Accept D4–D6 (SettleDeparture; rank-based transit outcome; ResolveFromInputs writing ClashInputs)?
4. Adopt clash Phase B (rules change) before the M1 exit, or keep Phase A only?
5. Accept the ticket challenge-window correction (D12) and `T(b)` round-up (D11)?
6. Is Depart's maximum-stamina charge acceptable for M1?
7. Should ClaimDefence ship in M1 (as specified) or with M2's money work (the evidence fields are in M1 either way)?

---

## 17. Links

- This design: `(scratch)/scratchpad/frontier/m1/design/program.md`
- Clash lab (Phase A source tree, patches, equivalence test, results): `(scratch)/scratchpad/frontier/m1/lab/clash-opt/` — `phaseA.patch`, `phaseB.patch`, `host/tests/equiv.rs`, `results-phaseA/`, `results-phaseB/`, `logs/`, `README.md` (the `.so` files under `program/out/` are the Phase B build)
- Capacity re-run: `(scratch)/scratchpad/frontier/m1/lab/rev31-m1/m1-results.txt`
- Inputs: `.claude/worktrees/frontier-integ/docs/frontier/DESIGN.md`, `docs/frontier/m0/{M0-FINAL,SPIKE-SP-V2,SPIKE-SP-FEE}.md`; SP-V2 lab `(scratch)/scratchpad/frontier/m0b/spikes/SP-V2/`; S-SIZE-JOIN `(scratch)/scratchpad/frontier/m0/spikes/S-SIZE-JOIN/results.txt`; E4 `(scratch)/scratchpad/openworld/lab/E4-free/sbf-results.txt`; kernels `permutation-rules/src/frontier/`; v9 harness `permutation-chain/svm-tests/`
