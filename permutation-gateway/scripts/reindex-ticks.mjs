// Rebuild a season's tick index (.local/ticks/<season>.jsonl) from the ER's
// own transaction history: every ResolveTick writes to world chunk 0, and its
// logs carry the PS_TICK record; the input it hashes is in the PS_INPUT
// records of the LogTickInput transactions just before it. Use it when the crank could not archive a
// record (the gateway index is a convenience; the chain is the source).
//
//   node scripts/reindex-ticks.mjs [--state season.json]
import { copyFileSync, existsSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { Connection } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { LOCAL_DIR, loadConfig, readState } from '../src/config.mjs';
import { records } from '../src/send.mjs';

const cfg = loadConfig();
const state = readState(cfg.stateFile);
const er = new Connection(cfg.erRpc, 'confirmed');
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const world = chain.worldChunks[0];
const hex = b => Buffer.from(b).toString('hex');

// Newest first, 1000 per page, until the history is exhausted.
const sigs = [];
for (let before; ;) {
  const page = await er.getSignaturesForAddress(world, { before, limit: 1000 }, 'confirmed');
  sigs.push(...page.filter(s => !s.err));
  if (page.length < 1000) break;
  before = page[page.length - 1].signature;
}
sigs.reverse();
const lines = [];
const inputs = new Map(); // hash → chunks
for (const s of sigs) {
  const t = await er.getTransaction(s.signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  for (const rec of records(t?.meta?.logMessages || [])) {
    if (rec.tag === 'PS_INPUT') {
      const h = hex(rec.hash);
      if (!inputs.has(h)) inputs.set(h, { total: rec.total, chunks: [], signatures: [] });
      const e = inputs.get(h);
      if (!e.chunks[rec.chunk]) (e.chunks[rec.chunk] = rec.bytes), e.signatures.push(s.signature);
    }
    if (rec.tag === 'PS_TICK') {
      lines.push({ slot: s.slot, tick: rec.tick, to: rec.to, preRoot: hex(rec.preRoot), root: hex(rec.root), inputHash: hex(rec.inputHash), signature: s.signature, cu: t.meta.computeUnitsConsumed ?? null });
    }
  }
}
for (const l of lines) {
  const e = inputs.get(l.inputHash);
  const complete = e && e.chunks.filter(Boolean).length === e.total;
  if (!complete) { console.error(`tick ${l.tick}: its input (${l.inputHash.slice(0, 16)}…) is not fully published`); continue; }
  const bytes = Buffer.concat(e.chunks);
  if (createHash('sha256').update(bytes).digest('hex') !== l.inputHash) { console.error(`tick ${l.tick}: published input does not match its hash`); continue; }
  Object.assign(l, { input: bytes.toString('hex'), inputSignatures: e.signatures });
}
// Same-slot order is not guaranteed by the listing: chain the records by root.
lines.sort((a, b) => a.slot - b.slot);
const out = [];
for (const first = lines.find(l => l.tick === 0); first && out.length < lines.length;) {
  const prev = out[out.length - 1];
  const next = prev ? lines.find(l => !out.includes(l) && l.preRoot === prev.root && (l.tick > prev.tick || l.to >= prev.to || l.preRoot !== l.root)) : first;
  if (!next) break;
  out.push(next);
}
const file = path.join(LOCAL_DIR, 'ticks', `${state.seasonId}.jsonl`);
if (existsSync(file)) copyFileSync(file, `${file}.bak`);
writeFileSync(file, out.filter(l => l.input).map(({ slot, ...l }) => JSON.stringify(l)).join('\n') + '\n');
const ticks = new Set(out.map(l => l.tick));
console.log(`${sigs.length} transactions on world chunk 0; ${lines.length} PS_TICK records; ${out.length} chained covering ticks 0..${Math.max(...ticks)} (${ticks.size} ticks) → ${file}`);
if (out.length < lines.length) console.log(`${lines.length - out.length} records did not chain (no-op parts or a gap); check with verify`);
