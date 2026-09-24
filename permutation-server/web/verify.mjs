// In-browser verification of decision logs (§4.3, §7.5).
// Nothing here trusts the server's "verified" flag: the browser recomputes
// the digest from the revealed parts and walks Merkle proofs itself with
// Web Crypto SHA-256, using the same byte layout as permutation-rules.

const enc = new TextEncoder();
export const fromHex = h => Uint8Array.from(h.match(/../g) || [], b => parseInt(b, 16));
export const toHex = b => [...b].map(x => x.toString(16).padStart(2, '0')).join('');
const cat = (...parts) => { const n = parts.reduce((a, p) => a + p.length, 0); const o = new Uint8Array(n); let i = 0; for (const p of parts) { o.set(p, i); i += p.length; } return o; };
export async function sha256(...parts) { return new Uint8Array(await crypto.subtle.digest('SHA-256', cat(...parts))); }
const u16le = v => new Uint8Array([v & 255, (v >> 8) & 255]);

/** sha256("PS/decision/v1" ‖ tick ‖ obs_root ‖ sha256("PS/policy/v1" ‖ policy) ‖ sha256("PS/rationale/v1" ‖ salt ‖ text)) */
export async function recomputeDigest({ tick, obsRoot, policy, salt, text }) {
  const pid = await sha256(enc.encode('PS/policy/v1'), enc.encode(policy));
  const rh = await sha256(enc.encode('PS/rationale/v1'), fromHex(salt), enc.encode(text));
  return toHex(await sha256(enc.encode('PS/decision/v1'), u16le(tick), fromHex(obsRoot), pid, rh));
}

export async function verifyRecord(r) {
  if (!r.reveal) return null;
  return (await recomputeDigest({ tick: r.tick, obsRoot: r.obsRoot, policy: r.policy, salt: r.reveal.salt, text: r.reveal.text })) === r.digest;
}

/** Leaf = sha256(0x00 ‖ len(kind) ‖ kind ‖ body); node = sha256(0x01 ‖ left ‖ right). */
export async function verifyProof(p) {
  const kind = enc.encode(p.kind);
  let acc = await sha256(new Uint8Array([0, kind.length]), kind, fromHex(p.body));
  for (const [sib, left] of p.proof) acc = left ? await sha256(new Uint8Array([1]), fromHex(sib), acc) : await sha256(new Uint8Array([1]), acc, fromHex(sib));
  return toHex(acc) === p.root;
}

/** Borsh tile leaf: (u32 index, i32 q, i32 r, u8 fog, Option<u32> owner_city, Option<u16> ruin). */
export function decodeTile(bodyHex) {
  const b = fromHex(bodyHex), v = new DataView(b.buffer);
  const out = { index: v.getUint32(0, true), q: v.getInt32(4, true), r: v.getInt32(8, true), fog: b[12] };
  out.ownerCity = b[13] === 1 ? v.getUint32(14, true) : null;
  return out;
}
