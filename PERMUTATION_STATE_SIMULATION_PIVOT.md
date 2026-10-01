# Wylls — Simulation Pivot

> Archived intermediate design. This walking-and-repair slice did not satisfy the required strategic civilization gameplay. The active design is [the map-first rebuild](PERMUTATION_STATE_REBUILD.md), playable at `/civilization/`. The completion language below describes only the old narrow technical experiment, not a completed game.

## Product correction

The existing East Sluice proof is a useful Solana/MagicBlock causality spike, but it is not the game. It demonstrates that one citizen's accepted action can alter a later citizen's valid action and that the resulting state can be checkpointed. Its static scene, three-step command chain, and receipt-led interface must not define the main play experience.

Wylls is now defined as:

> A persistent civilization RPG in which every human player and AI citizen inhabits the same civilization as an embodied character, physically moves through the world, works inside one shared economy, and changes the civilization's seasonal fate through production, logistics, trade, governance, exploration, and defence.

The target is not an Eternum reskin. Eternum supplies the systemic backbone—world time, spatial constraint, production chains, transport, construction, markets, armies, and seasonal pressure. Wylls adds an embodied citizen perspective and a simulated AI society.

## Non-negotiable player fantasy

The player is one citizen, not an omnipresent city cursor and not the owner of a private nation.

- The character is always visible in the world.
- The character must travel to a place before acting there.
- Goods exist at a location and must be carried or transported.
- Buildings consume inputs, take time to construct, and change future production.
- Other players and AI citizens occupy the same world and obey the same physical and economic constraints.
- The world continues while an individual player is offline.
- A season produces a result and prize settlement, not one of a fixed set of authored story endings.

## The simulation loop

```text
observe a shortage or opportunity
→ travel to the relevant place
→ gather, buy, craft, fight, negotiate, or accept a contract
→ physically move goods or people through the world
→ contribute to a workshop, market, defence, or public project
→ change production rates, prices, access, safety, and NPC behaviour
→ create a new shortage or opportunity for the shared civilization
```

The closed causal loop is more important than the number of resources:

```text
inputs → timed production → storage capacity → transport
       → construction / consumption / defence
       → changed capacity, rates, access, and risk
```

## Shared world model

### Space

The authoritative world records positions for players, AI citizens, resource nodes, buildings, goods in transit, threats, and construction sites. The map is the primary input surface. Contextual actions appear because the character is close enough to perform them.

### Time

The world has one authoritative clock. Movement, work, production, consumption, construction, resource regeneration, weather, needs, and market prices resolve from elapsed time. A player click issues an intent; it does not instantly materialize an outcome.

### Economy

Resources have origin, location, quantity, weight, inputs, outputs, and destination. The initial slice uses food, timber, stone, ore, and tools. Shared stores do not rise because a dialogue choice adds a fixed delta; they rise because somebody gathered and delivered an item or a building completed a production cycle.

### Construction

Public works progress through material delivery and labour. Their visual state changes from damaged to under construction to operational. Completion changes the simulation—for example, the repaired East Sluice changes water production per second, which affects farms, food supply, prices, NPC morale, and later projects.

### Society

AI citizens are simulated actors, not dialogue skins. Each has a location, home, workplace, inventory, money, needs, relationships, job, schedule, and bounded memory. Deterministic systems execute movement, production, trade, and needs. A language model may interpret memories, choose among legal high-level intents, and express dialogue; it may never invent an item, bypass distance, or mutate canonical state directly.

### Cooperation and competition

All human players belong to one civilization. They cooperate to keep it alive and complete its seasonal ambition, while competing through trade, contracts, discovery, reputation, office, and verifiable contribution. Other civilizations may exist as AI-governed external societies, so diplomacy and war remain possible without turning each player into a separate country.

## Season semantics

There are no four authored endings. There is an ongoing history divided into seasons because entry fees, marketplace fees, victory evaluation, and prize settlement need a bounded accounting period.

A season may emphasize prosperity, discovery, diplomacy, or defence, but the outcome emerges from the simulation. At the deadline the chain records the civilization state, objective status, contribution commitments, chronicle root, and settlement root. The next season inherits selected monuments, names, relationships, discoveries, and scars.

## Hackathon vertical slice

The first playable slice is one East Sluice district, one accelerated civic day, two to eight human citizens, and four to eight AI citizens.

Required loop:

1. A player clicks or uses movement keys and the on-map character travels to the forest.
2. A timed work action gathers timber into a capacity-limited personal inventory.
3. The player physically returns to the civic warehouse and deposits it.
4. A second player or AI citizen delivers stone.
5. Players travel to the damaged East Sluice and contribute labour.
6. The structure visibly changes to operational.
7. Water production begins over time without another click.
8. Farm output, food reserves, market prices, and NPC behaviour react to the new water supply.
9. Two browser windows see the same movement, inventories, construction, and rates.

The slice is complete only when this causal chain is playable end to end. Conversation, Chronicle prose, prize UI, and receipts are secondary surfaces.

## UI hierarchy

1. **World map:** movement, other actors, goods, buildings, construction, weather, and danger.
2. **Context action bar:** Move, Work, Gather, Deposit, Build, Transfer, Trade, Defend, Talk.
3. **Compact simulation HUD:** world clock, inventory, current job, civilization vitals, production rates, and seasonal objective.
4. **Secondary panels:** market, contracts, people, governance, Chronicle, and chain receipts.

The previous proof UI remains available as a judge/debug console. It is not the default game route.

## Solana and MagicBlock boundary

The chain should verify the game, not replace visual animation.

- MagicBlock's real-time execution layer is appropriate for shared authoritative intents and rapidly changing game state such as actor position, inventory transfer, construction contribution, and combat result.
- Solana base-layer checkpoints preserve economically important state: asset ownership, market settlement, major public works, season purse accounting, final outcome, and claims.
- Client-only interpolation, particles, camera movement, and animation are never presented as canonical chain state.
- During the transition, any gateway-only simulation is labelled `OFFCHAIN SIMULATION ALPHA`; the existing onchain proof may not be used to imply that the new world model is already onchain.

## Migration rule

Reuse:

- Aster's art direction and East Sluice environment
- the season purse ledger
- MagicBlock lifecycle and receipt infrastructure
- deterministic action validation and state-root discipline
- Tala and the named-citizen memory concept

Replace:

- the fixed Mara → Ivo → Nia turn chain
- choice cards as the primary verb
- the static scene as the primary game surface
- instantaneous fixed resource deltas
- role-specific URLs as the model of multiplayer

Build in this order:

```text
world clock → spatial entities → visible movement → inventory and work
→ resource production → physical logistics → construction
→ shared synchronization → AI citizen schedules and needs
→ markets / conflict → seasonal checkpoint and payout
```

## Acceptance gates

- A new player understands how to move within ten seconds.
- The central character visibly moves when the player acts.
- No remote gathering, depositing, building, trading, or talking.
- At least one useful resource changes without a click because time is passing.
- At least one building changes a production rate after physical inputs and labour.
- At least one AI citizen travels, works, consumes, and adapts to a shortage.
- A second browser sees the first player's movement and economic effects.
- Reloading does not reset the shared world.
- The default screen is recognizably a game world before any explanatory copy is read.

## Implemented vertical-slice status

The current local build now clears the playable simulation gates for the East Sluice slice:

- the map is the primary screen and the player's walking character is always visible;
- movement, gathering, depositing, building, crafting, trading, and conversation are proximity-gated;
- gathering and construction are timed jobs with capacity-limited inventories;
- two browsers read and mutate one persistent authoritative gateway world;
- AI workers gather into their own inventories, travel with cargo, and deposit at the warehouse or market;
- Tala physically collects repair materials before travelling to the sluice;
- four repair stages consume exactly eight timber and four stone;
- completion changes water production from `0.70` to `4.80` per world minute;
- irrigation then raises farm yield, food deliveries and reserves, and lowers food-price pressure;
- the UI visibly labels this richer runtime `OFFCHAIN SIMULATION ALPHA`.

The compact blockchain mirror is separately implemented and verified: an eight-citizen World PDA supports signer-authorized join, move, gather, deposit, and four-stage repair actions; delegates to MagicBlock; executes the 15-event loop; commits; and reads the same root back from the local Solana base layer. The remaining integration gate is wallet/session-key authentication and live synchronization between that compact canonical PDA and the richer browser simulation.
