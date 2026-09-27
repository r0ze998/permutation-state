# M1 area: M0 closeout (week 1 of M1)

- **Date:** 2026-09-27. **Planning only:** no repo edits, no commits, no push, no servers, no devnet or mainnet transactions.
- **Baseline:** branch `codex/frontier` at `d95fa25` (worktree `.claude/worktrees/frontier-integ`), which is `bcc6382` plus the docs commit. The branch tracks `origin/codex/frontier` at `d95fa25`: the push the owner approved once on 2026-09-27 has happened.
- **Inputs:** `docs/frontier/m0/M0-FINAL.md`, which the task calls "§4". The remaining-work list is M0-FINAL **§5**, and the owner questions are **§6**. Also: DESIGN rev 3.1 §0–§13, §19 and §20 (§9.4 for the test families, §14 for D18–D25); SPIKE-SP-V2, SPIKE-SP-FEE; the lab code under `scratchpad/frontier/m0b/spikes/`.
- **Tags:** as in the design. [measured] means it was checked in code or run for this note. [estimate] means a guess.

---

## 0. Summary

1. **38 tasks (CL-01 … CL-38)** in 8 groups, each with its files, tests and an acceptance check. The groups:
   - kernel bounds (the §9.4 row), plus three §4.8 items that M0-FINAL §5 left out;
   - code-review minors;
   - design minors;
   - spike minors;
   - D18;
   - D22, D23 and D24;
   - CI;
   - the decisions log.
2. **The gate.** Before the new `permutation-frontier` program links a rules-v10 kernel in any instruction (**gate G0**), 17 tasks must land. They are listed in §2. Most are kernel bounds, because the program will feed untrusted account data (ClashInputs, Holdings, instruction args) straight into `clash::resolve_clash`, `Holding::apply` and `ProvinceCoord`.
3. **Three findings from checking the code at `d95fa25`** [measured]:
   - **The vigil change is not at a UTC midnight.** `siege::Vigil::request_change` takes effect at `now + 24 h`. Design §8.3 and §4.8 want "a UTC midnight ≥ 24 h later". The item is still open, although M0-FINAL §5 does not list it.
   - **The province caps are not recounted after the hex fair share.** Step 2a of `clash::resolve_clash` counts every arrival that entered, and step 2b can then bounce it, but the count is never rebuilt. Status: to be confirmed with a test.
   - **Scouts (0 attack, 0 defence) can still make a tile "contested".** In step 5 they enter `factions` and the strength sums. Status: to be confirmed.
4. **GitHub CI now runs**, because the branch was pushed [measured, `gh run view 36303403992`, 16:36 JST]:
   - "Browser source smoke test" **failed** on 2 legacy `permutation-state-prototype/civilization` tests. I reproduced this in a lab copy: `not ok 28` (the fifteen-minute settlement run) and `not ok 36` (the lens renders "eight building types", 9 ≠ 8). That code is not Frontier code.
   - "Gateway and web client" and "Solana receipt scaffold" passed.
   - "Rules, program, server" and "Frontier simulator" were still running.
   - A fix needs **a new push, which needs a new owner approval**.
5. **Decisions log.** Appendix A is ready to commit as `docs/frontier/DECISIONS.md`. It records N1, N2 (a), N3 (as a fact), N4, N5 and D23. D18, D22, D24 and the Season-1 sweep target stay open, each with a recommendation.

---

## 1. Gate definitions

| Gate | Meaning |
|---|---|
| **G0** | Must be merged on the M1 work branch **before the first `permutation-frontier` instruction that calls a rules-v10 kernel** (in practice before Join, Harvest, Muster, Depart and resolve are wired up, so days 1–3 of M1) |
| **G1(x)** | Must land before instruction *x* is built |
| **GX** | Before the M1 exit (the 7-day, 1,000-bot season and the verifier PASS/FAIL) |
| **G2** | Before the M2 money program (Claim, AddStake, DefencePool). It is written down now because it is a closeout item |
| **DOC** | A text-only change to DESIGN.md or the spike reports, with no code |

**Rules for every kernel task:**
- **A bound that only refuses invalid input** must leave every honest outcome unchanged. Acceptance: the existing `permutation-rules` tests pass (354 at `bcc6382`); the `frontier-sim suite` digest on the fixed seed set is **bit-identical**; the SP-V2 native digest equals the on-chain digest in a lab copy.
- **A task that changes outcomes** (CL-09, CL-10) must also re-run the doctrine proxy gate (30 paired seeds, all 3 controls rejected) and the bot criterion (`--best-response --gate`). It must report the change against the m0c numbers.

Where the tests go:
- new kernel tests in **`permutation-rules/tests/frontier_bounds.rs`** (new file), using the seeded xorshift pattern of `tests/frontier_world.rs`, since there is no proptest dependency;
- unit tests next to the code.

---

## 2. What must land before the program uses the kernels (G0)

CL-01, CL-02, CL-03, CL-04, CL-05, CL-06, CL-07, CL-08, CL-11, CL-12, CL-16, CL-19, CL-20, CL-21, CL-23, CL-24, CL-25.

Why each group is needed first:
- **Overflow and garbage-in** (CL-01 to CL-08, CL-16). Every one of these kernels gets on-chain data that an attacker can shape: gathered fighters, queued effects, coordinates in instruction data, multiplier fields. Frontier kernels run without overflow checks (the rules crate is excluded for CU, `Cargo.toml` comment on WP08). So a bad value wraps silently, or panics and fails the transaction. A panic is a permanent freeze for a province-bell whose ClashInputs holds that value.
- **Time and address primitives** (CL-19, CL-20, CL-21). The seed, ring, genesis and tlock rounds, the posture and roster cutoffs, and the with-seed grammar are consensus rules. The program, keeper, verifier and web must share one implementation from the first instruction.
- **Program rules** (CL-23, CL-24, CL-25): the reveal latch, the minimum tip and the genesis re-roll guard. They change instruction layouts: an extra read-only key in Reveal, a tip floor in Depart, an announcement account in CreateSeason. Adding them later means a layout migration.
- **CL-11 and CL-12** (`PayoutParams`, `CitizenRecord`) are money kernels. The M1 program does not call them in play. They are in G0 because `CreateSeason` stores and validates the season parameters from day 1, and the Citizen layout is fixed in M1.

The rest of the list is later:
- G1 or GX: CL-09, CL-10, CL-13, CL-14, CL-15, CL-17, CL-18, CL-22, CL-26–CL-33, CL-35–CL-38.
- G2: CL-12's settlement callers, CL-15, CL-34.

---

## 3. Tasks

Estimates are for one engineer who knows the kernels [estimate]. Total: about **5.5 working days**, plus the owner's inputs. That matches M0-FINAL's "5–6 days".

### 3.1 Kernel bounds (design §9.4 "Kernel bounds"; M0-FINAL §5 item 1; first-pass §4.8)

| ID | Task | Files | Tests | Acceptance | Gate | Est. |
|---|---|---|---|---|---|---|
| **CL-01** | `clash::validate` refuses:<br>- any fighter or garrison with `troops > MAX_HOST_TROOPS`;<br>- `stamina > STAMINA_CAP`;<br>- `dealt_bps` outside `[BPS_ONE, Doctrine::COMBAT_MAX_BPS]` (11,500);<br>- `retreat_bps > BPS_ONE × 10` (pick and pin a cap).<br>New `ClashError` variants: `TroopsAboveCap(id)`, `StaminaAboveCap(id)`, `BadMultiplier(id)`, `BadRetreat(id)`. | `permutation-rules/src/frontier/clash.rs` (fn `validate` at l. 421, `ClashError` l. 266) | `frontier_bounds.rs::clash_validate_refuses_out_of_range` (each field at cap is accepted, cap + 1 is refused, u32/u16 max is refused); `clash_bounds_do_not_change_honest_outcomes`: 300 random honest scenarios, digest equal to before | Refusals as listed. Honest digests are unchanged. On a lab copy of SP-V2, ResolveFromInputs is still ≤ 541k + 2k CU at the worst fill | **G0** | 0.25 d |
| **CL-02** | Walls, production and upkeep caps:<br>- constants `MAX_WALLS`, `MAX_PRODUCTION_PER_HOUR`, `MAX_UPKEEP_PER_HOUR` (pin from the building table's largest legal value × `MAX_HOLDINGS_PER_WALLET`, with headroom 2×);<br>- `Holding::enqueue` refuses an `Effect` whose delta would pass them;<br>- `apply` clamps as a second line;<br>- `walls_at` and `commit_walls` stay ≤ `MAX_WALLS`.<br>This closes first-pass §4.8: "one large Walls item makes a holding immune to sieges". | `holding.rs` (`Effect`, `enqueue` l. 495, `apply`, `walls_at` l. 399, `set_upkeep` l. 556) | `holding_effects_are_capped`: a seeded fuzz of 10,000 effect sequences; no field above its cap and no i64 overflow in `Accrual::settle` at `rate × 28 days`; the enqueue refusal is tested | No state above its cap. `siege::required_bells(MAX_WALLS, extra)` is finite and ≤ a pinned number | **G0** | 0.5 d |
| **CL-03** | `duplicate_cost` checked: return `Option<u64>` (or saturate at a pinned `MAX_COST`) and refuse `n > MAX_DUPLICATES`. It is `const fn` today, `base * (2 + k*k) / 2`, unchecked. | `holding.rs` l. 131, and its callers | `duplicate_cost_is_checked` (u64 max base, n = u32 max → None/refused); the existing `duplicate_rule` values are kept | No overflow path. The existing cost values are unchanged | **G0** | 0.1 d |
| **CL-04** | `ProvinceCoord` ring bound:<br>- a constructor `ProvinceCoord::checked(p, q, r_max) -> Result<_, OutOfBounds>` refusing `ring() > r_max` (≤ `R_MAX_HARD` 128);<br>- the program deserialises coordinates only through it;<br>- `index`/`from_index` refuse `i ≥ provinces_within(R_MAX_HARD)`;<br>- `ring()` must not overflow on `i32::MIN` input (compute in i64). | `geometry.rs` (l. 102–215) | extend `coordinates_are_bounded`: i32 extremes; ring 128 accepted, 129 refused; `from_index(index(p)) == p` for every province within 128 | No panic on any i32 pair. Round trip over all 49,537 provinces within ring 128 | **G0** | 0.25 d |
| **CL-05** | `Stamina::set` is monotone in the bell. Today it silently applies `value` at `b.max(self.bell)`, so a set for an earlier bell can overwrite a later spend. Change it to `set(b, v) -> Result<(), HostError::TimeReversed>`, refusing `b < self.bell`. Callers in the clash-apply path pass the resolved bell. | `host.rs` l. 108; callers in `host.rs`/`clash.rs` apply | `stamina_set_refuses_an_earlier_bell`; `stamina_spend_then_late_set` (the first-pass scenario) | The scenario is refused. The lag-invariance tests still pass | **G0** | 0.15 d |
| **CL-06** | Faction ids limited to `0..=5` plus `NEUTRAL` (6):<br>- `FACTION_LIMIT` stays 8 for array sizes;<br>- `validate` refuses faction 7;<br>- `Relations::set_peaceful` and `SiegeCheck` refuse ≥ 7.<br>One helper, `geometry::valid_faction(f, allow_neutral)`. | `clash.rs` l. 84, 121, 436, 456; `siege.rs` l. 258; `geometry.rs` | `faction_ids_are_limited` | 7 is refused everywhere a faction is read from data | **G0** | 0.1 d |
| **CL-07** | Economy conservation checks written as **bounds**, not identities, in `frontier-sim`'s settlement:<br>- `Σ claims + Σ swept + dust ≤ prize`;<br>- each claim ≤ 5 × paid;<br>- `Σ laurels credited ≤ Σ emitted`;<br>- Mandate `paid + burned ≤ deposited`.<br>Keep the identities as a second check. | `frontier-sim/src/settle.rs` (l. 184–260) | `a_small_season_conserves_money_and_laurels` gains **mutation controls**: pay 1 extra unit to one wallet → the bound fails; credit 1 laurel twice → the bound fails | Both controls fail. All the existing suite seeds pass | **G0** (the verifier v2 reuses these checks) | 0.5 d |
| **CL-08** | **Per-faction Ledger**:<br>- `payout::Ledger` becomes per faction (`FactionLedger { pot_citizen, pot_laurel, claimed, swept }`), plus a season total;<br>- `Ledger::record` refuses a claim that would take faction k past its own pots, even while the global total is fine;<br>- the simulator and (M2) `FinalizeFaction`/`Claim` use it. | `payout.rs` l. 584–620; `frontier-sim/src/settle.rs` | `ledger_is_per_faction`: an over-claim in faction 2, offset by an under-claim in faction 4, is refused | The refusal holds. The suite totals are unchanged | **G0** for the type (the verifier), G2 for Claim | 0.35 d |
| **CL-09** | **Vigil change at a UTC midnight** (found open [measured]): `request_change` takes effect at the first UTC midnight ≥ `now + VIGIL_NOTICE`. This removes the possible 16-hour vigil. | `siege.rs` l. 114 | `vigil_change_lands_on_a_midnight`; `no_vigil_window_exceeds_8h_across_a_change` (sweep `now` over one day at 60-s steps) | Every change lands at `ts % 86,400 == 0`. No covered stretch is longer than 8 h. **Outcome-changing:** re-run the doctrine proxy gate | G1(SetVigil, M3). Cheap, so do it in week 1 | 0.2 d |
| **CL-10** | **Recount the caps after the fair share, and neutralise scouts** (both to be confirmed first):<br>- (a) after step 2b, rebuild `per_faction`/`total` and re-admit bounced-by-cap arrivals in mass order, until the counts are stable;<br>- (b) units with zero attack and zero defence (Scout, Settler) are left out of `contested` and the field-holding strength ranking, and never advance a siege. | `clash.rs` steps 2a/2b (l. 652–716) and 5 (l. 837); `siege.rs` | First a **failing test** for each, `a_bounce_by_fair_share_frees_a_cap_slot` and `scouts_do_not_contest_a_tile`; then the fix. The order-independence test (300 × 7) is still green | If either test passes at `d95fa25`, the item closes as "not a bug" and the test stays as a regression test. Otherwise it is fixed, and the doctrine proxy gate plus the criterion are re-run with the deltas reported | **G1(ResolveFromInputs)** | 0.5 d |

### 3.2 Code-review minors (M0-FINAL §5 item 2)

| ID | Task | Files | Tests | Acceptance | Gate | Est. |
|---|---|---|---|---|---|---|
| **CL-11** | `PayoutParams::validate` refuses `office_ceiling_bps > 10_000`. Today it accepts up to `cap_multiple × 10,000`, which is 5× paid and would allow a profitable office. Keep `u32::MAX` ("off") only for `REV2` comparison runs: add `validate_for_season()`, which also refuses `u32::MAX`. CreateSeason calls the strict form. | `payout.rs` l. 325 | `office_ceiling_above_paid_is_refused` (10,000 ok, 10,001 refused, `u32::MAX` refused by the strict form, `REV3` ok) | As listed | **G0** (CreateSeason stores the params) | 0.1 d |
| **CL-12** | Remove the `CitizenRecord::weights()` compile-time Works-rate trap. It always uses `WORKS_PER_USDC` (105), whatever the season's `works_per_usdc` is. Delete `weights()`; every caller passes the season's rate through `weights_at(p.works_per_usdc)`. | `payout.rs` l. 172; callers in `payout.rs`, `frontier-sim` | a compile check (no `weights()` left); `weights_follow_the_season_rate` (140 vs 105 give different works weights, consistent across `transfer_counted` and settle) | No call site uses the constant. The suite digest is unchanged at 105 | **G0** (Citizen record layout); G2 (Claim) | 0.15 d |
| **CL-13** | Closed form for `EntrySchedule::accrual_left`. Today it loops up to 366 terms per Join/AddStake. With `j = last_join_day`, `R = ramp`, `S = season_days`: `Σ_{t=d}^{S-1} (jB + R·min(t, j))` = `(S−d)·jB + R·[Σ_{t=d}^{min(j,S)−1} t + j·(S − max(d, j))]`. Use u128 throughout. | `pools.rs` l. 115 | `accrual_left_closed_form_matches_the_loop`: every `day` in 0..=366 × a grid of (S ≤ 366, j < S, ramp ≤ 100,000); `laurel_stake` values unchanged for `SEASON1` | Equal on the whole grid. On SBF the CU is constant in `day` (lab copy) | G2 (AddStake/Join with money); done in week 1 | 0.25 d |
| **CL-14** | **Bound the product of combat multipliers.** `scale()` in step 3 multiplies `raw × damage_bps(stance) × dealt_bps` (and variant or drill factors via the combatant). Pin `MAX_DAMAGE_PRODUCT_BPS` (the stance max × `COMBAT_MAX_BPS` × the variant max, all in bps) with a `const _: () = assert!(…)` showing that `MAX_HOST_TROOPS × product ≤ u64::MAX`. `doctrine::validate_table` refuses any table whose product passes it. | `clash.rs` l. 786–792; `doctrine.rs` l. 209, 332, 380; `stance.rs` | `damage_product_cannot_overflow` (the extreme fighter at every cap, both halves); `validate_table_refuses_a_product_above_the_bound` | The const assert compiles. The negative table is refused | **G0** | 0.25 d |
| **CL-15** | **Mandate claim deadline, and its order against the Reckoning.** The rule:<br>- (1) every term's Mandate claims close at the **earlier of** term end + 1 term, or `T_end + 72 h` (the end of the banking window);<br>- (2) the last term's `CloseTerm` runs at `T_end`;<br>- (3) `SweepTerm` for every open term runs **before** `FinalizeFaction`;<br>- (4) the reserve balance left at the sweep joins the faction's **laurel pool general split**, like unbanked accruals (§8.4). It is not carried forward.<br>Kernel: `MandateTerm::claim` takes `now` and refuses after `claim_deadline`; `Reserve::final_sweep()` returns the balance for the laurel split. | `mandate.rs` (l. 154–260); DESIGN §4.2, §8.4, §8.10 (DOC part) | `mandate_claims_close_before_the_reckoning`; `final_sweep_conserves` (`deposited = paid + burned + final_sweep`) over 200 random 7-term histories | The conservation equation holds. A late claim is refused | G2 (the claim path); the kernel and text in week 1 | 0.35 d |
| **CL-16** | **`frontier-sim criterion --first-seed N`** and a held-out restatement of the criterion:<br>- `criterion_best` hard-codes `seed: 200 + k` (`suite.rs` l. 1202); make it `first_seed + k` with a default of 200;<br>- run `--best-response --gate` on a held-out set (e.g. 30001–30003) **with D23 on** (CL-31);<br>- publish both tables. | `frontier-sim/src/suite.rs` l. 1193–1210, `main.rs` l. 283 | `criterion_first_seed_changes_the_seeds` (a unit test on the job list) | Held-out worst cell < 1.0 at 1/2/5/10%, with the margin stated. The table goes into DESIGN §5.4 | G0 for the flag (CI uses it); GX for the table | 0.25 d + about 1 h of CPU |
| **CL-17** | **Where the officer-ceiling and 5× sweeps go in Season 1** (there is no successor season). Recommendation: a season parameter `SweepTarget`:<br>- `NextSeason { deadline }` escrows the swept USDC for a declared successor season;<br>- if no successor season is created by the deadline (default T_end + 90 days), the escrow is paid **pro rata to citizen fee paid** to that season's claimants. That is fee-proportional, so extra wallets cannot farm it.<br>Never to the operator. Owner confirmation needed (open question). | DESIGN §4.3, §5.4 (DOC); `payout.rs` (a `SweepTarget` enum and params validate); `frontier-sim/src/settle.rs` | `sweep_falls_back_to_fee_pro_rata` (the conservation bound from CL-07 holds; per-wallet cap still ≤ 5×) | Owner OK recorded in DECISIONS.md | G2 | 0.25 d |
| **CL-18** | `index::pow_frac`: M0 asked for a doc line or assert stating whether γ > 1 is intended. **Already resolved at `d95fa25`** [measured]: the doc says any `num` works, and `IndexParams::validate` bounds `num ≤ 3 den`, `den ≤ 20`. Close with one test, `pow_frac_accepts_gamma_above_one`. | `index.rs` l. 183 | the test | Closed | GX | 0.05 d |

### 3.3 Design minors not applied in 3.1 (M0-FINAL §5 item 3)

| ID | Task | Files | Tests | Acceptance | Gate | Est. |
|---|---|---|---|---|---|---|
| **CL-19** | **"The first round at or after."** DESIGN §0.5 item 5 and the §8.5 table write the bell, ring and genesis seeds as `round_at(…)`, which rounds down. The kernel already rounds up for bells (`clash::seed_round` → `first_round_from`).<br>- Text: replace every `round_at(x)` with "the first quicknet round scheduled at or after x".<br>- Kernel: add `ring_seed_round(clock, t_open)` and `genesis_seed_round(clock, t_create)`, both `first_round_from(t + 600 + SEED_MARGIN_SECS)`. | DESIGN §0.5, §8.5 (DOC); `clash.rs` l. 1124–1187 (or a new `frontier/beacon.rs` that moves the beacon clock out of `clash.rs`) | `seed_rounds_round_up` (all three draws, over 7,200 times × every drand phase, `round_time(r) ≥ t + 660` and `< t + 663`) | Text and kernel agree. The SP-V2 `seed_round_margin` vectors pass on the kernel fns | **G0** | 0.25 d |
| **CL-20** | **Pin `T(b)` and the cutoffs.**<br>- Define `bell_start(b) = genesis_ts + 600·b`, `bell_end(b) = bell_start(b+1)`, and `T(b)` = the first round scheduled at or after `bell_end(b)`.<br>- **The posture close and the roster freeze are wall-clock bell boundaries** (`bell_start(b)`), never derived from `T(b)`, so rounding cannot move them.<br>- Kernel `tlock_round(clock, genesis_ts, b)`. | DESIGN §2.1, §6.2, §6.3, §8.4 (DOC); `frontier/beacon.rs` | `tlock_round_is_at_or_after_bell_end` (`0 ≤ round_time(T(b)) − bell_end(b) < period`); `cutoffs_do_not_depend_on_round_phase` | As listed. The web client's seal code and the keeper use the kernel function (via wasm or a JS port with shared vectors) | **G0** (Depart, CommitPosture) | 0.25 d |
| **CL-21** | **Pin the with-seed grammar and length bounds** for every with-seed account. Port the SP-V2 `acct.rs` table (`an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa`: a 2-byte tag ‖ lowercase hex of the fixed-width fields) into a **pure** kernel module `frontier::addr` (seed strings only, no solana types). The program adds `create_with_seed(season_pda, seed, program_id)`. Longest seed: `ar` = 2 + 8 + 8 + 8 + 2 + 2 = 30 B (≤ 32). | new `permutation-rules/src/frontier/addr.rs`; DESIGN §8.2 (DOC table) | `seed_strings_fit_32_bytes` (at the extremes: i32::MIN/MAX coordinates, u32::MAX bell, u64::MAX host); `seed_strings_are_injective` (a random 1M-key sample per kind, no collision across kinds, because the tags are distinct and the widths fixed); vectors shared with the keeper and web | Every seed ≤ 32 B. No cross-kind collision. The vectors match SP-V2's `acct.rs` byte for byte | **G0** | 0.35 d |
| **CL-22** | **Minimum reveal tip, in priority terms.**<br>- Season parameter `min_reveal_priority_milli` (default 433, the SP-FEE default-tip priority).<br>- Depart and CommitPosture refuse `tip < ceil(p_min × (reveal_cu_limit + 1,336)) + 2,500 − base`, with the formula pinned from §8.7 and `reveal_cu_limit` taken from the **M1-measured full Reveal** (CL-35 input).<br>- The zero-tip "hope nobody reveals" option in §6.4 becomes "minimum tip".<br>- The relic tip (100,000 lamports) stays a separate minimum for Relic Sites (M3). | DESIGN §6.2, §6.4, §8.7 (DOC); `frontier/beacon.rs` or a new `frontier/fees.rs` (pure fn `min_tip_lamports`) | `min_tip_matches_priority` (at 16k/20k/26k CU: 0.433/0.35/0.27 reproduced) | The kernel fn reproduces the §8.7 priorities. Depart's refusal is tested at the program level (M1 Depart test) | G1(Depart); the value is final after the Reveal CU | 0.25 d |
| **CL-23** | **One-way reveal latch.** Reveal refuses once the province-bell's **ClashInputs exists** (any gather has run). Reveal gets ClashInputs as a read-only key; the check is an absence proof (about 0.5k CU). This makes "no Reveal after gather" structural, not just a consequence of `Clock ≥ A + W` (Solana's Clock is a stake-weighted estimate). | DESIGN §6.2, §6.3, §8.3 (the Reveal row gains a read key), §9.4 (a new row) | program: `reveal_refused_after_first_gather` (gather part 1 at A + W, then a Reveal with the Clock warped back to A + W − 1 → refused); a forged ClashInputs address → refused | Both refusals. The Reveal tx size is re-measured (+32 B) | **G0** (Reveal layout) | 0.15 d (text) + in the Reveal build |
| **CL-24** | **Operator re-roll guard on the genesis seed.** Without it, an operator can CreateSeason, see the genesis seed about 11 min later, Abort before joins, and create again. The guard:<br>- (1) `AnnounceSeason(id, params_hash, t_create_min)` must precede `CreateSeason` by ≥ 24 h, and fixes `genesis_round = first_round_from(t_create_min + 660)`, whatever the actual creation time;<br>- (2) the season id is single-use (the PDA stays with a tombstone after Abort);<br>- (3) a pre-join Abort after the genesis round is public **forfeits a creation bond** (default 1 SOL [design]) to the ProvinceFund of the next season, or burns it;<br>- (4) the verifier lists every announced season and its fate. | DESIGN §3.2, §8.5, §8.10 (DOC); the program's CreateSeason/Abort (M1 lifecycle subset) | program: `create_before_announcement_refused`; `genesis_round_fixed_by_announcement`; `season_id_single_use`; `pre_join_abort_forfeits_bond` | As listed | **G0** (the CreateSeason layout) | 0.25 d (text) + in the lifecycle build |
| **CL-25** | Mark the 0.4-s slot rows as [model] in DESIGN §8.7/§8.9 and the C4 tables, and note that the 250-ms slot rescale is unverified. | DESIGN (DOC) | — | Every 0.4-s row is tagged | G0 (doc hygiene before the M1 docs fork) | 0.05 d |

### 3.4 Spike minors (M0-FINAL §5 item 4; pool sizing is §3.5)

All lab work is done in copies under `scratchpad/frontier/m1/lab/<name>/`, each with its own `CARGO_TARGET_DIR`, and never on reserved ports.

| ID | Task | Where | Acceptance | Gate | Est. |
|---|---|---|---|---|---|
| **CL-26** | Report **1,200-s windows next to 600-s** in the C4 model and tables (D19 says a 1,200-s window doubles the attacker's cost), and **price relic clashes at the relic tip** (100,000 lamports → a higher keeper priority, hence a higher attacker price, for relic reveals). Produce `c4-model.txt` v3. | lab copy `m1/lab/c4-v3/` of `SP-FEE/driver` (the model script) and `frontier-sim c4 --relics` | v3 has 600/1,200 columns and a relic-tip row, with every PASS/FAIL restated on valuation (a) (N2) | GX (feeds CL-30) | 0.35 d |
| **CL-27** | Relabel SP-FEE's "Clock drift" figure as **confirmation lag** (it measured beacon → confirmation, not Clock-vs-drand skew). Mark the lookup-table 60-lock count as **[unverified]** (the v0 + LUT path never landed locally). Same wording in DESIGN §8.6/§8.7. | `docs/frontier/m0/SPIKE-SP-FEE.md` (correction header), DESIGN | Both labels changed | DOC | 0.1 d |
| **CL-28** | **Seed Join's wallet RNG.** The SP-V2 harness uses `Keypair::new()`, so Join's CU is not reproducible (14.4–25.0k). The M1 LiteSVM harness uses `Keypair::from_seed(sha256("join" ‖ i))`, and adds an **adversarial wallet**: search offline for a wallet whose Citizen PDA's canonical bump is ≤ 247 (≥ 8 failed bump attempts). | the M1 program's `svm-tests` (new crate, modelled on `permutation-chain/svm-tests`) | Join CU is identical across runs. The worst case over 1,000 seeded wallets plus the adversarial one stays under the Join budget (proposed 40k) | GX (the Join budget test) | 0.2 d |
| **CL-29** | **Run the 1,200-fill worst-case search on ResolveFromInputs**, which is now the primary path. SP-V2 ran it only on the hybrid ResolveClash (top 12 of 1,200). Re-screen with the native kernel after CL-01/CL-10/CL-14, take the top 24 by engagements **and** by field-holding work, and run them on SBF v2. Record CU, heap and tx bytes. | lab copy `m1/lab/rfi-search/` of SP-V2 `host/` and `program/`, with the **kernel at the CL-01…CL-14 head** | Worst ResolveFromInputs ≤ 650k budget and heap ≤ 32 KiB × 0.85 (27.8 KB), or a filed optimisation ticket for the M1 clash-optimisation area | **G1(ResolveFromInputs)**; must be re-run after the clash optimisation | 0.35 d |

### 3.5 D18: re-sizing the defence pool

**The problem** [review, M0-FINAL §5 item 4]:
- D18's "≤ 0.02 SOL per attacked bell" counts only the target side's reveals.
- A whole-block attack delays *every* low-bid Frontier write in the window: all reveals of every side, 16 PostAnchors, ≥ 16 PostSeeds, every gather and resolve.
- If keepers escalate all of them from the pool, the spend is about **1–2 SOL per war bell** [model, review]. My own rough check at 50k players [estimate]: about 300 resolves × about 1.1M lamports, plus about 1,500 gathers × about 60k, plus about 32 beacon posts × about 0.66M, plus about 1,900 reveals × ≤ 30k. That is ≈ 0.5 SOL at p = 2.0 before the peak multiplier.
- So 20 SOL buys 10–40 attacked bells, not about 1,000.

**The design fix (recommended).** Split writes by whether lateness **changes an outcome**:

| Class | Writes | Lateness effect | Escalation |
|---|---|---|---|
| **Window-closing** | Reveal, RevealPosture | routed (50%) or Disarray: an outcome change | pool-eligible up to P_def = 2.0 (as today) |
| **Delay-only** | PostAnchor (the window and seed move with A), PostSeed, GatherClash (no deadline), ResolveFromInputs, SettleTransit, ArchiveAnchors | the province or region waits; no outcome changes (§8.4) | keeper's own budget, capped at **P_delay = 0.5** [design proposal]; **never pool-eligible** |

With only window-closing writes eligible, the pool pays per attacked bell ≈ `R_bell × subsidy`:
- `R_bell` is the reveals plus revealed postures world-wide in that bell;
- the subsidy is ≤ 30k lamports;
- about 1,900 × 30k ≈ **0.06 SOL at 50k players**, up to about **0.2 SOL** at the p99 bell [estimate];
- so 20 SOL covers ≈ 100–300 whole-block-attacked bells, and each of those bells costs the attacker ≥ $26.5k [model].

| ID | Task | Files | Acceptance | Gate | Est. |
|---|---|---|---|---|---|
| **CL-30** | **D18 model v3.**<br>- Extend `frontier-sim c4` to output, per bell, the world-wide counts of reveals, posture reveals, gathers, resolves and beacon posts (p50/p99/max at 10k and 50k agents).<br>- Compute pool spend per attacked bell under (A) all critical writes eligible and (B) window-closing writes only, at P_def 2.0 and P_delay 0.5.<br>- Recommend the pool size and the per-bell cap. | lab copy `m1/lab/d18/` of `frontier-sim` (c4.rs) + the model script | A table with the spend per attacked bell (A/B × 10k/50k × p50/p99/max) and bells covered by 20 SOL. Recommendation: keep 20 SOL if (B) covers ≥ 100 p99 bells, otherwise size it to 100 p99 bells [design proposal] | before the M1 keeper escalation policy is final (GX); before M2 DefencePool (G2) | 0.5 d |
| **CL-31a** | Write the class split into the spec:<br>- DESIGN §6.4 defence 1 and 2 (the keeper SDK rules: window-closing writes to P_def, delay-only writes to P_delay);<br>- §8.8 (pool numbers from CL-30);<br>- §14 D18 (new default, owner to confirm);<br>- `ClaimDefence` eligibility lists only Reveal and RevealPosture;<br>- per-bell cap = `R_bell × max_subsidy`, from a folded count. | DESIGN (DOC); the keeper spec in the M1 keeper area | Text updated; D18 marked "re-sized, owner to confirm" | GX | 0.2 d |

Does the class split change the C4 verdict? No, as far as I can see. C4 prices the attacker's cost of keeping the target's **reveals** out, and those stay at P_def 2.0. Delay-only writes at 0.5 only let an attacker delay a region more cheaply, which A4 accepts as "only waits". CL-30 must confirm that the verifier's liveness report (§9.1 item 5) flags long delays.

### 3.6 D22, D23 and D24

| ID | Task | Files | Acceptance | Gate | Est. |
|---|---|---|---|---|---|
| **CL-31** | **D23, decided: at most one office-term per wallet per season** (Warden or Minister; a by-election term counts; a term cut short by recall counts; the caretaker first term (H2) does **not** count [design proposal, flagged]).<br>- (1) Simulator: `Config::office_term_limit` defaults to `Some(1)` in the K3/Season-1 preset (it is `None` today, `config.rs` l. 174), and every suite, criterion and c4 default uses it.<br>- (2) Kernel: a `GovernanceParams { office_terms_per_wallet: u8 = 1 }` with `validate()`, and a pure `may_stand(terms_used, params)`.<br>- (3) Program (M1): **reserve `office_terms_used: u8` in the 320-B Citizen layout now**, so M3 needs no migration.<br>- (4) **Vacancy rule** for small seasons: if no eligible candidate stands, the seat stays vacant for the term (no pay, no Assembly weight). That matters for the M1 playtest (50–200 people) and the 1,000-bot season.<br>- (5) DESIGN §4.3, §13 R8, §14 D23 → decided. | `frontier-sim/src/config.rs` l. 118/174, `sim.rs` l. 3136/3191; new `permutation-rules/src/frontier/office.rs` (or in `mandate.rs`); the M1 Citizen layout; DESIGN | Sim:<br>- re-run `criterion --best-response --gate` (seeds 201–203 and held-out, CL-16) with the limit;<br>- the worst cell stays < 1.0 (m0c: 0.967 worst with the limit, bots in office);<br>- the bots' office share at 1% is ≈ 9.5% (m0c).<br>Doctrine proxy gate re-run (office rotation touches Mandates). Kernel test `second_office_term_refused`. Citizen layout test: the field is present and zeroed on Join | G0 (Citizen layout); M3 for enforcement | 0.5 d |
| **CL-32** | **D22 stake ramp (still open).** Give the owner a measured choice:<br>- run `criterion --best-response --gate` and the honest-player table (late-staker loss by join window) at `--stake-ramp 20000` and `10000`, **with D23 on**;<br>- the m0c numbers (ramp 2.0: margin 2 points, late stakers −10 to −18; ramp 1.0: margin about 1.5 points) were measured without the term limit.<br>Recommendation rule [design proposal]: pick 1.0 if its worst cell is ≤ 0.985 with D23 on; otherwise keep 2.0. | `frontier-sim` (no code change beyond CL-16); a result file in `m1/lab/d22/` | A table and a recommendation; the owner decides before M2 (money) | G2 | 0.25 d + CPU |
| **CL-33** | **D24 Relic Site role (still open).** Keep the working default "Works and Dominion only, no laurels" for Season 1.<br>- Add a simulator variant `--relic-to-mandate` (the relic emission is paid into the holder faction's Mandate reserve, which pays staking completers under the share floor) and measure the criterion.<br>- The owner decides before the M3 Relic Site build.<br>- M1 impact: none on the program (no Relic Sites in M1). CL-26 prices relic clashes for C4. | `frontier-sim/src/config.rs`, `sim.rs` (relic path), `laurel.rs` (`relic_credit_at` rerouted) | The variant runs, the criterion result is recorded, and D24 stays open with the measured number | before the M3 relic work | 0.35 d (can slip to M3) |

### 3.7 CI (M0-FINAL §5 item 5; N3 happened)

| ID | Task | Acceptance | Gate |
|---|---|---|---|
| **CL-34** | Read the finished run `36303403992` (`gh run view --log-failed`). Record the job times of "Frontier simulator" and "Rules, program, server" against the 15–40 min estimate. Recording this is not a pass/fail condition. | Job results and durations recorded in DECISIONS.md or the CI notes | G0 (the program crate is added to CI) |
| **CL-35** | **The legacy prototype job is red** [measured, lab reproduction with node 20.19.4]: `civilization/*.test.mjs` tests 28 and 36 fail (a settlement-run assertion; a lens that expects 8 building types but gets 9). There are two options:<br>- (a) fix the two tests or the code in `permutation-state-prototype/civilization`;<br>- (b) scope the job to `main` or mark it `continue-on-error` on `codex/frontier`, since the Frontier does not use this code.<br>**Recommend (a) if the fix is a stale expected count, otherwise (b).** Either one needs a push, so owner approval comes first. | CI green on the next approved push | GX |
| **CL-36** | Add the new program to CI: `permutation-frontier` host tests, the SBF v2 build through a copy of `scripts/build-program.sh` (the ELF `e_flags == 2` check, `overflow-checks = true` for the program crate in the root `Cargo.toml` profile, and the build script checks the `.so` carries them), and the LiteSVM suite. **Toolchain note:** CI installs Agave 3.1.9, whose test-validator lacks BLS12-381 (R20). LiteSVM with the mainnet feature set is enough for CU tests; the 1,000-bot season needs Agave ≥ 4.0 locally. | CI job defined; runs green on the first approved push | G0 (the job definition lands with the crate) |

### 3.8 Decisions log

| ID | Task | Files | Acceptance | Gate |
|---|---|---|---|---|
| **CL-37** | Commit Appendix A as `docs/frontier/DECISIONS.md`. Link it from `docs/frontier/README.md`. Update DESIGN §14:<br>- D23 → decided;<br>- D25 → decided (a);<br>- a new "N" block;<br>- §12 M0 row (c) → "met in the model on valuation (a), unverified until the M1 Reveal CU and the M4 soak";<br>- close item 8 → closed by N4.<br>Add a short line to `SUMMARY.ja.md`. | `docs/frontier/{DECISIONS.md, README.md, DESIGN.md, SUMMARY.ja.md}` | A local commit on the work branch (allowed). The push waits for approval | G0 |
| **CL-38** | After CL-01…CL-16 land, write **M0-CLOSE.md** (a short addendum to M0-FINAL): each §5 item → task ID → commit → test name, and the M0 exit table re-graded. | `docs/frontier/m0/M0-CLOSE.md` | Every M0-FINAL §5 line maps to a closed task or a named later gate | end of week 1 |

---

## 4. Week-1 order

| Day | Tasks |
|---|---|
| 1 | CL-37 (the decisions log, so later commits cite it), CL-01, CL-03, CL-05, CL-06, CL-14, CL-18 |
| 2 | CL-02, CL-04, CL-19, CL-20, CL-21 (the G0 primitives the program's first instructions need) |
| 3 | CL-07, CL-08, CL-11, CL-12, CL-13, CL-16; start the CL-31 simulator default; CL-34 |
| 4 | CL-09, CL-10 (with gate re-runs), CL-15, CL-17 (text), CL-22–CL-25 (text) |
| 5 | CL-26, CL-27, CL-29, CL-30, CL-31a, CL-32 (CPU runs overnight), CL-35 triage |
| 6 (buffer) | CL-28 (with the M1 svm-tests crate), CL-33 (may slip to M3), CL-36, CL-38 |

Program work (the Join/Citizen skeleton, account init, with-seed helpers) runs in parallel from day 1. It must not link a kernel instruction before its G0 tasks merge.

---

## 5. Interfaces fixed here

| Kind | Name | Spec |
|---|---|---|
| kernel fn | `frontier::beacon::tlock_round(clock, genesis_ts, b) -> u64` | the first round with `round_time ≥ genesis_ts + 600(b+1)` |
| kernel fn | `frontier::beacon::{seed_round, ring_seed_round, genesis_seed_round}` | `first_round_from(t + 600 + SEED_MARGIN_SECS)`, with `t = A`, `t_open` or the announced `t_create_min` |
| kernel fn | `frontier::beacon::{bell_start, bell_end}` | wall-clock bell boundaries; the roster freeze and posture close use `bell_start(b)` |
| kernel mod | `frontier::addr` | `seed(kind, fields) -> ([u8; 32], len)`; tags `an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa`; lowercase hex of fixed-width big-endian fields; ≤ 32 B; injective |
| kernel fn | `ProvinceCoord::checked(p, q, r_max) -> Result<ProvinceCoord, OutOfBounds>` | ring ≤ r_max ≤ 128; the only way the program builds a coordinate from data |
| kernel fn | `clash::validate` (extended) | adds `TroopsAboveCap`, `StaminaAboveCap`, `BadMultiplier`, `BadRetreat`, and faction ≤ 6 |
| kernel const | `clash::MAX_DAMAGE_PRODUCT_BPS` | a const assert that `MAX_HOST_TROOPS × product` fits in u64; `validate_table` enforces it |
| kernel fn | `holding::duplicate_cost(base, n) -> Option<u64>` | checked |
| kernel fn | `host::Stamina::set(b, v) -> Result<(), HostError>` | refuses `b < self.bell` |
| kernel type | `payout::FactionLedger`, `payout::SeasonLedger` | per-faction pots; a claim beyond the faction's pot is refused |
| kernel fn | `PayoutParams::validate_for_season()` | `office_ceiling_bps ≤ 10,000`; `u32::MAX` refused |
| kernel fn | `MandateTerm::claim(now, …)`, `Reserve::final_sweep()` | claims close at `min(term_end + term, T_end + 72 h)`; the final sweep goes to the laurel general split |
| kernel type | `GovernanceParams { office_terms_per_wallet: u8 }`, `may_stand(terms_used, &params)` | D23 = 1 |
| kernel fn | `fees::min_tip_lamports(p_min_milli, cu_limit) -> u64` | from the §8.7 priority formula |
| account field | `Citizen.office_terms_used: u8` | reserved in M1's 320-B layout |
| program rule | Reveal takes a read-only ClashInputs key | refuses if that account exists (the one-way latch) |
| program ix | `AnnounceSeason(id, params_hash, t_create_min)` | ≥ 24 h before CreateSeason; fixes the genesis round; single-use id; creation bond |
| season param | `min_reveal_priority_milli` (433), `SweepTarget`, `office_terms_per_wallet` (1) | validated at CreateSeason |
| keeper rule | the write classes | window-closing writes escalate to P_def 2.0 (pool-eligible); delay-only writes escalate to P_delay 0.5 (never pool-eligible) |
| CLI | `frontier-sim criterion [--best-response] --first-seed N` | the seeds `N + k`; default 200 |

---

## 6. Risks

1. **CL-10 changes clash outcomes.** Recounting the caps and neutralising scouts can move the doctrine balance: a 0.25% index edge moves a win rate by about 4 points (R10). The proxy gate must pass, and the nightly 1,500-season run must be re-confirmed. If a doctrine falls out of the band, re-tuning costs about 1–2 days that the plan does not include.
2. **The kernel bounds add CU to a clash already at 633k of a 650k budget.** The checks are O(hosts), so under 2k [estimate]. But CL-29's 1,200-fill search on ResolveFromInputs may find a fill above 650k even before the bounds. The clash-optimisation area must own that result.
3. **D23 at small scale.** With one term per wallet, a 50–200-person playtest (or small factions in the 1,000-bot season) may not fill the Warden and Minister seats. Without the vacancy rule in CL-31 (4), governance stalls. The criterion with D23 was measured only at 10k wallets.
4. **D18 re-sizing depends on the claim that delay-only writes never change outcomes.** That holds for the kernels (lag invariance, green). The program-level lag test is an M1 gate. If it fails, delay-only writes must be pool-eligible again, and the pool is under-sized by about 5–10× [estimate].
5. **CI is red on legacy prototype code**, and any fix needs a new push approval. Until then GitHub CI cannot show M0's gates green, even if the Frontier jobs pass.
6. **The genesis re-roll guard (CL-24) adds a lifecycle instruction and a bond to M1.** Skipping it keeps a known operator-trust hole in the private playtest. That is acceptable only because the playtest has no money, and the DECISIONS log must say so if it is deferred.
7. **The minimum tip (CL-22) depends on the M1 full-Reveal CU.** Until the Reveal is measured, Depart's floor uses a placeholder (16k CU), which may be too low if the Reveal lands near 26k.

## 7. Open questions for the owner

1. **Sweep target in Season 1 (CL-17).** Should swept USDC (officer ceiling, 5× cap) be escrowed for a successor season, and fall back to fee-pro-rata to the same season's claimants after 90 days?
2. **D18 re-size (CL-30/31a).** Should the pool pay only window-closing writes (Reveal, RevealPosture), with delay-only writes capped at priority 0.5 from keepers' own budgets, and the pool kept at 20 SOL if that covers ≥ 100 p99 attacked bells?
3. **D22 (CL-32).** Ramp 2.0 or 1.0, decided on the D23-on re-run?
4. **D24 (CL-33).** Keep "no laurels, Works and Dominion only" for Season 1 unless the Mandate-reserve variant passes the criterion?
5. **D23 details.** Does the caretaker first term count toward the one-term limit (proposed: no)? Is a vacant seat acceptable in small seasons (proposed: yes)?
6. **CL-24.** Adopt the announcement plus creation bond (default 1 SOL) for the genesis re-roll guard, or defer it past the no-money playtest?
7. **CL-35.** Fix the two legacy prototype tests, or scope that job off `codex/frontier`? And approve the push that carries the fix.

---

## Appendix A: `docs/frontier/DECISIONS.md` (ready to commit)

```markdown
# The Sixfold Frontier: decisions log

Owner decisions after design revision 3.1. Earlier decisions (O1–O10,
D1–D17) are in DESIGN.md §14. Standing rule: every devnet step and every
push needs the owner's separate approval.

| # | Date | Decision | Consequence | Where applied |
|---|---|---|---|---|
| N1 | 2026-09-27 | Accept revision 3.1's restated M0 exit (c): C4 is tested with keeper bids above the tip from an operator/faction **defence pool, capped at scheduler priority 2.0**, paid from **≥ 150 rotating keeper fee payers**. The failure at the default tip is a recorded known result, not a blocker. | Peacetime reveals stay at the default tip; the pool pays only under attack. The ≥ 150-payer rule and the escalation schedule are part of the keeper spec. C4 is met in the model (12–540×) and stays unverified until the M1 full-Reveal CU and the M4 soak. | DESIGN §6.4, §8.6–8.8, §12 (M0 exit c), §14 O7 |
| N2 | 2026-09-27 | D25 decided: valuation **(a)**, $7 × every clash the targeted side takes part in (the most conservative). | Pass threshold: attack cost ≥ 10× that value at the p99 and max bells (p99 $1,372, max $1,456, $2,206 with relics [sim]). The pool configuration passes at ≥ 12× in the worst case [model]. | DESIGN §6.4, §14 D25 |
| N3 | 2026-09-27 | (For the record) One push of `codex/frontier` was approved and made (`d95fa25`); GitHub CI ran for the first time (run 36303403992). | Further pushes each need a new approval. | — |
| N4 | 2026-09-27 | SP-V2 `RESULTS.md` (with the correction headers on the v0 logs) replaces separate S-BEACON and S-TLOCK reports. | M0 close item 8 is closed. | DESIGN §12 (M0 row) |
| N5 | 2026-09-27 | Start M1 now; close the remaining M0 items in M1's first week. | M0 is closed by the closeout tasks (kernel bounds, review, design and spike minors, D18 re-size) before the program links the kernels; see `m0/M0-CLOSE.md` when written. | DESIGN §12 |
| D23 | 2026-09-27 | **At most one office-term per wallet per season** (was: no term limit). | Bots' share of office-terms at 1% bots falls from 61% to 9.5% [sim]; the bot criterion still passes (0.967 worst, bots in office) [sim]. It does not remove Mandate steering; the fixed Mandate menu and the share floor still do that. Simulator default, kernel parameter and a Citizen field follow; a vacancy rule covers small seasons. | DESIGN §4.3, §13 R8, §14 D23 |

Still open (working defaults): D18 defence-pool size (re-sizing in
progress: pool-eligible writes limited to reveals), D22 stake ramp (2.0),
D24 Relic Site role (Works and Dominion only), and the Season-1 sweep target.
```
