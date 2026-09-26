// ed25519 signing and verification of members' messages with Node's crypto
// (the SDK signs, the gateway and verify-talk check). The signed bytes are
// talk.mjs `talkBytes`; the web client signs them with WebCrypto instead.
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';

export { MAX_TALK_CHARS, TALK_PER_TICK, talkBytes } from './talk.mjs';

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
