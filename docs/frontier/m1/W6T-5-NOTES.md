# W6T-5 (fix unit U5, W6-E docs): contract v1.12 §28, DECISIONS part S, DESIGN, keeper guide

- **Unit:** U5 of the `w6-s7` triage (`scratchpad/frontier/m1/triage/PLAN.md` §1 U5), wave-6 paths of W6-E. **Branch:** `frontier/m1-w6t-U5`, cut from `frontier/m1-integ` at `7dcacdf`. Worktree `.claude/worktrees/m1-w6t-U5`. Local commits only, **no push**. No devnet or mainnet transaction, no install, no download, no service started, no port bound. The live `w6-s7` stack was read once (keeper A `GET /v1/status`, read-only, §3).
- **Owned paths touched:** `docs/frontier/m1/M1-CONTRACT.md`, `docs/frontier/DECISIONS.md`, `docs/frontier/DESIGN.md`, `docs/frontier/m1/RUN-A-KEEPER.md`, new `docs/frontier/m1/checks/w6t_docs_check.py`, this file. Nothing else; no manifest or lockfile; no dependency request.
- **Drafted in parallel with U1–U4** (PLAN §9: "U5 drafts in parallel and finalises after the merges"). The text follows the pinned interfaces of PLAN §2 and each unit's brief; the values only U1 can measure are marked `⟨pending: U1 …⟩` (§4), and `checks/w6t_docs_check.py --final` lists every code fact the text states that the tree does not have yet.

## 1. What landed

| Brief item | Where | Summary |
|---|---|---|
| §5.11 Depart step 4 | contract §5.11, §28 | `arrive_bell ∈ [now_bell + 2, min(now_bell + 72, end_bell − 1)]`, `ArrivalBell`; why (no anchor, gather, resolve or settle exists at or after `end_bell`); last useful Depart `end_bell − 3` |
| The CAMP record | contract §6 row 43, §5.11 ResolveFromInputs, §8.5 V11, §28 | troops 0 = the clear of the camp present at the clash, including one this transaction's day check spawned; log order spawn, clear, CLASH; V11's reading of it |
| §5.5 / §10.2 close limits | contract §5.5 rows 0x65 / 0x66, §10.2, §28 | CloseArrivalDay and CloseArrivalSlot limit 8,000 (= budget), measured on all four season states × pre-funded / never-created; the worst measured value is U1's (pending) |
| §8.2 keeper | contract §8.2 duties (Anchor, Reveal, SettleDeparture, Settle transit, Closes, Quiet skip), retry ladder, config, API, §28 (5 rows) | anchors < `end_bell` and the in-slot drand retry; reveals skip `arrive ≥ end_bell` and are deduplicated by `(host, arrive)`; `409 {code: "Shielded"}` / `409 {code: "ArrivalBell"}` (and the existing `409 TransitState`, which the table lacked); the CU-exhaustion rule; Dead closes not re-planned; batched closes; `backup_delay_slots`; skip splits; `/v1/status` within 1 s with the fields under `duties` / `pools` |
| §8.5 | contract §8.5 V5, V11, JSON schema, tamper list, §28 (3 rows) | `unrevealed_by_rule[]` with `reason` ∈ {shielded-own, shielded-dest, path, arrival-bell, bounced}, judged from post-states and plaintext; attempts per `(host, arrive)`; `ArrivalAfterEnd`; **T24**; the V11 camp clear |
| §13.4 A1–A3 | contract §13.4 criteria 3, 4, 6, environment, a "Report additions" paragraph; §13.5; §28 (5 rows) | A1 idle day = no roster move **and** no GATHER/CLASH; A2 rule refusals are `unrevealed_by_rule`, honest refusals fail criterion 5; A3 the viewer error after standard recovery, outage reporting, WS coverage ≥ 99 %, the denominator; every hold must fire; §13.5 23 required tamper classes, 30 judged |
| §12 Gate W6 notes | contract §12 notes (e)–(j) | `--season-end-at-play-end` (+ the config key and the archive re-check), `--chaos-force herald:<h>`, the viewer flags, the hold arming, "a skipped hold makes the run not exit-grade", keeper B `backup_delay_slots = 8`, the `w6-s7b` re-run on the same archive and G0 |
| §3.2 | contract §3.2, §28 | the release build of record paragraph: the new sha (pending, U1), replacing `072b1205…a98b`; `RULESET_HASH` unchanged `72c6b583…54bd9` |
| Also (every U1–U4 behaviour change needs a row) | contract §8.3 (relay pass-through, optional pre-check), §8.6 (bots), §11 row (the fix units' ownership), header version line | not named in the brief's row list but required by "Done when" (PLAN §1 U5) |
| DECISIONS part S | `DECISIONS.md` part S (S1–S22), change-log row, header version | one entry per §28 row with the evidence pointers (triage write-ups, run log, unit notes and test names); S21 records the 37,042 failed transactions by class and the live floor check; S22 lists what stays open |
| DESIGN | status bullet, §2.6 "Season end for marches", §6.2 Depart bound, §6.2 "Not revealed" (the shield-refused march: routed by rule, clients prevent it), new §23 | |
| RUN-A-KEEPER.md | header, §2 roles (beacon, reveal, settle-departure, settle, skip, close), §3 retry ladder and backup keepers, §5 `backup_delay_slots`, §7 `/v1/status` behaviour, the `POST /v1/reveal` answer table (incl. both 409s), `w6-s7` reference numbers, §9 (the fixed reveal-floor gap removed, items renumbered) | |
| §15 | contract §15 O-M1-25, O-M1-26, O-M1-27 and the W6-D web follow-up paragraph | arrival clamp in `fmarch.mjs` / `screens/march.mjs`, own-shield target greying and the shield end in `screens/holding.mjs` / `fland.mjs`, `session.mjs` untouched; not needed for the exit run, needed before any playtest |

## 2. Test: failing first, then passing

A docs unit has no program behaviour to test, so the test is a structural and cross-reference check of the documents: `docs/frontier/m1/checks/w6t_docs_check.py` (standard library only). It checks the version line, that §28 has a filled row for every section the triage changed, that the normative text changed **in place** (step 4's bound, row 43, the RFI log order, the §8.2 API answers, `backup_delay_slots`, the status behaviour, V5/V11/T24/`ArrivalAfterEnd`, the §8.6 clamp, §10.2, §12's flags, §13.4's A1–A3 markers, §13.5's T24, §15's questions and web files), that DECISIONS part S has ≥ 14 entries each with an evidence pointer and one per changed section, the change-log row, the DESIGN lines, the keeper guide's keys and answers, and this notes file. With `--final` it also fails on any `⟨pending` value and on each code fact the text states (U1 Depart bound and budgets.json close limits equal to §28's number, U2 `backup_delay_slots` and the two 409s, U3 `ArrivalAfterEnd` / `"T24"` and the viewer flags, U4 the stack flags, a new `.so` sha in §3.2).

Committed first (`b6cbad3`) and run on the unchanged docs (= `7dcacdf`):

```
$ python3 docs/frontier/m1/checks/w6t_docs_check.py          # before (7dcacdf docs)
FAIL contract: header is not **Version v1.12**
FAIL contract: header does not point at §28
FAIL contract: no '## 28. Amendments v1.12'
…
w6t docs check: 7 pass, 118 fail, 7 pending                   exit 1
```

After the change:

```
$ python3 docs/frontier/m1/checks/w6t_docs_check.py          # after
PENDING code: M1-CONTRACT.md: 6 value(s) marked ⟨pending… left for the integrator
PENDING code: U1: budgets.json cu_limit of tags 0x65/0x66 = 6000/6500, §28 says 8000
PENDING code: U2: keeper.toml key backup_delay_slots (keeper/src/config.rs)
PENDING code: U2: reveal_accept answers 409 Shielded / ArrivalBell
PENDING code: U3: verifier ArrivalAfterEnd and tamper class T24
PENDING code: U4: stack flags --season-end-at-play-end, --chaos-force
PENDING code: U3: viewer flags --retry-budget-ms, --follow-status
PENDING code: U1: §3.2 names the new release .so sha256
w6t docs check: 169 pass, 0 fail, 8 pending                   exit 0
$ python3 docs/frontier/m1/checks/w6t_docs_check.py --final  # exit 1 until U1–U4 are merged and the values filled
```

Check of the check: on a scratch copy of the changed docs with step 4's bound reverted and `backup_delay_slots` renamed in the guide, it fails with exactly `§5.11 Depart step 4: no end_bell bound` and `RUN-A-KEEPER: missing 'backup_delay_slots'` (plus the budgets row, absent from the docs-only copy). `--final` on this branch exits 1 with the 8 pending items.

Regression: the contract is `include_str!`'d by `permutation-frontier/src/layout/text_check.rs` (§5.3 offsets); §5.3 is untouched, and `CARGO_TARGET_DIR=<scratch> cargo test --locked -p permutation-frontier --lib text_check` passes 2/2 on the changed contract (load average 3.46 before, 7.24 after; shared machine, no timing is claimed). The markdown tables added (§28, part S, §15 rows) were checked for a constant column count.

## 3. Evidence read for the text

- The run log `.claude/data/w6-s7-run.log` (report.md: criteria, CU table, latency table, catch-up table, hold lines, failed-transaction map; tamper table: 22 required + 7 extra classes = 29).
- The five triage write-ups and `PLAN.md` in `scratchpad/frontier/m1/triage/`.
- Code at `7dcacdf`, to keep the text true to what exists: `keeper/src/api.rs` (refusal body `{code, detail}`), `keeper/src/reveal_accept.rs` (409 `CommitMismatch` / `TransitState`, 410, 422), `keeper/src/lib.rs::status_json` (latency fields under `duties`, floor under `pools.reveal.floor`), `verify/src/tamper.rs` (T23 and T23b exist as extras, so T24 is the next free id), `frontier-abi/vectors/budgets.json` (tag 101 limit 6,000, 102 limit 6,500, 100 limit 8,000), the web file names under `permutation-server/web/frontier/`.
- **Live, read-only:** keeper A `GET /v1/status` on the paused `w6-s7` stack (41050, bearer token from the run's `keeper-a/keeper.token`) at bell 1034: `pools.reveal` = `{n: 150, effective_n: 150, floor: 215076060}`, `duties.anchor_latency_slots_p99` 5. This closes integ-W6r §5's floor check (DECISIONS S21). Load average 3.49 at the time.

## 4. For the integrator (at the U5 merge, after U1–U4)

Fill the values only U1 measures, then run the check in `--final` mode:

| Marker | Where | From |
|---|---|---|
| `⟨pending: U1 release .so sha256⟩` (×2) | contract §3.2, §28 row §3.2 | `W6T-1-NOTES.md` (`build-frontier.sh --twice`) |
| `⟨pending: U1 .so size⟩`, `⟨pending: U1 test-beacon sha256⟩` | contract §3.2 | same |
| `⟨pending: U1 worst close CU⟩` (×2) | contract §10.2, §28 row §5.5 | U1's `g01_close_arrival_*_ended_paths` table |

Then check against what merged, and amend the text if a unit deviated from its brief:
- **U1:** if the measured worst close path exceeds 7,600, the limit or the program path changes; update §5.5, §10.2, §28 and S3 (the check compares budgets.json with §28's "limit **N**").
- **U2:** the 409 body shape. The PLAN §2 interface says `{"error":"Shielded"}`; the keeper's existing refusal body is `{"code": …, "detail": …}` and the contract, the guide and the relay text say `code`. If U2 ships `error`, one of the two must move (S5 records the reading). Also the CU-exhaustion ladder's cap: the text says "the CU rungs like `ComputeBudgetExceeded`"; PLAN says "up to the budgets row", which is ambiguous; set the exact rung list from U2's code.
- **U3:** the reason strings of `unrevealed_by_rule` (`shielded-own`, `shielded-dest`, `path`, `arrival-bell`, `bounced`) and the field `failed_attempts`; T24's label.
- **U4:** the config key names (`season_end_at_play_end`), the flag spellings, and whether the no-landing detector threshold is 20 slots.
- Tamper count: PLAN says the required classes "go from 29 to 30"; on `7dcacdf` 22 classes are required and 7 are extras (29 judged). With T24 required: **23 required, 30 judged** — the text uses these numbers (S11).

## 5. Deviations

1. **Branch, worktree and notes names.** The SETUP line of the brief substituted the unit's full title into the names (`frontier/m1-w6t-U5 W6-E docs: contract v1.12 §28, …`, not a valid ref). I used `frontier/m1-w6t-U5` and `.claude/worktrees/m1-w6t-U5` (as the existing `m1-w6t-U1`), and `docs/frontier/m1/W6T-5-NOTES.md` as PLAN §1 names unit notes (`W6T-<n>-NOTES.md`).
2. **Placeholders instead of numbers.** U1 had not started when this unit ran (its worktree was at `7dcacdf` with no change), so the new `.so` sha, its size and the worst close CU are marked pending (§4); the 8,000 limit is PLAN §1 U1.2's pinned value.
3. **Rows beyond the brief's list:** §8.3 relay, §8.6 bots, §13.4 environment and report additions, §13.5, §11 ownership — each a U1–U4 behaviour change or count that "Done when" requires to have a row. The existing `409 TransitState` answer (in the keeper since W3-C, missing from §8.2's table) was added with the new ones.
4. **RUN-A-KEEPER §9 item 4** (the reveal-floor pricing gap) removed: it was fixed in integ-W6r (R9) and the live `w6-s7` keeper shows the fixed floor.
5. **A new file under `docs/frontier/m1/checks/`** (the docs check), inside U5's paths. It is not wired into any gate; the integrator may add `python3 docs/frontier/m1/checks/w6t_docs_check.py --final` to the U5 merge checklist.
6. Not done here (PLAN §6 and integ-W6r §5 assign them to W6-E / the main session, not this triage unit): folding the `w6-s7` Reveal sample (n 1,656; p50 19,441 / p99 22,735 / max 24,064 whole-transaction CU) into DESIGN §22.2 with `m1/c4-v3/reveal_cu_from_runs.py`, the c4 50k × 3 re-runs, the spectator poll-rate decision. Recorded as open in DECISIONS S22.
