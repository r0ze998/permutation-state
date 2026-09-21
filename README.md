# PERMUTATION STATE

**One shared civilization. Different citizens. Consequences that connect.**

A rebuilt, map-first civilization strategy prototype. Players are citizens in one shared real-time
world. They explore terrain, invest common resources in different facilities, extend roads, and
develop connected production chains. The map is simulation data, not a static background image.

This is a playable foundation, **not a completed MMO**. It has no live generative AI, combat,
real entry fees, real prizes, or live Solana execution of the new simulation. The earlier blockchain
experiments remain available separately and are not represented as proof that this new game is onchain.

## Play

**日本語:** [プレイガイド](PLAY_GUIDE.ja.md) — 操作方法、最初の5分、施設・物流・探索・研究。
ゲーム右上の「？」からも、[画面版ガイド](http://127.0.0.1:4173/civilization/guide.html)を開けます。
文明の画面上の呼び名は「私たちの文明」です。ゲームタイトルは **PERMUTATION STATE** のままです。

No blockchain stack or npm installation is necessary for the new game:

```sh
cd permutation-state-solana-receipt-spike
npm run start:civilization
```

Open [the civilization](http://127.0.0.1:4174/civilization/).
The existing integrated gateway also serves it at
[port 4173](http://127.0.0.1:4173/civilization/) when the local Solana/MagicBlock stack is running.
Both servers redirect their root URL to the new game.

**Run only one server against a given state directory.** The default is `../work/devnet` relative
to this repository; both server modes share it. For isolated tests, set
`PERMSTATE_CIVILIZATION_WORK_DIR` on the standalone server. Its port can be changed with
`CIVILIZATION_PORT=4173 npm run start:civilization`.

Every ordinary game tab joins the same civilization. The legacy storage ID `aster` is retained
only for save compatibility; it is not the civilization's display name. Each tab has
a separate citizen capability stored in sessionStorage; reload resumes that citizen. Closing the
tab ends that local identity, but never resets the world. These are local prototype capabilities,
not wallet accounts or a production anti-abuse identity system.

## Controls and the first few minutes

- **Click a hex** to see its terrain, resources, legal investments, costs and constraints.
- **Double-click known land**, or choose **ここへ移動**, to move your visible citizen. Roads are
  faster; hills and mountains take longer; water cannot be crossed.
- **Build nearby**. A farm produces food; a lumbermill produces wood; a quarry supplies stone.
  A workshop consumes delivered wood and ore to produce tools. Those choices compete for the
  same starting resources. There is no required first building.
- **Follow the goods**. Workers travel to facilities; input and output couriers follow connected
  roads. Goods enter common stock only when delivered. The inspector explains blocked production.
- **Expand**. Explore an adjacent unknown hex, then extend roads to open new building sites.
  A watchtower reveals a wider area. An archive makes knowledge; research changes farming,
  transport, and the ability to build mines.
- **Recover**. When resources are short, gather manually on nearby terrain. Your citizen
  carries those goods back to the depot automatically. There is no purchase requirement.
- **Read the world**. Switch food, industry, logistics or terrain lenses. Click resource totals
  for the economy; open the town, research, and history panels for details.
- **Next Action is optional**. It focuses a suggested location; it does not spend, execute,
  complete a turn, or lock you into a quest. Cycle it to choose another opportunity.
- Drag to pan, scroll or use `+ / −` to zoom, `F` to find your citizen, `N` to inspect the current
  suggestion, `Esc` to close panels. The `?` button explains the game in Japanese.

## Implemented and verified

- 217 state-driven hexes with shared discovery, terrain-aware pathfinding and visible movement
- Seven buildable facility types plus the communal depot, with physical placement and timed work
- Limited shared starting assets, differing investment paths, renewable manual recovery
- Road connectivity, source inventories, inbound materials, outbound deliveries and net rates
- Deterministic NPC workers and couriers (not LLM characters)
- Three research unlocks and shared civilization milestones; milestone completion does not end play
- Japanese-first contextual UI, five lenses, selection previews and optional task suggestions
- Two-client shared state, capability-authorized actions, serialized mutations and disk persistence
- Rejected actions do not consume resources; previews and authoritative actions use the same rules
- A tested 15-minute legal progression from founding through agriculture, industry and exploration

The service runs a 250ms authoritative simulation; the UI polls snapshots and interpolates movement.
It is a local development server bound to loopback, not a public multiplayer deployment. Catch-up
after server downtime is bounded to 60 seconds; it does not invent unlimited offline production.

## Verify

```sh
node --check permutation-state-prototype/civilization/app.mjs
node --check permutation-state-prototype/civilization/copy.mjs
node --check permutation-state-prototype/civilization/map.mjs
node --test permutation-state-prototype/civilization/*.test.mjs
cd permutation-state-solana-receipt-spike
npm ci --ignore-scripts
npm test
```

The new simulation and service suites test branching investments, pathfinding, discovery, locality,
construction, input/output deliveries, road connectivity, previews, research, recovery from depleted
assets, multi-citizen state, authentication, persistence, and restart behaviour.

## Project map

| Path | Purpose |
|---|---|
| `permutation-state-prototype/civilization/` | New game, deterministic core, renderer and UI |
| `PERMUTATION_STATE_REBUILD.md` | Active design and module contract |
| `permutation-state-solana-receipt-spike/server/civilization-*` | Standalone/shared authoritative service and tests |
| `permutation-state-prototype/world/` | Archived walking-and-watergate experiment |
| `permutation-state-prototype/proof/` | Archived choice-based chain proof, not the new game |
| `permutation-state-solana-receipt-spike/src/` | Existing Solana experiments; new simulation integration remains future work |
| `ARCHIVED_REPAIR_DEMO.md` | Previous experiment instructions and limitations |

Next product work: playtest the decisions and pacing; then connect generated NPC intentions to this
validated simulation, add meaningful threats/trade, and design an authenticated chain/season economy.
Those systems are not silently mocked into the playable build.
