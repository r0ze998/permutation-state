# W1-E abi: notes

**Unit:** W1-E abi (wave 1). **Branch:** `frontier/m1-W1-E`, cut from `frontier/m1-integ` at `d9d32ec`. **Contract:** M1-CONTRACT v1.1 §3.1, §3.5, §4, §5, §6, §10, §11 (W1-E row), §12 (Gate W1). **Date:** 2026-09-27.

## What landed

A new crate `frontier-abi/` (root workspace member, `#![no_std]`, no Solana types, depends only on `permutation-rules`), plus its vectors.

| Module | Contract | Content |
|---|---|---|
| `bytes` | §5.3 | bounds-checked LE readers/writers, `Cursor`, `Writer` (no panics, usable under the program's no-`unwrap` rule) |
| `layout/{mod,world,player,province,clash,beacon}` | §4.3, §5.2, §5.3 v1.1 | all 17 account kinds: `SIZE`, `MAGIC`, `CHAINED`, one offset constant per field, `FIELDS` tables that tile `[0, SIZE)` exactly (reserved ranges included), const range assertions, `AccountKind`, rent, sub-records (transit, entry, site mirror, arrival record, archive entry, cohort, camp, accrual, queue item, explore, resolve summary, holding ref, ticket site) |
| `addr` | §4.1, I-01, I-02 | seed grammar (SP-V2 byte for byte), `with_seed`, one seed builder per kind (+ reserved `sv`, `po`), `citizen_tag15`, `keeper_tag8`, `citizen_tag`, `join_shard_of`, host id and its inverse, `AddrCtx` (full addresses from the Season PDA and program id) |
| `tags` | §5.5 | `Ix` (50 tags), keeper class, top-level rule, reserved tags, relay shape predicates |
| `ix` | §5.5–§5.12 | data encodings for all 50 instructions (fixed width, LE, tag byte first; FileTicket, ArchiveAnchors and CreateSeason variable) with `LEN`/`encode`/`to_bytes`/`decode` |
| `error` | §5.4 | `FrontierError` codes 1–61 and 99, names, keeper action map (offchain P8) |
| `log` | §6 | PS2 kinds with pinned key/payload widths (`SPECS`), `write_body`/`write_tail`/`decode`, `next_head`/`advance`, `chains_of` (expected chained entities, derivable addresses), fates packing, outcome/seal/bond/divert code tables, CLOSE key |
| `budgets` | §5.5, §10.1, §10.2, I-45, I-50 | CU budget and (placeholder) limit per tag, tx ceilings, `L(kind)` from the worst account sets, legacy tx-size estimator, lock counts, `MULTI_MAX_REGIONS` |
| `presets` | §5.7 | `SeasonParams` (224-B layout), validation, `params_hash`, `QUICKNET_*`, `M1_LOCAL_7D`, `M1_PLAYTEST`/`m1_playtest(gate)` |
| `prologue` | §5.6–§5.12, I-55 | the account list of every instruction (groups with repeat counts, signer/writable, account kind), `check_flags`, `check_top_level`, absence/presence/key checks, `check_season`, `player_prologue`, `debit_bucket`, `check_holding`, cohort closure and the lazy finality flip, `host_in_transit`, `resident_ok`, `keeper_prologue` |
| `entry` | §5.3 Entry, §4.1 host id, I-55 | Province entry ↔ kernel `Host` codec (all pending ops incl. program-level Leave/Forfeit), `read_entry`/`write_entry`/`find_entry` |
| `src/bin/abi-vectors.rs` | §3.5 | the only producer of `frontier-abi/vectors/*.json`; `--check` exits 1 when a file is stale |
| `vectors/*.json` | §3.3, §4.1 | `layouts`, `tags` (incl. account lists), `errors`, `logs` (one encoded vector per kind with chain heads), `budgets`, `addresses` (full addresses for a fixed season PDA and program id, hex and base58), `presets`, `ix` (one sample per instruction), `entries` |

Tests: 33 unit tests, `tests/addresses.rs` (20,000 seeded inputs against a verbatim copy of SP-V2 `acct.rs`'s seed builders for `an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa`), `tests/contract.rs` (63 offsets transcribed from the §5.3 text, vector freshness through the built binary, 20,000 seeded arbitrary inputs through every decoder with no panic). The vectors were also cross-checked with an independent Python recomputation of `sha256(season ‖ seed ‖ program)` and base58 (36/36 addresses match).

## I-37: root-workspace membership (decided with evidence; integrator to confirm and W1-D to record in DECISIONS)

**Recommendation: `frontier-abi` and `permutation-frontier` are root workspace members** (no separate program workspace). Evidence [measured, 2026-09-27, scratch copy of the root workspace in `scratchpad/w1e-i37/`, nothing in the repo touched]:

- A stub `permutation-frontier` with SP-V2's dependency set (`solana-program =4.0.0`, `ark-ff/ark-ec/ark-bls12-381 =0.5.0`, `borsh =1.6.1`, `permutation-rules`) added as a root member resolves with the existing lock: **20 packages added, 0 existing entries changed or removed**; afterwards `cargo metadata --locked` succeeds.
- `cargo check --locked` of the stub and of `permutation-chain` pass on 1.89.0.
- `cargo-build-sbf 3.1.9 --tools-version v1.52 --arch v2 -- --locked` of the stub, with `frontier-abi` linked and its address, entry, log, ix and preset code called, builds (ELF `e_flags = 2`); no edition-2024 crate forced `RUSTUP_TOOLCHAIN=1.95.0`.

## Measurements and derived numbers

- **Transaction sizes** (legacy message, 64-B signatures, blockhash, the three ComputeBudget instructions SetComputeUnitLimit/Price/LoadedAccountsDataSizeLimit) [model, exact byte count]: Depart 712 B (2 signers), Reveal 829–928 B (0–3 path provinces), SettleTransit 858 B, PostAnchor 780 B, Join 631 B, ResolveFromInputs **465 B**, Harvest 427 B.
- **`MULTI_MAX_REGIONS = 7`**: PostAnchorMulti with 7 regions is 1,177 B; 8 regions is 1,243 B > 1,232.
- **GatherClash**: one part holds at most 22 slot + holding keys (30 accounts); 23 does not fit.
- **FoldOccupancy part 1** (payer, season, Frontier, 24 JoinShards, 6 ProvinceFunds = 33 accounts) is **1,288 B**: it does not fit a legacy transaction (see deviations).
- **Loaded data** at the placeholder programdata (SP-V2 `program-kprobe` .so 540,608 B [measured in SP-V2] → `max_len` 675,840): every worst set needs ≤ 1 MiB (largest SkipQuiet 976,913 B with 24 archive reads, then PostAnchorMulti 766,883 B, GatherClash 716,821 B, Reveal 712,426 B). `L(kind)` requested = max(need, 1 MiB working default).
- **Quicknet**: `QUICKNET_PK_HASH = sha256(96-B compressed pk) = 96e74fcd…19cee134` (pk from SP-V2 `quicknet-info.json`).

## Deviations and contract findings (for the integrator / amendments)

1. **`po` seed (reserved, M3):** the §4.1 table says `P, Q, bell, pos u8, 0 u8` (14 raw B, 30-B seed); SP-V2 `acct.rs` uses 13 raw bytes (28-B seed), and §4.1 also says the vectors MUST match SP-V2 byte for byte for `po`. I followed SP-V2 (13 B). W1-C's `addr-vectors-v1.json` must make the same choice; the integrator should amend one of the two sentences.
2. **Hints are 290 B**, not "≈ 145 B": two SSWU hints of 145 B (SP-V2 `quick::HINT_LEN`). PostAnchor data is 384 B, PostSeed 385 B, ConsumeGenesisSeed 347 B.
3. **§5.5 "Tx B max" is below the real minimum for 23 instructions** (it seems to omit the 64-B signatures, blockhash and ComputeBudget instructions). `budgets::tx_ceiling` keeps the contract value in `tx_contract` and gates on `max(contract, worst estimate rounded up to 8 B)` capped at 1,232: AbortSeason 300→336, SetWindowSchedule 200→272, CloseSeedCache 300→376, CloseProvince 300→336, SetSession 300→440, SetVigil 250→400, FileTicket 560→576, CloseHolding 330→368, CloseCitizen 300→368, Harvest 320→432, Build 360→464, Train 330→432, Muster/Dissolve/Garrison/Explore 380→472, DisbandStranded 300→336, SweepPoolOwed 300→336, **ResolveFromInputs 460→472**, CloseClashInputs/CloseArrivalDay/CloseArrivalSlot 300→376, FoldOccupancy 1,200→1,232.
4. **FoldOccupancy part 1 does not fit a legacy transaction** (1,288 B). Needs an amendment before W3-A: e.g. three parts (16 JoinShards each, the 6 ProvinceFunds in the last) or address lookup tables. The account table here keeps the contract's two-part list.
5. **`MULTI_MAX_REGIONS` is 7**, not "8 or 16" (item above); W2-A confirms with its tx-size test.
6. **RING_SEED chains nothing**: §6 lists "Frontier", but ConsumeRingSeed writes only the short-header RingSeed, so it cannot advance the Frontier's head.
7. **SEASON_CREATED's body is 138 B**, over §6's 128-B soft ceiling (third exception with DEPART and CLASH); the fields are the contract's.
8. **The Concord (0, 0) has no wedge** (`ProvinceCoord::wedge() == None`), so OpenProvince's `pf(w)` is undefined for ring 0 (W3-A / amendment: e.g. fund ring 0 from wedge 0).
9. **Tail order pinned** (§6 left it open): ascending `entity_kind`, then instruction account order. `chains_of` returns `EntityRef::InTx` / citizen references (`Tag8`, `OfHolding`) where key and payload do not determine the entity (DISSOLVE's and STRANDED's province, DEPARTURE_SETTLED/TRANSIT_SETTLED provinces and inputs, SETTLE's other ticket provinces).
10. **Log payload widths pinned** where §6 lists names only: SETTLE/RELEASE carry the u64 `citizen_tag`; TRANSIT_SETTLED payments are 4 × `{recipient first 8 B, amount u64}` (tip, fee, bond, reward); TRANSIT_SETTLED outcomes 1–5 = the fate codes, 6 bounced-unranked, 7 routed, 8 bad-seal; EXPLORE adds `n u8`; FOLD always carries both parts' fields (zeros on part 0); CLOSE key = account kind ‖ raw key padded to 15 B; DIVERT reasons 1–6.
11. **`SeasonParams` layout pinned** (224 B); `params_hash = sha256("PSF-PARAMS-v1" ‖ SeasonParams ‖ PayoutParams borsh)`; validation adds M1 constants the bell model and network assume (`bell_secs == 600`, `postures_enabled == 0`, `network == 2`, `drand_period ≥ 1`, bps ≤ 10,000, `release_after ≥ dormant_after`, non-zero pk hash). `SeasonParams` carries the DefencePool caps (they live in the DefencePool, not the Season).
12. **Troop units:** instruction data (`Train.n`, `Muster.troops`, `Garrison.delta`) is whole troops (the unit of `Holding.reserve`, whose u32 cannot hold MilliTroops at scale); account state (entries, arrival records, garrisons) is the kernel's MilliTroops.
13. **`check_flags` refuses a program account passed writable in a read-only position** (keeps the §8.8 lock set honest); stricter than the contract text.
14. **Entry codec:** `Host.owner` = the host id with its sequence cleared (the issuing holding and generation); Split (no M1 instruction) packs `of` over `op_a`/`op_b` and the top bits of `op_troops`; Leave/Forfeit are program-level (`to_host` gives `pending = None`, `busy()` is true).
15. **Presets:** `M1_LOCAL_7D` g = 3, `r_max` 16, `pfund_initial` 18 SOL (all provinces within ring 16), `dpool_initial` 20 SOL, `per_bell_region_cap` 0.2 SOL and `per_keeper_day_cap` 2 SOL [placeholders until CL-30]; `reveal_cu_limit` 26,000 and `reveal_loaded_limit` 1 MiB [placeholders until W3-B / the release `.so`]. `M1_PLAYTEST` = the 7-day preset with a join gate that fails closed (`[0xFF; 32]`, not a signable key) until `m1_playtest(relay_gate_key)` is used.
16. **Vectors:** u64 values that can exceed 2^53 (host ids, lamports in presets) are decimal strings.

## Dependency requests (integrator-owned files changed on this branch to build)

- Root `Cargo.toml`: `members += "frontier-abi"` (and, per I-37 above, `"permutation-frontier"` when W2-A lands; `[profile.release.package.permutation-frontier] overflow-checks = true` per §3.1).
- Root `Cargo.lock`: one new path package `frontier-abi` (depends on `permutation-rules`); **no new external crate**.
- `frontier-abi/Cargo.toml` `[dependencies]`: `permutation-rules = { path = "../permutation-rules" }` (new crate; the section is integrator-owned).

## Interfaces with the other wave-1 units (pending the merge)

- **W1-C `addr`:** `frontier_abi::addr` implements the pinned grammar itself because W1-C's `permutation_rules::frontier::addr` does not exist at the wave base. After W1-C merges, the integrator should either re-export the kernel builders from `frontier_abi::addr` or add an equality test over the seeded inputs of `tests/addresses.rs` (same seeds, same extremes).
- **W1-C `fees::loaded_limit`:** `budgets::loaded_need`/`loaded_limit_for` implement the same `round_up(pd + 45 + Σ(data + 64), 32 KiB)`; after W1-C merges, call the kernel function or add an equality test.
- **W1-C `RULESET_HASH` input function:** §3.2 says `frontier-abi`'s build step generates `RULESET_HASH`; the input function is W1-C's, so the generator is **pending** (integration window or W2-A).
- **W1-B `PayoutParams`:** `presets.json` encodes `PayoutParams::REV3` field by field (borsh of plain integers); if CL-11 adds fields, the vector writer must add them.
- **W5-A:** regenerates `budgets.rs` CU limits and `L(kind)` from the release `.so`; `frontier-abi/src/budgets.rs` and `vectors/budgets.json` are handed over then (§11).

## What I ran (all exit 0 unless stated)

```
cargo fmt --all -- --check
cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings
cargo test --locked -p frontier-abi                      # 33 unit + 2 + 3 integration tests
cargo run --locked -p frontier-abi --bin abi-vectors -- --check   # 9 files fresh
cargo test --locked -p permutation-chain
cargo test --locked --release -p permutation-rules
git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src
```
Plus the I-37 scratch probe above (`cargo metadata --locked`, host `cargo check`, `cargo-build-sbf --arch v2`), outside the repo.

Not run (not W1-E files, or not present on this branch): the frontier-sim, frontier-node, gateway `npm test` and civilization items of Gate W1. No O-M1-12 item is needed by this unit (nothing downloaded or installed; the SBF build used the installed 3.1.9 tools). No service was started, no port bound, no chain transaction, no push.
