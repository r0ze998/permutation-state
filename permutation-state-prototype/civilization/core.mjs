/** Shared, deterministic, dependency-free civilization simulation. No wallets or LLMs. */
export const RESOURCE_META = Object.freeze({
  food: { name: "食料", color: "#d3b55b", icon: "◒" },
  wood: { name: "木材", color: "#a4875c", icon: "♠" },
  stone: { name: "石材", color: "#a9b7bd", icon: "◆" },
  ore: { name: "鉱石", color: "#cb9c7f", icon: "⬟" },
  tools: { name: "道具", color: "#93bac7", icon: "⚒" },
  knowledge: { name: "知識", color: "#c2afe0", icon: "✧" }
});

export const BUILDINGS = Object.freeze({
  farm: { name: "農場", cost: { wood: 12, stone: 6 }, durationMs: 15_000, terrains: ["grass"], cycleMs: 12_000, inputs: {}, outputs: { food: 5 }, effect: "食料 +25/分。働く市民を支える。", description: "肥沃な平原を継続的な食料源にする。" },
  lumbermill: { name: "製材所", cost: { wood: 12, stone: 4 }, durationMs: 18_000, terrains: ["forest"], cycleMs: 15_000, inputs: {}, outputs: { wood: 4 }, effect: "木材 +16/分。道と次の施設を建てやすくする。", description: "森の木材を製材し、街へ運ぶ。" },
  quarry: { name: "採石場", cost: { wood: 8, stone: 10 }, durationMs: 20_000, terrains: ["hill"], cycleMs: 18_000, inputs: {}, outputs: { stone: 3 }, effect: "石材 +10/分。建設の石不足を解消する。", description: "丘陵から石材を採り出す。" },
  mine: { name: "鉱山", cost: { wood: 12, stone: 12, tools: 2 }, durationMs: 24_000, terrains: ["mountain"], requires: "metallurgy", cycleMs: 18_000, inputs: {}, outputs: { ore: 3 }, effect: "鉱石 +10/分。鍛冶工房の原料を供給する。", description: "冶金を研究すると建設できる。" },
  workshop: { name: "鍛冶工房", cost: { wood: 18, stone: 12 }, durationMs: 22_000, terrains: ["grass", "hill"], cycleMs: 20_000, inputs: { wood: 2, ore: 1 }, outputs: { tools: 2 }, effect: "木材 2 + 鉱石 1 → 道具 2 / 20秒。原料がないと停止。", description: "石の炉と木製の作業台から道具づくりを始める。建設に道具は不要。" },
  watchtower: { name: "見張り塔", cost: { wood: 10, stone: 10 }, durationMs: 18_000, terrains: ["grass", "hill"], cycleMs: 0, inputs: {}, outputs: {}, effect: "完成時に周囲3マスを発見。新しい資源と開拓地が見える。", description: "遠くの開拓機会を見つける。" },
  archive: { name: "学術院", cost: { wood: 12, stone: 14, tools: 2 }, durationMs: 24_000, terrains: ["grass", "hill"], cycleMs: 18_000, inputs: { food: 1 }, outputs: { knowledge: 3 }, effect: "食料 1 → 知識 3 / 18秒。農業・物流・冶金研究へ。", description: "食料を使って研究に必要な知識を生む。" },
  townhall: { name: "共同倉庫", cost: {}, durationMs: 0, terrains: ["grass"], cycleMs: 0, inputs: {}, outputs: {}, effect: "全市民の資源と物流の拠点。", buildable: false }
});

export const TECHNOLOGIES = Object.freeze({
  agriculture: { name: "輪作農法", cost: { knowledge: 6, food: 6 }, durationMs: 25_000, effect: "全農場の収穫量 +40%" },
  logistics: { name: "輸送術", cost: { knowledge: 6, wood: 4 }, durationMs: 25_000, effect: "輸送速度 +60%、道路の費用を軽減" },
  metallurgy: { name: "冶金", cost: { knowledge: 8, tools: 2 }, durationMs: 30_000, effect: "鉱山建設を解禁" }
});

const KEYS = Object.keys(RESOURCE_META);
const DIRECTIONS = [[1, 0], [1, -1], [0, -1], [-1, 0], [-1, 1], [0, 1]];
const HOME = "0,0";
const MAX_ADVANCE_MS = 60_000;
const STEP_MS = 250;
const EPSILON = 1e-8;
const TERRAIN_COST = { grass: 1, forest: 1.25, hill: 1.6, mountain: 2.5, water: Infinity };
const clone = (value) => JSON.parse(JSON.stringify(value));
const zeroStock = () => Object.fromEntries(KEYS.map((key) => [key, 0]));
const clamp = (value, low, high) => Math.max(low, Math.min(high, value));
const pointId = (q, r) => `${q},${r}`;
const neighbors = (tile) => DIRECTIONS.map(([q, r]) => pointId(tile.q + q, tile.r + r));
const finite = (value) => typeof value === "number" && Number.isFinite(value);

export function hexDistance(a, b) {
  return Math.max(Math.abs(a.q - b.q), Math.abs(a.r - b.r), Math.abs((a.q + a.r) - (b.q + b.r)));
}

export function tileById(world, id) { return world.tiles.find((tile) => tile.id === id) || null; }

function requireWorld(world) {
  if (!world || world.schemaVersion !== "permutation.civilization.v1") throw new Error("文明データが不正です。");
}

function requireActor(world, actorId) {
  const player = Object.hasOwn(world.players, actorId) ? world.players[actorId] : null;
  if (!player) throw new Error("先に市民として参加してください。");
  return player;
}

function actorIdOf(action) {
  if (typeof action.actorId !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(action.actorId)) throw new Error("市民IDが不正です。");
  if (["__proto__", "prototype", "constructor"].includes(action.actorId)) throw new Error("この市民IDは使用できません。");
  return action.actorId;
}

function pushEvent(world, type, text, tileId = HOME) {
  world.eventSerial += 1;
  world.events.push({ id: `event-${world.eventSerial}`, type, text, timeMs: world.timeMs, tileId });
  if (world.events.length > 100) world.events.splice(0, world.events.length - 100);
}

function canPay(stock, cost) { return Object.entries(cost).every(([key, value]) => stock[key] + EPSILON >= value); }
function pay(stock, cost) { Object.entries(cost).forEach(([key, value]) => { stock[key] = Math.max(0, stock[key] - value); }); }
function add(stock, amounts) { Object.entries(amounts).forEach(([key, value]) => { stock[key] = (stock[key] || 0) + value; }); }
function costText(cost) { return Object.entries(cost).map(([key, amount]) => `${RESOURCE_META[key].name} ${amount}`).join("・"); }
function idleReason(player) {
  if (player.job) return "今の作業が終わるまでお待ちください。";
  if (player.cargo) return "採集した資源を共同倉庫へ運んでいます。";
  if (player.path.length) return "目的地に到着してから作業してください。";
  return "";
}
function requireIdle(player) {
  const reason = idleReason(player);
  if (reason) throw new Error(reason);
}

function requireLocal(player, tile) {
  if (hexDistance(player, tile) > 1) throw new Error("作業地点か、その隣のマスまで移動してください。");
}

function terrainAt(q, r) {
  const id = pointId(q, r);
  const fixed = {
    "0,0": "grass", "1,0": "grass", "-1,0": "grass", "0,1": "grass", "0,-1": "grass",
    "-1,1": "forest", "-2,1": "forest", "-2,0": "forest", "-1,2": "forest",
    "1,-1": "hill", "2,-1": "hill", "2,-2": "mountain", "1,-2": "hill",
    "1,1": "grass", "0,2": "grass", "-1,-1": "grass", "0,-2": "grass", "2,0": "grass",
    "3,-1": "water", "3,0": "water", "3,1": "water", "4,-2": "mountain", "4,-3": "mountain",
    "-3,1": "forest", "-3,2": "forest", "-4,2": "forest", "-3,0": "grass"
  };
  if (fixed[id]) return fixed[id];
  const hash = Math.abs((q * 374761393) ^ (r * 668265263) ^ 174901);
  if ((q === 3 && r >= -1 && r <= 4) || (r === -5 && q > 0)) return "water";
  const n = hash % 100;
  return n < 40 ? "grass" : n < 67 ? "forest" : n < 89 ? "hill" : "mountain";
}

function createTiles() {
  const tiles = [];
  for (let q = -8; q <= 8; q += 1) {
    for (let r = -8; r <= 8; r += 1) {
      if (hexDistance({ q, r }, { q: 0, r: 0 }) > 8) continue;
      const terrain = terrainAt(q, r);
      const resource = { forest: "wood", hill: "stone", mountain: "ore" }[terrain] || null;
      const id = pointId(q, r);
      tiles.push({ id, q, r, terrain, resource, explored: hexDistance({ q, r }, { q: 0, r: 0 }) <= 2,
        road: [HOME, "1,0", "-1,0", "0,1", "0,-1"].includes(id), buildingId: id === HOME ? "civic-depot" : null,
        resourceAmount: resource ? 30 : terrain === "grass" ? 16 : 0, resourceCapacity: resource ? 30 : 16 });
    }
  }
  return tiles;
}

export function createCivilization({ sessionId = "aster", nowMs = 0 } = {}) {
  if (typeof sessionId !== "string" || !/^[a-zA-Z0-9_-]{1,80}$/.test(sessionId)) throw new Error("文明IDが不正です。");
  if (!finite(nowMs) || nowMs < 0) throw new Error("時刻が不正です。");
  const world = {
    schemaVersion: "permutation.civilization.v1", sessionId, revision: 0, timeMs: 0, lastNowMs: nowMs,
    eventSerial: 0, entitySerial: 0, tiles: createTiles(), players: {},
    buildings: { "civic-depot": { id: "civic-depot", type: "townhall", tileId: HOME, status: "active", progress: 1, ownerId: "civilization", connected: true, localStock: zeroStock(), reason: "", productionMs: 0 } },
    stock: { food: 30, wood: 30, stone: 22, ore: 6, tools: 3, knowledge: 0 }, rates: zeroStock(),
    npcs: [{ id: "steward", name: "ニア", q: 0, r: 0, role: "steward", status: "共同倉庫で荷物を受け取る", targetTileId: HOME }],
    caravans: [], events: [],
    season: { name: "黎明の開拓期", elapsedMs: 0, durationMs: 30 * 60_000, complete: false, objectives: [
      { id: "food", label: "食料備蓄 75", current: 30, target: 75, complete: false },
      { id: "industry", label: "道具備蓄 12", current: 3, target: 12, complete: false },
      { id: "exploration", label: "既知の土地 45", current: 19, target: 45, complete: false },
      { id: "research", label: "研究を1つ完成", current: 0, target: 1, complete: false }
    ] },
    research: { active: null, progress: 0, unlocked: [] }, totals: { delivered: zeroStock(), explored: 19, buildings: 0 }
  };
  refreshRatesAndObjectives(world);
  pushEvent(world, "FOUNDING", "全員で一つの文明。農業・産業・探索のどれに資源を使うかは、市民が決める。");
  return world;
}

/** Shortest known-land path. Water and undiscovered tiles cannot be crossed. */
export function findPath(world, fromId, toId, roadOnly = false) {
  const tileMap = new Map(world.tiles.map((tile) => [tile.id, tile]));
  const from = tileMap.get(fromId), to = tileMap.get(toId);
  if (!from || !to || !to.explored || to.terrain === "water") return null;
  if (fromId === toId) return [];
  const distances = new Map([[fromId, 0]]), previous = new Map(), open = new Set([fromId]);
  while (open.size) {
    let current = null, score = Infinity;
    for (const id of open) if (distances.get(id) < score) { current = id; score = distances.get(id); }
    open.delete(current);
    if (current === toId) break;
    for (const id of neighbors(tileMap.get(current))) {
      const next = tileMap.get(id);
      if (!next || !next.explored || next.terrain === "water") continue;
      if (roadOnly && !next.road && id !== toId) continue;
      const candidate = score + (next.road ? 0.65 : TERRAIN_COST[next.terrain]);
      if (candidate < (distances.get(id) ?? Infinity)) {
        distances.set(id, candidate); previous.set(id, current); open.add(id);
      }
    }
  }
  if (!previous.has(toId)) return null;
  const path = [];
  for (let current = toId; current !== fromId; current = previous.get(current)) path.unshift(current);
  return path;
}

function connectedRoads(world) {
  const tileMap = new Map(world.tiles.map((tile) => [tile.id, tile]));
  const connected = new Set([HOME]), queue = [HOME];
  for (let index = 0; index < queue.length; index += 1) {
    for (const id of neighbors(tileMap.get(queue[index]))) {
      const tile = tileMap.get(id);
      if (tile && tile.road && tile.explored && tile.terrain !== "water" && !connected.has(id)) { connected.add(id); queue.push(id); }
    }
  }
  return connected;
}

function isConnected(tile, roads) { return roads.has(tile.id) || neighbors(tile).some((id) => roads.has(id)); }

export function getBuildPreview(world, actorId, tileId, buildingType) {
  const definition = Object.hasOwn(BUILDINGS, buildingType) ? BUILDINGS[buildingType] : null;
  const result = { allowed: false, reason: "", cost: clone(definition?.cost || {}), durationMs: definition?.durationMs || 0, effect: definition?.effect || "" };
  const tile = tileById(world, tileId), player = world.players[actorId];
  if (!definition || definition.buildable === false) result.reason = "この建物は建設できません。";
  else if (!player) result.reason = "先に市民として参加してください。";
  else if (!tile || !tile.explored) result.reason = "まずこの土地を探索してください。";
  else if (tile.buildingId || tile.roadJob) result.reason = "この土地はすでに使われています。";
  else if (!definition.terrains.includes(tile.terrain)) result.reason = `${definition.name}は${definition.terrains.map((t) => ({ grass: "平原", forest: "森林", hill: "丘陵", mountain: "鉱山地形" })[t]).join("・")}に建てられます。`;
  else if (definition.requires && !world.research.unlocked.includes(definition.requires)) result.reason = `先に「${TECHNOLOGIES[definition.requires].name}」を研究してください。`;
  else if (!isConnected(tile, connectedRoads(world))) result.reason = "共同倉庫につながる道を、この土地の隣まで延ばしてください。";
  else if (!canPay(world.stock, definition.cost)) result.reason = `共同資源が不足：${costText(definition.cost)}`;
  else if (player.job || player.cargo || player.path.length) result.reason = "今の移動・作業・運搬が終わってから建設できます。";
  else if (hexDistance(player, tile) > 1) result.reason = "建設予定地か、その隣まで移動してください。";
  else { result.allowed = true; result.reason = "ここに建設できます。共有資源を使用します。"; }
  return result;
}

function revealAround(world, tile, radius, citizenName = "市民") {
  let count = 0;
  for (const next of world.tiles) if (!next.explored && hexDistance(tile, next) <= radius) { next.explored = true; count += 1; }
  if (count) pushEvent(world, "DISCOVERY", `${citizenName}が新たに${count}マスを発見した。道路を伸ばすと新しい開拓地になる。`, tile.id);
  return count;
}

function roadCost(world) { return world.research.unlocked.includes("logistics") ? { wood: 1 } : { wood: 2, stone: 1 }; }

export function getRoadPreview(world, actorId, tileId) {
  const result = { allowed: false, reason: "", cost: roadCost(world), durationMs: 4_000, effect: "道路網を1マス延ばす。隣接地の建設と物資の輸送を可能にする。" };
  const player = Object.hasOwn(world.players, actorId) ? world.players[actorId] : null;
  const tile = tileById(world, tileId);
  if (!player) result.reason = "先に市民として参加してください。";
  else if (!tile) result.reason = "その土地はありません。";
  else if (idleReason(player)) result.reason = idleReason(player);
  else if (hexDistance(player, tile) > 1) result.reason = "作業地点か、その隣のマスまで移動してください。";
  else if (!tile.explored) result.reason = "まずこの土地を探索してください。";
  else if (tile.terrain === "water") result.reason = "水面には道路を建設できません。";
  else if (tile.road || tile.roadJob) result.reason = "この土地にはすでに道があります。";
  else if (!isConnected(tile, connectedRoads(world))) result.reason = "道は共同倉庫につながる道の隣から延ばせます。";
  else if (!canPay(world.stock, result.cost)) result.reason = `道路の資源が不足：${costText(result.cost)}`;
  else { result.allowed = true; result.reason = "ここに道路を敷設できます。共有資源を使用します。"; }
  return result;
}

function availableArchive(world, player) {
  return Object.values(world.buildings).find((building) => building.type === "archive" && building.status !== "building" && building.connected && hexDistance(player, tileById(world, building.tileId)) <= 1);
}

export function getResearchPreview(world, actorId, techId) {
  const tech = Object.hasOwn(TECHNOLOGIES, techId) ? TECHNOLOGIES[techId] : null;
  const result = { allowed: false, reason: "", cost: clone(tech?.cost || {}), durationMs: tech?.durationMs || 0, effect: tech?.effect || "" };
  const player = Object.hasOwn(world.players, actorId) ? world.players[actorId] : null;
  if (!player) result.reason = "先に市民として参加してください。";
  else if (idleReason(player)) result.reason = idleReason(player);
  else if (!tech) result.reason = "その研究はありません。";
  else if (world.research.active) result.reason = "文明はすでに研究を進めています。";
  else if (world.research.unlocked.includes(techId)) result.reason = "この研究は完成しています。";
  else if (!availableArchive(world, player)) result.reason = "完成した学術院か、その隣まで移動してください。";
  else if (!canPay(world.stock, result.cost)) result.reason = `研究資源が不足：${costText(result.cost)}`;
  else { result.allowed = true; result.reason = "共同研究を始められます。完成した技術は全市民が利用できます。"; }
  return result;
}

function command(world, action) {
  const actorId = actorIdOf(action), type = String(action.type || "").toUpperCase();
  if (type === "JOIN") {
    if (!world.players[actorId]) {
      const name = String(action.name || "旅人").replace(/[<>\x00-\x1f]/g, "").trim().slice(0, 24) || "旅人";
      world.players[actorId] = { id: actorId, name, q: 0, r: 0, path: [], status: "待機", job: null, contribution: 0, cargo: null, moveProgress: 0 };
      pushEvent(world, "CITIZEN_JOINED", `${name}がアスターの市民になった。`);
    }
    return;
  }
  const player = requireActor(world, actorId);
  if (type === "RESEARCH") {
    const preview = getResearchPreview(world, actorId, action.techId);
    if (!preview.allowed) throw new Error(preview.reason);
    const tech = TECHNOLOGIES[action.techId], archive = availableArchive(world, player);
    pay(world.stock, preview.cost);
    world.research.active = action.techId; world.research.progress = 0; world.research.ownerId = actorId; world.research.archiveId = archive.id;
    pushEvent(world, "RESEARCH_STARTED", `${player.name}が「${tech.name}」の共同研究を始めた。`, archive.tileId);
    return;
  }
  const tile = tileById(world, action.tileId);
  if (!tile) throw new Error("その土地はありません。");
  if (type === "MOVE") {
    if (player.job || player.cargo) throw new Error("今の作業・運搬を終えてから移動してください。");
    if (!tile.explored) throw new Error("未知の土地です。隣まで移動して探索してください。");
    const path = findPath(world, pointId(player.q, player.r), tile.id);
    if (!path) throw new Error("ここへ続く陸路がありません。水面は渡れません。");
    player.path = path; player.moveProgress = 0; player.status = path.length ? "移動中" : "待機";
    return;
  }
  if (type === "ROAD") {
    const preview = getRoadPreview(world, actorId, tile.id);
    if (!preview.allowed) throw new Error(preview.reason);
    pay(world.stock, preview.cost);
    tile.roadJob = actorId;
    player.job = { type: "road", tileId: tile.id, elapsedMs: 0, durationMs: preview.durationMs, progress: 0 };
    player.status = "道路を敷設中";
    return;
  }
  requireIdle(player); requireLocal(player, tile);
  if (type === "EXPLORE") {
    if (tile.explored && !neighbors(tile).some((id) => { const n = tileById(world, id); return n && !n.explored; })) throw new Error("この土地の周辺はすでに調査済みです。");
    player.job = { type: "explore", tileId: tile.id, elapsedMs: 0, durationMs: 5_000, progress: 0 }; player.status = "周辺を探索中";
    return;
  }
  if (!tile.explored) throw new Error("まずこの土地を探索してください。");
  if (type === "BUILD") {
    const preview = getBuildPreview(world, actorId, tile.id, action.buildingType);
    if (!preview.allowed) throw new Error(preview.reason);
    const id = `building-${++world.entitySerial}`, definition = BUILDINGS[action.buildingType];
    pay(world.stock, definition.cost);
    world.buildings[id] = { id, type: action.buildingType, tileId: tile.id, status: "building", progress: 0, ownerId: actorId, connected: true, localStock: zeroStock(), reason: "建設中", productionMs: 0, elapsedMs: 0, durationMs: definition.durationMs };
    tile.buildingId = id;
    player.job = { type: "build", tileId: tile.id, buildingId: id, elapsedMs: 0, durationMs: definition.durationMs, progress: 0 };
    player.status = `${definition.name}を建設中`;
    pushEvent(world, "BUILD_STARTED", `${player.name}が${definition.name}に投資した（${costText(definition.cost)}）。`, tile.id);
    return;
  }
  if (type === "GATHER") {
    if (tile.terrain === "water" || tile.buildingId) throw new Error("この土地では手作業で採集できません。");
    if (tile.resourceAmount < 1) throw new Error("資源が回復するまで少し待ってください。");
    if (!findPath(world, pointId(player.q, player.r), HOME)) throw new Error("共同倉庫への帰り道がありません。");
    const resource = tile.resource || "food", desired = resource === "stone" ? 3 : resource === "ore" ? 2 : 4;
    const amount = Math.min(desired, Math.floor(tile.resourceAmount));
    tile.resourceAmount -= amount;
    player.job = { type: "gather", tileId: tile.id, resource, amount, elapsedMs: 0, durationMs: 6_000, progress: 0 };
    player.status = `${RESOURCE_META[resource].name}を採集中`;
    return;
  }
  throw new Error("この操作はまだ利用できません。");
}

function completeJob(world, player) {
  const job = player.job, tile = tileById(world, job.tileId);
  player.job = null; player.status = "待機";
  if (job.type === "explore") {
    player.contribution += revealAround(world, tile, 1, player.name);
  } else if (job.type === "road") {
    tile.road = true; delete tile.roadJob; player.contribution += 2;
    pushEvent(world, "ROAD_COMPLETED", `${player.name}が道を延ばした。近くの土地に建設できる。`, tile.id);
  } else if (job.type === "build") {
    const building = world.buildings[job.buildingId];
    building.progress = 1; building.status = "active"; building.reason = "";
    player.contribution += 10; world.totals.buildings += 1;
    world.npcs.push({ id: `worker-${building.id}`, name: ["タラ", "オリン", "ブラム", "セラ", "イヴォ"][world.totals.buildings % 5], q: 0, r: 0, role: building.type, status: "職場へ移動中", targetTileId: tile.id, path: findPath(world, HOME, tile.id) || [], moveProgress: 0, buildingId: building.id });
    pushEvent(world, "BUILD_COMPLETED", `${BUILDINGS[building.type].name}が完成。${BUILDINGS[building.type].effect}`, tile.id);
    if (building.type === "watchtower") player.contribution += revealAround(world, tile, 3, player.name);
  } else if (job.type === "gather") {
    player.cargo = { resource: job.resource, amount: job.amount };
    player.path = findPath(world, pointId(player.q, player.r), HOME) || [];
    player.moveProgress = 0; player.status = "採集品を共同倉庫へ運搬中";
    depositCargo(world, player);
  }
}

function depositCargo(world, player) {
  if (!player.cargo || player.q !== 0 || player.r !== 0) return;
  const { resource, amount } = player.cargo;
  world.stock[resource] += amount; world.totals.delivered[resource] += amount; player.contribution += amount;
  player.cargo = null; player.path = []; player.status = "待機";
  pushEvent(world, "FORAGE_DELIVERED", `${player.name}が${RESOURCE_META[resource].name}${amount}を共同倉庫へ運んだ。`);
}

function moveUnit(world, unit, dtMs) {
  if (!unit.path?.length) return;
  unit.moveProgress = (unit.moveProgress || 0) + dtMs;
  while (unit.path.length) {
    const next = tileById(world, unit.path[0]);
    if (!next || !next.explored || next.terrain === "water") { unit.path = []; unit.moveProgress = 0; unit.status = "移動経路が塞がれた"; return; }
    const duration = 900 * (next.road ? 0.65 : TERRAIN_COST[next.terrain]);
    if (unit.moveProgress + EPSILON < duration) break;
    unit.moveProgress -= duration; unit.q = next.q; unit.r = next.r; unit.path.shift();
  }
  if (!unit.path.length) { unit.moveProgress = 0; unit.status = unit.cargo ? "荷下ろし中" : "待機"; }
  if (unit.cargo) depositCargo(world, unit);
}

function launchCourier(world, building, resource, amount, direction) {
  const fromTileId = direction === "input" ? HOME : building.tileId;
  const toTileId = direction === "input" ? building.tileId : HOME;
  const path = findPath(world, fromTileId, toTileId, true);
  if (!path) return false;
  const route = [fromTileId, ...path], logistics = world.research.unlocked.includes("logistics");
  const durationMs = Math.max(1_800, path.reduce((sum, id) => sum + (tileById(world, id).road ? 1 : 1.5), 0) * 2_200 / (logistics ? 1.6 : 1));
  world.caravans.push({ id: `courier-${++world.entitySerial}`, fromTileId, toTileId, progress: 0, resource, amount, direction, buildingId: building.id, route, durationMs, elapsedMs: 0, q: tileById(world, fromTileId).q, r: tileById(world, fromTileId).r });
  return true;
}

function advanceCouriers(world, dtMs) {
  const complete = [];
  for (const courier of world.caravans) {
    courier.elapsedMs += dtMs; courier.progress = clamp(courier.elapsedMs / courier.durationMs, 0, 1);
    const routeStep = courier.progress * (courier.route.length - 1), index = Math.min(Math.floor(routeStep), courier.route.length - 1);
    const from = tileById(world, courier.route[index]), to = tileById(world, courier.route[Math.min(index + 1, courier.route.length - 1)]), between = routeStep - index;
    courier.q = from.q + (to.q - from.q) * between; courier.r = from.r + (to.r - from.r) * between;
    if (courier.progress >= 1) {
      if (courier.direction === "output") { world.stock[courier.resource] += courier.amount; world.totals.delivered[courier.resource] += courier.amount; }
      else if (world.buildings[courier.buildingId]) world.buildings[courier.buildingId].localStock[courier.resource] += courier.amount;
      complete.push(courier.id);
    }
  }
  if (complete.length) world.caravans = world.caravans.filter((courier) => !complete.includes(courier.id));
}

function buildingOutput(world, building) {
  const output = { ...BUILDINGS[building.type].outputs };
  if (building.type === "farm" && world.research.unlocked.includes("agriculture")) output.food *= 1.4;
  return output;
}

function updateBuildings(world, dtMs, roads) {
  for (const building of Object.values(world.buildings)) {
    const definition = BUILDINGS[building.type];
    building.connected = isConnected(tileById(world, building.tileId), roads);
    if (building.status === "building" || building.type === "townhall") continue;
    if (!building.connected) { building.status = "blocked"; building.reason = "共同倉庫への道路がつながっていない"; continue; }
    if (!definition.cycleMs) { building.status = "active"; building.reason = ""; continue; }
    const worker = world.npcs.find((npc) => npc.buildingId === building.id);
    if (worker?.path?.length) { building.status = "blocked"; building.reason = "担当の市民が職場へ移動中"; continue; }
    const output = buildingOutput(world, building);
    for (const [resource] of Object.entries(output)) {
      if (building.localStock[resource] < 1 || world.caravans.some((courier) => courier.buildingId === building.id && courier.direction === "output" && courier.resource === resource)) continue;
      const amount = Math.min(Math.floor(building.localStock[resource]), world.research.unlocked.includes("logistics") ? 12 : 8);
      if (launchCourier(world, building, resource, amount, "output")) building.localStock[resource] -= amount;
    }
    for (const [resource, required] of Object.entries(definition.inputs)) {
      const inbound = world.caravans.filter((courier) => courier.buildingId === building.id && courier.direction === "input" && courier.resource === resource).reduce((sum, courier) => sum + courier.amount, 0);
      if (building.localStock[resource] + inbound >= required * 2 || world.stock[resource] < required) continue;
      const amount = Math.min(required * 2, Math.floor(world.stock[resource]));
      if (launchCourier(world, building, resource, amount, "input")) world.stock[resource] -= amount;
    }
    if (!building.cyclePaid && !canPay(building.localStock, definition.inputs)) {
      building.status = "blocked";
      building.reason = Object.entries(definition.inputs).filter(([resource, quantity]) => building.localStock[resource] < quantity).map(([resource]) => `${RESOURCE_META[resource].name}${world.caravans.some((courier) => courier.buildingId === building.id && courier.direction === "input" && courier.resource === resource) ? "を輸送中" : "が不足"}`).join("・");
      if (worker) worker.status = building.reason;
      continue;
    }
    building.status = "active"; building.reason = "";
    if (!building.cyclePaid) { pay(building.localStock, definition.inputs); building.cyclePaid = true; }
    if (worker) worker.status = "生産中";
    const speed = world.stock.food <= 0 ? 0.55 : 1;
    building.productionMs += dtMs * speed;
    if (building.productionMs >= definition.cycleMs) {
      building.productionMs -= definition.cycleMs; add(building.localStock, output); building.cyclePaid = false;
    }
  }
}

function upkeepPerMinute(world) { return 1.5 + Math.max(0, world.npcs.length - 1) * 0.45 + Object.keys(world.players).length * 0.25; }

function refreshRatesAndObjectives(world) {
  world.rates = zeroStock(); world.rates.food -= upkeepPerMinute(world);
  for (const building of Object.values(world.buildings)) {
    const definition = BUILDINGS[building.type];
    if (building.status !== "active" || !definition.cycleMs) continue;
    const factor = 60_000 / definition.cycleMs * (world.stock.food <= 0 ? 0.55 : 1);
    for (const [key, amount] of Object.entries(buildingOutput(world, building))) world.rates[key] += amount * factor;
    for (const [key, amount] of Object.entries(definition.inputs)) world.rates[key] -= amount * factor;
  }
  const values = { food: world.stock.food, industry: world.stock.tools, exploration: world.tiles.filter((tile) => tile.explored).length, research: world.research.unlocked.length };
  world.totals.explored = values.exploration;
  for (const objective of world.season.objectives) {
    objective.current = Math.floor(values[objective.id] || 0);
    if (objective.current >= objective.target) objective.complete = true;
  }
  if (!world.season.complete && world.season.objectives.every((objective) => objective.complete)) {
    world.season.complete = true;
    pushEvent(world, "AMBITION_REACHED", "アスターは開拓期の共同目標を達成した。文明の発展はこのまま続く。");
  }
}

function step(world, dtMs) {
  world.timeMs += dtMs; world.season.elapsedMs = world.timeMs;
  const roads = connectedRoads(world);
  for (const tile of world.tiles) if (tile.resourceAmount < tile.resourceCapacity) tile.resourceAmount = Math.min(tile.resourceCapacity, tile.resourceAmount + dtMs / 1_000 * (tile.resource ? 0.025 : 0.04));
  for (const player of Object.values(world.players)) {
    moveUnit(world, player, dtMs);
    if (!player.job) continue;
    player.job.elapsedMs += dtMs; player.job.progress = clamp(player.job.elapsedMs / player.job.durationMs, 0, 1);
    if (player.job.type === "build") world.buildings[player.job.buildingId].progress = player.job.progress;
    if (player.job.progress >= 1) completeJob(world, player);
  }
  for (const npc of world.npcs) moveUnit(world, npc, dtMs);
  advanceCouriers(world, dtMs);
  world.stock.food = Math.max(0, world.stock.food - upkeepPerMinute(world) * dtMs / 60_000);
  updateBuildings(world, dtMs, roads);
  if (world.research.active) {
    const archive = world.buildings[world.research.archiveId];
    if (archive && archive.connected) {
      const tech = TECHNOLOGIES[world.research.active];
      world.research.progress = Math.min(1, world.research.progress + dtMs / tech.durationMs);
      if (world.research.progress >= 1 - EPSILON) {
        const id = world.research.active; world.research.unlocked.push(id); world.research.active = null; world.research.progress = 1;
        if (world.players[world.research.ownerId]) world.players[world.research.ownerId].contribution += 15;
        pushEvent(world, "RESEARCH_COMPLETED", `「${tech.name}」の研究が完成。${tech.effect}。`, archive.tileId);
      }
    }
  }
}

export function advanceCivilization(world, nowMs) {
  requireWorld(world);
  if (!finite(nowMs) || nowMs < 0) throw new Error("時刻が不正です。");
  if (nowMs <= world.lastNowMs) return world;
  const elapsed = Math.min(MAX_ADVANCE_MS, nowMs - world.lastNowMs);
  world.lastNowMs = nowMs;
  for (let remaining = elapsed; remaining > 0; remaining -= STEP_MS) step(world, Math.min(STEP_MS, remaining));
  refreshRatesAndObjectives(world); world.revision += 1;
  return world;
}

/** Failed commands are atomic: no fees, materials, jobs or clock changes are committed. */
export function applyCivilizationAction(world, action, nowMs) {
  requireWorld(world);
  if (!action || typeof action !== "object") throw new Error("操作が不正です。");
  const draft = clone(world);
  advanceCivilization(draft, nowMs);
  command(draft, action);
  refreshRatesAndObjectives(draft); draft.revision += 1;
  for (const key of Object.keys(world)) delete world[key];
  Object.assign(world, draft);
  return world;
}

export function getCitizenTasks(world, actorId) {
  const player = world.players[actorId];
  if (!player) return [];
  const tasks = [], roads = connectedRoads(world);
  const nearest = (tiles) => tiles.sort((a, b) => hexDistance(player, a) - hexDistance(player, b))[0];
  const buildingTask = (type, title, detail, priority) => {
    const definition = BUILDINGS[type];
    const tile = nearest(world.tiles.filter((tile) => tile.explored && !tile.buildingId && !tile.roadJob && definition.terrains.includes(tile.terrain) && isConnected(tile, roads)));
    if (tile) tasks.push({ id: `build-${type}`, kind: "build", title, detail: `${detail} 費用：${costText(definition.cost)}。`, tileId: tile.id, priority, buildingType: type });
  };
  if (world.stock.food < 40 && !Object.values(world.buildings).some((building) => building.type === "farm")) buildingTask("farm", "食料の供給をつくる", "農場は毎分25の食料を生む。産業への投資はその分遅れる。", 90);
  if (world.stock.wood < 35 && !Object.values(world.buildings).some((building) => building.type === "lumbermill")) buildingTask("lumbermill", "次の建設に向けて木材を増やす", "製材所は毎分16の木材を生む。食料は増えない。", 85);
  if (world.stock.stone < 18 && !Object.values(world.buildings).some((building) => building.type === "quarry")) buildingTask("quarry", "石材の供給をつくる", "採石場が建設のボトルネックを解消する。", 84);
  for (const building of Object.values(world.buildings)) {
    if (building.status !== "blocked") continue;
    tasks.push({ id: `supply-${building.id}`, kind: "supply", title: `${BUILDINGS[building.type].name}の停止を調べる`, detail: building.reason, tileId: building.tileId, priority: 95 });
  }
  for (const resource of ["food", "wood", "stone", "ore"]) {
    if (world.stock[resource] >= (resource === "food" ? 15 : resource === "ore" ? 3 : 8)) continue;
    const tile = nearest(world.tiles.filter((tile) => tile.explored && !tile.buildingId && tile.terrain !== "water" && (tile.resource || "food") === resource && tile.resourceAmount >= 1));
    if (tile) tasks.push({ id: `gather-${resource}`, kind: "gather", title: `${RESOURCE_META[resource].name}を手で集める`, detail: "建設資源が足りなくても回復できる。採集後、市民が共同倉庫まで運ぶ。", tileId: tile.id, priority: 100 });
  }
  const frontier = nearest(world.tiles.filter((tile) => tile.explored && tile.terrain !== "water" && neighbors(tile).some((id) => { const next = tileById(world, id); return next && !next.explored; }) && findPath(world, pointId(player.q, player.r), tile.id)));
  if (frontier) tasks.push({ id: "explore-frontier", kind: "explore", title: "文明の外側を調べる", detail: "未知の資源・地形を発見する。発見後に道を延ばせば、新しい土地を使える。", tileId: frontier.id, priority: 70 });
  if (!Object.values(world.buildings).some((building) => building.type === "archive")) buildingTask("archive", "研究の拠点をつくる", "学術院は食料から知識を生む。農業・輸送・鉱山を研究できる。", 60);
  if (!Object.values(world.buildings).some((building) => building.type === "workshop")) buildingTask("workshop", "道具の生産を始める", "木材と鉱石を消費する。鉱石を供給する計画も必要。", 65);
  return tasks.sort((a, b) => b.priority - a.priority).slice(0, 7);
}
