// Program regression check on the base layer alone (no ER), Game Design V5:
// create a season, register AI members with test USDC, run genesis, seat the
// members and hold the first election, then play every tick on base — each
// office submits an (empty) sealed batch, the input is published
// (LogTickInput) and the tick resolved (ResolveTick, in parts when a tick
// exceeds one transaction, as the crank does) — and finally FinishSeason and
// the conservation of the vault. Prints the compute used per tick.
//
//   node scripts/e2e-base.mjs [--base http://127.0.0.1:18899] [--ticks 180] [--state e2e-base.json]
//
// Works against the local stack's base validator or a plain
// solana-test-validator with the program loaded (`--bpf-program`).
import { createHash } from 'node:crypto';
import { Connection } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeNationHeader, decodeSeason, decodeWorldHeader, NOBODY, ROLES } from '../client/src/codec.mjs';
import { createStateStore, loadConfig, namedKey, parseArgs } from '../src/config.mjs';
import { bootstrap, defaultRoster, startAndDelegate } from '../src/season.mjs';
import { send } from '../src/send.mjs';
import { tokenBalance } from '../src/spl.mjs';
import { publishTickInput, resolveInParts } from '../src/ticks.mjs';

const TICKS = 180;
const args = parseArgs(process.argv.slice(2), { ticks: String(TICKS) });
const maxTicks = Number(args.ticks);
if (!Number.isInteger(maxTicks) || maxTicks < 1) throw new Error(`--ticks must be a positive integer, got ${args.ticks}`);
const cfg = loadConfig({ defaults: { stateFile: 'e2e-base.json' } });
const base = new Connection(cfg.baseRpc, 'confirmed');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

const store = createStateStore(cfg.stateFile);
await bootstrap({ base, cfg, store, roster: defaultRoster({ humans: 0, ai: 2 }), log });
const state = await startAndDelegate({ base, er: null, cfg, store, log, delegate: false });
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = namedKey('crank');
const readSeason = async () => decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
const nations = (await readSeason()).nations;
const sessionOf = new Map(state.members.map(m => [m.index, namedKey(`member${m.key}-session`)]));

const cus = [];
const reach = {};
for (;;) {
  const accounts = await base.getMultipleAccountsInfo([...chain.worldChunks, ...chain.nations(nations)], 'confirmed');
  const header = decodeWorldHeader(Buffer.concat(accounts.slice(0, chain.worldChunks.length).map(a => a.data)));
  if (header.meta.finished) break;
  const heads = accounts.slice(chain.worldChunks.length).map(a => decodeNationHeader(a.data));
  const tick = heads[0].openTick;
  if (tick >= maxTicks) break;
  // Every office seals an empty batch (in parallel): the holder's session
  // key, the crank for a vacant office.
  await Promise.all(heads.flatMap(n => ROLES.map((role, i) => {
    if (n.submitted[i] === tick) return null;
    const signer = n.officers[i] === NOBODY ? crank : sessionOf.get(n.officers[i]);
    const decisionDigest = createHash('sha256').update(`e2e-base/${state.seasonId}/${tick}/${n.civ}/${role}`).digest();
    return send(base, chain.submitOrders({ signer: signer.publicKey, civ: n.civ, role, tick, decisionDigest, orders: [] }),
      signer === crank ? [crank] : [crank, signer], `submit ${n.civ}/${role}`);
  })));
  // Publish the input (every chunk, checked against its hash), then resolve.
  await publishTickInput({ tick, publishChunk: chunk => send(base, chain.logTickInput({ nations, chunk }), [crank], `publish tick ${tick} input ${chunk}`) });
  if (tick % 10 === 0) for (const k of Object.keys(reach)) delete reach[k];
  const parts = await resolveInParts({ reach, resolve: to => send(base, chain.resolveTick({ nations, to }), [crank], `resolve tick ${tick} to ${to}`) });
  const used = parts.map(r => r.cu);
  cus.push(used.reduce((a, b) => a + b, 0));
  if (tick % 20 === 0) log(`tick ${tick} resolved on base (${used.join(' + ')} CU)`);
}
const avg = cus.reduce((a, b) => a + b, 0) / cus.length;
log(`${cus.length} ticks on base: ${Math.round(avg)} CU per tick on average, ${Math.max(...cus)} at most`);

let s = await readSeason();
const header = decodeWorldHeader(Buffer.concat((await base.getMultipleAccountsInfo(chain.worldChunks, 'confirmed')).map(a => a.data)));
if (header.meta.finished && s.status === 'Running') {
  await send(base, chain.finishSeason(), [crank], 'finishSeason');
  s = await readSeason();
  const paid = s.payouts.reduce((a, b) => a + b, 0n);
  const vault = await tokenBalance(base, chain.vault);
  const refunds = s.treasuryFinal.reduce((a, b) => a + b, 0n);
  const ok = paid + s.ops + refunds === vault;
  log(`season ${s.status}: pool ${s.pool}, payouts ${paid}, operations ${s.ops}, treasury refunds ${refunds}, vault ${vault} — conserved ${ok}`);
  if (!ok) process.exit(1);
} else log(`stopped at tick ${cus.length} (season ${s.status}); run with --ticks ${TICKS} to finish it`);
