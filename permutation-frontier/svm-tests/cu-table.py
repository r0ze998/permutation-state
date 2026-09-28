#!/usr/bin/env python3
"""Regenerate or check `frontier_abi::budgets::MEASURED` from a G1 CU log.

The G1 table (M1 contract §5.5, §13.1, v1.8 §24) is the largest CU of each
instruction kind over every landed single-Frontier-instruction transaction
of a full `--release` svm run on the release and test-beacon `.so`:

    rm -f /tmp/cu.log
    (cd permutation-frontier/svm-tests && \
       RELEASE_CHECK=1 PSF_CU_LOG=/tmp/cu.log ./run.sh --release)
    permutation-frontier/svm-tests/cu-table.py /tmp/cu.log            # print
    permutation-frontier/svm-tests/cu-table.py /tmp/cu.log --check    # exit 1 on drift
    permutation-frontier/svm-tests/cu-table.py /tmp/cu.log --write    # rewrite MEASURED

Log lines (svm-tests `Chain::log_cu`): `build kind cu tx_bytes locks loaded heap`.
Only `Release` and `TestBeacon` rows count (the trace build's markers add
CU; the oracle build is test-only). Kinds whose MEASURED is 0 by design
(SkipQuiet scales with its bells, ResolveClash is ungated) stay 0.

`--check` fails when a kind's logged maximum is above its MEASURED value
(the table under-states a measured transaction) or more than 2 % below it
(the table is stale), or when a gated kind's maximum is above its §5.5 gate.
`--write` sets every MEASURED value to the logged maximum.
"""
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BUDGETS = os.path.join(HERE, "..", "..", "frontier-abi", "src", "budgets.rs")
ROW = re.compile(r"^(\s+)(\w+): ([\d_]+), ([\d_]+), ([\d_]+), ([\d_]+);$", re.M)
FIXED_ZERO = {"SkipQuiet", "ResolveClash"}


def num(s):
    return int(s.replace("_", ""))


def fmt(n):
    s = f"{n:,}".replace(",", "_")
    return s


def load_table(src):
    rows = {}
    for m in ROW.finditer(src):
        rows[m.group(2)] = dict(
            gate=num(m.group(3)), per=num(m.group(4)), tx=num(m.group(5)), measured=num(m.group(6))
        )
    if not rows:
        sys.exit("cu-table: no budgets! rows found in " + BUDGETS)
    return rows


def load_log(paths):
    mx, n = {}, {}
    for path in paths:
        with open(path) as f:
            for line in f:
                parts = line.split()
                if len(parts) != 7 or parts[0] not in ("Release", "TestBeacon"):
                    continue
                kind, cu = parts[1], int(parts[2])
                n[kind] = n.get(kind, 0) + 1
                if cu > mx.get(kind, (0, ""))[0]:
                    mx[kind] = (cu, parts[0])
    return mx, n


def limit(measured):
    return math.ceil(measured * 105 / 100 / 500) * 500


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = {a for a in sys.argv[1:] if a.startswith("--")}
    if not args:
        sys.exit(__doc__)
    src = open(BUDGETS).read()
    rows = load_table(src)
    mx, n = load_log(args)
    bad = []
    print(f"{'kind':20s} {'log max':>8s} {'n':>6s} {'MEASURED':>9s} {'gate':>8s} {'limit':>8s} {'margin':>7s}")
    for kind, r in rows.items():
        m = mx.get(kind, (0, ""))[0]
        lim = limit(m) if m else 0
        margin = f"{(r['gate'] - m) * 100 / r['gate']:6.1f}%" if r["gate"] and m else "     -"
        print(f"{kind:20s} {m:8d} {n.get(kind, 0):6d} {r['measured']:9d} {r['gate']:8d} {lim:8d} {margin}")
        if kind in FIXED_ZERO:
            continue
        if m == 0:
            bad.append(f"{kind}: no landed transaction in the log")
            continue
        if r["gate"] and m > r["gate"]:
            bad.append(f"{kind}: logged max {m} > gate {r['gate']}")
        if m > r["measured"]:
            bad.append(f"{kind}: logged max {m} > MEASURED {r['measured']}")
        elif m * 100 < r["measured"] * 98:
            bad.append(f"{kind}: logged max {m} is more than 2 % below MEASURED {r['measured']}")
    if "--write" in flags:
        def sub(mo):
            kind = mo.group(2)
            if kind in FIXED_ZERO or kind not in mx:
                return mo.group(0)
            return f"{mo.group(1)}{kind}: {mo.group(3)}, {mo.group(4)}, {mo.group(5)}, {fmt(mx[kind][0])};"
        open(BUDGETS, "w").write(ROW.sub(sub, src))
        print("cu-table: MEASURED rewritten in", os.path.relpath(BUDGETS))
        return 0
    if "--check" in flags and bad:
        for b in bad:
            print("cu-table: " + b, file=sys.stderr)
        return 1
    for b in bad:
        print("cu-table: note: " + b)
    return 0


if __name__ == "__main__":
    sys.exit(main())
