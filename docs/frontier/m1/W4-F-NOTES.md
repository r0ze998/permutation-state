# W4-F integration — notes

- **Unit:** W4-F (wave 4), branch `frontier/m1-W4-F` cut from `frontier/m1-integ` at `7289822` (contract v1.5).
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.5 — §3.3–§3.5, §5.9–§5.11, §8.1–§8.4, §8.6, §8.7, §11 (W4-F brief), §12 Gate W4, §13.4 criteria 1, 4, 5, 8, 9; I-29, I-44, I-47, I-53, I-54, I-55, I-57. DECISIONS J2 (decided by the main session 2026-09-28), K11 (bot items deferred to W4-F/W5).
- **Owned paths touched:** `frontier-node/crates/{itest,herald,agents,bots}/**` and this file. `findex`, `localnet` and `drand-replay` needed no change. Integrator-owned files changed on this branch so it builds (dependency requests, §6): `frontier-node/Cargo.toml` (one workspace dependency), `frontier-node/Cargo.lock`, the dependency sections of `crates/{itest,herald}/Cargo.toml`.
- **Tags:** [measured] = run on this machine on 2026-09-28; [design] = a rule implemented as the contract states it.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no `playwright install`, drand round fetch, Agave or `rustup` install; no crate downloaded (flate2 and its four crates were already in the local registry cache; everything resolved `--offline`). No service started on a fixed port: every test binds `127.0.0.1:0` (herald, relay stand-in, keeper API); nothing in 41000–41999 was used. `permutation-server/web/session.mjs`, the main tree, `codex/magicblock-playable` and `codex/v9-security` untouched. The rustfmt/clippy install for 1.95.0 is already recorded in `docs/frontier/DECISIONS.md` part A (and H3), so nothing was added there (and DECISIONS is not this unit's file).

## 1. What landed

### `itest` (the in-process system test, I-57)

| module | what |
|---|---|
| `program` | the test-beacon `.so`: `PSF_FRONTIER_SO`, else built once per test process with `scripts/build-frontier.sh --features test-beacon` (incremental, ≈ 18 s cold [measured]) so a gate never runs a stale program after a merge; `ITEST_NO_BUILD=1` uses the file as is. Any source must carry the `PSF_TEST_BEACON_BUILD` marker |
| `world` | `localnet::InProcess` at 20× (virtual time, 400-ms slots of 8 game s, I-54), the program deployed with its `--max-len`, AnnounceSeason → the 24-h lead at scale 2,000 → CreateSeason, InitBeaconLogs, **InitShards × 6** (operator; the first run found Join and FoldOccupancy `BadAccount` without them), the test-key drand gated by the chain's Clock (`round_time + 1 s ≤ Clock`) |
| `relay` | an in-process stand-in of the relay's **public listener** on `127.0.0.1:0` (the bots use their real `HttpRelay`, the herald its real `/f/quota` call): `GET /f/relay` (fee payer drawn uniformly from a 150-key `relay` pool, blockhash, quota), `POST /f/relay` and `/f/join` with the §8.3 shape allowlist (four instructions, pool fee payer, CU price 0, the budgets table's limit and loaded limit, program id, player tags or settle tags, every other signature, Reveal → `UseRevealRoute`, Join only on `/f/join`, Depart tip ∈ the three presets → else `TipNotPreset`), the per-citizen game-day quota (40, `429 QuotaExceeded`), the v1.3 settle requester rule (the citizen is charged only for its wallet's or unexpired session's signature, else the client bucket), co-sign, the **drain guard** (simulate with signatures: a program refusal is `{ok:false, code}` with the program's error name and nothing is sent or charged; the fee payer's Δlamports ≤ fee + the kind's allowance), send; `POST /f/reveal` → the keeper's loopback `/v1/reveal` with its bearer token; `GET /f/quota`. **Not emulated** (the gateway's own suite covers them): per-IP limits (bots are loopback clients, exempt), invites, the replay cache, operator routes, the v1.3 exact account-key/writability check |
| `direct` | the bots' `DirectPort` over the in-process chain (the six personas' own transactions, airdrops, `frontier_hold`) |
| `standin` | **stub mode only** (`ITEST_STUBS=1`): the resolution stand-in for W4-A/W4-C (below) |
| `day` | the orchestrated run: one loop moves time; each slot the keeper (every role of `config::ROLES`, the contract's pool minimums, journal, API on `127.0.0.1:0` with `ChainGate`) ticks and one block is produced; every 5 slots (40 game s) the herald ingests the feed (findex archive → fold → files; `/h/*` served on `127.0.0.1:0`) and the bots that are due observe and act (`Fleet::step_due`); play stops at `play_bells`, then an 8-bell keeper-only drain. Optionally records the herald's answers (`FRONTIER_RECORD_FIXTURES=1`) |
| `checks` | the chain's facts after the run, from the **final accounts and the program's whole feed** (never a component's belief): failed transactions by instruction and error, records by kind, every DEPART's seal opened with the **stock `tlock` opener** at `T(arrive)` (`fclient::seal::judge`), TRANSIT_SETTLED outcome and seal code per host, REVEAL per host, every Province's `resolved_next`, every Holding's transits in state 1–3, and a **stub probe**: every assigned tag simulated with no accounts (a stub answers `NotImplemented` before any check) |

**`tests/inproc_day.rs`** (`#[ignore]`, run by Gate W4's `--include-ignored inproc_`): 100 bots (the simulator's mix, all joining on day 0, the 13 personas once each), one game day (144 bells), then the drain. Named conditions, each `PASS`, `FAIL` or (stub mode only) `PENDING`:

| condition | pass (strict mode, the gate) |
|---|---|
| `no-stubs` | no instruction answered `NotImplemented` (failed txs, relay simulations, keeper alerts) and the stub probe finds none |
| `activity` | ≥ 95% of the bots due to join joined, a site for at least half of them, marches |
| `zero-stuck-province-bells` | every Province `resolved_next ≥ 144` and CLASH/SKIP records exist (resolution is real) |
| `transits-settled-or-routed` | every DEPART due (arrival + 3 bells < now) has its TRANSIT_SETTLED and no transit of a due arrival is still in state 1–3 |
| `bad-seals-destroyed-with-stock-code` | ≥ 1 bad seal due, every one settled, every TRANSIT_SETTLED seal code equal to the stock opener's, no bad seal settled with any outcome but `BAD_SEAL` (the verifier's `BadSealSurvived`) |
| `min-tip-revealed-by-keepers` | every due `min_tip` march has a landed REVEAL (it never self-reveals, §8.6) |
| `personas` | no persona verdict `violated` |
| `keeper-writes` | no `dead`, `retry-ladder` or `write-expired` alert |
| `herald` | no fold alarm (bad records, rewrites, clash mismatch or unchecked, write errors) and every clash file `heraldCheck == "match"` |
| `relay-drain-guard` | no sponsored transaction moved more than its allowance |

`ITEST_BOTS`, `ITEST_BELLS`, `ITEST_SUMMARY=<file>` (the JSON summary: chain facts, bot report, relay counts, keeper alerts and writes, herald alarms, persona marches with their seal and settle state) tune and record a run.

**`tests/relay_standin.rs`** (not ignored, 0.02 s): every relay refusal above over HTTP on `127.0.0.1:0`; a well-shaped transaction reaches the drain guard. **`standin` unit test**: a Spend departs, a Leave frees, a muster-pending entry joins at its bell, idempotent.

### Stub mode (the brief's "first run with stubs for units not yet merged")

On this wave's base the program's GatherClash, ResolveFromInputs, SkipQuiet, the closes, SettleTransit, SweepPoolOwed, ClaimDefence, EndSeason/AbortSeason/CloseSeason, ArchiveAnchors and CloseSeedCache answer `NotImplemented` (W4-A, W4-B), and the keeper has no reveal pipeline, gather/resolve/skip, SettleDeparture or SettleTransit duties (W4-C). Without resolution no Province's `resolved_next` moves and every resident action is `NotResident`, so the first run could not reach a march. `ITEST_STUBS=1` adds `standin`: once bell `b`'s window closed (`now ≥ bell_end(b) + W + seed_margin`), every Province is set (`set_account`) to `resolved_next = b + 1` with the svm harness's `resolve_through` rule (Spend → departed, Leave/Forfeit freed, musters join). It ignores arrivals, logs nothing (the herald sees the bytes only when a later transaction writes the Province) and **is not the program**: strict mode never uses it, and every condition a missing unit decides is `PENDING` in stub mode, never `PASS`.

### Fixes found by the first run (each with a test that fails on the old rule)

| # | crate | finding [measured, first runs] | fix |
|---|---|---|---|
| F1 | itest | Join and FoldOccupancy `BadAccount`: no JoinShards | the world runs InitShards × 6 (operator), as the keeper land tests do |
| F2 | agents | 35 holdings, **no economy**: a provisional holding flips final lazily in its owner's first resident action (I-29, §5.6 step 5), but the bot waited for the herald to show `final` — a deadlock | `policy::final_by_rule` (stored final, or provisional with `now ≥ final_ts` and its cohort closed, I-47); the synthetic fixture's provisional holding now has an open cohort (bell 20, 2 filed, 1 settled) so "waits" stays meaningful. Test `a_provisional_holding_final_by_rule_acts` (failed first) |
| F3 | agents | 13 of 16 Musters `Insufficient`: the Muster counted troops trained in the same step, and the relay simulates each transaction against the landed state | Muster from the reserve only; the next step musters the rest. Test `a_bot_musters_only_troops_already_in_reserve` (fails on the old rule, checked) |
| F4 | agents | Builds `QueueFull` on Hamlets: `queue_free` looked for any free of the four stored items | running items (`kind ≠ 0`, `done_at > now`) below `Tier::queue_slots` (Hamlet 2, Town 3, City/Stronghold 4). Test `the_build_queue_counts_the_tiers_slots` |
| F5 | agents | 18–22 Builds `Insufficient`: the copy number matched queue items by building index; the item's `arg` is the **resource** (W3-B P1) | count Production items of the building's resource, running or finished-not-settled (the program's `copy_number`); `policy::QUEUE_PRODUCTION`. Test `building_copies_count_production_items_by_resource`. After it: 0 `Insufficient` Builds in the day [measured] |
| F6 | agents | the settle racer never marched: every province opens with a camp and `stay_targets` wanted camp-free provinces | camp-free first, then provinces with a camp (a free tile away from it). Test `the_settle_racer_marches_when_every_province_has_a_camp` (fails on the old rule, checked) |
| F7 | agents, bots | the settle racer's re-depart was answered `HostBusy`: it re-departed at its origin while in transit, which tests nothing (G12's `HostInTransit` is a Stays host at the destination before its settlement) | re-depart once the destination resolved the arrival bell, naming the destination as the host's province. Tests `the_settle_racer_redeparts_only_after_the_arrival`, and the fleet test now asserts the redepart's province account |
| F8 | bots | personas marched late or never within one day (their archetypes' sessions); an eager session at 0–20 s into a bell met `NotResident` 1,738 times (the province resolves bell − 2 about 70 s into the bell) | `Config.eager_personas` (off by default; itest turns it on): persona bots get a session every bell, **90–150 s in**. `Fleet::step_due` drives the fleet on a moved clock; the pacing is one `Pace` shared with `Fleet::run` |

### Herald: `.gz` siblings back (DECISIONS J2)

The main session decided J2 on 2026-09-28: `flate2 =1.1.10` (and its transitive crates) is approved for the herald's precompressed siblings; brotli is not needed now. W3-D's deterministic gzip siblings (mtime 0, level 6), removed by integ-W3 in `476ba03`, are restored (reverse-applied; the later "a Same write is synced with the next checkpoint" change kept), with the gz assertions of `files.rs`, `tests/fold.rs` (byte-identical trees including siblings) and `tests/server.rs`. The integrator applies the manifest request (R2) and records J2 as applied.

### Recorded herald fixtures (W3-E notes §2)

`crates/agents/fixtures/herald-recorded/` (56 files, ≈ 330 KB): the answers the **real herald** gave at the end of the in-process day (stub mode, bell 144): `/h/season`, overviews of rings 0–2, the 19 provinces of rings 0–2 (`latest`), the bell-region files of bells 141–143 that exist, `/h/me` of 21 wallets (indices < 8 and every persona) and `index.json` (seed, bell, each wallet's spec). Producer: `FRONTIER_RECORD_FIXTURES=1 ITEST_STUBS=1 cargo test --release -p itest --test inproc_day -- --include-ignored`. Checker: `agents` `the_recorded_herald_parses_and_drives_the_policies` (every file parses with the same readers as the synthetic set; every anchor verifies under the season's test key at `tlock_round(bell)`; each wallet is `keys::wallet(seed, index)` and joined; the policies decide on the recorded world and every planned march passes the kernel checks). **Honest labelling:** the set is a snapshot, not byte-reproducible (keeper payers are drawn from the OS CSPRNG; rent payers and signatures differ run to run), so its checker is the parse-and-decide test, not a freshness test; the synthetic set (`fixtures/herald`, producer `fixture::world`) keeps its freshness test.

## 2. Runs [measured, 2026-09-28, release, this machine; program = test-beacon `.so` sha256 `6555facc…96ca` built from this branch]

**Final stub-mode run** (`ITEST_STUBS=1`, 100 bots, 144 bells + 8 drain, 11,456 slots, 160 s wall; summary `(session scratch)/scratchpad/w4f/sum10.json`):

| | |
|---|---|
| chain records | JOIN 100/100, SETTLE 103, HARVEST 201, BUILD 126, TRAIN 84, MUSTER 44, EXPLORE 60 (+60 EXPLORE_RESULT by the keeper), DEPART 23, REVEAL 2 (self_tip's own), HOLDING_FINAL 16, FOLD 450, ANCHOR 2,384, SEED 2,368, BEACON 2,400 |
| failed on chain | Reveal `BadAddress` 2 (forger, expected), Reveal `WindowClosed` 1 (late revealer, expected), Depart `TipTooLow` 1 (zero_tip direct, expected), Muster `Insufficient` 1, Muster `NotResident` 1 (bell-boundary races) |
| relay | 100 Joins, 100 FileTickets, 201 Harvests, 126 Builds, 84 Trains, 46 Musters, 58 Explores, 23 Departs sent; refused at simulation: spammer `QuotaExceeded` 375 (429), Depart `HostBusy` 0 after F7, `TipNotPreset` 1 (zero_tip), `/f/reveal` 16 accepted by the keeper API (queued; the pipeline is W4-C's), `BadPlaintext` 2 (bad_plaintext, expected), `WindowClosed` 1 (late revealer) |
| seals | 5 bad seals marched: stock opener codes 2, 5, 2, 2, 2 (garbage seals `BAD_POINT`, the bad plaintext `PLAINTEXT_INVALID`); none settled (SettleTransit is a stub) |
| personas (bot-visible) | forger, late_revealer, spammer, zero_tip **observed**; the other nine `needs-chain` |
| keeper | alerts: `reveal-pool-low` × 4 (bells 0–1, payer care fills the reveal pool from the funders; W2 behaviour) |
| herald | 8,667 events folded, alarms 0, no clash file |
| conditions | PASS activity, personas, keeper-writes, herald, relay-drain-guard; PENDING no-stubs (14 program stubs), zero-stuck-province-bells (stand-in), transits (23 unsettled), bad seals (5 marched, 0 settled), min_tip (2 due, unrevealed) |

**Strict mode on this branch** (the gate's mode; expected to fail until W4-A/W4-B/W4-C merge): see §3 item 5. The conditions fail for the reasons the stubs predict — `no-stubs` (the probe lists the 14 stub instructions), `activity` (no Muster lands: every Province stays at `resolved_next = 0`, 2,172 Musters refused `NotResident` at the relay), `zero-stuck-province-bells` (5,328 province-bells behind over 37 provinces), transits, bad seals and min_tip (no march). `personas`, `keeper-writes`, `herald` and `relay-drain-guard` pass.

**The final run after all merges** (the brief's second run) belongs to the integration window: after W4-A…W4-E merge, `(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)` runs `inproc_day` strictly (it rebuilds the test-beacon `.so` from the merged tree). What the strict mode needs from the merged units: GatherClash/ResolveFromInputs/SkipQuiet that admit a Stays host into the destination's roster (the settle racer's `HostInTransit`), SettleTransit with the stock seal code, and the keeper's reveal pipeline, SettleDeparture, gather/resolve/skip and SettleTransit duties. Wall time on this branch: 82 s strict, 104–160 s stub [measured]; the 8-bell drain covers arrivals ≤ bell 148.

## 3. Gate items that concern these files (run on this branch)

| # | Command | Result |
|---|---|---|
| 1 | `(cd frontier-node && cargo fmt --all -- --check)` | pass |
| 2 | `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | pass (with this branch's lock, §6) |
| 3 | `(cd frontier-node && cargo test --locked --workspace)` (Gate W1 line) | pass: 199 passed, 0 failed, 4 ignored; 3 min 1 s; `web3.js conformance NOT RUN` 0 times |
| 4 | `(cd frontier-node && cargo test --locked --release --workspace)` (Gate W2/W3 line) | pass: 199 passed, 0 failed, 4 ignored; 1 min 35 s; `web3.js conformance NOT RUN` 0 times |
| 5 | `(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)` (Gate W4 line, PATH with the Solana tools) | **exit 101 on this branch, as designed**: `inproc_day` (strict; the `.so` built by the test itself, origin `scripts/build-frontier.sh --features test-beacon`, 11,456 slots, 108 s) fails `no-stubs` (the 14 stub instructions), `activity` (MUSTER 0: 2,172 Musters refused `NotResident`), `zero-stuck-province-bells` (5,328 behind), transits, bad seals and min_tip (no march); `personas`, `keeper-writes`, `herald` (8,567 events, no alarm), `relay-drain-guard` pass. Cargo stops at that binary, so `inproc_smoke_one_bell_with_the_test_key` was run on its own: pass. `lag_gate`/`crash_injection` are W4-C's (none on this branch) |
| 6 | `ITEST_STUBS=1 … inproc_day` (the first run) | pass (above) |

Not run by this unit (not its files): the root workspace lines, `permutation-frontier` clippy/tests, `svm-tests/run.sh`, `build-frontier.sh --twice` (the test-beacon build was run: `scripts/build-frontier.sh --features test-beacon`, e_flags 2, `.so` 563,712 B, deployable no), the verifier's `tamper_` line (W4-D), the gateway `npm test`, `build-wasm` (the integrator's, now unblocked).

## 4. Deviations and choices (for review)

1. **The relay is an in-process Rust stand-in**, not the Node relay: the brief lists bots + keeper + herald; the stand-in keeps every check a bot or the chain can observe (§1) and names what it does not emulate. The E5 stack (W5) runs the real relay.
2. **Stub mode** writes Provinces directly (`standin`). It is opt-in (`ITEST_STUBS=1`), labelled in every output, and the strict mode (the default and the gate's) never uses it.
3. **All bots join on day 0** in `inproc_day` (`Mix.day0_share = 1.0`; join bells spread over day 0), and **personas are eager** (a session every bell): a one-day run otherwise leaves 40% of the bots idle and most personas unmarched. Both are itest settings; the bots' defaults are unchanged.
4. **Chain facts from the feed, not from the verifier**: `checks` recomputes the stock seal code itself (the verifier V5 is W4-D's); the two must agree, and the verifier's recorded fixtures are W4-D's.
5. **The `.so` is built inside the test** unless `PSF_FRONTIER_SO` is given: a gate after a merge must not run a stale program; the build is incremental.
6. **Recorded fixtures are a snapshot** (not byte-reproducible; §1).
7. **Resident actions at the start of a bell.** The province resolves bell − 2 about 70 s into bell `b` (close at the bell start + the seed margin + the resolve), so a resident action in the first ~70 s is `NotResident` by rule. Eager personas act 90–150 s in; a policy-level residency check (wait until the herald shows `resolved_next + 1 ≥ bell`) is the better fix but needs the real resolver's records (the stand-in is invisible to the herald), so it is left for W5-C with the real resolver.

## 5. Findings for other units and the integrator

1. **Integrator:** apply R1/R2 (§6); record J2 as applied (flate2 in, `.gz` siblings restored by W4-F; brotli not needed). The final strict `inproc_day` run is the integration window's (§2).
2. **K9 (cohort rule):** not implemented here — SettleTicket is in `permutation-frontier/src/proc/citizen.rs`, which no wave-4 unit owns; per the wave note the integrator implements it with a failing-first test in its window.
3. **W4-A:** the settle racer's `HostInTransit` (G12, §8.6) is observable only if the resolve admits a Stays arrival into the destination roster with its transit unsettled; `inproc_day`'s strict run and the bot verdict depend on it.
4. **W4-C:** `/v1/reveal` queued 16 owner reveals in the stub run (tracks stay `queued` on this branch); `min-tip-revealed-by-keepers` checks that keepers reveal every `min_tip` march.
5. **Relay (gateway) and W5-C:** the spammer's identical Harvests in one slot are the same signature; the stand-in answers the chain's duplicate refusal as `RelayRejected` (the Node relay's replay cache should answer it with its own code). Depart's refusal order (`NotResident`, `HostBusy`, then `HostInTransit`) matches G12's intent; §5.10's sentence lists Depart with Dissolve/Explore, whose order puts `HostInTransit` first — worth one line in the contract.
6. **K11 bot items:** verdict codes — the real keeper/relay/program codes are now what the fleet sees in process (forger `BadAddress`, late revealer `WindowClosed`, zero_tip `TipNotPreset` + `TipTooLow`, spammer 429 `QuotaExceeded`), all recognised by the report; `Hold` slot length — 3 bells = 225 slots at 20× in the day (the clock's scale or 8 game s per slot), correct; journal/transport recovery, the 4-province path search, control port 41070 and the 1,000-bot CPU measurement stay W5 (the stack launches the bots as a process).

## 6. Dependency requests (integrator, I-55)

- **R1 `itest`:** dependencies `frontier-abi`, `permutation-rules`, `findex` (workspace), `keeper`, `herald`, `agents`, `bots` (path), `axum`, `serde_json`, `sha2`, `hex`, `base64` (workspace) — no new crate, lock edges of the `itest` package only. (`localnet`, `fclient`, `tokio` and the `drand-replay` dev-dependency were already there.)
- **R2 `herald` (DECISIONS J2, approved 2026-09-28):** workspace `flate2 = { version = "=1.1.10", default-features = false, features = ["rust_backend"] }`, herald `flate2.workspace = true`; `Cargo.lock` gains `flate2 1.1.10`, `miniz_oxide 0.9.1`, `crc32fast 1.5.2`, `adler2 2.0.1`, `simd-adler32 0.3.10` (resolved offline from the local cache). Reason: the herald's `.gz` siblings (§8.4).
