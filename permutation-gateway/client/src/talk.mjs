// Members' messages (V5 §18.7): the bytes a member signs with its session
// key, and the limits. Pure, so the SDK, the gateway and the web client sign
// and check the same bytes; ed25519 with Node's crypto is in talk-node.mjs
// (the browser signs with WebCrypto).
//
// A message's bytes are `"permutation-rules/talk" ‖ season u64 ‖ tick u16 ‖
// member u32 ‖ to ‖ text` where `to` is 0 (everyone), 1 ‖ civ u16 (a
// nation) or 2 ‖ member u32 (one member). test/talk.test.mjs pins them.
import { concat, u16le, u32le, u64le, utf8 } from './bytes.mjs';

/** Characters (code points) per message at most. */
export const MAX_TALK_CHARS = 280;
/** Messages one member may send in one tick. */
export const TALK_PER_TICK = 3;

const TAG = utf8('permutation-rules/talk');

/** The signed bytes of a message `{season, tick, member, to, text}`; `to` is null, {civ} or {member}. */
export function talkBytes({ season, tick, member, to, text }) {
  const target = to == null ? [0] : to.civ !== undefined ? concat([1], u16le(to.civ)) : concat([2], u32le(to.member));
  return concat(TAG, u64le(season), u16le(tick), u32le(member), target, utf8(text));
}
