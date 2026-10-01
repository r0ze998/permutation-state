# Wylls — Prototype QA Report

**Build reviewed:** Living Chronicle world-first prototype  
**Date:** 2026-09-21  
**Result:** **PASS — world-first redesign, all four Worksite 31 paths, continuous-world invariants, and the local MagicBlock handoff**  
**Scope note:** This covers browser behavior and the isolated local Solana + MagicBlock stack. It is not user validation, a security review, public-cluster verification, wallet authentication, or a human-operated stage-demo recording.

## Living Chronicle redesign revalidation

The September 21 visual rebuild replaced the forensic-dashboard hierarchy with a close, inhabited East Sluice scene. Gameplay, network, and canonical reducer contracts were preserved.

- `18/18` JavaScript client/server tests pass.
- `9/9` Rust program tests pass.
- The live local-stack path passes: base initialization → delegation → Mara oath → Ivo Service Stair resolution → verified Solana base-layer checkpoint → Nia mandate acceptance.
- Final local sequence is `3`; ER and checkpoint signatures are distinct and present.
- JavaScript syntax checks, required DOM-ID contract checks, unique-ID checks, and `git diff --check` pass.
- Desktop `1440 × 900`, compact desktop, and narrow mobile layouts were visually checked; the mobile proof orders the current citizen action before Almanac and Chronicle detail.
- The World Chronicle opens and closes without removing proof accessibility; Observer starts open, playable citizen views start closed.
- Runtime and browser-console errors observed during the redesign checks: `0`.

## Outcome

The corrected build no longer treats Ivo's local action as an ending, season victory, or payout trigger. Each path resolves only Worksite 31, appends the result to the Live Chronicle, generates three branch-specific Mandates, and leaves Season Zero active.

The browser was driven through all four paths from a cleared local fixture at a 1440 × 1000 viewport. The run exercised the real controls, including Mara's choice, the citizen handoff overlay, Ivo's branch-gated choices, Chronicle, Purse, and stored world state.

| Mara's civic act | Ivo's repair | Local result | Verified final state | Result |
|---|---|---|---|---|
| Grain Oath | Service Stair | `R31-1` · stabilized | Water 60, Cohesion 64, Timber 12, Food debt 12 | PASS |
| Grain Oath | Old Millrace | `R31-2` · stabilized | Water 54, Cohesion 56, Timber 15, Food debt 12 | PASS |
| Founding Charter | Cut Through | `R31-3` · stabilized with fracture | Water 55, Cohesion 41, Timber 8, Food debt 0 | PASS |
| Founding Charter | Reconcile | `R31-4` · stabilized | Water 51, Cohesion 54, Timber 12, Food debt 8 | PASS |

## Continuous-world assertions

Every path verified:

- `seasonStatus = active`;
- `seasonOutcome = unresolved`;
- `purseStatus = accumulating`;
- `settlementStatus = not_started`;
- the exact `R31-*` Worksite resolution and branch-specific status;
- the correct Memory Receipt, Tala trust, Food debt, service-stair state, and resources;
- exactly three branch-specific downstream Mandates;
- Chronicle copy stating that the season continues;
- Purse copy showing `348.50 MOCK USDC`, an active season, and settlement not started;
- no enabled or functional claim action.

The branch-specific Mandate sets also passed:

- `R31-1`: `M-032` Prepare Twelve Crates; `E-019` Write River Compact; `S-044` Inspect Flooded Archive.
- `R31-2`: `E-020` Answer for the Bypass; `M-032` Prepare Twelve Crates; `S-045` Survey Old Millrace.
- `R31-3`: `E-021` Stop Riverkeeper Walkout; `W-014` Hold Eastern Cistern; `S-046` Record Charter Cut.
- `R31-4`: `M-033` Prepare Eight Crates; `E-022` Ratify New Water Terms; `S-047` Map Reopened Stair.

## Reliability and layout checks

- Four complete browser paths passed.
- The 1.35-second Mara-to-Ivo identity handoff was observed on every path.
- Ivo received only the two actions allowed by Mara's inherited branch.
- Fifty-two desktop overflow checkpoints—thirteen per path—measured `0 px` horizontal overflow.
- Runtime exceptions: `0`.
- Console errors: `0`.
- Browser-log errors: `0`.
- Enabled claim actions: `0`.
- In-scope defects after the run: `0`.

A separate responsive audit found that the decision drawer itself remained width-safe through 280 px. It also identified and corrected top-bar and East Sluice marker clipping below 360 px, plus brand intrusion in an intermediate desktop range. Final 320 × 844 and 360 × 844 measurements both reported `0 px` document overflow; a full mobile interaction-path run remains advisable before a public build.

## Visual review

Reviewed states included:

- the initial Living Civic Atlas;
- Mara's Mandate and Tala encounter;
- the Memory Receipt and citizen handoff;
- Ivo's inherited Oath scene;
- cooperative `R31-1` and fracture `R31-3` resolutions;
- the Live Chronicle;
- the active, unsettled Season Purse.

Observed result:

- the inhabited East Sluice district is the main interaction surface rather than a decorative map behind a dashboard;
- shared civilization resources remain readable in a compact civic HUD while the parchment story journal holds the current conversation and choices;
- Tala's memory, the inherited action set, and the continuing season remain legible without exposing raw hashes during ordinary play;
- the optional World Chronicle retains event-chain, invariant, ER, and Solana-checkpoint evidence for judges;
- `PROTOTYPE WORLD`, local-cluster, disposable-signer, no-real-funds, ER, and Solana boundaries remain explicit where relevant;
- the result screen communicates “Worksite resolved; season continues” and immediately exposes new work.

## 1280 × 720 Judge Mode

The `&judge=1` presentation mode was visually checked on Ivo's inherited Oath scene and the `R31-1` result.

- The causal affordance ribbon states exactly what Mara changed and the counterfactual route that is absent.
- Both legal Ivo actions and the primary resolution button remain visible without drawer scrolling.
- The result leads with before/after Water, Cohesion, and Timber plus `SEASON ACTIVE`.
- All three generated Mandates are visible before the expanded causal trace.
- The main build retains the persistent map, shared resource ribbon, active citizen, and simulated-world disclosure.

The full four-path automated regression was rerun after these changes: all four paths passed, all 52 horizontal-overflow checkpoints remained at `0 px`, and runtime, console, and browser-log errors remained at zero.

## Still unproved

- real AI-generated Tala responses with structured-output validation and deterministic fallback;
- separate wallets, remote-device persistence, and network consensus; the companion Handoff Proof now verifies four separate same-origin browser clients on one device;
- a confirmed Solana devnet Memory Receipt, Worksite result, and shared-state update;
- a separate season-boundary settlement and claim fixture;
- external player comprehension, completion, replay intent, or willingness to return;
- 9/10 consecutive human-operated live-demo reliability; the automated four-client failure-injection suite passed 10/10;
- security, anti-sybil, legal, or real-money readiness.

These remain Evidence Ledger items. The clickable build proves the local causal loop and continuous-world semantics only.

The companion [`permutation-state-solana-receipt-spike/README.md`](permutation-state-solana-receipt-spike/README.md) supplies a host-tested native-Rust account/instruction contract and JavaScript adapter. The current suite passes 9/9 Rust and 18/18 JavaScript tests. The official isolated local stack also passes the complete ER-to-base checkpoint path; this does not change the public-devnet item above.

## Companion multi-client proof

The separate [`PERMUTATION_STATE_HANDOFF_PROOF.md`](PERMUTATION_STATE_HANDOFF_PROOF.md) artifact closes the earlier single-client identity-switch gap locally.

- Mara, Ivo, successor, and Observer run at separate role-locked URLs.
- Accepted events propagate across open clients and survive reload or late join.
- Current state is derived by deterministic replay of an append-only event log.
- SHA-256 links genesis, accepted events, and before/after state hashes.
- The read-only Observer shows diffs, branch assertions, invariant status, and client-head agreement.
- A third citizen accepts a branch-generated Mandate, proving that the local resolution creates later play.
- `session-proof.json` can be exported and independently replayed.
- Ten browser scenarios passed, including duplicate, stale-order, wrong-branch, tamper, reload, notification-loss, competing-command, and cross-session-isolation cases.

The UI continuously labels this as `LOCAL MULTI-CLIENT PROOF · NOT SOLANA · NOT LIVE AI`.
