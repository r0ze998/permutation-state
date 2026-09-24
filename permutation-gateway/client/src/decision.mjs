// Commit-reveal of an agent's reasoning (§4.3, §7.5), byte-for-byte the
// same as permutation-rules `decision.rs`:
//   policy_id      = sha256("PS/policy/v1" ‖ policy)
//   rationale_hash = sha256("PS/rationale/v1" ‖ salt16 ‖ text)
//   digest         = sha256("PS/decision/v1" ‖ tick u16le ‖ obs_root ‖ policy_id ‖ rationale_hash)
// The batch carries only the digest; the next batches reveal policy, salt
// and text, and anyone can recompute the digest.
import { createHash, randomBytes } from 'node:crypto';

export const MAX_POLICY = 64;
export const MAX_RATIONALE = 512;

const enc = new TextEncoder();
const sha = (...parts) => { const h = createHash('sha256'); for (const p of parts) h.update(p); return new Uint8Array(h.digest()); };
const hexToBytes = h => Uint8Array.from(Buffer.from(h, 'hex'));
const toHex = b => Buffer.from(b).toString('hex');

/** Cut `s` to at most `max` UTF-8 bytes on a character boundary (as the server does). */
export function clip(s, max) {
  const b = enc.encode(s);
  if (b.length <= max) return s;
  let end = max;
  while (end > 0 && (b[end] & 0xc0) === 0x80) end--;
  return new TextDecoder().decode(b.slice(0, end));
}

export function decisionDigest({ tick, obsRoot, policy, salt, text }) {
  const pid = sha(enc.encode('PS/policy/v1'), enc.encode(policy));
  const rh = sha(enc.encode('PS/rationale/v1'), hexToBytes(salt), enc.encode(text));
  const t = new Uint8Array(2); new DataView(t.buffer).setUint16(0, tick, true);
  return toHex(sha(enc.encode('PS/decision/v1'), t, hexToBytes(obsRoot), pid, rh));
}

/** A fresh commitment for `tick` against the observation root the server published. */
export function commit({ tick, obsRoot, policy, text }) {
  const record = { tick, obsRoot, policy: clip(policy, MAX_POLICY), salt: toHex(randomBytes(16)), text: clip(text || '', MAX_RATIONALE) };
  return { ...record, digest: decisionDigest(record) };
}

/** The `RevealRationale` order that opens `record` in a later batch. */
export const revealOrder = r => ({ type: 'RevealRationale', tick: r.tick, policy: r.policy, salt: r.salt, text: r.text });
