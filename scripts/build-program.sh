#!/usr/bin/env bash
# Reproducible SBF build of permutation-chain: same sources + same tools =
# same hash, at any checkout path (root Cargo workspace, one lock, pinned
# platform-tools). Writes permutation-chain/target/deploy/permutation_chain.so
# with default features (the devnet artefact: never `dev-randomness`) and
# prints:
#   file_sha256   sha256 of the .so file
#   program_hash  sha256 of the .so with trailing zero bytes stripped, the
#                 form comparable with on-chain program data
# It then checks that the artefact was built with overflow checks on for
# permutation-chain (root Cargo.toml `[profile.release.package.permutation-chain]`,
# audit WP08): the program's own arithmetic must abort, not wrap. Rust
# compiles a checked `+ - *` into a panic whose message is "attempt to add
# with overflow" (subtract, multiply); an unchecked build has none of them
# (the rules engine is unchecked on purpose, and the prebuilt core/alloc
# carry none). No such message in the .so: the build is refused (exit 3).
set -euo pipefail
cd "$(dirname "$0")/.."
want="solana-cargo-build-sbf 3.1.9"
have="$(cargo-build-sbf --version | head -1)"
[ "$have" = "$want" ] || { echo "need $want (have $have)" >&2; exit 1; }
# --sbf-out-dir is redundant with .cargo/config.toml but kept, so the script
# does not depend on the config file or on an inherited CARGO_TARGET_DIR.
cargo-build-sbf --manifest-path permutation-chain/Cargo.toml --tools-version v1.52 \
  --sbf-out-dir permutation-chain/target/deploy -- --locked
so=permutation-chain/target/deploy/permutation_chain.so
checks="$({ LC_ALL=C grep -a -o -E 'attempt to (add|subtract|multiply) with overflow' "$so" || true; } | wc -l | tr -d ' ')"
if [ "$checks" -eq 0 ]; then
  echo "$so: built without overflow checks (no overflow panic message); see [profile.release.package.permutation-chain] in Cargo.toml" >&2
  exit 3
fi
echo "file_sha256    $(shasum -a 256 "$so" | cut -d' ' -f1)"
echo "program_hash   $(node -e 'const d=require("fs").readFileSync(process.argv[1]);let n=d.length;while(n&&d[n-1]===0)n--;console.log(require("crypto").createHash("sha256").update(d.subarray(0,n)).digest("hex"))' "$so")"
echo "overflow_panic $checks"
