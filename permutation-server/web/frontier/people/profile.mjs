// A player's own name (design session, "people" request): optional, off
// the chain, signed by the wallet so nobody else can rename a player. The
// derived name (identity.mjs) stays the default everywhere; a profile only
// replaces it after its signature verifies against the Citizen's wallet.
//
//   message  = PROFILE_DOMAIN ‖ "\n" ‖ canonical JSON of
//              {v: 1, season, wallet (base58), name, ts}
//   profile  = {...those fields, sig (base64 of the ed25519 signature)}
//
// Where profiles live is not decided yet (M2: a herald `/h/profiles` file
// that the herald itself verifies before serving, so spectators verify
// nothing). Until then the page keeps the viewer's own profile on this
// device only, verified with WebCrypto Ed25519 where the browser has it.
import { fromBase64, toBase64 } from '../../sdk/bytes.mjs';
import { decode as fromBase58 } from '../../sdk/base58.mjs';

export const PROFILE_DOMAIN = 'PS-FRONTIER-PROFILE-v1';
export const NAME_MAX = 24;

/** A name a profile may carry: 1–24 letters, digits, spaces, '-', '_' or '.', in any script; no control or markup characters. */
export function validName(name) {
  const n = String(name ?? '').normalize('NFC').trim();
  if (!n || [...n].length > NAME_MAX) return null;
  if (!/^[\p{L}\p{N} ._\-]+$/u.test(n)) return null;
  return n;
}

/** The bytes the wallet signs for a profile. */
export function profileMessage({ season, wallet, name, ts }) {
  const body = JSON.stringify({ v: 1, season: String(season), wallet: String(wallet), name: validName(name), ts: Number(ts) });
  return new TextEncoder().encode(`${PROFILE_DOMAIN}\n${body}`);
}

/** Make a profile with a signer `sign(bytes) → Uint8Array(64)` (a wallet's signMessage). */
export async function makeProfile({ season, wallet, name, ts = Math.floor(Date.now() / 1000) }, sign) {
  const n = validName(name);
  if (!n) throw Object.assign(new Error('bad name'), { code: 'BadName' });
  const sig = await sign(profileMessage({ season, wallet, name: n, ts }));
  return { v: 1, season: String(season), wallet: String(wallet), name: n, ts, sig: toBase64(sig) };
}

/** Whether a profile's signature verifies against its wallet (false when the browser has no Ed25519). */
export async function verifyProfile(p, subtle = globalThis.crypto?.subtle) {
  try {
    if (!p || p.v !== 1 || !validName(p.name) || !subtle) return false;
    const key = await subtle.importKey('raw', fromBase58(p.wallet), { name: 'Ed25519' }, false, ['verify']);
    return await subtle.verify({ name: 'Ed25519' }, key, fromBase64(p.sig), profileMessage(p));
  } catch { return false; }
}
