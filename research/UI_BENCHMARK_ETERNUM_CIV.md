# PERMUTATION STATE: UI/UX benchmark (Eternum, Civilization VI/VII, Old World, Humankind, Polytopia)

**Scope and method.**
- **Eternum:** I read the client source statically at `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/work/eternum`, revision `510c4b9` (2026-09-20). All Eternum paths below are relative to `apps/game/src/` unless marked otherwise. Nothing was run and no files were changed.
- **Civ and the others:** web research. gameuidatabase.com and the official 2K 1.1.0 page returned 403, and civilization.fandom returned 402. Where I relied on search-result snippets, I say so.
- **Unverified items:**
  - I found no sourced pixel measurements for the Civ HUD. Civ placements described as conventional come from general knowledge.
  - Two guides disagree on where Old World's Orders counter sits: a scroll on the leader portrait at bottom-left (https://ladiesgamers.com/old-world-tips-guide-1/, snippet only) versus the top resource bar (https://www.thegamer.com/old-world-how-to-buy-get-more-orders-legitimacy/).

**What still applies from the earlier notes.**
- **Still holds** (`ETERNUM_UI_BENCHMARK.md`, `MAP_FIRST_UI_DIRECTION.md`):
  - The map fills the screen and the HUD sits at its edges.
  - Selecting something on the map drives which panel appears, and only one panel is open at a time.
  - The player sees the predicted change before committing.
  - A durable, causal Chronicle, where each entry says what caused what.
  - Lens buttons carry text labels.
  - The "consequence triad": after a change, the map, the meter and the Chronicle agree.
  - Minimum 44 px click targets, and colour is never the only signal.
- **No longer holds:**
  - Everything built around "one shared civilization".
  - "Cut the minimap / fog / countdown": the tick timer is now a real deadline, and the map is multi-civ and larger than one screen.
  - The 90-second scripted demo route.
  - The persona-by-persona Mandate screens (`PERMUTATION_STATE_UI_SPEC.md`).

---

## A. Screen inventory and information architecture

### A1. Eternum: measured layout

| Zone | Placement / size | Source |
|---|---|---|
| Header | 44 px row, plus a 12 px gutter; columns start at `top-[60px]` | `ui/features/world/containers/hud-layout.ts:1-3` |
| Left command column | 320 px wide (360 px at ≥1800 px screens); section height `clamp(180px, 30vh, 320px)` | `hud-layout.ts:1-4` |
| Right column | 288 px (312 px at ≥1800 px); stacks the quick feed, tile details, and log/chat, with only one of those focused at a time | `ui/features/world/containers/right-hud-column.tsx:10-30` |
| Header contents | Identity/rank chip, Local/World switch, game clock, Attention pill, settings. No global resource bar. | `ui/features/world/containers/top-header/top-header.tsx`, `identity-chip.tsx:128` (`#rank`) |
| Resources | Shown for the active structure only, not empire-wide | `ui/features/world/containers/left-facets/empire-cockpit.tsx`, `merged-resource-panel.tsx` |
| Minimap | Bottom-right; drawn from real SVG hexes, with semantic markers and the camera footprint | `ui/features/world/components/bottom-right-panel/hex-minimap.tsx` |
| Map interaction | Map layer is non-interactive except for the HUD elements that opt in | `ui/layouts/world.tsx:38` |
| Compact / mobile | One tab bar and one open sheet; tapping the map closes the sheet | `ui/features/world/containers/compact-hud.tsx:103,328` |

### A2. Civilization conventions

- **Civ VI:**
  - The World Tracker at top-left holds current research and civic, with a toggle for the unit list (https://forums.civfanatics.com/threads/world-tracker-hide-finally.611656/).
  - The Lenses button is attached to the minimap panel (https://www.civilopedia.net/en-US/standard-rules/concepts/world_7/).
  - World Rankings sits top-right.
  - By convention (unverified): the yields ribbon with per-turn deltas is top-left, the unit/city panel bottom-left, and the notification stack runs down the right edge above the end-turn button.
- **Civ VII's launch UI shows what goes wrong when things are hidden:**
  - Cities list, units list and existing trade routes were not visible (https://forums.civfanatics.com/threads/your-beef-with-the-ui-is-mainly-based-on.695458/).
  - The UI scaled only for 1080p or 4K (https://steamcommunity.com/app/1295660/discussions/0/591762405949780093/).
  - Patches then added:
    - an Age icon on the turn counter, a UI-scale setting and a Victory button in the HUD (1.4.0: https://civilization.2k.com/civ-vii/game-update-notes/2026-may-19-patch-1-4-0/);
    - rounded yields and an "Always Show Ribbon Yields" option (1.2.0: https://civilization.2k.com/civ-vii/game-update-notes/2025-apr-22-patch-1-2-0/).

### A3. Recommended information architecture for PERMUTATION STATE

**Always visible, at 1440×900:**
1. **Top bar (48 px).**
   - Left: civ identity, with a HUMAN or AGENT badge.
   - Centre: global yields with per-tick deltas — gold, science, influence/concord, and military upkeep — at most 5.
   - Right: season, tick, the tick timer, and the Victory mini-tracker (three track pips).
2. **Order dock (bottom-centre, ~560×72 px).** Order budget pips, bank, pending-order chips, Commit button, and the ready count.
3. **Right column (~300 px).** Attention/notification stack at the top, and the context panel for whatever is selected below it.
4. **Minimap (bottom-right, about 200×160 px)** with the lens rail attached above it (following Civ VI's convention).

**On demand, one panel at a time:** City, Army, Civ overview, Victory, Diplomacy, Market, Exchange, Chronicle, Tech/Star Gate.

**Scope rule:** global numbers live in the top bar, and local numbers live only inside the panel for the selected object. This follows Eternum's split between the header and the active-structure cockpit, adapted to a multi-city civ.

---

## B. The per-tick decision loop

### B1. What the sources show

- **Civ VI's end-turn button is a to-do list.**
  - Its icon changes to show the next thing that needs doing: unit orders, research, production.
  - Clicking it jumps the camera to that item. Shift+Enter forces the turn to end.
  - Auto-end-turn and auto unit cycling are both options.
  - Complaint: units with a leftover fraction of movement still demand a manual skip.
  - Sources: https://steamcommunity.com/app/289070/discussions/0/340412122418074543, https://steamcommunity.com/app/289070/discussions/0/2788173147748944042/
- **Civ VI's multiplayer timer.**
  - "Dynamic" scales with empire size, reported as roughly 30 s + 10 s × cities + 5 s × units (formula from snippet: https://steamcommunity.com/sharedfiles/filedetails/?id=1180364645).
  - Competitive players replace it with mods (https://steamcommunity.com/sharedfiles/filedetails/?id=2994111596).
  - Civ VII 1.1.0 shows "waiting on you" only to the last human still to finish (https://forums.civfanatics.com/threads/civilization-vii-update-1-1-0-march-4-2025.696380/).
- **Old World's Orders are the closest match to our model.**
  - One shared per-turn pool pays for every command.
  - Unspent Orders turn into gold and normally cannot be saved; the "Elite" law allows storing up to 100.
  - Hovering the "+N" shows where the income comes from.
  - Designer's reasoning (Soren Johnson): "If you can move every unit every turn, that's just what you're going to do". A budget turns chores into choices (https://mohawkgames.com/oldworld/gameplay/, https://forums.civfanatics.com/threads/gdc-2022-soren-johnson-my-elephant-in-the-room-an-old-world-postmortem.681293/).
  - Warning: late-game budgets reach 130–150 Orders per turn, so the scarcity erodes (https://steamcommunity.com/app/597180/discussions/0/3426689579754213696/). Our cap of 8 avoids this. Keep it.
  - The Turn Summary is a categorised, iconed recap at the start of each turn (https://mohawkgames.com/2023/05/10/old-world-update-108/).
- **Humankind shows the failure mode of simultaneous turns.** Start-of-turn popups ate human time while the AI acted instantly, so the AI "moves first ~99% of the time" (https://community.amplitude-studios.com/amplitude-studios/humankind/forums/168-general/threads/44258-simultaneous-move-is-bad-in-its-current-state).
- **Eternum:**
  - The **Attention pill** walks through a combined list: structures under attack, arrivals, and ranked suggestions. Each click centres the map and opens the matching suggestion (`ui/features/world/containers/top-header/attention-pill.tsx:23-84`, `attention-policy.ts:22-26`).
  - Suggestions carry `label`, `reason`, `priority` and `emphasis` (`left-facets/blitz-suggestions.ts:64-74`).
  - The clock applies a border to the whole viewport at 300 s (warning), 120 s (critical) and 30 s (final) (`top-header/game-clock.tsx:32`).
  - A pending transaction is marked "stuck" after 30 s (`hooks/store/use-transaction-store.ts:25`).
  - A single `canIssueOrders` gate hides every order control for spectators and after game over (`utils/can-issue-orders.ts`).

### B2. How this maps to tick + order budget

Civ's per-unit orders become per-civ order slots. Civ's end-turn blockers become non-blocking "idle" advisories, because the deadline cannot wait for the player. Eternum's Attention cycle becomes the "Next decision" key.

### B3. Exact HUD elements

1. **Tick timer** (top-right, next to the season).
   - Shows `TICK 142 · 00:47` plus a thin bar draining over the tick.
   - Urgency thresholds are proportional, not Eternum's absolute ones: 25% of the tick remaining turns amber, 10% turns red, the last 5 s adds a pulse.
   - Always show text alongside the colour.
   - Under the timer: `Resolves at 14:02:30 UTC` (tooltip).
2. **Order budget meter** (order dock, left side).
   - Eight segmented pips. Pips up to `3 + cities` are "this tick"; pips above that are greyed and labelled `cap 8`.
   - States: filled = queued, outlined = free, hatched = funded from the bank.
   - Next to it: `BANK 4` with a hover breakdown, `+1 next tick if unspent`, and the bank cap if the rules have one.
   - Hover shows the formula line by line: `Base 3 · Cities +4 · Cap 8 → 7 this tick`. This follows Old World's "+N" breakdown.
3. **Pending orders queue** (order dock, centre).
   - One chip per order: icon, verb, target, predicted cost/effect, and a ✕ to cancel.
   - Clicking a chip focuses the map on the target.
   - Show conflicts before commit: an army ordered twice, or an unaffordable order.
   - Allow drag-reorder only if resolution order is meaningful. If it isn't, say "Order within a tick does not matter".
4. **Commit state** (order dock, right side), as a four-state machine with persistent text:
   - `DRAFT (3/7 used)` → `COMMITTED ✓ (editable until lock)` → `LOCKED · resolving` → `RESOLVED · see summary`.
   - Unspent orders at the deadline auto-commit and bank the remainder, stated in text. Never lose orders silently.
   - Show `Ready: 5/9 civs` for everyone, but only flash "waiting on you" when you are the last human (Civ VII rule).
   - If submissions go on-chain, reuse Eternum's 30 s "stuck" state: `Submitted — not yet confirmed (retry)`.
5. **Next Decision key** (Space/Tab), following Eternum's Attention cycle.
   - Steps through idle cities, idle armies, expiring proposals, then ranked suggestions, each with a one-line *reason*.
   - It never blocks the tick. It only helps spend orders well.
6. **Start-of-tick summary**, non-modal.
   - Old World-style categorised list — combat, diplomacy, market, victory, cities — docked in the right column.
   - It must not steal time on a timed tick; this is Humankind's lesson.

---

## C. Map readability for pixel hexes

- **Semantic zoom.** Eternum has an explicit per-zoom-level table:
  - Near: models + full labels.
  - Mid: models + priority-only labels + army tier glyphs.
  - Far: atlas icons only, with no text and no effects.
  - Every renderer reads this one table (`three/scenes/worldmap-content-ladder.ts:5-60`).
  - For pixel art, use two or three integer zoom levels (1×/2×/3×) so sprites stay crisp. This is general pixel-art practice, not taken from a source. Map each level to a row of the same table.
- **Ownership and borders.**
  - Eternum resolves every entity into `mine | ally | enemy | agent | neutral` (`three/utils/labels/entity-label-view-model.ts:7, 93-97`) and gives each enemy player a unique colour profile (`three/utils/labels/label-config.ts:162-205`).
  - For us: a 1–2 px border drawn in the civ colour along the outer hex edge, plus a civ-specific edge *pattern* (solid, dash, dot) so colour-blind players can tell civs apart.
  - Allies get a double line. City-state suzerainty shows as the suzerain's colour on an inner ring.
- **Fog.**
  - Civ VI's parchment fog made explored and unexplored areas hard to tell apart (https://steamcommunity.com/app/289070/discussions/0/365172547942765243/).
  - Use three clearly different states:
    - **Unexplored:** flat dark field with the hex grid only.
    - **Remembered:** desaturated terrain, plus a "last seen T-12" stamp on units and cities.
    - **Visible:** full colour.
- **Lenses.**
  - Civ VI has 10 lenses. The key pattern is *automatic* lens switching: District Placement turns on when you place a district, Religion when you select a religious unit (https://www.civilopedia.net/en-US/standard-rules/concepts/world_7/).
  - Civ VII added green/red up-down yield arrows when placing a building (https://civilization.2k.com/civ-vii/game-update-notes/2025-jun-17-patch-1-2-5/ via https://civilization.2k.com/civ-vii/game-guide/gameplay/developing-settlements/). It also added a Trade lens and a zone-of-control display (1.1.1: https://civilization.2k.com/civ-vii/game-update-notes/2025-mar-25-patch-1-1-1/; 1.4.0).
  - Proposed lenses:
    - Yields (tile icons at most 2 per hex).
    - Political (borders, pacts, wars as hatched fronts).
    - Concord (city-state influence rings).
    - Military (zone of control, threat range).
    - Market routes.
    - Agent intents (post-resolution only; see E).
- **Unit stacks.** Civ VI allows one unit per tile, with Corps and Army formations (https://civilization.fandom.com/wiki/Unit_(Civ6), snippet). If stacks are allowed, draw one sprite with a numeric badge and a strength pip, then expand the stack in the panel. This matches Eternum's rule that labels show troop count and stamina (`three/managers/army-label-content.ts:10-20`).
- **City banners.**
  - Content: name, population, HP bar, current build with ticks remaining, owner badge.
  - Add *timed threats*, as Eternum's structure labels do with battle cooldown and incoming troop arrivals (`three/managers/structure-label-state.ts`).
  - Civ VII had to make settlement health bars and unit flags bolder after launch (1.1.1).
- **Planned-move preview.** Eternum colours route highlights by action type (`three/managers/highlight-hex-manager.ts:22,49`). Queued orders should draw ghost arrows on the map that are clearly marked "pending", distinct from resolved paths.

---

## D. Panels

- **City panel.**
  - Yields, each with a "why" tooltip breakdown. Civ VI's amenities tooltip lists sources line by line (https://civ6.fandom.com/wiki/Amenities, snippet). Old World lets you hover any yield for its sources, and its tooltips nest (https://forums.civfanatics.com/threads/my-old-world-experience-6-hours-in-initial-pros-and-cons.676942/).
  - Costs follow Eternum's `held / needed` requirement chips: red while short, with "+N in transit, ETA" (`ui/design-system/molecules/requirement-chips.tsx:10-40`).
  - Blocked options stay visible with the reason (`aria-disabled` pattern in `ui/features/settlement/construction/plot-construction-picker.tsx`).
  - Actions are labelled "Queue order (1 slot)".
- **Army panel.**
  - Eternum's army detail: owner avatar, status pill, troop chip, and a stamina bar whose tooltip says exactly why travel is blocked, e.g. "missing 40 wheat and 10 fish" (`ui/features/world/components/entities/banner/army-banner-entity-detail.tsx:100-200`, `components/armies/army-warning-copy.ts`).
  - Combat preview is a 280 px card: losses and remaining troops per side, plus an explicit statement of the assumptions ("Preview assumes +X%… each side rolls a d20") (`ui/features/military/battle/quick-attack-preview.tsx:556-605`).
  - Combine that with Civ VI's victory/stalemate/defeat label and modifier list, and Old World's skull-on-kill marker (update #108).
  - With simultaneous resolution, say what the preview does *not* know: "Enemy may also move this tick."
- **Civ overview.**
  - A cities list and armies list with idle flags. Their absence was the top Civ VII launch complaint (https://forums.civfanatics.com/threads/your-beef-with-the-ui-is-mainly-based-on.695458/).
  - Plus yields over time and the order-usage history.
- **Victory.**
  - Civ VI has World Rankings tabs per victory. Its Science tab shows rivals' space-race stages, and it spawned a "Better World Rankings" mod (https://forums.civfanatics.com/threads/better-world-rankings-ui.659741/).
  - Civ VII 1.4.0 added a Summary tab with every player's points and rank, plus per-path graphs over time. It also added crisis pips and "turns until" on the age wheel (1.1.1). Its "track progress" checkbox unticks itself after each milestone, which is a bug (https://forums.civfanatics.com/threads/is-the-track-progress-checkbox-in-the-legacy-paths-supposed-to-stay-ticked.695627/).
  - Our layout:
    - **Summary** tab: every civ × 3 tracks.
    - **Dominion:** capitals/regions held against the threshold.
    - **Star Gate:** 3 stages × rivals, like Civ VI's space race, with the ETA in ticks.
    - **Concord:** peace-tick streaks and suzerainties.
    - A pinned tracker in the top bar that never unticks.
- **Diplomacy.**
  - Civ VI's access levels unlock the *reasons* behind an AI's opinion (±X modifiers) and its hidden agendas (https://civilization.fandom.com/wiki/Diplomatic_Visibility_and_Gossip_(Civ6), snippet).
  - Civ VII: Support/Accept/Reject costs Influence, plus relationship tooltips with reasons and action cooldown timers (https://civilization.2k.com/civ-vii/en-GB/game-guide/dev-diary/diplomacy-influence-trade; 1.2.0).
  - Ours: a proposals inbox with **expiry in ticks** and the order-slot cost.
  - NAP card:
    - Bond amount escrowed by each party.
    - The exact break condition, and who gets the slashed bond.
    - Duration.
    - Current breach risk.
  - The relationship strip lists every modifier.
- **Markets.**
  - **AMM:** Eternum's swap lists price, slippage, owner fee % and LP fee %, and its confirm dialog shows `-X → +Y` (`ui/features/economy/banking/swap.tsx:250-345`).
    - Gaps to fix: slippage is always red regardless of size, and there's no "minimum received" or max-slippage setting.
    - Add all three, with warnings at 1% / 5% price impact.
  - **Exchange (USDC):** reuse Eternum's "Open orders N · Locked X" summary (`ui/features/economy/trading/trade-summary-bar.tsx`).
    - Show per-player caps as meters: "Traded this season $120 / $500".
    - Fees as explicit lines.
    - Show expiry visibly. Eternum hides it in a hover title (`trading/market-order-panel.tsx:412`).
    - Give real money its own visual style: different accent colour, a "USDC · real value" label, and two-step confirmation.
- **Chronicle and notifications.** Eternum routes every notice through one function (`ui/features/event-feed/notify.ts`) and uses three levels:
  - Headline banner: 8 s, with a **View** button that jumps to the location (`ui/features/news-headlines/headline-types.ts:20`, `news-headline-bridge.tsx:178-198`).
  - Quick feed: at most 5 rows for 20 s (`event-feed/quick-feed.tsx:20-21`).
  - Filtered log: All / Mine / Combat. The "important" filter merges duplicate battle events and drops routine production (`event-feed/important-feed-rows.ts:27-60`).
  - Every Chronicle row gets a tick number and a "view on map" link.

---

## E. Agents vs humans

- **Labels.** Eternum makes "agent" its own ownership class, with its own icon and label style on the map (`three/utils/labels/entity-label-view-model.ts:47,97`; `label-config.ts:150`). Carry a HUMAN or AGENT badge everywhere: banners, diplomacy portraits, rankings, market counterparties, Chronicle rows.
- **Same limits for both.** Show the same order-budget meter on every civ's overview card, so players can see agents get no extra orders.
- **Committed rationale.**
  - Eternum's exploration strategy returns `{path, reason}` and stores the reason as `lastAction` (`automation/exploration/types.ts:21-24`; `hooks/use-exploration-automation-runner.ts:349`). But the compact UI that would show it is commented out (`army-banner-entity-detail.tsx`, block before `ArmyBannerEntityDetailContent.displayName`), so the rationale is never shown to the player.
  - Proposal: an agent commits a rationale hash along with its orders and reveals the text after the tick resolves. Surface it as:
    1. an "Intent" line on each of the agent's Chronicle entries;
    2. an **Agent Intents lens**, showing the last tick's orders as arrows with reasons;
    3. a diff: "Said: defend Varo · Did: moved 2 armies to Varo ✓".
  - Never show rationale before the tick resolves: it would leak orders to humans mid-tick.
- **Standing orders and governors.** Eternum's pattern is presets (Smart / Custom / Idle) with one-line descriptions (`utils/automation-presets.ts:22-30`). Each run reports Success / Failed / Skipped with a message, a "stale" flag after 3 missed intervals, and warning/critical severity by consecutive failures (`utils/automation-status.ts`). For humans:
  - Per-city governors ("Grow / Produce / Defend") that draft orders into the queue. They are not auto-committed unless the player opts in.
  - Every automation failure appears in the start-of-tick summary with its reason.
- **Owner console (for people who deploy agents).** Eternum's hired-agent design gives owners:
  - short `report_to_owner` status;
  - *directions* dropped in as `.md` files;
  - pause / resume / stop controls;
  - a per-run manifest of actions attempted and confirmed, failure classes and cost (`/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/work/eternum/docs/plans/hired-agents-architecture-brief.md:175-236,360-376`; `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/work/eternum/apps/agent-runner/README.md`).

  Mirror this as an "Agent" tab: Directions (text box), Reports feed, Ledger (orders by tick, rejected orders and why), Pause.

---

## F1. 25 prioritised recommendations

**P0 (build first):**
1. **Order dock** with 8 segmented pips, the `3+cities` formula on hover, and the bank count. [Old World Orders: https://mohawkgames.com/oldworld/gameplay/; thegamer breakdown]
2. **Four-state commit machine** (Draft / Committed / Locked / Resolved) shown as text. Unspent orders auto-bank at the deadline. [Civ VII 1.1.0 waiting logic; Eternum `can-issue-orders.ts`]
3. **Pending-order chips** that can be cancelled, focus the map when clicked, and show predicted effect and conflict flags. [Eternum `requirement-chips.tsx`; `ETERNUM_UI_BENCHMARK.md` consequence preview]
4. **Tick timer with proportional urgency** at 25/10/5% remaining, plus a UTC resolve time. [Eternum `game-clock.tsx:32`]
5. **No modal can interrupt a timed tick.** The start-of-tick summary is docked. [Humankind thread; Old World update #108]
6. **Next Decision key** that cycles idle cities and armies, expiring proposals, and suggestions with reasons. [Eternum `attention-pill.tsx`, `blitz-suggestions.ts`; Civ VI end-turn button]
7. **Every number explains itself on hover** with a source breakdown. Nested tooltips come in P1. [Civ VI amenities; Old World; Civ VII launch criticism: https://aftermath.site/civilization-vii-review/]
8. **HUMAN/AGENT badge** on every civ surface and map label. [Eternum `entity-label-view-model.ts:7`]
9. **Ownership borders with colour plus pattern**, and three distinct fog states with "last seen" stamps. [Eternum `label-config.ts:162`; Civ VI parchment-fog complaint]
10. **Victory Summary tab** (all civs × 3 tracks) plus a pinned top-bar tracker. [Civ VII 1.4.0; Civ VI World Rankings]
11. **Market confirmation** showing `-X → +Y`, fee lines, price impact and minimum received. USDC gets its own style and a two-step confirmation. [Eternum `swap.tsx:250-345`]
12. **One notification path, three levels** (headline 8 s with View, quick feed 5 rows × 20 s, filtered Chronicle). [Eternum `notify.ts`, `quick-feed.tsx:20`, `headline-types.ts:20`]
13. **Cities list and armies list** with idle flags in the Civ overview. [Civ VII launch complaints: CivFanatics 695458]

**P1:**

14. **Semantic zoom table** (2–3 pixel-integer levels) that every renderer reads. [Eternum `worldmap-content-ladder.ts`]
15. **Lenses that switch on automatically** in context (placing a city → yields lens; selecting an army → military lens). [Civ VI Civilopedia lenses]
16. **Diplomacy proposal cards** with expiry, bond escrow, breach terms and modifier reasons. [Civ VI access levels; Civ VII cooldowns in 1.2.0]
17. **Combat preview** stating its assumptions and the simultaneous-move caveat. [Eternum `quick-attack-preview.tsx:601-605`; Civ VI preview mod gap]
18. **Committed-rationale reveal after resolution**, with an Agent Intents lens and a "said vs did" diff. [Eternum `exploration/types.ts` reason, whose UI was never shipped]
19. **Governor presets** (Grow/Produce/Defend/Idle) that draft orders into the queue, with success/failed/skipped status. [Eternum `automation-presets.ts`, `automation-status.ts`]
20. **Star Gate stage tracker** showing rivals' stage and ETA in ticks. [Civ VI space race]
21. **Keyboard map:**
    - From Eternum: Tab/Shift-Tab cycle armies/cities, V switches view, Esc closes, WASD pans, Enter opens chat.
    - Add: Space = next decision, Ctrl+Enter = commit.
    - Sources: `three/scenes/worldmap.tsx:1728-1768`, `three/renderer-interaction-runtime.ts:127-132`, `ui/features/world/containers/chat-shortcut.ts`.
    - The settings screen lists every shortcut.

**P2:**

22. **UI-scale setting and pixel-font minimums** (see anti-pattern 7). [Civ VII 1.4.0 UI scale; 1440p complaint]
23. **Onboarding** built from contextual "Take me there / Tell me more" advisor cards, with the encyclopedia secondary. [Civ VII 1.4.0; Polytopia legibility: https://www.pixelatedplaygrounds.com/sidequests/game-design-perspective-the-battle-of-polytopia]
24. **Exchange cap meters and a locked-funds summary.** [Eternum `trade-summary-bar.tsx`]
25. **Agent owner console:** directions, reports, ledger, pause. [Eternum hired-agents brief]

## F2. Ten anti-patterns to avoid

1. **Blocking popups during simultaneous turns.** Humankind's AI moved first "~99%" of the time (Amplitude forum thread above).
2. **A budget that grows until it stops being scarce.** Old World late-game hits 130–150 Orders per turn (Steam thread above). Keep the hard cap.
3. **Hiding vital lists and costs.** Civ VII launch: no cities/units lists, no tech costs (CivFanatics 695458). Civ VII launch tooltips were largely missing (Aftermath).
4. **Fog that looks like explored land.** Civ VI parchment (Steam thread 365172547942765243).
5. **Chores forced by the end-turn button.** Civ VI's manual skip for leftover movement fractions, and its late-game "clickspam" (https://steamcommunity.com/app/289070/discussions/0/282992562607266710/).
6. **Notification spam.** Civ VII 1.2.0 had to cut town-specialisation notifications; Civ VI players ask to mute research-complete notices.
7. **Tiny HUD type.** Eternum's HUD tokens run 10–12 px (`ui/design-system/atoms/hud-typography.ts`). Pixel fonts need at least about 12 px for labels and 14 px for body text, rendered at integer scale.
8. **Important terms only in hover titles.** Eternum puts order expiry in a `title=` tooltip (`market-order-panel.tsx:412`), and tooltips are disabled on touch devices (`ui/design-system/molecules/tooltip.tsx`, `isCoarsePointer`).
9. **Colour-only signals.** Eternum's swap shows slippage always in red, whatever its size (`swap.tsx:315`). Automation status relies on emerald/amber/red classes (`automation-status.ts`).
10. **Encyclopedia-first onboarding and self-resetting trackers.** Eternum's 1100 px "Lordpedia" modal (`ui/features/progression/hints/hint-modal.tsx:95`). Old World's "walls of text" (https://steamcommunity.com/app/597180/discussions/0/4337600100422070858/). Civ VII's progress checkbox that unticks itself.
