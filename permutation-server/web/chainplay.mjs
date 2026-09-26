// Chain mode: the member's own chain actions in play, signed in this browser
// with its session key (S.session) and sent through the gateway's public
// routes (chainio.mjs) — what any x402 agent does. No toasts here: callers
// (orders.mjs, drawers) say what happened.
//
//   orders      `/api/validate {member, orders, adopt}` checks the drafts
//               (orders.mjs); then per held office, in parallel: the decision
//               digest (policy officer@2 against the view's obsRoot), packBatch
//               with the office's pending rationale reveals, a fresh salt, the
//               commitment; the local copy first (sealbook.mjs); the seal
//               receipt signed → POST /seal (the gateway reveals the batch after
//               the deadline: the browser never signs RevealOrders); only then
//               CommitOrders → POST /relay (a fresh ER blockhash per
//               transaction). A refused seal sends no commitment.
//   governance  proposals, votes, candidacy, support, recalls → SubmitGov, up to
//               8 per transaction (one packet each), only in the tick's commit
//               phase; outside it they wait in S.govQueue.
//   talk        talkBytes signed → POST /talk.
// And what the dock shows: each office's state from the gateway's /tick flags
// and the local copies, restored after a reload.
import * as api from './api.mjs';
import * as chainio from './chainio.mjs';
import { S, held, invalidate } from './state.mjs';
import { commit as newDecision, DEFAULT_POLICY } from './sdk/decision.mjs';
import { packBatch } from './sdk/batch.mjs';
import { orderCommitment } from './sdk/codec.mjs';
import { commitOrdersIxs, sealMessage, submitGovIxs, SYSTEM_PROGRAM } from './sdk/player.mjs';
import { compileMessage, PACKET_BYTES } from './sdk/solana-tx.mjs';
import { fromHex, randomBytes, toHex } from './sdk/bytes.mjs';
import { MAX_TALK_CHARS, talkBytes } from './sdk/talk.mjs';
import * as book from './sealbook.mjs';

/** Shown when this browser has no key for the member (entered with 「鍵なしで見る」). */
export const NO_KEY = 'この端末にはゲーム内の鍵がないため、見るだけです（鍵のバックアップを読み込むと操作できます）。';
/** Codes that mean "the tick's commit phase just closed": governance waits for the next tick. */
export const LATE = new Set(['TickFrozen', 'WrongTick', 'WrongPhase']);

const fail = (code, error) => chainio.fail(code, error ?? code);

/** Chain mode, playing as a member (not watching). */
export const inChain = () => !!S.view?.chain && S.watch === null;
/** Whether this browser can sign for its member (it holds the session key). */
export const canSign = () => !!S.session && !!chainio.pinned() && S.memberId !== null;
/** The open tick's sealed-orders phase (commit | reveal | frozen | finished). */
export const chainPhase = (v = S.view) => v?.chainPhase ?? 'commit';

/** Why nothing can be signed now, or null. */
function notReady() {
  if (!chainio.pinned()) return fail('NoPin', 'シーズンがまだわかりません');
  if (!S.session) return fail('NoKey', NO_KEY);
  if (S.memberId === null) return fail('NotAMember');
  return null;
}

// ------------------------------------------------------------------ the book (localStorage)
let current = null, kept = null;
/** This member's book for the pinned season (loaded once per member), or null. */
export function playBook() {
  const p = chainio.pinned();
  if (!p || S.memberId === null) return null;
  const key = book.bookKey(p, S.memberId);
  if (key !== current) { current = key; kept = book.loadBook(book.browserStorage, key); }
  return kept;
}
const save = () => { if (kept) book.saveBook(book.browserStorage, current, kept); };

// ------------------------------------------------------------------ office batches
/**
 * Seal and commit one batch per office of `plans` ([{role, orders, adopt}],
 * the offices this member holds) for the open tick, every office at once,
 * with the rationale `rationale`. `tick`, when given, is the tick the plans
 * were checked for (nothing goes out once the view moved on). Every batch is
 * packed before any goes out (one that does not fit sends nothing).
 * Resolves one result per office: `{role, ok, digest, signature}` or
 * `{role, ok: false, stage ('seal' | 'commit' | null), code, error}`.
 */
export async function commitOffices(plans, { rationale = '', tick: checked } = {}) {
  const everyone = r => plans.map(p => ({ role: p.role, stage: null, ...r }));
  const bad = notReady();
  if (bad) return everyone(bad);
  const v = S.view, tick = v.tick, civ = S.myCiv, member = S.memberId;
  if (checked !== undefined && checked !== tick) return everyone(fail('WrongTick'));
  if (chainPhase(v) !== 'commit') return everyone(fail('WrongPhase'));
  const obsRoot = v.decision?.tick === tick ? v.decision.obsRoot : null;
  if (!obsRoot) return everyone(fail('NoObservation', '観測ルートがまだ届いていません。少し待ってからもう一度確定してください'));
  if (reconciling) await reconciling; // the reveals below must be the ones still pending
  const b = playBook();
  const packed = plans.map(p => ({
    ...p,
    adopt: [...(p.adopt ?? [])],
    decision: newDecision({ tick, obsRoot, policy: DEFAULT_POLICY, text: rationale }),
    batch: packBatch({ orders: p.orders, pending: book.pendingReveals(b, p.role, tick) }),
  }));
  const big = packed.find(p => !p.batch.fits);
  if (big) return everyone(fail('BatchTooLarge'));
  const capped = packed.find(p => book.commitCount(b, tick, p.role) >= book.MAX_COMMITS);
  if (capped) return everyone(fail('TooManyCommits'));
  return Promise.all(packed.map(p => sealAndCommit(b, p, { tick, civ, member, rationale })));
}

async function sealAndCommit(b, p, { tick, civ, member, rationale }) {
  const pin = chainio.pinned(), key = S.session;
  const salt = randomBytes(32);
  const batch = { civ, tick, role: p.role, member, decisionDigest: fromHex(p.decision.digest), orders: p.batch.orders, adopt: p.adopt };
  const commitment = orderCommitment(batch, salt);
  const hex = toHex(commitment);
  // The local copy first: an answer lost on the way must not lose the batch.
  book.addCandidate(b, {
    tick, role: p.role, civ, member, digest: p.decision.digest, commitment: hex, salt: toHex(salt), orders: p.batch.orders, adopt: p.adopt,
    drafts: p.orders, rationale, reveals: p.batch.reveals.map(d => d.tick), at: Date.now(),
  }, { tick, role: p.role, civ, policy: p.decision.policy, salt: p.decision.salt, text: p.decision.text, digest: p.decision.digest, obsRoot: p.decision.obsRoot });
  save();
  invalidate('dock', 'nextTurn');
  const failed = (stage, r) => {
    book.markCandidate(b, tick, p.role, hex, { state: 'failed', stage, code: r.code ?? null, error: r.error ?? null });
    save();
    invalidate('dock', 'nextTurn');
    return { role: p.role, ok: false, stage, code: r.code ?? null, error: r.error ?? null, httpStatus: r.httpStatus ?? 0 };
  };
  // 1. Hand the batch and its salt to the gateway, which reveals it after the deadline.
  let signature;
  try {
    signature = await key.sign(sealMessage({ seasonId: pin.seasonId, tick, civ, role: p.role, commitment }));
  } catch (e) {
    return failed('seal', fail('SignFailed', `署名できませんでした（${e?.message ?? e}）`));
  }
  const sealed = await chainio.seal({ civ, role: p.role, tick, member, digest: p.decision.digest, orders: p.batch.orders, adopt: p.adopt, salt: toHex(salt), signature: toHex(signature) });
  if (!sealed.ok) return failed('seal', sealed);
  if (sealed.commitment && sealed.commitment !== hex) return failed('seal', fail('CommitMismatch'));
  book.markCandidate(b, tick, p.role, hex, { state: 'sealed' });
  save();
  // 2. The commitment on chain, signed by the office's key.
  const r = await chainio.relay(commitOrdersIxs({ programId: pin.programId, seasonId: pin.seasonId, signer: key.publicKey, civ, role: p.role, tick, commitment }), key);
  if (!r.ok) return failed('commit', r);
  book.markCandidate(b, tick, p.role, hex, { state: 'committed', signature: r.signature ?? null });
  book.markSent(b, p.role, tick, p.batch.reveals.map(d => d.tick));
  save();
  invalidate('dock', 'nextTurn', 'drawer');
  return { role: p.role, ok: true, digest: p.decision.digest, signature: r.signature ?? null, reveals: p.batch.reveals.length };
}

/** Each held office's state in the open tick (sealbook.mjs officeState). */
export function officeStates() {
  const v = S.view;
  if (!v) return [];
  const b = playBook() ?? book.emptyBook();
  const flags = S.chainSeals && S.chainSeals.tick === v.tick ? S.chainSeals : null;
  return held().map(role => book.officeState(b, { tick: v.tick, role, flags }));
}
/** Every held office has a commitment out this tick (its turn is over until the deadline). */
export function turnEnded() {
  const s = officeStates();
  return s.length > 0 && s.every(x => x.committed);
}
/** What this browser sealed for the open tick (sealbook.mjs restoreFrom), or null. */
export function restore(v = S.view) {
  const b = playBook();
  return b ? book.restoreFrom(b, v.tick, held()) : null;
}

/**
 * This browser's own decisions (for the decision log): the open tick's
 * newest commitment per held office, and earlier ones that landed and wait
 * for their reveal — `{tick, civ, role, digest, obsRoot, text, open}`.
 */
export function ownDecisions() {
  const b = playBook(), v = S.view;
  if (!b || !v) return [];
  const open = held().map(role => book.lastCommitted(b, v.tick, role)).filter(Boolean)
    .map(c => ({ tick: c.tick, civ: c.civ, role: c.role, digest: c.digest, obsRoot: v.decision?.obsRoot ?? null, text: c.rationale ?? '', open: true }));
  const landed = b.decisions.filter(d => d.status === 'landed' && d.tick < v.tick)
    .map(d => ({ tick: d.tick, civ: d.civ, role: d.role, digest: d.digest, obsRoot: d.obsRoot, text: d.text, open: false }));
  return [...open, ...landed];
}

// ------------------------------------------------------------------ keeping in step with the chain
/** The gateway's per-office flags for my nation: S.chainSeals = {tick, phase, committed[4], revealed[4], at}. */
let flagsInFlight = null;
export function refreshFlags() {
  if (!inChain()) return Promise.resolve();
  flagsInFlight ??= chainio.tick().then(t => { if (t.ok) setFlags(t); }).finally(() => { flagsInFlight = null; });
  return flagsInFlight;
}
function setFlags(t) {
  const n = (t.nations || []).find(x => x.civ === S.myCiv);
  S.chainSeals = { tick: t.tick, phase: t.phase, committed: n?.committed ?? null, revealed: n?.revealed ?? null, at: Date.now() };
  invalidate('dock', 'nextTurn');
}

/** Decisions this many records back are checked against the play server's log (more on the first look). */
const DECISIONS = 240, DECISIONS_FIRST = 600;
let reconciling = null;
/** Match the decision records with what landed and was revealed (the play server's /api/decisions). */
export function reconcileDecisions(limit = DECISIONS) {
  const b = playBook();
  if (!b) return Promise.resolve();
  reconciling ??= api.tryGet(`/api/decisions?limit=${limit}`).then(d => {
    if (Array.isArray(d?.records) && Number.isInteger(d.open)) {
      book.reconcile(b, { records: d.records, open: d.open, civ: S.myCiv, complete: d.records.length < limit });
      save();
    }
  }).finally(() => { reconciling = null; });
  return reconciling;
}

let seenTick = null, seenPhase = null, firstLook = true;
/**
 * Every applied view (chain mode, as a member): on a new tick forget the
 * batches of resolved ticks and check the decisions; on a phase change (and
 * now and then) re-read the gateway's flags.
 */
export function onView(v) {
  if (!inChain() || S.memberId === null) return;
  const b = playBook();
  if (!b) return;
  if (v.tick !== seenTick) {
    seenTick = v.tick;
    book.pruneSealed(b, v.tick);
    save();
    reconcileDecisions(firstLook ? DECISIONS_FIRST : DECISIONS);
    firstLook = false;
  }
  const phase = `${v.tick}:${chainPhase(v)}`;
  const stale = !S.chainSeals || Date.now() - S.chainSeals.at > 10_000;
  if (phase !== seenPhase || (stale && held().length && chainPhase(v) === 'commit')) {
    seenPhase = phase;
    refreshFlags();
  }
}

// ------------------------------------------------------------------ governance
// A placeholder key for measuring a transaction (a real key of the same size).
const SIZE_KEY = SYSTEM_PROGRAM;
function govIxs(actions) {
  const pin = chainio.pinned();
  return submitGovIxs({ programId: pin.programId, seasonId: pin.seasonId, signer: S.session.publicKey, civ: S.myCiv, member: S.memberId, actions });
}
/** Bytes of the signed transaction for `actions` (fee payer and session key: two signatures). */
function govSize(actions) {
  try {
    return 1 + 2 * 64 + compileMessage({ feePayer: SIZE_KEY, recentBlockhash: SIZE_KEY, instructions: govIxs(actions) }).length;
  } catch {
    return Infinity; // cannot be encoded: never fits
  }
}
/** Whether one governance action fits a transaction on its own. */
export const govFits = action => canSign() && govSize([action]) <= PACKET_BYTES;

/**
 * Proposal actions for drafts of offices this member does not hold:
 * `byRole` {role: [order]} → `{actions, tooLarge}` (sealbook.mjs proposals).
 */
export function proposalActions(byRole) {
  const actions = [], tooLarge = [];
  for (const [role, orders] of Object.entries(byRole)) {
    const r = book.proposals(role, orders, govFits);
    actions.push(...r.actions);
    tooLarge.push(...r.tooLarge);
  }
  return { actions, tooLarge };
}

/**
 * Send governance actions now, in order: SubmitGov transactions of up to 8
 * actions that fit one packet, signed by the session key. Resolves `{ok,
 * sent: [action], failed: [{actions, code, error}], tooLarge: [action]}`.
 */
export async function sendGov(actions) {
  const bad = notReady();
  if (bad) return { ...bad, sent: [], failed: [{ actions, code: bad.code, error: bad.error }], tooLarge: [] };
  const { chunks, tooLarge } = book.packGov(actions, { sizeOf: govSize });
  const sent = [], failed = [];
  // One after another: a vote after a candidacy must land after it.
  for (const chunk of chunks) {
    const r = await chainio.relay(govIxs(chunk), S.session);
    if (r.ok) sent.push(...chunk);
    else failed.push({ actions: chunk, code: r.code, error: r.error, httpStatus: r.httpStatus });
  }
  return { ok: !failed.length && !tooLarge.length, sent, failed, tooLarge };
}

/** Keep governance actions for the next commit phase. */
export function queueGov(...actions) { S.govQueue.push(...actions); }

/**
 * Send what waits in the queue together with `actions`; actions refused
 * because the tick's commitments just closed go back to the queue
 * (`requeued`). Resolves sendGov's answer plus `requeued`.
 */
export async function submitGov(actions = []) {
  const all = [...S.govQueue.splice(0), ...actions];
  if (!all.length) return { ok: true, sent: [], failed: [], tooLarge: [], requeued: [] };
  const r = await sendGov(all);
  const late = r.failed.filter(f => LATE.has(f.code));
  const requeued = late.flatMap(f => f.actions);
  S.govQueue.push(...requeued);
  return { ...r, failed: r.failed.filter(f => !LATE.has(f.code)), requeued };
}

// ------------------------------------------------------------------ talk
/**
 * A public message (V5 §18.7): talkBytes for the open tick signed by the
 * session key → POST /talk. `to` is null (everyone), {civ} or {member}.
 */
export async function sendTalk({ to = null, text }) {
  const bad = notReady();
  if (bad) return bad;
  if (!text || [...text].length > MAX_TALK_CHARS) return fail('TalkRefused', `メッセージは1〜${MAX_TALK_CHARS}字です`);
  const pin = chainio.pinned();
  const t = await chainio.tick();
  if (t.ok) setFlags(t);
  const tick = t.ok && Number.isInteger(t.tick) ? t.tick : S.view.tick;
  let signature;
  try {
    signature = await S.session.sign(talkBytes({ season: BigInt(pin.seasonId), tick, member: S.memberId, to, text }));
  } catch (e) {
    return fail('SignFailed', `署名できませんでした（${e?.message ?? e}）`);
  }
  return chainio.talk({ member: S.memberId, to, text, tick, signature: toHex(signature) });
}
