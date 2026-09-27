// The Frontier's in-game key (contract §9.1, I-40; web design §5.3). The
// wallet signs one fixed printable-ASCII text (a message, not a
// transaction); the key's seed is sha256("PS/frontier-session/v1" ‖ that
// signature), so the same wallet makes the same key on any device. Join
// registers the key's public key in the Citizen (no session signature);
// the key then signs every player action except Join and SetSession.
//
// The text below is what a wallet shows the player: it is pinned byte for
// byte by web-frontier-session.test.mjs and is never translated. v9's
// `sessionText` (web/session.mjs) is not used and never edited; this file
// reuses only its Ed25519 primitives.
//
// Keys live under `ps-fsession:<cluster>:<program>:<season>:<wallet>` (in
// memory when storage is unavailable), with a hex backup.
import { sha256 } from '../sdk/sha256.mjs';
import { encode as toBase58 } from '../sdk/base58.mjs';
import { equal, fromHex, toHex, utf8 } from '../sdk/bytes.mjs';
import { codeError, keyFromSeed, parseBackup, publicKeyOf, verify } from '../session.mjs';
import { L } from '../lang.mjs';

export const SEED_DOMAIN = 'PS/frontier-session/v1';
export const STORE_PREFIX = 'ps-fsession:';

/** The text a wallet signs (never translated; printable ASCII only). */
export function sessionText({ origin, host, cluster, programId, seasonId }) {
  const text = 'Permutation State: Sixfold Frontier wants you to create an in-game key.\n'
    + `Site: ${origin}\n`
    + `Cluster: ${cluster}\n`
    + `Program: ${programId}\n`
    + `Season: ${seasonId}\n`
    + 'Anyone holding this key can act as you in this season (build, train, march, explore). It cannot move tokens from your wallet.\n'
    + `Sign only on ${host}.`;
  if (!/^[\x20-\x7e\n]*$/.test(text)) throw codeError('BadSessionText', 'the key text must be printable ASCII');
  return text;
}

/** The seed of the key a wallet signature makes. */
export const seedOf = signature => sha256(utf8(SEED_DOMAIN), Uint8Array.from(signature));

// ------------------------------------------------------------------ storage
const memory = new Map();
export const storage = {
  get(k) {
    try { const v = globalThis.localStorage?.getItem(k); if (v != null) return v; } catch { /* unavailable */ }
    return memory.get(k) ?? null;
  },
  set(k, v) {
    memory.set(k, v);
    try { globalThis.localStorage.setItem(k, v); return true; } catch { return false; }
  },
  del(k) {
    memory.delete(k);
    try { globalThis.localStorage?.removeItem(k); } catch { /* unavailable */ }
  },
};

const scopeOf = ({ cluster, programId, seasonId }) => `${STORE_PREFIX}${cluster}:${programId}:${seasonId}:`;
/** `ps-fsession:<cluster>:<program>:<season>:<wallet>`. */
export const storeKey = (scope, wallet) => `${scopeOf(scope)}${wallet}`;

async function sessionOf(seed, scope, wallet) {
  const k = await keyFromSeed(seed);
  const hex = toHex(seed);
  return {
    ...k,
    wallet,
    scope: { cluster: scope.cluster, programId: scope.programId, seasonId: String(scope.seasonId) },
    stored: false,
    seedHex: () => hex,
    backupText: () => `PERMUTATION STATE Sixfold Frontier key\nSeason: ${scope.seasonId}\nCluster: ${scope.cluster}\nProgram: ${scope.programId}\n`
      + `Wallet: ${wallet}\nSession: ${k.publicKey}\nKey: ${hex}\n`
      + 'Anyone holding this key can act as you in this season. Keep it private.\n',
  };
}

/**
 * Ask the wallet (`{address, signMessage}`) to sign the text and derive the
 * key. The signature must verify against the wallet before it is used.
 */
export async function derive({ wallet, cluster, programId, seasonId, origin = globalThis.location?.origin ?? '', host = globalThis.location?.host ?? '' }) {
  const message = utf8(sessionText({ origin, host, cluster, programId, seasonId }));
  const out = await wallet.signMessage(message);
  const signature = Uint8Array.from(out?.signature ?? []);
  const signed = out?.signedMessage?.length ? Uint8Array.from(out.signedMessage) : message;
  if (!equal(signed, message)) throw codeError('WalletAlteredMessage', L`ウォレットが署名した文面が違います`);
  if (!(await verify(wallet.address, signed, signature))) throw codeError('WalletBadSignature', L`ウォレットの署名を確認できませんでした`);
  return sessionOf(seedOf(signature), { cluster, programId, seasonId }, wallet.address);
}

/** Keep a key in this browser; returns whether it survives a reload. */
export function remember(session) {
  session.stored = storage.set(storeKey(session.scope, session.wallet), session.seedHex());
  return session.stored;
}
/** Forget this season's key on this device. */
export const forget = session => storage.del(storeKey(session.scope, session.wallet));

/** The kept key of one wallet in this season, or null. */
export async function restore(scope, wallet) {
  const hex = storage.get(storeKey(scope, wallet));
  if (!/^[0-9a-f]{64}$/i.test(hex ?? '')) return null;
  const s = await sessionOf(fromHex(hex), scope, wallet);
  s.stored = true;
  return s;
}

/**
 * Whether a key is the one the Citizen registered (`Citizen.session`, 32
 * bytes, zero = none) and still valid at chain time `now` (s).
 */
export function matchesCitizen(session, citizen, now) {
  const reg = citizen?.session;
  if (!reg || reg.every?.(x => x === 0)) return { ok: false, code: 'NoSession' };
  if (toBase58(reg) !== session.publicKey) return { ok: false, code: 'SessionMismatch' };
  if (now !== undefined && citizen.sessionExpiry !== undefined && BigInt(Math.floor(now)) >= BigInt(citizen.sessionExpiry)) return { ok: false, code: 'SessionExpired' };
  return { ok: true };
}

/**
 * Import a backup: the key must be the one a Citizen of this season
 * registered (`citizens`: `[{wallet, session (base58)}]` from the herald).
 */
export async function importBackup(text, scope, citizens) {
  const seed = parseBackup(text);
  if (!seed) throw codeError('BadBackup', L`鍵のバックアップを読めませんでした（16進64文字の鍵が必要です）`);
  const pub = toBase58(await publicKeyOf(seed));
  const c = (citizens || []).find(x => x.session === pub);
  if (!c) throw codeError('SessionMismatch', L`この鍵は、このシーズンのどの市民の鍵とも一致しません`);
  const session = await sessionOf(seed, scope, c.wallet);
  remember(session);
  return { session, citizen: c };
}
