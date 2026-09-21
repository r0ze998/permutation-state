import { existsSync } from "node:fs";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  advanceWorld,
  applyWorldAction,
  createWorld,
} from "../../permutation-state-prototype/world/core.mjs";

export const WORLD_SCHEMA_VERSION = "permutation-state.world-gateway.v1";
export const WORLD_DISCLOSURE = "OFFCHAIN SIMULATION ALPHA · SOLANA CHECKPOINTS FOLLOW";

const JSON_LIMIT = 64_000;
const DEFAULT_TICK_INTERVAL_MS = 250;
const DEFAULT_PERSIST_INTERVAL_MS = 1_000;

function httpError(message, statusCode = 400) {
  return Object.assign(new Error(message), { statusCode });
}

function plainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, expected, label) {
  if (!plainObject(value)) throw httpError(`${label} must be a JSON object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw httpError(`${label} must contain exactly: ${wanted.join(", ")}`);
  }
}

export function sanitizeWorldIdentifier(value, label = "id", {
  maxLength = 40,
  lowercase = false,
  allowColon = false,
} = {}) {
  if (typeof value !== "string") throw httpError(`${label} must be a string`);
  const source = lowercase ? value.trim().toLowerCase() : value.trim();
  const unsafe = allowColon ? /[^a-zA-Z0-9:_-]+/g : /[^a-zA-Z0-9_-]+/g;
  const sanitized = source
    .replace(unsafe, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, maxLength);
  if (!sanitized) throw httpError(`${label} is required`);
  return sanitized;
}

function displayName(value) {
  if (typeof value !== "string") throw httpError("name must be a string");
  const name = value.trim();
  if (!name || name.length > 32 || /[\u0000-\u001f\u007f]/.test(name)) {
    throw httpError("name must contain 1 to 32 printable characters");
  }
  return name;
}

function finiteInteger(value, label, { min = -1_000, max = 1_000 } = {}) {
  if (!Number.isInteger(value) || value < min || value > max) {
    throw httpError(`${label} must be an integer from ${min} to ${max}`);
  }
  return value;
}

function actionIdentifier(value, label) {
  return sanitizeWorldIdentifier(value, label, { lowercase: true });
}

function finiteCoordinate(value, label, { min, max }) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) {
    throw httpError(`${label} must be a finite number from ${min} to ${max}`);
  }
  return value;
}

export function validateWorldAction(value) {
  if (!plainObject(value) || typeof value.type !== "string") {
    throw httpError("action must be an object with a type");
  }

  switch (value.type) {
    case "MOVE":
      exactKeys(value, ["type", "x", "y"], "MOVE action");
      return {
        type: "MOVE",
        x: finiteCoordinate(value.x, "x", { min: 0, max: 100 }),
        y: finiteCoordinate(value.y, "y", { min: 0, max: 60 }),
      };
    case "GATHER":
      exactKeys(value, ["type", "nodeId"], "GATHER action");
      return { type: "GATHER", nodeId: actionIdentifier(value.nodeId, "nodeId") };
    case "DEPOSIT":
      if (Object.keys(value).some((key) => !["type", "item", "quantity"].includes(key))) {
        throw httpError("DEPOSIT action may only contain: item, quantity, type");
      }
      return {
        type: "DEPOSIT",
        ...(value.item === undefined ? {} : { item: actionIdentifier(value.item, "item") }),
        ...(value.quantity === undefined ? {} : {
          quantity: finiteInteger(value.quantity, "quantity", { min: 1, max: 99 }),
        }),
      };
    case "BUILD":
      exactKeys(value, ["type", "projectId"], "BUILD action");
      return { type: "BUILD", projectId: actionIdentifier(value.projectId, "projectId") };
    case "CRAFT":
      exactKeys(value, ["type", "recipeId"], "CRAFT action");
      return { type: "CRAFT", recipeId: actionIdentifier(value.recipeId, "recipeId") };
    case "BUY":
    case "SELL":
      exactKeys(value, ["type", "item", "quantity"], `${value.type} action`);
      return {
        type: value.type,
        item: actionIdentifier(value.item, "item"),
        quantity: finiteInteger(value.quantity, "quantity", { min: 1, max: 10 }),
      };
    case "TALK":
      exactKeys(value, ["type", "npcId"], "TALK action");
      return { type: "TALK", npcId: actionIdentifier(value.npcId, "npcId") };
    default:
      throw httpError(`Unsupported world action: ${value.type}`);
  }
}

function returnedWorld(current, result) {
  if (result === undefined || result === null) return current;
  if (plainObject(result) && plainObject(result.world)) return result.world;
  return result;
}

function publicWorld(world) {
  return {
    schemaVersion: WORLD_SCHEMA_VERSION,
    authoritative: "gateway",
    disclosure: WORLD_DISCLOSURE,
    world,
  };
}

async function writeJsonAtomic(filePath, value) {
  await mkdir(path.dirname(filePath), { recursive: true });
  const temporary = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  await writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
  await rename(temporary, filePath);
}

function actorExists(world, actorId) {
  for (const collection of [world?.players, world?.actors, world?.citizens]) {
    if (Array.isArray(collection) && collection.some((actor) => actor?.id === actorId || actor?.actorId === actorId)) {
      return true;
    }
    if (plainObject(collection) && Object.hasOwn(collection, actorId)) return true;
  }
  return false;
}

function gameActionError(error) {
  if (error?.statusCode) return error;
  return Object.assign(error instanceof Error ? error : new Error(String(error)), { statusCode: 409 });
}

/**
 * The gateway is the authority for this alpha simulation. This service does
 * not claim that its JSON snapshots are Solana or MagicBlock state.
 */
export function createWorldService({
  workDir,
  now = () => Date.now(),
  tickIntervalMs = DEFAULT_TICK_INTERVAL_MS,
  persistIntervalMs = DEFAULT_PERSIST_INTERVAL_MS,
  autoStart = true,
  onError = (error) => console.error(`[world] ${error.message}`),
} = {}) {
  if (typeof workDir !== "string" || !workDir) throw new Error("world service workDir is required");
  if (typeof now !== "function") throw new Error("world service now must be a function");
  if (!Number.isInteger(tickIntervalMs) || tickIntervalMs < 1) throw new Error("tickIntervalMs must be positive");
  if (!Number.isInteger(persistIntervalMs) || persistIntervalMs < 1) throw new Error("persistIntervalMs must be positive");

  const worldsDir = path.join(workDir, "worlds");
  const records = new Map();
  const locks = new Map();
  let timer = null;
  let closed = false;

  const filePathFor = (sessionId) => path.join(worldsDir, `${sessionId}.json`);

  async function exclusive(sessionId, operation) {
    const previous = locks.get(sessionId) || Promise.resolve();
    const current = previous.catch(() => {}).then(operation);
    locks.set(sessionId, current);
    try {
      return await current;
    } finally {
      if (locks.get(sessionId) === current) locks.delete(sessionId);
    }
  }

  async function loadRecord(sessionId, nowMs, { create = false } = {}) {
    const cached = records.get(sessionId);
    if (cached) return cached;

    const filePath = filePathFor(sessionId);
    let world;
    if (existsSync(filePath)) {
      try {
        world = JSON.parse(await readFile(filePath, "utf8"));
      } catch (error) {
        throw httpError(`Persisted world could not be loaded: ${error.message}`, 500);
      }
    } else if (create) {
      world = createWorld({ sessionId, nowMs });
    } else {
      throw httpError("World session was not found; bootstrap it first", 404);
    }

    if (!plainObject(world)) throw httpError("World core returned an invalid world", 500);
    const record = { world, dirty: create, lastPersistedAt: create ? 0 : nowMs };
    records.set(sessionId, record);
    return record;
  }

  function advanceRecord(record, nowMs) {
    record.world = returnedWorld(record.world, advanceWorld(record.world, nowMs));
    if (!plainObject(record.world)) throw httpError("World core returned an invalid world", 500);
    record.dirty = true;
  }

  async function persistRecord(sessionId, record, nowMs, { force = false } = {}) {
    if (!record.dirty) return;
    if (!force && nowMs - record.lastPersistedAt < persistIntervalMs) return;
    await writeJsonAtomic(filePathFor(sessionId), record.world);
    record.dirty = false;
    record.lastPersistedAt = nowMs;
  }

  async function get(sessionInput, actorInput) {
    const sessionId = sanitizeWorldIdentifier(sessionInput, "session", { lowercase: true });
    sanitizeWorldIdentifier(actorInput, "actor", { maxLength: 64, allowColon: true });
    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs);
      advanceRecord(record, nowMs);
      await persistRecord(sessionId, record, nowMs);
      return publicWorld(record.world);
    });
  }

  async function bootstrap(body) {
    exactKeys(body, ["session", "actorId", "name"], "bootstrap body");
    const sessionId = sanitizeWorldIdentifier(body.session, "session", { lowercase: true });
    const actorId = sanitizeWorldIdentifier(body.actorId, "actorId", { maxLength: 64, allowColon: true });
    const name = displayName(body.name);
    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs, { create: true });
      advanceRecord(record, nowMs);
      if (!actorExists(record.world, actorId)) {
        try {
          record.world = returnedWorld(
            record.world,
            applyWorldAction(record.world, { type: "JOIN", actorId, name }, nowMs),
          );
        } catch (error) {
          throw gameActionError(error);
        }
      }
      record.dirty = true;
      await persistRecord(sessionId, record, nowMs, { force: true });
      return publicWorld(record.world);
    });
  }

  async function action(body) {
    exactKeys(body, ["session", "actorId", "action"], "action body");
    const sessionId = sanitizeWorldIdentifier(body.session, "session", { lowercase: true });
    const actorId = sanitizeWorldIdentifier(body.actorId, "actorId", { maxLength: 64, allowColon: true });
    const clientAction = validateWorldAction(body.action);
    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs);
      advanceRecord(record, nowMs);
      try {
        record.world = returnedWorld(
          record.world,
          applyWorldAction(record.world, { ...clientAction, actorId }, nowMs),
        );
      } catch (error) {
        throw gameActionError(error);
      }
      if (!plainObject(record.world)) throw httpError("World core returned an invalid world", 500);
      record.dirty = true;
      await persistRecord(sessionId, record, nowMs, { force: true });
      return publicWorld(record.world);
    });
  }

  async function tickAll() {
    if (closed) return;
    const nowMs = now();
    const results = await Promise.allSettled([...records.keys()].map((sessionId) => (
      exclusive(sessionId, async () => {
        const record = records.get(sessionId);
        if (!record) return;
        advanceRecord(record, nowMs);
        await persistRecord(sessionId, record, nowMs);
      })
    )));
    for (const result of results) {
      if (result.status === "rejected") onError(result.reason);
    }
  }

  async function close() {
    if (closed) return;
    closed = true;
    if (timer) clearInterval(timer);
    const nowMs = now();
    await Promise.all([...records.keys()].map((sessionId) => (
      exclusive(sessionId, async () => {
        const record = records.get(sessionId);
        if (record) await persistRecord(sessionId, record, nowMs, { force: true });
      })
    )));
  }

  if (autoStart) {
    timer = setInterval(() => { tickAll().catch(onError); }, tickIntervalMs);
    timer.unref?.();
  }

  return {
    worldsDir,
    get,
    bootstrap,
    action,
    tickAll,
    close,
  };
}

async function parseBody(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > JSON_LIMIT) throw httpError("Request body is too large", 413);
    chunks.push(chunk);
  }
  if (!chunks.length) throw httpError("Request body is required");
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw httpError("Request body must be valid JSON");
  }
}

function sendJson(response, statusCode, value) {
  const body = `${JSON.stringify(value)}\n`;
  response.writeHead(statusCode, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
  });
  response.end(body);
}

export async function routeWorldRequest(service, request, response, requestUrl = new URL(request.url, "http://127.0.0.1")) {
  if (requestUrl.pathname === "/api/world/session") {
    if (request.method !== "GET") throw httpError("Method not allowed", 405);
    sendJson(
      response,
      200,
      await service.get(requestUrl.searchParams.get("session"), requestUrl.searchParams.get("actor")),
    );
    return true;
  }
  if (requestUrl.pathname === "/api/world/bootstrap") {
    if (request.method !== "POST") throw httpError("Method not allowed", 405);
    sendJson(response, 200, await service.bootstrap(await parseBody(request)));
    return true;
  }
  if (requestUrl.pathname === "/api/world/action") {
    if (request.method !== "POST") throw httpError("Method not allowed", 405);
    sendJson(response, 200, await service.action(await parseBody(request)));
    return true;
  }
  return false;
}
