const $ = (selector, root = document) => root.querySelector(selector);

const query = new URLSearchParams(window.location.search);
const SESSION = cleanId(query.get("session")) || "aster-living-alpha";
const ACTOR_ID = cleanId(query.get("actor") || query.get("role")) || "mara";
const ACTOR_NAME = cleanName(query.get("name")) || (ACTOR_ID === "mara" ? "Mara Venn" : titleCase(ACTOR_ID));
const POLL_MS = 400;
const MAP_WIDTH = 100;
const MAP_HEIGHT = 60;

const dom = {
  shell: $("#gameShell"),
  viewport: $("#worldViewport"),
  camera: $("#mapCamera"),
  plane: $("#mapPlane"),
  actorLayer: $("#actorLayer"),
  hotspotLayer: $("#hotspotLayer"),
  parcelLayer: $("#parcelLayer"),
  marker: $("#moveMarker"),
  actionDock: $("#actionDock"),
  actionButtons: $("#actionButtons"),
  actionContextIcon: $("#actionContextIcon"),
  actionContextLabel: $("#actionContextLabel"),
  connection: $("#connectionState"),
  loading: $("#loadingScreen"),
  toastRegion: $("#toastRegion"),
};

const state = {
  rawWorld: null,
  world: null,
  transport: "connecting",
  disclosure: "OFFCHAIN SIMULATION ALPHA",
  offlineModule: null,
  polling: false,
  pollFailures: 0,
  selected: null,
  zoom: 1,
  panX: 0,
  panY: 0,
  actors: new Map(),
  actorTracks: new Map(),
  hotspots: new Map(),
  actions: [],
  heldKeys: new Set(),
  spriteAvailable: false,
  lastEventKey: "",
  lastIngestAt: 0,
};

const DEFAULT_HOTSPOTS = [
  { id: "forest", name: "Willow Forest", type: "forest", resource: "timber", x: 8, y: 16, radius: 4, amount: 80, maxAmount: 80 },
  { id: "farm", name: "Terrace Farm", type: "farm", resource: "food", x: 85, y: 38, radius: 4, amount: 70, maxAmount: 70 },
  { id: "quarry", name: "Sunstone Quarry", type: "quarry", resource: "stone", x: 58, y: 14, radius: 4, amount: 55, maxAmount: 55 },
  { id: "warehouse", name: "River Warehouse", type: "warehouse", x: 70, y: 20, radius: 4 },
  { id: "eastSluice", name: "East Sluice", type: "project", x: 43, y: 29, radius: 5, progress: 0.24 },
  { id: "workshop", name: "Makers’ Workshop", type: "workshop", x: 13, y: 37, radius: 4, status: "open" },
  { id: "market", name: "Evening Market", type: "market", x: 87, y: 31, radius: 4, status: "trading" },
];

const HOTSPOT_META = {
  forest: { icon: "♧", verb: "Harvest", hint: "Timber grows here" },
  farm: { icon: "♢", verb: "Gather", hint: "Food for the shared store" },
  quarry: { icon: "◆", verb: "Mine", hint: "Stone for civic works" },
  mine: { icon: "◆", verb: "Mine", hint: "Ore and stone" },
  ore: { icon: "⬟", verb: "Mine", hint: "Ore for mechanisms" },
  resource: { icon: "✦", verb: "Gather", hint: "Shared resource site" },
  warehouse: { icon: "▣", verb: "Deposit", hint: "Civilization stockpile" },
  project: { icon: "⚙", verb: "Build", hint: "A shared construction site" },
  sluice: { icon: "⚙", verb: "Build", hint: "Restore the city’s water" },
  workshop: { icon: "⚒", verb: "Craft", hint: "Turn materials into goods" },
  market: { icon: "◈", verb: "Trade", hint: "Exchange with citizens" },
  building: { icon: "⌂", verb: "Work", hint: "A civic building" },
  npc: { icon: "☻", verb: "Talk", hint: "A citizen of Aster" },
};

const ITEM_META = {
  timber: { icon: "▰", name: "Timber" },
  wood: { icon: "▰", name: "Wood" },
  food: { icon: "♢", name: "Food" },
  stone: { icon: "◆", name: "Stone" },
  ore: { icon: "⬟", name: "Ore" },
  goods: { icon: "✦", name: "Goods" },
  planks: { icon: "▤", name: "Planks" },
  coin: { icon: "◈", name: "Coin" },
};

function cleanId(value) {
  return String(value || "").replace(/[^a-zA-Z0-9:_-]/g, "").slice(0, 64);
}

function cleanName(value) {
  return String(value || "").replace(/[<>]/g, "").trim().slice(0, 40);
}

function cleanDisclosure(value) {
  return String(value || "").replace(/[<>]/g, "").trim().slice(0, 80);
}

function titleCase(value) {
  return String(value || "citizen")
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function finite(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, finite(value, min)));
}

function percentage(value) {
  const number = finite(value, 0);
  return clamp(number > 0 && number <= 1 ? number * 100 : number, 0, 100);
}

function collection(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value;
  if (typeof value === "object") {
    return Object.entries(value).map(([id, entry]) => {
      if (entry && typeof entry === "object") return { id, ...entry };
      return { id, value: entry };
    });
  }
  return [];
}

function uniqueById(items) {
  const result = new Map();
  for (const item of items) {
    if (!item) continue;
    const id = String(item.id || item.key || item.name || `item-${result.size}`);
    if (!result.has(id)) result.set(id, { ...item, id });
  }
  return [...result.values()];
}

function positionOf(entity, fallback = { x: 50, y: 30 }) {
  const position = entity?.position || entity?.pos || entity?.coordinates || {};
  const rawX = entity?.x ?? position.x;
  const rawY = entity?.y ?? position.y;
  return {
    x: rawX == null ? fallback.x : clamp(rawX, 0, MAP_WIDTH),
    y: rawY == null ? fallback.y : clamp(rawY, 0, MAP_HEIGHT),
  };
}

function targetOf(entity) {
  const target = entity?.target || entity?.destination || entity?.moveTarget;
  if (!target || typeof target !== "object") return null;
  return { x: clamp(target.x, 0, MAP_WIDTH), y: clamp(target.y, 0, MAP_HEIGHT) };
}

function normalizeInventory(inventory) {
  const source = inventory && typeof inventory === "object" ? inventory : {};
  const normalized = {};
  for (const [key, value] of Object.entries(source)) {
    const amount = typeof value === "object" ? finite(value.amount ?? value.quantity, 0) : finite(value, 0);
    normalized[String(key).toLowerCase()] = Math.max(0, Math.round(amount));
  }
  return normalized;
}

function normalizeActor(actor, forcedKind) {
  const position = positionOf(actor);
  const kind = String(forcedKind || actor.kind || actor.type || (actor.isNpc ? "npc" : "player")).toLowerCase();
  return {
    ...actor,
    id: String(actor.id || actor.actorId || actor.name || "unknown"),
    name: cleanName(actor.name || actor.displayName || actor.id) || "Citizen",
    kind: kind.includes("npc") || kind.includes("agent") ? "npc" : "player",
    x: position.x,
    y: position.y,
    target: targetOf(actor),
    inventory: normalizeInventory(actor.inventory || actor.items || actor.carrying),
    capacity: Math.max(1, finite(actor.capacity ?? actor.maxCarry ?? actor.inventoryCapacity, 8)),
    profession: String(actor.profession || actor.role || actor.trade || (kind.includes("npc") ? "Citizen" : "Civic Runner")),
    status: String(actor.status || actor.activity || actor.state || "ready"),
    job: actor.job || actor.currentJob || null,
  };
}

function normalizeHotspot(item, forcedType) {
  const position = positionOf(item);
  let type = String(forcedType || item.type || item.kind || item.resource || "building").toLowerCase();
  if (type.includes("sluice") || type.includes("project")) type = "project";
  if (type.includes("forest") || String(item.resource).toLowerCase() === "timber") type = "forest";
  if (type.includes("farm") || String(item.resource).toLowerCase() === "food") type = "farm";
  if (type.includes("quarry") || String(item.resource).toLowerCase() === "stone") type = "quarry";
  if (type === "ore" || type.includes("mine") || String(item.resource).toLowerCase() === "ore") type = "mine";
  if (type.includes("warehouse") || type.includes("stockpile")) type = "warehouse";
  if (type.includes("workshop") || type.includes("craft")) type = "workshop";
  if (type.includes("market") || type.includes("trade")) type = "market";
  return {
    ...item,
    id: String(item.id || item.nodeId || item.buildingId || item.projectId || item.name || type),
    name: cleanName(item.name || item.label || titleCase(item.id || type)),
    type,
    resource: String(item.resource || item.output || "").toLowerCase(),
    x: position.x,
    y: position.y,
    radius: clamp(item.radius ?? item.interactionRadius ?? 4, 1, 12),
    amount: Math.max(0, finite(item.amount ?? item.remaining ?? item.stock, 0)),
    maxAmount: Math.max(0, finite(item.maxAmount ?? item.capacity ?? item.initialAmount, 0)),
    progress: percentage(item.progress ?? item.completion ?? 0),
    status: String(item.status || item.state || "open"),
  };
}

function unwrapWorld(payload) {
  if (!payload) return null;
  return payload.world || payload.state?.world || payload.state || payload;
}

function normalizeWorld(payload) {
  const world = unwrapWorld(payload) || {};

  const rawActors = [
    ...collection(world.actors).map((actor) => normalizeActor(actor)),
    ...collection(world.players).map((actor) => normalizeActor(actor, "player")),
    ...collection(world.npcs).map((actor) => normalizeActor(actor, "npc")),
  ];
  const actors = uniqueById(rawActors);

  const rawHotspots = [
    ...collection(world.hotspots).map((item) => normalizeHotspot(item)),
    ...collection(world.nodes).map((item) => normalizeHotspot(item, item.type || item.resource || "resource")),
    ...(world.warehouse && typeof world.warehouse === "object" ? [normalizeHotspot({ id: "warehouse", ...world.warehouse }, "warehouse")] : []),
    ...collection(world.projects).map((item) => normalizeHotspot(item, "project")),
    ...collection(world.buildings).map((item) => normalizeHotspot(item, item.type || item.id || "building")),
    ...collection(world.buildingsList).map((item) => normalizeHotspot(item, item.type || item.id || "building")),
    ...(world.market && typeof world.market === "object" ? [normalizeHotspot({ id: "market", name: "Lantern Market", ...world.market }, "market")] : []),
  ];

  const hotspots = uniqueById(rawHotspots.length ? rawHotspots : DEFAULT_HOTSPOTS);
  const project = hotspots.find((item) => item.type === "project") || normalizeHotspot(DEFAULT_HOTSPOTS[4]);
  const civilization = world.civilization || world.civ || world.shared || {};
  const clock = world.clock || world.time || {};
  const buildings = hotspots.filter((item) => ["warehouse", "workshop", "market", "building", "project"].includes(item.type));
  const events = collection(world.events || world.chronicle || world.log).map((event, index) => {
    if (typeof event === "string") return { id: `event-${index}`, text: event };
    return {
      ...event,
      id: String(event.id || event.seq || `event-${index}`),
      text: String(event.text || event.message || event.summary || event.description || event.type || "The world changed."),
    };
  });
  if (events.length > 1) {
    const firstTime = finite(events[0].atMs ?? events[0].at ?? events[0].timestamp, NaN);
    const lastTime = finite(events.at(-1).atMs ?? events.at(-1).at ?? events.at(-1).timestamp, NaN);
    if (Number.isFinite(firstTime) && Number.isFinite(lastTime) && firstTime < lastTime) events.reverse();
  }

  return {
    source: world,
    revision: world.revision ?? world.tick ?? world.sequence ?? 0,
    actors,
    hotspots,
    buildings,
    project,
    civilization,
    clock: {
      day: Math.max(1, finite(clock.day ?? world.day, 1)),
      minuteOfDay: clamp(clock.minuteOfDay ?? clock.minutes ?? world.minuteOfDay ?? 440, 0, 1439),
      label: String(clock.label || ""),
      weather: String(clock.weather || world.weather || "riverlight"),
      season: String(clock.season || world.season?.age || world.season?.name || world.season?.id || (typeof world.season === "string" ? world.season : "Early Autumn")),
    },
    events,
  };
}

function demoWorld() {
  const now = Date.now();
  return {
    revision: 1,
    clock: { day: 18, minuteOfDay: 472, label: "07:52", weather: "riverlight", season: "Early Autumn" },
    players: {
      [ACTOR_ID]: {
        id: ACTOR_ID,
        name: ACTOR_NAME,
        kind: "player",
        x: 37,
        y: 38,
        inventory: { timber: 0, food: 1, stone: 0, ore: 0, goods: 0 },
        capacity: 8,
        profession: "Civic Runner",
        status: "ready",
      },
    },
    npcs: {
      tala: { id: "tala", name: "Tala Orin", kind: "npc", x: 61, y: 29, profession: "Riverkeeper", status: "inspecting the flow", inventory: {}, routePhase: 0.2 },
      oren: { id: "oren", name: "Oren Vale", kind: "npc", x: 18, y: 38, profession: "Maker", status: "shaping a valve", inventory: {}, routePhase: 1.8 },
      ves: { id: "ves", name: "Ves Calo", kind: "npc", x: 82, y: 32, profession: "Market Steward", status: "opening a stall", inventory: {}, routePhase: 3.1 },
    },
    nodes: Object.fromEntries(DEFAULT_HOTSPOTS.slice(0, 3).map((item) => [item.id, { ...item }])),
    warehouse: { id: "warehouse", name: "River Warehouse", x: 70, y: 20, radius: 4, stocks: { timber: 7, food: 16, stone: 5, goods: 1 } },
    buildings: {
      workshop: { ...DEFAULT_HOTSPOTS[5], status: "open" },
      market: { ...DEFAULT_HOTSPOTS[6], status: "trading" },
    },
    projects: {
      eastSluice: {
        ...DEFAULT_HOTSPOTS[4],
        status: "under construction",
        progress: 24,
        requirements: { timber: 12, stone: 8, goods: 3 },
        delivered: { timber: 7, stone: 5, goods: 1 },
      },
    },
    civilization: { water: 38, food: 61, cohesion: 72, prosperity: 29, waterRatePerMinute: -0.08 },
    events: [
      { id: "welcome", at: now, text: "The East Sluice work bell rang across Aster." },
      { id: "tala", at: now - 28000, text: "Tala began inspecting the eastern gate." },
      { id: "market", at: now - 54000, text: "The evening market opened its first stalls." },
    ],
    _lastAdvanced: now,
  };
}

async function requestJson(url, options = {}, timeoutMs = 2200) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal,
      headers: { "Content-Type": "application/json", Accept: "application/json", ...(options.headers || {}) },
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      const error = new Error(data.error || data.message || `World request failed (${response.status})`);
      error.status = response.status;
      throw error;
    }
    return data;
  } finally {
    window.clearTimeout(timeout);
  }
}

async function connect() {
  try {
    let payload;
    try {
      payload = await requestJson(`/api/world/session?session=${encodeURIComponent(SESSION)}&actor=${encodeURIComponent(ACTOR_ID)}`);
    } catch (error) {
      if (error.status !== 404) throw error;
    }

    const initial = payload ? normalizeWorld(payload) : null;
    if (!payload || !initial.actors.some((actor) => actor.id === ACTOR_ID)) {
      payload = await requestJson("/api/world/bootstrap", {
        method: "POST",
        body: JSON.stringify({ session: SESSION, actorId: ACTOR_ID, name: ACTOR_NAME }),
      });
    }

    state.transport = "online";
    ingestWorld(payload);
    setConnection(disclosureLabel(payload), "online");
    window.setInterval(pollOnline, POLL_MS);
  } catch (error) {
    console.info("Shared world gateway unavailable; starting local simulation.", error);
    await startOffline();
  }
}

async function pollOnline() {
  if (state.polling || state.transport !== "online") return;
  state.polling = true;
  try {
    const payload = await requestJson(`/api/world/session?session=${encodeURIComponent(SESSION)}&actor=${encodeURIComponent(ACTOR_ID)}`, {}, 1800);
    state.pollFailures = 0;
    ingestWorld(payload);
    setConnection(disclosureLabel(payload), "online");
  } catch (error) {
    state.pollFailures += 1;
    if (state.pollFailures > 1) setConnection("RECONNECTING…", "offline");
  } finally {
    state.polling = false;
  }
}

async function startOffline() {
  state.transport = "offline";
  setConnection("LOCAL WORLD · OFFLINE", "offline");

  try {
    state.offlineModule = await import("./core.mjs");
  } catch (error) {
    console.warn("Local simulation module could not load; using the embedded world.", error);
  }

  state.rawWorld = createOfflineWorld(state.offlineModule) || demoWorld();
  ensureOfflinePlayer();
  ingestWorld(state.rawWorld);
  window.setInterval(advanceOffline, 250);
}

function createOfflineWorld(module) {
  if (!module) return null;
  const factory = module.createWorld || module.createInitialWorld || module.bootstrapWorld || module.initialWorld;
  if (typeof factory !== "function") return null;
  const attempts = [
    () => factory({ session: SESSION, sessionId: SESSION, nowMs: Date.now() }),
    () => factory(Date.now()),
    () => factory(),
  ];
  for (const attempt of attempts) {
    try {
      const result = attempt();
      const world = unwrapWorld(result);
      if (world && typeof world === "object") return world;
    } catch (error) {
      // Try the next known factory signature.
    }
  }
  return null;
}

function ensureOfflinePlayer() {
  if (!state.rawWorld) return;
  const normalized = normalizeWorld(state.rawWorld);
  if (normalized.actors.some((actor) => actor.id === ACTOR_ID)) return;
  const joined = applyOfflineCoreAction({ type: "JOIN", actorId: ACTOR_ID, name: ACTOR_NAME });
  if (joined) return;
  state.rawWorld.players ||= {};
  state.rawWorld.players[ACTOR_ID] = {
    id: ACTOR_ID,
    name: ACTOR_NAME,
    kind: "player",
    x: 37,
    y: 38,
    inventory: { timber: 0, food: 1, stone: 0, ore: 0, goods: 0 },
    capacity: 8,
    profession: "Civic Runner",
    status: "ready",
  };
}

function applyOfflineCoreAction(action) {
  const module = state.offlineModule;
  if (!module || !state.rawWorld) return false;
  const reducer = module.applyAction || module.applyWorldAction || module.reduceWorld || module.dispatchAction;
  if (typeof reducer !== "function") return false;
  const attempts = [
    () => reducer(state.rawWorld, action, Date.now()),
    () => reducer(state.rawWorld, action),
    () => reducer({ world: state.rawWorld, action, nowMs: Date.now() }),
  ];
  for (const attempt of attempts) {
    try {
      const result = attempt();
      const next = unwrapWorld(result);
      if (next && typeof next === "object" && next !== action) state.rawWorld = next;
      return true;
    } catch (error) {
      // Try the next known reducer signature.
    }
  }
  return false;
}

function advanceOffline() {
  if (state.transport !== "offline" || !state.rawWorld) return;
  const module = state.offlineModule;
  const advance = module?.advanceWorld || module?.tickWorld || module?.stepWorld || module?.updateWorld;
  let advanced = false;
  if (typeof advance === "function") {
    const attempts = [
      () => advance(state.rawWorld, Date.now()),
      () => advance(state.rawWorld, 250),
      () => advance({ world: state.rawWorld, nowMs: Date.now(), deltaMs: 250 }),
    ];
    for (const attempt of attempts) {
      try {
        const result = attempt();
        const next = unwrapWorld(result);
        if (next && typeof next === "object") state.rawWorld = next;
        advanced = true;
        break;
      } catch (error) {
        // Try the next known simulation signature.
      }
    }
  }
  if (!advanced) advanceDemoWorld(state.rawWorld, Date.now());
  ingestWorld(state.rawWorld);
}

function findRawActor(world, actorId) {
  if (!world) return null;
  for (const key of ["players", "npcs", "actors"]) {
    const group = world[key];
    if (!group) continue;
    if (Array.isArray(group)) {
      const found = group.find((actor) => String(actor.id || actor.actorId) === actorId);
      if (found) return found;
    } else if (group[actorId]) return group[actorId];
  }
  return null;
}

function advanceDemoWorld(world, now) {
  const previous = finite(world._lastAdvanced, now - 250);
  const deltaSeconds = clamp((now - previous) / 1000, 0, 1);
  world._lastAdvanced = now;
  world.revision = finite(world.revision, 0) + 1;

  const player = findRawActor(world, ACTOR_ID);
  if (player?.target) {
    const dx = player.target.x - player.x;
    const dy = player.target.y - player.y;
    const distanceLeft = Math.hypot(dx, dy);
    const step = deltaSeconds * 10;
    if (distanceLeft <= step) {
      player.x = player.target.x;
      player.y = player.target.y;
      player.target = null;
      player.status = "ready";
    } else {
      player.x += (dx / distanceLeft) * step;
      player.y += (dy / distanceLeft) * step;
      player.status = "walking";
    }
  }

  for (const npc of collection(world.npcs)) {
    const phase = finite(npc.routePhase, 0) + now / 8000;
    const originX = npc.id === "tala" ? 61 : npc.id === "oren" ? 18 : 82;
    const originY = npc.id === "tala" ? 29 : npc.id === "oren" ? 38 : 32;
    npc.x = originX + Math.sin(phase) * 2.2;
    npc.y = originY + Math.cos(phase * 0.82) * 1.1;
  }

  if (world.clock) {
    world.clock.minuteOfDay = (finite(world.clock.minuteOfDay, 440) + deltaSeconds * 0.7) % 1440;
    world.clock.label = formatClock(world.clock.minuteOfDay);
  }
}

function ingestWorld(payload) {
  if (payload?.disclosure) state.disclosure = cleanDisclosure(payload.disclosure);
  const raw = unwrapWorld(payload);
  if (!raw || typeof raw !== "object") return;
  state.rawWorld = raw;
  state.world = normalizeWorld(raw);
  state.lastIngestAt = performance.now();
  renderWorld();
  if (dom.shell.classList.contains("is-loading")) {
    window.setTimeout(() => dom.shell.classList.remove("is-loading"), 250);
  }
}

function renderWorld() {
  renderHud();
  renderHotspots();
  renderActors();
  renderParcels();
  renderContext();
}

function renderHud() {
  const world = state.world;
  if (!world) return;
  const player = playerActor();
  const civ = world.civilization;

  setVital("Water", civ.water ?? civ.waterLevel ?? civ.waterReserve ?? 0);
  setVital("Food", civ.food ?? civ.foodReserve ?? 0);
  setVital("Cohesion", civ.cohesion ?? civ.morale ?? 0);
  setVital("Prosperity", civ.prosperity ?? civ.wealth ?? 0);

  const clock = world.clock;
  $("#clockLabel").textContent = clock.label || formatClock(clock.minuteOfDay);
  $("#seasonLabel").textContent = `${clock.season.toUpperCase()} · DAY ${Math.round(clock.day)}`;
  $("#weatherIcon").textContent = weatherIcon(clock.weather);
  $("#worldRevision").textContent = `R${world.revision}`;

  const project = world.project;
  const progress = percentage(project.progress);
  const projectComplete = project.status === "complete" || progress >= 100;
  $("#missionTitle").textContent = projectComplete ? "The East Sluice is flowing" : project.name || "Restore the East Sluice";
  $("#missionDescription").textContent = projectComplete
    ? `Water reaches Aster at ${finite(civ.waterRatePerMinute, 0).toFixed(1)}/min. Irrigated farms now produce at ×${finite(civ.farmProductionMultiplier, 1).toFixed(2)}.`
    : project.description || "Gather supplies, carry them through Aster, and rebuild the waterworks together.";
  $("#projectProgress").style.width = `${progress}%`;
  $("#projectProgressLabel").textContent = `${Math.round(progress)}%`;
  renderProjectNeeds(project);
  renderBuildingStatus(world.buildings);
  dom.viewport.classList.toggle("sluice-complete", projectComplete);

  if (player) {
    $("#playerName").textContent = player.name;
    $("#playerProfession").textContent = player.profession.toUpperCase();
    renderInventory(player);
    renderJob(player);
  }

  renderEvents(world.events);
  const mostRecent = world.events[0]?.text;
  $("#statusLine").textContent = player?.status && player.status !== "ready" ? titleCase(player.status) : mostRecent || "The city is waking.";
}

function setVital(name, rawValue) {
  const value = finite(rawValue, 0);
  const display = Math.abs(value) >= 1000 ? `${(value / 1000).toFixed(1)}K` : Math.round(value);
  $(`#vital${name}`).textContent = display;
  $(`#bar${name}`).style.width = `${clamp(value, 0, 100)}%`;
}

function renderProjectNeeds(project) {
  const container = $("#projectNeeds");
  const perStepCost = project.buildCost || project.cost || project.needs || {};
  const required = project.requirements || (project.id === "east-sluice"
    ? Object.fromEntries(Object.entries(perStepCost).map(([item, quantity]) => [item, finite(quantity) * 4]))
    : perStepCost);
  const delivered = project.delivered || project.materialsUsed || project.contributions || project.supplied || {};
  let keys = Object.keys(required);
  if (!keys.length) keys = ["timber", "stone", "goods"];
  container.innerHTML = keys.slice(0, 4).map((key) => {
    const needed = finite(required[key], key === "timber" ? 12 : key === "stone" ? 8 : 3);
    const has = finite(delivered[key], 0);
    return `<span class="need-chip ${has >= needed ? "met" : ""}">${escapeHtml(key.toUpperCase())} ${Math.round(has)}/${Math.round(needed)}</span>`;
  }).join("");
}

function renderBuildingStatus(buildings) {
  $("#buildingStatus").innerHTML = buildings.slice(0, 3).map((building) => {
    const status = building.progress > 0 && building.progress < 100 ? `${Math.round(building.progress)}%` : titleCase(building.status);
    const stalled = /stall|closed|damaged|blocked/.test(building.status.toLowerCase());
    return `<span class="building-state ${stalled ? "is-stalled" : ""}" title="${escapeHtml(building.name)}">${escapeHtml(building.name)} · ${escapeHtml(status)}</span>`;
  }).join("");
}

function renderInventory(player) {
  const inventory = player.inventory;
  const preferred = ["timber", "food", "stone", "ore", "goods"];
  const extras = Object.keys(inventory).filter((key) => !preferred.includes(key));
  const keys = [...preferred, ...extras].slice(0, 5);
  $("#inventoryGrid").innerHTML = keys.map((key) => {
    const amount = finite(inventory[key], 0);
    const meta = ITEM_META[key] || { icon: "•", name: titleCase(key) };
    return `<div class="inventory-slot ${amount <= 0 ? "is-empty" : ""}" title="${escapeHtml(meta.name)}"><i>${meta.icon}</i><span>${Math.round(amount)}</span></div>`;
  }).join("");
  const load = Object.values(inventory).reduce((sum, value) => sum + finite(value, 0), 0);
  const capacity = Math.max(1, player.capacity);
  $("#capacityLabel").textContent = `${Math.round(load)}/${Math.round(capacity)}`;
  $("#capacityRing").style.setProperty("--capacity", `${clamp((load / capacity) * 100, 0, 100)}%`);
}

function renderJob(player) {
  const row = $("#jobRow");
  if (!player.job) {
    row.hidden = true;
    return;
  }
  const job = player.job;
  const progress = jobProgress(job);
  row.hidden = false;
  $("#jobLabel").textContent = cleanName(job.label || job.name || job.type || player.status) || "Working";
  $("#jobProgress").style.width = `${progress}%`;
}

function jobProgress(job) {
  if (!job) return 0;
  if (job.progress != null) return percentage(job.progress);
  if (job.durationMs && job.remainingMs != null) return clamp((1 - finite(job.remainingMs) / finite(job.durationMs, 1)) * 100, 0, 100);
  const start = finite(job.startedAt ?? job.startTime, 0);
  const end = finite(job.endsAt ?? job.endTime, 0);
  if (start && end > start) return clamp(((Date.now() - start) / (end - start)) * 100, 0, 100);
  return 20;
}

function renderEvents(events) {
  const visible = events.slice(0, 4);
  $("#eventFeed").innerHTML = visible.length
    ? visible.map((event) => `<li><time>${eventTime(event)}</time>${escapeHtml(event.text)}</li>`).join("")
    : `<li><time>NOW</time>Aster waits for its citizens.</li>`;

  const eventKey = visible[0]?.id || visible[0]?.text || "";
  const newestType = String(visible[0]?.type || "").toUpperCase();
  if (state.lastEventKey && eventKey && state.lastEventKey !== eventKey && /PROJECT_COMPLETED|WEATHER_CHANGED/.test(newestType)) {
    toast(visible[0].text);
  }
  state.lastEventKey = eventKey;
}

function eventTime(event) {
  const minute = event.minuteOfDay ?? event.minute;
  if (minute != null) return formatClock(minute);
  const timestamp = finite(event.atMs ?? event.at ?? event.timestamp ?? event.createdAt, 0);
  if (timestamp) return new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return "NOW";
}

function renderHotspots() {
  if (!state.world) return;
  const seen = new Set();
  for (const hotspot of state.world.hotspots) {
    seen.add(hotspot.id);
    let element = state.hotspots.get(hotspot.id);
    if (!element) {
      element = document.createElement("button");
      element.type = "button";
      element.className = "hotspot";
      element.addEventListener("click", (event) => {
        event.stopPropagation();
        selectHotspot(hotspot.id);
      });
      state.hotspots.set(hotspot.id, element);
      dom.hotspotLayer.append(element);
    }
    const meta = hotspotMeta(hotspot);
    element.dataset.type = hotspot.type;
    element.dataset.status = hotspot.status.toLowerCase();
    element.dataset.depleted = String(hotspot.maxAmount > 0 && hotspot.amount <= 0);
    element.style.left = `${hotspot.x}%`;
    element.style.top = `${(hotspot.y / MAP_HEIGHT) * 100}%`;
    element.setAttribute("aria-label", `${hotspot.name}. ${meta.hint}`);
    element.innerHTML = `${hotspot.progress > 0 ? `<i class="hotspot-progress" style="--progress:${hotspot.progress}%"></i>` : ""}<span class="hotspot-label"><strong>${escapeHtml(hotspot.name)}</strong><small>${escapeHtml(hotspotStatus(hotspot))}</small></span>`;
  }
  for (const [id, element] of state.hotspots) {
    if (seen.has(id)) continue;
    element.remove();
    state.hotspots.delete(id);
  }
}

function renderParcels() {
  if (!state.world) return;
  const parcels = [];
  for (const hotspot of state.world.hotspots) {
    if (!hotspot.resource || hotspot.amount <= 0) continue;
    const ratio = hotspot.maxAmount ? hotspot.amount / hotspot.maxAmount : 0.7;
    const count = clamp(Math.ceil(ratio * 3), 1, 3);
    for (let index = 0; index < count; index += 1) {
      const angle = hashNumber(`${hotspot.id}-${index}`) * Math.PI * 2;
      const offset = 1.2 + index * 0.65;
      parcels.push({
        id: `${hotspot.id}-${index}`,
        icon: (ITEM_META[hotspot.resource] || ITEM_META.stone).icon,
        x: hotspot.x + Math.cos(angle) * offset,
        y: hotspot.y + Math.sin(angle) * offset,
      });
    }
  }
  dom.parcelLayer.innerHTML = parcels.map((parcel) => `<span class="resource-parcel" style="left:${parcel.x}%;top:${(parcel.y / MAP_HEIGHT) * 100}%">${parcel.icon}</span>`).join("");
}

function createActorElement(actor) {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "actor";
  element.setAttribute("aria-label", actor.name);
  element.innerHTML = `
    <i class="actor-shadow"></i>
    <div class="player-sprite" aria-hidden="true"></div>
    <div class="figure" aria-hidden="true">
      <i class="figure-head"></i><i class="figure-body"></i>
      <i class="figure-arm left"></i><i class="figure-arm right"></i>
      <i class="figure-leg left"></i><i class="figure-leg right"></i>
    </div>
    <span class="actor-tool"></span>
    <span class="actor-label"></span>
    <i class="actor-status"></i>`;
  element.addEventListener("click", (event) => {
    event.stopPropagation();
    state.selected = { kind: "actor", id: actor.id };
    renderContext();
  });
  dom.actorLayer.append(element);
  return element;
}

function renderActors() {
  if (!state.world) return;
  const seen = new Set();
  const now = performance.now();
  const visualPositions = deconflictActorPositions(state.world.actors);
  for (const actor of state.world.actors) {
    seen.add(actor.id);
    let element = state.actors.get(actor.id);
    if (!element) {
      element = createActorElement(actor);
      state.actors.set(actor.id, element);
    }

    const isPlayer = actor.id === ACTOR_ID;
    const visual = visualPositions.get(actor.id) || actor;
    const track = state.actorTracks.get(actor.id);
    if (!track) {
      state.actorTracks.set(actor.id, {
        fromX: visual.x, fromY: visual.y, toX: visual.x, toY: visual.y,
        currentX: visual.x, currentY: visual.y, startedAt: now, duration: POLL_MS,
      });
    } else if (Math.abs(track.toX - visual.x) > 0.001 || Math.abs(track.toY - visual.y) > 0.001) {
      track.fromX = track.currentX;
      track.fromY = track.currentY;
      track.toX = visual.x;
      track.toY = visual.y;
      track.startedAt = now;
      track.duration = POLL_MS + 35;
    }

    const activeTrack = state.actorTracks.get(actor.id);
    const dx = activeTrack.toX - activeTrack.fromX;
    const dy = activeTrack.toY - activeTrack.fromY;
    const moving = actor.target || Math.hypot(dx, dy) > 0.08 || /walk|move|haul|deliver/.test(actor.status.toLowerCase());
    const working = /work|build|gather|mine|farm|craft|inspect|repair/.test(actor.status.toLowerCase()) || Boolean(actor.job);
    const direction = movementDirection(dx, dy, element.dataset.direction || "down");
    element.dataset.direction = direction;
    element.className = `actor ${isPlayer ? "is-player" : "is-npc"} ${moving ? "is-moving" : ""} ${working ? "is-working" : ""} direction-${direction} ${isPlayer && state.spriteAvailable ? "has-sprite" : ""}`;
    element.style.setProperty("--actor-color", actorColor(actor));
    element.style.setProperty("--actor-accent", actorAccent(actor));
    element.title = `${actor.name} · ${actor.profession} · ${actor.status}`;
    $(".actor-label", element).textContent = isPlayer ? `${actor.name} · YOU` : actor.name;
    $(".actor-tool", element).textContent = professionIcon(actor.profession);
    $(".actor-status", element).style.background = working ? "#edbe6f" : moving ? "#80c7b7" : "#99d6a2";
  }

  for (const [id, element] of state.actors) {
    if (seen.has(id)) continue;
    element.remove();
    state.actors.delete(id);
    state.actorTracks.delete(id);
  }
}

function deconflictActorPositions(actors) {
  const positions = new Map();
  const placed = [];
  const ordered = [...actors].sort((left, right) => Number(right.id === ACTOR_ID) - Number(left.id === ACTOR_ID) || left.id.localeCompare(right.id));
  for (const actor of ordered) {
    let x = actor.x;
    let y = actor.y;
    const overlaps = (candidateX, candidateY) => placed.some((entry) => Math.hypot(entry.x - candidateX, entry.y - candidateY) < 4.8);
    if (overlaps(x, y)) {
      const baseAngle = hashNumber(actor.id) * Math.PI * 2;
      for (let attempt = 1; attempt <= 14; attempt += 1) {
        const radius = 2.8 + Math.floor((attempt - 1) / 4) * 1.55;
        const angle = baseAngle + attempt * 2.39996;
        const candidateX = clamp(actor.x + Math.cos(angle) * radius, 0, MAP_WIDTH);
        const candidateY = clamp(actor.y + Math.sin(angle) * radius, 0, MAP_HEIGHT);
        x = candidateX;
        y = candidateY;
        if (!overlaps(candidateX, candidateY)) break;
      }
    }
    positions.set(actor.id, { x, y });
    placed.push({ x, y });
  }
  return positions;
}

function animateActors(now) {
  for (const [id, track] of state.actorTracks) {
    const element = state.actors.get(id);
    if (!element) continue;
    const progress = clamp((now - track.startedAt) / Math.max(1, track.duration), 0, 1);
    track.currentX = track.fromX + (track.toX - track.fromX) * progress;
    track.currentY = track.fromY + (track.toY - track.fromY) * progress;
    element.style.left = `${track.currentX}%`;
    element.style.top = `${(track.currentY / MAP_HEIGHT) * 100}%`;
    element.style.zIndex = String(20 + Math.round(track.currentY));
  }
  window.requestAnimationFrame(animateActors);
}

function renderContext() {
  if (!state.world) return;
  const player = playerActor();
  if (!player) return;

  const interaction = interactionTarget(player);
  updateNearbyCard(interaction);
  updateHotspotClasses(player, interaction);
  renderActionDock(player, interaction);

  const nearestDistrict = [...state.world.hotspots]
    .sort((a, b) => distance(player, a) - distance(player, b))[0];
  if (nearestDistrict) $("#districtLabel").textContent = `ASTER · ${nearestDistrict.name.toUpperCase()}`;
}

function interactionTarget(player) {
  const candidates = [
    ...state.world.hotspots.map((hotspot) => ({ ...hotspot, kind: "hotspot", distance: distance(player, hotspot) })),
    ...state.world.actors.filter((actor) => actor.kind === "npc").map((actor) => ({ ...actor, kind: "actor", type: "npc", radius: 3.2, distance: distance(player, actor) })),
  ];
  let selected = null;
  if (state.selected) {
    selected = candidates.find((candidate) => candidate.kind === state.selected.kind && candidate.id === state.selected.id) || null;
  }
  if (selected) return selected;
  const nearest = candidates.sort((a, b) => a.distance - b.distance)[0];
  return nearest && nearest.distance <= Math.max(nearest.radius + 4, 9) ? nearest : null;
}

function updateNearbyCard(target) {
  if (!target) {
    $("#nearbyGlyph").textContent = "⌖";
    $("#nearbyName").textContent = "Walk into the world";
    $("#nearbyHint").textContent = "Click anywhere to move";
    $("#nearbyDistance").textContent = "—";
    return;
  }
  const meta = hotspotMeta(target);
  $("#nearbyGlyph").textContent = meta.icon;
  $("#nearbyName").textContent = target.name;
  $("#nearbyHint").textContent = target.kind === "actor" ? `${target.profession} · ${target.status}` : meta.hint;
  $("#nearbyDistance").textContent = `${target.distance.toFixed(1)}m`;
}

function updateHotspotClasses(player, target) {
  for (const hotspot of state.world.hotspots) {
    const element = state.hotspots.get(hotspot.id);
    if (!element) continue;
    const near = distance(player, hotspot) <= hotspot.radius + 1.4;
    element.classList.toggle("is-near", near);
    element.classList.toggle("is-selected", target?.kind === "hotspot" && target.id === hotspot.id);
  }
}

function renderActionDock(player, target) {
  const actions = [];
  if (player.job) {
    const label = titleCase(player.job.label || player.job.name || player.job.type || "Working");
    dom.actionContextIcon.textContent = "◌";
    dom.actionContextLabel.textContent = label;
    actions.push({ label, kicker: "In progress", key: "…", disabled: true });
    actions.push({ label: "Find me", kicker: "Camera", key: "F", local: "focus-player" });
    state.actions = actions;
    dom.actionButtons.innerHTML = actions.map((descriptor, index) => `
      <button class="action-button ${descriptor.primary ? "primary" : ""}" data-action-index="${index}" ${descriptor.disabled ? "disabled" : ""}>
        <kbd>${escapeHtml(descriptor.key || String(index + 1))}</kbd>
        <span><small>${escapeHtml(descriptor.kicker || "ACTION")}</small><strong>${escapeHtml(descriptor.label)}</strong></span>
      </button>`).join("");
    return;
  }
  if (!target) {
    dom.actionContextIcon.textContent = "⌖";
    dom.actionContextLabel.textContent = "Choose a place in the world";
  } else {
    const meta = hotspotMeta(target);
    const near = target.distance <= finite(target.radius, 3) + 1.5;
    dom.actionContextIcon.textContent = meta.icon;
    dom.actionContextLabel.textContent = near ? target.name : `Walk to ${target.name}`;

    if (!near) {
      actions.push({ label: "Walk there", kicker: target.name, key: "1", primary: true, local: "walk-target", target });
    } else if (target.kind === "actor") {
      actions.push({ label: "Talk", kicker: target.profession, key: "1", primary: true, action: { type: "TALK", npcId: target.id } });
    } else {
      actions.push(...actionsForHotspot(target, player));
    }
  }

  actions.push({ label: "Find me", kicker: "Camera", key: "F", local: "focus-player", primary: actions.length === 0 });
  state.actions = actions;
  dom.actionButtons.innerHTML = actions.map((descriptor, index) => `
    <button class="action-button ${descriptor.primary ? "primary" : ""}" data-action-index="${index}" ${descriptor.disabled ? "disabled" : ""}>
      <kbd>${escapeHtml(descriptor.key || String(index + 1))}</kbd>
      <span><small>${escapeHtml(descriptor.kicker || "ACTION")}</small><strong>${escapeHtml(descriptor.label)}</strong></span>
    </button>`).join("");
}

function actionsForHotspot(target, player) {
  const meta = hotspotMeta(target);
  const carried = Object.values(player.inventory).reduce((sum, value) => sum + finite(value, 0), 0);
  if (["forest", "farm", "quarry", "mine", "ore", "resource"].includes(target.type)) {
    const unavailable = target.amount < 1 || carried >= player.capacity;
    return [{
      label: unavailable ? (target.amount < 1 ? "Site is resting" : "Pack is full") : meta.verb,
      kicker: target.resource || target.type,
      key: "1",
      primary: true,
      disabled: unavailable,
      action: { type: "GATHER", nodeId: target.id }
    }];
  }
  if (target.type === "warehouse") {
    return [{
      label: carried > 0 ? "Deposit all" : "Pack is empty",
      kicker: "Shared stock",
      key: "1",
      primary: true,
      disabled: carried <= 0,
      action: { type: "DEPOSIT" }
    }];
  }
  if (target.type === "project") {
    if (target.status === "complete" || target.progress >= 100) {
      return [{ label: "Water is flowing", kicker: "Project complete", key: "✓", primary: true, disabled: true }];
    }
    const stocks = state.world?.source?.warehouse?.stocks || {};
    const cost = target.buildCost || { timber: 2, stone: 1 };
    const hasMaterials = Object.entries(cost).every(([item, quantity]) => finite(stocks[item], 0) >= finite(quantity, 0));
    return [{
      label: hasMaterials ? "Contribute" : "Awaiting materials",
      kicker: hasMaterials ? "Civic project" : "Need 2 timber · 1 stone",
      key: "1",
      primary: true,
      disabled: !hasMaterials,
      action: { type: "BUILD", projectId: target.id }
    }];
  }
  if (target.type === "workshop") {
    const canCraft = finite(player.inventory.stone, 0) >= 1 && finite(player.inventory.ore, 0) >= 1;
    return [{ label: canCraft ? "Craft sluice parts" : "Missing materials", kicker: "1 stone · 1 ore", key: "1", primary: true, disabled: !canCraft, action: { type: "CRAFT", recipeId: "parts" } }];
  }
  if (target.type === "market") {
    return [
      { label: "Buy food", kicker: "Market", key: "1", primary: true, action: { type: "BUY", item: "food", quantity: 1 } },
      { label: finite(player.inventory.goods, 0) > 0 ? "Sell goods" : "No goods to sell", kicker: "Market", key: "2", disabled: finite(player.inventory.goods, 0) <= 0, action: { type: "SELL", item: "goods", quantity: 1 } },
    ];
  }
  return [{ label: "Work here", kicker: target.name, key: "1", primary: true, action: { type: "BUILD", projectId: target.id } }];
}

function selectHotspot(id) {
  const hotspot = state.world?.hotspots.find((item) => item.id === id);
  if (!hotspot) return;
  state.selected = { kind: "hotspot", id };
  focusWorldPoint(hotspot.x, hotspot.y, Math.max(state.zoom, 1.42));
  const player = playerActor();
  if (player && distance(player, hotspot) > hotspot.radius + 1.5) {
    moveTo(hotspot.x, hotspot.y, `Walking to ${hotspot.name}`);
  }
  renderContext();
}

async function dispatchAction(action, quiet = false) {
  if (!action) return;
  const canonical = { ...action };
  delete canonical.actorId;
  try {
    if (state.transport === "online") {
      const payload = await requestJson("/api/world/action", {
        method: "POST",
        body: JSON.stringify({ session: SESSION, actorId: ACTOR_ID, action: canonical }),
      });
      ingestWorld(payload);
    } else {
      const withActor = { ...canonical, actorId: ACTOR_ID };
      if (!applyOfflineCoreAction(withActor)) applyDemoAction(state.rawWorld, withActor);
      ingestWorld(state.rawWorld);
    }
    if (!quiet && canonical.type !== "MOVE") toast(actionSuccessLabel(canonical));
  } catch (error) {
    toast(error.message || "That action could not be completed.", true);
  }
}

function applyDemoAction(world, action) {
  const player = findRawActor(world, action.actorId || ACTOR_ID);
  if (!player) return;
  player.inventory ||= {};
  const normalized = normalizeWorld(world);
  const playerView = normalized.actors.find((actor) => actor.id === ACTOR_ID);

  if (action.type === "MOVE") {
    player.target = { x: clamp(action.x, 0, MAP_WIDTH), y: clamp(action.y, 0, MAP_HEIGHT) };
    player.status = "walking";
    return;
  }

  const targetId = action.nodeId || action.projectId || action.npcId;
  const target = [...normalized.hotspots, ...normalized.actors].find((item) => item.id === targetId);
  if (target && playerView && distance(playerView, target) > finite(target.radius, 3) + 2) {
    throw new Error(`Move closer to ${target.name} first.`);
  }

  if (action.type === "GATHER") {
    const node = findRawEntity(world.nodes, action.nodeId);
    if (!node || finite(node.amount, 0) <= 0) throw new Error("That resource site is resting.");
    const resource = node.resource || (String(node.type).includes("farm") ? "food" : String(node.type).includes("forest") ? "timber" : "stone");
    const load = Object.values(player.inventory).reduce((sum, value) => sum + finite(value, 0), 0);
    if (load >= finite(player.capacity, 8)) throw new Error("Your pack is full. Deposit supplies at the warehouse.");
    node.amount = Math.max(0, finite(node.amount, 0) - 1);
    player.inventory[resource] = finite(player.inventory[resource], 0) + 1;
    player.status = `gathering ${resource}`;
    addDemoEvent(world, `${player.name} gathered ${resource} for Aster.`);
  } else if (action.type === "DEPOSIT") {
    world.warehouse.stocks ||= {};
    let total = 0;
    for (const [item, amount] of Object.entries(player.inventory)) {
      total += finite(amount, 0);
      world.warehouse.stocks[item] = finite(world.warehouse.stocks[item], 0) + finite(amount, 0);
      player.inventory[item] = 0;
    }
    if (!total) throw new Error("Your pack is empty.");
    player.status = "unloading supplies";
    addDemoEvent(world, `${player.name} deposited ${total} supplies in the shared warehouse.`);
  } else if (action.type === "BUILD") {
    const project = findRawEntity(world.projects, action.projectId);
    if (!project) throw new Error("That civic project is unavailable.");
    project.progress = clamp(finite(project.progress, 0) + 4, 0, 100);
    world.civilization.cohesion = clamp(finite(world.civilization.cohesion, 50) + 1, 0, 100);
    player.status = "repairing the sluice";
    addDemoEvent(world, `${player.name} advanced ${project.name} to ${Math.round(project.progress)}%.`);
  } else if (action.type === "CRAFT") {
    if (finite(player.inventory.timber, 0) < 1) throw new Error("You need timber to craft planks.");
    player.inventory.timber -= 1;
    player.inventory.goods = finite(player.inventory.goods, 0) + 1;
    player.status = "crafting planks";
    addDemoEvent(world, `${player.name} crafted civic goods at the workshop.`);
  } else if (action.type === "BUY") {
    player.inventory.food = finite(player.inventory.food, 0) + 1;
    player.status = "trading";
    addDemoEvent(world, `${player.name} bought provisions at the market.`);
  } else if (action.type === "SELL") {
    if (finite(player.inventory.goods, 0) < 1) throw new Error("You have no goods to sell.");
    player.inventory.goods -= 1;
    world.civilization.prosperity = clamp(finite(world.civilization.prosperity, 0) + 2, 0, 100);
    player.status = "trading";
    addDemoEvent(world, `${player.name} sold crafted goods. Aster prospered.`);
  } else if (action.type === "TALK") {
    const npc = findRawActor(world, action.npcId);
    player.status = `speaking with ${npc?.name || "a citizen"}`;
    addDemoEvent(world, `${npc?.name || "A citizen"} shared a memory of the old river.`);
  }
}

function findRawEntity(group, id) {
  if (!group) return null;
  if (Array.isArray(group)) return group.find((item) => String(item.id) === String(id));
  return group[id] || Object.values(group).find((item) => String(item.id) === String(id));
}

function addDemoEvent(world, text) {
  world.events ||= [];
  world.events.unshift({ id: `event-${Date.now()}-${Math.random().toString(16).slice(2)}`, at: Date.now(), text });
  world.events = world.events.slice(0, 12);
}

function moveTo(x, y, status = "Walking through Aster") {
  const targetX = clamp(x, 0, MAP_WIDTH);
  const targetY = clamp(y, 0, MAP_HEIGHT);
  showMoveMarker(targetX, targetY);
  $("#statusLine").textContent = status;
  dispatchAction({ type: "MOVE", x: Number(targetX.toFixed(2)), y: Number(targetY.toFixed(2)) }, true);
}

function showMoveMarker(x, y) {
  dom.marker.style.left = `${x}%`;
  dom.marker.style.top = `${(y / MAP_HEIGHT) * 100}%`;
  dom.marker.classList.remove("show");
  void dom.marker.offsetWidth;
  dom.marker.classList.add("show");
}

function playerActor() {
  return state.world?.actors.find((actor) => actor.id === ACTOR_ID)
    || state.world?.actors.find((actor) => actor.kind === "player")
    || null;
}

function distance(a, b) {
  return Math.hypot(finite(a.x) - finite(b.x), finite(a.y) - finite(b.y));
}

function hotspotMeta(hotspot) {
  return HOTSPOT_META[hotspot.type] || HOTSPOT_META.building;
}

function hotspotStatus(hotspot) {
  if (hotspot.resource && hotspot.maxAmount) return `${Math.round(hotspot.amount)}/${Math.round(hotspot.maxAmount)} ${hotspot.resource}`;
  if (hotspot.progress > 0 && hotspot.progress < 100) return `${Math.round(hotspot.progress)}% complete`;
  return hotspot.status || hotspotMeta(hotspot).hint;
}

function formatClock(minuteOfDay) {
  const minute = Math.round(finite(minuteOfDay, 0)) % 1440;
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

function weatherIcon(weather) {
  const value = String(weather).toLowerCase();
  if (value.includes("rain") || value.includes("storm")) return "☂";
  if (value.includes("cloud") || value.includes("mist")) return "☁";
  if (value.includes("night") || value.includes("moon")) return "☾";
  if (value.includes("wind")) return "≋";
  return "☀";
}

function movementDirection(dx, dy, fallback) {
  if (Math.abs(dx) < 0.02 && Math.abs(dy) < 0.02) return fallback;
  if (Math.abs(dx) > Math.abs(dy)) return dx < 0 ? "left" : "right";
  return dy < 0 ? "up" : "down";
}

function professionIcon(profession) {
  const value = String(profession).toLowerCase();
  if (/river|water/.test(value)) return "≈";
  if (/maker|smith|craft|build/.test(value)) return "⚒";
  if (/farm|grow/.test(value)) return "♢";
  if (/market|trade|merchant/.test(value)) return "◈";
  if (/guard|warden|soldier/.test(value)) return "†";
  if (/scout|runner|envoy/.test(value)) return "⌁";
  return "✦";
}

function actorColor(actor) {
  const palette = ["#276c68", "#675c82", "#8c5c49", "#53724d", "#7d5d42", "#3f6278"];
  return palette[Math.floor(hashNumber(actor.id) * palette.length) % palette.length];
}

function actorAccent(actor) {
  const palette = ["#f1c675", "#dd9c72", "#a9c982", "#83c7bd"];
  return palette[Math.floor(hashNumber(`${actor.id}-accent`) * palette.length) % palette.length];
}

function hashNumber(value) {
  let hash = 2166136261;
  for (const char of String(value)) {
    hash ^= char.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4294967295;
}

function actionSuccessLabel(action) {
  const labels = {
    GATHER: "Work begun. The resource will be added to your pack.",
    DEPOSIT: "Supplies delivered to the shared civilization.",
    BUILD: "Your contribution changed the East Sluice.",
    CRAFT: "The workshop has begun shaping your materials.",
    BUY: "Trade accepted by the market.",
    SELL: "Goods sold. Aster’s prosperity grew.",
    TALK: "A citizen’s memory has entered your story.",
  };
  return labels[action.type] || "The world answered your action.";
}

function setConnection(label, mode) {
  dom.connection.className = `connection-state is-${mode}`;
  $("span", dom.connection).textContent = label;
}

function disclosureLabel(payload) {
  const disclosure = cleanDisclosure(payload?.disclosure || state.disclosure || "OFFCHAIN SIMULATION ALPHA");
  return disclosure || "OFFCHAIN SIMULATION ALPHA";
}

function toast(message, error = false) {
  if (!message) return;
  const element = document.createElement("div");
  element.className = `toast ${error ? "is-error" : ""}`;
  element.textContent = cleanName(message) || "The world changed.";
  dom.toastRegion.append(element);
  window.setTimeout(() => element.remove(), 3500);
}

function setZoom(nextZoom) {
  state.zoom = clamp(nextZoom, 1, 1.65);
  if (state.zoom <= 1.001) {
    state.panX = 0;
    state.panY = 0;
  } else {
    clampCameraPan();
  }
  applyCamera();
}

function focusWorldPoint(x, y, zoom = state.zoom) {
  state.zoom = clamp(zoom, 1, 1.65);
  const rect = dom.viewport.getBoundingClientRect();
  state.panX = (0.5 - clamp(x, 0, MAP_WIDTH) / MAP_WIDTH) * rect.width * state.zoom;
  state.panY = (0.5 - clamp(y, 0, MAP_HEIGHT) / MAP_HEIGHT) * rect.height * state.zoom;
  clampCameraPan();
  applyCamera();
}

function clampCameraPan() {
  const rect = dom.viewport.getBoundingClientRect();
  const maxX = Math.max(0, ((state.zoom - 1) * rect.width) / 2);
  const maxY = Math.max(0, ((state.zoom - 1) * rect.height) / 2);
  state.panX = clamp(state.panX, -maxX, maxX);
  state.panY = clamp(state.panY, -maxY, maxY);
}

function applyCamera() {
  dom.camera.style.setProperty("--zoom", state.zoom.toFixed(2));
  dom.camera.style.setProperty("--pan-x", `${state.panX.toFixed(1)}px`);
  dom.camera.style.setProperty("--pan-y", `${state.panY.toFixed(1)}px`);
}

dom.viewport.addEventListener("click", (event) => {
  if (event.target.closest(".hotspot, .actor")) return;
  const rect = dom.plane.getBoundingClientRect();
  const x = clamp(((event.clientX - rect.left) / rect.width) * MAP_WIDTH, 0, MAP_WIDTH);
  const y = clamp(((event.clientY - rect.top) / rect.height) * MAP_HEIGHT, 0, MAP_HEIGHT);
  state.selected = null;
  moveTo(x, y);
  renderContext();
});

dom.viewport.addEventListener("wheel", (event) => {
  event.preventDefault();
  setZoom(state.zoom + (event.deltaY < 0 ? 0.1 : -0.1));
}, { passive: false });

$("#zoomIn").addEventListener("click", () => setZoom(state.zoom + 0.12));
$("#zoomOut").addEventListener("click", () => setZoom(state.zoom - 0.12));

dom.actionButtons.addEventListener("click", (event) => {
  const button = event.target.closest("[data-action-index]");
  if (!button) return;
  const descriptor = state.actions[Number(button.dataset.actionIndex)];
  runDescriptor(descriptor);
});

function runDescriptor(descriptor) {
  if (!descriptor || descriptor.disabled) return;
  if (descriptor.local === "focus-player") {
    const player = playerActor();
    if (player) {
      focusWorldPoint(player.x, player.y, Math.max(state.zoom, 1.3));
      showMoveMarker(player.x, player.y);
      toast("You are here.");
    }
  } else if (descriptor.local === "walk-target") {
    moveTo(descriptor.target.x, descriptor.target.y, `Walking to ${descriptor.target.name}`);
  } else if (descriptor.action) {
    dispatchAction(descriptor.action);
  }
}

window.addEventListener("keydown", (event) => {
  if (event.target.matches("input, textarea, select")) return;
  const key = event.key.toLowerCase();
  if (["w", "a", "s", "d", "arrowup", "arrowdown", "arrowleft", "arrowright"].includes(key)) {
    event.preventDefault();
    state.heldKeys.add(key);
  }
  if (key === "f") runDescriptor({ local: "focus-player" });
  const actionIndex = Number(key) - 1;
  if (actionIndex >= 0 && actionIndex < state.actions.length) runDescriptor(state.actions[actionIndex]);
});

window.addEventListener("keyup", (event) => state.heldKeys.delete(event.key.toLowerCase()));
window.addEventListener("blur", () => state.heldKeys.clear());
window.addEventListener("resize", () => {
  clampCameraPan();
  applyCamera();
});

window.setInterval(() => {
  if (!state.heldKeys.size || !state.world) return;
  const player = playerActor();
  if (!player) return;
  let dx = 0;
  let dy = 0;
  if (state.heldKeys.has("a") || state.heldKeys.has("arrowleft")) dx -= 1;
  if (state.heldKeys.has("d") || state.heldKeys.has("arrowright")) dx += 1;
  if (state.heldKeys.has("w") || state.heldKeys.has("arrowup")) dy -= 1;
  if (state.heldKeys.has("s") || state.heldKeys.has("arrowdown")) dy += 1;
  if (!dx && !dy) return;
  const length = Math.hypot(dx, dy) || 1;
  moveTo(player.x + (dx / length) * 2.4, player.y + (dy / length) * 2.4, "Walking through Aster");
}, 170);

const sprite = new Image();
sprite.addEventListener("load", () => {
  state.spriteAvailable = true;
  if (state.world) renderActors();
});
sprite.src = "../assets/mara-walk-sprite-v1.png";

window.requestAnimationFrame(animateActors);
connect();
