# Wylls — Map-first rebuild

Status: implementation contract for the new `/civilization/` game, 2026-09-21.
This replaces the fixed Handoff / East Sluice demo as the primary gameplay design.
The old `/proof/` and `/world/` implementations remain intact as archived experiments.

## Product and acceptance

All citizens share one real-time civilization. The map is actual world data, not a painted backdrop.
The player controls one visible citizen, participates in construction and exploration, and reads
the shared economy through contextual panels and switchable lenses. No end-turn, authored ending,
fixed repair quest, private nation, or fictional claim of onchain/LLM execution.

The first playable build must support competing investments in food, industry and exploration.
Buildings change continuous production; inputs, transport connectivity and time constrain output.
Exploration expands the known map and reveals different development opportunities. Another player
sees and can use the first player's infrastructure. NPC workers and couriers visibly enact production
and delivery; deterministic behaviour is labelled as such. A season ambition tracks civilization-wide
progress without ending free play or paying real money.

Acceptance is behavioural: identical starting resources invested differently produce different maps,
resource balances, and subsequent legal actions. A second client observes that same canonical state.
Next Action suggests opportunities and focuses the map; it never spends resources or forces a quest.

## Ownership

- `civilization/core.mjs`, `core.test.mjs`: simulation agent.
- `civilization/map.mjs`: renderer agent. Exports `CivilizationMap`.
- `civilization/index.html`, `styles.css`, `app.mjs`: main agent.
- `server/civilization-service.mjs`, tests, gateway integration: service agent.
- Root docs and final integration / browser QA: main agent.

## Shared module contract

Core is browser-safe ESM with no dependencies. Exports:

```
createCivilization({sessionId = 'aster', nowMs = 0}) -> world
advanceCivilization(world, nowMs) -> world // mutate, monotonic bounded elapsed time
applyCivilizationAction(world, action, nowMs) -> world // validate then mutate; throws Error
getCitizenTasks(world, actorId) -> [{id,kind,title,detail,tileId,priority}]
getBuildPreview(world, actorId, tileId, buildingType) -> {allowed,reason,cost,durationMs,effect}
BUILDINGS // keyed by farm, lumbermill, quarry, mine, workshop, watchtower, archive
RESOURCE_META // food,wood,stone,ore,tools,knowledge {name,color,...}
hexDistance(a,b), tileById(world,id)
```

Authoritative JSON shape (extensions allowed; do not rename these fields):

```
world = {
 schemaVersion: 'permutation.civilization.v1', sessionId, revision, timeMs, lastNowMs,
 tiles: [{id:'q,r',q,r,terrain:'grass'|'forest'|'hill'|'mountain'|'water',
          resource:null|'wood'|'stone'|'ore',explored:boolean,road:boolean,buildingId:null|string}],
 players: {[id]:{id,name,q,r,path:[tileId],status,job:null|object,contribution:number}},
 buildings: {[id]:{id,type,tileId,status:'building'|'active'|'blocked',
                   progress:0..1,ownerId,connected:boolean,localStock:object,reason:string}},
 stock:{food,wood,stone,ore,tools,knowledge}, rates:{food,wood,stone,ore,tools,knowledge},
 npcs:[{id,name,q,r,role,status,targetTileId}],
 caravans:[{id,fromTileId,toTileId,progress:0..1,resource,amount}],
 events:[{id,type,text,timeMs,tileId}],
 season:{name,elapsedMs,durationMs,objectives:[{id,label,current,target,complete}],complete:boolean},
 research:{active:null|string,progress:0..1,unlocked:[string]}
}
```

Actions: `JOIN {actorId,name}`, `MOVE {actorId,tileId}`, `EXPLORE {actorId,tileId}`,
`BUILD {actorId,tileId,buildingType}`, `ROAD {actorId,tileId}`,
`GATHER {actorId,tileId}`, `RESEARCH {actorId,techId}`.
Terrain affects pathfinding. Unknown terrain cannot be remotely harvested or built on. Work needs
the citizen on or adjacent to its target. Shared-stock construction is only allowed within the road
network; show that rule in preview. Never silently teleport or charge resources on rejected actions.
Production output first exists at its source and is delivered by visible timed couriers. Rates are
net expected units per minute, blocked inputs/connectivity are explicit. No NPC auto-build of the
player's entire civilization. Research tech IDs: agriculture, logistics, metallurgy.

## Gateway / browser contract

The display name is 「私たちの文明」. The default local storage/session ID remains `aster` solely
for save and citizen-token compatibility; it is not a public brand. Additional IDs are test isolation only.
Browser-generated identity is stored in sessionStorage, not role-specific links.

```
POST /api/civilization/join {session,actorId,name} -> {world,actorId,token,disclosure}
GET /api/civilization/state?session=aster -> {world,disclosure}
POST /api/civilization/action {session,actorId,token,action:{type,...}} -> {world,disclosure}
```

Use a capability token per citizen; never allow a supplied actorId to impersonate another citizen.
Persist worlds and authentication safely, serialize changes per world, bound catch-up, validate input.
No fallback fake simulation when the gateway is unavailable. Show an honest reconnect state instead.
The new runtime is an offchain prototype; existing chain proofs stay separate.

## Renderer contract

```
new CivilizationMap(canvas, {onSelect(tileId),onHover(tileId|null),onMove(tileId)})
map.setState(world, actorId)
map.setSelection(tileId|null)
map.setLens('normal'|'food'|'industry'|'logistics'|'danger')
map.focusTile(tileId) / map.focusPlayer() / map.zoomBy(factor)
map.destroy()
```

Use an attractive code-rendered 2.5D hex diorama with trees, mountains, water, warm roofed buildings,
fog, roads, animated citizens/couriers, construction progress. Pan by drag, wheel zoom, click selects,
double-click requests movement. Frame the known starting settlement initially. Canvas must resize
crisply for devicePixelRatio and expose tile screen positions via `map.screenPosition(tileId)` for QA.
Objects must match simulation state; no baked-in fake houses, units or paths.

## UI hierarchy

Large map; compact top resource stock and per-minute flow; civilization ambition; right contextual
selection panel; left collapsible production/research/chronicle; bottom lenses and citizen card;
bottom-right optional Next Action. Japanese-first copy. Costs and reasons before confirmation.
Personal command versus shared civilization resources clearly labelled. Explain blocked production
and downstream effects. Minimize permanent panels, preserve keyboard and small-screen access.
