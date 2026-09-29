# W6T-3: W6-C node-fix (verifier, agents, bots, herald load generator, localnet, relay)

Fix unit U3 of the w6-s7 triage (`PLAN.md` §1 U3). Branch `frontier/m1-w6t-U3`, cut from `frontier/m1-integ` 7dcacdf, local commits only. Worktree `.claude/worktrees/m1-w6t-U3`.

Input: the w6-s7 run (7 days, 1,000 bots, archive rounds, 20x, chaos, adversary, 5,000 viewers; `.so` 072b1205…a98b). Its verify input is `frontier-node/.local/frontier/w6-s7/verify/input.json.gz` in the m1-integ worktree. The live w6-s7 stack was only read. I made no queries that change state and sent no transactions.

Every timing below has the load average next to it. The machine was shared, and the 1-minute load average ran from 3.3 to 25.3 during this work.

## 0. Result against the unit's "done when"

| Condition | Result |
|---|---|
| `cargo test --release -p verify -p agents -p bots -p herald -p localnet` (with `fclient`, `findex`, `drand-replay`) | **green**: 274 passed, 0 failed, 42 test binaries (load 4.8 → 7.9). `cargo test --release --workspace --no-run` builds, and `-p stack` passes 38 |
| gateway `npm test` | **green**: 529 of 529. `test/frontier-*.test.mjs` passes 44 of 44 |
| `frontier-verify` on the w6-s7 input | no `CampMismatch`; `ValidSealUnrevealed` **0**; `unrevealed_by_rule` **36** (27 `shielded-own` + 9 `bounced`); `MissingData` 9. Verdict FAIL, because of the new `ArrivalAfterEnd` ×9 on the same 9 old marches (expected on this old input, see §3) |
| `frontier-verify tamper` on the same input | **30 of 30 classes detected**. The base run is FAIL with `ArrivalAfterEnd` (the old input), not with `CampMismatch` |
| R4 | **0 errors**, `staleRetries` 8,000, `ws.reconnects` 2,000; control 0 errors |

## 1. V11 camp clear (failure 2)

- **Cause:** a verifier bug. The program logs CAMP (spawn), CAMP (clear), CLASH in one ResolveFromInputs. V11 judged the clear against the camp as it stood before the transaction (state 0), not the camp the same transaction had just spawned.
- **Fix:** `crates/verify/src/checks/v11_land.rs`, from `triage/camp-mismatch/v11-fix.diff`.
  - The camp present at the clash counts as state 1 when an earlier CAMP spawn of the same (P, Q) in the transaction has `troops > 0`.
  - Added hardening: that spawn's `day` must also equal the bell of the transaction's CLASH ÷ 144.
- **Fixture:** `fixtures/verify/w6s7-camp-spawn-clear.json.gz` (sha256 `bb10dcf0…4757e`).
- **Tests:** `crates/verify/tests/camp_clear.rs`, three tests.
- **Before (7dcacdf):** `v11_a_camp_spawned_and_cleared_in_one_resolve_is_no_mismatch` failed with exactly the two run findings:
  - `camp (0, -3) @578: … Some(0) → Some(0)`
  - `camp (4, 2) @584: …`
- **After:** 3 of 3 pass. The check of the check (`…_no_spawn_still_fails`) still reports `CampMismatch`.

## 2. V5: rule refusals and attempts per march (failure 4)

The triage patch (`verifier-v5-shield.patch`) is landed and extended in `crates/verify/src/checks/v5_seals.rs`. Only a valid seal that was never revealed and settled `ROUTED` is judged. `rule_refusal` follows §5.11 Reveal step 6 in the program's order:

1. **Path.** The plaintext's walk from the transit's origin tile is rebuilt with kernel geometry (`locate`). Each step's province masks (`passable`/`rough`/`road`) come from the archived post-states. The check also covers ≤ 32 steps, ≤ 4 provinces, and ending on the destination.
2. **ArrivalBell.** The walk's seconds, with the road and cavalry halvings, then the doctrine's `travel_secs`, are checked with `travel::check_arrival_bell`.
3. **Shield.** The destination must be another faction's holding site. It is `shielded-dest` when its `shield_until_bell > arrive`. It is `shielded-own` when the host's own Holding has `shield_until > bell_start(arrive)` and is not dormant.

Other rules for this check:
- **Which state is read:** the one before the first transaction at or after the arrival bell's end, and never after the settlement.
- **Missing state:** if a state is not in the archive, the result is `None`, so the march stays a liveness warning (fail safe).
- **Refusal codes:** they are never read.

Report changes:
- `liveness.unrevealed_by_rule[]` is now `{host_id, arrive_bell, outcome, reason, failed_attempts}`.
  - `reason` is one of `shielded-own`, `shielded-dest`, `path`, `arrival-bell`, `bounced`.
  - `bounced` means any unrevealed valid seal settled with an outcome other than ROUTED.
- A new `liveness.unrevealed_by_reason{reason: n}` gives the counts, and `report.md` prints them.
- Failed Reveal attempts are counted per `(host, arrive)`, from the plaintext each attempt carried.

Tests:
- **Fixture** `fixtures/verify/w6s7-shielded-march.json.gz` (sha256 `bd960bb3…aa7d`, 17 transactions). It holds host 1909855992414208's marches 394 and 552 and all six of that host's failed Reveals. It was cut by the new `crates/verify/examples/march_slice.rs`.
- **Failing first:** `failed_attempts_counted_per_march`. It reads the report JSON, so it compiles on both versions.
  - 7dcacdf: `Some(6) != Some(2)`. Both marches listed 6 host-wide signatures.
  - Now: 2 and 3.
- `shielded_refused_march_is_unrevealed_by_rule`: `(394, ROUTED, shielded-own, 2)` and `(552, ROUTED, shielded-own, 3)`, with no warning.
- `unshielded_rout_stays_a_liveness_warning`: the check of the check. With the Holding's `SHIELD_UNTIL` zeroed in the slice, both marches are `ValidSealUnrevealed` again. This also shows that the path and arrival-bell judgement passes these real marches.

w6-s7 input (load 15.4):

| | ValidSealUnrevealed | unrevealed_by_rule | attempts per march |
|---|---|---|---|
| 7dcacdf | 27 | 9 | 2–8 (host-wide) |
| W6T-3 | **0** | **36** (27 shielded-own, 9 bounced) | 20 × 2, 7 × 3 (matches the triage table) |

## 3. V5 invariant `ArrivalAfterEnd` and tamper T24 (failure 1)

- **Invariant:** a landed DEPART with `arrive_bell ≥ end_bell` (the CreateSeason parameter) is a FAIL with the new code `ArrivalAfterEnd`, listed in `FAIL_CODES`.
- **Tamper class T24, "DEPART arriving at end_bell":** required, built on the program recording with a fallback to any run.
  - It is a consistent forgery. CreateSeason's `end_bell` becomes the latest landed arrival. The ANNOUNCE and SEASON_CREATED `params_hash` and the Season account's `END_BELL` and `PARAMS_HASH` in every post-state and final follow, and the chains are re-chained.
  - So only V5 sees it: with `--features mutate-v5` both T24 tests PASS (checked).
- **Suite size:** 23 required + 7 extra = 30 classes. `tests/tamper.rs` asserts `required == 23` and 30 outcomes.
- **Test:** `arrival_after_end_is_fail` on the synthetic season.
  - `end_bell = max arrival`: one finding per DEPART at that bell.
  - `end_bell = max arrival + 1`: none.
- **On the w6-s7 input:** 9 `ArrivalAfterEnd`, arrivals 1008–1011 against `end_bell` 1008. These are the same 9 marches as the 9 `MissingData`. The input predates U1's program fix, so FAIL is the right verdict for it. A run on U1's `.so` cannot contain such a DEPART.
- **Tamper on the w6-s7 input** (`--jobs 4`, load 10.3 → 8.8): 30 classes, 30 detected. The base run is FAIL (ArrivalAfterEnd), so the suite exits 2 on this input.

## 4. Agents (failures 1 and 4, the 111 ProvinceFull)

Changes in `crates/agents/src/policy.rs`:
- **`plan_march`:** `arrive = min(earliest + 1 + extra, dep_bell + 72, end_bell − 1)`, and there is no plan when `earliest ≥ end_bell`.
- **`first_plan` and `shield_refuses`:** each candidate is planned first, then dropped if §5.11 step 6's shield clauses refuse it at the planned arrival. The two clauses are the destination site's `shield_until_bell > arrive`, and the own Holding's `shield_until > bell_start(arrive)` when it is not dormant. They apply only when the destination is another faction's holding site.
- **`targets(obs, faction, arrive_max)`:**
  - no longer filters by `bell + 4`;
  - drops only sites still shielded after the latest possible arrival;
  - `military` passes `bell + 73`.
- **`muster_room`:** the program's rule. Fewer than 48 entries in states 1–2, fewer than 8 of the faction, and a free entry.
- **`fclient::abi::layout::holding::FLAG_DORMANT_CACHE`** is added, with a twin test against frontier-abi.

Tests in `crates/agents/tests/herald_fixtures.rs`:
- **`plan_near_end_never_arrives_at_or_after_end_bell`**, failing first. On 7dcacdf `policy.rs` it gives `extra 0: arrive 44` with `end_bell` 44 (`u3logs/4-agents-before.log`).
  - `dep_bell = end − 4` on the longest path that can still arrive: `arrive ≤ end − 1` for `extra` 0–2. Unclamped, it would be `dep + 6`.
  - `end − 2`: no Depart.
  - Over the whole policy: no planned march arrives at or after the end.
- **`shielded_holding_offers_no_war_target`:**
  - the control plans the war target;
  - a shielded holding gets none, or falls back to a camp;
  - a dormant holding and a shield lapsing before `bell_start(arrive)` do not refuse;
  - camps are never refused;
  - 48 policy bells plan no war march.
- **`dest_shield_judged_at_arrival`:** a site shield ending exactly at the arrival keeps the target, `arrive + 1` refuses it, and `bell + 74` drops it from `targets`.
- **`muster_skips_full_province`:** the control musters. 48 roster entries of another faction, or no free entry, mean no Muster.

## 5. Bots (failure 4, R4 observability)

`crates/bots`:
- **Reveal outcomes** (`bot.rs`):
  - A 2xx from `/f/reveal`, or a landed direct Reveal, sets `accepted` and journals `accepted` with its route.
  - A refusal journals `reveal_refused` with its code, and the memo keeps `last_code`.
- **Observed reveals** (`observe_reveals`, which runs in `reconcile` every step): `revealed` is set and journalled only when the herald shows the march's ArrivalSlot `(bell = arrive, host_id)`, in the latest envelope or the arrival bell's own envelope, or the host among the ClashInputs arrivals.
- **Unrevealed marches:** a march that settles with no REVEAL observed goes to the report. `report.json` gains:
  - `unrevealed[]`: `{bot, persona, seal, host, depart_bell, arrive, dest, route (keeper|direct|none), accepted, tries, last_code}`;
  - `unrevealed_honest`, the count of honest-seal entries.
- **`Answer::code`** falls back to an `error` field that is a single identifier. That is the keeper's `409 {"error":"Shielded"}`, which the relay passes through.
- **`agents::MarchMemo`** gains `accepted` and `last_code`. `reveal_done()` keeps the owner's retry rule exactly as before (`revealed || accepted`).

Tests:
- `revealed_only_on_observed_reveal`:
  - 202 gives `accepted`, not `revealed`, and the journal has no `revealed`;
  - the bell-38 envelope's slot gives `revealed`;
  - the relay's 409 Shielded gives `last_code` Shielded, journalled;
  - a march settled by someone else with no REVEAL is in `unrevealed` with route `keeper` and `last_code` Shielded.
- `owners_reveal_through_the_keeper_link` now asserts `accepted && !revealed`. On 7dcacdf it asserted `revealed` on the 202, which is the bug.
- `journal::tests` folds `accepted` and `reveal_refused`.

## 6. Viewer generator (failure 5)

- **Triage patch:** `triage/herald-errors/viewers-fix.patch` is landed:
  - a stale keep-alive retry (`staleRetries`);
  - a budgeted connect (`--retry-budget-ms`, default 5,000; `unavailable`, `unavailable_ms`);
  - WS reconnect with 0–500 ms of jitter, re-subscribe and a sequence reset (`ws.reconnects`);
  - `errorSeconds`.
- **Changes on top of the patch:**
  - `errorSeconds` is an object `{"<second>": n}`, as §2 of the plan pins it.
  - `--follow-status URL`: a follower task reads `/h/season` (`genesisTs`, `bellSecs`), then polls `/h/status` `live[1]` once a second. The per-bell requests go to the live bell and the two before it (`FOLLOW_BACK` = 3) instead of `0..bells`. The report gains `follow.liveBell` and `follow.errors`. The follower's own reads are not counted as load.
  - `--rings` accepts `a-b` ranges and mixes such as `0-2,5`. `--provinces P,Q;…` was already there.
  - `KeepAlive::get_body` for non-gzip JSON reads.
- **Failing first:** `crates/herald/tests/viewers.rs viewers_ride_out_two_herald_restarts`.
  - The herald runs in its own runtime. A kill is `shutdown_background`, which drops the listener and every connection as `kill -9` does. It is restarted on the same port after 0.2 s. The run has 40 pollers and 10 WS viewers, think 1 s.
  - 7dcacdf (load 7.4): `errors` 80 = 40 × 2, `ws.errors` 10 (`u3logs/6-viewers-before.log`).
  - Now: `errors` 0, `ws.errors` 0, `ws.reconnects` 20 = kills × ws.
  - The plan's text says `ws.reconnects == ws`. With two restarts, every WS viewer reconnects twice, so the test asserts kills × ws. R4's expected 2,000 for 1,000 WS and 2 kills agrees with this reading.
- `viewers_follow_the_live_bell` checks the follower's `liveBell` against `/h/status` (the runner's `observe_clock`), and that without `--follow-status` there is no live bell.

R4 ran in `scratchpad/u3r4`, a private APFS clone of the triage's herald data, with the 7dcacdf herald binary on **41140** and the stats port on 41175. It used 4,000 pollers and 1,000 WS, 60 s, think 3 s, and kills at 15 s and 40 s with 3.0 s down. The herald was stopped afterwards and 41140 is free.

| run | requests | errors | ws.errors | staleRetries | ws.reconnects | p99 | load (1 min) before → after |
|---|---|---|---|---|---|---|---|
| 7dcacdf generator | 80,039 | **9,335** | 1,000 | – | – | 6.1 ms | 12.4 → 6.7 |
| W6T-3 generator | 74,916 | **0** | 0 | 8,000 | 2,000 | 3,146 ms (11% of the window is outage; 7,455 requests waited, 15,568 s in all) | 5.6 → 6.1 |
| W6T-3, control (no kills) | 80,031 | **0** | 0 | 0 | 0 | 10.2 ms (upper 10.8) | 5.8 → 14.1 |

The 7dcacdf generator gives 9,335 rather than 8,000. With 3.0 s down, longer than the 1.5-s shortest pause, some pollers also hit a refused connect.

`targetsMet` is false in the kill runs for two reasons. The outage raises p99, and a herald with a dead RPC folds nothing, so no WS diff is timed. Both are expected in this repro.

## 7. Relay

- **Keeper answers:** the relay already passed the keeper's `/v1/reveal` answers through (`routes/keeper.mjs`). The new test `W6T-3: the keeper's 409 answers … pass through unchanged` pins that `409 {"error":"Shielded"}` and `409 {"error":"ArrivalBell"}` reach the client byte-identical.
- **`shapes.mjs checkDepartArrival`:** a sponsored Depart with `arrive_bell ≥` the Season's `END_BELL` is `400 ArrivalBell {endBell}`. It is refused inside `allowanceFor`, before the quota check and the simulation, so nothing is charged or simulated. A Season with `END_BELL` 0 is not judged.
- **Failing first:** on 7dcacdf `shapes.mjs`, `end 147: {"ok":true,…}` (`u3logs/7-relay-before.log`). Now the refusal comes back with `simulated == 0`, `sent == 0` and the quota untouched.
- **Running the tests:** the worktree has no `node_modules`. For the runs I symlinked m1-integ's, so nothing was installed, and I removed the symlink afterwards.

## 8. Localnet: the no-landing window (slots 827–1028)

Time-boxed. **Verdict: not a localnet fault.** The window is the contention emulator working as specified. Three concurrent single-key holds filled the 100M block.

Evidence, from the w6-s7 ledger read with the new `crates/localnet/examples/wal_slots.rs` (it streams `ledger.wal`: per block the slot, game time, landed/failed, §10.1 cost and priority range; `WAL_TXS=1` lists each transaction):
- A block was produced for every slot 700–1100, with no gap. Slots 827–1028 executed **0** transactions.
- From slot 1029, every block executed **19.99M** of cost (1029–1044: 19,988,320 … 19,994,984), 56 keeper anchors at about 357k each. That is exactly `100M − 2 × 40M`: two holds' filler was still charged to the block.
- Keeper A's journal (a copy, read-only):
  - The anchors sent at slot 880 landed at slot **1029**. That is their last blockhash-valid slot (`bh 879 + 150`), so they were deferred for 149 slots, not dropped.
  - Keeper A's journal shows no landing between slot 850 and 1028.
- `chain.rs` `fill_hold` charges each hold's filler, which fills its key's 40M, to the shared block budget as well. This matches the documented model: the module doc, `the_filler_takes_block_room_and_multi_key_holds_fill_every_key`, contract §8.8 and DESIGN §8.6 ("holding one account costs p × 40M per block").
- The holds active in the window:
  - the adversary's `ticket` hold (1 key at 1.0 milli-lamports per CU, bells 2–25 = slots about 350–2074; in `events.jsonl`);
  - holds from the `ticket_holder` persona. Its bots hold their Province with `frontier_hold` at priority 500 for 3 bells once their ticket makes them provisional. That is `policy.rs`: it is the only other caller of `frontier_hold`, and SettleTicket landed at slots 813–816 just before the window.
  - Three holds take 40M + 40M + 20M, the whole block, before any transaction at or below 500 is placed. Keepers' D bids are capped at 0.5, and ties lose to the hold. Sponsored player transactions bid about 0.1. So nothing landed anywhere until one hold ended.
- The bots' per-persona hold outcomes were lost with the bot restarts (`bots/report.json` shows `ticket_holder` `{}`). I cannot cite the timing of the individual bot holds. The 20M-exact drain and the single stack hold make the count certain (≥ 3 during the window, 2 after it).

Test (it pins the mechanism and is not failing first, because the behaviour is the specified one): `crates/localnet/tests/contention.rs three_concurrent_single_key_holds_fill_the_block`.
- Holds at 1,000, 500 and 500 on three unrelated keys, and 80 unrelated anchor-sized transactions.
- 0 land while the three holds run, with `cu == BLOCK_CU`.
- When the third hold ends, `floor(20M / cost)` land.

**For U4/U5, an owner decision, not a U3 change:** the §13.4 environment can stack holds into a chain-wide stall. Three independent streams cost `Σp × 40M` per block and stop every writer at or below their price. Options:
- keep it (it is a priced attack; the report should label such windows: U4's "no landing ≥ 20 slots" detector);
- cap the concurrent holds in the stack's schedule and the persona;
- price the persona's hold below the D cap (for example 0.4), so keepers outbid it.

The `ticket_holder` persona's own expectation in §8.6, "finality waits for the cohort", does not need a chain-wide stall.

## 9. Interfaces for the other units (PLAN §2)

| Producer → consumer | As built |
|---|---|
| verify → U4 report | `report.json.liveness.unrevealed_by_rule[]` = `{"host_id": "<u64 string>", "arrive_bell", "outcome", "reason", "failed_attempts"}`. The field names follow the existing `valid_unrevealed` and `unrevealed_by_rule` keys, not the plan's shorthand `{host, arrive}`. `liveness.unrevealed_by_reason{reason: n}`. New fail code `ArrivalAfterEnd`. Tamper class `T24` (required; 30 classes in all) |
| bots → U4 report | `bots/report.json.unrevealed[]` = `{bot, persona, seal, host, depart_bell, arrive, dest, route, accepted, tries, last_code}` and `unrevealed_honest` |
| viewers → U4 load | `--retry-budget-ms` (default 5,000), `--think-ms`, `--follow-status URL` (or `host:port`), `--provinces P,Q;…`, `--rings a-b` or a list. Report: `staleRetries`, `unavailable`, `unavailable_ms`, `ws.reconnects`, `errorSeconds{"sec": n}`, `follow{liveBell, errors}` |
| U2 → relay | Keeper 409 bodies pass through unchanged (tested) |
| relay → clients | `400 ArrivalBell {endBell}` for a sponsored Depart at or after `end_bell` |

## 10. Deviations and notes

- **Worktree and branch names:** the computed SETUP line had the unit title pasted into the worktree path and the branch name. I used `.claude/worktrees/m1-w6t-U3` and `frontier/m1-w6t-U3`. The notes file is `W6T-3-NOTES.md`, the path the ownership list names.
- **`march_synth_passes`:** the synthetic generator (`verify/src/fixture/march.rs`) writes a one-step placeholder path into every plaintext, because its model program never checked step 6. Its low-tip rout is therefore now `unrevealed_by_rule` with reason `path`, not a warning, and the test says so.
- **`march_program_passes`:** the plan says this test "keeps its recorded rout flagged", but the program recording has no rout (its `valid_unrevealed` is empty on 7dcacdf too). The flagged-rout control is `unshielded_rout_stays_a_liveness_warning`, on real w6-s7 marches.
- **WS reconnects:** they are asserted as kills × ws, not ws (§6).
- **Localnet:** no code change. The mechanism is specified behaviour (§8).
- **Examples added (no manifest change):** `verify/examples/march_slice.rs` (the fixture cutter) and `localnet/examples/wal_slots.rs` (the ledger reader).
- **Dependencies:** no manifest or lockfile was touched, and there are no dependency requests.
- **Not run:** `mutate.sh` for all checks (only `mutate-v5` over `tests/tamper.rs`: 58 of 58 pass, both T24 tests PASS with the check disabled); `itest` in-process days; `clippy` and `rustfmt` (not installed in the pinned 1.95.0 toolchain, W1-F).

## 11. Logs and artefacts (scratchpad)

`…/scratchpad/u3logs/`:
- `1-camp-before.log`, `1-camp-after.log`
- `2-attempts-before.log`
- `4-agents-before.log`
- `6-viewers-before.log`, `6-viewers-after.log`, `6-herald-all.log`
- `7-relay-before.log`
- `8-wal-700-1100.tsv`, `8-wal-300-1300.tsv`
- `verify-v5/`, `verify-final/`, `tamper-w6s7/`, `tamper-w6s7.log`, `mutate-v5.log`, `final-tests.log`

`…/scratchpad/u3r4/`: the R4 driver, the logs `before-full`, `after-full` and `control` (each with `.json`, `.out` and `.uptime`), and the binaries.

Build target: `…/scratchpad/u3target` (release).
