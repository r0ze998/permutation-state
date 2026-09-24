// After FinishSeason: every civ hosted by this gateway claims its payout
// into its own test-USDC account, and the vault is checked for conservation.
//
//   node scripts/claim-hosted.mjs [--state season.json]
import { Connection, PublicKey } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeSeason } from '../client/src/codec.mjs';
import { loadConfig, namedKey, readState } from '../src/config.mjs';
import { send } from '../src/send.mjs';
import { tokenBalance } from '../src/spl.mjs';

const cfg = loadConfig();
const state = readState(cfg.stateFile);
const base = new Connection(cfg.baseRpc, 'confirmed');
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = namedKey('crank');
const read = async () => decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);

const s = await read();
if (s.status !== 'Finalized') { console.error(`season ${state.seasonId} is ${s.status}, not Finalized`); process.exit(1); }
const mint = new PublicKey(s.usdcMint);
const vault0 = await tokenBalance(base, chain.vault);
console.log(`season ${state.seasonId}: pool ${Number(s.pool) / 1e6} USDC, payouts [${s.payouts.map(p => Number(p) / 1e6).join(', ')}], rollover ${Number(s.rollover) / 1e6}`);
let paid = 0n;
for (const c of state.civs) {
  if (c.hosted === 'external') continue; // outside agents claim with their own wallet
  const amount = s.payouts[c.civ] ?? 0n;
  if (!amount || s.claimed[c.civ]) { console.log(`civ ${c.civ} ${c.name}: nothing to claim`); continue; }
  const wallet = namedKey(`civ${c.civ}-wallet`);
  const dest = new PublicKey(c.usdc);
  const before = await tokenBalance(base, dest);
  const r = await send(base, chain.claim({ owner: wallet.publicKey, civ: c.civ, dest, mint }), [crank, wallet], `claim ${c.civ}`);
  const after = await tokenBalance(base, dest);
  paid += after - before;
  console.log(`civ ${c.civ} ${c.name}: claimed ${Number(after - before) / 1e6} USDC (${r.signature.slice(0, 16)}…)`);
}
const vault1 = await tokenBalance(base, chain.vault);
console.log(`vault ${Number(vault0) / 1e6} → ${Number(vault1) / 1e6}; conserved: ${vault0 - paid === vault1}`);
const first = state.civs.find(c => c.hosted !== 'external' && s.payouts[c.civ] > 0n);
if (first) {
  const w = namedKey(`civ${first.civ}-wallet`);
  const again = await send(base, chain.claim({ owner: w.publicKey, civ: first.civ, dest: new PublicKey(first.usdc), mint }), [crank, w], 'double claim').then(() => false, () => true);
  console.log(`second claim by civ ${first.civ} rejected: ${again}`);
}
