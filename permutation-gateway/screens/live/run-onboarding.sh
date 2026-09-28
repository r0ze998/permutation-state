#!/usr/bin/env bash
# The scripted onboarding run on a local stack (M1 contract §13.6 E7, §12
# Gate W6 "scripted onboarding run green in JA and EN"; unit W6-D):
#
#   1. `frontier-stack check-ports` and `up` (Mode A, test key, scale 20 by
#      default, 1 game day, 20 bots) at --base-port (default 41000: the
#      herald on 41040, the port the contract names);
#   2. waits for the herald's season record (an until-loop, ≤ 10 min);
#   3. runs live/onboarding.live.mjs at 390 × 844 in JA, then EN
#      (join → site → build → scout/explore → sealed march on a camp →
#      report → verify in this browser green), shots and summary in
#      screens/artifacts/live/;
#   4. with --spectator, keeps the spectator page open for 24 game hours
#      (live/spectator-memory.live.mjs; 72 wall minutes at 20×);
#   5. `frontier-stack down` (always, also on failure).
#
#   permutation-gateway/screens/live/run-onboarding.sh [--base-port P] [--scale S]
#       [--run-id ID] [--spectator] [--record DIR]
#
# Environment: FRONTIER_STACK (default frontier-node/target/release/
# frontier-stack), FRONTIER_BIN / PSF_REPO (passed through to the stack),
# SO (a test-beacon .so; default the stack's own default path).
# Exit 0 only when every run passed. Ports: 41000-41999 only (the stack's
# check-ports refuses the reserved list).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
S="${FRONTIER_STACK:-$ROOT/frontier-node/target/release/frontier-stack}"
BASE=41000
SCALE=20
RUN_ID="w6d-onboarding-$(date -u +%Y%m%d%H%M%S)"
SPECTATOR=0
RECORD=""
while [ $# -gt 0 ]; do
  case "$1" in
    --base-port) BASE="$2"; shift 2 ;;
    --scale) SCALE="$2"; shift 2 ;;
    --run-id) RUN_ID="$2"; shift 2 ;;
    --spectator) SPECTATOR=1; shift ;;
    --record) RECORD="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
HERALD="http://127.0.0.1:$((BASE + 40))"
LOGDIR="$HERE/../artifacts/live"
mkdir -p "$LOGDIR"
cd "$ROOT" || exit 2

"$S" check-ports --config frontier-node/configs/w5-smoke.toml --base-port "$BASE" || exit 1
SO_ARGS=()
[ -n "${SO:-}" ] && SO_ARGS=(--so "$SO")
"$S" up --mode accel --beacon test-key --scale "$SCALE" --days 1 --bots 20 --run-id "$RUN_ID" \
  --base-port "$BASE" ${SO_ARGS[@]+"${SO_ARGS[@]}"} > "$LOGDIR/stack-up.log" 2>&1 &
UP_PID=$!
cleanup() {
  "$S" down --run-id "$RUN_ID" >> "$LOGDIR/stack-up.log" 2>&1
  kill "$UP_PID" 2>/dev/null
  wait "$UP_PID" 2>/dev/null
}
trap cleanup EXIT

# The herald serves /h/season once the season exists (setup ≈ 1 min).
deadline=$((SECONDS + 600))
until curl -sf "$HERALD/h/season" > /dev/null; do
  if ! kill -0 "$UP_PID" 2>/dev/null || [ $SECONDS -ge $deadline ]; then
    echo "the stack did not come up (see $LOGDIR/stack-up.log)" >&2
    exit 1
  fi
  sleep 2
done
# … and play has begun: the chain clock past bell 1 (genesis + one bell), so Join is open.
until curl -sf "$HERALD/h/season" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);process.exit(Number(j.latestUnix)>=Number(j.genesisTs)+Number(j.bellSecs??600)?0:1)})'; do
  if ! kill -0 "$UP_PID" 2>/dev/null || [ $SECONDS -ge $deadline ]; then
    echo "the season did not start (see $LOGDIR/stack-up.log)" >&2
    exit 1
  fi
  sleep 2
done
echo "stack $RUN_ID up: herald $HERALD"

cd "$HERE/.." || exit 2
code=0
LIVE_HERALD="$HERALD" LIVE_LANGS=ja,en LIVE_RECORD="$RECORD" node --test live/onboarding.live.mjs || code=1
if [ $SPECTATOR -eq 1 ]; then
  LIVE_HERALD="$HERALD" node --test live/spectator-memory.live.mjs || code=1
fi
exit $code
