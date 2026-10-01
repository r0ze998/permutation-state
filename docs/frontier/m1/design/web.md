# M1 "First Bell": web client design

- **Area:** web (browser client for Wylls, M1, Solana base only, no money)
- **Date:** 2026-09-27. Planning phase: nothing in the repo was changed, no commit, no server started, no chain transaction.
- **Normative inputs:** `docs/frontier/DESIGN.md` rev 3.1 (§0–§13, §19), `SUMMARY.ja.md`, `m0/M0-FINAL.md`, SP-V2 / SP-FEE results, the S-TLOCK lab (`m0/spikes/S-TLOCK/js`, tlock-js 0.9 on @noble/curves 1.9.7), SP-V2 `program/src/seal.rs` (the compact-16 opener).
- **Code read:** `codex/frontier` at `d95fa25`, worktree `.claude/worktrees/frontier-integ`: `permutation-server/web/**` (plain ES modules, no build step; 708 KB raw / 225 KB gzip for the play page's files [measured]), `permutation-gateway/test/web-*.test.mjs` (12 files, 2.6k lines, node test runner with a stub DOM), `permutation-gateway/scripts/sync-web-sdk.mjs`, `src/routes/relay.mjs`, `src/cosign.mjs`, `permutation-rules/src/frontier/{clash,travel,terrain,holding,host,geometry,stance}.rs`.
- **Tags:** as in the design. [measured] = measured in the lab or in code; [estimate] = to be measured in M1; [design] = a choice this document makes.

---

## 0. Summary

The M1 web client is a **new page set, `permutation-server/web/frontier/`**, that stays a static, no-build ES-module client like today's, and reuses the parts of the v9 client that are not about v9's rules: the language system (`lang.mjs`, the JA/EN toggle, the completeness scan), `util.mjs`, the Wallet Standard flow and dev wallet (`wallet.mjs`), the session-key primitives in `session.mjs` (but **not** `sessionText`, which stays untouched), the generated browser SDK (`sdk/`: bytes, base58, sha256, borsh, legacy transaction compiler), the gateway request and pin-check pattern (`chainio.mjs`), the pure-state "book" pattern (`sealbook.mjs`), the render scheduler pattern (`state.mjs`) and the hex painter primitives in `map.mjs`.

What is new:

1. **A WebAssembly build of the rules-v10 kernels** (`frontier-wasm`, raw `extern "C"` exports, no wasm-bindgen glue) so the browser uses the same code as the program for terrain, paths and arrival bells, the seal plaintext and commitment, the bell/round arithmetic, and the clash kernel (practice mode and "verify this clash").
2. **The tlock seal made in the browser**, in a Web Worker, with vendored and pinned `@noble/curves`/`@noble/hashes`; the client self-audits every seal (the same FO relation `ProveBadSeal` checks) before it signs Depart, and checks the season's drand key against the pinned quicknet key. Sealing needs **no network access to drand**.
3. **A read path through the herald cache** (snapshot + sequence-numbered diffs, CDN files per province per bell), with raw account bytes decoded by the client's own codec, and on-screen-only polling with jitter so thousands of viewers do not stampede at the bell.
4. **A write path through the relay** with exact transaction shapes per instruction, a session key for play, the wallet only for Join, and **Reveal sent with no player signature at all** (fee payer drawn by the relay from its rotating pool), so a self-reveal exposes no predictable account.
5. **Screens:** shell with bell chip, ring/province map with LOD and presentation-only fog, holding panel (harvest/build/train/muster), host panel and Explore, march composer (sealed destination, path, arrival bell, stance, retreat ratio, tip), march tracker, bell pipeline sheet, clash report with in-browser verification, incoming-arrival warnings, guided first bells, an off-chain practice mode, a spectator view. Phone-first (360 CSS px), with a list alternative for every map action.
6. **Tests:** new `web-frontier-*.test.mjs` files in the existing node runner (seal vectors shared with the Rust opener, codec vectors from the program crate, bell clock, march limits and transaction sizes, relay shapes, herald client, marchbook, i18n completeness, onboarding state machine, practice digest equality) plus a **separate screenshot smoke suite** (Playwright + axe, fixture server on port 0) over 10 screens × JA/EN × 3 viewports.

**Two findings for the design owner** (details in §6.3 and §15): the first holding appears **11–21 minutes** after a site ticket and the first clash report **31–41 minutes** after a departure, not "minute 10+" as §2.3 says; and the pinned 37-byte plaintext has `retreat_bps u16` while the kernel takes `Option<Bps>`, so "never retreat" has no pinned encoding yet.

---

## 1. Scope

### 1.1 In M1

| Area (design §12 M1) | Web work |
|---|---|
| Join (free), Citizen, site tickets with displacement, vigil | Faction picker (members, doctrine, free land in wedge, no prize numbers in M1), site picker with up to 3 choices and pair tickets, provisional/final state, vigil hours |
| Holding, Harvest/Build/Train, Muster, dormancy, shield | Holding panel, 4-item queue, training, muster into hosts, pending-after-clash labels, shield and frontier-protection timers, dormancy warnings |
| Province, rings, OpenRing/OpenProvince | Ring/province map with LOD, wedges, crowding meter per wedge, ring-open events |
| Depart/Reveal with mandatory seal and tips; ArrivalSlots | March composer, browser seal, tip and retreat ratio, marchbook, self-reveal at the arrival bell's start, tracker through keeper reveal, slot admission or displacement, rout |
| GatherClash/ResolveFromInputs, ProveBadSeal/SealVerdict, SettleTransit | Clash reports, verify in browser, settle button, verdict display (never expected for this client's own seals) |
| Explore | Scout exploration, floor-reward counter for the first three |
| Beacons (PostAnchor, SeedCache), ArchiveAnchors | Bell pipeline per region (anchored, reveal open/closed, seed, resolved), archived bells read from the archive |
| Herald fold + cached read path | The client side of the herald contract (§4) |
| Onboarding, practice without Skirmish | Guided first bells, practice mode on the WASM kernel |
| Bots for a 1,000-bot local season | Spectator view; fixtures recorded from that season for the tests |

### 1.2 Not in M1

Money (fees, stakes, claims, payout tables: M2), governance, Mandates, Companies, delegation, sieges, captures, caravans, Bourse, accusations (M3). Postures (CommitPosture/RevealPosture) are listed under M1 accounts in §12 but under M3 play; the composer is designed (§7.7) and ships behind a flag that the program's M1 instruction set turns on (open question Q3). The v9 client (`index.html`, `spectate.html`) is not changed except for additive exports, and its tests keep passing.

---

## 2. What is reused from `permutation-server/web`

| Module | Fate in the Frontier client | Why |
|---|---|---|
| `lang.mjs` (L, Lh, t, `data-i18n`, `mountLangToggle`, `ps-lang` storage), `lang/helpers.mjs`, `lang/GLOSSARY.md` | **Reused as is.** One new dictionary `lang/en-frontier.mjs` added to `EN_GROUPS`; one new glossary section (§9) | The switch, the saved choice (same key `ps-lang` on both pages) and the completeness scan work unchanged: the scan already walks every directory under `web/` except `sdk/` and `lang/`, so `web/frontier/` is covered automatically |
| `i18n.mjs` | Reused for `CIV_NAMES`, `CIV_COLORS`, `twin`, number and date formatting; Frontier enum tables (resources, tiers, units, stances, fates, program errors) go in a new `frontier/fi18n.mjs` with the same `twin` pattern | Keep v9 tables untouched |
| `util.mjs` (`html`, `esc`, `setHtml`, `toast`, `singleFlight`, `fmt`, `short`, `logOnce`) | Reused as is | Pure and tested |
| `wallet.mjs` (Wallet Standard discovery, connect, `signMessage`, `signTransaction`, dev wallet for localnet) | Reused as is | Only Join is wallet-signed (§5.2) |
| `session.mjs` | **Primitives reused** (`keyFromSeed`, `publicKeyOf`, `verify`, `preflight`, storage helpers, `parseBackup`). **`sessionText` is not edited and not used**: the Frontier has its own text and seed domain in `frontier/fsession.mjs` (§5.3) | The v9 text talks about "your nation's treasury", which is wrong for the Frontier, and it must never change |
| `sdk/*` (generated from `permutation-gateway/client/src` by `sync-web-sdk.mjs`) | `bytes`, `base58`, `sha256`, `borsh`, `solana-tx` reused; new generated files under `sdk/frontier/` (codec, budgets, relay shapes, seal envelope) and a vendored `sdk/vendor/noble/` tree | Same "edit `client/src`, sync, test checks freshness" discipline for bots, keeper and browser |
| `chainio.mjs` | **Pattern reused**, new `frontier/fchainio.mjs`: gateway base URL, `request()` returning `{ok, httpStatus, code, error}`, pins (program id, cluster, season id) set from the herald's season record and re-derived locally, message checked byte for byte before and after any signature | The v9 routes (x402 Register, ER relay, claims) do not apply in M1 |
| `sealbook.mjs` | **Pattern reused** for `frontier/marchbook.mjs`: pure functions over an injected storage, written before anything is sent | Same failure model: a lost answer must not lose a sealed order |
| `state.mjs` (renderer registry, `invalidate`, `flush`) | Pattern copied into `frontier/fstate.mjs` (the v9 `S` object is v9-shaped) | Keep the proven render scheduler, avoid coupling |
| `map.mjs` (`WorldMap`: pointer/zoom/pan input, projection with `FLATTEN`, `hexPoints`, `inverseHex`, terrain palettes, `polygon`, `rounded`, border edges, minimap) | Painter primitives reused through **additive exports** from `map.mjs` (no behaviour change; v9 tests unchanged); a new `frontier/map/fmap.mjs` class adds levels of detail, rings, wedges, sites, fog and march overlays, and touch gestures | The v9 class draws a whole small world every frame; the Frontier needs province-level LOD and culling (§7.3) |
| `drawers/index.mjs` (one panel at a time), notifications, loading screen, `dialog` help | Pattern reused; on phones every drawer becomes a bottom sheet (§10) | |
| `verify.mjs`, `spectate.*` | Pattern reused: in-browser re-verification and a read-only spectator page | |
| `orders.mjs`, `hud/dock.mjs`, `chainplay.mjs`, `lobby.mjs`, `claim.mjs`, `drawers/*` (nation, era, research, diplomacy, market, decisions), `sdk/{decision,batch,offices,player,talk}` | **Not used** | v9 tick orders, offices and treasury do not exist in the Frontier; claims are M2 |
| `base.css`, `play.css` | Colour tokens and type scale reused; layout not reused | Today's phone layout hides the lens bar and map tools and uses 32-px buttons and 11–12.5-px text; the Frontier is phone-first (§10) |

Additive exports needed from `map.mjs`: `project`, `hexPoints`, `inverseHex`, `COLORS` (terrain palette), `polygon`, `rounded`, `shade`, `EDGE_NEIGHBOR`. A later cleanup can move them to `web/hexdraw.mjs`; M1 does not need that move.

---

## 3. Architecture

### 3.1 Layout

```
permutation-server/web/
  frontier/
    index.html          the game page (/frontier/)
    spectate.html       read-only spectator (/frontier/spectate.html)
    practice.html       practice mode, usable before any wallet (/frontier/practice.html)
    app.mjs             boot: config → herald season → wallet/session → screens
    fstate.mjs          store FS + render scheduler (registerRenderers/invalidate/flush)
    config.mjs          herald and relay base URLs (same origin by default; ?herald= for dev)
    herald.mjs          read path: snapshots, diffs, cache, poll scheduler (§4)
    fchainio.mjs        write path: relay, pins, tx building and checks (§5)
    fsession.mjs        Frontier session text and key (§5.3)
    clock.mjs           bell clock and pipeline state per region (§6)
    wasm.mjs            loader for frontier.wasm (§3.3)
    seal.mjs            main-thread API; seal-worker.mjs does the pairing work (§8)
    seal-worker.mjs
    marchbook.mjs       local record of sealed marches and reveal attempts (pure)
    onboarding.mjs      guided first bells (pure state machine + panel)
    fi18n.mjs           Frontier enum tables (twin JA/EN), error texts
    map/fmap.mjs        FrontierMap: LOD, culling, fog, overlays, touch
    map/layers.mjs      painters: rings, wedges, sites, hosts, marches, clashes
    screens/{shell,join,holding,host,march,tracker,incoming,bell,report,explore,chronicle,practice}.mjs
    frontier.css
    wasm/frontier.wasm  built artifact, checked in with its sha256 (§3.3)
  lang/en-frontier.mjs
  sdk/frontier/*.mjs    generated from permutation-gateway/client/src/frontier/
  sdk/vendor/noble/**   vendored @noble/curves 1.9.x + @noble/hashes 1.8.0 ESM, specifiers rewritten
```

The page is fully static. In the local season and the playtest it is served by the herald's HTTP listener under `/frontier/` (same origin as `/h/*` reads and the `/gw/*` relay proxy), so there is no CORS and the CSP can be `connect-src 'self'` (§12).

### 3.2 Data flow

```
           herald (CDN files + WS diffs)                    relay (/gw/f/relay)
                 │ raw account bytes + slot + event head         ▲ signed wire tx
                 ▼                                               │
  herald.mjs ──► codec (sdk/frontier) ──► FS store ──► screens ──┤
                                           ▲    │                │
  clock.mjs ◄── anchors, seed caches ──────┘    ▼                │
  wasm.mjs (kernel: terrain, paths, bells, plaintext, clash) ◄───┤
  seal-worker.mjs (IBE on quicknet pk) ──► marchbook (localStorage) ──► fchainio.mjs
```

- The client **decodes raw account bytes itself** with the generated codec; the herald's JSON is a convenience the client never uses for anything it signs or verifies.
- Every transaction is built from the client's own decoded state, compiled with `sdk/solana-tx.mjs`, and checked again after the relay answers (`walletMessageProblem`-style checks).

### 3.3 The WASM kernel (`frontier-wasm`)

A new crate outside the root workspace (like `frontier-sim`), `crate-type = ["cdylib"]`, depending on `permutation-rules` without `std`. It exports plain C-ABI functions over a linear-memory byte buffer (borsh in, borsh out), plus `alloc`/`free`. No wasm-bindgen, so there is no generated JS glue and the loader (`wasm.mjs`, ~80 lines) stays reviewable.

| Export | Kernel | Used by |
|---|---|---|
| `ruleset_hash()` | `frontier::clash::frontier_ruleset` digest | refuse to run practice/verify when it differs from the season's ruleset hash |
| `province_of(q, r)`, `province_centre`, `ring_of`, `wedge_of`, `region_of` | `geometry` | map, region lookup for the bell pipeline |
| `generate_province(ring_seed, P, Q)` → 61 tiles, 12 sites, passability | `terrain` | tile LOD without shipping tiles from the herald |
| `plan_path(start, dest, unit, blocked)` / `path_cost(start, steps, unit)` | `travel` (client-side planner; the chain only verifies) | composer |
| `earliest_arrival_bell`, `check_arrival_bell`, `bell_at`, `bell_start` | `travel` | composer, clock |
| `seal_round(genesis_ts, bell)` (T(b), first quicknet round at or after the end of bell b), `seed_round(A)` (first round at or after A + W + Δ) | seal-margin functions | clock, seal |
| `plaintext_pack(fields)` / `plaintext_unpack(37 B)`, `commit(pt, salt)`, `salt_of(k)`, `body_xor(k, pt)` | `frontier::seal` (to be added in M1 by the rules area) | seal, reveal |
| `resolve_clash(ClashInput)`, `resolve_from_inputs(ClashInputs bytes, seed)` → outcome + digest | `clash` | practice mode, "verify this clash" |
| `reachable(origin, depart_bell, target_bell, unit)` | `travel` | incoming-arrival warnings (§7.7) |

- **Build:** `scripts/build-wasm.sh` (pinned toolchain 1.95.0, `--remap-path-prefix`, `wasm32-unknown-unknown`, release, `opt-level = "s"`, `panic = "abort"`), output copied to `web/frontier/wasm/frontier.wasm` with `frontier.wasm.sha256`. A test rebuilds and compares, the same freshness rule as `sync-web-sdk.mjs --check`; the job needs the wasm target, so it runs in its own CI step.
- **Budget [estimate, to measure in M1 week 1]:** ≤ 400 KB uncompressed, ≤ 150 KB gzip; worst-case clash in the browser ≤ 50 ms on a mid-range phone (the kernel is ~300–500k SBF CU; native code is far faster than SBF, but no browser number exists yet). The wasm32 target is not installed in this environment, so nothing was measured here.
- **Loading:** lazily, on first need (tile LOD, composer, practice, verify). The province-level map and every panel work without it.

---

## 4. Reading: the herald contract the client consumes

The herald is another M1 area; this section is what the web needs from it, to be reconciled with the herald design (open question Q1). All paths are relative to the page's origin.

### 4.1 Endpoints

| Path | Content | Caching |
|---|---|---|
| `GET /h/season` | Season record (raw bytes + decoded): program id, cluster, season id, `genesis_ts`, bell length, reveal window W, Δ, drand chain hash, **quicknet public key**, period and genesis time, ruleset hash, R_MAX, open rings with their ring seeds, default tip priority and the Reveal CU limit, march fee, seal bond, sponsorship quotas; herald `head_seq`, latest slot and its unix time | `max-age=30`, ETag |
| `GET /h/overview/{ring}/{bell}.bin` | One compact record per province of ring d: owner faction per site (12 × 3 bits), host count per faction, clash flag, resolved-through bell, dormant/free flags | per (ring, bell): immutable once the bell is resolved everywhere in the ring; the "latest" alias `…/latest.bin` has `max-age=5` |
| `GET /h/province/{P},{Q}/{bell}` | The Province account bytes (sites, owners, garrison mirror, walls, residents incl. *departed* entries, result ring, reward indices) with `slot` and `event_head`; the ArrivalSlots and ClashInputs of that bell when they existed (from logs after close) | immutable once resolved; `latest` alias short TTL |
| `GET /h/me/{wallet}` | Citizen, Holdings (≤ 3), their hosts and transit records, open ArrivalSlots of this citizen, SealVerdicts, sponsorship quota left | `no-store`; pushed over WS |
| `GET /h/bell/{bell}/region/{r}` | BellAnchor (A, signature), seed round S, SeedCache nonce(s) and seed, tombstone state, per-province resolved flags for the region, archive record after 48 h | immutable once seeded |
| `GET /h/events?after={seq}` | Chronicle and PS2 log lines (founding, ring open, clash summary, reveal, rout, verdict, settle) | paged |
| `GET /h/clash/{P},{Q}/{bell}` | Clash report: ClashInputs bytes, seed, outcome digest stored in the Province, decoded fighters | immutable |
| `WS /h/ws` | Subscribe `{provinces: [...], rings: [...], wallet, bells: true}`; receives `{seq, kind, key, slot, bytes}` diffs; `seq` gaps trigger a resync from the snapshot | — |

### 4.2 Client behaviour for many viewers

- **Only what is on screen:** the overview of the rings in the viewport, full province records only at tile LOD for provinces intersecting the viewport (≤ 12 at phone size), plus the viewer's own entities and the provinces where the viewer has arrivals, departures or holdings.
- **Bell-boundary stampede control:** after a "bell b resolved in ring d" notice, each client waits a random 0–15 s before refetching immutable files (they are CDN hits anyway), and prefers WS diffs for its own entities.
- **Polling without WS:** own entities every 10 s while a march is in flight or a transaction is pending, else 30 s; overview once per bell; backoff ×2 to 5 min on errors; `document.hidden` pauses everything except the marchbook's self-reveal timer.
- **Request model per viewer [design]:** ≈ 1 overview file per open ring in view per bell + ≤ 12 province files per bell + own entities; for 10,000 viewers that is ≈ 10,000 × 13 / 600 s ≈ 220 requests/s, almost all immutable CDN hits. This number is an input to the herald's capacity test.
- **Measured (integ-W6r, `screens/live/spectator-polls.live.mjs`: the spectator page on the fixture server through a real day at the 30-s poll) [measured]:** 5,765 requests per viewer-day = **40 per bell**: `/h/season` and `/h/events` 20 each per bell, 3 overview reads in the day — 3× the ≈ 13 of this model, which counts immutable files only. Either the herald capacity input is 40 per bell per viewer (5,000 viewers ≈ 330 requests/s, two small live files) or the live poll drops to once per bell with WS diffs for the rest; open for the triage pass (DECISIONS part R).
- **Staleness shown:** every panel says "as of slot X (N s ago)"; if the herald's latest slot is more than 60 s behind the chain clock estimate, a banner says so and actions that need fresh state (Muster, Depart) ask to reload first.
- **LRU:** at most 256 province records and 64 generated terrain tiles-sets in memory; nothing chain-derived is persisted except the marchbook and the language choice.

### 4.3 Trust and light verification (design §9.2)

- The herald is never a trust root. Each record carries raw bytes, slot and `event_head`.
- **"Check with the chain"** (per panel, optional): the client fetches the same account from a user-chosen RPC URL (never a default third-party URL, never with personal data in the query) and compares bytes; for the viewer's own Citizen and Holdings it also walks the event-head chain from the logs the herald serves.
- **"Verify this clash"**: the WASM kernel re-runs `resolve_from_inputs(ClashInputs, seed)`; the digest must equal the one the Province recorded; the seed is checked to come from THE anchor's region-bell (address recomputed, A read, S = first round ≥ A + W + Δ); the BLS check of the seed signature is offered as a heavier optional step (noble `verify`, [estimate] ≈ 50–150 ms).
- **Seals:** for each of the viewer's marches, the client checks that the Depart log carries exactly the commitment and seal it made (`seal_root` recomputed) — a relay or herald that swapped them is caught.

---

## 5. Writing: relay, session key, transaction shapes

### 5.1 Relay contract consumed

The Frontier relay is a new route set next to v9's ER relay (gateway area; open question Q2). The client needs:

| Route | Request | Answer |
|---|---|---|
| `GET /gw/f/relay` | — | `{feePayer, blockhash, lastValidBlockHeight, programId, quota: {left, resetsAt}}`; `feePayer` drawn at random per request from the relay's pool of **≥ 150** keys (design §6.4) |
| `POST /gw/f/relay` | `{tx}` (base64 legacy wire, signed by every signer except the fee payer) | `{ok, signature}` or `{ok:false, code}`; the relay parses exactly, checks the shape (§5.4), verifies signatures, co-signs, simulates with signatures checked, then sends (same model as `cosign.mjs`) |
| `POST /gw/f/nudge` | `{province, bell}` | asks the keeper to gather and resolve a province that blocks a resident action (the client never builds keeper transactions) |
| `GET /gw/f/tx/{signature}` | — | landed / failed with program error / expired |

Sponsorship: relay-fronted fees (D4: 40 transactions a day for 7 days, then 20). The client shows the quota, the on-chain action bucket (30/h, burst 60) and refuses locally before either runs out.

### 5.2 Who signs what

| Instruction | Signers besides the relay's fee payer | Accounts written (design §8.3) | Notes |
|---|---|---|---|
| Join(faction, session pubkey) | **wallet** | Citizen (init), JoinShard; reads Frontier | the only wallet signature in M1; registers the session key in the Citizen |
| FileTicket(≤ 3 sites, pair?) | session | Citizen | |
| SetVigil | session | Holding, Citizen | effective at a UTC midnight ≥ 24 h later |
| Harvest / Build / Queue / Train | session | Holding, Citizen | harvest is implicit in any action |
| Muster / Dissolve | session | Holding, Province, Citizen | pending until after the bell's clash |
| Explore(host, hexes) | session | Province, Holding, Citizen | |
| Depart(host, commit, seal, arrive_bell, tip) | session | Holding, origin Province, Citizen | ≈ 699 B [estimate]; the test asserts ≤ 1,232 B with the compute-budget instructions |
| **Reveal(plaintext 37, salt 32, ct_hash 32)** | **none** | ArrivalSlot only | sent by the owner's browser from the arrival bell's start; the only signer is the relay's rotating fee payer, so no account the attacker can list in advance is involved |
| SettleTransit | none | Holding, ArrivalSlot (close), origin Province | offered as a button after the reveal close + 1 bell; keepers usually do it |
| CommitPosture (flagged, §7.7) | session | PosturePDA, Citizen | |

Every transaction carries `SetComputeUnitLimit` from a budgets table generated from the program crate's constants (`sdk/frontier/budgets.mjs`); Reveal also carries `SetLoadedAccountsDataSizeLimit` 64 KiB (keeper SDK rule). Player actions send priority 0.

### 5.3 The Frontier session key

- `fsession.mjs` builds its own printable-ASCII text, e.g. `Wylls wants you to create an in-game key.\nSite: …\nCluster: …\nProgram: …\nSeason: …\nAnyone holding this key can act as you in this season (build, train, march, explore). It cannot move tokens from your wallet.\nSign only on <host>.`, and seed domain `PS/frontier-session/v1`, stored under `ps-fsession:<cluster>:<program>:<season>:<wallet>`.
- The text is part of what a wallet signs, so it is **never translated** (glossary "Never translate" list gets a line for it) and is pinned by a test like `web-session.test.mjs` pins v9's.
- Backup and import reuse `parseBackup`; the key is checked against the Citizen's registered session key from the herald.

### 5.4 Checks the client makes before and after signing

- The message contains exactly the expected program instructions, in order, with the expected accounts (recomputed PDAs and with-seed addresses; no bump from anyone), the fee payer the relay announced, and a recent blockhash.
- After the relay answers, the landed signature's transaction is fetched through the herald and compared.
- Program errors map to text through a generated error table; `web-frontier-errors.test.mjs` checks every program error code has JA and EN text.

---

## 6. Time: the bell clock

### 6.1 Sources

- `genesis_ts`, bell length (600 s), W and Δ come from the Season record; `bell(now) = floor((now − genesis_ts) / 600)` via the WASM kernel.
- "Now" is the chain clock estimate: the herald's latest slot time plus local elapsed time; the local clock offset is shown in the bell sheet when it exceeds 2 s.
- Reveal windows are **anchored, not timed**: a window's close is `A(b, r) + W` only once THE anchor exists. Before that the UI says "waiting for the beacon to be anchored" and shows no countdown.

### 6.2 States of a bell, per region, as the UI names them

| State | Condition (from herald data) | Player-facing meaning |
|---|---|---|
| Open | now < bell_start(b+1) | "Arrivals for this bell are sealed; the defenders' roster is frozen since the bell began" |
| Awaiting beacon | now ≥ T(b), no anchor | "Seals open when the beacon is anchored" |
| Revealing | anchor A exists, now < A + W | countdown to A + W; "keepers are revealing" |
| Awaiting seed | now ≥ A + W, no SeedCache | countdown to the seed round time (A + W + Δ, rounded up to a quicknet round) |
| Resolving | seed cached, province not resolved | "clashes are being resolved" |
| Resolved | Province resolved through b | report available |
| Tombstoned / archived | archive record | read from the AnchorArchive |

### 6.3 Latency the onboarding must plan for (finding)

From the rules above [derived from design §2.1, §6.2, §8.5]:

- A **site ticket** filed during bell b is settled by S(b) ≈ bell_start(b) + 600 + ~2 + 600 + 60 s ≈ **bell_start(b) + 21 min**, so the first holding appears **11–21 min** after filing (plus the resolve and settle transactions).
- A **march** departing in bell d arrives no earlier than bell d + 2; its clash resolves after S(d + 2) ≈ bell_start(d) + 20 min + 21 min, so the **first clash report comes 31–41 min after departure**. An owner's in-bell reveal does not shorten this (the close is anchored).
- Explorations roll from the bell seed, so their results also come ≈ 11–21 min later.

Design §2.3's "minute 10+" for the first combat report is therefore not reachable. The onboarding (§7.11) fills these waits with the practice mode and the map tour, and every waiting state shows its expected time. The design owner should restate §2.3 (open question Q4).

### 6.4 The bell chip and sheet

- **Chip (always visible, top bar):** "鐘 1,034 · 残り 6:12" / "Bell 1,034 · 6:12 left", plus a dot when any of the viewer's marches changes state. `aria-live="polite"` announcements only on state changes of the viewer's own marches, never every second.
- **Sheet:** the current bell; the next bell in which the viewer has an arrival, departure or holding clash; for each, the pipeline states of §6.2 for the relevant region, with times; links to the anchor, seed cache and report; an explanation line per state (progressive disclosure: collapsed by default).

---

## 7. Screens

All screens exist in JA and EN, at 360–1440 CSS px, and have a keyboard and screen-reader path. Wording uses the glossary of §9.

### 7.1 Shell

- Desktop: top bar (brand, faction chip, bell chip, quota chip, language toggle, help), map full-bleed, right column for the selected thing (holding, host, province, march), left rail for lists (My holdings, My hosts, Marches, Chronicle).
- Phone (< 760 px): top bar with faction chip, bell chip, language toggle; **bottom tab bar** (Map, Holding, Hosts, Marches, More); panels open as bottom sheets with three heights (peek, half, full), draggable and closable with Escape/back.
- Progressive disclosure (design §2.3): a newcomer sees Holding, Host, Bell, March, Explore; later items (vigil changes, SettleTransit, verification, spectator links) appear after the first clash report.

### 7.2 Join, faction and site

1. **Preflight** (reuse `session.preflight`): secure context and Ed25519.
2. **Connect wallet** (reuse `wallet.mjs`; dev wallet only when the season says localnet).
3. **Faction picker:** for each faction: name, colour and sigil, doctrine card (unit variant, stance or movement edge, civic power — M1 shows only mechanics that exist in M1 and marks the rest "later"), members, free sites in its wedge, Frontier Grant badge when below the average size. **No prize numbers in M1** (no money); the M2 join page adds them.
4. **Sign the Frontier session text** (§5.3); offer the key backup.
5. **Join** (wallet-signed, relay-paid).
6. **Site ticket:** the map opens on the faction's wedge; free sites highlighted; choose up to 3 in order, optionally a friend for a pair ticket (by wallet or invite link); send FileTicket; the ticket card shows "drawn at bell b, result ≈ hh:mm" (§6.3) and the practice-mode invitation while waiting.
7. **Result:** settled (provisional until the end of bell b + 1, the challenge window), displaced (back to step 6 with the reason), or not placed (retry next bell).
8. **Vigil hours:** default 00:00–08:00 in the browser's time zone, shown in local time and UTC; the change rule (effective at a UTC midnight ≥ 24 h later, once a week) is stated before sending.

### 7.3 Map: rings, provinces, fog

**Levels of detail** (by zoom, with hysteresis):

| LOD | Drawn | Data |
|---|---|---|
| World (zoomed out) | Concord, rings, six wedges, one filled hexagon per province coloured by the majority owner of its sites, clash markers, ring-open glow, crowding meter per wedge (θ 55% / 65%) | overview files |
| Province | 12 sites with owner colour and tier glyph, host counts per faction, barbarian camps, Free Cities, departures (origin + arrival bell), own marches | overview + province records for the viewport |
| Tile | 61 terrain tiles per province generated in WASM from the ring seed, site art, hosts per hex (≤ 6), paths | + terrain (WASM) |

- **Culling and caching:** only provinces intersecting the viewport are drawn; each province's tile art is rendered once per zoom band into an `OffscreenCanvas`/`ImageBitmap` and reused; frames render only when dirty (the v9 class's `dirty` flag pattern); device-pixel ratio capped at 2.
- **Fog is presentation only** (design §3.3; all accounts are public). Levels: *unopened* (ring not open: blank), *distant* (province LOD only, muted), *known* (tile LOD: provinces with the viewer's holdings, hosts, past arrivals or explored hexes), *sight* (within 2 provinces of the viewer's holdings and hosts: live host positions highlighted). A setting "show everything (every account is public)" removes the fog, and the help text says fog hides nothing that the chain does not already show. What is really hidden is stated explicitly: sealed destinations and postures until their bell, discoveries until the seed, Shade identities until the end.
- **Overlays:** own marches (secret destination drawn dashed and labelled "only you can see this"), other factions' departures as origin + arrival bell only, clashes of the last bell, incoming-risk halo around the viewer's holdings (§7.7).
- **Input:** tap/click select, drag pan, wheel and pinch zoom, two-finger pan, long-press context menu; keyboard: arrows pan, +/− zoom, Tab cycles selectable features in the viewport, Enter selects, H home.
- **List alternative:** "Nearby" list (provinces within 2 rings of the viewer, with sites, owners, hosts) and search by province coordinates; every action reachable from the canvas is reachable from a list.
- Ring growth, `OpenRing`/`OpenProvince` and the new ring's terrain appear as chronicle events and a one-time glow.

### 7.4 Holding panel

- Header: name (site coordinates), tier (Hamlet → Town → City → Stronghold), order (first/second/third), shield and frontier-protection timers, vigil hours, dormancy state ("act within 18 h to stay active"; Dormant; first holding released after 10 days, with the refugee kit explained).
- Resources: the 8 resources with value, rate, cap and time-to-cap, settled lazily in the client with the same `Accrual` arithmetic (WASM) so numbers tick smoothly between reads.
- Queue (4 items): build and train with cost, duration, the quadratic duplicate cost shown before confirming; completed items show "counts from bell b + 1" where the roster-freeze rule applies (walls).
- Garrison and troops: trained troops, garrison, **Muster** (≥ 100 troops, unit type, "joins the roster from bell b + 1"), Dissolve.
- Transit slots (4): marches in flight with state (§7.6).
- Every action shows the reason when it cannot run, from the kernel's error (e.g. province not resolved through b − 2: "waiting for this province to catch up to bell n" with a **Catch up** button that calls `/gw/f/nudge`).

### 7.5 Host panel and Explore

- Host: unit type, troops, stamina (cap 120, lazy, with time to full), cooldown, position, pending change, supply warning (> 3 provinces from a friendly holding: −1% per bell).
- **Explore** (Scouts): pick up to two hexes adjacent to the host (map or list), send; results at the seed (§6.3); the first three explorations of a new holding show their floor reward; results listed in the host's log and the chronicle. Survey charters (doctrine E) and relic odds are later.

### 7.6 March composer and tracker

**Composer (one sheet, four steps, each editable before sending):**

1. **Destination:** tap a hex (tile LOD) or pick from the list (own holdings, barbarian camps in reach, a province coordinate and hex). The composer shows the destination to the player only, with a lock icon and "sealed: others see only the arrival bell".
2. **Path and arrival:** the WASM planner proposes a path (≤ 32 steps, ≤ 4 provinces); waypoints can be added; the earliest arrival bell is computed (never earlier than departure + 2); a later bell can be chosen up to 72 bells ahead; stamina cost and out-of-supply warnings are shown. Invalid paths show the kernel's reason.
3. **Orders:** stance (Hold default, Assault, Flank, Brace, with the triangle diagram and "+20% damage for the winner" text; Disarray explained for postures only) and **retreat ratio** as a choice list — never retreat / retreat if defenders > 2× / 1.5× / 1× / 0.5× mine / custom — with the rule text "checked against the defenders present when bell b begins" and a guide number from the destination's current frozen roster, labelled as possibly different by then. (Encoding of "never" is open question Q5.)
4. **Tip and costs:** reveal tip presets in priority terms: *Standard* (season default priority, e.g. 0.433 → ≈ 10,000 lamports at a 16k-CU Reveal limit [model]), *Decisive* (×2), *Match the defence pool cap* (priority 2.0), *Custom*; **zero tip** needs an explicit confirmation stating the 50% rout risk if no keeper reveals. The sheet lists the march fee (10,000 lamports, paid to the resolver), the seal bond (20,000 lamports, returned at settlement) and the tip (paid to the revealer), and in M1 says "test SOL, sponsored, no value" (who funds them is open question Q6).

**Send:** seal in the worker (§8) → self-audit → marchbook write → Depart signed by the session key → relay → landed. Progress is a four-step inline indicator (Sealing, Saved on this device, Sent, On chain) with retry on a failure that may pass and no retry on a refusal (the `sealbook` rules).

**Tracker (per march):**

| Step | Shown as |
|---|---|
| Departed (bell d) | public: origin and arrival bell; private: destination |
| Arrival bell starts | "revealing myself" if this browser is open (self-reveal at the bell start + 0–20 s random delay, unsigned, §5.2); otherwise "keepers will reveal after the beacon" |
| Beacon anchored | "the seal can now be opened by anyone" |
| Revealed | by whom (self or keeper, tip paid), slot index i in the province's four for the faction, or **displaced** by a larger same-faction arrival (host bounces home, no loss), or **retreated** |
| Not revealed by the close | **routed**: −50% troops, stamina and tip; flagged as a keeper-liveness finding |
| Clash resolved | link to the report |
| Settled | tip and fee paid, bond returned (or the verdict if a bad seal was proven) |

### 7.7 Incoming arrivals and postures

- **Incoming risk:** departures are public (origin and arrival bell). For each departure from a hostile faction, the WASM `reachable()` tells whether one of the viewer's holdings is reachable by that bell; the holding panel shows "up to N hosts (M troops) may arrive at bell b; destinations unknown", and a notification is raised once per bell.
- **Posture composer (behind a flag until the program's M1 instruction list is fixed):** for each defending host or the garrison, choose a stance, seal it like a march (plaintext version 2), tip and bond, commit **before bell b starts**; the UI explains Disarray (×0.6 dealt, ×1.25 taken) for a committed posture that is never revealed, and self-reveals like a march.

### 7.8 Bell sheet

§6.4.

### 7.9 Clash report

- Per province-bell: sides, each fighter (host or garrison) with troops before and after, stance, fate (held the field, withdrew, destroyed, bounced, retreated, routed), engagements per hex (damage dealt and taken, the stance edge, retaliation of garrisons and ranged defenders at ×0.5), the fair-share allocation by side (who got slots on each hex), the damage-ratio stamina refund, loot and Works points (display only in M1).
- Provenance line: seed round, region anchor and cache addresses, ClashInputs digest, outcome digest; **Verify in this browser** (§4.3) turns the line green or red.
- **Try it in practice:** opens the same inputs in practice mode to change the viewer's own stance or retreat ratio and re-run (the clearly labelled "what if", nothing on chain).

### 7.10 Chronicle and notifications

Founding, ring openings, the viewer's clashes, reveals, routs, settlements, dormancy warnings; a toast stack (`aria-live`), and a notification centre. Browser push notifications are out of M1.

### 7.11 Guided first bells (onboarding)

A checklist card (docked on desktop, peeking sheet on phone), driven by a pure state machine whose inputs are **chain facts first** (Citizen exists, ticket filed, holding settled, scout trained, exploration sent, march departed, revealed, report seen) and local flags only for "seen" states, so it survives reloads and device changes.

| Step | What the player does | While waiting |
|---|---|---|
| 1 Welcome | pick language, read the 5 nouns (Holding, Host, Bell, March, Explore) | — |
| 2 Faction and site | §7.2 | practice mode offered: "your site is drawn at ≈ hh:mm" |
| 3 First build | queue a Farm and a Lumber Camp | — |
| 4 First scout | train Scouts, muster, explore two hexes | results ≈ 11–21 min |
| 5 Practice clash | a practice run against a camp with the same stances and retreat rules | — |
| 6 First sealed march | march on a barbarian camp in reach; the card explains the seal, the tip and why keepers can reveal even if the tab is closed | report ≈ 31–41 min; the card counts down by pipeline state |
| 7 First report | read the clash report; "verify" shown once | — |
| 8 Done | the checklist collapses into the help menu | — |

The steps never gate play: every step can be skipped, and experienced players can dismiss the whole card.

### 7.12 Practice mode (no chain, no money)

- Runs the rules-v10 clash kernel in WASM against a rule bot; nothing is signed, nothing is sent; available before connecting a wallet (`practice.html`) and from the report ("what if").
- Scenarios: raid a barbarian camp; defend a holding (garrison retaliation); stance triangle; retreat ratio; hex fair share by side (allies cannot pool slots); arrival quota (four largest same-faction arrivals win the slots, displacement); unrevealed arrival routed at 50% (why the tip matters).
- Each run shows the same report component as §7.9 and a "re-roll the seed" button to show variance; seeds are local and marked "practice".
- The rule bot is a small deterministic policy (stance by counter-pick with a seeded mix) in JS; nothing it does is a claim about real opponents.
- The page states that practice results earn nothing and are not stored anywhere but this browser.

### 7.13 Spectator

`spectate.html`: the map, chronicle, bell sheet and clash reports with no wallet; used to watch the 1,000-bot local season and for the playtest's observers; same herald rules (§4.2).

---

## 8. The seal in the browser

### 8.1 Format (pinned in design §6.2; the client must match `frontier::seal` byte for byte)

- Plaintext (37 B): `version u8 = 1 | host_id u64 | arrive_bell u32 | dest P i16 | dest Q i16 | dest tile u8 | stance u8 | retreat_bps u16 | path_len u8 | path 12 B (≤ 32 × 3-bit directions) | reserved 3 B = 0`, packed by the WASM kernel (little-endian as the kernel defines).
- `k` 16 random bytes; `salt = sha256("PS-SALT" ‖ k)`; `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ pt ‖ salt)` (postures `"PS-FRONTIER-POSTURE-v1"`).
- Round `T(b)` = `seal_round(genesis_ts, b)` from the kernel (first quicknet round at or after the end of bell b); identity = `sha256(round as u64 big-endian)` (tlock's `hashedRoundNumber`).
- IBE on G2 with the RFC 9380 G1 DST (tlock `encryptOnG2RFC9380`): `U` (96 B compressed G2) ‖ `V` (16) ‖ `W` (16); body = `pt ⊕ SHA256-CTR("PS-KS" ‖ k ‖ c)[0..37]`; seal = 165 B.

### 8.2 Steps in the client

1. The worker receives `(pt37, round, drand pk)`; the main thread first checks the Season's drand chain hash and public key **equal the pinned quicknet constants** in `sdk/frontier/drand.mjs`, else it refuses to seal ("this season's beacon is not quicknet").
2. The worker draws `k` and `sigma` with `crypto.getRandomValues`, computes the IBE block and the body.
3. **Self-audit** (in the worker): recompute `r = H3(sigma, k)`, check `U = r·G2`, `W = k ⊕ H4(sigma)`, decrypt the body with `k`, re-pack and compare the plaintext, recompute `commit` and `seal_root = sha256(commit ‖ sha256(seal))`. This is the relation the on-chain FO check tests, so a bug in the client cannot produce a seal that `ProveBadSeal` would destroy. Failing the audit aborts the march with an error report; nothing is sent.
4. The worker returns `{seal, commit, salt, sealRoot}` and zeroes `k` and `sigma`.
5. The marchbook stores `{host, departBell, arriveBell, pt, salt, commit, sealRoot, round, tip, state}` **before** Depart is signed. `k` is not stored (the salt and the plaintext are what a self-reveal needs).
6. Depart carries `commit` and `seal`; after it lands, the client checks the transit record's `seal_root` equals its own.

### 8.3 Vendoring and cost

- `@noble/curves` 1.9.x (the version tlock-js 0.9 uses) and `@noble/hashes` 1.8.0 (the gateway's pinned version), ESM files only, bare specifiers rewritten to relative paths by a `scripts/vendor-noble.mjs` that also writes a manifest of file hashes; a test checks the manifest (same freshness rule as the web SDK).
- Size: ≈ 212 KB unminified for the files the pairing path needs [measured: file sizes in the S-TLOCK lab's node_modules], ≈ 50–60 KB gzip [estimate]; loaded only by the worker on the first march or posture.
- Time: tlock-js IBE encryption 22 ms per seal in node [measured, S-TLOCK `js-bench.txt`]; in a mid-range phone browser ≈ 100–250 ms [estimate], off the main thread.
- If module workers are unavailable, the same code runs on the main thread with a spinner.

### 8.4 Reveal policy

- **Self-reveal:** if this browser is open at the arrival bell's start, it reveals once (plaintext, salt, `ct_hash`) with no player signature, at a random 0–20 s delay, then again only if the ArrivalSlot is absent after 60 s and the window is open. It never sends the plaintext anywhere before the arrival bell starts.
- **Keeper reveal:** always possible after the beacon for `T(b)` is anchored, thanks to the mandatory seal and the tip. The UI never tells the player that closing the tab is risky when a non-zero tip was set.
- **Lost marchbook** (cleared storage, other device): nothing breaks; the tracker shows "keepers will reveal"; the destination is shown as "sealed (not on this device)" until the reveal.

### 8.5 Vectors shared with Rust

`permutation-rules` (M1, rules area) writes `seal-vectors-v1.json`: for fixed `k`, `sigma`, plaintext and round, the expected seal, commit and root, plus a recorded quicknet signature for the round so both sides can open it. The node test encrypts with the injected `k`/`sigma` and must produce the same bytes, and opens with the signature; the Rust test opens the same seal with the program's opener. The existing S-TLOCK/SP-V2 vectors (`q3-vector.json`, `q4-vector.json`) seed the first version.

---

## 9. Japanese and English

- The switch is today's: `mountLangToggle` in the top bar ("EN" while Japanese is shown, "日本語" while English is), choice saved in `ps-lang` (shared with the v9 page), no reload, every renderer re-runs.
- Source language stays Japanese inline (`L`…`` / `Lh` / `t`); English in `lang/en-frontier.mjs`, added to `EN_GROUPS`, so the existing tests check it: placeholders consistent, the same English for the same key in every file, completeness (a) and (b) over `web/frontier/**`.
- **Conflict rule:** where a Japanese word already has a different English in the v9 dictionaries (軍 → army), the Frontier uses its own Japanese term (軍勢 → host), so the one merged dictionary stays consistent.
- **Glossary additions** (a new "The Frontier" section in `lang/GLOSSARY.md`), proposed:

| Japanese | English | Note |
|---|---|---|
| 鐘 / 第N鐘 | bell / Bell N | never "tick" or "turn" |
| 州 | province | |
| 輪 / 第d輪 | ring / Ring d | |
| 扇区（本拠の扇区） | wedge (home wedge) | |
| 辺境区 | March | the 7-province district; capital M |
| 進軍 | march | the movement; lower case |
| 拠点（村→町→都市→城塞） | holding (Hamlet → Town → City → Stronghold) | the summary's 村 stays for the Hamlet tier |
| 軍勢 | host | |
| 守備隊 | garrison | |
| 封（時限式の封） | seal (timelock seal) | |
| 開封（公開） | reveal | |
| キーパー | keeper | |
| チップ | tip | |
| 撤退比 | retreat ratio | |
| 構え：待機・突撃・側撃・迎撃・混乱 | stance: Hold, Assault, Flank, Brace, Disarray | |
| 探索 / 斥候 | Explore / Scout | |
| 蛮族の野営地 | barbarian camp | |
| 保護（シールド）/ 辺境保護 | Shield / Frontier protection | |
| 夜番の時間 | vigil hours | |
| 入植希望 / ペア希望 | site ticket / pair ticket | |
| 休眠 | dormant | |
| 敗走 | routed | the 50% loss |
| 押し出し | displaced | ArrivalSlot displacement |
| 練習モード | practice mode | |
| 観戦 | spectate | as v9 |

- Numbers: bells with thousands separators (`1,034`), lamports as integers, SOL with up to 6 decimals, times in the viewer's zone with UTC on hover.
- **Never translated:** the Frontier session text, the seal plaintext and domains, program error codes, localStorage keys.
- **Naming collision to settle:** faction display names are Aster, Borealis, Cinder, Dunmar, Ember, Fjordal (ids 0–5), and doctrine C is named "Ember" while faction Ember (id 4) would carry doctrine E "Lumen" if doctrines follow ids. The UI needs distinct names (open question Q7).

---

## 10. Accessibility and phone width

- **Target:** WCAG 2.2 AA; minimum width 360 CSS px with no horizontal scroll; touch targets ≥ 44 × 44 px on phones (≥ 24 px everywhere); base text 15 px on phones, 14 px on desktop, never below 12 px.
- **Map:** the canvas has an `aria-label` and `aria-describedby` pointing to a live text summary of the selection; every map action has a list path (§7.3); focus is visible on the canvas selection; zoom buttons stay visible on phones (today they are hidden below 760 px).
- **Colour:** each faction has a sigil and a pattern at province LOD, so colour is never the only signal; contrast ≥ 4.5:1 for text, 3:1 for map strokes on terrain; fog does not reduce text contrast.
- **Sheets and dialogs:** `role="dialog"` with focus trap and return, Escape and back-button close, headings in order; bottom sheets resize with the on-screen keyboard (`visualViewport`).
- **Motion:** `prefers-reduced-motion` disables pan easing, glows and spinners (as today).
- **Time and announcements:** countdowns are not announced each second; state changes of the viewer's own marches are; time limits in the UI never expire an action the chain would still accept.
- **Japanese typography:** `line-break: strict`, `word-break: normal`, `overflow-wrap: anywhere` only for addresses and hashes; tabular numerals for timers.
- **Network and power:** works on slow 3G with the province LOD only; the wasm and the noble bundle load lazily; the page pauses polling when hidden.

---

## 11. Budgets

| Item | Budget | Basis |
|---|---|---|
| First load (HTML + CSS + JS, excluding wasm and noble) | ≤ 200 KB gzip | [design]; for scale, every `.mjs`/`.css`/`.html` of today's v9 page (dictionaries and SDK included) is 708 KB raw, 225 KB gzip concatenated [measured]; the Frontier page imports only what it uses |
| `frontier.wasm` | ≤ 400 KB raw, ≤ 150 KB gzip, lazy | [estimate], measured in week 1 |
| noble vendored tree | ≈ 212 KB raw, ≈ 60 KB gzip, worker-only | [measured file sizes] / [estimate] |
| Seal (IBE + self-audit) | ≤ 300 ms on a mid-range phone, off the main thread | 22 ms in node [measured]; browser [estimate] |
| Worst-case clash in WASM (verify, practice) | ≤ 50 ms on a mid-range phone | [estimate] |
| Frame | ≤ 8 ms at 390 px, ≤ 12 ms at 1440 px, only when dirty | [design] |
| Province records in memory | ≤ 256 (LRU) | [design] |
| Reads per viewer | ≈ 13 files per bell + own entities (§4.2) | [design]; herald capacity input |
| Depart transaction | ≤ 1,232 B including compute-budget instructions (≈ 699 B + ~60 B) | [estimate]; asserted in a test |
| Reveal transaction | ≤ 450 B (no player signature; 101 B of data) | [estimate]; asserted |
| Time from "Send march" to "On chain" | ≤ 3 s at p50 on localnet/devnet | [estimate] |
| Screenshot suite in CI | ≤ 5 min | [design] |

---

## 12. Security and privacy

- **CSP** on the Frontier pages: `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; frame-ancestors 'none'`. No third-party scripts or fonts, no drand or RPC calls by default (sealing needs none); the optional "check with the chain" uses an RPC URL the player types, added to `connect-src` only through a same-origin proxy or skipped.
- **What never leaves the browser before its time:** the salt and plaintext of a march (until the arrival bell starts), `k` and `sigma` (never stored). Nothing is sent to the operator that it could use against a sealed order (design E3).
- **What the client signs:** only messages it compiled, with every account recomputed; the wallet signs only Join; the session key never signs a transaction with an unexpected program or account.
- **Pins:** program id, cluster, season id, drand chain hash and public key, ruleset hash; a mismatch stops the client with a clear message.
- **Storage:** session key (as v9, with a backup offer), marchbook, language, UI preferences; all under season-scoped keys; a "forget this season on this device" button.
- **Supply chain:** vendored noble files are pinned by hash; no CDN.

---

## 13. Tests

### 13.1 Node test runner (existing CI job "Gateway and web client tests", `npm test` in `permutation-gateway`)

| File | Checks |
|---|---|
| `web-frontier-seal.test.mjs` | vectors shared with Rust (§8.5): plaintext pack/unpack, salt, commit, seal bytes for injected `k`/`sigma`, opening with a recorded quicknet signature; self-audit catches a flipped bit in U, V, W and body; refuses a non-quicknet key; `T(b)` rounds up |
| `web-frontier-codec.test.mjs` | account and instruction layouts against vectors the program crate writes (the `codec_vectors.rs` pattern); PDA and with-seed addresses recomputed (no bump accepted from outside) |
| `web-frontier-clock.test.mjs` | pipeline states of §6.2 from herald records: absent anchor = open and no countdown, tombstone, seed round ≥ A + W + Δ, clock offset |
| `web-frontier-march.test.mjs` | path encoding (3-bit, ≤ 32 steps, ≤ 4 provinces), arrival bell ≥ d + 2 and ≤ d + 72, tip from priority (`2,500 + p × (limit + 1,336)`), zero-tip confirmation required, retreat encoding, **transaction sizes** for Depart and Reveal ≤ 1,232 B |
| `web-frontier-relay.test.mjs` | exact shapes per instruction against a fake relay on port 0; Reveal carries no player signer; the client refuses a relay answer whose message differs; errors mapped |
| `web-frontier-herald.test.mjs` | decoding raw bytes, ignoring JSON; `seq` gap → resync; poll scheduler: only on-screen provinces, jitter window, backoff, hidden-page pause; staleness banner |
| `web-frontier-marchbook.test.mjs` | written before send; self-reveal timing (never before the bell start, at most two attempts); reconciliation with herald transit records; lost-book behaviour |
| `web-frontier-onboarding.test.mjs` | state machine from chain facts; reload in every state; skip |
| `web-frontier-practice.test.mjs` | the WASM kernel loads in node; for fixed practice inputs the outcome digest equals the native `permutation-rules` test vector; ruleset hash check |
| `web-frontier-session.test.mjs` | the Frontier session text is printable ASCII, names site, cluster, program and season, is pinned byte for byte; seed domain; storage keys distinct from v9 |
| `web-frontier-errors.test.mjs` | every program error code has JA and EN text |
| `web-lang.test.mjs` (existing) | picks up `web/frontier/**` and `en-frontier.mjs` automatically |
| `web-sdk.test.mjs` (existing, extended) | `sdk/frontier/*` and the vendored noble tree are fresh |
| `web-frontier-wasm.test.mjs` | `frontier.wasm` hash matches the manifest; a rebuild step (separate CI step with the wasm target) matches it |

The tests reuse the stub-DOM approach of `web-play.test.mjs` for panel logic, and keep the pure parts (clock, marchbook, onboarding, march limits) DOM-free.

**Fixtures:** a small recording from the 1,000-bot local season (season record, 3 rings of overview files for 3 bells, 12 province records, one clash with its ClashInputs and seed, one viewer's entities, anchors and caches) in `permutation-gateway/test/fixtures/frontier/`, ≤ 2 MB, regenerated by a script from the herald's files.

### 13.2 Screenshot smoke tests (separate package, separate CI job)

- `permutation-gateway/screens/` with its own `package.json` pinning `playwright-core` and `axe-core`; `node --test screens/*.screen.mjs`. The default `npm test` stays browser-free.
- A fixture server (node `http`, **127.0.0.1, port 0**) serves `permutation-server/web/` plus a fake herald from the fixtures and a fake relay that accepts the known shapes; nothing touches a chain or a reserved port.
- Deterministic: `page.clock` fixed at a fixture time, fixed `Math.random`/`crypto.getRandomValues` seed in the page for the seal, fonts from the repo only.
- Matrix: 10 screens (join/faction, site picker, map world LOD, map tile LOD with fog, holding, march composer, march tracker, bell sheet, clash report, practice result; plus onboarding card and spectator as extra) × **JA and EN** × **360×740, 390×844, 1440×900**.
- Assertions per shot: no console errors or failed requests; no horizontal overflow (`scrollWidth ≤ innerWidth`); landmarks and the bell chip visible; every interactive element ≥ 44 × 44 px at phone widths; axe-core with no serious or critical violations; in EN, no Japanese characters in rendered text except proper names and user text; the language toggle switches and re-renders without reload; screenshots written to an artifacts directory.
- Pixel comparison with goldens is **advisory** (reported, not failing) for the HTML chrome only, never the canvas, until the layout settles.
- CI: a new job `web-screens` installing Chromium through Playwright; runs on every push once the owner approves pushing (standing rule); locally the cached Chromium under `~/Library/Caches/ms-playwright` can be used.

### 13.3 Playtest readiness checks (before the owner-approved devnet playtest)

- The whole onboarding completed by a scripted browser run on the local season (Playwright driving the dev wallet), a march from composer to report, and a practice run, at 390 px in both languages.
- 1,000-bot local season: the spectator open for 24 h without memory growth beyond 200 MB and with the request rate of §4.2.

---

## 14. Work plan (inside M1 weeks 7–15)

| Week | Web work | Needs from other areas |
|---|---|---|
| 1 | Skeleton pages, `fstate`, `config`, `fsession`, `fchainio` pins; `frontier-wasm` crate with geometry, terrain, travel, clash; size and speed measured; vendoring script | rules: `frontier::seal` API and vectors; program: account layouts draft |
| 2 | Herald client and codec against fixtures; map world/province LOD; bell clock | herald: API draft (Q1); program: codec vectors |
| 3 | Join, faction picker, site tickets, vigil; holding panel | program: Join, FileTicket, SettleTicket, Harvest/Build/Train; relay shapes (Q2) |
| 4 | Seal worker, self-audit, marchbook, composer, Depart; tile LOD and path planner | program: Depart; rules: seal vectors with recorded signatures |
| 5 | Reveal (unsigned) and tracker; bell sheet; clash report and verify; Explore; SettleTransit | program: Reveal, Explore, SettleTransit; keeper running on the local season |
| 6 | Onboarding, practice mode, incoming warnings, (posture if in M1) | bots: fixtures from the local season |
| 7 | Phone layout pass, accessibility pass, screenshot suite, spectator | — |
| 8 | Local 1,000-bot season support, playtest hardening, JA/EN copy review | — |

---

## 15. Risks and open questions

**Risks**

1. **Onboarding latency.** 11–21 min to a first holding and 31–41 min to a first clash report (§6.3); without the practice mode and clear waiting states, newcomers leave (R3). Mitigation: §7.11, and consider a design change (Q4).
2. **Browser crypto correctness.** A client seal bug destroys the player's host through ProveBadSeal. Mitigation: self-audit of the FO relation, shared vectors with the Rust opener, pinned noble versions (R17).
3. **WASM size and speed on phones** are unmeasured; the tile LOD depends on terrain generation in WASM. Mitigation: week-1 measurement; province LOD works without WASM.
4. **Herald and relay contracts are not designed yet** from their side; the client assumes raw bytes + event heads and per-(province, bell) immutable files. Mismatch costs weeks.
5. **Canvas at scale:** 12k provinces at R_MAX 64; M1 plays at ≈ ring 7–10, so the far-zoom path is exercised only by synthetic fixtures.
6. **The v9 client shares `lang/`, `util.mjs`, `wallet.mjs`, `session.mjs` and `map.mjs`.** Additive exports only; its tests guard it.
7. **Screenshot tests are flaky** if they compare the canvas; kept advisory.

**Open questions**

- **Q1 (herald):** does the herald serve raw account bytes with slot and event head, immutable per-(province, bell) files and per-(ring, bell) overviews, and a WS diff stream with sequence numbers, as §4.1 assumes?
- **Q2 (relay/gateway):** the Frontier relay routes (§5.1): random fee payer per request from ≥ 150 keys, unsigned Reveal accepted (shape and rate limits per holding), `/f/nudge` for catch-up resolves, per-citizen sponsorship quota.
- **Q3 (program scope):** are CommitPosture/RevealPosture in the M1 program? The composer is ready behind a flag.
- **Q4 (design owner):** restate design §2.3's timeline ("minute 10+") to the reachable 31–41 min, or change the pipeline for the onboarding camps (for example a shorter W for barbarian-camp bells is **not** proposed here, since W is a season parameter that protects C4).
- **Q5 (rules/seal spec):** the plaintext's `retreat_bps u16` vs the kernel's `Option<Bps>`: pin the encoding of "never retreat" (proposal: `0` = never, `1..=65,535` = the ratio in bps) and the path direction order, in `frontier::seal` and its vectors.
- **Q6 (economy of M1):** who funds the march fee, seal bond and tip in a no-money M1 (relay allowance per citizen, faucet on localnet/devnet)? The UI shows them as test SOL either way.
- **Q7 (naming):** doctrine C "Ember" vs faction "Ember"; pick distinct display names before the join screen is written.
- **Q8 (hosting):** is the Frontier page served by the herald's listener (same origin as `/h/*` and `/gw/*`), and on which non-reserved port in the local stack?
- **Q9 (fog):** is presentation-only fog with a "show everything" switch acceptable for the playtest, or should fog be on without a switch (still presentation only)?
