#!/usr/bin/env bash
# The one-time I-45 validator drill (M1 contract §11 W2-B, §12 Gate W2).
#
# Starts the installed solana-test-validator 3.1.9 on the contract's drill
# ports (RPC 41080, pubsub 41081, gossip 41085, faucet 41086, dynamic
# 41100-41140; §10.3), builds the harness's probe program padded to the
# release .so's size (non-BLS: 3.1.9 has no BLS12-381 syscalls), deploys it
# twice under LoaderV3 (at --max-len = round_up(1.25 × .so, 4 KiB), as the
# release deploys, and at a smaller max_len), runs tests/drill.rs against it,
# and stops the validator. Local only: no devnet or mainnet step.
#
#   permutation-frontier/svm-tests/drill/validator-drill.sh [out-dir]
#
# PSF_DRILL_SO_LEN   size to pad the probe to (default: the release .so if
#                    one is found, else SP-V2's 540,608-B kprobe, the
#                    placeholder frontier-abi uses)
# The out-dir (default: a new temporary directory) receives the ledger, the
# throwaway keypairs and drill.log.
set -euo pipefail
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
here="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$(mktemp -d "${TMPDIR:-/tmp}/psf-drill.XXXXXX")}"
mkdir -p "$out"
RPC=41080; GOSSIP=41085; FAUCET=41086; DYN=41100-41140
URL="http://127.0.0.1:$RPC"

want="solana-test-validator 3.1.9"
have="$(solana-test-validator --version | cut -d' ' -f1-2)"
[ "$have" = "$want" ] || { echo "need $want (have $have)" >&2; exit 1; }
for p in $RPC $((RPC + 1)) $GOSSIP $FAUCET; do
  if lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; then echo "drill port $p is busy" >&2; exit 1; fi
done

# The size to emulate.
so_len="${PSF_DRILL_SO_LEN:-}"
if [ -z "$so_len" ]; then
  for c in "$here/../target/deploy" "$here/../../target/deploy" "$here/../../permutation-chain/target/deploy"; do
    if [ -f "$c/permutation_frontier.so" ]; then so_len="$(wc -c < "$c/permutation_frontier.so" | tr -d ' ')"; break; fi
  done
fi
so_len="${so_len:-540608}"

# The probe, unpadded then padded to so_len.
( cd "$here/probe" && cargo-build-sbf --tools-version v1.52 --arch v2 --sbf-out-dir "$here/target/deploy-probe" -- --locked >/dev/null 2>&1 )
base="$(wc -c < "$here/target/deploy-probe/psf_probe.so" | tr -d ' ')"
pad=$(( so_len > base ? so_len - base : 0 ))
( cd "$here/probe" && PSF_PROBE_PAD="$pad" CARGO_TARGET_DIR="$here/target/probe-padded" \
    cargo-build-sbf --tools-version v1.52 --arch v2 --sbf-out-dir "$here/target/deploy-probe-padded" -- --locked >/dev/null 2>&1 )
so="$here/target/deploy-probe-padded/psf_probe.so"
len="$(wc -c < "$so" | tr -d ' ')"
max_len=$(( ( (len * 5 + 3) / 4 + 4095 ) / 4096 * 4096 ))
small_len=$(( (len + 4095) / 4096 * 4096 ))
echo "drill: probe $len B (target $so_len B), max_len $max_len, small max_len $small_len" | tee "$out/drill.log"

solana-test-validator --ledger "$out/ledger" --reset --quiet --bind-address 127.0.0.1 \
  --rpc-port "$RPC" --gossip-port "$GOSSIP" --faucet-port "$FAUCET" --dynamic-port-range "$DYN" \
  >"$out/validator.out" 2>&1 &
vpid=$!
trap 'kill "$vpid" 2>/dev/null || true; wait "$vpid" 2>/dev/null || true' EXIT
for _ in $(seq 1 120); do
  if solana --url "$URL" cluster-version >/dev/null 2>&1; then break; fi
  sleep 0.5
done
solana --url "$URL" cluster-version | tee -a "$out/drill.log"

solana-keygen new --no-bip39-passphrase --silent --force -o "$out/payer.json" >/dev/null
solana-keygen new --no-bip39-passphrase --silent --force -o "$out/probe-a.json" >/dev/null
solana-keygen new --no-bip39-passphrase --silent --force -o "$out/probe-b.json" >/dev/null
solana --url "$URL" airdrop 500 --keypair "$out/payer.json" >/dev/null
a="$(solana-keygen pubkey "$out/probe-a.json")"
b="$(solana-keygen pubkey "$out/probe-b.json")"
solana --url "$URL" --keypair "$out/payer.json" program deploy --program-id "$out/probe-a.json" --max-len "$max_len" "$so" | tee -a "$out/drill.log"
solana --url "$URL" --keypair "$out/payer.json" program deploy --program-id "$out/probe-b.json" --max-len "$small_len" "$so" | tee -a "$out/drill.log"
solana --url "$URL" --keypair "$out/payer.json" program show "$a" | tee -a "$out/drill.log"
solana --url "$URL" --keypair "$out/payer.json" program show "$b" | tee -a "$out/drill.log"

( cd "$here" && PSF_DRILL_RPC="$URL" PSF_DRILL_PROGRAM="$a" PSF_DRILL_PROGRAM_SMALL="$b" \
    PSF_DRILL_KEYPAIR="$out/payer.json" \
    cargo test --locked --release --test drill -- --ignored --nocapture ) 2>&1 | tee -a "$out/drill.log"
echo "drill: log in $out/drill.log"
