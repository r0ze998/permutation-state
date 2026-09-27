# SP-FEE results: fee and write-lock drill redone, and the C4 decision

> **Corrections in M1 wave 1 (CL-27, I-45; 2026-09-27, unit W1-D).** Three labels and one parameter were wrong. Nothing was re-run; the measurements stand, only what they mean changes.
> 1. **"Clock" is confirmation lag, not clock drift.** The §2 row "mainnet Clock 1.3–1.7 s behind wall time" measured the time from a beacon to its confirmed transaction (confirmation lag). It is **not** Clock-vs-drand skew, which is still unmeasured; the seed margin Δ ≥ 60 s (DESIGN §8.5) does not rest on it.
> 2. **The lookup-table lock count is [unverified].** "64 (ALT)" / "60 per lookup-table stream" was never landed locally (§1, §6 item 8): only the legacy streams (≈ 20–26 known accounts) are measured. Every "60 (ALT)" figure in this report and in DESIGN §8.6/§8.7 is a limit from the lock rules, not a measurement.
> 3. **The loaded-data limit cannot be 64 KiB for the real program (I-45).** SIMD-0186 counts the invoked program's ProgramData toward `SetLoadedAccountsDataSizeLimit`. The drills used the 13,600-B `sfee_probe.so`; the SP-V2 program is 480,512 B (`plain-v2`) to 540,608 B (`program-kprobe`) [measured, M1 contract review]. With a 64-KiB limit every Frontier transaction would fail to load and still pay its fee. M1 sets a per-kind limit `L(kind) = round_up(programdata_len + 45 + Σ(account data + 64), 32 KiB)` from the release build (deployed with `--max-len = round_up(1.25 × .so, 4 KiB)`), 1 MiB until measured. The loaded-data cost term becomes `8 × ⌈L/32,768⌉` (+256 cost units at 1 MiB instead of +16), which moves every priority in this report by < 1% (a 26k Reveal: 14,441 lamports at priority 0.433 instead of 14,337). Finding 1 of §3 (set the limit, or a bid's priority halves) stands.

> **Corrections after review (m0c, 2026-09-27).** Seven review issues were checked and all confirmed. What changed:
> 1. **Headline.** **C4 NOT MET at the default tip [model].** The O7 pool at cap 2.0 passes on every valuation only when the side's reveals are paid from ≥ 150 rotating fee payers; that, the Reveal CU and the leader execution rate are unverified until drill (g)'s rule is in the keeper SDK, the M1 Reveal CU and the M4 soak. The first version read as a conditional pass; §1 below is the original text, kept for the record.
> 2. **Drill (g), new [measured]:** locking the keeper's **fee payer** (listed as a writable, non-signing key in the attacker's fillers) held a Reveal into an address the attacker did not know for 11.2 s at 0.0395 SOL per slot (one account's price, p × 40M); 20 known payers in one 21-key legacy stream were all held (0.042–0.045 SOL per slot); a **fresh payer landed in 0.89 s**; a keeper at 2.0 landed in 0.53 s (one payer) and 2.3 s (pool). Validator `solana-test-validator` 3.1.9 on ports 39970–40025, stopped, ledger deleted. Rows `g-*` in `results/drill-summary.md`, log `results/g.log`.
> 3. **Model v2** (`driver/c4_model.py` → `results/c4-model.txt`; v1 kept in `results/superseded/`): (a) payer-lock path `min(p × 40M × ⌈payers/60⌉, p × 100M)`; (b) **Reveal CU is an input** (16k / 20k / 26k: priority 0.433 / 0.35 / 0.27 at the 10,000-lamport tip), and the tip is stated in priority terms; (c) **capacity is a three-case range** (execution-bound [estimate: 55M CU/s per account, 150M CU/s per block], 100M per 0.4 s, 100M per 0.265 s unscaled) with **organic traffic above p subtracted** [measured]; the old "250M CU/s is conservative" claim is withdrawn; (d) **value from the simulator**: $7 × all participations (v1), $7 × clashes with the faction's own arrivals, and the counterfactual loss of claims from `frontier-sim c4` (50k wallets, 3 seeds, K3 economy).
> 4. **Mainnet priority sampling fixed** (`driver/mainnet_blocks.py`): v1 appended a price of 0 for every transaction, and `mainnet_ro.cjs` failed on v1 (SIMD-0385) transactions. Now priority = (fee − 2,500 × signatures) / `meta.costUnits` for legacy, v0 and v1 alike. **24 blocks [measured, read-only]:** mean non-vote cost 29.0M per block; share of non-vote cost bidding ≥ 0.433: **13.2%** (3.8M CU per block the attacker gets free), ≥ 1.0: 8.2%, ≥ 2.0: 2.5%. `results/mainnet-blocks-m0c.json`.
> 5. **The tail is one episode per seed.** In seeds 1–3 at 50k the p99 and max bells all lie in one run of ~30–40 bells at day 10.6–11.0, with ~200 clashes of the busiest faction of which only 2–29 involve its own arrivals [sim, `simprobe/c4_50k_s3.md`]. It coincides with the release of day-0 idle wallets' first holdings after 10 idle days [estimate]. The v1 value counted those defence-only clashes at $7 each.
> 6. **Anchor address.** Design impact 5's "keep unpredictable anchor addresses" and the D6 conclusion are **withdrawn**: SP-V2 showed an unpredictable address breaks Reveal's "anchor absent = window open" proof, and D6 measured the attacker's cold-start ramp in this harness, not priority ordering. With the predictable address, holding the anchor costs p × 40M per block and only delays (D1).
> 7. **Verdicts on model v2** (600 s, PASS ≥ 10×): default tip, v1 valuation: fail (1.0–9.6×; 10.2× only at the p99 bell with unpredictable payers and unscaled limits); default tip, simulator valuation (envelope $125–164 per bell): pass except a known payer with a 26k Reveal on the execution-bound case (8.3×); **cap 2.0 with ≥ 150 rotating payers: pass on every valuation and capacity case (12–540×)**; cap 2.0 with one known payer, v1 valuation: 4.5–20× (depends on capacity).

- **Date:** 2026-09-27
- **Milestone:** M0, second pass. This is exit criterion 3, C4, restated under owner decision O7.
- **Restated C4:** excluding every reveal bid at the default tip for one bell must cost at least 10× the value of all clashes in that bell for the targeted side.
- **Where the work is:** `scratchpad/frontier/m0b/spikes/SP-FEE/`: `driver/`, `program/`, `results/` and `simprobe/`.
- **Nothing in the repo was changed.**
  - The probe program is the M0 `sfee-probe` binary, unchanged (sha256 `32be9f03…`).
  - The simulator probe is a patched copy of `frontier-sim` at `cea89be`, built in scratch. Its seed-1 10k digest, `c71a8a0eb7641b53`, is unchanged.
- **Chain use:**
  - Local `solana-test-validator` 3.1.9: RPC 39899, pubsub 39900, gossip 39901, dynamic 39902–39940, faucet 39950.
  - It has been stopped, and its 5.6 GB ledger deleted.
  - Mainnet was read only. No devnet or mainnet transactions were sent.

**Tags**

| Tag | Meaning |
|---|---|
| [measured] | measured by us, on the local validator or through read-only mainnet RPC |
| [sim] | output of the probed rules-v10 simulator (50k wallets, seed 1) |
| [model] | arithmetic on measured and assumed inputs |
| [estimate] | an extrapolation or a guess |

## 1. Verdict (first version; superseded by the corrections above): C4 (restated) is PARTIAL

**The mechanism works as modelled [measured].**
- A keeper is excluded exactly when its scheduler priority is below the attacker's.
- While excluding, the attacker pays 1.01–1.16× p × (the binding cap) for every slot.
- A keeper that outbids the attacker lands within 1–2 slots.

**It passes with a defence budget [model].** Pool-funded keeper bids capped at a scheduler priority ≥ 1.0 make a whole-side block cost ≥ $22.5k per 600-s bell. That is ≥ 10× the busiest simulated bell ($2.2k, relic clashes included). A cap of 2.0 gives $45k–68k, which is 20–31×.

**Why it is not met.**
1. **The default tip alone fails at busy bells.**
   - A 10k-lamport tip gives a Reveal a scheduler priority of only 0.433.
   - A whole-side block for 600 s then costs $9.8k–14.8k.
   - Against 10× the bell's value this is 4.5–10.8× at the p99, max and max-with-relics bells. The mean and p90 bells pass (67× and 38×).
2. **The local validator cannot show mainnet ordering:** Jito bundles, leader discretion, stake-weighted ingress, 4-slot leader rotation, Firedancer's pack, and execution time in a 250-ms slot.
3. **Two scenarios were limited by the harness:**
   - holding all 256 SeedCache nonces with 10 streams did not fill blocks;
   - v0+ALT multi-lock transactions did not land locally, and the cause was not found.

## 2. Parameters

| | Local 3.1.9 | Mainnet (read-only) |
|---|---|---|
| Account cap | ≤ 39.9M CU per account per slot; `would_exceed_max_account_cost_limit` up to 1,248 per interval [measured] | SIMD-0306 active since slot 379,296,000: 40M [measured] |
| Block cap | 98.8–99.9M CU per slot; `would_exceed_max_block_cost_limit` up to 1,841 per interval [measured] | SIMD-0286 active since slot 435,888,000: 100M [measured] |
| Fee payer | a writable account, so ≤ 40M CU per block across all its transactions [measured] | same |
| Locks per tx | 64 (the 128 feature deactivated) | 64; the 128 feature is inactive [measured] |
| Slot time | 0.5 s idle; 0.6–0.9 s under flood [measured] | 0.263–0.275 s. The 250-ms feature has been active since slot 447,552,000. Whether the 100M limit is rescaled is not verified. |
| Block fill | — | 12 blocks: 10–34M CU; top writable account ≤ 10.8M [measured] |
| Priority market | — | minimum landed priority fee 0 in 150 of 150 recent slots [measured] |
| Confirmation lag (labelled "Clock" before M1 CL-27) | — | 1.3–1.7 s from beacon to confirmation [measured]; **not** Clock-vs-drand skew, which is unmeasured |
| Clients | — | Agave 4.3.0 on about 77% of stake; 26.x versions on about 10% [measured] |
| Fee | 5,000 + limit × price; the priority fee is charged on the declared limit [measured] | same |

## 3. Scheduler facts that decide the outcome [measured]

The scheduler ranks by

```
priority = (priority fee + 2,500) / cost
cost     = limit + 720 per signature + 300 per write lock + loaded-data cost
```

The loaded-data cost is 8 CU per 32 KiB declared; the default 64 MiB costs 16,384 CU.

1. **Without `SetLoadedAccountsDataSizeLimit`, a keeper's bid is halved.** A Reveal bidding 1.1 had priority 0.596 and was excluded for 11.3 s by an attacker at 1.0. With a 64 KiB limit the same bid had priority 1.159 and landed in 0.97 s.
2. **The base fee gives a floor.** A Reveal with zero priority fee still has priority 0.144. It landed against floods bidding 0.1, both single-account and whole-block.
3. **Only a well-packed flood excludes.**
   - A request-only flood (declares 1.4M, uses 4k) excluded nothing, even while paying 0.84 SOL per slot at a bid of 5.
   - A flood of big fills alone left gaps of 100k–1.4M that a 17k Reveal filled at half the attacker's bid (superseded runs 2 and 3).
   - Exclusion needs priced mid fillers (200k) and tail fillers (11k) whose cost is below the keeper's.
   - Each write lock adds 300 CU. So streams locking 24–26 addresses let a half-price keeper in within 1.8–3.1 s. An attacker can close that with per-account fillers.

## 4. Drill results [measured]

**Setup.**
- Reveal: 16k limit, 13.9k used, cost about 17.3k.
- ResolveClash: 280k (270k used).
- PostAnchor and PostSeed: 345k (332k used).
- "Held" means still excluded when the 8.9–14.4-s window ended.
- Full table: `results/drill-summary.md`.

**(a) One ArrivalSlot**
- Held, attacker's bid Pa over keeper's bid: 1 over 0.5 (11.0 s); 1 over 1.1 without the loaded-data limit (11.3 s); 0.3 over 0.1; 5 over 4; 1 over an escalating keeper capped at 0.67 (12.8 s).
- Landed:
  - Pa 1: keeper 1.1 in 0.97 s; keeper 2 in 0.79 s.
  - Pa 0.3: keeper 0.4 in 1.4 s.
  - Pa 0.1: keeper 0 in 0.6 s.
  - Pa 5: keeper 6 in 1.0 s.
  - Escalating keepers from 0.02: 4.3 s (cap 3) and 5.3 s (cap 10).
- Near tie: a keeper at 0.9 (priority 0.975) against 1.0 got in after 4.3 s, when the stream dipped.
- Spend per slot: 0.0427 SOL at Pa 1 against a model of 0.040; 0.2149 at Pa 5 against 0.200.

**(b) Four slots of one (province, bell, faction)**
- **Multi-lock** (every attacker tx locks all 4 slots, 1 payer) held 11.4 s, with all 4 accounts at 39.9M, for **0.0403 SOL per slot: the same cost as one slot**.
- Naive attacker (4 streams, 4 payers): held 11.7 s, because the 99.9M block limit binds. It paid 0.1028 SOL per slot (model p × 100M = 0.100).
- A keeper at 2 landed in 2.1 s.
- 24- and 26-address legacy multi-lock (1,030 B and 1,096 B): a keeper at 0.5 got in after 1.8–3.1 s, through the filler-granularity gap.

**(c) BellAnchor (anchor lock drill D1–D6)**
- **D1:** held against a keeper at 0.5 (12.7 s) and against an escalating keeper capped at 0.8 (12.8 s). A keeper at 2 landed in 0.53 s. An escalating keeper capped at 3 landed at 1.37–2.2 after 12.2 s; escalation was slowed by the keeper's own RPC round trips.
- **D2 and D3:** see (d) and (e).
- **D4:** not applicable; inline verification is dropped.
- **D5:** holding only region 7 blocked the combined 16-region anchor transaction (9.8 s). The region-3 fallback landed in 2.0 s; the region-7 fallback was held 9.5 s.
- **D6:** with an unpredictable address and a simultaneous start, the keeper at 0.1 landed first in 5 of 6 races against an attacker at 1.0 (0.65–1.5 s). *m0c: this measures the harness's cold-start ramp (the attacker's flood needs ~1–2 s to fill blocks), not priority ordering; it is not evidence for unpredictable addresses, which the design no longer uses.*
- **Seed choice:** not testable with this probe; it rests on the B2 canonical-anchor fix.

**(d) SeedCache with a nonce switch**
- A held nonce stayed held (8.9 s; 9.3 s against a 26-nonce stream).
- Switching to an unheld nonce landed in 1.1 s (single hold) and 2.4 s (26-nonce hold).
- All 256 nonces with 10 streams: harness-limited (30–57M CU per block), so the keeper landed. By construction this is the whole-block case.

**(e) Province (270k ResolveClash)**
- A resolver at 0.5 was held 10.7 s.
- A resolver at 2 landed in 0.65 s.
- An escalating resolver capped at 3 landed at 1.37 after 5.7 s.

**(f) Whole block, reveal into an address the attacker does not know** (5 streams, blocks at 99.8–99.9M)
- Held: keeper 0.5 under Pa 1 for 12.2 s (0.115 SOL per slot against a model of 0.100); keeper 0.1 under Pa 0.3 for 11.8 s.
- Landed: keeper 2 in 3.7 s; keeper 0 in 1.3 s against Pa 0.1 (the floor).
- An escalating keeper was held 14.4 s: its bid reached only 0.34, because its RPC was congested.

**Totals**
- A well-packed flood excluded every lower-priority keeper: 16 of 16. Each exception above has a named cause.
- Outbidding keepers always landed: 0.53–2.1 s against one account, 3.7 s against whole blocks.
- The attacker spent about 27.8 SOL in total over about 75,000 transactions.

## 5. Restated C4

**Value [sim × model].**
- Probed simulator at 50k wallets: 374,072 clashes over 4,032 bells; 8.9% are player against player.
- Clashes the busiest faction takes part in, per bell: mean 20.8, p90 37, p99 196, max 208. PvP only: 4.1, 7, 47, 52. Provinces with its arrivals: 6.1, 14, 44, 62.
- At $7 per clash:

| Bell | Value | 10× target |
|---|---|---|
| mean | $146 | $1.5k |
| p99 | $1,372 | $13.7k |
| max | $1,456 | $14.6k |
| max plus 5 relic clashes at $150 | $2,206 | $22.1k |

- The simulator's own economy values a laurel at about $0.0086, so a capture moves about $1.

**Attack cost [model].**
- A side-wide attack (sealed destinations) is whole-block pricing: p × 100M per block.
- A one-province attack is p × 40M per block for up to about 26 (legacy) or 64 (ALT, **[unverified]**: never landed locally) known accounts.
- The keeper's priority at the default tip is 0.433.

| Attack at the default tip | 600-s window | 1,200-s window |
|---|---|---|
| Side-wide, capacity 250M CU/s | $9.8k | $19.6k |
| Side-wide, 100M per 0.265-s slot | $14.8k | $29.6k |
| One province | $3.9k | — |

- **Ratios (side-wide, 600 s, 250M CU/s):** mean 67×, p90 38×, **p99 7.2×, max 6.7×, max with relics 4.5×**.
- The one-province attack passes: 26× a relic clash, 560× an ordinary clash.
- The delay-only targets (anchor, SeedCache, Province) cost $4.5k–45k per 10 minutes of delay against ≤ 0.0007 SOL per keeper transaction.

**Defence budget [model].**
- Target: p\* ≈ 0.98, i.e. about 19.5k lamports per Reveal.
- The subsidy above the tip is about 9.5k lamports per Reveal, ≤ 0.018 SOL per attacked bell.
- Recommended cap: **2.0** (about 40k lamports per Reveal).
  - A side-wide attack then costs $45k–68k per 600-s bell, 20–31× the worst bell.
  - A one-province attack costs $18k.
- One block of attack pays for about 3,600–5,400 keeper Reveals, so griefing the pool is a losing trade for the attacker.

## 6. What a local validator can and cannot show

**It can show, and did:**
- cost-tracker account and block caps;
- the priority formula, and exclusion following it;
- spend ≈ p × cap × blocks;
- multi-lock equivalence;
- the fee-payer cap;
- request-only floods doing nothing;
- an unfilled block releasing every pending keeper transaction;
- the loaded-data pitfall and the base-fee floor;
- the combined-anchor weakness;
- nonce switching.

**It cannot show:**
1. **Jito bundle auctions and the BundleStage.** They could fill a target account ahead of priority ordering. This is the largest unknown.
2. **Leader discretion.** Ordering is a policy, not consensus; only the cost limits are consensus rules.
3. **Multi-leader rotation and stake-weighted QUIC ingress.** Locally the keeper shared a saturated RPC with the attacker.
4. **Firedancer's pack.**
5. **Serial execution time in a 250-ms slot.** Our rough figure is about 55M CU/s [estimate], so a single account may absorb less than 40M per block, which would make one-province holds cheaper.
6. **Whether 100M is rescaled for 250-ms slots.**
7. **Real traffic.** Blocks are 10–34% full, so the attacker must buy about 70–90M CU per block itself, as modelled.
8. **The v0+ALT 64-lock path**, which did not land locally.

**Next step.** An M4 devnet or mainnet-fork soak with Jito and multiple leaders, each step with the owner's approval.

## 7. Design impact

1. **Rewrite the §8.6 and §8.7 cost model.**
   - k known accounts cost p × 40M per block (up to about 26 per legacy stream, 64 with ALT [unverified]).
   - A side-wide attack is p × 100M per block.
   - Drop the 4-slot multiplier and the "$7.2k per province-bell" figure.
2. **Keeper SDK.**
   - Pay each Reveal from a fee payer drawn at random from ≥ 150 funded keys (m0c, drill (g)).
   - Set `SetLoadedAccountsDataSizeLimit` and a tight CU limit (M1, I-45: at `L(kind)` from the release program, not 64 KiB; see the correction header).
   - Bid in priority terms: (fee + 2,500) / cost.
   - Escalate from the tip level, at least ×2 per slot, up to the cap.
   - Resend every slot.
3. **O7 defence budget.** An operator or faction pool funds bids above the tip, up to a published cap of priority 2.0 (about 40k lamports per Reveal). Expected spend is ≤ 0.02 SOL per attacked bell.
4. **Keep owners' reveals from the bell start** (window 1,200 s). Consider owner-extendable windows.
5. **Anchors.**
   - Always send per-region fallbacks alongside the combined 16-region anchor.
   - ~~Keep unpredictable anchor addresses; they stop pre-holding.~~ *Withdrawn (m0c): use SP-V2's predictable address; pre-holding only delays (D1), and an unpredictable address breaks Reveal's absence proof.*
6. **Keep keeper-nonce SeedCaches.** Holding all 256 nonces costs a whole block.
7. **The Province and the anchor are delay-only.** Price them at the cap.
8. **Per-season R14 check.** Record the account and block limits, slot time and lock limit, and re-run `c4_model.py`.
9. **Add a per-bell participation metric to `frontier-sim`**, and gate C4 on its p99 and max bells. *Done (m0c): `frontier-sim c4` on `codex/frontier`.*

## 8. Reproduce

```
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
cd scratchpad/frontier/m0b/spikes/SP-FEE
solana-test-validator --reset --quiet --ledger ledger --bind-address 127.0.0.1 --rpc-port 39899 --gossip-port 39901 \
  --dynamic-port-range 39902-39940 --faucet-port 39950 --limit-ledger-size 50000000 \
  --deactivate-feature 9LZdXeKGeBV6hRLdxS1rHbHoEUsKqesCC2ZAPTPKJAbK \
  --bpf-program program/deploy/sfee_probe-keypair.json program/deploy/sfee_probe.so &
cd driver && node calib.cjs && ./runall.sh caps a b e c d f && python3 summarize.py && python3 c4_model.py > ../results/c4-model.txt
node mainnet_ro.cjs; python3 mainnet_blocks.py ../results/mainnet-blocks.json    # read-only
# stop the validator; rm -rf ledger
cd ../simprobe/frontier-sim && RUSTUP_TOOLCHAIN=1.89.0 CARGO_TARGET_DIR=../target cargo build --release
FEE_PROBE_OUT=../clashes-50k-s1.csv ../target/release/frontier-sim run --agents 50000 --seed 1 && python3 ../analyze.py ../clashes-50k-s1.csv 6
```

**m0c additions** (validator on ports 39970–40025 with `--dynamic-port-range 39980-40020 --faucet-port 40025 --gossip-port 39972 --rpc-port 39970`, ledger `ledger-m0c`, deleted afterwards):

```
cd driver && SFEE_RPC=http://127.0.0.1:39970 node drill.cjs g > ../results/g.log && python3 summarize.py
python3 mainnet_blocks.py ../results/mainnet-blocks-m0c.json 24        # read-only
cp <frontier-sim c4 --agents 50000 --seeds 3 --out c4_50k_s3.md output> ../simprobe/   # c4_50k_s3.md and .json
python3 c4_model.py > ../results/c4-model.txt
```

**Superseded runs:**
- `results/superseded/`: without the loaded-data limit; with naive tail fillers.
- `results/run4-superseded/`: a single sender loop.
