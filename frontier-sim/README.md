# frontier-sim

Host balance simulator for the Sixfold Frontier (open-world design §12 M0):
a 28-day season of up to tens of thousands of agents whose clashes,
sieges, holdings, laurel reward indices, faction indices, pools and claims
all run through the rules-v10 kernels in `permutation_rules::frontier`.
Behaviour and in-game costs are assumptions, collected in `src/model.rs`.

A host binary only: its own workspace root, excluded from the root
workspace, so the program's lock and SBF build are untouched.

```sh
cargo test --release                                   # conservation, determinism, doctrine gate
cargo run --release -- run --agents 10000 --seed 1     # one season, full report
cargo run --release -- run --agents 10000 --sizes 3,1,1,1,1,1 --gamma 3/5
cargo run --release -- run --agents 10000 --doctrines  # rules-v10 doctrine table
cargo run --release -- suite --agents 10000 --seeds 5 --out RESULTS-suite.md
# doctrine balance: 6 rotations × K seeds, paired (the rotations of a seed share it)
cargo run --release -- doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate
cargo run --release -- doctrines --seeds 50 --dx "C.drill=10250,F.upkeep=8000"   # try overrides
cargo run --release -- doctrine-gate --controls        # the CI gate's harness (10k × 30 seeds × 6) and its negative controls
cargo run --release -- criterion --seeds 5                  # bot criterion (O4), 1/2/5/10% bots, with and without offices
cargo run --release -- criterion --seeds 5 --rev2-economy   # the same on the M0 economy
cargo run --release -- criterion --best-response --seeds 3 --gate   # the max over the bot's join window × stake × office
cargo run --release -- criterion --seeds 5 --bot-mandates 0.25      # bot officers steer Mandates (humans complete ×0.25)
cargo run --release -- c4 --agents 50000 --seeds 3 --out c4.md      # restated C4: per-bell participation, tail episodes,
                                                                    # counterfactual value of one attacked bell
```

The default economy is K3's (owner decisions O3, O4, O6, O10; see
`frontier/m0b/sim/ECONOMY.md`): officer pay may lift a claim to at most 95%
of what the wallet paid, 105 Works per USDC, the laurel stake priced by
expected accrual left (`EntrySchedule::SEASON1`), holdings emit by their
order factor, Relic Sites pay no laurels, the Mandate reserve pays only
staker completers through `frontier::mandate` with its share floor (the
divisor is at least half of the faction's active stakers; the unclaimable
part is burned), γ = 0.6.

The O5 doctrine band (16.7% ± 2 on ≥ 1,500 paired seasons) is checked by
`.github/workflows/doctrine-balance.yml` (nightly); the per-push CI test is a
180-season proxy on the mean index with three negative controls (see
`balance.rs`).

Options: `--bots SHARE`, `--bot-q Q`, `--bot-aggression A`, `--day0 SHARE`,
`--rotation K`, `--set kernel|draft|m0` (doctrine table), `--dx OVERRIDES`
(`model::apply_tweaks`), `--unpaired` (M0's seed layout),
`--emission full|first-only|order-weighted`,
`--relics`, `--no-relics`, `--works-cap N`, `--works-per-usdc N`,
`--stake-ramp BPS`, `--office-ceiling BPS|none`,
`--office-pay usdc|share:BPS|laurels`, `--mandate-all`, `--rev2-economy`,
`--bot-officers`, `--late-stake none|bots|stakers`, `--bot-join-days LO-HI`,
`--bot-mandates MULT`, `--office-term-limit N`, `--no-mandate-floor`,
`--first-seed N`, `--gate-index PCT`, `--verbose`.
`suite --only` takes a comma list of
`payout,variant,herding,bots,decompose,ladder,doctrines,determinism`.

| File | Contents |
|---|---|
| `src/model.rs` | archetypes, play profiles, placeholder costs (all assumptions); the doctrine tables (kernel, draft, M0) and overrides |
| `src/balance.rs` | the doctrine balance harness and its CI gate (`doctrine_balance_gate`) |
| `src/sim.rs` | the season loop: map growth, joins, sessions, economy, marches, clashes, sieges, relics, folds |
| `src/settle.rs` | Shade voiding, faction index, settlement, claims, conservation checks |
| `src/report.rs`, `src/suite.rs` | tables, the M0 measurement suite and the bot criterion (default mix and best response) |
| `src/c4.rs` | restated C4 from the simulator: per-bell participation, tail episodes, counterfactual attacks (`Config::attack`) |
