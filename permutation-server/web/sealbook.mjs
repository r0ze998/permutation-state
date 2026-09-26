// Chain mode: this browser's own record of its play, and the pure rules the
// in-game chain actions follow (chainplay.mjs sends them).
//
// The play server keeps nothing for a member that signs its own batches, so
// the member's sealed batches of the open tick and its decision records
// (the salts of its rationales, V5 D17) live here, in localStorage per
// season and member. A batch is written before its seal or its commit goes
// out (a lost answer must not lose it); a decision record waits until the
// play server shows that exact digest landed, is revealed in a later batch
// of the same office, and is dropped once the reveal is on record (or when
// another batch landed instead). Everything here is pure apart from the
// storage handed in (tests: permutation-gateway/test/web-sealbook.test.mjs).
import { ROLES } from './sdk/codec.mjs';
import { PACKET_BYTES } from './sdk/solana-tx.mjs';

/** CommitOrders one office key may relay per tick (the gateway's cap: TooManyCommits). */
export const MAX_COMMITS = 8;
/** Governance actions per transaction (SubmitGov × n, as the operator's AI members send theirs). */
export const MAX_GOV_PER_TX = 8;
/** Orders per proposal (V5 §5.4). */
export const PROPOSAL_ORDERS = 4;
/** Decision records whose fate is unknown are kept this many ticks, then dropped. */
export const KEEP_TICKS = 40;

/** Seconds before the commit deadline at which chain mode seals a dirty draft: max(8, tick/3). */
export const autoCommitSeconds = tickSeconds => Math.max(8, (Number(tickSeconds) || 0) / 3);
/** Auto-commit waits until the drafts (and proposals adopted) have been unchanged this long (ms). */
export const AUTO_QUIET_MS = 2000;
/** After a failure that may pass, auto-commit tries again this much later (ms). */
export const AUTO_RETRY_MS = 2500;
/** Auto-commit tries again only with more than this many seconds left before the deadline. */
export const AUTO_RETRY_MIN_SECONDS = 2;
/** Auto-commit attempts per draft state: the first and its retries. */
export const AUTO_ATTEMPTS = 3;

// ------------------------------------------------------------------ storage
const memory = new Map();
/** localStorage when it works, with a copy in memory for this page's life. */
export const browserStorage = {
  get(k) {
    try { const v = globalThis.localStorage?.getItem(k); if (v != null) return v; } catch { /* unavailable */ }
    return memory.get(k) ?? null;
  },
  /** Returns whether the value reached localStorage (survives a reload). */
  set(k, v) {
    memory.set(k, v);
    try { globalThis.localStorage.setItem(k, v); return true; } catch { return false; }
  },
};

/** Where one member's book of one season is kept. */
export const bookKey = ({ cluster, programId, seasonId }, member) => `ps-play:${cluster}:${programId}:${seasonId}:${member}`;

/**
 * A book: `{v, sealed: {"tick:role": [candidate, …newest first]}, decisions: [record]}`.
 * candidate: {tick, role, civ, member, digest, commitment, salt, orders (the batch, reveals
 * included), adopt, drafts (the member's own orders), rationale, reveals (ticks), state:
 * 'sealing' | 'sealed' | 'committed' | 'failed', stage?, code?, error?, signature?, at}.
 * record: {tick, role, civ, policy, salt (16 bytes hex), text, digest, obsRoot, status:
 * 'sealed' (sent, fate unknown) | 'landed' (the play server shows this digest), sentIn?}.
 */
export const emptyBook = () => ({ v: 1, sealed: {}, decisions: [] });

export function loadBook(storage, key) {
  try {
    const b = JSON.parse(storage.get(key) ?? 'null');
    if (b && b.v === 1 && b.sealed && typeof b.sealed === 'object' && !Array.isArray(b.sealed) && Array.isArray(b.decisions)) return b;
  } catch { /* damaged: start again */ }
  return emptyBook();
}
export const saveBook = (storage, key, book) => storage.set(key, JSON.stringify(book));

// ------------------------------------------------------------------ sealed batches
const slot = (tick, role) => `${tick}:${role}`;
const slotTick = k => Number(k.slice(0, k.indexOf(':')));

/** The batches sealed for one office in one tick, newest first. */
export const candidates = (book, tick, role) => book.sealed[slot(tick, role)] ?? [];
/** How many times this office sealed in this tick (the gateway allows MAX_COMMITS). */
export const commitCount = (book, tick, role) => candidates(book, tick, role).length;
/** The newest batch of an office that went out as its commitment this tick, or null. */
export const lastCommitted = (book, tick, role) => candidates(book, tick, role).find(c => c.state === 'committed') ?? null;

/** Keep a batch about to be sealed (state 'sealing'), with the decision record it commits to. */
export function addCandidate(book, candidate, decision) {
  const k = slot(candidate.tick, candidate.role);
  const c = { ...candidate, state: 'sealing' };
  book.sealed[k] = [c, ...(book.sealed[k] ?? [])];
  book.decisions.push({ ...decision, status: 'sealed' });
  return c;
}

/** Update one batch (found by its commitment); returns it, or null. */
export function markCandidate(book, tick, role, commitment, patch) {
  const c = candidates(book, tick, role).find(x => x.commitment === commitment);
  if (c) Object.assign(c, patch);
  return c ?? null;
}

/** Forget the batches of ticks before `open` (they resolved; the gateway revealed what was on chain). */
export function pruneSealed(book, open) {
  for (const k of Object.keys(book.sealed)) if (slotTick(k) < open) delete book.sealed[k];
  return book;
}

// ------------------------------------------------------------------ decision records
/**
 * Earlier decisions of `role` to reveal in a batch of tick `tick`: those the
 * play server shows landed, not yet revealed, and not already riding in a
 * batch of an earlier tick whose fate is unknown ({tick, policy, salt, text},
 * as batch.mjs packBatch takes them).
 */
export function pendingReveals(book, role, tick) {
  return book.decisions
    .filter(d => d.role === role && d.status === 'landed' && d.tick < tick && !(d.sentIn !== undefined && d.sentIn < tick))
    .sort((a, b) => a.tick - b.tick)
    .map(({ tick: t, policy, salt, text }) => ({ tick: t, policy, salt, text }));
}

/** The reveals of `ticks` (of `role`) went out in a committed batch of tick `tick`. */
export function markSent(book, role, tick, ticks) {
  for (const d of book.decisions) if (d.role === role && d.status === 'landed' && ticks.includes(d.tick)) d.sentIn = tick;
  return book;
}

/**
 * Bring the decision records in line with the play server's `/api/decisions`
 * (`records` of resolved ticks, newest first; `open` its open tick; `civ`
 * mine; `complete` when the list was not cut at its limit). A record whose
 * office landed another digest, or none, goes; one that was revealed goes; one
 * whose digest landed becomes `landed` (and, if the batch that carried its
 * reveal did not reveal it, pending again). Records of ticks the list does not
 * reach are kept for KEEP_TICKS.
 */
export function reconcile(book, { records = [], open, civ, complete = false }) {
  const mine = new Map(records.filter(r => r.civ === civ).map(r => [slot(r.tick, r.role), r]));
  const oldest = records.reduce((m, r) => Math.min(m, r.tick), Infinity);
  book.decisions = book.decisions.filter(d => {
    if (d.tick >= open) return true; // not resolved yet
    const r = mine.get(slot(d.tick, d.role));
    if (r) {
      if (r.digest !== d.digest || r.reveal) return false;
      d.status = 'landed';
      if (d.sentIn !== undefined && d.sentIn < open) delete d.sentIn;
      return true;
    }
    const covered = records.length > 0 && d.tick >= oldest && (d.tick > oldest || complete);
    if (covered) return false; // no batch of that office landed that tick
    return d.tick + KEEP_TICKS >= open;
  });
  return book;
}

// ------------------------------------------------------------------ what the dock shows
/**
 * One office's state in the open tick, from the local batches and the
 * gateway's /tick flags for my nation (`flags`: {tick, committed[4],
 * revealed[4]}, or null when unknown): {role, sending, committed (a
 * commitment is out: this browser's, or one the chain shows), onChain (true
 * / false / null unknown), revealed, digest (of this browser's commitment),
 * failed (the last attempt, when nothing went out), count}.
 */
export function officeState(book, { tick, role, flags = null }) {
  const list = candidates(book, tick, role);
  const out = list.find(c => c.state === 'committed') ?? null;
  const i = ROLES.indexOf(role);
  const known = flags && flags.tick === tick && Array.isArray(flags.committed);
  const onChain = known ? !!flags.committed[i] : null;
  const revealed = known && Array.isArray(flags.revealed) ? !!flags.revealed[i] : null;
  const last = list[0] ?? null;
  return {
    role,
    sending: !!last && (last.state === 'sealing' || last.state === 'sealed'),
    committed: !!out || onChain === true,
    onChain,
    revealed,
    digest: out?.digest ?? null,
    failed: !out && last?.state === 'failed' ? { stage: last.stage, code: last.code, error: last.error } : null,
    count: list.length,
  };
}

/**
 * What this browser sealed for the open tick, to show again after a reload:
 * `{orders, adopt: {role: ids}, rationale}` from each held office's newest
 * committed batch, or null when none went out.
 */
export function restoreFrom(book, tick, held) {
  const out = held.map(r => lastCommitted(book, tick, r)).filter(Boolean);
  if (!out.length) return null;
  const adopt = {};
  for (const c of out) if (c.adopt?.length) adopt[c.role] = [...c.adopt];
  return { orders: out.flatMap(c => c.drafts ?? []), adopt, rationale: out[0].rationale ?? '' };
}

// ------------------------------------------------------------------ auto-commit (chain mode)
// Codes of failures that may pass on their own: no answer, a busy gateway
// (per-IP or per-key rate limit), a blockhash that expired on the way, an
// unavailable gateway, and an observation root that has not arrived yet.
const PASSING = new Set(['network', 'RateLimited', 'BlockhashExpired', 'Unavailable', 'NoObservation']);

/**
 * Whether a failure (a chainio/chainplay result, or the play server's
 * `{ok: false, error: 'network'}`) may pass if the same thing is sent again
 * a little later: the codes above or an HTTP 5xx. Everything else (a refused
 * order, TooManyCommits, a closed tick…) would fail the same way again.
 */
export function retryable(f) {
  if (!f || typeof f !== 'object') return false;
  const code = String(f.code ?? f.error ?? '');
  const status = Number(f.httpStatus ?? 0);
  return PASSING.has(code) || /^HTTP5\d\d$/.test(code) || (status >= 500 && status < 600);
}

/**
 * Chain mode's auto-commit, asked on every applied view. `a` is its memory
 * (mutated; start with {}), about one draft state `key`: the tick, the
 * drafts and the proposals adopted, but not the rationale, so typing a memo
 * seals nothing again. Fires when the draft is dirty, nothing is being sent
 * (`busy`), the tick takes commitments (`phase`), fewer than `window`
 * seconds are left, no held office has used its MAX_COMMITS (`counts`), and
 * the key has not changed for AUTO_QUIET_MS (unless the deadline is too
 * close to wait). Each key goes out once; a failure that may pass is tried
 * again after AUTO_RETRY_MS while more than AUTO_RETRY_MIN_SECONDS are left
 * (up to AUTO_ATTEMPTS in all). Returns `{fire, report}`: `report` is the
 * errors of a retry that can no longer happen (the tick closed), to show.
 */
export function autoCommitStep(a, { key, now, dirty, busy = false, phase, secondsLeft, window, counts = [] }) {
  if (a.key !== key) Object.assign(a, { key, since: now, attempts: 0, inFlight: false, settled: false, retryAt: 0, errors: null });
  const capped = counts.some(n => n >= MAX_COMMITS);
  if (a.inFlight || a.settled) return { fire: false, report: null };
  if (a.attempts > 0) { // a retry is waiting
    if (!dirty) { a.settled = true; return { fire: false, report: null }; } // sent by hand meanwhile
    if (phase !== 'commit' || !(secondsLeft > AUTO_RETRY_MIN_SECONDS) || capped) { a.settled = true; return { fire: false, report: a.errors ?? [] }; }
    return { fire: !busy && now >= a.retryAt, report: null };
  }
  const quiet = now - a.since >= AUTO_QUIET_MS || secondsLeft <= AUTO_QUIET_MS / 1000 + 1;
  return { fire: !!dirty && !busy && phase === 'commit' && secondsLeft < window && !capped && quiet, report: null };
}

/** An auto-commit of `a.key` goes out. */
export function autoCommitStarted(a) {
  a.inFlight = true;
  a.attempts = (a.attempts ?? 0) + 1;
}

/**
 * What became of the auto-commit of `key` (`outcome`: `{ok, retry, errors}`
 * from the commit): 'done', 'retry' (tried again after AUTO_RETRY_MS; the
 * errors are kept to show if that never happens), 'failed' (settled: this
 * draft state is not sent again automatically), or 'stale' (the draft
 * changed meanwhile; the new one has its own attempts).
 */
export function autoCommitSettled(a, key, outcome, now) {
  if (a.key !== key || !a.inFlight) return 'stale';
  a.inFlight = false;
  if (outcome?.ok) { a.settled = true; return 'done'; }
  if (outcome?.retry && a.attempts < AUTO_ATTEMPTS) {
    a.retryAt = now + AUTO_RETRY_MS;
    a.errors = [...(outcome.errors ?? [])];
    return 'retry';
  }
  a.settled = true;
  return 'failed';
}

// ------------------------------------------------------------------ checking the play server's split
/**
 * JSON with object keys sorted and keys whose value is null or undefined
 * left out: an order echoed by the server may list its keys in another
 * order, and may write an optional field the member left out as null (an
 * open OfferContract's `to`) or leave out one sent as null. The codec
 * encodes a missing field and a null one alike (None), so both mean the same
 * order; what gets sealed is always the object sent.
 */
export function canonical(x) {
  if (Array.isArray(x)) return `[${x.map(canonical).join(',')}]`;
  if (x && typeof x === 'object') return `{${Object.keys(x).filter(k => x[k] != null).sort().map(k => `${JSON.stringify(k)}:${canonical(x[k])}`).join(',')}}`;
  return JSON.stringify(x) ?? 'null';
}

/**
 * The play server's `/api/validate {member, orders, adopt}` answer, checked
 * against what was sent: its per-office split may only regroup `orders`
 * (each exactly once, none added) and must echo `adopt` for each office.
 * Returns `{problem}` or `{ok, offices: [{role, orders (the objects sent),
 * adopt, cost, spendable, error}], refused: [{order, error}], warnings}`, so
 * what gets sealed is always the member's own drafts.
 */
export function checkValidation(res, { orders, adopt = {}, tick }) {
  if (!res || typeof res !== 'object' || !Array.isArray(res.offices)) return { problem: 'answer' };
  if (res.tick !== tick) return { problem: 'tick' };
  const pool = new Map();
  for (const o of orders) {
    const k = canonical(o);
    if (!pool.has(k)) pool.set(k, []);
    pool.get(k).push(o);
  }
  const take = o => pool.get(canonical(o))?.shift() ?? null;
  const ids = list => canonical([...(list ?? [])].map(Number).sort((a, b) => a - b));
  const offices = [];
  for (const off of res.offices) {
    if (!ROLES.includes(off?.role)) return { problem: 'office' };
    const mine = [];
    for (const o of off.orders ?? []) {
      const x = take(o);
      if (!x) return { problem: 'orders' };
      mine.push(x);
    }
    if (ids(off.adopt) !== ids(adopt[off.role])) return { problem: 'adopt' };
    offices.push({ role: off.role, orders: mine, adopt: [...(adopt[off.role] ?? [])], cost: off.cost ?? null, spendable: off.spendable ?? null, error: off.error ?? null });
  }
  const refused = [];
  for (const r of res.refused ?? []) {
    const x = take(r?.order);
    if (!x) return { problem: 'refused' };
    refused.push({ order: x, error: r.error ?? '' });
  }
  if ([...pool.values()].some(l => l.length)) return { problem: 'missing' };
  return { ok: res.ok === true && offices.every(o => !o.error), offices, refused, warnings: res.warnings ?? [] };
}

// ------------------------------------------------------------------ governance transactions
/**
 * Proposals of orders to one office (V5 §5.4): PROPOSAL_ORDERS orders each,
 * halved until each fits one transaction (`fits(action)`); a single order
 * that does not fit alone comes back in `tooLarge`.
 */
export function proposals(role, orders, fits, per = PROPOSAL_ORDERS) {
  const actions = [], tooLarge = [];
  const push = list => {
    const a = { type: 'Propose', role, orders: list };
    if (fits(a)) actions.push(a);
    else if (list.length > 1) { const h = Math.ceil(list.length / 2); push(list.slice(0, h)); push(list.slice(h)); }
    else tooLarge.push(a);
  };
  for (let i = 0; i < orders.length; i += per) push(orders.slice(i, i + per));
  return { actions, tooLarge };
}

/**
 * Governance actions into transactions, in order: at most `max` each and
 * each within `limit` bytes as `sizeOf(actions)` measures the signed
 * transaction; an action too large alone comes back in `tooLarge`.
 */
export function packGov(actions, { sizeOf, max = MAX_GOV_PER_TX, limit = PACKET_BYTES }) {
  const chunks = [], tooLarge = [];
  let cur = [];
  for (const a of actions) {
    if (sizeOf([a]) > limit) { tooLarge.push(a); continue; }
    if (cur.length && (cur.length >= max || sizeOf([...cur, a]) > limit)) { chunks.push(cur); cur = []; }
    cur.push(a);
  }
  if (cur.length) chunks.push(cur);
  return { chunks, tooLarge };
}
