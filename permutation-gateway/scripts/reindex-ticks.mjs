// Rebuild a season's tick index (.local/ticks/<season>.jsonl) from the ER's
// own transaction history: every ResolveTick writes to world chunk 0, and
// its logs carry the PS_TICK record; the input it hashes is in the PS_INPUT
// records of the LogTickInput transactions just before it. Use it when the
// crank could not archive a record (the gateway index is a convenience; the
// chain is the source). Lines have the crank's format (src/ticks.mjs
// `tickLine`): the CloseCommits transaction (its PS_COMMITS record, which
// the verifier checks reveals against) and the counts of commitments and of
// revealed batches (the PS_SALTS record) come from the chain too.
//
//   node scripts/reindex-ticks.mjs [--state season.json]
import { copyFileSync, existsSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { Connection } from '@solana/web3.js';
import { toHex as hex } from '../client/src/bytes.mjs';
import { ChainClient } from '../client/src/chain.mjs';
import { createStateStore, LOCAL_DIR, loadConfig } from '../src/config.mjs';
import { records } from '../src/send.mjs';
import { assembleInput, formatTickLines, tickLine } from '../src/ticks.mjs';

const cfg = loadConfig();
const state = createStateStore(cfg.stateFile).load();
if (!state) throw new Error(`no season state in ${cfg.stateFile}`);
const er = new Connection(cfg.erRpc, 'confirmed');
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const world = chain.worldChunks[0];
const file = path.join(LOCAL_DIR, 'ticks', `${state.seasonId}.jsonl`);

// Newest first, 1000 per page, until the history is exhausted.
const sigs = [];
for (let before; ;) {
  const page = await er.getSignaturesForAddress(world, { before, limit: 1000 }, 'confirmed');
  sigs.push(...page.filter(s => !s.err));
  if (page.length < 1000) break;
  before = page[page.length - 1].signature;
}
sigs.reverse();
const ticks = []; // { slot, rec, signature, cu }
const inputs = new Map(); // hash → { parts, signatures }
const closes = new Map(); // tick → { signature, committed } (the latest CloseCommits of that tick)
const saltCount = new Map(); // tick → revealed batches
for (const s of sigs) {
  const t = await er.getTransaction(s.signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  for (const rec of records(t?.meta?.logMessages || [])) {
    if (rec.tag === 'PS_INPUT') {
      const h = hex(rec.hash);
      if (!inputs.has(h)) inputs.set(h, { parts: [], signatures: [] });
      const e = inputs.get(h);
      e.parts.push(rec);
      e.signatures.push(s.signature);
    }
    if (rec.tag === 'PS_TICK') ticks.push({ slot: s.slot, rec, signature: s.signature, cu: t.meta.computeUnitsConsumed ?? null });
    if (rec.tag === 'PS_COMMITS') closes.set(rec.tick, { signature: s.signature, committed: rec.commits.length });
    if (rec.tag === 'PS_SALTS') saltCount.set(rec.tick, rec.salts.length);
  }
}
const lines = [];
for (const { slot, rec, signature, cu } of ticks) {
  const e = inputs.get(hex(rec.inputHash));
  // A part whose input is not fully published still chains the roots; it is left out of the index.
  let published = { input: null, hash: hex(rec.inputHash), signatures: [] };
  try {
    if (!e) throw new Error(`tick ${rec.tick}: its input (${hex(rec.inputHash).slice(0, 16)}…) is not published`);
    published = assembleInput(e.parts, e.signatures);
  } catch (err) {
    console.error(err.message);
  }
  const close = closes.get(rec.tick);
  const seals = { committed: close?.committed ?? null, revealed: saltCount.get(rec.tick) ?? null, commitSignature: close?.signature ?? null };
  lines.push({ slot, line: tickLine({ rec, published, signature, cu, seals }) });
}
// Same-slot order is not guaranteed by the listing: chain the records by root.
lines.sort((a, b) => a.slot - b.slot);
const all = lines.map(x => x.line);
const out = [];
for (const first = all.find(l => l.tick === 0); first && out.length < all.length;) {
  const prev = out[out.length - 1];
  const next = prev ? all.find(l => !out.includes(l) && l.preRoot === prev.root && (l.tick > prev.tick || l.to >= prev.to || l.preRoot !== l.root)) : first;
  if (!next) break;
  out.push(next);
}
if (existsSync(file)) copyFileSync(file, `${file}.bak`);
writeFileSync(file, formatTickLines(out.filter(l => l.input)));
const covered = new Set(out.map(l => l.tick));
console.log(`${sigs.length} transactions on world chunk 0; ${ticks.length} PS_TICK records; ${out.length} chained covering ticks 0..${Math.max(...covered)} (${covered.size} ticks) → ${file}`);
if (out.length < ticks.length) console.log(`${ticks.length - out.length} records did not chain (no-op parts, an unpublished input or a gap); check with verify`);
