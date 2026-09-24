// End-to-end season on a plain local validator (no ER): USDC entry, on-chain
// genesis, every tick resolved on chain, payouts and claims.
//
//   node scripts/e2e-base.mjs --rpc http://127.0.0.1:8989 --program <id> [--ticks 180]
import { Connection, Keypair, LAMPORTS_PER_SOL, Transaction, sendAndConfirmTransaction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { createMintIxs, createTokenAccountIxs, mintToIx, tokenBalance } from '../src/spl.mjs';
import { decodeSeason, decodeWorldHeader, decodeOrdersHeader } from '../client/src/codec.mjs';

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const rpc = arg('--rpc', 'http://127.0.0.1:8989');
const programId = arg('--program', 'J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n');
const maxTicks = Number(arg('--ticks', '180'));
const conn = new Connection(rpc, 'confirmed');
const USDC = 1_000_000n;

async function send(ixs, signers, label) {
  const tx = new Transaction().add(...ixs);
  try {
    const sig = await sendAndConfirmTransaction(conn, tx, signers, { commitment: 'confirmed', skipPreflight: true });
    const t = await conn.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
    if (t?.meta?.err) throw Object.assign(new Error(`${label} failed ${JSON.stringify(t.meta.err)}`), { logs: t.meta.logMessages });
    return t;
  } catch (e) {
    const logs = e.logs || (await e.getLogs?.(conn).catch(() => null));
    console.error(label, 'FAILED', String(e).slice(0, 200), (logs || []).slice(-6).join('\n  '));
    throw e;
  }
}
const airdrop = async kp => conn.confirmTransaction(await conn.requestAirdrop(kp.publicKey, 20 * LAMPORTS_PER_SOL), 'confirmed');

const admin = Keypair.generate(), crank = Keypair.generate(), mint = Keypair.generate();
await airdrop(admin); await airdrop(crank);
await send(await createMintIxs(conn, { payer: admin.publicKey, mint: mint.publicKey, authority: admin.publicKey }), [admin, mint], 'mint');

const seasonId = BigInt(Date.now());
const chain = new ChainClient(programId, seasonId);
await send(chain.createSeason({ admin: admin.publicKey, mint: mint.publicKey, maxCivs: 6, entryFee: 10n * USDC, exchangeCredit: 20n * USDC, tickSeconds: 1, worldSeed: new Uint8Array(32).fill(7), crank: crank.publicKey }), [admin], 'createSeason');
for (let k = 0; k < chain.worldChunks.length; k++) await send(chain.allocWorld({ payer: crank.publicKey, chunk: k }), [crank], `allocWorld ${k}`);

const players = [];
for (let i = 0; i < 6; i++) {
  const p = { wallet: Keypair.generate(), session: Keypair.generate(), token: Keypair.generate() };
  await airdrop(p.wallet);
  await send([...await createTokenAccountIxs(conn, { payer: p.wallet.publicKey, account: p.token.publicKey, mint: mint.publicKey, owner: p.wallet.publicKey }),
    mintToIx({ mint: mint.publicKey, dest: p.token.publicKey, authority: admin.publicKey, amount: 100n * USDC })], [p.wallet, p.token, admin], 'fund');
  // Fee payer = the crank (like the x402 facilitator): players need no SOL for entry.
  await send(chain.joinSeason({ player: p.wallet.publicKey, feePayer: crank.publicKey, civ: i, playerToken: p.token.publicKey, mint: mint.publicKey,
    name: ['Aster', 'Borealis', 'Cinder', 'Dunmar', 'Ember', 'Fjordhal'][i], kind: i === 0 ? 0 : 1, session: p.session.publicKey, payout: p.wallet.publicKey }), [crank, p.wallet], `join ${i}`);
  players.push(p);
}
const vault0 = await tokenBalance(conn, chain.vault);
console.log('joined 6, vault', Number(vault0) / 1e6, 'USDC');

await send(chain.startSeason({ authority: crank.publicKey }), [crank], 'startSeason');
let steps = 0;
for (;;) {
  const s = decodeSeason((await conn.getAccountInfo(chain.season)).data);
  if (s.status === 'Running') break;
  await send(chain.genesisStep({ civs: 6, work: 50 }), [crank], 'genesis'); steps++;
}
const season = decodeSeason((await conn.getAccountInfo(chain.season)).data);
console.log('genesis done in', steps, 'steps; season seed', Buffer.from(season.seasonSeed).toString('hex').slice(0, 16));

const cus = [];
for (let t = 0; t < maxTicks; t++) {
  const o0 = decodeOrdersHeader((await conn.getAccountInfo(chain.orders(0))).data);
  await Promise.all(players.map((p, i) => send(chain.submitOrders({ signer: p.session.publicKey, civ: i, tick: o0.openTick, decisionDigest: new Uint8Array(32), orders: [] }), [crank, p.session], `submit ${i}`)));
  const r = await send(chain.resolveTick({ civs: 6 }), [crank], `resolve ${t}`);
  cus.push(r.meta.computeUnitsConsumed);
  if (t % 30 === 0) console.log('tick', t, 'CU', r.meta.computeUnitsConsumed);
}
const header = decodeWorldHeader((await conn.getAccountInfo(chain.world)).data);
console.log('ticks resolved', cus.length, 'max CU', Math.max(...cus), 'finished', header.meta.finished);
if (!header.meta.finished) process.exit(0);

await send(chain.finishSeason(), [crank], 'finish');
const done = decodeSeason((await conn.getAccountInfo(chain.season)).data);
console.log('status', done.status, 'payouts', done.payouts.map(x => Number(x) / 1e6), 'rollover', Number(done.rollover) / 1e6);
let paid = 0n;
for (let i = 0; i < 6; i++) {
  if (done.payouts[i] === 0n) continue;
  await send(chain.claim({ owner: players[i].wallet.publicKey, civ: i, dest: players[i].token.publicKey, mint: mint.publicKey }), [crank, players[i].wallet], `claim ${i}`);
  paid += done.payouts[i];
}
const vault1 = await tokenBalance(conn, chain.vault);
console.log('claimed', Number(paid) / 1e6, 'vault left', Number(vault1) / 1e6, 'conserved', vault0 - paid === vault1);
let double = false;
try { await send(chain.claim({ owner: players[0].wallet.publicKey, civ: 0, dest: players[0].token.publicKey, mint: mint.publicKey }), [crank, players[0].wallet], 'double claim'); } catch { double = true; }
console.log('second claim rejected', double);
