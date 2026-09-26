#!/usr/bin/env bash
# Build the program and run the program tests against that build.
#   permutation-chain/svm-tests/run.sh [cargo test arguments]
# Environment: RELEASE_CHECK=1 (no test may wait for a fix), OLD_REF=<tag or
# commit> (the upgrade test starts on that tree's program), and the test
# variables of README.md (DLP_SO, SVM_HEAVY, PLAYED_TICKS, NEED_EVERY, MEMBERS).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
"$here/../../scripts/build-program.sh"
export PERMUTATION_CHAIN_SO="$here/../target/deploy/permutation_chain.so"
# Opt-in: a real previous release for the upgrade test. OLD_REF=<tag or commit>
# builds that tree's program into $here/target/old (needs the ref in the clone).
if [ -n "${OLD_REF:-}" ] && [ -z "${OLD_SO:-}" ]; then
  old="$here/target/old"
  rm -rf "$old"
  mkdir -p "$old/src"
  git -C "$here" archive "$OLD_REF" | tar -x -C "$old/src"
  if [ -f "$old/src/Cargo.toml" ]; then
    (cd "$old/src" && CARGO_TARGET_DIR="$old/target" cargo-build-sbf --manifest-path permutation-chain/Cargo.toml --sbf-out-dir "$old/target/deploy")
  else
    (cd "$old/src/permutation-chain" && CARGO_TARGET_DIR="$old/target" cargo-build-sbf --sbf-out-dir "$old/target/deploy")
  fi
  export OLD_SO="$old/target/deploy/permutation_chain.so"
fi
cd "$here" && cargo test --locked "$@"
