# W6-C node-fix: notes

- **Unit:** W6-C node-fix (wave 6, pass 1: prep and known fixes). **Branch:** `frontier/m1-W6-C`, cut from `frontier/m1-integ` at `0514b06` (contract v1.9). **Date:** 2026-09-28.
- **Brief (contract §11 wave 6, and the wave note):** the node items deferred to W6 in DECISIONS O11 (the herald-recorded fixture that drives an unrested march; `frontier-fund` and `defence-pool` holds tied to a ring opening and a claim grace; W5-C's process-level smoke of the relay and the JS SDK) and the W5 findings (SkipQuiet `OutOfOrder` refusals and SkipQuiet transactions per province-day, W5-B F4; `frontier-bots --day0-share / --eager-personas`, W5-B F2/R3; keeper payer-care cadence in slots, W5-B F1), plus N11's `ValidSealUnrevealed` restricted to seals settled `ROUTED` (W5-C F4).
- **Owner decisions in force:** O-M1-01…24 working defaults; O-M1-12 items 1–3 approved (nothing of them needed here: no wasm, no Playwright, the archive not read); Agave ≥ 4.0 not approved (nothing here needs it); O-M1-17 local; O-M1-18 not approved (nothing devnet). The rustfmt/clippy install is already recorded in `docs/frontier/DECISIONS.md` part A (row of 2026-09-27) and H3; DECISIONS is W6-E's file in this wave, nothing added.
- **Not done, by rule:** no push; no devnet or mainnet transaction; no install or download (the gateway's `node_modules` in this worktree is an APFS clone of the integ worktree's, `cp -Rc`, not an `npm ci`); no 7-day run (the main session runs w6-s7). Services only on 41800–41899 and 41900–41999 (seven stack runs, at most two at a time, each stopped with `down`; 41000, 41300, 41500 and 41700 were in use by other units and not touched); tests bind `127.0.0.1:0`. `permutation-server/web/session.mjs`, the main tree, `codex/magicblock-playable` and `codex/v9-security` untouched.
- **Logs:** `(session scratch)/scratchpad/w6c/` — `day1.log` (+ `day1-summary.json`, the recording run), `base-day.log` (+ `base-day-summary.json`, the same day at the wave base), `ws1.log`, `ws2.log` (workspace tests), `stack-{n1,n2,n3,b1,b2,b3,b4}.log` with `w6c-*-{verify,tamper,report}.log`, `f-*.log` (the final gate lines of §3), `jb2.sqlite` (keeper A's journal of `w6c-b2`). The baseline worktree `(session scratch)/scratchpad/w6c/base` (detached at `0514b06`) was removed after use.

## 1. What landed

### 1.1 SkipQuiet: fewer losing versions and fewer transactions (W5-B F4)

**Cause, measured.** The W5-B nightly's 23 `SkipQuiet: OutOfOrder` came from **one** write: keeper A's journal shows `skip:0,3:51:15` sending 24 versions (slots 1147–1170, one per slot) while the `lag` hold held Province (0, 3); when the hold ended all 24 executed in slot 1171, one landed and 23 failed `OutOfOrder` (the keeper already counts that code as "done" for SkipQuiet, so nothing re-planned; the losers still paid fees). The smoke's 3 were three skips whose first version (sent at slot 1241 by a tick that ran past its slot) landed at 1243, after a second version had gone out at 1242. The engine sent a new version of **every** write every slot (§8.2 "resend every slot").

**Fix 1 — resend cadence of D and N writes** (`keeper::engine`, amendment request A1 in §5). W writes (Reveals) keep a new version every slot. A D or N write whose earlier version is still in flight (sent, status unknown, blockhash valid) gets its next version only `d_resend_slots` (2) slots after the last while its bid still rises, and only every `cap_resend_slots` (16) slots once the bid is at the class cap (D: P_delay 0.5; N: the fixed bid) — a version at the same bid buys only a fresh payer. A version whose status is known (a failure the retry ladder answers) is followed at once, and contested detection now runs every slot whether or not a version went out. The bids still follow `EngineParams::bid(slots since the first version)`, so a D write reaches P_delay at the same slot as before. Both periods are `keeper.toml` keys (`d_resend_slots`, `cap_resend_slots`). A 24-slot hold now leaves at most 3 losing versions (versions at +0, +2, +4, +20), not 23. The cadence counts from the chain's slot **when the version went out** (`Version::chain_slot`, a Clock read in the same `send`), not the slot the tick read at its start: the first stack run with eager personas showed ticks running two slots late (a version "sent at 1213" landed at 1216 beside the one sent at 1215: 8 of its 12 `OutOfOrder`).

**Fix 2 — skip when the work waits, not at every bell** (`keeper::play::skip_target`). Before, a province with *any* pending work (a resident change, a `Leave`, a departure to settle, an arrival ahead, or the last 24 bells of the season) was "urgent" and each bell was skipped alone as soon as it closed. Now each province has a **target bell** — the earliest bell its `resolved_next` must pass: a nudge → the next bell (at once); a pending change or `Leave` → its bell; a departure to settle → its departure bell; an arrival ahead → the bell before it (so the arrival bell is next when its window closes: close → resolve is unchanged); an arrival bell left clear, or a settlement judged against this province (a never-revealed march) → that bell. A skip goes out when its run of closed quiet bells covers the target, as a whole 24-bell batch, or at the season's end. Bells already passed are no target (their settlement can go now).

### 1.2 Payer care on game time and at once when low (W5-B F1)

`keeper::care_due`: care runs on the first tick, then every `care_every_slots` (150) slots **or** `care_every_game_secs` (new, 300) game seconds, whichever comes first — at 100× that is every 8 slots instead of every 150 (10 bells, when the W5-B run's reveal pool sat at effective N = 0 for 21 bells) — and, when a pool is below its minimum effective N, 2 slots after a care that planned top-ups (§8.2 "tops up at once"; a care that found no funder able to pay waits for the cadence). Care never runs while care transfers are still pending (that would top the same payer up twice). Care transfers are N writes, so the cadence of §1.1 also stops a slow tick from sending a transfer twice.

### 1.3 `frontier-bots --day0-share F --eager-personas` (W5-B F2 / R3), and why the bots stopped early

`--day0-share F` (0–1) sets the mix's day-0 join share (default 0.6; `inproc_day` uses 1.0), kept when the roster is re-dealt for `join_close_bell`; `--eager-personas` gives persona bots a session every bell (as `inproc_day` does). The start line prints the share, the flag and how many bots join on day 0. The stack passes them with `bots_args` / `--bots-args` (W5-B wired it; empty by default).

The first stack run with the flags (`w6c-b1`) still joined only 75 of 100: **every bot action stopped at bell 102 of 144** (the last JOIN, TICKET, HARVEST and DEPART records all at bells 97–104; the relay log ends there) while the fleet process ran on until the supervisor stopped it after play. The fleet's game clock was the cause; it took four changes, each found by a run:

1. **`fclient::clock::GameClock::observe` drops a read equal to the last one** (same slot and time). The bots sampled the herald's `latestSlot/latestUnix`, which stays put between folds; each repeat measured a scale of 0 and cleared the window, and the next fold's jump over a fraction of a second then set a scale tens of times too high; the extrapolated clock ran past `until` and every bot task returned. (`drand-replay` reads the same type and only ever uses the last observed value; a changed time at an unchanged slot is still recorded.) Test `repeated_slots_do_not_inflate_the_scale`.
2. **A bot ends on observed time, not an extrapolation** (`fleet::run_bot` re-reads `/h/season` and returns only when the observed time ≥ `until`). With 1 and 2 (`w6c-b2`) actions reached bell 124.
3. **A bot naps at most 5 s wall (`fleet::MAX_NAP`) and re-plans** (a pre-join bot slept once until its join bell on the estimate of that moment). **Alone this made things worse** (`w6c-n2`, `w6c-b3`: actions stopped at bells 50 and 20): the clock only got samples when some bot ran and fetched `/h/season`, so once the estimate went slow nobody came due and nothing corrected it.
4. **With `--rpc` (the stack always passes it) the fleet's clock follows the chain's Clock sysvar, read every 200 ms** (as `drand-replay` does); the herald's samples still count if ever newer (never in practice: the clock is monotone in slot). With 1–4: **`w6c-n3` (default flags) JOIN 100/100** (66 on day 0, and the 34 whose join bell is 144 joined at bell 144, as dealt) and **`w6c-b4` (`--day0-share 1.0 --eager-personas`) JOIN 100/100 by bell 143**, activity through bell 144 (DEPART 63, REVEAL 59, CLASH 59; was 45/39/38 when the fleet stopped at 102).

**Request to W6-A / the integrator (R1):** set `bots_args = "--day0-share 1.0 --eager-personas"` in the one-day configs (`w5-smoke.toml`, `nightly.toml`) — but see finding F1 (`late_revealer`) first, since the eager personas make the report's criterion 5 fail today — and `--eager-personas` in the 7-day configs if criterion 5 needs every persona to march.

`late_revealer` now judges "the window has closed" on the time the herald observed on chain (`min(now, season.latest_unix)`), never on the runner's extrapolated clock (test in `owners_reveal_in_the_arrival_bell_by_persona`). **This did not fix the persona's `violated` verdict** in the eager runs (F1, §4).

### 1.4 The `frontier-fund` and `defence-pool` holds aim at their situation (DECISIONS O11, contract v1.9 §25 open item)

- `fclient::land::ring_opening(season, frontier, funds, provinces on chain, now)`: `InProgress {ring, wedge, missing}` while an opened ring still has provinces to open (the wedge of the first missing one, whose ProvinceFund pays it), else `Due {ring, wedge}` when OpenRing may land by §5.9 on the folded values (`ring_open_check`; the most crowded wedge).
- `fclient::play::open_claim(slot, anchor slot, A, season, now)` → the claim grace's end when a defence claim is open (unclaimed, Reveal late by `lateness_slots`, a refund by `fees::defence_refund`, before `reveal_close + 6 bells`). **The keeper's claims duty now uses it too**, so the hold and the claimer judge a claim by one rule.
- Stack (`adversary.rs`, `up.rs`; W6-A's paths, see deviation D1): both holds are **armed from the first hour to the end of play** like the ticket hold; while one waits, the supervisor's once-a-bell probe reads the Frontier, the six funds and (for the pool) every ArrivalSlot with its anchor (`adversary::probe_land_and_claims`). `frontier-fund` holds Frontier + the ProvinceFund of the ring opening's wedge (1 bell); `defence-pool` holds the DefencePool for 3 bells only when a claim is open whose grace outlasts the hold by a bell (so claims wait and none is lost). The event's `detail` names the ring and wedge, or the open claims and the earliest grace end.

### 1.5 The herald-recorded fixture drives an unrested march (DECISIONS O11)

- `frontier_agents::recorded` (new): `load(dir)` a recording and `plans(recording, L(reveal))` — the marches its joined wallets plan, with the recorded stamina and (for a wallet that plans none) with the roster rested. One rule for the test and the recorder.
- `itest::day`'s recorder (`FRONTIER_RECORD_FIXTURES=1`) now takes the recording **before the bots act** at the first herald step of each bell from `ITEST_RECORD_BELL` (96) and keeps the first one that drives an unrested march (else the try with the most marches). Taken after the bots act — as before — a host rested at that bell has already marched, which is why the integ-W5 re-recordings at bells 60, 120 and 140 drove none.
- **Re-recorded** from the strict day at the wave base's test-beacon program (`094dc2d6…`): **recorded at bell 107, 1 march planned with the recorded stamina, 1 more with rested hosts** (`day1.log`). The test `the_recorded_herald_parses_and_drives_the_policies` now asserts ≥ 1 unrested march (it fails on the old bell-96 recording: 0 unrested, 2 rested) and still runs every plan through the kernel's checks and the stamina Depart charges. **If W6-B's Phase B changes the chain, re-record the same way** (`FRONTIER_RECORD_FIXTURES=1 PSF_FRONTIER_SO=<test-beacon .so> cargo test --locked --release -p itest --test inproc_day -- --include-ignored`).

### 1.6 `ValidSealUnrevealed` only for seals settled `ROUTED` (N11, W5-C F4)

V5 warns `ValidSealUnrevealed` (and lists it in `liveness.valid_unrevealed`, which E5 criterion 4 reads) only when a valid seal never revealed settled `ROUTED`; one that settled with another outcome (bounced-unranked: outranked, quota-refused, the citizen's second arrival — no loss by rule) goes to the new `liveness.unrevealed_by_rule` `(host, arrive bell, outcome)` and the report's liveness line. The committed `march-synth` fixture: 1 warning (the rout), 1 by rule (the refused arrival, `BOUNCED_UNRANKED`), was 2 warnings. This is the verifier-side twin of `itest::gate`'s `no-valid-seal-routed`.

### 1.7 The relay as a process with the JS SDK (DECISIONS O11)

`permutation-gateway/test/frontier-process.test.mjs`: `node src/frontier/server.mjs` started as a child (operator listener on `127.0.0.1:0`, pool of 4 with `--dev`) against a scripted JSON-RPC chain on `127.0.0.1:0` holding a Season encoded by the JS SDK's codec; `GET /f/season` answers the SDK's Season address, tip minimum and presets; `/f/tx/<sig>` → `unknown`; the operator route without the token → 403; a bad quota query → 400; SIGTERM → exit 0. (The public listener cannot take port 0 — `publicPort: 0` means "none" in `server.mjs` — so the test uses the operator listener, which serves every route.)
## 2. Measurements [measured, 2026-09-28, this machine]

Programs: the wave base's test-beacon `.so` (874,624 B, sha256 `094dc2d6…39c5`, copied from the integ worktree's `deploy-test-beacon`; no program code changed in this unit). Stack runs: `configs/nightly.toml` (100 bots, one game day at 100×, adversary on, test key) on base ports 41800 / 41900, two at a time, then `verify`, `tamper`, `report`, `down`.

### 2.1 In-process day, before and after (same program, same seed; `itest::inproc_day`, strict)

| | wave base `0514b06` (`base-day.log`) | W6-C (`day1.log`, final `f-w4a.log`) |
|---|---|---|
| all 12 conditions | PASS | PASS |
| SKIP records | 1,078 | **811** (−25 %) |
| `SkipQuiet: OutOfOrder` (failed txs) | 63 | **6** |
| keeper alerts | contested 1, **reveal-pool-low 4** | contested 1 |
| JOIN / DEPART / REVEAL / CLASH / TRANSIT_SETTLED | 100 / 73 / 66 / 94 / 73 | 100 / 73 / 66 / 93 / 73 |

The 6 left are not isolated (the one contested write of the run is a skip at bell 0, `skip:-2,-1:0:13`).

### 2.2 Stack runs (`frontier-stack`, nightly config)

| run | code | bots flags | JOIN (last bell) | DEPART / REVEAL / CLASH | SKIP | SkipQuiet `OutOfOrder` | SKIP per province-day p50 / p90 / p99 / max (days > 6) | reveal effective N min | criterion 5 | verify / tamper |
|---|---|---|---|---|---|---|---|---|---|---|
| W5-B `nightly-20260928` (reference, W5-B worktree) | wave 5 | — | 63 | 5 / 5 / 5 | 325 | **23** | 6 / 6 / 40 / 40 (5) | 150 | — | PASS / 26 of 26 |
| `w6c-n1` | 1.1–1.2 | — | 65 (143) | 9 / 9 / 9 | 270 | **0** | 6 / 6 / 30 / 30 (3) | 150 | pass | PASS / 29 of 29 |
| `w6c-b1` | 1.1–1.2 | day0 1.0, eager | 75 (**102**) | 45 / 39 / 38 | 666 | 4 | 6 / 55 / 94 / 94 (19) | 150 | late_revealer | PASS / 29 of 29 |
| `w6c-b2` | + clock fixes 1–2 | day0 1.0, eager | 87 (124) | 48 / 40 / 39 | 618 | 12 | 6 / 28 / 89 / 89 (19) | 150 | late_revealer | PASS / 29 of 29 |
| `w6c-n2` | + nap (fix 3) | — | 32 (**50**) | 3 / 3 / 3 | 242 | 0 | 6 / 6 / 18 / 18 (2) | 150 | pass | PASS / 29 of 29 |
| `w6c-b3` | + nap (fix 3) | day0 1.0, eager | 23 (**20**) | 15 / 12 / 11 | 298 | 4 | 6 / 16 / 19 / 19 (9) | 150 | pass | PASS / 29 of 29 |
| **`w6c-n3`** | final | — | **100 (144)** | 8 / 8 / 8 | 283 | **0** | 6 / 6 / 38 / 38 (3) | 150 | pass | PASS / 29 of 29 |
| **`w6c-b4`** | final | day0 1.0, eager | **100 (143)** | 63 / 59 / 59 | 801 | 4 | 7 / 41 / 124 / 124 (24) | 150 | **late_revealer** | PASS / 29 of 29 (all on the run) |

Reading it:
- **OutOfOrder:** 23 → 0 on the nightly shape. In the eager runs the 4–12 left are two versions of one skip landing in one block after a late tick (8 of `w6c-b2`'s 12, before `chain_slot`) and the contested first skip of the play (`skip:-2,3:0:7`, 3 losers); `w6c-b4` (final) has 4. Note the `lag` hold found no march in flight in any of these runs (`hold-skipped`), so the case that produced W5-B's 23 is covered by the engine test (`d_and_n_writes_resend_on_their_cadence`), not by a run.
- **SkipQuiet per province-day:** idle province-days sit at 6 (p50, p90 on the nightly shape). The p99/max are churned province-days (a player acting every bell nudges its province every bell, and a nudge is a skip at once); `frontier-stack report` cannot yet tell idle from churned (W6-A's item), so criterion 3's "≤ 6 per idle province-day" is not decided by these numbers.
- **Payer care:** the reveal pool's effective N stayed 150 in every sampled bell of every run (`bells_below_150` empty for both keepers); the stack airdrops the pools at start (W5-B), so the starvation of W5-B's `dev1` did not recur either way; the in-process day's 4 `reveal-pool-low` alerts are gone (§2.1).
- **Holds:** `frontier-fund` and `defence-pool` were `hold-skipped` in every run: no ring opened and no defence claim was open after the first hour of a one-game-day run (their situations need the 7-day run, or a scripted ring opening / late reveal; see §4 F3).
- **Wall time** of `verify` on these runs 0.5 s, `tamper` ≈ 3 s.

### 2.3 G14 (two game days in process) on the final tree

`cargo test --locked --release -p itest -- --include-ignored g14_`: **PASS**, all 15 conditions. SKIP 1,399 over 10,949 quiet bells (W5-C's final G14: 1,980 SKIP over 10,976), every one kernel-quiet at every bell and reproduced byte for byte; 224 CLASH matched; verifier PASS in 1.1 s with warnings `PrefundedAddress` 1 only (W5-C's run: `ValidSealUnrevealed` 5 as well — §1.6); both tampers FAIL with their codes.

## 3. Gate items that concern these files (run on this branch, final tree)

| # | Command | Result |
|---|---|---|
| 1 | `(cd frontier-node && cargo fmt --all -- --check)` | exit 0 |
| 2 | `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | exit 0 |
| 3 | `(cd frontier-node && cargo test --locked --release --workspace --no-fail-fast)` (`PSF_FRONTIER_SO` = the test-beacon `.so`) | exit 0: 341 passed, 0 failed, 10 ignored |
| 4 | `(cd frontier-node && cargo test --locked --workspace)` (debug, Gate W1 line) | exit 0: 341 passed, 0 failed, 10 ignored |
| 5 | `cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection` (Gate W4) | exit 0: `inproc_day` (12/12, §2.1), `inproc_smoke_one_bell_with_the_test_key`, `lag_gate_in_process`, `crash_injection_every_journal_point` |
| 6 | `PSF_FRONTIER_SO=… cargo test --locked --release -p keeper --test play -- --include-ignored` (Gate W4) | exit 0: 7 passed |
| 7 | `cargo test --locked --release -p verify -- --include-ignored tamper_` (Gate W4) | exit 0: 56 passed |
| 8 | `cargo test --locked --release -p itest -- --include-ignored g14_` (Gate W5, G14) | exit 0 (§2.3) |
| 9 | `MUTATE_CHECKS="v5" crates/verify/mutate.sh` (Gate W5 line, the check this unit changed) | exit 0: T2, T7, T22 PASS under `mutate-v5`, every other class FAILS; default binary relinked |
| 10 | `(cd permutation-gateway && npm test)` | 516 pass, 0 fail (was 515: + `frontier-process.test.mjs`) |
| 11 | `node permutation-gateway/scripts/sync-web-sdk.mjs --check` | up to date (no SDK source changed) |
| 12 | `frontier-stack check-ports / up / verify / tamper / report / down` on the nightly config, base 41800 and 41900 (§2.2; not the Gate W6 lines, which are W6-A's and the integrator's) | every `up` exit 0; every `verify` PASS; every `tamper` 29 of 29 detected; `report` exit 0 on the default-flag runs, **exit 1 on the eager-persona runs** (criterion 5, `late_revealer`, F1); every `down` exit 0, nothing of this unit listens afterwards |

Not run by this unit: the full `mutate.sh` (only `v5` changed here), the Gate W6 lines (`scripts/m1-nightly.sh`, `w6-latency`, `w6-s7`: W6-A and the integrator; the main session runs w6-s7), the program and root-workspace lines, the screens package. **PENDING-OWNER in this unit: none.**

## 4. Findings for the triage pass and other units

- **F1 (bots / report, open): `late_revealer` reads `violated` whenever the eager personas march it** (`w6c-b1`, `b2`, `b4`: one "late" reveal of about six counted `ok`; the default-flag runs never march it and read pass). No REVEAL landed after its arrival bell in any of these runs (every REVEAL record's slot time is inside its arrival bell), so the `ok` is an answer the bot counted as success — a keeper `202` on `/f/reveal` or a clean simulation of its direct Reveal (`Bot::direct_tx` records the simulation, not the landing) — not a reveal the program took late. Judging the window on the herald-observed time (§1.3) did not change it. Not isolated in this pass; it makes `frontier-stack report` exit 1 on criterion 5 as soon as `--eager-personas` is on. Suggested: record a direct transaction's landing (`getSignatureStatuses`) rather than its simulation, and log the `/f/reveal` answer with its time for this persona.
- **F2 (report, W6-A):** SkipQuiet per province-day p99/max are churned provinces (nudged every bell by active players); idle days sit at 6. The report's idle-vs-churned split is W6-A's.
- **F3 (stack, W6-A / main session):** in one-game-day runs the `lag`, `slots-below`, `frontier-fund` and `defence-pool` holds are usually `hold-skipped` (no march in flight / no ring opening / no open claim in their window). The 7-day run has ring openings and late reveals; if a nightly must exercise them, it needs the eager personas (marches) and a scripted crowding (ring) or a `slots-below` hold that actually meets arrivals.
- **F4 (keeper):** 4–6 `OutOfOrder` remain per run from contested first skips and two versions landing in one block; they are paid duplicates, bounded by the cadence, and the keeper already treats `OutOfOrder` on SkipQuiet as done.
- **F5 (web / W6-D):** nothing changed in the SDK or the herald's formats.
- **For W6-E ("Run a keeper"):** new `keeper.toml` keys `care_every_game_secs` (300), `d_resend_slots` (2), `cap_resend_slots` (16).
- **If W6-B's Phase B changes the chain**, re-record `crates/agents/fixtures/herald-recorded` (§1.5).

## 5. Amendment requests and deviations

**Amendment requests (for the integrator's window):**
- **A1 §8.2 Bid policy:** "Resend every slot" → W writes every slot; D and N writes, while a version is in flight, the next version `d_resend_slots` (2) slots after the last (counted from the chain's slot when it went out) while the bid rises and every `cap_resend_slots` (16) slots at the class cap; a version whose failure is known is followed at once; contested detection every slot. (§1.1)
- **A2 §8.2 Quiet skip:** the trigger "`resolved_next < b − 1` and the bits are clear" gains the batching rule: a skip goes out when its run of closed quiet bells covers the province's target bell (nudge, pending change or `Leave`, departure to settle, the bell before an arrival, a clear arrival bell or a settlement judged there), as a 24-bell batch, or at the season's end. (§1.1)
- **A3 §8.2 Payer care and `keeper.toml`:** care every `care_every_slots` or `care_every_game_secs` (300) whichever comes first, at once (2 slots after a care that planned top-ups) when a pool is below its minimum, never over pending care transfers; the new keys `care_every_game_secs`, `d_resend_slots`, `cap_resend_slots`. (§1.2)
- **A4 §8.5 V5:** `ValidSealUnrevealed` only for a valid seal settled `ROUTED` unrevealed; other unrevealed valid seals in `liveness.unrevealed_by_rule` `(host, arrive, outcome)`. E5 criterion 4 reads the warning as before. (§1.6)
- **A5 §13.4 adversary / v1.9 §25 open item:** closed — `frontier-fund` during a ring opening (`fclient::land::ring_opening`), `defence-pool` inside an open claim grace (`fclient::play::open_claim`, the keeper's own rule), both armed over the whole play window. (§1.4)
- **A6 §8.6 bots CLI:** `frontier-bots --day0-share F --eager-personas`; with `--rpc` the fleet's game clock follows the chain's Clock sysvar. (§1.3)

**Deviations:**
- **D1 (ownership):** `frontier-node/crates/stack/src/{adversary.rs, up.rs}` are W6-A's paths in wave 6; DECISIONS O11 names W6-C for the two holds, so the change is here: the predicates live in `fclient` (this unit's), the stack gets the `Pending` fields, `probe_land_and_claims` and ~25 lines in the supervisor's hold probe. W6-A merges after W6-C (merge order W6-B, W6-C, W6-D, W6-A); if W6-A touched the same hunk of `up.rs` the integrator resolves it (the change is additive).
- **D2:** `crates/keeper/tests/held_accounts.rs::anchor_held_past_the_version_cap_lands_after_the_hold` now asserts the anchor lands within one resend cadence of the hold's end and that no version cap was reached (with the cadence a 240-slot hold no longer runs a D write into its 64-version cap); the cap-and-expiry path stays covered by `engine::tests::capped_write_whose_versions_expired_ends_and_restarts`.
- **D3:** `frontier_agents` (a "no IO" crate) gains `recorded::load`, which reads a fixture directory (like `fixture::write` already writes one).
- **D4:** the baseline for §2.1 ran in a detached worktree at `0514b06` in the session scratch (removed afterwards), with its own target directory; the W5-B nightly figures in §2.2 are read from the W5-B worktree's run directory (not re-run).

## 6. Dependency requests (integrator, I-55)

None. No manifest, lockfile, toolchain file or `.gitignore` changed (the new code uses crates each package already depends on).

## Links

- Keeper: `frontier-node/crates/keeper/src/engine.rs` (cadence, `chain_slot`), `src/play.rs` (`skip_target`, `skip_now`), `src/lib.rs` (`care_due`), `src/config.rs`, `tests/held_accounts.rs`
- Clock and bots: `frontier-node/crates/fclient/src/clock.rs`, `crates/bots/src/{main.rs, fleet.rs, bot.rs}`, `crates/agents/src/policy.rs` (late_revealer)
- Holds: `frontier-node/crates/fclient/src/land.rs` (`ring_opening`), `crates/fclient/src/play.rs` (`open_claim`), `crates/stack/src/adversary.rs`, `crates/stack/src/up.rs`
- Fixture: `frontier-node/crates/agents/src/recorded.rs`, `crates/agents/tests/herald_fixtures.rs`, `crates/itest/src/day.rs`, `crates/agents/fixtures/herald-recorded/`
- Verifier: `frontier-node/crates/verify/src/checks/v5_seals.rs`, `src/lib.rs`, `src/report.rs`, `tests/fixtures.rs`
- Relay process test: `permutation-gateway/test/frontier-process.test.mjs`
- Contract: `docs/frontier/m1/M1-CONTRACT.md` v1.9 §8.2, §8.5, §8.6, §11 (W6-C), §13.4; DECISIONS O11, N11
