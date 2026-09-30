# M1 close, U4: fund the keeper beneficiaries so ClaimDefence lands

Branch `frontier/m1-close-U4` from `frontier/m1-integ` (18e599e), 2026-10-01. Closes open item **U4** (`DECISIONS.md` part U; `M1-EXIT-NOTES.md` §4.2) with the first of the two fixes named there: **the stack funds each beneficiary**. Keeper code, its fee payer and the program are unchanged, so under contract §11 (W7 rows) this is a stack/config fix and needs no new exit season.

**Rules followed:** no file under `permutation-frontier/src`, `frontier-abi/src`, `permutation-rules/src` or `frontier-node/crates/keeper/src` changed (`git diff --stat frontier/m1-integ` lists only `frontier-node/crates/stack/**`, `frontier-node/configs/**` and `docs/**`). Local commit only, no push. The run used ports 41600–41699 and was stopped (`down`). The paused `m1-exit` stack on 41000–41099 was not touched. No devnet or mainnet transaction, no install: the gateway's `node_modules` was an APFS clone of `m1-integ`'s copy, which has the same `package-lock.json`.

## 1. The fix (stack side only)

| file | change |
|---|---|
| `frontier-node/crates/stack/src/config.rs` | new key `pools.beneficiary_lamports`, default `BENEFICIARY_LAMPORTS` = 1 SOL: 50 × the keeper's per-write spend cap of 20,000,000 lamports. Written to `state.json` under `config.pools`. Negative values are refused and 0 turns the airdrop off. Tests: `beneficiary_funding_key`, and `every_committed_config_parses_and_keeps_the_port_rule` now requires every committed config to fund at least one write's cap |
| `frontier-node/crates/stack/src/up.rs` | `fund()` airdrops `beneficiary_airdrops(cfg, a, b)`: keeper A's beneficiary, and keeper B's when keeper B runs. Start order step 4 in the module doc now says so. Test: `beneficiaries_are_funded` |
| `frontier-node/configs/nightly.toml`, `m1-exit.toml` | `[pools] beneficiary_lamports = 1000000000`, written out with the reason. The other configs take the default |
| `frontier-node/crates/stack/src/report.rs` | a new report section **"ClaimDefence (M1 exit U4)"** (`report.json` `defence_claims`). It lists each landed claim with its keeper, slot, bell, day, slots, the `DEFENCE_CLAIM` record's refund `amount` and `partial`, the fee, the beneficiary's balance after the claim and the signature, then gives the failed count. So a nightly now shows directly whether a claim landed. The claim's extra versions, refused `NotEligible` once the first version has set `claimed = 1`, are classed `redundancy / claim versions` (reported only; they do not gate anything). Tests: `defence_claims_are_listed_with_their_refund` and an added assertion in `failed_tx_classes_follow_the_triage` |
| `docs/frontier/m1/RUN-A-KEEPER.md` | §1 and §4.4: the beneficiary is ClaimDefence's fee payer and must hold lamports; the funders never pay for it; the stack airdrops `pools.beneficiary_lamports` |

`cargo test --locked --release -p stack --lib`: 70 passed, 0 failed.

## 2. The run: `u4c-nightly-2`

`scripts/m1-nightly.sh --no-build --run-id u4c-nightly-2 --base-port 41600` (nightly config: test key, 100 bots, 1 game day at 100×, adversary on). The binaries are this branch's release build. The `.so` is the test-beacon build from this tree; its sha256 `b2cef4a7…a4e7` is identical to `m1-integ`'s. The first attempt, `u4c-nightly-1`, stopped at `up` before anything started because `permutation-gateway/node_modules` was missing in the new worktree. Records: `runs/u4c-nightly-2/` (`nightly.json`; `report.md` from the run itself; `rereport.md` from the final commit's binary, which differs only in how the 4 `NotEligible` rows are classed).

| step | result |
|---|---|
| funded | 524 airdrops (522 before this fix + the 2 beneficiaries). Balances at slot 974: A 999,830,482, B 1,000,029,336 |
| up | exit 0; 1,100 s; phase complete |
| verify | **PASS**, 8,651 txs |
| tamper | 30/30 |
| load (post-play, 5,000 viewers) | **fail**: file p99 327.7 ms > 250 ms; errors 0; WS coverage 100 %; ingest → WS p99 0.75 s. This is the post-play measurement that `m1x-nightlies.md` already records as noisy on the shared machine (7–524 ms over five nights, and red on `m1x-nightly-2`). It is not criterion-6 evidence, and nothing it measures depends on U4 |
| report | exit 0; criteria 1, 2, 4, 8, 9 pass; 3, 5, 6, 7 n.a.; E not exit-grade (test key, no `.so` pin, `frontier-fund` hold-skipped as in every one-day night) |
| down | exit 0; ports 416xx free afterwards |

### 2.1 The hold fired and the claim landed

- `slots-above` (above cap, 3,000 milli) held province (2,1) bell 22 from slot 451 to 488. Keeper A's Reveal there landed at slot 489, late by on-chain evidence.
- **`defence-pool` fired** (`open_claims: 1`, `grace_end_min` 1785649200) and held the DefencePool from **slot 491 to 535** (45 slots, above cap). Held-key writes: 0 inside the window, 2 after it, the first at slot 536.
- Keeper A sent the claim `claim:0:2,1,22,0,0` in five versions (first valid at slots 492, 494, 496, 512 and 528; bids 100 → 500 milli; payer = the beneficiary `9jVq5EKxLxvkrWSJ1HqU2pFU2ARgDYNF43F4LtgXzdCk`). All five reached the chain in **slot 536**, the first slot after the hold.

**ClaimDefence landed:**

| | |
|---|---|
| signature | `hcywzT89UTZpr3cUrDqZgnYtszdohY6MpgKkKSAvcWXjtxiHf4YuzjXhT8LKurts2feLXySSg9kJsdL9qrRYXuD` |
| slot / bell | **536** / 27 (day 0) |
| keeper | A (beneficiary `9jVq5EKx…zdCk`, signer and fee payer) |
| slots claimed | 1: ArrivalSlot (2,1) bell 22, faction 0, i 0 |
| **refund** (`DEFENCE_CLAIM.amount`) | **42,132 lamports**, `partial` 0 |
| fee | 16,347 lamports; 12,537 CU (budget 25,500) |
| beneficiary balance | 1,000,319,346 → 999,044,651 (= −16,347 fee − 1,300,480 DefenceClaim rent + 42,132 refund; the rent comes back when the claim account closes) |

The four sibling versions in the same slot were refused `NotEligible` (43), because the landed one had already set `claimed = 1`. Keeper A's journal records them as `failed 43` and the landed one as `landed`. Before this fix, `m1-exit` showed 64 versions expired and 0 landed.

## 3. Observation, not fixed (keeper behaviour; a change would need a new exit season)

The DefencePool hold kept every claim version waiting, and all five then landed in one slot. The four refused versions still paid fees: 16,347 + 16,347 + 13,578 + 5,271 = 51,543 lamports, which is more than the 42,132 refund. In total this claim cost its beneficiary 67,890 lamports in fees against a 42,132-lamport refund. The loss comes from the hold letting several versions pile up and land together. Without a hold, the first version usually lands before the next one is sent (`d_resend_slots` = 2). This is recorded for the playtest decision. It belongs to the keeper (the class-D resend ladder and the `per_write_cap`), and under §11 it cannot change without a new exit season.

## 4. What stays open

- The integrator should set `DECISIONS.md` U4 to closed (stack fix, this record) and `M1-EXIT-NOTES.md` §4.2/§8 item 2 to done. This unit left both files unedited, so they do not conflict with U3's edits to the same table.
- §3's observation, for the owner.
