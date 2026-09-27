# frontier-sim

Host balance simulator for the Sixfold Frontier (open-world design §12 M0):
a 28-day season of up to tens of thousands of agents whose clashes,
sieges, holdings, laurel reward indices, faction indices, pools and claims
all run through the rules-v10 kernels in `permutation_rules::frontier`.
Behaviour and in-game costs are assumptions, collected in `src/model.rs`.

A host binary only: its own workspace root, excluded from the root
workspace, so the program's lock and SBF build are untouched.

```sh
cargo test --release                                   # conservation, determinism
cargo run --release -- run --agents 10000 --seed 1     # one season, full report
cargo run --release -- run --agents 10000 --sizes 3,1,1,1,1,1 --gamma 3/5
cargo run --release -- run --agents 10000 --doctrines  # draft doctrine table
cargo run --release -- suite --agents 10000 --seeds 5 --out RESULTS-suite.md
cargo run --release -- criterion --seeds 5                  # bot criterion (O4), 1/2/5/10% bots, with and without offices
cargo run --release -- criterion --seeds 5 --rev2-economy   # the same on the M0 economy
```

The default economy is K3's (owner decisions O3, O4, O6, O10; see
`frontier/m0b/sim/ECONOMY.md`): officer pay may lift a claim to at most 95%
of what the wallet paid, 105 Works per USDC, the laurel stake priced by
expected accrual left (`EntrySchedule::SEASON1`), holdings emit by their
order factor, Relic Sites pay no laurels, the Mandate reserve pays only
staker completers through `frontier::mandate`, γ = 0.6.

Options: `--bots SHARE`, `--bot-q Q`, `--bot-aggression A`, `--day0 SHARE`,
`--rotation K`, `--doctrines-tuned`, `--emission full|first-only|order-weighted`,
`--relics`, `--no-relics`, `--works-cap N`, `--works-per-usdc N`,
`--stake-ramp BPS`, `--office-ceiling BPS|none`,
`--office-pay usdc|share:BPS|laurels`, `--mandate-all`, `--rev2-economy`,
`--bot-officers`, `--late-stake none|bots|stakers`, `--verbose`.
`suite --only` takes a comma list of
`payout,variant,herding,bots,decompose,ladder,doctrines,determinism`.

| File | Contents |
|---|---|
| `src/model.rs` | archetypes, play profiles, placeholder costs, doctrine knobs (all assumptions) |
| `src/sim.rs` | the season loop: map growth, joins, sessions, economy, marches, clashes, sieges, relics, folds |
| `src/settle.rs` | Shade voiding, faction index, settlement, claims, conservation checks |
| `src/report.rs`, `src/suite.rs` | tables and the M0 measurement suite |
