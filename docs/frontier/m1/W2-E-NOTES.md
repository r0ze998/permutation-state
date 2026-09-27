# W2-E web-foundation — notes

- **Unit:** W2-E (wave 2), branch `frontier/m1-W2-E`, cut from `frontier/m1-integ` at `ed31438`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.2 — §3.3 (code rules, dependencies), §3.5 (tests), §7 (`seal`, `beacon`), §8.3–§8.4 (relay and herald contracts the page consumes), §9 (web data contracts), §11 (W2-E brief and ownership), §12 (Gate W2), §13.6 (E7 web smoke). Web area design `docs/frontier/m1/design/web.md`.
- **Owner decisions applied (2026-09-27):** O-M1-12 not approved — no `rustup target add`, no Playwright, no drand fetch, no Agave; the wasm32 build is **PENDING-OWNER** and the exports are tested on the host, as the brief says. rustfmt/clippy for 1.95.0 were installed by the main session with the owner's OK (recorded in `docs/frontier/DECISIONS.md` part A, see "Files outside the list" below).
- **Tags:** [measured] = run on this machine on 2026-09-27.
- **Not done, by rule:** no push, no chain transaction, no service started (tests bind `127.0.0.1:0` only), nothing downloaded beyond `npm ci --ignore-scripts` from the gateway's existing lockfile, `permutation-server/web/session.mjs` untouched.

## 1. What landed

### `frontier-wasm/` — the kernels for the browser (§9.5)

A `cdylib` + `rlib` outside the root workspace (own `[workspace]`, toolchain 1.95.0, `.cargo/config.toml` keeps `target/` local), depending on `permutation-rules` **without `std`** and `borsh =1.6.1` (the rules crate's own pin). Every export is `name(ptr, len) -> ptr` over linear memory: borsh in; out a **frame** `status u8 | len u32 LE | payload` released with `free(ptr, 5 + len)`; `alloc(len)` for inputs. Status 0 ok, 1 bad input (ASCII reason), 2 kernel refusal (`Refusal {code u8, arg u32}`, code tables in `api.rs`), 3 not in this build. Exports are unmangled only on `wasm32` (an unmangled `free` on the host would replace libc's).

Exports (28): all of §9.5 — `ruleset_hash, province_of, province_centre, ring_of, wedge_of, region_of, generate_province, plan_path, path_cost, earliest_arrival_bell, check_arrival_bell, bell_at, bell_start, tlock_round, seed_round, plaintext_pack, plaintext_unpack, plaintext_validate, commit, salt_of, body_xor, resolve_clash, resolve_from_inputs, reachable, accrual_at` — plus `abi_version`, `seal_root`, `ct_hash`.
- `plan_path` is a client-side planner (Dijkstra on `(secs, steps)` over hexes within reach of both ends, terrain from `terrain::generate_province` per ring seed, unknown land never entered, blocked hexes honoured) whose result is re-priced by `travel::path_cost`, so it only ever proposes a path the kernel accepts (≤ 32 steps, ≤ 4 provinces).
- `reachable` is deliberately optimistic (straight-line hexes at the unit's open-ground pace): a "no" is certain, a "yes" is only a warning (§7.7).
- **`resolve_from_inputs` answers UNAVAILABLE** (see deviations): building a `ClashInput` from Province + ClashInputs bytes is the program's code (W4-A); "verify this clash" can use `resolve_clash` meanwhile.

Host tests (`tests/exports.rs`, 14) call every export **through the frame protocol** (`frontier_wasm::call`): the §9.5 list; bad input is a frame, not a panic; `ruleset_hash` = `frontier-abi/vectors/presets.json` = the kernel; geometry over 500 random tiles = the kernel; `generate_province` = the kernel (+ passable mask); `plan_path` on 5 province pairs (cavalry never slower, a blocked first step changes the path, unknown land refused, too far = None), `path_cost` refusals; the clock exports against `clock-vectors-v1.json`; arrival-bell refusals carry the earliest/latest bell; `reachable` bounds; the seal exports against **all 24 cases of `seal-vectors-v1.json`**; `resolve_clash` = the kernel with its digest, a refusal for a bad faction; `accrual_at` = `value_at`. `wasm_vectors_are_fresh` writes/checks **`frontier-wasm/vectors/wasm-vectors.json`** (33 recorded calls: args as JSON, borsh input, framed answer) — the JS side's cross-check.

### `scripts/build-wasm.sh` (+ `--check`)
1.95.0, `wasm32-unknown-unknown`, `--locked --release` (opt-level `s`, `panic = "abort"`, LTO, one codegen unit, stripped), `--remap-path-prefix` for the checkout, cargo home and rustup home; checks every crate export + `alloc`/`free`/`memory` is present (reads the `exports!` list), the 400 KB raw / 150 KB gzip budget; writes `web/frontier/wasm/frontier.wasm` + `frontier.wasm.sha256`. `--check` builds into a scratch target dir and fails unless the committed artefact is byte-identical. **Without the target it changes nothing, prints `PENDING-OWNER …` and exits 3** (usage error: 2).

### `scripts/vendor-noble.mjs` + `permutation-server/web/sdk/vendor/noble/`
Copies the **import closure** of `curves/bls12-381.js`, `hashes/sha2.js`, `hashes/utils.js` from `permutation-gateway/node_modules/@noble/*` (installed by `npm ci` from the gateway lockfile, which already pins `@noble/curves 1.9.7` and `@noble/hashes 1.8.0`), renames to `.mjs`, rewrites bare/relative specifiers to relative paths (comments masked, so doc-comment examples are untouched), adds both LICENSE files and `manifest.json` (`packages` with version/resolved/integrity from the lockfile, `entries`, `files: {path: sha256}`). `--check` exits 1 unless the tree equals what would be written. 17 files; 235,340 B of `.mjs` raw, 68,281 B gzip [measured]; loaded only by the seal worker.

### `permutation-server/web/frontier/` — the page foundation
| File | What |
|---|---|
| `index.html`, `practice.html`, `spectate.html` | skeletons (topbar with bell chip and language toggle, stale/error banners, the map canvas with an accessible name and summary, a panel, bottom tabs), `data-mode` per page, one module script, no inline code or third-party origin |
| `app.mjs` | boot: config → `/h/season` → pins (`setPin`: Season PDA re-derived, ruleset hash) → beacon pin → chain clock → chip/status renderers → map + overviews; 30-s season poll through `nextPoll`, paused while hidden. W4-E routes the screens here |
| `fstate.mjs` | store `FS` + render scheduler (state.mjs pattern) |
| `config.mjs` | same-origin herald `/h/*` and relay `/gw/*`; `?herald=` only from a loopback page to a loopback herald |
| `abi.mjs` | **generated** from `frontier-abi/vectors/{layouts,errors,presets}.json` + the beacon block of `frontier-vectors.json` (layouts, records, 62 error codes, ruleset hash, quicknet and test-beacon pins); freshness = `web-frontier-codec.test.mjs` (`PSF_WRITE_ABI=1` rewrites) |
| `fcodec.mjs` | layout-driven decoder of every account kind (u64/i64 → BigInt, records, arrays), magic/size/season checks, the Season tombstone, effective status, `W(b)` |
| `faddr.mjs`, `fgeo.mjs` | with-seed addresses of every kind, Season PDA + bump found locally, citizen/keeper tags, join shard, host id ⇄ parts; province geometry (locate, centres, rings, wedges, regions, dense index) |
| `herald.mjs` | read path: `/h/season`, province envelopes (§9.2, every account decoded and checked against its key and the pinned season), the overview binary (§9.3), bell-region, me, events, clash; LRU (256) for immutable files, `latest` never cached; `DiffStream` (seq, duplicates, gap → resync); `nextPoll` (own 10/30 s, overview after the bell + 0–15 s jitter, backoff ×2 to 5 min, paused when hidden); `wantedProvinces` (own always, ≤ 12 visible at tile LOD); `staleness` (banner past 60 s); never rejects |
| `fchainio.mjs` | write path: relay base, pins, `request` (never rejects), `relayInfo` (program checked against the pin), `sendTx`, `sendJoin`, `reveal` (I-24 material), `nudge`, `txStatus`, `quota`; `messageProblems` (fee payer, exact signers, `[limit, price = 0, loaded-data]` prefix + budgets' limits, one Frontier ix of a player/settle tag, Reveal → `UseRevealRoute`); `sameMessage` after co-signing |
| `fsession.mjs` | the pinned §9.1 text (never translated, printable ASCII), seed domain `PS/frontier-session/v1`, `ps-fsession:` keys, derive (signature verified; a wallet that signs another text is refused), remember/restore/forget, `matchesCitizen` (session + expiry), backup import against the citizens' keys; reuses only v9 `session.mjs`'s Ed25519 primitives |
| `clock.mjs` | round/bell arithmetic (first round at or after x), `ChainClock` (herald sample + local elapsed × detected rate, monotone), bell chip, the §6.2 pipeline (no countdown before THE anchor; the window closed by the Clock or by the BeaconLog reaching S; S at or after A + W + Δ; archived bells) |
| `seal.mjs` | plaintext pack/unpack/validate (kernel order), path codec, salt/commit/body/ct_hash/root, **`checkBeacon`** (quicknet only; the test key only when the pinned cluster is `localnet`; sha256(pk) must equal the Season's `quicknet_pk_hash` and the pin; period/genesis/chain hash), `sealMarch` (validate, then the worker; same code inline without Worker) |
| `seal-worker.mjs` | tlock IBE on G2 with the RFC 9380 G1 DST (compact-16), `openSeal` (FO check → `BadPoint`/`FoCheck`), `sealWith`, the **self-audit** (U = H3(σ,k)·G2, V recomputed with a second pairing, W, body reopened, re-pack, `validate` against the transit, commitment, seal root), `sealRequest` (draws k, σ; never returns them; zeroes them) |
| `marchbook.mjs` | §9.1 entries written before signing (k/σ refused and never read back), re-seal rules, self-reveal timing (bell start + 0–20 s, a second try only 60 s later with the slot absent and the window open, never before the bell, ≤ 2), reveal material, reconciliation (landed / swapped seal root → failed, never revealed / revealed / settled), bounded to 64 entries, lost-book hint |
| `fi18n.mjs` | faction names/colours (v9 tables), resources, units, stances, fates, tiers, doctrine C "Flame" (I-34), pipeline and season-status texts, **a JA/EN text for every one of the 62 error codes**, client refusal texts — all via `L`/`lazyTable` |
| `map/fmap.mjs`, `map/layers.mjs` | `FrontierMap` (LOD world/province/tile with hysteresis, culling, pick under a point, zoom around a point, drag/wheel/pinch/keyboard, redraw only when dirty, tile LOD only when the caller has WASM terrain), painters (province cells on the lattice, fills from the overview, presentation-only fog with the "show everything" switch, clash marker, tiles and sites) |
| `frontier.css` | phone-first skeleton (360 px, 44 px targets, bottom tabs under 760 px); W5-E owns the full pass |

### Language, map, glossary
- `lang/en-frontier.mjs` (140 entries), empty `lang/en-frontier-play.mjs`, both registered in `lang.mjs` `EN_GROUPS`/`EN` (**additive**: two imports, the group map and merge extended, one comment line reworded to mention them).
- `map.mjs`: one added `export { … }` line (`SQRT3, RADIUS, FLATTEN, COLORS, EDGE_NEIGHBOR, project, shade, alpha, rounded, polygon, hexPoints, inverseHex`); `web-frontier-map.test.mjs` asserts no line of `map.mjs` was removed or changed since `d95fa25`.
- `GLOSSARY.md`: section "The Frontier" (31 terms) and a "Never translate" bullet for the key text, seal domains, error names and storage keys.

### Tests (`permutation-gateway/test/`, all `node --test`, no network except `127.0.0.1:0`)
| File | Tests | Covers |
|---|---|---|
| `web-frontier-seal.test.mjs` | 10 | the 24 seal vectors byte for byte (plaintext, salt, commit, body, ct_hash, root, `validate` reason); **the browser's IBE reproduces the Rust `tlock =0.0.10` seals for all 17 fixed-σ cases**; opening with the recorded quicknet signatures (tampered cases fail as the program's codes); self-audit catches a flipped bit in U, V, W, body, a wrong transit, commitment or round; `sealRequest` never returns k/σ and zeroes them; beacon pin; T(b) rounds up; a seal to each test-key round opens with that round's recorded signature and not with another's |
| `web-frontier-codec.test.mjs` | 8 | `abi.mjs` freshness; every field of all 17 account kinds (3 random rounds each) against an independent reader; layouts tile their size; refusals; `effectiveStatus`, `W(b)`; every address of `addresses.json`, every seed of the kernel's `addr-vectors-v1.json` (all kinds the web derives, incl. citizen and defence-claim tags), the Season PDA + bump, citizen/shard/holding addresses, host ids and their inverse |
| `web-frontier-clock.test.mjs` | 6 | the kernel's clock vectors (bells over three drand phases, S, genesis_ts, window schedule); `ChainClock`; bell chip; the §6.2 pipeline |
| `web-frontier-herald.test.mjs` | 9 | fixtures fresh; a fixture herald on port 0: season from bytes (JSON conveniences ignored), envelopes checked, caching, overview binary, bell-region/me/events, marchbook reconcile against the fixture Holding, never rejects, diffs, polling, on-screen selection, staleness, LRU |
| `web-frontier-marchbook.test.mjs` | 6 | as listed above |
| `web-frontier-session.test.mjs` | 4 | the text pinned byte for byte (+ its sha256), never translated; distinct from v9 (text, domain, prefix); derive/refusals; keep/restore/forget; Citizen check; backup import |
| `web-frontier-wasm.test.mjs` | 6 (1 PENDING-OWNER skip) | JS borsh encoders = the recorded inputs of every call; decoders read every answer; **the page's JS geometry/clock/seal transcriptions = the kernel's recorded answers**; the loader over real `WebAssembly.Memory` (every buffer freed); hash refusal; the real `frontier.wasm` (hash, all 33 calls, budget) **skips with PENDING-OWNER** until built |
| `web-frontier-chainio.test.mjs` | 4 | pins; message checks (each deviation named); co-signed message unchanged; relay routes over a fake relay on port 0 |
| `web-frontier-map.test.mjs` | 9 | LOD hysteresis, projection/zoom, culling, picking, province cells share corners, fog, fills, the class without a DOM, `map.mjs` additive only |
| `web-frontier-shell.test.mjs` | 5 | config, scheduler, bell chip JA/EN, every error code JA/EN, page markup rules |
| `web-lang.test.mjs` (extended) | +2 | `webFiles()` now also scans `web/frontier/**.html`; the frontier modules and pages are in the scan, the groups registered, `sdk/` excluded; the session text never marked for translation. Completeness (a)/(b) green over `web/frontier/**` |

Fixtures: `permutation-gateway/test/fixtures/frontier/` — **synthetic** herald files (season record + Season bytes on the test beacon, one province envelope with a slot and a day, the ring-2 overview, a bell-region record, a viewer's Citizen and Holding with a transit, an events page), built byte-exact from the layout table by `make-fixtures.mjs` and checked fresh by the herald test. They stand in for W6-D's recording from a local season.

## 2. Measurements [measured]

| Item | Result |
|---|---|
| `frontier-wasm`: `cargo fmt --check`, `cargo clippy --locked --all-targets -D warnings`, `cargo test --locked` (debug and `--release`) on 1.95.0 | exit 0; 14/14 |
| Seal in node 20.19 (vendored noble): first seal + self-audit | 158 ms (JIT warm-up) |
| warm seal + self-audit (two pairings) / seal only / open with a signature | 44.2 / 21.9 / 11.3 ms |
| Vendored noble tree | 17 files; `.mjs` 235,340 B raw, 68,281 B gzip (design est. ≈ 212 KB / 50–60 KB) |
| `web/frontier/**` page files excl. the worker (html + css + mjs) | 141,155 B raw, 47,417 B gzip (first-load budget 200 KB gzip incl. shared files) |
| `permutation-gateway` `npm test` on this branch | 396 tests: **394 pass, 1 fail, 1 skip** — the fail is `web-sdk.test.mjs` "web/sdk holds exactly the browser-safe set" on the **pre-W2-D** test, which rejects the new `web/sdk/vendor/` directory; the skip is the `frontier.wasm` PENDING-OWNER. See §4 |
| `frontier.wasm` size | **not measured**: PENDING-OWNER (no wasm32 target) |

## 3. Gate W2 items for these files (run 2026-09-27 on this branch)

| Gate item | Result |
|---|---|
| `cargo fmt --all -- --check` (root; frontier-wasm is its own workspace) | exit 0 |
| `(cd frontier-wasm && cargo fmt -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --release)` | exit 0 (not in §12's list yet; see requests) |
| `(cd permutation-gateway && npm test)` | 1 failure, expected until W2-D merges (§4) |
| `(cd permutation-gateway && node scripts/sync-web-sdk.mjs --check)` | fails on this branch for the same reason ("vendor (not in the set)"); W2-D's extended script leaves `vendor/` alone |
| `node scripts/vendor-noble.mjs --check` | exit 0 |
| `if rustup target list --installed --toolchain 1.95.0 \| grep -q wasm32-unknown-unknown; then scripts/build-wasm.sh --check; else pending …; fi` | **PENDING-OWNER: build-wasm (wasm32 target, O-M1-12)** — the target is not installed; `scripts/build-wasm.sh` itself exits 3 with the same message |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |

A dry run of the merge order was done after the commit, see §7.

## 4. Deviations and why

| # | Deviation | Why | Who resolves |
|---|---|---|---|
| D1 | The page decodes accounts with its own layout-driven `fcodec.mjs` over a generated `abi.mjs`, and derives addresses in `faddr.mjs`, instead of importing `web/sdk/frontier/{codec,addresses}.mjs` | W2-D's SDK is built in parallel and was not on this branch; the brief's module list has no codec, but the herald client needs one. Both are generated from / checked against the same `frontier-abi` vectors, so they cannot drift silently | W3-F (owns `web/frontier/**` in wave 3) may switch to `sdk/frontier/*` and delete `abi.mjs`/`fcodec.mjs`/`faddr.mjs` if the APIs fit; the codec test then moves to the SDK |
| D2 | The plaintext, path, salt/commit/body/root, bell/round arithmetic and province geometry are also in JS (`seal.mjs`, `clock.mjs`, `fgeo.mjs`), not only in the WASM | the bell chip, the map at province LOD and the seal (incl. its self-audit's `Plain::validate`) must work before and without the lazily loaded WASM (design §3.3) — and the WASM is not buildable yet. Every JS transcription is pinned against the kernel's recorded answers (`wasm-vectors.json`) and the rules vectors | none (pinned) |
| D3 | `resolve_from_inputs` answers status 3 (UNAVAILABLE) | the ClashInputs → `ClashInput` builder (residents from entries, garrisons, walls, camp, occupancy) is the program's (W4-A). Duplicating it here would create a second, unpinned implementation | W4-A moves the builder somewhere both can call (e.g. `frontier-abi` or `permutation-rules`); W4-E wires "verify in this browser" |
| D4 | Marchbook entries carry three fields beyond §9.1's list: `holding`, `ctHash_hex`, `revealDelay` | the reveal body needs the Holding and `ct_hash` (the seal root cannot give it back); the delay is drawn once so a reload does not re-roll it | none (additive; recorded here) |
| D5 | The self-audit also recomputes V (a second pairing, ≈ 20 ms in node) | the design lists U, W, body, commitment and root; without V a pairing bug would pass the audit and fail on chain; the test "catches a flipped bit in V" needs it | none |
| D6 | The test beacon is accepted only when the pinned cluster is `localnet` (and its sha256 must match the Season's `quicknet_pk_hash`) | "chain info from /h/season, so the test key works" without ever letting a public season steer the browser to a local key | none |
| D7 | No `<meta>` CSP in the pages | the herald sets the CSP header for `/frontier/*` (§8.4, W3-D); a meta CSP would also block the loopback-only `?herald=` dev override. The pages hold no inline script or handler and no third-party origin (asserted) | W3-D |
| D8 | `docs/frontier/DECISIONS.md` edited (one row in part A) | the task asked to record the rustfmt/clippy install there if absent; `docs/frontier/**` is not in W2-E's list | integrator (keep or move the row) |

## 5. Dependency requests (integrator, I-55)

- **R1 — new workspace files** created here to build (adopt or re-create): `frontier-wasm/Cargo.toml` (own `[workspace]`; `[dependencies] permutation-rules = { path = "../permutation-rules", default-features = false }`, `borsh = { version = "=1.6.1", default-features = false, features = ["derive"] }` — the rules crate's own pin; release profile `opt-level = "s"`, `panic = "abort"`, `lto = true`, `codegen-units = 1`, `strip = true`), `frontier-wasm/Cargo.lock` (generated offline from the local cargo cache: borsh 1.6.1, sha2 0.10.9 and their proc-macro deps), `frontier-wasm/rust-toolchain.toml` (`1.95.0`, `profile = "minimal"`, deliberately no `targets`/`components` line). `frontier-wasm/.cargo/config.toml` (`target-dir = "target"`) is in the unit's paths.
- **R2 — root `Cargo.toml`:** add `"frontier-wasm"` to `exclude` as §3.1 lists (harmless; the crate has its own `[workspace]`).
- **R3 — `permutation-gateway/package.json`:** optional: list `"@noble/curves": "1.9.7"` as a direct devDependency. Today it is in the lockfile only as a transitive dependency (of `@solana/web3.js`); `vendor-noble.mjs` refuses any other version, so a lockfile change that moves it is caught, but a direct pin states the intent. `@noble/hashes 1.8.0` is already direct.
- **R4 — Gate W2 line:** consider adding `(cd frontier-wasm && cargo fmt -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --release)` and `node scripts/vendor-noble.mjs --check` to the gate (W1-D's CI job definition `frontier-wasm` already expects a crate).
- **R5 — O-M1-12 (1):** `rustup target add wasm32-unknown-unknown --toolchain 1.95.0`, then `scripts/build-wasm.sh` once, commit `web/frontier/wasm/frontier.wasm{,.sha256}`; `web-frontier-wasm.test.mjs` then runs every recorded call through the real module and gates the 400 KB / 150 KB budget.

## 6. Notes for the next owners

- **W2-D / integrator:** after W2-D's merge the `web-sdk` failure above disappears: W2-D's `sync-web-sdk.mjs` leaves `web/sdk/vendor/` alone and its `web-sdk.test.mjs` checks this unit's `manifest.json` (`files` is an object `{path: sha256}`, `entries` are output paths, `packages` carry version/resolved/integrity; the two LICENSE files are listed like any file).
- **W3-F:** the screens plug into `fstate.registerRenderers`; `fchainio.messageProblems` is the pre-signing check for every shape; `marchbook.revealStep` is the self-reveal timer's decision; `seal.sealMarch` expects the transit's `hostId`/`arriveBell`; `fi18n.errorText(code)` covers every ABI code (your `web-frontier-errors.test.mjs` can assert against `fi18n.ERROR_NAMES`).
- **W4-E:** `app.mjs` boots every page by `data-mode`; practice and spectate are placeholders. `wasm.mjs` `kernel()` loads lazily and refuses another ruleset hash.
- **W3-D (herald):** the client expects §9.2 envelopes with base64 `bytes`, the §9.3 overview binary, `/h/season` with `bytes_b64`, `drand {publicKey, chainHash, period, genesis}`, `latestUnix`, `latestSlot`, and (optional) `seasonAddress`; `/h/me` with `citizen.bytes_b64` and `holdings[].bytes_b64`; `/h/bell/{b}/region/{r}` with `anchor.bytes_b64`. The synthetic fixtures show each shape.

## 7. Integration dry run [measured]

A detached scratch worktree at this unit's commit `87c09b2`, `git merge frontier/m1-W2-D` (W2-D merges before W2-E): clean merge (only `docs/frontier/DECISIONS.md` auto-merged). There, `(cd permutation-gateway && npm ci --ignore-scripts && npm test)`: **438 tests, 437 pass, 0 fail, 1 skipped** (the `frontier.wasm` PENDING-OWNER test); W2-D's noble-manifest test ran against this unit's tree and passed; `node scripts/sync-web-sdk.mjs --check` exit 0. The scratch worktree was removed afterwards (nothing of it is on any branch).

## Links

- Crate: `frontier-wasm/src/{lib,api,path}.rs`, `frontier-wasm/tests/exports.rs`, `frontier-wasm/vectors/wasm-vectors.json`
- Scripts: `scripts/build-wasm.sh`, `scripts/vendor-noble.mjs`
- Page: `permutation-server/web/frontier/` (index/practice/spectate, app, fstate, config, abi, fcodec, faddr, fgeo, herald, fchainio, fsession, clock, seal, seal-worker, marchbook, fi18n, wasm, map/fmap, map/layers, frontier.css)
- Vendored: `permutation-server/web/sdk/vendor/noble/` (+ `manifest.json`)
- Language: `permutation-server/web/lang/{en-frontier.mjs,en-frontier-play.mjs,GLOSSARY.md}`, `permutation-server/web/lang.mjs`, `permutation-server/web/map.mjs`
- Tests: `permutation-gateway/test/web-frontier-{seal,codec,clock,herald,marchbook,session,wasm,chainio,map,shell}.test.mjs`, `permutation-gateway/test/web-lang.test.mjs`, fixtures `permutation-gateway/test/fixtures/frontier/`
