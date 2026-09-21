import test from "node:test";
import assert from "node:assert/strict";

import {
  WORLD_SCHEMA,
  WorldActionError,
  advanceWorld,
  applyWorldAction,
  createWorld,
  getNearbyInteractions,
  getPublicWorld,
  serializeWorld
} from "./core.mjs";

function join(world, nowMs = 1_000, overrides = {}) {
  return applyWorldAction(world, {
    type: "JOIN",
    actorId: "player-one",
    name: "Mara",
    profession: "forester",
    ...overrides
  }, nowMs);
}

function moveAndArrive(world, target, startMs, arriveMs) {
  const moving = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: target.x, y: target.y }, startMs);
  return advanceWorld(moving, arriveMs);
}

test("createWorld returns a JSON-safe world with the required simulation systems", () => {
  const world = createWorld({ sessionId: "schema-test", nowMs: 1_000 });
  assert.equal(world.schemaVersion, WORLD_SCHEMA.id);
  assert.deepEqual(world.bounds, { minX: 0, maxX: 100, minY: 0, maxY: 60 });
  assert.deepEqual(Object.keys(world.nodes).sort(), ["farm", "forest", "ore", "quarry"]);
  assert.ok(Object.keys(world.npcs).length >= 4);
  assert.equal(world.projects.eastSluice.status, "repairing");
  assert.doesNotThrow(() => JSON.parse(JSON.stringify(world)));
});

test("JOIN and MOVE produce real bounded movement without mutating prior state", () => {
  const original = createWorld({ sessionId: "movement", nowMs: 1_000 });
  const joined = join(original, 1_000);
  assert.equal(Object.keys(original.players).length, 0);
  const start = { x: joined.players["player-one"].x, y: joined.players["player-one"].y };
  const moving = applyWorldAction(joined, { type: "MOVE", actorId: "player-one", x: 200, y: -10 }, 1_000);
  assert.deepEqual(moving.players["player-one"].target, { x: 100, y: 0 });
  const advanced = advanceWorld(moving, 2_000);
  const player = advanced.players["player-one"];
  assert.ok(Math.hypot(player.x - start.x, player.y - start.y) > 9.9);
  assert.ok(player.x <= 100 && player.y >= 0);
  assert.equal(joined.players["player-one"].target, null);
});

test("the shared district admits up to eight human citizens", () => {
  let world = createWorld({ sessionId: "capacity", nowMs: 0 });
  for (let index = 0; index < WORLD_SCHEMA.maxPlayers; index += 1) {
    world = applyWorldAction(world, { type: "JOIN", actorId: `player-${index}`, name: `Player ${index}` }, 0);
  }
  assert.equal(Object.keys(world.players).length, 8);
  assert.throws(
    () => applyWorldAction(world, { type: "JOIN", actorId: "player-8", name: "Ninth" }, 0),
    (error) => error instanceof WorldActionError && error.code === "WORLD_FULL"
  );
});

test("proximity is enforced for world interactions", () => {
  const world = join(createWorld({ sessionId: "proximity", nowMs: 0 }), 0);
  assert.throws(
    () => applyWorldAction(world, { type: "GATHER", actorId: "player-one", nodeId: "ore" }, 0),
    (error) => error instanceof WorldActionError && error.code === "TOO_FAR"
  );
});

test("gather, carry, deposit, and build form a complete timed loop", () => {
  let now = 0;
  let world = join(createWorld({ sessionId: "loop", nowMs: now }), now);
  const startingTimber = world.warehouse.stocks.timber;
  const startingProgress = world.projects.eastSluice.progress;

  now += 4_000;
  world = moveAndArrive(world, world.nodes.forest, 0, now);
  assert.ok(Math.hypot(world.players["player-one"].x - world.nodes.forest.x, world.players["player-one"].y - world.nodes.forest.y) < 0.1);

  world = applyWorldAction(world, { type: "GATHER", actorId: "player-one", nodeId: "forest" }, now);
  assert.equal(world.players["player-one"].job.type, "gather");
  now += 1_600;
  world = advanceWorld(world, now);
  assert.equal(world.players["player-one"].inventory.timber, 4);

  world = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: world.warehouse.x, y: world.warehouse.y }, now);
  now += 3_500;
  world = advanceWorld(world, now);
  world = applyWorldAction(world, { type: "DEPOSIT", actorId: "player-one" }, now);
  assert.equal(world.players["player-one"].inventory.timber, 0);
  assert.equal(world.warehouse.stocks.timber, startingTimber + 4);

  world = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: world.nodes.quarry.x, y: world.nodes.quarry.y }, now);
  now += 4_000;
  world = advanceWorld(world, now);
  world = applyWorldAction(world, { type: "GATHER", actorId: "player-one", nodeId: "quarry" }, now);
  now += 2_500;
  world = advanceWorld(world, now);
  assert.equal(world.players["player-one"].inventory.stone, 4);
  world = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: world.warehouse.x, y: world.warehouse.y }, now);
  now += 4_000;
  world = advanceWorld(world, now);
  world = applyWorldAction(world, { type: "DEPOSIT", actorId: "player-one" }, now);
  assert.equal(world.players["player-one"].inventory.stone, 0);

  world = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: world.projects.eastSluice.x, y: world.projects.eastSluice.y }, now);
  now += 3_000;
  world = advanceWorld(world, now);
  const timberBeforeBuild = world.warehouse.stocks.timber;
  world = applyWorldAction(world, { type: "BUILD", actorId: "player-one", projectId: "east-sluice" }, now);
  assert.equal(world.players["player-one"].job.type, "build");
  assert.equal(world.warehouse.stocks.timber, timberBeforeBuild - 2);
  now += 3_600;
  world = advanceWorld(world, now);
  assert.ok(world.projects.eastSluice.progress > startingProgress);
  assert.equal(world.projects.eastSluice.contributors["player-one"], 25);
});

test("East Sluice completion materially increases passive water production", () => {
  const damaged = createWorld({ sessionId: "water-rate", nowMs: 0 });
  const repaired = JSON.parse(JSON.stringify(damaged));
  repaired.projects.eastSluice.status = "complete";
  repaired.projects.eastSluice.progress = 100;
  repaired.projects.eastSluice.labor = repaired.projects.eastSluice.laborRequired;
  repaired.buildings.eastSluice.status = "operating";

  const damagedAfter = advanceWorld(damaged, 60_000);
  const repairedAfter = advanceWorld(repaired, 60_000);
  assert.ok(repairedAfter.civilization.water > damagedAfter.civilization.water + 3.5);
  assert.equal(damagedAfter.civilization.waterRatePerMinute, 0.7);
  assert.equal(repairedAfter.civilization.waterRatePerMinute, 4.8);
});

test("advanceWorld runs workshop production and NPC schedules", () => {
  const world = createWorld({ sessionId: "living-world", nowMs: 0 });
  const talaStart = { x: world.npcs.tala.x, y: world.npcs.tala.y };
  const workshopStart = { ...world.buildings.workshop.inventory };
  const advanced = advanceWorld(world, 11_000);
  assert.ok(Math.hypot(advanced.npcs.tala.x - talaStart.x, advanced.npcs.tala.y - talaStart.y) > 1);
  assert.ok(advanced.npcs.tala.destinationId);
  assert.equal(advanced.buildings.workshop.inventory.timber, workshopStart.timber - 1);
  assert.equal(advanced.buildings.workshop.inventory.ore, workshopStart.ore - 1);
  assert.equal(advanced.buildings.workshop.inventory.goods, workshopStart.goods + 1);
});

test("AI workers gather into inventory and physically haul cargo before storage changes", () => {
  const world = advanceWorld(createWorld({ sessionId: "npc-logistics", nowMs: 0 }), 5_000);
  const bram = world.npcs.bram;
  bram.x = world.nodes.farm.x;
  bram.y = world.nodes.farm.y;
  bram.target = null;
  bram.destinationId = "farm";
  bram.workAccumulatorMs = bram.workIntervalMs - 250;
  const farmBefore = world.nodes.farm.amount;
  const warehouseBefore = world.warehouse.stocks.food;

  const gathered = advanceWorld(world, 5_250);
  assert.ok(gathered.npcs.bram.inventory.food >= 1);
  assert.ok(gathered.nodes.farm.amount < farmBefore);
  assert.equal(gathered.warehouse.stocks.food, warehouseBefore);

  const hauling = JSON.parse(JSON.stringify(gathered));
  hauling.npcs.bram.inventory.food = 4;
  hauling.npcs.bram.x = hauling.warehouse.x;
  hauling.npcs.bram.y = hauling.warehouse.y;
  hauling.npcs.bram.target = null;
  hauling.npcs.bram.destinationId = "warehouse";
  const delivered = advanceWorld(hauling, hauling.lastAdvancedAtMs + 250);
  assert.equal(delivered.npcs.bram.inventory.food, 0);
  assert.equal(delivered.warehouse.stocks.food, warehouseBefore + 4);
  assert.ok(delivered.events.some((event) => event.type === "NPC_DELIVERED" && event.data.npcId === "bram"));
});

test("repaired sluice propagates through irrigation, food logistics, reserves, and prices", () => {
  const damaged = createWorld({ sessionId: "irrigation-damaged", nowMs: 0 });
  const repaired = createWorld({ sessionId: "irrigation-repaired", nowMs: 0 });
  repaired.projects.eastSluice.status = "complete";
  repaired.projects.eastSluice.progress = 100;
  repaired.projects.eastSluice.labor = repaired.projects.eastSluice.laborRequired;
  repaired.buildings.eastSluice.status = "operating";

  for (const world of [damaged, repaired]) {
    world.npcs = { bram: world.npcs.bram };
    world.npcs.bram.x = world.nodes.farm.x;
    world.npcs.bram.y = world.nodes.farm.y;
    world.npcs.bram.target = null;
    world.npcs.bram.destinationId = "farm";
  }

  const damagedAfter = advanceWorld(damaged, 90_000);
  const repairedAfter = advanceWorld(repaired, 90_000);
  assert.equal(damagedAfter.civilization.farmProductionMultiplier <= 1, true);
  assert.equal(repairedAfter.civilization.farmProductionMultiplier >= 1.5, true);
  assert.ok(repairedAfter.warehouse.stocks.food > damagedAfter.warehouse.stocks.food);
  assert.ok(repairedAfter.civilization.food > damagedAfter.civilization.food);
  assert.ok(repairedAfter.market.prices.food < damagedAfter.market.prices.food);
  assert.ok(repairedAfter.events.some((event) => event.type === "NPC_GATHERED" && event.data.quantity === 2));
});

test("market prices react to supply and trading contributes a fee", () => {
  let world = join(createWorld({ sessionId: "market", nowMs: 0 }), 0);
  world = moveAndArrive(world, world.market, 0, 2_000);
  const priceBefore = world.market.prices.food;
  const poolBefore = world.civilization.prizePool;
  world = applyWorldAction(world, { type: "BUY", actorId: "player-one", item: "food", quantity: 2 }, 2_000);
  assert.equal(world.players["player-one"].inventory.food, 2);
  assert.ok(world.civilization.prizePool > poolBefore);
  assert.ok(world.market.prices.food > priceBefore);
});

test("nearby interactions and public serialization expose UI-friendly views", () => {
  let world = join(createWorld({ sessionId: "public-view", nowMs: 0 }), 0);
  world = moveAndArrive(world, world.warehouse, 0, 1_000);
  world.players["player-one"].inventory.food = 1;
  const interactions = getNearbyInteractions(world, "player-one");
  assert.ok(interactions.some((interaction) => interaction.type === "DEPOSIT"));
  const publicWorld = getPublicWorld(world);
  assert.ok(publicWorld.actors.some((actor) => actor.id === "player-one"));
  assert.ok(publicWorld.hotspots.some((hotspot) => hotspot.id === "east-sluice"));
  assert.equal(JSON.parse(serializeWorld(world)).schemaVersion, WORLD_SCHEMA.id);
});

test("the same initial state, actions, and timestamps are deterministic", () => {
  const run = () => {
    let world = createWorld({ sessionId: "determinism", nowMs: 10 });
    world = join(world, 10);
    world = applyWorldAction(world, { type: "MOVE", actorId: "player-one", x: 20, y: 20 }, 10);
    return advanceWorld(world, 4_321);
  };
  assert.deepEqual(run(), run());
});

test("catch-up time is bounded", () => {
  const world = createWorld({ sessionId: "catchup", nowMs: 0 });
  const advanced = advanceWorld(world, 3_600_000);
  assert.equal(advanced.simulationTimeMs, WORLD_SCHEMA.maxCatchupMs);
  assert.ok(advanced.events.some((event) => event.type === "CATCHUP_CLAMPED"));
});
