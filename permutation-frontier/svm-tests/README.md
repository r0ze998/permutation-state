# permutation-frontier/svm-tests

Program-level tests of the Sixfold Frontier (M1 contract §3.5, §11 W2-B, §13):
the SBPF v2 build of `permutation-frontier` run in LiteSVM 0.16 with the
loaded-data accounting of SIMD-0186 enforced by the harness.

Own workspace, lock and toolchain (1.95.0; LiteSVM 0.16 does not build on
1.89). The program is tested as a binary; the crates linked are
`frontier-abi` (layouts, tags, errors, account lists, budgets), `fclient`
(builders, beacons and the test key, seals, fees) and `permutation-rules`
(native oracles).

## Running

```sh
permutation-frontier/svm-tests/run.sh                        # build everything, run everything
permutation-frontier/svm-tests/run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_   # Gate W2
PSF_SKIP_BUILD=1 PSF_SO=… PSF_SO_TEST_BEACON=… ./run.sh --release   # test existing builds
RELEASE_CHECK=1 ./run.sh --release -- g13_                   # G13 with no Pending coverage
```

`run.sh` builds the release `.so` with `scripts/build-frontier.sh` (W2-A),
the feature builds `test-beacon`, `trace` and `oracle` (never deployable;
`PSF_FEATURES` narrows the list) and the probe program, sets the variables
below and runs `cargo test --locked` with its arguments.

| Variable | What |
|---|---|
| `PSF_SO` | release binary (`permutation-frontier/target/deploy/permutation_frontier.so`) |
| `PSF_SO_TEST_BEACON` | `test-beacon` build (I-53): verifies rounds of the local test key |
| `PSF_SO_TRACE`, `PSF_SO_ORACLE` | `trace` (heap peaks for G1) and `oracle` (ResolveClash) builds |
| `PSF_PROBE_SO` | the probe (`target/deploy-probe/psf_probe.so`) |
| `PSF_PROGRAM_ID` | the id to deploy at (default: a fixed test key) |
| `RELEASE_CHECK=1` | G13: no `Pending` coverage, every program code asserted |
| `PSF_SKIP_BUILD=1` | do not build (use the variables or the default paths) |

Test names start with their gate (`g01_…` … `g14_…`).

## Layout

| Path | What | Owner (§11) |
|---|---|---|
| `src/chain.rs` | LiteSVM with mainnet rent, the program deployed under LoaderV3 at `--max-len`, client and ladder profiles, **SIMD-0186 enforcement** (fee charged on a load failure), accounts, Clock | W2-B |
| `src/fixtures/` | the 32 SP-V2 quicknet rounds, test-key rounds, S-TLOCK vectors, seals for every seal code | W2-B |
| `src/records.rs` | PS2 records, the event-chain check (`ChainWatch`) | W2-B |
| `src/budget.rs` | CU, heap (trace build), tx bytes, locks, loaded data; the §5.5 ceilings | W2-B |
| `src/wallets.rs` | seeded wallets (CL-28) | W2-B |
| `src/probe.rs`, `probe/` | the stand-alone probe program: SIMD-0186 control, the pre-funding regression, a CPI forwarder | W2-B |
| `src/world/mod.rs` | a season through the program's own instructions, crafted accounts | W2-B |
| `src/world/{land,holding,clash,transit}.rs` | area builders (stubs) | W3-A, W3-B, W4-A, W4-B |
| `src/ix/`, `src/cover/` `{season,beacon}` | W2-A's instructions | W2-B, then W4-B |
| `src/ix/`, `src/cover/` `{map,citizen}`, `{holding,host,reveal}`, `clash`, `{transit,defence}` | stubs | W3-A, W3-B, W4-A, W4-B |
| `tests/g01_loaded_limit.rs`, `g02_prefund.rs`, `g03_forgery.rs`, `g04_reveal_window.rs`, `g05_one_anchor.rs`, `g13_season_beacon.rs`, `season_records.rs`, `harness.rs`, `coverage.rs` | the wave-2 gates for W2-A's instructions, the harness's own proofs, the G13 guard | W2-B (W5-A completes G1–G13) |
| `tests/drill.rs`, `drill/validator-drill.sh`, `drill/drill-2026-09-27.txt` | the one-time I-45 validator drill and its record | W2-B |

## The I-45 validator drill

`drill/validator-drill.sh [out-dir]` starts `solana-test-validator 3.1.9` on
41080 (RPC), 41081 (pubsub), 41085 (gossip), 41086 (faucet) and 41100–41140,
deploys the probe padded to the release `.so`'s size (non-BLS: 3.1.9 has no
BLS12-381 syscalls) at two `--max-len`s, runs `tests/drill.rs` and stops the
validator. Recorded 2026-09-27 (`drill/drill-2026-09-27.txt`): base need =
Σ(64 + data) incl. the unlisted ProgramData (680,295 B for a 679,981-B
ProgramData); an absent account and the instructions sysvar count 0, a
pre-funded one 64, the System program 78 (its data is 14 B, LiteSVM stores
21 B); a failed load lands in a block and is charged the fee (5,000
lamports). `chain::loaded_size` implements exactly this and
`harness::g01_loaded_limit_control_*` pins it.
