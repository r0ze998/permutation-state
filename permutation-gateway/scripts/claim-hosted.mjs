// After FinishSeason: every member hosted by this gateway claims its prize
// (plus its treasury refund) into its own test-USDC account, the operations
// share is withdrawn, and the vault is checked for conservation.
//
//   node scripts/claim-hosted.mjs [--state season.json]
import { Connection, PublicKey } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { claimParts, decodeMember, decodeSeason, NATIONS } from '../client/src/codec.mjs';
import { createStateStore, loadConfig, namedKey } from '../src/config.mjs';
import { send } from '../src/send.mjs';
import { createTokenAccountIxs, tokenBalance } from '../src/spl.mjs';

const cfg = loadConfig();
const state = createStateStore(cfg.stateFile).load();
if (!state) throw new Error(`no season state in ${cfg.stateFile}`);
const base = new Connection(cfg.baseRpc, 'confirmed');
const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
const crank = namedKey('crank');
const admin = namedKey('admin');
const usdc = x => (Number(x) / 1e6).toFixed(2);
const read = async () => decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);

const s = await read();
if (s.status !== 'Finalized') { console.error(`season ${state.seasonId} is ${s.status}, not Finalized`); process.exit(1); }
const mint = new PublicKey(s.usdcMint);
const vault0 = await tokenBalance(base, chain.vault);
const prizes = s.payouts.reduce((a, b) => a + b, 0n);
console.log(`season ${state.seasonId}: pool ${usdc(s.pool)} USDC to ${s.payouts.filter(p => p > 0n).length} of ${s.memberCount} members (${usdc(prizes)} paid out, the rest is rounding), operations ${usdc(s.ops)}`);
let paid = 0n;
for (const m of state.members) {
  if (m.hosted === 'external') continue; // outside agents claim with their own wallet
  const wallet = namedKey(`member${m.key}-wallet`);
  const acc = decodeMember((await base.getAccountInfo(chain.member(wallet.publicKey), 'confirmed')).data);
  const { prize, refund, total } = claimParts(s, acc); // what the program pays (claim_amount)
  if (total === 0n || acc.claimed) { console.log(`member ${acc.index} ${m.name}: nothing to claim`); continue; }
  const dest = new PublicKey(m.usdc);
  const before = await tokenBalance(base, dest);
  const r = await send(base, chain.claim({ wallet: wallet.publicKey, dest, mint }), [crank, wallet], `claim ${acc.index}`);
  const after = await tokenBalance(base, dest);
  paid += after - before;
  console.log(`member ${acc.index} ${m.name} (${NATIONS[acc.civ]}): claimed ${usdc(after - before)} USDC = prize ${usdc(prize)} + treasury ${usdc(refund)}${after - before === total ? '' : ` — expected ${usdc(total)}`} (${r.signature.slice(0, 16)}…)`);
}
// Operations share, to the admin's own test-USDC account.
const opsAccount = namedKey('ops-usdc');
if (!(await base.getAccountInfo(opsAccount.publicKey))) {
  await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: opsAccount.publicKey, mint, owner: admin.publicKey }), [crank, opsAccount], 'ops account');
}
let ops = 0n;
if (!s.opsWithdrawn) {
  const before = await tokenBalance(base, opsAccount.publicKey);
  await send(base, chain.withdrawOps({ admin: admin.publicKey, dest: opsAccount.publicKey, mint }), [crank, admin], 'withdraw ops');
  ops = (await tokenBalance(base, opsAccount.publicKey)) - before;
  console.log(`operations share withdrawn: ${usdc(ops)} USDC (20% of fees and of in-play income, plus rounding)`);
}
const vault1 = await tokenBalance(base, chain.vault);
console.log(`vault ${usdc(vault0)} → ${usdc(vault1)}; conserved: ${vault0 - paid - ops === vault1}`);
const first = state.members.find(m => m.hosted !== 'external');
if (first) {
  const w = namedKey(`member${first.key}-wallet`);
  const again = await send(base, chain.claim({ wallet: w.publicKey, dest: new PublicKey(first.usdc), mint }), [crank, w], 'double claim').then(() => false, () => true);
  console.log(`second claim by member ${first.index} rejected: ${again}`);
}
