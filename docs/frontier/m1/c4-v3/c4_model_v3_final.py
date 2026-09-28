#!/usr/bin/env python3
"""Restated-C4 model v3, final for M1 (CL-26 / CL-30 / I-49; unit W6-E).

Successor of W1-D's `c4_model_v3.py` (lab c4-v3) and `d18_model.py` (lab d18). What changed:
  1. **Measured Reveal inputs** instead of the 16k/20k/26k placeholders: the Reveal CU limit the season
     and every keeper request (`reveal_cu_limit` = the G1 worst Reveal + 5 %, rounded up to 500) and
     `L(reveal)` from the release `.so` (`frontier-abi/vectors/budgets.json`, W5-A regeneration), the
     worst Reveal of the G1 suite, and the Reveal CU distribution measured in play (stack runs) and in the
     svm suite (`reveal_cu_from_runs.py` output, JSON rows).
  2. The cost of a Reveal counted as the runtime counts it: 1 signature, 2 write locks (fee payer, slot),
     **3 on the first reveal of a province-bell in its day** (the ArrivalDay is written), `8·⌈L/32 KiB⌉`.
  3. The D18 table priced at the **CU limits, write locks and `L(kind)` of the budgets table** (the
     numbers a keeper actually requests), with ResolveFromInputs at the Phase A limit and at Phase B's
     (W6-B commits Phase B; W5-A measured RFI max 274,007 CU → limit 288,000).
  4. The defence refund per Reveal by the program's formula (`fees::defence_refund`, §5.12):
     `min(fee(P_def), P_def·cost − 2,500) − (tip_min − 2,500)`.
  5. Value inputs from `frontier-sim c4` re-run at the W6-E base (falls back to W1-D's JSON).
Everything else (capacity cases, organic traffic, valuation (a), relic tip, payer-lock pricing) is v3's.

Tags: [measured] (budgets, runs, svm), [sim] (frontier-sim), [model] (everything priced).
Run: python3 c4_model_v3_final.py --budgets <repo>/frontier-abi/vectors/budgets.json \
        --sim-dir <dir with c4_50k_s3.md.json, c4_10k_s3.md.json, c4_50k_s1_relics.md.json> \
        --reveal-rows reveal-rows.json [--svm-summary TEXT] > c4-model-v3-final.txt
"""
import argparse, json, math, os

HERE = os.path.dirname(os.path.abspath(__file__))
SPFEE = os.path.join(HERE, '..', '..', '..', '..', 'm0b', 'spikes', 'SP-FEE', 'results')
SOL_USD, LAM = 150.0, 1e9
ACCT_CAP, BLOCK_CAP = 40_000_000, 100_000_000
KEYS_PER_STREAM = {'legacy': 20, 'alt': 60}
SLOT_MAINNET, SLOT_DESIGN = 0.265, 0.40
P_MIN, P_DEF, P_DELAY = 0.433, 2.0, 0.5
RELIC_TIP = 100_000
V_CLASH, V_RELIC = 7.0, 150.0
RENT_PER_BYTE = 5_080
def rent(size): return (128 + size) * RENT_PER_BYTE
RENT_SLOT, RENT_DAY = rent(160), rent(96)          # ArrivalSlot 160 B, ArrivalDay 96 B (§5.2)

ap = argparse.ArgumentParser()
ap.add_argument('--budgets', required=True)
ap.add_argument('--sim-dir', default=os.path.join(HERE, '..'))
ap.add_argument('--reveal-rows')
ap.add_argument('--svm-log', action='append', default=[])
ap.add_argument('--spfee', default=SPFEE, help='SP-FEE results directory (mainnet-blocks-m0c.json)')
args = ap.parse_args()

B = {i['name']: i for i in json.load(open(args.budgets))['instructions']}
REV = B['Reveal']
LIMIT, L_REV = REV['cu_limit'], REV['loaded_limit']
def cost(limit, locks, L, sigs=1):
    return limit + 720 * sigs + 300 * locks + 8 * math.ceil(L / 32_768)
COST2, COST3 = cost(LIMIT, 2, L_REV), cost(LIMIT, 3, L_REV)
TIP_MIN = math.ceil(P_MIN * (LIMIT + 1_320 + 8 * math.ceil(L_REV / 32_768))) + 2_500   # §10.1 (2 locks)
def prio(tip, c): return (tip - 2_500) / c
def fee(p, c): return max(0.0, p * c - 2_500)
def refund(c):  # fees::defence_refund at P_def, ev_price·ev_limit = fee(P_def, c)
    return max(0, min(math.ceil(fee(P_DEF, c)), P_DEF * c - 2_500) - (TIP_MIN - 2_500))

def q(v, x):
    s = sorted(v); return s[min(len(s) - 1, int(x * len(s)))] if s else None
def dist(v):
    v = [x for x in v if x is not None]
    return f'n {len(v)}, p50 {q(v,.5):,}, p99 {q(v,.99):,}, max {max(v):,}' if v else 'n 0'

print('# Restated C4 model v3, final for M1 (CL-26 / CL-30 / I-49)   SOL = $150\n')
print('## 1. Measured Reveal inputs [measured]\n')
print(f"- Budgets table (`budgets.json` {json.load(open(args.budgets)).get('abi_version')}, release `.so` placeholder length "
      f"{json.load(open(args.budgets))['constants']['placeholder_so_len']:,} B, programdata "
      f"{json.load(open(args.budgets))['constants']['placeholder_programdata_len']:,} B):")
print(f"  Reveal gate {REV['cu_budget']:,} CU; **CU limit requested {LIMIT:,}** (G1 worst + 5 %); **L(reveal) = {L_REV:,} B** "
      f"({math.ceil(L_REV/32768)} pages of 32 KiB → +{8*math.ceil(L_REV/32768)} cost units); tx worst {REV['tx_worst_estimate']} B")
rows = json.load(open(args.reveal_rows)) if args.reveal_rows and os.path.exists(args.reveal_rows) else []
if rows:
    runs = sorted(set(r['run'] for r in rows))
    print(f"- In play ({', '.join(runs)}): program CU {dist([r['prog_cu'] for r in rows])}; "
          f"whole-tx units {dist([r['tx_units'] for r in rows])}; first reveal of the province-bell (ArrivalDay written): "
          f"{sum(1 for r in rows if r['arrivalday_written'])} of {len(rows)}")
for f in args.svm_log:
    by = {}
    for line in open(f):
        p = line.split()
        if len(p) >= 3 and p[1] == 'Reveal' and p[0] in ('Release', 'TestBeacon'):
            by.setdefault(p[0], []).append(int(p[2]))
    for b, v in sorted(by.items()):
        print(f'- svm suite, {b} build: whole-tx units {dist(v)} (every landed single-Reveal transaction of the suite)')
print(f'\nCost of a Reveal (§10.1): {COST2:,} cost units (2 write locks) / **{COST3:,} (3, first of a province-bell)**; '
      f'`tip_min` = {TIP_MIN:,} lamports (presets {TIP_MIN:,} / {math.ceil(1.5*TIP_MIN):,} / {2*TIP_MIN:,}).')
print(f'Keeper priority when it spends exactly `tip_min`: {prio(TIP_MIN, COST2):.4f} (2 locks) / **{prio(TIP_MIN, COST3):.4f}** (3 locks). '
      f'A bid at P_def 2.0 costs a priority fee of {fee(P_DEF, COST2):,.0f} / {fee(P_DEF, COST3):,.0f} lamports; '
      f'defence refund per Reveal {refund(COST2):,} / {refund(COST3):,} lamports. Relic tip priority {prio(RELIC_TIP, COST2):.2f}.')
if rows:
    mx = max(r['prog_cu'] for r in rows)
    tight = math.ceil(mx * 1.05 / 500) * 500
    print(f'Sensitivity (not adopted): a keeper that requested the in-play maximum + 5 % ({tight:,}) instead of {LIMIT:,} '
          f'would reach priority {prio(TIP_MIN, cost(tight, 2, L_REV)):.3f} at `tip_min` (+{prio(TIP_MIN, cost(tight,2,L_REV))/prio(TIP_MIN, COST2)*100-100:.0f} %); '
          f'a Reveal above its request fails and re-sends at the ladder (I-50), so the season keeps the G1 limit.')
print()

mb = json.load(open(os.path.join(args.spfee, 'mainnet-blocks-m0c.json')))
ORG_COST = mb['summary']['mean_non_vote_cost_units_per_block']
ORG_SHARE = {float(k): v for k, v in mb['summary']['non_vote_cost_share_at_or_above'].items()}
def organic_above(p):
    ks = sorted(k for k in ORG_SHARE if k <= p + 1e-9)
    return (ORG_SHARE[ks[-1]] if ks else ORG_SHARE[min(ORG_SHARE)]) * ORG_COST
CAPS = {'exec-bound [estimate]': (SLOT_MAINNET, 55e6 * SLOT_MAINNET, 150e6 * SLOT_MAINNET),
        'design 100M/0.4 s [model]': (SLOT_DESIGN, ACCT_CAP, BLOCK_CAP),
        'unscaled 100M/0.265 s [model]': (SLOT_MAINNET, ACCT_CAP, BLOCK_CAP)}
def side_cost_sol(p, W, cap, payers=None, stream='alt'):
    slot, acct, block = CAPS[cap]
    block_net = max(0.0, min(block, BLOCK_CAP) - organic_above(p))
    cu = block_net if payers is None else min(math.ceil(payers / KEYS_PER_STREAM[stream]) * min(acct, ACCT_CAP), block_net)
    return (W / slot) * p * cu / LAM
def province_cost_sol(p, W, cap):
    slot, acct, _ = CAPS[cap]
    return (W / slot) * p * min(acct, ACCT_CAP) / LAM
def rng(fn):
    v = [fn(c) * SOL_USD for c in CAPS]; return min(v), max(v)

def load(name):
    for d in (args.sim_dir, os.path.join(HERE, '..')):
        p = os.path.join(d, name)
        if os.path.exists(p): return json.load(open(p)), p
    return None, None
base, base_p = load('c4_50k_s3.md.json')
relic, relic_p = load('c4_50k_s1_relics.md.json')
small, small_p = load('c4_10k_s3.md.json')
cases = {}
print(f'## 2. Value of one bell, valuation (a) (N2) [sim]\n')
print('Inputs: ' + ', '.join(f'`{os.path.basename(x)}`' for x in (base_p, relic_p, small_p)) + f' (in `{os.path.dirname(base_p)}`).')
parts99 = [s['part']['p99'] for s in base['seeds']]; partsmx = [s['part']['max'] for s in base['seeds']]
print(f"- 50k wallets, seeds {[s['seed'] for s in base['seeds']]}: busiest faction's clashes per bell p99 {parts99}, max {partsmx}; "
      f"with its own arrivals p99 {[s['arr']['p99'] for s in base['seeds']]}, max {[s['arr']['max'] for s in base['seeds']]}")
cases['(a) p99 bell'] = max(parts99) * V_CLASH
cases['(a) max bell'] = max(partsmx) * V_CLASH
cases['(a) max bell + 5 relic clashes at $150'] = max(partsmx) * V_CLASH + 5 * V_RELIC
if relic:
    s0 = relic['seeds'][0]
    cases['(a) max bell, relics on + 5 relic clashes'] = s0['part']['max'] * V_CLASH + 5 * V_RELIC
    rr = s0['writes']['relic_reveals'][:s0['writes']['bells']]
    print(f"- 50k with --relics (seed {s0['seed']}): busiest p99 {s0['part']['p99']}, max {s0['part']['max']}; relic-province reveals per bell p99 {q(rr,.99)}, max {max(rr)}")
print('- values: ' + '; '.join(f'{k} ${v:,.0f}' for k, v in cases.items()))
print()

print('## 3. Attack cost and C4 verdicts, whole side, valuation (a), PASS ≥ 10× [model]\n')
print('Ratio range: the lowest capacity case against the largest value case (max bell + 5 relic clashes) → the highest capacity case against the p99 bell. PASS = every combination ≥ 10×; FAIL = none.\n')
print('| keeper bid | payers | window | attack cost (3 capacity cases) | attack ÷ value, worst case → best case | verdict |')
print('|---|---|---|---|---|---|')
BIDS = ((f'tip_min, 2-lock Reveal (p {prio(TIP_MIN, COST2):.3f})', prio(TIP_MIN, COST2)),
        (f'tip_min, first-of-bell Reveal (p {prio(TIP_MIN, COST3):.3f})', prio(TIP_MIN, COST3)),
        ('defence-pool cap P_def 2.0', P_DEF))
PAYERS = (('1 known payer', 1), ('≥ 150 rotating payers', 150))
verdicts = []
for bl, p in BIDS:
    for pl, n in PAYERS:
        for W in (600, 1_200):
            lo, hi = rng(lambda c: side_cost_sol(p, W, c, n))
            worst_v, best_v = max(cases.values()), min(cases.values())
            rlo, rhi = lo / worst_v, hi / best_v
            v = 'PASS' if rlo >= 10 else ('FAIL' if rhi < 10 else 'depends on capacity / case')
            verdicts.append((bl, pl, W, rlo, rhi, v))
            print(f'| {bl} | {pl} | {W} s | ${lo:,.0f} – ${hi:,.0f} | {rlo:.1f}× – {rhi:.1f}× | {v} |')
print()
print('## 4. Relic clashes at the relic tip [model]\n')
pr = prio(RELIC_TIP, COST2)
for W in (600, 1_200):
    lo, hi = rng(lambda c: province_cost_sol(pr, W, c))
    print(f'- relic tip priority {pr:.2f}: excluding one relic province for {W} s costs ${lo:,.0f} – ${hi:,.0f} '
          f'({lo/V_RELIC:,.0f}× – {hi/V_RELIC:,.0f}× a $150 relic clash)')
print()

print('## 5. Keeper reveal-payer band from R99 (I-49) [model]\n')
per_rev = RENT_SLOT + RENT_DAY + math.ceil(fee(P_DEF, COST3))
def floor(r99, n=150): return 3 * math.ceil(r99 / n) * per_rev
print(f'Per reveal a payer fronts slot rent {RENT_SLOT:,} + day rent {RENT_DAY:,} + the fee at P_def {math.ceil(fee(P_DEF, COST3)):,} = {per_rev:,} lamports.')
r99s = []
for label, js in (('10k [sim]', small), ('50k [sim]', base)):
    if js:
        r = max(s['writes']['r99'] for s in js['seeds']); r99s.append((label, r))
if rows:
    per = {}
    for x in rows: per[(x['run'], x['arrive'])] = per.get((x['run'], x['arrive']), 0) + 1
    r99s.append(('in play, stack runs [measured]', q(list(per.values()), .99)))
r99s.append(('contract default (fclient R99_DEFAULT)', 4_000))
for label, r in r99s:
    print(f'- R99 {label} = {r:,} → F_r = 3 × ⌈{r:,}/150⌉ × {per_rev:,} = {floor(r)/LAM:.4f} SOL per payer; 150 payers hold {150*floor(r)/LAM:,.2f} – {300*floor(r)/LAM:,.2f} SOL (floor – ceiling 2 F_r)')
print(f'- F_r is flat for every R99 ≤ 150 ({floor(150)/LAM:.4f} SOL) and steps by {3*per_rev/LAM:.4f} SOL per further 150 reveals per bell.')
print()

print('## 6. D18: defence-pool spend per attacked bell (CL-30 / CL-31a), at the budgets table [model on sim counts]\n')
def kcost(name, limit=None):
    b = B[name]; return cost(limit or b['cu_limit'], b['write_locks_worst'], b['loaded_limit'])
C = {k: kcost(k) for k in ('GatherClash', 'ResolveFromInputs', 'SettleTransit', 'SettleDeparture', 'PostAnchor', 'PostSeed')}
C_RFI_B = kcost('ResolveFromInputs', 288_000)
print('Costs per write (limit + locks + L): ' + ', '.join(f'{k} {v:,}' for k, v in C.items()) + f'; RFI with Phase B {C_RFI_B:,}')
SUB = refund(COST3)
print(f'Pool pays only Reveal (class W): refund ≤ {SUB:,} lamports per defended Reveal (first-of-bell cost, the larger).')
def per_bell(w, b, rfi_cost):
    r, g, rs, st = w['reveals'][b], w['gathers'][b], w['resolves'][b], w['settles'][b] // 2
    def delay(p):
        return (g * fee(p, C['GatherClash']) + rs * fee(p, rfi_cost)
                + st * (fee(p, C['SettleTransit']) + fee(p, C['SettleDeparture']))
                + 16 * (fee(p, C['PostAnchor']) + fee(p, C['PostSeed'])))
    return r * SUB + delay(P_DEF), r * SUB, delay(P_DELAY), r
print('\n| wallets | reveals per bell p50 / p99 / max | (A) every critical write at 2.0: p99 SOL | (B) Reveal only: p50 / p99 / max SOL | 20 SOL covers (B, p99 bells) | keepers\' own spend in (B), p99 SOL: Phase A / Phase B RFI |')
print('|---|---|---|---|---|---|')
rec = []
for label, js in (('10,000', small), ('50,000', base)):
    if not js: continue
    A, Bp, K, KB, R = [], [], [], [], []
    for sd in js['seeds']:
        w = sd['writes']
        for b in range(w['bells']):
            a, bp, k, r = per_bell(w, b, C['ResolveFromInputs'])
            _, _, kb, _ = per_bell(w, b, C_RFI_B)
            A.append(a); Bp.append(bp); K.append(k); KB.append(kb); R.append(r)
    p99 = q(Bp, .99)
    rec.append((label, p99))
    print(f'| {label} | {q(R,.5)} / {q(R,.99)} / {max(R)} | {q(A,.99)/LAM:.3f} | {q(Bp,.5)/LAM:.4f} / {p99/LAM:.4f} / {max(Bp)/LAM:.4f} | '
          f'{20*LAM/p99:,.0f} | {q(K,.99)/LAM:.3f} / {q(KB,.99)/LAM:.3f} |')
print()
for label, p99 in rec:
    print(f'- {label}: 100 p99 attacked bells need {100*p99/LAM:.2f} SOL → ' + ('**keep 20 SOL**' if 20*LAM/p99 >= 100 else f'size to {math.ceil(100*p99/LAM)} SOL'))
print(f'- Per-bell cap (CL-31a): R_bell × {SUB:,} lamports, R_bell from the folded reveal count.')
