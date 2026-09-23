const SESSION_KEY_PREFIX = "permutation.citizen.v2:";
const REMEMBERED_KEY_PREFIX = "permutation.citizen.remembered.v1:";

function storageError(operation) {
  const message = {
    read: "保存した市民情報を読み込めませんでした。ブラウザの保存設定を確認してください。",
    save: "市民情報をこの端末に保存できませんでした。ブラウザの保存設定を確認してください。",
    remove: "この端末の市民情報を削除できませんでした。ブラウザの保存設定を確認してください。",
  }[operation] || "市民情報の保存領域を利用できませんでした。";
  // Storage implementations can throw arbitrary text. Do not retain it here:
  // a browser extension could include the value being saved (the capability)
  // in that text, and callers may display or report this error.
  return Object.assign(new Error(message), { code: "IDENTITY_STORAGE_UNAVAILABLE" });
}

function assertStorage(storage, label) {
  if (
    !storage
    || typeof storage.getItem !== "function"
    || typeof storage.setItem !== "function"
    || typeof storage.removeItem !== "function"
  ) {
    throw new TypeError(`${label} must implement getItem, setItem, and removeItem`);
  }
  return storage;
}

function normalizeSession(value) {
  if (typeof value !== "string" || !/^[a-z0-9][a-z0-9_-]{0,39}$/.test(value)) {
    throw Object.assign(new Error("文明の保存IDが正しくありません。"), { code: "INVALID_IDENTITY_SESSION" });
  }
  return value;
}

function normalizeIdentity(value, { requireToken = false, throwOnInvalid = true } = {}) {
  const fail = () => {
    if (!throwOnInvalid) return null;
    throw Object.assign(new Error("市民情報が正しくありません。"), { code: "INVALID_CITIZEN_IDENTITY" });
  };
  if (!value || typeof value !== "object" || Array.isArray(value)) return fail();

  const { actorId, name, token } = value;
  const reserved = actorId === "prototype"
    || (typeof actorId === "string" && Object.prototype.hasOwnProperty.call(Object.prototype, actorId));
  if (typeof actorId !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(actorId) || reserved) return fail();
  if (
    typeof name !== "string"
    || name !== name.trim()
    || !name
    || name.length > 24
    || /[<>\u0000-\u001f\u007f]/.test(name)
  ) return fail();

  const hasToken = token !== undefined && token !== null && token !== "";
  if (requireToken && !hasToken) return fail();
  if (hasToken && (typeof token !== "string" || !/^[A-Za-z0-9_-]{32,128}$/.test(token))) return fail();
  return hasToken ? { actorId, name, token } : { actorId, name };
}

function parseStoredIdentity(raw, options) {
  if (typeof raw !== "string" || !raw) return null;
  try {
    return normalizeIdentity(JSON.parse(raw), { ...options, throwOnInvalid: false });
  } catch {
    return null;
  }
}

function read(storage, key, options) {
  let raw;
  try {
    raw = storage.getItem(key);
  } catch (error) {
    throw storageError("read");
  }
  return parseStoredIdentity(raw, options);
}

function save(storage, key, identity, options) {
  const normalized = normalizeIdentity(identity, options);
  try {
    storage.setItem(key, JSON.stringify(normalized));
  } catch (error) {
    throw storageError("save");
  }
  return { ...normalized };
}

/**
 * Browser identity storage with deliberately separate lifetimes.
 *
 * Session identity remains the default and wins within each tab. Remembered
 * identity is only read or written by an explicit user action; this helper
 * never copies it into sessionStorage or silently merges browser tabs.
 */
export function createCitizenIdentityStore({ sessionStorage, localStorage } = {}) {
  const sessionStore = assertStorage(sessionStorage, "sessionStorage");
  const rememberedStore = assertStorage(localStorage, "localStorage");

  return Object.freeze({
    loadSession(session) {
      return read(sessionStore, `${SESSION_KEY_PREFIX}${normalizeSession(session)}`, { requireToken: false });
    },

    saveSession(session, identity) {
      return save(sessionStore, `${SESSION_KEY_PREFIX}${normalizeSession(session)}`, identity, { requireToken: false });
    },

    loadRemembered(session) {
      return read(rememberedStore, `${REMEMBERED_KEY_PREFIX}${normalizeSession(session)}`, { requireToken: true });
    },

    saveRemembered(session, identity) {
      return save(rememberedStore, `${REMEMBERED_KEY_PREFIX}${normalizeSession(session)}`, identity, { requireToken: true });
    },

    forgetRemembered(session) {
      const key = `${REMEMBERED_KEY_PREFIX}${normalizeSession(session)}`;
      try {
        rememberedStore.removeItem(key);
      } catch (error) {
        throw storageError("remove");
      }
    },
  });
}

export const CITIZEN_SESSION_KEY_PREFIX = SESSION_KEY_PREFIX;
export const CITIZEN_REMEMBERED_KEY_PREFIX = REMEMBERED_KEY_PREFIX;
