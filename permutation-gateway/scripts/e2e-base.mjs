// Program regression check on the base layer alone (no ER), Game Design V5:
// create a season, register AI members with test USDC, run genesis, seat the
// members and hold the first election, then play every tick on base — each
// office submits an (empty) sealed batch, the input is published
// (LogTickInput) and the tick resolved (ResolveTick, in parts when a tick
// exceeds one transaction) — and finally FinishSeason and the conservation
// of the vault. Prints the compute used per tick.
//
//   node scripts/e2e-base.mjs [--base http://127.0.0.1:18899] [--ticks 180] [--state e2e-base.json]
//
// Works against the local stack's base validator or a plain
// solana-test-validator with the program loaded (`--bpf-program`).
import { createHash } from 'node:crypto';
import { Connection } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeNationHeader, decodeSeason, decodeWorldHeader, NOBODY, ROLES } from '../client/src/codec.mjs';
import { loadConfig, namedKey } from '../src/config.mjs';
import { bootstrap, defaultRoster, startAndDelegate } from '../src/season.mjs';
import { send } from '../src/send.mjs';
import { tokenBalance } from '../src/spl.mjs';

const argv = process.argv.includes('--state') ? process.argv : [...process.argv, '--state', 'e2e-base.json'];
const cfg = loadConfig(argv);
const maxTicks = Number(argv[argv.indexOf('--ticks') + 1] ?? 180) || 180;
const base = new Connection(cfg.baseRpc, 'confirmed');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);
const PLANS = [[12], [5, 7, 12], [2, 4, 5, 6, 7, 9, 12]];

let state = await bootstrap({ base, cfg, roster: defaultRoster({ humans: 0, ai: 2 }), log });
state = await startAndDelegate({ base, er: null, cfg, state, log, delegate: false });
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = namedKey('crank');
const nations = state.nations.length;
const sessionOf = new Map(state.members.map(m => [m.index, namedKey(`member${m.key}-session`)]));

const cus = [];
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
  // Publish the input (every chunk), then resolve.
  for (let chunk = 0, total = 1; chunk < total; chunk++) {
    const r = await send(base, chain.logTickInput({ nations, chunk }), [crank], `publish tick ${tick} input ${chunk}`);
    total = r.records.find(x => x.tag === 'PS_INPUT')?.total ?? total;
  }
  let used = null;
  for (const plan of PLANS) {
    try {
      used = [];
      for (const to of plan) used.push((await send(base, chain.resolveTick({ nations, to }), [crank], `resolve tick ${tick} to ${to}`)).cu);
      break;
    } catch (e) {
      if (!/exceeded CUs|ProgramFailedToComplete/.test(`${e.message} ${(e.logs || []).join(' ')}`) || plan === PLANS.at(-1)) throw e;
    }
  }
  cus.push(used.reduce((a, b) => a + b, 0));
  if (tick % 20 === 0) log(`tick ${tick} resolved on base (${used.join(' + ')} CU)`);
}
const avg = cus.reduce((a, b) => a + b, 0) / cus.length;
log(`${cus.length} ticks on base: ${Math.round(avg)} CU per tick on average, ${Math.max(...cus)} at most`);

let s = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
const header = decodeWorldHeader(Buffer.concat((await base.getMultipleAccountsInfo(chain.worldChunks, 'confirmed')).map(a => a.data)));
if (header.meta.finished && s.status === 'Running') {
  await send(base, chain.finishSeason(), [crank], 'finishSeason');
  s = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
  const paid = s.payouts.reduce((a, b) => a + b, 0n);
  const vault = await tokenBalance(base, chain.vault);
  const refunds = s.treasuryFinal.reduce((a, b) => a + b, 0n);
  const ok = paid + s.ops + refunds === vault;
  log(`season ${s.status}: pool ${s.pool}, payouts ${paid}, operations ${s.ops}, treasury refunds ${refunds}, vault ${vault} — conserved ${ok}`);
  if (!ok) process.exit(1);
} else log(`stopped at tick ${cus.length} (season ${s.status}); run with --ticks 180 to finish it`);
