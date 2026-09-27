// This browser's record of its sealed marches (contract §9.1; web design
// §8.2 step 5, §8.4) — pure functions over an injected storage, in the
// pattern of sealbook.mjs. An entry is written **before** Depart is signed,
// so a lost answer never loses a sealed order; it holds what a self-reveal
// needs (the plaintext and the salt, never k or σ).
//
// Key: `ps-fmarch:<cluster>:<program>:<season>:<wallet>`, value: a JSON
// array of entries (v1):
//   {v:1, host:"u64 dec", transitSlot, departBell, arriveBell, plain_b64,
//    salt_b64, commit_hex, sealRoot_hex, round, tip, state, attempts:[{t,
//    route:"self"|"keeper", result}]}
// plus three fields this module adds (additive, recorded in W2-E notes):
// `holding` (the Holding address the reveal names), `ctHash_hex` (the
// reveal's ct_hash: the seal root cannot give it back) and `revealDelay`
// (the 0–20 s self-reveal delay, drawn once).
//
// Self-reveal (§9.1): at bell_start(arrive) + the delay, once; again only
// if the ArrivalSlot is still absent 60 s later and the window is open;
// never before the arrival bell starts, never after the window closed.
import { fromBase64, fromHex, toBase64, toHex } from '../sdk/bytes.mjs';

export const STATES = Object.freeze(['sealed', 'sent', 'landed', 'revealing', 'revealed', 'settled', 'failed']);
export const MAX_ENTRIES = 64;
export const REVEAL_DELAY_MAX = 20;
export const RETRY_AFTER = 60;
export const MAX_SELF_ATTEMPTS = 2;
/** Keys an entry may never carry (the seal key and σ, web design §12). */
const FORBIDDEN = ['k', 'sigma', 'k_b64', 'sigma_b64', 'key'];

export const bookKey = ({ cluster, programId, seasonId }, wallet) => `ps-fmarch:${cluster}:${programId}:${seasonId}:${wallet}`;

/** localStorage when it works (else nothing survives a reload; the caller says so). */
export const browserStorage = {
  get(k) { try { return globalThis.localStorage?.getItem(k) ?? null; } catch { return null; } },
  set(k, v) { try { globalThis.localStorage.setItem(k, v); return true; } catch { return false; } },
};

const isEntry = e => e && e.v === 1 && /^\d+$/.test(String(e.host)) && Number.isInteger(e.transitSlot) && Number.isInteger(e.arriveBell)
  && typeof e.plain_b64 === 'string' && typeof e.salt_b64 === 'string' && STATES.includes(e.state) && Array.isArray(e.attempts);
const scrub = e => { const o = { ...e }; for (const k of FORBIDDEN) delete o[k]; return o; };

/** The stored book (an array), with damaged entries dropped and forbidden fields never read back. */
export function loadBook(storage, key) {
  try {
    const v = JSON.parse(storage.get(key) ?? '[]');
    return Array.isArray(v) ? v.filter(isEntry).map(scrub) : [];
  } catch {
    return [];
  }
}
/** Save; returns whether it reached storage (a march may only be sent when it did). */
export const saveBook = (storage, key, book) => storage.set(key, JSON.stringify(book));

const same = (a, b) => a.host === b.host && a.transitSlot === b.transitSlot && a.arriveBell === b.arriveBell;

/**
 * A new entry from a sealing result (bytes as Uint8Array). Refuses any
 * field named like the seal key. `random` draws the reveal delay.
 */
export function entryOf(m, random = Math.random) {
  for (const k of FORBIDDEN) if (k in m) throw new Error(`the marchbook never stores ${k}`);
  return {
    v: 1,
    host: String(BigInt(m.host)),
    transitSlot: m.transitSlot,
    departBell: m.departBell,
    arriveBell: m.arriveBell,
    holding: m.holding,
    plain_b64: toBase64(m.plain),
    salt_b64: toBase64(m.salt),
    commit_hex: toHex(m.commit),
    sealRoot_hex: toHex(m.sealRoot),
    ctHash_hex: toHex(m.ctHash),
    round: Number(m.round),
    tip: String(m.tip),
    state: 'sealed',
    revealDelay: Math.floor(random() * (REVEAL_DELAY_MAX + 1)),
    attempts: [],
  };
}

/**
 * Add a sealed march (before Depart is signed). A re-seal of the same march
 * (same host, slot and arrival bell) replaces an entry that never left this
 * browser; one already sent is kept, and the new one is refused.
 */
export function addSealed(book, entry) {
  const i = book.findIndex(e => same(e, entry));
  if (i >= 0 && book[i].state !== 'sealed' && book[i].state !== 'failed') throw new Error('this march was already sent');
  const out = i >= 0 ? book.map((e, j) => (j === i ? entry : e)) : [...book, entry];
  return out.length > MAX_ENTRIES ? prune(out) : out;
}

const at = (book, m) => book.findIndex(e => same(e, m));
/** A copy with the matching entry patched (unknown: unchanged). */
export function mark(book, m, patch) {
  const i = at(book, m);
  if (i < 0) return book;
  return book.map((e, j) => (j === i ? scrub({ ...e, ...patch }) : e));
}
/** Record a reveal attempt `{t, route, result}`; a self attempt moves the entry to `revealing`. */
export function recordAttempt(book, m, attempt) {
  const i = at(book, m);
  if (i < 0) return book;
  const e = book[i];
  const state = attempt.route === 'self' && (e.state === 'landed' || e.state === 'revealing') ? 'revealing' : e.state;
  return mark(book, m, { state, attempts: [...e.attempts, attempt] });
}

/**
 * What the self-reveal timer should do for one entry at chain time `now`:
 *   {action: 'send'} | {action: 'wait', at} | {action: 'none', reason}
 * `bellStart` is bell_start(arriveBell); `windowOpen` whether THE anchor's
 * window is still open (true before the anchor exists); `slotPresent`
 * whether the herald shows this march's ArrivalSlot.
 */
export function revealStep(entry, { now, bellStart, windowOpen = true, slotPresent = false }) {
  if (!entry) return { action: 'none', reason: 'noEntry' };
  if (['revealed', 'settled', 'failed'].includes(entry.state)) return { action: 'none', reason: entry.state };
  if (entry.state === 'sealed' || entry.state === 'sent') return { action: 'none', reason: 'notLanded' };
  if (slotPresent) return { action: 'none', reason: 'revealed' };
  if (!windowOpen) return { action: 'none', reason: 'windowClosed' };
  const self = entry.attempts.filter(a => a.route === 'self');
  if (self.length >= MAX_SELF_ATTEMPTS) return { action: 'none', reason: 'attempted' };
  const due = self.length === 0 ? bellStart + (entry.revealDelay ?? 0) : Math.max(bellStart, self[self.length - 1].t + RETRY_AFTER);
  if (now < bellStart || now < due) return { action: 'wait', at: due };
  return { action: 'send' };
}

/** The body of POST /gw/f/reveal for an entry (I-24): no signature, no seal key. */
export const revealMaterial = e => ({
  holding: e.holding,
  transit_slot: e.transitSlot,
  plain_b64: e.plain_b64,
  salt_b64: e.salt_b64,
  ct_hash_b64: toBase64(fromHex(e.ctHash_hex)),
});

/** The plaintext and salt bytes of an entry. */
export const materialBytes = e => ({ plain: fromBase64(e.plain_b64), salt: fromBase64(e.salt_b64) });

/**
 * Bring the book in line with the chain (the Holding's transit records
 * from the herald: `[{slot, state (0 free, 1–3), hostId, arriveBell,
 * sealRoot (hex)}]`, and the host ids whose ArrivalSlot exists):
 *  - a record in state 1–3 with this entry's host and seal root → landed
 *    (the Depart is on chain; a later state is kept);
 *  - the same slot with this host but **another seal root** → failed with
 *    `mismatch` (a relay or herald swapped the seal: never reveal it);
 *  - an entry that had landed and whose record is free again → settled;
 *  - an ArrivalSlot of the host → revealed.
 * `complete` says the transit list is the whole Holding (a missing record
 * then means settled).
 */
export function reconcile(book, { transits = [], revealedHosts = [], complete = false }) {
  const revealed = new Set(revealedHosts.map(h => String(h)));
  return book.map(e => {
    if (e.state === 'settled' || e.state === 'failed') return e;
    const r = transits.find(t => t.slot === e.transitSlot);
    const live = r && r.state >= 1 && r.state <= 3 && String(r.hostId) === e.host && r.arriveBell === e.arriveBell;
    if (live && String(r.sealRoot).toLowerCase() !== e.sealRoot_hex) return { ...e, state: 'failed', mismatch: true };
    if (live) {
      const landed = e.state === 'sealed' || e.state === 'sent' ? { ...e, state: 'landed' } : e;
      return revealed.has(e.host) && landed.state !== 'revealed' ? { ...landed, state: 'revealed' } : landed;
    }
    const wasOn = ['landed', 'revealing', 'revealed'].includes(e.state);
    if (wasOn && (complete || (r && r.state === 0))) return { ...e, state: 'settled' };
    return e;
  });
}

/** Drop the oldest finished entries past MAX_ENTRIES (entries in flight are never dropped). */
export function prune(book) {
  const done = e => e.state === 'settled' || e.state === 'failed';
  const out = [...book];
  while (out.length > MAX_ENTRIES) {
    const i = out.findIndex(done);
    if (i < 0) break;
    out.splice(i, 1);
  }
  return out;
}

/**
 * The tracker's line for a march (web design §8.4): with no entry on this
 * device (cleared storage, another device) nothing breaks — keepers reveal.
 */
export function trackerHint(entry) {
  if (!entry) return 'keepersWillReveal';
  if (entry.state === 'failed' && entry.mismatch) return 'sealMismatch';
  return entry.state;
}
