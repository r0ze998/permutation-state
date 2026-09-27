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
```

Options: `--bots SHARE`, `--bot-q Q`, `--bot-aggression A`, `--day0 SHARE`,
`--rotation K`, `--doctrines-tuned`, `--emission full|first-only|order-weighted`,
`--no-relics`, `--works-cap N`, `--verbose`. `suite --only` takes a comma
list of `payout,variant,herding,bots,decompose,doctrines,determinism`.

| File | Contents |
|---|---|
| `src/model.rs` | archetypes, play profiles, placeholder costs, doctrine knobs (all assumptions) |
| `src/sim.rs` | the season loop: map growth, joins, sessions, economy, marches, clashes, sieges, relics, folds |
| `src/settle.rs` | Shade voiding, faction index, settlement, claims, conservation checks |
| `src/report.rs`, `src/suite.rs` | tables and the M0 measurement suite |
