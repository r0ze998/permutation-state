// The gateway's side of members' messages (V5 §18.7): kept, public, and
// anchored on chain once per tick (`AnchorTalk` logs the Merkle root of the
// tick's messages), so anyone can later prove who said what by when. Words
// bind nothing; contracts do. Signing and the signed bytes: client/src/talk.mjs.
import { createHash } from 'node:crypto';
import { toHex } from '../client/src/bytes.mjs';
import { MAX_TALK_CHARS, signTalk, talkBytes, verifyTalk } from '../client/src/talk.mjs';

export { MAX_TALK_CHARS, signTalk, talkBytes, verifyTalk };
/** Messages one member may send in one tick. */
export const TALK_PER_TICK = 3;

const sha256 = (...parts) => { const h = createHash('sha256'); for (const p of parts) h.update(p); return new Uint8Array(h.digest()); };

/** A message's leaf in the tick's Merkle tree. */
export const talkLeaf = m => sha256(Buffer.from([0]), Buffer.from(m.bytes, 'hex'), Buffer.from(m.signature, 'hex'));

/** One level up the Merkle tree: pairs hashed with a 0x01 prefix, an odd last node carried up as is. */
export function nextLevel(level) {
  const next = [];
  for (let i = 0; i < level.length; i += 2) next.push(i + 1 < level.length ? sha256(Buffer.from([1]), level[i], level[i + 1]) : level[i]);
  return next;
}

/** Merkle root of leaves (in order); 32 zero bytes for none. */
export function merkleRoot(leaves) {
  if (!leaves.length) return new Uint8Array(32);
  let level = leaves;
  while (level.length > 1) level = nextLevel(level);
  return level[0];
}

/** Merkle proof of leaf `i`: sibling hashes from the bottom, each with its side. */
export function merkleProof(leaves, i) {
  const proof = [];
  let level = leaves;
  let k = i;
  while (level.length > 1) {
    const sib = k ^ 1;
    // A node carried up alone has no sibling at this level.
    if (sib < level.length) proof.push({ hash: toHex(level[sib]), left: sib < k });
    level = nextLevel(level);
    k >>= 1;
  }
  return proof;
}

/**
 * The messages of one season, kept in the gateway's state (`state.talk`):
 * `{id, tick, member, to, text, bytes, signature, anchored?}`.
 */
export class TalkBook {
  constructor(state) {
    this.state = state;
    state.talk ??= [];
  }

  /** Add a signed message; throws with a reason if it is refused. */
  add({ season, tick, member, to = null, text, signature, publicKey }) {
    if (typeof text !== 'string' || !text.trim() || [...text].length > MAX_TALK_CHARS) throw new Error(`text: 1–${MAX_TALK_CHARS} characters`);
    const sent = this.state.talk.filter(m => m.tick === tick && m.member === member).length;
    if (sent >= TALK_PER_TICK) throw new Error(`at most ${TALK_PER_TICK} messages per member and tick`);
    const bytes = talkBytes({ season, tick, member, to, text });
    if (!verifyTalk(bytes, signature, publicKey)) throw new Error('the signature is not the member\'s session key over the message');
    const m = { id: this.state.talk.length, tick, member, to, text, bytes: bytes.toString('hex'), signature: toHex(signature) };
    this.state.talk.push(m);
    return m;
  }

  /** Whether tick `tick`'s messages were anchored already. */
  isAnchored(tick) { return this.state.talk.some(m => m.tick === tick && m.anchored); }

  since(id = 0) { return this.state.talk.filter(m => m.id >= id); }

  /** The unanchored messages of ticks before `open`, grouped by tick. */
  pending(open) {
    const byTick = new Map();
    for (const m of this.state.talk) if (!m.anchored && m.tick < open) byTick.set(m.tick, [...(byTick.get(m.tick) ?? []), m]);
    return byTick;
  }

  /** The root over a tick's messages (in id order). */
  static root(messages) { return merkleRoot(messages.map(talkLeaf)); }
}
