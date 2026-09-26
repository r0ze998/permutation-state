// The key each member was seated with in the world, which is the key its
// in-game signatures (orders, votes, talk) must verify against.
//
// Register cannot compare session keys across members, so two members can
// register one key. The program (permutation-chain seat.rs `seat_key`) seats
// members in registration order: the first with a key keeps it, and a later
// one gets a substitute nobody can sign with,
// `sha256("permutation-state/duplicate-session" ‖ season u64le ‖ wallet)`
// (hashed again with the previous candidate while that one is taken). Its
// PS_SEAT records list every seated key. So a member's session key in its
// Member account is not always its key in the world.
import { Reader } from '../client/src/borsh.mjs';
import { fromHex, toHex, u64le } from '../client/src/bytes.mjs';
import { sha256 } from '../client/src/sha256.mjs';
import { pubkeyBytes } from '../client/src/solana-tx.mjs';

export const DUPLICATE_SESSION = new TextEncoder().encode('permutation-state/duplicate-session');

/** The first substitute for a duplicate session key of `wallet`'s member (seat.rs `substitute_key`). */
export const substituteKey = (seasonId, wallet) => sha256(DUPLICATE_SESSION, u64le(seasonId), pubkeyBytes(wallet));

/**
 * The keys the program seats `members` ([{index, wallet, session}], keys
 * base58 or bytes; every member of the season) with, replayed as seat.rs
 * does: index → Uint8Array(32).
 */
export function seatKeys(members, seasonId) {
  const seated = new Set();
  const keys = new Map();
  for (const m of [...members].sort((a, b) => a.index - b.index)) {
    let key = pubkeyBytes(m.session);
    if (seated.has(toHex(key))) {
      const wallet = pubkeyBytes(m.wallet);
      key = substituteKey(seasonId, wallet);
      for (let i = 0, n = seated.size; i < n && seated.has(toHex(key)); i++) key = sha256(DUPLICATE_SESSION, u64le(seasonId), wallet, key);
    }
    seated.add(toHex(key));
    keys.set(m.index, key);
  }
  return keys;
}

/**
 * The seats the PS_SEAT records list, in registration order (`seating` as
 * the state file keeps them: [{members: hex of borsh Vec<(civ u16, key
 * [32], stand u8, votes [u32; 4])>}], in the order they were logged).
 */
export function seatsFromRecords(seating = []) {
  const seats = [];
  for (const rec of seating) {
    const r = new Reader(fromHex(rec.members));
    seats.push(...r.vec(x => ({ civ: x.u16(), key: x.fixed(32), stand: x.u8(), votes: [x.u32(), x.u32(), x.u32(), x.u32()] })));
  }
  return seats;
}

/**
 * Each member's seated key: from the PS_SEAT records the program logged when
 * they list every member (the world exists and seating is complete), else
 * replayed from the Member accounts (seatKeys; before seating, the key the
 * program will seat it with). index → Uint8Array(32).
 */
export function seatedKeyMap({ members, seasonId, seating = [] }) {
  const keys = seatKeys(members, seasonId);
  let seats = [];
  try {
    seats = seatsFromRecords(seating);
  } catch {
    return keys; // an unreadable record: the replay says the same
  }
  const indices = [...keys.keys()].sort((a, b) => a - b);
  if (seats.length === indices.length && indices.every((index, i) => index === i)) {
    for (const [i, s] of seats.entries()) keys.set(i, Uint8Array.from(s.key));
  }
  return keys;
}
