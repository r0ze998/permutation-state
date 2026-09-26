// Resolving a tick and archiving it, shared by the crank, the base-layer
// regression script and the tick reindexer:
//
// * the split-tick plan: a tick that does not fit one transaction (compute
//   budget, or the program's heap: its bump allocator never frees) is
//   resolved in parts — `ResolveTick { to }` runs the phases up to `to` and
//   the engine resumes at its phase cursor;
// * publishing a tick's input (`LogTickInput`, PS_INPUT chunks) and
//   reassembling it against the hash the program logged;
// * the tick index line (.local/ticks/<season>.jsonl), one per resolve part.
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, readFileSync } from 'node:fs';
import { toHex as hex } from '../client/src/bytes.mjs';
import { isHeavyError } from '../client/src/retry.mjs';

/**
 * Phase boundaries a tick may be split at (`LAST_PHASE` = the whole tick):
 * every one, so any single phase can run in a transaction of its own.
 */
export const STOPS = Object.freeze([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
export const LAST_PHASE = 12;

/**
 * Stops to try from phase `cursor`, furthest first: every stop past the
 * cursor up to the furthest that worked from there before (`reach[cursor]`,
 * or the whole tick if none did).
 */
export function nextStops(cursor, reach = {}) {
  return STOPS.filter(t => t > cursor && t <= (reach[cursor] ?? LAST_PHASE)).reverse();
}

/**
 * Resolve the open tick in as few parts as fit: from each cursor try the
 * furthest stop, then shorter ones while the failure is "too heavy".
 * `resolve(to)` sends one `ResolveTick { to }` and returns its result;
 * `onPart(result, to)` archives it. `reach` (cursor → stop) is updated in
 * place, so the next tick starts from what worked.
 * @returns the results, in order
 */
export async function resolveInParts({ resolve, reach = {}, onPart }) {
  const parts = [];
  for (let cursor = 0; cursor < LAST_PHASE;) {
    const stops = nextStops(cursor, reach);
    let landed = null;
    for (const to of stops) {
      try {
        landed = await resolve(to);
      } catch (e) {
        if (!isHeavyError(e) || to === stops.at(-1)) throw e;
        continue;
      }
      await onPart?.(landed, to);
      parts.push(landed);
      reach[cursor] = to;
      cursor = to;
      break;
    }
    if (!landed) throw new Error(`no part from phase ${cursor} fits one transaction`);
  }
  return parts;
}

/**
 * Reassemble a tick's input from its PS_INPUT records ({tick, chunk, total,
 * hash, bytes}, one per chunk, in any order) and check it against the hash
 * the program logged. `signatures[i]` is the transaction of `parts[i]`.
 * @returns {{tick, input: string, hash: string, signatures: string[]}}
 */
export function assembleInput(parts, signatures = []) {
  if (!parts.length) throw new Error('no PS_INPUT records');
  const { tick, total } = parts[0];
  const hash = hex(parts[0].hash);
  const chunks = new Array(total);
  const sigs = new Array(total);
  parts.forEach((p, i) => {
    if (p.tick !== tick || p.total !== total || hex(p.hash) !== hash) throw new Error(`tick ${tick}: PS_INPUT records of different inputs (tick ${p.tick}, chunk ${p.chunk})`);
    if (p.chunk >= total) throw new Error(`tick ${tick}: PS_INPUT chunk ${p.chunk} of ${total}`);
    chunks[p.chunk] ??= p.bytes;
    sigs[p.chunk] ??= signatures[i];
  });
  const missing = [...chunks.keys()].filter(k => chunks[k] === undefined);
  if (missing.length) throw new Error(`tick ${tick}: input chunks ${missing.join(',')} of ${total} not published`);
  const bytes = Buffer.concat(chunks.map(c => Buffer.from(c)));
  if (createHash('sha256').update(bytes).digest('hex') !== hash) throw new Error(`tick ${tick}: the reassembled input does not match its hash`);
  return { tick, input: bytes.toString('hex'), hash, signatures: sigs.filter(Boolean) };
}

/**
 * Publish every chunk of the open tick's input (chunk 0 freezes it;
 * re-logging a chunk is allowed and harmless). `publishChunk(chunk)` sends
 * one `LogTickInput` and returns its result (with `records`).
 */
export async function publishTickInput({ tick, publishChunk }) {
  const parts = [];
  const signatures = [];
  for (let chunk = 0, total = 1; chunk < total; chunk++) {
    const r = await publishChunk(chunk, total);
    const rec = r.records.find(x => x.tag === 'PS_INPUT');
    if (!rec) throw new Error(`publish ${r.signature}: landed, but its PS_INPUT record could not be read; run scripts/reindex-ticks.mjs`);
    if (rec.tick !== tick || rec.chunk !== chunk) throw new Error(`publish tick ${tick}: unexpected PS_INPUT (tick ${rec.tick}, chunk ${rec.chunk})`);
    total = rec.total;
    parts.push(rec);
    signatures.push(r.signature);
  }
  return assembleInput(parts, signatures);
}

/**
 * One line of the tick index for a resolve part (a PS_TICK record) and the
 * input it resolved. `seals` = {committed, revealed, commitSignature}: the
 * offices that sealed orders, how many revealed them (`submitted`, kept for
 * older readers), and the `CloseCommits` transaction whose PS_COMMITS record
 * the verifier checks the reveals against (nulls when unknown, e.g. rebuilt
 * from the chain, or closed by someone else). `to` is the stop that ran:
 * the program logs the raw byte of `ResolveTick { to }`, and a v8 record
 * runs `min(to, 12)` phases.
 */
export function tickLine({ rec, published, signature, cu, seals = {} }) {
  const { committed = null, revealed = null, commitSignature = null } = seals;
  return {
    tick: rec.tick, to: Math.min(rec.to, LAST_PHASE), preRoot: hex(rec.preRoot), root: hex(rec.root),
    input: published.input, inputHash: published.hash, inputSignatures: published.signatures,
    signature, cu, submitted: revealed, committed, revealed, commitSignature,
  };
}

/** The index lines of one landed resolve transaction; a missing record, or one for another input, is an error. */
export function tickLinesOf(result, published, seals) {
  const recs = result.records.filter(x => x.tag === 'PS_TICK');
  if (!recs.length) {
    throw new Error(`resolve ${result.signature}: landed, but its PS_TICK record could not be read (${result.fetched ? 'no record in the logs' : 'transaction not fetchable'}); run scripts/reindex-ticks.mjs`);
  }
  return recs.map(rec => {
    if (hex(rec.inputHash) !== published.hash) throw new Error(`resolve tick ${rec.tick}: the program resolved another input than the one published`);
    return tickLine({ rec, published, signature: result.signature, cu: result.cu, seals });
  });
}

/** Read an index file (lines with `tick >= from`). */
export function readTickLines(file, from = 0) {
  if (!existsSync(file)) return [];
  return readFileSync(file, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l)).filter(r => r.tick >= from);
}

export const appendTickLines = (file, lines) => { if (lines.length) appendFileSync(file, lines.map(l => JSON.stringify(l) + '\n').join('')); };
export const formatTickLines = lines => lines.map(l => JSON.stringify(l)).join('\n') + '\n';
