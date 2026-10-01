# frontier-node

The off-chain workspace of Wylls, M1
"First Bell" (contract: `docs/frontier/m1/M1-CONTRACT.md` v1.1, §8). It is its
own cargo workspace with its own lock, toolchain (1.95.0: LiteSVM 0.16 does
not build on 1.89) and build directory (`frontier-node/target`).

## Crates

| Crate | Binary / lib | State after wave 1 (W1-F) | Next owner |
|---|---|---|---|
| `fclient` | lib | **complete** for §8.1: ABI glue, addresses, a builder per instruction, decoders, PS2 log parser and head chains, fees incl. `L(kind)`, budgets, transactions, loopback HTTP + JSON-RPC, `ChainPort` / `DrandPort`, beacons (blstrs verify, SP-V2 hints, test key), seals (`tlock =0.0.10`), `GameClock`, payer pools; writes `permutation-gateway/test/frontier-vectors.json` | W2-F |
| `localnet` | `frontier-localnet`, lib | **MVP**: LiteSVM with the BLS12-381 syscalls, real 400-ms slots with scaled game seconds, SIMD-0186 loaded-data enforcement with the fee charged, priority block builder with the 100M / 40M caps, feed, RPC subset, `ChainPort::InProcess` | W2-C |
| `drand-replay` | `drand-replay`, lib | **MVP**: drand HTTP API over the SP-V2 fixture rounds or `--test-key`, gated by the game clock | W2-C |
| `findex` | lib | skeleton (feed pull, PS2 split) | W2-F |
| `keeper` | `frontier-keeper`, lib `keeper_core` | skeleton (bid schedule) | W2-F |
| `herald` | `frontier-herald`, lib `herald_fold` | skeleton (overview header) | wave 3 |
| `verify` | `frontier-verify`, lib `verify_core` | skeleton (report schema, exit codes) | wave 4 |
| `agents` | lib `frontier_agents` | skeleton (adversarial personas) | wave 3 |
| `bots` | `frontier-bots` | skeleton | wave 3 |
| `stack` | `frontier-stack` | skeleton (`check-ports`) | wave 5 |
| `itest` | tests | skeleton (in-process smoke) | wave 4 |

## Build and test

```sh
cd frontier-node
cargo test --locked --workspace                 # everything (≈ 5 s of real-time slot tests)
cargo test --locked -p localnet                 # the loaded-data control, 400-ms slots, RPC subset
cargo test --locked -p drand-replay             # fixture rounds, --test-key, gating on a live localnet clock
cargo test --locked -p fclient                  # also rewrites permutation-gateway/test/frontier-vectors.json
FRONTIER_VECTORS_CHECK=1 cargo test --locked -p fclient writes_frontier_vectors   # fails if the file was stale
# The SP-V2 BLS program on localnet (needs the lab build, not in the repo):
PSF_SPV2_SO=<scratch>/frontier/m0b/spikes/SP-V2/program/out/plain-v2/spv2.so cargo test -p localnet -- --ignored
```

`cargo fmt` and `cargo clippy` need the `rustfmt` and `clippy` components of
1.95.0, which are not installed on this machine (see the W1-F notes);
`cargo +1.89.0 fmt --all -- --check` works, and `cargo +1.89.0 clippy
--ignore-rust-version` works for every crate that does not link LiteSVM.

## Running the MVP services

Ports: 41000–41999 only, never a reserved port (4185, 4190, 4191, 4194,
18899, 17799, 28899, 27799, 26699, 5185, 5191); tests bind `127.0.0.1:0`.

```sh
# Local chain at 20× (8 game seconds per 400-ms slot), a program deployed with
# ProgramData max_len = round_up(1.25 × .so, 4 KiB) and an upgrade authority:
cargo run --release -p localnet -- --port 41010 --scale 20 \
    --program <PROGRAM_ID>=<path/to/permutation_frontier.so>@<AUTHORITY>
# Beacons gated by that chain's Clock, signed by the test key (test-beacon builds only):
cargo run --release -p drand-replay -- --port 41020 --test-key --clock chain:http://127.0.0.1:41010
# …or the recorded quicknet fixture rounds:
cargo run --release -p drand-replay -- --port 41020 --archive crates/fclient/fixtures/quicknet
```

Stop both with Ctrl-C.

## Notes for the next units

- `fclient::abi`, and the kernel twins in `addr`, `fees`, `seal` and
  `clock`, were written against the contract's text because `frontier-abi`
  (W1-E) and the new `permutation-rules::frontier` modules (W1-C) were built
  in parallel. W2-F switches them to re-exports and calls; every test here
  must stay green across the switch.
- The PS2 parser finds the tail from the end and refuses an ambiguous body;
  real traffic needs the per-kind lengths of `frontier-abi::log` passed as
  `BodyLens` (W2-F).
- `localnet` enforces SIMD-0186 itself: LiteSVM 0.16 does not count the
  ProgramData and charges nothing on a load failure
  (`litesvm_alone_undercounts_programdata`).
