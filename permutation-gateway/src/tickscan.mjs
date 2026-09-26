// Reading the tick index back from the chain, shared by the crank's
// background indexer and scripts/reindex-ticks.mjs. The rules are the
// verifier's (permutation-server `replay.rs`):
//
// * a record counts only if the season program itself logged it (the
//   innermost invocation), and a log line is a frame line only if its
//   program id has no `:` (a program can print `Program data:` lines, but
//   no fake invoke/success lines);
// * each record carries the accounts of the instruction that logged it, so
//   a `PS_COMMITS` (which names no season) is taken only from an
//   instruction on this season's world chunk 0;
// * tick lines follow the chain of state roots: one line per advancing
//   `PS_TICK` (no-op records, `preRoot === root`, are left out), wherever
//   it sits in a transaction.
import { PublicKey } from '@solana/web3.js';
import { toHex as hex } from '../client/src/bytes.mjs';
import { parseRecord, RECORD_TAGS } from '../client/src/codec.mjs';
import { assembleInput, tickLine } from './ticks.mjs';

const TX_OPTS = { commitment: 'confirmed', maxSupportedTransactionVersion: 0 };
const decode = f => new Uint8Array(Buffer.from(f, 'base64'));

/**
 * The `Program data:` lines of `logs`, each with the instruction that
 * logged it: `top` (top-level index), `inner` (index among that
 * instruction's inner instructions, null at top level) and `truncated`
 * (the logs were cut before it). With `program`, only lines logged while
 * `program` is the innermost invocation; without, every line.
 */
export function programLog(logs, program = null) {
  const stack = []; // [{id, inner}]
  let top = -1, inner = 0, truncated = false;
  const out = [];
  for (const l of logs) {
    if (l === 'Log truncated') { truncated = true; continue; }
    const m = /^Program (\S+) (invoke \[(\d+)\]|success|failed:)/.exec(l);
    if (m && !m[1].includes(':')) {
      if (m[2].startsWith('invoke')) {
        const depth = Number(m[3]);
        if (depth <= 1) { top++; inner = 0; stack.length = 0; stack.push({ id: m[1], inner: null }); }
        else { stack.length = Math.min(stack.length, depth - 1); stack.push({ id: m[1], inner: inner++ }); }
      } else if (stack.at(-1)?.id === m[1]) stack.pop();
      continue;
    }
    if (!l.startsWith('Program data: ')) continue;
    const frame = stack.at(-1);
    if (program && frame?.id !== program) continue;
    out.push({ fields: l.slice('Program data: '.length).split(' ').map(decode), top: Math.max(top, 0), inner: frame?.inner ?? null, truncated });
  }
  return out;
}

const keyText = k => (typeof k === 'string' ? k : k?.pubkey ? keyText(k.pubkey) : k?.toBase58?.() ?? String(k));

/** A transaction's account keys (a v0 transaction's loaded addresses appended). */
function accountKeys(tx) {
  const m = tx?.transaction?.message ?? {};
  const la = tx?.meta?.loadedAddresses;
  return [...(m.staticAccountKeys ?? m.accountKeys ?? []), ...(la?.writable ?? []), ...(la?.readonly ?? [])].map(keyText);
}

/** An instruction as {programIdIndex, accounts} (raw JSON or web3.js message). */
const ixOf = ix => ix && { programIdIndex: ix.programIdIndex, accounts: ix.accountKeyIndexes ?? ix.accounts ?? [] };

/**
 * The program's records in a getTransaction result, each with the account
 * keys of the instruction that logged it (`accounts`, null when not
 * attributable: another program's instruction, no innerInstructions for a
 * CPI, or cut logs). Same rules as the Rust `tx_records`. A failed
 * transaction has none.
 */
export function txRecords(tx, program) {
  if (!tx || tx.meta?.err) return [];
  const keys = accountKeys(tx);
  const m = tx.transaction?.message ?? {};
  const tops = m.compiledInstructions ?? m.instructions ?? [];
  const out = [];
  for (const { fields, top, inner, truncated } of programLog(tx.meta?.logMessages ?? [], keyText(program))) {
    let ix = null;
    if (!truncated) {
      ix = inner === null ? ixOf(tops[top]) : ixOf(tx.meta?.innerInstructions?.find(e => e.index === top)?.instructions?.[inner]);
    }
    const accounts = ix && keys[ix.programIdIndex] === keyText(program) ? ix.accounts.map(i => keys[i]) : null;
    let record;
    try { record = parseRecord(fields); } catch { continue; }
    if (RECORD_TAGS.includes(record.tag)) out.push({ record, accounts: accounts?.some(a => a === undefined) ? null : accounts });
  }
  return out;
}

/** `f` over `items`, at most `n` at a time, results in order. */
async function pool(items, n, f) {
  const out = new Array(items.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, async () => {
    while (next < items.length) {
      const k = next++;
      out[k] = await f(items[k]);
    }
  }));
  return out;
}

/**
 * Successful transactions listing `address` in slots lo..=hi (hi null: up
 * to the newest), listed newest first from `start` (a signature; null: the
 * newest), with their records: at most `maxPages` pages of 1000 (null:
 * unbounded), getTransaction `concurrency` at a time. `complete` is false
 * when the page limit stopped the listing or a transaction could not be
 * read. `txs` come oldest first.
 * @returns {Promise<{txs: {signature, slot, cu, emitted}[], complete: boolean}>}
 */
export async function scanRecords({ connection, address, program, lo = 0, hi = null, start = null, maxPages = null, concurrency = 4 }) {
  const listed = [];
  let before = start ?? undefined;
  let complete = true;
  for (let pages = 0; ; ) {
    const page = await connection.getSignaturesForAddress(typeof address === 'string' ? new PublicKey(address) : address, { before, limit: 1000 }, 'confirmed');
    pages++;
    let below = false;
    for (const s of page) {
      if (s.slot < lo) { below = true; break; }
      if ((hi !== null && s.slot > hi) || s.err) continue;
      listed.push(s);
    }
    if (below || page.length < 1000) break;
    if (maxPages !== null && pages >= maxPages) { complete = false; break; }
    before = page.at(-1).signature;
  }
  const txs = await pool(listed, concurrency, async s => {
    try {
      const t = await connection.getTransaction(s.signature, TX_OPTS);
      if (!t) { complete = false; return null; }
      return { signature: s.signature, slot: t.slot ?? s.slot, cu: t.meta?.computeUnitsConsumed ?? null, emitted: txRecords(t, program) };
    } catch {
      complete = false;
      return null;
    }
  });
  return { txs: txs.filter(Boolean).sort((a, b) => a.slot - b.slot), complete };
}

/**
 * One tick line per advancing PS_TICK in `txs` (scanRecords' order), `to`
 * normalized by tickLine. Inputs come from the PS_INPUT records in `txs`
 * and from `knownLines` (index lines already written); a part whose input
 * is not fully published gets no line. `commitSignature` comes only from a
 * PS_COMMITS logged by an instruction whose accounts[0] is `anchor` (this
 * season's world chunk 0); `committed`/`revealed` from that close and this
 * season's PS_SALTS.
 */
export function buildTickLines(txs, { anchor, knownLines = [] }) {
  const anchorKey = keyText(anchor);
  const inputs = new Map(); // hash → { parts, signatures }
  const known = new Map(knownLines.filter(l => l.input).map(l => [l.inputHash, { input: l.input, hash: l.inputHash, signatures: l.inputSignatures ?? [] }]));
  const closes = new Map(); // tick → { signature, committed }
  const revealed = new Map(); // tick → salts
  const ticks = [];
  for (const t of txs) {
    for (const { record: r, accounts } of t.emitted) {
      const bound = accounts?.[0] === anchorKey;
      if (r.tag === 'PS_INPUT') {
        const h = hex(r.hash);
        if (!inputs.has(h)) inputs.set(h, { parts: [], signatures: [] });
        inputs.get(h).parts.push(r);
        inputs.get(h).signatures.push(t.signature);
      } else if (r.tag === 'PS_COMMITS' && bound) closes.set(r.tick, { signature: t.signature, committed: r.commits.length });
      else if (r.tag === 'PS_SALTS' && bound) revealed.set(r.tick, r.salts.length);
      else if (r.tag === 'PS_TICK' && hex(r.preRoot) !== hex(r.root)) ticks.push({ rec: r, t });
    }
  }
  const lines = [];
  for (const { rec, t } of ticks) {
    const h = hex(rec.inputHash);
    let published = known.get(h);
    if (!published && inputs.has(h)) {
      try { published = assembleInput(inputs.get(h).parts, inputs.get(h).signatures); } catch { published = null; }
    }
    if (!published) continue;
    const close = closes.get(rec.tick);
    const seals = { committed: close?.committed ?? null, revealed: revealed.get(rec.tick) ?? null, commitSignature: close?.signature ?? null };
    lines.push(tickLine({ rec, published, signature: t.signature, cu: t.cu, seals }));
  }
  return lines;
}

/** The lines that chain from `fromRoot` (each `preRoot` the previous `root`), in chain order; stops at a gap. */
export function chainLines(lines, fromRoot) {
  const byPre = new Map();
  for (const l of lines) if (l.preRoot !== l.root && !byPre.has(l.preRoot)) byPre.set(l.preRoot, l);
  const out = [];
  const seen = new Set();
  for (let root = fromRoot; root && byPre.has(root) && !seen.has(root); root = out.at(-1).root) {
    seen.add(root);
    out.push(byPre.get(root));
  }
  return out;
}
