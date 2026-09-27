# W3-F web-play — notes

- **Unit:** W3-F (wave 3), branch `frontier/m1-W3-F`, cut from `frontier/m1-integ` at `241c52d`.
- **Contract:** `docs/frontier/m1/M1-CONTRACT.md` v1.3. The sections this unit uses are §5.3 (layouts), §5.4 (errors), §5.6/§5.9–§5.11 (the player shapes), §6 (PS2 records), §8.3 (relay), §8.4 (herald), §9 (web data contracts), §10.1 (tips), §11 (the W3-F brief), §12 (gates) and §13.6 (E7). The web area design is `docs/frontier/m1/design/web.md` §7.
- **Owner decisions applied (2026-09-27):** O-M1-12 is not approved, so this unit ran no `rustup target add`, no Playwright, no drand fetch and no Agave install. `frontier.wasm` stays **PENDING-OWNER**, and the kernel paths are tested against the recorded `frontier-wasm/vectors/wasm-vectors.json` answers. O-M1-06 (three tip presets, no zero tip), O-M1-13 (the 11–21 / 31–41 minute copy), O-M1-14 ("Flame", fog with a "show everything" switch), O-M1-20 (cohorts, refile) and O-M1-22 (Works-only explore floor, immediate Train) are the working defaults, and the page follows them. The rustfmt/clippy install is already recorded in `docs/frontier/DECISIONS.md` part A, so this unit made no DECISIONS edit.
- **Tags:** [measured] means run on this machine on 2026-09-28.
- **Not done, by rule:** no push; no chain transaction; no devnet step; `permutation-server/web/session.mjs` was not touched. The only service started was one ad-hoc fixture server on **127.0.0.1:41780** (the §10.3 per-unit range, unit 18), used for a browser check. It was stopped afterwards, and the tests bind `127.0.0.1:0`.

## 1. What landed

All of it is under `permutation-server/web/frontier/`, `permutation-server/web/lang/en-frontier.mjs` and three new tests.

### Logic modules (DOM-free and tested)
| File | What |
|---|---|
| `fplay.mjs` | The write path for every shape the page sends. `accountsFor(name, …)` recomputes each account from the pinned season: Join (wallet, JoinShard, the join gate when the Season has one), SetSession, SetVigil, FileTicket (the distinct provinces of the sites), Harvest, Build (the Province only for walls), Train, Muster, Dissolve, Garrison, Explore (the host's province), Depart, SettleExplore and SettleTransit. `buildShape` adds the budgets prefix at CU price 0. `submit` runs GET /f/relay, builds, runs `messageProblems` (exact accounts and data, the relay's allowlist), signs once (the session key, or the wallet for Join and SetSession, or nobody for a settle shape, where the session key signs the request as `requester`), checks the size, and POSTs. **The answer's signature must be the announced fee payer's Ed25519 signature of the message this page signed**, otherwise `RelayMessageChanged`. `track` follows GET /f/tx to landed, failed (program code) or expired. A Reveal is refused by construction (I-24). |
| `fmarch.mjs` | Composer model: the retreat list with `never` = 0 first (I-27); `tipOptions` gives exactly the three presets of §8.3 from the Season (no zero tip, no custom tip); `marchCosts`; `arrivalWindow` from `min_lead`/`max_lead` and the planner's earliest bell. `planRoute` and `earliestBell` use the WASM kernel. `checkMarch` names every refusal it can foresee: NotFinal, NotResident, HostBusy, **HostInTransit**, Cooldown (stamina for the I-32 charge of 74, or the ready bell), TransitState, NoDestination, NoPath/Path, ArrivalBell, Stance, Retreat, TipTooLow and TipNotPreset. `plaintextOf` packs and validates. `sendMarch` runs seal → **marchbook saved before signing** (not sent if storage refuses) → Depart → on chain; a refusal marks the entry failed (it can be re-sealed), and a lost answer keeps it for reconciliation. `tracker` gives the steps and the lock while the transit is in state 1–3. `incomingWarnings` uses the kernel's `reachable`. |
| `fland.mjs` | Free land per wedge and the candidate provinces (rings ≥ 2, home wedge = faction). Free sites come from the Province itself (sites beyond `site_count` do not exist; a released site counts as free). `landState` covers none/joined/ticket/provisional/final/refugee and the escrow still needed (7,152,640 lamports). `ticketTimes` gives the result time and the cohort end at ticket_bell + 24. `refileOffer` offers one-tap refile after a displacement or an ended ticket. `accrualAt` transcribes `Accrual::value_at` (floor as `div_euclid`). Also here: `storesAt`, `holdingFacts`, `hostsIn`, `actionBlocks` (I-29, I-44) and the explore targets. |
| `flog.mjs` | Decoder for the PS2 kinds the chronicle, tracker and warnings read (15 kinds), pinned field for field against `frontier-abi/vectors/logs.json`. `logBell` is the head's bell, because CLASH has its own `bell` key. |
| `fui.mjs` | `ps-fui:<cluster>:<program>:<season>`: fog, LOD, tab, dismissed hints and the last ticket's sites. Every field is checked, and damage falls back to the defaults. This was an integ-W2 deferred item. |
| `controller.mjs` | The play controller. It handles wallet discovery (the dev wallet only for a localnet season on a loopback page), a silent reconnect, the kept session key, the refresh loop (§4.2 cadence), the self-reveal timer (`marchbook.revealStep` → POST /gw/f/reveal, at most twice), marchbook reconciliation, tracker facts from bell-region records and per-bell envelopes, the chronicle and the incoming warnings. It maps every `data-act`, `data-form` and `data-bind` of the screens to an action. |

### Screens (`screens/`, the file names W5-E owns in wave 5)
- `shell.mjs`: faction and quota chips, the bottom tabs (Map, Holding, Hosts, Marches, More; Holding and Hosts appear once the viewer holds land), the notice line (re-rendered in the current language), lamports and times.
- `join.mjs`: the wallet list, faction cards (doctrine, free land), invite field on a gated season, the key and Join, re-making the key, the site picker (1–3 sites in order, escrow shown), the ticket card and the provisional card (cohort finality).
- `holding.mjs`: stores, build list (first-copy costs), queue, train, reserve, muster, garrison, vigil, transits, blocked reasons and the Catch up nudge.
- `host.mjs`: the host list with stamina and cooldown; **the lock copy for a host in transit** and disabled actions.
- `explore.mjs`: target tiles, the floor counter, the pending record and the settle button.
- `march.mjs`: the composer (four fieldsets, **three tip radios**, retreat select with "never" first, costs, the four-step indicator).
- `tracker.mjs`: per-march steps, the private destination ("only you can see this"), the rout warning, the outcome, the lock and the settle button.
- `incoming.mjs`, `bell.mjs` (pipeline per bell, no countdown before THE anchor), `chronicle.mjs`.

### Changes to W2-E foundation files (all additive)
- `app.mjs` wires the play screens in play mode only; practice and spectate are unchanged.
- `index.html`: `#panel-body` and `#tabs`.
- `frontier.css`: play styles, 44-px targets, faction classes `.f0`–`.f6` (the CSP forbids style attributes).
- `fchainio.sendTx/sendJoin` take `extra` body fields (`lastValidBlockHeight`, and the settle `requester`, `requesterSig`, `citizen` of §8.3 v1.3).
- `herald.mjs` (integ-W2 deferred checks): **`me` key checks** (the Citizen at its canonical address naming the wallet; each Holding canonical, owned by that Citizen, named by it, at most 3) and **envelope bell checks** for per-bell files.
- `seal.checkBeacon` also takes the Season's **NETWORK byte**, which must be 2 (integ-W2 deferred).
- `marchbook.entryOf` stores `seal_b64` (see D5).
- `fi18n.mjs`: texts for every relay refusal and page code, `failureText`, doctrine, building, holding-state and transit-outcome tables.
- `lang/en-frontier.mjs`: +295 entries.

### Tests (`permutation-gateway/test/`, `node --test`, no network except `127.0.0.1:0`)
| File | Tests | Covers |
|---|---|---|
| `web-frontier-march.test.mjs` | 18 | rules constants and the BUILDINGS table pinned against `host.rs`/`travel.rs`/`fixed.rs`/`catalog.rs` and the sim's wedge rule; PS2 kinds against `logs.json`; the retreat list; the three presets (14,441 / 21,662 / 28,882 at the default season); the arrival window; every `checkMarch` refusal; the plaintext; planner, earliest bell and `reachable` through a kernel answering from the recorded vectors; the composer markup (exactly three tip values, never first, the send button disabled with the reason); `sendMarch` with **real IBE sealing to the test key**, the book checked *inside* submit (saved, no k/σ), seal root = sha256(commit ‖ sha256(seal)), round = T(arrive); refusal, lost answer, no storage, bad tip and failed audit; **Depart ≤ 800 B**; the tracker and the HostInTransit lock in tracker and host screens (JA/EN); accrual pinned by the recorded `accrual_at` call; land states, cohort times, refile; the site picker on the fixture overview; explore targets and I-29 blocks; ps-fui |
| `web-frontier-relay.test.mjs` | 7 | the **real relay** (`createFrontierApps`, public listener on port 0) over a scripted chain that verifies signatures and applies each shape's escrow. It covers every player shape the page builds (SetVigil, FileTicket with 1 and 3 sites, Harvest, Build and walls, Train, Muster, Dissolve, Garrison, Explore, Depart at each preset, Join and SetSession wallet-signed, a gated Join with the gate co-signed after an invite). The chain received exactly the message the page signed, with the accounts the SDK derives. Settle shapes are charged to the requester's citizen (quota −1 each). TipNotPreset is enforced for another tip and for zero. Reveal material goes to the keeper with exactly five fields. A relay answering another signature is caught (`RelayMessageChanged`), as is a relay naming another program (`PinMismatch`). `HostInTransit` comes back as 409 with code 58 and maps to text. GET /f/tx reports landed, failed (TipTooLow 51), expired and unknown. |
| `web-frontier-errors.test.mjs` | 4 | every ABI code (errors.json = abi.mjs) has distinct JA/EN text; **every relay code scanned from `src/frontier/**`, `cosign`, `guards`, `send`, `routes/errors` and the SDK's `refuse(...)`** has a text; every code the page's modules return (scanned) has a text; `failureText` precedence |

## 2. Measurements [measured]

| Item | Result |
|---|---|
| `(cd permutation-gateway && npm test)` | **472 tests: 471 pass, 0 fail, 1 skip** (the PENDING-OWNER `frontier.wasm` test). Before this unit: 443/442/1 |
| Depart wire size (session key + relay fee payer, compute-budget prefix, 219-B data) | ≤ 800 B asserted. It was also reported per preset in the relay test (`r.bytes`) |
| Real IBE seal + self-audit, inline in node (test key) | the march tests complete in ≈ 1 s total |
| `web/frontier/**` html+css+mjs excluding `seal-worker.mjs` (37 files) | **283,900 B raw, 93,509 B gzip** by `cat $(find web/frontier \( -name '*.mjs' -o -name '*.html' -o -name '*.css' \) ! -name seal-worker.mjs \| sort) \| gzip -9 \| wc -c` (concatenated; this restates W2-E's figure with its command, as integ-W2 asked). `lang/en-frontier.mjs`: 35,697 raw / 12,559 gzip. The page budget of 200 KB gzip including shared files still has headroom. |
| Browser check (in-app browser, fixture server on 41780: web tree + synthetic herald + a generated viewer) | The page booted with no script errors (only the fixture server's expected 404s and the 503 relay). Checked: wallet list → dev wallet → faction cards → key derivation (the dev wallet signs the session text) → Join refused with the relay-unavailable text; Holding, Hosts (lock copy on the departed host), the March composer (camp quick destination, the NoWasm reason, three tips, costs 44,441), More (bell, chronicle, settings); JA ↔ EN switch without reload; the tab remembered through ps-fui |

## 3. Gate W3 items for these files (2026-09-28, on this branch)

| Item | Result |
|---|---|
| `(cd permutation-gateway && npm ci --ignore-scripts && npm test)` | exit 0 (471/472, 1 skip) |
| `(cd permutation-gateway && node scripts/sync-web-sdk.mjs --check)` | exit 0 |
| `node scripts/vendor-noble.mjs --check` | exit 0 |
| `git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src` | exit 0 |
| `build-wasm --check` | **PENDING-OWNER: build-wasm (wasm32 target, O-M1-12)** |
| Rust gate lines (fmt, clippy, svm-tests, frontier-node) | **not run**: this unit changed no Rust file and no manifest |

## 4. Deviations and why

| # | Deviation | Why | Who resolves |
|---|---|---|---|
| D1 | The screens are pure renderers; a new `controller.mjs` holds the actions and refresh. `app.mjs` only routes | W5-E owns the screen files in wave 5 and W4-E owns `app.mjs` in wave 4, so keeping logic outside both shrinks their diffs | — |
| D2 | A few rules constants (STAMINA_CAP 120, march stamina 10 + 2·32, MILLI, MIN_HOST_TROOPS) and the catalog's BUILDINGS table are transcribed, as are the 15 PS2 kinds the page decodes | No generated vector carries them, and the wasm exports have no catalog. Each is **pinned by a test against its Rust source or `logs.json`**, so a change fails a test instead of drifting | request R2 |
| D3 | The composer's earliest bell uses the unit's pace without the doctrine's travel bias | `plan_path` takes no faction. Every M1 doctrine's travel bias is ≤ 1.0, so the bell offered is never earlier than the program accepts; a doctrine-E player cannot pick their true earliest bell | request R3 |
| D4 | `reachable` keeps W2-E's signature `(origin, dest, genesis_ts, depart_ts, target_bell, unit)`, not §9.5's `(origin, depart_bell, target_bell, unit)`; `depart_ts` = the start of the departure bell (optimistic, so warnings err towards warning) | a warning needs the holding it is about; `frontier-wasm` is not this unit's to change | integrator: amend §9.5 (R4) |
| D5 | Marchbook entries add `seal_b64`, `signature` and `failure` | SettleTransit must carry the logged commit and seal (§9.4). Depart publishes the seal anyway. The page uses its copy only after sha256(commit ‖ sha256(seal)) equals the on-chain `seal_root`, i.e. only the logged pair | — |
| D6 | The page's SettleTransit names the relay fee payer as `beneficiary` (and as slot beneficiary or resolver when the herald has not given those accounts), and slot index 0 when no slot is known | the bad-seal reward is the only payment to the beneficiary, and this client's seals are self-audited; the fee payer paid the fee. Keepers normally settle first | W4-B/W4-C may give the page the exact accounts through the herald |
| D7 | Muster `troops` and Garrison `delta` are sent in **whole troops** (the reserve's unit), not MilliTroops | §5.10 gives Muster bounds 100–30,000 and "reserve[unit] ≥ troops", with the reserve in trained troops; W3-B was not merged to confirm | W3-B / integrator: confirm; if milli, change `FORMS.muster`/`garrison` in `controller.mjs` |
| D8 | The page keeps its own decoder (`fcodec.mjs`) instead of switching to `sdk/frontier/codec.mjs` (W2-E D1's option) | the screens use the camelCase fields throughout; both decoders are generated from, or pinned to, the same frontier-abi vectors | — |
| D9 | The stance "Hold" shows as "Idle" in English | `fi18n.STANCES.Hold` uses 待機, whose English comes from v9's dictionary; W2-E's shell test pins the Japanese | W5-E (copy review): a Frontier-only key such as 待機の構え → "Hold" |
| D10 | Not built: the design's optional "check with the chain" (a user-chosen RPC); fetching the landed transaction back through the herald (replaced by the stronger fee-payer-signature check of the exact message); bottom sheets and the phone layout pass | outside W3-F's brief or later units' (W5-E layout, W4-E onboarding and report) | W5-E / W6-D |
| D11 | Without `frontier.wasm` the composer cannot plan a path: Send stays disabled and says the rules module is missing; incoming warnings say they need it | O-M1-12 not approved | owner (O-M1-12) |

## 5. Dependency requests (integrator, I-55)

- **R1 — manifests and lockfiles:** none. No new dependency was added; the tests use what `permutation-gateway`'s lockfile already installs.
- **R2 — generated rules data for the web:** have `frontier-abi` (or `frontier-wasm`) export the catalog (building resources, rates and costs, `ITEM_WALLS`), `STAMINA_CAP`, `march_stamina` and `MILLI`, so D2's pinned transcriptions can become generated (W5-A/W6-B).
- **R3 — `plan_path` / `earliest_arrival_bell` with the doctrine's travel bias** (take the faction) in `frontier-wasm` (W6-D).
- **R4 — contract §9.5:** amend `reachable` to the implemented signature (D4).
- **R5 — GLOSSARY (W5-E):** add to the Frontier section: 区画 = site (a holding site), マス = tile (a hex of a province), 預け金 = escrow, 中継 = relay.

## 6. Notes for the next owners

- **W3-D (herald):** the page reads `/h/me/{wallet}` as `{citizen: {address?, bytes_b64}, holdings: [{address?, bytes_b64}], quota?}`. Addresses are checked when present. It also reads `/h/events?after=` as `{events: [{seq, slot, sig, body_b64}], next}` and decodes the body itself; `/h/bell/{b}/region/{r}` as `{anchor: {bytes_b64}, caches: [{nonce, round, seed}], archived, tombstoned}`; and per-bell province files whose `bell`, slots, day and inputs are all that bell's.
- **W4-E:** `app.mjs` has `panelMarkup(FS)` (tab → screens); add the report, practice and onboarding there. `FS.chronicle` holds decoded records `{seq, record}` for the onboarding facts. `fmarch.tracker` gives the steps a report link can extend.
- **W4-B/W4-C:** SettleTransit from the page carries the marchbook's seal only after the seal-root check (D5). A settle is charged to the requester's citizen, as §8.3 v1.3 says.
- **W5-E:** the screens return markup from pure functions; the CSS is a working layer, not the phone pass. The notice line is `role="status"`/`alert`, and the panel body is deliberately not live.
- **W6-D:** re-record the herald fixtures from a local season. `web-frontier-march.test.mjs` builds its accounts from the layout table and needs no fixture beyond the Season, the overview and the wasm vectors.

## Links

- Logic: `permutation-server/web/frontier/{fplay,fmarch,fland,flog,fui,controller}.mjs`
- Screens: `permutation-server/web/frontier/screens/{shell,join,holding,host,explore,march,tracker,incoming,bell,chronicle}.mjs`
- Changed foundation: `permutation-server/web/frontier/{app,herald,seal,fchainio,marchbook,fi18n}.mjs`, `index.html`, `frontier.css`; `permutation-server/web/lang/en-frontier.mjs`
- Tests: `permutation-gateway/test/web-frontier-{march,relay,errors}.test.mjs`

## Post-merge addendum (integ-W3 review, 2026-09-28)

- **Size figure restated:** re-running the stated command on `a7e855c` gives **283,896 B** raw (the notes said 283,900); gzip 93,509 B and `en-frontier.mjs` 35,697 / 12,559 B match.
- **Review fixes by the integrator** (`integ-W3-NOTES.md` §6.6, contract v1.5 §21): send-time earliest arrival bell (90-s margin, re-checked at send), chronicle from `headSeq − 500` with full-page paging, warnings at cavalry pace, remote hosts' provinces loaded, invite field on a gated season, SettleExplore enabled from the bell record's seed source, a reveal-only tick on a hidden page, failed-send revival, SettleTransit only with the arrival bell's envelope, garrison increases only. D10 (relay-signature check instead of fetching the landed transaction back) is now §9.4's text.
