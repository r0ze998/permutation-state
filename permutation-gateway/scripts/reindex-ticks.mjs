// Rebuild a season's tick index (.local/ticks/<season>.jsonl) from the ER's
// own transaction history: every CloseCommits, LogTickInput and ResolveTick
// lists world chunk 0, so its history holds every PS_TICK record and the
// PS_INPUT records of the inputs they hash. Use it when the crank could not
// archive a part (the gateway index is a convenience; the chain is the
// source). Lines have the crank's format (src/ticks.mjs `tickLine`).
//
// The lines follow the chain of state roots from the first election's root
// (PS_OPEN, from the season state; else the first tick-0 record's pre-state
// root), so a no-op record cannot start or fork them. Records of other
// programs are ignored, and so are PS_COMMITS logged by another season's
// instruction (its CloseCommits can list this season's chunk 0 as an extra
// account); no-op records get no line and `to` is the stop that ran (≤ 12).
//
//   node scripts/reindex-ticks.mjs [--state season.json]
import { copyFileSync, existsSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { Connection } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { createStateStore, LOCAL_DIR, loadConfig } from '../src/config.mjs';
import { formatTickLines } from '../src/ticks.mjs';
import { buildTickLines, chainLines, scanRecords } from '../src/tickscan.mjs';

const cfg = loadConfig();
const state = createStateStore(cfg.stateFile).load();
if (!state) throw new Error(`no season state in ${cfg.stateFile}`);
const er = new Connection(cfg.erRpc, 'confirmed');
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const world = chain.worldChunks[0];
const file = path.join(LOCAL_DIR, 'ticks', `${state.seasonId}.jsonl`);

const { txs, complete } = await scanRecords({ connection: er, address: world, program: cfg.programId });
const all = buildTickLines(txs, { anchor: world });
const firstTick0 = all.find(l => l.tick === 0);
const out = chainLines(all, state.open?.root ?? firstTick0?.preRoot);
if (existsSync(file)) copyFileSync(file, `${file}.bak`);
writeFileSync(file, formatTickLines(out));
const ticks = txs.reduce((n, t) => n + t.emitted.filter(e => e.record.tag === 'PS_TICK').length, 0);
const covered = new Set(out.map(l => l.tick));
console.log(`${txs.length} transactions on world chunk 0; ${ticks} PS_TICK records; ${out.length} chained covering ticks 0..${Math.max(-1, ...covered)} (${covered.size} ticks) → ${file}`);
if (out.length < all.length) console.log(`${all.length - out.length} advancing records did not chain (another history, or a gap); check with verify`);
if (!complete) console.log('some transactions could not be read; run again, or check with verify');
