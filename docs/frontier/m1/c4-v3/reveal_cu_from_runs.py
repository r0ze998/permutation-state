#!/usr/bin/env python3
"""Reveal CU distribution from frontier-stack runs and svm CU logs (unit W6-E, M1 CL-26/30 final).

Inputs (read-only):
  * run directories written by `frontier-stack` (`<run>/verify/input.json.gz`: every program
    transaction with its wire bytes, logs and consumed units; `<run>/report.json` for the header);
  * optional svm-tests CU logs (`PSF_CU_LOG` lines `build kind cu tx_bytes locks loaded heap`).

For every landed transaction whose Frontier instruction is Reveal (tag 0x51) it records:
  program CU (the `consumed N of M` line of the Frontier program), whole-transaction units,
  the requested CU limit and loaded-data limit (ComputeBudget instructions), the number of
  writable keys (write locks), whether the ArrivalDay was written (first reveal of a province-bell
  in its day), the path length (plaintext byte 21), the number of path provinces supplied, and
  the arrival bell (plaintext bytes 9..13) for the per-bell reveal counts.

Run:  python3 reveal_cu_from_runs.py [--svm cu.log ...] RUN_DIR ...  > reveal-cu.txt
      (--json FILE also writes the rows)
"""
import gzip, json, math, os, sys, base64

B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58d(s):
    n = 0
    for c in s:
        n = n * 58 + B58.index(c)
    raw = n.to_bytes((n.bit_length() + 7) // 8, 'big') if n else b''
    pad = len(s) - len(s.lstrip('1'))
    return b'\0' * pad + raw
CB = b58d('ComputeBudget111111111111111111111111111111').rjust(32, b'\0')

def cu16(b, i):
    v = s = 0
    while True:
        x = b[i]; i += 1
        v |= (x & 0x7F) << s; s += 7
        if not x & 0x80:
            return v, i

def parse_tx(raw):
    i = 0
    nsig, i = cu16(raw, i); i += 64 * nsig
    req, ro_s, ro_u = raw[i], raw[i + 1], raw[i + 2]; i += 3
    nk, i = cu16(raw, i)
    keys = [raw[i + 32 * k:i + 32 * k + 32] for k in range(nk)]; i += 32 * nk
    i += 32  # blockhash
    nix, i = cu16(raw, i)
    ixs = []
    for _ in range(nix):
        p = raw[i]; i += 1
        na, i = cu16(raw, i); accs = list(raw[i:i + na]); i += na
        nd, i = cu16(raw, i); data = raw[i:i + nd]; i += nd
        ixs.append((p, accs, data))
    writable = set(range(req - ro_s)) | set(range(req, nk - ro_u))
    return keys, ixs, writable

def reveal_rows(run):
    p = os.path.join(run, 'verify', 'input.json.gz')
    if not os.path.exists(p):
        return None, []
    j = json.load(gzip.open(p))
    rows = []
    for t in j['txs']:
        if t.get('err') is not None:
            continue
        raw = base64.b64decode(t['tx'])
        keys, ixs, writable = parse_tx(raw)
        limit = loaded = None
        fr = None
        for (pi, accs, data) in ixs:
            if keys[pi] == CB:
                if data[:1] == b'\x02':
                    limit = int.from_bytes(data[1:5], 'little')
                elif data[:1] == b'\x04':
                    loaded = int.from_bytes(data[1:5], 'little')
            elif data[:1] == b'\x51':
                fr = (pi, accs, data)
        if fr is None:
            continue
        pi, accs, data = fr
        prog = None
        for l in t['logs']:
            if ' consumed ' in l and not l.startswith('Program ComputeBudget'):
                prog = int(l.split(' consumed ')[1].split(' ')[0])
        plain = data[3:40]
        path_len = plain[21]
        arrive = int.from_bytes(plain[9:13], 'little')
        # accounts: 0 fee payer .. 8 arrivalday, 9..12 slots, 13.. path provinces, then ix sysvar, system
        n_path = max(0, len(accs) - 15)
        day_w = accs[8] in writable if len(accs) > 8 else False
        rows.append(dict(run=os.path.basename(run.rstrip('/')), prog_cu=prog, tx_units=t.get('units'),
                         limit=limit, loaded=loaded, write_locks=len([a for a in set(accs) | {0} if a in writable]),
                         arrivalday_written=day_w, path_len=path_len, path_provinces=n_path, arrive=arrive))
    return j, rows

def q(v, x):
    s = sorted(v)
    return s[min(len(s) - 1, int(x * len(s)))] if s else None

def dist(v):
    v = [x for x in v if x is not None]
    if not v:
        return 'n 0'
    return f'n {len(v)}, min {min(v):,}, p50 {q(v,.5):,}, p90 {q(v,.9):,}, p99 {q(v,.99):,}, max {max(v):,}, mean {sum(v)/len(v):,.0f}'

def main(argv):
    svm, runs, out_json = [], [], None
    i = 0
    while i < len(argv):
        if argv[i] == '--svm':
            svm.append(argv[i + 1]); i += 2
        elif argv[i] == '--json':
            out_json = argv[i + 1]; i += 2
        else:
            runs.append(argv[i]); i += 1
    allrows = []
    print('# Reveal CU measured in play and in the svm suite (W6-E) [measured]\n')
    print('## 1. Stack runs (test-key or archive; `frontier-stack`, localnet LiteSVM with the deployed .so)\n')
    for r in runs:
        rep = os.path.join(r, 'report.json')
        if not os.path.exists(rep):
            rep = os.path.join(r, 'state.json')   # a run whose report is not written yet
        hdr = ''
        if os.path.exists(rep):
            rj = json.load(open(rep))
            c = rj.get('config', {})
            so = rj.get('so', {})
            hdr = (f"beacon {c.get('beacon')}, scale {c.get('scale')}, bots {c.get('bots')}, days {c.get('days')}, "
                   f"game_hours {c.get('game_hours')}, .so {so.get('len')} B sha256 {str(so.get('sha256'))[:12]}..., last bell {rj.get('last_bell')}")
        _, rows = reveal_rows(r)
        allrows += rows
        print(f'- `{os.path.basename(r.rstrip("/"))}` ({r}): {hdr}')
        print(f'  - Reveal program CU: {dist([x["prog_cu"] for x in rows])}')
        if rows:
            print(f'  - whole-tx units: {dist([x["tx_units"] for x in rows])}; requested limit {sorted(set(x["limit"] for x in rows))}, '
                  f'loaded limit {sorted(set(x["loaded"] for x in rows))}; write locks {sorted(set(x["write_locks"] for x in rows))}')
            fb = [x['prog_cu'] for x in rows if x['arrivalday_written']]
            nf = [x['prog_cu'] for x in rows if not x['arrivalday_written']]
            print(f'  - first of the province-bell (ArrivalDay written): {dist(fb)}; later: {dist(nf)}')
            print(f'  - path steps: {dist([x["path_len"] for x in rows])}; path provinces beyond the destination: {dist([x["path_provinces"] for x in rows])}')
            per = {}
            for x in rows:
                per[x['arrive']] = per.get(x['arrive'], 0) + 1
            print(f'  - reveals per arrival bell (bells with ≥ 1 reveal): {dist(list(per.values()))}; bells with reveals {len(per)}')
    if runs:
        v = [x['prog_cu'] for x in allrows]
        print(f'\n**All runs pooled:** {dist(v)}\n')
    if svm:
        print('## 2. svm suite (`PSF_CU_LOG`, landed single-instruction Reveal transactions; Release and TestBeacon builds)\n')
        for f in svm:
            by = {}
            for line in open(f):
                p = line.split()
                if len(p) >= 3 and p[1] == 'Reveal' and p[0] in ('Release', 'TestBeacon'):
                    by.setdefault(p[0], []).append(int(p[2]))
            for b, v in sorted(by.items()):
                print(f'- `{os.path.basename(f)}` {b}: whole-tx units {dist(v)} (the units include the harness ComputeBudget instructions, 150 each, usually two)')
    if out_json:
        json.dump(allrows, open(out_json, 'w'), indent=1)

if __name__ == '__main__':
    main(sys.argv[1:])
