# C4 model v3, final for M1 (unit W6-E)

The committed copy of the wave-6 model run (M1 contract CL-26, CL-30, I-49; exit item E8). Tables and conclusions: [`../../DESIGN.md`](../../DESIGN.md) §22.2–§22.4; decisions: [`../../DECISIONS.md`](../../DECISIONS.md) part P.

| File | What |
|---|---|
| `reveal_cu_from_runs.py` | Extracts every landed Reveal of `frontier-stack` runs (`<run>/verify/input.json.gz`: wire bytes, logs, consumed units) and of svm CU logs: program CU, whole-transaction units, requested CU and loaded-data limits, write locks, whether the ArrivalDay was written, path length, path provinces, reveals per arrival bell |
| `reveal-cu.txt`, `reveal-rows.json` | its output over `nightly-20260928` (W5-B), `w5-smoke` (integ-W5) and `w6a-real1` (W6-A's archive smoke on the release `.so`) — 18 in-play Reveals |
| `svm-reveal-cu.log` | the Reveal lines of a full `svm-tests/run.sh --release` with `PSF_CU_LOG` (release + test-beacon builds, W6-E base, 126 Reveals) |
| `c4_model_v3_final.py` | the model: W1-D's `c4_model_v3.py` and `d18_model.py` with the measured Reveal inputs, the budgets table's limits, write locks and `L(kind)`, the program's refund formula and both RFI limits (Phase A / B) |
| `c4-model-v3-final.txt` | its output |

Inputs outside the repository (lab, `(session scratch)/scratchpad/frontier/`): `m0b/spikes/SP-FEE/results/mainnet-blocks-m0c.json` (organic mainnet fill, read-only RPC, m0c) and the `frontier-sim c4` JSON re-run at the W6-E base in `m1/lab/c4-v3/w6e/` (`c4 --agents 50000 --seeds 3`, `--agents 10000 --seeds 3`, `--agents 50000 --seeds 1 --relics`).

Re-run (after the latency run or the 7-day season, only the run list changes):

```sh
cd .claude/worktrees            # the directory holding the unit worktrees
python3 <repo>/docs/frontier/m1/c4-v3/reveal_cu_from_runs.py --svm <repo>/docs/frontier/m1/c4-v3/svm-reveal-cu.log \
    --json /tmp/reveal-rows.json <run dir> [<run dir> ...] > /tmp/reveal-cu.txt
python3 <repo>/docs/frontier/m1/c4-v3/c4_model_v3_final.py \
    --budgets <repo>/frontier-abi/vectors/budgets.json \
    --spfee <scratch>/frontier/m0b/spikes/SP-FEE/results \
    --sim-dir <scratch>/frontier/m1/lab/c4-v3/w6e \
    --reveal-rows /tmp/reveal-rows.json --svm-log <repo>/docs/frontier/m1/c4-v3/svm-reveal-cu.log > c4-model-v3-final.txt
```

Only the in-play rows (§22.2) and the measured R99 (§22.4) depend on the run list: every C4 and D18 figure is priced at the *requested* limits of the budgets table, which bound every measured Reveal.
