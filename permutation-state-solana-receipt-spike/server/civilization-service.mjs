import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  advanceCivilization,
  applyCivilizationAction,
  createCivilization,
} from "../../permutation-state-prototype/civilization/core.mjs";

export const CIVILIZATION_DISCLOSURE = "OFFCHAIN CIVILIZATION ALPHA · SOLANA PROOFS ARE SEPARATE";
export const CIVILIZATION_SERVICE_SCHEMA = "permutation.civilization-service.v1";

const BODY_LIMIT_BYTES = 64_000;
const DEFAULT_TICK_INTERVAL_MS = 250;
const DEFAULT_PERSIST_INTERVAL_MS = 1_000;
const BUILDING_TYPES = new Set(["farm", "lumbermill", "quarry", "mine", "workshop", "watchtower", "archive"]);
const RESEARCH_TECHS = new Set(["agriculture", "logistics", "metallurgy"]);

function serviceError(message, statusCode = 400, code = "INVALID_REQUEST") {
  return Object.assign(new Error(message), { statusCode, code });
}

function isObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, expected, label) {
  if (!isObject(value)) throw serviceError(`${label} must be a JSON object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw serviceError(`${label} must contain exactly: ${wanted.join(", ")}`);
  }
}

function exactKeysOneOf(value, alternatives, label) {
  if (!isObject(value)) throw serviceError(`${label} must be a JSON object`);
  const actual = Object.keys(value).sort().join("\u0000");
  if (!alternatives.some((keys) => [...keys].sort().join("\u0000") === actual)) {
    throw serviceError(`${label} contains missing or unsupported fields`);
  }
}

export function validateCivilizationSession(value) {
  if (typeof value !== "string" || !/^[a-z0-9][a-z0-9_-]{0,39}$/.test(value)) {
    throw serviceError("session must be 1-40 lowercase letters, numbers, underscores, or dashes");
  }
  return value;
}

export function validateCivilizationActorId(value) {
  const reserved = value === "prototype"
    || (typeof value === "string" && Object.prototype.hasOwnProperty.call(Object.prototype, value));
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(value) || reserved) {
    throw serviceError("actorId must be 1-64 letters, numbers, underscores, or dashes");
  }
  return value;
}

function validateName(value) {
  if (typeof value !== "string") throw serviceError("name must be a string");
  const name = value.trim();
  if (!name || name.length > 24 || /[<>\u0000-\u001f\u007f]/.test(name)) {
    throw serviceError("name must contain 1-24 printable characters without angle brackets");
  }
  return name;
}

function validateTileId(value) {
  if (typeof value !== "string" || !/^-?\d{1,3},-?\d{1,3}$/.test(value)) {
    throw serviceError("tileId must be a q,r hex coordinate");
  }
  const [q, r] = value.split(",").map(Number);
  if (Math.abs(q) > 250 || Math.abs(r) > 250) throw serviceError("tileId is outside the supported map bounds");
  return `${q},${r}`;
}

export function validateCivilizationAction(value) {
  if (!isObject(value) || typeof value.type !== "string") {
    throw serviceError("action must be an object with a type");
  }
  const type = value.type.trim().toUpperCase();
  switch (type) {
    case "MOVE":
    case "EXPLORE":
    case "ROAD":
    case "GATHER":
      exactKeys(value, ["type", "tileId"], `${type} action`);
      return { type, tileId: validateTileId(value.tileId) };
    case "BUILD": {
      exactKeys(value, ["type", "tileId", "buildingType"], "BUILD action");
      if (typeof value.buildingType !== "string" || !BUILDING_TYPES.has(value.buildingType)) {
        throw serviceError("buildingType is not supported");
      }
      return { type, tileId: validateTileId(value.tileId), buildingType: value.buildingType };
    }
    case "RESEARCH":
      exactKeys(value, ["type", "techId"], "RESEARCH action");
      if (typeof value.techId !== "string" || !RESEARCH_TECHS.has(value.techId)) {
        throw serviceError("techId is not supported");
      }
      return { type, techId: value.techId };
    default:
      throw serviceError(`Unsupported civilization action: ${value.type}`);
  }
}

function validateToken(value, { optional = false } = {}) {
  if (optional && (value === undefined || value === null || value === "")) return null;
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{32,128}$/.test(value)) {
    throw serviceError("A valid capability token is required", 401, "AUTH_REQUIRED");
  }
  return value;
}

function tokenHash(sessionId, actorId, token) {
  return createHash("sha256")
    .update("PERMSTATE/CIVILIZATION_CAPABILITY/V1\u0000", "utf8")
    .update(sessionId, "utf8")
    .update("\u0000", "utf8")
    .update(actorId, "utf8")
    .update("\u0000", "utf8")
    .update(token, "utf8")
    .digest();
}

function matchesToken(storedHex, sessionId, actorId, token) {
  if (typeof storedHex !== "string" || !/^[a-f0-9]{64}$/.test(storedHex)) return false;
  const expected = Buffer.from(storedHex, "hex");
  const supplied = tokenHash(sessionId, actorId, token);
  return expected.length === supplied.length && timingSafeEqual(expected, supplied);
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function returnedWorld(current, result) {
  return result === undefined || result === null ? current : result;
}

function actionError(error) {
  if (error?.statusCode) return error;
  return Object.assign(error instanceof Error ? error : new Error(String(error)), {
    statusCode: 409,
    code: error?.code || "ACTION_REJECTED",
  });
}

async function writeJsonAtomic(filePath, value) {
  await mkdir(path.dirname(filePath), { recursive: true });
  const temporary = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  await writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
  await rename(temporary, filePath);
}

function publicState(world) {
  return { world: clone(world), disclosure: CIVILIZATION_DISCLOSURE };
}

function persistedEnvelope(record) {
  return {
    schemaVersion: CIVILIZATION_SERVICE_SCHEMA,
    world: record.world,
    auth: record.auth,
  };
}

function assertPersistedEnvelope(value, sessionId) {
  if (
    !isObject(value)
    || value.schemaVersion !== CIVILIZATION_SERVICE_SCHEMA
    || !isObject(value.world)
    || value.world.sessionId !== sessionId
    || !isObject(value.auth)
  ) {
    throw serviceError("Persisted civilization has an invalid envelope", 500, "CORRUPT_STATE");
  }
  for (const [actorId, auth] of Object.entries(value.auth)) {
    const validActorId = typeof actorId === "string"
      && /^[A-Za-z0-9_-]{1,64}$/.test(actorId)
      && actorId !== "prototype"
      && !Object.prototype.hasOwnProperty.call(Object.prototype, actorId);
    if (!validActorId || !isObject(auth) || typeof auth.tokenHash !== "string" || !/^[a-f0-9]{64}$/.test(auth.tokenHash)) {
      throw serviceError("Persisted civilization authentication is invalid", 500, "CORRUPT_AUTH");
    }
  }
}

export function createCivilizationService({
  workDir,
  now = () => Date.now(),
  tickIntervalMs = DEFAULT_TICK_INTERVAL_MS,
  persistIntervalMs = DEFAULT_PERSIST_INTERVAL_MS,
  autoStart = true,
  tokenFactory = () => randomBytes(32).toString("base64url"),
  onError = (error) => console.error(`[civilization] ${error.message}`),
} = {}) {
  if (typeof workDir !== "string" || !workDir) throw new Error("civilization service workDir is required");
  if (typeof now !== "function") throw new Error("civilization service now must be a function");
  if (typeof tokenFactory !== "function") throw new Error("civilization service tokenFactory must be a function");
  if (!Number.isInteger(tickIntervalMs) || tickIntervalMs < 1) throw new Error("tickIntervalMs must be positive");
  if (!Number.isInteger(persistIntervalMs) || persistIntervalMs < 1) throw new Error("persistIntervalMs must be positive");

  const civilizationsDir = path.join(workDir, "civilizations");
  const records = new Map();
  const locks = new Map();
  let timer = null;
  let closed = false;

  const filePathFor = (sessionId) => path.join(civilizationsDir, `${sessionId}.json`);

  function assertOpen() {
    if (closed) throw serviceError("Civilization service is unavailable", 503, "SERVICE_CLOSED");
  }

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
    let record;
    if (existsSync(filePath)) {
      let value;
      try {
        value = JSON.parse(await readFile(filePath, "utf8"));
      } catch (error) {
        throw serviceError(`Persisted civilization could not be loaded: ${error.message}`, 500, "CORRUPT_STATE");
      }
      assertPersistedEnvelope(value, sessionId);
      record = { world: value.world, auth: value.auth, dirty: false, lastPersistedAt: nowMs };
    } else if (create) {
      const world = createCivilization({ sessionId, nowMs });
      if (!isObject(world)) throw serviceError("Civilization core returned invalid state", 500, "INVALID_CORE_STATE");
      record = { world, auth: {}, dirty: true, lastPersistedAt: 0 };
    } else {
      throw serviceError("Civilization session was not found; join it first", 404, "SESSION_NOT_FOUND");
    }
    records.set(sessionId, record);
    return record;
  }

  function advanceRecord(record, nowMs) {
    record.world = returnedWorld(record.world, advanceCivilization(record.world, nowMs));
    if (!isObject(record.world)) throw serviceError("Civilization core returned invalid state", 500, "INVALID_CORE_STATE");
    record.dirty = true;
  }

  async function persistRecord(sessionId, record, nowMs, { force = false } = {}) {
    if (!record.dirty) return;
    if (!force && nowMs - record.lastPersistedAt < persistIntervalMs) return;
    await writeJsonAtomic(filePathFor(sessionId), persistedEnvelope(record));
    record.dirty = false;
    record.lastPersistedAt = nowMs;
  }

  function requireCapability(record, actorId, suppliedToken) {
    const token = validateToken(suppliedToken);
    const auth = record.auth[actorId];
    if (!auth || !matchesToken(auth.tokenHash, record.world.sessionId, actorId, token)) {
      throw serviceError("Citizen capability was rejected", 403, "AUTH_REJECTED");
    }
    return token;
  }

  function capabilityIsInUse(record, token) {
    return Object.keys(record.auth).some((existingActorId) => (
      matchesToken(
        record.auth[existingActorId].tokenHash,
        record.world.sessionId,
        existingActorId,
        token,
      )
    ));
  }

  function issueCapability(record) {
    for (let attempt = 0; attempt < 5; attempt += 1) {
      const token = tokenFactory();
      validateToken(token);
      if (!capabilityIsInUse(record, token)) return token;
    }
    throw serviceError("Could not issue a unique citizen capability", 500, "TOKEN_ISSUE_FAILED");
  }

  async function state(sessionInput) {
    assertOpen();
    const sessionId = validateCivilizationSession(sessionInput);
    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs);
      advanceRecord(record, nowMs);
      await persistRecord(sessionId, record, nowMs);
      return publicState(record.world);
    });
  }

  async function join(body) {
    assertOpen();
    exactKeysOneOf(body, [
      ["session", "actorId", "name"],
      ["session", "actorId", "name", "token"],
    ], "join body");
    const sessionId = validateCivilizationSession(body.session);
    const actorId = validateCivilizationActorId(body.actorId);
    const name = validateName(body.name);
    const suppliedToken = validateToken(body.token, { optional: true });

    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs, { create: true });
      const hasPlayer = Boolean(record.world.players && Object.hasOwn(record.world.players, actorId));
      const hasAuth = Object.hasOwn(record.auth, actorId);
      let token;

      if (hasPlayer || hasAuth) {
        if (!hasPlayer || !hasAuth) {
          throw serviceError("Citizen identity and authentication records disagree", 409, "IDENTITY_CONFLICT");
        }
        token = requireCapability(record, actorId, suppliedToken);
      } else {
        // A browser can retain sessionStorage while the local world directory
        // is intentionally replaced. Reusing that high-entropy capability for
        // a genuinely absent citizen is safe and makes the reload recoverable.
        if (suppliedToken && capabilityIsInUse(record, suppliedToken)) {
          throw serviceError("Citizen capability already belongs to another citizen", 403, "AUTH_REJECTED");
        }
        token = suppliedToken || issueCapability(record);
      }

      advanceRecord(record, nowMs);
      const candidate = clone(record.world);
      try {
        record.world = returnedWorld(candidate, applyCivilizationAction(candidate, {
          type: "JOIN",
          actorId,
          name,
        }, nowMs));
      } catch (error) {
        throw actionError(error);
      }
      if (!hasAuth) {
        record.auth[actorId] = {
          tokenHash: tokenHash(sessionId, actorId, token).toString("hex"),
          createdAtMs: nowMs,
          lastUsedAtMs: nowMs,
        };
      } else {
        record.auth[actorId].lastUsedAtMs = nowMs;
      }
      record.dirty = true;
      await persistRecord(sessionId, record, nowMs, { force: true });
      return { ...publicState(record.world), actorId, token };
    });
  }

  async function action(body) {
    assertOpen();
    exactKeys(body, ["session", "actorId", "token", "action"], "action body");
    const sessionId = validateCivilizationSession(body.session);
    const actorId = validateCivilizationActorId(body.actorId);
    const token = validateToken(body.token);
    const clientAction = validateCivilizationAction(body.action);

    return exclusive(sessionId, async () => {
      const nowMs = now();
      const record = await loadRecord(sessionId, nowMs);
      requireCapability(record, actorId, token);
      if (!record.world.players || !Object.hasOwn(record.world.players, actorId)) {
        throw serviceError("Authenticated citizen is missing from the world", 409, "IDENTITY_CONFLICT");
      }
      advanceRecord(record, nowMs);
      const candidate = clone(record.world);
      try {
        record.world = returnedWorld(candidate, applyCivilizationAction(candidate, {
          ...clientAction,
          actorId,
        }, nowMs));
      } catch (error) {
        throw actionError(error);
      }
      record.auth[actorId].lastUsedAtMs = nowMs;
      record.dirty = true;
      await persistRecord(sessionId, record, nowMs, { force: true });
      return publicState(record.world);
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
    results.forEach((result) => {
      if (result.status === "rejected") onError(result.reason);
    });
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

  return { civilizationsDir, state, join, action, tickAll, close };
}

async function parseBody(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > BODY_LIMIT_BYTES) throw serviceError("Request body is too large", 413, "BODY_TOO_LARGE");
    chunks.push(chunk);
  }
  if (!chunks.length) throw serviceError("Request body is required");
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw serviceError("Request body must be valid JSON");
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

export async function routeCivilizationRequest(
  service,
  request,
  response,
  requestUrl = new URL(request.url, "http://127.0.0.1"),
) {
  if (requestUrl.pathname === "/api/civilization/state") {
    if (request.method !== "GET") throw serviceError("Method not allowed", 405, "METHOD_NOT_ALLOWED");
    sendJson(response, 200, await service.state(requestUrl.searchParams.get("session")));
    return true;
  }
  if (requestUrl.pathname === "/api/civilization/join") {
    if (request.method !== "POST") throw serviceError("Method not allowed", 405, "METHOD_NOT_ALLOWED");
    sendJson(response, 200, await service.join(await parseBody(request)));
    return true;
  }
  if (requestUrl.pathname === "/api/civilization/action") {
    if (request.method !== "POST") throw serviceError("Method not allowed", 405, "METHOD_NOT_ALLOWED");
    sendJson(response, 200, await service.action(await parseBody(request)));
    return true;
  }
  return false;
}
