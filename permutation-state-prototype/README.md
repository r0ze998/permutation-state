# Wylls — Living Civilization Prototype

> The primary game has been rebuilt at **[`civilization/`](civilization/)**. See the [current project README](../README.md). The older world/repair and proof instructions below are archived experiments, not the new game's design or completion status.

> **One civilization. Thousands of citizens. One continuously evolving world.**

## Current game

Open `/civilization/`, not the archived routes below. See the [current project README](../README.md)
for startup, implementation status and limitations, or the [Japanese player guide](../PLAY_GUIDE.ja.md)
for controls. The new game uses one shared civilization with exploration, construction, production,
logistics and research. Economy and shared-project extensions are being integrated.

## Archived playable-world experiment

The previous walking-and-repair experiment used:

```text
http://127.0.0.1:4173/world/?session=aster-living-alpha&actor=mara
```

It is a real-time shared simulation, not a choice-card sequence. The player controls a visible citizen, travels to resource sites, performs timed work, carries goods, supplies the civic warehouse, and constructs the East Sluice. AI citizens now gather into their own inventories, walk cargo to its destination, and participate in the same economy. Repairing the sluice propagates through irrigation, farm yield, food reserves, and market prices. Open the same session with a different `actor` value to verify shared movement and state.

The browser route is explicitly labelled `OFFCHAIN SIMULATION ALPHA`. A separate signer-authorized eight-citizen World PDA and MagicBlock checkpoint path is implemented and locally verified, but wallet/session-key synchronization between that compact canonical model and this richer simulation is still pending.

The original Living Civic Atlas and `proof/` routes below remain as a causal-receipt and MagicBlock proof lab. They are supporting evidence, not the main game.

This dependency-free browser prototype presents Wylls as a world-first civilization RPG. The close-up East Sluice district, its citizens, resources, worksites, Chronicle, and Season Purse remain part of one continuous world while the player moves between civic actions.

The playable scenario is **Season Zero: The Water Debt**. Mara Venn's decision at the River Guild changes Ivo Sen's available repair routes at the East Sluice. Ivo then resolves that local Worksite, leaving new resource values, obligations, faction memory, and history for the rest of the civilization.

The East Sluice is one Worksite inside an active season. Resolving it is **not a game ending, season ending, civilization victory, or payout trigger**.

## Living Civic Atlas

The prototype is organized around one persistent, inhabited district rather than a sequence of standalone dashboard screens:

- The East Sluice environment remains the primary interaction surface, with the wider Aster atlas available as context.
- Water, Food, Cohesion, Timber, and Prosperity appear in a compact civic HUD rather than dominating the world.
- Selectable place markers represent nearby districts, societies, markets, and the active Worksite.
- A parchment story journal supplies NPC interaction, deterministic choices, and Cause Receipts while leaving the district visible behind it.
- The bottom journal dock opens World, Stories, People, Memory, and the Season Purse.
- Tala's memory, the open or sealed Service Stair, and the Worksite result are reflected visually in the scene as well as in text.
- The MagicBlock experience uses the same world-first art direction; detailed receipts move into an optional World Chronicle drawer.
- Mara's action persists when control passes to Ivo; a new citizen inherits the same world rather than entering a reset scene.
- The Chronicle records local canonical events while the season continues.

The core proof remains the Handoff:

```text
Mara makes a civic promise
→ Tala changes access to the East Sluice
→ Ivo inherits different repair routes
→ one local Worksite resolves
→ Aster continues from the changed state
```

## Run the archived proof demo

The archived chain-proof demo uses the companion MagicBlock gateway. Follow [`../permutation-state-solana-receipt-spike/README.md`](../permutation-state-solana-receipt-spike/README.md), then open:

```text
http://127.0.0.1:4173/proof/?proof=1&session=aster-demo&role=observer&transport=magicblock
```

This mode creates one shared Worksite on the local Solana base layer, delegates it to the local Ephemeral Rollup, and lets Mara, Ivo, and Nia change the same account from separate role URLs. Those URLs are presentation and turn-taking aids, not player authentication; the gateway holds all three disposable demo signers. Ivo's local resolution requests a base-layer checkpoint; the UI labels the ER transaction and verified checkpoint separately. The implemented path is a finite three-action vertical slice, not the complete ongoing season loop.

For the browser-local fallback, open `index.html` directly or serve this directory without the gateway:

```bash
python3 -m http.server 4173
```

Then visit `http://localhost:4173`.

Then open:

```text
http://localhost:4173/proof/?proof=1&session=aster-demo&role=observer
```

The Observer view links to three role-locked URLs for Mara, Ivo, and the next citizen. Keep them open in separate tabs to watch accepted events propagate. Omitting `transport=magicblock` intentionally uses the browser-local proof and sends no transaction.

The prototype stores the local world state in browser local storage. Use **Reset** to restore Aster to the start of the East Sluice crisis.

## Controls

- **Map nodes:** select Aster Central, the River Guild, the East Sluice, Northern Terraces, or Glass Harbor to inspect that location.
- **World:** return to the shared map and civilization pulse.
- **Mandates:** open the current citizen's actionable duty.
- **Citizens:** inspect the active citizen and the cross-citizen Handoff.
- **Chronicle:** inspect the live record of accepted actions and Worksite resolutions. It is not the final season chronicle.
- **Purse:** inspect the active mock Season Purse and settlement status.
- **Compact civic HUD:** inspect the civilization's persistent state and current Prosperity Path progress.
- **Close (`×`):** dismiss the context drawer without leaving or resetting the map.
- **Reset:** in browser-local mode, clear this browser's proof state. In MagicBlock mode, start a fresh session ID; the prior Worksite and receipts remain intact.
- **Keyboard:** use `Tab` / `Shift+Tab` to move through controls and `Enter` or `Space` to activate focused buttons.

## Recommended 90-Second Route

1. Begin on the Living Civic Atlas and point out the persistent resources, multiple world nodes, and active Season Zero state.
2. Select **East Sluice** and open Mara's Envoy Mandate.
3. Meet Tala and choose **Bind the Grain Oath**.
4. Show the changed map, active civic obligation, and Handoff from Mara to Ivo.
5. Continue as **Ivo Sen, Maker**. Show that Tala remembers Mara's promise and that the Service Stair exists because of it.
6. Choose **Repair through the Service Stair**.
7. Show local resolution **R31-1**, its Cause Receipt, and the changed shared resources.
8. Open the live Chronicle to show that the event has entered Aster's continuing history.
9. Open the Purse last: it remains **active**, and season settlement has **not started**.

The alternate Founding Charter path is also playable. `Charter → Cut Through` produces local resolution **R31-3**: the city restores water but damages cohesion and loses River Guild cooperation. This is a serious local consequence, not an alternate game ending.

## Local Worksite Resolutions

The four deterministic branches are identified as East Sluice Worksite resolutions, not endings:

| Resolution | Civic path | Local result |
|---|---|---|
| **R31-1** | Grain Oath → Service Stair | River Guild engineers work beside Ivo; Water and Cohesion rise; the Food obligation persists into later play. |
| **R31-2** | Grain Oath → Old Millrace | Water returns with fewer materials, but the River Guild remembers being bypassed despite Mara's promise. |
| **R31-3** | Founding Charter → Cut Through | Water returns without River Guild help; Cohesion falls and the social rupture remains active. |
| **R31-4** | Founding Charter → Reconcile | Ivo repairs the relationship, accepts a smaller Food obligation, and reopens the Service Stair. |

These four codes are test vectors for this single hackathon Worksite—not a closed list of authored outcomes for the full game. A live season contains many Worksites whose combined consequences keep generating new situations.

Each resolution:

- closes only Worksite 31 at the East Sluice;
- changes persistent civilization resources and social state;
- creates a local entry in the live Chronicle;
- advances or damages the Prosperity Path;
- leaves Season Zero active for later citizens and Worksites;
- does not unlock, lock, calculate, or distribute the Season Purse.

## Season and Purse Status

This prototype depicts an active season, not its settlement screen.

- **Season Zero:** active
- **Current phase:** Epoch 3 of 4
- **East Sluice Worksite:** unresolved at start; locally resolved after Ivo's action
- **Prosperity Path:** active and incomplete
- **Season Purse:** active, denominated in `MOCK USDC`
- **Settlement:** not started
- **Claims:** unavailable until the full season reaches a published terminal condition and settlement is executed

The Purse panel may show simulated entry and marketplace allocations, but no East Sluice branch pays a citizen. A local Worksite can improve or harm the civilization's prospects without deciding the whole season.

## What Is Functional

- A persistent map-first application shell with selectable world nodes.
- Always-visible civilization resources and season progress.
- Two citizen identities sharing one local civilization state.
- Mara's accepted action changing Ivo's Mandate, NPC memory, routes, costs, and available resolutions.
- Four deterministic East Sluice Worksite resolutions: `R31-1` through `R31-4`.
- A live Chronicle that records the causal chain without declaring the season complete.
- An active Purse view whose settlement remains not started.
- Browser-local persistence, reset, keyboard-operable buttons, and reduced-motion support.
- A separate multi-client Handoff Proof in `proof/`: URL-locked Mara, Ivo, successor, and read-only Observer views share an append-only local event log across tabs.
- SHA-256 event chaining, deterministic replay, branch and order enforcement, cross-tab head agreement, generated-Mandate acceptance, fail-closed integrity checks, and replayable `session-proof.json` export.
- An optional real local MagicBlock transport: base-layer Worksite initialization, Delegation Program CPI, ER actions, continued shared play, and a root-verified base-layer checkpoint after Ivo.
- Separate ER and base-layer receipts, public Program ID and Worksite PDA, sequence/root invariants, 1.25-second polling, and fresh-session reset semantics.

## What Is Simulated or Planned

- No player wallet is connected. The local gateway holds three disposable demo signers; production wallet approval and Session Keys are future work.
- The default MagicBlock path sends real transactions only to the isolated local stack. Nothing is deployed to public devnet or mainnet.
- The full prose memory and browser evidence log remain offchain; the deterministic Worksite branch, resources, role progression, resolution, Mandate IDs/status, roots, and event head are stored in the delegated account.
- Currency is `MOCK USDC`; no real funds move and no claim is executed.
- Tala's lines are authored to demonstrate the intended AI boundary. A future model may vary expression, but not rules, Worksite effects, eligibility, or value movement.
- Forty active citizens, other Worksites, the remainder of Epoch 3, and the final season settlement are scenario assumptions outside this vertical slice.
- The browser-local fallback uses same-origin `localStorage` plus `BroadcastChannel`. The MagicBlock path uses a shared local validator account plus polling/BroadcastChannel, but still does not prove remote hosting, separate wallets, public Solana consensus, or production security.

The UI must continue to label these boundaries. Do not remove `SIMULATED WORLD`, `MOCK USDC`, localnet, disposable-signer, or not-started disclosures until genuine integrations replace them.

## Presenter Preview Routes

These query parameters open deterministic states for review, recording, and screenshots:

- `?preview=scene-a` — Mara's River Guild decision
- `?preview=after-a-oath` — the map immediately after Mara binds the Grain Oath
- `?preview=scene-b-oath` — Ivo inherits Mara's promise and altered routes
- `?preview=resolved-r31-1` — the cooperative Service Stair resolution
- `?preview=resolved-r31-3` — the coercive Cut Through resolution
- `?preview=chronicle-r31-1` — the live Chronicle after R31-1
- `?preview=vault` — active Season Purse; settlement not started

Append the parameter to the local URL, for example:

```text
http://localhost:4173/?preview=resolved-r31-1
```

Preview routes do not replace the uninterrupted live demonstration.

Append `&judge=1` to any preview route for the 1280 × 720 presentation layout. It keeps the causal affordance ribbon, both choices, the primary action, result deltas, all three next Mandates, and active-season status inside the compact review drawer. Example:

```text
http://localhost:4173/?preview=scene-b-oath&judge=1
```

## System Boundary

| Layer | Responsible for | Must never do |
|---|---|---|
| AI | Character voice, bounded memory expression, contextual stakes, candidate intent | Decide Worksite effects, season victory, eligibility, settlement, or directly mutate canonical state |
| Deterministic resolver | Validate actions, apply bounded deltas, assign R31 resolution IDs, and update published civilization state | Invent narrative facts or move funds outside published rules |
| Solana + MagicBlock — local slice | Enforce accepted Worksite outcomes on the ER and checkpoint important state roots to the local base layer | Treat arbitrary model prose as verified truth or settle after a local Worksite alone |

## Included Files

- `index.html` — semantic map-first application shell
- `styles.css` — Living Civic Atlas visual system and responsive states
- `app.js` — local shared-world state machine, four Worksite resolutions, Chronicle, previews, and Purse state
- `assets/aster-east-sluice-v1.png` — current world-first East Sluice environment
- `assets/tala-pixel-v1.png` — current Riverkeeper dialogue portrait
- `assets/aster-map-v2.png` — wider Aster atlas used inside almanac and proof context
- `assets/aster-map.png` — legacy first-direction Aster map
- `assets/tala-river-guild.png` — legacy painterly Tala portrait retained for provenance
- `ART_ASSET_PROMPTS.md` — built-in ImageGen mode and exact visual prompts
- `proof/` — local multi-citizen Handoff Proof with four URL-locked clients, deterministic reducer, SHA-256 evidence chain, Observer rail, and JSON export

This is a hackathon validation artifact, not a production wallet, finished seasonal game, or real-money application.

The companion [`../permutation-state-solana-receipt-spike/`](../permutation-state-solana-receipt-spike/README.md) contains the native-Rust program, transport, gateway, reproducible local runner, and exact public-devnet limitations. The local integration is verified; public devnet remains a separate proof gate.
