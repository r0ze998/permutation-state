# Frontier K3 economy: officer pay, the bot criterion, the Mandate reserve, γ

- **Date:** 2026-09-27
- **Owner decisions implemented:** O3 (bound officer pay), O4 (bot criterion, in the owner's order), O6 (γ = 0.6), O10 (the Mandate reserve pays only completers who staked, closed form).
- **Code:** branch `frontier/K3-economy` (from `codex/frontier` at `cea89be`), worktree `.claude/worktrees/frontier-K3-economy`. The branch has local commits only; nothing was pushed, and no devnet or mainnet step was taken.
- **Suite:** `frontier-sim suite --agents 10000 --seeds 5`. The appendix is its verbatim output; it is also saved as `suite-output.md` next to this file.

**Tags**

| Tag | Meaning |
|---|---|
| [measured] | a property of the code, measured on this machine: tests, conservation checks, determinism, run time |
| [sim] | simulator output: the real rules-v10 kernels driven by the assumed behaviour in `frontier-sim/src/model.rs` (unchanged from M0) |
| [estimate] | an extrapolation beyond what was simulated |

> **Corrections after review (m0c, 2026-09-27; runs in `scratchpad/frontier/m0c/`).**
> 1. **The criterion's margin is about 2 points, not 4–5.** The table below pools bots that draw their join day like humans (60% on day 0). A bot operator picks its window, so the criterion is now the **maximum over the bot's own choices** (join window × stake × office; `frontier-sim criterion --best-response`, every bot of a season making the same choice, so each cell holds 100–1,000 bots per seed): worst **0.980** (1%, all bots joining on days 1–7 and staking, barred; max seed 0.982), 0.978 in office; at 2–10% the best choice is a fee-only wallet joining on days 15–21 (0.957–0.974) [sim, 3 seeds × 10k, `best_k3_s3.md`]. It still passes, so no retune was needed; M3 must re-measure it on the final economy.
> 2. **Bots hold most offices**: 61% of office-terms at 1% bots, 77% at 2%, 93% at 5%, 98% at 10% [sim, `crit_k3_s5.md`]. The ceiling bounds their USDC, not their steering. **If bot Ministers pick Mandates only always-online wallets can do, the criterion fails**: 1.041 / 1.029 / 1.001 / 0.965 at 1 / 2 / 5 / 10% when humans then complete a quarter as often, 1.64 / 1.58 / 1.44 / 1.16 when they cannot complete at all [sim, `--bot-mandates 0.25` / `0`]. A **share floor** in `mandate.rs` (divisor ≥ half of the faction's active stakers; the unclaimable part burned) cuts the worst case to 1.22 / 1.20 / 1.14 / 1.06 and leaves the default seasons bit-identical; the design adds a **fixed Mandate menu** of single-action tasks with ≥ 24-h windows so the lever does not exist. A term limit of one office-term per wallet per season cuts bots' office share at 1% to 9.5% (criterion 0.967) but does not stop steering (1.06–1.14 with humans at 0) [sim].
> 3. **Costs to honest players, stated in §1 as they should have been:** late stakers (days 15–21) lose 10–18 points against M0: casual 0.67 → 0.57, daily 0.90 → 0.76, skilled 1.01 → 0.83, very skilled 1.10 → 0.87 (A.2 vs B.1). **The skill premium over a script is gone**: very skilled fee-only 0.96 = bot fee-only 0.96; very skilled with stake 0.95 against 0.93 for bots. "Every other honest cell moves by 0.02 or less" holds only for the pooled cells. A flatter stake ramp trades these back: at ramp 1.0 the days-15–21 stakers get 0.60 / 0.79 / 0.87 / 0.90 (no worse than M0's day-0 cells 0.59 / 0.75 / 0.83) and the worst bot choice is 0.985 (1.5-point margin) [sim, `payout_ramp10000.md`, `best_ramp10000.md`]; ramp 1.5 lands between. The criterion output now prints the bots' office share and the human-vs-bot gap.

## 1. Summary

**The bot criterion now passes** (at the SDK-default join mix; see correction 1). A scripted staking wallet at the SDK default returns less than 1.0× what it pays at 1, 2, 5 and 10% of wallets, both with bots barred from office and with bots standing for office [sim, §3 and appendix K].

| Bot share | M0, bots barred from office | M0, bots in office | **K3, bots barred from office** | **K3, bots in office** |
|---|---|---|---|---|
| 1% | 1.066 | 1.785 | **0.949** (max seed 0.960) | **0.962** (max seed 0.966) |
| 2% | 1.046 | 1.513 | **0.946** (0.952) | **0.959** (0.961) |
| 5% | 0.999 | 1.213 | **0.935** (0.939) | **0.948** (0.950) |
| 10% | 0.950 | 1.061 | **0.912** (0.915) | **0.926** (0.929) |

What changed, by decision:

| Decision | What K3 does | Where |
|---|---|---|
| **O3** officer pay | **An officer-pay ceiling.** Officer pay may raise a wallet's claim to at most 95% of what the wallet paid, and never beyond it: `steward_i ≤ max(0, 0.95 × paid_i − rest_of_claim_i)`. What the ceiling cuts is swept to the next season's pools, like the 5× cap. So no wallet ends a season in profit because of an office, however many offices a farm wins. | `payout::PayoutParams::office_ceiling_bps`, `payout::claim` |
| **O4** bot criterion | The four steps were tried in order and measured after each. Only step 4 passed (§3). The shipped set is the passing set that costs honest players least: **steps 2–4, with the Works rate lowered from 140 to 105 per USDC** rather than to 70. | see §3 |
| O4 step 1 | 105 Works per USDC (was 140) | `payout::WORKS_PER_USDC`, `PayoutParams::works_per_usdc` |
| O4 step 2 | The laurel stake is priced by the expected accrual still to come (ramp 2.0, fitted to the simulated world emission), not by days left | `pools::EntrySchedule::SEASON1`, `stake_ramp_bps` |
| O4 step 3 | Holdings emit in proportion to their order factor (1, ½, ¼) | `laurel::RewardIndex::emit_quarters`, `laurel::emission_quarters` |
| O4 step 4 | Relic Sites mint no laurels | `laurel::RELIC_EMISSION_PER_BELL = 0` |
| **O10** Mandate reserve | New closed-form kernel. Only wallets that are stakers when they complete a term's Mandate get a share. Each claim is `min(cap, budget × s_i / S)`, one O(1) step. The cap cut, rounding and unclaimed shares roll into the next term. Conservation is exact: `deposited = balance + Σ open terms (budget − paid) + paid`. | new `frontier/mandate.rs` |
| **O6** γ | γ = 0.6 stays the default (`IndexParams::REV2`, 3/5). Added `IndexParams::validate()`. The herding ratio at 3× size is **0.684** at γ = 0.6 [sim]. | `index.rs` |

**Conservation [measured].**
- All 1,620 settlements in the suite passed every check. These now include "officer pay within its ceiling" and "Mandate reserve: deposited = balance + paid".
- The 560 seasons of the §K ladder also assert every check inside the run, and none failed.
- New kernel property tests:
  - `officer_pay_never_makes_an_office_profitable`: 200 random seasons, plus a 12-wallet farm holding every office.
  - `mandate_reserve_pays_staker_completers_in_closed_form`: 200 random seven-term histories, checked against a member loop, with the conservation identity verified after every claim.

**What it costs honest players [sim, 5% bots; appendix A vs B]**

| Archetype | fee only: M0 → K3 | with stake: M0 → K3 |
|---|---|---|
| idle | 0.51 → 0.51 | 0.24 → 0.24 |
| casual | 0.80 → 0.80 | 0.63 → 0.64 |
| daily | 0.87 → 0.88 | 0.81 → 0.82 |
| skilled | **1.00 → 0.93** | 0.90 → 0.89 |
| very skilled | **2.87 → 0.96** | **1.58 → 0.95** |
| scripted bot | 1.02 → 0.96 | 1.00 → 0.93 |

The one large cost is **officer pay**. In M0 the very skilled players' multiples came almost entirely from steward rows: 5.6–6.8 USDC against 3.6–8.4 USDC paid. The ceiling removes most of that. In seed 1 the rows asked for about 840 USDC, and 744 USDC of it was cut and swept to the next season, which is 1.8% of the prize. The whole suite shows 1.6–2.2% swept. **That is O3 as asked: an always-online wallet cannot profit from an office, and neither can any other wallet.** If the owner wants offices to pay the most active humans more, the only way that keeps the bot criterion is a higher ceiling:
- a 100% ceiling (break-even) still passes, with bots at 0.989 at 1%;
- going above 100% gives up the "no profit from office" guarantee.

Every other honest cell moves by 0.02 or less.

## 2. O3: why a ceiling on the claim, and not the two suggested forms

The owner suggested two forms: cap each officer's pay at a share of what that officer paid, or pay officers in laurels from the Mandate budget. Both were implemented as simulator variants and measured on the final K3 economy, with bots standing for office (appendix K.1):

| Officer pay | bot + stake at 1 / 2 / 5 / 10% | very skilled + stake | passes? |
|---|---|---|---|
| a. no bound (revision 2) | 1.625 / 1.390 / 1.144 / 1.019 | 1.03 | no |
| b. USDC rows capped at 25% of what the officer paid | 1.164 / 1.135 / 1.059 / 0.989 | 1.01 | no |
| c. laurels from the Mandate budget (Minister 2 shares, paid Warden 1, stakers only; no USDC rows) | 1.110 / 1.046 / 0.982 / 0.939 | 0.97 | no |
| d. ceiling 90% | 0.951 / 0.947 / 0.936 / 0.917 | 0.93 | yes |
| **e. ceiling 95% (chosen)** | **0.962 / 0.959 / 0.948 / 0.926** | 0.94 | **yes** |
| f. ceiling 100% (break-even) | 0.989 / 0.986 / 0.966 / 0.937 | 0.95 | yes (thin at 1–2%) |

**Why (b) fails.** A cap of s × paid lets every officer gain up to s. When bots are few, they win most offices [sim], so nearly every bot gains close to s. To keep bots under 1.0× in office, s would have to be smaller than the bot's margin under 1.0 (a few percent). At that size office pay means nothing to humans either.

**Why (c) fails.** The Mandate budget is a fixed pot per faction. Bots who win offices take a share of it that does not shrink as the number of bots shrinks, so the gain per bot is largest when bots are rare: 1.11× at 1%. It is also the wrong shape: a fee-only officer gets nothing at all.

**Why the ceiling works.** It bounds each wallet's outcome, not the pay rate. Officer pay can only raise a claim toward 95% of what was paid, so:
- no wallet profits from an office, whatever the farm size or bot share, including under the collusion that was not modelled (farms voting for each other);
- it stays a closed form: one `min` per claim, O(1), with the cut going to `swept`, so `claimed + swept + dust = prize` still holds exactly.

The group multiple of bots in office can still rise a little: bots below 95% are topped up toward it (0.949 → 0.962 at 1%). 95% was chosen over 100% for that margin.

**Cost, stated plainly.** Officer pay now acts as compensation for time, not as income. An officer who already gets back 95% or more is paid nothing more. The design text in §4.3 (H4) must say so.

## 3. O4: the steps, in order, with measurements

"Passes" means the SDK-default staking bot is below 1.0× at 1, 2, 5 and 10%, both with bots barred from office and with bots in office (O3 active). The full rows, with the honest cells from the same seasons, are in appendix K.

| # | Economy (cumulative, owner's order) | bots barred: worst of 1/2/5/10% | bots in office: worst | passes? |
|---|---|---|---|---|
| 0 | M0 (revision 2) | 1.066 | 1.785 | no |
| 1 | + O10 + O3 ceiling 95% | 1.068 | 1.122 | no |
| 2 | **step 1**: + 70 Works per USDC (35 tried too: 1.006 / 1.095) | 1.021 | 1.101 | no |
| 3 | **step 2**: + stake priced by accrual left | 1.021 | 1.088 | no |
| 4 | **step 3**: + order-weighted emission | 1.060 | 1.101 | no |
| 5 | **step 4**: + no Relic Site laurels | **0.928** | **0.951** | **yes: stop** |

Step 4 was the first that passed, so the O4 search stops there. Relic Site laurels were removed because the steps before did not pass without it ("only if needed").

**Backing off to spare honest players.** With step 4 in place, the rows below test which earlier steps are still needed:

| # | Economy | barred | in office | passes? | very skilled + stake at 5%, bots barred | skilled + stake | daily fee only |
|---|---|---|---|---|---|---|---|
| 5 | all four steps, 70 Works per USDC | 0.928 | 0.951 | yes | 0.94 | 0.88 | 0.89 |
| 7 | step 4 alone (140, D3 stake, full emission) | 0.967 | **1.007** | no | 1.00 | 0.89 | 0.87 |
| 8 | 70 Works + step 4 | 0.920 | 0.985 | yes | 0.98 | 0.89 | 0.89 |
| 6 | steps 2–4, 140 Works per USDC | 0.973 | 0.978 | yes | 0.96 | 0.89 | 0.87 |
| **9** | **steps 2–4, 105 Works per USDC (shipped)** | **0.949** | **0.962** | **yes** | 0.95 | 0.89 | 0.88 |

Findings:
- **Relic Site laurels were the largest single edge.** Restoring them alone puts bots back at 1.04× at 2% [sim, appendix G]. Bots are online every hour and hold the sites by the bell.
- **Order-weighted emission is what keeps bots under 1.0 once they stand for office.** Without it, bots in office reach 1.007 (row 7) against 0.978 with it (row 6). In M0 the same change looked harmful, because Relic Sites dominated. It also improves join-day parity (§4).
- **Pricing the stake by accrual barely moves bots:** 1.021 → 1.021. A dearer late stake mostly enlarges the laurel pool, and bots share it pro rata. It matters for join-day fairness (§4), and it closes the late-stake exploit: bots that stake on day 21 now return 0.86–0.88×, against 1.04–1.14× in M0 [sim, appendix G].
- **Works rate.** 140 still passes, but with a margin of only 2–3 points at 1%. 105 gives 4–5 points of margin, and honest cells move by 0.01 or less. 70 costs very skilled and skilled players a little more for margin that is not needed. **105 is the choice.** If the owner prefers the smallest change, row 6 (140) also passes.
- **The tuned script** (quality 0.9, aggression 0.8) returns 0.94 at 5% and 0.92 at 10%, no better than the default [sim, appendix D].

## 4. Payout multiples by archetype and join day (K3, 5% bots) [sim]

Pooled over 5 seeds × 10k wallets. Full tables: appendix A (K3) and B (M0 on the same seeds).

| Archetype | stake | day 0 | days 1–7 | days 8–14 | days 15–21 | all |
|---|---|---|---|---|---|---|
| casual | no | 0.80 | 0.80 | 0.80 | 0.80 | 0.80 |
| casual | yes | 0.64 | 0.68 | 0.62 | 0.57 | 0.64 |
| daily | no | 0.87 | 0.88 | 0.88 | 0.89 | 0.88 |
| daily | yes | 0.81 | 0.90 | 0.83 | 0.76 | 0.82 |
| skilled | yes | 0.88 | 0.96 | 0.90 | 0.83 | 0.89 |
| very skilled | yes | 0.96 | 0.97 | 0.93 | 0.87 | 0.95 |
| scripted bot | yes | 0.94 | 0.98 | 0.91 | 0.85 | 0.93 |

**Join-day parity improved.** For daily stakers, day 0 against days 1–7 went from 0.75 vs 1.00 (M0) to 0.81 vs 0.90. The laurel-pool claim per staked USDC by join bucket is now 0.77 / 0.90 / 0.79 / 0.70, against 0.67 / 1.08 / 1.08 / 0.95 in M0 (appendix A.3 and B.2).

The remaining shape has two causes:
- days 1–7 are still best, because of placement: late joiners' first holdings carry 1.6–1.75× their province's mean weight (A.4);
- days 15–21 are now slightly worse, because the ramp prices them for the season's late emission.

## 5. Bots by share [sim, appendix D]

| Bot share | 1% | 2% | 5% | 10% | 20% | 30% | 50% |
|---|---|---|---|---|---|---|---|
| bot + stake | 0.95 | 0.95 | 0.94 | 0.91 | 0.88 | 0.85 | 0.82 |
| bot fee only | 0.97 | 0.97 | 0.96 | 0.95 | 0.92 | 0.90 | 0.87 |
| bots' share of stakers' laurels | 3.7% | 7.1% | 16.6% | 29.3% | 47.8% | 61.0% | 78.6% |

With bots standing for office: 0.962 / 0.959 / 0.948 / 0.926 at 1 / 2 / 5 / 10% (appendix K, row 9). Without the ceiling: 1.39 / 1.15 / 1.02 at 2 / 5 / 10% (appendix G, last rows).

## 6. Herding at γ = 0.6 (O6) [sim, appendix C]

- The herding ratio (big faction's payout per USDC ÷ the small factions') at γ = 0.6 is **0.862 at m = 1.5, 0.781 at m = 2 and 0.684 at m = 3**. The maximum over seeds at m = 3 is 0.692. The K3 economy does not change herding: M0 had 0.680.
- β on the undamped index is 0.86–0.88. γ = 0.6 keeps the ratio at or below 1.0 at m = 3 for a real β up to about 1.45 [estimate, C.4: minimum γ is 0.27 / 0.54 / 0.81 for β = 1.2 / 1.4 / 1.6].
- γ stays 0.6 until M3 measures β with real players. `IndexParams::validate()` now bounds γ to [0, 3] and its denominator to 20 or less.

## 7. The Mandate reserve kernel (O10)

`frontier::mandate`:
- `Reserve` per faction: `balance`, `deposited`, `paid`.
- `MandateTerm` per (faction, term): `shares`, `budget`, `claimed_shares`, `paid`, and a state of Open, Closed or Swept.

| Instruction | Kernel call | Cost |
|---|---|---|
| every laurel credit | `Reserve::deposit(10%)` | O(1) |
| complete a Mandate | `MandateTerm::complete(staker)`: 1 share for a staker, 0 for a fee-only wallet (it still earns its Works) | O(1) |
| `CloseTerm` (permissionless, after the term) | moves the whole reserve balance into the budget; a term with no shares leaves it in the reserve | O(1) |
| claim | `min(96 laurels × s_i, budget × s_i / S)` | O(1) |
| `SweepTerm` | returns `budget − paid` to the reserve, once every share is claimed or the deadline has passed | O(1) |

**Seed 1 [sim]:**
- 17,103 staker completions earned shares.
- 15,769 fee-only completions earned Works only.
- 297,057 laurels were paid out, and 0 were left in the reserve at the end.
- Conservation: deposited 3,564,684,884,266 units = balance 1,012 + paid 3,564,684,883,254.

Paying stakers only raised Mandate laurels per completing staker by about 40%: about 95 a season for skilled players and bots, against 68 in M0 (appendix A.6 vs the M0 RESULTS). It moves the bot multiple by less than 0.01 (appendix G, "Mandate reserve to every completer").

**The simulator also changed order at the end of a season:** it now banks every holding first and then closes the last term. The last credits' 10% therefore reaches the last term's budget and is not left over. The M0 run closed the term first.

**For the program (M1):**
- The Citizen stores the last term it completed and its shares.
- The MandateTerm is a small PDA per (faction, term).
- The claim deadline must be fixed; one term after close is suggested.

## 8. What was not done, and follow-ups

- **Relic Sites no longer pay laurels.** The design must give them another role or remove them. One option, not measured: pay their emission into the holder faction's Mandate reserve.
- **Officer pay goes to the next season.** About 2% of each prize is cut by the ceiling and swept forward. Lowering the rows (for example Minister 1 USDC, Warden 0.25) would return more of the steward pot to this season's citizen pool in closed form, at the cost of less top-up for officers who are losing money.
- **The stake ramp (2.0) is fitted to simulated emission.** M3 must re-fit it on real play. It is a season parameter, validated to at most 10.0.
- **Doctrine tables re-ran on the K3 economy:**
  - the tuned set now has 2 of 6 in the band (A 18.7%, B 16.0%; C and D 19.3%, E 13.7%, F 13.0%) against M0's 3 of 6;
  - the shift is within the 1.5-point binomial error of a 600-season run;
  - doctrine tuning belongs to K3-doctrines. *Integration (k3-integ) and m0c: the merged kernel table on the K3 economy was re-checked and re-tuned on ≥ 1,500 paired seasons; see DOCTRINES.md's correction header.*
- **Still open from the §4.8 review (not in this task):**
  - the Ledger is global rather than per faction;
  - `permutation-rules` builds without release overflow checks;
  - some simulator conservation checks are identities rather than bounds.
- **Bot farms voting for each other** were not modelled. The ceiling's guarantee does not depend on them. *m0c: but Mandate steering by bot officers does break the criterion; see correction 2.*

## 9. How to reproduce

```sh
cd /Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-K3-economy
cargo test -p permutation-rules                 # 348 passed (debug); --release: 348 passed [measured]
cd frontier-sim
cargo test                                      # 4 passed (debug, overflow checks on)
cargo run --release -- criterion --seeds 5      # K3: the table in §1
cargo run --release -- criterion --seeds 5 --rev2-economy   # M0 on the same seeds
cargo run --release -- suite --agents 10000 --seeds 5 --out suite-output.md
```

Suite wall time: 1,629 s on a shared 16-thread M4 Max, which ran at about 5.6 cores because other jobs were running [measured]. One 10k season takes 7.3–8.1 s with 5 running in parallel under that load. The seed-1 digest is `dcb040cb5084f905` on both the main thread and a worker thread; M0's was `c71a8a0eb7641b53`, and the economy changed.

---

# Appendix: suite output (verbatim)

<!-- generated by frontier-sim suite: 10000 wallets, 5 seeds per cell -->

## A. Payout multiple by archetype and join day (baseline)

5 seasons × 10000 wallets, equal factions, bots 5%, Shades 0.5%, doctrines off, γ = 0.6, **K3 economy** (officer-pay ceiling 95%, 105 Works per USDC, stake priced by accrual left, order-weighted emission, no Relic Site laurels, Mandate reserve to staker completers). Pooled over seeds 1..=5. Multiples are Σ claims / Σ paid for the group [sim].

| Archetype | wallets | citizen fee only (x fee) | with laurel stake (x fee+stake) | stakers |
|---|---|---|---|---|
| idle wallet | 7090 | 0.51 | 0.24 | 324 |
| casual 3x/week | 21260 | 0.80 | 0.64 | 2112 |
| daily 30 min | 14175 | 0.88 | 0.82 | 7171 |
| skilled 1-2 h | 4255 | 0.93 | 0.89 | 3834 |
| very skilled 4 h | 470 | 0.96 | 0.95 | 418 |
| scripted bot | 2500 | 0.96 | 0.93 | 2250 |

Per-seed spread of key cells:

| Cell | per seed | mean |
|---|---|---|
| scripted bot, with stake | 0.933, 0.932, 0.930, 0.933, 0.932 | 0.932 |
| scripted bot, fee only | 0.963, 0.964, 0.964, 0.966, 0.966 | 0.965 |
| very skilled, with stake | 0.953, 0.952, 0.949, 0.955, 0.954 | 0.953 |
| skilled, with stake | 0.882, 0.885, 0.888, 0.891, 0.888 | 0.887 |
| daily, with stake | 0.821, 0.820, 0.819, 0.819, 0.821 | 0.820 |
| daily, fee only | 0.876, 0.876, 0.876, 0.877, 0.877 | 0.876 |
| casual, fee only | 0.797, 0.797, 0.797, 0.798, 0.799 | 0.798 |
| idle, fee only | 0.506, 0.506, 0.506, 0.507, 0.507 | 0.506 |

### A.1 Claim parts (USDC per wallet)

| Archetype | stake | wallets | paid | citizen pool | Civilisation Share | laurel pool | steward | total claim | multiple |
|---|---|---|---|---|---|---|---|---|---|
| idle wallet | no | 6766 | 3.37 | 1.70 | 0.00 | 0.00 | 0.00 | 1.70 | 0.51 |
| idle wallet | yes | 324 | 8.72 | 1.72 | 0.00 | 0.42 | 0.00 | 2.13 | 0.24 |
| casual 3x/week | no | 19148 | 3.38 | 2.22 | 0.47 | 0.00 | 0.00 | 2.70 | 0.80 |
| casual 3x/week | yes | 2112 | 8.66 | 2.21 | 0.47 | 2.83 | 0.00 | 5.52 | 0.64 |
| daily 30 min | no | 7004 | 3.35 | 2.46 | 0.47 | 0.00 | 0.00 | 2.94 | 0.88 |
| daily 30 min | yes | 7171 | 8.64 | 2.47 | 0.47 | 4.14 | 0.00 | 7.08 | 0.82 |
| skilled 1-2 h | no | 421 | 3.40 | 2.69 | 0.48 | 0.00 | 0.01 | 3.17 | 0.93 |
| skilled 1-2 h | yes | 3834 | 8.67 | 2.67 | 0.47 | 4.47 | 0.08 | 7.69 | 0.89 |
| very skilled 4 h | no | 52 | 3.61 | 2.97 | 0.51 | 0.00 | 0.00 | 3.48 | 0.96 |
| very skilled 4 h | yes | 418 | 8.61 | 2.75 | 0.47 | 4.55 | 0.42 | 8.20 | 0.95 |
| scripted bot | no | 250 | 3.36 | 2.77 | 0.47 | 0.00 | 0.00 | 3.24 | 0.96 |
| scripted bot | yes | 2250 | 8.60 | 2.75 | 0.47 | 4.80 | 0.00 | 8.01 | 0.93 |

### A.2 By join day

| Archetype | stake | day 0 | days 1-7 | days 8-14 | days 15-21 |
|---|---|---|---|---|---|
| idle wallet | no | 0.51 | 0.51 | 0.51 | 0.51 |
| idle wallet | yes | 0.24 | 0.24 | 0.24 | 0.27 |
| casual 3x/week | no | 0.80 | 0.80 | 0.80 | 0.80 |
| casual 3x/week | yes | 0.64 | 0.68 | 0.62 | 0.57 |
| daily 30 min | no | 0.87 | 0.88 | 0.88 | 0.89 |
| daily 30 min | yes | 0.81 | 0.90 | 0.83 | 0.76 |
| skilled 1-2 h | no | 0.93 | 0.95 | 0.95 | 0.95 |
| skilled 1-2 h | yes | 0.88 | 0.96 | 0.90 | 0.83 |
| very skilled 4 h | no | 0.97 | 0.96 (n=9) | 0.96 (n=4) | 0.96 (n=4) |
| very skilled 4 h | yes | 0.96 | 0.97 | 0.93 | 0.87 |
| scripted bot | no | 0.96 | 0.96 | 0.96 | 0.96 |
| scripted bot | yes | 0.94 | 0.98 | 0.91 | 0.85 |

### A.3 Laurels by join day (stakers, idle excluded)

| Join | stakers | laurels / wallet | per day available: all | holding share | Mandates | relics | other | laurels per staked USDC | laurel-pool claim / stake | first holding Stronghold |
|---|---|---|---|---|---|---|---|---|---|---|
| day 0 | 9323 | 478.7 | 17.10 | 13.35 | 3.75 | 0.00 | -0.00 | 79.8 | 0.77 | 99% |
| days 1-7 | 2185 | 513.2 | 21.45 | 17.30 | 4.15 | 0.00 | -0.00 | 92.8 | 0.90 | 98% |
| days 8-14 | 2110 | 359.7 | 21.19 | 16.76 | 4.43 | 0.00 | -0.00 | 81.7 | 0.79 | 51% |
| days 15-21 | 2167 | 205.1 | 20.36 | 15.48 | 4.88 | 0.00 | -0.00 | 72.3 | 0.70 | 0% |

### A.4 Where first holdings sit at T_end (stakers, idle excluded)

| Join | first holdings | attached holdings in its province | of which first holdings | own weight / province mean |
|---|---|---|---|---|
| day 0 | 9318 | 11.51 | 9.59 | 1.12 |
| days 1-7 | 2185 | 10.79 | 4.80 | 1.61 |
| days 8-14 | 2109 | 9.15 | 3.49 | 1.75 |
| days 15-21 | 2166 | 7.87 | 3.24 | 1.68 |

### A.5 First holding at T_end

| Archetype | Hamlet | Town | City | Stronghold | none (released) | holdings / wallet |
|---|---|---|---|---|---|---|
| idle wallet | 8% | 0% | 0% | 0% | 92% | 0.08 |
| casual 3x/week | 1% | 4% | 21% | 74% | 0% | 1.68 |
| daily 30 min | 0% | 1% | 18% | 81% | 0% | 2.14 |
| skilled 1-2 h | 0% | 1% | 18% | 81% | 0% | 2.33 |
| very skilled 4 h | 0% | 1% | 20% | 79% | 0% | 2.46 |
| scripted bot | 0% | 1% | 21% | 78% | 0% | 2.85 |

### A.6 Laurel sources, Works, sessions (per wallet)

| Archetype | laurels banked / wallet | holding share | occupation | relic | mandate | captures in | captures out | siege stakes net | works / wallet | sessions / wallet |
|---|---|---|---|---|---|---|---|---|---|---|
| idle wallet | 42.7 | 42.7 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| casual 3x/week | 265.9 | 262.7 | 0.0 | 0.0 | 3.2 | 0.0 | 0.0 | -0.0 | 60.3 | 10.7 |
| daily 30 min | 380.3 | 330.7 | 0.0 | 0.0 | 49.6 | 0.1 | 0.1 | -0.0 | 188.7 | 23.5 |
| skilled 1-2 h | 452.0 | 356.7 | 0.0 | 0.0 | 95.4 | 0.0 | 0.2 | -0.0 | 295.6 | 46.2 |
| very skilled 4 h | 464.3 | 370.4 | 0.0 | 0.0 | 94.1 | 0.1 | 0.2 | 0.0 | 418.6 | 91.3 |
| scripted bot | 486.3 | 391.4 | 0.0 | 0.0 | 94.9 | 0.0 | 0.0 | 0.0 | 466.5 | 537.6 |

### A.7 One season in numbers (seed 1)

Final ring 28, rings opened 27; sessions 458269; clashes 84404 (19588 engagements, 0 kernel refusals); sieges declared 1301 / completed 1267 / failed 21 (never held 16, lost 5); occupations 14, liberations 1, captures 24, Free City captures 1183; camps beaten 15934; routs 78; Disarray postures 0; Engine stages 5 on days [6, 12, 15, 18, 21]; Relic Sites 0 minting 0 laurels; dormancies 5299, first holdings released 1438; 2nd/3rd holdings founded 7219; withdrawn joins 0; Mandate shares 17103 (fee-only completions without a share 15769), Mandate laurels paid 297057, left in reserve 0; office-terms Minister 168 / paid Warden 927.

### A.8 Conservation (seed 1)

| Check | result | detail |
|---|---|---|
| vault = paid - withdrawn | PASS | pools.total 51260320280 = paid 51260320280 - withdrawn 0 |
| no escrow left pending | PASS | pending 0 |
| operator escrow = 20% | PASS | operator 10252066154 of settled 51260320280 |
| claims + swept + dust = prize | PASS | claimed 40264474722 + swept 743762509 + dust 16895 = prize 41008254126 |
| allocated <= prize | PASS | allocated 41008254120 prize 41008254126 |
| wallet cap 5x paid | PASS | 0 claims above 5x |
| officer pay within its ceiling | PASS | 0 officers paid above the ceiling |
| voided Shades paid nothing | PASS | 0 voided claims > 0 |
| laurels: held = credited + relic minted | PASS | held 35646853034299 sources 35646853034299 |
| Mandate reserve: deposited = balance + paid | PASS | deposited 3564684884266 = balance 1012 + paid 3564684883254; open 0 |
| reward index: credited <= emitted - orphaned | PASS | emitted 35646853500000, credited 35646853034299, orphaned 0, rounding dust 465701 base units |

Season wall time [measured]: 7.3–8.1 s per 10000-wallet season (release build, one thread each, 5 in parallel).

## B. For comparison: the M0 (revision 2) economy on the same seeds

Same seeds and settings as A, with the economy of `cea89be`: every holding emits 1/12 a bell, Relic Sites pay 1 laurel a bell, 140 Works per USDC, stakes priced by days left, officer pay without a ceiling, Mandate reserve split among every completer [sim].

| Archetype | wallets | citizen fee only (x fee) | with laurel stake (x fee+stake) | stakers |
|---|---|---|---|---|
| idle wallet | 7090 | 0.51 | 0.24 | 324 |
| casual 3x/week | 21260 | 0.80 | 0.63 | 2112 |
| daily 30 min | 14175 | 0.87 | 0.81 | 7171 |
| skilled 1-2 h | 4255 | 1.00 | 0.90 | 3834 |
| very skilled 4 h | 470 | 2.87 | 1.58 | 418 |
| scripted bot | 2500 | 1.02 | 1.00 | 2250 |

### B.1 By join day

| Archetype | stake | day 0 | days 1-7 | days 8-14 | days 15-21 |
|---|---|---|---|---|---|
| idle wallet | no | 0.51 | 0.51 | 0.51 | 0.51 |
| idle wallet | yes | 0.24 | 0.25 | 0.26 | 0.31 |
| casual 3x/week | no | 0.80 | 0.80 | 0.80 | 0.80 |
| casual 3x/week | yes | 0.59 | 0.77 | 0.71 | 0.67 |
| daily 30 min | no | 0.87 | 0.88 | 0.88 | 0.88 |
| daily 30 min | yes | 0.75 | 1.00 | 0.97 | 0.90 |
| skilled 1-2 h | no | 1.00 | 1.01 | 1.01 | 0.97 |
| skilled 1-2 h | yes | 0.83 | 1.09 | 1.08 | 1.01 |
| very skilled 4 h | no | 3.19 | 2.25 (n=9) | 1.34 (n=4) | 1.00 (n=4) |
| very skilled 4 h | yes | 1.68 | 1.53 | 1.27 | 1.10 |
| scripted bot | no | 1.02 | 1.02 | 1.01 | 1.01 |
| scripted bot | yes | 0.90 | 1.23 | 1.30 | 1.15 |

### B.2 Laurels by join day

| Join | stakers | laurels / wallet | per day available: all | holding share | Mandates | relics | other | laurels per staked USDC | laurel-pool claim / stake | first holding Stronghold |
|---|---|---|---|---|---|---|---|---|---|---|
| day 0 | 9323 | 528.3 | 18.87 | 16.18 | 2.49 | 0.20 | -0.01 | 88.0 | 0.67 | 100% |
| days 1-7 | 2185 | 728.6 | 30.42 | 26.48 | 2.79 | 1.15 | -0.00 | 142.1 | 1.08 | 98% |
| days 8-14 | 2110 | 515.6 | 30.35 | 25.10 | 3.15 | 2.10 | -0.00 | 141.8 | 1.08 | 52% |
| days 15-21 | 2167 | 266.9 | 26.23 | 21.79 | 3.56 | 0.88 | -0.00 | 124.3 | 0.95 | 0% |

### B.3 Where first holdings sit

| Join | first holdings | attached holdings in its province | of which first holdings | own weight / province mean |
|---|---|---|---|---|
| day 0 | 9320 | 11.55 | 9.60 | 1.12 |
| days 1-7 | 2185 | 10.69 | 4.75 | 1.61 |
| days 8-14 | 2108 | 9.17 | 3.45 | 1.76 |
| days 15-21 | 2165 | 8.12 | 3.35 | 1.68 |

## C. Herding: faction size, β and γ

Faction 0 has m× the members of each other faction (same archetype mix in every faction, stratified). 5 seeds per size, 10000 wallets. β is measured as `1 + ln(r_big / r_small) / ln m`, where r is the per-active-member path value (per path) or the undamped index `clamp(mean ratio)` [sim].

### C.1 β by path (mean over seeds; min–max)

| m | Dominion | Prosperity | Knowledge | Concord | index (undamped) |
|---|---|---|---|---|---|
| 1.5 | 0.52 (0.48–0.55) | 0.98 (0.96–1.05) | 0.99 (0.97–1.05) | 0.99 (0.96–1.02) | 0.88 (0.86–0.91) |
| 2 | 0.50 (0.48–0.52) | 0.99 (0.98–1.00) | 0.99 (0.97–1.00) | 0.98 (0.96–0.99) | 0.87 (0.85–0.89) |
| 3 | 0.44 (0.43–0.46) | 0.99 (0.98–1.00) | 0.99 (0.98–1.00) | 0.98 (0.96–1.00) | 0.86 (0.85–0.88) |

### C.2 Herding ratio: per-capita payout (Σ claims / Σ paid) of the big faction ÷ the small factions

| m | γ = 0.00 | γ = 0.30 | γ = 0.60 | γ = 0.80 | γ = 1.00 | γ = 1.20 | γ = 1.50 | γ = 2.00 |
|---|---|---|---|---|---|---|---|---|
| 1 | 0.993 (max 1.006) | 0.993 (max 1.005) | 0.993 (max 1.005) | 0.992 (max 1.005) | 0.992 (max 1.005) | 0.992 (max 1.005) | 0.992 (max 1.004) | 0.991 (max 1.004) |
| 1.5 | 0.970 (max 0.978) | 0.914 (max 0.922) | 0.862 (max 0.869) | 0.829 (max 0.835) | 0.798 (max 0.804) | 0.768 (max 0.773) | 0.725 (max 0.730) | 0.660 (max 0.664) |
| 2 | 0.948 (max 0.956) | 0.860 (max 0.868) | 0.781 (max 0.788) | 0.733 (max 0.740) | 0.689 (max 0.695) | 0.647 (max 0.653) | 0.591 (max 0.596) | 0.509 (max 0.514) |
| 3 | 0.915 (max 0.924) | 0.791 (max 0.799) | 0.684 (max 0.692) | 0.622 (max 0.630) | 0.567 (max 0.575) | 0.518 (max 0.525) | 0.454 (max 0.461) | 0.368 (max 0.375) |

### C.3 Stakers and fee-only citizens separately (big ÷ small)

| m | γ | fee only | with stake | s_big | s_small (mean) | h_big |
|---|---|---|---|---|---|---|
| 1.5 | 0.00 | 0.979 | 0.964 | 0.962 | 1.011 | 1.000 |
| 1.5 | 0.60 | 0.896 | 0.837 | 0.790 | 1.011 | 0.821 |
| 1.5 | 1.00 | 0.845 | 0.763 | 0.692 | 1.011 | 0.720 |
| 2 | 0.00 | 0.963 | 0.937 | 0.936 | 1.026 | 1.000 |
| 2 | 0.60 | 0.833 | 0.743 | 0.677 | 1.026 | 0.723 |
| 2 | 1.00 | 0.757 | 0.638 | 0.545 | 1.026 | 0.582 |
| 3 | 0.00 | 0.940 | 0.897 | 0.908 | 1.056 | 1.000 |
| 3 | 0.60 | 0.756 | 0.633 | 0.557 | 1.056 | 0.614 |
| 3 | 1.00 | 0.656 | 0.505 | 0.402 | 1.056 | 0.443 |

### C.4 If real players scale better than the simulated ones [estimate]

At m = 3 the payout ratio moves as h_big^α with α = 0.59 (fitted from the γ grid; α is between ½ for the citizen pool's √s and 1 for the laurel pool's s). With the simulated β = 0.86 and ratio 0.915 at γ = 0, a population with a higher β needs γ ≥ (ln r0 + α (β − β_sim) ln 3) / (α ln 2.25):

| assumed β | minimum γ for ratio ≤ 1.0 at m = 3 |
|---|---|
| 1.0 | 0.00 |
| 1.2 | 0.27 |
| 1.4 | 0.54 |
| 1.6 | 0.81 |

The undamped index stays within 0.901–1.084 in every herding run, inside the clamp [0.5, 2], so the clamp never binds here.

**Smallest γ on the grid with the ratio ≤ 1.0 at m = 3 in every seed: γ = 0/1 = 0.00.**

## D. Scripted bots: return vs bot share

Bot share of all wallets; the rest is the baseline human mix. 5 seeds per row, 10000 wallets. SDK default: decision quality 0.6, aggression 0.5, hourly sessions, never withholds; tuned script: quality 0.9, aggression 0.8 [sim].

| strategy | bot share | bot with stake | (min–max) | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | bots' share of laurels |
|---|---|---|---|---|---|---|---|---|---|---|
| SDK default | 1% | **0.95** | 0.94–0.96 | 0.97 | 0.96 | 0.91 | 0.84 | 0.88 | 0.80 | 3.7% |
| SDK default | 2% | **0.95** | 0.94–0.95 | 0.97 | 0.96 | 0.90 | 0.83 | 0.88 | 0.80 | 7.1% |
| SDK default | 5% | **0.94** | 0.93–0.94 | 0.96 | 0.95 | 0.89 | 0.82 | 0.88 | 0.80 | 16.6% |
| SDK default | 10% | **0.91** | 0.91–0.91 | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.79 | 29.3% |
| SDK default | 20% | **0.88** | 0.87–0.88 | 0.92 | 0.94 | 0.85 | 0.78 | 0.85 | 0.79 | 47.8% |
| SDK default | 30% | **0.85** | 0.85–0.86 | 0.90 | 0.94 | 0.83 | 0.77 | 0.84 | 0.78 | 61.0% |
| SDK default | 50% | **0.82** | 0.82–0.83 | 0.87 | 0.94 | 0.84 | 0.75 | 0.83 | 0.78 | 78.6% |
| tuned script | 5% | **0.94** | 0.93–0.94 | 0.96 | 0.95 | 0.89 | 0.82 | 0.88 | 0.80 | 16.7% |
| tuned script | 10% | **0.92** | 0.91–0.92 | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.79 | 29.5% |
| tuned script | 30% | **0.86** | 0.85–0.86 | 0.90 | 0.94 | 0.83 | 0.77 | 0.84 | 0.78 | 61.1% |

SDK-default bots at 5%, with stake, by join day: day 0 0.94, days 1-7 0.98, days 8-14 0.91, days 15-21 0.85.

## G. What drives the bot edge

SDK-default bots; each row switches one K3 change back (or adds one behaviour) [sim variants]. 5 seeds per cell, 10000 wallets.

| variant | bot share | bot + stake | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | late-join daily + stake (days 1-21) vs day 0 |
|---|---|---|---|---|---|---|---|---|---|
| K3 economy (baseline) | 2% | **0.95** | 0.97 | 0.96 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| K3 economy (baseline) | 5% | **0.93** | 0.96 | 0.95 | 0.89 | 0.82 | 0.88 | 0.80 | 0.85 vs 0.81 |
| K3 economy (baseline) | 10% | **0.91** | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.79 | 0.84 vs 0.79 |
| Relic Sites pay again (rev2) | 2% | **1.04** | 0.97 | 0.98 | 0.90 | 0.82 | 0.88 | 0.80 | 0.86 vs 0.81 |
| Relic Sites pay again (rev2) | 5% | **1.00** | 0.96 | 0.96 | 0.88 | 0.81 | 0.88 | 0.80 | 0.84 vs 0.80 |
| Relic Sites pay again (rev2) | 10% | **0.95** | 0.95 | 0.95 | 0.86 | 0.79 | 0.87 | 0.79 | 0.82 vs 0.78 |
| every holding emits 1/12 (rev2) | 2% | **0.94** | 0.97 | 0.98 | 0.91 | 0.84 | 0.88 | 0.80 | 0.92 vs 0.80 |
| every holding emits 1/12 (rev2) | 5% | **0.93** | 0.96 | 0.99 | 0.89 | 0.82 | 0.88 | 0.80 | 0.91 vs 0.79 |
| every holding emits 1/12 (rev2) | 10% | **0.91** | 0.95 | 0.98 | 0.88 | 0.81 | 0.87 | 0.79 | 0.90 vs 0.77 |
| holdings 2-3 do not emit | 2% | **0.96** | 0.97 | 0.95 | 0.90 | 0.83 | 0.88 | 0.80 | 0.80 vs 0.84 |
| holdings 2-3 do not emit | 5% | **0.94** | 0.96 | 0.94 | 0.89 | 0.82 | 0.88 | 0.80 | 0.79 vs 0.83 |
| holdings 2-3 do not emit | 10% | **0.92** | 0.95 | 0.93 | 0.87 | 0.80 | 0.87 | 0.79 | 0.77 vs 0.81 |
| 140 Works per USDC (rev2) | 2% | **0.97** | 1.03 | 0.97 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| 140 Works per USDC (rev2) | 5% | **0.95** | 1.02 | 0.96 | 0.89 | 0.82 | 0.87 | 0.80 | 0.85 vs 0.81 |
| 140 Works per USDC (rev2) | 10% | **0.93** | 0.99 | 0.95 | 0.87 | 0.80 | 0.86 | 0.79 | 0.83 vs 0.79 |
| Works cap 60/day | 2% | **0.95** | 0.97 | 0.96 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| Works cap 60/day | 5% | **0.94** | 0.97 | 0.95 | 0.89 | 0.82 | 0.88 | 0.80 | 0.85 vs 0.81 |
| Works cap 60/day | 10% | **0.91** | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.79 | 0.84 vs 0.79 |
| stake priced by days left (D3) | 2% | **0.95** | 0.97 | 0.97 | 0.90 | 0.83 | 0.88 | 0.80 | 0.92 vs 0.80 |
| stake priced by days left (D3) | 5% | **0.94** | 0.96 | 0.96 | 0.89 | 0.82 | 0.88 | 0.80 | 0.91 vs 0.79 |
| stake priced by days left (D3) | 10% | **0.91** | 0.95 | 0.96 | 0.87 | 0.80 | 0.87 | 0.79 | 0.89 vs 0.77 |
| Mandate reserve to every completer (M0) | 2% | **0.95** | 0.97 | 0.96 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.81 |
| Mandate reserve to every completer (M0) | 5% | **0.93** | 0.96 | 0.95 | 0.88 | 0.82 | 0.88 | 0.80 | 0.85 vs 0.80 |
| Mandate reserve to every completer (M0) | 10% | **0.91** | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.79 | 0.84 vs 0.79 |
| bots add their stake late (day 21) | 2% | **0.88** | 0.97 | 0.96 | 0.90 | 0.84 | 0.88 | 0.80 | 0.86 vs 0.82 |
| bots add their stake late (day 21) | 5% | **0.87** | 0.96 | 0.96 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| bots add their stake late (day 21) | 10% | **0.86** | 0.95 | 0.96 | 0.90 | 0.83 | 0.86 | 0.79 | 0.86 vs 0.82 |
| every staker adds its stake late (day 21) | 2% | **0.91** | 0.96 | 0.94 | 0.89 | 0.87 | 0.87 | 0.79 | 0.92 vs 0.85 |
| every staker adds its stake late (day 21) | 5% | **0.90** | 0.95 | 0.94 | 0.88 | 0.87 | 0.86 | 0.79 | 0.91 vs 0.84 |
| every staker adds its stake late (day 21) | 10% | **0.89** | 0.93 | 0.94 | 0.87 | 0.86 | 0.85 | 0.78 | 0.91 vs 0.83 |
| bots stand for office | 2% | **0.96** | 0.97 | 0.93 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| bots stand for office | 5% | **0.95** | 0.96 | 0.91 | 0.88 | 0.82 | 0.87 | 0.79 | 0.85 vs 0.81 |
| bots stand for office | 10% | **0.93** | 0.95 | 0.88 | 0.86 | 0.80 | 0.86 | 0.79 | 0.83 vs 0.79 |
| bots stand for office, no officer ceiling (rev2) | 2% | **1.39** | 2.22 | 0.97 | 0.90 | 0.83 | 0.88 | 0.80 | 0.86 vs 0.82 |
| bots stand for office, no officer ceiling (rev2) | 5% | **1.15** | 1.46 | 0.92 | 0.88 | 0.82 | 0.87 | 0.79 | 0.85 vs 0.81 |
| bots stand for office, no officer ceiling (rev2) | 10% | **1.02** | 1.24 | 0.89 | 0.86 | 0.80 | 0.86 | 0.79 | 0.83 vs 0.79 |

## K. The bot criterion, step by step (O4) 

SDK-default staking bots at 1, 2, 5 and 10% of wallets, 5 seeds per cell (seeds 201..), 10000 wallets; each economy is run with bots barred from office and with bots standing for office (the most engaged wallet wins, as in M0 §G). Steps are cumulative in the owner's order (O4); rows 6-9 back steps off to find the passing set that costs honest players least. The honest cells are the same seasons' multiples. "swept" is what the officer-pay ceiling (and the 5× cap) carries to the next season [sim].

| variant | bots in office | bot share | **bot + stake** | max seed | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual + stake | casual fee only | idle fee only | swept % of prize |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0. M0 economy (rev2) | no | 1% | **1.066** | 1.095 | 1.03 | 1.64 | 0.93 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 0.00% |
| 0. M0 economy (rev2) | no | 2% | **1.046** | 1.063 | 1.03 | 1.64 | 0.92 | 0.82 | 0.88 | 0.64 | 0.80 | 0.51 | 0.00% |
| 0. M0 economy (rev2) | no | 5% | **0.999** | 1.012 | 1.01 | 1.61 | 0.91 | 0.81 | 0.87 | 0.64 | 0.79 | 0.51 | 0.00% |
| 0. M0 economy (rev2) | no | 10% | **0.950** | 0.955 | 0.99 | 1.59 | 0.89 | 0.79 | 0.86 | 0.62 | 0.79 | 0.51 | 0.00% |
| 0. M0 economy (rev2) | yes | 1% | **1.785** | 1.828 | 2.93 | 1.07 | 0.91 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 0.00% |
| 0. M0 economy (rev2) | yes | 2% | **1.513** | 1.531 | 2.24 | 0.98 | 0.90 | 0.82 | 0.88 | 0.64 | 0.80 | 0.50 | 0.00% |
| 0. M0 economy (rev2) | yes | 5% | **1.213** | 1.240 | 1.58 | 0.92 | 0.87 | 0.80 | 0.87 | 0.64 | 0.79 | 0.50 | 0.00% |
| 0. M0 economy (rev2) | yes | 10% | **1.061** | 1.073 | 1.32 | 0.89 | 0.84 | 0.79 | 0.85 | 0.63 | 0.79 | 0.50 | 0.00% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | no | 1% | **1.068** | 1.082 | 1.04 | 1.02 | 0.91 | 0.83 | 0.88 | 0.63 | 0.80 | 0.51 | 1.82% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | no | 2% | **1.032** | 1.051 | 1.03 | 1.02 | 0.91 | 0.83 | 0.88 | 0.62 | 0.80 | 0.51 | 1.78% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | no | 5% | **0.993** | 1.006 | 1.01 | 1.00 | 0.89 | 0.81 | 0.87 | 0.62 | 0.79 | 0.51 | 1.71% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | no | 10% | **0.949** | 0.952 | 0.99 | 0.99 | 0.87 | 0.79 | 0.86 | 0.61 | 0.79 | 0.51 | 1.59% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | yes | 1% | **1.122** | 1.167 | 1.04 | 0.98 | 0.91 | 0.83 | 0.88 | 0.62 | 0.80 | 0.51 | 1.83% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | yes | 2% | **1.096** | 1.123 | 1.03 | 0.94 | 0.90 | 0.82 | 0.88 | 0.63 | 0.80 | 0.50 | 2.03% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | yes | 5% | **1.030** | 1.048 | 1.01 | 0.91 | 0.87 | 0.81 | 0.87 | 0.62 | 0.79 | 0.50 | 2.01% |
| 1. + O10 Mandate to staker completers + O3 ceiling 95% | yes | 10% | **0.974** | 0.979 | 0.98 | 0.88 | 0.85 | 0.79 | 0.85 | 0.61 | 0.79 | 0.50 | 1.81% |
| 2. step 1: + 70 Works per USDC | no | 1% | **1.021** | 1.034 | 0.92 | 1.01 | 0.90 | 0.84 | 0.89 | 0.63 | 0.80 | 0.51 | 1.77% |
| 2. step 1: + 70 Works per USDC | no | 2% | **0.986** | 1.005 | 0.92 | 1.01 | 0.90 | 0.83 | 0.89 | 0.63 | 0.80 | 0.51 | 1.73% |
| 2. step 1: + 70 Works per USDC | no | 5% | **0.952** | 0.965 | 0.91 | 0.99 | 0.88 | 0.82 | 0.89 | 0.62 | 0.80 | 0.51 | 1.68% |
| 2. step 1: + 70 Works per USDC | no | 10% | **0.916** | 0.920 | 0.91 | 0.98 | 0.87 | 0.80 | 0.88 | 0.62 | 0.80 | 0.51 | 1.57% |
| 2. step 1: + 70 Works per USDC | yes | 1% | **1.101** | 1.140 | 0.95 | 0.95 | 0.90 | 0.84 | 0.89 | 0.62 | 0.80 | 0.51 | 1.76% |
| 2. step 1: + 70 Works per USDC | yes | 2% | **1.073** | 1.099 | 0.94 | 0.91 | 0.89 | 0.83 | 0.89 | 0.63 | 0.80 | 0.50 | 1.93% |
| 2. step 1: + 70 Works per USDC | yes | 5% | **1.004** | 1.020 | 0.93 | 0.88 | 0.87 | 0.82 | 0.88 | 0.62 | 0.80 | 0.50 | 1.87% |
| 2. step 1: + 70 Works per USDC | yes | 10% | **0.949** | 0.954 | 0.92 | 0.86 | 0.85 | 0.80 | 0.87 | 0.62 | 0.79 | 0.50 | 1.68% |
| 2b. step 1 alt: 35 Works per USDC | no | 1% | **1.006** | 1.019 | 0.88 | 1.00 | 0.89 | 0.83 | 0.88 | 0.63 | 0.82 | 0.51 | 1.73% |
| 2b. step 1 alt: 35 Works per USDC | no | 2% | **0.971** | 0.990 | 0.88 | 1.00 | 0.88 | 0.83 | 0.88 | 0.63 | 0.82 | 0.51 | 1.69% |
| 2b. step 1 alt: 35 Works per USDC | no | 5% | **0.938** | 0.951 | 0.88 | 0.98 | 0.87 | 0.81 | 0.88 | 0.63 | 0.82 | 0.51 | 1.64% |
| 2b. step 1 alt: 35 Works per USDC | no | 10% | **0.903** | 0.907 | 0.87 | 0.98 | 0.86 | 0.80 | 0.87 | 0.62 | 0.81 | 0.51 | 1.54% |
| 2b. step 1 alt: 35 Works per USDC | yes | 1% | **1.095** | 1.135 | 0.94 | 0.94 | 0.88 | 0.83 | 0.88 | 0.63 | 0.82 | 0.51 | 1.72% |
| 2b. step 1 alt: 35 Works per USDC | yes | 2% | **1.066** | 1.092 | 0.94 | 0.89 | 0.87 | 0.82 | 0.88 | 0.64 | 0.82 | 0.50 | 1.89% |
| 2b. step 1 alt: 35 Works per USDC | yes | 5% | **0.995** | 1.011 | 0.92 | 0.87 | 0.86 | 0.81 | 0.87 | 0.63 | 0.81 | 0.50 | 1.82% |
| 2b. step 1 alt: 35 Works per USDC | yes | 10% | **0.939** | 0.944 | 0.90 | 0.85 | 0.84 | 0.80 | 0.87 | 0.62 | 0.81 | 0.50 | 1.61% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | no | 1% | **1.021** | 1.029 | 0.92 | 0.99 | 0.90 | 0.84 | 0.89 | 0.62 | 0.80 | 0.51 | 1.77% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | no | 2% | **0.987** | 1.002 | 0.92 | 1.00 | 0.90 | 0.83 | 0.89 | 0.62 | 0.80 | 0.51 | 1.72% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | no | 5% | **0.954** | 0.965 | 0.91 | 0.97 | 0.88 | 0.82 | 0.89 | 0.62 | 0.80 | 0.51 | 1.66% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | no | 10% | **0.917** | 0.920 | 0.91 | 0.97 | 0.87 | 0.80 | 0.88 | 0.61 | 0.80 | 0.51 | 1.55% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | yes | 1% | **1.088** | 1.130 | 0.95 | 0.95 | 0.90 | 0.84 | 0.89 | 0.62 | 0.80 | 0.51 | 1.76% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | yes | 2% | **1.063** | 1.086 | 0.94 | 0.91 | 0.89 | 0.83 | 0.89 | 0.62 | 0.80 | 0.50 | 1.93% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | yes | 5% | **0.998** | 1.013 | 0.93 | 0.88 | 0.87 | 0.81 | 0.88 | 0.62 | 0.80 | 0.50 | 1.90% |
| 3. step 2: + stake priced by accrual left (ramp 2.0) | yes | 10% | **0.945** | 0.949 | 0.92 | 0.86 | 0.85 | 0.80 | 0.88 | 0.61 | 0.79 | 0.50 | 1.71% |
| 4. step 3: + order-weighted emission | no | 1% | **1.060** | 1.069 | 0.92 | 0.98 | 0.90 | 0.83 | 0.89 | 0.63 | 0.80 | 0.51 | 1.84% |
| 4. step 3: + order-weighted emission | no | 2% | **1.022** | 1.054 | 0.92 | 0.99 | 0.89 | 0.82 | 0.89 | 0.63 | 0.80 | 0.51 | 1.78% |
| 4. step 3: + order-weighted emission | no | 5% | **0.981** | 0.987 | 0.91 | 0.95 | 0.87 | 0.81 | 0.89 | 0.63 | 0.80 | 0.51 | 1.72% |
| 4. step 3: + order-weighted emission | no | 10% | **0.931** | 0.934 | 0.91 | 0.94 | 0.86 | 0.80 | 0.88 | 0.62 | 0.80 | 0.51 | 1.65% |
| 4. step 3: + order-weighted emission | yes | 1% | **1.101** | 1.149 | 0.95 | 0.96 | 0.90 | 0.83 | 0.89 | 0.63 | 0.80 | 0.51 | 1.82% |
| 4. step 3: + order-weighted emission | yes | 2% | **1.066** | 1.083 | 0.94 | 0.93 | 0.88 | 0.82 | 0.89 | 0.63 | 0.80 | 0.50 | 2.02% |
| 4. step 3: + order-weighted emission | yes | 5% | **1.005** | 1.016 | 0.93 | 0.88 | 0.86 | 0.81 | 0.88 | 0.63 | 0.80 | 0.50 | 2.04% |
| 4. step 3: + order-weighted emission | yes | 10% | **0.949** | 0.952 | 0.92 | 0.86 | 0.84 | 0.79 | 0.88 | 0.62 | 0.79 | 0.50 | 1.88% |
| 5. step 4: + no Relic Site laurels | no | 1% | **0.928** | 0.939 | 0.92 | 0.95 | 0.90 | 0.84 | 0.89 | 0.64 | 0.80 | 0.51 | 1.87% |
| 5. step 4: + no Relic Site laurels | no | 2% | **0.925** | 0.931 | 0.92 | 0.95 | 0.89 | 0.83 | 0.89 | 0.64 | 0.80 | 0.51 | 1.85% |
| 5. step 4: + no Relic Site laurels | no | 5% | **0.916** | 0.920 | 0.91 | 0.94 | 0.88 | 0.82 | 0.89 | 0.64 | 0.80 | 0.51 | 1.75% |
| 5. step 4: + no Relic Site laurels | no | 10% | **0.895** | 0.898 | 0.91 | 0.94 | 0.87 | 0.81 | 0.88 | 0.64 | 0.80 | 0.51 | 1.67% |
| 5. step 4: + no Relic Site laurels | yes | 1% | **0.951** | 0.955 | 0.95 | 0.92 | 0.89 | 0.84 | 0.89 | 0.64 | 0.80 | 0.51 | 1.85% |
| 5. step 4: + no Relic Site laurels | yes | 2% | **0.949** | 0.950 | 0.94 | 0.91 | 0.89 | 0.83 | 0.89 | 0.64 | 0.80 | 0.50 | 2.06% |
| 5. step 4: + no Relic Site laurels | yes | 5% | **0.935** | 0.938 | 0.93 | 0.89 | 0.87 | 0.82 | 0.88 | 0.64 | 0.80 | 0.50 | 2.09% |
| 5. step 4: + no Relic Site laurels | yes | 10% | **0.913** | 0.916 | 0.92 | 0.87 | 0.85 | 0.81 | 0.88 | 0.63 | 0.79 | 0.50 | 1.94% |
| 6. back-off: steps 2-4 with 140 Works per USDC | no | 1% | **0.973** | 0.984 | 1.04 | 0.97 | 0.91 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 1.93% |
| 6. back-off: steps 2-4 with 140 Works per USDC | no | 2% | **0.969** | 0.975 | 1.03 | 0.97 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 1.90% |
| 6. back-off: steps 2-4 with 140 Works per USDC | no | 5% | **0.955** | 0.959 | 1.02 | 0.96 | 0.89 | 0.82 | 0.87 | 0.63 | 0.80 | 0.51 | 1.79% |
| 6. back-off: steps 2-4 with 140 Works per USDC | no | 10% | **0.927** | 0.930 | 0.99 | 0.95 | 0.86 | 0.80 | 0.86 | 0.63 | 0.79 | 0.51 | 1.69% |
| 6. back-off: steps 2-4 with 140 Works per USDC | yes | 1% | **0.978** | 0.982 | 1.04 | 0.95 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 1.92% |
| 6. back-off: steps 2-4 with 140 Works per USDC | yes | 2% | **0.973** | 0.977 | 1.03 | 0.94 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 2.16% |
| 6. back-off: steps 2-4 with 140 Works per USDC | yes | 5% | **0.962** | 0.964 | 1.01 | 0.91 | 0.87 | 0.81 | 0.87 | 0.63 | 0.79 | 0.50 | 2.22% |
| 6. back-off: steps 2-4 with 140 Works per USDC | yes | 10% | **0.938** | 0.941 | 0.98 | 0.89 | 0.85 | 0.80 | 0.85 | 0.63 | 0.79 | 0.50 | 2.07% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | no | 1% | **0.967** | 1.000 | 1.03 | 1.00 | 0.91 | 0.84 | 0.88 | 0.63 | 0.80 | 0.51 | 1.84% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | no | 2% | **0.959** | 0.978 | 1.03 | 1.00 | 0.91 | 0.83 | 0.88 | 0.63 | 0.80 | 0.51 | 1.82% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | no | 5% | **0.946** | 0.959 | 1.01 | 1.00 | 0.89 | 0.82 | 0.87 | 0.63 | 0.80 | 0.51 | 1.71% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | no | 10% | **0.922** | 0.929 | 0.99 | 0.99 | 0.87 | 0.80 | 0.86 | 0.62 | 0.79 | 0.51 | 1.63% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | yes | 1% | **1.007** | 1.023 | 1.04 | 0.96 | 0.90 | 0.84 | 0.88 | 0.63 | 0.80 | 0.51 | 1.84% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | yes | 2% | **1.002** | 1.015 | 1.03 | 0.93 | 0.90 | 0.83 | 0.88 | 0.63 | 0.80 | 0.50 | 2.05% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | yes | 5% | **0.979** | 0.987 | 1.01 | 0.91 | 0.88 | 0.82 | 0.87 | 0.63 | 0.79 | 0.50 | 2.03% |
| 7. back-off: step 4 alone (140, D3 stake, full emission) | yes | 10% | **0.948** | 0.954 | 0.98 | 0.89 | 0.86 | 0.80 | 0.85 | 0.62 | 0.79 | 0.50 | 1.85% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | no | 1% | **0.920** | 0.953 | 0.92 | 0.98 | 0.90 | 0.84 | 0.89 | 0.63 | 0.80 | 0.51 | 1.79% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | no | 2% | **0.913** | 0.932 | 0.92 | 0.98 | 0.90 | 0.84 | 0.89 | 0.64 | 0.80 | 0.51 | 1.77% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | no | 5% | **0.906** | 0.918 | 0.91 | 0.98 | 0.89 | 0.83 | 0.89 | 0.63 | 0.80 | 0.51 | 1.68% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | no | 10% | **0.890** | 0.897 | 0.91 | 0.98 | 0.87 | 0.81 | 0.88 | 0.62 | 0.80 | 0.51 | 1.61% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | yes | 1% | **0.985** | 0.995 | 0.95 | 0.93 | 0.89 | 0.84 | 0.89 | 0.63 | 0.80 | 0.51 | 1.78% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | yes | 2% | **0.979** | 0.990 | 0.94 | 0.90 | 0.89 | 0.84 | 0.89 | 0.63 | 0.80 | 0.50 | 1.95% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | yes | 5% | **0.952** | 0.959 | 0.93 | 0.88 | 0.88 | 0.82 | 0.88 | 0.63 | 0.80 | 0.50 | 1.90% |
| 8. back-off: 70 Works + step 4 (no ramp, full emission) | yes | 10% | **0.922** | 0.928 | 0.92 | 0.87 | 0.86 | 0.81 | 0.87 | 0.62 | 0.79 | 0.50 | 1.72% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | no | 1% | **0.949** | 0.960 | 0.97 | 0.96 | 0.91 | 0.84 | 0.88 | 0.64 | 0.80 | 0.51 | 1.92% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | no | 2% | **0.946** | 0.952 | 0.97 | 0.96 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.51 | 1.89% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | no | 5% | **0.935** | 0.939 | 0.96 | 0.95 | 0.89 | 0.82 | 0.88 | 0.63 | 0.80 | 0.51 | 1.79% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | no | 10% | **0.912** | 0.915 | 0.95 | 0.95 | 0.87 | 0.80 | 0.87 | 0.63 | 0.79 | 0.51 | 1.70% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | yes | 1% | **0.962** | 0.966 | 0.98 | 0.94 | 0.90 | 0.83 | 0.89 | 0.64 | 0.80 | 0.51 | 1.90% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | yes | 2% | **0.959** | 0.961 | 0.97 | 0.93 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 2.12% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | yes | 5% | **0.948** | 0.950 | 0.96 | 0.91 | 0.88 | 0.82 | 0.87 | 0.64 | 0.79 | 0.50 | 2.16% |
| 9. **K3 choice**: steps 2-4 with 105 Works per USDC | yes | 10% | **0.926** | 0.929 | 0.95 | 0.89 | 0.85 | 0.80 | 0.86 | 0.63 | 0.79 | 0.50 | 2.01% |

- 0. M0 economy (rev2), bots barred from office: worst pooled bot + stake 1.066 → **fails**
- 0. M0 economy (rev2), bots in office: worst pooled bot + stake 1.785 → **fails**
- 1. + O10 Mandate to staker completers + O3 ceiling 95%, bots barred from office: worst pooled bot + stake 1.068 → **fails**
- 1. + O10 Mandate to staker completers + O3 ceiling 95%, bots in office: worst pooled bot + stake 1.122 → **fails**
- 2. step 1: + 70 Works per USDC, bots barred from office: worst pooled bot + stake 1.021 → **fails**
- 2. step 1: + 70 Works per USDC, bots in office: worst pooled bot + stake 1.101 → **fails**
- 2b. step 1 alt: 35 Works per USDC, bots barred from office: worst pooled bot + stake 1.006 → **fails**
- 2b. step 1 alt: 35 Works per USDC, bots in office: worst pooled bot + stake 1.095 → **fails**
- 3. step 2: + stake priced by accrual left (ramp 2.0), bots barred from office: worst pooled bot + stake 1.021 → **fails**
- 3. step 2: + stake priced by accrual left (ramp 2.0), bots in office: worst pooled bot + stake 1.088 → **fails**
- 4. step 3: + order-weighted emission, bots barred from office: worst pooled bot + stake 1.060 → **fails**
- 4. step 3: + order-weighted emission, bots in office: worst pooled bot + stake 1.101 → **fails**
- 5. step 4: + no Relic Site laurels, bots barred from office: worst pooled bot + stake 0.928 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 5. step 4: + no Relic Site laurels, bots in office: worst pooled bot + stake 0.951 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 6. back-off: steps 2-4 with 140 Works per USDC, bots barred from office: worst pooled bot + stake 0.973 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 6. back-off: steps 2-4 with 140 Works per USDC, bots in office: worst pooled bot + stake 0.978 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 7. back-off: step 4 alone (140, D3 stake, full emission), bots barred from office: worst pooled bot + stake 0.967 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 7. back-off: step 4 alone (140, D3 stake, full emission), bots in office: worst pooled bot + stake 1.007 → **fails**
- 8. back-off: 70 Works + step 4 (no ramp, full emission), bots barred from office: worst pooled bot + stake 0.920 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 8. back-off: 70 Works + step 4 (no ramp, full emission), bots in office: worst pooled bot + stake 0.985 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 9. **K3 choice**: steps 2-4 with 105 Works per USDC, bots barred from office: worst pooled bot + stake 0.949 → **passes (< 1.0 at 1, 2, 5, 10%)**
- 9. **K3 choice**: steps 2-4 with 105 Works per USDC, bots in office: worst pooled bot + stake 0.962 → **passes (< 1.0 at 1, 2, 5, 10%)**

### K.1 Officer pay (O3): the alternatives on the K3 economy, bots standing for office

| variant | bots in office | bot share | **bot + stake** | max seed | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual + stake | casual fee only | idle fee only | swept % of prize |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a. no bound (rev2 rows) | yes | 1% | **1.625** | 1.644 | 2.88 | 1.03 | 0.92 | 0.84 | 0.89 | 0.64 | 0.80 | 0.51 | 0.00% |
| a. no bound (rev2 rows) | yes | 2% | **1.390** | 1.403 | 2.09 | 0.97 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 0.00% |
| a. no bound (rev2 rows) | yes | 5% | **1.144** | 1.152 | 1.50 | 0.91 | 0.88 | 0.82 | 0.87 | 0.64 | 0.79 | 0.50 | 0.00% |
| a. no bound (rev2 rows) | yes | 10% | **1.019** | 1.025 | 1.28 | 0.89 | 0.85 | 0.80 | 0.86 | 0.63 | 0.79 | 0.50 | 0.00% |
| b. rows capped at 25% of what the officer paid | yes | 1% | **1.164** | 1.176 | 1.22 | 1.01 | 0.92 | 0.84 | 0.90 | 0.65 | 0.81 | 0.52 | 0.00% |
| b. rows capped at 25% of what the officer paid | yes | 2% | **1.135** | 1.141 | 1.19 | 0.97 | 0.91 | 0.84 | 0.90 | 0.65 | 0.81 | 0.52 | 0.00% |
| b. rows capped at 25% of what the officer paid | yes | 5% | **1.059** | 1.063 | 1.11 | 0.92 | 0.89 | 0.82 | 0.89 | 0.64 | 0.81 | 0.51 | 0.00% |
| b. rows capped at 25% of what the officer paid | yes | 10% | **0.989** | 0.993 | 1.05 | 0.89 | 0.86 | 0.81 | 0.87 | 0.63 | 0.80 | 0.51 | 0.00% |
| c. laurels from the Mandate budget (Minister 2, Warden 1 shares) | yes | 1% | **1.110** | 1.112 | 1.00 | 0.97 | 0.91 | 0.84 | 0.91 | 0.65 | 0.82 | 0.52 | 0.00% |
| c. laurels from the Mandate budget (Minister 2, Warden 1 shares) | yes | 2% | **1.046** | 1.049 | 1.00 | 0.94 | 0.90 | 0.83 | 0.91 | 0.65 | 0.82 | 0.52 | 0.00% |
| c. laurels from the Mandate budget (Minister 2, Warden 1 shares) | yes | 5% | **0.982** | 0.987 | 0.99 | 0.91 | 0.88 | 0.82 | 0.90 | 0.64 | 0.82 | 0.52 | 0.00% |
| c. laurels from the Mandate budget (Minister 2, Warden 1 shares) | yes | 10% | **0.939** | 0.942 | 0.98 | 0.89 | 0.86 | 0.81 | 0.89 | 0.64 | 0.82 | 0.53 | 0.00% |
| d. ceiling 90% of what the wallet paid | yes | 1% | **0.951** | 0.955 | 0.98 | 0.93 | 0.90 | 0.83 | 0.89 | 0.64 | 0.80 | 0.51 | 1.97% |
| d. ceiling 90% of what the wallet paid | yes | 2% | **0.947** | 0.951 | 0.97 | 0.92 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 2.20% |
| d. ceiling 90% of what the wallet paid | yes | 5% | **0.936** | 0.938 | 0.96 | 0.90 | 0.88 | 0.82 | 0.87 | 0.64 | 0.79 | 0.50 | 2.29% |
| d. ceiling 90% of what the wallet paid | yes | 10% | **0.917** | 0.919 | 0.94 | 0.89 | 0.85 | 0.80 | 0.86 | 0.63 | 0.79 | 0.50 | 2.18% |
| e. **ceiling 95%** (K3 choice) | yes | 1% | **0.962** | 0.966 | 0.98 | 0.94 | 0.90 | 0.83 | 0.89 | 0.64 | 0.80 | 0.51 | 1.90% |
| e. **ceiling 95%** (K3 choice) | yes | 2% | **0.959** | 0.961 | 0.97 | 0.93 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 2.12% |
| e. **ceiling 95%** (K3 choice) | yes | 5% | **0.948** | 0.950 | 0.96 | 0.91 | 0.88 | 0.82 | 0.87 | 0.64 | 0.79 | 0.50 | 2.16% |
| e. **ceiling 95%** (K3 choice) | yes | 10% | **0.926** | 0.929 | 0.95 | 0.89 | 0.85 | 0.80 | 0.86 | 0.63 | 0.79 | 0.50 | 2.01% |
| f. ceiling 100% (break-even) | yes | 1% | **0.989** | 0.993 | 1.00 | 0.95 | 0.91 | 0.84 | 0.89 | 0.64 | 0.80 | 0.51 | 1.77% |
| f. ceiling 100% (break-even) | yes | 2% | **0.986** | 0.987 | 0.99 | 0.94 | 0.90 | 0.83 | 0.88 | 0.64 | 0.80 | 0.50 | 1.97% |
| f. ceiling 100% (break-even) | yes | 5% | **0.966** | 0.968 | 0.98 | 0.91 | 0.88 | 0.82 | 0.87 | 0.64 | 0.79 | 0.50 | 1.97% |
| f. ceiling 100% (break-even) | yes | 10% | **0.937** | 0.940 | 0.97 | 0.89 | 0.85 | 0.80 | 0.86 | 0.63 | 0.79 | 0.50 | 1.78% |

- a. no bound (rev2 rows), bots in office: worst pooled bot + stake 1.625 → **fails**
- b. rows capped at 25% of what the officer paid, bots in office: worst pooled bot + stake 1.164 → **fails**
- c. laurels from the Mandate budget (Minister 2, Warden 1 shares), bots in office: worst pooled bot + stake 1.110 → **fails**
- d. ceiling 90% of what the wallet paid, bots in office: worst pooled bot + stake 0.951 → **passes (< 1.0 at 1, 2, 5, 10%)**
- e. **ceiling 95%** (K3 choice), bots in office: worst pooled bot + stake 0.962 → **passes (< 1.0 at 1, 2, 5, 10%)**
- f. ceiling 100% (break-even), bots in office: worst pooled bot + stake 0.989 → **passes (< 1.0 at 1, 2, 5, 10%)**

## E1. Doctrines: draft table of design §4.1

600 seasons: every doctrine in every wedge (6 rotations × 100 seeds), 10000 wallets, equal expected sizes with each wallet's faction drawn at random (so faction composition varies as it would in a real season). "Win" = highest s_k at the Reckoning. CI band: 16.7% ± 2 points [sim].

| Doctrine | win rate | in band | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |
|---|---|---|---|---|---|---|---|---|---|
| A Wardens of Stone | 0.0% | **no** | 0.9798 | 0.0143 | 1.0005 | 0.9911 | 0.9300 | 0.9976 | 0.782 |
| B Tide | 0.0% | **no** | 0.9805 | 0.0143 | 1.0004 | 0.9929 | 0.9320 | 0.9969 | 0.782 |
| C Ember | 0.0% | **no** | 0.9822 | 0.0131 | 1.0019 | 0.9942 | 0.9333 | 0.9996 | 0.784 |
| D Verdant | 0.8% | **no** | 0.9986 | 0.0159 | 1.0021 | 1.0318 | 0.9414 | 1.0189 | 0.791 |
| E Lumen | 99.2% | **no** | 1.0802 | 0.0157 | 0.9988 | 0.9903 | 1.3362 | 0.9954 | 0.831 |
| F Iron | 0.0% | **no** | 0.9801 | 0.0144 | 0.9993 | 1.0006 | 0.9280 | 0.9927 | 0.782 |

0 of 6 doctrines in the band. Binomial standard error of one win rate at this sample: 1.5 points.

## E2. Doctrines: tuned proposal (no direct multipliers on scored facts)

600 seasons: every doctrine in every wedge (6 rotations × 100 seeds), 10000 wallets, equal expected sizes with each wallet's faction drawn at random (so faction composition varies as it would in a real season). "Win" = highest s_k at the Reckoning. CI band: 16.7% ± 2 points [sim].

| Doctrine | win rate | in band | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |
|---|---|---|---|---|---|---|---|---|---|
| A Wardens of Stone | 18.7% | yes | 1.0004 | 0.0150 | 1.0009 | 0.9996 | 0.9998 | 1.0014 | 0.792 |
| B Tide | 16.0% | yes | 1.0000 | 0.0142 | 0.9993 | 1.0003 | 1.0008 | 0.9995 | 0.792 |
| C Ember | 19.3% | **no** | 1.0027 | 0.0139 | 1.0028 | 1.0024 | 1.0029 | 1.0027 | 0.794 |
| D Verdant | 19.3% | **no** | 1.0019 | 0.0145 | 1.0004 | 1.0017 | 1.0024 | 1.0031 | 0.793 |
| E Lumen | 13.7% | **no** | 0.9989 | 0.0143 | 0.9996 | 0.9987 | 0.9985 | 0.9987 | 0.792 |
| F Iron | 13.0% | **no** | 0.9976 | 0.0148 | 1.0000 | 0.9983 | 0.9966 | 0.9955 | 0.790 |

2 of 6 doctrines in the band. Binomial standard error of one win rate at this sample: 1.5 points.

## F. Determinism

Seed 1 on the main thread: `dcb040cb5084f905`; seed 1 on a worker thread: `dcb040cb5084f905` (identical); seed 2: `4af88f21502c8b9c` (differs: true).

## Conservation over the whole suite

Every settlement (each season × each γ) ran all conservation checks: **1620 of 1620 passed every check**.

Suite wall time [measured]: 1629 s.
