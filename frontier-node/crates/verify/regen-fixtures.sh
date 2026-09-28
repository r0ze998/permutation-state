#!/usr/bin/env bash
# Regenerates the verifier's recorded fixtures (M1 contract §11 W5-D:
# "fixtures regenerated from the nightly") from the current program, then
# checks that every one PASSES and that T1–T22 FAIL on them.
#
#   crates/verify/regen-fixtures.sh                (from anywhere)
#
# - land-program.json      tests/record.rs::record_land_program: the keeper's
#                          land duties on the test-beacon .so, in process;
# - march-synth.json       tests/record.rs::record_march_synth: the
#                          synthetic march season (the verifier's own
#                          regression);
# - march-program.json.gz  itest::inproc_day with VERIFY_DUMP: 100 bots over
#                          one game day and the keeper drain, test key — the
#                          nightly's shape (scripts/m1-nightly.sh: test key,
#                          100 bots × 1 game day), in process.
#
# A stack or nightly run (W5-B's frontier-stack) is saved as a fourth
# fixture when NIGHTLY_ARGS names its source, e.g.
#   NIGHTLY_ARGS="--program <id> --season <n> --rpc http://127.0.0.1:<port> --localnet"
# (or "--archive <run>/findex --finals <file>"): the run is verified with
# the test key, saved to fixtures/verify/nightly.json.gz only if it PASSES,
# and the tamper suite must detect every class on it.
#
# PSF_FRONTIER_SO names the test-beacon .so (default: built by
# scripts/build-frontier.sh --features test-beacon).
set -euo pipefail
cd "$(dirname "$0")/../.."
root="$(cd .. && pwd)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
so="${PSF_FRONTIER_SO:-}"
if [ -z "$so" ]; then
  (cd "$root" && scripts/build-frontier.sh --features test-beacon) | tail -8
  so="$root/permutation-frontier/target/deploy-test-beacon/permutation_frontier.so"
fi
echo "== program: $so ($(shasum -a 256 "$so" | cut -c1-16)…)"
fx="$PWD/fixtures/verify"

echo "== land-program.json"
PSF_FRONTIER_SO="$so" cargo test --locked --release -p verify --test record -- \
  --ignored record_land_program --exact --nocapture 2>&1 | grep -E "wrote|test result"
echo "== march-synth.json"
cargo test --locked --release -p verify --test record -- \
  --ignored record_march_synth --exact --nocapture 2>&1 | grep -E "wrote|test result"
echo "== march-program.json.gz (itest::inproc_day, strict, 100 bots × 144 bells)"
PSF_FRONTIER_SO="$so" ITEST_NO_BUILD=1 VERIFY_DUMP="$fx/march-program.json.gz" \
  cargo test --locked --release -p itest --test inproc_day -- \
  --include-ignored inproc_day --exact --nocapture 2>&1 | grep -E "^inproc_day|^  (PASS|FAIL)|test result"

cargo build --locked --release -p verify
v="${CARGO_TARGET_DIR:-$PWD/target}/release/frontier-verify"
if [ -n "${NIGHTLY_ARGS:-}" ]; then
  echo "== nightly.json.gz ($NIGHTLY_ARGS)"
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2086
  $v $NIGHTLY_ARGS --test-key --save "$tmp/nightly.json.gz" --out "$tmp/report" >/dev/null
  $v tamper --fixture "$tmp/nightly.json.gz" --test-key --out "$tmp/tamper" >/dev/null
  cp "$tmp/nightly.json.gz" "$fx/nightly.json.gz"
  rm -rf "$tmp"
fi

echo "== the fixtures PASS, T1–T22 FAIL on them"
cargo test --locked --release -p verify
for f in "$fx"/march-program.json.gz ${NIGHTLY_ARGS:+"$fx"/nightly.json.gz}; do
  tmp="$(mktemp -d)"
  $v tamper --fixture "$f" --test-key --out "$tmp" >/dev/null
  echo "$(basename "$f"): $(grep -m1 'Required classes' "$tmp/tamper.md")"
  rm -rf "$tmp"
done
echo "regen-fixtures: done (commit fixtures/verify/*)"
