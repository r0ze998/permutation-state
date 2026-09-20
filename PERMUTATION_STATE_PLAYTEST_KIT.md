# PERMUTATION STATE — Playtest Kit

## Season Zero: The Water Debt

**Format:** Two players, sequential A→B session  
**Session length:** 12 minutes  
**Purpose:** Test whether a local action by one citizen creates a legible, meaningful change in another citizen’s RPG experience—and whether both players understand that they share one civilization.

This is a research protocol, not a sales demo. Do not teach the intended insight before measuring whether players discover it. Do not record a positive result unless the participant actually demonstrates it.

## Research Questions

1. Do players understand that they are citizens of the same civilization, not members of competing teams?
2. Does Player A feel that the negotiation is a meaningful choice rather than flavor text?
3. Does Player B notice that Player A’s action changed Tala, the available mandate, or the cost of repair?
4. Can players reconstruct the causal chain from citizen action to the updated civilization state and later Mandates?
5. Does the Live Chronicle make the shared result and the fact that the season continues legible?
6. Is the experience usable within 12 minutes without facilitator rescue?
7. Does either player take an observable next step: selecting a generated next Mandate, booking another test, or inviting someone?

## What This Test Does Not Prove

- Long-term retention
- Willingness to pay real money
- Legal viability of paid-entry seasons
- Large-scale concurrency
- Sustainable unit economics
- General market demand

Report those as open hypotheses unless separate evidence exists.

## Seed Scenario

The East Sluice is closed. At sunset, 4,800 citizens lose water because Aster failed to repay an earlier grain debt to the River Guild. Tala, the Guild’s keeper, controls access to the repair site.

Starting state:

- **Civilization:** Aster, one shared civilization for all players
- **Crisis:** `THE_WATER_DEBT`
- **Water:** 28 / 100
- **Food:** 46 / 100
- **Cohesion:** 54 / 100
- **Timber:** 18
- **Season Purse:** 348.50 MOCK USDC; no real funds
- **Player A:** Mara Venn, Envoy
- **Player B:** Ivo Sen, Maker
- **Public Worksite stabilization rule:** Water ≥ 50 and Cohesion ≥ 50 after Ivo’s repair
- **Season status after every branch:** ACTIVE; purse accumulating; settlement not started; no claim available

The exact numerical values may be changed between builds, but the build number and seeded state must be recorded before each session. Do not change them during a session.

## Intended Causal Branches

Player A must be free to choose. The facilitator must not recommend a branch.

| Player A commitment | Persistent event | What Player B should encounter | Intended civilization consequence |
|---|---|---|---|
| **Bind the Grain Oath:** promise 12 Food at Epoch close | `MR-031-OATH` | Tala acknowledges the pledge; the Service Stair and Riverkeeper help become available | Water 36, Cohesion 60, 12 Food debt; Ivo may choose Service Stair or Old Millrace |
| **Invoke the Founding Charter:** claim access without a new debt | `MR-031-CHARTER` | Tala cites Mara’s refusal; the Service Stair is locked and Riverkeepers withdraw | Water 33, Cohesion 46, no Food debt; Ivo may Cut Through or Reconcile |

Ivo then makes one of two branch-specific repairs. Three of the four local resolution paths stabilize Worksite 31. `Charter → Cut Through` restores Water but leaves Cohesion below 50, so a civic-fracture bottleneck remains open. All four paths leave Season Zero active, keep the mock Season Purse accumulating, and generate later Mandates.

Use these resolution codes and expected downstream Mandates:

| Code | Local resolution | Required generated next Mandates |
|---|---|---|
| `R31-1` | Oath → Service Stair; Worksite stabilized; 12 Food debt persists | Prepare Twelve Crates; Write River Compact; Inspect Flooded Archive |
| `R31-2` | Oath → Old Millrace; Worksite stabilized; Tala remembers the bypass | Answer for the Bypass; Prepare Twelve Crates; Survey Old Millrace |
| `R31-3` | Charter → Cut Through; Water restored; Cohesion 41 | Stop Riverkeeper Walkout; Hold Eastern Cistern; Record Charter Cut |
| `R31-4` | Charter → Reconcile; Worksite stabilized; 8 Food debt persists | Prepare Eight Crates; Ratify New Water Terms; Map Reopened Stair |

The test passes only if the branch shown to Player B matches Player A’s recorded choice. A plausible AI line with the wrong structured state is a failure.

## Participant Profile

For the full test round, recruit at least five two-player pairs. Seek a mix of:

- strategy, MMO, Eco, Foxhole, Civilization, or Eternum players;
- Solana or crypto-game players;
- at least three crypto-naive participants;
- people who did not help design the product.

Do not use only friends who already know the intended pitch.

## Materials and Setup

- Two separate devices, browsers, or isolated sessions
- Test accounts or disposable devnet wallets; never use a participant’s seed phrase
- Seeded build of `THE_WATER_DEBT`
- Stopwatch visible only to the observer
- Observation sheet from this document
- Screen/audio capture if separately permitted
- Backup note-taking method
- Build number, program ID, and reset procedure
- Deterministic AI fallback enabled and disclosed in the session record if used

Before the participant arrives:

1. Reset the local Worksite test fixture.
2. Confirm Player A and Player B have separate identities.
3. Confirm the incident-specific Chronicle segment has returned to its seed state. This is a test reset, not an in-world season reset.
4. Confirm no real funds are required.
5. Run one silent smoke test.
6. Close diagnostic panels that a normal player would not see.

## Consent and Recording

Read this before beginning:

> Thank you for helping us test an early prototype. We are testing the product, not you. Participation is voluntary, you may stop at any time, and there are no right answers. We may ask you to think aloud, but you do not need to reveal personal or financial information. Please do not use a personal wallet or share a seed phrase. The session takes about 12 minutes.

Ask each participant separately:

- `[ ]` I agree to participate in this product test.
- `[ ]` I agree to screen and audio recording for internal research.
- `[ ]` I agree that anonymized quotations may be used in the hackathon submission.

Recording and quotation permission are optional and separate from participation. If recording is declined, take written observations only. Store consent records separately from anonymized findings. Define the retention/deletion date before the first session: `[date]`.

## Roles

### Player A — Envoy

Your job is to represent the civilization in a dispute with Tala. You can make a binding commitment, but you cannot perform the physical repair.

### Player B — Maker

Your job is to respond to the world you inherit and decide how to repair or redirect the aqueduct. You should not be told what Player A chose.

### Facilitator

Read the script exactly, keep time, and avoid teaching the interface. If a player asks what to click, first reply:

> What would you try if I were not here?

If the player remains blocked, give the smallest possible neutral prompt and record the assistance level.

### Observer

Record behavior, timing, exact quotes, assistance, and discrepancies between expected and actual state. Do not interpret aloud during play.

One person may facilitate and observe, but should not take notes so aggressively that the participant feels evaluated.

## Assistance Codes

- **A0 — None:** Standard facilitator script only.
- **A1 — Neutral prompt:** “What would you try?” or rereading the task.
- **A2 — Directional help:** Telling the player where a control or relevant fact is.
- **A3 — Rescue:** Taking control, explaining the answer, resetting, or manually repairing state.

A session requiring A3 does not count as unaided completion. Record the reason rather than hiding the session.

## Twelve-Minute Facilitator Script

### 0:00–0:45 — Consent and Think-Aloud Instruction

Read the consent language above, collect choices, then say:

> Please say what you are looking at, what you think it means, and what you expect to happen before you act. I may stay quiet or answer a question with another question because we want to see what the prototype communicates on its own. Player A and Player B, please do not tell each other what you chose until I invite you to discuss it.

Start the timer after consent.

### 0:45–1:15 — Minimal World Context

Read only this:

> You are both citizens of Aster, the same civilization—not opponents. At sunset, 4,800 citizens lose water. The River Guild has closed the East Sluice because Aster never repaid an old grain debt. Act as you would.

Do not mention the intended branch, prize design, AI architecture, or blockchain.

### 1:15–4:15 — Player A: The Envoy’s Mandate

Say:

> Player A, begin from the screen in front of you. Please think aloud. Decide what commitment, if any, your civilization should make to Tala. Stop when the game tells you that your mandate has resolved.

Observe whether Player A can:

1. identify the civilization crisis;
2. understand the Envoy role;
3. recognize that the commitment affects the shared civilization;
4. make one of the two commitments;
5. identify the confirmation or Chronicle event;
6. explain what they expect another citizen to encounter.

When resolved, record the exact branch and structured event. Say:

> Please stop here. Do not explain your decision to Player B.

### 4:15–7:45 — Player B: The Maker’s Inherited World

Say:

> Player B, begin from the screen in front of you. Player A has finished, but will not tell you what happened. Please think aloud and respond to the world you find. Stop when your mandate resolves.

Observe whether Player B can:

1. understand the current civilization state;
2. notice Tala’s changed memory or attitude;
3. notice the branch-specific mandate or repair route;
4. infer that another citizen caused the change;
5. complete a repair decision;
6. distinguish personal action from civilization outcome.

Do not reveal Player A’s choice if Player B fails to notice it. Record the failure.

### 7:45–8:45 — Live Chronicle and Next Handoff

Say:

> Both players may now look at the Live Chronicle and updated shared state. Please remain silent for 30 seconds. Separately, write one sentence explaining what caused the current result and what another citizen could do next.

Observe whether the Chronicle shows, in order:

1. Player A’s commitment;
2. Tala’s response;
3. Player B’s newly available or modified mandate;
4. Player B’s action;
5. the Water/Cohesion Worksite resolution;
6. the branch-specific next Mandates.

The Season Purse must read `ACCUMULATING · SETTLEMENT NOT STARTED` in every path. No local branch may display an available claim. Observe whether both players understand that Worksite 31 has resolved while Season Zero continues.

### 8:45–12:00 — Individual Questions, Then Debrief

Ask Player B first while Player A remains silent. Then ask Player A. Do not correct either answer until all individual questions are complete.

Use the exact questions in the next section. End with:

> You can now compare what you experienced. What is the first thing you would change about this game?

Thank participants, explain the purpose of the A→B test, and remind them how to request deletion of their recording if applicable.

## Post-Test Questions

Record answers verbatim where possible.

### Ask Player B First

1. What do you believe happened before you arrived?
2. What specific clue led you to that conclusion?
3. Did another player change any option available to you? If so, what changed?
4. Why did Tala behave the way she did?
5. What did your own action change for the civilization?
6. Which later citizen or Mandate did your action enable?
7. What would you want to do next?

### Ask Player A

1. What commitment did you make, and what did you expect it to change?
2. Who or what did you believe would be affected?
3. Did the result match your expectation?
4. Did your choice feel consequential? Why or why not?
5. Which later citizen or Mandate did your action enable?
6. What would you want to do next?

### Ask Both Separately

1. In one sentence, what is this game?
2. Are you competing against each other, cooperating as a small party, or living in one larger civilization? Explain.
3. Did Ivo end Season Zero, or did the shared season continue? What evidence led you to that answer?
4. Which parts, if any, should be impossible for the game operator or AI to rewrite afterward?
5. What value, if any, does a blockchain add here?
6. What was the most confusing moment?
7. What was the most memorable moment?
8. If one element had to be removed, what would you remove?
9. Would you take one of these actions now? Record behavior, not just stated intent:
   - select one of the generated next Mandates;
   - reserve a place in the next test;
   - invite one relevant friend;
   - none of these.

Do not ask “Was it fun?” until the behavioral question is answered.

## Session Observation Sheet

### Session Metadata

| Field | Entry |
|---|---|
| Session ID | |
| Date / time / timezone | |
| Build / commit / release | |
| Scenario seed | |
| Facilitator | |
| Observer | |
| Player A profile | |
| Player B profile | |
| Crypto experience: A / B | |
| Relevant game experience: A / B | |
| Recording consent: A / B | |
| Quote consent: A / B | |
| Model fallback used? When? | |
| RPC or transaction issue? | |

### Timing and Completion

| Measure | Player A | Player B | Notes |
|---|---:|---:|---|
| Time to understand current goal | | | |
| Time to first meaningful choice | | | |
| Time to mandate resolution | | | |
| Highest assistance code | | | |
| Completed required path? | Yes / No | Yes / No | |

### Causal Integrity

| Check | Expected | Observed | Pass? |
|---|---|---|---|
| Player A branch | One of two defined commitments | | |
| Structured event | Matches selected commitment | | |
| Tala’s Player B memory | References correct commitment | | |
| Player B mandate | Correct branch-specific route | | |
| Civilization change | Matches Player B action | | |
| Chronicle order | A → Tala → B mandate → B action → Worksite resolution → next Mandates | | |
| Season continuity | Season remains ACTIVE after Ivo | | |
| Treasury state | ACCUMULATING; settlement NOT STARTED; no claim | | |

### Timestamped Observations

| Time | Player | Observed action or exact quote | Interpretation to verify later | Severity |
|---|---|---|---|---|
| | | | | |
| | | | | |
| | | | | |
| | | | | |

Severity:

- **Blocker:** Cannot continue or state becomes incorrect.
- **Major:** Completes only with A2/A3 help or misunderstands the core premise.
- **Minor:** Friction that does not change comprehension or completion.
- **Observation:** Preference or idea, not yet a problem.

### Individual Comprehension Coding

Code only after the session. Retain the underlying quote.

| Measure | Player A | Player B | Coding rule |
|---|---|---|---|
| One-civilization comprehension | Pass / Fail | Pass / Fail | Says all season participants share one civilization; “our two-person team” alone is not sufficient. |
| A→B causal recognition | N/A | Pass / Fail | Names Player A’s specific commitment and the option/state it changed. |
| Personal vs civilization state | Pass / Fail | Pass / Fail | Distinguishes their action from the shared outcome. |
| Continuous-world comprehension | Pass / Fail | Pass / Fail | States that Ivo resolved one Worksite and did not end Season Zero. |
| Solana necessity comprehension | Pass / Fail | Pass / Fail | Identifies canonical history, neutral settlement, or shared treasury—not merely “crypto payments.” |
| Observable continuation | Yes / No | Yes / No | Actually starts, books, or invites; verbal praise alone is No. |

## Metrics and Calculation

Keep raw counts beside every percentage. With five sessions, report “4/5 sessions (80%),” not only “80%.”

### Core Metrics

**Session completion rate**

`sessions where both mandates and the Worksite resolution completed within 12 minutes ÷ all started sessions`

Report a second unaided rate excluding sessions where either player required A2 or A3.

**One-civilization comprehension**

`participants who independently describe one shared civilization ÷ participants asked`

**A→B causal recognition**

`Player B participants who name A’s specific action and its effect ÷ Player B participants asked`

**Causal integrity rate**

`sessions where selected branch, stored event, NPC memory, B mandate, Worksite resolution, Chronicle update, and next Mandates all match ÷ completed sessions`

**Continuous-world comprehension**

`participants who state that Worksite 31 resolved while Season Zero remained active ÷ participants asked`

**Time to first meaningful choice**

Report the median separately for Player A and Player B. A meaningful choice changes a commitment, mandate, resource, or outcome; wallet connection and “continue” do not count.

**Facilitator dependency**

Report count and percentage of participants at A0, A1, A2, and A3. Do not average the codes.

**Observable continuation rate**

`participants who actually select a generated next Mandate, book another session, or invite a relevant person ÷ participants offered the actions`

Report each action separately to avoid double counting.

### Qualitative Evidence

For every highlighted quote, retain:

- anonymous participant ID;
- timestamp;
- question that prompted it;
- recording/quotation consent;
- whether the quote was spontaneous or prompted.

Include negative and contradictory quotations in the research summary.

## Pass, Iterate, and Pivot Gates

Evaluate after at least five complete two-player sessions unless a safety or integrity failure requires an immediate stop.

### Gate A — Concept Comprehension

**Pass:** At least 8 of 10 participants independently understand that everyone shares one civilization.  
**Iterate:** 6–7 of 10 understand. Rewrite opening copy and civilization-status hierarchy, then retest.  
**Pivot presentation:** 5 or fewer understand. Stop adding systems; redesign the first 30 seconds before continuing.

### Gate B — Cross-Player “Aha”

**Pass:** At least 4 of 5 Player B participants identify Player A’s specific commitment and the changed option/state.  
**Iterate:** 3 of 5 identify it. Strengthen Tala’s reference, the branch-specific mandate, and Chronicle causality.  
**Pivot interaction:** 2 or fewer identify it. The NPC conversation is functioning as flavor, not shared-world gameplay; redesign the handoff.

### Gate C — Continuous World

**Pass:** At least 8 of 10 participants state that Ivo resolved one Worksite while Season Zero remained active, and at least 6 of 10 can name one generated next Mandate.  
**Iterate:** 6–7 of 10 understand continuity. Strengthen `SEASON ACTIVE`, the Live Chronicle heading, and the next-Mandate handoff, then retest.  
**Stop-ship:** 5 or fewer understand, or any participant is offered a claim after Ivo. Do not record the final demo while the two-player chain still reads as a season ending.

### Gate D — Usability

**Pass:** At least 4 of 5 sessions finish within 12 minutes; median first meaningful choice is under 2 minutes; no blocker appears in the final three sessions.  
**Iterate:** Sessions finish but require A2 help or exceed time. Remove onboarding text and optional controls.  
**Pivot scope:** Fewer than 3 of 5 finish. Reduce the test to one crisis, one NPC, one A choice, and one B action while preserving A→B causality.

### Gate E — State Integrity

**Pass:** 100% of completed sessions show the correct branch, stored event, NPC memory, B mandate, Worksite resolution, Chronicle update, next Mandates, and active-purse state.  
**Stop-ship:** Any incorrect branch, duplicated Worksite resolution, incorrect next-Mandate set, early settlement, or reachable claim. Do not record the final demo until corrected and rerun.

### Gate F — Desire to Continue

**Pass:** In at least 3 of 5 sessions, one or both players selects a generated next Mandate, books another test, or invites a relevant participant.  
**Iterate:** Interest is verbal but no action occurs. Increase the unresolved dramatic question and make the next mandate immediately available.  
**Pivot motivation:** Fewer than 2 sessions produce an action. Revisit the player fantasy and stakes before adding content.

### Gate G — Solana Legibility

**Pass:** At least 6 of 10 participants identify canonical shared history, operator-resistant active-season state, or eventual neutral treasury settlement as the blockchain value.  
**Iterate:** Participants mention only payments or speculation. Show the active-state Chronicle proof now and the separate eventual-settlement boundary without implying that Worksite 31 triggers it.  
**Do not solve with jargon:** Adding terms such as PDA, consensus, or composability to onboarding does not count as improvement.

### Gate H — Demo Reliability

After user testing, run ten seeded internal demos.

**Pass:** 9 of 10 finish without intervention.  
**Iterate:** 7–8 finish. Enable deterministic model fallback and remove nonessential network calls.  
**Pivot demo scope:** 6 or fewer finish. Cut the marketplace scene or secondary visuals before cutting the A→B causal chain.

## Immediate Decision Rules

Stop the session and label it failed if:

- the stored event does not match Player A’s choice;
- Player B receives the wrong branch;
- an old participant’s data appears;
- a real-money transaction is requested;
- the same Worksite resolution can execute twice;
- any Worksite path starts season settlement or exposes a claim;
- recording continues after consent is withdrawn;
- a participant is asked for a seed phrase or sensitive personal data.

## After Each Session

Within 15 minutes:

1. Save the raw notes under the anonymous session ID.
2. Confirm consent labels before retaining recordings or quotes.
3. Record the build and branch.
4. Calculate no aggregate metric until the raw row is complete.
5. Add issues to the iteration log as observation, hypothesis, proposed change, and owner.
6. Do not change the build between Player A and Player B.
7. If the next session uses a changed build, assign a new build number.

## Round Summary Template

**Round:** `[number]`  
**Dates:** `[range]`  
**Build(s):** `[ids]`  
**Sessions started / completed:** `[n] / [n]`  
**Participants:** `[n]`  
**Relevant-player mix:** `[description]`

### Results

- Session completion: `[raw / total] ([%])`
- Unaided completion: `[raw / total] ([%])`
- One-civilization comprehension: `[raw / total] ([%])`
- A→B causal recognition: `[raw / total] ([%])`
- Causal integrity: `[raw / total] ([%])`
- Median time to first meaningful choice, A / B: `[time] / [time]`
- Continuous-world comprehension: `[raw / total] ([%])`
- Observable continuation: `[raw / total] ([%])`
- Blockers / major / minor issues: `[n] / [n] / [n]`

### Strongest Supporting Evidence

- `[behavior, quote, or recording timestamp]`

### Strongest Contradicting Evidence

- `[behavior, quote, or recording timestamp]`

### Decision

- `[ ]` Pass and preserve
- `[ ]` Iterate and retest
- `[ ]` Pivot interaction or scope
- `[ ]` Stop-ship integrity issue

**What changes:** `[one to three items]`  
**What remains unchanged:** `[core hypothesis]`  
**Owner:** `[name]`  
**Retest date:** `[date]`
