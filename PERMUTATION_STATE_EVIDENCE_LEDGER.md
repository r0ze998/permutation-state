# PERMUTATION STATE — Hackathon Evidence Ledger

**Version:** 2026-09-21  
**Internal submission deadline:** 2026-10-11, 23:59 JST  
**Official deadline:** 2026-10-12, 23:59 PT  
**Official criteria:** Functionality, Potential Impact, Novelty, UX, Open-source, and Business Plan

This ledger is the single source of truth for submission claims. A claim is not “proved” until the underlying artifact, recording, metric, transaction, or public link exists and another team member has checked it.

Do not replace missing evidence with confident language.

## Submission Thesis

> Every season, all players become citizens of one civilization. Each person plays a local RPG mandate, but their verified outcomes change the choices available to other citizens. AI turns those outcomes into a responsive society; Solana fixes the civilization’s canonical history and shared season treasury.

The submission must prove one chain end to end:

> Player A makes a promise to Tala → the promise is recorded → Player B encounters a changed Tala and a newly available repair mandate → Player B repairs the aqueduct → Worksite 31 changes the shared civilization state → the Live Chronicle explains the causal chain → branch-specific Mandates are generated → Season Zero remains active and the Season Purse continues accumulating.

## Status Key

- **NOT STARTED:** No reviewable artifact exists.
- **DEFINED / UNVERIFIED:** The claim and intended proof are specified, but no external evidence exists.
- **IN PROGRESS:** A reviewable artifact exists but has not met its target.
- **PROVED:** The artifact exists, meets the target, and has been independently checked.
- **FAILED / PIVOT:** The evidence missed its target and a corrective decision is recorded.

`PROVED LOCALLY` is used only where a reproducible browser artifact and automated check now exist. No external playtest, traction, revenue, wallet-authenticated multiplayer, or Solana result has been assumed.

## Official-Criteria Evidence Matrix

| Official criterion | Claim the judge must believe | Current proof status | Exact next evidence | Target / acceptance condition | DRI | Deadline |
|---|---|---|---|---|---|---|
| **Functionality** | The multi-citizen A→B→C causal loop actually works, rather than existing only in a concept video. | PROVED LOCALLY / RECORDING PENDING — four role-locked browser clients share a replayable event log; the automated failure-injection suite passed 10/10. A human-operated uninterrupted video remains. | Record the proof path from Mara through Ivo and the successor, ending on the read-only Observer with matching heads and exported JSON. Retain the build number and session proof. | Complete the required path in **9 of 10 consecutive human-operated dry runs**; automated regression remains 10/10; the run ends with `Season ACTIVE`, a correct next-Mandate assignment, and no hidden reset or early claim. | `[Name — Tech]` | 2026-10-04 |
| **Functionality** | Solana, not the AI model or an administrator, commits accepted Worksite outcomes to the active shared season. | IN PROGRESS / NOT DEPLOYED — a native-Rust Worksite PDA scaffold and browser adapter now enforce actor signature, stale-head, branch, state-root, and no-claim invariants in host tests (Rust 4/4; JS 3/3). No SBF build, validator, devnet signature, or account read-back exists yet. | Build and deploy the scaffold, then publish devnet program/account identifiers and Explorer links for MR-031, the Worksite 31 resolution digest, and resulting shared-state update. Annotate which transaction proves each transition. | At least **one reproducible active-season devnet chain** covering accepted Civic Act → verified Worksite result → updated shared state, with confirmed signature and matching PDA read-back. Season settlement and claim are not triggered by this chain. | `[Name — Solana]` | 2026-10-02 |
| **Functionality** | A local Worksite cannot prematurely settle the Season Purse or create a claim. | PROVED LOCALLY — all four `R31-*` browser paths preserve `ACTIVE / UNRESOLVED / ACCUMULATING / NOT_STARTED`; no enabled claim action exists. Onchain enforcement remains unverified. | Port the same invariant matrix to program tests and devnet; if a payout demo is retained, isolate it behind an explicitly seeded season-finalization fixture. | Every Worksite path leaves `Season ACTIVE`, purse `ACCUMULATING`, settlement `NOT STARTED`, and claim unavailable; any separate finalization fixture is clearly labeled and unreachable from normal Worksite play. | `[Name — Tech/Solana]` | 2026-10-04 |
| **Functionality** | The demo remains credible when the model or RPC is slow or unavailable. | NOT STARTED | Run documented failure drills for model timeout, malformed structured output, rejected transaction, and page refresh. Capture expected recovery behavior. | The demo finishes safely using a disclosed deterministic fallback; no duplicated Worksite resolution, premature settlement, or exposed claim; recovery steps fit on one page. | `[Name — Tech]` | 2026-10-04 |
| **Potential Impact** | A real beachhead of strategy, shared-world, and onchain-game players cares about the problem. | DEFINED / UNVERIFIED | Conduct problem interviews and the two-player test using the companion Playtest Kit. Save anonymized notes, participant profile, behavioral commitments, and exact quotes. | **12–18 relevant external participants**, including shared-world/strategy players and at least three crypto-naive players; report all results, not only positive responses. | `[Name — Research]` | 2026-10-04 |
| **Potential Impact** | This can grow from one scripted crisis into a venture-scale seasonal world without requiring every action onchain. | DEFINED / UNVERIFIED | Create a one-page scaling model covering local Mandates, regional Fronts/Worksites, civilization-wide epochs, AI aggregation, and selective settlement. Add a bottom-up 100 / 1,000 / 10,000-citizen operating model. | Every scale assumption is labeled; costs and bottlenecks are shown; no unsupported “millions of users” or generic gaming-TAM claim. | `[Name — Product]` | 2026-10-05 |
| **Novelty** | “Everyone is one civilization” is a structural difference, not a reworded co-op or DAO. | RESEARCHED / ARTIFACT NOT STARTED | Publish a comparison against Eternum, Eco, Foxhole, Civilization, and relevant AI-NPC games across player identity, shared state, opponent, causality, AI role, season outcome, and treasury. | A cold reviewer can identify the unique wedge in **one sentence within 30 seconds**; every comparison is factual and sourced. | `[Name — Product]` | 2026-09-23 |
| **Novelty** | AI is a social simulation layer that changes later play, not a decorative chatbot. | IN PROGRESS — the prototype demonstrates the state change with authored Tala lines; live model expression is not connected. | Show the same NPC producing a materially different state, line, and available mandate after Player A’s structured commitment. Include before/after captures and the underlying structured event. | A reviewer can point to the exact Player A event that caused Player B’s changed option; the AI never unilaterally changes prize eligibility or payout. | `[Name — AI/Game]` | 2026-09-30 |
| **UX** | A new player understands that all players share one civilization and that another citizen changed their experience. | NOT STARTED | Run the 12-minute two-player test. Ask each participant to explain the game before the facilitator explains it. Retain verbatim answers. | After at least five sessions: **≥80% concept comprehension** and **≥80% of Player B participants correctly identify the specific A→B causal link**. | `[Name — Research]` | 2026-10-04 |
| **UX** | The core experience is playable without crypto expertise or facilitator rescue. | NOT STARTED | Measure first meaningful choice, completion, assistance level, transaction confusion, and total session time. Test at least three crypto-naive participants. | **≥80% session completion within 12 minutes**, median first meaningful choice **<2 minutes**, and no blocker in the final three sessions. | `[Name — UX]` | 2026-10-04 |
| **UX** | The player can see why the civilization changed and why the world continues, rather than reading unexplained stat movement or mistaking one Worksite for the season ending. | IN PROGRESS — the Cause Receipt, Live Chronicle, continuing-season copy, and branch-specific next Mandates are implemented and passed internal UI checks; external comprehension remains untested. | Test the Live Chronicle showing citizen action → NPC response → inherited mandate → Worksite resolution → generated next Mandates. Ask players to reconstruct the chain without prompts. | At least **4 of 5 Player B participants** reconstruct the chain in order, name one downstream Mandate, and state that Season Zero remains active; no more than one confuses local resolution with season victory. | `[Name — Game/UX]` | 2026-10-04 |
| **Open-source** | Judges can inspect, run, and distinguish real functionality from mock or future scope. | NOT STARTED | Publish an English README with a 10-second description, demo GIF, architecture, setup, seed scenario, program IDs, transaction links, tests, limitations, roadmap, and explicit “live / mocked / planned” labels. | A person outside the team can reach the seeded scenario using only the README; every submitted link works in a signed-out browser. | `[Name — Tech]` | 2026-10-06 |
| **Open-source** | The project is composable rather than a closed demo. | NOT STARTED | Document the minimum Live Chronicle event schema and how an external client can read active season, citizen, Worksite result, resulting state, and next-Mandate references. Keep any season-final settlement fixture separate. | Schema is versioned and licensed; one teammate not responsible for it successfully follows the active-season read path. | `[Name — Solana]` | 2026-10-06 |
| **Open-source** | The team did meaningful, strategically prioritized work during the hackathon. | NOT STARTED | Preserve commit history; disclose pre-existing work and third-party components; tag a submission release; write a short prioritization log explaining what was cut and why. | No ambiguous ownership; all pre-existing work is disclosed; the release corresponds to the recorded demo build. | `[Name — Tech/Producer]` | 2026-10-11 |
| **Business Plan** | Seasons and marketplace activity can fund operations and a shared purse without a native token. | HYPOTHESIS / UNVERIFIED | Produce a transparent funds-flow model separating network fees, application fees, seller proceeds, the prize vault, and protocol revenue. Model 100 / 1,000 / 10,000 active citizens. | All inflows equal all outflows; assumptions are editable; spending never creates contribution score; seller proceeds are not mislabeled as prize funding. | `[Name — Business]` | 2026-10-05 |
| **Business Plan** | There is a credible path to initial distribution and repeat seasons. | HYPOTHESIS / UNVERIFIED | Recruit a named “Founding Citizens” cohort from strategy/shared-world communities; document channel, response, scheduled tests, return behavior, and actual referrals. | At least **three observable commitments** beyond praise—such as completing a second mandate, booking the next test, or bringing another tester. Report denominator and drop-offs. | `[Name — Growth]` | 2026-10-09 |
| **Business Plan** | The team understands paid-entry risk and has a responsible launch sequence. | DEFINED / UNVERIFIED | Add a staged launch plan: free closed alpha → public mock-USDC season → jurisdiction-specific paid season only after legal/compliance review. State cancellation, refund, identity, anti-sybil, and failure policies as open questions where unresolved. | No claim of legal compliance without counsel; no real-money mainnet test in the hackathon demo; risks and owners are visible in the roadmap. | `[Name — Business/Legal]` | 2026-10-05 |

## Supplemental Startup Evidence

These items support the broader Colosseum review described in its hackathon FAQ, including Founder + Market Fit, Insight, Execution, Communication, Viability, and Traction.

| Evidence | Current status | Exact next artifact | Acceptance condition | DRI | Deadline |
|---|---|---|---|---|---|
| Founder–market fit | NOT STARTED | A 150-word founder story explaining the observed problem, relevant experience, why this team, and why it intends to continue after the hackathon. | Specific lived evidence; no generic passion statement; each claimed credential is verifiable. | `[Name — Founder]` | 2026-10-05 |
| Iteration speed | NOT STARTED | An iteration log with date, observed problem, evidence, decision, shipped change, and retest result. | At least three complete feedback → change → retest entries; failed hypotheses remain visible. | `[Name — Producer]` | 2026-10-09 |
| Founder communication | NOT STARTED | A 2:45–2:55 English pitch and a separate ≤3:00 product/technical demo. | Three cold reviewers can state product, user, unique insight, why Solana, and business model after one viewing. | `[Name — Founder/Visual]` | 2026-10-10 |
| Post-hackathon intent | NOT STARTED | A dated 30/60/90-day roadmap with the next season test, hiring/cofounder needs, legal work, and product milestones. | Named owners and measurable outcomes; no feature dump. | `[Name — Founder]` | 2026-10-05 |

## Evidence Artifact Register

Use stable public links wherever possible. Keep raw consented research material private and link only an anonymized synthesis in the submission.

| ID | Required artifact | Location / URL | Status | Reviewer | Due |
|---|---|---|---|---|---|
| E-01 | One-page Game Constitution and non-goals | [`PERMUTATION_STATE_GAME_CONSTITUTION.md`](PERMUTATION_STATE_GAME_CONSTITUTION.md) | READY FOR REVIEW | `[Name]` | Sep 21 |
| E-02 | Sourced competitor and precedent matrix | `[link]` | NOT STARTED | `[Name]` | Sep 23 |
| E-03 | 30-second comprehension-test record | `[link]` | NOT STARTED | `[Name]` | Sep 24 |
| E-04 | A→B→C uninterrupted Worksite demo recording | [`PERMUTATION_STATE_HANDOFF_PROOF.md`](PERMUTATION_STATE_HANDOFF_PROOF.md) | FOUR-CLIENT INTERACTIVE PROOF EXISTS; RECORDING NOT COMPLETE | `[Name]` | Oct 02 |
| E-05 | Active-season devnet proof map; separate season-final fixture if retained | [`permutation-state-solana-receipt-spike/README.md`](permutation-state-solana-receipt-spike/README.md) | LOCAL NATIVE-RUST + JS SCAFFOLD TESTED; SBF BUILD / VALIDATOR / DEVNET RECEIPT NOT STARTED | `[Name]` | Oct 02 |
| E-06 | Ten-run reliability log | [`PERMUTATION_STATE_HANDOFF_PROOF.md`](PERMUTATION_STATE_HANDOFF_PROOF.md) | AUTOMATED MULTI-CLIENT FAILURE-INJECTION SUITE PASSED 10/10; HUMAN-OPERATED STAGE RUN PENDING | `[Name]` | Oct 04 |
| E-07 | Anonymized playtest dataset and synthesis | [`PERMUTATION_STATE_PLAYTEST_KIT.md`](PERMUTATION_STATE_PLAYTEST_KIT.md) | PROTOCOL READY; DATA NOT STARTED | `[Name]` | Oct 04 |
| E-08 | Feedback → change → retest log | `[link]` | NOT STARTED | `[Name]` | Oct 09 |
| E-09 | AI / offchain / Solana boundary diagram | [`permutation-state-prototype/README.md`](permutation-state-prototype/README.md) | BOUNDARY TABLE READY; DIAGRAM NOT STARTED | `[Name]` | Oct 02 |
| E-10 | Funds-flow and bottom-up economics model | `[link]` | NOT STARTED | `[Name]` | Oct 05 |
| E-11 | Beachhead and distribution evidence | `[link]` | NOT STARTED | `[Name]` | Oct 09 |
| E-12 | Public repository and tagged release | `[link]` | NOT STARTED | `[Name]` | Oct 11 |
| E-13 | English pitch video | `[link]` | NOT STARTED | `[Name]` | Oct 10 |
| E-14 | English product/technical demo | `[link]` | NOT STARTED | `[Name]` | Oct 10 |
| E-15 | Completed submission link and confirmation | `[link]` | NOT STARTED | `[Name]` | Oct 11 |

## Claims That Must Remain Qualified

Do not use the following statements unless the corresponding evidence exists:

- “Players love it.” Use the observed behavior and denominator instead.
- “Players will pay.” A hypothetical survey answer does not prove payment behavior.
- “The system supports thousands of concurrent citizens.” A scaling design is not a load test.
- “The economy is sustainable.” A balanced spreadsheet is a hypothesis, not retention or revenue.
- “Legally compliant.” Compliance requires jurisdiction-specific professional review.
- “Secure” or “trustless.” State exactly what is verified, who can upgrade it, and what remains offchain.
- “Fully onchain.” The design is intentionally selective; describe the actual boundary.
- “World model.” Use this only if the system predicts or simulates world state beyond authored branching and LLM narration.
- “Traction.” Report completed tests, repeat sessions, referrals, or revenue separately.

## Evidence Review Ritual

At the end of each workday, the team answers five questions:

1. Which judge objection became harder to make today?
2. What concrete artifact proves that?
3. Where is the artifact linked in this ledger?
4. What failed or contradicted the hypothesis?
5. Which planned task should now be cut?

A task should continue only if it does at least one of the following:

- proves the core A→B experience;
- proves that the resolved Handoff creates later play rather than ending the season;
- removes a demo reliability risk;
- produces user evidence;
- answers an official judging criterion; or
- makes the submission materially easier to understand.

## Final Evidence Gates

### Gate 1 — Comprehension, September 24

- At least 4 of 5 cold viewers can say that all players share one civilization.
- At least 3 of 5 can describe the A→B causal premise.
- If failed: rewrite the opening, onboarding, and first screen before building more systems.

### Gate 2 — Core Experience, September 27

- Player B can identify that Player A changed their available mandate.
- Player B can state that Worksite 31 resolved while Season Zero remained active.
- At least half of participants select or request one of the generated next Mandates.
- If failed: remove economy exposition from the opening and strengthen the before/after consequence.

### Gate 3 — Evidence-Ready Product, October 4

- Five or more two-player sessions are logged.
- At least 80% finish within 12 minutes without major facilitator assistance.
- The A→B Worksite chain, next-Mandate composition, and active-season devnet proof are reproducible.
- If failed: cut secondary marketplace presentation, additional NPCs, and visual polish before cutting the causal chain.

### Gate 4 — Judge Readiness, October 8

- Every official criterion has at least one reviewable artifact.
- Pitch is under 2:55 and technical/product demo is under 3:00.
- Three cold reviewers can explain the product, user, difference, Solana necessity, and business model.
- If failed: revise the story and evidence order; do not add features.

### Gate 5 — Submission Safety, October 11

- All links work in a signed-out browser.
- Repository release matches the recorded build.
- All content is English and all mock/planned elements are labeled.
- Pre-existing work and third-party assets are disclosed.
- Submission confirmation is saved before the internal deadline.

## Official References

- [Crypto World’s Fair Official Rules](https://colosseum.com/legal/Crypto%20World%27s%20Fair%20Hackathon%20Rules.pdf)
- [Colosseum Hackathon FAQ and Submission Requirements](https://colosseum.com/hackathon)
- [Crypto World’s Fair](https://colosseum.com/worldsfair)
- [Perfecting Your Hackathon Submission](https://blog.colosseum.com/perfecting-your-hackathon-submission/)
