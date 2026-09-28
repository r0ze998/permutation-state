#!/usr/bin/env bash
# The checks of the checks (M1 contract §8.5, Gate W5): for every check
# with a `mutate-<check>` feature, build the verifier with that check
# disabled and run the tamper suite. `tests/tamper.rs` asserts, per build,
# that the tamper classes of the disabled check now PASS and every other
# class still FAILS with its code. Exit 0 only if every build does.
#
# The suite over one run (`tamper_suite_on_the_program_recording`, W5-D)
# runs in every build too: exactly the classes of the disabled check are
# missed on the program's recording (or on VERIFY_RUN, a stack or nightly
# run saved with `frontier-verify … --save FILE`).
#
#   crates/verify/mutate.sh              (from anywhere)
#   MUTATE_PROFILE=--release (default) | MUTATE_PROFILE=" "   (debug)
#   MUTATE_CHECKS="v5 v7"                (a subset)
#   VERIFY_RUN=run.json.gz               (the suite over that run)
set -uo pipefail
cd "$(dirname "$0")/../.."
profile="${MUTATE_PROFILE:---release}"
checks="${MUTATE_CHECKS:-v1 v2 v3 v4 v5 v6 v7 v8 v9 v11 v12 v13}"
log="$(mktemp -t verify-mutate.XXXXXX)"
failed=""
for c in $checks; do
  echo "== mutate-$c"
  # shellcheck disable=SC2086
  if cargo test --locked $profile -p verify --features "mutate-$c" --test tamper -- --include-ignored tamper_ --nocapture >"$log" 2>&1; then
    grep -E "PASS with mutate-$c" "$log" || true
    grep -E "^test result" "$log"
  else
    grep -E "panicked|must (FAIL|PASS)|^test result" "$log" | head -20
    failed="$failed $c"
  fi
done
rm -f "$log"
if [ -n "$failed" ]; then
  echo "mutate: FAILED for:$failed"
  exit 1
fi
echo "mutate: every disabled check lets its tampers PASS; every other class still FAILS"
