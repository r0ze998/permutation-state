#!/usr/bin/env bash
# M1 nightly smoke (contract §11 W5-B, §12 Gate W6: three consecutive nights
# green). Test key, 100 bots x 1 game day at 100x, the second stack instance
# on ports 41500-41599 (frontier-node/configs/nightly.toml, with the
# adversary schedule), then verify, tamper, a 5,000-viewer load for one game
# hour, the run report, and down (always, also on failure).
#
#   scripts/m1-nightly.sh [--no-build] [extra `frontier-stack up` flags...]
#
# Builds first (frontier-node release binaries and the test-beacon .so)
# unless --no-build. Writes frontier-node/.local/frontier/<run-id>/ and a
# one-line JSON summary to <run-dir>/nightly.json. Exit 0 only when every
# step passed; 3 when a step is PENDING-OWNER.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
CONFIG="frontier-node/configs/nightly.toml"
RUN_ID="nightly-$(date -u +%Y%m%d)"
BUILD=1
EXTRA=""
for a in "$@"; do
  case "$a" in
    --no-build) BUILD=0 ;;
    *) EXTRA="$EXTRA $a" ;;
  esac
done

cd "$ROOT" || exit 2
S="$ROOT/frontier-node/target/release/frontier-stack"
RUN_DIR="$ROOT/frontier-node/.local/frontier/$RUN_ID"
# "name=code" pairs (bash 3.2: no associative arrays).
RESULTS=""

step() { # step NAME CMD...
  local name="$1"; shift
  local t0=$SECONDS
  echo "== nightly: $name: $*"
  "$@"
  local code=$?
  RESULTS="$RESULTS $name=$code"
  echo "== nightly: $name exit $code ($((SECONDS - t0)) s)"
  return $code
}

summary() {
  local all=0 pending=0 items=""
  for pair in $RESULTS; do
    local n="${pair%%=*}" c="${pair#*=}"
    [ "$c" -eq 3 ] && pending=1
    [ "$c" -ne 0 ] && all=1
    items="$items\"$n\": $c, "
  done
  mkdir -p "$RUN_DIR"
  printf '{"run_id": "%s", "date": "%s", %s"pass": %s}\n' "$RUN_ID" "$(date -u +%FT%TZ)" "$items" \
    "$([ $all -eq 0 ] && echo true || echo false)" | tee "$RUN_DIR/nightly.json"
  if [ $all -eq 0 ]; then return 0; fi
  if [ $pending -eq 1 ]; then return 3; fi
  return 1
}

cleanup() {
  if [ -f "$RUN_DIR/state.json" ]; then
    step down "$S" down --run-id "$RUN_ID"
  fi
}

if [ $BUILD -eq 1 ]; then
  step build-node bash -c "cd frontier-node && cargo build --locked --release --workspace" || { summary; exit $?; }
  step build-so scripts/build-frontier.sh --features test-beacon || { summary; exit $?; }
fi

step check-ports "$S" check-ports --config "$CONFIG" || { summary; exit $?; }
trap cleanup EXIT
if step up "$S" up --config "$CONFIG" --run-id "$RUN_ID" $EXTRA; then
  step verify "$S" verify --run-id "$RUN_ID"
  step tamper "$S" tamper --run-id "$RUN_ID"
  step load "$S" load --run-id "$RUN_ID" --viewers 5000 --game-hours 1
  # The report decides the §13.4 criteria it can (exit 1 when one fails;
  # wave-5 review: it used to exit 0 whatever it found).
  step report "$S" report --run-id "$RUN_ID"
fi
trap - EXIT
cleanup
summary
exit $?
