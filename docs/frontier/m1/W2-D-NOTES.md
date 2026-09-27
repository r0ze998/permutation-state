# W2-D relay-sdk: notes

- **Unit:** W2-D (wave 2), branch `frontier/m1-W2-D` cut from `frontier/m1-integ` at `ed31438`; worktree `.claude/worktrees/m1-W2-D`.
- **Contract:** `M1-CONTRACT.md` v1.2, §8.3 (relay and JS SDK), §9.1–§9.4 (web data contracts the SDK serves), §10.1 (fees), §10.3 (ports), §11 (W2-D brief), §12 (Gate W2), §13.6 (E7 web smoke: `web-sdk` freshness incl. `sdk/frontier` and the noble manifest).
- **Owned paths touched:** `permutation-gateway/src/frontier/**`, `permutation-gateway/client/src/frontier/**`, `permutation-gateway/test/frontier-{sdk,relay,relay-parts}.test.mjs`, `permutation-gateway/test/web-sdk.test.mjs`, `permutation-gateway/scripts/sync-web-sdk.mjs`, `permutation-server/web/sdk/frontier/**` (generated), this file. **One file outside the list:** `docs/frontier/DECISIONS.md` (one row, on the workflow's explicit instruction; see deviations).
- **Not touched:** manifests and lockfiles (no new dependency), `permutation-server/web/session.mjs`, `permutation-chain/src`, the main tree, other units' worktrees (W2-E's noble tree was copied into this worktree for one test run and removed again; nothing of it is committed). No push, no chain transaction, no service left running, no port bound except `127.0.0.1:0` in tests.

## What landed

### JS SDK (`permutation-gateway/client/src/frontier/`, synced to `permutation-server/web/sdk/frontier/`)

| Module | Content | Source of truth |
|---|---|---|
| `abi-tags.mjs`, `abi-errors.mjs`, `abi-budgets.mjs`, `abi-layouts.mjs`, `abi-presets.mjs` | **generated** data (deep-frozen) from `frontier-abi/vectors/{tags,errors,budgets,layouts,presets}.json` by `sync-web-sdk.mjs`; nothing hand-copied (§3.3) | frontier-abi (W1-E) |
| `budgets.mjs` | per-kind CU limit, `L(kind)`, tx ceiling (the byte model, §18), the compute-budget prefix `[limit, price, loaded]` (+ heap frame only on request) and its parser | abi-budgets |
| `fees.mjs` | §10.1 formulas as bigints (`cost`, `priorityMilli`, `feeForPriority`, `cuPriceMicro`, `feeOfPrice`, `loadedLimit`, `deployMaxLen`, `minTipLamports`, `tipPriorityMilli`, `defenceRefund` v1.2), `rent`, `tipPresets` `{tip_min, ⌈1.5 tip_min⌉, 2 tip_min}`, `seasonTipMin`, `departEscrow` | kernel `frontier::fees` |
| `codec.mjs` | account decode/encode over the ABI layouts (records, arrays, i16/i64…), `SeasonParams`, instruction data encode/decode (FileTicket's site list included), the flattened account list per instruction, error names/codes, `programError` for RPC errors | abi-layouts, abi-tags, abi-errors |
| `addresses.mjs` | §4.1 grammar: seeds per kind, `withSeed`, Season PDA, ProgramData, `FrontierAddresses`, `citizenTag15`, `keeperTag8`, `citizenTag`, `joinShardOf`, province index (kernel `ProvinceCoord::index`) and inverse, `hostId` / `hostParts`, `holdingOfHost` | kernel `frontier::addr`, `geometry` |
| `seal.mjs` | plaintext pack/unpack/validate (I-28, kernel order), path encode/decode, `saltOf`, `commit`, `ctHash`, `sealRoot`, `bodyXor`, `openBody`, seal split/assemble, `sealParts`, the browser self-audit `auditSeal`, `revealMaterial` (the `/gw/f/reveal` body, I-24) | kernel `frontier::seal` |
| `shapes.mjs` | the sponsored-shape allowlist `classify` (player: 0x30–0x33, 0x40–0x46, 0x50; settle: 0x47, 0x54; any Reveal → `UseRevealRoute`) and the builders `frontierIx` / `shapeIxs` / `shapeMessage` the web uses | abi-tags (`relay_*_shape`), abi-budgets |
| `herald.mjs` | route paths (§8.4), `HeraldClient` over `fetch`, envelope decoder (§9.2), overview binary decoder + encoder (§9.3) | contract §9 |

Size [measured]: `web/sdk/frontier/` is 148,765 B raw, 31,437 B gzip (the five ABI data modules are ≈ 88 KB raw of it); the web page imports only what it uses.

### Relay (`permutation-gateway/src/frontier/`)

| File | Content |
|---|---|
| `config.mjs` | defaults (operator 41030, public 41033, herald peer 127.0.0.1, keeper 41050, pool 150), flag/`FRONTIER_*` parser, **port rule** (41000–41999, never a reserved port; 0 only for tests), **public listener loopback only** (I-51), pool ≥ 150 unless `--dev`, 32-byte secret files (mode 0600) |
| `payers.mjs` | `payer_i = ed25519(sha256("PS-FRONTIER-PAYER-v1" ‖ master ‖ "relay" ‖ le32 i))` (fclient's scheme), `PayerPool`: uniform CSPRNG draw over payers at or above the floor, no round-robin, balances via `getMultipleAccountsInfo` |
| `quota.mjs` | per game day (Clock, not wall clock): 40 txs on days 0–6, 20 after, unused allowance carried to a burst of 60; lamports ≤ 24 Depart escrows at the top preset + rent(Citizen) + rent(Holding); `check` / `charge` / `refund` |
| `invites.mjs` | one-time HMAC invites bound to the season id; `reserve` (one Join in flight per invite) / `consume` / `release`; used nonces persisted |
| `keeperlink.mjs` | body checks for `/f/reveal` (exact sizes, canonical base64, transit slot 0–3) and `/f/nudge`; forwards to the keeper's `/v1/reveal` / `/v1/nudge` with its bearer token; passes the answer through; no answer → 502 `KeeperUnavailable` |
| `chain.mjs` | Season / Clock / Citizen reads (cached), `simulateWatching` (signatures checked, fee payer's post-balance), `frontierRefusal` (program errors named from the SDK's error table, statuses per class) |
| `shapes.mjs` | pool membership and season check, Depart tip presets (`TipNotPreset`), allowances (Join: rent(Citizen); FileTicket: `max(0, rent(1,280) − ticket_escrow)` read from the Citizen; Depart: `tip + march_fee + seal_bond`; others 0), the drain guard, quota keys (player → citizen; settle → requester session or client-address bucket) |
| `app.mjs` | two-listener handler, route tables, `FRONTIER_PUBLIC_ROUTES`, per-address limits (40/2 relay, 10/0.2 join, 40/2 reveal…), loopback clients exempt from address limits (not from quotas), the funds breaker, **`clientAddress`: X-Forwarded-For only from the herald's peer, its last entry** |
| `routes/relay.mjs` | `GET /f/relay`, `POST /f/relay`, `POST /f/join` (open season; gated season with the gate co-signature and an invite) |
| `routes/keeper.mjs` | `POST /f/reveal` (+ a per-holding limit), `POST /f/nudge` |
| `routes/season.mjs` | `GET /f/season` (program, season, pool size, quotas, herald URL, join gate, tip floor and presets, march fee, seal bond), `GET /f/quota?citizen=`, `GET /f/tx/{signature}` (landed / failed with the program error / expired / unknown) |
| `routes/operator.mjs` | `POST /f/operator/invites`, `GET /f/operator/pool` (operator listener + token) |
| `server.mjs` | CLI `node src/frontier/server.mjs --program <id> --season <n> …`; binds 127.0.0.1 only |

`POST /f/relay` order: exact parse → allowlist → pool and season → authority signature → replay key and the signer's bucket → quota check → **hold** one transaction and the kind's allowance → co-sign → balance, simulate with signatures (fee payer's post-balance), balance again → drain guard with the larger pre-balance → keep the charge at the lamports really moved (a refusal anywhere gives everything back) → send (not waiting for confirmation; `GET /f/tx/{sig}` reports it).

### `sync-web-sdk.mjs` and `web-sdk.test.mjs`

- The script now generates `client/src/frontier/abi-*.mjs` from `frontier-abi/vectors/*.json`, copies the Frontier set to `web/sdk/frontier/`, checks it is browser-safe (siblings, or `../` modules of the flat set only; no `node:*`, `Buffer`, `process`), and leaves `web/sdk/vendor/` (W2-E's) alone; the flat sync no longer deletes `frontier/` or `vendor/`. `--check` covers all three.
- `web-sdk.test.mjs` adds: ABI modules fresh against the vectors; `web/sdk/frontier` = the set, each file its source under the header; the import rule; the web copies run without `Buffer` and agree with `client/src/frontier`; **the noble manifest**: every file under `web/sdk/vendor/noble/` listed in `manifest.json` with a matching sha256 and nothing unlisted, entries present, `@noble/curves` 1.9.x and `@noble/hashes` 1.8.0, `.mjs` files importing only relative paths inside the tree (comments masked). The noble test **skips** while the tree is absent (it lands with W2-E, merged after W2-D); run against W2-E's current tree copied in, it passes, and a one-byte change to `hashes/sha2.mjs` fails it.

## Tests and gate lines run [measured, 2026-09-27]

| Command | Result |
|---|---|
| `(cd permutation-gateway && npm ci --ignore-scripts && npm test)` | exit 0: **369 tests, 368 pass, 0 fail, 1 skipped** (the noble manifest test, tree absent on this branch). Base before this unit: 327/327 |
| `(cd permutation-gateway && npm test && node scripts/sync-web-sdk.mjs --check)` | exit 0; "web/sdk is up to date (11 files, frontier/ 12, 5 generated from frontier-abi)" |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| `node --test permutation-state-prototype/civilization/*.test.mjs` | 47 pass (not this unit's files; run as a sanity check) |
| `web-sdk.test.mjs` with W2-E's `web/sdk/vendor/noble` copied in (then removed) | 8/8 pass; tampered file → FAIL with "sha256 differs from the manifest" |

New test files: `frontier-sdk.test.mjs` (17 tests: codec offsets vs fclient's pinned offsets, > 250 compared; 17 kinds, magics, sizes, rent; all 50 instruction samples round-trip; every address of both address vectors incl. the reserved `sv`/`po`; host ids and province indices; seal vectors both ways incl. the Rust→JS tlock seal's body, root and ct_hash; every invalid plaintext; all fee vectors incl. defence refunds; budgets; fclient's signed shapes classify and the builders reproduce their instructions; 15 refusal cases of the allowlist; herald formats), `frontier-relay.test.mjs` (13 tests over a scripted chain that verifies every signature in simulation), `frontier-relay-parts.test.mjs` (7 tests incl. a real socket on 127.0.0.1:0).

Measurements:
- Payer draw χ² over 149 eligible payers × 200 draws, threshold 240 (p ≈ 10⁻⁶ at 148 dof): green; a payer under the floor is never drawn.
- Payer derivation cross-checked with Rust: a scratch binary (`scratchpad/w2d-payer-xcheck`, `solana-keypair =3.1.2` `Keypair::new_from_array`, `sha2 =0.10.9`, built offline with toolchain 1.95.0, not committed) printed the keys for master `[9; 32]`, `relay` 0/7/149 and `reveal` 7; the JS test pins them.
- fclient's shapes classify at their measured sizes: Join 534 B, FileTicket (3 sites) 575 B, Harvest 427 B, Muster 466 B, Depart 712 B, SettleTransit 762 B; each ≤ its `budgets::tx_ceiling` (the §5.5 "Tx B max" column is below the measured size for FileTicket, Harvest and Muster, as W1-F reported; the relay gates on `tx_ceiling`, §18).

Gate W2 items of other units (cargo, svm-tests, `build-frontier.sh`, frontier-node, build-wasm) were **not run** here: they are not this unit's files. `build-wasm` stays `PENDING-OWNER` (O-M1-12) for W2-E.

## Deviations and interpretations (for review)

1. **Quota "burst 60"** (§8.3 D4) read as: unused daily allowance carries over, capped at 60 transactions; a new citizen starts with the day's allowance (40 or 20). The per-signer rate bucket (24, 1/s, as v9's relay) is separate.
2. **Settle-shape requester** (I-51, "its session key, or … its client-address bucket"): the request may carry `requester` (base58) and `requesterSig` (its ed25519 signature of the transaction message); a valid pair charges `session:<requester>`, none charges `addr:<bucket>`, an invalid pair is 400 `BadSignature`.
3. **Join has its own route:** `POST /f/relay` refuses a Join (`RelayRejected`, "use POST /f/join"); `POST /f/join` takes only a Join. In an open season the Join must not name a gate account; in a gated one it must name exactly `Season.join_gate`, and the relay must hold that key (else 503 `GateUnavailable`).
4. **Quota charging:** the relay holds one transaction and the kind's allowance while it checks, refunds all of it on any refusal (so "nothing charged on a failed simulation" holds even with concurrent requests), and keeps the lamports the simulation actually moved beyond the fee.
5. **Drain guard pre-balance** = the larger of `getBalance` before and after the simulation (a top-up landing in between can only cause a refusal, never hide a drain). The fee counted is 5,000 × signatures (CU price is 0 on every sponsored shape).
6. **Refusal statuses** (`chain.mjs FRONTIER_ERROR_STATUS`): state/window errors 409, `Bucket` 429, `Auth`/`NotOwner`/`JoinGate` 403, `SessionExpired` 401, the rest 400; the body carries `code` (the §5.4 name) and `programCode`.
7. **Overview bit order** (§9.3 leaves it implicit): `owners` and `site_state` are little-endian integers, site i at bits `3i..3i+2` and `2i..2i+1`. The herald (W3-D) must write the same; `encodeOverview` is there for its tests.
8. **Settle-shape CU limit** is required to equal the budgets table's limit, like the player shapes (§8.3 names no value for settle shapes).
9. **`GET /f/relay` quota field:** with `?citizen=` that citizen's `{left, resetsAt}`, without it the day's allowance.
10. **`docs/frontier/DECISIONS.md`** is outside W2-D's list; one row was added to part A on the workflow's explicit instruction (the owner's OK for the `rustfmt`/`clippy` components of 1.95.0, installed by the main session). Other wave-2 units may add a similar row: the integrator keeps one.
11. The SDK has no separate `drand.mjs` (web.md §8): the pinned quicknet constants are in `abi-presets.mjs` (`PRESETS.quicknet`: genesis, period, public key, pk hash), generated like the rest.

## Dependency requests

None. The relay uses `@solana/web3.js` (Connection, Keypair, VersionedTransaction) and `node:crypto`, both already in the gateway; the SDK uses only the flat web SDK (`bytes`, `base58`, `sha256`, `solana-tx`). No `package.json` / lockfile change.

## For the integrator and later units

- After any change to `frontier-abi/vectors/*.json` (e.g. W2-A/W2-B regenerating `L(kind)` from the release `.so`, the wave-5 CU limits), run `node permutation-gateway/scripts/sync-web-sdk.mjs`; the gate's `--check` and `web-sdk.test.mjs` fail until then.
- **W2-C (localnet):** the drain guard needs `simulateTransaction` with `accounts: {addresses: [fee payer], encoding: 'base64'}` to return the post-state lamports (§8.7 lists "accounts post-state"); a missing value is 502 `SimulationIncomplete`, never an acceptance. The relay also reads the Clock sysvar with `getAccountInfo` (fallback `getSlot` + `getBlockTime`).
- **W2-F (fclient):** please add a `payers` section (pool id, index, pubkey) to `frontier-vectors.json`; `frontier-relay-parts.test.mjs` pins four keys computed with solana-keypair meanwhile.
- **W2-E (web):** import `sdk/frontier/{shapes,budgets,addresses,codec,seal,herald}.mjs`; `shapeMessage` builds exactly what the relay accepts (fee payer and blockhash from `GET /gw/f/relay`); Depart offers `tipPresets(seasonTipMin(season))` only; the reveal body is `revealMaterial(...)`.
- **W3-D (herald):** the `/gw/*` proxy must connect from the address the relay is started with as `--herald-peer` (default 127.0.0.1) and append the client to `X-Forwarded-For`; `GET /f/season.heraldUrl` is `--herald`.
- **W4/W5 (stack):** ports 41030 / 41033 per §10.3; `server.mjs` refuses reserved or out-of-range ports and a non-loopback public host.

## Pending

- Live run of the relay against `frontier-localnet` with the real program: needs W2-A's program and W2-C's localnet on the integration branch (first possible in the wave-2 integration window or wave 3). Not run here.
- The noble manifest test runs for real once W2-E's `web/sdk/vendor/noble/` is merged (it skips until then).
- No O-M1-12 item is needed by this unit.

## Post-merge addendum (integrator, integ-W2 window, 2026-09-28)

After the wave-2 review (contract v1.3 §8.3): `classify` requires the message's account keys to be exactly the fee payer, the instruction's accounts and the two program ids, with the ABI's writability; settle shapes are charged to a requester's on-chain-verified Citizen (`POST /f/relay {…, citizen}`), else the client-address bucket — no `session:<key>` buckets any more; the replay key is claimed before any await; a send that throws charges nothing; quota pruning keeps idle keys at the burst; simulation failures are attributed to the failing program. **Size figure corrected:** 31,437 B is `gzip -9` of the twelve files concatenated; gzipped one by one they total ≈ 36.2 KB (zlib default) / 36.1 KB (-9); raw 148,765 B at the time of the review (the v1.3 regeneration changes the ABI data modules slightly). The pooled daily lamport cap is kept as an accepted M1 residual (DECISIONS I9).
