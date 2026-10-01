/**
 * Deterministic simulation core for the Wylls vertical slice.
 *
 * The module deliberately contains no rendering, networking, wallet, or LLM
 * code. Given the same JSON state, action, and timestamp it returns the same
 * next JSON state. Every exported transition clones its input.
 */

export const WORLD_SCHEMA = Object.freeze({
  id: "permutation-state.world.v1",
  bounds: Object.freeze({ minX: 0, maxX: 100, minY: 0, maxY: 60 }),
  resources: Object.freeze(["timber", "food", "stone", "ore", "goods", "tools", "rations"]),
  actions: Object.freeze(["JOIN", "MOVE", "GATHER", "DEPOSIT", "BUILD", "CRAFT", "BUY", "SELL", "TRADE", "TALK"]),
  maxCatchupMs: 120_000,
  simulationStepMs: 250,
  worldMinutesPerRealSecond: 2,
  maxPlayers: 8
});

const RESOURCE_KEYS = [...WORLD_SCHEMA.resources];
const RAW_RESOURCE_KEYS = ["timber", "food", "stone", "ore"];
const PROFESSIONS = new Set(["citizen", "forester", "farmer", "mason", "miner", "maker", "engineer", "trader", "warden"]);
const PLAYER_SPEED = 10;
const INTERACTION_MARGIN = 1.6;
const EVENT_LIMIT = 160;
const CLOCK_START_MINUTE = 7 * 60 + 50;

const RECIPES = Object.freeze({
  tools: Object.freeze({ id: "tools", name: "River tools", inputs: { timber: 2, ore: 1 }, outputs: { tools: 1 }, durationMs: 2_500 }),
  rations: Object.freeze({ id: "rations", name: "Travel rations", inputs: { food: 2 }, outputs: { rations: 2 }, durationMs: 1_500 }),
  parts: Object.freeze({ id: "parts", name: "Sluice parts", inputs: { stone: 1, ore: 1 }, outputs: { goods: 1 }, durationMs: 2_000 })
});

export class WorldActionError extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "WorldActionError";
    this.code = code;
    this.details = details;
  }
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(maximum, value));
}

function round(value, places = 4) {
  const scale = 10 ** places;
  return Math.round((value + Number.EPSILON) * scale) / scale;
}

function finiteNumber(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function normalizeNow(nowMs) {
  const parsed = finiteNumber(nowMs, NaN);
  if (!Number.isFinite(parsed)) {
    throw new WorldActionError("INVALID_TIME", "nowMs must be a finite timestamp");
  }
  return Math.max(0, Math.trunc(parsed));
}

function stableHash(value) {
  let hash = 2166136261;
  for (let index = 0; index < String(value).length; index += 1) {
    hash ^= String(value).charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function emptyInventory(overrides = {}) {
  return RESOURCE_KEYS.reduce((inventory, key) => {
    inventory[key] = Math.max(0, finiteNumber(overrides[key], 0));
    return inventory;
  }, {});
}

function inventoryTotal(inventory) {
  return RESOURCE_KEYS.reduce((total, key) => total + finiteNumber(inventory[key], 0), 0);
}

function distanceBetween(a, b) {
  return Math.hypot(finiteNumber(a.x) - finiteNumber(b.x), finiteNumber(a.y) - finiteNumber(b.y));
}

function currentVirtualTime(world) {
  return world.createdAtMs + world.simulationTimeMs;
}

function pushEvent(world, type, message, data = {}, atMs = currentVirtualTime(world)) {
  world.eventSerial += 1;
  world.events.push({
    id: `event-${String(world.eventSerial).padStart(6, "0")}`,
    atMs: Math.trunc(atMs),
    type,
    message,
    data: clone(data)
  });
  if (world.events.length > EVENT_LIMIT) {
    world.events.splice(0, world.events.length - EVENT_LIMIT);
  }
}

function weatherFor(world, day, sixHourBlock) {
  const roll = stableHash(`${world.sessionId}:${day}:${sixHourBlock}`) % 10;
  if (roll <= 3) return "clear";
  if (roll <= 5) return "rain";
  if (roll <= 7) return "wind";
  if (roll === 8) return "mist";
  return "drought";
}

function clockLabel(minuteOfDay) {
  const hours = Math.floor(minuteOfDay / 60) % 24;
  const minutes = Math.floor(minuteOfDay % 60);
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

function updateClock(world, emitWeatherEvent = true) {
  const previousWeather = world.clock && world.clock.weather;
  const elapsedWorldMinutes = Math.floor((world.simulationTimeMs / 1_000) * WORLD_SCHEMA.worldMinutesPerRealSecond);
  const absoluteMinute = CLOCK_START_MINUTE + elapsedWorldMinutes;
  const day = Math.floor(absoluteMinute / 1_440) + 1;
  const minuteOfDay = absoluteMinute % 1_440;
  const sixHourBlock = Math.floor(minuteOfDay / 360);
  const weather = weatherFor(world, day, sixHourBlock);
  world.clock = {
    day,
    minuteOfDay,
    label: clockLabel(minuteOfDay),
    weather,
    phase: minuteOfDay < 360 || minuteOfDay >= 1_200 ? "night" : minuteOfDay < 480 ? "dawn" : minuteOfDay < 1_080 ? "day" : "dusk"
  };
  if (emitWeatherEvent && previousWeather && previousWeather !== weather) {
    pushEvent(world, "WEATHER_CHANGED", `The weather turned ${weather}.`, { weather, day, minuteOfDay });
  }
}

function makePlayer(actorId, name, profession) {
  const seed = stableHash(actorId);
  const x = 44 + (seed % 5) * 0.55;
  const y = 34 + (Math.floor(seed / 5) % 4) * 0.55;
  return {
    id: actorId,
    name,
    kind: "player",
    x: round(x),
    y: round(y),
    target: null,
    speed: PLAYER_SPEED,
    inventory: emptyInventory(),
    capacity: 12,
    profession,
    role: profession,
    job: null,
    status: "idle",
    wallet: { coins: 30 },
    connected: true,
    joinedAtMs: 0,
    lastSeenAtMs: 0
  };
}

function makeNpc({ id, name, profession, x, y, home, workplaceId, speed, workIntervalMs }) {
  return {
    id,
    name,
    kind: "npc",
    x,
    y,
    target: null,
    speed,
    inventory: emptyInventory(),
    capacity: 10,
    profession,
    role: profession,
    job: null,
    status: "off_duty",
    home,
    workplaceId,
    destinationId: null,
    workIntervalMs,
    workAccumulatorMs: 0,
    needs: { hunger: 22, energy: 78, social: 64 },
    cooldowns: { mealMs: 0 },
    relationships: {},
    memories: [],
    lastDialogue: null
  };
}

function initialNpcs() {
  return {
    tala: makeNpc({ id: "tala", name: "Tala Riverwright", profession: "engineer", x: 47, y: 42, home: { x: 47, y: 42 }, workplaceId: "east-sluice", speed: 6.2, workIntervalMs: 30_000 }),
    ivo: makeNpc({ id: "ivo", name: "Ivo Sen", profession: "maker", x: 43, y: 43, home: { x: 43, y: 43 }, workplaceId: "workshop", speed: 5.8, workIntervalMs: 15_000 }),
    bram: makeNpc({ id: "bram", name: "Bram Fen", profession: "farmer", x: 49, y: 45, home: { x: 49, y: 45 }, workplaceId: "farm", speed: 5.4, workIntervalMs: 8_000 }),
    orin: makeNpc({ id: "orin", name: "Orin Stonehand", profession: "mason", x: 51, y: 43, home: { x: 51, y: 43 }, workplaceId: "quarry", speed: 5.2, workIntervalMs: 10_000 }),
    sera: makeNpc({ id: "sera", name: "Sera Vale", profession: "miner", x: 45, y: 46, home: { x: 45, y: 46 }, workplaceId: "ore", speed: 5.5, workIntervalMs: 12_000 }),
    nia: makeNpc({ id: "nia", name: "Nia Quell", profession: "trader", x: 53, y: 44, home: { x: 53, y: 44 }, workplaceId: "market", speed: 6, workIntervalMs: 10_000 })
  };
}

function resourceNode(id, name, type, resource, x, y, amount, maxAmount, regenPerSecond) {
  return { id, name, type, resource, x, y, radius: 3.2, amount, maxAmount, regenPerSecond };
}

function initialNodes() {
  return {
    forest: resourceNode("forest", "Whisperwood Grove", "forest", "timber", 18, 20, 18, 24, 0.045),
    farm: resourceNode("farm", "Sunward Fields", "farm", "food", 31, 12, 16, 24, 0.06),
    quarry: resourceNode("quarry", "Old Granite Cut", "quarry", "stone", 79, 17, 15, 22, 0.032),
    ore: resourceNode("ore", "Starfall Vein", "ore", "ore", 89, 47, 12, 18, 0.024)
  };
}

function initialBuildings() {
  return {
    warehouse: {
      id: "warehouse",
      name: "Civic Warehouse",
      type: "warehouse",
      x: 46,
      y: 31,
      radius: 4,
      status: "operating",
      condition: 92
    },
    workshop: {
      id: "workshop",
      name: "Copperleaf Workshop",
      type: "workshop",
      x: 40,
      y: 28,
      radius: 3.6,
      status: "operating",
      condition: 86,
      cycleMs: 10_000,
      productionAccumulatorMs: 0,
      inventory: emptyInventory({ timber: 3, ore: 2 })
    },
    market: {
      id: "market",
      name: "Lantern Market",
      type: "market",
      x: 56,
      y: 31,
      radius: 4.2,
      status: "open",
      condition: 88
    },
    eastSluice: {
      id: "east-sluice",
      name: "East Sluice",
      type: "waterworks",
      x: 72,
      y: 30,
      radius: 4.4,
      status: "damaged",
      condition: 34,
      completedAtMs: null
    }
  };
}

function initialWorld(sessionId, nowMs) {
  const world = {
    schemaVersion: WORLD_SCHEMA.id,
    sessionId,
    createdAtMs: nowMs,
    lastAdvancedAtMs: nowMs,
    simulationTimeMs: 0,
    tick: 0,
    revision: 0,
    eventSerial: 0,
    bounds: clone(WORLD_SCHEMA.bounds),
    clock: { day: 1, minuteOfDay: CLOCK_START_MINUTE, label: clockLabel(CLOCK_START_MINUTE), weather: "clear", phase: "dawn" },
    season: { id: "aster-foundation", status: "active", age: "Riverwake", victoryPressure: 0 },
    civilization: {
      name: "Aster",
      water: 42,
      food: 58,
      cohesion: 61,
      prosperity: 34,
      waterRatePerMinute: 0.7,
      waterUsePerMinute: 1.1,
      foodUsePerMinute: 0.55,
      farmProductionMultiplier: 0.85,
      treasury: 180,
      prizePool: 0
    },
    players: {},
    npcs: initialNpcs(),
    nodes: initialNodes(),
    warehouse: {
      id: "warehouse",
      name: "Civic Warehouse",
      x: 46,
      y: 31,
      radius: 4,
      capacity: 400,
      stocks: emptyInventory({ food: 18, ore: 4, goods: 2 })
    },
    buildings: initialBuildings(),
    projects: {
      eastSluice: {
        id: "east-sluice",
        name: "Repair the East Sluice",
        x: 72,
        y: 30,
        radius: 4.4,
        status: "repairing",
        progress: 0,
        labor: 0,
        laborRequired: 100,
        buildCost: { timber: 2, stone: 1 },
        requirements: { timber: 8, stone: 4 },
        materialsUsed: { timber: 0, stone: 0, ore: 0 },
        contributors: {}
      }
    },
    market: {
      id: "market",
      x: 56,
      y: 31,
      radius: 4.2,
      treasury: 240,
      feeRate: 0.03,
      stocks: emptyInventory({ timber: 4, food: 8, stone: 3, ore: 2, goods: 3, tools: 2, rations: 4 }),
      basePrices: { timber: 4, food: 3, stone: 5, ore: 8, goods: 12, tools: 16, rations: 5 },
      desiredStocks: { timber: 14, food: 20, stone: 12, ore: 9, goods: 8, tools: 6, rations: 12 },
      prices: {}
    },
    events: [],
    lastAction: null
  };
  updateClock(world, false);
  updateMarketPrices(world);
  Object.values(world.npcs).forEach((npc) => assignNpcDestination(world, npc));
  pushEvent(world, "WORLD_CREATED", "Aster wakes at Riverwake dawn.", { sessionId });
  return world;
}

export function createWorld({ sessionId = "aster-demo", nowMs = 0 } = {}) {
  const normalizedNow = normalizeNow(nowMs);
  const normalizedSessionId = String(sessionId || "aster-demo").slice(0, 80);
  return initialWorld(normalizedSessionId, normalizedNow);
}

function assertWorld(world) {
  if (!world || typeof world !== "object" || world.schemaVersion !== WORLD_SCHEMA.id) {
    throw new WorldActionError("INVALID_WORLD", `Expected world schema ${WORLD_SCHEMA.id}`);
  }
}

function getLocation(world, locationId) {
  if (!locationId) return null;
  if (locationId === "warehouse") return world.warehouse;
  if (locationId === "market") return world.market;
  if (locationId === "east-sluice") return world.projects.eastSluice;
  return world.nodes[locationId] || world.buildings[locationId] || null;
}

function desiredNpcDestination(world, npc) {
  const minute = world.clock.minuteOfDay;
  const isWorkShift = minute >= 480 && minute < 1_020;
  const resourceByNpc = { bram: "food", orin: "stone", sera: "ore" };
  const carriedResource = resourceByNpc[npc.id];

  // Cargo has a physical destination. A citizen carrying goods finishes the
  // delivery even if the work bell has already rung for the day.
  if (carriedResource && npc.inventory[carriedResource] > 0) {
    const node = world.nodes[npc.workplaceId];
    if (!isWorkShift || inventoryTotal(npc.inventory) >= 4 || Math.floor(node.amount) < 1) {
      return { id: "warehouse", x: world.warehouse.x, y: world.warehouse.y };
    }
  }
  if (npc.id === "ivo" && npc.inventory.goods > 0) {
    return { id: "market", x: world.market.x, y: world.market.y };
  }
  if (npc.id === "tala" && npc.inventory.timber >= 2 && npc.inventory.stone >= 1) {
    const project = world.projects.eastSluice;
    return { id: project.id, x: project.x, y: project.y };
  }

  if (npc.needs.energy < 18) return { id: `home:${npc.id}`, ...npc.home };
  if (npc.needs.hunger > 82) return { id: "market", x: world.market.x, y: world.market.y };
  if (minute < 360 || minute >= 1_200) return { id: `home:${npc.id}`, ...npc.home };
  if (minute < 480) return { id: "market", x: world.market.x, y: world.market.y };
  if (isWorkShift) {
    if (npc.id === "tala" && world.projects.eastSluice.status !== "complete") {
      return { id: "warehouse", x: world.warehouse.x, y: world.warehouse.y };
    }
    const workplace = getLocation(world, npc.workplaceId);
    return { id: npc.workplaceId, x: workplace.x, y: workplace.y };
  }
  if (minute < 1_140) return { id: "market", x: world.market.x, y: world.market.y };
  return { id: `home:${npc.id}`, ...npc.home };
}

function assignNpcDestination(world, npc) {
  const destination = desiredNpcDestination(world, npc);
  if (npc.destinationId !== destination.id) {
    npc.destinationId = destination.id;
    npc.target = { x: destination.x, y: destination.y };
  } else if (!npc.target && distanceBetween(npc, destination) > 0.05) {
    npc.target = { x: destination.x, y: destination.y };
  }
}

function moveActor(actor, dtSeconds) {
  if (!actor.target) return;
  const dx = actor.target.x - actor.x;
  const dy = actor.target.y - actor.y;
  const distance = Math.hypot(dx, dy);
  const travel = Math.max(0, actor.speed * dtSeconds);
  if (distance <= travel || distance < 0.0001) {
    actor.x = actor.target.x;
    actor.y = actor.target.y;
    actor.target = null;
    actor.status = actor.kind === "npc" ? "arrived" : "idle";
    return;
  }
  actor.x += (dx / distance) * travel;
  actor.y += (dy / distance) * travel;
  actor.status = "moving";
}

function weatherRegenFactor(weather) {
  if (weather === "rain") return 1.55;
  if (weather === "drought") return 0.48;
  if (weather === "mist") return 1.12;
  return 1;
}

function updateResourceNodes(world, dtSeconds) {
  const factor = weatherRegenFactor(world.clock.weather);
  Object.values(world.nodes).forEach((node) => {
    const irrigation = node.resource === "food"
      ? finiteNumber(world.civilization.farmProductionMultiplier, 1)
      : 1;
    const localFactor = node.resource === "food" && world.clock.weather === "drought"
      ? 0.35 * irrigation
      : factor * irrigation;
    node.amount = Math.min(node.maxAmount, node.amount + node.regenPerSecond * localFactor * dtSeconds);
  });
}

function updateCivilization(world, dtSeconds) {
  const sluiceComplete = world.projects.eastSluice.status === "complete";
  const rainBonus = world.clock.weather === "rain" ? 0.7 : 0;
  const droughtPenalty = world.clock.weather === "drought" ? 0.35 : 0;
  world.civilization.waterRatePerMinute = (sluiceComplete ? 4.8 : 0.7) + rainBonus - droughtPenalty;
  world.civilization.farmProductionMultiplier = sluiceComplete
    ? (world.clock.weather === "drought" ? 1.35 : 1.8)
    : clamp(world.civilization.water / 50, 0.45, 1);
  const netWaterPerMinute = world.civilization.waterRatePerMinute - world.civilization.waterUsePerMinute;
  world.civilization.water = clamp(world.civilization.water + netWaterPerMinute * (dtSeconds / 60), 0, 100);
  world.civilization.food = clamp(world.civilization.food - world.civilization.foodUsePerMinute * (dtSeconds / 60), 0, 100);

  if (world.civilization.water < 18 || world.civilization.food < 18) {
    world.civilization.cohesion = clamp(world.civilization.cohesion - 0.18 * dtSeconds / 60, 0, 100);
  } else if (world.civilization.water > 55 && world.civilization.food > 45) {
    world.civilization.cohesion = clamp(world.civilization.cohesion + 0.05 * dtSeconds / 60, 0, 100);
  }
}

function updateBuildingProduction(world, dtMs) {
  const workshop = world.buildings.workshop;
  if (workshop.status !== "operating") return;
  workshop.productionAccumulatorMs += dtMs;
  while (workshop.productionAccumulatorMs >= workshop.cycleMs) {
    workshop.productionAccumulatorMs -= workshop.cycleMs;
    if (workshop.inventory.timber < 1 || workshop.inventory.ore < 1) break;
    workshop.inventory.timber -= 1;
    workshop.inventory.ore -= 1;
    workshop.inventory.goods += 1;
    world.civilization.prosperity = clamp(world.civilization.prosperity + 0.35, 0, 100);
    pushEvent(world, "WORKSHOP_PRODUCED", "Copperleaf Workshop completed a crate of mechanisms.", { item: "goods", quantity: 1 });
  }
}

function isNear(actor, target, margin = INTERACTION_MARGIN) {
  return distanceBetween(actor, target) <= finiteNumber(target.radius, 0) + margin;
}

function applyProjectLabor(world, actorId, labor) {
  const project = world.projects.eastSluice;
  if (project.status === "complete") return;
  project.labor = Math.min(project.laborRequired, project.labor + labor);
  project.progress = round((project.labor / project.laborRequired) * 100, 2);
  project.contributors[actorId] = round(finiteNumber(project.contributors[actorId], 0) + labor, 2);
  if (project.labor >= project.laborRequired) {
    project.status = "complete";
    project.progress = 100;
    world.buildings.eastSluice.status = "operating";
    world.buildings.eastSluice.condition = 100;
    world.buildings.eastSluice.completedAtMs = currentVirtualTime(world);
    world.civilization.cohesion = clamp(world.civilization.cohesion + 5, 0, 100);
    world.civilization.prosperity = clamp(world.civilization.prosperity + 7, 0, 100);
    world.civilization.waterRatePerMinute = 4.8;
    pushEvent(world, "PROJECT_COMPLETED", "The East Sluice opened. Fresh water now reaches Aster continuously.", { projectId: project.id });
  }
}

function npcAtDestination(world, npc) {
  const destination = desiredNpcDestination(world, npc);
  return distanceBetween(npc, destination) <= 0.9;
}

function resolveNpcLogistics(world, npc) {
  const resourceByNpc = { bram: "food", orin: "stone", sera: "ore" };
  const carriedResource = resourceByNpc[npc.id];

  if (carriedResource && npc.destinationId === "warehouse" && distanceBetween(npc, world.warehouse) <= 0.9) {
    const quantity = Math.floor(npc.inventory[carriedResource]);
    if (quantity > 0) {
      npc.inventory[carriedResource] -= quantity;
      world.warehouse.stocks[carriedResource] += quantity;
      if (carriedResource === "food") {
        world.civilization.food = clamp(world.civilization.food + quantity * 0.45, 0, 100);
      }
      npc.status = "delivering";
      npc.workAccumulatorMs = 0;
      pushEvent(world, "NPC_DELIVERED", `${npc.name} delivered ${quantity} ${carriedResource} to the Civic Warehouse.`, {
        npcId: npc.id,
        item: carriedResource,
        quantity
      });
    }
  }

  if (npc.id === "ivo" && npc.destinationId === "market" && distanceBetween(npc, world.market) <= 0.9) {
    const quantity = Math.floor(npc.inventory.goods);
    if (quantity > 0) {
      npc.inventory.goods -= quantity;
      world.market.stocks.goods += quantity;
      npc.status = "delivering";
      npc.workAccumulatorMs = 0;
      pushEvent(world, "NPC_DELIVERED", `${npc.name} carried ${quantity} mechanism crate to Lantern Market.`, {
        npcId: npc.id,
        item: "goods",
        quantity
      });
    }
  }

  if (npc.id === "tala" && npc.destinationId === "warehouse" && distanceBetween(npc, world.warehouse) <= 0.9) {
    const project = world.projects.eastSluice;
    if (
      project.status !== "complete"
      && npc.inventory.timber < project.buildCost.timber
      && npc.inventory.stone < project.buildCost.stone
      && world.warehouse.stocks.timber >= project.buildCost.timber
      && world.warehouse.stocks.stone >= project.buildCost.stone
    ) {
      world.warehouse.stocks.timber -= project.buildCost.timber;
      world.warehouse.stocks.stone -= project.buildCost.stone;
      npc.inventory.timber += project.buildCost.timber;
      npc.inventory.stone += project.buildCost.stone;
      npc.status = "hauling repair supplies";
      npc.workAccumulatorMs = 0;
      pushEvent(world, "NPC_PICKED_UP", `${npc.name} loaded repair supplies for the East Sluice.`, {
        npcId: npc.id,
        destinationId: project.id,
        timber: project.buildCost.timber,
        stone: project.buildCost.stone
      });
    } else if (project.status !== "complete" && npc.inventory.timber === 0 && npc.inventory.stone === 0) {
      npc.status = "waiting for repair supplies";
    }
  }
}

function performNpcWork(world, npc) {
  if (npc.id === "tala") {
    const project = world.projects.eastSluice;
    if (
      project.status !== "complete"
      && npc.inventory.timber >= project.buildCost.timber
      && npc.inventory.stone >= project.buildCost.stone
    ) {
      npc.inventory.timber -= project.buildCost.timber;
      npc.inventory.stone -= project.buildCost.stone;
      project.materialsUsed.timber += project.buildCost.timber;
      project.materialsUsed.stone += project.buildCost.stone;
      applyProjectLabor(world, npc.id, 25);
      pushEvent(world, "NPC_WORK", "Tala fitted a hauled brace to the East Sluice.", {
        npcId: npc.id,
        projectId: project.id,
        labor: 25,
        timber: project.buildCost.timber,
        stone: project.buildCost.stone
      });
    }
    return;
  }

  if (npc.id === "ivo") {
    const workshop = world.buildings.workshop;
    if (workshop.inventory.goods >= 1 && inventoryTotal(npc.inventory) < npc.capacity) {
      workshop.inventory.goods -= 1;
      npc.inventory.goods += 1;
      pushEvent(world, "NPC_PICKED_UP", "Ivo packed a mechanism crate for Lantern Market.", {
        npcId: npc.id,
        item: "goods",
        quantity: 1,
        destinationId: "market"
      });
    }
    return;
  }

  if (npc.id === "nia") {
    world.market.treasury = round(world.market.treasury + 0.5, 2);
    world.civilization.prosperity = clamp(world.civilization.prosperity + 0.08, 0, 100);
    return;
  }

  const resourceByNpc = { bram: "food", orin: "stone", sera: "ore" };
  const item = resourceByNpc[npc.id];
  if (!item) return;
  const node = world.nodes[npc.workplaceId];
  const freeCapacity = Math.max(0, npc.capacity - inventoryTotal(npc.inventory));
  const irrigationYield = item === "food" && world.civilization.farmProductionMultiplier >= 1.5 ? 2 : 1;
  const quantity = Math.max(0, Math.min(irrigationYield, Math.floor(node.amount), Math.floor(freeCapacity)));
  if (quantity < 1) {
    npc.status = freeCapacity < 1 ? "pack full" : "waiting for resources";
    return;
  }
  node.amount -= quantity;
  npc.inventory[item] += quantity;
  pushEvent(world, "NPC_GATHERED", `${npc.name} gathered ${quantity} ${item}.`, {
    npcId: npc.id,
    nodeId: node.id,
    item,
    quantity
  });
}

function updateNpc(world, npc, dtMs) {
  const dtSeconds = dtMs / 1_000;
  npc.cooldowns.mealMs = Math.max(0, npc.cooldowns.mealMs - dtMs);
  npc.needs.hunger = clamp(npc.needs.hunger + 0.12 * dtSeconds, 0, 100);
  npc.needs.social = clamp(npc.needs.social - 0.035 * dtSeconds, 0, 100);
  assignNpcDestination(world, npc);
  moveActor(npc, dtSeconds);
  resolveNpcLogistics(world, npc);

  const atHome = distanceBetween(npc, npc.home) <= 1;
  if (atHome) {
    npc.needs.energy = clamp(npc.needs.energy + 0.5 * dtSeconds, 0, 100);
    npc.status = npc.target ? npc.status : "resting";
  } else {
    npc.needs.energy = clamp(npc.needs.energy - 0.065 * dtSeconds, 0, 100);
  }

  if (npc.destinationId === "market" && distanceBetween(npc, world.market) <= world.market.radius && npc.needs.hunger > 56 && npc.cooldowns.mealMs <= 0) {
    if (world.market.stocks.food >= 1) {
      world.market.stocks.food -= 1;
      npc.needs.hunger = clamp(npc.needs.hunger - 46, 0, 100);
      npc.needs.social = clamp(npc.needs.social + 18, 0, 100);
      npc.cooldowns.mealMs = 45_000;
      npc.status = "eating";
    }
  }

  const isWorkShift = world.clock.minuteOfDay >= 480 && world.clock.minuteOfDay < 1_020;
  if (isWorkShift && npc.destinationId === npc.workplaceId && npcAtDestination(world, npc)) {
    npc.status = "working";
    npc.workAccumulatorMs += dtMs;
    while (npc.workAccumulatorMs >= npc.workIntervalMs) {
      npc.workAccumulatorMs -= npc.workIntervalMs;
      performNpcWork(world, npc);
    }
  }
}

function resolvePlayerJob(world, player) {
  const job = player.job;
  if (!job) return;
  if (job.type === "gather") {
    const node = world.nodes[job.targetId];
    const freeCapacity = Math.max(0, player.capacity - inventoryTotal(player.inventory));
    const quantity = Math.max(0, Math.min(job.yield, Math.floor(node.amount), Math.floor(freeCapacity)));
    if (quantity > 0) {
      node.amount -= quantity;
      player.inventory[node.resource] += quantity;
      pushEvent(world, "PLAYER_GATHERED", `${player.name} gathered ${quantity} ${node.resource}.`, { actorId: player.id, nodeId: node.id, item: node.resource, quantity });
    } else {
      pushEvent(world, "JOB_FAILED", `${player.name} could not gather from ${node.name}.`, { actorId: player.id, jobType: job.type, targetId: node.id });
    }
  } else if (job.type === "build") {
    applyProjectLabor(world, player.id, job.labor);
    pushEvent(world, "PLAYER_BUILT", `${player.name} advanced the East Sluice repairs.`, { actorId: player.id, projectId: job.targetId, labor: job.labor });
  } else if (job.type === "craft") {
    Object.entries(job.outputs).forEach(([item, quantity]) => {
      player.inventory[item] += quantity;
    });
    pushEvent(world, "PLAYER_CRAFTED", `${player.name} completed ${job.recipeName}.`, { actorId: player.id, recipeId: job.recipeId, outputs: job.outputs });
  }
  player.job = null;
  player.status = "idle";
}

function updatePlayer(world, player, dtMs) {
  if (player.job) {
    player.job.remainingMs -= dtMs;
    player.status = player.job.type;
    if (player.job.remainingMs <= 0) resolvePlayerJob(world, player);
    return;
  }
  moveActor(player, dtMs / 1_000);
}

function updateMarketPrices(world) {
  Object.keys(world.market.basePrices).forEach((item) => {
    const available = finiteNumber(world.market.stocks[item], 0) + finiteNumber(world.warehouse.stocks[item], 0) * 0.35;
    const desired = world.market.desiredStocks[item];
    let pressure = desired / Math.max(1, available);
    if (item === "food") pressure *= 1 + Math.max(0, 45 - world.civilization.food) / 80;
    if (item === "goods" || item === "tools") pressure *= 1 + world.civilization.prosperity / 350;
    world.market.prices[item] = round(world.market.basePrices[item] * clamp(pressure, 0.55, 2.5), 2);
  });
}

function simulateStep(world, dtMs) {
  world.simulationTimeMs += dtMs;
  world.tick = Math.floor(world.simulationTimeMs / WORLD_SCHEMA.simulationStepMs);
  updateClock(world, true);
  const dtSeconds = dtMs / 1_000;
  updateResourceNodes(world, dtSeconds);
  updateCivilization(world, dtSeconds);
  Object.values(world.players).forEach((player) => updatePlayer(world, player, dtMs));
  Object.values(world.npcs).forEach((npc) => updateNpc(world, npc, dtMs));
  updateBuildingProduction(world, dtMs);
}

function roundWorldNumbers(world) {
  Object.values(world.players).forEach((actor) => {
    actor.x = round(actor.x);
    actor.y = round(actor.y);
    if (actor.target) {
      actor.target.x = round(actor.target.x);
      actor.target.y = round(actor.target.y);
    }
    actor.wallet.coins = round(actor.wallet.coins, 2);
  });
  Object.values(world.npcs).forEach((actor) => {
    actor.x = round(actor.x);
    actor.y = round(actor.y);
    Object.keys(actor.needs).forEach((key) => { actor.needs[key] = round(actor.needs[key], 3); });
  });
  Object.values(world.nodes).forEach((node) => { node.amount = round(node.amount, 4); });
  ["water", "food", "cohesion", "prosperity", "waterRatePerMinute", "farmProductionMultiplier"].forEach((key) => {
    world.civilization[key] = round(world.civilization[key], 4);
  });
}

export function advanceWorld(worldInput, nowMs) {
  assertWorld(worldInput);
  const now = normalizeNow(nowMs);
  const world = clone(worldInput);
  if (now <= world.lastAdvancedAtMs) return world;

  const requestedDtMs = now - world.lastAdvancedAtMs;
  const effectiveDtMs = Math.min(requestedDtMs, WORLD_SCHEMA.maxCatchupMs);
  let remainingMs = effectiveDtMs;
  while (remainingMs > 0) {
    const stepMs = Math.min(WORLD_SCHEMA.simulationStepMs, remainingMs);
    simulateStep(world, stepMs);
    remainingMs -= stepMs;
  }
  world.lastAdvancedAtMs = now;
  if (requestedDtMs > effectiveDtMs) {
    pushEvent(world, "CATCHUP_CLAMPED", "Offline catch-up was capped to protect the shared simulation.", { requestedDtMs, simulatedDtMs: effectiveDtMs }, now);
  }
  updateMarketPrices(world);
  roundWorldNumbers(world);
  world.revision += 1;
  return world;
}

function requirePlayer(world, actorId) {
  const player = world.players[actorId];
  if (!player) throw new WorldActionError("UNKNOWN_ACTOR", `Player ${actorId || "(missing)"} has not joined`, { actorId });
  return player;
}

function requireIdle(player) {
  if (player.job) {
    throw new WorldActionError("ACTOR_BUSY", `${player.name} is already ${player.job.type}`, { actorId: player.id, job: player.job.type });
  }
}

function requireNear(player, target, label) {
  if (!isNear(player, target)) {
    throw new WorldActionError("TOO_FAR", `${player.name} is too far from ${label}`, {
      actorId: player.id,
      targetId: target.id,
      distance: round(distanceBetween(player, target), 2)
    });
  }
}

function validateActorId(value) {
  const actorId = String(value || "").trim();
  if (!/^[a-zA-Z0-9:_-]{1,64}$/.test(actorId)) {
    throw new WorldActionError("INVALID_ACTOR_ID", "actorId must be 1-64 letters, numbers, colons, underscores, or dashes");
  }
  return actorId;
}

function normalizeActionType(action) {
  return String(action && action.type || "").trim().toUpperCase();
}

function startJob(world, player, job) {
  player.target = null;
  player.job = {
    ...job,
    startedAtMs: currentVirtualTime(world),
    remainingMs: job.durationMs
  };
  player.status = job.type;
}

function joinAction(world, action, now) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const requestedName = String(action.name || actorId).trim().slice(0, 32) || actorId;
  const requestedProfession = String(action.profession || action.role || "citizen").toLowerCase();
  const profession = PROFESSIONS.has(requestedProfession) ? requestedProfession : "citizen";
  if (world.players[actorId]) {
    world.players[actorId].connected = true;
    world.players[actorId].lastSeenAtMs = now;
    if (action.name) world.players[actorId].name = requestedName;
    return { actorId, rejoined: true };
  }
  if (Object.keys(world.players).length >= WORLD_SCHEMA.maxPlayers) {
    throw new WorldActionError("WORLD_FULL", `Aster's current district supports ${WORLD_SCHEMA.maxPlayers} human citizens`, {
      maxPlayers: WORLD_SCHEMA.maxPlayers
    });
  }
  const player = makePlayer(actorId, requestedName, profession);
  player.joinedAtMs = now;
  player.lastSeenAtMs = now;
  world.players[actorId] = player;
  pushEvent(world, "PLAYER_JOINED", `${player.name} entered Aster.`, { actorId, profession });
  return { actorId, rejoined: false };
}

function moveAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  const x = finiteNumber(action.x, NaN);
  const y = finiteNumber(action.y, NaN);
  if (!Number.isFinite(x) || !Number.isFinite(y)) throw new WorldActionError("INVALID_DESTINATION", "MOVE requires finite x and y coordinates");
  player.target = {
    x: clamp(x, WORLD_SCHEMA.bounds.minX, WORLD_SCHEMA.bounds.maxX),
    y: clamp(y, WORLD_SCHEMA.bounds.minY, WORLD_SCHEMA.bounds.maxY)
  };
  player.status = "moving";
  return { actorId, target: clone(player.target) };
}

function gatherAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  const nodeId = String(action.nodeId || action.resourceId || action.targetId || "");
  const node = world.nodes[nodeId];
  if (!node) throw new WorldActionError("UNKNOWN_NODE", `Unknown resource node ${nodeId || "(missing)"}`, { nodeId });
  requireNear(player, node, node.name);
  if (Math.floor(node.amount) < 1) throw new WorldActionError("NODE_DEPLETED", `${node.name} is temporarily depleted`, { nodeId });
  const freeCapacity = player.capacity - inventoryTotal(player.inventory);
  if (freeCapacity < 1) throw new WorldActionError("INVENTORY_FULL", `${player.name}'s pack is full`, { actorId });
  const specialist = { timber: "forester", food: "farmer", stone: "mason", ore: "miner" }[node.resource];
  const isSpecialist = player.profession === specialist;
  startJob(world, player, {
    type: "gather",
    targetId: nodeId,
    durationMs: isSpecialist ? 1_500 : 2_400,
    yield: Math.min(4, Math.floor(freeCapacity))
  });
  return { actorId, job: "gather", nodeId };
}

function depositAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  requireNear(player, world.warehouse, world.warehouse.name);
  const requestedItem = action.item ? String(action.item).toLowerCase() : null;
  if (requestedItem && !RESOURCE_KEYS.includes(requestedItem)) throw new WorldActionError("UNKNOWN_ITEM", `Unknown item ${requestedItem}`);
  const transfers = {};
  const keys = requestedItem ? [requestedItem] : RESOURCE_KEYS;
  let remainingRequested = action.quantity == null ? Infinity : Math.max(0, Math.floor(finiteNumber(action.quantity, 0)));
  keys.forEach((item) => {
    if (remainingRequested <= 0) return;
    const quantity = Math.min(player.inventory[item], remainingRequested);
    if (quantity > 0) {
      player.inventory[item] -= quantity;
      world.warehouse.stocks[item] += quantity;
      transfers[item] = quantity;
      remainingRequested -= quantity;
      if (item === "food") world.civilization.food = clamp(world.civilization.food + quantity * 0.3, 0, 100);
    }
  });
  if (Object.keys(transfers).length === 0) throw new WorldActionError("NOTHING_TO_DEPOSIT", `${player.name} has nothing to deposit`);
  world.civilization.cohesion = clamp(world.civilization.cohesion + 0.08, 0, 100);
  pushEvent(world, "PLAYER_DEPOSITED", `${player.name} supplied the Civic Warehouse.`, { actorId, transfers });
  return { actorId, transfers };
}

function buildAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  const requestedId = String(action.projectId || action.buildingId || action.targetId || "east-sluice");
  const project = requestedId === "east-sluice" || requestedId === "eastSluice" ? world.projects.eastSluice : null;
  if (!project) throw new WorldActionError("UNKNOWN_PROJECT", `Unknown project ${requestedId}`);
  if (project.status === "complete") throw new WorldActionError("PROJECT_COMPLETE", `${project.name} is already complete`);
  requireNear(player, project, project.name);
  Object.entries(project.buildCost).forEach(([item, quantity]) => {
    if (world.warehouse.stocks[item] < quantity) {
      throw new WorldActionError("MISSING_MATERIALS", `${project.name} needs ${quantity} ${item} in the Civic Warehouse`, { item, required: quantity, available: world.warehouse.stocks[item] });
    }
  });
  Object.entries(project.buildCost).forEach(([item, quantity]) => {
    world.warehouse.stocks[item] -= quantity;
    project.materialsUsed[item] = finiteNumber(project.materialsUsed[item], 0) + quantity;
  });
  const specialist = player.profession === "engineer" || player.profession === "maker";
  startJob(world, player, {
    type: "build",
    targetId: project.id,
    durationMs: specialist ? 2_200 : 3_500,
    labor: 25,
    reservedMaterials: clone(project.buildCost)
  });
  return { actorId, job: "build", projectId: project.id };
}

function craftAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  requireNear(player, world.buildings.workshop, world.buildings.workshop.name);
  const recipeId = String(action.recipeId || "tools").toLowerCase();
  const recipe = RECIPES[recipeId];
  if (!recipe) throw new WorldActionError("UNKNOWN_RECIPE", `Unknown recipe ${recipeId}`);
  Object.entries(recipe.inputs).forEach(([item, quantity]) => {
    if (player.inventory[item] < quantity) {
      throw new WorldActionError("MISSING_MATERIALS", `${recipe.name} needs ${quantity} ${item} in your pack`, { item, required: quantity, available: player.inventory[item] });
    }
  });
  const inputCount = Object.values(recipe.inputs).reduce((sum, value) => sum + value, 0);
  const outputCount = Object.values(recipe.outputs).reduce((sum, value) => sum + value, 0);
  if (inventoryTotal(player.inventory) - inputCount + outputCount > player.capacity) {
    throw new WorldActionError("INVENTORY_FULL", `${player.name}'s pack cannot hold the crafted items`);
  }
  Object.entries(recipe.inputs).forEach(([item, quantity]) => { player.inventory[item] -= quantity; });
  startJob(world, player, {
    type: "craft",
    targetId: "workshop",
    recipeId,
    recipeName: recipe.name,
    durationMs: player.profession === "maker" ? Math.round(recipe.durationMs * 0.7) : recipe.durationMs,
    outputs: clone(recipe.outputs),
    reservedMaterials: clone(recipe.inputs)
  });
  return { actorId, job: "craft", recipeId };
}

function requireMarketItem(world, item) {
  if (!Object.prototype.hasOwnProperty.call(world.market.prices, item)) {
    throw new WorldActionError("UNKNOWN_ITEM", `Lantern Market does not trade ${item || "(missing)"}`, { item });
  }
}

function buyAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  requireNear(player, world.market, "Lantern Market");
  const item = String(action.item || "").toLowerCase();
  requireMarketItem(world, item);
  const quantity = clamp(Math.floor(finiteNumber(action.quantity, 1)), 1, 10);
  if (world.market.stocks[item] < quantity) throw new WorldActionError("OUT_OF_STOCK", `Lantern Market has insufficient ${item}`, { item, quantity });
  if (inventoryTotal(player.inventory) + quantity > player.capacity) throw new WorldActionError("INVENTORY_FULL", `${player.name}'s pack is full`);
  const subtotal = round(world.market.prices[item] * quantity, 2);
  const fee = round(subtotal * world.market.feeRate, 2);
  const total = round(subtotal + fee, 2);
  if (player.wallet.coins < total) throw new WorldActionError("INSUFFICIENT_FUNDS", `${player.name} needs ${total} coins`, { total, balance: player.wallet.coins });
  player.wallet.coins -= total;
  player.inventory[item] += quantity;
  world.market.stocks[item] -= quantity;
  world.market.treasury += subtotal;
  world.civilization.prizePool += fee;
  pushEvent(world, "MARKET_BUY", `${player.name} bought ${quantity} ${item}.`, { actorId, item, quantity, total, fee });
  return { actorId, side: "buy", item, quantity, total, fee };
}

function sellAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  requireNear(player, world.market, "Lantern Market");
  const item = String(action.item || "").toLowerCase();
  requireMarketItem(world, item);
  const quantity = clamp(Math.floor(finiteNumber(action.quantity, 1)), 1, 10);
  if (player.inventory[item] < quantity) throw new WorldActionError("MISSING_ITEM", `${player.name} does not carry ${quantity} ${item}`, { item, quantity });
  const gross = round(world.market.prices[item] * quantity * 0.82, 2);
  const fee = round(gross * world.market.feeRate, 2);
  const payout = round(gross - fee, 2);
  if (world.market.treasury < gross) throw new WorldActionError("MARKET_ILLIQUID", "Lantern Market cannot settle that sale yet", { gross });
  player.inventory[item] -= quantity;
  player.wallet.coins += payout;
  world.market.stocks[item] += quantity;
  world.market.treasury -= gross;
  world.civilization.prizePool += fee;
  pushEvent(world, "MARKET_SELL", `${player.name} sold ${quantity} ${item}.`, { actorId, item, quantity, payout, fee });
  return { actorId, side: "sell", item, quantity, payout, fee };
}

function dialogueFor(world, npc) {
  if (npc.id === "tala") {
    if (world.projects.eastSluice.status === "complete") return { key: "sluice-open", text: "Hear that? The east channel is carrying water again." };
    if (world.warehouse.stocks.timber < 2) return { key: "need-timber", text: "Bring timber to the warehouse. Stone alone will not hold the gate." };
    return { key: "repair-ready", text: "The braces are ready. Meet me at the sluice and lend your hands." };
  }
  if (npc.id === "bram") return world.clock.weather === "drought"
    ? { key: "dry-fields", text: "The field is thirsty. If the sluice fails, the next harvest follows." }
    : { key: "field-shift", text: "Food is not a number. Someone has to bring it home." };
  if (npc.id === "ivo") return { key: "workshop", text: "Raw ore becomes tools only when somebody carries it to a bench." };
  if (npc.id === "orin") return { key: "quarry", text: "The old cut still has stone, but the walk costs daylight." };
  if (npc.id === "sera") return { key: "ore", text: "Starfall ore buys prosperity. It also buys trouble." };
  return { key: "market", text: `Lantern Market is paying ${world.market.prices.food} for food today.` };
}

function talkAction(world, action) {
  const actorId = validateActorId(action.actorId || action.playerId);
  const player = requirePlayer(world, actorId);
  requireIdle(player);
  const npcId = String(action.npcId || action.targetId || "");
  const npc = world.npcs[npcId];
  if (!npc) throw new WorldActionError("UNKNOWN_NPC", `Unknown citizen ${npcId || "(missing)"}`, { npcId });
  requireNear(player, { ...npc, radius: 1.2 }, npc.name);
  const dialogue = dialogueFor(world, npc);
  npc.relationships[actorId] = clamp(finiteNumber(npc.relationships[actorId], 0) + 1, -100, 100);
  npc.needs.social = clamp(npc.needs.social + 8, 0, 100);
  npc.lastDialogue = { actorId, key: dialogue.key, text: dialogue.text, atMs: currentVirtualTime(world) };
  npc.memories.push({ actorId, key: dialogue.key, atMs: currentVirtualTime(world) });
  if (npc.memories.length > 12) npc.memories.splice(0, npc.memories.length - 12);
  pushEvent(world, "NPC_CONVERSATION", `${player.name} spoke with ${npc.name}.`, { actorId, npcId, dialogueKey: dialogue.key, text: dialogue.text });
  return { actorId, npcId, dialogue };
}

function inferInteractionAction(world, action) {
  const targetId = String(action.targetId || "");
  if (world.nodes[targetId]) return { ...action, type: "GATHER", nodeId: targetId };
  if (world.npcs[targetId]) return { ...action, type: "TALK", npcId: targetId };
  if (targetId === "warehouse") return { ...action, type: "DEPOSIT" };
  if (targetId === "east-sluice") return { ...action, type: "BUILD", projectId: targetId };
  if (targetId === "workshop") return { ...action, type: "CRAFT", recipeId: action.recipeId || "tools" };
  throw new WorldActionError("UNKNOWN_TARGET", `No interaction is available for ${targetId || "(missing)"}`, { targetId });
}

export function applyWorldAction(worldInput, actionInput, nowMs) {
  assertWorld(worldInput);
  if (!actionInput || typeof actionInput !== "object") throw new WorldActionError("INVALID_ACTION", "An action object is required");
  const now = normalizeNow(nowMs);
  const world = advanceWorld(worldInput, now);
  let action = clone(actionInput);
  let type = normalizeActionType(action);
  if (type === "INTERACT") {
    action = inferInteractionAction(world, action);
    type = normalizeActionType(action);
  }
  if (type === "TRADE") {
    const side = String(action.side || action.direction || "").toUpperCase();
    if (side !== "BUY" && side !== "SELL") throw new WorldActionError("INVALID_TRADE", "TRADE requires side BUY or SELL");
    type = side;
  }

  let result;
  if (type === "JOIN") result = joinAction(world, action, now);
  else if (type === "MOVE") result = moveAction(world, action);
  else if (type === "GATHER") result = gatherAction(world, action);
  else if (type === "DEPOSIT") result = depositAction(world, action);
  else if (type === "BUILD") result = buildAction(world, action);
  else if (type === "CRAFT") result = craftAction(world, action);
  else if (type === "BUY") result = buyAction(world, action);
  else if (type === "SELL") result = sellAction(world, action);
  else if (type === "TALK") result = talkAction(world, action);
  else throw new WorldActionError("UNSUPPORTED_ACTION", `Unsupported world action ${type || "(missing)"}`, { type });

  updateMarketPrices(world);
  roundWorldNumbers(world);
  world.revision += 1;
  world.lastAction = { type, actorId: action.actorId || action.playerId || null, atMs: now, result: clone(result) };
  return world;
}

export function getNearbyInteractions(world, actorIdInput) {
  assertWorld(world);
  const actorId = validateActorId(actorIdInput);
  const player = requirePlayer(world, actorId);
  const interactions = [];
  Object.values(world.nodes).forEach((node) => {
    const distance = distanceBetween(player, node);
    if (distance <= node.radius + 5) {
      const inRange = isNear(player, node);
      interactions.push({ id: `gather:${node.id}`, type: "GATHER", targetId: node.id, label: `Gather ${node.resource}`, distance: round(distance, 2), inRange, available: inRange && !player.job && Math.floor(node.amount) > 0 });
    }
  });
  const warehouseDistance = distanceBetween(player, world.warehouse);
  if (warehouseDistance <= world.warehouse.radius + 5) {
    const inRange = isNear(player, world.warehouse);
    interactions.push({ id: "deposit:warehouse", type: "DEPOSIT", targetId: "warehouse", label: "Deposit supplies", distance: round(warehouseDistance, 2), inRange, available: inRange && !player.job && inventoryTotal(player.inventory) > 0 });
  }
  const project = world.projects.eastSluice;
  const projectDistance = distanceBetween(player, project);
  if (projectDistance <= project.radius + 5) {
    const hasMaterials = Object.entries(project.buildCost).every(([item, quantity]) => world.warehouse.stocks[item] >= quantity);
    const inRange = isNear(player, project);
    interactions.push({ id: "build:east-sluice", type: "BUILD", targetId: project.id, label: project.status === "complete" ? "East Sluice complete" : "Repair East Sluice", distance: round(projectDistance, 2), inRange, available: inRange && !player.job && project.status !== "complete" && hasMaterials });
  }
  const workshopDistance = distanceBetween(player, world.buildings.workshop);
  if (workshopDistance <= world.buildings.workshop.radius + 5) {
    const inRange = isNear(player, world.buildings.workshop);
    interactions.push({ id: "craft:workshop", type: "CRAFT", targetId: "workshop", label: "Use workshop", distance: round(workshopDistance, 2), inRange, available: inRange && !player.job });
  }
  const marketDistance = distanceBetween(player, world.market);
  if (marketDistance <= world.market.radius + 5) {
    const inRange = isNear(player, world.market);
    interactions.push({ id: "trade:market", type: "TRADE", targetId: "market", label: "Trade at Lantern Market", distance: round(marketDistance, 2), inRange, available: inRange && !player.job });
  }
  Object.values(world.npcs).forEach((npc) => {
    const distance = distanceBetween(player, npc);
    if (distance <= 6) {
      const inRange = isNear(player, { ...npc, radius: 1.2 });
      interactions.push({ id: `talk:${npc.id}`, type: "TALK", targetId: npc.id, label: `Talk to ${npc.name}`, distance: round(distance, 2), inRange, available: inRange && !player.job });
    }
  });
  return interactions.sort((left, right) => left.distance - right.distance || left.id.localeCompare(right.id));
}

export function getPublicWorld(worldInput) {
  assertWorld(worldInput);
  const world = clone(worldInput);
  const actorView = (actor) => ({
    ...clone(actor),
    targetX: actor.target ? actor.target.x : null,
    targetY: actor.target ? actor.target.y : null
  });
  world.actors = [
    ...Object.values(world.players).map(actorView),
    ...Object.values(world.npcs).map(actorView)
  ];
  world.hotspots = [
    ...Object.values(world.nodes).map((node) => ({ ...clone(node), kind: "resource" })),
    { ...clone(world.warehouse), kind: "warehouse" },
    { ...clone(world.buildings.workshop), kind: "workshop" },
    { ...clone(world.market), kind: "market" },
    { ...clone(world.projects.eastSluice), kind: "project" }
  ];
  world.resources = clone(world.nodes);
  world.sharedResources = clone(world.warehouse.stocks);
  world.buildingsList = Object.values(world.buildings).map((building) => clone(building));
  world.construction = clone(world.projects.eastSluice);
  return world;
}

export function serializeWorld(world) {
  return JSON.stringify(getPublicWorld(world));
}
