# W6-E c4-docs: unit notes

- **Unit:** W6-E (M1 contract v1.9 §11, wave 6, pass 1). **Branch:** `frontier/m1-W6-E`, cut from `frontier/m1-integ` at `0514b06`. Worktree `.claude/worktrees/m1-W6-E`. **No push.** No devnet or mainnet transaction, no devnet read, no service started, no port bound (the svm suite and the simulator are host processes; LiteSVM binds nothing).
- **Owner decisions applied (wave-6 brief):** O-M1-01…24 working defaults; O-M1-12 items 1–3 approved (the archive is complete; this unit read W6-A's archive run, fetched nothing); Agave ≥ 4.0 not approved; O-M1-18 (devnet playtest) **not approved** — the runbook is configuration only; the rustfmt/clippy install for 1.95.0 is already recorded in DECISIONS part A (checked, not duplicated); the archive's completion is now recorded there too (relayed fact).
- **Owned paths touched:** `docs/frontier/{DESIGN.md, DECISIONS.md, README.md, SUMMARY.ja.md}`, new `docs/frontier/m1/{RUN-A-KEEPER.md, PLAYTEST-RUNBOOK.md, W6-E-NOTES.md}`, new `docs/frontier/m1/c4-v3/**`, lab `scratchpad/frontier/m1/lab/c4-v3/w6e/**`. Nothing else.
- **Tags:** [measured] run here or read from a run directory; [sim] `frontier-sim`; [model] arithmetic on stated inputs.

## 1. What landed

| Brief item | Where | Summary |
|---|---|---|
| **c4 model v3 with the measured Reveal CU distribution and `L(reveal)`** (CL-26 final, E8) | DESIGN §22.2; `m1/c4-v3/` (`c4_model_v3_final.py`, `c4-model-v3-final.txt`, `reveal_cu_from_runs.py`, `reveal-cu.txt`, `reveal-rows.json`, `svm-reveal-cu.log`, README) | Reveal cost at the measured limit 26,500 and `L(reveal)` 1,146,880 → 28,100 cost units (28,400 on a first reveal of a province-bell) → `tip_min` 14,668; keeper at `tip_min` bids 0.433 (0.428); verdicts: fails at the minimum tip, **passes at P_def 2.0 with ≥ 150 rotating payers, 11.7–47.9× (600 s), 23.4–95.9× (1,200 s)** on valuation (a) [model] |
| **D18 table** (CL-30 final) | DESIGN §22.3, §14 D18; DECISIONS P2, part D | priced at the budgets table's limits, locks and `L(kind)` and the program's refund formula: 0.0021 / 0.0102 SOL per attacked p99 bell at 10k / 50k → keep 20 SOL; the preset caps (0.2 SOL per bell-region, 2 SOL per keeper-day) adequate as final values |
| **Payer band R99** (I-49) | DESIGN §22.4; DECISIONS P3; keeper guide §4.3 | `F_r` 0.0080 SOL for R99 ≤ 150, 0.0159 at 50k, 0.2151 at the code default 4,000; recommendation `r99_reveals` = 150 up to ≈ 30k wallets, 300 up to 50k |
| **DESIGN updates for the M1 decisions** (incl. §6.2 without ProveBadSeal, §2.2 cohorts, §8.6 lock table) | DESIGN status bullet, §2.2, §6.2, §6.3, §6.4, §8.1–§8.8, §9.1, §9.4, §10, §11, §12, §14 D8/D18, §21.2–§21.3 (superseded notes), new §22 | each change marked "(M1, I-xx)" in place and listed in §22.1; revision-3.1 text kept where it is history, struck or labelled "superseded" where M1 replaced it |
| **"Run a keeper" doc** | `m1/RUN-A-KEEPER.md` | what a keeper needs, the 16 roles, bidding classes, retry ladder, payers and funders, the band, every `keeper.toml` key, start/stop and crash safety, monitoring and alerts, economics, and the six gaps before a public network |
| **Playtest runbook (devnet configuration only, no devnet step)** | `m1/PLAYTEST-RUNBOOK.md` | preconditions P1–P5, components, gaps G1–G8 that block a devnet run, steps with **[approval]** marks, relay/keeper/herald configuration, a test-SOL budget (≈ 100–135 SOL as the preset stands, ≈ 37–43 SOL with a smaller ProvinceFund and pool), watching and stopping, what the owner decides |
| Links | `README.md`, `SUMMARY.ja.md` (one Japanese paragraph for the owner) | — |

## 2. What was measured and where the Reveal distribution comes from

**Reveal CU [measured]** — the sources, as the brief asks ("say which"):

| Source | Reveals | Program CU p50 / p99 / max |
|---|---|---|
| `nightly-20260928` (W5-B's nightly: test key, 100 bots × 1 game day at 100×, pre-wave-5 `.so` 1,032,688 B) | 5 | 19,826 / 20,985 / 20,985 |
| `w5-smoke` (integ-W5's Gate W5 run: test key, `.so` 874,624 B) | 3 | 17,402 / 19,752 / 19,752 |
| `w6a-real1` (**W6-A's archive smoke with real quicknet rounds and the release `.so` 874,120 B, sha256 `1b1968af…06fc`**; verify PASS) | 10 | 19,911 / 21,418 / 21,418 |
| **pooled in play** | **18** | **19,772 / 21,418 / 21,418** (whole transaction ≈ +450) |
| svm suite, every landed single-Reveal transaction, run here (`PSF_FEATURES="test-beacon" PSF_CU_LOG=… svm-tests/run.sh --release --no-fail-fast`) | 126 | 20,172 / 25,155 / 25,155 (units per transaction; the G1 worst 25,155 is `budgets::MEASURED`) |
| `w6a-nightly-1` (W6-A's first wave-6 nightly: test key, 100 bots × 1 game day, verify PASS; read after it finished) | 0 | — (its bots marched none) |
| **not included** | — | W6-A's scale-2 latency run (`w6-latency`, running, ≈ 3 h), the 7-day season `w6-s7` (main session, after wave 6) |

Every in-play Reveal so far was the first of its province-bell (ArrivalDay written, 3 write locks) and ≤ 1 reveal per bell: the bots of these runs march rarely (W6-C's `--day0-share`/`--eager-personas` fix is wave 6). The model is re-run in one command from any run directory (`m1/c4-v3/README.md`); only the in-play rows and the measured R99 can move, since the scheduler prices the *requested* limit.

**`L(reveal)`** 1,146,880 B (35 pages) from `frontier-abi/vectors/budgets.json`; at the actual W6-base release `.so` (programdata 1,093,632 B) the need is 1,124,234 B, the same 35 pages [measured formula].

**Value side [sim]:** `frontier-sim c4` re-run at the W6-E base (`--agents 50000 --seeds 3`, 9 min 15 s; `--agents 10000 --seeds 3`, 71 s; `--agents 50000 --seeds 1 --relics`, 10 min 47 s on a machine shared with W6-B's doctrine gates): p99 bell $1,407, max $1,512, $2,262 with five relic clashes, $2,227 with Relic Sites on; R99 49–50 (10k), 243–246 (50k). Before Phase B's variance stream (W6-B commits it). **integ-W6r (review response):** the clash counts do depend on the variance stream, slightly — re-run on the merged Phase B tree (`c4 --agents 10000 --seeds 3`) the per-bell counts move by ≤ 2 at 10k (numbers in `integ-W6r-NOTES.md` §2) and the total reveals by < 1 %; the C4 margins (11.7× at the worst case) absorb it; the 50k × 3 and relics inputs are re-run in the triage pass.

**svm run:** 242 passed, 2 failed, 4 ignored; the 2 failures are by construction: `g08_gathers_in_any_order_equal_the_oracle_and_the_kernel` and `g01_resolve_from_inputs_all_fills` need the `oracle` build, which this run did not build (`PSF_FEATURES="test-beacon"`, to save time; they panic "no Oracle build … PSF_SO_ORACLE unset"). Every other test passed. The run's purpose was the CU log, not a gate; the gate's full svm line is the integrator's.

## 3. Gate W6 items for these files

| Item | Result |
|---|---|
| "the c4 v3 report cites the measured Reveal CU distribution and `L(reveal)`" | met: DESIGN §22.2 and `m1/c4-v3/c4-model-v3-final.txt` cite the in-play distribution (three runs, 18 Reveals, named), the svm distribution (126), the G1 worst and `L(reveal)` = 1,146,880 B |
| `git diff --check` | clean |
| Markdown table shape check (every row has its header's column count) over the touched files | clean |
| Committed model reproduces its committed output from the repo copy (`--spfee`, `--sim-dir` to the lab) | identical (`diff` empty) |
| Everything else in Gate W6 (nightlies, latency run, w6-s7 and its verify/tamper/report, onboarding run) | not this unit's; not run here |

No line of this unit is `PENDING-OWNER`.

## 4. Deviations and judgement calls

1. **The in-play sample is 18 Reveals**, all first-of-bell. The brief asked for the distribution "measured so far"; the latency run and the 7-day season will be larger. The report says so and the re-run is one command.
2. **The svm suite was run without the `trace` and `oracle` builds** (two tests fail for that reason alone, §2).
3. **The verdict table of §22.2 is condensed** from v3's 96 rows to 12, each giving the range from the worst to the best (capacity, value) combination; the full per-case numbers are in the committed output. The first-of-bell rows (priority 0.428) are new.
4. **The model prices D18 at the budgets table's CU *limits*** (what keepers request), where W1-D's `d18_model.py` used the *gates*; the pool's refund uses the program's formula (`fees::defence_refund`) instead of `(P_def − p_tip) × cost`. Both make the table what the program and keeper actually do; the difference is ≤ 4 %.
5. **The playtest budget's "smaller option"** (ProvinceFund for rings ≤ 6, a 2-SOL pool) is a suggestion for the owner; it needs a preset change in `frontier-abi` (not this unit's) and is marked so.
6. **DECISIONS part A gained a relayed fact row** (the archive completed, the wave-6 split). The integrator may prefer to own that row; it records only what the brief relayed.
7. **DESIGN keeps revision 3.1's ProveBadSeal/SealVerdict text** as labelled history (struck rows, "superseded" labels) instead of deleting it, as §21 did for wave 1; §0.x, §19 and §20 (revision notes) are unchanged history.
8. **Commit trailer (recorded by integ-W6r):** both W6-E commits (`3769e9e`, `46a3cfa`) end with `Co-Authored-By: Claude Opus 5.5` although the wave-6 brief's TRAILER OVERRIDE named `Claude Fable 5.1`; they are merged and not rewritten; every follow-up commit uses the overridden trailer.

## 5. Findings for other units and the owner

| # | Finding | For |
|---|---|---|
| F1 | The keeper's reveal-pool floor is computed at the Reveal **budget** (26,000) and a 1-MiB loaded-data limit (`keeper/src/pools.rs` `Payers::new`), not the requested limit (26,500) and `L(reveal)` (1,146,880): ≈ 3,000 lamports low per payer (0.04 %). Cosmetic | W6-C |
| F2 | `fclient::payers::R99_DEFAULT` = 4,000 sizes each reveal payer at 0.215 SOL (32–65 SOL per 150-payer pool); the measurement and the simulator support 150 (≤ 30k wallets) or 300 (50k): 27× less float. A config value; the code default waits for the owner (DECISIONS P3) | owner / W6-C / W6-A configs |
| F3 | The keeper binary cannot run against a public RPC (feed via `frontier_feed` only; no TLS client) and cannot print its payer/funder addresses; the operator steps (Announce/Create/Init) exist only inside `frontier-stack` | later waves (runbook G1–G5) |
| F4 | `M1_LOCAL_7D.per_bell_region_cap` (0.2 SOL) and `per_keeper_day_cap` (2 SOL) are marked "[placeholder] until CL-30" in `frontier-abi/src/presets.rs`: CL-30 (this unit) finds both adequate as final values; the comment can drop "placeholder" | W6-B (`frontier-abi`) |
| F5 | The delay-pool floor default (0.5 SOL per payer, 16–32 SOL per 32-payer pool) is sized for a large season; a 200-player playtest needs ≈ 0.011 SOL by the `F_d` formula (the runbook configures 0.05) | owner / keeper config |

## 5b. Dependency requests

None (no manifest, lockfile, toolchain or `.gitignore` change; the scripts use the Python 3 standard library only).

## 6. Pending

- ~~Re-run the model with W6-A's `w6-latency` run directory when it finishes~~ integ-W6r re-ran it over thirteen run directories (416 in-play Reveals, the latency run's 23 included: the Gate W6 latency line writes no `verify` input, so the integ-W6r re-run added an extra `verify` before `report && down`) and on the merged budgets table (DESIGN §22.2–§22.4 updated; `svm-reveal-cu.log` committed). Still to do: `w6-s7` after the main session's 7-day season (commands in `m1/c4-v3/README.md`).
- Owner confirmations: D18 (keep 20 SOL, caps final), the R99 recommendation, and — separately and only if wanted — O-M1-18 (the playtest), for which the runbook's gaps G1–G8 must close first.
