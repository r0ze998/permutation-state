# PERMUTATION STATE

> **One civilization. Thousands of citizens. One world that keeps moving.**

PERMUTATION STATE is a persistent civilization RPG simulation. Every human player enters Aster as one embodied citizen, walks to real places, carries real goods, and works inside the same economy as other players and AI citizens.

The current playable alpha proves a complete systemic loop:

```text
walk to a resource site
→ perform timed work
→ carry a capacity-limited inventory
→ deliver it to the shared warehouse
→ travel to a public construction site
→ contribute materials and labour
→ complete the East Sluice
→ change Aster's water production over time for every player
```

This replaces the old choice-card demo as the primary game. The earlier Mara → Ivo Handoff remains available as a separate Solana/MagicBlock proof lab; it no longer defines the game loop.

## Play the shared world

The easiest route is the existing local MagicBlock stack plus the shared-world gateway.

```bash
cd permutation-state-solana-receipt-spike
npm ci --ignore-scripts
npm run start:stack:local
```

In a second terminal:

```bash
cd permutation-state-solana-receipt-spike
npm start
```

Open:

```text
http://127.0.0.1:4173/world/?session=aster-living-alpha&actor=mara
```

Open a second browser or tab in the same shared civilization:

```text
http://127.0.0.1:4173/world/?session=aster-living-alpha&actor=ivo&name=Ivo%20Sen
```

Both clients see the same character positions, NPC schedules, resources, warehouse, construction, market, world clock, and civic consequences.

## How to play

- Click a named place to walk there, or click anywhere on the world.
- `WASD` walks in short steps; `F` recentres your character.
- Work is local: gathering, depositing, building, trading, crafting, and talking are rejected when the character is too far away.
- Gather timber at **Whisperwood Grove** and stone at **Old Granite Cut**, carry both to the **Civic Warehouse**, then travel to the **East Sluice** and contribute four repair stages.
- Other named citizens continue moving, working, eating, producing, and changing shared supply while you play.

## What is simulated now

- One authoritative world clock with bounded offline catch-up
- Visible player and AI-citizen movement
- Proximity-gated actions and timed jobs
- Capacity-limited personal inventories
- Resource regeneration and physical delivery
- AI citizens who gather into personal inventories and physically haul cargo
- Shared warehouse and construction inputs
- A workshop production cycle
- Supply-sensitive market prices and marketplace fee accounting
- NPC schedules, needs, work, relationships, and deterministic memory
- A public project whose completion changes water, irrigation, farm yield, food reserves, and prices
- Persistent shared sessions across reloads and browsers

## Honest chain boundary

The playable browser world is currently an **authoritative gateway simulation alpha** and is labelled `OFFCHAIN SIMULATION ALPHA` in both its API and UI. Browser actions are not yet wallet-signed or mirrored into the World PDA.

The repository's Solana program and MagicBlock integration now genuinely implement:

- a dedicated eight-citizen World PDA with positions, inventories, resource nodes, warehouse stock, East Sluice progress, and water rate;
- signer-authorized join, move, gather, deposit, and repair transitions guarded by sequence, prior root, event-chain head, and event ID;
- World PDA initialization, MagicBlock delegation, ER execution, commit, and undelegation lifecycle instructions;
- an automated local end-to-end path that completes all 15 world events and verifies the same state root after a Solana base-layer checkpoint;
- local Solana initialization and account ownership;
- MagicBlock delegation and Ephemeral Rollup execution;
- deterministic Worksite state roots and event receipts;
- base-layer checkpoint verification;
- a season purse ledger and settlement scaffold.

The remaining integration boundary is the browser runtime: wallet/session-key authentication and synchronization between the richer gateway simulation and the compact canonical World PDA. Until that lands, the UI deliberately does not claim that its live JSON simulation is onchain.

Proof lab:

```text
http://127.0.0.1:4173/proof/?proof=1&session=aster-demo&role=observer&transport=magicblock
```

## Repository map

| Path | Purpose |
|---|---|
| [`permutation-state-prototype/world/`](permutation-state-prototype/world/) | Primary map-first playable simulation. |
| [`permutation-state-prototype/proof/`](permutation-state-prototype/proof/) | Previous multi-client Handoff and chain-receipt proof lab. |
| [`permutation-state-solana-receipt-spike/`](permutation-state-solana-receipt-spike/) | Solana program, MagicBlock transport, gateway, and tests. |
| [`PERMUTATION_STATE_SIMULATION_PIVOT.md`](PERMUTATION_STATE_SIMULATION_PIVOT.md) | Corrected game definition, system boundaries, and acceptance gates. |
| [`PERMUTATION_STATE_GAME_CONSTITUTION.md`](PERMUTATION_STATE_GAME_CONSTITUTION.md) | Product and economic principles; sections superseded by the simulation pivot should be revised next. |

## Verify

```bash
node --check permutation-state-prototype/world/app.js
node --test permutation-state-prototype/world/core.test.mjs

cd permutation-state-solana-receipt-spike
cargo test --locked
npm test

# With the local MagicBlock stack running:
npm run test:e2e:world:local
```

The automated suite covers deterministic movement, proximity rejection, gathering, carrying, AI logistics, depositing, construction, irrigation knock-on effects, passive production, NPC schedules, market response, shared clients, persistence, chain authorization, and a verified World PDA checkpoint.
