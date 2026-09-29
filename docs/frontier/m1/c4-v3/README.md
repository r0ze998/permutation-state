# C4 model v3, final for M1 (unit W6-E; sample and pricing updated by integ-W6r)

The committed copy of the wave-6 model run (M1 contract CL-26, CL-30, I-49; exit item E8). Tables and conclusions: [`../../DESIGN.md`](../../DESIGN.md) §22.2–§22.4; decisions: [`../../DECISIONS.md`](../../DECISIONS.md) part P (W6-E) and part R (integ-W6r).

| File | What |
|---|---|
| `reveal_cu_from_runs.py` | Extracts every landed Reveal of `frontier-stack` runs (`<run>/verify/input.json.gz`: wire bytes, logs, consumed units) and of svm CU logs: program CU, whole-transaction units, requested CU and loaded-data limits, write locks, whether the ArrivalDay was written, path length, path provinces, reveals per arrival bell |
| `reveal-cu.txt`, `reveal-rows.json` | its output over **416 in-play Reveals** (integ-W6r): W5-B's `nightly-20260928` (5), W6-A's `w6a-real1` (10, the archive smoke on the release `.so`), `w6a-nightly-2` (8) and `w6a-nightly-3` (3), integ-W6's Phase B runs `w5-smoke` (10), nightly 1 `nightly-20260928` (53), `integ-w6-nightly-2` (56), `integ-w6-nightly-3` (56), integ-W6r's Gate W6 re-run `nightly-20260929` (58), `integ-w6r-nightly-2` (66), `integ-w6r-nightly-3` (68) and the latency run `w6-latency` (23, after the re-run's extra `verify`); `w6a-nightly-1` lists 0 (no Reveal landed). **Run directories under `frontier-node/.local/frontier/` are mutable** (a run id reused overwrites them: integ-W6's `w5-smoke` is now a Phase B run, W6-E's 3-row `w5-smoke` sample with `.so` `094dc2d6…` can no longer be regenerated from that path); **`reveal-rows.json` is the record**, the text is derived from it |
| `svm-reveal-cu.log` | the 126 Reveal lines (10 `Release`, 116 `TestBeacon`) of a full `svm-tests/run.sh --release` with `PSF_CU_LOG` at the W6-E base (the lab file `(session scratch)/frontier/m1/lab/c4-v3/w6e/cu.log` holds every instruction; committed by integ-W6r, the review found it referenced but missing) |
| `c4_model_v3_final.py` | the model: W1-D's `c4_model_v3.py` and `d18_model.py` with the measured Reveal inputs, the budgets table's limits, write locks and `L(kind)`, the program's refund formula. Priced at the **merged** table (Phase B committed: ResolveFromInputs limit 285,500, gate 290,000); the W6-E base run's Phase A column is in git history (`46a3cfa`) |
| `c4-model-v3-final.txt` | its output on the merged table and the 416-Reveal sample (integ-W6r). The `Inputs:` line of §2 prints the lab directory's absolute path; the committed copy writes it as `(session scratch)/…` by hand — the only edit |

Inputs outside the repository (lab, `(session scratch)/scratchpad/frontier/`): `m0b/spikes/SP-FEE/results/mainnet-blocks-m0c.json` (organic mainnet fill, read-only RPC, m0c) and the `frontier-sim c4` JSON re-run at the W6-E base in `m1/lab/c4-v3/w6e/` (`c4 --agents 50000 --seeds 3`, `--agents 10000 --seeds 3`, `--agents 50000 --seeds 1 --relics`; before Phase B's variance stream — the counts move by ≤ 2 per bell at 10k on the merged tree, W6-E notes §2).

Re-run (after a latency run with a `verify` input or the 7-day season, only the run list changes; verified from the repository copy by integ-W6r):

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

A `frontier-stack` run has a `verify/input.json.gz` only after `frontier-stack verify --run-id <id>` ran while its services were up (the nightly script and the Gate W5 lines do; the Gate W6 latency line does not — run `verify` before its `report && down`).

Only the in-play rows (§22.2) and the measured R99 (§22.4) depend on the run list: every C4 and D18 figure is priced at the *requested* limits of the budgets table, which bound every measured Reveal.
