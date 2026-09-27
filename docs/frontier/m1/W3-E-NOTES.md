# W3-E bots-agents — notes

- **Unit:** W3-E (wave 3), branch `frontier/m1-W3-E` cut from `frontier/m1-integ` at `241c52d`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.3: §3.3–§3.5, §4.1, §5.9–§5.11, §8.1, §8.3 (relay shapes, quotas, `/f/reveal`), §8.4 (herald files), §8.6 (bots and the 13 personas), §9.1–§9.3, §10.1, §11 (W3-E brief), §12 Gate W3, §13.4 criteria 5, 7, 8, 9; I-36, I-44, I-47, I-49, I-53, I-54.
- **Owned paths touched:** `frontier-node/crates/{agents,bots}/**` and this file. Integrator-owned files changed on this branch so it builds (dependency requests, §5): the dependency sections of `crates/{agents,bots}/Cargo.toml` and `frontier-node/Cargo.lock` (dependency edges only).
- **Tags:** [measured] = run on this machine on 2026-09-28; [design] = a rule implemented as the contract states it; [sim] = taken from `frontier-sim`.
- **Not done, by rule:** no push; no devnet/mainnet transaction; no download or install of any kind (no new crate either: every dependency added is already in the workspace lock). O-M1-12 items do not touch this unit. No service was started: the bots bind no port, and the tests bind only `127.0.0.1:0`. `DECISIONS.md` already records the rustfmt/clippy install (part A, 2026-09-27 row), so nothing was added there (and it is not this unit's file).

## 1. What landed

### `frontier-agents` (lib `frontier_agents`, no IO)

| Module | Content |
|---|---|
| `profile` | `Arch`, `ARCHS`, `Profile`, `profile()` copied field for field from `frontier-sim/src/model.rs` (I-36). `Mix` (the simulator's `human_mix`, `bot_share` 5%, `day0_share` 0.6) and `roster(n, seed, mix)`, which deals archetypes, stratified factions and join days by `Sim::make_agents`' rules. Personas: `personas_for(n)` = `clamp(n/200, 1, n/100)` per persona (5 each at 1,000 bots, 1 each at 100), drawn from non-idle agents, who join in the first 12 bells of day 0. |
| `persona` | The 13 personas of §8.6 with their names, expected outcomes, `needs_direct()` and `locally_checkable()`. This replaces W1-F's skeleton list. |
| `rng` | The simulator's SplitMix64. |
| `keys` | Wallet, session and direct keys: `ed25519(sha256("PS-FRONTIER-BOT-v1" ‖ le64(seed) ‖ le32(index) ‖ role))`. These are for tests and local seasons only. |
| `obs` | Herald readers: `SeasonView` (`/h/season`, with the Season account decoded, `tlock_round`, `tip_min`, the three tip presets of §8.3), `MeView` (`/h/me`; 404 means not joined), `ProvinceView` (the §9.2 envelope: Province, ArrivalSlots, ArrivalDay, ClashInputs), `Overview` (the §9.3 binary, decode and encode), `BellView` (`/h/bell`: THE anchor, `S`, caches, archive state; `seed_source()`, `close()`). All of them make up an `Observation`. Account bytes are decoded with `fclient::decode`; the herald's own convenience fields are used only for pacing. |
| `path` | Dijkstra over the observed provinces' passable, rough and road masks with the kernel's hex costs. It produces ≤ 32 steps and ≤ 4 provinces, in `hex::DIRECTIONS` order, with the path provinces other than the destination (Reveal's accounts). `earliest_arrival` is the kernel's. `adjacent_tiles` gives Explore's tiles. |
| `policy` | `decide(obs, ctx) → Vec<Intent>`, deterministic in (seed, bot index, bell, observation, memory): `Rng::fork(seed, index ‖ bell)`. The order is the simulator's `session`: (1) Join from the join bell; (2) duties on the bot's own marches: owner reveal via the keeper in the arrival bell with probability `1 − withhold`, SettleTransit once `now ≥ close + 600` and the destination is resolved (an eager share `q`; the settle racer always); (3) no holding → FileTicket (free sites of rings ≥ 2 in the own wedge, outermost ring first, ≤ 3 sites); provisional → wait; (4) a session: Harvest, Build (the cheapest affordable building; the `thrift` check keeps enough for a garrison), Train (in hundreds, the doctrine's unit; scouts), Muster, Explore with a scout host and SettleExplore once the seed exists, then a march (camps first with probability 0.6, else enemy unshielded holdings; nearest first; stance by `pick_stance`, retreat by `q`, tip by preset). `Memory` holds the marchbook entries (`MarchMemo`) and the persona state. |
| `fixture` | The fixed herald world the tests read (§2). |

### `frontier-bots` (lib `frontier_bots`, bin `frontier-bots`)

| Module | Content |
|---|---|
| `ports` | `HeraldPort` (`HttpHerald`, `DirHerald` over recorded files), `RelayPort` (`HttpRelay`), `DirectPort` (`RpcDirect` over `frontier-localnet`: blockhash, simulate, send, airdrop, balance, `frontier_hold`; `NoDirect`). The traits are async and `Send + Sync`, so W4-F can plug in-process ports into the same bots. |
| `seal` | Honest seals use the stock Rust `tlock =0.0.10` through `fclient::seal`, to `tlock_round(arrive)` under the drand key `/h/season` publishes, so the test key works (I-53). Garbage seals are 165 random bytes; byte 0 is a compressed-G2 flag so Depart's syntax check passes, and the commitment to a valid plaintext is valid. Bad-plaintext seals are honest seals over stance 9. `SealPool` is a shared blocking pool with one worker per core. |
| `journal` | The marchbook: JSON lines, `fsync`ed, one file per process. The `sealed` line (plaintext, salt, commit, seal, `ct_hash`, round, tip, kind) is written **before** the Depart is signed (§9.1); state lines follow. `load` folds the file back into each bot's memory, skipping a torn last line. The key `k` is never stored. |
| `txb` | The relay's sponsored shapes: `[SetComputeUnitLimit(budget), SetComputeUnitPrice(0), SetLoadedAccountsDataSizeLimit(L(kind)), one instruction]`. The fee payer is the one `GET /f/relay` drew; every other signer has signed. Budgets come from `frontier-abi/vectors/budgets.json`, embedded at build time. A settle shape's requester signature covers the message bytes (§8.3 v1.3). Direct transactions are complete, paid by the persona's own key. |
| `bot` | `Bot::observe` (season and overviews cached and shared; own `/h/me`; ≤ 12 provinces: holdings, march destinations, neighbours, ticket candidates; the bell files the duties need) → `reconcile` (a march whose transit record is gone after its arrival bell counts as settled; settled marches older than two days are forgotten) → `decide` → `act`. Pacing: sponsored writes stay inside `/h/me`'s `quota.left`, with 6 kept back for marches and settlements. The spammer ignores the quota, which is its test. `ClockSource::Game` extrapolates the herald's `latestSlot`/`latestUnix` with `fclient::clock::GameClock`; `Fixed` is for tests. |
| `fleet` | `Fleet::step_all(session, concurrency)` runs one decision per bot concurrently on tokio tasks; the tests and W4-F drive it. `Fleet::run(until, stop)` gives each bot its own task and its own schedule. Sessions follow the profile (`day_p`, `sessions` spread over the game day). Duties are polled every bell, 0–20 game seconds in (§9.1), while the bot is onboarding (its first two days) or has an unsettled march. `restore` reloads the marchbook after a restart. |
| `report` | One `Outcome` per action. Counts are grouped by archetype or persona × action × result. Each persona gets a verdict: `observed` / `violated` / `pending` for the five a bot can see itself (settle_racer `HostInTransit`, late_revealer `WindowClosed`/`LatchClosed`/`Archived`, forger `BadAccount`/`BadAddress`/`WrongRegion`/`NoAnchor`, spammer 429/`QuotaExceeded`/`Bucket`, zero_tip `TipTooLow`/`TipNotPreset`), and `needs-chain` for the rest (the stack report and the verifier judge those: E5 criteria 5, 8 and 9). |
| `main` | `frontier-bots --herald URL --relay URL [--rpc URL] [--seed] [--bots 1000] [--first-index] [--days 7] [--personas default\|off\|N] [--journal DIR] [--report FILE] [--invites FILE] [--game-hours H] [--scale S]`. Only loopback `http://` URLs are accepted (exit 2 otherwise). The report is written atomically every 30 s and at the end. With `--rpc`, the program id comes from `/h/season`. |

### Personas (§8.6) as implemented

| Persona | What the bot does | Route |
|---|---|---|
| min_tip | tip = `tip_min`, never self-reveals | relay |
| garbage_seal | garbage seal; the owner reveals through the keeper only when no other faction has revealed more arrivals into that province-bell | relay, keeper |
| bad_plaintext | honest seal over stance 9 (the owner's reveal attempt gets 422 `BadPlaintext`) | relay, keeper |
| settle_racer | garbage seal to a quiet "stays" tile; while in transit it tries to Depart the same host again (`HostInTransit`); it settles at the first instant | relay |
| prefunder | before each march: 1,000,000 lamports to the march's 4 slot addresses, ArrivalDay, ClashInputs, THE anchor and 3 seed-cache nonces | direct |
| squatter | 100-troop hosts (3) into the province where its faction has the most hosts, all in one bell | relay |
| late_revealer | reveals after `A + W` by THE anchor (or 2 bells after arrival when the bell file is missing), through `/f/reveal` and with its own Reveal | keeper, direct |
| forger | its own Reveal with a non-canonical anchor; SettleTransit with a non-canonical slot key | direct, relay |
| spammer | 120 sponsored Harvests a day until 429 / `QuotaExceeded` / `Bucket` | relay |
| double_arrival | two hosts to one province-bell (distinct transit slots) | relay |
| zero_tip | Depart with tip 0 through the relay (`TipNotPreset`) and directly (`TipTooLow`) | relay, direct |
| self_tip | sponsored Depart at `2 × tip_min`; its own Reveal with beneficiary = its wallet, bid at 2 × the minimum reveal priority | relay, direct |
| ticket_holder | settles its own ticket first (SettleTicket, anyone may), then `frontier_hold([Province], 500, 3 bells)` | direct |

### Tests

| Test | What it checks |
|---|---|
| `agents::profile_equality` (3) | Compiles `frontier-sim/src/{model,rng,config}.rs` beside the copy. Every archetype matches field for field (a destructuring pattern, so a new field on either side fails to compile). The generator matches draw for draw (4 seeds × 1,000 draws, forks included). The default mix matches the simulator's `Config::default()`. |
| `agents::herald_fixtures` (12) | Fixtures fresh (one producer, one checker, §3.5). The world parses and is coherent (every anchor verifies under the season's test key at `tlock_round(bell)`; the overview round-trips; the tip presets). W2-E's gateway herald fixtures parse with the same readers. An unjoined bot joins from its join bell, not before. A joined bot tickets in its own wedge, ring ≥ 2, free sites, and does not refile within a bell. A provisional bot waits (the ticket_holder holds). A final bot builds, explores and marches, deterministically, and **every planned march passes the kernel's own checks**: `seal::validate`, a walk over passable hexes of observed provinces ending on the destination tile, `travel::path_cost`, `travel::check_arrival_bell`. Personas shape their marches (tips, seal kinds, stays target, prefund targets, spam once a day). Owners reveal in the arrival bell by persona (min_tip never; late only after close; self_tip direct; forger forged plus honest). double_arrival and squatter send 2 and 3 hosts to one province-bell. The roster follows the mix (143/427/285/86/9 humans + 50 bots at 1,000; stratified factions ±1; join days; 5 per persona; deterministic). |
| `bots` unit (4) | Seals: honest opens VALID with the test key and fails with the next round's signature; garbage gives 1 or 2 and its commitment is consistent; bad plaintext gives 5. Journal round trip, states, failed march dropped, torn line skipped. The sponsored shape (limit and loaded limit equal the budgets table, price 0, fee payer unsigned, session signature verifies, wire round trip, requester signature). Report verdicts. |
| `bots::fleet` (7) | Against the fixture herald (`DirHerald`) and a mock relay that enforces §8.3's shape checks: four fixture wallets act (Join through `/f/join`, FileTicket, a provisional bot waits, a final min_tip bot builds and marches at `tip_min`). **The journal holds the march before the relay sees its Depart.** The Depart's seal opens with the test key at `tlock_round(arrive)`, and the journal restores the march. Owner reveals send exactly the marchbook's plaintext, salt and `ct_hash` to `/f/reveal`. The personas get their refusals and verdicts: zero_tip `TipNotPreset` + `TipTooLow`, late_revealer, forger, spammer; the ticket_holder holds 225 slots = 3 bells at scale 20. The settle racer's redepart is refused `HostInTransit`, then it settles with the logged commit and seal, the requester signature and its citizen. Reconciliation of marches settled by others. **1,000 bots in one process**. HTTP ports on a loopback server at `127.0.0.1:0` (404 means not joined; the refusal code is carried). |

## 2. The herald fixtures (honest labelling)

`crates/agents/fixtures/herald/` (45 files, 252 KB) is **synthetic**, not recorded: W3-D's herald is built in the same wave. `fixture::world()` writes it and `fixtures_are_fresh` checks it (`FRONTIER_WRITE_FIXTURES=1 cargo test -p agents --test herald_fixtures` rewrites it).

- The land is the kernel's: `terrain::generate_province` and `camp::place` for rings 1–2.
- The accounts are written at the `fclient::abi::layout` offsets and read back with `fclient::decode`.
- The anchors are signed by the test key.
- Contents: bell 40; wallets `keys::wallet(7, i)` for i = 0 (not joined, 404), 1 (joined, no holding), 2 (provisional), 3 (final: a combat host and a scout at home, and a march in transit arriving at bell 38), 4 (a faction-1 holding next door); bell files for 38 (anchored and seeded) and 40 (anchored).

W2-E's gateway fixtures (a second, independent producer of the same format) are parsed by the same readers in `w2e_gateway_fixtures_parse_with_the_same_readers`. **W4-F should re-record** these files from the herald of the first in-process day. The readers must accept both.

## 3. Measurements [measured, 2026-09-28, release, this machine]

| Item | Result |
|---|---|
| 1,000 bots, one decision each (`step_all`, 1,000 concurrent tasks, fixture herald from disk, mock relay) | **31 ms** (235 of them joined: the ones whose join bell ≤ 40) |
| 200 tlock seals on the shared pool (`available_parallelism` workers) | **25 ms** (≈ 0.13 ms each, amortised over the cores) |
| `cargo test --locked --release --workspace` (frontier-node, all crates) | 144 passed, 0 failed; 1 min 44 s wall with a warm cache |

The 1,000-bot figure measures the bot side only. A real run is bounded by the herald and the relay: per decision ≤ 1 `/h/me` + ≤ 12 province files + the bell files, and one `GET /f/relay` + one POST per sponsored write. The offchain design's "≤ 2 cores at 20×" [estimate] remains W4-F/W5's to measure on the stack.

## 4. Gate W3 items run for this unit's files

From the integration worktree layout, on this branch:

| Command | Result |
|---|---|
| `(cd frontier-node && cargo fmt --all -- --check)` | pass |
| `(cd frontier-node && cargo clippy --locked --workspace --all-targets -- -D warnings)` | pass (with this branch's lock, see §5) |
| `(cd frontier-node && cargo test --locked --workspace)` | pass (all crates) |
| `(cd frontier-node && cargo test --locked --release --workspace)` | pass: 144 tests, 0 failed |

Not run by this unit (not its files): the `permutation-frontier/svm-tests` lines, `build-frontier.sh`, the gateway `npm test`, frontier-sim. The binary was smoke-tested for argument handling only: exit 2 without `--herald`, exit 2 on a non-loopback URL. It was **not** run against a live stack, because nothing in wave 3 starts one (W4-F/W5).

## 5. Dependency requests (integrator, I-55)

No new external crate. Only package dependency edges change in `frontier-node/Cargo.lock`:

- **R1 `agents`:** `permutation-rules`, `sha2`, `hex`, `base64`, `serde_json` (all `.workspace = true`). Kernels for paths, catalog, doctrine and profiles; key derivation; herald JSON/base64/hex.
- **R2 `bots`:** `permutation-rules`, `tokio`, `serde_json`, `hex`, `rand` (all `.workspace = true`). The async runner, JSON bodies, journal hex, OS randomness for garbage seals and salts. There is also a `[lib] name = "frontier_bots"` section; it is not a dependency field.

On the integration branch: apply the two manifests and run `cargo build` (without `--locked`) once to update the edges, then gate with `--locked`.

## 6. Deviations and choices (for review)

1. **Join spread.** The simulator spreads late joins over days 1..=21 of 28. `Mix::for_season_days(d)` spreads them over `1..=d−1` (6 for the 7-day season). The shares and rules are unchanged.
2. **Roster draws.** They follow `make_agents`' rules but are not bit-identical to it, because the simulator interleaves stake and fee draws in the same stream. Profiles, mix and generator **are** identical and tested. Shades (M3) are left out.
3. **Personas** take non-idle agents (each keeps its archetype's pacing) and join in the first 12 bells of day 0, so a one-day run (W4-F, 100 bots) sees all 13.
4. **Direct transactions.** The relay never sponsors a Reveal, a transfer or a non-preset tip (§8.3), so six personas use their own airdropped key through `--rpc` (localnet only): prefunder, the direct parts of late_revealer, forger and zero_tip, self_tip's reveal, and ticket_holder. Without `--rpc` those parts record `NoDirectPort`. For **zero_tip**, §8.6 expects `TipTooLow`, which is the program's answer; through the relay the answer is `TipNotPreset`. The bot tries both and the report accepts either.
5. **self_tip's reveal** is its own transaction, because `/f/reveal`'s body carries no beneficiary: a keeper-sent reveal pays the keeper's beneficiary.
6. **Building counts.** The Holding stores none, so the bot estimates the `n` of `catalog::building(item, n)` from `production[r]` above the tier base plus queued items. A wrong estimate is a refused Build (`Insufficient`), never a wrong state.
7. **Pacing binds on the relay quota** (40 a day on days 0–6), which is tighter than the 30/h, burst 60 bucket every day. The bot keeps 6 transactions for marches and settlements.
8. **No CommitPosture.** The offchain design lists it, but postures are M3 (I-16). There are no pair tickets either (M3).
9. **Ticket site counts.** The overview does not say how many sites a province has, so a ticketing bot reads the candidate provinces' files (within the 12-province limit) before naming sites.
10. **settle_racer's "Stays host"** marches to a quiet tile: a province in view with no camp and no other faction's host.
11. **Owner reveals** count as done when `/f/reveal` answers 202 (queued at the keeper). The bot does not re-check the slot. A lost queue entry is the keeper's to report (`/v1/track`), and the verifier's `ValidSealUnrevealed` covers it.

## 7. Pending (later waves)

- **W4-F:** `itest::inproc_day` (100 bots over `ChainPort::InProcess` with the test key) drives `Fleet::step_all` through in-process `HeraldPort`/`RelayPort`/`DirectPort` implementations. It also re-records the herald fixtures (§2) and fixes whatever the first real day finds in agents and bots (it owns both paths in wave 4).
- **W5/W6:** `frontier-stack` launches `frontier-bots` with stack ports in 41000–41999 and reads `--report` into the E5 report (criteria 5, 7, 8, 9). It also covers the chain-side persona verdicts (`needs-chain`: min_tip revealed by keepers, garbage and bad seals destroyed with the stock code, squatter displaced, double arrival bounced without loss, prefunder never blocking, ticket_holder never keeping a site it did not win), and the bots' CPU at 20× (target ≤ 2 cores).
- **Real relay and keeper:** the exact refusal code names the real relay and keeper return for forged and late actions (the report lists every code it saw, and anything unexpected shows as `needs-chain`, never as a pass).
