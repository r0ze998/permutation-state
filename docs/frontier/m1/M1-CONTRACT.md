# M1 "First Bell": the integration contract

**Version v1.4** (2026-09-28, integ-W3 amendment in the wave-3 integration window; section changed: §5.5; **§20 lists the change**). **v1.3** (2026-09-28, integ-W2 amendments after the wave-2 review; sections changed: §4.1, §4.2, §5.2, §5.3, §5.5, §5.7, §5.8, §5.11, §8.2, §8.3, §8.7, §9.1, §9.4, §12, §13.1; **§19 lists each change**). **v1.2** (2026-09-27, integ-W1 amendments after the wave-1 review; sections changed: §3.2, §4.1, §4.2, §5.3, §5.4, §5.5, §5.6, §5.9, §5.10, §5.12, §6, §7, §8.7, §12; **§18 lists each change**). **v1.1** (2026-09-27, review revision of v1.0). v1.1 answers a review of 21 issues (5 marked blocker, 16 major; three duplicate others and are merged, so 18 distinct findings). **§17 gives each issue's verdict with its evidence and the sections it changed.** Where v1.1 and v1.0 differ, v1.1 wins; the first issue is kept as `lab/contract-rev/M1-CONTRACT.v1.0.md`.

- **Date:** 2026-09-27. **Role:** M1 integration architect. **Status:** normative for every M1 implementer from the moment the owner accepts it (owner decisions still open are listed in §15 and in `DECISIONS.md`; each has a working default that implementers build to).
- **Base:** branch `codex/frontier` at `d95fa25` (worktree `.claude/worktrees/frontier-integ`, clean). Planning phase: nothing in the repo was changed, no commit, no push, no server, no chain transaction.
- **Merged from:** the four area designs in `scratchpad/frontier/m1/design/` — `program.md` (the on-chain program), `offchain.md` (keeper, relay, herald, bots, verifier, local stack), `web.md` (browser client), `closeout.md` (the M0 remainder, CL-01…CL-38). **Normative above them:** DESIGN rev 3.1 §0–§13 and §19, `M0-FINAL.md`, the owner decisions O1–O8, N1–N5, D23. Where this contract and an area design differ, **this contract wins**; §2 lists every conflict and how it was resolved.
- **Evidence it relies on:** SP-V2 and SP-FEE (M0b), the M1 lab runs `m1/lab/clash-opt` (Phase A clash rewrite: worst ResolveFromInputs 540,891 → 274,798 CU, heap 25,832 → 21,024 B, digest-identical on 4,320 inputs [measured]), `m1/lab/offchain-svmspeed` (LiteSVM ≈ 60–90M CU/s on this machine [measured]), `m1/lab/rev31-m1` (capacity with M1 budgets [model]), `m1/lab/closeout-civ` (the legacy CI failure reproduced [measured]).
- **Review checks for v1.1 (2026-09-27, read-only) [measured]:** SP-V2 `spv2.so` is 480,512 B (`out/plain-v2`) and 540,608 B (`program-kprobe`, sbpfv2 release) against SP-FEE's 13,600-B `sfee_probe.so`; `permutation-rules/src/frontier/clash.rs` step 2a (l. 652) seeds its caps from the non-arrival units of the input only; `m0b/spikes/SP-V2/beacons/quicknet` holds 32 rounds; `rustup target list --installed --toolchain 1.95.0` lists only `aarch64-apple-darwin`; `lsof` shows the owner's services on 127.0.0.1:4185, 4191 and 4194; `solana-test-validator 3.1.9` is installed; `frontier-sim` models one camp per province on a non-site tile (`sim.rs` l. 87–103, 1119–1147) and trains troops instantly (l. 1535–1560).
- **Tags:** [measured], [sim], [model], [estimate], [design] as in DESIGN. **MUST / MUST NOT** are binding on implementers; "should" is advice.

---

## 0. The contract in one page

1. **What M1 builds.** A new program `permutation-frontier` (new program id, SBPF v2, no money, no MagicBlock) with 50 instructions (49 plus the test-only `oracle` ResolveClash) over **17 account kinds** (v1.1 removes ProveBadSeal and SealVerdict: SettleTransit itself is the seal proof, and adds SweepPoolOwed); a shared no-Solana ABI crate `frontier-abi`; new pure kernels in `permutation-rules::frontier` (`beacon`, `addr`, `seal`, `fees`, `office`, `camp`, `explore`, `catalog`, the last three specified from `frontier-sim`) plus the M0 kernel bounds, the Phase-A clash rewrite and a storage-aware clash room; a Rust off-chain workspace `frontier-node` (keeper, herald, verifier v2, bots, LiteSVM-backed local chain node, drand replay server with a test-key mode, stack orchestrator, in-process integration tests); a Node relay under `permutation-gateway/src/frontier/` with a JS SDK; a static web client `permutation-server/web/frontier/` with a WASM build of the kernels and a browser tlock seal.
2. **What M1 does not build** (layouts reserve room): money, vaults, fees, stakes, claims, FactionShards, reward indices, banking (M2); postures, sieges, occupation, capture, holdings 2–3, pair tickets, governance, Mandates, decrees, relations, caravans, Bourse, Engine, Relic Sites, delegation, Shades (M2/M3).
3. **Key resolved choices** (§2): every program account except the Season PDA is a **with-seed address of the Season PDA** in SP-V2's exact grammar; **quiet-bell proof = ArrivalDay bitmap + SkipQuiet**; **SettleDeparture** moves departed-host values into the Holding and GatherClash reads Holdings; transit outcomes by **rank against the final arrival set**; **one-way reveal latch**; **minimum tip in priority terms**; **AnnounceSeason** re-roll guard; **Reveal takes the salt, not the seal key**; a **beneficiary** field on every keeper write; the **PS2 log contract** with per-entity event heads; keeper write classes **W (Reveal, to priority 2.0, pool-eligible)** and **D (delay-only, to 0.5, never pool-eligible)**; exit run on **ports 41000–41999**. **v1.1 adds:** a storage-aware clash room so a Province can never overflow its 56 entries (I-43); **settlement is the seal proof** and hosts in transit cannot act (I-44); loaded-data limits sized from the real program (I-45); tombstone-checked gathers and explicit status sets (I-46); ticket cohorts instead of a landing-time deadline (I-47); no global account written on the reveal, gather, resolve or settle paths; the remaining global writers are priced in the §8.8 lock table (I-48); rent refunds go back to the payer that paid (I-49); CU and heap escape hatches (I-50); a program-level join gate and upgrade-authority AnnounceSeason for the playtest (I-51); a claim grace for defence refunds (I-52); a `test-beacon` build so day-scale gates do not wait for downloads (I-53); real 400-ms slots in Mode A (I-54); the integrator owns manifests and locks (I-55); kernels specified from the simulator (I-56); a 7-wave plan (I-57); verifier checks V11–V13 and tampers T17–T22 (I-58).
4. **Delivery:** 7 waves, ≤ 6 units each, exclusive file ownership per wave, and an integrator who owns manifests, lockfiles and a two-day integration window after each merge (§11); a gate per wave with exact commands (§12); the exit test plan (§13). **Honest estimate (§14): 12 calendar weeks nominal, ≈ 50% by 12.5 weeks and ≈ 80% by 14.5, with six parallel implementers plus an integrator; ≈ 58.5 unit engineer-weeks + ≈ 4 integrator = ≈ 62.5 engineer-weeks.** v1.0's "10 weeks, ≈ 45 (§0) / 47 (§14) engineer-weeks" is withdrawn: the two figures disagreed and left out integration, gate wall time and owner-approval latency. The M0 closeout (N5) completes at week 1.5, not in week 1.

---

## 1. Scope and non-goals

**In (M1 program):** Season lifecycle (announce, create, genesis seed, run, end, close, abort before joins), rings and provinces (OpenRing, ring seeds, OpenProvince with barbarian camps), FoldOccupancy, Join (free; invite-gated through `Season.join_gate` in the playtest preset, I-51), session keys, vigil, site tickets with displacement and a challenge window, Harvest/Build/Train, Muster/Dissolve/Garrison, Explore/SettleExplore, dormancy release and stranded hosts, Depart with the mandatory seal and a minimum tip, Reveal (quota ranking, displacement, ArrivalDay, latch), SettleDeparture, GatherClash, ResolveFromInputs, SkipQuiet, SettleTransit (which opens and judges the seal: the bad-seal proof, I-44), SweepPoolOwed, beacons (PostAnchor single and combined, PostSeed, PostBeacon), ArchiveAnchors and every close path, DefencePool with ClaimDefence (Reveal-only eligibility, claim grace I-52), event chains and logs. ResolveClash (single transaction) exists **only** under the `oracle` feature, for tests.

**In (off-chain and web):** keeper (beacons, genesis, rings, folds, tickets, reveals with escalation from ≥ 150 rotating payers, departure settlement, gathers, resolves, quiet skips, transit settlement, archives, closes, defence claims), herald fold and cached read path, relay (free, sponsored), bots (1,000, with 8 adversarial personas), verifier v2 (entity chains, randomness audit, seal audit with the stock `tlock` crate, clash replay, land, explore and payment checks, 22 tamper classes), local stack (Mode A accelerated), web client (map, holding panel, march UI, tracker, bell sheet, clash report with in-browser verification, onboarding, practice mode, spectator), JA/EN.

**Out of M1 even though DESIGN §12 lists it:** PosturePDAs / CommitPosture / RevealPosture (M3; DESIGN §12 lists them under M1 accounts — deviation I-16, owner to confirm). Pair tickets (M3). The ER lane, Skirmish, Arena (O8).

**Exit (DESIGN §12 + task):** every instruction under budget on SBF (SBPF v2) with adversarial fill; a pre-funding test for every creation path and a forgery test for every keyed account; program-level lag and "no Reveal after close" property tests; a 7-day accelerated local season with 1,000 bots on ports in 41000–41999; verifier PASS on the real season and FAIL on every tamper class (22); web smoke tests; the full Reveal CU measured and fed to the C4 model; a cheaper quiet-bell proof in place; the clash kernel optimised with heap margin kept. **Then, only with the owner's approval,** a private devnet playtest of 50–200 people with no money.

---

## 2. Conflict register (resolved)

"Owner?" = yes means the resolution changes something the owner decided or DESIGN rev 3.1 states, so it is also an owner decision in §15 (the resolution is the working default until the owner rules).

| # | Topic | Sources in conflict | Resolution (normative) | Owner? |
|---|---|---|---|---|
| I-01 | Player account addresses | DESIGN §8.2 (PDAs with stored bump for Citizen/Holding/Province); program D2 (with-seed) | **With-seed of the Season PDA for every program account except the Season.** No bump anywhere except the Season's, found once in AnnounceSeason and stored | yes (D2) |
| I-02 | Seed grammar | closeout CL-21 ("big-endian", `ar` = 30 B with i32); program §3.1 (little-endian, i16 coordinates, `day u16`); SP-V2 `acct.rs` (LE, i32, `day u32`) | **SP-V2's grammar byte for byte:** `tag(2) ‖ lowercase hex of the fixed-width little-endian key fields`, coordinates as **i32**, days as **u32**, at most 15 raw bytes (≤ 32 B). Accounts *store* P, Q as i16; seeds widen them to i32. §4.1 | no |
| I-03 | ABI home | program: layouts in the program crate without the `program` feature; offchain P1: a no-Solana `frontier-abi` crate | **`frontier-abi`** (root workspace member, `no_std`, depends only on `permutation-rules`): layouts, tags, data encodings, errors, log kinds, event-head function, budgets table, JSON vector writer. The program, svm-tests, `frontier-node` and the JS vectors all consume it | no |
| I-04 | Who gets rent refunds and tips on keeper writes | program: `rent_payer = revealer` (the fee payer); offchain P4: a beneficiary in the data | **v1.1 (I-49): the fee payer pays the rent and gets it back** (`rent_to` = the fee payer), so a keeper's payer pool replenishes itself as accounts close; **tips, march fees and rewards go to `beneficiary [32]`** in the data of Reveal, GatherClash, ResolveFromInputs and SettleTransit. PostAnchor, PostAnchorMulti and PostSeed carry `beneficiary` for the liveness log only (anchors and caches pay no reward) | no |
| I-05 | Relay funding of player actions | offchain P5 (separate rent_payer/funder signer); program prologue (`payer (s,w)`, may equal actor) | The player prologue's **`payer` signer is the funder** of rent and escrow; the relay uses its pool key as both fee payer and `payer`. **v1.1 (I-47):** the Holding's rent is escrowed in the Citizen at FileTicket by the ticket's funder and moved into the Holding by SettleTicket, so `Holding.rent_payer` is the player's funder even when a keeper settles the ticket. **Escrow refunds (bond, bounced tip) go to `Holding.rent_payer`** (M2 adds a per-transit funder) | no |
| I-06 | Reveal data: seal key `k` or salt | program: `k [16]`; web: marchbook never stores `k`, sends the salt | **Reveal and SettleTransit take `salt [32]`** (program §10 cut (b)): `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain37 ‖ salt)`; saves one sha256; the browser never stores `k` or `sigma` | no |
| I-07 | "No Reveal after gather" | closeout CL-23 (latch via read-only ClashInputs absence); program Reveal (no such key) | Reveal takes the canonical ClashInputs key read-only and refuses if it is not absent (`LatchClosed`), and also refuses if the destination Province has `resolved_next > arrive` (defence in depth; the destination is Reveal's last path province anyway) | no |
| I-08 | Tip floor | program `tip_min` 10,006 lamports; closeout CL-22 (priority terms, from the measured Reveal); web (zero tip allowed with confirmation) | **Minimum tip in priority terms**: `tip_min = fees::min_tip_lamports(season.min_reveal_priority_milli, season.reveal_cu_limit, season.reveal_loaded_limit)` = `ceil(p × (limit + 1,320 + 8·⌈L/32,768⌉)) + 2,500` lamports; defaults p = 0.433, limit 26,000 until the wave-3 measurement, **L = 1 MiB until the release `.so` is measured (I-45) → 14,441 lamports** (v1.0: 14,337 with a 64-KiB L). **Zero tip is impossible**; Depart refuses `TipTooLow`; the web drops the zero-tip option | yes (D8 wording "optional tip") |
| I-09 | Genesis re-roll guard | closeout CL-24 (AnnounceSeason ≥ 24 h before, single-use id, bond); program (none) | **AnnounceSeason (0x08) is in M1**; genesis round fixed by the announcement; id single-use; creation bond (default 1 SOL, test SOL in M1) burned on a pre-join abort after the genesis round is public | yes |
| I-10 | Quiet-bell proof | DESIGN §6.3 (open item); program D3 (ArrivalDay + SkipQuiet); offchain P9 (asks for one) | **ArrivalDay `ad‖(P,Q,day)` bitmap + `SkipQuiet(b0, n ≤ 24)`**. Idle day ≈ 6 transactions ≤ 60k CU each instead of 144 × 50–73k | yes (D3) |
| I-11 | Where a departed host's post-clash values live | DESIGN §6.2 (origin Province entry until SettleTransit; GatherClash reads origin Provinces); program D4 (SettleDeparture copies into the Holding; gathers read Holdings) | **Program D4.** Keeper dependency becomes: resolve(origin, depart_bell) → SettleDeparture → gather(dest) | yes (D4) |
| I-12 | Quota-refused and displaced arrivals | offchain P7 (must not be routed); program D5 (rank against the final arrival set in ClashInputs) | **Program D5**: at SettleTransit an arrival not in the final set is compared by `(dep_mass, slot_key)` against the faction's recorded final set: outranked, or its citizen already has a higher-ranked arrival there → **bounce, no loss**; otherwise → routed. No extra write at Reveal. Keepers MUST settle refused arrivals before `CloseClashInputs` (grace 1,008 bells) | yes (D5) |
| I-13 | ResolveFromInputs write set | DESIGN §8.6 rule 2 (exactly one account); program D6 (Province + ClashInputs fate table) | **Program D6**: writes Province and the same province-bell's ClashInputs (delay-only, same stream) | yes (D6) |
| I-14 | Clash kernel optimisation | program D7 (Phase A now; Phase B before exit if gates re-pass); closeout CL-29 (1,200-fill search on RFI) | **Phase A lands in wave 1** (digest-identical, equivalence test promoted). **Phase B is decided at the wave-5 gate** only if the doctrine proxy gate, the nightly 1,500-season band and golden digests re-pass on the new variance stream; otherwise Phase A only | yes (rules change) |
| I-15 | Budgets | DESIGN 650k per clash / 541k RFI; program 460k / 340k; offchain keeper "RFI up to 650k with heap frame" | **ResolveFromInputs 340k (290k with Phase B), worst-case clash 460k (≤ 3 gathers × 40k + 340k), Reveal 26k, SettleTransit 85k.** v1.1 (I-50): **budgets are gates, not liveness limits**. Every gated fill keeps heap ≤ 28 KiB and CU ≤ budget, and the keeper's CU limit is the budgets table (measured max + 5%); a write that still exceeds it is retried up to 1.4M CU and with a 256-KiB heap frame, so a fill worse than the worst found costs money, never a frozen province | no |
| I-16 | Postures in M1 | DESIGN §12 (PosturePDAs in M1 accounts); program D9 (no postures); web (composer behind a flag); offchain (RevealPosture duties, CommitPosture bots) | **No postures in M1.** Every resident fights at Hold; ClashInputs reserves the posture area; `po` seed tag and tags 0x90/0x91 reserved; keeper, bots and web ship no posture code paths (web composer not built) | yes |
| I-17 | Land features | web (pair tickets); offchain bots (pair tickets) | **No pair tickets, no holdings 2–3, no sieges, captures, occupation, diplomacy** in M1 (program D10) | no |
| I-18 | Round rounding | DESIGN §8.5 `round_at` (rounds down); kernel/lab (round up) | **"The first quicknet round scheduled at or after x"** everywhere: `T(b) = first_round_from(bell_end(b))`, `S(b,r) = first_round_from(A + W + Δ)`, ring and genesis seeds `first_round_from(t + 600 + Δ)`; kernel `frontier::beacon` (CL-19/20) | no (M0 close item) |
| I-19 | Ticket challenge window | DESIGN §2.2 ("end of bell b + 1"); program D12; web ("until end of b + 1") | **Superseded by I-47 (v1.1).** A provisional holding becomes final at `round_time(S(b_ticket, r_site)) + 600` **and** only once its province's ticket cohort for `b_ticket` is closed (every ticket filed there at `b_ticket` settled, or 24 bells passed). Displacement has no time condition inside the cohort. Web copy states this | yes (D12, text) |
| I-20 | Payments to drained accounts | — (gap) | **`pay_or_divert`** (program D13): a payment that would leave a non-program recipient below rent exemption goes to the DefencePool, logged `DIVERT` | no |
| I-21 | Keeper classes and pool eligibility | program §3.5 (evidence + eligibility on Reveal, PostAnchor*, PostSeed, Gather, Resolve); closeout CL-30/31a (only window-closing writes eligible); offchain F7 (the split) | **Class W = Reveal** (RevealPosture in M3): escalate ×2/slot to **P_def 2.0**, pool-eligible. **Class D = PostAnchor*, PostSeed, ConsumeGenesisSeed, GatherClash, ResolveFromInputs, SkipQuiet, SettleDeparture, SettleTransit, ArchiveAnchors, and (v1.1, I-47/I-48/I-52) SettleTicket, OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy, ClaimDefence**: escalate to **P_delay 0.5** from the keeper's own budget, never pool-eligible. **Class N** = the rest at a fixed low bid (closes, sweeps, beacon logs, explore and dormancy settlement). ProveBadSeal no longer exists (I-44). Evidence fields are still recorded on anchors and ClashInputs (liveness report); ClaimDefence accepts only ArrivalSlot evidence | yes (D18 re-size) |
| I-22 | ClaimDefence timing | DESIGN §12 (pool in M2); program §6.7 (in M1); offchain Q4 | **ClaimDefence built in M1 (wave 4), Reveal-only eligibility, test SOL.** v1.1 (I-52): SettleTransit keeps a claim-eligible slot until it is claimed or a 6-bell claim grace passes. First item on the cut list if wave 4 slips (then M2) | yes |
| I-23 | Keeper start bid | DESIGN §6.4 ("start at the tip level"); offchain Q3 (zero margin) | Default **p_start = p_tip** (as designed); flag `--peace-start <fraction>` exists, default off | yes |
| I-24 | Owner self-reveal path | web §5 (unsigned Reveal tx through `/gw/f/relay`); offchain F8/§7.3 (`/f/reveal` → keeper loopback) | **The browser sends reveal material, not a transaction:** `POST /gw/f/reveal {holding, transit_slot, plain, salt, ct_hash}` → relay → keeper `POST /v1/reveal`, which builds, signs with a random pool payer and escalates. `/f/relay` refuses any Reveal shape (`UseRevealRoute`) | no |
| I-25 | The "7-day local season" environment | program §14 (real-time Agave ≥ 4 validator); offchain F5/F6 (Mode A LiteSVM-backed node at 20×, drand-replay; Mode R optional) | **Mode A is the exit run** (release `.so`, real historical quicknet rounds, scaled Clock, 20×: 7 game days in 8.4 h). **v1.1 (I-54): slots stay 400 ms of real time at every scale**; the Clock advances 8 game seconds per slot at 20×. **Mode R** (24 h real time on a validator) is a complement if the owner approves | yes |
| I-26 | Ports | offchain 38810–38930; task: 41000–41999 | **All M1 services and runs use 41000–41999** (§10.3). Tests bind `127.0.0.1:0`. **v1.1:** gates check only the ports M1 binds; they never require the reserved ports to be idle (the owner's own services listen on 4185, 4191 and 4194 [measured 2026-09-27]) | no |
| I-27 | "Never retreat" encoding | DESIGN plaintext `retreat_bps u16`; kernel `Option<Bps>`; closeout CL-01 cap `BPS_ONE × 10` (does not fit u16) | **`retreat_bps = 0` means never retreat; `1..=60,000` is the ratio in bps; above 60,000 is an invalid plaintext.** Kernel `clash::RETREAT_MAX_BPS = 60,000`, **defined in `clash.rs` (W1-A) and re-exported by `seal` (v1.1)**; `clash::validate` refuses above it | no (pins a gap) |
| I-28 | Invalid plaintext inside a valid seal | — (gap) | `frontier::seal::Plain::validate` (version 1, reserved bytes zero, `path_len ≤ 32`, unused path bits zero, **every step a hex direction (< 6), `dest_tile < 61`** (v1.2), stance ∈ {Hold, Assault, Flank, Brace}, retreat ≤ 60,000, host and arrive match the transit; `seal-vectors-v1.json` has a case for every refusal and fclient re-exports the kernel's validate). Reveal refuses `BadPlaintext`; **SettleTransit judges it a bad seal (code 5, I-44)**, so it destroys the host. The browser self-audit runs the same validate | no |
| I-29 | Provisional holdings and hosts | program (Holding rewritten in place on displacement; nothing forbids marching while provisional) → a displaced Holding would orphan transit records and freeze gathers (A1) | **Muster, Dissolve, Garrison, Explore and Depart require `holding.state == final`** (`NotFinal`); Harvest/Build/Train are allowed while provisional (lost on displacement). The flip provisional → final is lazy in the prologue of any owner action or SettleTicket once `now ≥ ticket_final_ts` | no |
| I-30 | Genesis rings | program (RingSeeds 0..g "derivable"; no creator named); DESIGN §3.1 (ring 0 Concord neutral, ring 1 Six Seats) | `OpenRing(d ≤ g)` is allowed once the Season is Seeded and creates RingSeed(d) with `seed = sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))`, status seeded (no ConsumeRingSeed). **Rings 0–1 are opened with their sites `reserved`** (no tickets, no camps); tickets start at ring 2 | no |
| I-31 | Holding size | DESIGN 832 B; program 1,280 B | **1,280 B** (four 96-B transit records, full Accruals; v1.1 takes `pool_owed u64` from its reserve). With the 384-B Citizen of v1.1 (I-47) per-player rent is **9,753,600 lamports ≈ 0.0098 SOL** (+24% over DESIGN) | no (noted) |
| I-32 | Depart stamina | kernel (march stamina by path); program (path sealed → charge the maximum `march_stamina(32)`) | **Charge the maximum at Depart** (leaks nothing; simple); the composer shows it | yes |
| I-33 | Onboarding timeline | DESIGN §2.3 ("minute 10+" first report); web §6.3 (first holding 11–21 min, first report 31–41 min) | The web and the DESIGN text state the reachable times; no rule change (W protects C4) | yes (text) |
| I-34 | Doctrine display name | faction "Ember" (id 4) vs doctrine C "Ember" | Working default: doctrine C is displayed as **"Flame"** in M1 UI strings; kernel identifiers unchanged | yes |
| I-35 | Fog | web Q9 | Presentation-only fog **with** a "show everything" switch (every account is public) | yes (low) |
| I-36 | Bot profiles | offchain Q8 | **Copy** `Arch`/`Profile` into `frontier-agents` with a field-for-field equality test; `frontier-sim` is not changed for this (its digests stay stable) | no |
| I-37 | Workspace | program (root member); root lock already carries `solana-program 4.0.0` | `permutation-frontier` and `frontier-abi` join the root workspace; if `cargo metadata --locked` cannot resolve the arkworks 0.5 set together with `permutation-chain`, the program moves to its own workspace (like `svm-tests`) with its own lock, and `build-frontier.sh` handles both (decided by the wave-1 unit, recorded in DECISIONS) | no |
| I-38 | Log format | program (`"PSF1"` record, key fields, new heads); offchain P2 (`"PS2"`, entity 32 B, seq u32); DESIGN §8.3 ("PS2 event", seal in the Depart log) | **§6**: `sol_log_data(["PS2", body])`; body = ver, kind, bell, kind-specific key and payload, then a chain tail `{entity kind, seq u64, head}` per chained entity; `head = sha256(prev_head ‖ le64(seq) ‖ body_without_tail)`. DEPART carries the commitment and the 165-B seal (DESIGN wins) | no |
| I-39 | Error codes the keeper needs | offchain P8 (`SlotChanged`, `WindowClosed`, `NotAnchored`, `SeedNotReady`, `NotAllGathered`, `AlreadyDone`) | Mapped onto the program table plus new codes 51–61 (§5.4): 35 SlotMoved, 12 WindowClosed, 8 NoAnchor, 54 SeedNotReady, 39 NotGathered, 52 AlreadyDone | no |
| I-40 | Join session registration | program (`[session s?]` optional signer); web (session pubkey in Join data) | **Session pubkey in Join's data; no session signature** (the wallet's signature authorises it) | no |
| I-41 | Explore / camps / catalog kernels | program §11 lists them as new rules work, no owner | New modules `frontier::{camp, explore, catalog}` in wave 1 (rules-shared unit). **v1.1: specified from the simulator (I-56); `holding::Effect::Troops` is dropped because training is immediate** | no |
| I-42 | Mutation count in the Phase A report | program.md "5 of 7 caught"; clash-opt README "4 of 5" | Recorded as a discrepancy; the promoted rules test MUST add the untested refund `d > 0` corner and re-run the mutation set; the number in the wave-1 gate report is the one that counts | no |
| I-43 | Province entry storage at ResolveFromInputs (review blocker) | kernel step 2a counts only residents; Muster caps states 1–2 at 48; departed entries (state 3) wait for SettleDeparture | **Storage-aware room.** The program passes the kernel `Occupancy {pending[f] = entries in state 2 of faction f, storage_free = 56 − entries in states 1–3}`; step 2a seeds its per-faction and total counts with the pending musters and admits an arrival only while `total < 48`, `per_faction < 8` and `admitted < storage_free`; the rest **bounce with no loss** in kernel rank order. So `entries after = R + M + D + admitted ≤ 56` always, and a faction never exceeds 8 residents when its musters join. With no pending or departed entries the outcome is bit-identical to v1.0 | yes (outcome change when entries are pending or departed; doctrine proxy gate re-run) |
| I-44 | Bad seals and hosts in transit (review blocker) | DESIGN §6.2 (ProveBadSeal + SealVerdict; SettleTransit after close + 1 bell); v1.0 (ProveBadSeal class N; no action block while a transit is unsettled) | **Settlement is the proof.** SettleTransit takes the logged `commit` and `seal`, checks them against `seal_root`, reads the round-T(arrive) signature from THE anchor or, once archived, from the archive entry (which now stores it), opens the seal with the FO check and judges it (codes 1–5). Settlement cannot skip the proof, the race with a held proof disappears, and a late settlement (after archive) still judges. **ProveBadSeal and SealVerdict are removed** (tag 0x53, seed tag `sv`, magic `PSF1SVRD` and error 14 reserved). **Depart, Dissolve and Explore refuse a host that appears in any transit record of its Holding in state 1–3** (`HostInTransit`); the bad-seal branch removes a destination resident through a `Forfeit` pending op (troops lost at the settle of the current bell; past clashes unchanged) and never returns a routed host | yes (DESIGN §6.2 names ProveBadSeal) |
| I-45 | Loaded-data limit (review blocker) | v1.0 / SP-FEE: `SetLoadedAccountsDataSizeLimit(65,536)` everywhere, measured with a 13,600-B probe program | **Per-kind limit `L(kind) = round_up(programdata_len + 45 + Σ(account data + 64), 32 KiB)`** at the kind's worst account set (SIMD-0186 counts the LoaderV3 programdata), generated into `budgets.rs` from the release `.so` deployed with `--max-len = round_up(1.25 × .so, 4 KiB)`; `Season.reveal_loaded_limit` feeds `tip_min`; working default 1 MiB (+256 cost units instead of +16). Checked by `g01_loaded_limit_*` against the release `.so` in a harness that enforces SIMD-0186 accounting (control test), and once on the installed `solana-test-validator 3.1.9` | no |
| I-46 | Lifecycle gates (review major) | v1.0 GatherClash checks the window and absence only; OpenRing `status ≥ Seeded`; OpenProvince no status | **GatherClash refuses `bell < province.resolved_next`** (`LatchClosed`: the tombstone rule of §4.2 holds for inputs); **SettleTransit uses ClashInputs only with flag 2 (resolved)**; OpenRing, ConsumeRingSeed and OpenProvince require effective status ∈ {Seeded, Running}. G2/G3 get a re-creation test for every closable kind | no |
| I-47 | Ticket finality (review major) | v1.0: displacement only before `final_ts = round_time(S) + 600`, SettleTicket class N | **Ticket cohorts.** FileTicket writes the ≤ 3 Provinces of its sites (they must exist) and counts the ticket in each province's cohort for `ticket_bell`; the SettleTicket that ends a ticket (fresh, displace, exhausted, expired) marks it settled in all of them. A holding becomes final only at `round_time(S) + 600` **and** once its cohort is closed (all settled, or `ticket_bell + 24` bells). Displacement inside the cohort has no time condition; a displaced ticket ends (the web offers a one-tap refile). SettleTicket is class D. To steal a site an attacker must now hold the Province against a D-class keeper for 24 bells (≈ $59k–163k on DESIGN §8.7's model) instead of 10 minutes against a fixed low bid (≈ 0.6 SOL) | yes (D12 text; onboarding unchanged when keepers settle promptly) |
| I-48 | Global accounts on hot paths (review major) | v1.0: `[dpool w]` in every SettleTransit; Frontier and one ProvinceFund written by N-class openings | **No W or D instruction writes the DefencePool except ClaimDefence, whose purpose is to debit it:** routed tips and fees and every divert in SettleTransit go to `Holding.pool_owed` and are swept by the N-class SweepPoolOwed (DESIGN §8.6 rule 2). **ProvinceFund is sharded per wedge** (`pf‖w`, 6 accounts) and carries its wedge's opening counters; OpenProvince and CloseProvince no longer write Frontier; FoldOccupancy folds the shards. OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy and ClaimDefence are class D. Every remaining global writer is priced in the new lock table §8.8 | no |
| I-49 | Keeper payer economics (review major) | v1.0: payers pay rent, refunds go to the beneficiary; band [0.01, 0.05] SOL; one pool | **Rent refunds return to the fee payer that paid** (I-04 revised); **two pools** (reveal ≥ 150, delay ≥ 32); band floors computed from the c4 v3 p99 reveal count (default 0.215 SOL per reveal payer); **≥ 4 funders**, top-ups at rest; the keeper alerts below an effective N of 150 but never stops sending W writes; per-bell effective N is reported and gated in E5 | no |
| I-50 | CU and heap escape hatches (review major) | v1.0: no heap frame ever; CU limit never above the budget; SkipQuiet 60k assumed a cached quiet test; RFI 340k from a lighter write-back | **Allocator 256-KiB aware; keeper heap-frame retry; CU retry ladder to 1.4M**; kernel entries on chain use fixed-capacity arrays at the maxima; **SkipQuiet stops and commits below 40k CU remaining** and its budget is `60k + 30k per recomputed bell`; **RFI measured with the full M1 write-back on all 1,240 fills on SBF** (≈ 5–7 s at 60–90M CU/s), not the top 24 | no |
| I-51 | Abuse paths of a free M1 (review major) | v1.0: invites only at the relay; settle shapes charged to the named citizen; no tip or CU-price ceiling; XFF trusted | **`Season.join_gate`**: when set (playtest preset), Join needs the gate key's co-signature (`JoinGate`); the relay co-signs only with a valid invite. **AnnounceSeason requires the program's upgrade authority** (read from its ProgramData). Relay: settle shapes charged to the requester, nothing charged on a failed simulation, sponsored Depart tip ∈ {tip_min, ⌈1.5 tip_min⌉, 2 tip_min}, CU price 0, `X-Forwarded-For` trusted only from the herald's loopback peer | yes (playtest policy) |
| I-52 | Defence refund evidence lifetime (review major, two issues merged) | v1.0: SettleTransit closes the slot at close + 600; displacement overwrote evidence without a rule for `claimed` | **SettleTransit leaves a claim-eligible, unclaimed slot open** (flag `settled`) until it is claimed or `close + claim_grace` (6 bells) passes; CloseArrivalSlot closes it after that. **Displacement replaces `ev_*` and the beneficiary and resets `claimed = 0`**; the displaced reveal's evidence is not claimable (logged for the liveness report). ClaimDefence is class D | no (inside I-22) |
| I-53 | Day-scale gates without downloads (review blocker) | v1.0: W2–W4 gates need a game day of real rounds; 32 fixture rounds; wasm32 not installed; O-M1-12 unapproved; archive sized ≈ 20k rounds | **`test-beacon` build and `drand-replay --test-key`** (a deterministic local BLS key pinned by its own `QUICKNET_PK_HASH`, marker `PSF_TEST_BEACON_BUILD`, never deployable) for every in-process gate, nightly and rehearsal; real rounds only for W6's real-round runs and the exit. **O-M1-12 is a dated wave-1 ask** with a contiguous archive of ≈ 250k rounds (≈ 1.2 h fetch). Items that need an unapproved install are marked `PENDING-OWNER` in the gate report, not failed and not skipped silently | yes (O-M1-12) |
| I-54 | Mode A time model (review major) | v1.0: slot = 400 ms / scale (20 ms at 20×); latency criteria in game seconds; 24-h announce lead not in the wall time | **Real 400-ms slots at every scale**; the Clock advances `0.4 × scale` game seconds per slot; keeper escalation and contested detection stay per slot. E5 criterion 3 is stated in slots for the 20× run, and the game-second targets are checked in a scale-2 latency run (6 game hours, 3 h). The pre-season runs at scale 2,000 (the 24-h lead in ≈ 43 s) | no |
| I-55 | File ownership (review major) | v1.0: gaps in manifests, locks, the player prologue, the entry codec, svm-tests harness files, `web-lang`/`web-sdk` tests, app routing, G14 | **The integrator owns every Cargo.toml dependency section, every Cargo.lock, every package.json and package-lock.json, `rust-toolchain.toml` files and `.gitignore`s**; units request additions in their notes. The prologue and entry codec live in `frontier-abi` (W1-E). svm-tests area files are assigned per unit. `web-lang`/`web-sdk` tests, `app.mjs` and G14 have named owners. Each wave ends with a two-day integration window in which the integrator may fix any file | no |
| I-56 | Kernels from the simulator (review major) | v1.0: camp/explore/catalog specified by signature only; ≤ 2 camps on sites; Train queued | **`frontier-sim/src/model.rs` and the named `sim.rs` rules are normative** for `catalog`, `camp` and `explore`, with a field-equality test in `frontier-sim/tests/catalog_equality.rs`. **One camp per province on a passable non-site tile** (troops 100–400, daily respawn with chance ½ in provinces with a holding, evaluated at the province's first resolve or skip of a game day), plus an initial camp at OpenProvince for rings ≥ 2 (onboarding; a recorded deviation from the sim); camps no longer reduce `open_sites`. **Train is immediate** (sim). Explore gives `WORKS_EXPLORE` (4) always on the 3 floor explorations, otherwise with chance ½; **no goods in M1** (the sim has none; DESIGN §2.3's goods floor waits) | yes (camps, training time, floor goods) |
| I-57 | Plan and estimate (review majors) | v1.0: W1-E carried the ABI, the program core and the harness in 1.5 weeks; first integration at the W3/W4 gates; estimate 45 vs 47 ew | **7 waves** (§11): W1-E splits into abi (wave 1), program-core and svm-harness (wave 2); an integration unit in wave 4 owns the in-process tests; G14 moves to wave 5. **12 weeks nominal, ≈ 62.5 ew** (§14) | yes (schedule) |
| I-58 | Verifier coverage (review major) | v1.0: V1–V10, T1–T16; `BadSealUnproven` a warning | **V11 land** (ticket scores, cohort displacement rule, terrain and camps from ring seeds, genesis ring seeds), **V12 explore rolls**, **V13 payments** (rule-computed recipients and amounts FAIL; global lamport conservation informational); **`BadSealSurvived` FAIL**; tampers **T17–T22**. (Rebutted in part: SETTLE and EXPLORE_RESULT are already chained records carrying score and finds, so no new digest is needed; the checks were missing, not the data) | no |

---

## 3. Conventions every implementer follows

### 3.1 Repository layout (new paths)

```
Cargo.toml                          root workspace: + "frontier-abi", "permutation-frontier"; exclude + "permutation-frontier/svm-tests", "frontier-node", "frontier-wasm"
                                    [profile.release.package.permutation-frontier] overflow-checks = true
frontier-abi/                       no_std ABI crate (layouts, tags, data, errors, logs, budgets incl. loaded-data limits, vectors, prologue and entry codecs)
  vectors/*.json                    generated; consumed by JS tests and frontier-node
permutation-frontier/               the program (cdylib + lib)
  src/{lib,error,ix,addr,init,clock,events,evidence,heap,prologue}.rs   (prologue: thin wrapper over frontier-abi::prologue)
  src/layout/*.rs                   typed accessors over frontier-abi offsets
  src/crypto/{quick,field,xmd,sys,seal}.rs   ported from SP-V2 unchanged (+ salt-based commitment; the opener SettleTransit runs, I-44; feature `test-beacon` pins the test key, I-53)
  src/proc/{season,beacon,map,citizen,holding,host,reveal,clash,transit,defence}.rs
  svm-tests/                        own workspace, toolchain 1.95.0, litesvm =0.16.0; run.sh; src/{chain,fixtures,records,budget}.rs, src/world/*.rs, src/cover/*.rs, src/ix/*.rs (area files per unit, §11)
scripts/build-frontier.sh           reproducible SBPF v2 build (+ --twice)
scripts/build-wasm.sh               frontier-wasm build (+ --check)
permutation-rules/src/frontier/{beacon,addr,seal,fees,office,camp,explore,catalog}.rs   new pure kernels
permutation-rules/tests/frontier_{bounds,clash_bounds,clash_equiv,shared}.rs
permutation-rules/vectors/{seal-vectors-v1,addr-vectors-v1,clock-vectors-v1}.json
frontier-node/                      own workspace, toolchain 1.95.0
  crates/{fclient,findex,keeper,herald,verify,agents,bots,localnet,drand-replay,stack,itest}/   (itest: in-process integration tests, I-57)
  fixtures/                         recorded mini-seasons for the verifier
frontier-wasm/                      cdylib, outside the root workspace, toolchain 1.95.0
permutation-gateway/src/frontier/   relay (server, app, shapes, quota, payers, invites, keeperlink, routes/*)
permutation-gateway/client/src/frontier/   JS SDK (codec, addresses, seal, fees, budgets, shapes, herald)
permutation-gateway/test/frontier-*.test.mjs, web-frontier-*.test.mjs
permutation-gateway/screens/        Playwright + axe smoke package (own package.json)
permutation-server/web/frontier/    the client (index, spectate, practice, modules, screens, map, wasm/)
permutation-server/web/lang/en-frontier.mjs, en-frontier-play.mjs
permutation-server/web/sdk/frontier/ (generated), web/sdk/vendor/noble/ (vendored, hashed)
docs/frontier/DECISIONS.md, docs/frontier/m0/M0-CLOSE.md, docs/frontier/m1/*-NOTES.md
```

### 3.2 Toolchains and builds

| Tree | Toolchain | Notes |
|---|---|---|
| root workspace (rules, abi, program host tests, permutation-chain) | 1.89.0 (`rust-toolchain.toml`) | `cargo test --locked` |
| SBF build | `solana-cargo-build-sbf 3.1.9`, `--tools-version v1.52 --arch v2` | `export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"`; `RUSTUP_TOOLCHAIN=1.95.0` only if `cargo metadata` meets an edition-2024 crate |
| `permutation-frontier/svm-tests`, `frontier-node`, `frontier-wasm` | 1.95.0 (own `rust-toolchain.toml`) | LiteSVM 0.16 does not build on 1.89 |
| `frontier-sim` | 1.89.0 | unchanged |
| program on the host | 1.89.0 | the program crate MUST build, clippy and test on the host with default features: the custom heap and the entrypoint are `cfg(target_os = "solana")`-gated; `--no-default-features` gives layouts and pure helpers only |
| gateway, web tests | node 20 | `npm test` in `permutation-gateway` |

`scripts/build-frontier.sh` (ported from `build-program.sh`) MUST: check the tool version; build `--arch v2`; refuse the artefact unless ELF `e_flags == 2`, the overflow-panic strings are present, and the marker strings `PSF_ORACLE_BUILD` and `PSF_TRACE_BUILD` are absent; print `file_sha256`, `program_hash`, `e_flags`; with `--twice`, build twice into two target dirs and fail unless the hashes match. Feature builds for tests: `--features trace` → `target/deploy-trace/`, `--features oracle` → `target/deploy-oracle/` (never deployable). The program embeds `RULESET_HASH` (v1.2: `permutation_rules::frontier::ruleset_hash()`: a version constant for **every** module of the frontier tree, the v9 combat tables, the catalog, the doctrine table, the stance damage table, `KERNEL_CONSTANTS` and the seal/camp/explore/office constants; published as the const `frontier_abi::presets::RULESET_HASH`, pinned to the kernel by a test and written to `presets.json`; an algorithm change that keeps every constant MUST bump its module's version) and `QUICKNET_PK_HASH`.

### 3.3 Code rules

- **Program:** no `unwrap`/`expect`/`panic!` outside tests; every arithmetic in the program crate is checked (overflow checks on in the release profile, explicit `checked_*` where a refusal code is wanted: `Overflow` 19); no borsh on hot paths (fixed-offset accessors from `frontier-abi`); every instruction validates its exact account count, owners, magics, signers and writability within its first ≈ 1.5k CU (DESIGN §8.6 rule 6); **never** `CreateAccount`/`CreateAccountWithSeed` (§4.2); **never** a caller-supplied bump; recompute every keyed address a reader trusts; arkworks field ops stay `#[inline(never)]`; no heap-frame request by default: the bump allocator accepts up to 256 KiB (it faults cleanly past the mapped region), every gated fill keeps heap ≤ 28 KiB, and kernel entries called on chain use fixed-capacity arrays at the maxima (48 hosts, 24 arrivals, 12 garrisons, 240 engagements) or a proved static bound (I-50).
- **Kernels (`permutation-rules::frontier`)**: pure, integer-only, `no_std`-compatible, no Solana types; a bound that only refuses invalid input MUST leave every honest outcome bit-identical (proved by the existing suite digests); a change that alters outcomes MUST re-run the doctrine proxy gate and the bot criterion and report the deltas against m0c.
- **Shared constants live once:** layouts/tags/errors/logs in `frontier-abi`; rules constants in `permutation-rules`; JS gets them only through generated files and vectors (`frontier-abi/vectors/*.json` → `permutation-gateway/client/src/frontier/` → `sync-web-sdk.mjs` → `web/sdk/frontier/`). Hand-copied constants are a review failure.
- **Style:** `cargo fmt`, `cargo clippy -- -D warnings` on every Rust tree; JS in the repo's existing style (plain ESM, no build step, no new runtime deps in the web page except the vendored noble tree); docs in plain English with the confidence tags; Japanese UI text inline via `L`/`Lh`/`t`, English in the `en-frontier*.mjs` dictionaries.
- **New dependencies** MUST be pinned with `=`, listed in the unit's notes with a one-line reason and **requested from the integrator**, who applies them to the manifests and lockfiles (§3.4, I-55): expected set `litesvm =0.16.0`, `tokio`, `axum`, `rusqlite` (bundled), `blstrs`, `tlock =0.0.10`, `playwright-core`, `axe-core`, `@noble/curves 1.9.x`, `@noble/hashes 1.8.0` (vendored).
- **Never** edit `permutation-server/web/session.mjs`'s `sessionText`; the Frontier has its own text in `web/frontier/fsession.mjs`.
- v9 stays green: `permutation-chain`, `permutation-server`, the v9 web page and its tests MUST keep passing; changes to shared web files (`lang.mjs`, `map.mjs`) are **additive only**.

### 3.4 Git, commits and approvals

- Each unit works in its own worktree `.claude/worktrees/m1-<unit-id>` on branch `frontier/m1-<unit-id>` cut from the wave base. **Local commits on work branches are allowed. No push, ever, without a new owner approval** (the 2026-09-27 approval covered one push of `codex/frontier` only).
- Commit subject: `Frontier M1 <unit-id>: <what changed>`; body lists the CL/I/gate ids touched; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- The integrator merges the wave's branches **in the order listed in §11** into a local `frontier/m1-integ`, runs the wave gate (§12), then fast-forwards local `codex/frontier`.
- **The integrator owns (I-55)** every `[dependencies]`/`[dev-dependencies]` section of every `Cargo.toml`, the root `Cargo.lock`, `frontier-node/Cargo.toml` and its `Cargo.lock`, `permutation-frontier/svm-tests/Cargo.lock`, `frontier-wasm/Cargo.lock`, every `package.json` and `package-lock.json` (`permutation-gateway`, `permutation-gateway/screens`), every `rust-toolchain.toml` and `.gitignore`. A unit may change them on its own branch to build; at merge the integrator re-applies the requested change on the integration branch (`cargo update -p <crate> --precise <v>` or a manifest edit followed by a build without `--locked`; `npm install --package-lock-only`) and commits it, then gates with `--locked` / `npm ci`.
- **Integration window:** after each wave's merge the integrator has up to two working days to make the gate green, and may change any file for that purpose (one commit per gate item, subject `Frontier M1 integ-W<n>: <gate item>`); ownership of the next wave starts after it. Its cost is in §14. A unit MUST NOT modify a file outside its ownership list; if it needs one, it asks the integrator, who either amends this contract (versioned, §16) or assigns the change to the owning unit.
- **No devnet or mainnet transaction** in M1 before the owner approves the playtest. Read-only public data downloads (historical quicknet rounds) and tool installs (wasm32 target, Agave ≥ 4.0, Playwright Chromium) wait for the owner decision in §15 (O-M1-12, asked on day 1 with dates). Until then every gate item that needs one is reported `PENDING-OWNER` (I-53). `solana-test-validator 3.1.9` is already installed; running it locally on 41080–41089 for the I-45 drill is not a devnet step.
- Unit reports go to `docs/frontier/m1/<unit-id>-NOTES.md` (if a hook refuses a file named `REPORT.md`, `-NOTES.md` is the rule anyway).

### 3.5 Tests every unit writes

- Kernel: unit tests next to the code, integration tests in `permutation-rules/tests/frontier_*.rs` (seeded xorshift, no proptest).
- Program: one LiteSVM test per (instruction, error code) (G13 coverage table, `svm-tests/src/cover/<area>.rs`, one file per area owner), test names prefixed by gate (`g01_…` … `g14_…`) so gates filter by prefix; host unit tests for pure helpers in the program crate.
- Off-chain: in-process tests over `ChainPort::InProcess` (LiteSVM + virtual time) first; binaries only add IO.
- JS: `node --test` files under `permutation-gateway/test/`; no network except `127.0.0.1:0` fixture servers.
- Every vector file is generated by exactly one producer and checked for freshness by exactly one test (`frontier-abi` writes, `web-frontier-codec.test.mjs` / `frontier-vectors.test.mjs` read).

---

## 4. Addresses, initialisation and headers

### 4.1 Address grammar (pinned; kernel `permutation_rules::frontier::addr`)

`addr(kind, key) = create_with_seed(season_pda, seed, program_id) = sha256(season_pda ‖ seed ‖ program_id)`. `seed = tag(2 ASCII) ‖ lowercase-hex(raw)`, `raw` = the key fields little-endian, fixed width, ≤ 15 bytes. The Season is the only PDA: `["season", le64(id)]`, bump found once by AnnounceSeason and stored.

| Account | Tag | Raw key fields (LE) | Raw B | Seed B |
|---|---|---|---|---|
| Frontier | `fr` | — | 0 | 2 |
| RingSeed | `rs` | d u16 | 2 | 6 |
| ProvinceFund ×6 (v1.1: one per wedge, I-48) | `pf` | w u8 | 1 | 4 |
| JoinShard | `js` | faction u8, shard u8 | 2 | 6 |
| BeaconLog | `bl` | region u8 | 1 | 4 |
| DefencePool | `dp` | — | 0 | 2 |
| Citizen | `ct` | `sha256("PSF-CIT" ‖ wallet)[0..15]` | 15 | 32 |
| Holding | `ho` | P i32, Q i32, site u8 | 9 | 20 |
| Province | `pv` | P i32, Q i32 | 8 | 18 |
| ArrivalSlot | `ar` | P i32, Q i32, bell u32, faction u8, i u8 | 14 | 30 |
| ArrivalDay | `ad` | P i32, Q i32, day u32 | 12 | 26 |
| ClashInputs | `ci` | P i32, Q i32, bell u32 | 12 | 26 |
| *SealVerdict (removed in v1.1, I-44; tag reserved)* | `sv` | host_id u64, arrive_bell u32 | 12 | 26 |
| BellAnchor | `an` | bell u32, region u8 | 5 | 12 |
| SeedCache | `sd` | bell u32, region u8, nonce u8 | 6 | 14 |
| AnchorArchive | `aa` | region u8, part u32 (v1.3: `part = bell / 72`, a half day) | 5 | 12 |
| DefenceClaim | `dc` | `sha256("PSF-KPR" ‖ beneficiary)[0..8]`, day u32 | 12 | 26 |
| *PosturePDA (M3, reserved)* | `po` | P i32, Q i32, bell u32, pos u8 (v1.2: SP-V2's grammar) | 13 | 28 |

- **Presence** is authenticated by owner = program, magic, `season_id` and the stored key fields equal to the expected key; readers that need *the* instance (anchors, caches, slots, days, inputs, archives) also recompute the address (≈ 0.5k CU).
- **Absence** = the canonical address, owner = System, data length 0. **Lamports are ignored** (pre-funded = absent).
- `day(b) = b / 144`; `region_of(P,Q)` = kernel `geometry::region_of` (16 regions); `citizen_tag` = first 8 bytes of the Citizen address (little-endian u64), the quota's "citizen" id.
- **Host id (pinned):** `host_id = (province_index(P,Q) as u64) << 44 | (site as u64) << 40 | (gen as u64) << 32 | seq` (province index < 2^20 for R ≤ 128; site < 12; `gen` u8 bumped on every re-founding of the site; `seq` u32 from `Holding.host_seq`). The owner Holding's address is derivable from any host id; a `gen` that differs from the Holding's marks a stranded host.
- Vectors: `permutation-rules/vectors/addr-vectors-v1.json` (seed strings at the extremes; i32::MIN/MAX coordinates, u32::MAX bell, u64::MAX host) and `frontier-abi/vectors/addresses.json` (full addresses for a fixed season PDA and program id). They MUST match SP-V2 `acct.rs` byte for byte for `an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa`.

### 4.2 Initialisation, closing and payments

- `init_with_seed(payer, target, seed, space)`: require absent; transfer `max(0, rent(space) − target.lamports)` from `payer` (System Transfer; payer signs); `AllocateWithSeed{base: season_pda, seed, space, owner: program}` signed by the Season PDA. The Season itself uses the PDA variant (`Allocate` + `Assign` with `invoke_signed`).
- `init_funded(fund, target, seed, space)`: as above, but the shortfall moves by direct lamport arithmetic from a **program-owned** fund (ProvinceFund for Provinces).
- `close_to(target, recipient)`: `resize(0)`, `assign(System)`, move all lamports with `pay_or_divert`. **v1.3:** `close_to` does not log; **every closing instruction emits `CLOSE` (§6) itself, before calling `close_to`**, carrying the final seq and head for chained accounts (W2-A's pinned split; the hand-over rule is in `permutation-frontier/src/proc/mod.rs`).
- `pay_or_divert(from, to, amount, sink)`: if `to` is not program-owned and `to.lamports + amount < rent_exempt(to.data_len)`, credit `sink` instead and log `DIVERT`. Applies to tips, march fees, bonds, settlement rewards and rent refunds. **The sink is `Holding.pool_owed`** (lamports that stay in the Holding, swept later by SweepPoolOwed) in SettleTransit and every W, D or player instruction; the DefencePool is a sink only in N-class instructions that already list it (SweepPoolOwed, ReleaseDormant, CloseHolding, season-end closes). **No W or D instruction writes the DefencePool except ClaimDefence** (which exists to debit it, off every critical path) (DESIGN §8.6 rule 2; I-48). A rent refund of a whole account (≥ `rent(0)` = 650,240 lamports, v1.2: v1.1's 890,880 was the old per-byte rate) never diverts.
- **Tombstones** (a closed address is absent again): anchors — AnchorArchive `tombstone` bit set **before** the anchor closes; slots and days — the reveal window of their bell is closed for good and the latch holds; inputs — the Province has resolved or skipped past the bell **and GatherClash refuses any bell below `resolved_next`** (I-46), so a closed or never-needed ClashInputs cannot be created again; provinces, holdings and citizens — their creating instruction checks the season status (I-46). G2/G3 hold a re-creation test for every closable kind.
- **Rent** = `(128 + size) × 5,080` lamports.

### 4.3 Headers and event chains

**Chained header H (64 B)** — Season, Frontier, JoinShard, Citizen, Holding, Province, ClashInputs:

| Off | Size | Field |
|---|---|---|
| 0 | 8 | magic |
| 8 | 8 | season_id u64 |
| 16 | 2 | layout_version u16 (= 1) |
| 18 | 6 | reserved (0) |
| 24 | 8 | event_seq u64 (0 before the creation record) |
| 32 | 32 | event_head (32 zero bytes before the creation record) |

**Short header SH (16 B)** — every other account: magic (8) + season_id u64 (8). Short accounts are immutable or append-only and are verified by content against the logs.

Magics (8 ASCII bytes): `PSF1SEAS` Season, `PSF1FRNT` Frontier, `PSF1RING` RingSeed, `PSF1PFND` ProvinceFund, `PSF1JSHD` JoinShard, `PSF1BLOG` BeaconLog, `PSF1DPOL` DefencePool, `PSF1CITZ` Citizen, `PSF1HOLD` Holding, `PSF1PROV` Province, `PSF1ASLT` ArrivalSlot, `PSF1ADAY` ArrivalDay, `PSF1CLIN` ClashInputs, `PSF1SVRD` (reserved; SealVerdict removed), `PSF1ANCH` BellAnchor, `PSF1SEED` SeedCache, `PSF1ARCH` AnchorArchive, `PSF1DCLM` DefenceClaim.

---
## 5. Program interface (normative)

### 5.1 Clock and bell model (kernel `frontier::beacon`, CL-19/20)

All rule times come from the Clock sysvar's `unix_timestamp` on chain and from `GameClock` (§8.1) off chain — **never the wall clock**. Integers: bells u32, seconds i64, rounds u64.

| Quantity | Definition | Kernel fn |
|---|---|---|
| bell of t | `b(t) = ⌊(t − genesis_ts) / 600⌋`, t ≥ genesis_ts | `beacon::bell_at` (= `travel::bell_at`) |
| bell bounds | `bell_start(b) = genesis_ts + 600 b`, `bell_end(b) = bell_start(b + 1)` | `beacon::{bell_start, bell_end}` |
| round time | `round_time(r) = drand_genesis + (r − 1) × 3` (quicknet) | `beacon::round_time` |
| first round at or after t | smallest r with `round_time(r) ≥ t` | `beacon::first_round_from` |
| tlock round | **`T(b) = first_round_from(bell_end(b))`** | `beacon::tlock_round(clock, genesis_ts, b)` |
| anchor time | `A(b, r)` = Clock at the creation of THE BellAnchor `(b, r)` | stored in anchor and archive (`a_off = A − bell_end(b)`) |
| window | `W(b) = window_next` if `b ≥ window_from_bell`, else `reveal_window` (600–1,800 s; a change needs ≥ 144 bells' notice) | `beacon::window` |
| reveal close | `close(b, r) = A(b, r) + W(b)` | `beacon::reveal_close` |
| seed round | **`S(b, r) = first_round_from(close(b, r) + Δ)`**, `Δ = seed_margin ≥ 60` | `beacon::seed_round` |
| genesis round | **`first_round_from(t_create_min + 600 + Δ)`**, `t_create_min` from AnnounceSeason (CL-24) | `beacon::genesis_seed_round` |
| ring seed round | **`first_round_from(t_open + 600 + Δ)`** | `beacon::ring_seed_round` |
| roster freeze, posture close | `bell_start(b)` (wall-clock boundaries, never derived from T(b)) | — |
| day | `day(b) = b / 144` | — |
| resolved | `Province.resolved_next` = first bell neither resolved nor skipped | — |
| resident action at bell b | allowed iff `resolved_next + 1 ≥ b` (resolved through b − 2) and `holding.state == final` (I-29); a named host must not appear in any transit record of its Holding in state 1–3 (`HostInTransit`, I-44); effects pending until after b's clash | kernel `Host::check_issue` |
| arrival bell | `arrive ∈ [max(depart_bell + 2, earliest_arrival_bell(depart_ts, secs)), depart_bell + 72]` | `travel::check_arrival_bell` |
| reveal open | THE anchor present: `now < close ∧ BeaconLog(r).latest_round < S(A)`; anchor absent: archive `(r, day)` does not tombstone `arrive`; **and** ClashInputs `(dest, arrive)` absent **and** `dest.resolved_next ≤ arrive` (latch, I-07) | `beacon::reveal_open` + program |
| gather / skip allowed | `now ≥ close` (anchor or archive entry) | — |
| resolve allowed | gathered, and a SeedCache of THE anchor with round `S(A)` (or the archive's seed) | — |
| settle transit | `now ≥ close(arrive, r_dest) + 600` and `dest.resolved_next > arrive`; the seal is opened and judged inside (I-44) | — |
| ticket final | `now ≥ round_time(S(ticket_bell, r_site)) + 600` (stored as `Holding.final_ts`) **and** the Province's cohort for `ticket_bell` is closed: `settled == filed`, or `now_bell ≥ ticket_bell + 24` (I-47) | — |
| archive | `now ≥ A + archive_after` (172,800 s); tombstone before close | — |
| genesis_ts | `round_time(genesis_round) + 600` (set by CreateSeason) | — |

Economic timers (production, dormancy 5 days, release 10 days, shield, vigil) use Clock time; combat timers (stamina refill, cooldown) count resolved bells.

### 5.2 Accounts: summary

| Account | Address | Size B | Rent (lamports) | Created by / paid by | Written by | Closed by → refund |
|---|---|---|---|---|---|---|
| Season | PDA `["season", id]` | 2,048 | 11,054,080 | AnnounceSeason / authority (+ creation bond held in it) | CreateSeason, ConsumeGenesisSeed, SetWindowSchedule, EndSeason, AbortSeason | never closed; CloseSeason shrinks it to a 128-B tombstone → authority |
| Frontier | `fr` | 512 | 3,251,200 | CreateSeason / authority | OpenRing, FoldOccupancy (v1.1: openings and closes write the wedge's ProvinceFund instead, I-48) | CloseSeason → authority |
| RingSeed | `rs‖d` | 128 | 1,300,480 | OpenRing / caller | ConsumeRingSeed, OpenProvince | CloseSeason → payer |
| ProvinceFund ×6 | `pf‖w` | 128 | 1,300,480 + fund | CreateSeason / authority (`pfund_initial / 6` each); top-ups by anyone | OpenProvince, CloseProvince (their wedge) | CloseSeason → authority |
| JoinShard ×48 | `js‖f,s` | 256 | 1,950,720 | InitShards / authority | Join, SettleTicket, ReleaseDormant | CloseSeason → authority |
| BeaconLog ×16 | `bl‖r` | 128 | 1,300,480 | InitBeaconLogs / authority | PostBeacon | CloseSeason → authority |
| DefencePool | `dp` | 256 | 1,950,720 + escrow | CreateSeason / authority (20 SOL default, test SOL in M1) | SweepPoolOwed, ReleaseDormant, CloseHolding and season-end closes credit; ClaimDefence debits; never written by a W or D instruction other than ClaimDefence (I-48) | CloseSeason → authority |
| Citizen | `ct‖tag` | 384 | 2,600,960 (+ ticket escrow) | Join / `payer` (relay) | own player instructions, SettleTicket, ReleaseDormant, SettleExplore | CloseCitizen (after end + 72 h) → `rent_payer` (escrow → `ticket_funder`) |
| Holding | `ho‖P,Q,site` | 1,280 | 7,152,640 | SettleTicket / the ticket funder's escrow in the Citizen (I-47) | owner actions, SettleTicket, SettleDeparture, SettleTransit, SettleExplore, ReleaseDormant, SweepPoolOwed | ReleaseDormant, CloseHolding → `rent_payer` (`pool_owed` → DefencePool) |
| Province | `pv‖P,Q` | 4,096 | 21,457,920 | OpenProvince / its wedge's ProvinceFund | FileTicket (cohort), SettleTicket, Build (walls), Muster, Dissolve, Garrison, Depart, Explore, SettleDeparture, SettleTransit, ResolveFromInputs, SkipQuiet, ReleaseDormant, DisbandStranded | CloseProvince → its ProvinceFund |
| ArrivalSlot | `ar‖P,Q,b,f,i` | 160 | 1,463,040 | Reveal / fee payer | Reveal (displacement overwrites), SettleTransit (`settled` flag), ClaimDefence (`claimed`) | SettleTransit, CloseArrivalSlot → `rent_to` (the fee payer that paid, I-49) |
| ArrivalDay | `ad‖P,Q,day` | 96 | 1,137,920 | first Reveal of a province-bell in the day / fee payer | Reveal (bit set) | CloseArrivalDay → `rent_to` |
| ClashInputs | `ci‖P,Q,b` | 1,280 | 7,152,640 | first GatherClash / fee payer | GatherClash, ResolveFromInputs, SettleTransit | CloseClashInputs → `rent_to` |
| BellAnchor | `an‖b,r` | 144 | 1,381,760 | PostAnchor(Multi) / fee payer | — | ArchiveAnchors → `rent_to` (the fee payer) |
| SeedCache | `sd‖b,r,n` | 144 | 1,381,760 | PostSeed / fee payer | — | CloseSeedCache → `rent_to` (the fee payer) |
| AnchorArchive | `aa‖r,part` (v1.3: half day, `part = bell / 72`) | 6,144 | 31,861,760 | first ArchiveAnchors of the region-half-day / payer | ArchiveAnchors | CloseSeason → `rent_to` |
| DefenceClaim | `dc‖kpr,day` | 128 | 1,300,480 | first ClaimDefence of the keeper-day / keeper | ClaimDefence | CloseSeason → `beneficiary` |

Per-player refundable rent: Citizen + Holding = **9,753,600 lamports ≈ 0.0098 SOL** (I-31, I-47). AnchorArchives of a 7-day season: 16 × 16 × 31.9M ≈ 8.2 SOL of refundable float (v1.1 stores each bell's signature, I-44; v1.3 halves each archive, which doubles their count).

### 5.3 Byte layouts

All integers little-endian; "rsv" = reserved, zero. Each layout file in `frontier-abi/src/layout/` has a const assertion that its last field ends at or before the size, and the offsets below are copied into `frontier-abi` exactly (the abi's vector test prints every offset; `web-frontier-codec.test.mjs` and `fclient` check them).

**Season (2,048)** — H(0..64), then:

| Off | Field | Off | Field |
|---|---|---|---|
| 64 | status u8 (0 Announced, 1 Created, 2 Seeded, 3 Running*, 4 Ended, 5 Closed, 6 Aborted) | 65 | bump u8 |
| 66 | regions u8 (16) | 67 | genesis_ring g u8 (≤ 4) |
| 68 | r_max u16 (≤ 128) | 70 | office_terms_per_wallet u8 (1, D23; unused in M1) |
| 71 | postures_enabled u8 (0 in M1) | 72 | authority [32] |
| 104 | ruleset_hash [32] | 136 | rules_version u16 (10) |
| 138 | program_version u16 | 140 | bell_secs u32 (600) |
| 144 | genesis_ts i64 | 152 | created_ts i64 |
| 160 | join_close_bell u32 (3,024) | 164 | end_bell u32 (4,032; 1,008 for a 7-day season) |
| 168 | drand_genesis i64 (1,692,803,367) | 176 | drand_period u32 (3) |
| 180 | network u8 (2 = quicknet), rsv[3] | 184 | quicknet_pk_hash [32] |
| 216 | reveal_window u32 | 220 | seed_margin u32 (≥ 60) |
| 224 | window_next u32 | 228 | window_from_bell u32 (u32::MAX = none) |
| 232 | genesis_round u64 | 240 | genesis_seed [32] |
| 272 | archive_after u32 (172,800) | 276 | min_lead u8 (2), max_lead u8 (72), transit_slots u8 (4), rsv u8 |
| 280 | march_fee u64 (10,000) | 288 | seal_bond u64 (20,000) |
| 296 | min_reveal_priority_milli u32 (433) | 300 | reveal_cu_limit u32 (26,000 until measured) |
| 304 | bucket_rate_per_h u16 (30), bucket_burst u16 (60) | 308 | defence_cap_milli u32 (2,000) |
| 312 | lateness_slots u8 (4), rsv[3] | 316 | theta_early_bps u16 (5,500), theta_late_bps u16 (6,500) |
| 320 | theta_switch_secs u32 (259,200) | 324 | reserve_bps u16 (200), extra_free_bps u16 (2,000) |
| 328 | clash_close_grace u32 (1,008 bells) | 332 | camp_regrow_bells u32 (144) |
| 336 | params_hash [32] (announced) | 368 | t_create_min i64 |
| 376 | announced_ts i64 | 384 | creation_bond u64 (lamports held) |
| 392 | payout_params_hash [32] (`PayoutParams::validate_for_season`, CL-11) | 424 | shade_auditor [32] (zero in M1) |
| 456 | dormant_after_secs u32 (432,000) | 460 | release_after_secs u32 (864,000) |
| 464 | pfund_initial u64 (split over 6 wedge funds) | 472 | dpool_initial u64 |
| 480 | reveal_loaded_limit u32 (I-45; multiple of 32,768) | 484 | join_gate [32] (I-51; zero = open) |
| 516..1,024 | reserved M2/M3 (mint, vaults, roster_root, policy_code_hash, γ, prices) | 1,024..2,048 | reserved |

\* Running is never stored: status Seeded with `now ≥ genesis_ts` **is** Running; every instruction computes the effective status.

**Frontier (512)** — H · 64 rings_opened u16 (count; ring d opens when `d == rings_opened`) · 66 rsv u16 · 68 last_ring_open_bell u32 · 72 last_ring_open_ts i64 · 80 fold_bell u32 · 84 fold_part u8, rsv[3] · 88 open_sites u32 (folded) · 92 occupied_sites u32 (folded) · 96 provinces_opened u32 (folded; the live counts are in the ProvinceFund shards, I-48) · 100 wedge_open [6] u32 (folded) · 124 wedge_occupied [6] u32 · 148 acc_occupied u32 · 152 acc_wedge [6] u32 · 176..512 rsv.

**RingSeed (128)** — SH · 16 d u16 · 18 status u8 (1 requested, 2 seeded) · 19 rsv · 20 opened_bell u32 · 24 t_open i64 · 32 round u64 · 40 seed [32] · 72 provinces_created u16 · 74 rsv[6] · 80 payer [32] · 112..128 rsv.

**ProvinceFund (128, one per wedge, v1.1)** — SH · 16 wedge u8 · 17 rsv[3] · 20 provinces_opened u32 (live, this wedge) · 24 funded_total u64 · 32 spent_total u64 · 40 open_sites u32 (this wedge) · 44 provinces_funded u32 · 48..128 rsv. Lamports above rent are the fund.

**JoinShard (256)** — H · 64 faction u8 · 65 shard u8 · 66 rsv[2] · 68 members u32 · 72 holdings u32 (live first holdings, provisional or final) · 76 final_holdings u32 · 80 holdings_by_wedge [6] u32 · 104 released u32 (cumulative) · 108..256 rsv (M2: fee and stake sums).

**BeaconLog (128)** — SH · 16 region u8 · 17 rsv[7] · 24 latest_round u64 · 32 posted_ts i64 · 40 posted_slot u64 · 48 sig48 [48] · 96 beneficiary [32].

**DefencePool (256)** — SH · 16 paid_total u64 · 24 diverted_total u64 · 32 per_bell_region_cap u64 · 40 per_keeper_day_cap u64 · 48 claims u64 · 56..256 rsv. Lamports above rent are the escrow.

**Citizen (384, v1.1)** — H, then:

| Off | Field |
|---|---|
| 64 | wallet [32] |
| 96 | session [32] (zero = none) |
| 128 | session_expiry i64 |
| 136 | faction u8 · 137 flags u8 (1 joined, 4 first holding final, 8 provisional holding, 16 refugee) · 138 holdings_n u8 · 139 explores_floor_left u8 (3) |
| 140 | join_bell u32 |
| 144 | join_shard u8 · 145 rsv · 146 vigil_start_min u16 · 148 vigil_next_min u16 · 150 rsv u16 |
| 152 | vigil_from_ts i64 (first UTC midnight ≥ request + 24 h, CL-09) |
| 160 | bucket_milli u32 · 164 bucket_t u32 (seconds since genesis_ts) |
| 168 | holding [3] × {P i16, Q i16, site u8, gen u8} |
| 186 | office_terms_used u8 (D23, reserved, 0 in M1) · 187 rsv |
| 188 | ticket_bell u32 (u32::MAX none) |
| 192 | ticket_sites [3] × {P i16, Q i16, site u8} · 207 ticket_next u8 |
| 208 | citizen_tag u64 |
| 216 | last_action_ts i64 |
| 224 | works u64 (counted only) |
| 232 | explores u32 · 236 arrivals u32 |
| 240 | rent_payer [32] |
| 272 | ticket_escrow u64 (the Holding rent held for the open ticket, I-47) |
| 280 | ticket_funder [32] (who funded it; receives it back) |
| 312..384 | rsv (M2: fee, stake, stake bell, counted laurels, path facts) |

**Holding (1,280)** — H, then:

| Off | Size | Field |
|---|---|---|
| 64 | 6 | P i16, Q i16, site u8, gen u8 |
| 70 | 1 | tile u8 |
| 71 | 1 | state u8 (0 none, 1 provisional, 2 final, 3 released) |
| 72 | 32 | owner_citizen (Citizen address) |
| 104 | 8 | ticket_score u64 |
| 112 | 4 | faction u8, order u8 (1), tier u8, flags u8 (1 dormant-flag cache) |
| 116 | 4 | ticket_bell u32 |
| 120 | 8 | founded_ts i64 |
| 128 | 4 | founded_day u32 |
| 132 | 4 | host_seq u32 |
| 136 | 8 | last_owner_action i64 |
| 144 | 8 | shield_until i64 |
| 152 | 320 | stores [8] × Accrual {value i64, rate i64, cap i64, t0 i64, frac i64} |
| 472 | 64 | production [8] i64 |
| 536 | 64 | upkeep [8] i64 |
| 600 | 96 | queue [4] × {done_at i64, kind u8, arg u8, rsv[6], delta i64} |
| 696 | 8 | walls u32, rsv u32 |
| 704 | 8 | walls_committed_before i64 |
| 712 | 8 | food_shortfall i64 |
| 720 | 32 | reserve [8] u32 (trained troops by unit) |
| 752 | 32 | delegate [32] (M3, zero) |
| 784 | 384 | transit [4] × 96 B |
| 1168 | 24 | explore {bell u32, P i16, Q i16, tiles [2] u8, host u64, state u8, rsv[5]} |
| 1192 | 8 | escrow u64 (tips + fees + bonds held) |
| 1200 | 32 | rent_payer [32] (funder; escrow and rent refunds, I-05) |
| 1232 | 8 | final_ts i64 (I-19) |
| 1240 | 8 | pool_owed u64 (lamports in this Holding owed to the DefencePool: routed tips and fees, diverted payments; swept by SweepPoolOwed, I-48) |
| 1248 | 32 | rsv |

Transit record (96): 0 state u8 (0 free, 1 departed, 2 values settled, 3 values settled — destroyed at origin, troops 0) · 1 unit u8 · 2 faction u8 · 3 origin_tile u8 · 4 origin P i16 · 6 origin Q i16 · 8 host_id u64 · 16 depart_bell u32 · 20 arrive_bell u32 · 24 depart_ts i64 · 32 dep_mass u32 · 36 march_stamina u16 · 38 dealt_bps u16 · 40 troops_after u32 · 44 stamina_after u16 · 46 ready_bell_off u16 · 48 seal_root [32] · 80 tip u64 · 88 flags u8 (1 fee escrowed, 2 bond escrowed) · 89..96 rsv.

**Province (4,096)** — H, then:

| Off | Size | Field |
|---|---|---|
| 64 | 8 | P i16, Q i16, ring u16, wedge u8, region u8 |
| 72 | 4 | resolved_next u32 |
| 76 | 4 | opened_bell u32 |
| 80 | 8 | relations u64 (0 in M1) |
| 88 | 32 | last outcome digest |
| 120 | 4 | n_entries u8, n_sites_used u8, quiet_ok u8 (cached `is_quiet` for `roster_epoch`), rsv |
| 124 | 4 | roster_epoch u32 |
| 128 | 61 | terrain class per tile |
| 189 | 61 | resource per tile (0 none, 1 + `TileResource`) |
| 250 | 14 | sites [12] tile u8, site_count u8, rsv |
| 264 | 32 | passable_mask u64, rough_mask u64, road_mask u64 (M3), explored_mask u64 |
| 296 | 768 | site mirror [12] × 64 B |
| 1064 | 2,688 | entries [56] × 48 B (≤ 48 in the roster, ≤ 8 per faction; the rest muster-pending or departed) |
| 3752 | 32 | last resolve summary {bell u32, engagements u32, arrivals u8, destroyed u8, bounced u8, rsv, resolver [8], rsv} |
| 3784 | 16 | camp {tile u8, state u8 (0 none, 1 present), rsv u16, troops u32, next_check_day u32, gen u32} (I-56) |
| 3800 | 64 | ticket_cohorts [8] × {bell u32, filed u16, settled u16} (I-47) |
| 3864 | 232 | rsv |

Site mirror (64): 0 state u8 (0 free, 1 holding, 2 unused since v1.1 — camps sit on non-site tiles, I-56, 3 released-free, 4 reserved — rings 0–1) · 1 faction u8 (owner, or NEUTRAL = 6) · 2 order u8 · 3 tier u8 · 4 gen u8 · 5 rsv[3] · 8 garrison u32 · 12 pend0_bell u32 · 16 pend0_delta i64 · 24 pend1_bell u32 · 28 rsv · 32 pend1_delta i64 · 40 walls_committed u32 · 44 wall_item0 {effective_bell u32, delta u32} · 52 wall_item1 · 60 shield_until_bell u32 (camps: regrow_bell).

Entry (48): 0 id u64 · 8 faction u8 · 9 unit u8 · 10 tile u8 · 11 state u8 (0 free, 1 roster, 2 muster_pending, 3 departed) · 12 troops u32 · 16 stamina_value u16 · 18 dealt_bps u16 · 20 stamina_bell u32 · 24 ready_bell u32 · 28 from_bell u32 · 32 pend_bell u32 · 36 pend_op u8 (0 none, 1 Spend, 2 Split, 3 Absorb, 4 AbsorbedInto, 5 Leave, 6 Forfeit — bad seal, troops lost at the settle, I-44) · 37 op_a u8 · 38 op_b u16 · 40 op_troops u32 · 44 op_ref u32.

**ArrivalSlot (160)** — SH · 16 P i16 · 18 Q i16 · 20 bell u32 · 24 faction u8 · 25 i u8 · 26 unit u8 · 27 stance u8 · 28 tile u8 · 29 flags u8 (1 settled, I-52; **2 created_day** (v1.2): the Reveal that wrote this evidence also created the ArrivalDay; set by Reveal, cleared by a displacement with the other `ev_*` fields; `fees::Evidence::created_day`) · 30 retreat_bps u16 (0 = never) · 32 host_id u64 · 40 citizen_tag u64 · 48 dep_mass u32 · 52 dealt_bps u16 · 54 rsv u16 · 56 beneficiary [32] (tip recipient; replaced on displacement) · 88 rent_to [32] (the creating Reveal's fee payer, I-49; unchanged on displacement) · 120 ev_slot u64 · 128 ev_price u64 (µlamports/CU) · 136 ev_limit u32 · 140 ev_loaded u32 · 144 claimed u8 (reset to 0 on displacement, I-52) · 145..160 rsv.

**ArrivalDay (96)** — SH · 16 P i16 · 18 Q i16 · 20 day u32 · 24 bits [18] (bit `b mod 144` set ⇔ an ArrivalSlot of `(P,Q,b)` was created) · 42 rsv[22] · 64 rent_to [32].

**ClashInputs (1,280)** — H · 64 P i16 · 66 Q i16 · 68 bell u32 · 72 arrivals_mask u32 (24 bits gathered) · 76 rsv u32 · 80 posture_mask u64 (M3) · 88 flags u8 (1 no-arrivals proven by ArrivalDay, 2 resolved) · 89 n_present u8 · 90 rsv[2] · 92 settled_mask u32 · 96 arrivals [24] × 40 B · 1056 postures [60] × 2 B (M3, zero) · 1176 resolver [32] (resolve beneficiary; march fees) · 1208 ev_slot u64 · 1216 ev_price u64 · 1224 ev_limit u32 · 1228 resolved_ts u32 (s since genesis) · 1232 rent_to [32] (the first gather's fee payer, I-49) · 1264..1280 rsv. Arrival record (40): 0 host_id u64 · 8 citizen_tag u64 · 16 dep_mass u32 · 20 troops u32 (after origin clash) · 24 stamina u16 · 26 retreat u16 · 28 dealt u16 · 30 faction u8 · 31 unit u8 · 32 tile u8 · 33 stance u8 · 34 present u8 · 35 fate u8 (0 none, 1 Stays, 2 Withdrew, 3 Bounced, 4 Retreated, 5 Destroyed) · 36 troops_after u32.

**SealVerdict** — removed in v1.1 (I-44). Its codes are kept as SettleTransit's seal codes: 0 valid, 1 FO check failed, 2 bad point / not in subgroup, 3 *reserved, never emitted in M1* (v1.3: a seal to another round cannot be told from any other FO failure, so it is code 1; W1-C's `wrong_round` vector expects `fo_fail`), 4 commitment mismatch after opening, 5 plaintext invalid (I-28).

**BellAnchor (144)** — SH · 16 bell u32 · 20 region u8 · 21 net u8 · 22 rsv[2] · 24 round T u64 · 32 A i64 · 40 slot u64 · 48 sig48 [48] · 96 rent_to [32] (the fee payer; the data's beneficiary is logged only, I-49) · 128 ev_price u64 · 136 ev_limit u32 · 140 rsv u32.

**SeedCache (144)** — SH · 16 bell u32 · 20 region u8 · 21 nonce u8 · 22 rsv[2] · 24 round S u64 · 32 seed [32] · 64 anchor_key [32] · 96 A i64 · 104 slot u64 · 112 rent_to [32] (the fee payer).

**AnchorArchive (6,144, v1.3)** — SH · 16 region u8 · 17 rsv[3] · 20 part u32 · 24 tombstone bits [9] · 33 archived bits [9] · 42 rsv[22] · 64 entries [72] × {a_off u32, seed [32], sig [48]} (84 B; entry of bell b at `b mod 72`; `sig` = THE anchor's round-T(b) signature, so SettleTransit can judge a seal after the anchor closes, I-44) · 6,112 rent_to [32]. One archive per region and **half day** (`part = bell / 72`): v1.1's 12,192-B day archive exceeded the 10,240 B a program can allocate by CPI in one instruction (`MAX_PERMITTED_DATA_INCREASE`, W2-A F2 [measured]), so ArchiveAnchors could not create it; 6,144 B keeps a single-step creation.

**DefenceClaim (128)** — SH · 16 beneficiary [32] · 48 day u32 · 52 rsv u32 · 56 claimed u64 · 64 count u32 · 68..128 rsv.

### 5.4 Error codes

`ProgramError::Custom(code)`; codes are stable forever; one svm test per (instruction, code) pair (G13).

| Code | Name | Code | Name |
|---|---|---|---|
| 1 | BadData | 30 | TransitState |
| 2 | BadAccount (owner/magic/season/key) | 31 | ArrivalBell (kernel `TravelError`) |
| 3 | BadAddress (not canonical) | 32 | Path (not adjacent, impassable, too long, > 4 provinces, wrong end) |
| 4 | NotSigner / Auth | 33 | CommitMismatch |
| 5 | WrongStatus (season) | 34 | QuotaRefused (kernel `SlotRefusal`) |
| 6 | RulesetMismatch | 35 | SlotMoved (keeper: re-read slots, retry) |
| 7 | WrongRound | 36 | NeedArrivalDay |
| 8 | NoAnchor | 37 | Shielded |
| 9 | Crypto (BadHint / BadPoint / PairingFailed; sub-code in log) | 38 | DepartureUnsettled |
| 10 | Capacity | 39 | NotGathered |
| 11 | SiteTaken | 40 | OutOfOrder (bell ≠ resolved_next) |
| 12 | WindowClosed | 41 | NotQuiet |
| 13 | TooEarly | 42 | InputsOpen |
| 14 | (reserved; was SealValid) | 43 | NotEligible / AlreadyClaimed (defence) |
| 15 | Kernel (sub-code in log) | 44 | FoldStale |
| 16 | Archived (tombstoned bell) | 45 | TicketState |
| 17 | Bucket | 46 | NotDormant / HasTransits |
| 18 | NotTopLevel | 47 | Explored |
| 19 | Overflow | 48 | SessionExpired |
| 20 | NotOwner | 49 | WrongRegion |
| 21 | Insufficient (resources, reserve, lamports) | 50 | Aborted |
| 22 | QueueFull | **51** | **TipTooLow** (I-08) |
| 23 | NoTicket | **52** | **AlreadyDone** (idempotent repeat; keeper treats as success) |
| 24 | NotFinal (holding provisional, I-29) | **53** | **LatchClosed** (ClashInputs exists or bell resolved, I-07) |
| 25 | TooManyAccounts | **54** | **SeedNotReady** |
| 26 | NotResident | **55** | **BadPlaintext** (I-28) |
| 27 | ProvinceFull | **56** | **Announce** (lead < 24 h, params hash mismatch, id used) |
| 28 | HostBusy | **57** | **ReservedSite** (rings 0–1) |
| 29 | Cooldown / NoStamina | **58** | **HostInTransit** (I-44) |
| — | | **59** | **JoinGate** (I-51) |
| — | | **60** | **CohortFull** (I-47) |
| — | | **61** | **TipNotPreset** (relay only; never a program code) — reserved so the web maps one table |
| — | | 99 | NotImplemented (dev stubs only; `RELEASE_CHECK=1` fails if any path returns it) |

Keeper mapping (offchain P8): `SlotChanged` = 35, `WindowClosed` = 12 / 16 / 53 (stop), `NotAnchored` = 8, `SeedNotReady` = 54, `NotAllGathered` = 39, `AlreadyDone` = 52. PostAnchor/PostSeed on a present account return **success as a no-op** (≈ 1.75k CU, SP-V2), not 52.

### 5.5 Instruction tag map, classes and budgets

Instruction data: byte 0 = tag, then the listed fields, little-endian, fixed width, no borsh. **Budget** = the CU ceiling the M1 gate asserts at the named adversarial fill (§13.1); the client/keeper CU limit is the budgets table (`frontier-abi::budgets`, measured max + 5%, regenerated at the wave-5 gate). **Class** (keeper): W window-closing (escalate to P_def 2.0, pool-eligible), D delay-only (to P_delay 0.5), N non-critical, P player (priority 0 via relay), O operator. Heap ≤ 28 KiB at every gated fill (the allocator accepts up to 256 KiB under a heap frame, I-50); tx ≤ 1,232 B; locks ≤ 64; loaded data ≤ `L(kind)` (I-45).

| Tag | Instruction | Class | Writes | CU budget | Tx B max | Top-level only |
|---|---|---|---|---|---|---|
| 0x08 | AnnounceSeason | O | Season (init); reads the program's ProgramData (upgrade authority, I-51) | **25k** (v1.3; 18,570 worst of 64 ids [measured]: ≈ 1.5k per extra bump try) | 480 | — |
| 0x01 | CreateSeason | O | Season, Frontier, 6 ProvinceFunds, DefencePool | 70k | 1,100 | — |
| 0x09 | InitBeaconLogs | O | 16 BeaconLogs | **80k** (v1.3; 74,688 [measured]: 16 × ≈ 4.2k per created account) | 900 | — |
| 0x02 | InitShards(f) | O | 8 JoinShards | **45k** (v1.3; 39,313 [measured]) | 600 | — |
| 0x03 | ConsumeGenesisSeed | D | Season | 345k | 760 | yes |
| 0x04 | EndSeason | N | Season | 10k | 300 | — |
| 0x05 | CloseSeason | O | closes (Season shrunk; 6 ProvinceFunds) | 60k | 1,232 | — |
| 0x06 | AbortSeason | N/O | Season, (bond) | 20k | 300 | — |
| 0x07 | SetWindowSchedule | O | Season | 5k | 200 | — |
| 0x10 | PostAnchor | D | 1 BellAnchor | 345k | 800 | yes |
| 0x11 | PostAnchorMulti | D | ≤ `MULTI_MAX_REGIONS` BellAnchors (present skipped) | 400k | 1,232 | yes |
| 0x12 | PostSeed | D | 1 SeedCache | 345k | 800 | yes |
| 0x13 | PostBeacon | N | 1 BeaconLog | 340k | 760 | yes |
| 0x14 | ArchiveAnchors(r, part, ≤ 8 bells) | D | AnchorArchive (v1.3: region-half-day); closes ≤ 8 anchors | 60k | 1,232 | — |
| 0x15 | CloseSeedCache | N | closes 1 cache | 6k | 300 | — |
| 0x20 | OpenRing(d) | **D** | Frontier, RingSeed (reads 6 ProvinceFunds) | 30k | 600 | — |
| 0x21 | ConsumeRingSeed(d) | **D** | RingSeed | 345k | 760 | yes |
| 0x22 | OpenProvince(P, Q) | **D** | Province, RingSeed, its wedge's ProvinceFund (no Frontier write, I-48) | 220k | 400 | yes |
| 0x23 | FoldOccupancy(part) | **D** | Frontier (v1.2: parts 0, 1 read 24 JoinShards each, part 2 the 6 ProvinceFunds) | 30k per part | 1,232 | — |
| 0x24 | CloseProvince | N | Province (close), its ProvinceFund | 10k | 300 | — |
| 0x30 | Join | P (wallet) | Citizen (init), JoinShard; `join_gate` co-signs when set (I-51) | 25k | 700 | — |
| 0x31 | SetSession | P (wallet) | Citizen | 6k | 300 | — |
| 0x32 | SetVigil | P | Citizen | 6k | 250 | — |
| 0x33 | FileTicket(≤ 3) | P | Citizen (+ Holding-rent escrow), ≤ 3 Provinces (cohort counters, I-47) | **17k** (v1.4) | 560 | — |
| 0x34 | SettleTicket(k) | **D** | Holding, Province, Citizen, JoinShard, ≤ 2 more ticket Provinces (cohort), displaced Holding's rent payer (+ displaced Citizen, JoinShard) | 40k | 900 | — |
| 0x35 | ReleaseDormant | N | Holding (close), Province, Citizen, JoinShard, DefencePool (`pool_owed`) | 25k | 480 | — |
| 0x36 | CloseHolding | N | Holding (close), DefencePool (`pool_owed`) | 15k | 330 | — |
| 0x37 | CloseCitizen | N | Citizen (close) | 10k | 300 | — |
| 0x40 | Harvest | P | Holding, Citizen | 12k | 320 | — |
| 0x41 | Build(item) | P | Holding, Citizen (+ Province for walls) | 22k | 360 | — |
| 0x42 | Train(unit, n) | P | Holding, Citizen (immediate, I-56) | 15k | 330 | — |
| 0x43 | Muster | P | Holding, Province, Citizen | 25k | 380 | — |
| 0x44 | Dissolve(host) | P | Holding, Province, Citizen | 25k | 380 | — |
| 0x45 | Garrison(delta) | P | Holding, Province, Citizen | 25k | 380 | — |
| 0x46 | Explore(host, ≤ 2 tiles) | P | Holding, Province, Citizen | 18k | 380 | — |
| 0x47 | SettleExplore | N | Holding, Citizen | 15k | 400 | — |
| 0x48 | DisbandStranded(entry) | N | Province | 12k | 300 | — |
| 0x50 | Depart | P | Holding, Province, Citizen | 15k | 800 | — |
| 0x51 | **Reveal** | **W** | 1 ArrivalSlot (+ ArrivalDay on the first reveal of a province-bell) | **26k** (target 20k) | 1,100 | yes |
| 0x52 | SettleDeparture | D | Holding, origin Province | 15k | 400 | — |
| 0x53 | *(reserved: ProveBadSeal removed in v1.1, I-44)* | | | | | |
| 0x54 | SettleTransit | D | Holding, ClashInputs, dest/home Province, slot (close or `settled` flag), recipients; **never the DefencePool** | **85k** (seal opener ≤ 48k inside) | 1,100 | — |
| 0x55 | SweepPoolOwed | N | Holding, DefencePool | 8k | 300 | — |
| 0x60 | GatherClash(part) | D | 1 ClashInputs | 40k | 1,232 | yes |
| 0x61 | **ResolveFromInputs** | D | Province, ClashInputs | **340k** (290k with Phase B) | 460 | yes |
| 0x62 | ResolveClash (feature `oracle`) | tests | Province | recorded, not gated | 1,232 | yes |
| 0x63 | SkipQuiet(b0, n ≤ 24) | D | Province | 60k + 30k per bell whose quiet test is recomputed; stops and commits below 40k remaining (I-50) | 1,232 | yes |
| 0x64 | CloseClashInputs | N | closes | 8k | 300 | — |
| 0x65 | CloseArrivalDay | N | closes | 8k | 300 | — |
| 0x66 | CloseArrivalSlot | N | closes | 8k | 300 | — |
| 0x70 | ClaimDefence | **D** | DefencePool, DefenceClaim, ≤ 6 ArrivalSlots (claimed flag) | 25k | 1,000 | — |
| 0x80–0x8F | reserved (M2 money) | | | | | |
| 0x90, 0x91 | reserved (M3 CommitPosture, RevealPosture) | | | | | |

"Top-level only" instructions refuse `get_stack_height() > 1` (`NotTopLevel`). The worst-case clash budget is **460k** (≤ 3 × GatherClash 40k + ResolveFromInputs 340k). Every transaction also carries `SetLoadedAccountsDataSizeLimit(L(kind))` from `budgets.rs` (§10.1, I-45). Budgets are gates; the keeper's retry ladder (§8.2, I-50) keeps a breach from freezing anything.

### 5.6 Common prologues

**Player prologue (P)** — accounts `[0] actor (s)`, `[1] payer (s,w)` (the funder; may equal actor; the relay's pool key in sponsored play), `[2] season (r)`, `[3] citizen (w)`, then the instruction's accounts. Checks, in order, within ≈ 1.5k CU:
1. season: owner, magic `PSF1SEAS`, id; effective status Running (Seeded ∧ now ≥ genesis_ts) else `WrongStatus`; **then** `ruleset_hash == RULESET_HASH` else `RulesetMismatch` (v1.2: the status first, so a season that is not Running reports `WrongStatus` whatever its ruleset; a missing signature is `Auth`, a writability mismatch `BadAccount`, as `check_flags`).
2. citizen: owner, magic, season, canonical address; `actor == wallet`, or `actor == session ∧ now < session_expiry` (else `Auth` / `SessionExpired`).
3. action bucket: refill `rate × Δt` (milli-tokens, cap `burst × 1,000`), debit 1,000, else `Bucket`.
4. `bell(now) < end_bell` (M1 has no post-end player action except those marked).
5. if the instruction names a holding: owner/magic/season/canonical address, `owner_citizen == citizen`; lazy finality flip, **only in instructions that carry the holding's Province** (resident actions): `state == 1 ∧ now ≥ final_ts ∧` the Province's cohort for `ticket_bell` is closed (I-47) → 2 (JoinShard `final_holdings` is **not** written here — the fold counts provisional and final alike; Citizen flag 4 set); `touch_owner(now)`; `Holding::settle(now)` (harvest is implicit).
6. if the instruction names a host (Dissolve, Explore, Depart): the host id appears in no transit record of the Holding in state 1–3, else `HostInTransit` (58; I-44). A host that stayed at a destination can act once its SettleTransit has landed.

**Keeper-write prologue (K)** — `[0] fee_payer (s,w)`, `[1] season (r)`; data carries `beneficiary [32]` where the table says so; the fee payer pays any rent and is stored as `rent_to` (I-49); evidence (§5.12) from the instructions sysvar for W writes and anchors/caches/inputs.

### 5.7 Lifecycle

| Tag | Accounts (in order) | Data | Checks (in order) | Effects / log |
|---|---|---|---|---|
| 0x08 AnnounceSeason | `[authority s,w] [season w (PDA)] [program r] [programdata r] [system]` | id u64, params_hash [32], t_create_min i64, bond u64 | **`authority` is the program's upgrade authority** read from its ProgramData at the canonical LoaderV3 address (`Auth`, I-51); season PDA canonical (find once) and absent; `t_create_min ≥ now + 86,400` else `Announce`; `bond ≥ MIN_CREATION_BOND` (1 SOL) | Season allocated (PDA, pre-funding-safe), status Announced, authority, id, bump, params_hash, t_create_min, announced_ts, creation_bond held above rent / `ANNOUNCE` |
| 0x01 CreateSeason | `[authority s,w] [season w] [frontier w] [pfund × 6 w] [dpool w] [system]` | `SeasonParams` (fixed layout in `frontier-abi`, ≤ 256 B), `PayoutParams` (borsh, ≤ 128 B) | status Announced; authority; `t_create_min ≤ now < t_create_min + 7 d`; `sha256("PSF-PARAMS-v1" ‖ params) == params_hash` else `Announce`; params validated (ranges below); `PayoutParams::validate_for_season()` (CL-11); targets absent | Season filled, status Created, `genesis_round = genesis_seed_round(t_create_min)`, `genesis_ts = round_time(genesis_round) + 600`, `payout_params_hash`; Frontier; 6 ProvinceFund shards funded with `pfund_initial / 6` each; DefencePool with `dpool_initial` and caps; `reveal_loaded_limit`, `join_gate` / `SEASON_CREATED` |
| 0x09 InitBeaconLogs | `[authority s,w] [season] [blog × 16 w] [system]` | — | status Created/Seeded; each absent | 16 BeaconLogs / (none) |
| 0x02 InitShards | `[authority s,w] [season] [js × 8 w] [system]` | faction u8 | faction ≤ 5; each absent | 8 JoinShards / (none) |
| 0x03 ConsumeGenesisSeed | K + `[season w]` | round u64, sig48, hints | status Created; `round == genesis_round`; quicknet verify (hinted) | `genesis_seed = seed_of(round, sig)` (domain `PSF-SEED-v1`), status Seeded / `GENESIS_SEED` |
| 0x04 EndSeason | `[any s] [season w]` | — | effective Running; `bell ≥ end_bell` | status Ended / `SEASON_STATUS` |
| 0x06 AbortSeason | `[any s] [season w] [authority w] [incinerator w]` | — | (a) authority signer and `now < genesis_ts`, or (b) anyone and status ∈ {Announced, Created} and `now ≥ t_create_min + 7 d` | status Aborted; bond → authority if `now < round_time(genesis_round)`, else **burned** to `1nc1nerator11111111111111111111111111111111` (CL-24) / `SEASON_STATUS` |
| 0x05 CloseSeason | `[authority s,w] [season w] [frontier w] [pfund × 6 w] [dpool w] [js × n w] [blog × n w]` (repeatable in parts) | part u8 | status Ended ∧ `now ≥ end + 72 h`, or Aborted; every ProvinceFund shard's `provinces_opened == 0` for the final part | closes to authority; last part shrinks Season to a 128-B tombstone (status Closed) / `CLOSE`, `SEASON_STATUS` |
| 0x07 SetWindowSchedule | `[authority s] [season w]` | window u32, from_bell u32 | `600 ≤ window ≤ 1,800` and `from_bell < end_bell` (so never the `u32::MAX` sentinel) else `BadData`; `from_bell ≥ now_bell + 144` else `TooEarly`; **v1.3: one change per season** — once `window_from_bell` is set any further call is `AlreadyDone`, and `reveal_window` is never rewritten, so `W(b)` of a bell never changes after the bell exists (v1.2's "no pending change" let a later call fold `window_next` into `reveal_window`, changing `W` — hence `S(b, r)` — for bells whose caches, gathers and settlements could still be pending) | `window_next`, `window_from_bell` / `WINDOW` |

`SeasonParams` validation: `genesis_ring ≤ 4`; `r_max ∈ [genesis_ring + 1, 128]`; `regions == 16`; `reveal_window ∈ [600, 1,800]`; `seed_margin ≥ 60`; `join_close_bell < end_bell ≤ 4,032`; `archive_after ≥ 172,800`; `min_lead == 2`, `max_lead == 72`, `transit_slots == 4`; `min_reveal_priority_milli ≥ 100`; `reveal_cu_limit ∈ [16,000, 40,000]`; `bucket_rate ≥ 1`, `bucket_burst ≥ bucket_rate`; `defence_cap_milli ≤ 4,000`; `office_terms_per_wallet == 1`; `camp_regrow_bells ≥ 6`; `reveal_loaded_limit` a multiple of 32,768 in [65,536, 4,194,304]; `join_gate` any (zero = open); the M1 preset lives in `frontier-abi::presets::{M1_LOCAL_7D, M1_PLAYTEST}` (7-day: `end_bell = 1,008`, `join_close_bell = 756`, `join_gate` zero; playtest: `join_gate` = the relay's gate key).

### 5.8 Beacons and archives

| Tag | Accounts | Data | Checks (in order) | Effects / log |
|---|---|---|---|---|
| 0x10 PostAnchor | K + `[anchor w] [archive r] [ix sysvar] [system]` | region u8, bell u32, round u64, sig48 [48], hints (≈ 145 B), beneficiary [32] | anchor canonical `an‖(bell, r)`; present → **success no-op**; archive canonical `aa‖(r, part(bell))` (v1.3: `part = bell / 72`), tombstoned → `Archived`; `round == tlock_round(bell)` else `WrongRound`; verify | anchor {bell, region, net, round, A = now, slot, sig48, `rent_to` = fee payer, evidence}; beneficiary logged / `ANCHOR` |
| 0x11 PostAnchorMulti | K + `[anchor × k w] [archive × k r] [ix sysvar] [system]` | bell u32, round u64, sig48, hints, mask u16, beneficiary [32] | as PostAnchor, one verification; present anchors skipped; `k ≤ MULTI_MAX_REGIONS` = **7** (v1.2, W1-E's tx-size test: 7 regions 1,177 B, 8 regions 1,243 B) | ≤ k anchors / `ANCHOR` each |
| 0x12 PostSeed | K + `[anchor r] [cache w] [ix sysvar] [system]` | region u8, bell u32, nonce u8, round u64, sig48, hints, beneficiary [32] | THE anchor present (canonical) else `NoAnchor`; cache canonical `sd‖(bell, r, nonce)` and absent (present → no-op); `round == S(A)` else `WrongRound`; verify | cache {bell, region, nonce, round, seed = seed_of, anchor_key, A, slot, `rent_to` = fee payer} / `SEED` |
| 0x13 PostBeacon | K + `[beaconlog w]` | region u8, round u64, sig48, hints | `round > latest_round`; verify | BeaconLog updated / `BEACON` |
| 0x14 ArchiveAnchors | `[payer s,w] [season] [archive w] [system] ([anchor w] [cache r] [anchor_beneficiary w]) × ≤ 8` | region u8, part u32, n u8, bells [n] u32 | archive canonical `aa‖(r, part)` (init if absent, 6,144 B, `rent_to = payer`); every bell has `bell / 72 == part` (`BadData`); per bell: `now ≥ A + archive_after`; cache of THE anchor gives the seed | per bell: entry `{A − bell_end(b), seed, sig}` (I-44), **tombstone and archived bits set first**, then anchor closed to its `rent_to` / `ARCHIVE` |
| 0x15 CloseSeedCache | `[any s] [season] [cache w] [archive r] [rent_to w]` | bell u32, region u8, nonce u8 | archive's archived bit set; `rent_to == cache.rent_to` | close / `CLOSE` |

Every "anchor present" check in §5.1 accepts the archive entry instead once archived; PostSeed and GatherClash need the anchor itself or refuse (`NoAnchor`); gathers/resolves of archived bells read the archive's `a_off` and seed; SettleTransit reads the anchor's or the archive entry's `sig` (I-44).

### 5.9 Rings, provinces, citizens and land

| Tag | Accounts | Data | Checks (in order) | Effects / log |
|---|---|---|---|---|
| 0x20 OpenRing | `[payer s,w] [season] [frontier w] [ringseed w] [pfund × 6 r] [system]` | d u16 | season effective status ∈ {Seeded, Running} (never Ended, Closed or Aborted, I-46); `d == frontier.rings_opened ≤ r_max`; ringseed absent; **d ≤ g**: RingSeed seeded with `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))` (I-30); **d > g**: effective Running; ≥ 1 bell since `last_ring_open_bell`; some wedge's folded `wedge_occupied ≥ θ × wedge_open` (θ early/late by `now − genesis_ts`); every wedge fund `pf(w).lamports − rent ≥ d × rent(4,096)` | RingSeed (status 1 with `round = ring_seed_round(now)`, or status 2), `rings_opened = d + 1` / `RING_OPEN` |
| 0x21 ConsumeRingSeed | K + `[ringseed w]` | d u16, round u64, sig48, hints | season effective status ∈ {Seeded, Running} (I-46); status 1; `round == ringseed.round`; verify | seed = `seed_of`, status 2 / `RING_SEED` |
| 0x22 OpenProvince | `[payer s,w] [season] [ringseed w] [pfund(w) w] [province w] [system]` | P i16, Q i16 | season effective status ∈ {Seeded, Running} (I-46); `ProvinceCoord::checked(P, Q, r_max)` (CL-04); ring of (P,Q) = d; RingSeed d seeded; `pf(w)` is the province's wedge; province absent at `pv‖P,Q` | `init_funded` from `pf(w)`; `terrain::generate_province(ring_seed, p)` → arrays and masks; sites; **rings 0–1: sites `reserved`**; ring ≥ 2: initial camp `camp::place(ring_seed, p, day, true, true)` → one NEUTRAL camp on a passable non-site tile in `Province.camp` (I-56); `resolved_next = now_bell` (or 0 before genesis); region; `pf(w)`: `open_sites += site_count − reserved`, `provinces_opened += 1` (**no Frontier write**, I-48) / `PROVINCE_OPEN` |
| 0x23 FoldOccupancy | `[payer s] [season] [frontier w]` + parts 0, 1: `[js × 24 r]` (factions 0–2, 3–5); part 2: `[pfund × 6 r]` (v1.2: v1.1's part 1 with 24 shards and 6 funds was 1,288 B, over the packet) | part u8 (0–2) | part 0: sums 24 shards into `acc_*`, `fold_bell = now_bell`, `fold_part = 1`; part 1: same bell as part 0 else `FoldStale`, add the other 24, write `occupied_sites`, `wedge_occupied`, `fold_part = 2`; part 2: same bell else `FoldStale`, fold `open_sites`, `wedge_open`, `provinces_opened` from the 6 ProvinceFund shards; `fold_part = 0` | / `FOLD` |
| 0x24 CloseProvince | `[any s] [season] [province w] [pfund(w) w]` | P i16, Q i16 | status Ended/Aborted; `now ≥ end + 72 h` (Aborted: immediately) | close into `pf(w)`; `pf(w).provinces_opened −= 1` / `CLOSE`. Re-opening is impossible: OpenProvince refuses outside {Seeded, Running} (I-46) |
| 0x30 Join | `[wallet s] [payer s,w] [season] [frontier r] [citizen w] [joinshard w] [system] [join_gate s?]` | faction u8, session [32], session_expiry i64 | effective Running and `bell < join_close_bell`; **if `season.join_gate ≠ 0`, account 7 is `join_gate` and signs** (`JoinGate`, I-51); faction ≤ 5; citizen absent at `ct‖tag(wallet)`; joinshard = `(faction, sha256(wallet)[0] mod 8)`; `session_expiry ≤ now + 30 d`; **capacity**: `(open_sites − occupied_sites) × 10,000 ≥ reserve_bps × open_sites` on the folded values, or a ring can still open (`rings_opened ≤ r_max` and every wedge fund covers it), else `Capacity` (nothing created) | Citizen (`citizen_tag`, bucket full, `explores_floor_left = 3`, vigil 0, `rent_payer = payer`, `ticket_bell = u32::MAX`, `ticket_escrow = 0`, `office_terms_used = 0`); `members += 1` / `JOIN` |
| 0x31 SetSession | P (actor = wallet only) | session [32], expiry i64 | `expiry ≤ now + 30 d` | / `SESSION` |
| 0x32 SetVigil | P | start_min u16 | `start_min < 1,440`; kernel `Vigil::request_change` (≥ 24 h notice, ≤ 1 per 7 days, **effective at the first UTC midnight ≥ now + 24 h**, CL-09) | `vigil_next_min`, `vigil_from_ts` / `VIGIL` |
| 0x33 FileTicket | P + `[frontier r] [province × m w] [system]` (m ≤ 3: the distinct provinces of the sites, canonical and present) | n u8 (1–3), n × {P i16, Q i16, site u8} | no provisional or final first holding; no open ticket (`ticket_bell == u32::MAX`); `ticket_bell ≠ now_bell`; each site: `ProvinceCoord::checked`, ring ≥ 2 (`ReservedSite`), ring < `rings_opened`, **its Province exists** (`BadAccount`, I-48), site < 12, wedge = citizen's faction wedge (or, when the folded `wedge_open − wedge_occupied == 0` for the own wedge, the adjacent wedges' outermost open ring); **cohort**: each province has a cohort for `now_bell`, or a closed/expired one to reuse, else `CohortFull` (I-47); **escrow**: `payer` transfers `max(0, rent(1,280) − ticket_escrow)` into the Citizen | ticket stored, `ticket_bell = now_bell`, `ticket_next = 0`, `ticket_escrow = rent(1,280)`, `ticket_funder = payer`; each province's cohort `filed += 1` / `TICKET` |
| 0x34 SettleTicket | `[payer s,w] [season] [citizen w] [holding w] [province w] [joinshard w] [seedcache|archive r] [anchor|archive r] [other ticket provinces w × ≤ 2] [displaced_rent_payer w?] [displaced_citizen w?] [displaced_joinshard w?] [system]` | k u8 | `k == ticket_next`, ticket present; **expired** (`now_bell ≥ ticket_bell + 24`) → outcome `expired`; else site from the ticket; seed `S(ticket_bell, r_site)` from a SeedCache of THE anchor (or the archive entry) else `SeedNotReady`; `score = rng::rand(S, "site", P‖Q‖site‖citizen_tag)`; site mirror: free / released-free → **fresh**; a **provisional** holding (state 1) with the same `ticket_bell` and a lower score (tie: lower `citizen_tag` wins) → **displace** (no time condition: while the cohort is open no holding in it can be final, I-47); otherwise `taken` with `ticket_next += 1` (success; the next preference follows) | fresh: Holding init funded **from `citizen.ticket_escrow`** (program-to-program lamports), `rent_payer = ticket_funder`, `Holding::found(now, day, 1)` with `STARTER_KIT`, shield, `final_ts = round_time(S) + 600`; displace: Holding rewritten in place (`gen += 1`), the new escrow paid to the displaced Holding's old `rent_payer`, `rent_payer = ticket_funder`, the displaced Citizen reverts (flags, `holding[0]`) and **its ticket ends** (the web offers a refile), its JoinShard decrements; Province site mirror (holding, faction, gen, shield), `roster_epoch += 1`; Citizen provisional (flag 8, `holding[0]`), `ticket_escrow = 0`; JoinShard `holdings += 1`, `holdings_by_wedge`. **When the ticket ends** (fresh, displace, exhausted or expired) every distinct province of the ticket gets cohort `settled += 1` (they are all in the transaction) / `SETTLE` |
| 0x35 ReleaseDormant | `[any s] [season] [holding w] [province w] [citizen w] [joinshard w] [rent_payer w] [dpool w]` | — | order 1; `now ≥ last_owner_action + release_after`; no transit in state 1–3 (`HasTransits`) | site → released-free; Citizen refugee flag, `holding[0]` cleared; JoinShard `holdings −= 1`, `released += 1`; `pool_owed` → DefencePool; Holding closed to its rent payer; its hosts become stranded / `RELEASE` |
| 0x36 CloseHolding / 0x37 CloseCitizen | `[any s] [season] [holding|citizen w] [rent_payer w] ([dpool w] for a Holding; [ticket_funder w] for a Citizen with escrow)` | — | status Ended ∧ `now ≥ end + 72 h`, or Aborted; Holding: no escrow left (unsettled transits refunded to `rent_payer` in the same instruction) | close; `pool_owed` → DefencePool; Citizen escrow → `ticket_funder` / `CLOSE` |

Note on SettleTicket "taken": to keep the ticket state machine moving without a failed transaction, a taken site returns **success** with the Citizen's `ticket_next += 1` and a `SETTLE` record with outcome `taken`. When `ticket_next == n`, the ticket is exhausted (`ticket_bell = u32::MAX`, cohorts marked settled, escrow kept for the next FileTicket); the player files again.

Note on cohorts (I-47): a Province keeps 8 cohort records; a record is free when `settled == filed` or `now_bell ≥ bell + 24`. The keeper settles every ticket of a cohort in descending score in the first slots after S, so honest cohorts close within a few slots and onboarding times are unchanged; an attacker who wants a site it did not win must hold the Province (or the winner's Citizen) against a class-D keeper for 24 bells.

### 5.10 Holdings and resident actions

All: player prologue + `[holding w]`; resident actions (Muster, Dissolve, Garrison, Explore, Depart) also need `state == final` (`NotFinal`) and the province resolved through b − 2 (`NotResident`); Dissolve, Explore and Depart also need the host out of transit (`HostInTransit`, I-44).

| Tag | Extra accounts | Data | Checks | Effects / log |
|---|---|---|---|---|
| 0x40 Harvest | — | — | — | settle only / `HARVEST` (stores digest) |
| 0x41 Build | `[province w?]` (walls) | item u8 | `catalog::building(item, tier, n)` → cost (via checked `duplicate_cost`), effect, secs; `pay`; `enqueue` (`QueueFull`, `Insufficient`; v1.2: a build past a CL-02 cap, e.g. walls above `MAX_WALLS` = 1,200, is `HoldingError::AboveCap` → `Kernel` 15); walls item: province = holding's | queue item; walls: site mirror wall item `{effective_bell = bell_at(done_at) + 1, delta}` / `BUILD` |
| 0x42 Train | — | unit u8, n u32 | `catalog::train(unit, n)` → cost; not Settler; `pay` (`Insufficient`) | `reserve[unit] += n` **at once** (the simulator trains instantly, I-56) / `TRAIN` |
| 0x43 Muster | `[province w]` | unit u8, troops u32, tile u8 | province = holding's; `reserve[unit] ≥ troops`; `Host::muster` bounds (100–30,000; not Settler); caps after the change (48 total, 8 per faction over states 1–2); a free entry (`ProvinceFull`) | `reserve −= troops`; entry state 2 (`from_bell = now_bell + 1`), id from `host_seq`, `roster_epoch += 1` / `MUSTER` |
| 0x44 Dissolve | `[province w]` | host_id u64 | host is the caller's (id → holding), state 1, no pending op | pending Leave (troops back to reserve after the clash) / `DISSOLVE` |
| 0x45 Garrison | `[province w]` | delta i64 | `GarrisonState::change(b, delta, resolved_next)`; reserve covers a positive delta | site mirror pending, reserve moved / `GARRISON` |
| 0x46 Explore | `[province w]` (where the host is) | host_id u64, n u8 (1–2), tiles [2] u8 | host is the caller's, state 1 in this province, unit Scout, no pending op; tiles within 1 hex of the host's tile and not in `explored_mask` (`Explored`); holding's explore record free | `explored_mask` bits set (first claim wins); record {bell, P, Q, tiles, host} / `EXPLORE` |
| 0x47 SettleExplore | `[payer s] [season] [holding w] [citizen w] [seedcache|archive r] [anchor|archive r]` (no prologue; anyone) | — | record present; `S(record.bell, r_province)` available (`SeedNotReady`) | `explore::roll(S, P, Q, tile, host, floor)` per tile → `Find { works }` (I-56: 4 Works always while `explores_floor_left > 0`, else with chance ½; no goods in M1); `works += find.works`; record cleared / `EXPLORE_RESULT` |
| 0x48 DisbandStranded | `[any s] [season] [province w] [holding r (canonical, may be absent)]` | entry u8 | entry's host id → Holding address; Holding absent or its `gen` ≠ the id's gen | entry freed (troops lost), `roster_epoch += 1` / `STRANDED` |

### 5.11 Marches, reveals, clashes and transits

**Depart (0x50)** — player prologue + `[holding w] [province w] [system]`. Data: host_id u64, commit [32], seal [165], arrive_bell u32, tip u64, transit_slot u8 (219 B with the tag). Checks in order:
1. `holding.state == final` (`NotFinal`); province = the host's, resolved through b − 2 (`NotResident`).
2. `seal` byte 0 is a valid compressed-G2 flag (syntax only; validity is judged by SettleTransit, I-44) else `BadData`.
3. Host is the caller's, state 1, no pending op (`HostBusy`), in no transit record of the Holding in state 1–3 (`HostInTransit`, I-44); kernel `Host::depart(b, march_stamina(32), resolved_next)` (I-32; `Cooldown`/`NoStamina`).
4. `arrive_bell ∈ [now_bell + 2, now_bell + 72]` (`ArrivalBell`; the exact earliest bell is Reveal's job).
5. `tip ≥ fees::min_tip_lamports(min_reveal_priority_milli, reveal_cu_limit)` (`TipTooLow`); transit slot free (`TransitState`); `payer.lamports ≥ tip + march_fee + seal_bond` (`Insufficient`).

Effects: `seal_root = sha256(commit ‖ sha256(seal))` computed by the program; transit record {state 1, host, unit, faction, origin, depart_bell, arrive_bell, depart_ts, dep_mass = troops, march_stamina, dealt_bps = doctrine, seal_root, tip, flags 1|2}; System transfer payer → holding of `tip + march_fee + seal_bond`, `escrow +=`; Province entry pending op Spend (becomes `departed`, state 3, at the bell's settle); Citizen `arrivals += 1`. Log `DEPART` with the commitment and the full seal.

**Reveal (0x51)** — class W, top-level only, no player signer. Accounts (in order):
`[0 fee_payer s,w] [1 season r] [2 holding r] [3 anchor r] [4 archive r] [5 beaconlog r] [6 inputs r] [7 dest_province r] [8 arrivalday r|w] [9..12 slot0..slot3 (exactly the target writable)] [13.. path provinces other than the destination, 0–3, in first-entered order] [ix sysvar r] [system r]`.
Data: transit_slot u8, target_i u8, plain [37], salt [32], ct_hash [32], beneficiary [32] (135 B + tag).
Checks in order (cheap first):
1. season effective Running, or Ended with `arrive < end_bell`; ruleset; top-level.
2. holding owner/magic/season/canonical; transit state ∈ {1, 2, 3}; `Plain::validate(plain, transit)` (version 1, reserved zero, host and arrive match, `path_len ≤ 32`, unused bits zero, stance, retreat ≤ 60,000) else `BadPlaintext`.
3. `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain ‖ salt)`; `sha256(commit ‖ ct_hash) == transit.seal_root` else `CommitMismatch`.
4. destination (P, Q, tile) from plain; `r = region_of(P, Q)` (`WrongRegion` if the supplied anchor/archive/beaconlog are not r's); window: THE anchor `an‖(arrive, r)` present → `now < A + W(arrive)` and `beaconlog.latest_round < S(A)` else `WindowClosed`; absent → archive `aa‖(r, part(arrive))` (v1.3) must not tombstone `arrive` else `Archived`.
5. **latch**: inputs at `ci‖(P, Q, arrive)` absent else `LatchClosed`; `dest_province` canonical and `resolved_next ≤ arrive` else `LatchClosed`.
6. path: ≤ 32 steps of 3-bit directions from the origin tile; every step's province is the destination or one of the supplied path provinces (canonical); every step passable; last hex = destination tile; `travel::path_cost` × doctrine travel bps → `check_arrival_bell(genesis_ts, depart_ts, secs, arrive)` (`ArrivalBell`); destination tile is not the site of a shielded holding of another faction, and a host of a shielded holding may not target another faction's holding site (`Shielded`); else `Path`.
7. quota: read the 4 slots of `(dest, arrive, faction)` at their canonical addresses; `admit_arrival(p, arrive, &slots, SlotEntry{host_id, citizen_tag, dep_mass})`: `Refuse(_)` → `QuotaRefused` (nothing written); host already in a slot → `AlreadyDone`; `Fill{i}`/`Displace{i}` → `i == target_i` and that slot is the writable one, else `SlotMoved`.
8. ArrivalDay: if bit `arrive mod 144` is clear, account 8 must be writable (`NeedArrivalDay`); init pre-funding-safe if absent (`rent_to` = fee payer, I-49); set the bit.
9. evidence (§5.12).
Effects: slot created (`rent_to` = fee payer, I-49; `beneficiary` from the data) or overwritten on displacement (`rent_to` kept; `beneficiary`, `ev_*` replaced and `claimed = 0`; the displaced reveal's evidence is logged, not claimable, I-52); fields from plain and transit; `citizen_tag` from `holding.owner_citizen[0..8]`. **Nothing else is written.** Log `REVEAL` (host, dest, tile, stance, retreat, i, fill/displace, displaced host, beneficiary, evidence).

**SettleDeparture (0x52)** — `[payer s] [season] [origin province w] [holding w]`; data transit_slot u8. Checks: transit state 1; `province.resolved_next > depart_bell` (`TooEarly`); entry with the host id in state 3 (departed). Effects: `troops_after`, `stamina_after`, `ready_bell_off` from `Host::march_values`; state 2 (or 3 when destroyed at origin, troops 0); entry freed. Log `DEPARTURE_SETTLED`.

**GatherClash (0x60)** — K + `[province (dest) r] [anchor|archive r] [arrivalday r] [inputs w] [ix sysvar] [system] [slot_k r …] [holding_k r …]`. Data: bell u32, start u8, n u8, holdings_bitmap u32 (which positions are expected present), beneficiary [32]. Checks: province canonical; **`bell ≥ province.resolved_next`, else `LatchClosed`** (a resolved or skipped bell is tombstoned for inputs, I-46); window closed (`now ≥ A + W`, anchor or archive) else `TooEarly`; inputs absent → init (`rent_to` = fee payer, evidence) / present → owner/magic/key; ArrivalDay bit of `bell` clear (or ArrivalDay absent) → `flags |= 1`, `arrivals_mask = 0xFFFFFF`, done; else for each position k in `start..start+n` not yet in the mask: slot canonical; absent → not present; present → owner/magic/key; its host's Holding (address from host id) supplied and canonical, transit state 2 or 3 with matching host and arrive (`DepartureUnsettled`); record = slot fields + `troops_after`, `stamina_after`. Positions commute; repeats are no-ops. Log `GATHER`. Per gather ≤ 24 slot keys, ≤ 10 Holdings, ≤ 1,232 B; a full province-bell needs ≤ 3 gathers.

**ResolveFromInputs (0x61)** — K + `[province w] [inputs w] [seedcache|archive r] [anchor|archive r] [ix sysvar]`. Data: bell u32, beneficiary [32]. Checks: `bell == province.resolved_next` (`OutOfOrder`); `arrivals_mask == 0xFFFFFF` (`NotGathered`); seed from a SeedCache whose `anchor_key` is THE anchor and `round == S(A)` with A equal to THE anchor's, or from the archive entry (`SeedNotReady`). Effects: build `ClashInput` (residents in state 1 with `from_bell ≤ bell` at `Host::values_at(bell)` fighting at Hold; garrisons from the site mirror with `GarrisonState::at(bell)` and walls effective ≤ bell; arrivals = present records; terrain from the compact arrays; `Relations{peaceful: 0}`; **room `Occupancy{pending[f]` = entries in state 2 of faction f, `storage_free` = 56 − entries in states 1–3`}`** so arrivals are admitted only while roster + pending < 48, per faction < 8 and admitted < `storage_free`, the rest bouncing with no loss in kernel rank order (I-43); the camp as a NEUTRAL garrison on its tile, respawned first if the day's check is due (I-56)); `clash::resolve_clash` (Phase A); write back residents (`Host::apply_clash`), Stays/Withdrew arrivals → entries (state 1, `from_bell = bell + 1`), garrisons (`GarrisonState::apply_clash`), cleared camp → `camp.state = 0`, `WORKS_CAMP` credited to the winning faction's arrivals in the fate table (I-56); settle every pending op of the bell (`Host::settle`, `settle_merge`, `GarrisonState::settle`): musters join, departures → state 3 with post-clash values; inputs fate table, `flags |= 2`, `resolver = beneficiary`, evidence; Province `resolved_next = bell + 1`, digest, `roster_epoch += 1`. Log `CLASH`.

**SkipQuiet (0x63)** — `[payer s] [season] [province w] [arrivalday_0 r] [arrivalday_1 r] [anchor_or_archive × n r]`. Data: b0 u32 (= `resolved_next`), n u8 (1–24). For each bell b in order: (1) window closed via THE anchor or the archive entry (`TooEarly`); (2) ArrivalDay bit of b clear (absent = clear), else stop (`NotQuiet`); (3) apply the pending ops that take effect at b; (4) `clash::is_quiet(rules, inp)` with no arrivals, cached by `(roster_epoch, relations)`; a non-quiet bell stops with `NotQuiet` if it is the first, otherwise the instruction commits the bells before it; (5) **before each bell after the first, if `sol_remaining_compute_units() < 40,000` the instruction commits the bells done so far** (I-50: a roster changed at every bell costs ≈ 25k per bell, so a 24-bell skip of a churned province needs ≈ 0.7M or several transactions). Effects: `resolved_next = b0 + k`, lazy timers; log `SKIP` with the quiet digest. Equivalence gate G11 (§13.3).

**ProveBadSeal (0x53)** — removed in v1.1 (I-44): SettleTransit opens and judges every seal, so the proof cannot be skipped or raced, and it still works after the anchor is archived.

**SettleTransit (0x54)** — class D, anyone. Accounts: `[payer s,w] [season] [holding w] [dest province w] [inputs w (or the canonical absent address)] [slot w (or canonical absent)] [home province w] [anchor|archive r (THE anchor of (arrive, r_dest), or its region-half-day archive `aa‖(r_dest, part(arrive))`, v1.3)] [slot_beneficiary w] [resolver w] [holding_rent_payer w] [settle_beneficiary w] [system]`. Data: transit_slot u8, commit [32], seal [165], beneficiary [32] (231 B with the tag; tx ≈ 990 B ≤ 1,100 B). Checks in order:
1. transit state 2 or 3 (`DepartureUnsettled` for 1, `TransitState` for 0).
2. `now ≥ close(arrive, r_dest) + 600` and `dest.resolved_next > arrive` (`TooEarly`).
3. `sha256(commit ‖ sha256(seal)) == transit.seal_root` (`CommitMismatch`): the pair judged is the logged pair.
4. **The proof (I-44):** the round-T(arrive) signature from THE anchor (present) or the archive entry's `sig` (archived; canonical addresses, `NoAnchor` otherwise); open the seal with the FO check; `commit' = sha256(DOMAIN_MARCH ‖ plain ‖ salt_of(k))` must equal `commit`; `Plain::validate(plain, host, arrive)`. Any failure → **bad seal** with code 1, 2, 4 or 5 (3 is reserved, v1.3); otherwise code 0 and `plain` is the march's order (whether or not it was revealed). Budget: the opener ≤ 48.0k [measured, SP-V2 ProveBadSeal], instruction 85k.
5. `settle_beneficiary == beneficiary`; ClashInputs are used **only if present with flag 2** (resolved, I-46); otherwise the "no final set" branch applies.

Outcome (D5, order-free):
- **Bad seal** → the host is destroyed: a Stays/Withdrew host still among the destination's entries gets pending op `Forfeit` (troops lost at the settle of `bell(now)`; past clashes are never re-run); a bounced or retreated host is not returned; the transit closes; **tip + march fee + bond → `settle_beneficiary`** (the prover's reward of DESIGN §6.2, now paid to whoever settles).
- else **resolved ClashInputs**: host in the records → fate (Stays/Withdrew: transit closed, host is a destination resident and may act from now on; Bounced/Retreated: returns home, no loss; Destroyed: gone); slot beneficiary gets the tip, inputs' resolver gets the march fee, bond → `holding.rent_payer`; `settled_mask` bit set. Host not in the records → compare `SlotEntry{host, citizen_tag, dep_mass}` with the faction's recorded final set: outranked by the 4th, or its citizen has a higher-ranked arrival → **bounce, no loss** (tip and bond → `holding.rent_payer`, fee → resolver); otherwise → **routed** (`rout_survivors`, stamina 0, tip → `Holding.pool_owed`, bond → rent payer, fee → resolver).
- else (no resolved inputs: closed, never gathered, or gathered without arrivals) → **routed** (tip and fee → `Holding.pool_owed`; bond → rent payer).
- **Slot (I-52):** closed to its `rent_to`, unless its evidence is claim-eligible (lateness ≥ `lateness_slots` against THE anchor's slot and a price above the tip level), unclaimed and `now < close + claim_grace` (6 bells): then only its `settled` flag is set and CloseArrivalSlot closes it later.
Returning hosts rejoin the home Province as a muster-pending entry if an entry and the caps allow, else troops go to `reserve`. Every payment uses `pay_or_divert` with sink `Holding.pool_owed`; **no DefencePool account is listed** (I-48). Log `TRANSIT_SETTLED` (outcome, seal code).

**Closes (0x64–0x66)**

| Tag | Accounts | Data | Checks | Effects |
|---|---|---|---|---|
| 0x64 CloseClashInputs | `[any s] [season] [province r] [inputs w] [rent_to w]` | P i16, Q i16, bell u32 | `flags & 2`; every present arrival's `settled_mask` bit set; `resolved_ts + clash_close_grace × 600 ≤ now − genesis_ts` (`InputsOpen`) | close → `rent_to` / `CLOSE` |
| 0x65 CloseArrivalDay | `[any s] [season] [province r] [day w] [rent_to w]` | P, Q, day u32 | `province.resolved_next ≥ 144 × (day + 1)` | close / `CLOSE` |
| 0x66 CloseArrivalSlot | `[any s] [season] [slot w] [rent_to w] ([anchor r] for case a)` | P, Q, bell, f, i | (a) the slot's `settled` flag is set and (`claimed` or `now ≥ close + claim_grace`); or (b) status Ended ∧ `now ≥ end + 72 h` (orphans of holdings closed at the end) | close / `CLOSE` |

### 5.12 Fee evidence and the defence pool

- **Evidence** (Reveal; also recorded on BellAnchor, and on ClashInputs at init and at resolve, for the liveness report): landing slot (Clock), `SetComputeUnitPrice` µlamports (0 if absent), `SetComputeUnitLimit`, `SetLoadedAccountsDataSizeLimit`, read from the instructions sysvar (read-only key). The transaction's signature count is assumed 1 (keeper transactions have one signer).
- **ClaimDefence (0x70)** — class D. `[keeper s,w (= beneficiary)] [season] [dpool w] [claim w] [system] ([slot w] [anchor r]) × ≤ 6`. Data: day u32, n u8. Per slot: `beneficiary == keeper`, `claimed == 0`, `now < close + claim_grace` (6 bells; SettleTransit keeps an eligible slot open that long, I-52); lateness `ev_slot − anchor.slot ≥ lateness_slots` (reference: THE anchor of `(bell, r)`); `refund = fees::defence_refund(ev, season)` = `min(⌈ev_price × ev_limit / 10⁶⌉, defence_cap × cost − 2,500) − (tip_min − 2,500)` if positive (v1.2: rounded up, as the runtime charges the priority fee), `cost = ev_limit + 720 + 300 × (2 + created_day) + 8 × ceil(ev_loaded / 32,768)`, `created_day` = the slot's flags bit 2 (v1.2, `arrival_slot::evidence`); per-bell-region cap and per-keeper-day cap (`claim`). Effects: `claimed = 1`; pay the keeper from the pool (empty → partial, logged). Eligibility is **ArrivalSlot evidence only** (I-21). Log `DEFENCE_CLAIM`.
- **SweepPoolOwed (0x55)** — class N, anyone: `[any s] [season] [holding w] [dpool w]`; moves `pool_owed` lamports from the Holding to the DefencePool and zeroes it (ReleaseDormant and CloseHolding do the same). Log `POOL_SWEEP`. The pool therefore receives routed tips late, never on a critical or delay path (I-48).

---

## 6. Log records: the PS2 contract (verifier and herald input)

**Encoding.** One `sol_log_data(&[b"PS2", body])` per record; a transaction emits one record per account it writes (a record touching several chained entities carries all their heads). `body` =

| Part | Bytes | Content |
|---|---|---|
| head | 6 | `ver u8 = 1` ‖ `kind u8` ‖ `bell u32` (current bell; `u32::MAX` before genesis) |
| key | kind-specific, fixed | the entity key fields (§4.1 raw form, coordinates i32) |
| payload | kind-specific, fixed | the fields listed below, in order, LE |
| tail | `1 + 41 × n` | `n u8`, then n × `{entity_kind u8, seq u64, head [32]}` for every chained entity the record touches |

`entity_kind`: 1 Season, 2 Frontier, 3 JoinShard, 4 Citizen, 5 Holding, 6 Province, 7 ClashInputs. For each chained entity E: `E.seq += 1; E.head = sha256(E.head ‖ le64(E.seq) ‖ body_without_tail)`, and the tail carries the new `(seq, head)`. `frontier-abi::log` provides `encode`, `decode`, `chains_of(kind, key, payload) -> [(entity_kind, address)]`, and the head function; `frontier-abi/vectors/logs.json` pins one vector per kind. `body_without_tail` ≤ 128 B except DEPART (+ commit 32 + seal 165) and CLASH (+ fates ≤ 72 B).

| Kind | Name | Key | Payload (in order) | Chains |
|---|---|---|---|---|
| 1 | ANNOUNCE | season id u64 | params_hash, t_create_min, bond | Season |
| 2 | SEASON_CREATED | id | params digest, genesis_round, genesis_ts, ruleset_hash, quicknet_pk_hash, W, Δ, r_max, program_version | Season |
| 3 | GENESIS_SEED | id | round, seed | Season |
| 4 | RING_OPEN | d u16 | t_open, round (0 for genesis rings), seed (genesis rings) | Frontier |
| 5 | RING_SEED | d | round, seed | — (v1.2: ConsumeRingSeed writes only the short-header RingSeed) |
| 6 | PROVINCE_OPEN | P, Q | ring, wedge, region, terrain digest [32], site_count, camp tile u8, camp troops u32, reserved flag | Province (v1.1: no Frontier write) |
| 7 | FOLD | part u8 | occupied, wedge_occupied [6]; part 1 also open_sites, wedge_open [6], provinces_opened | Frontier |
| 8 | SEASON_STATUS | id | old u8, new u8, bond outcome u8 | Season |
| 9 | WINDOW | id | window_next, window_from_bell | Season |
| 10 | JOIN | citizen tag [15] | wallet [32], faction, shard, session [32], expiry | Citizen, JoinShard |
| 11 | SESSION | tag | session, expiry | Citizen |
| 12 | VIGIL | tag | start_min, from_ts | Citizen |
| 13 | TICKET | tag | ticket_bell, n, sites, escrow, funder [32] | Citizen, Province × m (cohort) |
| 14 | SETTLE | P, Q, site | outcome u8 (0 fresh, 1 displace, 2 taken, 3 expired), citizen tag, score, displaced tag, gen, final_ts, ticket_bell | Citizen, Holding (fresh/displace), Province (not for taken), other ticket Provinces (when the ticket ends), JoinShard, displaced Citizen + JoinShard |
| 15 | RELEASE | P, Q, site | citizen tag | Holding (final), Province, Citizen, JoinShard |
| 16 | HOLDING_FINAL | P, Q, site | final_ts | Holding, Citizen |
| 20 | HARVEST | P, Q, site | stores digest [32] | Holding, Citizen |
| 21 | BUILD | P, Q, site | item, cost digest, done_at | Holding, Citizen (, Province for walls) |
| 22 | TRAIN | P, Q, site | unit, n, done_at | Holding, Citizen |
| 23 | MUSTER | host_id u64 | unit, troops, tile, entry | Holding, Province, Citizen |
| 24 | DISSOLVE / 25 GARRISON | host_id / P,Q,site | pending bell, delta | Holding, Province, Citizen |
| 26 | EXPLORE | host_id | P, Q, tiles | Holding, Province, Citizen |
| 27 | EXPLORE_RESULT | host_id | finds per tile, works, floor used | Holding, Citizen |
| 28 | STRANDED | host_id | troops lost | Province |
| 30 | DEPART | host_id | origin P,Q,tile, depart_bell, arrive_bell, dep_mass, march_stamina, tip, seal_root [32], **commit [32], seal [165]** | Holding, Province, Citizen |
| 31 | REVEAL | P, Q, arrive, faction, i | host_id, tile, stance, retreat, fill/displace u8, displaced host, beneficiary [32], ev_slot, ev_price, ev_limit, arrivalday_created u8 | — (slot content) |
| 32 | DEPARTURE_SETTLED | host_id | troops_after, stamina_after, destroyed u8 | Holding, Province |
| 33 | *(reserved: BAD_SEAL removed in v1.1; the seal code is in TRANSIT_SETTLED)* | | | |
| 34 | TRANSIT_SETTLED | host_id | outcome u8 (stays, withdrew, bounced, retreated, destroyed, bounced-unranked, routed, bad-seal), **seal code u8 (0 valid, 1–5 bad)**, troops, payments (tip, fee, bond, reward recipients' first 8 B + amounts), pool_owed delta, slot kept u8 | Holding, ClashInputs (if resolved), dest/home Province (if written) |
| 40 | GATHER | P, Q, bell | start, n, arrivals_mask, no-arrivals u8 | ClashInputs |
| 41 | CLASH | P, Q, bell | outcome digest [32], input digest [32], engagements, fates (packed 3 bits × 24) | Province, ClashInputs |
| 42 | SKIP | P, Q | b0, n, quiet digest [32] | Province |
| 43 | CAMP | P, Q | tile, troops, day (spawn or respawn) | Province |
| 50 | ANCHOR | bell, region | round, A, slot, beneficiary | — |
| 51 | SEED | bell, region, nonce | round, seed, A | — |
| 52 | BEACON | region | round | — |
| 53 | ARCHIVE | region, day | bell, a_off, seed | — |
| 60 | DIVERT | recipient [32] | amount, reason u8 | — |
| 61 | DEFENCE_CLAIM | beneficiary | day, slots n, amount, partial u8 | — |
| 62 | POOL_SWEEP | P, Q, site | amount | Holding |
| 70 | CLOSE | account kind u8, key | final seq, final head (chained only), recipient, lamports | the closed entity (its last record) |

The verifier (§8.5) walks each chain from these records and requires the final head to equal the on-chain account's (or, for closed accounts, the CLOSE record's). The herald folds these records per province and bell.

---

## 7. Kernel interfaces (`permutation-rules::frontier`, pinned signatures)

New or changed items; everything else is called unchanged (program §11). All are pure, `no_std`, no Solana types.

| Module / item | Signature (Rust) | Unit | CL / I |
|---|---|---|---|
| `beacon::first_round_from` | `fn first_round_from(genesis: i64, period: u32, t: i64) -> u64` | W1-C | CL-19 |
| `beacon::{round_time, bell_start, bell_end, bell_at}` | `fn round_time(genesis: i64, period: u32, r: u64) -> i64`; `fn bell_start(genesis_ts: i64, b: u32) -> i64`; `fn bell_end(..) -> i64`; `fn bell_at(genesis_ts: i64, t: i64) -> Option<u32>` | W1-C | CL-20 |
| `beacon::tlock_round` | `fn tlock_round(c: &BeaconClock, genesis_ts: i64, b: u32) -> u64` | W1-C | CL-20 |
| `beacon::{seed_round, ring_seed_round, genesis_seed_round, reveal_close, window}` | `fn seed_round(c: &BeaconClock, close: i64, margin: u32) -> u64`; `fn ring_seed_round(c, t_open, margin) -> u64`; `fn genesis_seed_round(c, t_create_min, margin) -> u64`; `fn reveal_close(a: i64, w: u32) -> i64`; `fn window(s: &WindowSchedule, b: u32) -> u32` | W1-C (calls the existing `clash::{BeaconClock, seed_round}`; no move) | CL-19 |
| `addr` | `fn seed(kind: SeedKind, raw: &[u8]) -> ([u8; 32], usize)`; one constructor per kind (`anchor(bell, region)`, `slot(p, q, bell, f, i)`, …); `fn citizen_tag15(wallet: &[u8; 32]) -> [u8; 15]`; `fn keeper_tag8(b: &[u8; 32]) -> [u8; 8]`; `fn host_id(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> u64` and its inverse | W1-C | CL-21, I-02 |
| `seal` | `const DOMAIN_MARCH: &[u8] = b"PS-FRONTIER-MARCH-v1"`; `DOMAIN_POSTURE`; `DOMAIN_SALT = b"PS-SALT"`; `DOMAIN_KS = b"PS-KS"`; `struct Plain { version, host_id, arrive_bell, dest_p, dest_q, dest_tile, stance, retreat_bps, path_len, path: [u8; 12] }`; `fn pack(&Plain) -> [u8; 37]`; `fn unpack(&[u8; 37]) -> Plain`; `fn validate(&Plain, host_id, arrive_bell) -> Result<(), PlainError>`; `fn salt_of(k: &[u8; 16]) -> [u8; 32]`; `fn commit(pt: &[u8; 37], salt: &[u8; 32]) -> [u8; 32]`; `fn seal_root(commit, ct_hash) -> [u8; 32]`; `fn body_xor(k, pt) -> [u8; 37]`; `pub use super::clash::RETREAT_MAX_BPS` (defined by W1-A, v1.1); path direction order pinned (0 = E, then counter-clockwise, as `hex`), bits little-endian | W1-C | I-06, I-27, I-28 |
| `fees` | `fn priority_milli(fee: u64, cost: u64) -> u64`; `fn cost(limit: u32, sigs: u8, writes: u8, loaded: u32) -> u64`; `fn loaded_limit(programdata_len: u32, account_bytes: u32, n_accounts: u8) -> u32` (Σ + 45 + 64 per account, rounded up to 32 KiB, I-45); `fn min_tip_lamports(p_milli: u32, limit: u32, loaded: u32) -> u64` (= `ceil(p × (limit + 1,320 + 8·⌈loaded/32,768⌉)) + 2,500`); `fn defence_refund(ev: &Evidence, s: &DefenceParams) -> u64` | W1-C | CL-22, I-08, I-45 |
| `office` | `struct GovernanceParams { office_terms_per_wallet: u8 }` + `validate()`; `fn may_stand(terms_used: u8, p: &GovernanceParams) -> bool` | W1-C | CL-31 |
| `camp` | `struct Camp { tile: u8, troops: u32 }`; `fn place(ring_seed: &[u8; 32], p: ProvinceCoord, terrain: &ProvinceTerrain, day: u32, has_holding: bool, initial: bool) -> Option<Camp>`: one camp on a passable non-site tile, troops `100 + rand(0..=300)` (sim `sim.rs` l. 1131–1147); daily respawn with chance ½ in provinces with a holding (sim l. 1119–1128), evaluated lazily at the province's first resolve or skip of a game day; `initial` places one at OpenProvince for rings ≥ 2 (recorded deviation, onboarding); `fn loot() -> u32 = WORKS_CAMP` (10 Works, no goods, as the sim) | W1-C | I-56 |
| `explore` | `fn roll(seed: &[u8; 32], p: ProvinceCoord, tile: u8, host: u64, floor: bool) -> Find { works: u32 }`: `WORKS_EXPLORE` (4) always when `floor`, otherwise with chance ½ (sim l. 1370–1380); no goods in M1 (the sim has none) | W1-C | I-56 |
| `catalog` | From `frontier-sim/src/model.rs` (normative): `fn building(item: u8, n: u32, d: &Doctrine) -> Option<(Cost, Effect, u32 /*secs*/)>` over `BUILDINGS` (6 kinds: resource, per-hour production, cost) with `build_secs(n) = 3,600 + 1,800 n` and the doctrine's science bps (sim l. 1480–1505); walls: `WALL_STEP` 100 for `WALL_COST_STONE` 300 × doctrine `wall_cost`, 4 h (sim l. 1508–1530); tier-up from `tier_up`; `fn train(unit: u8, n: u32) -> Option<Cost>` = `⌈n/100⌉ × TROOP_COST_PER_100` (food, ore, gold) × the unit's production cost / 6, **no training time** (sim l. 1535–1560, 1800); `STARTER_KIT` at found; `BASE_PROD`, `tier_bonus_pct`; all tables in the ruleset hash; field-equality test against `model.rs` in `frontier-sim/tests/catalog_equality.rs` (W1-D) | W1-C | I-56 |
| *`holding::Effect::Troops`* | dropped in v1.1: Train is immediate (I-56) | — | — |
| `holding::duplicate_cost` | `fn duplicate_cost(base: u64, n: u32) -> Option<u64>` (refuses `n > MAX_DUPLICATES`) | W1-B | CL-03 |
| `holding` caps | `MAX_WALLS`, `MAX_PRODUCTION_PER_HOUR`, `MAX_UPKEEP_PER_HOUR`; `enqueue` refuses past them; `apply` clamps | W1-B | CL-02 |
| `geometry::ProvinceCoord::checked` | `fn checked(p: i32, q: i32, r_max: u16) -> Result<ProvinceCoord, OutOfBounds>` (ring ≤ r_max ≤ 128; i64 internally) | W1-B | CL-04 |
| `geometry::valid_faction` | `fn valid_faction(f: u8, allow_neutral: bool) -> bool` (0..=5, NEUTRAL = 6) | W1-B (`clash.rs` keeps a private copy; a test asserts identical semantics, so no unit edits another's file) | CL-06 |
| `host::Stamina::set` | `fn set(&mut self, b: u32, v: u16) -> Result<(), HostError>` (`TimeReversed` for `b < self.bell`) | W1-B | CL-05 |
| `siege::Vigil::request_change` | effective at the first UTC midnight ≥ `now + VIGIL_NOTICE` | W1-B | CL-09 (outcome-changing) |
| `clash::validate` | adds `ClashError::{TroopsAboveCap(id), StaminaAboveCap(id), BadMultiplier(id), BadRetreat(id), BadFaction(id)}`; `dealt_bps ∈ [BPS_ONE, COMBAT_MAX_BPS]`; retreat ≤ `RETREAT_MAX_BPS`; **`pub const RETREAT_MAX_BPS: u16 = 60_000` lives in `clash.rs`** (v1.1) | W1-A | CL-01, CL-06, I-27 |
| `clash::MAX_DAMAGE_PRODUCT_BPS` | const + `const _: () = assert!(MAX_HOST_TROOPS × product ≤ u64::MAX)`; `doctrine::validate_table` enforces it | W1-A | CL-14 |
| `clash::resolve_clash` | **Phase A** body (digest-identical); `resolve_clash_ref` kept under `#[cfg(any(test, feature = "std"))]` as the oracle | W1-A | I-14 |
| `clash::Occupancy` | `pub struct Occupancy { pub pending: [u8; FACTION_LIMIT as usize], pub storage_free: u8 }`, `ClashInput.occupancy` (`Occupancy::EMPTY` = no pending, `storage_free` 56); step 2a seeds `per_faction` and `total` with `pending` and bounces an arrival once `admitted ≥ storage_free`; same for `is_quiet` inputs (no arrivals: no effect). Failing-first test `pending_musters_and_departed_entries_bound_the_stays` | W1-A | I-43 (outcome-changing only with pending or departed entries; doctrine proxy gate re-run) |
| `clash` steps 2a/2b and 5 | cap recount after the fair share; zero-attack/zero-defence units never contest a tile (only if the failing-first tests fail at `d95fa25`) | W1-A | CL-10 (outcome-changing) |
| `payout` | `FactionLedger`, `SeasonLedger`; `PayoutParams::validate_for_season()`; `CitizenRecord::weights()` removed | W1-B | CL-08, CL-11, CL-12 |
| `pools::EntrySchedule::accrual_left` | closed form, u128 | W1-B | CL-13 |
| `mandate::MandateTerm::claim(now, …)`, `Reserve::final_sweep()` | claims close at `min(term_end + term, T_end + 72 h)` | W1-B | CL-15 |
| `index::pow_frac` | test `pow_frac_accepts_gamma_above_one` | W1-B | CL-18 |

Vectors produced by W1-C: `permutation-rules/vectors/seal-vectors-v1.json` (fixed `k`, `sigma`, plaintext, round, recorded quicknet signature → seal, commit, root; seeded from S-TLOCK `q3`/`q4`), `addr-vectors-v1.json`, `clock-vectors-v1.json` (T(b), S, ring/genesis rounds over every drand phase).

---

## 8. Off-chain interfaces

### 8.1 Shared spine (`frontier-node/crates/fclient`)

```rust
pub trait ChainPort {                                   // Rpc(urls) | Localnet(url) | InProcess(LiteSVM)
    async fn clock(&self) -> ClockSysvar;               // slot, unix_timestamp
    async fn accounts(&self, keys: &[Pubkey], min_slot: u64) -> Vec<Option<Account>>;
    async fn simulate(&self, tx: &[u8]) -> SimResult;   // logs, CU, post balances
    async fn send(&self, tx: &[u8]) -> Signature;
    async fn statuses(&self, sigs: &[Signature]) -> Vec<Option<Status>>;
    async fn feed(&self, after: Cursor) -> Vec<TxRecord>;   // program txs incl. failed, slot order
    async fn blockhash(&self) -> (Hash, u64);
}
pub trait DrandPort { async fn round(&self, r: u64) -> Option<Beacon>; fn info(&self) -> ChainInfo; }
pub struct GameClock { /* extrapolates the Clock sysvar; detects the scale; never reads the wall clock for rules */ }
```

`fclient` modules: `abi` (re-exports `frontier-abi`), `addr`, `ix` (one builder per instruction, account order = §5), `decode`, `log` (PS2 parser + head chain), `fees` (the §10.1 formulas), `tx` (compute budget + the per-kind loaded-data limit `L(kind)` from `budgets`, legacy message), `rpc`, `beacon` (drand HTTP, blstrs verify, SP-V2 hint code), `seal` (Rust `tlock =0.0.10` IBE + PS-KS body; encrypt for bots, open for keeper and verifier), `clock`, `ports`, `payers` (derive, fund, sweep; reveal and delay pools, funders, I-49). Vectors: `fclient` tests write `permutation-gateway/test/frontier-vectors.json` (codec, addresses, seal, fees, shapes).

### 8.2 Keeper (`frontier-keeper`, lib `keeper-core`)

**Duties** (all rule times from `GameClock`):

| Duty | Trigger | Deadline | Transactions | Class |
|---|---|---|---|---|
| Genesis | `genesis_round` published | 7-day abort guard | ConsumeGenesisSeed; OpenRing 0..g; OpenProvince for every genesis province | D |
| Anchor | T(b) published (0.8–2 s after its time) | none | PostAnchorMulti **and** per-region PostAnchor fallbacks (fallbacks may lag 1 slot in peacetime) | D |
| Seed caches | `S(b, r)` published | none | PostSeed(r, random unused nonce); switch nonce if not landed within 2 slots | D |
| BeaconLog | every bell | — | PostBeacon × 16 with the latest round | N |
| Decrypt | T(b) published | before `A + W` | open every seal targeted at T(b) (Rust tlock, parallel); check commit and root; a bad seal is flagged and destroyed at its SettleTransit (I-44) | — |
| Reveal | T(b) for keeper reveals; owner submissions from bell start | **never after `A + W − 2 slots`**; latch respected | Reveal per arrival, grouped by `(P, Q, b, faction)` and **sent in descending departure mass**, targeting the index `admit_arrival` computes; on `SlotMoved` re-read and retry ≤ 4 times; `QuotaRefused` / second arrival of a citizen → SettleTransit queue | **W** |
| SettleDeparture | origin resolved past the departure bell | none | SettleDeparture | D |
| Gather | `now ≥ A + W`, the province-bell has an ArrivalDay bit, every arrival's transit is in state ≥ 2 | none | GatherClash parts (≤ 10 arrivals per part); a clear bit → one gather (or SkipQuiet) | D |
| Resolve | all parts gathered, a SeedCache exists, `resolved_next == b` | none | ResolveFromInputs (CU limit from the budgets table) | D |
| Quiet skip | `resolved_next < b − 1` and the bits are clear | none | SkipQuiet (≤ 24 bells) | D |
| Settle transit | `close + 600` passed and the destination resolved | before `CloseClashInputs` grace | SettleTransit for every transit incl. quota-refused and unrevealed, with `commit` and `seal` from the DEPART log (the proof runs inside) | D |
| Tickets | `S(ticket_bell, r)` exists | cohort expiry (24 bells) | SettleTicket for every ticket of a (province, ticket_bell) cohort, **descending score, in the first slots after S**; expired tickets settled as `expired` | **D** |
| Explore | seed exists | — | SettleExplore | N |
| Archive | `A + 48 h` per region-half-day (v1.3) | — | ArchiveAnchors (tombstone, then close); CloseSeedCache | D / N |
| Closes | after resolution and settlement | — | CloseClashInputs, CloseArrivalDay, CloseArrivalSlot (after the claim grace) | N |
| Fold | every bell while joins are open | — | FoldOccupancy (2 txs, same bell) | **D** |
| Rings | crowding rule holds on the folded values | — | OpenRing → ConsumeRingSeed → OpenProvince × 6d | **D** |
| Dormancy | `now ≥ last_owner_action + release_after` | — | ReleaseDormant; DisbandStranded | N |
| Defence claims | contested bells with landed late Reveals | within `close + claim_grace` (6 bells) | ClaimDefence (≤ 6 slots per tx) | **D** |
| Sweeps | `pool_owed > 0` on a Holding | — | SweepPoolOwed | N |
| Payer care | a payer outside its pool's band (below) | — | fund from a funder / sweep to a funder (never inside a critical tx) | N |

**Bid policy (spec):** W writes start at `p_start` (= `p_tip` by default; `--peace-start f` sets `f × p_tip`) and double every slot to **P_def 2.0** (capped by `Season.defence_cap_milli`); D writes double to **P_delay 0.5**; N writes use a fixed low bid. **Resend every slot; each bid level is a new signature from a newly drawn random payer.** CU limit from the budgets table; **`SetLoadedAccountsDataSizeLimit(L(kind))` from the budgets table on every transaction** (I-45; v1.0's 65,536 would make every transaction against a ≈ 0.5–1 MB program fail `MaxLoadedAccountsDataSizeExceeded` and still pay its fee). **Retry ladder (I-50):** `ComputeBudgetExceeded` → resend at `min(2 × limit, 1,400,000)`, then 1,400,000; a heap access fault → resend with `RequestHeapFrame(262,144)`; each retry is journalled and alerted (a budget breach is a gate failure to fix, never a stuck province). Contested detection: a W or D write not landed within 2 slots at a bid ≥ `p_tip` marks `(bell, region)` contested → W writes for that region start at `p_tip`, anchor fallbacks every slot, alert, evidence journalled. Duplicate versions are bounded by `resends × (base + fee) ≤ per_write_cap`. **v1.3:** a write at its version or spend cap whose every version's blockhash has expired (151 slots) ends as a failure with a backoff and an alert (`write-expired`), and the duty re-plans it with a fresh escalation; the anchor scan window always reaches the newest bell (a bell left behind with an anchor missing is alerted, `anchor-missing`), so a long hold delays, never stalls (§8.2 "delay only").

**Payers (I-49):** two pools per keeper fleet: `pool_id = "reveal"` for class W (**N ≥ 150** enforced at start, refuses below unless `--dev`) and `pool_id = "delay"` for classes D and N (N ≥ 32); `payer_i = ed25519(sha256("PS-FRONTIER-PAYER-v1" ‖ master_seed ‖ pool_id ‖ le32(i)))`; uniform random draw per transaction version (OS CSPRNG), payers below the floor skipped, **no round-robin**. **The payer pays rent and gets it back** (`rent_to` = payer), so a pool refills as slots, days, inputs and anchors close; tips and rewards go to the configured beneficiary. **Band:** reveal floor `F_r = 3 × ⌈R99 / N⌉ × (rent(slot) + rent(day) + fee(P_def, reveal))` with R99 = the c4 v3 p99 reveals per bell for one fleet (CL-26; default 4,000 → F_r ≈ 0.215 SOL, ceiling 2 F_r; 150 payers ≈ 32–65 SOL, test SOL in M1); delay floor `F_d = 3 × (p99 per-bell D creation rent) / N_d` (default 0.5 SOL). **Effective N** = payers above the floor; below 150 the keeper alerts and tops up at once from **≥ 4 funder keys** (drawn at random, batched at rest and before any bell marked contested, never inside a critical transaction). It **does not stop sending W writes** (stopping would be the exclusion C4 guards against). Per-bell effective N goes into the liveness report and gates E5 criterion 4.

**Crash safety:** the chain is the state; SQLite journal (WAL, `synchronous=FULL` for `attempts`): `attempts(sig, kind, object_key, bell, region, class, payer, bid_milli, cu_limit, first_valid_slot, sent_slot, landed_slot, status)`, `plaintexts(host, bell, plain, salt)`, `cursor`, `claims`, `payers`. Startup: flock → reconcile in-flight with `getSignatureStatuses` → catch up ingest → rebuild queues → resume.

**Config `keeper.toml`:** `rpc = [..]`, `drand = [..]`, `roles = ["beacon","reveal","settle-departure","gather","resolve","skip","settle","tickets","explore","archive","close","fold","rings","dormancy","claims","sweep"]`, `regions = "0-15"`, `reveal_pool ≥ 150`, `delay_pool ≥ 32`, `funders ≥ 4`, `r99_reveals`, `p_def_milli ≤ season cap`, `p_delay_milli = 500`, `daily_budget_sol`, `beneficiary`, `api = "127.0.0.1:<port>"`, `token_file`.

**Loopback API** (127.0.0.1 only; bearer token from `token_file`):

| Route | Body | Answer |
|---|---|---|
| `POST /v1/reveal` | `{holding: b58, transit_slot: u8, plain_b64 (37 B), salt_b64 (32 B), ct_hash_b64 (32 B)}` | `202 {accepted: true, track: id}`; `409 {code: "CommitMismatch"}`; `410 {code: "WindowClosed"}`; `422 {code: "BadPlaintext"}` |
| `POST /v1/nudge` | `{province: [P, Q], bell}` | `{queued: true, blocking: [{kind, key}]}` |
| `GET /v1/track/{id}` | — | `{state: "queued|sent|landed|refused|expired", signature?, slot?, code?}` |
| `GET /v1/status` | — | duties per bell, lag, pool balances, spend |
| `GET /metrics` | — | Prometheus text (latencies, landed/failed per kind, contested regions, spend, beacon lag) |

### 8.3 Relay (`permutation-gateway/src/frontier/`, Node)

Reuses `cosign.mjs`, `guards.mjs`, `send.mjs`, `routes/errors.mjs`, the two-listener `app.mjs` pattern and `config.mjs`. Public listener routes (the herald proxies them as `/gw/*`):

| Route | Request | Answer |
|---|---|---|
| `GET /f/season` | — | `{programId, season, cluster, relayPool: 150, quotas, heraldUrl}` |
| `GET /f/relay` | — | `{feePayer, blockhash, lastValidBlockHeight, programId, quota: {left, resetsAt}}`; feePayer drawn uniformly from the relay pool (`pool_id = "relay"`, 150 keys) |
| `POST /f/relay` | `{tx: base64 legacy wire}` signed by every signer except the fee payer | `{ok: true, signature}` or `{ok: false, code}`; exact parse → **shape allowlist** → `signedBy` → drain guard (simulate with sigs; relay payer's Δlamports ≤ fee + allowed rent/escrow for that kind) → co-sign → send |
| `POST /f/join` | `{tx, invite?}` (wallet-signed Join; payer = relay key) | as `/f/relay`; when the season's `join_gate` is set the relay co-signs with the gate key only for a valid one-time invite, else `InviteRequired` (I-51) |
| `POST /f/reveal` | `{holding, transit_slot, plain_b64, salt_b64, ct_hash_b64}` | forwarded to the keeper `POST /v1/reveal`; answer passed through |
| `POST /f/nudge` | `{province: [P, Q], bell}` | forwarded to `/v1/nudge` |
| `GET /f/tx/{signature}` | — | `{state: "landed|failed|expired|unknown", slot?, code?}` |
| `GET /f/quota?citizen=` | — | `{left, resetsAt, lamportsLeft}` |
| `POST /f/operator/invites`, `GET /f/operator/pool` | operator token | invites; pool balances |

**Shape allowlist** (`shapes.mjs`, vectors shared with the web): exactly `[SetComputeUnitLimit(budget), SetComputeUnitPrice(0), SetLoadedAccountsDataSizeLimit(L(kind)), one Frontier player instruction ∈ {0x30–0x33, 0x40–0x46, 0x50}]` with the authority signature verified, **or** a settle shape `[SetComputeUnitLimit, SetComputeUnitPrice(0), SetLoadedAccountsDataSizeLimit(L(kind)), one of {0x47 SettleExplore, 0x54 SettleTransit}]` with no authority signature; program id matches; fee payer ∈ relay pool; the `payer` account = the fee payer; **v1.3: the message's account keys are exactly the fee payer, the instruction's accounts and the two program ids (no unreferenced key), and every key's writability is the ABI's** (writable iff the fee payer or at a writable position; program ids and read-only accounts read-only) — an unreferenced or over-writable key would be a relay-paid write lock the program never sees. **Drain guard (I-51):** simulate with signatures; the relay payer's Δlamports ≤ fee + the kind's allowance: Join = rent(Citizen); FileTicket = the Holding-rent escrow shortfall; Depart = `tip + march_fee + seal_bond` with **`tip` ∈ {tip_min, ⌈1.5 × tip_min⌉, 2 × tip_min}** (else `TipNotPreset`); every other kind 0; **`SetComputeUnitPrice` must be 0** on every sponsored shape. **Settle shapes are charged to the requester** — v1.3: `POST /f/relay {tx, requester, requesterSig, citizen}` charges `citizen:<citizen>` only when `requesterSig` is `requester`'s signature of the message **and** `citizen` is this season's canonical Citizen of its stored wallet whose wallet or unexpired session key is `requester` (read on chain); anything else, and an anonymous caller, is charged to the client-address bucket (a requester key never opens a bucket of its own) — never to the named holding's citizen, and **nothing is charged when simulation fails** (the transaction is not sent). **Any Reveal (0x51) shape → `400 UseRevealRoute`.** Refusal codes: the `cosign.mjs` `refusal()` codes plus `QuotaExceeded {retryAt}`, `InviteRequired`, `UseRevealRoute`, `TipNotPreset`, `RelayRejected`, `OperatorLowFunds` (503). Residual, accepted and reported: a player who self-reveals its own sponsored march can collect that march's preset tip (≤ 2 × tip_min ≈ 29k lamports, ≤ 24 marches a day under the quota).

**Quotas (D4):** 40 sponsored transactions per citizen per game day for days 0–6, then 20; burst 60; sponsored lamports per citizen per game day ≤ 24 Depart escrows + rent at Join and SettleTicket; per-IP limits (`/f/relay` 40 burst / 2 per s, `/f/join` 10 / 0.2 per s, `/f/reveal` 40 / 2 per s); ReplayCache 30 min; FundsGuard; loopback clients (bots) exempt from IP limits, not from quotas. **Client address (I-51):** the relay's public listener binds 127.0.0.1 only and trusts `X-Forwarded-For` only when the TCP peer is the herald's loopback address; otherwise it uses the peer address. Invites: one-time HMAC tokens (playtest preset), enforced on chain through `Season.join_gate`; AnnounceSeason is limited to the program's upgrade authority, so season ids cannot be squatted.

JS SDK (`permutation-gateway/client/src/frontier/`): `codec.mjs` (layouts from `frontier-abi` vectors), `addresses.mjs`, `seal.mjs` (plaintext, salt, commit, root, envelope; IBE in the browser worker), `fees.mjs`, `budgets.mjs`, `shapes.mjs`, `herald.mjs`; synced to `web/sdk/frontier/` by `scripts/sync-web-sdk.mjs` (`--check` freshness test).

### 8.4 Herald (`frontier-herald`, lib `herald-fold`)

| Path | Content | Cache |
|---|---|---|
| `GET /h/season` | JSON: `{v:1, programId, cluster, season, genesisTs, bellSecs, W, delta, drand: {chainHash, publicKey, period, genesis}, rulesetHash, rMax, rings: [{d, seed}], tipPriorityMilli, revealCuLimit, marchFee, sealBond, quotas, headSeq, latestSlot, latestUnix, bytes_b64 (Season account)}` | `max-age=30`, ETag |
| `GET /h/overview/{ring}/{bell}.bin` (+ `/latest.bin`) | binary, §9.3 | immutable once every province of the ring resolved `bell`; latest `max-age=5` |
| `GET /h/province/{P},{Q}/{bell}` (+ `/latest`) | JSON envelope §9.2 with the Province bytes after that bell's resolve or skip, and that bell's ArrivalSlots, ArrivalDay and ClashInputs bytes (captured before close) | immutable; latest `max-age=2` |
| `GET /h/clash/{P},{Q}/{bell}` | `{v:1, inputs_b64, seed, anchor: {key, A, round}, cache: {key, nonce}, outcomeDigest, inputDigest, decoded: {fighters, engagements, fates}, heraldCheck: "match|MISMATCH"}` (recomputed natively with `resolve_clash`) | immutable |
| `GET /h/bell/{bell}/region/{r}` | anchor (A, round, sig), `S`, caches (nonces, seed), tombstone/archived state, per-province resolved flags | immutable once seeded |
| `GET /h/me/{wallet}` | Citizen, ≤ 3 Holdings (bytes), their hosts, transit records, open slots, settled seal codes, relay quota | `no-store` |
| `GET /h/events?after={seq}` | PS2 records decoded + raw (`{seq, slot, sig, kind, body_b64}`), 500 per page | full pages immutable |
| `WS /h/ws` | client → `{op:"sub", provinces:[[P,Q]…≤64], rings:[…], wallet?, bells:true}`; server → `{seq, kind:"acct|bell|event", key, slot, head, bytes_b64}`; heartbeat 15 s; a `seq` gap → client resyncs from files | — |
| `/frontier/*` | the static web client | long max-age (hashed names) |
| `/gw/*` | proxy to the relay's public listener (forwards only `Content-Type`, adds `X-Forwarded-For`, as `permutation-server/src/play/proxy.rs`; the relay trusts it only from this loopback peer) | — |

Fold rules: every capture stamped `(slot, seq, head)`; per-bell files written atomically (temp + rename, `.br`/`.gz` siblings); **byte-identical output from the same archive** (determinism test); herald is never a trust root. Security headers as `permutation-server/src/play/http.rs`; CSP for `/frontier/*` as web §12.

### 8.5 Verifier v2 (`frontier-verify`, lib `verify-core`)

Inputs: program id, season id, RPC URL(s) or `--archive DIR` (speed-up only), pinned quicknet info, expected ruleset hash, optional `--keeper-journal`. Reads final on-chain account states at a pinned slot and requires every replayed chain to reach them (K4). Fails closed (K2): a missing transaction or undecodable record is FAIL. Exit codes: 0 PASS, 1 FAIL, 2 cannot verify.

| # | Check | FAIL codes (warn = PASS with warning) |
|---|---|---|
| V1 | entity chains (§6) | `ChainGap`, `HeadMismatch`, `DuplicateEvent`, `UnknownEntity` |
| V2 | program id, `.so` hash per slot range, ruleset hash, announce → create consistency | `ProgramMismatch`, `RulesetMismatch`, `AnnounceMismatch` |
| V3 | randomness: every anchor, cache and beacon re-verified (blstrs) against the pinned key; one anchor per (bell, region) at its canonical address incl. after archive; `S = first_round_from(A + W + Δ)`; genesis and ring rounds by rule | `BeaconSigInvalid`, `DuplicateAnchor`, `NonCanonicalAddress`, `SeedRoundRule`, `GenesisSeedRule`, `RingSeedRule` |
| V4 | windows: every landed Reveal has Clock < `A + W` and precedes the province-bell's first gather (latch) | `RevealAfterClose`, `RevealAfterLatch` |
| V5 | seals: every Depart seal opened with the stock `tlock` crate + PS-KS body + commitment + `Plain::validate`; revealed plaintext = opened; **every TRANSIT_SETTLED seal code agrees with stock decryption** (code > 0 ⇔ stock fails, commitment mismatch or invalid plaintext); **a bad seal whose transit settled with any other outcome is a FAIL** | `RevealCommitMismatch`, `VerdictDisagreesWithTlock`, **`BadSealSurvived`**; warn: `ValidSealUnrevealed`, `RevealNearClose`, `BadSealUnsettled` (a transit still unsettled at the end; E5 criterion 1 gates it) |
| V6 | quotas: final slots = the 4 largest revealed arrivals by `(mass, slot_key)`, one per citizen; dep_mass = Depart's | `QuotaSetMismatch`, `TransitMassMismatch` |
| V7 | replay: every province-bell re-run with `resolve_clash` from the logged inputs; SkipQuiet runs re-checked quiet; holdings/hosts/citizens through the kernel's lazy functions; transits (settle, bounce, rout, destroy) | `ClashReplayMismatch`, `SkipNotQuiet`, `HoldingReplayMismatch`, `TransitOutcomeMismatch` |
| V8 | lag witness: every gathered arrival's values = the origin's departure-bell resolve (via SettleDeparture) | `OriginValueMismatch` |
| V9 | accounts at canonical addresses; pre-funded addresses never blocked an init (informational) | `NonCanonicalAddress` |
| V11 | land: every SETTLE recomputes `score = rng(S(ticket_bell, r), "site", P‖Q‖site‖citizen_tag)` with S by rule; displacement only inside the cohort (same `ticket_bell`) of a provisional holding; each site's cohort winner = the highest settled score; cohort counters; PROVINCE_OPEN terrain digest = `terrain::generate_province(ring_seed, p)`; camps (initial and respawns) = `camp::place`; genesis ring seeds = `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))` | `TicketScoreMismatch`, `DisplacementRule`, `CohortMismatch`, `TerrainMismatch`, `CampMismatch`, `RingSeedRule` |
| V12 | explore: every EXPLORE_RESULT = `explore::roll(S(record.bell, r), …)` with the floor flag | `ExploreRollMismatch` |
| V13 | payments: every TRANSIT_SETTLED, SETTLE and DEFENCE_CLAIM payment by rule (recipients, amounts, `pay_or_divert`, `pool_owed`, `fees::defence_refund` within caps) FAILS on mismatch; global lamport conservation (rent, fees) is informational in M1 | `PaymentMismatch`, `DefenceRefundMismatch`; warn: `ConservationGap` |
| V10 | report | `report.json` (schema below) + `report.md` |

```json
{"verdict":"PASS|FAIL|UNVERIFIABLE","season":"…","program":"…","slot_range":[0,0],"entities":0,"bells":0,
 "counts":{"tx":0,"failed_tx":0,"seals":0,"reveals":0,"bad_seals":0,"clashes":0,"skips":0,"tickets":0,"explores":0},
 "findings":[{"code":"…","severity":"fail|warn","entity":"…","bell":0,"signature":"…","detail":"…"}],
 "liveness":{"valid_unrevealed":[],"reveals_near_close":0,"max_anchor_delay_s":0.0,"contested_bells":[]}}
```

**Tamper classes (each MUST FAIL with the named code):** T1 drop a Depart (`ChainGap`/`HeadMismatch`); T2 flip a Reveal plaintext byte (`RevealCommitMismatch`); T3 shift an anchor's A (`SeedRoundRule`); T4 inject a second anchor (`DuplicateAnchor`); T5 swap a cache signature (`SeedRoundRule`/`BeaconSigInvalid`); T6 `set_account` on a Province after a resolve (`ClashReplayMismatch`/`HeadMismatch`); T7 mark a valid seal's TRANSIT_SETTLED as bad-seal (`VerdictDisagreesWithTlock`); T8 change which slot a displacement hit (`QuotaSetMismatch`); T9 alter a departure mass (`TransitMassMismatch`); T10 wrong quicknet key (`BeaconSigInvalid`); T11 wrong ruleset hash (`RulesetMismatch`); T12 truncate the last game day (`HeadMismatch`); T13 move a Reveal past `A + W` (`RevealAfterClose`); T14 duplicate a transaction (`DuplicateEvent`); T15 origin values from a later bell (`OriginValueMismatch`); T16 wrong genesis round (`GenesisSeedRule`); **T17** a SkipQuiet over a bell that has an ArrivalSlot (`SkipNotQuiet`/`SkipOverArrival`); **T18** a tampered SETTLE score (`TicketScoreMismatch`); **T19** a tampered terrain digest (`TerrainMismatch`); **T20** a ClaimDefence amount above the formula (`DefenceRefundMismatch`); **T21** a tampered explore find (`ExploreRollMismatch`); **T22** a bad-seal transit logged as Stays (`BadSealSurvived`). **Checks of the checks:** cargo feature `mutate-<check>` disables one check; its tamper must then PASS, and the test asserts that. **Honest-but-adverse fixtures that MUST PASS:** keeper crash mid-bell, held anchor, lagging origin, displacement, low-tip rout (at `tip_min`, keeper B off), bad seal destroyed at settlement (revealed in-bell and unrevealed; settled before and after archive), ticket cohort with a displacement and an expired ticket, pre-funded addresses, archived anchors and tombstones, quota-refused arrival settled without loss, SkipQuiet runs.

### 8.6 Bots (`frontier-bots`, lib `frontier-agents`)

Profiles copied from `frontier-sim/src/model.rs` with a field-equality test (I-36); default mix = the simulator's suite mix (bots 5%); join schedule = the simulator's. Observations only from the herald (same read path as people) plus own accounts; every write through the relay (`/f/join`, `/f/relay`, `/f/reveal`). Policy: FileTicket → (keeper settles) → Harvest/Build/Train within the queue → Muster → Explore → Depart with a Rust tlock seal to `tlock_round(arrive)` (plaintext and salt journalled) → owner in-bell reveal for a profile-dependent share → SettleTransit. Pacing within the 30/h bucket. **Adversarial personas** (on by default in the exit run, each ≤ 1% of bots, expected outcome in brackets): `min_tip` (tip exactly `tip_min`, never self-reveals) [revealed by keepers; PASS]; `garbage_seal` (random 165 B with a valid commitment; its owner reveals in-bell only when the other reveals look favourable) [destroyed at SettleTransit, whatever the fight's result; verifier agrees]; `bad_plaintext` (valid seal over an invalid plaintext) [destroyed at SettleTransit, seal code 5]; `settle_racer` (garbage seal, Stays host, settles at the earliest instant and tries to Depart the host again first) [Depart refused `HostInTransit`; destroyed]; `prefunder` (lamports to future slot, day, inputs, anchor, cache addresses) [nothing blocked]; `squatter` (100-troop hosts into a faction's slots) [displaced]; `late_revealer` (Reveal at and after `A + W` and after the first gather) [refused]; `forger` (non-canonical anchor/cache/slot/inputs keys) [refused]; `spammer` (exhausts bucket and quota) [429 / `Bucket`]; `double_arrival` (two arrivals of one citizen into one province-bell) [second bounced without loss]; `zero_tip` (Depart with tip 0) [refused `TipTooLow`]; `self_tip` (sponsored Depart at 2 × tip_min, self-reveal with itself as beneficiary) [collects ≤ 2 × tip_min; reported]; `ticket_holder` (its own low-score ticket settled first, then holds the Province with `frontier_hold` at a D-capped price for 3 bells) [the higher score still displaces it; finality waits for the cohort].

### 8.7 Local chain node and drand replay

- **`frontier-localnet`**: LiteSVM 0.16 with the mainnet feature set incl. the SIMD-0388 BLS12-381 syscalls, the pinned release SBPF v2 `.so`, rent 5,080 lamports/byte, 64 account locks; block builder per slot, **400 ms of real time at every scale** (I-54), ordering by priority, ≤ 100M per block and ≤ 40M per writable account **counted in the §10.1 cost** (v1.3: CU limit + 720 per signature + 300 per write lock + 8 per 32 KiB of the loaded limit, as Agave's cost tracker; not the CU limit alone); Clock `unix_timestamp = G0 + Σ_slots 0.4 × scale` (monotone; `frontier_setScale` changes the rate at a slot boundary; the pre-season runs at scale 2,000 so AnnounceSeason's 24-h lead takes ≈ 43 s; G0 a past date so every needed round exists). **Loaded-data accounting (I-45):** a transaction whose Σ(account data + 64) incl. the LoaderV3 programdata exceeds its requested limit fails `MaxLoadedAccountsDataSizeExceeded` with the fee charged, as SIMD-0186; a control test proves it (one page below the need fails, at the need passes); if LiteSVM 0.16 does not enforce it, `localnet` checks before execution. RPC subset (web3.js 1.99 + `send.mjs` + `fclient`): `getLatestBlockhash, sendTransaction, simulateTransaction (accounts post-state), getAccountInfo, getMultipleAccounts, getSignatureStatuses, getTransaction, getSignaturesForAddress, getSlot, getBlockHeight, getBlockTime, getBalance, getMinimumBalanceForRentExemption, getProgramAccounts (memcmp), requestAirdrop, getEpochInfo, getVersion, getHealth`; WS `slotSubscribe, signatureSubscribe, logsSubscribe` (optional). Loopback extensions: `frontier_feed(after)`, `frontier_pause`, `frontier_resume`, `frontier_setScale`, `frontier_snapshot`, `frontier_restore`, `frontier_hold(keys, priority_milli, slots)` (contention emulator), `frontier_setAccount` (tamper fixtures only; refused unless started with `--allow-tamper`). Crash safety: tx WAL + snapshot every 36 game bells; restore is deterministic.
- **`drand-replay`**: serves `/{chain}/info`, `/{chain}/public/latest`, `/{chain}/public/{round}` from a verified local archive of **real historical quicknet rounds**, only when `round_time + delay ≤ game_now` (v1.2 wording; `delay` 0.8–2.0 s is quicknet's publication latency, so a round is never released early), else 425. With `ClockSource::Chain`, `game_now` is the last observed Clock `unix_timestamp`, never an extrapolation (v1.2). **Archive (I-53):** the game clock runs contiguously from G0 through the pre-season and 7 game days, so the archive is **contiguous**: ≈ 8.5 game days + margin ≈ **250,000 rounds** (≈ 40 MB JSON, ≈ 12 MB packed), fetched one round per request from the public drand HTTP endpoints at ≤ 20 requests/s per endpoint over 3 endpoints (≈ 1.2 h), verified with blstrs as fetched; only after O-M1-12. **`--test-key` mode (I-53):** rounds signed on demand by a deterministic local key (`sk = hash_to_field("PSF-TEST-BEACON-v1")`), accepted only by a program built with feature `test-beacon` (pins that key's public key and its own `QUICKNET_PK_HASH`, carries the marker `PSF_TEST_BEACON_BUILD`, refused by `build-frontier.sh` for any deployable build); the herald serves the matching chain info so bots and the web seal to it. In-process gates, nightlies and rehearsals use it; W6's real-round runs and the exit use the release `.so` and the archive (a test-key build does not test the release binary).

### 8.8 Account locks and their prices (v1.1, I-48; extends DESIGN §8.6)

"Holding" an account = listing it writable in the attacker's own transactions at priority p; the price is p × 40M CU per block for up to ~20–60 known accounts per stream [measured, SP-FEE]. Prices use DESIGN §8.7's model for 10 minutes ($150/SOL); the keeper cap sets p. Only writes count against the per-account cap; read contention was not measured.

| Account (writers) | Keeper class, cap | Effect while held | Price per 10 min | Blast radius |
|---|---|---|---|---|
| ArrivalSlots of a province-bell (Reveal) | W, 2.0 | reveals wait; routed if still held at close | $9.9k–27.2k (DESIGN §8.7) | one province-bell |
| Keeper payer (all keeper writes) | W 2.0 / D 0.5 | that payer's writes wait | whole block while ≥ 150 rotating reveal payers | none while effective N ≥ 150 (I-49) |
| Province (players P; FileTicket P; gathers, resolves, skips, settles, SettleTicket D) | D, 0.5 | resolution, settlement and ticket finality wait; the lottery winner changes only if held 24 bells (I-47) | $2.5k–6.8k; 24 bells ≈ $59k–163k | one province (delay) |
| ClashInputs (GatherClash, RFI, SettleTransit) | D, 0.5 | that resolution waits | $2.5k–6.8k | one province-bell (delay) |
| BellAnchor, SeedCache nonce | D, 0.5 | windows stay open longer; seed later; keepers switch nonce | $2.5k–6.8k | one region (delay) |
| Frontier (OpenRing, FoldOccupancy) | D, 0.5 | ring openings and folds wait; Join uses the last fold (capacity may tighten) | $2.5k–6.8k | world-wide ring growth (delay) |
| ProvinceFund(w) (OpenProvince, CloseProvince) | D, 0.5 | openings in that wedge wait | $2.5k–6.8k | one wedge (delay) |
| RingSeed(d) (ConsumeRingSeed, OpenProvince) | D, 0.5 | ring d's seed and provinces wait | $2.5k–6.8k | one ring (delay) |
| JoinShard (Join P, SettleTicket D, ReleaseDormant N) | P / D 0.5 | joins hashed there wait; ticket settlement waits | player writes ≈ free (priority 0, as DESIGN accepts); SettleTicket $2.5k–6.8k | 1/8 of one faction's joins |
| Citizen, Holding (player P; SettleTicket, SettleDeparture, SettleTransit D) | P / D 0.5 | that player waits | P ≈ free; D $2.5k–6.8k | one player |
| DefencePool (ClaimDefence D; SweepPoolOwed, dormancy, closes N) | D 0.5 / N | claims and sweeps wait; claims expire after the 6-bell grace | $2.5k–6.8k per 10 min, ≈ $15k–41k to void one grace | refunds of one bell-region |
| *Any address, by pre-funding* | — | nothing (§4.2) | — | none |

No W or D instruction except ClaimDefence writes the DefencePool, and none writes a world-wide counter other than Frontier (class D, priced above); v1.0's `[dpool w]` in every SettleTransit (a world-wide settlement stall for one account's price at a fixed low bid) is gone.

---

## 9. Web data contracts

### 9.1 Client state and storage

| Key (localStorage) | Content | Rule |
|---|---|---|
| `ps-lang` | `ja` / `en` | shared with v9, unchanged |
| `ps-fsession:<cluster>:<program>:<season>:<wallet>` | session key seed (v9 backup format) | domain `PS/frontier-session/v1`; checked against `Citizen.session` |
| `ps-fmarch:<cluster>:<program>:<season>:<wallet>` | marchbook JSON array | written **before** Depart is signed |
| `ps-fui:<cluster>:<program>:<season>` | UI prefs (fog switch, LOD, dismissed onboarding) | convenience only |

Marchbook entry (v1): `{v:1, host:"u64 dec", transitSlot, departBell, arriveBell, plain_b64, salt_b64, commit_hex, sealRoot_hex, round, tip, state:"sealed|sent|landed|revealing|revealed|settled|failed", attempts:[{t, route:"self|keeper", result}]}`. **`k` and `sigma` are never stored.** Self-reveal: at `bell_start(arrive)` + 0–20 s random, `POST /gw/f/reveal` once; again only if the slot is absent after 60 s and the window is open; never before the arrival bell starts.

**Frontier session text** (`web/frontier/fsession.mjs`, pinned byte for byte by `web-frontier-session.test.mjs`, never translated):
```
Permutation State: Sixfold Frontier wants you to create an in-game key.
Site: <origin>
Cluster: <cluster>
Program: <program id>
Season: <season id>
Anyone holding this key can act as you in this season (build, train, march, explore). It cannot move tokens from your wallet.
Sign only on <host>.
```

### 9.2 Herald JSON envelope (province, me, clash)

```json
{"v":1,"key":"pv:<P>,<Q>","bell":<b>,"slot":<s>,"seq":"<u64 dec>","head":"<hex32>","bytes":"<base64>",
 "slots":[{"key":"ar:<P>,<Q>,<b>,<f>,<i>","slot":<s>,"bytes":"<base64>"}],
 "day":{"key":"ad:<P>,<Q>,<day>","bytes":"<base64>"}|null,
 "inputs":{"key":"ci:<P>,<Q>,<b>","seq":"…","head":"…","bytes":"<base64>"}|null}
```
The client decodes `bytes` with the generated codec and ignores every decoded convenience field for anything it signs or verifies.

### 9.3 Overview binary (`/h/overview/{ring}/{bell}.bin`)

Header 32 B: `magic "PSFOV1\0\0"` · season u64 · ring u16 · n u16 · bell u32 · slot u64. Then `n` records × 24 B, sorted by (P, Q): `P i16 · Q i16 · owners 40 bits (12 × 3-bit faction: 0–5, 6 neutral/camp, 7 none) · site_state 24 bits (12 × 2: free, holding, camp, reserved/released) · hosts_by_faction [7] u8 (saturating) · flags u8 (1 clash this bell, 2 any dormant holding, 4 province opened this bell) · resolved_next u32`.

### 9.4 Transaction shapes the client builds

| Instruction | Signers besides the relay fee payer | Notes |
|---|---|---|
| Join | **wallet** | the only wallet signature; session pubkey in the data |
| SetSession | wallet | |
| FileTicket, SetVigil, Harvest, Build, Train, Muster, Dissolve, Garrison, Explore, Depart | session key | `payer` (account 1) = relay fee payer; priority 0; CU limit from `budgets.mjs` |
| Reveal | none — not a client transaction | `POST /gw/f/reveal` with material (I-24) |
| SettleTransit, SettleExplore | none (the relay fee payer is `payer`) | offered as buttons after their time; sent through `/gw/f/relay` as the settle shapes of §8.3 (charged to the requester's own quota); SettleTransit's data carries `commit` and `seal` from the DEPART log; keepers normally do them first, and a second attempt fails harmlessly |

Before signing, the client checks the message byte for byte (program id, accounts recomputed, no bump from anyone, the announced fee payer, recent blockhash); v1.3: `fchainio.messageProblems` requires the blockhash GET /f/relay gave and the Frontier instruction the page built from recomputed addresses (`expected`), compares the message's instruction to it account by account and byte for byte, requires the `payer` account to be the fee payer and runs the relay's own allowlist (`classify`); the seal's round is T(arrive_bell) of the season clock, never a caller's input (`sealMarch` refuses another, `WrongRound`); after the relay answers it fetches the landed transaction through the herald and compares. Depart ≤ 800 B asserted in `web-frontier-march.test.mjs`; the composer offers only the three tip presets (§8.3); FileTicket shows the refundable Holding-rent escrow; a displaced ticket offers a one-tap refile (I-47).

### 9.5 WASM exports (`frontier-wasm`, C ABI, borsh in/out over linear memory, `alloc`/`free`)

`ruleset_hash`, `province_of`, `province_centre`, `ring_of`, `wedge_of`, `region_of`, `generate_province(ring_seed, P, Q)`, `plan_path(start, dest, unit, blocked)`, `path_cost`, `earliest_arrival_bell`, `check_arrival_bell`, `bell_at`, `bell_start`, `tlock_round`, `seed_round`, `plaintext_pack`, `plaintext_unpack`, `plaintext_validate`, `commit`, `salt_of`, `body_xor`, `resolve_clash`, `resolve_from_inputs(ClashInputs bytes, seed)`, `reachable(origin, depart_bell, target_bell, unit)`, `accrual_at(Accrual, t)`. Build: `scripts/build-wasm.sh` (1.95.0, `wasm32-unknown-unknown`, `opt-level="s"`, `panic="abort"`, `--remap-path-prefix`), output `web/frontier/wasm/frontier.wasm` + `.sha256`; budget ≤ 400 KB raw / ≤ 150 KB gzip.

### 9.6 Program errors in the UI

`web/frontier/fi18n.mjs` maps every code of §5.4 to JA and EN text (generated table from `frontier-abi/vectors/errors.json`); `web-frontier-errors.test.mjs` fails on a missing code.

---

## 10. Operations constants

### 10.1 Fee and priority formulas (pinned; `fees` kernel, `fclient::fees`, `fees.mjs` share vectors)

```
cost      = cu_limit + 720·n_sig + 300·n_write_locks + 8·ceil(L / 32,768)
L(kind)   = round_up(programdata_len + 45 + Σ_accounts (data_len + 64), 32,768)      at the kind's worst account set (SIMD-0186, I-45)
            programdata_len = the deployed max_len; deploy with --max-len = round_up(1.25 × .so size, 4,096)
priority  = (priority_fee + 2,500) / cost                                                  (lamports per cost unit)
fee(p)    = max(0, p·cost − 2,500);  cu_price_µl = ceil(fee(p)·10⁶ / cu_limit)
p_tip     = (tip − 2,500) / cost_reveal
tip_min   = ceil(p_min × (reveal_cu_limit + 1,320 + 8·ceil(L_reveal / 32,768))) + 2,500
            (p_min = 0.433, L_reveal = 1 MiB until measured: 14,441 at 26k, 10,111 at 16k)
```

v1.0 pinned `L = 65,536` (+16) from SP-FEE, whose probe program is 13,600 B; SP-V2's program is already 480,512–540,608 B [measured], and SIMD-0186 counts the programdata, so every M1 transaction would have failed and still paid its fee. v1.0's "10,006 at 16k" also rounded down (integer ceil gives 10,007). The C4 priorities move by < 1% (cost +240 per Reveal).

### 10.2 Budgets table

`frontier-abi/src/budgets.rs` holds, per instruction kind: CU budget (§5.5), CU limit to request (= measured max + 5%, never above the budget; only the keeper's retry ladder goes above it, I-50), loaded-data limit `L(kind)` (§10.1, from the release `.so` and its max_len), tx byte ceiling, heap ceiling (28,672 B at gated fills; allocator ceiling 262,144 B), lock ceiling. Generated to `frontier-abi/vectors/budgets.json` → `budgets.mjs`. The wave-5 gate regenerates the CU limits from the G1 measurements and `L(kind)` from the release `.so`; every earlier gate regenerates `L(kind)` from the current build.

### 10.3 Ports (none on the reserved list 4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191)

| Service | Port |
|---|---|
| frontier-localnet JSON-RPC / WS | 41010 / 41011 |
| drand-replay | 41020 |
| relay operator (loopback) / public | 41030 / 41033 |
| herald (serves `/frontier/`, `/h/*`, `/gw/*`, WS) | 41040 |
| keeper A (operator roles) API / metrics | 41050 |
| keeper B (public profile: reveal, prove, settle) | 41051 |
| bots control / metrics | 41070 |
| viewer load generator stats | 41075 |
| Mode R (if approved) and the I-45 validator drill (`solana-test-validator 3.1.9`, already installed): RPC / pubsub / gossip / faucet / dynamic range | 41080 / 41081 / 41085 / 41086 / 41100–41140 |
| nightly smoke stack (second instance) | base + 500 (41510 …) |
| per-unit ad-hoc dev servers | unit k (1..36) uses 41600 + 10·k … + 9 |

Tests and fixture servers bind `127.0.0.1:0`. `frontier-stack` refuses to start if a configured port is reserved, outside 41000–41999, or already bound (`lsof -nP -iTCP:<port> -sTCP:LISTEN`).

---
## 11. Implementation units and file ownership

Rules: a unit owns exactly the paths listed for its wave (globs are recursive); every other path is read-only for it. Paths not listed anywhere in a wave are frozen during that wave, except for the integrator's files and integration window (§3.4, I-55): **the integrator owns every dependency section of every `Cargo.toml`, every `Cargo.lock`, every `package.json` and `package-lock.json`, every `rust-toolchain.toml` and `.gitignore`**, and units request changes to them in their notes. `frontier-abi/src/layout/**` is **frozen after wave 1** and `permutation-frontier/src/layout/**` after wave 2 (changes only by contract amendment, §16). Files created as stubs by one unit and "handed over" belong to the named later unit from its wave on. Within a wave, units that depend on another unit's new API code against the signatures pinned in §5–§9; the integrator merges in the listed order. Each unit writes `docs/frontier/m1/<unit-id>-NOTES.md` (what landed, measurements, deviations, dependency requests). Dates assume a start on Monday 2026-09-28.

### Wave 1 — Foundations and the M0 closeout (weeks 1–1.5, 2026-09-28 → 10-07). Merge order: W1-B, W1-A, W1-C, W1-E, W1-F, W1-D

| Unit | Brief | Owns |
|---|---|---|
| **W1-A rules-clash** | CL-01 (`clash::validate` bounds, faction ≤ 6; **`RETREAT_MAX_BPS = 60,000` defined here**, I-27), CL-06 (clash part), CL-14 (damage-product const assert + `doctrine::validate_table`), CL-10 (failing-first tests `a_bounce_by_fair_share_frees_a_cap_slot`, `scouts_do_not_contest_a_tile`; fix only if they fail), **I-43 `clash::Occupancy`** (failing-first test `pending_musters_and_departed_entries_bound_the_stays`; digests identical with `Occupancy::EMPTY`), **Phase A** from `m1/lab/clash-opt/phaseA.patch` with `resolve_clash_ref` kept and the 4,320-input equivalence test promoted (+ the refund `d > 0` corner; mutation set re-run), CL-29 screen of the 40 SP-V2 fills + 1,200 generated fills on SBF v2 in lab `m1/lab/rfi-search/` (SP-V2 write-back; the full M1 write-back is W4-A's); doctrine proxy gate and criterion re-run with deltas vs m0c whenever CL-10 or I-43 changes an outcome | `permutation-rules/src/frontier/{clash,stance,doctrine}.rs`; `permutation-rules/tests/{frontier_clash_bounds,frontier_clash_equiv}.rs`; `scratchpad/frontier/m1/lab/rfi-search/**`; `docs/frontier/m1/W1-A-NOTES.md` |
| **W1-B rules-bounds** | CL-02 (walls/production/upkeep caps), CL-03 (checked `duplicate_cost`), CL-04 (`ProvinceCoord::checked`, i64 ring), CL-05 (`Stamina::set` monotone), CL-06 (`geometry::valid_faction`, siege), CL-08 (per-faction Ledger type), CL-09 (vigil at UTC midnight; outcome-changing → gate re-run), CL-11 (`validate_for_season`), CL-12 (remove `weights()`), CL-13 (closed-form `accrual_left`), CL-15 (Mandate claim deadline, `final_sweep`), CL-18 (γ test). No `Effect::Troops` (I-56) | `permutation-rules/src/frontier/{holding,host,geometry,siege,index,payout,pools,mandate,laurel,travel,terrain}.rs`; `permutation-rules/tests/{frontier_bounds,frontier_world,frontier_economy}.rs`; `docs/frontier/m1/W1-B-NOTES.md` |
| **W1-C rules-shared** | New pure modules per §7: `beacon` (CL-19/20), `addr` (CL-21, SP-V2 grammar I-02), `seal` (I-06/27/28; re-exports `clash::RETREAT_MAX_BPS`), `fees` (CL-22; `loaded_limit`, I-45), `office` (CL-31 kernel), **`camp`, `explore`, `catalog` from `frontier-sim/src/model.rs` and the `sim.rs` lines named in §7 (I-56)**; vectors `seal-vectors-v1.json` (recorded quicknet signatures, S-TLOCK q3/q4 seeds), `addr-vectors-v1.json` (SP-V2 byte for byte), `clock-vectors-v1.json`; the `RULESET_HASH` input function | `permutation-rules/src/frontier/{mod,beacon,addr,seal,fees,office,camp,explore,catalog}.rs`; `permutation-rules/vectors/**`; `permutation-rules/tests/frontier_shared.rs`; `docs/frontier/m1/W1-C-NOTES.md` |
| **W1-D closeout-sim-docs-ci** | frontier-sim: CL-07 (bounds with mutation controls), CL-08 (sim side), CL-12 callers, CL-16 (`--first-seed`), CL-31 (term limit default 1, vacancy rule, re-runs), CL-30 (c4 counts → D18 table), CL-26 (c4 v3 600/1,200 + relic tip; **p99 reveals per bell R99 for the payer band, I-49**), CL-32/CL-33 measured choices, **`frontier-sim/tests/catalog_equality.rs`** (rules `catalog` == `model.rs`, I-56), the doctrine and criterion re-runs requested by W1-A/W1-B; docs: CL-19..CL-27 DOC parts in DESIGN, `DECISIONS.md` (v1.1), CL-27 SP-FEE correction (incl. the loaded-data limit, I-45), CL-37 README/SUMMARY lines, CL-38 `M0-CLOSE.md` at wave end (records the N5 overshoot: week 1.5, not 1); CI: CL-36 job definitions, CL-34 run record; CL-35 legacy `civilization` tests fixed (option a) — **no push** | `frontier-sim/**` (manifest dependency sections: integrator); `docs/frontier/**` (except other units' `docs/frontier/m1/*-NOTES.md`); `.github/workflows/**`; `permutation-state-prototype/civilization/**`; `scratchpad/frontier/m1/lab/{c4-v3,d18,d22,d24}/**` |
| **W1-E abi** | `frontier-abi` complete: layouts §5.3 (v1.1), tags and data §5.5–§5.12, errors §5.4, logs §6, budgets (placeholders + the `L(kind)` worst account sets), presets `M1_LOCAL_7D` and `M1_PLAYTEST`, **`prologue` (account-list tables and checks as pure functions over byte slices) and `entry` (Province entry ↔ Host codec, host-id inverse)** used by the program and off chain (I-55), the vector writer bin `abi-vectors`, address vectors; with the integrator, the root-workspace membership decision (I-37) | `frontier-abi/**`; `docs/frontier/m1/W1-E-NOTES.md` |
| **W1-F node-foundation** | `frontier-node` workspace layout and skeletons for all eleven crates; `fclient` complete (§8.1: ports, abi glue, ix builders for every tag, decoders, PS2 parser, fees incl. `L(kind)`, tx, rpc, beacon with SP-V2 hints and blstrs verify, seal with `tlock =0.0.10` and both-way interop vectors, clock, payers with two pools and funders); `localnet` MVP (RPC subset, **real 400-ms slots with scaled game seconds** (I-54), **SIMD-0186 loaded-data enforcement with its control test** (I-45), LiteSVM with BLS syscalls, feed); `drand-replay` MVP on the SP-V2 fixture rounds **and `--test-key` mode** (I-53); `fclient` writes `permutation-gateway/test/frontier-vectors.json` | `frontier-node/crates/**`; `frontier-node/README.md`; `permutation-gateway/test/frontier-vectors.json`; `docs/frontier/m1/W1-F-NOTES.md` |

**Integrator in wave 1:** manifests and lockfiles; sends the O-M1-12 ask on day 1 (§15); runs the Gate W1 overnight items.

### Wave 2 — Program core, harness, localnet, relay, web foundation, keeper beacons (weeks 1.5–3, 10-07 → 10-19). Merge order: W2-A, W2-B, W2-C, W2-F, W2-D, W2-E

| Unit | Brief | Owns |
|---|---|---|
| **W2-A program-core** | `permutation-frontier` skeleton: entrypoint and dispatch for **every** tag (stubs → `NotImplemented` in every `src/proc/*.rs`, handed over in waves 3–4), `error`, `ix`, `addr`, `init` (`init_with_seed`, `init_funded`, `close_to`, `pay_or_divert` with sinks), `clock`, `events` (PS2 + heads), `evidence`, `heap` (256-KiB-aware bump allocator, I-50), `prologue` (wrapper over `frontier-abi::prologue`), `layout/*`, `crypto/*` (SP-V2 port + the seal opener SettleTransit uses + feature `test-beacon`); implemented: AnnounceSeason (upgrade authority, I-51), CreateSeason (6 wedge funds, `join_gate`, `reveal_loaded_limit`), InitBeaconLogs, InitShards, ConsumeGenesisSeed, SetWindowSchedule, PostAnchor, PostAnchorMulti (`MULTI_MAX_REGIONS` pinned by a tx-size test), PostSeed, PostBeacon; `scripts/build-frontier.sh` (+ `--twice`; refuses the `oracle`, `trace` and `test-beacon` markers; prints the programdata length and the `--max-len` to deploy with) | `permutation-frontier/src/**`; `permutation-frontier/Cargo.toml` (non-dependency fields); `scripts/build-frontier.sh`; `docs/frontier/m1/W2-A-NOTES.md` |
| **W2-B svm-harness** | `permutation-frontier/svm-tests`: `chain` (LiteSVM wrapper with a SIMD-0186 enforcement control), `fixtures` (SP-V2 beacons, tlock vectors, **test-beacon rounds**), `records`, `budget` (CU, heap, tx bytes, locks, loaded data), `world/mod.rs` + area builders `world/{land,holding,clash,transit}.rs` (stubs, handed over), `cover/mod.rs` registry + area files `cover/{season,beacon,map,citizen,holding,host,reveal,clash,transit,defence}.rs` (stubs, handed over), `ix/` area files (stubs), seeded wallets (CL-28), `run.sh`; g02–g05 and `g01_loaded_limit_*` for W2-A's instructions; **the one-time I-45 validator drill** (`solana-test-validator 3.1.9` on 41080–41089, a padded non-BLS probe of the release `.so` size: programdata counts toward the limit, the fee is charged on failure) | `permutation-frontier/svm-tests/**`; `docs/frontier/m1/W2-B-NOTES.md` |
| **W2-C localnet-complete** | `frontier-localnet`: block builder with the 100M/40M caps and priority ordering, contention emulator (`frontier_hold`), snapshot + WAL restore, full RPC subset + conformance tests for the calls `send.mjs`/web3.js use, `frontier_setScale` at slot boundaries; `drand-replay`: archive format, verification, gating, prefetch tool (the download waits for O-M1-12), test-key round service | `frontier-node/crates/{localnet,drand-replay}/**`; `docs/frontier/m1/W2-C-NOTES.md` |
| **W2-D relay-sdk** | Relay routes, shapes (§8.3 v1.1: tip presets, CU price 0, `L(kind)`, FileTicket escrow allowance, requester charging, no charge on a failed simulation), quotas, invites + `join_gate` co-signing, payer pool (150, `pool_id = relay`), drain guard, keeper link (`/f/reveal`, `/f/nudge`), `UseRevealRoute`, the `X-Forwarded-For` rule; JS SDK `client/src/frontier/*`; `sync-web-sdk.mjs` extended (+ `--check`); **`web-sdk.test.mjs` extended for `sdk/frontier` and the noble manifest** | `permutation-gateway/src/frontier/**`; `permutation-gateway/client/src/frontier/**`; `permutation-gateway/test/frontier-*.test.mjs`; `permutation-gateway/test/web-sdk.test.mjs`; `permutation-gateway/scripts/sync-web-sdk.mjs`; `permutation-server/web/sdk/frontier/**` (generated); `docs/frontier/m1/W2-D-NOTES.md` |
| **W2-E web-foundation** | `frontier-wasm` crate + `scripts/build-wasm.sh` (**`--check` is `PENDING-OWNER` until the wasm32 target is approved**; until then the exports are tested on the host); `scripts/vendor-noble.mjs` + vendored tree with manifest; pages skeleton (`index`, `practice`, `spectate`), `app`, `fstate`, `config`, `herald.mjs`, `fchainio`, `fsession` (pinned text), `clock`, `wasm.mjs`, `seal.mjs` + `seal-worker.mjs` (IBE + self-audit; chain info from `/h/season`, so the test key works), `marchbook`, `fi18n`, `map/fmap.mjs` + `map/layers.mjs`; `lang/en-frontier.mjs`, empty `lang/en-frontier-play.mjs`, `EN_GROUPS` registration (additive), `map.mjs` additive exports, GLOSSARY section; tests `web-frontier-{seal,codec,clock,herald,marchbook,session,wasm}.test.mjs`; **`web-lang.test.mjs` extended over `web/frontier/**`**; synthetic herald fixtures | `frontier-wasm/**` (lock: integrator); `scripts/{build-wasm.sh,vendor-noble.mjs}`; `permutation-server/web/frontier/**`; `permutation-server/web/sdk/vendor/noble/**`; `permutation-server/web/lang/{en-frontier.mjs,en-frontier-play.mjs,GLOSSARY.md}`; `permutation-server/web/lang.mjs` (additive); `permutation-server/web/map.mjs` (additive); `permutation-gateway/test/web-frontier-*.test.mjs`; `permutation-gateway/test/web-lang.test.mjs`; `permutation-gateway/test/fixtures/frontier/**`; `docs/frontier/m1/W2-E-NOTES.md` |
| **W2-F keeper-core** | `findex` (ingest RpcPoll + LocalnetFeed, append-only archive with manifest, SQLite entity index, account snapshots); keeper: genesis, anchors (combined + fallbacks), seed caches with nonce switch, PostBeacon, archive, payer pools (reveal ≥ 150, delay ≥ 32, χ² uniformity test, rent returning to the payer), funders ≥ 4, escalation engine (W/D/N, ×2 per slot, new payer per version, the CU and heap retry ladder), journal, loopback API skeleton; in-process `one_day_beacons` over `localnet` **with the test-beacon key** | `frontier-node/crates/{findex,keeper,fclient}/**`; `docs/frontier/m1/W2-F-NOTES.md` |

### Wave 3 — Land, holdings, the Reveal measured, keeper land, herald, bots, web play (weeks 3–5, 10-19 → 11-02). Merge order: W3-A, W3-B, W3-C, W3-D, W3-E, W3-F

| Unit | Brief | Owns |
|---|---|---|
| **W3-A program-land** | OpenRing, ConsumeRingSeed, OpenProvince (status sets, wedge funds, initial camp), FoldOccupancy (+ fund shards), CloseProvince, Join (join gate), SetSession, SetVigil, FileTicket (cohorts, escrow), SettleTicket (fresh / displace / taken / expired, cohorts, funder), ReleaseDormant, CloseHolding, CloseCitizen; g02/g03/g13 rows for Citizen, Holding, Province, RingSeed, JoinShard, ProvinceFund; cohort tests (displacement without a deadline, a Province held through `final_ts`, expiry); re-creation tests for province, holding and citizen; OpenProvince ≤ 220k | `permutation-frontier/src/proc/{map,citizen}.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{map,citizen}.rs`; `permutation-frontier/svm-tests/src/world/land.rs`; `permutation-frontier/svm-tests/tests/{map,citizen}.rs`; `docs/frontier/m1/W3-A-NOTES.md` |
| **W3-B program-holding-march** | Harvest, Build, Train (immediate), Muster, Dissolve, Garrison, Explore, SettleExplore, DisbandStranded, Depart (min tip, max stamina, `HostInTransit`), SettleDeparture, **Reveal** (full check order, latch, ArrivalDay, evidence, displacement resets `claimed`) with the **worst-case measurement** (CU, tx bytes and `L(reveal)` against the current release `.so`: 32 steps, 4 provinces, displacement, first-of-bell ArrivalDay, adversarial slot fill) → `reveal_cu_limit` and `reveal_loaded_limit` in the presets via the integrator (amendment) and CL-22's final tip; the two optional Reveal cuts if the worst case exceeds 20k | `permutation-frontier/src/proc/{holding,host,reveal}.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{holding,host,reveal}.rs`; `permutation-frontier/svm-tests/src/world/holding.rs`; `permutation-frontier/svm-tests/tests/{holding,host,reveal}.rs`; `docs/frontier/m1/W3-B-NOTES.md` |
| **W3-C keeper-land** | Rings, fold and openings (class D), tickets (cohort burst in descending score in the first slots after S; expiry), explore settlement, dormancy and stranded hosts, sweeps, the `/v1/reveal` accept path (queue only; the pipeline is W4-C's); in-process tests over `localnet` with the test key | `frontier-node/crates/{keeper,fclient}/**`; `docs/frontier/m1/W3-C-NOTES.md` |
| **W3-D herald** | Fold (per-bell closing, atomic immutable files, overview `.bin`, clash reports recomputed natively, bell/region files), `/h/*` routes, WS fan-out with seq and resync, `/frontier/*` static, `/gw/*` proxy, security headers/CSP, checkpoint/restart; viewer load generator bin `frontier-viewers` | `frontier-node/crates/{herald,findex}/**`; `docs/frontier/m1/W3-D-NOTES.md` |
| **W3-E bots-agents** | `frontier-agents` (profiles copied + equality test, Observation, policies) and `frontier-bots` (1,000 bots per process, relay transport, Rust tlock sealing incl. the test key, owner reveals, the 13 personas of §8.6) with unit tests against recorded herald fixtures; the system test is W4-F's | `frontier-node/crates/{agents,bots}/**`; `docs/frontier/m1/W3-E-NOTES.md` |
| **W3-F web-play** | Shell (bell chip, quota chip, bottom tabs), join/faction/site picker (escrow shown, refile after displacement), holding panel, host panel + Explore, march composer (destination, WASM path, arrival bell, stance, retreat list with "never" = 0, **the three tip presets**, no zero tip), send flow (seal → audit → marchbook → Depart), tracker (host locked until settled: `HostInTransit` copy), incoming-arrival warnings, bell sheet, chronicle; tests `web-frontier-{march,relay,errors}.test.mjs` | `permutation-server/web/frontier/**` except `wasm/**`; `permutation-server/web/lang/en-frontier.mjs`; `permutation-gateway/test/web-frontier-{march,relay,errors}.test.mjs`; `docs/frontier/m1/W3-F-NOTES.md` |

### Wave 4 — Clash, transit, keeper play, verifier, web report, first system integration (weeks 5–7, 11-02 → 11-16). Merge order: W4-A, W4-B, W4-C, W4-D, W4-E, W4-F

| Unit | Brief | Owns |
|---|---|---|
| **W4-A program-clash** | GatherClash (tombstone rule I-46, Holdings, ArrivalDay fast path), ResolveFromInputs (Phase A, `Occupancy` room, write-back, fates, camps, evidence), ResolveClash (oracle feature), SkipQuiet (CU-aware stop), CloseClashInputs, CloseArrivalDay, CloseArrivalSlot; gates G6, G8 (+ the storage fill), G9, G11 (+ churned rosters); **RFI with the full M1 write-back over all 1,240 fills on SBF** (≤ 340k, heap ≤ 28 KiB) | `permutation-frontier/src/proc/clash.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/clash.rs`; `permutation-frontier/svm-tests/src/world/clash.rs`; `permutation-frontier/svm-tests/tests/clash.rs`; `docs/frontier/m1/W4-A-NOTES.md` |
| **W4-B program-transit-lifecycle** | SettleTransit (the seal proof, every D5 branch, `pool_owed`, claim grace), SweepPoolOwed, ClaimDefence (Reveal-only, grace), ArchiveAnchors (signatures in entries, tombstone first), CloseSeedCache, EndSeason, AbortSeason (bond rules), CloseSeason (parts, fund shards, tombstone); gates G10 (seal code vs stock tlock, before and after archive), G12 (every branch incl. settle-races-proof, Stays host re-departs before settlement, claim after settle, claim after displacement) | `permutation-frontier/src/proc/{transit,defence,beacon,season}.rs`; `permutation-frontier/svm-tests/src/{ix,cover}/{transit,defence,beacon,season}.rs`; `permutation-frontier/svm-tests/src/world/transit.rs`; `permutation-frontier/svm-tests/tests/{transit,defence,archive,lifecycle}.rs`; `docs/frontier/m1/W4-B-NOTES.md` |
| **W4-C keeper-play** | Decrypt at T(b), reveal pipeline (mass order, slot index, `SlotMoved` retry, never after `A + W − ε`, latch), owner submissions via `/v1/reveal`, SettleDeparture, gather/resolve DAG, SkipQuiet catch-up (CU-aware), SettleTransit with the logged commit and seal (incl. refused and unrevealed), closes after the claim grace, claims, crash injection (≈ 20 points × 3 duty kinds), duplicate keepers, **program-level lag gate in process** | `frontier-node/crates/{keeper,fclient}/**`; `docs/frontier/m1/W4-C-NOTES.md` |
| **W4-D verifier** | `frontier-verify` V1–V13, report schema, exit codes, T1–T22 on recorded test-beacon mini-seasons, `mutate-<check>` features and `mutate.sh`, honest-but-adverse fixtures | `frontier-node/crates/verify/**`; `frontier-node/fixtures/**`; `docs/frontier/m1/W4-D-NOTES.md` |
| **W4-E web-report-practice** | Clash report + "verify in this browser", practice mode (scenarios, rule bot, what-if), onboarding state machine and card, spectator page; **routes the new screens in `app.mjs`** | `permutation-server/web/frontier/screens/{report,practice,onboarding,spectate}.mjs`; `permutation-server/web/frontier/{practice.html,spectate.html,onboarding.mjs,app.mjs}`; `permutation-server/web/lang/en-frontier-play.mjs`; `permutation-gateway/test/web-frontier-{practice,onboarding}.test.mjs`; `docs/frontier/m1/W4-E-NOTES.md` |
| **W4-F integration** | The `itest` crate: `inproc_day` (100 bots + keeper + herald fold over `ChainPort::InProcess`, test key, one game day; a first run with stubs for units not yet merged, the final run after all merges); fixes in herald, findex, agents, bots, localnet and drand-replay found by it | `frontier-node/crates/{itest,herald,findex,agents,bots,localnet,drand-replay}/**`; `docs/frontier/m1/W4-F-NOTES.md` |

### Wave 5 — Hardening (weeks 7–8.5, 11-16 → 11-25). Merge order: W5-A, W5-D, W5-B, W5-C, W5-E

| Unit | Brief | Owns |
|---|---|---|
| **W5-A program-gates** | Complete G1–G13 (§13.1–§13.3 with the v1.1 fills), the (instruction, error) coverage table with `RELEASE_CHECK=1`, budgets regenerated (CU from G1; `L(kind)` from the release `.so` and its max_len), G7 lag gate in LiteSVM, heap/tx/lock/loaded-data assertions; **Phase B evaluation** (lab `m1/lab/phaseB-gate`: doctrine proxy gate, 1,500-season band overnight, golden digests) → adopt or drop, recorded as an amendment | `permutation-frontier/**`; `frontier-abi/src/budgets.rs`; `frontier-abi/vectors/budgets.json`; `permutation-rules/src/frontier/clash.rs` (Phase B only); `scratchpad/frontier/m1/lab/phaseB-gate/**`; `docs/frontier/m1/W5-A-NOTES.md` |
| **W5-B stack** | `frontier-stack` (`check-ports/up/verify/tamper/report/load/down`, the §12 port rule, pre-season at scale 2,000, start order, chaos, the adversary schedule incl. the ticket hold, Frontier and ProvinceFund holds and payer holds, the scale-2 latency config, `--beacon test-key|archive`, run report), configs, `scripts/m1-nightly.sh` (test key, 100 bots × 1 game day at 100×, ports 415xx) | `frontier-node/crates/{stack,localnet,drand-replay}/**`; `frontier-node/configs/**`; `scripts/m1-nightly.sh`; `docs/frontier/m1/W5-B-NOTES.md` |
| **W5-C node-integration** | Fixes from the first smoke runs across keeper, herald, findex, fclient, agents, bots, itest; relay and JS SDK fixes; herald 5,000-viewer load test (p99 targets §13.4); **G14** (100 bots × 2 game days through the program in LiteSVM with the native kernel re-run every bell, test key; verifier PASS; one tampered log → FAIL) | `frontier-node/crates/{keeper,herald,findex,fclient,agents,bots,itest}/**`; `permutation-gateway/src/frontier/**`; `permutation-gateway/client/src/frontier/**`; `permutation-gateway/test/frontier-*.test.mjs`; `permutation-server/web/sdk/frontier/**`; `docs/frontier/m1/W5-C-NOTES.md` |
| **W5-D verifier-tamper** | T1–T22 on stack runs, `mutate.sh` over every check, fixtures regenerated from the nightly | `frontier-node/crates/verify/**`; `frontier-node/fixtures/**`; `docs/frontier/m1/W5-D-NOTES.md` |
| **W5-E web-a11y-screens** | Phone-first layout pass (360 px, 44 px targets, bottom sheets), WCAG 2.2 AA fixes on the wave-3/4 screens and the map, screenshot smoke package (Playwright + axe, fixture server on port 0, 10 screens × JA/EN × 3 viewports; `PENDING-OWNER` until Chromium is approved), JA/EN copy review, `web-lang` completeness | `permutation-gateway/screens/**` (package files: integrator); `permutation-server/web/frontier/frontier.css`; `permutation-server/web/frontier/map/**`; `permutation-server/web/frontier/screens/{shell,join,holding,host,march,tracker,incoming,bell,explore,chronicle}.mjs`; `permutation-server/web/lang/{en-frontier.mjs,GLOSSARY.md}`; `permutation-gateway/test/web-lang.test.mjs`; `docs/frontier/m1/W5-E-NOTES.md` |

### Wave 6 — Season runs and fixes (weeks 8.5–10.5, 11-25 → 12-09). Merge order: W6-B, W6-C, W6-D, W6-A, W6-E

| Unit | Brief | Owns |
|---|---|---|
| **W6-A season-ops** | Nightly smokes (test key); the **scale-2 latency run** (6 game hours); the **first 7-day, 1,000-bot Mode A run** at 20× with chaos and 5,000 viewers for one game day — with real rounds if O-M1-12 is approved, otherwise a test-key rehearsal marked "not exit-grade"; triage lists to W6-B/C/D; run reports (committed summaries) | `frontier-node/crates/stack/**`; `frontier-node/configs/**`; `docs/frontier/m1/runs/**`; `docs/frontier/m1/W6-A-NOTES.md` |
| **W6-B program-fix** | Program, ABI and kernel fixes from the runs (outcome-changing kernel fixes re-run the doctrine gate and the criterion); Phase B commit if adopted | `permutation-frontier/**`; `frontier-abi/**`; `permutation-rules/src/frontier/**`; `permutation-rules/tests/frontier_*.rs`; `docs/frontier/m1/W6-B-NOTES.md` |
| **W6-C node-fix** | Keeper, herald, verifier, bots, localnet, drand-replay, relay fixes | `frontier-node/crates/{fclient,findex,keeper,herald,verify,agents,bots,localnet,drand-replay,itest}/**`; `frontier-node/fixtures/**`; `permutation-gateway/src/frontier/**`; `permutation-gateway/client/src/frontier/**`; `permutation-gateway/test/frontier-*.test.mjs`; `permutation-server/web/sdk/frontier/**` (generated); `docs/frontier/m1/W6-C-NOTES.md` |
| **W6-D web-fix-playtest-readiness** | Web fixes, the scripted onboarding run (Playwright driving the dev wallet on the local stack at 41040, 390 px, JA and EN), spectator 24-h memory check, fixtures regenerated from the local season | `permutation-server/web/**` except `session.mjs` and `sdk/frontier/**`; `permutation-gateway/test/fixtures/frontier/**`; `permutation-gateway/screens/**` (package files: integrator); `permutation-gateway/test/web-*.test.mjs` except `web-sdk.test.mjs`; `frontier-wasm/**`; `docs/frontier/m1/W6-D-NOTES.md` |
| **W6-E c4-docs** | c4 model v3 with the **measured Reveal CU distribution and `L(reveal)`** (CL-26/30 final), D18 table, payer-band R99; DESIGN updates for M1 decisions (incl. §6.2 without ProveBadSeal, §2.2 cohorts, §8.6 lock table); "Run a keeper" doc; playtest runbook (devnet configuration only, no devnet step) | `docs/frontier/**` except `docs/frontier/m1/{runs/**,W6-*-NOTES.md}`; `scratchpad/frontier/m1/lab/c4-v3/**`; `docs/frontier/m1/W6-E-NOTES.md` |

### Wave 7 — Exit (weeks 10.5–12, 12-09 → 12-21). Merge order: W7-B, W7-A, W7-C

| Unit | Brief | Owns |
|---|---|---|
| **W7-A exit-run** | The exit season (§13.4) with real rounds and the release `.so`, chaos on, verify, tamper, herald load, report; re-run after any W7-B fix that touches the program or the keeper | `frontier-node/crates/stack/**`; `frontier-node/configs/**`; `docs/frontier/m1/runs/**` |
| **W7-B last-fixes** | Any fix the exit run finds, anywhere except stack/configs/docs | `permutation-frontier/**`; `frontier-abi/**`; `permutation-rules/**`; `frontier-node/crates/{fclient,findex,keeper,herald,verify,agents,bots,localnet,drand-replay,itest}/**`; `frontier-node/fixtures/**`; `permutation-gateway/**` (except `screens/**` results and package files); `permutation-server/web/**` except `session.mjs`; `frontier-wasm/**`; `scripts/**` |
| **W7-C exit-report** | M1 exit report `docs/frontier/m1/M1-EXIT-NOTES.md` (EN) + JA summary line, DECISIONS final, CI job set final (green locally; **push only with a new owner approval**), the owner's playtest decision sheet | `docs/frontier/**` except `docs/frontier/m1/runs/**`; `.github/workflows/**` |

---

## 12. Wave gates (exact commands)

Common preamble (run from the integration worktree root `$R`, on `frontier/m1-integ` after merging the wave). **v1.1 (I-26): the gate checks only the ports M1 binds; it never requires the reserved ports to be idle** (the owner's own services listen on 4185, 4191 and 4194, and no M1 process ever binds a reserved port):

```sh
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
cd "$R"
RESERVED=" 4185 4190 4191 4194 18899 17799 28899 27799 26699 5185 5191 "
for p in ${M1_PORTS:-}; do          # empty for gates W1–W4: their tests bind 127.0.0.1:0
  case "$RESERVED" in *" $p "*) echo "M1 port $p is reserved: fix the config"; exit 1;; esac
  { [ "$p" -ge 41000 ] && [ "$p" -le 41999 ]; } || { echo "M1 port $p is outside 41000-41999"; exit 1; }
  if lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; then echo "M1 port $p is busy"; exit 1; fi
done
PENDING_OWNER=""                    # items blocked by an unapproved install or download (O-M1-12); listed in the gate report
pending() { PENDING_OWNER="$PENDING_OWNER $1"; echo "PENDING-OWNER: $1"; }
```
From gate W5 on, `frontier-stack check-ports --config <cfg>` applies the same rule to every port a stack config names. A `PENDING-OWNER` item is neither a pass nor a silent skip: the gate report lists it, and the wave that first needs it (W6 for real rounds, W7 for the exit) cannot pass while it is pending.

**Gate W1** (all must exit 0):
```sh
cargo fmt --all -- --check
cargo clippy --locked -p permutation-rules -p frontier-abi --all-targets -- -D warnings
cargo test --locked --release -p permutation-rules
cargo test --locked -p frontier-abi
cargo run --locked -p frontier-abi --bin abi-vectors -- --check
cargo test --locked -p permutation-chain
(cd frontier-sim && cargo fmt -- --check && cargo clippy --locked --release --all-targets -- -D warnings && cargo test --locked --release)
(cd frontier-sim && cargo run --release -- criterion --best-response --seeds 3 --first-seed 30001 --gate)
(cd frontier-sim && cargo run --release -- doctrine-gate --controls)
(cd frontier-node && cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace)
(cd permutation-gateway && npm ci --ignore-scripts && npm test)
node --test permutation-state-prototype/civilization/*.test.mjs
git diff --quiet d95fa25 -- permutation-server/web/session.mjs permutation-chain/src
```
Overnight, if CL-09, CL-10 or I-43 changed an outcome (W1-A/W1-B notes): `(cd frontier-sim && cargo run --release -- doctrines --agents 10000 --seeds 250 --first-seed 10000 --set kernel --gate)`. **Pass conditions beyond exit codes:** Phase A equivalence 4,320/4,320 identical; `Occupancy::EMPTY` digests identical to `d95fa25`; CL-29 worst ResolveFromInputs ≤ 340,000 CU and heap ≤ 28,672 B on SBF v2 (W1-A notes); `catalog_equality` green; `localnet`'s loaded-data control and `drand-replay --test-key` tests green; every G0 task (CL-01..08, 11, 12, 16, 19..21, 23..25) mapped to a merged commit in `M0-CLOSE.md`.

**Gate W2:**
```sh
# everything in Gate W1, then:
cargo clippy --locked -p permutation-frontier --all-targets -- -D warnings
cargo test --locked -p permutation-frontier --no-default-features
scripts/build-frontier.sh --twice
(cd permutation-frontier/svm-tests && ./run.sh --release -- g01_loaded_ g01_budget_ g02_ g03_ g04_ g05_)
(cd frontier-node && cargo test --locked --release --workspace)
(cd permutation-gateway && npm test && node scripts/sync-web-sdk.mjs --check)
if rustup target list --installed --toolchain 1.95.0 | grep -q wasm32-unknown-unknown; then scripts/build-wasm.sh --check; else pending "build-wasm (wasm32 target, O-M1-12)"; fi
```
Pass: `keeper::one_day_beacons` (one game day of genesis, rings, anchors, fallbacks, caches over `localnet` with the test key) green (v1.3: against the program the ring part is exercised from Gate W3 on, when OpenRing exists — W3-A; in Gate W2 it runs against the keeper's native model); `g01_budget_*` (v1.3: W2-A's instructions against §5.5) green; payer χ² test green; `g01_loaded_limit_*` green against the release `.so` with the harness's enforcement control; the validator drill result recorded in W2-B notes; `frontier.wasm` ≤ 400 KB raw once unblocked.

**Gate W3:**
```sh
# everything in Gate W2, then:
(cd permutation-frontier/svm-tests && ./run.sh --release -- g02_ g03_ g06_ map_ citizen_ holding_ host_ reveal_)
(cd permutation-frontier/svm-tests && PSF_TRACE=1 ./run.sh --release -- g01_reveal_worst g01_open_province g01_join g01_file_ticket g01_settle_ticket --nocapture)
(cd frontier-node && cargo test --locked --release --workspace)
```
Pass: Reveal worst ≤ 26,000 CU, tx ≤ 1,100 B, `L(reveal)` measured (numbers in W3-B notes, fed to the C4 model); cohort tests green (displacement without a deadline, Province held through `final_ts`, expiry); herald fold determinism green; keeper land tests over `localnet` green.

**Gate W4:**
```sh
# everything in Gate W3, then:
(cd permutation-frontier/svm-tests && ./run.sh --release)
(cd frontier-node && cargo test --locked --release --workspace -- --include-ignored inproc_ lag_gate crash_injection)
(cd frontier-node && cargo test --locked --release -p verify -- --include-ignored tamper_)
```
Pass: `itest::inproc_day` (100 bots, one game day, in process, test key) ends with **zero stuck province-bells**, every transit settled or routed by rule, every bad seal destroyed at settlement with the code the stock `tlock` opener gives; the in-process lag gate byte-identical; crash injection outcome digests identical with ≤ 1 duplicate version per in-flight write; RFI ≤ 340k CU and heap ≤ 28 KiB over all 1,240 fills with the full write-back (W4-A notes); verifier V1–V13 PASS on the recorded fixtures and T1–T22 FAIL on them.

**Gate W5:**
```sh
# everything in Gate W4, then:
(cd permutation-frontier/svm-tests && RELEASE_CHECK=1 ./run.sh --release)
(cd frontier-node && cargo build --locked --release --workspace && crates/verify/mutate.sh)
S=frontier-node/target/release/frontier-stack
$S check-ports --config frontier-node/configs/w5-smoke.toml
$S up --mode accel --beacon test-key --scale 100 --days 1 --bots 100 --run-id w5-smoke --base-port 41000
$S verify --run-id w5-smoke && $S tamper --run-id w5-smoke
$S load --run-id w5-smoke --viewers 5000 --game-hours 1
$S down --run-id w5-smoke
(cd frontier-node && cargo test --locked --release -p itest -- --include-ignored g14_)
if [ -d "$HOME/Library/Caches/ms-playwright" ]; then (cd permutation-gateway/screens && npm ci && node --test *.screen.mjs); else pending "screens (Playwright Chromium, O-M1-12)"; fi
```
Pass: G1–G14 green with no PENDING test; verify PASS; all 22 tamper classes FAIL with their codes; every `mutate-<check>` build lets its tamper PASS; herald p99 ≤ 250 ms for files and ≤ 2 s ingest-to-WS, error rate < 0.1%; screenshot matrix green once unblocked; Phase B decision recorded (amendment).

**Gate W6:**
```sh
scripts/m1-nightly.sh               # test key; three consecutive nights green
$S up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id w6-latency --base-port 41000 --chaos
$S report --run-id w6-latency && $S down --run-id w6-latency
$S up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 --run-id w6-s7 --base-port 41000 --chaos --viewers 5000   # archive needs O-M1-12; else pending "real rounds" and a --beacon test-key rehearsal
$S verify --run-id w6-s7 && $S tamper --run-id w6-s7 && $S report --run-id w6-s7
```
Pass: the run completes (fixes allowed, one re-run from snapshot allowed); verify PASS; tamper all FAIL; the latency run meets criterion 3's game-second targets; the c4 v3 report cites the measured Reveal CU distribution and `L(reveal)`; scripted onboarding run green in JA and EN. The gate cannot pass with "real rounds" pending.

**Gate W7 = the M1 exit**: §13 in full on the final commit (release `.so`, real rounds), then the exit report.

---

## 13. M1 exit test plan

Everything runs on the **release SBPF v2 `.so`** built by `scripts/build-frontier.sh` (hash recorded), except CU tracing builds, which only add `trace` markers (the gate compares plain-build CU; trace builds report heap).

### 13.1 E1 — SBF budgets with adversarial fill (G1)

For each instruction: its ceiling from §5.5, heap ≤ 28 KiB, tx ≤ its byte ceiling and ≤ 1,232 B, locks ≤ 64, **at the named worst case**:

| Instruction(s) | Adversarial fill |
|---|---|
| ResolveFromInputs | SP-V2's 40 hand-built fills + **all 1,200** fills of the native screen (v1.1; not the top 24), with the full M1 write-back, on the full-gather path; 48 residents, 24 arrivals, all hexes contested, asymmetric doctrines; **plus the storage fill (I-43): 40 residents, 8 pending musters of one faction, 1–8 departed entries awaiting SettleDeparture, 24 arrivals that would all stay** — entries after the resolve ≤ 56 and ≤ 8 per faction after the musters join |
| GatherClash | 24 positions with 10 Holdings (≤ 3 parts), mixed present/absent/pre-funded |
| Reveal | 32 steps over 4 provinces, displacement of the smallest of 4 full slots, first reveal of the province-bell (ArrivalDay init on a pre-funded address), anchor present and BeaconLog read |
| SkipQuiet | 24 bells, 48 residents with pending ops at every bell (quiet test recomputed every bell), 2 ArrivalDays, 24 anchor keys: the CU-aware stop commits a prefix within the limit; budget `60k + 30k × recomputed bells` (I-50) |
| SettleTransit | every outcome branch with the seal opener; the worst is a bad seal (code 1, FO failure after the pairing) on a Stays host settled after archive (archive `sig` path), and a bounce-by-rank against a full final set with a drained recipient (divert to `pool_owed`) |
| SettleTransit seal codes | each code 0, 1, 2, 4, 5 (was ProveBadSeal, I-44); code 3 is reserved and never emitted (v1.3: a wrong-round seal is code 1) |
| FileTicket, SettleTicket | 3 sites in 3 provinces with full cohort tables; SettleTicket displacement with the displaced rent payer, Citizen and JoinShard and 3 ticket provinces |
| every instruction | `g01_loaded_limit_<kind>`: its worst account set with `L(kind)` against the release `.so` passes, one page less fails (I-45) |
| PostAnchor, PostAnchorMulti, PostSeed, PostBeacon, ConsumeGenesisSeed, ConsumeRingSeed | 32 real quicknet beacons (worst hint path) |
| OpenProvince | the worst of ring 2..10 × 6 wedges (most sites, 2 camps) |
| SettleTicket (lottery) | see the FileTicket row |
| Join | 1,000 seeded wallets (CL-28) + an adversarial wallet with a long hash path |
| Depart, Muster, Build, Train, Explore, SettleExplore, SettleDeparture | full queue, 48-entry Province, max resources |
| ArchiveAnchors | 8 bells with 8 anchor closes and a fresh archive |
| ClaimDefence | 6 slots, caps binding |
| all others | their maximal account lists |

### 13.2 E2/E3 — Pre-funding and forgery (G2, G3)

- **Pre-funding, per creation path** (pre-fund the canonical address with rent and 10× rent, then create successfully; the payer pays only the shortfall; a stand-alone probe shows `CreateAccount` failing on the same address): Season (AnnounceSeason), Frontier, ProvinceFund ×6, DefencePool (CreateSeason), BeaconLog (InitBeaconLogs), JoinShard (InitShards), RingSeed (OpenRing, both paths), Province (OpenProvince, funded path), Citizen (Join), Holding (SettleTicket fresh), ArrivalSlot (Reveal), ArrivalDay (Reveal), ClashInputs (GatherClash), BellAnchor (PostAnchor and PostAnchorMulti), SeedCache (PostSeed), AnchorArchive (ArchiveAnchors), DefenceClaim (ClaimDefence) — **17 account kinds over 19 creation paths** (RingSeed and BellAnchor each have two; v1.1 removed SealVerdict). Plus: a pre-funded never-created ArrivalSlot and ArrivalDay count as absent at gather, skip and settle.
- **Re-creation, per closable kind (I-46):** after its close, the creating instruction is refused for ClashInputs (GatherClash `LatchClosed`, also for a skipped bell), ArrivalSlot and ArrivalDay (latch), BellAnchor and SeedCache (tombstone), Province, RingSeed, Holding and Citizen (season status or ticket rules), AnchorArchive (season status).
- **Forgery, per keyed account** (each refused with the pinned code): wrong address (`BadAddress`), wrong owner, wrong magic, wrong season, wrong key fields (`BadAccount`), absence claimed at a non-canonical address (`BadAddress`), for every account kind any instruction reads or writes; a forged anchor/cache/archive/inputs/ArrivalDay in every instruction that reads one.

### 13.3 E4 — Property tests (program level, LiteSVM)

| Gate | Property |
|---|---|
| G4 | No Reveal lands at `now ≥ A + W`, once the BeaconLog holds a round ≥ S, after the first gather (latch), or for a tombstoned bell — over random A, Clock, window schedules and supplied/omitted/forged anchors and archives |
| G5 | One anchor per (bell, region); every cache nonce gives the same seed; no second anchor after archive; ResolveFromInputs refuses a cache whose A differs from THE anchor's |
| G6 | A gather that omits a present slot cannot complete; a clear ArrivalDay bit gathers in one step |
| G7 | **Lag invariance:** hold the origin Province, the origin region's anchor and SettleDeparture past the destination's close → the destination's result is byte-identical to the unheld run |
| G8 | Gathers in random order and with repeats + ResolveFromInputs == ResolveClash (oracle build) == native kernel digest; **the I-43 storage fill never exceeds 56 entries or 8 per faction after musters join** |
| G9 | Quota fairness: 8 reveal orders × random arrivals → the same final slots = `quota_set`; SettleTransit outcomes (admitted/bounced/routed) identical across orders |
| G10 | SettleTransit's seal code = stock `tlock` decryption + commitment + `Plain::validate` for valid, garbage, tampered U/V/W, wrong round, invalid plaintext, logged-vs-committed mismatch, forged commitment — **before and after ArchiveAnchors** (I-44) |
| G11 | SkipQuiet over a run == gathering and resolving each bell (byte-identical Province), for random rosters with and without pending ops, **including rosters changed at every bell (the CU-aware stop commits prefixes whose sum equals the resolves)** |
| G12 | Every SettleTransit branch incl. **settle races proof** (a garbage seal revealed in-bell, settled at the earliest instant: destroyed), **a Stays host that tries to re-depart, dissolve or explore before its settlement** (`HostInTransit`), bad seal after archive, closed or never-resolved inputs, stranded hosts, drained recipients (`pool_owed`), **claim after settle** (slot kept for the grace) and **claim after displacement** (reset); a ticket cohort's Province held through `round_time(S) + 600` (the higher score still wins; finality waits) |
| G13 | One test per (instruction, error code); `RELEASE_CHECK=1` passes (no PENDING, no `NotImplemented`) |
| G14 | 100 bots × 2 game days in LiteSVM through the program with the native kernel re-run every bell (test key); verifier PASS; one tampered log → FAIL (owner: W5-C) |

### 13.4 E5 — The 7-day accelerated local season (Mode A)

```sh
frontier-node/target/release/frontier-stack up --mode accel --beacon archive --scale 20 --days 7 --bots 1000 \
  --run-id m1-exit --base-port 41000 --chaos --viewers 5000 --viewer-window-hours 24
frontier-node/target/release/frontier-stack verify --run-id m1-exit
frontier-node/target/release/frontier-stack tamper --run-id m1-exit
frontier-node/target/release/frontier-stack report --run-id m1-exit
frontier-node/target/release/frontier-stack down --run-id m1-exit
```
Environment: `frontier-localnet` with the pinned release `.so` (deployed with its `--max-len`), `drand-replay` with real historical quicknet rounds (**≈ 250k contiguous rounds**, I-53), relay, keeper A (operator roles) and keeper B (public profile) each with ≥ 150 reveal payers, ≥ 32 delay payers and ≥ 4 funders, herald, 1,000 bots with the thirteen personas, adversary schedule (hold one province's slots through a close below and above the keeper cap; hold one region's anchor; hold 20 known keeper payers; hold the origin Province and origin anchor past a destination's close; **hold a ticket cohort's Province through `final_ts`; hold Frontier and one ProvinceFund during a ring opening; hold the DefencePool through part of a claim grace**), chaos (`kill -9` a random component every 2–6 game hours, restart after 0–60 game seconds). All ports in 41000–41999. Slots are 400 ms real, 8 game seconds each (I-54). Wall time ≈ 8.4 h + ≈ 1 min of pre-season at scale 2,000. **Pass criteria:**
1. The season reaches `end_bell`, EndSeason lands, and within 2 game hours after the end: zero province-bells stuck (every opened province resolved or skipped through `end_bell − 1`), zero transits in state 1–2 older than `close + 2 bells`, zero transits unsettled, every ClashInputs closable after grace.
2. Max CU per instruction kind observed in play ≤ its §5.5 budget; the full **Reveal CU distribution** (p50/p99/max) reported.
3. Keeper latencies at 20×, **in slots** (I-54): round → anchor p99 ≤ 2 slots; S → first cache p99 ≤ 2 slots; T(b) anchor → last valid reveal p99 ≤ 4 slots; close → resolve p99 ≤ 8 slots for active provinces; catch-up ≤ 6 SkipQuiet txs per idle province-day whose roster did not change (churned provinces reported). The game-second targets (round → anchor ≤ 5 s, S → cache ≤ 5 s, anchor → last reveal ≤ 30 s, close → resolve ≤ 60 s, all p99) are checked in W6's scale-2 latency run.
4. Liveness: `ValidSealUnrevealed` = 0 outside the adversary-held windows where the hold was above the keeper cap (those are listed and expected); `min_tip` persona fully revealed by keepers; **the reveal pool's effective N ≥ 150 in every bell** (I-49).
5. Every persona's expected outcome observed (§8.6).
6. Herald: p99 file latency ≤ 250 ms, ingest → WS p99 ≤ 2 s, error rate < 0.1% at 5,000 viewers (4,000 polling, 1,000 WS) for 24 game hours.
7. Bots' outcome statistics within an order of magnitude of `frontier-sim` for the same agents (clashes per bell, returns by archetype) — reported, not gating.
8. **Bad seals (I-44):** verifier `BadSealSurvived` = 0; every `garbage_seal`, `bad_plaintext` and `settle_racer` transit settled as bad-seal with the stock opener's code.
9. **Tickets (I-47):** every cohort closed within 24 bells; the `ticket_holder` persona never keeps a site it did not win.

### 13.5 E6 — Verifier

`frontier-verify` over the exit season's RPC and archive: **PASS** (exit 0) in < 10 min on 8 threads; the tamper suite on a copy of the run: **all 22 classes FAIL** with their named codes; each `mutate-<check>` build lets its own tamper PASS (checks of the checks); honest-but-adverse fixtures PASS.

### 13.6 E7 — Web smoke

- `(cd permutation-gateway && npm test)`: every `web-frontier-*` test, the existing v9 web tests unchanged, `web-lang` completeness over `web/frontier/**`, `web-sdk` freshness incl. `sdk/frontier` and the noble manifest, `web-frontier-wasm` hash.
- Screenshot suite: 10 screens (join/faction, site picker, map world LOD, map tile LOD with fog, holding, march composer, march tracker, bell sheet, clash report, practice result) + onboarding card + spectator × JA/EN × 360×740, 390×844, 1440×900: no console errors or failed requests, no horizontal overflow, landmarks and bell chip visible, interactive elements ≥ 44 × 44 px at phone widths, axe no serious/critical, no Japanese glyphs in EN except names; language toggle re-renders without reload. Pixel diffs advisory.
- Scripted onboarding on the exit stack (Playwright + dev wallet, 390 px, JA and EN): join → site → build → scout/explore → sealed march on a camp → report → verify-in-browser green.
- Spectator open 24 game hours: memory growth ≤ 200 MB.

### 13.7 E8 — Measurements owed to M0

The full Reveal CU (worst and in-play distribution) and `L(reveal)` from the release `.so` (I-45) → c4 model v3 (CL-26/30) → the default tip priority and `tip_min` final; the quiet-bell proof's cost per idle province-day (and per churned province-day, I-50); the per-bell reveal count R99 and the payer pools' effective N (I-49); ResolveFromInputs worst CU and heap after Phase A (and B if adopted); the arrival-slot tie-break at Reveal time (kernel `slot_key`).

### 13.8 After the exit

Only with the owner's explicit approval: a private devnet playtest, 50–200 people, no money (config and runbook from W5-E; invites; relay 40/20 quotas; herald and relay hosting decided then). Every devnet step and every push needs its own approval.

---

## 14. Estimate (honest, v1.1)

| Wave | Calendar | Units | Unit engineer-weeks [estimate] | Main uncertainty |
|---|---|---|---|---|
| W1 foundations + closeout | 1.5 wk | 6 | 9.0 | CL-09/CL-10/I-43 outcome changes → doctrine re-tune (+1–5 days); root-lock resolution for arkworks |
| W2 program core, harness, localnet, relay, web foundation, keeper beacons | 1.5 wk | 6 | 9.5 | the validator drill (loaded-data accounting); test-key tlock interop |
| W3 land, holdings, Reveal, keeper land, herald, bots, web play | 2 wk | 6 | 11.5 | Reveal > 26k (the two cuts, +2–3 days); cohort edge cases |
| W4 clash, transit, keeper play, verifier, web report, integration | 2 wk | 6 | 11.5 | SettleTransit with the proof inside; the first system run |
| W5 hardening | 1.5 wk | 5 | 7.0 | coverage table (49 instructions × their codes); herald load numbers |
| W6 runs and fixes | 2 wk | 5 | 7.0 | each failed 8.4-h run ≈ 1 day; archive download timing |
| W7 exit | 1.5 wk | 3 | 3.0 | re-runs after late fixes |
| Integrator (merges, manifests and locks, gates, 7 integration windows of ≤ 2 days) | 12 wk | 1 | 4.0 | gate wall time (below) |
| **Total** | **12 weeks nominal** | — | **≈ 62.5 (58.5 units + 4 integrator)** | — |

- **Range:** ≈ 50% by 12.5 weeks (≈ 2026-12-24) and ≈ 80% by 14.5 weeks (≈ 2027-01-07). The calendar crosses the year end; count real availability. v1.0's "10 weeks, ≈ 45 (§0) / ≈ 47 (§14) engineer-weeks" is withdrawn: the two figures disagreed (the §14 rows sum to 47), and neither counted integration, gate wall time or owner-approval latency. Capacity check: six implementers × 12 weeks = 72 ew, so 58.5 unit ew is ≈ 81% utilisation.
- **Gate wall time** (inside the integration windows): W1 ≈ 3 h + the overnight doctrine run; W2 ≈ 3.5 h; W3 ≈ 4 h; W4 ≈ 5 h (full svm-tests, in-process day, tamper fixtures); W5 ≈ 8 h (22 mutate builds ≈ 2 h, the 14.4-min smoke, the 5,000-viewer load); W6 three nights + the 3-h latency run + ≤ 3 × 8.4-h runs; W7 ≈ 9 h per exit attempt (8.4 h + verification), planned twice.
- **Owner-approval latency:** O-M1-12 is asked on day 1 (2026-09-28). The wasm32 target is needed by 2026-10-07, Playwright Chromium by 2026-11-16, the round archive by 2026-11-25 (≈ 1.2 h to fetch). Until then the items are `PENDING-OWNER` and nothing else waits on them (the test key covers every in-process gate). If the archive is approved after 2026-12-01, the exit slips day for day. The 50% figure carries 0.5 week of slack for this.
- **N5:** the M0 closeout lands with wave 1 at week 1.5 (2026-10-07), not in week 1; the overnight doctrine re-runs may finish in week 2.
- **DESIGN schedule:** DESIGN planned M1 in design weeks 7–15. At 12 weeks M1 ends around design week 19, so M2 (weeks 13–19) must overlap or later milestones slip ≈ 4 weeks.
- **What would cut time (in this order):** ClaimDefence to M2 (−1.0 ew, removes the claim grace and DefenceClaim; I-22's first cut); herald WS (polling only, −1 ew); the screenshot matrix to phone + desktop (−0.5 ew); the practice what-if (−0.5 ew); Mode R is already optional. With all four cuts: ≈ 11 weeks nominal. **What must not be cut:** the gates of §13.1–§13.5, the verifier's tamper suite, the lag gate, the pre-funding, forgery and re-creation tests, the seal proof in settlement (I-44) and the storage room (I-43).
- **Tools cost:** each 7-day run is ≈ 8.4 h wall time plus ≈ 10 min verification; plan for 3 attempts in W6 and 2 in W7.

---

## 15. Owner decisions needed (working defaults in force until decided)

| # | Question | Working default (implementers build to it) | Blocks |
|---|---|---|---|
| O-M1-01 | Accept with-seed addresses for player accounts (I-01, program D2) | yes | W1-E layout freeze |
| O-M1-02 | Accept ArrivalDay + SkipQuiet as the quiet-bell proof (I-10, D3) | yes | W1-E, W3-B, W4-A |
| O-M1-03 | Accept SettleDeparture + Holding-based gathers (I-11, D4), rank-based transit outcomes (I-12, D5), ResolveFromInputs writing ClashInputs (I-13, D6) | yes | W3-B, W4-A, W4-B |
| O-M1-04 | Clash Phase B (a rules change: one hash per engagement) before the exit if the gates re-pass (I-14) | decide at the W5 gate on the measured gates | W5-A |
| O-M1-05 | No postures in M1 (I-16; DESIGN §12 lists them) | no postures | W1-E |
| O-M1-06 | Minimum tip in priority terms, zero tip removed (I-08) | yes, p_min 0.433 | W3-B, W3-F |
| O-M1-07 | AnnounceSeason ≥ 24 h ahead, single-use ids, 1 SOL creation bond burned on a post-round pre-join abort (I-09, CL-24); v1.1: only the program's upgrade authority may announce (I-51) | in M1 | W1-E, W2-A |
| O-M1-08 | D18 re-size: W/D class split, pool pays Reveal only, pool 20 SOL if it covers ≥ 100 p99 bells (I-21, CL-30/31a); ClaimDefence in M1 as class D with a 6-bell claim grace (I-22, I-52) | yes / yes | W4-B, W4-C |
| O-M1-09 | Keeper start bid at the tip level (I-23) | p_start = p_tip, `--peace-start` off | W2-F |
| O-M1-10 | Mode A accelerated LiteSVM-backed run as the "7-day local season" exit (I-25), with real 400-ms slots and 8 game seconds per slot at 20× (I-54); Mode R 24-h soak as a complement | Mode A; Mode R only if O-M1-12 approves the install | W5-B, W6-A, W7-A |
| O-M1-11 | Depart charges the maximum march stamina (I-32) | yes | W3-B |
| O-M1-12 | Approve downloads and installs — **asked on 2026-09-28 (wave 1, day 1)** (I-53): (1) `rustup target add wasm32-unknown-unknown --toolchain 1.95.0`, needed by **2026-10-07** (W2-E size gate); (2) Playwright Chromium if the cache is absent, by **2026-11-16** (W5 screens); (3) a **contiguous archive of ≈ 250,000 historical quicknet rounds** (read-only public data, ≈ 40 MB, ≈ 1.2 h at ≤ 20 requests/s per endpoint over 3 public endpoints, verified as fetched), by **2026-11-25** (W6 real-round runs; the exit cannot run without it); (4) Agave ≥ 4.0 for Mode R, optional. The installed `solana-test-validator 3.1.9` covers the I-45 drill without an install | none approved yet; until then the items are `PENDING-OWNER` and every in-process gate, nightly and rehearsal runs on the `test-beacon` build | W2-E, W5, W6, W7 |
| O-M1-13 | Ticket finality by cohort (I-47, replacing I-19's `round_time(S) + 600` alone) and the restated onboarding times (first holding 11–21 min, first clash report 31–41 min) in DESIGN §2.2/§2.3 (I-33) | yes | W1-D text, W3-F copy |
| O-M1-14 | Doctrine C display name "Flame" (I-34); fog with a "show everything" switch (I-35) | yes / yes | W3-F |
| O-M1-15 | Holding 1,280 B (I-31); v1.1 Citizen 384 B for the ticket escrow (per-player rent 9,753,600 lamports) | yes | W1-E |
| O-M1-16 | Carried from the closeout: D22 stake ramp (decide on the D23-on re-run), D24 Relic Sites, the Season-1 sweep target (CL-17), D23 details (caretaker term not counted; vacant seats allowed) | as closeout §7 | M2/M3 (not M1) |
| O-M1-17 | CL-35: fix the two legacy `civilization` tests (option a) — and, separately, a push approval when the owner wants GitHub CI to run | fix locally; no push | W1-D |
| O-M1-18 | After the exit: the private devnet playtest (50–200 people, no money), sponsorship values (40/20), invites, and hosting for the herald and relay | not approved | post-M1 |
| O-M1-19 | Settlement is the seal proof: SettleTransit opens and judges every seal (also after archive); ProveBadSeal and SealVerdict are removed; a host in an unsettled transit cannot Depart, Dissolve or Explore (I-44; DESIGN §6.2 names ProveBadSeal) | yes | W2-A, W3-B, W4-B |
| O-M1-20 | Ticket cohorts: finality waits for the cohort (≤ 24 bells), displacement has no time condition inside it, SettleTicket is class D, a displaced ticket ends and the player refiles (I-47) | yes | W3-A, W3-C, W3-F |
| O-M1-21 | Storage-aware clash room: arrivals beyond the room left by pending musters and departed entries bounce without loss (an outcome change only when such entries exist; the doctrine proxy gate is re-run in wave 1) (I-43) | yes | W1-A |
| O-M1-22 | Camps, exploration and training as the simulator models them: one camp per province on a non-site tile with daily respawn, plus an initial camp for onboarding; Train immediate; the explore floor gives Works only (DESIGN §2.3's goods floor waits) (I-56) | yes | W1-C, W1-D, W3-F |
| O-M1-23 | Playtest policy: invites enforced on chain by `Season.join_gate`; AnnounceSeason only by the program's upgrade authority; sponsored tips limited to three presets; settles charged to the requester (I-51) | yes | W2-A, W2-D, W3-A |
| O-M1-24 | Schedule: 7 waves, 12 weeks nominal (50% 12.5, 80% 14.5), ≈ 62.5 engineer-weeks; cut order ClaimDefence → herald WS → screenshot matrix → practice what-if (I-57) | accept; cut ClaimDefence first if wave 4 slips | all |

---

## 16. Amendments, sources and links

- **Amendments:** only the integrator changes this file. Each change bumps a version line at the top (`v1.0` = the first issue, kept as `lab/contract-rev/M1-CONTRACT.v1.0.md`; `v1.1` = this review revision, §17), lists the sections changed, and is copied into `DECISIONS.md` if it resolves a conflict. Layout, tag, error-code and log-kind changes after wave 1 require an amendment and regenerated vectors in the same merge.
- **This contract:** `scratchpad/frontier/m1/M1-CONTRACT.md`; **decisions log:** `scratchpad/frontier/m1/DECISIONS.md` (to be committed as `docs/frontier/DECISIONS.md` by W1-D).
- **Area designs (informative):** `scratchpad/frontier/m1/design/{program,offchain,web,closeout}.md`.
- **Labs:** `scratchpad/frontier/m1/lab/contract-rev/` (v1.0 copies and the v1.1 patch scripts), `scratchpad/frontier/m1/lab/clash-opt/` (Phase A/B patches, equivalence test, results), `m1/lab/offchain-svmspeed/`, `m1/lab/rev31-m1/m1-results.txt`, `m1/lab/closeout-civ/`; SP-V2 `m0b/spikes/SP-V2/` (program, host, beacons, vectors); `m0/spikes/{S-TLOCK,S-SIZE-JOIN,S-FEE,S-BEACON}/`.
- **Normative repo docs:** `.claude/worktrees/frontier-integ/docs/frontier/{DESIGN.md,SUMMARY.ja.md}`, `docs/frontier/m0/{M0-FINAL,SPIKE-SP-V2,SPIKE-SP-FEE,SIM-*}.md`.

---

## 17. Revision notes (v1.1, 2026-09-27)

Each review issue was checked against DESIGN rev 3.1, the v1.0 text, the code at `d95fa25` and the M0/M1 labs (read-only; no repo change, no server, no chain transaction). Numbering follows the review's order (R1–R21). "Accepted" means the finding was confirmed; "merged" means it duplicates another; parts rebutted are named with their evidence.

| # | Severity | Finding (short) | Verdict and evidence | What changed (sections) |
|---|---|---|---|---|
| R1 | blocker | Province entries (56) can overflow at ResolveFromInputs; the province freezes; faction cap leak | **Accepted.** `clash.rs` l. 652–671 seeds `per_faction`/`total` only from `!u.arrival` units, i.e. the state-1 residents in the input; Muster caps states 1–2 at 48 (v1.0 §5.10); state-3 entries wait for SettleDeparture, a class-D write (v1.0 §5.11). The reviewer's 40 + 8 + 8 + 1 = 57 case is reachable, and RFI had no refusal or bounce rule | I-43; §5.3 entry, §5.11 RFI, §7 `clash::Occupancy`, §13.1 storage fill, G8, W1-A, O-M1-21. The room is `min(48 − R − M, 56 − R − M − D)` in effect (v1.1 also counts pending musters against the 48, which the suggested `48 − R` did not); bit-identical when M = D = 0 |
| R2 | blocker | A bad seal is not reliably punished; a re-departed Stays host freezes another province | **Accepted, both parts.** v1.0 §5.5 classed ProveBadSeal N (tip-capped) and §5.11 let SettleTransit run at close + 600 without a verdict; ProveBadSeal refused archived bells; nothing blocked Depart/Dissolve/Explore of a host whose transit was unsettled. DESIGN §6.2's protection is a timing argument ("at least 20 minutes"), which a held account defeats | I-44: SettleTransit takes the logged `commit` + `seal`, opens and judges them (≤ 48.0k CU measured for the opener, budget 85k, tx ≈ 990 B); the archive stores each bell's signature so a late settlement still judges; ProveBadSeal and SealVerdict removed (suggestion 1 taken in full, SealVerdict not kept as an optimisation: one fewer kind, no race); `HostInTransit`; `Forfeit` pending op. §4.1, §4.3, §5.1–§5.6, §5.8, §5.11, §6, §8.2, §8.5, §8.6, §13.1–§13.4, O-M1-19 |
| R3 | blocker | `SetLoadedAccountsDataSizeLimit(65,536)` cannot load a ≈ 0.5 MB program | **Accepted.** SP-V2 `spv2.so` 480,512 B (plain v2) and 540,608 B (kprobe v2) vs SP-FEE's 13,600-B probe [measured]; SP-FEE RESULTS l. 76 and 213 pin 64 KiB. The conclusion holds whether or not SIMD-0186's exact accounting is active, because an invoked program's data was always loaded; the validator drill confirms the exact rule | I-45; §5.3 Season `reveal_loaded_limit`, §5.5, §7 `fees`, §8.1, §8.2, §8.7, §10.1 (tip_min 14,441 at the 1-MiB default), §10.2, §13.1 row, W1-F, W2-B drill |
| R4 | major | A closed ClashInputs can be created again; lifecycle status gaps | **Accepted.** v1.0 GatherClash checked window and absence only; OpenRing used `status ≥ Seeded` (includes Ended 4, Closed 5, Aborted 6); OpenProvince had no status check, and RingSeeds close only at CloseSeason, so CloseProvince → OpenProvince was possible before the final part | I-46; §4.2, §5.9, §5.11 (GatherClash `bell ≥ resolved_next`; SettleTransit needs flag 2), §13.2 re-creation tests |
| R5 | major | SettleTicket and ProveBadSeal are class N but close a window | **Accepted.** SettleTicket: the lottery depended on landing before `final_ts` against a fixed low bid. **Partly different fix:** the deadline is removed by cohorts (the reviewer's second option) and SettleTicket becomes class D; it is not made pool-eligible, which would re-open the D18 sizing (O-M1-08) for land. ProveBadSeal: solved by R2 | I-47 (supersedes I-19); §5.1, §5.3 Province cohorts and Citizen escrow, §5.5, §5.9, §8.2, §8.8, G12, E5 criterion 9, O-M1-20 |
| R6 | major | Global singletons written where the keeper does not defend | **Accepted.** v1.0 SettleTransit listed `[dpool w]` on every call, against DESIGN §8.6 rule 2 ("no shared pool account is ever on the critical path"); OpenRing/OpenProvince/CloseProvince/Fold wrote Frontier and one ProvinceFund at class N; FileTicket accepted a ring whose provinces did not exist yet (only `ring < rings_opened`) | I-48: `Holding.pool_owed` + SweepPoolOwed; ProvinceFund per wedge; openings and folds class D; FileTicket requires the Province; new lock table §8.8. §4.2, §5.2, §5.3, §5.5, §5.7, §5.9, §5.11, §5.12, §6 |
| R7 | major | The keeper payer pool drains one way; C4's ≥ 150 payers erode | **Accepted except one suggestion.** v1.0 §8.2: "the payer is also the rent payer; refunds go to the configured beneficiary"; SettleTicket (not a relay shape) made keeper payers the Holding's `rent_payer`. **Rebutted:** "refuse to run W writes while effective N < 150" — refusing would itself be the exclusion C4 guards against; v1.1 alerts, tops up from ≥ 4 funders and gates per-bell effective N in E5 instead | I-49 (rent back to the payer; two pools; band from R99), I-05/I-47 (the Holding is funded from the ticket funder's escrow, which gives the relay the refunds without a second signer); §8.2, §13.4 |
| R8 | major | No CU or heap escape hatch; SkipQuiet and RFI budgets inconsistent with their fills | **Accepted.** v1.0 §3.3 forbade heap frames and §10.2 capped the CU limit at the budget; SP-V2 PostAnchor 331,810–335,542 CU against a 345k budget before M1 additions; `program.md` l. 632–633 costs `is_quiet` at 10–30k per run and relies on a cache that the §13.1 fill (pending ops every bell) invalidates every bell | I-50; §3.3, §5.5, §5.11 SkipQuiet, §8.2 retry ladder, §10.2, §13.1, W4-A (all 1,240 fills with the full write-back) |
| R9 | major | Abuse paths of a free M1 | **Accepted, all four.** Invites existed only at the relay; settle shapes were "charged to the named holding's citizen"; the drain guard had no tip or CU-price ceiling; `/gw/*` adds `X-Forwarded-For` (hosting is post-M1, but the rule is cheap) | I-51; §5.3 Season `join_gate`, §5.4 codes 59/61, §5.7 AnnounceSeason, §5.9 Join, §8.3, §8.4, §8.6 personas, O-M1-23 |
| R10 | major | The defence refund can be pre-empted or erased | **Accepted** (merged with R19). **Chosen fix:** the claim grace in SettleTransit (the reviewer's first option), not copying evidence into ClashInputs (+1,344 B per province-bell for a feature that is first on the cut list) | I-52; §5.3 slot flags, §5.11, §5.12, §8.2, G12 |
| R11 | major | The verifier leaves rules unchecked | **Accepted, one claim rebutted.** "Tickets and explores have no digest in a chained record": SETTLE is chained on Citizen, Holding, Province and JoinShard and carries the score; EXPLORE_RESULT is chained on Holding and Citizen and carries the finds (v1.0 §6). The data was there; the checks were missing | I-58; §8.5 V5, V11–V13, T7, T17–T22; §13.4 criteria 8–9 |
| R12 | blocker | Gates W2–W4 need approvals nobody gave | **Accepted.** 32 rounds in `SP-V2/beacons/quicknet`; `rustup` lists no wasm32 target; §8.7 ruled out a test key; the archive must be contiguous (≈ 201,600 rounds for 7 days; ≈ 250k with the pre-season and margin) | I-53 (`test-beacon`, `--test-key`, `PENDING-OWNER`); §8.7, §12, §14, O-M1-12 dated and sized |
| R13 | blocker | The gate preamble aborts on this machine | **Accepted.** `lsof` shows node on 127.0.0.1:4191 and 4194 and play on 4185 (the owner's services) | §12 preamble (checks only M1's ports), I-26 |
| R14 | major | Mode A at 20× makes slots 20 ms | **Accepted.** v1.0 §8.7: "block builder per game slot (400 ms / scale)"; criterion 3 in game seconds; the announce lead was not in the 8.4 h | I-54; §8.7, §13.4 criterion 3, W6 latency run, pre-season at scale 2,000 |
| R15 | major | File ownership gaps and cross-edits | **Accepted, (a)–(h).** (a)(b)(c) manifests and locks → integrator; (d) prologue and entry codec in `frontier-abi` (W1-E), svm-tests harness to W2-B with area files handed over; (e) `web-sdk.test.mjs` to W2-D, `web-lang.test.mjs` to W2-E then W5-E; (f) `app.mjs` to W4-E; (g) G14 to W5-C; (h) `RETREAT_MAX_BPS` moves into `clash.rs` | I-55; §3.1, §3.4, §7, §11 |
| R16 | major | RFI can run out of Province entries (48 musters + 24 stays) | **Merged into R1** (same mechanism; this scenario is covered by the storage fill) | as R1 |
| R17 | major | The 64-KiB loaded-data limit ignores program size | **Merged into R3.** Both sizes it and R3 quote are real builds (540,608 B kprobe; 480,512 B plain). "LiteSVM may not enforce it" → `localnet` enforces SIMD-0186 accounting itself if LiteSVM does not, proved by a control test | as R3 |
| R18 | major | camp, explore and catalog are specified only by signature; the sim models camps differently | **Accepted.** `model.rs` l. 175–252 holds BUILDINGS, `build_secs`, `TROOP_COST_PER_100`, `WALL_*`, `STARTER_KIT`, `WORKS_*`; `sim.rs` models `Prov.camp: Option<Camp>` on a non-site tile with daily respawn (l. 87–103, 1119–1147) and instant training (l. 1535–1560) | I-56 (sim normative; one camp per province on a non-site tile + an initial camp; Train immediate; floor gives Works only); §5.3, §5.9, §5.10, §7, W1-C, W1-D equality test, O-M1-22 |
| R19 | major | ClaimDefence evidence does not live long enough | **Merged into R10.** Storing the anchor slot in the archive is unnecessary: claims end 6 bells after close, long before the 48-h archive | as R10 |
| R20 | major | The W3/W4 gates are the first integration; W1-E overloaded | **Accepted.** W1-E carried the ABI, a 50-tag skeleton, the crypto port, 10 instructions, the build script, the harness and g02–g05 in 1.5 weeks | I-57: 7 waves; W1-E → W1-E abi, W2-A program-core, W2-B svm-harness; W4-F integration unit (`itest`); G14 in W5; integration windows (§3.4); gates restated (§12) |
| R21 | major | The estimate is inconsistent and leaves out serialized costs | **Accepted.** v1.0 §0 said ≈ 45 ew; its §14 rows sum to 47 | §0, §14: 12 weeks nominal, ≈ 62.5 ew, 50% by 12.5 and 80% by 14.5 weeks, gate wall time, approval dates, N5 overshoot, cut order; O-M1-24 |

**Numbers that moved in v1.1:** `tip_min` 14,337 → 14,441 lamports at 26k CU (1-MiB loaded-data default; v1.0's "10,006 at 16k" was also rounded down, integer ceil gives 10,007, now 10,111); per-player rent 9,428,480 → 9,753,600 lamports (Citizen 384 B); AnchorArchive 5,280 → 12,192 B (7-day float ≈ 3.5 → ≈ 8.0 SOL, refundable); SettleTransit budget 30k → 85k and tx 900 → 1,100 B; FileTicket 8k → 14k CU; SkipQuiet `60k + 30k per recomputed bell`; account kinds 18 → 17; creation paths 20 → 19; tamper classes 16 → 22; personas 10 → 13; waves 6 → 7; estimate 10 weeks / 45–47 ew → 12 weeks / ≈ 62.5 ew.

**Residual risks stated, not solved:** player actions at priority 0 can be held at one account's price (DESIGN §8.6 accepts "the owner is delayed"); a displaced ticket ends instead of moving to its next preference; a self-revealing player can collect its own preset tip (≤ 2 × tip_min per march); the lock prices in §8.8 are DESIGN §8.7's model, not measurements; the exact SIMD-0186 accounting is confirmed on the installed 3.1.9 validator, not on mainnet (M4).

---

## 18. Amendments v1.2 (integ-W1, 2026-09-27)

The wave-1 review (six units) was answered in the integration window (§3.4). Each row is normative from v1.2; the code landed on `frontier/m1-integ` in the integ-W1 commits; decisions are copied to `DECISIONS.md` part G; the item-by-item response is `integ-W1-NOTES.md`.

| Section | Change | Why (review item) |
|---|---|---|
| §3.2 | `RULESET_HASH` binds a version constant for every module of `permutation_rules::frontier` (23 entries), the doctrine table, the stance damage table and `KERNEL_CONSTANTS`; the program embeds `frontier_abi::presets::RULESET_HASH` (a const pinned to the kernel by a test), not a build-step output | W1-C major: the hash was blind to CL-09/CL-10 |
| §4.1 | `po` = P i32, Q i32, bell u32, pos u8 → 13 raw / 28-B seed (SP-V2) | W1-C/W1-E erratum |
| §4.2 | Whole-account refund floor = `rent(0)` = 650,240 lamports | W1-E: 890,880 was the old rate |
| §5.3 | ArrivalSlot `flags` bit 2 = created_day (Reveal sets it, a displacement clears it) | W1-C major: `defence_refund` needs it |
| §5.4, §5.10 | Build past a CL-02 cap (walls > 1,200) → `Kernel` 15 (`HoldingError::AboveCap`); UI §9.6 maps it like other kernel refusals | W1-B |
| §5.5 | `MULTI_MAX_REGIONS` = 7; FoldOccupancy ceiling 1,232; tx ceilings gate on `budgets::tx_ceiling` (the byte model), the table's "Tx B max" is informative where below it | W1-E |
| §5.6 | Step 1 checks the status before the ruleset; `Auth` = a missing signature, `BadAccount` = writability; step 3 (bucket) runs before step 4 (`PlayerStart::finish`) | W1-E major |
| §5.9 | FoldOccupancy in three parts (24 shards / 24 shards / 6 funds, `FoldStale` between parts); SettleTicket's seed account `seedcache\|archive`; ring 0 (the Concord) funded from wedge 0 | W1-E major; W1-F; W1-E finding |
| §5.12 | Refund priority fee `⌈price × limit / 10⁶⌉` (the runtime's rounding); `created_day` from the slot flags | W1-F; W1-C |
| §6 | RING_SEED chains nothing; SEASON_CREATED (138 B) is an exception to the soft 128-B ceiling | W1-E |
| §7 | `seal::validate` also refuses a direction ≥ 6 and `dest_tile ≥ 61` (I-28); `clash` keeps a private `valid_faction` pinned by a test; `host::GarrisonState` is capped at `MAX_HOST_TROOPS`; `mandate::Reserve::final_sweep(open_terms)`; `siege::Vigil` first-window rule "a day after the last old window started"; `geometry::ProvinceCoord::{index, from_index}` are total | W1-A major, W1-B major, W1-C major |
| §8.7 | drand-replay: `round_time + delay ≤ game_now`, `game_now` = the last observed Clock (no extrapolation); port checks use the §10.3 `lsof` rule | W1-F |
| §12 | Gate W1's `doctrine-gate --controls` runs the Season-1 economy (D23, caretaker term exempt) at 60 gate seeds; `catalog_equality` compiles unconditionally (no `FRONTIER_REQUIRE_CATALOG`) | W1-D majors |


---

## 19. Amendments v1.3 (integ-W2, 2026-09-28)

The wave-2 review (six units) was answered in the integration window (§3.4). Each row is normative from v1.3; the code landed on `frontier/m1-integ` in the integ-W2 review commits; decisions are copied to `docs/frontier/DECISIONS.md` part I; the item-by-item response is `integ-W2-NOTES.md` §6.

| Section | Change | Why (review item) |
|---|---|---|
| §4.1, §5.2, §5.3, §5.8, §5.11 | **AnchorArchive per region and half day**: `aa‖region,part` with `part = bell / 72`; 6,144 B (72 entries of 84 B, bitmaps of 9 B, `rent_to` at 6,112), rent 31,861,760; ArchiveAnchors data `region, part, n, bells` with every `bell / 72 == part`; every reader addresses the archive of `part(bell)`. Float of a 7-day season ≈ 8.2 SOL | W2-A F2 (confirmed): 12,192 B exceeds the 10,240 B a program can allocate by CPI in one instruction, so ArchiveAnchors could not create v1.1's day archive |
| §4.2 | `close_to` does not log; every closing instruction emits `CLOSE` itself before calling it (hand-over rule in `proc/mod.rs`) | W2-A minor: the unlisted departure from §4.2 |
| §5.3, §13.1, G10 | Seal code 3 reserved, never emitted by the M1 opener: a seal to another round is code 1 (FO failure); §13.1's row lists codes 0, 1, 2, 4, 5 | W2-A minor, W2-B major: code 3 is not distinguishable from 1 |
| §5.5 | Budgets AnnounceSeason 18k → **25k**, InitBeaconLogs 60k → **80k**, InitShards 40k → **45k** (`frontier-abi::budgets`, vectors, fclient mirror) | W2-A F3 / major: 18,570, 74,688 and 39,313 CU [measured, `g01_budget_w2a`] |
| §5.7 | SetWindowSchedule: **one change per season** (a second call is `AlreadyDone`; `reveal_window` is never rewritten); `from_bell < end_bell` else `BadData` (refuses the `u32::MAX` sentinel) | W2-A major: the fold changed `W(b)`, hence `S(b, r)`, for bells whose caches, gathers or settlements could still be pending |
| §5.7 | CreateSeason refuses `program_version ≠` the binary's `PROGRAM_VERSION` (`BadData`) | W2-A minor |
| §8.2 | A capped write whose versions all expired ends (failure, backoff, `write-expired` alert) and is re-planned; the anchor scan window always reaches the newest bell (`anchor-missing` alert) | W2-F major: a hold longer than 64 versions + expiry stalled the write and the scan window for good |
| §8.3 | Exact account keys and ABI writability in the relay allowlist; settle quotas charged to a requester's on-chain-verified Citizen, else the client-address bucket; the replay key is claimed before any await; a send that throws charges nothing; simulation failures attributed to the failing program (System → `OperatorLowFunds`) | W2-D majors and minors |
| §8.7 | `localnet`'s block and account caps count the §10.1 cost, not the CU limit | W2-C major |
| §9.1 | The web chain clock runs at rate 1 except on a localnet season (estimated there, never below 1); staleness is measured against the local clock, not the herald-derived estimate | W2-E major |
| §9.4 | `messageProblems` requires the blockhash and the expected instruction and runs the relay allowlist; `sealMarch` derives the round from the season clock (T(arrive_bell)) and refuses any other | W2-E majors |
| §12 | Gate W2 runs `g01_budget_` too; `one_day_beacons` rings are exercised against the program from Gate W3 (OpenRing is W3-A's) | W2-A major (no CU gate); W2-F missing item (gate text vs the unit split) |

---

## 20. Amendments v1.4 (integ-W3, 2026-09-28)

Made in the wave-3 integration window (§3.4) to answer a Gate W3 item; normative from v1.4; the code landed on `frontier/m1-integ` in the integ-W3 commits; the decision is copied to `DECISIONS.md` part J; the record is `integ-W3-NOTES.md`.

| Section | Change | Why |
|---|---|---|
| §5.5 | Budget FileTicket 14k → **17k** CU (`frontier-abi::budgets`, vectors, fclient mirror, synced JS) | W3-A F3: 16,048 CU [measured, `g01_file_ticket_three_provinces_full_cohorts`: 3 provinces, 7 open cohorts each, the §13.1 fill] on the merged release `.so`; the cost is the player prologue (≈ 3.6k), three canonical Province addresses and their cohorts (≈ 3.7k), the escrow System CPI (≈ 2.2k) and the 4-chain `TICKET` (≈ 3.5k). FileTicket is class P (relay-sponsored, priority 0, no liveness role), so the budget moves no fee or tip formula |

Not amended in this window (open for the wave-3 review; recorded in `integ-W3-NOTES.md`): W3-A F4 (`chains_of(SETTLE)` and a shared JoinShard), F5 (§5.9 Join's wedge-fund clause), F8 (terrain encoding and ticket score in a shared crate), W3-B's presets request (`reveal_cu_limit`, `reveal_loaded_limit`, `tip_min`: to be regenerated from the release `.so` at W5), W3-C F1 (crowding rule), W3-D D2/D3/D6, W3-F R4 (`reachable`'s signature).
