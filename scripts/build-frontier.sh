#!/usr/bin/env bash
# Reproducible SBPF v2 build of permutation-frontier (M1 contract §3.2),
# ported from build-program.sh.
#
#   scripts/build-frontier.sh [--twice] [--features trace|oracle|test-beacon]
#
# Plain build (the deployable artefact):
#   permutation-frontier/target/deploy/permutation_frontier.so
# It is refused (exit 3) unless
#   - the ELF header's e_flags is 2 (SBPF v2),
#   - the program's overflow panic messages are present (overflow checks on
#     for permutation-frontier, [profile.release.package.permutation-frontier]
#     in the root Cargo.toml; the rules crate stays unchecked for compute),
#   - none of the test-build markers PSF_ORACLE_BUILD, PSF_TRACE_BUILD and
#     PSF_TEST_BEACON_BUILD is in the file,
#   - no R_SBF_64_RELATIVE relocation of a data section carries a non-zero
#     low word: platform-tools v1.52 writes a pointer into the middle of a
#     static object (e.g. a switch table of `&SPECS[i]`) as `(base << 32) |
#     offset`, and the loader (solana-sbpf 0.21) uses the high word only,
#     so such a pointer silently resolves to the object's start [measured,
#     W2-A notes]. Every build is checked (feature builds too).
# Feature builds are never deployable: --features trace|oracle|test-beacon
# builds into permutation-frontier/target/deploy-<feature>/, requires that
# feature's marker to be present (and the others absent), and prints
# "deployable no".
#
# Prints file_sha256 (the .so), program_hash (the .so with trailing zero
# bytes stripped, comparable with on-chain program data), e_flags, the .so
# length, the --max-len to deploy with (round_up(1.25 × .so, 4 KiB), I-45)
# and the ProgramData length that max_len gives (45 + max_len: the figure
# L(kind) counts, §10.1).
#
# --twice builds twice into two fresh target directories and fails (exit 4)
# unless both artefacts hash the same.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

twice=0
feature=""
while [ $# -gt 0 ]; do
  case "$1" in
    --twice) twice=1 ;;
    --features) shift; feature="${1:-}" ;;
    --features=*) feature="${1#--features=}" ;;
    -h|--help) sed -n '2,31p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
case "$feature" in
  ""|trace|oracle|test-beacon) ;;
  *) echo "unknown feature: $feature (trace, oracle or test-beacon)" >&2; exit 2 ;;
esac

want="solana-cargo-build-sbf 3.1.9"
have="$(cargo-build-sbf --version | head -1)"
[ "$have" = "$want" ] || { echo "need $want (have $have)" >&2; exit 1; }

manifest=permutation-frontier/Cargo.toml
out=permutation-frontier/target/deploy
[ -n "$feature" ] && out="permutation-frontier/target/deploy-$feature"

build() { # build <sbf-out-dir> [<cargo target dir>]
  local dir="$1" tgt="${2:-}"
  local args=(--manifest-path "$manifest" --tools-version v1.52 --arch v2 --sbf-out-dir "$dir")
  [ -n "$feature" ] && args+=(--features "$feature")
  if [ -n "$tgt" ]; then
    CARGO_TARGET_DIR="$tgt" cargo-build-sbf "${args[@]}" -- --locked
  else
    cargo-build-sbf "${args[@]}" -- --locked
  fi
}

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
program_hash() { # sha256 of the file with trailing zero bytes stripped
  node -e 'const d=require("fs").readFileSync(process.argv[1]);let n=d.length;while(n&&d[n-1]===0)n--;console.log(require("crypto").createHash("sha256").update(d.subarray(0,n)).digest("hex"))' "$1"
}
e_flags() { # ELF64 little-endian: e_flags is the u32 at offset 48
  od -An -t u4 -j 48 -N 4 "$1" | tr -d ' '
}
count() { { LC_ALL=C grep -a -o -E "$1" "$2" || true; } | wc -l | tr -d ' '; }
data_relocs_with_addend() { # R_SBF_64_RELATIVE in non-text sections with a non-zero low word
  node -e '
const d=require("fs").readFileSync(process.argv[1]);
const u16=o=>d.readUInt16LE(o), u32=o=>d.readUInt32LE(o), u64=o=>Number(d.readBigUInt64LE(o));
const shoff=u64(0x28), shes=u16(0x3a), shn=u16(0x3c);
const sh=[];for(let i=0;i<shn;i++){const o=shoff+i*shes;sh.push({type:u32(o+4),flags:u64(o+8),addr:u64(o+16),off:u64(o+24),size:u64(o+32),ent:u64(o+56)});}
let bad=0;
for(const s of sh){ if(s.type!==9) continue; // SHT_REL
  for(let o=s.off;o<s.off+s.size;o+=16){ const r_off=u64(o), type=u32(o+8); if(type!==8) continue;
    const t=sh.find(x=>x.addr&&r_off>=x.addr&&r_off<x.addr+x.size); if(!t||(t.flags&4)) continue; // text: lddw pairs are fine
    if(u32(t.off+(r_off-t.addr))!==0) bad++; } }
console.log(bad);' "$1"
}

check() { # check <so>: the refusals; exit 3 on any
  local so="$1" bad=0
  local flags; flags="$(e_flags "$so")"
  if [ "$flags" != "2" ]; then echo "$so: e_flags $flags, not 2 (SBPF v2)" >&2; bad=1; fi
  local ovf; ovf="$(count 'attempt to (add|subtract|multiply) with overflow' "$so")"
  if [ "$ovf" -eq 0 ]; then
    echo "$so: built without overflow checks (no overflow panic message); see [profile.release.package.permutation-frontier]" >&2
    bad=1
  fi
  local m want_m
  for m in PSF_ORACLE_BUILD PSF_TRACE_BUILD PSF_TEST_BEACON_BUILD; do
    want_m=0
    case "$feature:$m" in
      oracle:PSF_ORACLE_BUILD|trace:PSF_TRACE_BUILD|test-beacon:PSF_TEST_BEACON_BUILD) want_m=1 ;;
    esac
    local n; n="$(count "$m" "$so")"
    if [ "$want_m" = 1 ] && [ "$n" -eq 0 ]; then echo "$so: marker $m missing from a --features $feature build" >&2; bad=1; fi
    if [ "$want_m" = 0 ] && [ "$n" -ne 0 ]; then echo "$so: carries $m (a test build); refused" >&2; bad=1; fi
  done
  local rel; rel="$(data_relocs_with_addend "$so")"
  if [ "$rel" != "0" ]; then
    echo "$so: $rel data relocation(s) with a non-zero addend (mis-resolved by the SBPF loader); avoid static tables of interior pointers" >&2
    bad=1
  fi
  [ "$bad" = 0 ] || exit 3
  echo "$ovf"
}

so_name=permutation_frontier.so
if [ "$twice" = 1 ]; then
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/build-frontier.XXXXXX")"
  trap 'rm -rf "$tmp"' EXIT
  build "$tmp/out1" "$tmp/t1"
  build "$tmp/out2" "$tmp/t2"
  h1="$(sha "$tmp/out1/$so_name")"; h2="$(sha "$tmp/out2/$so_name")"
  if [ "$h1" != "$h2" ]; then
    echo "not reproducible: $h1 vs $h2" >&2
    exit 4
  fi
  mkdir -p "$out"
  cp "$tmp/out1/$so_name" "$out/$so_name"
  echo "twice          $h1 (both builds)"
else
  build "$out"
fi
so="$out/$so_name"
ovf="$(check "$so")"
len="$(wc -c < "$so" | tr -d ' ')"
# --max-len = round_up(1.25 × .so, 4,096) (fees::deploy_max_len)
max_len=$(( ( (len * 5 + 3) / 4 + 4095 ) / 4096 * 4096 ))
echo "file_sha256    $(sha "$so")"
echo "program_hash   $(program_hash "$so")"
echo "e_flags        $(e_flags "$so")"
echo "overflow_panic $ovf"
echo "so_len         $len"
echo "max_len        $max_len"
echo "programdata    $(( max_len + 45 ))"
if [ -n "$feature" ]; then
  echo "features       $feature"
  echo "deployable     no"
else
  echo "deployable     yes"
fi
