// One-time invites for the playtest preset (contract §8.3, I-51). An invite
// is an HMAC token the operator issues (`POST /f/operator/invites`):
//
//   token = base64url(nonce [12] ‖ HMAC-SHA256(secret, "PSF-INVITE-v1" ‖ le64(season_id) ‖ nonce)[0..16])
//
// The relay co-signs a Join with the season's join-gate key only for a
// valid, unused invite, and marks it used once the Join passed simulation
// and was sent; the program refuses a Join without the gate's signature
// (`JoinGate`, 59), so the invite rule holds on chain, not only here.
// Nothing about issued invites is stored (the HMAC proves them); used
// nonces are, so an invite works once.
import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto';

export const INVITE_DOMAIN = 'PSF-INVITE-v1';
const NONCE = 12;
const MAC = 16;

const b64url = b => Buffer.from(b).toString('base64url');

export class InviteBook {
  /** `secret`: 32 bytes; `store`: `{state, save()}` holding `state.invitesUsed` (nonce hex → Clock/unix time). */
  constructor({ secret, seasonId, store = { state: {}, save() {} }, now = () => Math.floor(Date.now() / 1000) }) {
    if (Buffer.from(secret).length !== 32) throw new Error('the invite secret is 32 bytes');
    this.secret = Buffer.from(secret);
    this.season = Buffer.alloc(8);
    this.season.writeBigUInt64LE(BigInt(seasonId));
    this.store = store;
    this.store.state ??= {};
    this.store.state.invitesUsed ??= {};
    this.now = now;
  }

  mac(nonce) {
    return createHmac('sha256', this.secret).update(INVITE_DOMAIN).update(this.season).update(nonce).digest().subarray(0, MAC);
  }

  /** `n` fresh invites (1–1,000). */
  issue(n = 1) {
    if (!Number.isInteger(n) || n < 1 || n > 1000) throw new RangeError('issue 1–1,000 invites at a time');
    return Array.from({ length: n }, () => {
      const nonce = randomBytes(NONCE);
      return b64url(Buffer.concat([nonce, this.mac(nonce)]));
    });
  }

  /** The nonce (hex) of a genuine invite of this season, or null (forged, malformed, another season's). */
  verify(token) {
    if (typeof token !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(token)) return null;
    const b = Buffer.from(token, 'base64url');
    if (b.length !== NONCE + MAC) return null;
    const nonce = b.subarray(0, NONCE);
    return timingSafeEqual(this.mac(nonce), b.subarray(NONCE)) ? nonce.toString('hex') : null;
  }

  used(nonceHex) { return Object.hasOwn(this.store.state.invitesUsed, nonceHex); }

  /**
   * Hold a genuine, unused, not already held invite for one Join in flight:
   * its nonce, or null. Then `consume` (sent) or `release` (refused).
   */
  reserve(token) {
    this.pending ??= new Set();
    const n = this.verify(token);
    if (!n || this.used(n) || this.pending.has(n)) return null;
    this.pending.add(n);
    return n;
  }

  release(nonceHex) { this.pending?.delete(nonceHex); }

  consume(nonceHex) {
    this.pending?.delete(nonceHex);
    this.store.state.invitesUsed[nonceHex] = this.now();
    this.store.save?.();
  }
}
