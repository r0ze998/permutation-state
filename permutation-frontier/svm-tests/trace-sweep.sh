#!/usr/bin/env bash
# Heap sweep (M1 contract §13.1: heap ≤ 28 KiB at every instruction's worst
# fill; wave-5 review of W5-A). Runs the whole svm suite with the release
# and test-beacon program replaced by their trace builds (`--features
# trace`, `--features "test-beacon trace"`), so every landed transaction
# reports its heap peak and the harness asserts it (`Chain::submit`: heap
# peak ≤ HEAP_GATE, else the test panics with "heap gate"). Prints the
# heap maximum per kind from the CU log and fails when a kind is missing.
#
#   permutation-frontier/svm-tests/trace-sweep.sh [log-file]
#
# The trace markers add CU, so the tests that assert a CU gate or the
# release build's markers fail on the trace builds by design; the sweep
# counts only "heap gate" panics as failures and lists the others.
set -uo pipefail
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
log="${1:-$here/target/trace-sweep.log}"
out="$here/target/trace-sweep.out"
mkdir -p "$here/target"
rm -f "$log"
if [ "${PSF_SKIP_BUILD:-0}" != 1 ]; then
  "$root/scripts/build-frontier.sh" --features trace >/dev/null || exit 1
  # the combined test-beacon + trace build (never deployable; not a
  # build-frontier.sh flavour: the release checks refuse its markers)
  (cd "$root" && cargo-build-sbf --manifest-path permutation-frontier/Cargo.toml \
     --tools-version v1.52 --arch v2 --features "test-beacon trace" \
     --sbf-out-dir permutation-frontier/target/deploy-test-beacon-trace -- --locked) >/dev/null 2>&1 || exit 1
fi
d="$root/permutation-frontier/target"
PSF_SKIP_BUILD=1 \
PSF_SO="$d/deploy-trace/permutation_frontier.so" \
PSF_SO_TEST_BEACON="$d/deploy-test-beacon-trace/permutation_frontier.so" \
PSF_CU_LOG="$log" \
  "$here/run.sh" --release --no-fail-fast >"$out" 2>&1
heap_fail=$(grep -c "heap gate:" "$out")
echo "trace sweep: $(grep -c '' "$log") transactions logged; $heap_fail heap-gate failures"
grep -E "^test .* FAILED$" "$out" | sed 's/^/  (CU or marker test on a trace build) /'
python3 - "$log" <<'EOF'
import sys
mx = {}
for line in open(sys.argv[1]):
    p = line.split()
    if len(p) == 7 and p[6] != "-":
        mx[p[1]] = max(mx.get(p[1], 0), int(p[6]))
for k in sorted(mx, key=lambda k: -mx[k]):
    print(f"  {k:20s} heap max {mx[k]:6d} B")
print(f"  {len(mx)} kinds covered")
EOF
[ "$heap_fail" -eq 0 ]
