import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  WORLD_DISCLOSURE,
  createWorldService,
  routeWorldRequest,
  validateWorldAction,
} from "./world-service.mjs";

async function temporaryWorkDir(t) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "permstate-world-test-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

async function startHttp(t, service) {
  const server = http.createServer((request, response) => {
    routeWorldRequest(service, request, response).then((handled) => {
      if (!handled && !response.headersSent) {
        response.writeHead(404, { "Content-Type": "application/json" });
        response.end('{"error":"Not found"}\n');
      }
    }).catch((error) => {
      if (response.headersSent) return response.destroy(error);
      const body = `${JSON.stringify({ error: error.message })}\n`;
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
  const address = server.address();
  return `http://127.0.0.1:${address.port}`;
}

async function jsonRequest(baseUrl, pathname, { method = "GET", body } = {}) {
  const response = await fetch(`${baseUrl}${pathname}`, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { response, value: await response.json() };
}

function worldActors(world) {
  const collection = world.players || world.actors || world.citizens;
  if (Array.isArray(collection)) return collection;
  return Object.values(collection || {});
}

test("two HTTP clients join and read the same authoritative world", async (t) => {
  const workDir = await temporaryWorkDir(t);
  let nowMs = 10_000;
  const service = createWorldService({ workDir, now: () => nowMs, autoStart: false });
  const baseUrl = await startHttp(t, service);

  const alice = await jsonRequest(baseUrl, "/api/world/bootstrap", {
    method: "POST",
    body: { session: "Shared Town", actorId: "Alice", name: "Alice" },
  });
  assert.equal(alice.response.status, 200);
  assert.equal(alice.value.authoritative, "gateway");
  assert.equal(alice.value.disclosure, WORLD_DISCLOSURE);

  const bob = await jsonRequest(baseUrl, "/api/world/bootstrap", {
    method: "POST",
    body: { session: "Shared Town", actorId: "Bob", name: "Bob" },
  });
  assert.equal(bob.response.status, 200);
  assert.deepEqual(worldActors(bob.value.world).map((actor) => actor.name).sort(), ["Alice", "Bob"]);

  const moved = await jsonRequest(baseUrl, "/api/world/action", {
    method: "POST",
    body: {
      session: "Shared Town",
      actorId: "Bob",
      action: { type: "MOVE", x: 50.5, y: 34.25 },
    },
  });
  assert.equal(moved.response.status, 200);
  assert.equal(moved.value.world.lastAction.actorId, "Bob");
  assert.deepEqual(moved.value.world.players.Bob.target, { x: 50.5, y: 34.25 });

  nowMs += 250;
  const fromAlice = await jsonRequest(baseUrl, "/api/world/session?session=shared-town&actor=Alice");
  const fromBob = await jsonRequest(baseUrl, "/api/world/session?session=shared-town&actor=Bob");
  assert.equal(fromAlice.response.status, 200);
  assert.equal(fromBob.response.status, 200);
  assert.deepEqual(fromAlice.value.world, fromBob.value.world);
});

test("world snapshots persist atomically and reload into a fresh service", async (t) => {
  const workDir = await temporaryWorkDir(t);
  let nowMs = 5_000;
  const first = createWorldService({ workDir, now: () => nowMs, autoStart: false });
  const bootstrapped = await first.bootstrap({ session: "reload", actorId: "mara", name: "Mara" });
  await first.close();

  const filePath = path.join(workDir, "worlds", "reload.json");
  const saved = JSON.parse(await readFile(filePath, "utf8"));
  assert.deepEqual(saved, bootstrapped.world);
  assert.deepEqual((await readdir(path.dirname(filePath))).sort(), ["reload.json"]);

  const second = createWorldService({ workDir, now: () => nowMs, autoStart: false });
  t.after(() => second.close());
  const reloaded = await second.get("reload", "mara");
  assert.deepEqual(reloaded.world, bootstrapped.world);
});

test("lazy reads advance the continuous simulation clock", async (t) => {
  const workDir = await temporaryWorkDir(t);
  let nowMs = 1_000;
  const service = createWorldService({ workDir, now: () => nowMs, autoStart: false });
  t.after(() => service.close());
  const before = await service.bootstrap({ session: "clock", actorId: "mara", name: "Mara" });

  nowMs = 61_000;
  const after = await service.get("clock", "mara");
  assert.ok(after.world.lastAdvancedAtMs > before.world.lastAdvancedAtMs);
  assert.ok(after.world.simulationTimeMs > before.world.simulationTimeMs);
  assert.ok(after.world.tick > before.world.tick);
});

test("schemas reject extra fields, client actor injection, and malformed actions", async (t) => {
  const workDir = await temporaryWorkDir(t);
  const service = createWorldService({ workDir, autoStart: false });
  const baseUrl = await startHttp(t, service);

  const extra = await jsonRequest(baseUrl, "/api/world/bootstrap", {
    method: "POST",
    body: { session: "strict", actorId: "mara", name: "Mara", admin: true },
  });
  assert.equal(extra.response.status, 400);

  const bootstrapped = await jsonRequest(baseUrl, "/api/world/bootstrap", {
    method: "POST",
    body: { session: "strict", actorId: "mara", name: "Mara" },
  });
  assert.equal(bootstrapped.response.status, 200);

  const injected = await jsonRequest(baseUrl, "/api/world/action", {
    method: "POST",
    body: {
      session: "strict",
      actorId: "mara",
      action: { type: "MOVE", x: 1, y: 1, actorId: "someone-else" },
    },
  });
  assert.equal(injected.response.status, 400);

  const unknown = await jsonRequest(baseUrl, "/api/world/action", {
    method: "POST",
    body: { session: "strict", actorId: "mara", action: { type: "MINT_PRIZE" } },
  });
  assert.equal(unknown.response.status, 400);

  assert.throws(() => validateWorldAction({ type: "BUY", item: "food", quantity: 0 }), /quantity/);
  assert.deepEqual(validateWorldAction({ type: "MOVE", x: 1.5, y: 2.25 }), { type: "MOVE", x: 1.5, y: 2.25 });
  assert.throws(() => validateWorldAction({ type: "MOVE", x: 101, y: 2 }), /finite number/);
});
