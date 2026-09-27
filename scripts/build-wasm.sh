#!/usr/bin/env bash
# Build frontier-wasm for the browser (M1 contract §9.5, §3.2):
# toolchain 1.95.0, wasm32-unknown-unknown, release (opt-level "s",
# panic "abort", LTO), paths remapped so the bytes do not depend on the
# checkout, output web/frontier/wasm/frontier.wasm + frontier.wasm.sha256.
# Fails when the file is over budget (400 KB raw, 150 KB gzip) or an export
# is missing.
#
#   scripts/build-wasm.sh           build and write the artefact
#   scripts/build-wasm.sh --check   build into a scratch dir; exit 1 unless
#                                   the committed artefact is byte-identical
#
# The wasm32-unknown-unknown target is an install that waits for the owner
# (O-M1-12). Without it this script changes nothing, prints PENDING-OWNER
# and exits 3; the gate checks for the target before calling it (§12).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="$ROOT/frontier-wasm"
OUT_DIR="$ROOT/permutation-server/web/frontier/wasm"
TOOLCHAIN=1.95.0
TARGET=wasm32-unknown-unknown
RAW_MAX=$((400 * 1024))
GZ_MAX=$((150 * 1024))
CHECK=0
case "${1:-}" in
  --check) CHECK=1 ;;
  "") ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

if ! rustup target list --installed --toolchain "$TOOLCHAIN" 2>/dev/null | grep -qx "$TARGET"; then
  echo "PENDING-OWNER: the $TARGET target is not installed for $TOOLCHAIN (O-M1-12); nothing built" >&2
  exit 3
fi

if [ "$CHECK" = 1 ]; then
  TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/frontier-wasm-check.XXXXXX")"
else
  TARGET_DIR="$CRATE/target"
fi

# Reproducible paths: the checkout and the cargo home never reach the bytes.
# integ-W4: `--remap-path-prefix` alone is not enough. permutation-rules is a
# path dependency outside frontier-wasm's workspace, so cargo hashes its
# absolute path into the crate metadata, and two checkouts built different
# bytes (same size, other symbol hashes and layout). The build therefore runs
# through one fixed symlink to the checkout, taken under a lock (mkdir is
# atomic) so that concurrent builds from other checkouts wait.
LINK=/tmp/psf-frontier-wasm-root
LOCK="$LINK.lock"
waited=0
until mkdir "$LOCK" 2>/dev/null; do
  waited=$((waited + 1))
  [ "$waited" -le 900 ] || { echo "build-wasm: $LOCK held for 15 min (remove it if no build is running)" >&2; exit 1; }
  sleep 1
done
cleanup() { rm -f "$LINK"; rmdir "$LOCK" 2>/dev/null || true; [ "$CHECK" = 1 ] && rm -rf "$TARGET_DIR"; return 0; }
trap cleanup EXIT
ln -sfn "$ROOT" "$LINK"
SEP=$'\x1f'
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$LINK=/psf${SEP}--remap-path-prefix=$ROOT=/psf${SEP}--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo${SEP}--remap-path-prefix=$HOME/.rustup=/rustup"
(cd "$CRATE" && cargo "+$TOOLCHAIN" build --locked --release --target "$TARGET" --target-dir "$TARGET_DIR" --lib \
  --manifest-path "$LINK/frontier-wasm/Cargo.toml")
BUILT="$TARGET_DIR/$TARGET/release/frontier_wasm.wasm"
[ -f "$BUILT" ] || { echo "build-wasm: no $BUILT" >&2; exit 1; }

RAW=$(wc -c < "$BUILT" | tr -d ' ')
GZ=$(gzip -9 -c "$BUILT" | wc -c | tr -d ' ')
SHA=$(shasum -a 256 "$BUILT" | cut -d' ' -f1)
echo "frontier.wasm: $RAW B raw, $GZ B gzip, sha256 $SHA"

# Every export of the crate (and alloc, free, memory) must be there.
node --input-type=module -e "
  import { readFileSync } from 'node:fs';
  const m = new WebAssembly.Module(readFileSync(process.argv[1]));
  const have = new Set(WebAssembly.Module.exports(m).map(e => e.name));
  const src = readFileSync(process.argv[2], 'utf8');
  const list = src.slice(src.indexOf('exports!(')).match(/^\s+([a-z_]+),$/gm).map(s => s.trim().slice(0, -1));
  const missing = ['memory', 'alloc', 'free', ...list].filter(n => !have.has(n));
  if (missing.length) { console.error('build-wasm: missing exports: ' + missing.join(', ')); process.exit(1); }
  console.log('exports: ' + list.length + ' + alloc, free, memory');
" "$BUILT" "$CRATE/src/lib.rs"

FAIL=0
[ "$RAW" -le "$RAW_MAX" ] || { echo "build-wasm: $RAW B raw is over the 400 KB budget" >&2; FAIL=1; }
[ "$GZ" -le "$GZ_MAX" ] || { echo "build-wasm: $GZ B gzip is over the 150 KB budget" >&2; FAIL=1; }

if [ "$CHECK" = 1 ]; then
  WANT="$OUT_DIR/frontier.wasm"
  if [ ! -f "$WANT" ] || ! cmp -s "$BUILT" "$WANT"; then
    echo "build-wasm --check: web/frontier/wasm/frontier.wasm is not what the source builds (run scripts/build-wasm.sh)" >&2
    exit 1
  fi
  if [ "$(cut -d' ' -f1 < "$OUT_DIR/frontier.wasm.sha256")" != "$SHA" ]; then
    echo "build-wasm --check: frontier.wasm.sha256 is stale" >&2
    exit 1
  fi
  echo "build-wasm --check: fresh"
  exit "$FAIL"
fi

[ "$FAIL" = 0 ] || exit 1
mkdir -p "$OUT_DIR"
cp "$BUILT" "$OUT_DIR/frontier.wasm"
echo "$SHA  frontier.wasm" > "$OUT_DIR/frontier.wasm.sha256"
echo "wrote $OUT_DIR/frontier.wasm"
