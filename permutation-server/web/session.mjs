// This browser's key for one season: the member's session key (V5 D13).
//
// The wallet signs one fixed, printable text (free: a message, not a
// transaction); the key's seed is sha256("PS/session/v1" ‖ that signature),
// so the same wallet makes the same key on any device. The key signs
// CommitOrders, SubmitGov, talk and seal receipts. It cannot claim the prize
// or move tokens from the wallet, but an officer's key can spend the
// nation's treasury (market, contracts): the text says so.
//
// Keys live in localStorage under `ps-session:<cluster>:<programId>:<seasonId>:<wallet>`
// (in memory when storage is unavailable: a reload then asks for the
// signature again), can be backed up as hex and imported back. Also the
// WebCrypto Ed25519 helpers the dev wallet shares.
import { sha256 } from './sdk/sha256.mjs';
import { encode as toBase58 } from './sdk/base58.mjs';
import { concat, equal, fromBase64, fromHex, toHex, utf8 } from './sdk/bytes.mjs';
import { pubkeyBytes } from './sdk/solana-tx.mjs';
import { L } from './lang.mjs';

const subtle = () => globalThis.crypto?.subtle;
const ED25519 = { name: 'Ed25519' };
/** PKCS#8 wrapping of a raw 32-byte Ed25519 seed (RFC 8410). */
const PKCS8_PREFIX = fromHex('302e020100300506032b657004220420');
const STORE_PREFIX = 'ps-session:';

/** An Error with a machine-readable `code` (i18n.mjs errorText translates it). */
export function codeError(code, message) {
  const e = new Error(message || code);
  e.code = code;
  return e;
}

// ------------------------------------------------------------------ Ed25519 public key (pure JS)
// WebCrypto cannot hand out the public key of an imported seed everywhere
// (a PKCS#8 without it, a JWK export some engines refuse), so it is computed
// here as RFC 8032 does — SHA-512 of the seed, clamped, times the base point —
// and cross-checked against the engine's JWK `x` when it gives one.
const P = 2n ** 255n - 19n;
const D2 = (2n * 37095705934669439343138083508754565189542113879843219016388785533085940283555n) % P;
const GX = 15112221349535400772501151409588531511454012693041857206046113283949847762202n;
const GY = 46316835694926478169428394003475163141307993866256225615783033603165251855960n;
const mod = a => { const r = a % P; return r < 0n ? r + P : r; };
function pow(b, e) {
  let r = 1n;
  for (b = mod(b); e > 0n; e >>= 1n) { if (e & 1n) r = (r * b) % P; b = (b * b) % P; }
  return r;
}
/** Unified addition in extended coordinates (a = −1; also doubles). */
function add([X1, Y1, Z1, T1], [X2, Y2, Z2, T2]) {
  const A = mod((Y1 - X1) * (Y2 - X2)), B = mod((Y1 + X1) * (Y2 + X2));
  const C = mod(T1 * D2 % P * T2), D = mod(2n * Z1 * Z2);
  const E = B - A, F = D - C, G = D + C, H = B + A;
  return [mod(E * F), mod(G * H), mod(F * G), mod(E * H)];
}
function times(n, point) {
  let r = [0n, 1n, 1n, 0n];
  for (let q = point; n > 0n; n >>= 1n) { if (n & 1n) r = add(r, q); q = add(q, q); }
  return r;
}
function encodePoint([X, Y, Z]) {
  const zi = pow(Z, P - 2n);
  const x = (X * zi) % P;
  let y = (Y * zi) % P;
  const out = new Uint8Array(32);
  for (let i = 0; i < 32; i++, y >>= 8n) out[i] = Number(y & 255n);
  if (x & 1n) out[31] |= 0x80;
  return out;
}

/** The Ed25519 public key (32 bytes) of a 32-byte seed. */
export async function publicKeyOf(seed) {
  const h = new Uint8Array(await subtle().digest('SHA-512', Uint8Array.from(seed)));
  const a = h.slice(0, 32);
  a[0] &= 248; a[31] &= 127; a[31] |= 64;
  let s = 0n;
  for (let i = 31; i >= 0; i--) s = (s << 8n) | BigInt(a[i]);
  return encodePoint(times(s, [GX, GY, 1n, (GX * GY) % P]));
}

// ------------------------------------------------------------------ WebCrypto keys
/**
 * An Ed25519 signer for a 32-byte seed: `{publicKey (base58), publicKeyBytes,
 * sign(bytes) → Promise<64 bytes>}`. The signing key is not extractable.
 */
export async function keyFromSeed(seed) {
  const s = Uint8Array.from(seed);
  if (s.length !== 32) throw codeError('BadKey', 'a key seed is 32 bytes');
  const pkcs8 = concat(PKCS8_PREFIX, s);
  const pub = await publicKeyOf(s);
  // The engine's own public key, where it exports one, must agree.
  let jwk = null;
  try { jwk = await subtle().exportKey('jwk', await subtle().importKey('pkcs8', pkcs8, ED25519, true, ['sign'])); } catch { /* not exportable here */ }
  if (jwk?.x && !equal(fromBase64(jwk.x), pub)) throw codeError('NoEd25519', 'Ed25519 public key mismatch');
  const key = await subtle().importKey('pkcs8', pkcs8, ED25519, false, ['sign']);
  return {
    publicKey: toBase58(pub),
    publicKeyBytes: pub,
    sign: async bytes => new Uint8Array(await subtle().sign(ED25519, key, Uint8Array.from(bytes))),
  };
}

/** Whether `signature` is `publicKey`'s (base58 or bytes) Ed25519 signature of `message`. Never throws. */
export async function verify(publicKey, message, signature) {
  try {
    const sig = Uint8Array.from(signature);
    if (sig.length !== 64) return false;
    const key = await subtle().importKey('raw', pubkeyBytes(publicKey), ED25519, false, ['verify']);
    return await subtle().verify(ED25519, key, sig, Uint8Array.from(message));
  } catch {
    return false;
  }
}

/**
 * Can this page make and use session keys? A secure context (https, or
 * 127.0.0.1 / localhost) and a WebCrypto that really signs with Ed25519
 * (Chrome/Edge 137+, Firefox 129+, Safari 17+). `{ok}` or `{ok:false, code, error}`.
 */
export async function preflight({ secure = globalThis.isSecureContext } = {}) {
  if (!secure || !subtle()) {
    return { ok: false, code: 'InsecureContext', error: L`https か 127.0.0.1 で開いてください（この接続ではブラウザの暗号機能が使えません）` };
  }
  try {
    const k = await keyFromSeed(new Uint8Array(32).fill(7));
    const msg = utf8('Permutation State preflight');
    if (!(await verify(k.publicKey, msg, await k.sign(msg)))) throw new Error('verify');
  } catch {
    return { ok: false, code: 'NoEd25519', error: L`このブラウザは Ed25519 署名に対応していません。最新の Chrome・Edge・Firefox・Safari で開いてください。` };
  }
  return { ok: true };
}

// ------------------------------------------------------------------ the text the wallet signs
/**
 * The message a wallet signs to make the session key: printable ASCII only
 * (a hardware wallet can show it without blind signing), naming the site,
 * cluster, program and season, and what the key can and cannot do.
 */
export function sessionText({ host, cluster, programId, seasonId }) {
  const text = 'Permutation State wants you to create an in-game key.\n'
    + `Site: ${host}\nCluster: ${cluster}\nProgram: ${programId}\nSeason: ${seasonId}\n`
    + 'Anyone holding this key can act as you in this season (orders, votes, and your nation\'s treasury if you hold an office). '
    + 'It cannot claim your prize or move tokens from your wallet.\n'
    + `Sign only on ${host}.`;
  if (!/^[\x20-\x7e\n]*$/.test(text)) throw codeError('BadSessionText', 'the key text must be printable ASCII');
  return text;
}

/** The seed of the session key a wallet signature makes. */
export const seedOf = signature => sha256(utf8('PS/session/v1'), Uint8Array.from(signature));

// ------------------------------------------------------------------ storage
// localStorage when it works, and a copy in memory for this page's life.
const memory = new Map();
const storage = {
  get(k) {
    try { const v = globalThis.localStorage?.getItem(k); if (v != null) return v; } catch { /* unavailable */ }
    return memory.get(k) ?? null;
  },
  /** Returns whether the value reached localStorage (survives a reload). */
  set(k, v) {
    memory.set(k, v);
    try { globalThis.localStorage.setItem(k, v); return true; } catch { return false; }
  },
  del(k) {
    memory.delete(k);
    try { globalThis.localStorage?.removeItem(k); } catch { /* unavailable */ }
  },
  keys(prefix) {
    const out = new Set([...memory.keys()].filter(k => k.startsWith(prefix)));
    try {
      const ls = globalThis.localStorage;
      for (let i = 0; ls && i < ls.length; i++) { const k = ls.key(i); if (k?.startsWith(prefix)) out.add(k); }
    } catch { /* unavailable */ }
    return [...out];
  },
};

/** The season a key belongs to: `{cluster, programId, seasonId}` (strings). */
const scopeOf = ({ cluster, programId, seasonId }) => `${STORE_PREFIX}${cluster}:${programId}:${seasonId}:`;
export const storeKey = (scope, wallet) => `${scopeOf(scope)}${wallet}`;

/**
 * A session: `{publicKey, publicKeyBytes, wallet, scope, stored, sign(bytes),
 * backupText()}`. `wallet` is the member's wallet (base58) the key is filed
 * under; `stored` whether it survives a reload.
 */
async function sessionOf(seed, scope, wallet) {
  const k = await keyFromSeed(seed);
  const hex = toHex(seed);
  return {
    ...k,
    wallet,
    scope: { cluster: scope.cluster, programId: scope.programId, seasonId: String(scope.seasonId) },
    stored: false,
    seedHex: () => hex,
    backupText: () => `Wylls session key\nSeason: ${scope.seasonId}\nCluster: ${scope.cluster}\nProgram: ${scope.programId}\n`
      + `Wallet: ${wallet}\nSession: ${k.publicKey}\nKey: ${hex}\n`
      + 'Anyone holding this key can act as you in this season. Keep it private.\n',
  };
}

/**
 * Ask the wallet for the signature and derive the session key (not saved:
 * see `remember`). `wallet` is a connected wallet (wallet.mjs): `{address,
 * signMessage}`. The signature is checked against the wallet's public key
 * before it is used.
 */
export async function derive({ wallet, cluster, programId, seasonId, host = globalThis.location?.host ?? '' }) {
  const message = utf8(sessionText({ host, cluster, programId, seasonId }));
  const out = await wallet.signMessage(message);
  const signature = Uint8Array.from(out?.signature ?? []);
  const signed = out?.signedMessage?.length ? Uint8Array.from(out.signedMessage) : message;
  if (!(await verify(wallet.address, signed, signature))) throw codeError('WalletBadSignature', L`ウォレットの署名を確認できませんでした`);
  return sessionOf(seedOf(signature), { cluster, programId, seasonId }, wallet.address);
}

/** Keep a session in this browser (and memory). Sets and returns `session.stored`. */
export function remember(session) {
  session.stored = storage.set(storeKey(session.scope, session.wallet), session.seedHex());
  return session.stored;
}

/** Drop a kept session. */
export const forget = session => storage.del(storeKey(session.scope, session.wallet));

/** The kept session of one wallet in this season, or null. */
export async function restore(scope, wallet) {
  const hex = storage.get(storeKey(scope, wallet));
  if (!/^[0-9a-f]{64}$/i.test(hex ?? '')) return null;
  const s = await sessionOf(fromHex(hex), scope, wallet);
  s.stored = true;
  return s;
}

/** Every session kept for this season, whatever the wallet (a returning visit needs no wallet popup). */
export async function cached(scope) {
  const prefix = scopeOf(scope);
  const out = [];
  for (const k of storage.keys(prefix)) {
    try { const s = await restore(scope, k.slice(prefix.length)); if (s) out.push(s); } catch { /* a damaged entry */ }
  }
  return out;
}

/** The 32-byte key in a backup (the text `backupText` writes, or bare hex), or null. */
export function parseBackup(text) {
  const m = String(text ?? '').match(/(?:^|[^0-9a-f])([0-9a-f]{64})(?![0-9a-f])/i);
  return m ? fromHex(m[1].toLowerCase()) : null;
}

/**
 * Import a backup: the key must be the session key of one of `members`
 * (`/season` members: {session, wallet, …}). Returns `{session, member}`,
 * with the session kept in this browser.
 */
export async function importBackup(text, scope, members) {
  const seed = parseBackup(text);
  if (!seed) throw codeError('BadBackup', L`鍵のバックアップを読めませんでした（16進64文字の鍵が必要です）`);
  const pub = toBase58(await publicKeyOf(seed));
  const member = (members || []).find(m => m.session === pub);
  if (!member) throw codeError('SessionMismatch', L`この鍵は、このシーズンのどのメンバーの鍵とも一致しません`);
  const session = await sessionOf(seed, scope, member.wallet);
  remember(session);
  return { session, member };
}
