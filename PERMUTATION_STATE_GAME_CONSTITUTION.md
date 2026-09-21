# PERMUTATION STATE

## Game Constitution — Hackathon Edition / Season Zero

**Status:** Historical design document. The fixed Handoff/East Sluice gameplay specification below is superseded by [the map-first rebuild](PERMUTATION_STATE_REBUILD.md). Shared civilization and economic safety principles remain relevant; the new playable route is `/civilization/`.  
**Version:** 0.2  
**Date:** 2026-09-21

This document defines what PERMUTATION STATE is, what the hackathon build must prove, and what the project must refuse to become. If a feature, pitch, economic mechanism, or implementation choice conflicts with this constitution, the constitution wins unless it is explicitly amended.

---

## The One-Line Pitch

**PERMUTATION STATE is a seasonal generative civilization RPG where every player is a citizen of the same civilization, each action changes another player's possible future, AI animates the society, and Solana settles its shared history and Season Purse.**

Short form:

> **One civilization. Thousands of citizens. One verifiable history.**

The product promise in plain language:

> You log in to inherit the consequences of choices made by strangers. You make one consequential decision of your own, then leave a changed world for the next citizen. At the end of the season, every action resolves into one civilization's victory, failure, treasury, and history.

---

## 1. The Player Fantasy

The player is not a god looking down on a map and not a sovereign who owns a private kingdom. The player is one named citizen inside a civilization shared by everyone in the season.

The intended emotional progression is:

1. **Belonging:** “This crisis belongs to all of us.”
2. **Agency:** “I can change one meaningful part of it.”
3. **Dependence:** “My action was possible because another citizen acted before me.”
4. **Responsibility:** “Someone I do not know will inherit what I decide now.”
5. **Remembrance:** “The world, its NPCs, and the living chronicle remember what I did.”

The signature player story is not “I earned 40 contribution points.” It is:

> “A citizen I never met promised grain to the River Guild. I had to decide whether to honor that promise when I rebuilt the aqueduct. Because I did, another group arrived in time to save the capital.”

If players remember meters rather than people, promises, sacrifices, and consequences, the design has failed.

---

## 2. Design Pillars

### Pillar 1 — One Civilization, One Fate

Every season has one human civilization and one civilization-scale result. Fronts, cities, worksites, roles, and philosophical factions may divide the work, but they do not create separate human teams with separate victory conditions.

The opposition comes from AI civilizations, hostile forces, disasters, scarcity, social fracture, and time—not from turning citizens into rival kingdoms.

### Pillar 2 — Personal Agency, Shared Consequence

Every meaningful action must change at least one of the following:

- an objective world state;
- an NPC or faction relationship;
- the options available to a later player.

An action that only adds anonymous points to a global meter is not enough.

### Pillar 3 — The Handoff, Not the Quest

The fundamental unit of play is a Handoff: one player inherits a consequence, resolves a role-specific problem, and creates a new consequence for somebody else.

Parallel solo quests that happen to fill the same bar are not shared civilization gameplay.

### Pillar 4 — Aligned Ends, Contested Means

Citizens share the goal of keeping their civilization alive, but they should disagree over how to do it. Scarcity must create defensible trade-offs between safety, prosperity, fairness, knowledge, loyalty, and survival.

There should rarely be a universally correct answer. There must always be an understandable answer.

### Pillar 5 — Generated Society, Fixed Rules

AI gives characters voice, memory, initiative, and social response. Authored rules define what actions are possible, how state changes, how victory is determined, and how value moves.

AI may generate expression and candidate intent. It may not improvise the rules of the contest.

### Pillar 6 — Legible Causality

Players must be able to answer:

- What enabled me?
- What did I change?
- What did it cost?
- Who can act differently because of me?
- How did this contribute to the civilization's outcome?

The system must explain important changes with “because,” not merely display new numbers.

### Pillar 7 — History Is a Reward

The season outcome is collective, but remembrance is personal. At each season boundary, the living chronicle records a civilization-scale result and each citizen's contribution; the world then persists into the next season.

PERMUTATION STATE recognizes citizens through named history, not a single global leaderboard.

### Pillar 8 — Stakes Without Pay-to-Win

The Season Purse gives collective decisions weight. It must never allow spending to purchase combat power, political authority, contribution credit, or a larger share of the prize.

The game must remain worth playing when the purse is hidden.

---

## 3. The Atomic Unit: A Handoff

A valid Handoff has six required parts:

1. **Inherited consequence** — a world fact, civic oath, discovery, shortage, or unfinished public work created by prior play;
2. **Named dependency** — a person, faction, location, or project that needs the outcome;
3. **Role-specific skill** — negotiation, deduction, construction, routing, defense, or another form of player judgment;
4. **Real trade-off** — at least two values cannot both be maximized;
5. **Verified result** — the deterministic resolver produces a bounded and explainable state change;
6. **Downstream opportunity** — the result opens, alters, or closes a later player's options.

In compact form:

```text
HANDOFF = inherited consequence
        + skilled choice
        + irreversible trade-off
        + verified state change
        + named downstream opportunity
```

Every completed Handoff produces a Cause Receipt:

```text
ENABLED BY   Who or what made this action possible
CHANGED      The objective and social state changes
COST         The resources, trust, time, or option sacrificed
ENABLES      The next named players, roles, or projects
REACTION     The NPC, faction, or enemy response
```

Example:

```text
Mara discovered a clean underground spring
→ Ilya negotiated access with the River Guild
→ Ren built the eastern channel
→ Sora kept the supply road open
→ the capital survived the drought
```

If a proposed activity cannot produce a Cause Receipt, it should not be a core Mandate.

---

## 4. The Three Loops

### The 30-Second Loop — Observe, Act, Witness

Within approximately 30 seconds, the player should:

1. observe an urgent fact or inherited consequence;
2. make or advance one meaningful choice;
3. witness an immediate response from a person, system, or future option.

This does not require a civilization-scale state change every 30 seconds. It requires visible causality. Feedback should arrive in four layers whenever appropriate:

- **Immediate:** animation, sound, tactical result, or resource change;
- **Social:** a named NPC, citizen, or faction reacts;
- **Systemic:** a later option, Mandate, or dependency changes;
- **Historical:** the event becomes eligible for the epoch summary or season chronicle.

### The 10-Minute Loop — One Mandate

**00:00–00:45 — Civilization Pulse**  
Read one headline: what happened, what is about to be lost, and which citizens or projects created the current situation.

**00:45–01:30 — Select a Mandate**  
Choose among a small number of needs generated from the civilization's real bottlenecks. The choice is about who to help and what to risk, not merely easy versus hard.

**01:30–03:00 — Investigate**  
Question an NPC, inspect evidence, review a promise, or understand the public project's dependencies.

**03:00–07:30 — Perform the Role Action**  
Negotiate, solve, route, construct, explore, defend, or make another role-specific skilled decision.

**07:30–09:00 — Commit the Trade-Off**  
Confirm the cost, promise, sacrifice, route, or priority that makes the result consequential.

**09:00–10:00 — Aftermath and Handoff**  
Receive the Cause Receipt, see the civilization response, and leave a changed situation for a named downstream role or project.

### The Season Loop — One Shared Fate

The initial product hypothesis is a 7–14 day asynchronous season divided into four epochs:

1. **Founding** — establish the civilization's immediate priorities and learn the major factions;
2. **Pressure** — external threats and resource dependencies become visible;
3. **Fracture** — the civilization can no longer protect every value at once;
4. **Reckoning** — citizens attempt a war or prosperity victory, or determine what can be saved from defeat.

At each epoch boundary:

1. player actions close;
2. the deterministic resolver applies the published transition rules;
3. canonical outcomes are committed;
4. AI factions and NPCs choose bounded reactions;
5. a concise civilization summary is generated;
6. the next epoch's Mandates are composed from the new state.

At the season boundary:

- victory or failure is finalized;
- the Season Purse is settled according to the precommitted rules;
- a civilization chronicle is created;
- each Active Citizen receives a personal season record;
- only bounded historical elements—not economic power—may carry into the next season.

---

## 5. Roles and Collective Scale

Roles are civic functions, not permanent character classes. Players may change roles between epochs. Mastery unlocks more difficult responsibilities, not superior statistics.

### Warden

- **Sees:** threats, routes, fronts, and defensive windows;
- **Does:** escorts, intercepts, fortifies, withdraws, and chooses what must be protected;
- **Produces:** security and time;
- **Depends on:** Seeker intelligence, Maker logistics, and Envoy ceasefires.

### Maker

- **Sees:** infrastructure, capacity, resource dependencies, and failure points;
- **Does:** builds, repairs, reroutes, allocates, and decides what cannot be completed;
- **Produces:** infrastructure and productive capacity;
- **Depends on:** Seeker discoveries, Envoy permissions, and Warden protection.

### Envoy

- **Sees:** motives, trust, obligations, factions, and social red lines;
- **Does:** bargains, mediates, promises, persuades, and renegotiates;
- **Produces:** treaties, legitimacy, and social access;
- **Depends on:** the civilization's ability to deliver what is promised.

### Seeker

- **Sees:** unknowns, evidence, causal clues, and hidden alternatives;
- **Does:** explores, researches, deduces, reveals, and reframes;
- **Produces:** knowledge and new options;
- **Depends on:** other roles to turn discovery into civilization-scale effect.

The whole player population remains one civilization, but its work is divided into **Fronts** and **Worksites** such as a border defense, an aqueduct, a Great Archive, a famine response, or a diplomatic compact. These are not separate teams. Their outputs feed the same civilization state and must depend on one another.

Scaling rules:

- difficulty is based on recent Active Citizens, not registered wallets;
- epoch targets are locked at the epoch's start;
- large works decompose into bounded Handoffs;
- repeated work in an overfilled role has diminishing returns;
- understaffed roles receive visibility and in-game need bonuses, never cash multipliers;
- AI citizens maintain a minimum baseline when a role is absent;
- only human players may cross important decision gates;
- no Mandate should require a particular person to be online at the same time.

High-impact civic actions use a two-key rule: two different civic functions must confirm actions capable of causing irreversible civilization-scale harm.

---

## 6. System Boundary: AI, Resolver, and Solana

The product has three authorities with deliberately different powers.

### AI: The Social Actor and Storyteller

AI may:

- speak in character;
- retain and summarize bounded memories;
- form beliefs from known information;
- express emotion and change relationships;
- propose deals and candidate actions;
- select a bounded faction reaction;
- vary Mandate framing inside an authored grammar;
- narrate epoch summaries and personal season records.

Every important NPC has at minimum:

- wants;
- red lines;
- limited knowledge;
- beliefs, including potentially false beliefs;
- relationships;
- commitments;
- faction membership;
- a bounded action budget per epoch.

The interface distinguishes **verified fact**, **binding civic oath**, **NPC belief**, and **unverified rumor**.

AI may not:

- transfer funds;
- decide eligibility or payout shares;
- invent new victory criteria;
- apply an unbounded state change;
- silently rewrite canonical history;
- make an irreversible civic action without deterministic validation.

### Deterministic Resolver: The Rules Authority

The resolver owns:

- legal action types and preconditions;
- contract and Civic Oath validation;
- resource costs and state deltas;
- cooldowns, caps, and two-key requirements;
- role-normalized Contribution Receipts;
- Active Citizen qualification;
- victory and failure conditions;
- payout computation;
- the exact mapping from a player-confirmed action to a canonical result.

Natural-language agreements follow this flow:

```text
player language
→ AI proposes structured terms
→ player reviews and confirms
→ resolver validates feasibility
→ canonical commitment is recorded
→ state effects resolve under published rules
→ AI narrates the social response
```

### Solana: The Settlement and Canon Authority

Solana records only the outcomes for which neutral settlement matters:

- the season configuration and rule hash;
- entry and Season Purse balances;
- application-level marketplace fee flows;
- binding Civic Oaths when they affect public state;
- digests of verified Handoff outcomes;
- critical civilization state transitions;
- finalized victory or failure;
- each eligible citizen's claim right;
- the season-closing chronicle root.

Solana does not need to store:

- every movement;
- every line of NPC dialogue;
- every tactical animation;
- raw model prompts or chain-of-thought;
- transient interface state.

The off-chain game may be expressive and fast. The canonical result must remain independently verifiable.

---

## 7. Season Zero: “The Water Debt”

Season Zero is the active shared season. The hackathon build shows one compressed causal chain inside it: **Worksite 31 — East Sluice**, during Epoch 3. It is not a miniature MMO, but neither is it a two-player season. The demo stops after proving that one citizen changes another citizen's play and that the result creates later work in the same living world.

### Premise

The East Sluice is closed. At sunset, 4,800 citizens lose water. Twenty-seven years ago, Aster diverted the river and failed to deliver the grain it promised the River Guild. Tala controls access to the damaged channel and will speak to one Envoy.

### Initial State

```text
Water         28 / 100 — CRITICAL
Food          46 / 100 — STRAINED
Cohesion      54 / 100 — UNSTEADY
Timber        18
Food Debt     0
Tala Trust    0
Season Purse  348.50 MOCK USDC
Time          1 resolution window
```

### Worksite 31 Stabilization

Worksite 31 is stabilized if, after the Maker resolves the aqueduct:

- Water is at least 50; and
- Cohesion is at least 50.

If Water reaches 50 but Cohesion remains below 50, the physical water objective is secured while a civic fracture remains open. Both outcomes become inputs to later Mandates. Neither outcome wins or ends Season Zero, unlocks or locks the Season Purse, or creates an immediate claim. The local rule is public before either citizen acts, and AI cannot alter it.

### Principal NPC: Tala

Tala leads the River Guild.

- **Wants:** secure water access and survival for her people;
- **Red line:** the capital may not take the full flow while abandoning the river settlements;
- **Knowledge:** the aqueduct can be repaired, but she does not know which structural route is safe;
- **Memory:** she remembers civilization-level promises and the citizens who made or broke them;
- **Agency:** after a promise is made, she may grant access, withhold labor, warn her faction, or close the channel within deterministic limits.

### Handoff A — The Envoy's Promise

Player A enters as **Mara Venn, Envoy**, and negotiates with Tala. She confirms one of two structured Civic Acts:

1. **Bind the Grain Oath** — Water +8, Cohesion +6, Tala Trust +20, and a debt of 12 Food at Epoch close. Tala opens the service stair and lends her engineers.
2. **Invoke the Founding Charter** — Water +5, Cohesion -8, no new Food debt, and Tala Trust -20. Tala opens the outer gate under protest, locks the service stair, and withdraws the Riverkeepers.

The AI proposes the terms. Player A must see and confirm the exact structured consequence before the oath becomes canonical.

### Handoff B — The Maker's Choice

Player B enters as **Ivo Sen, Maker**, from a second citizen identity. In the chain-connected build, this identity will be a separate wallet. Tala's dialogue, the Mandate, the accessible route, costs, and possible local resolutions change according to Mara's accepted act.

After **Bind the Grain Oath**, Ivo may:

- **Repair through the Service Stair** — Timber -6, Water +24, Cohesion +4. Ivo works with Tala's engineers and stabilizes Worksite 31. The 12 Food debt persists.
- **Use the Old Millrace** — Timber -3, Water +18, Cohesion -4. Worksite 31 is stabilized, but the 12 Food debt persists and Tala remembers that her offered help was bypassed.

After **Invoke the Founding Charter**, Ivo may:

- **Cut Through the Millrace** — Timber -10, Water +22, Cohesion -5. Water returns, but Cohesion falls to 41; Riverkeeper fracture becomes a new civilization bottleneck.
- **Return the Charter** — Timber -6, Water +18, Cohesion +8, and a new debt of 8 Food. Ivo repairs the relationship as well as the channel and stabilizes Worksite 31 under new terms.

The Maker is not deciding which number is largest. The Maker is deciding whether to use, bypass, deepen, or repair the conditions inherited from another citizen.

### Local Resolution, Live Chronicle, and Next Mandates

The complete visible chain is:

```text
Player A's negotiation
→ Tala's conditional access
→ Player B's repair options
→ the River Guild's reaction
→ Worksite 31's deterministic resolution
→ an append-only Live Chronicle update
→ branch-specific Mandates for later citizens
```

The three required “wow” moments are:

1. switching wallets reveals that Tala remembers and acts on Player A's promise;
2. Player A's choice materially changes Player B's available game state;
3. the Cause Receipt becomes shared history and creates the next citizens' work while the same season remains active.

The four local resolution codes are:

- **R31-1 — Oath → Service Stair:** Worksite stabilized; 12 Food debt persists; Tala Trust remains positive.
- **R31-2 — Oath → Old Millrace:** Worksite stabilized; 12 Food debt persists; Tala remembers the bypass.
- **R31-3 — Charter → Cut Through:** Water restored; civic fracture remains open at Cohesion 41.
- **R31-4 — Charter → Reconcile:** Worksite stabilized; an 8 Food debt persists; Tala Trust partially recovers.

These four codes are a deterministic hackathon test matrix for one Worksite, not the world's complete set of story outcomes. The full game combines many Worksites, citizens, memories, and consequences without funneling them into four authored conclusions.

Each resolution generates visible downstream needs. Examples include preparing the promised grain, ratifying new river terms, investigating the flooded archive, surveying the old millrace, preventing a Riverkeeper walkout, or defending the eastern cistern. The prototype may stop after showing these Mandates; the fictional world does not stop.

### Hackathon Slice Roles and Content Limit

The hackathon slice implements:

- one civilization;
- one crisis;
- one primary NPC;
- two directly playable roles: Envoy and Maker;
- two citizen identities, designed to become two separate wallets;
- four meaningful Worksite 31 resolutions, including one that restores Water while opening a Cohesion crisis;
- one visible marketplace-fee allocation in the mock Season Purse ledger;
- one mock-USDC Season Purse shown as accumulating, with settlement not started and no local claim.

The wider season scale may be illustrated. The cross-player memory, deterministic consequence, Live Chronicle update, downstream Mandates, and active-season state transition may not be faked.

> **Scale may be mocked. The core may not.**

---

## 8. Economy Constitution and Guardrails

### Purpose

The economy exists to:

- create commitment to a shared seasonal outcome;
- make settlement visibly neutral;
- fund continued operation;
- let citizens exchange identity and historical expression.

It does not exist to promise income or replace the game loop.

### Season Zero Safety

The hackathon build uses devnet and mock USDC only. No participant risks real money. Production deployment with paid entry or monetary prizes requires jurisdiction, age, identity, contest, custody, tax, and consumer-protection review.

### Demonstration Parameters

The following numbers are concrete demo parameters, not immutable production promises:

```text
10 mock USDC entry
├─ 7 to the Season Purse
└─ 3 to the operations reserve

100 mock USDC peer-to-peer marketplace purchase
├─ 97.5 to the seller
├─ 1.5 to the Season Purse
└─ 1.0 to protocol operations
```

The normal Solana network fee is not game revenue and cannot be redirected into the Season Purse. Only an explicitly disclosed application fee may fund it.

During an active season, the purse may accumulate disclosed entry and application-fee allocations, but no local Worksite may unlock, lock, or settle it. Claims are created only after the published season outcome is finalized at the true season boundary.

### Distribution Principle

The product default should favor shared victory over individual competition:

- the large majority of an unlocked purse is divided equally among Active Citizens;
- a small minority may reflect capped, role-normalized, objectively verified contribution;
- no citizen may receive an outsized share through repetition, spending, or a dominant combat role;
- exact rules are published and committed before entry closes.

For the demo, an illustrative split is:

```text
80% equal among Active Citizens
20% capped, role-normalized verified contribution
maximum individual payout: 2× the eligible-citizen median
```

For a production season, an Active Citizen should normally have:

- completed meaningful Handoffs across at least two epochs;
- connected at least one action to another citizen or public work;
- no unresolved major oath breach.

The hackathon slice demonstrates one verified Handoff that consumes or creates a downstream dependency. Season Zero remains a continuous season, so production Active Citizen qualification still spans multiple Handoffs or epochs under rules published before entry closes.

### Absolute Economic Prohibitions

PERMUTATION STATE must not:

- issue a native token merely to create speculative demand;
- sell combat strength, civic authority, votes, contribution credit, or superior payout weight;
- count transaction volume or marketplace spending as contribution;
- allow self-trading or linked-wallet trading to create eligibility;
- require payment for ordinary movement, dialogue, or core Mandates;
- let AI decide who receives money;
- alter payout rules after entry;
- describe a player-funded purse as yield or guaranteed profit.

Marketplace goods should be cosmetic, expressive, commemorative, or creator-made: banners, portraits, chronicle covers, memorials, housing decoration, and other identity artifacts. They must not determine victory.

---

## 9. Onboarding, Participation, and Failure Protection

The first 90 seconds teach only three facts:

1. everyone is part of one civilization;
2. the civilization is about to lose something specific;
3. the new citizen can help a named person or project now.

The preferred onboarding sequence is:

```text
one crisis headline
→ one named mentor
→ one unfinished Handoff from a real or seeded citizen
→ one small but canonical result
→ one downstream opportunity left for somebody else
```

Players should be able to observe the current civilization and experience a short preview before paying an entry fee.

Late joiners receive:

- a 90-second history recap;
- a named mentor NPC;
- three catch-up Mandates drawn from current bottlenecks;
- eligibility requirements prorated to the remaining epochs;
- no retroactive voting power or contribution credit.

Free riders may share the narrative fact that their civilization won, but entry alone does not create payout eligibility, governance authority, or historical honors.

No individual may irreversibly destroy the civilization or drain its treasury. High-impact acts require bounded permissions, deterministic validation, and, where appropriate, two independent civic functions.

Defeat must remain playable. When normal victory is no longer possible, the season opens a Last Choice such as saving the people, the archive, the seed vault, or the banner. Defeat creates a named chronicle, a bounded Scar, and a meaningful premise for the next season; it does not erase all player authorship.

---

## 10. Hackathon Scope

### In Scope

- a polished 2D civilization interface;
- Civilization Pulse, Mandate, Scene, Aftermath, and Chronicle/Vault screens;
- two wallets sharing one civilization state;
- Tala's bounded cross-player memory;
- one free-form conversation mapped to a structured Civic Oath;
- one Maker decision whose options depend on that oath;
- one deterministic Worksite resolution inside an active season;
- one clickable Cause Receipt and causal chain;
- one mock marketplace fee split;
- one accumulating mock Season Purse view with settlement not started;
- one append-only Live Chronicle update, one citizen echo, and branch-specific next Mandates;
- recorded evidence from external playtesting.

### Explicitly Out of Scope

- a 3D open world;
- real-time action combat;
- multiple human civilizations or PvP;
- a complete war route;
- a full technology tree;
- hundreds of NPCs;
- unrestricted procedural world generation;
- a production marketplace;
- a native token;
- a DAO governance suite;
- real-money entry or mainnet prize settlement;
- full implementation of all four roles;
- persistent character power across seasons.

The five required player-facing surfaces are:

1. **Civilization Pulse** — map, crisis, active public works, recent causes;
2. **Mandate** — inherited need, beneficiary, trade-off, and expected scope;
3. **Scene** — NPC negotiation or role-specific skilled play;
4. **Aftermath** — Cause Receipt and downstream Handoff;
5. **Chronicle & Vault** — live civilization history, citizen echo, next Mandates, and active-purse state.

---

## 11. Success Metrics and Test Gates

The primary design hypothesis is:

> Players care about a shared civilization when another person's past action changes their present, and their own action visibly changes a future stranger's game.

The North Star Metric is:

> **Meaningful downstream consequences per Active Citizen** — verified results that are later consumed by another citizen, public work, NPC, or civilization resolution.

### Prototype Targets

- **Time to First Meaningful Action:** 90 seconds or less;
- **Causal Recall:** at least 70% of testers can correctly explain what enabled them and what they changed;
- **Handoff Completion:** at least 50% of created downstream opportunities are consumed in the test window;
- **NPC Memory Recognition:** at least 70% notice and correctly describe Tala's cross-player memory;
- **No-Prize Intent:** at least 60% say they would play another season with the purse hidden;
- **Collective Identity:** most testers describe the experience using another citizen, NPC, promise, or public work before mentioning rewards;
- **Rules Integrity:** 100% of critical Worksite state transitions match the published resolver rules; any separately demonstrated season-final claim must also reconcile exactly;
- **Comprehension:** a new observer can state “all players share one civilization” within 15 seconds of the pitch.

### Required Playtests

1. **Retell Test:** “What happened, and what did you cause?”  
   Fail if the answer is only about filling a meter.

2. **Stranger Test:** run the Handoff with participants who do not know each other.  
   Fail if coordination requires a friend group or live voice chat.

3. **No-Prize Test:** hide the Season Purse from one cohort.  
   Fail if interest depends primarily on payout visibility.

4. **Loss Test:** force a pyrrhic or failed resolution.  
   Fail if players feel their previous choices became meaningless.

5. **Rule-Explanation Test:** ask a tester why the local Worksite resolution occurred and what remains unresolved in the season.  
   Fail if the answer is “the AI decided.”

### Hackathon Demo Test

Within the first 20 seconds, the judge must understand that Player A's earlier promise changed Player B's current game. Within the full demo, the judge must see:

1. shared civilization state;
2. cross-player NPC memory;
3. deterministic consequence;
4. legible causal history;
5. transparent Season Purse accumulation with settlement explicitly not started.

If those five facts are clear, the demo is complete. Additional systems are optional.

---

## 12. Non-Negotiable Rules

1. **There is one human civilization and one civilization-scale fate per season.**
2. **The core unit is a Handoff, not a disconnected quest.**
3. **Every core action must consume context and create downstream consequence.**
4. **No player should feel like a 0.001% contribution to an anonymous bar.**
5. **Shared ends require contested means; cooperation must still contain sacrifice and disagreement.**
6. **AI generates social expression and bounded intent; deterministic systems authorize effects.**
7. **AI never controls funds, victory, eligibility, or payout.**
8. **Solana records commitments and consequential outcomes, not every footstep.**
9. **Every critical state change must be explainable through a visible causal chain.**
10. **Money may buy entry and expression, never power, voice, contribution, or a larger payout weight.**
11. **Ordinary play is not charged per action. Solana network fees are never presented as purse contributions.**
12. **No single citizen can irreversibly destroy the civilization or drain shared assets.**
13. **The game must work asynchronously and must not require an exact party composition.**
14. **Entry alone does not make an Active Citizen. Repetition and spending do not prove contribution.**
15. **Victory produces collective settlement; failure still produces authored history and a final choice.**
16. **A new citizen must become useful within 90 seconds.**
17. **The game must remain emotionally coherent when the Season Purse is hidden.**
18. **For the hackathon, one deep causal chain is worth more than a broad but shallow world.**

---

## Final Standard

PERMUTATION STATE succeeds when a player can say:

> “I inherited a promise from somebody I did not know. I chose what that promise meant. Someone else inherited my decision. Together, those choices became our civilization's history.”

That experience is the product. AI, Solana, the Season Purse, the chronicle, and the seasonal structure exist to make it possible, believable, and impossible for any single operator to rewrite after the fact.
