import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  CIVILIZATION_DISCLOSURE,
  createCivilizationService,
  routeCivilizationRequest,
  validateCivilizationAction,
} from "./civilization-service.mjs";
import { startCivilizationServer } from "./civilization-server.mjs";

async function temporaryWorkDir(t) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "permstate-civilization-test-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

function deterministicTokens() {
  let index = 0;
  return () => `${String.fromCharCode(97 + (index++ % 26)).repeat(42)}-${index}`;
}

async function startHttp(t, service) {
  const server = http.createServer((request, response) => {
    routeCivilizationRequest(service, request, response).then((handled) => {
      if (!handled && !response.headersSent) {
        response.writeHead(404, { "Content-Type": "application/json" });
        response.end('{"error":"Not found"}\n');
      }
    }).catch((error) => {
      if (response.headersSent) return response.destroy(error);
      const body = `${JSON.stringify({ error: error.message, code: error.code })}\n`;
      response.writeHead(error.statusCode || 500, {
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(body),
      });
      response.end(body);
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  t.after(async () => {
    await new Promise((resolve) => server.close(resolve));
    await service.close();
  });
  return `http://127.0.0.1:${server.address().port}`;
}

async function jsonRequest(baseUrl, pathname, { method = "GET", body } = {}) {
  const response = await fetch(`${baseUrl}${pathname}`, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { response, value: await response.json() };
}

async function stopStandalone(instance) {
  if (!instance) return;
  await instance.runtime.civilizationService.close();
  if (instance.server.listening) {
    await new Promise((resolve) => instance.server.close(resolve));
  }
}

test("two authenticated clients join one canonical civilization", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const service = createCivilizationService({
    workDir,
    now: () => 10_000,
    autoStart: false,
    tokenFactory: deterministicTokens(),
  });
  const baseUrl = await startHttp(t, service);

  const [mara, ivo] = await Promise.all([
    jsonRequest(baseUrl, "/api/civilization/join", {
      method: "POST",
      body: { session: "aster", actorId: "mara", name: "Mara" },
    }),
    jsonRequest(baseUrl, "/api/civilization/join", {
      method: "POST",
      body: { session: "aster", actorId: "ivo", name: "Ivo" },
    }),
  ]);
  assert.equal(mara.response.status, 200);
  assert.equal(ivo.response.status, 200);
  assert.notEqual(mara.value.token, ivo.value.token);
  assert.equal(mara.value.disclosure, CIVILIZATION_DISCLOSURE);

  const [maraMove, ivoMove] = await Promise.all([
    jsonRequest(baseUrl, "/api/civilization/action", {
      method: "POST",
      body: {
        session: "aster",
        actorId: "mara",
        token: mara.value.token,
        action: { type: "MOVE", tileId: "1,0" },
      },
    }),
    jsonRequest(baseUrl, "/api/civilization/action", {
      method: "POST",
      body: {
        session: "aster",
        actorId: "ivo",
        token: ivo.value.token,
        action: { type: "MOVE", tileId: "-1,0" },
      },
    }),
  ]);
  assert.equal(maraMove.response.status, 200);
  assert.equal(ivoMove.response.status, 200);

  const state = await jsonRequest(baseUrl, "/api/civilization/state?session=aster");
  assert.equal(state.response.status, 200);
  assert.deepEqual(Object.keys(state.value.world.players).sort(), ["ivo", "mara"]);
  assert.deepEqual(state.value.world.players.mara.path, ["1,0"]);
  assert.deepEqual(state.value.world.players.ivo.path, ["-1,0"]);
  assert.equal(Object.hasOwn(state.value, "token"), false);
});

test("capability hashes and world state survive a service reload without persisting bearer tokens", async (t) => {
  const workDir = await temporaryWorkDir(t);
  let nowMs = 20_000;
  const first = createCivilizationService({
    workDir,
    now: () => nowMs,
    autoStart: false,
    tokenFactory: deterministicTokens(),
  });
  const joined = await first.join({ session: "persist", actorId: "Citizen-One", name: "Citizen One" });
  await first.close();

  const filePath = path.join(workDir, "civilizations", "persist.json");
  const serialized = await readFile(filePath, "utf8");
  assert.equal(serialized.includes(joined.token), false);
  assert.match(serialized, /"tokenHash": "[a-f0-9]{64}"/);
  assert.deepEqual(await readdir(path.dirname(filePath)), ["persist.json"]);

  nowMs += 1_000;
  const second = createCivilizationService({ workDir, now: () => nowMs, autoStart: false });
  t.after(() => second.close());
  await assert.rejects(
    second.join({ session: "persist", actorId: "Citizen-One", name: "Impostor" }),
    (error) => error.statusCode === 401,
  );
  const resumed = await second.join({
    session: "persist",
    actorId: "Citizen-One",
    name: "Citizen One",
    token: joined.token,
  });
  assert.equal(resumed.token, joined.token);
  assert.equal(resumed.world.players["Citizen-One"].name, "Citizen One");
});

test("a retained browser capability can recreate its absent citizen after a local state reset", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const retainedToken = "retained-browser-capability-token-00000001";
  const service = createCivilizationService({ workDir, now: () => 25_000, autoStart: false });
  t.after(() => service.close());

  const recreated = await service.join({
    session: "reset",
    actorId: "mara",
    name: "Mara",
    token: retainedToken,
  });
  assert.equal(recreated.token, retainedToken);
  const resumed = await service.join({
    session: "reset",
    actorId: "mara",
    name: "Mara",
    token: retainedToken,
  });
  assert.equal(resumed.world.players.mara.name, "Mara");
});

test("tokens cannot impersonate another citizen and client state injection is rejected", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const service = createCivilizationService({
    workDir,
    now: () => 30_000,
    autoStart: false,
    tokenFactory: deterministicTokens(),
  });
  t.after(() => service.close());
  const mara = await service.join({ session: "auth", actorId: "mara", name: "Mara" });
  const ivo = await service.join({ session: "auth", actorId: "ivo", name: "Ivo" });
  const ivoTile = `${ivo.world.players.ivo.q},${ivo.world.players.ivo.r}`;

  await assert.rejects(
    service.join({
      session: "auth",
      actorId: "nia",
      name: "Nia",
      token: mara.token,
    }),
    (error) => error.statusCode === 403,
  );

  await assert.rejects(
    service.action({
      session: "auth",
      actorId: "ivo",
      token: mara.token,
      action: { type: "MOVE", tileId: ivoTile },
    }),
    (error) => error.statusCode === 403,
  );
  await assert.rejects(
    service.action({
      session: "auth",
      actorId: "ivo",
      token: ivo.token,
      action: { type: "MOVE", tileId: ivoTile, actorId: "mara" },
    }),
    (error) => error.statusCode === 400,
  );
  await assert.rejects(
    service.action({
      session: "auth",
      actorId: "ivo",
      token: ivo.token,
      action: { type: "MOVE", tileId: ivoTile, stock: { food: 999_999 } },
    }),
    (error) => error.statusCode === 400,
  );
});

test("a legal-shaped but impossible action cannot partially mutate authoritative state", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const service = createCivilizationService({
    workDir,
    now: () => 35_000,
    autoStart: false,
    tokenFactory: deterministicTokens(),
  });
  t.after(() => service.close());
  const joined = await service.join({ session: "atomic", actorId: "mara", name: "Mara" });
  const before = await service.state("atomic");

  await assert.rejects(
    service.action({
      session: "atomic",
      actorId: "mara",
      token: joined.token,
      action: { type: "GATHER", tileId: "8,0" },
    }),
    (error) => error.statusCode === 409,
  );
  const after = await service.state("atomic");
  assert.deepEqual(after.world, before.world);
});

test("state reads advance the bounded continuous simulation", async (t) => {
  const workDir = await temporaryWorkDir(t);
  let nowMs = 1_000;
  const service = createCivilizationService({
    workDir,
    now: () => nowMs,
    autoStart: false,
    tokenFactory: deterministicTokens(),
  });
  t.after(() => service.close());
  const before = await service.join({ session: "clock", actorId: "mara", name: "Mara" });
  nowMs = 10_000_000;
  const after = await service.state("clock");
  assert.ok(after.world.lastNowMs > before.world.lastNowMs);
  assert.ok(after.world.timeMs > before.world.timeMs);
  assert.ok(after.world.timeMs - before.world.timeMs < nowMs - 1_000);
});

test("action schemas whitelist only the simulation contract", () => {
  assert.deepEqual(validateCivilizationAction({ type: "move", tileId: "-2,3" }), {
    type: "MOVE",
    tileId: "-2,3",
  });
  assert.deepEqual(validateCivilizationAction({
    type: "BUILD",
    tileId: "1,2",
    buildingType: "farm",
  }), {
    type: "BUILD",
    tileId: "1,2",
    buildingType: "farm",
  });
  assert.throws(() => validateCivilizationAction({ type: "JOIN", name: "Intruder" }), /Unsupported/);
  assert.throws(() => validateCivilizationAction({ type: "RESEARCH", techId: "mint-token" }), /techId/);
  assert.throws(() => validateCivilizationAction({ type: "ROAD", tileId: "../../secrets" }), /tileId/);
});

test("request limits and identifiers reject oversized or path-like input", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const service = createCivilizationService({ workDir, autoStart: false });
  const baseUrl = await startHttp(t, service);

  await assert.rejects(
    service.join({ session: "Uppercase", actorId: "mara", name: "Mara" }),
    (error) => error.statusCode === 400,
  );
  await assert.rejects(
    service.join({ session: "safe", actorId: "../mara", name: "Mara" }),
    (error) => error.statusCode === 400,
  );
  await assert.rejects(
    service.join({ session: "safe", actorId: "__proto__", name: "Mara" }),
    (error) => error.statusCode === 400,
  );
  await assert.rejects(
    service.join({ session: "safe", actorId: "mara", name: "Mara", stock: { food: 999 } }),
    (error) => error.statusCode === 400,
  );

  const response = await fetch(`${baseUrl}/api/civilization/join`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ padding: "x".repeat(65_000) }),
  });
  assert.equal(response.status, 413);
});

test("the chain-independent server restarts on the same persisted civilization", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const instances = [];
  t.after(async () => {
    for (const instance of instances) await stopStandalone(instance);
  });

  const first = await startCivilizationServer({ port: 0, workDir, log: () => {} });
  instances.push(first);
  const firstOrigin = new URL(first.url).origin;
  const redirect = await fetch(`${firstOrigin}/`, { redirect: "manual" });
  assert.equal(redirect.status, 302);
  assert.equal(redirect.headers.get("location"), "/civilization/");
  const joined = await jsonRequest(firstOrigin, "/api/civilization/join", {
    method: "POST",
    body: { session: "offline", actorId: "mara", name: "Mara" },
  });
  assert.equal(joined.response.status, 200);
  await stopStandalone(first);

  const second = await startCivilizationServer({ port: 0, workDir, log: () => {} });
  instances.push(second);
  const secondOrigin = new URL(second.url).origin;
  const persisted = await jsonRequest(secondOrigin, "/api/civilization/state?session=offline");
  assert.equal(persisted.response.status, 200);
  assert.ok(persisted.value.world.players.mara);
  const resumed = await jsonRequest(secondOrigin, "/api/civilization/join", {
    method: "POST",
    body: {
      session: "offline",
      actorId: "mara",
      name: "Mara",
      token: joined.value.token,
    },
  });
  assert.equal(resumed.response.status, 200);
  assert.equal(resumed.value.token, joined.value.token);
});
