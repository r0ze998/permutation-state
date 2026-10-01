# Units, movement and building on the map — redesign after an Eternum benchmark

Design session, 2026-10-01. The player's verdict on the people layer: "one tile holds
several people and nobody can tell what any of them is doing; building and moving are
not designed at all". This note benchmarks Realms: Eternum (the client source,
BibliothecaDAO/eternum @ f6a50ac, and the "Beginners Guide to Eternum" gameplay video)
and fixes the rules the map follows from now on.

## What Eternum does (evidence)

| Topic | Eternum | Source |
|---|---|---|
| Occupancy | one entity per hex, enforced on chain; no stacking UI anywhere | `contracts/.../map.cairo` `TileOccupancy`; docs `military/armies.mdx` |
| An army | **one** model per army (troop type × tier), troop count only in the label, owner colour tint, contact shadow, idle clip | `three/managers/army-manager.ts`, `constants/army-constants.ts` |
| Labels | always-on compact owner tag; the rich card (troops, stamina, capacity, composition, ⚔/🛡 arrows, cooldown) on hover | `utils/labels/army-label-type.ts`, `hover-label-manager.ts`; video 35:00 |
| Move preview | reachable hexes lit by action colour (move green, explore cyan, attack orange); cost tooltip; "right-click to confirm" | `highlight-hex-manager.ts`, `worldmap-interaction-palette.ts`; video 35:10–35:45 |
| Movement | path line in owner colour; the model walks the path (Catmull-Rom through hex centres), faces its way, bobs, banks on turns, lands with a small punch | `path-renderer.ts`, `army-manager.ts applyMovementPlan`, `utils/spline-path.ts` |
| Exploring | fog surface; revealed tile swept clear; found yield flies to the counter | `terrain-fog-field.ts`, `reveal-yield.ts` |
| Building | a separate local view: buildable plots lit, a ghost of the real model previews placement, the pending building shows dimmed until confirmed; one model per building, small animated parts (banner, flame) | `scenes/hexception.tsx`, `building-preview.ts`; video 16:25–19:40 |
| People | **none**: no villagers, workers or carriers; transport ("donkeys") is UI only | grep of `three/`; docs `transfers-and-trade.mdx` |
| Battle | the attacker plays an attack, archers loose volleys, the loser plays "Defeated!"; then arrows and cooldown on the labels | `combat-presentation-coordinator.ts` |
| Zoom | one table: near = models + labels, mid = models + tier glyphs (labels only for selected / hovered / threatened / own), far = flat owner-tinted icons | `scenes/worldmap-content-ladder.ts` |

The principle under all of it: **one thing on the map = one figure, and its state is
said by its motion and a label, never by a crowd.**

## Where Sixfold Frontier stood

On one tile the map drew at once: up to six host sprites in slots, a flag per faction,
"activity actors" (two builders, three recruits, two sword fighters), townsfolk on
errands, field hands, carriers, the lord, a departing column of six, a name banner and
badges. Nothing said which figure was which host, and building or marching had no
object of its own (a builder hammering beside a town is not a building going up).

## The rules from now on

1. **One token per host group.** On a tile, the hosts of one faction are one token: a
   single soldier of the group's main unit type on a disc in the faction's colour. The
   number of hosts and troops is in its label, never in more figures. Two factions on
   one tile stand face to face (that is a fight about to happen).
2. **A unit reads by silhouette.** Spearman (spear and round shield), Archer (bow),
   Horseman (on a horse), Pikeman (long pike), Crossbowman (crossbow), Knight (armoured
   on a horse, lance), Scout (hooded, staff), Settler (cart). Two colour blocks: the
   faction's cloth and steel.
3. **The label says the rest** (Eternum's compact tag): a pill above the token — unit
   glyph, troops, "×n" when it is several hosts — in the faction's colour, a gold rim
   for the viewer's own. A status mark before it: marching ➜, exploring ⌖, fighting ⚔,
   resting ☾ (stamina below a march), arriving (dashed rim). Rich details stay in the
   hover tip and the inspector.
4. **Movement is the token moving.** The viewer's own march: the token walks its planned
   route (stored with the sealed order in this browser) from the departure bell to the
   arrival bell, along a dashed path in the faction's colour, to a ring at the
   destination. Others' marches are sealed: at the origin the token turns to mist with
   a seal and "set out · arrives at bell N"; at the arrival bell the revealed token
   comes out of the mist where it arrived.
5. **Exploring is a scout going and a tile clearing.** The scout token walks from the
   holding to each explored tile and back; an explored tile gets a short sweep.
6. **Building is the building.** A construction in progress shows on the holding as a
   scaffold with a progress ring and the time left; it completes with a short flourish
   and the building's name. (Eternum has no build time; we do, so we show it.)
7. **No crowds.** Townsfolk, field hands, carriers, builders, recruits and the walking
   lord are gone. The lord is on the holding's banner (portrait, and a small mark of
   what they did lately).
8. **Battles use the same tokens**: each side's token lunges, archers loose, losses rise
   as numbers, the fates play on the tokens (holds the field, turns back, is pushed
   back, falls).
9. **Zoom ladder**: tile detail = tokens and pills; province detail = the flags; world =
   the realm map. One representation per entity at every level.
