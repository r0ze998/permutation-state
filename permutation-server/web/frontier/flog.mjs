// PS2 log records (contract §6) the page reads from the herald's
// `/h/events` pages: the chronicle, the tracker (DEPART, REVEAL,
// TRANSIT_SETTLED) and the incoming-arrival warnings (DEPART). The client
// decodes each record's raw body itself; the herald's decoded JSON is never
// used for anything the page acts on.
//
// Body: ver u8 ‖ kind u8 ‖ bell u32 ‖ key ‖ payload ‖ tail (n u8, then n ×
// {entity kind u8, seq u64, head [32]}). The field lists below are the
// kinds this page shows, a transcription of frontier-abi's log table pinned
// field for field against `frontier-abi/vectors/logs.json` by
// web-frontier-march.test.mjs (a changed kind fails that test, never the
// page silently).
import { fromBase64 } from '../sdk/bytes.mjs';

export const PS2_VERSION = 1;
const f = (name, len) => [name, len];
/** kind → {name, key: [[field, bytes]], payload: [[field, bytes]]}. */
export const KINDS = Object.freeze({
  4: { name: 'RING_OPEN', key: [f('d', 2)], payload: [f('t_open', 8), f('round', 8), f('seed', 32)] },
  6: { name: 'PROVINCE_OPEN', key: [f('p', 4), f('q', 4)], payload: [f('ring', 2), f('wedge', 1), f('region', 1), f('terrain_digest', 32), f('site_count', 1), f('camp_tile', 1), f('camp_troops', 4), f('reserved', 1)] },
  10: { name: 'JOIN', key: [f('citizen_tag15', 15)], payload: [f('wallet', 32), f('faction', 1), f('shard', 1), f('session', 32), f('expiry', 8)] },
  13: { name: 'TICKET', key: [f('citizen_tag15', 15)], payload: [f('ticket_bell', 4), f('n', 1), f('sites', 15), f('escrow', 8), f('funder', 32)] },
  14: { name: 'SETTLE', key: [f('p', 4), f('q', 4), f('site', 1)], payload: [f('outcome', 1), f('citizen_tag', 8), f('score', 8), f('displaced_tag', 8), f('gen', 1), f('final_ts', 8), f('ticket_bell', 4)] },
  16: { name: 'HOLDING_FINAL', key: [f('p', 4), f('q', 4), f('site', 1)], payload: [f('final_ts', 8)] },
  23: { name: 'MUSTER', key: [f('host_id', 8)], payload: [f('unit', 1), f('troops', 4), f('tile', 1), f('entry', 1)] },
  26: { name: 'EXPLORE', key: [f('host_id', 8)], payload: [f('p', 4), f('q', 4), f('n', 1), f('tiles', 2)] },
  27: { name: 'EXPLORE_RESULT', key: [f('host_id', 8)], payload: [f('works_per_tile', 8), f('works', 4), f('floor_used', 1)] },
  30: { name: 'DEPART', key: [f('host_id', 8)], payload: [f('origin_p', 4), f('origin_q', 4), f('origin_tile', 1), f('depart_bell', 4), f('arrive_bell', 4), f('dep_mass', 4), f('march_stamina', 2), f('tip', 8), f('seal_root', 32), f('commit', 32), f('seal', 165)] },
  31: { name: 'REVEAL', key: [f('p', 4), f('q', 4), f('arrive', 4), f('faction', 1), f('i', 1)], payload: [f('host_id', 8), f('tile', 1), f('stance', 1), f('retreat', 2), f('displace', 1), f('displaced_host', 8), f('beneficiary', 32), f('ev_slot', 8), f('ev_price', 8), f('ev_limit', 4), f('arrivalday_created', 1)] },
  32: { name: 'DEPARTURE_SETTLED', key: [f('host_id', 8)], payload: [f('troops_after', 4), f('stamina_after', 2), f('destroyed', 1)] },
  34: { name: 'TRANSIT_SETTLED', key: [f('host_id', 8)], payload: [f('outcome', 1), f('seal_code', 1), f('troops', 4), f('tip_to', 8), f('tip', 8), f('fee_to', 8), f('fee', 8), f('bond_to', 8), f('bond', 8), f('reward_to', 8), f('reward', 8), f('pool_owed_delta', 8), f('slot_kept', 1)] },
  41: { name: 'CLASH', key: [f('p', 4), f('q', 4), f('bell', 4)], payload: [f('outcome_digest', 32), f('input_digest', 32), f('engagements', 4), f('fates', 9)] },
  43: { name: 'CAMP', key: [f('p', 4), f('q', 4)], payload: [f('tile', 1), f('troops', 4), f('day', 4)] },
});
/** Fields read as signed integers (coordinates, deltas). */
const SIGNED = new Set(['p', 'q', 'origin_p', 'origin_q', 'delta']);
/** Fields read as byte strings although they are 1–8 bytes long. */
const BYTES = new Set(['tiles', 'works_per_tile']);
/** TRANSIT_SETTLED outcomes, in the log's order (§6). */
export const TRANSIT_OUTCOMES = Object.freeze(['Stays', 'Withdrew', 'Bounced', 'Retreated', 'Destroyed', 'BouncedUnranked', 'Routed', 'BadSeal']);
/** SETTLE outcomes (§6): 0 fresh, 1 displace, 2 taken, 3 expired. */
export const SETTLE_OUTCOMES = Object.freeze(['fresh', 'displace', 'taken', 'expired']);

function readInt(b, o, n, signed) {
  let v = 0n;
  for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(b[o + i]);
  if (signed && v >= 1n << BigInt(8 * n - 1)) v -= 1n << BigInt(8 * n);
  return n === 8 ? v : Number(v);
}

/**
 * One record body (bytes or base64) → `{kind, name, logBell, bell, …key and payload
 * fields}` (snake_case names as in §6; 8-byte integers as BigInt, byte
 * strings as Uint8Array), or null for a kind this page does not read. Throws
 * on a wrong version or a body too short for its kind.
 */
export function decodeRecord(body) {
  const b = typeof body === 'string' ? fromBase64(body) : Uint8Array.from(body);
  if (b.length < 6) throw new Error('PS2: short head');
  if (b[0] !== PS2_VERSION) throw new Error(`PS2: version ${b[0]}`);
  const spec = KINDS[b[1]];
  if (!spec) return null;
  // `logBell`: the bell in the record's head (when it was written); `bell` is it too,
  // unless the kind's key has its own `bell` (CLASH: the clash's bell).
  const out = { kind: b[1], name: spec.name, logBell: readInt(b, 2, 4, false) };
  out.bell = out.logBell;
  let o = 6;
  for (const [name, len] of [...spec.key, ...spec.payload]) {
    if (o + len > b.length) throw new Error(`PS2 ${spec.name}: short at ${name}`);
    out[name] = !BYTES.has(name) && [1, 2, 4, 8].includes(len) ? readInt(b, o, len, SIGNED.has(name)) : b.slice(o, o + len);
    o += len;
  }
  return out;
}

/**
 * The records of an events page (`[{seq, slot, sig, body_b64}]`) this page
 * reads, oldest first: `[{seq, slot, sig, record}]`; an undecodable record is
 * dropped (and counted in `bad`), never shown from the herald's JSON.
 */
export function decodePage(events = []) {
  const out = [];
  let bad = 0;
  for (const e of events) {
    try {
      const record = decodeRecord(e.body_b64 ?? '');
      if (record) out.push({ seq: String(e.seq ?? ''), slot: e.slot ?? null, sig: e.sig ?? null, record });
    } catch { bad++; }
  }
  return { records: out, bad };
}
