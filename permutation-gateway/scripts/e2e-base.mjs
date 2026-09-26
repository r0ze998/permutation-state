// Program regression check on the base layer alone (no ER), Game Design V5:
// create a season, register AI members with test USDC, run genesis, seat the
// members and hold the first election, then play every tick on base — each
// held office seals an (empty) batch (CommitOrders), the commitments close
// after the deadline (CloseCommits), every batch is revealed (RevealOrders),
// the input is published (LogTickInput: the randomness comes from the
// revealed salts) and the tick resolved (ResolveTick, in parts when a tick
// exceeds one transaction, as the crank does) — and finally the AI roster
// revealed in its committed order (RevealRoster), FinishSeason and the
// conservation of the vault. Prints the compute used per tick.
//
// It runs the gateway's dev mode on purpose (no registration window: the AI
// members register at creation, whatever the environment says).
//
//   node scripts/e2e-base.mjs [--base http://127.0.0.1:18899] [--ticks 180] [--tick-seconds 2] [--ai 2] [--state e2e-base.json]
//
// Works against the local stack's base validator or a plain
// solana-test-validator with the program loaded (`--bpf-program`).
import { createHash, randomBytes } from 'node:crypto';
import { Connection, PublicKey } from '@solana/web3.js';
import { ChainClient, readSeason } from '../client/src/chain.mjs';
import { chainError, decodeNationHeader, decodeWorldHeader, NOBODY, orderCommitment, ROLES } from '../client/src/codec.mjs';
import { fromHex } from '../client/src/bytes.mjs';
import { createStateStore, loadConfig, namedKey, parseArgs } from '../src/config.mjs';
import { aiMembers, bootstrap, memberKeyName, startAndDelegate } from '../src/season.mjs';
import { send } from '../src/send.mjs';
import { tokenBalance } from '../src/spl.mjs';
import { publishTickInput, resolveInParts } from '../src/ticks.mjs';

const TICKS = 180;
const args = parseArgs(process.argv.slice(2), { ticks: String(TICKS) });
const maxTicks = Number(args.ticks);
if (!Number.isInteger(maxTicks) || maxTicks < 1) throw new Error(`--ticks must be a positive integer, got ${args.ticks}`);
const cfg = loadConfig({ defaults: { stateFile: 'e2e-base.json', tickSeconds: 2 } });
// Dev mode, explicitly: every AI member registers now, nobody else is waited for.
Object.assign(cfg, { registrationSeconds: 0, waitExternal: 0, allowIdentifiableAi: true });
const base = new Connection(cfg.baseRpc, 'confirmed');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

const store = createStateStore(cfg.stateFile);
await bootstrap({ base, cfg, store, log });
const state = await startAndDelegate({ base, er: null, cfg, store, log, delegate: false });
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = namedKey('crank');
const nations = (await readSeason(base, chain)).nations;
const sessionOf = new Map(state.members.map(m => [m.index, namedKey(memberKeyName(m, 'session'))]));

const cus = [];
const reach = {};
for (;;) {
  const accounts = await base.getMultipleAccountsInfo([...chain.worldChunks, ...chain.nations(nations)], 'confirmed');
  const header = decodeWorldHeader(Buffer.concat(accounts.slice(0, chain.worldChunks.length).map(a => a.data)));
  if (header.meta.finished) break;
  const heads = accounts.slice(chain.worldChunks.length).map(a => decodeNationHeader(a.data));
  const tick = heads[0].openTick;
  if (tick >= maxTicks) break;
  // Every held office seals an empty batch (in parallel), signed by the
  // holder's session key; a vacant office takes none (the caretaker fills it).
  const sealed = [];
  await Promise.all(heads.flatMap(n => ROLES.map((role, i) => {
    if (n.officers[i] === NOBODY || n.committed[i] === tick) return null;
    const signer = sessionOf.get(n.officers[i]);
    const decisionDigest = createHash('sha256').update(`e2e-base/${state.seasonId}/${tick}/${n.civ}/${role}`).digest();
    const batch = { civ: n.civ, tick, role, member: n.officers[i], decisionDigest, orders: [], adopt: [] };
    const salt = new Uint8Array(randomBytes(32));
    sealed.push({ ...batch, salt });
    return send(base, chain.commitOrders({ signer: signer.publicKey, civ: n.civ, role, tick, commitment: orderCommitment(batch, salt) }),
      [crank, signer], `commit ${n.civ}/${role}`);
  })));
  // Close the commitments once the deadline passed (on the validator's clock), then reveal.
  for (;;) {
    try { await send(base, chain.closeCommits({ nations }), [crank], `close tick ${tick}`); break; } catch (e) {
      if (chainError(e.message) !== 'TooEarly') throw e;
      await new Promise(r => setTimeout(r, 400));
    }
  }
  await Promise.all(sealed.map(b => send(base, chain.revealOrders({ signer: crank.publicKey, ...b }), [crank], `reveal ${b.civ}/${b.role}`)));
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

let s = await readSeason(base, chain);
const header = decodeWorldHeader(Buffer.concat((await base.getMultipleAccountsInfo(chain.worldChunks, 'confirmed')).map(a => a.data)));
if (header.meta.finished && s.status === 'Running') {
  // The operator's AI roster, in the order the season committed (roster
  // position, not member index), then the payouts.
  const ais = aiMembers(state);
  for (let i = s.rosterRevealed; i < ais.length; i += 8) {
    const group = ais.slice(i, i + 8);
    await send(base, chain.revealRoster({ members: group.map(m => new PublicKey(m.wallet)), salts: group.map(m => fromHex(m.salt)) }), [crank], `reveal roster ${i}`);
  }
  if (ais.length) log(`AI roster revealed: members ${ais.map(m => m.index).join(', ')} (roster positions ${ais.map(m => m.pos).join(', ')})`);
  await send(base, chain.finishSeason({ roster: s.aiCount > 0 }), [crank], 'finishSeason');
  s = await readSeason(base, chain);
  const paid = s.payouts.reduce((a, b) => a + b, 0n);
  const vault = await tokenBalance(base, chain.vault);
  const refunds = s.treasuryFinal.reduce((a, b) => a + b, 0n);
  const ok = paid + s.ops + refunds === vault;
  log(`season ${s.status}: pool ${s.pool}, payouts ${paid}, operations ${s.ops}, treasury refunds ${refunds}, vault ${vault} — conserved ${ok}`);
  if (!ok) process.exit(1);
} else log(`stopped at tick ${cus.length} (season ${s.status}); run with --ticks ${TICKS} to finish it`);
