# M1 "First Bell": exit report (W7-A / W7-C)

- **Role:** W7-A (exit run records, the non-season §13 items) and W7-C (exit report, decisions). **Branch:** `frontier/m1-integ` (worktree `.claude/worktrees/m1-integ`). **Contract:** [`M1-CONTRACT.md`](M1-CONTRACT.md) v1.13, §12 Gate W7 = §13 in full. **Date:** 2026-10-01.
- **Exit commit:** **`1e5701b`** (`frontier/m1-integ` = `codex/frontier`; its code is that of `629d007`, the integ-W6t review head: the two later commits change `docs/` only). No code was changed in this step.
- **Release `.so`:** **`d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281`** (875,824 B, SBPF v2, `program_hash` `8f93e44e…d91c`, `max_len` 1,097,728). Rebuilt twice in this step (`scripts/build-frontier.sh --twice`): the same hash, so the exit season ran the program this tree builds.
- **Rules followed:** no push; no devnet or mainnet transaction; no install or download; `permutation-server/web/session.mjs` unchanged (Gate W1 line 13); the main tree, `codex/magicblock-playable` and `codex/v9-security` untouched. The exit season's services stay up with the chain paused on 41000–41099 for the owner to spectate; they were only read (one `getBalance` query each for the two keeper beneficiaries, §4.2). Every service this step started used 41200–41799 and was stopped.
- **Logs:** `(session scratch)/scratchpad/m1-exit/gate/` (`gate-w7.sh`, `{A,L,O,B,N,N2}-run.txt`, `logs/`); E8 working files in `(session scratch)/scratchpad/m1-exit/e8/`.

## 1. Verdict

**The exit season passed; the M1 exit is not fully green on `1e5701b`.** The 7-day, 1,000-bot real-round season `m1-exit` met every gating §13.4 criterion (1–6, 8, 9; 7 is reported, not gating) in an exit-grade environment, the verifier passed it and failed all 30 tamper classes, and every other §13 item passed except one: **G14 (and Gate W4's `inproc_day`) exit 101** on their resident-liveness condition. The cause is in the test, not in the game: since the integ-W6t review (`33f77cc`, `de9bb3d`) the `settle_racer` persona tries its re-depart every 8 game seconds and the relay refuses the early tries `NotResident` in simulation (nothing is sent), and the condition counts those designed refusals as liveness misses. Every refused Depart in both tests is the racer's (31 of 31 in `inproc_day`, 116 of 116 in a G14 re-run); without them the rate is 0 %. Every property §13.3 names for G14 passed inside the failing test.

A fix is a test-only change in `frontier-node/crates/itest` (count the racer's race tries apart, as the stack report already does) plus a re-run of those two tests. It touches neither the program nor the keeper, so under §11 (W7-A re-runs the season only after a fix that touches the program or the keeper) it does not need a new season, but it moves the exit commit. It was not made here (this step changes no code).

One finding outside the pass criteria: **no ClaimDefence has ever landed in a stack run, the exit season included**, because the keepers' beneficiary pays that transaction's fee and the stack never funds it (§4.2). The defence refund path is proven only in the svm suite and unit tests.

## 2. The §13 items

Tree `1e5701b`; binaries as the exit season used them (`frontier-node/target/release`, rebuilt by the gate's own cargo lines with no change); `.so` as above. "Part A" is the gate script's no-port part (02:33–03:12 JST); the stack lines ran at 41200 (latency), 41500 (nightlies), 41600 (W5 smoke) and 41700 (onboarding).

| Item | What | Result | Evidence |
|---|---|---|---|
| **E1** (G1) | Every instruction within its §5.5 ceiling, heap ≤ 28 KiB, tx ≤ 1,232 B, locks ≤ 64 at the named worst case; `g01_loaded_limit_*` | **pass** | `RELEASE_CHECK=1 PSF_CU_LOG=… ./run.sh --release`: 248 passed, 0 failed, 4 ignored by design (the validator drill and three `zz_profile_*` diagnostics), 44 `g01_*` tests. `cu-table.py --check` exit 0 over 26,883 logged transactions of 49 kinds: every kind within its gate (tightest PostAnchor 339,142 / 345,000; ResolveFromInputs 271,673 / 290,000 incl. all 1,200 native-screen fills and the I-43 storage fill; Reveal 25,155 / 26,000; ClaimDefence 24,111 / 25,500 with 6 slots and caps binding; SkipQuiet 170,412 for a 24-bell batch, within its per-bell budget). Largest transaction of the release and test-beacon builds 1,218 B (GatherClash), most locks 32 (the test-only `oracle` ResolveClash, never deployable, reaches 2,004 B and 56 locks). `trace-sweep.sh`: 26,804 transactions on the trace builds, **0 heap-gate failures**, heap max 15,320 B (ResolveFromInputs) over 49 kinds. Gate W3's trace line: Reveal worst (named) 24,300 CU plain / heap 2,680 B, 916 B; Join over 1,001 wallets worst 12,102; OpenProvince over 324 provinces of rings 2–10 worst 148,459 |
| **E2** (G2) | Pre-funding on 19 creation paths of 17 kinds; pre-funded, never-created slots absent | **pass** | 22 `g02_*` tests green in every svm run (Gate W2, W3, W4 lines and `RELEASE_CHECK=1`) |
| **E3** (G3) | Forgery per keyed account; re-creation after close (I-46) | **pass** | 19 `g03_*` tests green; re-creation cases in `g03_`/`g13_` |
| **E4** G4–G13 | Program-level property tests (LiteSVM) | **pass** | `g04_` 11, `g05_` 8, `g06_`–`g12_` 23, `g13_` 43 (one test per (instruction, code)); `RELEASE_CHECK=1` passed (no Pending row, no `NotImplemented`); G7 also on the keeper (`keeper --test play` 7/7 on the test-beacon `.so`) |
| **E4** G14 | 100 bots × 2 game days through the program, native kernel every bell, verifier PASS, one tampered log FAIL | **fail** | `cargo test -p itest -- --include-ignored g14_` exit 101, twice (03:07 and 03:13): every §13.3 property PASS (native kernel over 11,303 province-bells: 441 CLASH matched, 1,317 SKIP byte-identical; verifier PASS over 17,622 txs; tampered logs FAIL `ClashReplayMismatch`, `ChainGap`), but `resident-liveness` FAIL: Depart `NotResident` 116 refused against 154 sent (43 %; limit 25 %) — all 116 the settle racer's race tries (re-run summary `/f/relay Depart NotResident 116`, bots group `persona:settle_racer` `NotResident 116`). §1 |
| **E5** | The 7-day accelerated local season (Mode A) | **pass** | `runs/m1-exit/` (`run.md`, `criteria.md`, `report.md`, `verify.md`, `tamper.md`, `load-verdict.json`, `run-log.txt`): criteria 1–6, 8, 9 pass, 7 n.a., row E exit-grade; §3 below |
| **E6** | Verifier PASS < 10 min; 23 required tamper classes FAIL (30 judged); every `mutate-<check>` lets its tamper PASS; honest-but-adverse fixtures PASS | **pass** | exit season: verify PASS in 11 s (read 4.6 + verify 6.4) over 144,300 txs, tamper base PASS and 30/30 classes FAIL with their codes, all built from the run. Here: `crates/verify/mutate.sh` exit 0, 12 builds × 58/58 ("every disabled check lets its tampers PASS; every other class still FAILS"); `verify -- --include-ignored tamper_` 58 passed on the recorded fixtures (incl. the honest-but-adverse ones); W5 smoke tamper 30/30 (27 on the run, 3 on fixtures) and each nightly 30/30 |
| **E7** npm | `permutation-gateway` `npm test`: web-frontier tests, v9 web tests, `web-lang`, `web-sdk` freshness, `web-frontier-wasm` hash | **pass** | 529/529 (Gate W1 line 11 and W2 line 19), `sync-web-sdk.mjs --check` fresh; `build-wasm.sh --check` fresh |
| **E7** screens | 12 screens × JA/EN × 360/390/1440 px: no console errors or failed requests, no overflow, landmarks, bell chip, ≥ 44 px targets, axe no serious/critical, language toggle without reload | **pass** | `screens: npm ci && node --test *.screen.mjs` 51/51 (36 matrix tests = 72 shots, 14 interaction/logic tests, 1 fixture check) |
| **E7** onboarding | Scripted onboarding (Playwright + dev wallet, 390 px, JA and EN): join → site → build → scout/explore → sealed march on a camp → report → verify in browser | **pass** | `run-onboarding.sh --base-port 41700 --run-id m1x-onboarding --spectator`: `ok 1` JA, `ok 2` EN (`runs/m1x-onboarding/onboarding.md`). **Deviation:** on a fresh 20× test-key stack, not "the exit stack": the exit stack's chain is paused for the owner and must stay so, and a paused chain cannot take a join |
| **E7** spectator | Spectator open 24 game hours: memory growth ≤ 200 MB | **pass** | same run: 74 samples over bells 23 → 168 (24.2 game hours), retained JS heap 3.09 → 3.49 MB (growth 0.40 MB), DOM nodes 175 → 262, listeners 46 flat |
| **E8** | Measurements owed to M0 | **pass** (reported) | §5 |

### 2.1 Cumulative gate lines (§12 W1–W6, without a new 7-day season)

| Line | Result |
|---|---|
| Gate W1: `cargo fmt --check`; clippy rules + ABI `-D warnings`; `permutation-rules` release tests (Phase A equivalence, Phase B digests); `frontier-abi` tests; `abi-vectors --check`; `permutation-chain` tests; `frontier-sim` fmt, clippy, tests; `criterion --best-response --seeds 3 --first-seed 30001 --gate`; `doctrine-gate --controls`; `frontier-node` fmt, clippy, debug tests; gateway `npm ci && npm test`; `civilization` tests; `git diff --quiet d95fa25 -- session.mjs permutation-chain/src` | all 13 exit 0 |
| Gate W2: clippy program; program host tests; `build-frontier.sh --twice` (`d85e1bd7…2281` both times); svm `g01_loaded_ g01_budget_ g02_–g05_`; `frontier-node` release tests; `npm test && sync-web-sdk --check`; `build-wasm.sh --check` | all 7 exit 0 |
| Gate W3: svm `g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_`; the `PSF_TRACE=1` G1 line; `frontier-node` release tests | all 3 exit 0 |
| Gate W4: full svm suite (248 passed, 4 ignored); `--include-ignored inproc_ lag_gate crash_injection`; keeper play on the test-beacon `.so` (7/7); verifier `tamper_` (58) | **the second line exit 101** (`inproc_day` `resident-liveness`: Depart `NotResident` 31 refused against 64 sent (33 %), all `persona:settle_racer`; every other `inproc_day` condition PASS; cargo stops at the first failing target, so the line was re-run with `--no-fail-fast`: `inproc_smoke_one_bell_with_the_test_key`, `lag_gate_in_process` and `crash_injection_every_journal_point` ok, only `inproc_day` fails); the other three exit 0 |
| Gate W5: `RELEASE_CHECK=1` svm; release build + `mutate.sh`; G14; screens; the stack smoke (`check-ports`, `up --scale 100 --days 1 --bots 100` at 41600, `verify && tamper`, `load --viewers 5000 --game-hours 1`, `down`) | **G14 exit 101** (above); the rest exit 0: smoke verify PASS (7,205 txs), tamper base PASS and 30/30 (27 on the run, 3 on fixtures), load pass (file p99 6.4 ms, ingest → WS p99 0.06 s, error rate 0, WS coverage 100 %) (`runs/m1x-w5-smoke/`) |
| Gate W6: `m1-nightly.sh` ×3 (41500) | **three consecutive green nights (3, 4, 5)** after a red night 2: five nights, each verify PASS and tamper 30/30; night 2 failed only its post-play 36-s `load` step (file p99 524 ms > 250 ms, 0 errors; not criterion-6 evidence), which is noisy on the shared machine (7–524 ms over the five nights) (`runs/m1x-nightlies.md`) |
| Gate W6: the scale-2 latency line (`up --scale 2 --game-hours 6 --bots 300 --chaos` at 41200, `verify`, `report && down`) | **pass**: criterion 3 at the game-second targets, round → anchor p99 1 s, S → first cache 1 s, anchor → last reveal 0 s, S → resolve 3 s (targets 5 / 5 / 30 / 60 s); verify PASS (3,337 txs); criteria 1, 2, 4, 8, 9 pass; `report && down` exit 0 (`runs/m1x-latency/`) |
| Gate W6: scripted onboarding green in JA and EN | pass (E7 above) |
| Gate W6: the c4 v3 report cites the measured Reveal CU distribution and `L(reveal)` | pass; the exit season's 1,694 Reveals folded in (§5) |
| `PENDING-OWNER` | none |

## 3. The exit season (E5) in brief

`runs/m1-exit/run.md` has the environment table and the full record; `criteria.md` the figures.

- **Environment (exit-grade):** release `.so` pinned and matched; real quicknet rounds from the approved archive; 20×, 7 game days (1,008 play bells + 26 drain), 1,000 bots with the 13 personas; keepers A and B each with 150 reveal payers (effective N 150 in every bell), 32 delay payers, 4 funders; 43 `kill -9` chaos kills with 43 restarts and no crash; all 9 adversary hold kinds fired (14 windows; `slots-below` re-armed 5 times before its sixth window opened the claim `defence-pool` held); 5,000 viewers for 24 game hours. Wall time 8 h 39 min of `up`, then verify 41 s, tamper 44 s, report 2 s.
- **Criteria:** 1 pass (EndSeason; 1,712 due transits, 0 unsettled; 0 stuck province-bells); 2 pass (35 kinds, none over budget; Reveal p50 19,389 / p99 22,951 / max 24,050); 3 pass (slots p99: round → anchor 1, S → first cache 1, anchor → last reveal 1, S → resolve 3; idle province-days 122, SkipQuiet p99 6, none over 6); 4 pass (0 valid seals unrevealed; 9 unrevealed by rule, all "bounced"); 5 pass (no persona violated; 5 observed, 8 exercised through the chain); 6 pass (file p99 8.7 ms, ingest → WS p99 0.75 s, 0 errors of 3.46 M, WS coverage 100 %); 7 n.a.; 8 pass (15 garbage, 9 bad-plaintext transits settled bad-seal, codes 2 and 5); 9 pass.
- **O-M1-28 and O-M1-29 as decided (T4, T5):** criterion 6 judged over the whole viewer window, criterion 3 with no hold window excluded; both pass.

## 4. Findings of this step

### 4.1 The resident-liveness condition counts the settle racer's designed refusals (G14, `inproc_day`)

`itest::gate` (`resident-liveness`, integ-W4 review) divides the relay's `NotResident` refusals of Muster, Explore and Depart by the ones it sent and fails above 25 %. The settle racer (integ-W6t review, S29) now tries its re-depart from the bell after its arrival bell every 8 game seconds for 4 bells; the relay refuses the early tries in simulation, by design. Evidence: `inproc_day` 31 of 31 refused Departs are `persona:settle_racer` (its JSON output), G14 116 of 116 (re-run with `G14_SUMMARY`, 03:13–03:17: `persona:settle_racer {"HostInTransit": 8, "NotResident": 116, "ok": 10}`); Muster and Explore 0 refused. Integ-W6t's gate table re-ran the frontier-node release tests (which skip these ignored tests), the nightlies and the W5 smoke, but not Gate W4's `inproc_` line or G14, so this went unseen. **Fix (W7-B, test-only):** exclude the racer's race tries from the ratio (the relay counts per group, or the bots' report subtracts `persona:settle_racer`), then re-run `inproc_day` and G14.

### 4.2 ClaimDefence never lands in a stack run: the beneficiary cannot pay its fee

ClaimDefence is signed by the beneficiary (§5.12: `[keeper s,w (= beneficiary)]`) and the keeper sends it with the beneficiary as fee payer. The stack creates the beneficiary keys (`keeper-{a,b}/beneficiary.key`) but never funds them: both hold **0 lamports** at the end of the exit season (read-only `getBalance` on the paused chain, slot 77,751). `frontier-localnet` drops a transaction whose fee payer cannot pay (`InsufficientFundsForFee`/`AccountNotFound`) without a trace (`localnet/src/chain.rs`). So in `m1-exit` keeper A's claim for its late Reveal at bell 71 (the one the sixth `slots-below` window delayed ≥ 4 slots; landed at 2,050,151 µlamports per CU) went out **64 times from slot 5,608 to 6,588 and expired every time** (keeper A journal, read-only: `attempts` kind `claim`, all `expired`; alert `write-expired … 64 versions expired unlanded`), long after the `defence-pool` hold ended at slot 5,901. No run record so far lists a ClaimDefence row. Consequences: no §13.4 criterion is affected (the holds fired; the pool was held; nothing requires a claim to land), but the hold's purpose — a claim that survives a held pool within its grace — was never observed in play, and a playtest keeper configured the same way would never be refunded. **Fix (before a playtest):** fund the beneficiary with fee lamports in the stack (`crates/stack`) and say so in `RUN-A-KEEPER.md` (or let another payer pay the fee while the beneficiary only signs), then check a ClaimDefence landing in a nightly.

### 4.3 Smaller records

- **Criterion 7** (bots vs `frontier-sim`, reported, not gating) is not computed by the stack report; it reads n.a. in every run.
- **The no-landing detector's "unexplained" window** (slots 815–1029, bells 8–11) is the known stall of every 20× run: the `ticket` hold and the persona holds fill the block at bells 8–10 (W6T-3 §8, O-M1-29). The detector does not yet attribute it to the holds.
- **Failed transactions** 784 of 144,300: 9 unclassified (CloseArrivalSlot `BadAccount` 8, Reveal `TransitState` 1), reported.
- **The nightly's post-play load step is noisy** on the shared machine: file p99 7, 524, 30, 180 and 13 ms over the five nights (limit 250 ms; 0 errors in all five). Night 2 failed on it; criterion 6 itself is judged in-run (8.7 ms in the exit season). If the nightlies are to gate a playtest's CI, this 36-s measurement needs a longer window or a quieter machine.
- **The `w6-s7` run record** (`runs/w6-s7/`, the failing first season) was written by the run but never committed; it is committed with this report as evidence.

## 5. E8: the measurements owed to M0

| Measurement | Result |
|---|---|
| Reveal CU, worst (G1, release `.so`) | 25,155 whole-transaction units (named slot 24,300; tx 916–928 B; heap 2,810 B max); `L(reveal)` 1,146,880 B (35 pages); requested limit 26,500 |
| Reveal CU in play | exit season: n 1,694, program CU p50 18,940 / p99 22,501 / max 23,600 (whole transaction p50 19,389 / p99 22,951 / max 24,050); 1,676 first-of-bell (3 locks), 18 later (2 locks). Pooled with the 416 earlier in-play Reveals: n 2,110, p50 18,928 / p99 22,473 / max 23,600 (`c4-v3/reveal-cu.txt`, `reveal-rows.json`, DESIGN §22.2) |
| c4 model v3 → tip | re-run with the 2,110 rows: **`tip_min` 14,668 lamports** (presets 14,668 / 22,002 / 29,336) and the keeper's priority at `tip_min` 0.433 / 0.428 **unchanged**; every C4 verdict unchanged (fails at the minimum tip; passes at the pool cap 2.0 with ≥ 150 rotating payers, 11.7–47.9× at 600 s) [model]; only the in-play line, the sensitivity line and the measured R99 moved (`c4-v3/c4-model-v3-final.txt`) |
| Quiet-bell proof cost | exit season: SkipQuiet per idle province-day p50 6, p99 6, max 6 (122 days); per churned day p50 9, p99 28, max 32 (847); per resident day max 9; SkipQuiet CU in play p50 32,697 / p99 57,029 / max 67,141, so an idle province-day costs ≈ 6 × 33k ≈ 0.2 M CU [computed]; G1 worst SkipQuiet 170,412 for 24 bells |
| R99 and payer pools | exit season R99 **6** reveals per bell (max 8; 765 bells with a reveal); both keepers' reveal-pool effective N 150 in every bell, floor 215,076,060 lamports; `F_r` 0.0080 SOL at any R99 ≤ 150 (DESIGN §22.4) |
| ResolveFromInputs after Phase A and B | G1 worst 271,673 CU (gate 290,000; limit 285,500) over the full-gather fills incl. all 1,200 native-screen fills and the storage fill; heap max 15,320 B (≤ 28,672); in play max 50,984 |
| Arrival-slot tie-break at Reveal (`slot_key`) | rank `(dep_mass, slot_key)`, known at Reveal; G9 (8 reveal orders → the same final slots) green; the exit season's V6 (`QuotaSetMismatch`) clean and T8 detected; the quota part of Reveal is within the G1 figure above |

## 6. What M1 delivers

A playable, money-free first season of the Sixfold Frontier on a local chain, with every piece a public-network playtest needs except hosting:

- **Program** `permutation-frontier` (SBPF v2, 50 instructions incl. the test-only oracle, 17 account kinds, all with-seed addresses of the Season PDA): season lifecycle, rings and provinces with camps, joins (invite gate for the playtest), holdings with tickets and cohorts, harvest/build/train, muster/garrison/explore, sealed marches with a minimum tip, Reveal with quotas and the one-way latch, the quiet-bell proof (ArrivalDay + SkipQuiet), gathers and resolves, SettleTransit as the seal proof, beacons from drand quicknet, archives and every close path, the defence pool and ClaimDefence. Every instruction under its measured budget with adversarial fill; pre-funding, forgery and re-creation covered; reproducible build.
- **Off-chain** (`frontier-node`): keeper (two profiles, ≥ 150 rotating reveal payers, escalation classes, nudges), herald (fold, files, WebSocket), verifier v2 (V1–V13, 30 tamper classes, checks of the checks), 1,000 bots with 13 personas, the LiteSVM-backed local chain with drand replay, the stack orchestrator with chaos and adversary holds; relay and JS SDK in `permutation-gateway`.
- **Web client** (`permutation-server/web/frontier/`): map, holding, march composer and tracker, bell sheet, clash report with in-browser verification, onboarding, practice, spectator; JA/EN; screens and accessibility checked.
- **Evidence:** a 7-day, 1,000-bot season on the release program with real drand rounds, chaos and attacks, verified end to end.
- **Documents:** DESIGN §21–§23 with the M1 decisions, the final C4 v3 model and D18 table, `RUN-A-KEEPER.md`, `PLAYTEST-RUNBOOK.md` (devnet configuration only).

## 7. Known limits

- **Local only.** Everything ran on `frontier-localnet` (LiteSVM with 400-ms slots at 20×). Nothing ran on devnet; devnet's BLS syscalls and rent are unverified (runbook G6, G7).
- **C4 is a model.** The fee-market attack cost is modelled; it fails at the minimum tip and passes only with the defence pool at 2.0 and ≥ 150 rotating payers. The M4 soak decides it.
- **The defence refund is unproven in play** (§4.2).
- **Criterion 7 is not measured**; bot behaviour is compared with the simulator only in `frontier-sim` itself.
- **Latency at 20× with stacked holds:** the fixed windows (ticket holds at bells 8–10, above-cap holds) push single anchors to 154 slots; over 7 days they stay under the p99 (O-M1-29 as decided).
- **Criterion 5's persona coverage** depends on scale: at 100 bots (nightlies) some personas are not exercised; at 1,000 bots all were.
- **No money** (M2), no postures, sieges, governance or Relic Sites (M2/M3).

## 8. Open before a playtest

1. **The two test lines of §4.1** (test-only fix, re-run `inproc_day` and G14; moves the exit commit).
2. **ClaimDefence funding (§4.2)** and a nightly that shows a ClaimDefence landing.
3. **The W6-D web follow-up** (contract §15, S19): clamp the arrival bell to `end_bell − 1` and offer no Depart when the earliest arrival is ≥ `end_bell` (`fmarch.mjs`, `screens/march.mjs`); grey out other factions' holdings as march targets while the player's own holding is shielded and show the shield's end, and show `409 Shielded` from `/f/reveal` with the same text (`screens/holding.mjs`, `fland.mjs`). The program, keeper, relay and bots already refuse or avoid both cases.
4. **The `/frontier` redirect (W6-D F7):** the herald serves the game at `/frontier/frontier/index.html`; `/` and `/frontier` still redirect to the v9 page. A player link must land on the game.
5. **The runbook's devnet gaps G1–G8** (`PLAYTEST-RUNBOOK.md` §2): a TLS client in `frontier-node`, a public-RPC feed for the keeper, a live drand source, an operator tool for AnnounceSeason/CreateSeason, an address listing, the devnet BLS and rent checks (each read needs approval), and the small-season payer floors (`r99_reveals = 150`, `delay_floor`).
6. **Hosting and operations:** a host for the herald (the only public origin, behind TLS) and the relay, who runs keeper B, invites (the relay's gate key and `Season.join_gate`), sponsorship quotas (40 → 20 transactions per citizen per game day) and the test-SOL budget (≈ 100–135 SOL with the preset as it stands, ≈ 37–43 SOL with the smaller option; runbook §5).
7. **CI:** the `.github/workflows` job set was not changed in this step. It runs the rules, ABI, program (SBF build twice, the LiteSVM suite, `RELEASE_CHECK=1` on tags and `release/` branches), simulator, gateway and `frontier-node` jobs; it has no web-screens job (W5-E R6), no `mutate.sh` and none of the ignored in-process tests (`inproc_day`, G14), which is how §4.1 could go unseen. Adding them is W7-C's remaining CI item; any push needs a new owner approval.

## 9. Decisions for the owner

1. **O-M1-18: run the private devnet playtest (50–200 people, no money)?** It needs items 1–6 of §8 first, then per-step approvals (deploy, funding, AnnounceSeason, starting services), a host and domain, invites, the sponsorship values and a test-SOL budget. Recommendation: approve the preparation work (§8 items 1–5, code only, no devnet step) now; decide the playtest itself, its dates and budget when those are merged.
2. **D18 final values:** keep the 20-SOL defence pool, `per_bell_region_cap` 0.2 SOL, `per_keeper_day_cap` 2 SOL (DESIGN §22.3; 20 SOL covers ≈ 1,950 attacked p99 bells at 50k wallets [model]). For a 200-person playtest the smaller option is 2 SOL (runbook §5).
3. **`r99_reveals` (P3):** set 150 for seasons up to ≈ 30,000 wallets and 300 up to 50,000 (the code default 4,000 locks 32–65 SOL per keeper in payer floors); the exit season measured R99 6 at 1,000 bots.
4. Carried: **D22** stake ramp (keep 2.0 recommended) before M2; **D24** Relic Sites' role before M3; any **push** (a new approval).

## 10. What M2 starts with

M2 "Money and lifecycle" (DESIGN §12) starts from this program and stack: the citizen fee and laurel stake with escrow, vault shards, the escrowed operator share, FactionShards and reward indices, the banking window, FinalizeFaction and the closed-form Claim, Abort/Refund/WithdrawJoin, the faction sub-pools of the defence pool, rent close paths, the Shade roster, relay sponsorship with money; and a fresh review of every ported v9 module (token checks, solvency, Abort and refunds, roster reveal). It inherits: the layouts' reserved room, the verifier and tamper framework (money checks to add), the stack and its exit-grade environment, the open items of §8 that are not playtest-only (§4.1, §4.2), and the owner's D22 answer.

## Links

- Contract [`M1-CONTRACT.md`](M1-CONTRACT.md) v1.13 (§12, §13, §15); decisions [`../DECISIONS.md`](../DECISIONS.md) part U (this exit) and T (the owner's answers); Japanese summary [`M1-EXIT.ja.md`](M1-EXIT.ja.md).
- Exit season record [`runs/m1-exit/`](runs/m1-exit/) (`run.md`, `criteria.md`, `report.md`, `verify.md`, `tamper.md`); this step's runs [`runs/m1x-w5-smoke/`](runs/m1x-w5-smoke/), [`runs/m1x-onboarding/`](runs/m1x-onboarding/), [`runs/m1x-nightlies.md`](runs/m1x-nightlies.md), [`runs/m1x-latency/`](runs/m1x-latency/); the first season [`runs/w6-s7/`](runs/w6-s7/).
- C4 v3 with the exit sample [`c4-v3/`](c4-v3/); DESIGN §22 [`../DESIGN.md`](../DESIGN.md).
- Operators: [`RUN-A-KEEPER.md`](RUN-A-KEEPER.md), [`PLAYTEST-RUNBOOK.md`](PLAYTEST-RUNBOOK.md).
- Previous pass: [`integ-W6t-review-NOTES.md`](integ-W6t-review-NOTES.md).
- Run directories (not committed): `frontier-node/.local/frontier/{m1-exit,m1x-*}`; the exit log `.claude/data/m1-exit-run.log`.
