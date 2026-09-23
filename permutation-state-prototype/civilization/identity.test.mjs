import assert from "node:assert/strict";
import test from "node:test";
import {
  CITIZEN_REMEMBERED_KEY_PREFIX,
  CITIZEN_SESSION_KEY_PREFIX,
  createCitizenIdentityStore,
} from "./identity.mjs";

function memoryStorage(initial = {}) {
  const values = new Map(Object.entries(initial));
  return {
    getItem(key) { return values.has(key) ? values.get(key) : null; },
    setItem(key, value) { values.set(key, String(value)); },
    removeItem(key) { values.delete(key); },
    value(key) { return values.get(key); },
  };
}

const identity = Object.freeze({
  actorId: "citizen-browser_1",
  name: "Mara",
  token: "stable-browser-capability-token-00000001",
});

test("session identity keeps the existing per-tab storage key and shape", () => {
  const sessionStorage = memoryStorage();
  const localStorage = memoryStorage();
  const store = createCitizenIdentityStore({ sessionStorage, localStorage });

  const saved = store.saveSession("shared", { ...identity, ignored: "not persisted" });
  assert.deepEqual(saved, identity);
  assert.deepEqual(store.loadSession("shared"), identity);
  assert.deepEqual(
    JSON.parse(sessionStorage.value(`${CITIZEN_SESSION_KEY_PREFIX}shared`)),
    identity,
  );
});

test("a remembered citizen is opt-in and never silently replaces the tab identity", () => {
  const tabIdentity = { actorId: "tab-citizen", name: "Tab" };
  const rememberedIdentity = { ...identity, actorId: "remembered-citizen" };
  const sessionStorage = memoryStorage();
  const localStorage = memoryStorage();
  const store = createCitizenIdentityStore({ sessionStorage, localStorage });

  store.saveSession("shared", tabIdentity);
  store.saveRemembered("shared", rememberedIdentity);
  assert.deepEqual(store.loadSession("shared"), tabIdentity);
  assert.deepEqual(store.loadRemembered("shared"), rememberedIdentity);
  assert.deepEqual(store.loadSession("shared"), tabIdentity);

  store.forgetRemembered("shared");
  assert.equal(store.loadRemembered("shared"), null);
  assert.deepEqual(store.loadSession("shared"), tabIdentity);
});

test("remembering requires an authenticated capability but session staging does not", () => {
  const store = createCitizenIdentityStore({
    sessionStorage: memoryStorage(),
    localStorage: memoryStorage(),
  });
  const pending = { actorId: "new-citizen", name: "New Citizen" };
  assert.deepEqual(store.saveSession("shared", pending), pending);
  assert.throws(
    () => store.saveRemembered("shared", pending),
    (error) => error.code === "INVALID_CITIZEN_IDENTITY",
  );
});

test("malformed or injected stored values are ignored and never exposed", () => {
  const sessionStorage = memoryStorage({
    [`${CITIZEN_SESSION_KEY_PREFIX}broken-json`]: "{",
    [`${CITIZEN_SESSION_KEY_PREFIX}unsafe`]: JSON.stringify({
      actorId: "__proto__",
      name: "Intruder",
      token: identity.token,
    }),
  });
  const localStorage = memoryStorage({
    [`${CITIZEN_REMEMBERED_KEY_PREFIX}missing-token`]: JSON.stringify({
      actorId: "citizen-1",
      name: "Citizen",
    }),
  });
  const store = createCitizenIdentityStore({ sessionStorage, localStorage });

  assert.equal(store.loadSession("broken-json"), null);
  assert.equal(store.loadSession("unsafe"), null);
  assert.equal(store.loadRemembered("missing-token"), null);
});

test("storage failures produce a human-readable bounded error without token text", () => {
  const unavailable = {
    getItem() { throw new Error(`raw failure ${identity.token}`); },
    setItem() { throw new Error(`raw failure ${identity.token}`); },
    removeItem() { throw new Error(`raw failure ${identity.token}`); },
  };
  const store = createCitizenIdentityStore({ sessionStorage: unavailable, localStorage: unavailable });

  for (const operation of [
    () => store.loadSession("shared"),
    () => store.saveSession("shared", identity),
    () => store.forgetRemembered("shared"),
  ]) {
    assert.throws(operation, (error) => (
      error.code === "IDENTITY_STORAGE_UNAVAILABLE"
      && /市民情報/.test(error.message)
      && !error.message.includes(identity.token)
    ));
  }
});
