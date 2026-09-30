# W6T-1 — W6-B program-fix: the Depart arrival bound and the close budgets

Fix unit U1 of the w6-s7 triage (`scratchpad/frontier/m1/triage/PLAN.md` §1 U1). Branch `frontier/m1-w6t-U1`, cut from `frontier/m1-integ` = `codex/frontier` `7dcacdf`; worktree `.claude/worktrees/m1-w6t-U1`. Local commits only, nothing pushed.

- **Owns:** `permutation-frontier/**`, `frontier-abi/**`, this file. `permutation-rules/**` untouched (kernel, `RULESET_HASH` and the Phase B digests unchanged).
- **Logs:** `(session scratch)/scratchpad/u1/logs/` — `before.log` (host test on the 7dcacdf builds), `before-close.log` (close tests on the 7dcacdf builds), `after.log` (the three new tests, 168 close rows), `svm-full.log` / `svm-full2.log` (full suite, `RELEASE_CHECK=1`; run 2 after the `MEASURED` update), `cu.log` / `cu2.log` (their `PSF_CU_LOG`), `build-twice.log`, `build-features.log`, `criterion.txt`, `doctrine-gate.txt`, `sim-gates.log`, `node-tests.log`, `close-table.md`, `sim.py` (the read-only live check of §2.1).

## 1. Depart: the arrival must be a bell of the season

**Problem (w6-s7 criterion 1, V5 MissingData ×9 at bells 1008–1011, `end_bell` 1008).** §5.11 Depart step 4 bounded `arrive_bell` only by `[b + 2, b + 72]`. A march arriving at or after `end_bell` landed, but no anchor of such a bell can be posted (PostAnchor refuses `bell >= end_bell` with `BadData`), no resolve passes it and SettleTransit can never run: the transit is stranded for good.

**Fix** (`permutation-frontier/src/proc/host.rs` `depart()`, step 4):

```rust
if x.arrive_bell < lo || x.arrive_bell > hi || x.arrive_bell >= pl.pc.season.end_bell {
    return Err(FrontierError::ArrivalBell.into());
}
```

The existing `ArrivalBell` code; no ABI, layout, account-list or vector change. The last useful Depart bell is `end_bell − 3` (arrival `b + 2 = end_bell − 1`). Depart G1: 23,167 → **23,174 CU** (+7; gate 24,500, `cu_limit` unchanged at 24,500).

**Test (failing first)** `svm-tests/tests/host.rs::host_depart_arrival_at_or_after_end_bell_refused`, ported from the triage repro `triage_depart_arrival_at_or_after_end_bell` (test-beacon build, `end_bell` crafted, the Season still Running at B0 = 10):

| case | before (7dcacdf `.so`) | after |
|---|---|---|
| arrive = B0 + 6 = `end_bell` | **lands** (`expected ArrivalBell (31), landed`, host.rs:734) | `ArrivalBell` |
| arrive = B0 + 6 > `end_bell` = B0 + 5 | lands | `ArrivalBell` |
| arrive = `end_bell − 1` | lands | lands; PostAnchor of the arrival bell lands |
| Depart at `end_bell − 3`, arrive `b + 2` | lands | lands |
| Depart at `end_bell − 2`, arrive `b + 2` (= `end_bell`) | lands | `ArrivalBell` |

**G13:** new row in `svm-tests/src/cover/host.rs` `DEPART`: `host::host_depart_arrival_at_or_after_end_bell_refused` → `Err(ArrivalBell)`, `Lands("w.depart_ix(")`. `g13_coverage_*` green with `RELEASE_CHECK=1` (0 Pending).

## 2. The close budgets (CloseArrivalDay 0x65, CloseArrivalSlot 0x66)

### 2.1 Why they ran out of CUs [measured, live and svm]

w6-s7 lost 28,655 transactions to these two closes in the drain (CloseArrivalDay 25,655 at `cu_limit` 6,000; CloseArrivalSlot 3,000 at 6,500; 0 landed). A read-only look at the paused w6-s7 localnet (`getTransaction` of keeper-a's first failed close, `3Uxg9xF8M6j3…`, `close:day:-1,-3:6`, bell 1009): three ComputeBudget instructions, then `Program GS8UL… consumed 5550 of 5550 compute units … exceeded CUs meter` after the CLOSE record was emitted — the program had 6,000 − 3 × 150 = 5,550 CU.

Two fixed costs were not in the G1 table (`budgets::MEASURED`, the maximum of svm `send`s, which carry two ComputeBudget instructions and pay the rent back to the keeper itself):

1. **the keeper's third ComputeBudget instruction** (SetComputeUnitPrice: the keeper always bids a priority fee), 150 CU;
2. **a `rent_to` distinct from the fee payer** (in the live season the Revealer that paid the ArrivalDay/ArrivalSlot rent), one more account key: +192 CU (+191 for the slot).

With both, the svm measurement reproduces the live figures **exactly**: CloseArrivalDay Running, day resolved = **5,979** CU (w6-s7: "landed at 5,979 of 6,000"), CloseArrivalSlot Running, past the grace = **6,496** (w6-s7: 6,496 of 6,500). The Ended path adds 39–42 CU (the season-end fallback check) and passes the limits: **6,018 > 6,000** and **6,538 > 6,500**.

### 2.2 Tests (failing first)

`svm-tests/tests/clash.rs`: `g01_close_arrival_day_ended_paths`, `g01_close_arrival_slot_ended_paths`. Each path is sent with the keeper's prefix (SetComputeUnitLimit, SetComputeUnitPrice, SetLoadedAccountsDataSizeLimit at `L(kind)`) at the ladder top (the CU is recorded) and again at the budgets table's `cu_limit`: both must give the same outcome — a landing, or the refusal's own code (`TooEarly`), never CU exhaustion — and the CU must stay within the §5.5 gate and ≤ 95 % of it. Paths: Running, Ended + 0 h and + 71 h (the close's own rule, the Province read), Ended ≥ 72 h and the Closed tombstone (the Province not read: present, closed, or pre-funded with lamports only); CloseArrivalSlot also claimed, past the grace by THE anchor (read), anchor archived, within the grace and unsettled. `rent_to`: the payer itself, a funded distinct account, a never-created key. Release and test-beacon builds (identical CU in all 168 rows).

- **before** (7dcacdf `.so`, `cu_limit` 6,000 / 6,500): both fail — `Release Ended +0h, day resolved, rent_to funded at the table's cu_limit 6000: expected to land, failed with InstructionError(3, ProgramFailedToComplete)`, and the same for the slot at 6,500 (`before-close.log`). Committed first (`e056b98`).
- **after**: both pass.

### 2.3 The close CU table [measured, release = test-beacon]

Transaction CU with the keeper's three-instruction prefix (program CU = tx − 450).

**CloseArrivalDay**

| path | outcome | rent_to the payer | rent_to funded | rent_to never created |
|---|---|---:|---:|---:|
| Running, day open | TooEarly | 4,520 | 4,712 | 4,712 |
| Running, day resolved | lands | 5,787 | 5,979 | 5,979 |
| Ended +0h, day open | TooEarly | 4,559 | 4,751 | 4,751 |
| Ended +0h, day resolved | lands | 5,826 | 6,018 | 6,018 |
| Ended +71h, day open | TooEarly | 4,559 | 4,751 | 4,751 |
| Ended +71h, day resolved | lands | 5,826 | 6,018 | 6,018 |
| Ended +72h, day open, Province Present | lands | 5,675 | 5,867 | 5,867 |
| Ended +72h, day open, Province Absent | lands | 5,675 | 5,867 | 5,867 |
| Ended +72h, day open, Province PreFunded | lands | 5,675 | 5,867 | 5,867 |
| tombstone, Province Absent | lands | 5,453 | 5,645 | 5,645 |
| tombstone, Province PreFunded | lands | 5,453 | 5,645 | 5,645 |

**CloseArrivalSlot**

| path | outcome | rent_to the payer | rent_to funded | rent_to never created |
|---|---|---:|---:|---:|
| Running, unsettled | TooEarly | 4,304 | 4,495 | 4,495 |
| Running, settled within the grace | TooEarly | 5,033 | 5,224 | 5,224 |
| Running, claimed | lands | 5,200 | 5,392 | 5,392 |
| Running, past the grace (anchor read) | lands | 6,305 | 6,496 | 6,496 |
| Running, anchor archived | lands | 6,111 | 6,302 | 6,302 |
| Ended +0h, unsettled | TooEarly | 4,346 | 4,537 | 4,537 |
| Ended +0h, claimed | lands | 5,242 | 5,434 | 5,434 |
| Ended +0h, past the grace (anchor read) | lands | 6,347 | 6,538 | 6,538 |
| Ended +0h, anchor archived | lands | 6,153 | 6,344 | 6,344 |
| Ended +71h, unsettled | TooEarly | 4,346 | 4,537 | 4,537 |
| Ended +71h, claimed | lands | 5,242 | 5,434 | 5,434 |
| Ended +71h, past the grace (anchor read) | lands | 6,347 | 6,538 | 6,538 |
| Ended +71h, anchor archived | lands | 6,153 | 6,344 | 6,344 |
| Ended +72h, unsettled, anchor passed | lands | 5,612 | 5,803 | 5,803 |
| Ended +72h, unsettled, no anchor | lands | 5,239 | 5,431 | 5,431 |
| tombstone, anchor passed (gone) | lands | 5,386 | 5,577 | 5,577 |
| tombstone, no anchor | lands | 5,013 | 5,205 | 5,205 |

168 rows; release vs test-beacon differences: 0

**Worst case:** CloseArrivalDay **6,018** (Ended < 72 h, day resolved, distinct `rent_to`); CloseArrivalSlot **6,538** (Ended < 72 h, past the grace, THE anchor read, distinct `rent_to`). Both ≤ 7,600 (8,000 × 0.95), so no program path needed a fix; the refusals cost at most 5,224.

### 2.4 The budgets change

- `frontier-abi/src/budgets.rs`: new `pub const fn limit_at_gate(ix)` = CloseArrivalDay | CloseArrivalSlot; the `budgets!` table gives those kinds `cu_limit = cu_budget` = the §5.5 gate **8,000** (18.3 % / 24.8 % over the worst case; within DECISIONS N12's "never more than the gate + 5 %"). Every other kind keeps G1 max + 5 %.
- `MEASURED` updated to this branch's G1 maxima: Depart 23,167 → 23,174, CloseArrivalDay 5,637 → 6,018, CloseArrivalSlot 6,155 → 6,538 (the close maxima are the keeper-prefix rows; `cu-table.py cu2.log --check` exit 0).
- The budgets unit test pins `cu_limit` 8,000 for both and, for `limit_at_gate` kinds, `cu_limit == cu_budget` and `MEASURED ≤ 95 %` of the gate instead of the +5 % rule.
- `vectors/budgets.json`: rows 101 and 102 `cu_limit` 6,000 → 8,000, 6,500 → 8,000; nothing else (schema unchanged). `abi-vectors --check`: **9 files fresh**. The keeper, bots and relay embed this file (`fclient::budgets::CANONICAL_JSON`, `bots::txb`), so they pick it up on rebuild.

## 3. Builds

`scripts/build-frontier.sh --twice` (load 10.5):

| build | sha256 | bytes |
|---|---|---:|
| release (deployable, `--twice` identical) | **`d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`** | 875,824 |
| release program_hash (trailing zeros stripped) | `8f93e44efc67fde87c1fb65210ee322424352c7e2d5497d92985b171cde0d91c` | |
| test-beacon | `b2cef4a7502a14cdb002dac984668422ad7bddf740df69a025979ffc8ddda4e7` | 876,328 |
| trace | `22f5c03b5f9f9a22a0b693be750b977ae7cf137de21f033cfbdcd962de715abd` | 887,472 |
| oracle | `500a0794b0463de1b7016eaae1c50d1714fda8a7b4447cbf898d3b5e6ba63676` | 880,888 |

Was: release `072b1205…a98b` 875,768 B, test-beacon `797675b4…16f7` 876,272 B. +56 B; `max_len` 1,097,728 and ProgramData 1,097,773 unchanged (same 4-KiB page); `g01_loaded_limit_table_covers_the_release_so` passes (8,912 B left under `PLACEHOLDER_SO_LEN` 884,736). e_flags 2, overflow panics present, no test markers.

`RULESET_HASH` = `72c6b583…54bd9` unchanged (`presets::tests::ruleset_hash_is_the_kernels`); `permutation-rules` `m1_rules_keep_the_phase_b_digests` and `occupancy_empty_keeps_the_phase_b_digests` pass.

## 4. Verification

| command | result |
|---|---|
| svm-tests, 7dcacdf builds (`PSF_SKIP_BUILD=1`, the integ `.so`s): the three new tests | **fail** (host: Depart lands; closes: CU exhaustion at 6,000 / 6,500) |
| `RELEASE_CHECK=1 PSF_CU_LOG=… ./run.sh --release --no-fail-fast` on the new builds (twice: load 9.0 and 6.9) | exit 0: **248 passed** (incl. the 3 new tests), 0 failed, 4 ignored (`drill_validator_loaded_data`, `zz_profile_{gather,rfi,skip}`) |
| G1 lines: `g01_budget_w3b_host` (Depart 23,174 CU at entry 55, full queue), `g01_budget_clash_kinds` (closes), `g01_close_arrival_*_ended_paths` | green |
| `cu-table.py cu2.log --check` | exit 0 (after the `MEASURED` update) |
| `cargo test -p frontier-abi`; `abi-vectors --check` | 42 + 5 passed; 9 files fresh |
| `cargo test -p permutation-frontier --no-default-features` | 37 passed |
| `cargo fmt --check` (root, svm-tests); `cargo clippy --all-targets -D warnings` (frontier-abi, permutation-frontier, svm-tests) | clean |
| `scripts/build-wasm.sh --check` | fresh (`frontier.wasm` unchanged) |
| frontier-node `cargo test -p fclient -p bots -p keeper` (embed the new budgets.json; load 25) | exit 0 |

## 5. Outcome gate (the fix removes a player option) [sim, measured]

`frontier-sim` depends on `permutation-rules` only, so the expected delta is none. Same commands as W6-B §2.4, compared line by line with W6-B's Phase B outputs (`scratchpad/w6b/logs/`):

- `criterion --best-response --seeds 3 --first-seed 30001 --gate`: **exit 0**, worst bot choice **0.985** (bots in office, 1 %, days 1–7, stake); every table cell **identical** to W6-B Phase B (only the `time` lines differ). 32 s, load 8.8 → 15.7.
- `doctrine-gate --controls`: **exit 0**; kernel table 4/6 in band, gap 3.1, largest |Δ index| **0.060 %**; controls rejected as they must be: draft, Knight **−0.242 %**, A boost **+0.326 %**; 360/360 conserve. **Identical** to W6-B Phase B except the wall-time figures. 6 min 44 s, load up to 68.7 (other sessions).

**Deltas against W6-B §2.4: none.** The stack-level bot criteria (5 and 7) are R3/R5's (the bots still plan marches into `end_bell` until U3 clamps `plan_march`; they now get `ArrivalBell` instead of a stranded transit).

## 6. Requests and cross-unit notes

1. **Integrator (gateway generated files, not owned here):** `node permutation-gateway/scripts/sync-web-sdk.mjs --check` now reports `client/src/frontier/abi-budgets.mjs` and `web/sdk/frontier/abi-budgets.mjs` out of date (they embed budgets.json). Run `node permutation-gateway/scripts/sync-web-sdk.mjs` on the merge (it does not touch `permutation-server/web/session.mjs`). No manifest or lockfile change.
2. **U2 (keeper):** with `cu_limit` 8,000 the closes land on the first attempt; the CU-exhaustion classification (`units == cu_limit` / "exceeded CUs meter") and not re-planning a dead key still matter for any other kind. Small-headroom kinds measured without the price instruction (limit − G1 max): EndSeason 202, SetVigil 296, CloseSeedCache 358, SetWindowSchedule 390, AbortSeason 433, SweepPoolOwed 434, CloseProvince 498 CU; the keeper's price instruction takes 150 of it and a distinct beneficiary about 190 more. None failed in w6-s7; worth a keeper-prefix G1 pass later.
3. **U3 (agents/verifier):** clamp `plan_march` to `arrive < end_bell` (and Depart bells ≤ `end_bell − 3`); the verifier invariant "no DEPART with `arrive_bell >= end_bell`" now holds by program rule.
4. **U5 (contract v1.12 §28, DECISIONS S):** §5.11 Depart step 4 adds `arrive_bell < end_bell` (`ArrivalBell`); §5.5/§10.2: CloseArrivalDay and CloseArrivalSlot request their gate 8,000 (measured worst 6,018 / 6,538 with the keeper prefix); §3.2 the new release sha `d85e1bd7…2281`.
5. **Setup deviation:** the computed setup line carried the unit title inside the worktree path and branch name; used `.claude/worktrees/m1-w6t-U1` and `frontier/m1-w6t-U1`, and this notes path (`W6T-1-NOTES.md`, the owned path) rather than `W6T-U1 …-NOTES.md`.
