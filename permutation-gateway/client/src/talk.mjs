// Members' messages (V5 §18.7): the bytes a member signs with its session
// key, and ed25519 signing / verification with Node's crypto. Shared by the
// SDK (signing) and the gateway (verifying, anchoring).
//
// A message's bytes are `"permutation-rules/talk" ‖ season u64 ‖ tick u16 ‖
// member u32 ‖ to ‖ text` where `to` is 0 (everyone), 1 ‖ civ u16 (a
// nation) or 2 ‖ member u32 (one member).
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { u64le } from './bytes.mjs';

export const MAX_TALK_CHARS = 280;
/** The signed bytes of a message `{season, tick, member, to, text}`; `to` is null, {civ} or {member}. */
export function talkBytes({ season, tick, member, to, text }) {
  const head = Buffer.alloc(8 + 2 + 4);
  head.set(u64le(season), 0);
  head.writeUInt16LE(tick, 8);
  head.writeUInt32LE(member, 10);
  let target;
  if (to == null) target = Buffer.from([0]);
  else if (to.civ !== undefined) { target = Buffer.alloc(3); target[0] = 1; target.writeUInt16LE(to.civ, 1); }
  else { target = Buffer.alloc(5); target[0] = 2; target.writeUInt32LE(to.member, 1); }
  return Buffer.concat([Buffer.from('permutation-rules/talk'), head, target, Buffer.from(text, 'utf8')]);
}

const SPKI_ED25519 = Buffer.from('302a300506032b6570032100', 'hex');
const PKCS8_ED25519 = Buffer.from('302e020100300506032b657004220420', 'hex');

/** Verify an ed25519 signature by a 32-byte public key. */
export function verifyTalk(bytes, signature, publicKey) {
  try {
    const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(publicKey)]), format: 'der', type: 'spki' });
    return verify(null, bytes, key, Buffer.from(signature));
  } catch {
    return false;
  }
}

/** Sign with a Solana keypair (its 64-byte secret key: seed ‖ public key). */
export function signTalk(bytes, keypair) {
  const key = createPrivateKey({ key: Buffer.concat([PKCS8_ED25519, Buffer.from(keypair.secretKey.slice(0, 32))]), format: 'der', type: 'pkcs8' });
  return new Uint8Array(sign(null, bytes, key));
}

