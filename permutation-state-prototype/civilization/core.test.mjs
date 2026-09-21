import test from "node:test";
import assert from "node:assert/strict";
import {
  createCivilization, advanceCivilization, applyCivilizationAction,
  getCitizenTasks, getBuildPreview, getRoadPreview, getResearchPreview, BUILDINGS, RESOURCE_META,
  hexDistance, tileById, findPath
} from "./core.mjs";

function fixture(name = "test") {
  const world = createCivilization({ sessionId: name, nowMs: 0 });
  act(world, "JOIN", { name: "マラ" });
  return world;
}
function act(world, type, extra = {}, actorId = "mara") {
  return applyCivilizationAction(world, { type, actorId, ...extra }, world.lastNowMs);
}
function wait(world, milliseconds) {
  let left = milliseconds;
  while (left > 0) {
    const step = Math.min(30_000, left);
    advanceCivilization(world, world.lastNowMs + step); left -= step;
  }
  return world;
}
function move(world, tileId, actorId = "mara") {
  act(world, "MOVE", { tileId }, actorId); wait(world, 15_000);
  assert.equal(world.players[actorId].q, tileById(world, tileId).q);
  assert.equal(world.players[actorId].r, tileById(world, tileId).r);
}
function build(world, type, tileId, actorId = "mara") {
  act(world, "BUILD", { buildingType: type, tileId }, actorId);
  wait(world, BUILDINGS[type].durationMs + 6_000);
  return world.buildings[tileById(world, tileId).buildingId];
}

test("JSON state has exact public shape and deterministic initial resources", () => {
  const first = createCivilization(), second = createCivilization();
  assert.deepEqual(first, second);
  assert.equal(first.tiles.length, 217);
  assert.equal(first.tiles.filter((tile) => tile.explored).length, 19);
  assert.deepEqual(Object.keys(first.stock), Object.keys(RESOURCE_META));
  first.stock.wood = 0;
  assert.equal(second.stock.wood, 30);
  assert.equal(hexDistance({ q: 0, r: 0 }, { q: 2, r: -1 }), 2);
  assert.doesNotThrow(() => JSON.parse(JSON.stringify(first)));
});

test("functions mutate and return original world; shared citizens exceed old eight-person demo cap", () => {
  const world = fixture();
  for (let i = 0; i < 12; i += 1) assert.equal(act(world, "JOIN", { name: `Citizen ${i}` }, `citizen-${i}`), world);
  assert.equal(Object.keys(world.players).length, 13);
  assert.equal(advanceCivilization(world, 1_000), world);
  assert.equal(world.players.mara.q, 0);
});

test("malformed and inherited-object identifiers cannot become citizens or buildings", () => {
  const world = fixture();
  const snapshot = JSON.stringify(world);
  for (const actorId of ["__proto__", "constructor", "prototype", "bad id", ""]) assert.throws(() => act(world, "JOIN", {}, actorId));
  assert.equal(getBuildPreview(world, "mara", "0,1", "__proto__").allowed, false);
  assert.throws(() => act(world, "RESEARCH", { techId: "__proto__" }), /研究/);
  assert.equal(JSON.stringify(world), snapshot);
});

test("monotonic time and bounded catch-up never simulate unlimited offline earnings", () => {
  const world = fixture();
  advanceCivilization(world, 1_000_000);
  assert.equal(world.timeMs, 60_000);
  assert.equal(world.lastNowMs, 1_000_000);
  const snapshot = JSON.stringify(world);
  advanceCivilization(world, 999_000);
  assert.equal(JSON.stringify(world), snapshot);
  assert.throws(() => advanceCivilization(world, NaN));
});

test("invalid actions leave every field unchanged, even when submitted at a later time", () => {
  const world = fixture();
  const snapshot = JSON.stringify(world);
  for (const action of [
    { type: "BUILD", actorId: "mara", tileId: "1,1", buildingType: "farm" },
    { type: "BUILD", actorId: "mara", tileId: "0,1", buildingType: "mine" },
    { type: "MOVE", actorId: "mara", tileId: "8,0" },
    { type: "ROAD", actorId: "mara", tileId: "0,0" },
    { type: "RESEARCH", actorId: "mara", techId: "metallurgy" },
    { type: "BUILD", actorId: "missing", tileId: "0,1", buildingType: "farm" }
  ]) {
    assert.throws(() => applyCivilizationAction(world, action, 20_000));
    assert.equal(JSON.stringify(world), snapshot);
  }
});

test("citizens follow known terrain paths over time and cannot cross water", () => {
  const world = fixture();
  const destination = tileById(world, "2,0");
  tileById(world, "1,0").terrain = "water";
  const path = findPath(world, "0,0", destination.id);
  assert.ok(path.length >= 3);
  assert.ok(!path.includes("1,0"));
  act(world, "MOVE", { tileId: destination.id });
  assert.equal(world.players.mara.q, 0, "command must not teleport");
  wait(world, 250);
  assert.equal(world.players.mara.q, 0);
  wait(world, 12_000);
  assert.equal(world.players.mara.q, 2);
  assert.throws(() => act(world, "MOVE", { tileId: "1,0" }), /水面/);
});

test("water enclosure blocks every path and rejects movement without any mutation", () => {
  const world = fixture();
  for (const id of ["1,0", "1,-1", "0,-1", "-1,0", "-1,1", "0,1"]) tileById(world, id).terrain = "water";
  assert.equal(findPath(world, "0,0", "2,0"), null);
  const snapshot = JSON.stringify(world);
  assert.throws(() => act(world, "MOVE", { tileId: "2,0" }), /陸路/);
  assert.equal(JSON.stringify(world), snapshot);
});

test("exploration is local, timed, shared and reveals development opportunities", () => {
  const world = fixture();
  assert.equal(tileById(world, "-3,1").explored, false);
  assert.throws(() => act(world, "EXPLORE", { tileId: "-3,1" }), /隣/);
  move(world, "-2,1");
  act(world, "EXPLORE", { tileId: "-3,1" });
  assert.equal(tileById(world, "-3,1").explored, false);
  wait(world, 5_000);
  assert.equal(tileById(world, "-3,1").explored, true);
  assert.ok(world.tiles.filter((tile) => tile.explored).length > 19);
  assert.ok(world.players.mara.contribution > 0);
  act(world, "JOIN", { name: "イヴォ" }, "ivo");
  assert.ok(findPath(world, "0,0", "-3,1"));
  assert.equal(world.season.objectives.find((objective) => objective.id === "exploration").current, world.tiles.filter((tile) => tile.explored).length);
});

test("roads are investments which unlock a previously disconnected building site", () => {
  const world = fixture();
  move(world, "-2,1");
  act(world, "EXPLORE", { tileId: "-3,1" }); wait(world, 5_000);
  assert.equal(getBuildPreview(world, "mara", "-3,1", "lumbermill").allowed, false);
  assert.match(getBuildPreview(world, "mara", "-3,1", "lumbermill").reason, /道/);
  act(world, "ROAD", { tileId: "-2,1" });
  assert.equal(tileById(world, "-2,1").road, false);
  wait(world, 4_000);
  assert.equal(tileById(world, "-2,1").road, true);
  assert.equal(getBuildPreview(world, "mara", "-3,1", "lumbermill").allowed, true);
});

test("construction is local, timed, paid once, and visible to all citizens", () => {
  const world = fixture();
  act(world, "JOIN", { name: "イヴォ" }, "ivo");
  assert.equal(getBuildPreview(world, "mara", "0,1", "farm").allowed, true);
  act(world, "BUILD", { tileId: "0,1", buildingType: "farm" });
  const id = tileById(world, "0,1").buildingId;
  assert.equal(world.stock.wood, 18);
  assert.equal(world.buildings[id].progress, 0);
  assert.equal(world.buildings[id].status, "building");
  assert.throws(() => act(world, "BUILD", { tileId: "0,1", buildingType: "farm" }, "ivo"), /使われて/);
  wait(world, 7_500);
  assert.equal(world.buildings[id].progress, 0.5);
  wait(world, 7_500);
  assert.equal(world.buildings[id].progress, 1);
  assert.equal(world.players.mara.job, null);
  assert.equal(world.stock.wood, 18);
  assert.ok(world.npcs.some((npc) => npc.buildingId === id));
});

test("identical starting assets lead to divergent investments, rates and next legal buildings", () => {
  const agricultural = fixture("agriculture"), industrial = fixture("industry");
  build(agricultural, "farm", "0,1");
  build(industrial, "workshop", "0,1");
  assert.ok(agricultural.rates.food > 0);
  assert.ok(industrial.rates.tools > 0);
  assert.ok(industrial.rates.ore < 0);
  assert.notDeepEqual(agricultural.stock, industrial.stock);
  assert.notDeepEqual(agricultural.buildings, industrial.buildings);
  assert.equal(getBuildPreview(agricultural, "mara", "-1,0", "watchtower").allowed, true);
  assert.equal(getBuildPreview(industrial, "mara", "-1,0", "archive").allowed, false);
  assert.ok(agricultural.rates.food > industrial.rates.food);
});

test("starting reserves cannot fund every investment; the first choices have opportunity cost", () => {
  const world = fixture();
  act(world, "JOIN", { name: "イヴォ" }, "ivo");
  act(world, "BUILD", { tileId: "0,1", buildingType: "farm" });
  act(world, "BUILD", { tileId: "-1,1", buildingType: "lumbermill" }, "ivo");
  assert.equal(world.stock.wood, 6);
  act(world, "JOIN", {}, "tala");
  assert.match(getBuildPreview(world, "tala", "1,-1", "quarry").reason, /不足/);
});

test("production exists at source and shared resources arrive only after timed delivery", () => {
  const world = fixture();
  build(world, "farm", "0,1");
  let courier = null;
  for (let i = 0; i < 80 && !courier; i += 1) {
    wait(world, 250);
    courier = world.caravans.find((c) => c.direction === "output" && c.resource === "food");
  }
  assert.ok(courier, "farm must dispatch a visible courier");
  assert.ok(courier.progress < 1);
  const before = world.stock.food;
  assert.ok(before < 30, "no remote instant food credit");
  wait(world, Math.ceil(courier.durationMs));
  assert.ok(world.stock.food > before + 4.5);
  assert.equal(world.totals.delivered.food, 5);
});

test("industry requires physically delivered inputs and blocks when the input chain is exhausted", () => {
  const world = fixture();
  build(world, "workshop", "0,1");
  const building = world.buildings[tileById(world, "0,1").buildingId];
  assert.ok(world.stock.ore < 6);
  wait(world, 150_000);
  assert.ok(world.totals.delivered.tools >= 2);
  assert.equal(world.stock.ore, 0);
  assert.equal(building.id, world.buildings[building.id].id);
  assert.equal(world.buildings[building.id].status, "blocked");
  assert.match(world.buildings[building.id].reason, /不足/);
  assert.equal(world.rates.tools, 0);
});

test("manual gathering recovers from no construction assets without free teleporting supplies", () => {
  const world = fixture();
  world.stock.wood = 0;
  move(world, "-2,1");
  act(world, "GATHER", { tileId: "-2,1" });
  assert.equal(world.stock.wood, 0);
  wait(world, 6_000);
  assert.deepEqual(world.players.mara.cargo, { resource: "wood", amount: 4 });
  assert.equal(world.stock.wood, 0);
  assert.ok(world.players.mara.path.length > 0);
  assert.throws(() => act(world, "MOVE", { tileId: "-1,1" }), /運搬/);
  wait(world, 6_000);
  assert.equal(world.stock.wood, 4);
  assert.equal(world.players.mara.cargo, null);
  assert.equal(world.players.mara.q, 0);
});

test("metallurgy unlocks new legal investment instead of only incrementing a score", () => {
  const world = fixture();
  build(world, "archive", "0,1");
  world.stock.food = 100;
  wait(world, 80_000);
  assert.ok(world.stock.knowledge >= 8);
  world.stock.tools = 4;
  act(world, "RESEARCH", { techId: "metallurgy" });
  assert.equal(world.research.active, "metallurgy");
  wait(world, 30_000);
  assert.ok(world.research.unlocked.includes("metallurgy"));
  world.stock.wood = 30; world.stock.stone = 30;
  move(world, "1,-1");
  act(world, "ROAD", { tileId: "1,-1" }); wait(world, 4_000);
  move(world, "2,-1");
  const preview = getBuildPreview(world, "mara", "2,-2", "mine");
  assert.equal(preview.allowed, true, preview.reason);
});

test("disconnected facilities explicitly stop production and delivery", () => {
  const world = fixture();
  const building = build(world, "lumbermill", "-1,1");
  // Isolate a completed building to exercise the connectivity invariant.
  tileById(world, "-1,1").q = -7; tileById(world, "-1,1").r = 7;
  wait(world, 1_000);
  assert.equal(world.buildings[building.id].connected, false);
  assert.equal(world.buildings[building.id].status, "blocked");
  assert.match(world.buildings[building.id].reason, /道路/);
  assert.equal(world.rates.wood, 0);
});

test("next actions come from shortages and opportunities, are read-only and never spend", () => {
  const world = fixture();
  const before = JSON.stringify(world);
  const tasks = getCitizenTasks(world, "mara");
  assert.equal(JSON.stringify(world), before);
  assert.ok(tasks.some((task) => task.kind === "explore"));
  assert.ok(tasks.some((task) => task.buildingType === "farm"));
  assert.ok(tasks.some((task) => task.buildingType === "lumbermill"));
  world.stock.wood = 0;
  assert.equal(getCitizenTasks(world, "mara")[0].kind, "gather");
});

test("ambition completion records achievement but does not end free play", () => {
  const world = fixture();
  world.stock.food = 90; world.stock.tools = 20;
  world.tiles.forEach((tile) => { tile.explored = true; });
  world.research.unlocked.push("agriculture");
  wait(world, 1_000);
  assert.equal(world.season.complete, true);
  assert.ok(world.events.some((event) => event.type === "AMBITION_REACHED"));
  assert.doesNotThrow(() => act(world, "MOVE", { tileId: "1,0" }));
});

test("road preview and command share locality, resources, terrain and busy validation", () => {
  const base = fixture();
  const cases = [
    () => {},
    (world) => { world.stock.wood = 0; },
    (world) => { tileById(world, "1,-1").terrain = "water"; },
    (world) => { tileById(world, "1,-1").explored = false; },
    (world) => { tileById(world, "1,-1").road = true; },
    (world) => { world.players.mara.path = ["0,1"]; },
    (world) => { world.players.mara.cargo = { resource: "wood", amount: 4 }; },
    (world) => { world.players.mara.q = -2; world.players.mara.r = 1; }
  ];
  for (const change of cases) {
    const world = structuredClone(base); change(world);
    const snapshot = JSON.stringify(world);
    const preview = getRoadPreview(world, "mara", "1,-1");
    assert.equal(JSON.stringify(world), snapshot, "preview must be read-only");
    if (preview.allowed) {
      act(world, "ROAD", { tileId: "1,-1" });
      assert.equal(base.stock.wood - world.stock.wood, preview.cost.wood);
      assert.equal(world.players.mara.job.durationMs, preview.durationMs);
    } else {
      assert.throws(() => act(world, "ROAD", { tileId: "1,-1" }), { message: preview.reason });
      assert.equal(JSON.stringify(world), snapshot);
    }
  }
});

test("logistics road discount is quoted and charged from the same rule", () => {
  const world = fixture();
  world.research.unlocked.push("logistics"); world.stock.wood = 1; world.stock.stone = 0;
  const preview = getRoadPreview(world, "mara", "1,-1");
  assert.equal(preview.allowed, true);
  assert.deepEqual(preview.cost, { wood: 1 });
  act(world, "ROAD", { tileId: "1,-1" });
  assert.equal(world.stock.wood, 0); assert.equal(world.stock.stone, 0);
  wait(world, preview.durationMs);
  assert.equal(tileById(world, "1,-1").road, true);
});

test("research preview and command have exact matching preconditions and costs", () => {
  const base = fixture();
  build(base, "archive", "0,1");
  base.stock.knowledge = 20; base.stock.food = 50; base.stock.tools = 5;
  const cases = [
    () => {},
    (world) => { world.stock.knowledge = 0; },
    (world) => { world.players.mara.path = ["1,0"]; },
    (world) => { world.players.mara.q = -2; world.players.mara.r = 0; },
    (world) => { world.research.active = "logistics"; },
    (world) => { world.research.unlocked.push("agriculture"); },
    (world) => { world.buildings[tileById(world, "0,1").buildingId].connected = false; }
  ];
  for (const change of cases) {
    const world = structuredClone(base); change(world);
    const snapshot = JSON.stringify(world);
    const preview = getResearchPreview(world, "mara", "agriculture");
    assert.equal(JSON.stringify(world), snapshot);
    if (preview.allowed) {
      act(world, "RESEARCH", { techId: "agriculture" });
      assert.equal(base.stock.knowledge - world.stock.knowledge, preview.cost.knowledge);
      assert.equal(base.stock.food - world.stock.food, preview.cost.food);
      wait(world, preview.durationMs);
      assert.ok(world.research.unlocked.includes("agriculture"));
    } else {
      assert.throws(() => act(world, "RESEARCH", { techId: "agriculture" }), { message: preview.reason });
      assert.equal(JSON.stringify(world), snapshot);
    }
  }
});

test("archive-first investment does not softlock tool production and metallurgy", () => {
  const world = fixture();
  build(world, "archive", "0,1");
  assert.equal(world.stock.tools, 1, "archive leaves fewer than old workshop tool requirement");
  // Recover missing raw construction materials through legal, local work.
  act(world, "GATHER", { tileId: "1,-1" }); wait(world, 6_000);
  act(world, "GATHER", { tileId: "1,-1" }); wait(world, 6_000);
  assert.equal(getBuildPreview(world, "mara", "-1,0", "workshop").allowed, true);
  build(world, "workshop", "-1,0");
  act(world, "GATHER", { tileId: "-1,1" }); wait(world, 6_000);
  wait(world, 55_000);
  assert.ok(world.stock.tools >= 3, "a raw-material forge bootstraps higher technology");
  assert.equal(getResearchPreview(world, "mara", "metallurgy").allowed, true);
  act(world, "RESEARCH", { techId: "metallurgy" }); wait(world, 30_000);
  assert.ok(world.research.unlocked.includes("metallurgy"));
});

test("road-only courier path may leave a non-road building but never shortcuts across unconnected ground", () => {
  const world = fixture();
  assert.equal(tileById(world, "-1,1").road, false);
  assert.deepEqual(findPath(world, "-1,1", "0,0", true), ["0,0"]);
  tileById(world, "-2,1").road = true;
  tileById(world, "-3,1").explored = true;
  const route = findPath(world, "-3,1", "0,0", true);
  assert.ok(route);
  for (const id of route.slice(0, -1)) assert.equal(tileById(world, id).road, true);
  tileById(world, "-2,1").road = false;
  assert.equal(findPath(world, "-3,1", "0,0", true), null);
});

test("watchtower reveals beyond a single free boundary survey without an authored mission", () => {
  const world = fixture();
  const before = world.tiles.filter((tile) => tile.explored).length;
  build(world, "watchtower", "0,1");
  const after = world.tiles.filter((tile) => tile.explored).length;
  assert.ok(after - before > 7);
  assert.equal(world.buildings[tileById(world, "0,1").buildingId].status, "active");
  assert.ok(getCitizenTasks(world, "mara").some((task) => task.kind === "explore"));
});

test("a legal fifteen-minute settlement run expands from raw production into research and industry without grants", () => {
  const world = fixture();
  const until = (predicate, message, limitMs = 180_000) => {
    for (let elapsed = 0; !predicate() && elapsed < limitMs; elapsed += 1_000) wait(world, 1_000);
    assert.ok(predicate(), message);
  };
  const invest = (type, tileId) => {
    until(() => getBuildPreview(world, "mara", tileId, type).allowed, `${type} should become affordable through production`);
    return build(world, type, tileId);
  };
  const research = (techId) => {
    until(() => getResearchPreview(world, "mara", techId).allowed, `${techId} should become possible through delivered materials`);
    const preview = getResearchPreview(world, "mara", techId);
    act(world, "RESEARCH", { techId }); wait(world, preview.durationMs);
  };
  invest("farm", "0,1");
  invest("lumbermill", "-1,1");
  invest("quarry", "1,-1");
  invest("archive", "-1,0");
  invest("workshop", "0,-1");
  research("metallurgy");
  move(world, "1,-1");
  act(world, "ROAD", { tileId: "1,-1" }); wait(world, 4_000);
  invest("mine", "2,-2");
  move(world, "0,0");
  research("agriculture");
  research("logistics");
  move(world, "0,1");
  invest("watchtower", "0,2");
  while (world.totals.explored < 45) {
    const task = getCitizenTasks(world, "mara").find((item) => item.kind === "explore");
    assert.ok(task);
    move(world, task.tileId);
    act(world, "EXPLORE", { tileId: task.tileId }); wait(world, 5_000);
  }
  assert.ok(world.timeMs <= 15 * 60_000, `settlement progression took ${world.timeMs / 1_000}s`);
  wait(world, 15 * 60_000 - world.timeMs);
  assert.equal(world.timeMs, 15 * 60_000);
  assert.ok(world.stock.food >= 75);
  assert.ok(world.stock.tools >= 12);
  assert.ok(world.stock.wood > 0 && world.stock.stone > 0 && world.stock.ore > 0);
  assert.equal(world.research.unlocked.length, 3);
  assert.equal(world.season.complete, true);
  assert.ok(Object.values(world.buildings).every((building) => building.status === "active"));
  assert.ok(Object.values(world.stock).every((value) => Number.isFinite(value) && value >= 0));
});
