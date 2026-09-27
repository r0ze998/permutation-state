# Frontier K3: doctrine balance (owner decision O5)

- **Date:** 2026-09-27
- **Code:** branch `frontier/K3-doctrines` at `264c898` (on `codex/frontier` `cea89be`), worktree `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-K3-doctrines`. One local commit. Nothing pushed, no servers, no network.
- **Machine:** Apple M4 Max, 16 threads, rustc 1.89.0. The machine was shared with other agents (load average 30–90), so the wall times are slower than M0's.

**Tags:** [measured] = a property of the code measured here (tests, run time, conservation, determinism). [sim] = simulator output: the rules-v10 kernels driven by assumed behaviour. [estimate] = an extrapolation.

## 1. Result

**All six doctrines are in the 16.7% ± 2-point band on two large, disjoint seed sets. Neither seed set was used for tuning [sim].**

| Run (10,000 wallets each) | seasons | A Wardens | B Tide | C Ember | D Verdant | E Lumen | F Iron | in band | largest gap |
|---|---|---|---|---|---|---|---|---|---|
| **Paired**, seeds 10001–10250 × 6 rotations | 1,500 | 17.9% | 15.4% | 16.5% | 16.6% | 16.4% | 17.1% | **6 / 6** | 1.3 pt |
| **Unpaired** (M0 layout), seeds 21000 + 7k + rotation, k = 1..300 | 1,800 | 16.7% | 16.4% | 16.7% | 17.0% | 17.0% | 16.2% | **6 / 6** | 0.4 pt |
| Unpaired (M0 layout, M0's exact seed set), k = 1..100 | 600 | 20.0% | 13.2% | 19.0% | 17.3% | 16.2% | 14.3% | 2 / 6 | 3.5 pt |

- **Mean undamped index relative to the mean over doctrines:**
  - Paired 1,500: A +0.083%, B −0.046%, C +0.013%, D −0.003%, E +0.001%, F −0.048% (SE ≈ 0.016).
  - Unpaired 1,800: A +0.081%, B −0.013%, C −0.029%, D +0.039%, E +0.001%, F −0.078% (SE ≈ 0.035).
  - For comparison: M0's proposal spread over 0.53 points (+0.29% to −0.31%), and the draft spread over 10 points (Lumen +7.96%) [sim].
- **Conservation:** 1,500 of 1,500 and 1,800 of 1,800 seasons passed every check [measured].
- **About the 600-season row.** It uses M0's seed layout and M0's seed set, so it answers the question "what would M0's §E table have shown?" Its binomial standard error is 1.5 points per doctrine, so the ± 2 band is only 1.3 SE wide. Even a perfectly balanced table puts all six in the band at that size only about 30% of the time [estimate: 0.82⁶]. Measured against its own SE, the row is somewhat unlucky (χ² ≈ 15 on 5 degrees of freedom). But the two larger runs show no systematic edge above 0.09% of index, and 0.09% of index is worth about 1.5 win points [sim]. A is the most consistent residual (+0.08% in both large runs; its win rate is 17.9% and 16.7%). If a later change pushes A up, the first knob to turn is A's wall discount (`wall_cost_bps`).
- **Recommendation:** state the criterion on 1,500 or more seasons, which gives a win-rate SE of 1.0 point or less. Paired seeds are the cheaper way to get there.

## 2. The table (rules v10, `permutation_rules::frontier::doctrine::DOCTRINES`)

**Rule (O5).** No doctrine multiplies a scored fact. The scored facts are production, science, the path weights, Engine pledges, settler cost and grants. The `Doctrine` struct has **no field** for any of them, so the program, the verifier and the simulator cannot apply one. A doctrine changes only how facts are earned: combat, time, costs that are not facts, logistics and siege rules.

| Doctrine | Tech bias | Unit variant | Civic power | Changed from the draft (§4.1) |
|---|---|---|---|---|
| A Wardens of Stone | walls −10% cost | Pikes: drilled in Brace, ×1.10 damage in Brace | heartland sieges against it need +12 bells *(not simulated)* | unchanged |
| B Tide | caravans ×0.8 time, Bourse fee 1% *(not simulated)* | light cavalry (Horseman) | +1 Waystone per City *(not simulated)* | none in the kernel. M0's sim added a march ×0.8 proxy for the Waystone; the kernel has no such knob, so B has no travel bonus |
| C Ember | **+5%** damage on the arrival bell (draft +10%) | shock infantry: drilled in Assault, **×1.05** (draft ×1.10) | war horn 24 bells *(not simulated: no War in the sim)* | both bonuses halved |
| D Verdant | **Foraging:** half the supply attrition (draft: +10% food and cheaper Hamlets, which are scored-fact and growth multipliers) | Rangers: drilled in Flank, **×1.05** | **Long supply lines:** supply range +1 province (draft: Frontier Grant doubled, a growth multiplier) | tech bias and civic power replaced; drill halved |
| E Lumen | **Cartography:** marches ×0.75 time (the M0 sim's proxy for the Engineers' roads; draft: +1 research focus slot) | Engineers: build roads ×2 *(not simulated directly)* | **Survey charters:** Explore reveals the adjacent tiles *(not simulated)* (draft: science pledges ×1.2 toward Knowledge) | tech bias and civic power replaced |
| F Iron | troop upkeep −10% (draft also had ore +10%) | heavy cavalry **fielded from the Horseman line** (draft: tier-2 Knight) | captured Free Cities keep their walls | ore bonus dropped; Knight changed to Horseman |

**Bounds** (checked by `Doctrine::validate`, `validate_table` and CI):
- combat multipliers between ×1.00 and ×1.15;
- time and cost discounts between ×0.5 and ×1 (road building between ×1 and ×2);
- heartland extra bells ≤ 24; war horn between 24 and 36 bells; supply bonus ≤ 1; Waystones ≤ 1; Bourse fee ≤ 2%;
- no drill in Hold;
- the unit variant must be a fighting host unit;
- every doctrine has a tech bias, a unit variant and a civic power, and no two doctrines are identical.

**Why F lost the Knight** [sim, 50 paired seeds per row, 300 seasons, SE ≈ 0.035–0.045 per Δ]:
- With Knights, F sat 0.17–0.30% of index below the mean in every run: F −0.22 with upkeep ×0.5, −0.17 with the variant's muster surcharge ×0.4, and −0.21 with that surcharge plus a weaker C.
- In the same runs C sat +0.09 to +0.17 above the mean, even with C's own bonuses cut.
- With F on the Horseman line (`F.unit=horseman`), all six doctrines fell within ±0.07%.
- I did not isolate the mechanism. Candidates:
  - Knights are tier 2: ×1.5 upkeep weight and a 10/6 muster surcharge.
  - The simulator sizes a host by strength per troop, so F sends small, strong hosts.
  - Barbarian camps are melee, which counters mounted ×1.5.
- If the design wants a Knight-based Iron, the simulator's host-sizing and Knight-cost assumptions must be revisited first. The M0 muster-surcharge knob was tried and removed, since it did not help.

**Mechanics the simulator does not have** (Bourse, caravans, Waystones, research tree, War horn, heartland sieges, Explore reveal): these parts of the table are balanced by argument only. They are kept small and inside the bounds. They must be re-checked when the mechanisms land (M1–M3).

## 3. Tuning trail [sim]

Every row below is 10,000 wallets × 50 seeds × 6 rotations (300 seasons), paired on seeds 1..50. Δ is the mean undamped index relative to the mean over doctrines, in % (SE 0.03–0.045). "kernel v1" is the first kernel table: M0's proposal with C and D drills at ×1.05, C's arrival bonus at ×1.05, D's foraging and supply range, and F upkeep ×0.8.

| Run | A | B | C | D | E | F | in band |
|---|---|---|---|---|---|---|---|
| draft (§4.1) | −2.03 | −1.91 | −1.93 | −0.15 | **+7.96** | −1.95 | 0 (E wins 99.7%) |
| M0 proposal, paired | +0.05 | +0.09 | +0.05 | +0.13 | −0.11 | −0.22 | 3 |
| kernel v1 | +0.03 | +0.06 | +0.17 | +0.15 | −0.10 | −0.30 | 3 |
| v1, C arrival ×1.00 | +0.03 | +0.10 | +0.13 | +0.08 | −0.11 | −0.22 | 2 |
| v1, D drill ×1.00 | +0.01 | +0.03 | +0.16 | +0.07 | −0.09 | −0.18 | 3 |
| v1, E march ×0.60 | +0.04 | +0.05 | +0.09 | +0.16 | −0.09 | −0.24 | 4 |
| v1, F upkeep ×0.50 | −0.02 | +0.08 | +0.15 | +0.14 | −0.13 | −0.22 | 4 |
| **v1, F on Horseman** | +0.07 | +0.00 | −0.06 | +0.03 | −0.01 | −0.03 | **6** |
| v1, A drill ×1.15 | +0.01 | +0.04 | +0.11 | +0.11 | −0.07 | −0.19 | 3 |
| v1, F muster surcharge ×0.4 | +0.01 | +0.10 | +0.05 | +0.13 | −0.12 | −0.17 | 3 |
| same + C ×1.025 / ×1.025 | +0.00 | +0.01 | +0.17 | +0.15 | −0.12 | −0.21 | 3 |
| **final: F on Horseman, upkeep ×0.9** | +0.07 | −0.00 | −0.04 | +0.03 | −0.03 | −0.03 | **6** |

The two muster-surcharge rows used a temporary `F.muster` override that was removed with the knob, so the final binary cannot replay them; their outputs are kept in `doctrine-runs/s2_*.md`. The final row is the kernel table. `run --agents 2000 --seed 3 --rotation 1` gives digest `5329991dff91c3af` from both the kernel table and the v1 + `F.unit=horseman,F.upkeep=9000` override, so they are the same table [measured].

**Things learned** [sim]:
- The drill and arrival magnitudes (×1.00–×1.15) move a doctrine by at most about 0.08%, which is within two standard errors of zero. In this model, the stance a doctrine is drilled in and its unit line matter far more than the size of the bonus.
- Doctrines interact with their wedge neighbours: in the ring, faction k's neighbours are always k ± 1. So one outlier (the Knight) also shifted C and E. The Latin square spreads every doctrine over every wedge but keeps the neighbour pairs fixed, as the real map does.

## 4. The harness

- **Paired Latin square** (`frontier-sim/src/balance.rs`). Each seed is played once per rotation, so each doctrine sits on each faction slot and wedge once per seed.
  - Paired (the default): the six rotations share the seed, and with it the wallets, archetypes and faction draws. Every season is distributed exactly as in an unpaired run, but population noise cancels between doctrines.
  - The Δ SE is 0.016% at 1,500 seasons paired, against 0.034% at 1,800 unpaired [sim].
  - `--unpaired` reproduces M0's layout (`1000 + 7k + rotation`).
- **Memory.** Each season's `Outcome` is reduced to a small summary as soon as it finishes, so thousands of 10k-wallet seasons fit in memory.
- **Reproduces M0 exactly.** The refactor that moved doctrines into the kernel leaves every existing table's output unchanged: seed 1, no doctrines, digest `c71a8a0eb7641b53` (equal to M0); the M0 set, seed 1009 rotation 2, digest `e67831de2333b9bb`; the draft, seed 1010 rotation 3, digest `8aac17db1e035169`. Each matches the pre-refactor binary [measured].
- **The M0 suite §E** now prints E1 (draft), E2 (M0 proposal) and E3 (kernel) through this harness. I did not re-run the full suite here: it takes more than 10 minutes on a loaded machine, and the runs above replace its §E.

## 5. CI gate

- **Where:** `.github/workflows/ci.yml` has a new job, `frontier-sim`, which runs `cargo fmt --check`, `cargo clippy --locked --release --all-targets -D warnings` and `cargo test --locked --release` in `frontier-sim/`.
- **Tests:**
  - `balance::tests::doctrine_balance_gate` runs the kernel table on 10,000 wallets × 12 paired seeds × 6 rotations (72 seasons). It asserts:
    - `validate_table`;
    - conservation in every season;
    - every doctrine's mean undamped index within **±0.3%** of the mean;
    - every win rate within 16.7 ± **15** points.
  - `doctrine_balance_gate_rejects_the_draft` is the negative control: the same harness must fail the draft table.
  - Both tests are `#[ignore]`d in debug builds, because they are release-only.
- **Measured locally:**
  - The gate passes: largest |Δ| 0.132% (SE 0.07–0.09), win rates 12.5–22.2%, 72 of 72 seasons conserved.
  - The draft fails: Lumen +7.6%.
  - The whole `cargo test --release` takes 55 s wall and 654 CPU-s on 16 threads [measured]. That is about 3–6 minutes on a 2–4-core GitHub runner [estimate].
- **Why these bounds:**
  - At 72 seasons, the win-rate SE is 4.4 points, so a ± 2-point win-rate gate would fail a balanced table most of the time.
  - The gate therefore bounds the mean index, which is the systematic quantity. Its SE is about 0.08%, and ±0.3% is about 3.5 SE.
  - Any doctrine edge of about 0.3% of index or more fails the gate. That is roughly 5 win points at 10k wallets [sim: 0.25% ≈ 4 points], so it catches every gross imbalance, from a multiplier on a scored fact to the Knight case.
  - The full ± 2-point claim rests on the large runs in §1. Re-run them (command below, about 15 minutes on 16 free threads) whenever the table or a combat or economy kernel changes.
- **Caveats:**
  - The simulator uses `f64`, `ln` and `exp` from the platform libm. The CI numbers may therefore differ slightly from these macOS/ARM numbers. The gate is statistical, so that is acceptable, but the digests above are local.
  - CI has never run on this branch. Nothing is pushed, and a push needs the owner's approval.
- **In the rules crate** (`cargo test -p permutation-rules`, which is already in CI's `program` job): `doctrines_are_asymmetric_bounded_and_never_multiply_a_scored_fact` checks the table's bounds and shape, that the drills cover the stance cycle once, the semantics of the drill and arrival bonuses, attrition, and heartland siege bells. It also checks that the bounds reject a ×1.30 drill, a Hold drill, a ×0.1 upkeep, twin doctrines and a doctrine with no traits.

## 6. Other kernel work in this branch

- **§9.4 quota fairness** (the fourth property-test family was missing in M0):
  - `clash::admit_arrival`, `apply_slot` and `quota_set`. A Reveal writes at most one ArrivalSlot. It fills a free slot, or displaces the lowest-ranked arrival (or the citizen's own smaller one), or it is refused.
  - The rank is troops first, then `slot_key = tie_key(sha256("frontier/slot" ‖ P ‖ Q ‖ b), host_id)`, which is known at Reveal. There is one arrival per citizen per province and bell.
  - Test `arrival_slots_are_the_four_largest_regardless_of_reveal_order`: 400 random cases with repeated citizens and tied troop counts, 8 reveal orders each. It checks that the final set equals `quota_set` (the four highest-ranked of each citizen's best arrival), that no reveal writes more than one slot, and that no citizen ever holds two slots. More than 500 displacements are exercised.
  - A mutation that displaces the first slot instead of the lowest makes the test fail [measured].
  - A second test covers the spy case (four 100-troop squatters are displaced by the faction's real hosts) and the one-per-citizen rules.
- **Seed-round margin** (M0 §4.2 item 6):
  - `clash::SEED_MARGIN_SECS = 60` (checked at compile time to be ≥ 60), `REVEAL_WINDOW_SECS = 600`, `BeaconClock` and `QUICKNET` (genesis 1692803367, period 3 s), `seed_round` and `reveal_open`.
  - S is defined as the first round scheduled **at or after** close + M (rounding up, not `round_at`'s rounding down). So `round_time(S) ≥ close + M` holds exactly.
  - Reveal closes on whichever comes first: the chain clock reaching the close, or any round ≥ S being on chain.
  - Test `the_seed_round_is_after_the_reveal_close_plus_the_margin`: 60,000 anchors over three clocks.
  - Open item: another workstream may be adding a beacon module. If so, these functions should move there; they live in `clash.rs` only because that is a file this task owns.

## 7. Commands

```sh
W=/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-K3-doctrines
cd $W && cargo test --locked --release -p permutation-rules        # 343 passed (338 before + 5 new) [measured]
cd $W && cargo test --locked -p permutation-rules                  # debug: 343 passed
cd $W && cargo clippy --locked -p permutation-rules --all-targets -- -D warnings && cargo fmt -p permutation-rules -- --check
cd $W/frontier-sim && cargo fmt --check && cargo clippy --release --all-targets -- -D warnings
cd $W/frontier-sim && cargo test --locked --release                # 5 passed incl. the doctrine gate, 55 s

# the balance runs of §1 and §3 (outputs in doctrine-runs/)
F=$W/frontier-sim/target/release/frontier-sim
$F doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate   # paired 1,500: 6/6, 911 s
$F doctrines --agents 10000 --seeds 300 --first-seed 20000 --set kernel --unpaired  # 1,800: 6/6, 673 s
$F doctrines --agents 10000 --seeds 100 --set kernel --unpaired                  # M0 layout 600: 2/6
$F doctrines --agents 10000 --seeds 50 --set draft                               # draft
$F doctrines --agents 10000 --seeds 50 --set m0                                  # M0 proposal, paired
$F doctrines --agents 10000 --seeds 50 --set kernel --dx "F.unit=horseman"       # a tuning row
$F doctrine-gate                                                                 # the CI harness
```

The full output of every run is in `doctrine-runs/` next to this file (`confirm_paired_250.md`, `confirm_unpaired_300.md`, `confirm_unpaired_100.md`, `draft_p50.md`, `m0_p50.md`, `k1_p50.md`, `s1_*.md`, `s2_*.md`, `v2a.md`).

## 8. Not done / for the owner

- **Design text.** §4.1's table should be replaced by §2 above in design revision 3: D's and E's tech biases and civic powers changed, and F's unit line changed.
- **The ± 2-point criterion should say "over ≥ 1,500 seasons"**, or CI should gate the mean index as it does now. At 600 seasons, even a balanced table misses the band most of the time.
- **Doctrines and faction size:** not re-checked. All runs use equal expected sizes; the herding runs use no doctrines.
- **Not re-run after this change:** the SBF build, LiteSVM, `permutation-server` goldens and the full M0 suite. v9 is untouched: only a new module and additions to `clash.rs` and the test file.
- **Links:**
  - Code: `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-K3-doctrines/permutation-rules/src/frontier/doctrine.rs`
  - `.../frontier-sim/src/balance.rs`
  - `.../.github/workflows/ci.yml`
  - `.../permutation-rules/tests/frontier_world.rs`
