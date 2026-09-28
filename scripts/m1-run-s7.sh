#!/usr/bin/env bash
# The first 7-day, 1,000-bot Mode A season (M1 contract §11 W6-A, §12 Gate
# W6 line "w6-s7", §13.4): real historical quicknet rounds from the
# approved archive (O-M1-12 item 3; G0 = 1788998400), the RELEASE .so,
# 20x (7 game days in about 8.4 h of wall time), chaos, 5,000 viewers for
# one game day; then verify, tamper and report exactly as Gate W6 lists
# them. Written by W6-A; W6-A does not start it (the main session does).
#
#   scripts/m1-run-s7.sh [--no-build] [--down] [--dry-run] [--run-id ID] [extra `up` flags...]
#
# --dry-run does the build and the .so checks, then prints the commands it
# would run (check-ports, up, verify, tamper, report) and exits 0.
#
# Steps (each logged; a one-line JSON summary goes to <run-dir>/s7.json):
#   1. build (unless --no-build): the frontier-node release binaries and
#      the release .so (scripts/build-frontier.sh, deployable build). The
#      .so's file_sha256 from the build record is kept, re-checked with
#      shasum, written to <run-dir>/so.sha256 and pinned with
#      --expect-so-sha256 (the stack refuses another .so; verifier V2 checks
#      the deployed program against this pin, not against its own hash).
#   2. check-ports for base 41000 (configs/w6-s7.toml's ports).
#   3. up: the Gate W6 line
#        up --mode accel --beacon archive --scale 20 --days 7 --bots 1000
#           --run-id w6-s7 --base-port 41000 --chaos --viewers 5000
#      plus --expect-so-sha256 <build record> and any extra flags given
#      here (for example --adversary for the §13.4 hold schedule, which the
#      Gate W6 line does not name). Wrapped in `caffeinate -i` when present
#      (a sleeping Mac stops the season's clock).
#   4. verify, tamper, report (Gate W6: `verify && tamper && report`; each
#      runs even when an earlier one failed, so the report is always
#      written; the exit code says whether all passed).
#   5. copies report.md, s7.json and so.sha256 to docs/frontier/m1/runs/<run-id>/
#      (not committed by this script).
# The services stay up (chain paused) for triage unless --down; stop them
# later with `frontier-node/target/release/frontier-stack down --run-id w6-s7`.
# Exit 0 only when every step passed; 4 when the archive is not ready
# (PENDING), 3 PENDING-OWNER, else 1 (2 on bad arguments).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
RUN_ID="w6-s7"
BASE_PORT=41000
BUILD=1
DOWN=0
DRY=0
EXTRA=()
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --down) DOWN=1 ;;
    --dry-run) DRY=1 ;;
    --run-id) shift; RUN_ID="${1:-}"; [ -n "$RUN_ID" ] || { echo "--run-id needs a value" >&2; exit 2; } ;;
    --run-id=*) RUN_ID="${1#--run-id=}" ;;
    --base-port|--base-port=*|--beacon|--beacon=*|--scale|--scale=*|--days|--days=*|--so|--so=*|--expect-so-sha256|--expect-so-sha256=*)
      echo "$1 is fixed by the Gate W6 line; not accepted here" >&2; exit 2 ;;
    -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
    *) EXTRA+=("$1") ;;
  esac
  shift
done

cd "$ROOT" || exit 2
S="$ROOT/frontier-node/target/release/frontier-stack"
SO="$ROOT/permutation-frontier/target/deploy/permutation_frontier.so"
RUN_DIR="$ROOT/frontier-node/.local/frontier/$RUN_ID"
LOG_DIR="$ROOT/frontier-node/.local/s7-logs/$RUN_ID"
mkdir -p "$LOG_DIR"
RESULTS=""
SO_SHA=""
STARTED="$(date -u +%FT%TZ)"

step() { # step NAME CMD...
  local name="$1"; shift
  local t0=$SECONDS
  echo "== s7: $name: $*"
  "$@" 2>&1 | tee "$LOG_DIR/$name.log"
  local code=${PIPESTATUS[0]}
  RESULTS="$RESULTS $name=$code"
  echo "== s7: $name exit $code ($((SECONDS - t0)) s)"
  return "$code"
}

summary() {
  local all=0 worst=0 items=""
  for pair in $RESULTS; do
    local n="${pair%%=*}" c="${pair#*=}"
    items="$items\"$n\": $c, "
    if [ "$c" -ne 0 ]; then
      all=1
      # PENDING (4) and PENDING-OWNER (3) are reported as such; any other
      # failure wins over them.
      case "$c" in 3|4) [ "$worst" -eq 0 ] && worst=$c ;; *) worst=1 ;; esac
    fi
  done
  mkdir -p "$RUN_DIR"
  printf '{"run_id": "%s", "started": "%s", "finished": "%s", "git_head": "%s", "so_sha256": "%s", %s"pass": %s}\n' \
    "$RUN_ID" "$STARTED" "$(date -u +%FT%TZ)" "$(git rev-parse HEAD 2>/dev/null)" "$SO_SHA" "$items" \
    "$([ $all -eq 0 ] && echo true || echo false)" | tee "$RUN_DIR/s7.json"
  local keep="$ROOT/docs/frontier/m1/runs/$RUN_ID"
  mkdir -p "$keep"
  for f in s7.json so.sha256 report.md; do
    [ -f "$RUN_DIR/$f" ] && cp "$RUN_DIR/$f" "$keep/$f"
  done
  return "$worst"
}

if [ $BUILD -eq 1 ]; then
  step build-node bash -c "cd frontier-node && cargo build --locked --release --workspace" || { summary; exit 1; }
  step build-so scripts/build-frontier.sh || { summary; exit 1; }
  SO_SHA="$(awk '$1 == "file_sha256" {print $2}' "$LOG_DIR/build-so.log" | tail -1)"
  grep -q '^deployable *yes' "$LOG_DIR/build-so.log" || { echo "the release build is not deployable" >&2; RESULTS="$RESULTS so-deployable=1"; summary; exit 1; }
fi
[ -f "$SO" ] || { echo "no release .so at $SO (build it, or drop --no-build)" >&2; RESULTS="$RESULTS so=1"; summary; exit 1; }
HAVE="$(shasum -a 256 "$SO" | awk '{print $1}')"
if [ -z "$SO_SHA" ]; then
  # --no-build: the file on disk is the record; say so.
  echo "== s7: --no-build: pinning the .so on disk ($HAVE)"
  SO_SHA="$HAVE"
elif [ "$SO_SHA" != "$HAVE" ]; then
  echo "the release .so ($HAVE) is not the one the build recorded ($SO_SHA)" >&2
  RESULTS="$RESULTS so-sha256=1"; summary; exit 1
fi
UP=("$S" up --mode accel --beacon archive --scale 20 --days 7 --bots 1000
    --run-id "$RUN_ID" --base-port "$BASE_PORT" --chaos --viewers 5000
    --expect-so-sha256 "$SO_SHA" ${EXTRA[@]+"${EXTRA[@]}"})
if [ $DRY -eq 1 ]; then
  echo "== s7 (dry run): $S check-ports --config frontier-node/configs/w6-s7.toml --base-port $BASE_PORT"
  echo "== s7 (dry run): ${UP[*]}"
  for c in verify tamper report; do echo "== s7 (dry run): $S $c --run-id $RUN_ID"; done
  exit 0
fi

mkdir -p "$RUN_DIR"
echo "$SO_SHA  permutation-frontier/target/deploy/permutation_frontier.so" > "$RUN_DIR/so.sha256"
step check-ports "$S" check-ports --config frontier-node/configs/w6-s7.toml --base-port "$BASE_PORT" || { summary; exit $?; }

CAFF=()
command -v caffeinate >/dev/null 2>&1 && CAFF=(caffeinate -i)
# The Gate W6 w6-s7 line, with the release build's pin.
if step up ${CAFF[@]+"${CAFF[@]}"} "${UP[@]}"; then
  step verify "$S" verify --run-id "$RUN_ID"
  step tamper "$S" tamper --run-id "$RUN_ID"
  step report "$S" report --run-id "$RUN_ID"
elif [ -f "$RUN_DIR/state.json" ]; then
  # A run that started and failed still gets its report (triage input).
  step report "$S" report --run-id "$RUN_ID"
fi
if [ $DOWN -eq 1 ] && [ -f "$RUN_DIR/state.json" ]; then
  step down "$S" down --run-id "$RUN_ID"
else
  echo "== s7: services left up for triage; stop them with: $S down --run-id $RUN_ID"
fi
summary
exit $?
