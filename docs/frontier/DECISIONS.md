# The Sixfold Frontier: decisions log

Kept with the M1 integration contract (`m1/M1-CONTRACT.md`, **v1.2**, 2026-09-27; v1.0 kept as `lab/contract-rev/DECISIONS.v1.0.md`). Committed here by unit W1-D (CL-37) and linked from `README.md`; `m1/DECISIONS.md` is a pointer to this file. Earlier decisions (O1–O10, D1–D25) are in DESIGN.md §14. Part F records what the first M1 week measured for the questions still open.

**Standing rule:** every devnet step and every push needs the owner's separate approval. One push of `codex/frontier` was approved and made on 2026-09-27 (`d95fa25`); no further push is assumed. Local commits on work branches are allowed.

Status words: **decided** (by the owner), **architect** (decided in the contract; reversible without owner input), **default** (the contract's working default; the owner is asked, see part C).

---

## A. Owner decisions in force

| # | Date | Decision | Consequence for M1 |
|---|---|---|---|
| O1 | 2026-09-27 | drand **quicknet** for bell, ring and genesis seeds and tlock seals; program built for **SBPF v2** | One network; `beacon` kernel; `build-frontier.sh --arch v2` |
| O2 | 2026-09-27 | Genesis seed from drand only; no MagicBlock VRF anywhere | `genesis_seed_round`; no VRF code |
| O3 | 2026-09-27 | Bounded officer pay (95% ceiling) | Not in M1 play; `PayoutParams::validate_for_season` at CreateSeason (CL-11) |
| O4 | 2026-09-27 | Bot criterion plan (steps in order) | Criterion re-run with D23 on held-out seeds (CL-16, CL-31) |
| O5 | 2026-09-27 | Asymmetric doctrines without direct multipliers, gated in CI | Outcome-changing kernel fixes (CL-09, CL-10, Phase B) re-run the gate |
| O6 | 2026-09-27 | γ = 0.6 | Unchanged |
| O7 / N1 | 2026-09-27 | C4 restated: tested with keeper bids above the tip from a defence pool, cap priority 2.0, paid from ≥ 150 rotating keeper fee payers; the default-tip failure is a recorded known result | Keeper escalation classes, ≥ 150 payers enforced, evidence fields, ClaimDefence |
| O8 | 2026-09-27 | Season 1 on Solana base only; no Skirmish, Arena or ER lane | No MagicBlock code in M1 |
| N2 | 2026-09-27 | D25 decided: valuation **(a)**, $7 × every clash the targeted side takes part in | c4 v3 (CL-26/30) reports against (a) |
| N3 | 2026-09-27 | (Fact) one push of `codex/frontier` approved and made; GitHub CI run 36303403992 | Further pushes need new approvals |
| N4 | 2026-09-27 | SP-V2 `RESULTS.md` replaces separate S-BEACON and S-TLOCK reports | M0 close item 8 closed |
| N5 | 2026-09-27 | Start M1 now; close the remaining M0 items in M1's first week | Wave 1 carries CL-01…CL-38; G0 tasks land before any gameplay instruction links a kernel |
| D23 | 2026-09-27 | At most one office-term per wallet per season | Simulator default 1 (done, W1-D: `Config::office_term_limit = Some(1)`, vacancies counted); kernel `GovernanceParams` (W1-C); `Citizen.office_terms_used` reserved (W1-E); vacancy rule for small seasons (sim: a seat with no eligible wallet stays vacant, no pay) |
| — | 2026-09-27 | **Answers to part C, as relayed to the M1 units by the M1 workflow:** every working default O-M1-01…O-M1-24 accepted, except **O-M1-12 (downloads and installs: not approved)** and **O-M1-18 (devnet playtest: not approved)**; O-M1-17 as "fix the legacy tests locally, no push" | Part C's status column records this. The integrator confirms it with the owner's own wording at the wave-1 merge; until then it is recorded as relayed, not as the owner's text |
| — | 2026-09-27 | (Fact, relayed by the M1 workflow with the wave-2 briefs) The `rustfmt` and `clippy` components of toolchain 1.95.0 were installed by the main session **with the owner's explicit OK**; the `frontier-node` fmt and clippy gate lines therefore run normally. This answers the question in `m1/integ-W1-NOTES.md` §F (who installed them). O-M1-12 (wasm32 target, Playwright Chromium, the round archive, Agave ≥ 4.0) stays not approved | Gate W1/W2 `frontier-node` fmt/clippy lines run as written; every O-M1-12 item stays `PENDING-OWNER` (recorded by W2-A) |

---

## B. Integration decisions made in the contract

Numbers match the conflict register (`M1-CONTRACT.md` §2).

| # | Decision | Status |
|---|---|---|
| I-01 | Every program account except the Season PDA is a with-seed address of the Season PDA | default (O-M1-01) |
| I-02 | Seed grammar = SP-V2 byte for byte: `tag ‖ lowercase hex of LE fixed-width fields`, i32 coordinates, u32 days, ≤ 32 B | architect |
| I-03 | `frontier-abi` crate (no Solana deps) holds layouts, tags, errors, logs, budgets, vectors | architect |
| I-04 | The fee payer pays rent and gets it back (`rent_to` = payer, v1.1 via I-49); tips, march fees and rewards go to the `beneficiary` in the data | architect |
| I-05 | The player prologue's `payer` signer funds rent and escrow; the Holding's rent is escrowed at FileTicket by the ticket funder (v1.1, I-47); escrow refunds go to `Holding.rent_payer` | architect |
| I-06 | Reveal and SettleTransit take the salt, not the seal key; the browser never stores `k` or `sigma` | architect |
| I-07 | One-way reveal latch: ClashInputs absence + destination `resolved_next ≤ arrive` | architect (CL-23) |
| I-08 | Minimum tip in priority terms: `ceil(0.433 × (limit + 1,320 + 8·⌈L/32 KiB⌉)) + 2,500` (14,441 at 26k with L = 1 MiB); zero tip removed | default (O-M1-06) |
| I-09 | AnnounceSeason ≥ 24 h before CreateSeason; single-use ids; 1 SOL creation bond burned on a post-round pre-join abort | default (O-M1-07) |
| I-10 | Quiet-bell proof: ArrivalDay bitmap + SkipQuiet | default (O-M1-02) |
| I-11 | SettleDeparture copies departed-host values into the Holding; GatherClash reads Holdings | default (O-M1-03) |
| I-12 | Transit outcome by rank against the final arrival set; quota-refused arrivals bounce without loss | default (O-M1-03) |
| I-13 | ResolveFromInputs writes the Province and the same province-bell's ClashInputs | default (O-M1-03) |
| I-14 | Clash Phase A in wave 1 (digest-identical); Phase B decided at the wave-5 gate | Phase A architect; Phase B owner (O-M1-04) |
| I-15 | Budgets: ResolveFromInputs 340k, worst-case clash 460k, Reveal 26k, SettleTransit 85k; heap ≤ 28 KiB at gated fills; budgets are gates, with the keeper's CU/heap retry ladder (I-50) | architect |
| I-16 | No postures in M1 | default (O-M1-05) |
| I-17 | No pair tickets, holdings 2–3, sieges, captures, occupation or diplomacy in M1 | architect (M1 scope) |
| I-18 | Every seed and tlock round is "the first round at or after" | architect (CL-19/20) |
| I-19 | Superseded by I-47 (ticket cohorts) | default (O-M1-13, O-M1-20) |
| I-20 | Pay-or-divert for every payment | architect |
| I-21 | Keeper classes: W = Reveal (to 2.0, pool-eligible); D = delay-only writes incl. (v1.1) SettleTicket, OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy, ClaimDefence (to 0.5, own budget); N = the rest | default (O-M1-08) |
| I-22 | ClaimDefence in M1 (wave 4), Reveal-only eligibility, 6-bell claim grace (I-52); first item on the cut list | default (O-M1-08) |
| I-23 | Keeper start bid = tip level; `--peace-start` flag off | default (O-M1-09) |
| I-24 | Owner self-reveal: the browser posts reveal material to `/gw/f/reveal`; the keeper builds and escalates; `/f/relay` refuses Reveal shapes | architect |
| I-25 | Exit season in Mode A (LiteSVM-backed local node, 20×, real 400-ms slots, real historical quicknet rounds); Mode R optional | default (O-M1-10) |
| I-26 | All M1 services and runs on ports 41000–41999; tests on port 0; gates check only M1's ports, never that reserved ports are idle | architect (task rule) |
| I-27 | `retreat_bps` 0 = never; 1..=60,000 valid; above is an invalid plaintext; `RETREAT_MAX_BPS` lives in `clash.rs` | architect |
| I-28 | An invalid plaintext inside a valid seal is a bad seal (SettleTransit seal code 5) | architect |
| I-29 | Resident actions and Depart need a final holding | architect (closes an A1 freeze) |
| I-30 | Genesis rings opened by OpenRing after Seeded; rings 0–1 have reserved sites; tickets from ring 2 | architect |
| I-31 | Holding 1,280 B (`pool_owed` from its reserve); Citizen 384 B; per-player rent 9,753,600 lamports | default (O-M1-15) |
| I-32 | Depart charges the maximum march stamina | default (O-M1-11) |
| I-33 | Onboarding times restated (first holding 11–21 min, first clash report 31–41 min) | default (O-M1-13) |
| I-34 | Doctrine C displayed as "Flame" | default (O-M1-14) |
| I-35 | Presentation-only fog with a "show everything" switch | default (O-M1-14) |
| I-36 | Bot profiles copied into `frontier-agents` with an equality test | architect |
| I-37 | Program and ABI in the root workspace, own workspace if the lock does not resolve | architect |
| I-38 | PS2 log contract with per-entity `(seq, head)` tails; DEPART carries commitment and seal | architect |
| I-39 | Keeper error mapping; new codes 51–61 | architect |
| I-40 | Session pubkey in Join's data, no session signature | architect |
| I-41 | New kernels `camp`, `explore`, `catalog` in wave 1, specified from the simulator (I-56); no `Effect::Troops` | architect |
| I-42 | Phase A mutation-count discrepancy (5/7 vs 4/5) resolved by the wave-1 re-run | architect |
| I-43 | Storage-aware clash room: pending musters and departed entries limit the arrivals admitted; the rest bounce without loss | default (O-M1-21) |
| I-44 | SettleTransit opens and judges every seal (also after archive: the archive stores signatures); ProveBadSeal and SealVerdict removed; hosts in unsettled transits cannot act | default (O-M1-19) |
| I-45 | Loaded-data limit per instruction kind from the release `.so` (SIMD-0186), not 64 KiB | architect |
| I-46 | GatherClash refuses resolved or skipped bells; SettleTransit uses only resolved inputs; status sets for OpenRing, ConsumeRingSeed, OpenProvince; re-creation tests | architect |
| I-47 | Ticket cohorts: finality waits for the cohort (≤ 24 bells); displacement without a deadline; SettleTicket class D; Holding rent escrowed at FileTicket | default (O-M1-20) |
| I-48 | No W or D instruction except ClaimDefence writes the DefencePool (`Holding.pool_owed` + SweepPoolOwed); ProvinceFund per wedge; lock table §8.8 | architect |
| I-49 | Rent refunds to the payer that paid; reveal and delay payer pools; band from R99; ≥ 4 funders; effective N gated in E5 | architect |
| I-50 | CU and heap escape hatches; SkipQuiet CU-aware stop; RFI measured on all fills with the full write-back | architect |
| I-51 | `Season.join_gate`; AnnounceSeason by the upgrade authority; relay tip presets, CU price 0, requester-charged settles, XFF from the herald only | default (O-M1-23) |
| I-52 | Claim grace: SettleTransit keeps claim-eligible slots 6 bells; displacement resets `claimed` | architect |
| I-53 | `test-beacon` build and `drand-replay --test-key` for gates, nightlies and rehearsals; real rounds for W6 runs and the exit; `PENDING-OWNER` items | default (O-M1-12) |
| I-54 | Mode A keeps 400-ms real slots; latency criteria in slots at 20× plus a scale-2 latency run; pre-season at scale 2,000 | architect |
| I-55 | The integrator owns manifests, lockfiles, toolchain files; ownership gaps closed; two-day integration windows | architect |
| I-56 | camp/explore/catalog from `frontier-sim` (one camp per province on a non-site tile + an initial camp; Train immediate; explore floor Works only) | default (O-M1-22) |
| I-57 | 7 waves; W1-E split; W4-F integration unit; 12 weeks nominal, ≈ 62.5 ew | default (O-M1-24) |
| I-58 | Verifier V11–V13, `BadSealSurvived` FAIL, tampers T17–T22 | architect |

---

## C. Owner questions (status 2026-09-27: working defaults accepted as relayed, see part A)

| # | Question | Default → status |
|---|---|---|
| O-M1-01 | With-seed addresses for player accounts (I-01) | yes |
| O-M1-02 | ArrivalDay + SkipQuiet (I-10) | yes |
| O-M1-03 | SettleDeparture, rank-based transit outcomes, ResolveFromInputs writing ClashInputs (I-11..13) | yes |
| O-M1-04 | Clash Phase B (rules change) before the exit if the gates re-pass (I-14) | decide at the W5 gate |
| O-M1-05 | No postures in M1 (I-16) | no postures |
| O-M1-06 | Minimum tip in priority terms; zero tip removed (I-08) | yes |
| O-M1-07 | AnnounceSeason and the creation bond in M1 (I-09) | yes, 1 SOL (test SOL) |
| O-M1-08 | D18 re-size and ClaimDefence in M1, class D with a claim grace (I-21, I-22, I-52) | yes / yes |
| O-M1-09 | Keeper start bid (I-23) | tip level |
| O-M1-10 | Mode A as the exit run; Mode R optional (I-25) | Mode A |
| O-M1-11 | Depart charges the maximum march stamina (I-32) | yes |
| O-M1-12 | Downloads and installs, asked 2026-09-28: wasm32 target (by 10-07), Playwright Chromium (by 11-16), ≈ 250k contiguous quicknet rounds (by 11-25; the exit needs them), Agave ≥ 4.0 for Mode R (optional) | **not approved** (2026-09-27): every gate item that needs one is `PENDING-OWNER`; the test-beacon build and `drand-replay --test-key` meanwhile |
| O-M1-13 | Ticket finality by cohort and onboarding times in DESIGN §2.2/§2.3 (I-47, I-33) | yes |
| O-M1-14 | Doctrine C display name; fog switch (I-34, I-35) | "Flame"; switch on |
| O-M1-15 | Holding 1,280 B (I-31) | yes |
| O-M1-16 | D22 stake ramp, D24 Relic Sites, Season-1 sweep target (CL-17), D23 details (caretaker term not counted; vacant seats allowed) | closeout §7 defaults; not needed for M1 |
| O-M1-17 | Fix the two legacy `civilization` tests (CL-35); a push approval when GitHub CI should run | **done locally (W1-D, option a), no push**: both were stale expectations (a ninth building kind; storage back-pressure) |
| O-M1-18 | After the exit: the private devnet playtest (50–200 people, no money), sponsorship values, invites, hosting | **not approved** (2026-09-27) |
| O-M1-19 | Settlement is the seal proof; ProveBadSeal and SealVerdict removed; hosts in transit cannot act (I-44) | yes |
| O-M1-20 | Ticket cohorts; SettleTicket class D; a displaced ticket ends (I-47) | yes |
| O-M1-21 | Storage-aware clash room (outcome change only with pending or departed entries) (I-43) | yes, after the doctrine proxy gate re-run |
| O-M1-22 | Camps, exploration and training as the simulator models them (I-56) | yes |
| O-M1-23 | Playtest policy: on-chain invites (`join_gate`), upgrade-authority AnnounceSeason, tip presets, requester-charged settles (I-51) | yes |
| O-M1-24 | Schedule: 7 waves, 12 weeks nominal (80% 14.5), ≈ 62.5 ew; cut order ClaimDefence → herald WS → screenshot matrix → practice what-if (I-57) | accept |

---

## D. Still open from earlier revisions

- **D18** defence-pool size: **re-sized (CL-30)**, pool-eligible writes limited to reveals; measured 0.0105 SOL per attacked p99 bell at 50k wallets, so 20 SOL covers ≈ 1,900 p99 bells → **keep 20 SOL** (owner to confirm; DESIGN §21.3).
- **D22** stake ramp: **re-run with D23 on (CL-32)**: ramp 1.0 worst bot choice 0.983 on seeds 201–203 but **0.989 on the held-out seeds** (rule: ≤ 0.985), ramp 2.0 0.979 / 0.985; late stakers +3–4 points at 1.0. **Recommendation: keep 2.0** (1.0 only if the owner accepts a ≈ 1-point margin); decided before M2 (DESIGN §21.4).
- **D24** Relic Sites' role: Works and Dominion only; **the Mandate-reserve variant was measured (CL-33)**: worst bot choice 0.977 (passes); the owner decides before the M3 relic work.
- **Season-1 sweep target** (CL-17): escrow for a successor season with a fee-pro-rata fallback after 90 days (recommended).

## F. M1 wave-1 records (unit W1-D, 2026-09-27)

| # | Record | Evidence |
|---|---|---|
| F1 (CL-34) | GitHub CI run 36303403992 (push of `d95fa25`, 2026-09-27 07:32 UTC): "Rules, program (host + SBF + LiteSVM), server" **passed in 13 min 42 s** (the SBF + LiteSVM step 8 min 22 s); "Frontier simulator" **passed in 34 min 39 s** (its tests 34 min 12 s), inside the 15–40 min estimate; "Gateway and web client" passed (1 min 02 s); "Solana receipt scaffold" passed (2 min 07 s); "Browser source smoke test" **failed** on the two legacy civilization tests | `gh run view 36303403992 --json jobs` (read-only) |
| F2 (CL-35) | The two legacy failures were stale expectations, fixed locally (option a): the lens test expected 8 building kinds (9 since `cb92a2f` added the warehouse); the 15-minute run expected every building active, but the storage capacity of `cb92a2f` blocks a producer whose shared store is full (designed back-pressure). **The second change is a relaxed assertion under option (a), not a stale count** (integ-W1 review): `core.test.mjs` now allows a building blocked by storage back-pressure, guarded by the blocking reason text and the store's capacity. 47/47 pass locally; **no push** | `node --test permutation-state-prototype/civilization/*.test.mjs` |
| F3 (CL-36) | CI jobs defined for M1: the held-out criterion step in the simulator job; `frontier-program` (root fmt, rules and ABI lints, ABI tests and vector freshness, program host tests, SBF v2 build twice, LiteSVM suite, the v9 no-touch diff), `frontier-node`, `frontier-wasm`; each step runs once its crate exists | `.github/workflows/ci.yml` |
| F4 (CL-31) | D23 is the simulator default. Criterion with D23: 0.979 (seeds 201–203), **0.985 on the held-out seeds 30002–30004** (Gate W1's `--first-seed 30001 --gate`); bots' office-terms at 1% bots 60% → 9% | DESIGN §21.4 |
| F5 (CL-31) | **Finding: with D23 on, the doctrine proxy gate loses its Knight control** (−0.178% against the ±0.2% bound on the 30 gate seeds; the kernel table passes at 0.152%, the draft control is rejected). The per-push proxy keeps its calibrated economy (no term limit, `balance::gate_config`); the O5 band with D23 on passes 6/6 on 1,500 paired seasons (largest gap 1.0 point). Re-calibrate the proxy (more seeds or a tighter bound) at the W5 Phase B gate. **Superseded by G12** (re-calibrated in the integ-W1 window) | `m1/W1-D-NOTES.md` §3 |
| F6 (CL-26, I-49) | c4 v3: C4 passes on valuation (a) with the pool at 2.0 and ≥ 150 rotating payers (11.8–47.9× at 600 s, 23.5–95.9× at 1,200 s); fails at the minimum tip at 600 s. **R99 = 243 reveals per bell at 50k [sim]** against the contract's default 4,000 for the payer band (kept until M1 measures reveals in play) | DESIGN §21.2, §21.3 |


## G. Integration window W1 (integrator, 2026-09-27, after the wave-1 review)

Contract amendments are in `M1-CONTRACT.md` v1.2 §18; the review response item by item is `m1/integ-W1-NOTES.md`.

| # | Decision | Status | Evidence |
|---|---|---|---|
| G1 (CL-01) | A garrison never exceeds `MAX_HOST_TROOPS` (30,000 troops): `GarrisonState::change` refuses past `room()`, `settle`/`apply_clash`/`new` clamp. `clash::validate` keeps refusing a garrison above the cap (review option a). A program path that must not fail (a host returning home) adds at most `room()` | architect | `a_garrison_at_the_cap_resolves_and_cannot_pass_it` |
| G2 (CL-10, W1-A D1) | The cap recount is **one pass** into free hex slots, not the closeout's "until the counts are stable" cascade (which doubled K3 and let a refused arrival displace an admitted host) | architect | W1-A notes §4 D1; `the_cap_recount_takes_only_free_hex_slots` |
| G3 (CL-10, W1-A D3) | Civilians (Scout, Settler) never engage, contest a hex, hold it or count for a siege either way; DESIGN §6.1 reconciled | architect | `scouts_do_not_contest_a_tile` |
| G4 (CL-09) | A vigil change skips the new schedule's first window unless it starts ≥ 1 day after the last old window started: no run of covered bells > 48, ≤ 48 covered in any 144 | architect | `no_vigil_covers_more_than_48_bells_across_a_change` |
| G5 (CL-02) | Holding caps pinned from simulator peaks ×2 (walls 1,200, production 1,000/h), not from the catalog maximum × 3; the walls cap is a new player-visible refusal (`AboveCap` → `Kernel` 15), gameplay limit 1,200 wall points | architect (W1-B deviation) | W1-B notes §2 |
| G6 (§3.2) | `RULESET_HASH` binds a version for **every** frontier module, the doctrine and stance tables and `KERNEL_CONSTANTS`; v10 is pre-deployment, so no `RULES_VERSION_FRONTIER` bump; modules changed in wave 1 are at v2. Hash `1ac11f85…d03f` | architect | `ruleset_hash_binds_versions_and_catalog`, `ruleset_hash_is_the_kernels` |
| G7 (§5.12) | ArrivalSlot flags bit 2 = created_day; the priority fee in the refund rounds **up** like the runtime | architect | `evidence_reads_the_created_day_flag`, `defence_refund_equals_the_kernels` |
| G8 (§5.9) | FoldOccupancy has three parts (24 shards, 24 shards, 6 funds); SettleTicket's seed position is `seedcache\|archive`; `MULTI_MAX_REGIONS` = 7; §5.5 tx ceilings: gate on `budgets::tx_ceiling` (the byte model), the table values are informative | architect | frontier-abi budgets tests |
| G9 (§6, §4.1) | RING_SEED chains nothing; `po` seed is 13 raw / 28 B (SP-V2); SEASON_CREATED (138 B) is a third exception to the 128-B soft ceiling | architect | W1-E notes |
| G10 (Concord) | The Concord (0,0) has no wedge: OpenProvince of ring 0 is funded from wedge 0's ProvinceFund (`pf‖0`) | default (W3-A implements; owner may change) | `ProvinceCoord::wedge() == None` |
| G11 (CL-31) | The simulator's D23 exempts the caretaker first term (H2), as the kernel's `office::counts_toward_limit` | architect | `one_office_term_per_wallet_and_vacant_seats` |
| G12 (CL-31) | The doctrine proxy gate runs the shipping economy (D23 on) at **60 seeds** (was 30 seeds on the pre-D23 economy); bound ±0.2% unchanged | architect | integ-W1 notes §D; `doctrine-gate --controls` |
| G13 (§8.7) | drand-replay serves a round when `round_time + delay ≤ game_now`, `game_now` = the last observed chain Clock | architect | `chain_clock_never_extrapolates` |
| G14 | `frontier-abi` is a dev-dependency of `fclient` (path only) for the twin tests; fclient re-exports the kernel's seal, clock and host-id rules | architect | fclient `twin_tests` |

## E. Change log

| Version | Date | Change |
|---|---|---|
| v1.0 | 2026-09-27 | First issue with the M1 integration contract v1.0 |
| v1.1 | 2026-09-27 | Review revision: 21 issues (5 blocker, 16 major; 3 merged as duplicates) answered in `M1-CONTRACT.md` §17; new I-43..I-58; I-04, I-05, I-08, I-14, I-15, I-19, I-21, I-22, I-25..I-28, I-31, I-39, I-41 revised; new O-M1-19..24; O-M1-12 dated and sized |
| v1.1 (W1-D) | 2026-09-27 | Moved to `docs/frontier/DECISIONS.md`; the relayed answers to part C recorded; part D updated with the CL-30/32/33 measurements; part F (wave-1 records) added |
| v1.2 (integ-W1) | 2026-09-27 | Part G (the integration window's decisions after the wave-1 review); F2 wording corrected; contract v1.2 |
