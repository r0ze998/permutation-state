// Season bootstrap on a local (or devnet) stack: test USDC, a season, six
// civilizations paying their entry, on-chain genesis, and delegation of the
// world and every orders account to the ER.
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js';
import { ChainClient, ORDERS_TARGET } from '../client/src/chain.mjs';
import { createMintIxs, createTokenAccountIxs, mintToIx, tokenBalance } from './spl.mjs';
import { decodeSeason } from '../client/src/codec.mjs';
import { namedKey, writeState } from './config.mjs';
import { send } from './send.mjs';

export const CIVS = [
  { name: 'Aster', kind: 0, hosted: 'human' },
  { name: 'Borealis', kind: 1, hosted: 'bot' },
  { name: 'Cinder', kind: 1, hosted: 'bot' },
  { name: 'Dunmar', kind: 1, hosted: 'bot' },
  { name: 'Ember', kind: 1, hosted: 'bot' },
  { name: 'Fjordhal', kind: 1, hosted: 'bot' },
];
const USDC = 1_000_000n;

async function airdrop(conn, key, sol = 20) {
  const bal = await conn.getBalance(key, 'confirmed');
  if (bal >= sol * LAMPORTS_PER_SOL / 2) return;
  await conn.confirmTransaction(await conn.requestAirdrop(key, sol * LAMPORTS_PER_SOL), 'confirmed');
}

/**
 * Create and start a new season. `log` receives progress lines.
 * Returns the persisted gateway state.
 */
export async function bootstrap({ base, er, cfg, civs = CIVS, joinOpen = false, log = console.log }) {
  const admin = namedKey('admin');
  const crank = namedKey('crank');
  const mint = namedKey('usdc-mint');
  if (cfg.cluster === 'localnet') {
    await airdrop(base, admin.publicKey, 50);
    await airdrop(base, crank.publicKey, 50);
  }
  if (!(await base.getAccountInfo(mint.publicKey))) {
    await send(base, await createMintIxs(base, { payer: admin.publicKey, mint: mint.publicKey, authority: admin.publicKey }), [admin, mint], 'create test USDC mint');
    log(`test USDC mint ${mint.publicKey.toBase58()} (localnet only, not real USDC)`);
  }
  const seasonId = BigInt(Date.now());
  const chain = new ChainClient(cfg.programId, seasonId);
  await send(base, chain.createSeason({ admin: admin.publicKey, mint: mint.publicKey, maxCivs: 6, entryFee: cfg.entryFee, exchangeCredit: 20n * USDC,
    tickSeconds: cfg.tickSeconds, worldSeed: worldSeedFor(seasonId), crank: crank.publicKey }), [admin], 'createSeason');
  for (let k = 0; k < chain.worldChunks.length; k++) await send(base, chain.allocWorld({ payer: crank.publicKey, chunk: k }), [crank], `allocWorld ${k}`);
  log(`season ${seasonId} created; world allocated`);

  const state = { seasonId: seasonId.toString(), programId: cfg.programId, mint: mint.publicKey.toBase58(), cluster: cfg.cluster, civs: [], ticks: [] };
  for (const [i, c] of civs.entries()) {
    if (joinOpen && c.hosted === 'open') continue; // left for an outside entrant (x402)
    const wallet = namedKey(`civ${i}-wallet`), session = namedKey(`civ${i}-session`), token = namedKey(`civ${i}-usdc`);
    await joinWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ: i, name: c.name, kind: c.kind });
    state.civs.push({ civ: i, name: c.name, kind: c.kind, hosted: c.hosted, wallet: wallet.publicKey.toBase58(), session: session.publicKey.toBase58(), usdc: token.publicKey.toBase58() });
    log(`civ ${i} ${c.name} joined (${c.hosted})`);
  }
  writeState(state);
  return state;
}

/** Fund a local player with test USDC and join. The crank pays SOL fees. */
export async function joinWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ, name, kind }) {
  if (!(await base.getAccountInfo(token.publicKey))) {
    await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: token.publicKey, mint: mint.publicKey, owner: wallet.publicKey }), [crank, token], 'create USDC account');
  }
  if ((await tokenBalance(base, token.publicKey)) < cfg.entryFee) {
    await send(base, [mintToIx({ mint: mint.publicKey, dest: token.publicKey, authority: admin.publicKey, amount: 100n * USDC })], [crank, admin], 'mint test USDC');
  }
  await send(base, chain.joinSeason({ player: wallet.publicKey, feePayer: crank.publicKey, civ, playerToken: token.publicKey, mint: mint.publicKey,
    name, kind, session: session.publicKey, payout: wallet.publicKey }), [crank, wallet], `join ${name}`);
}

/** Close entry, run genesis to completion and delegate everything to the ER. */
export async function startAndDelegate({ base, er, cfg, state, log = console.log }) {
  const crank = namedKey('crank');
  const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
  const season = decodeSeason((await base.getAccountInfo(chain.season)).data);
  const civs = season.civs.length;
  if (season.status === 'Registering') {
    await send(base, chain.startSeason({ authority: crank.publicKey }), [crank], 'startSeason');
  }
  let steps = 0;
  for (;;) {
    const s = decodeSeason((await base.getAccountInfo(chain.season)).data);
    if (s.status !== 'Genesis') break;
    const r = await send(base, chain.genesisStep({ civs, work: 50 }), [crank], 'genesisStep');
    steps++;
    for (const rec of r.records) if (rec.tag === 'PS_GENESIS') state.genesis = { root: Buffer.from(rec.root).toString('hex'), seasonSeed: Buffer.from(rec.seasonSeed).toString('hex'), signature: r.signature };
  }
  log(`genesis complete on chain in ${steps} steps`);
  const targets = [...chain.worldChunks.map((_, k) => k), ...Array.from({ length: civs }, (_, c) => ORDERS_TARGET + c)];
  for (const target of targets) {
    await send(base, chain.delegate({ authority: crank.publicKey, target, validator: cfg.erValidator }), [crank], `delegate ${target}`);
  }
  // Wait until the ER serves the delegated world.
  for (let i = 0; i < 60; i++) {
    const w = await er.getAccountInfo(chain.world, 'confirmed').catch(() => null);
    if (w && w.owner.equals(new PublicKey(cfg.programId))) break;
    await new Promise(r => setTimeout(r, 500));
  }
  state.delegated = true;
  writeState(state);
  log(`world and ${civs} orders accounts delegated to the ER`);
  return state;
}

/** World seed of a season: public and fixed at creation (terrain, §2.4). */
export function worldSeedFor(seasonId) {
  const b = new Uint8Array(32);
  new DataView(b.buffer).setBigUint64(0, BigInt(seasonId), true);
  b.set(new TextEncoder().encode('PS-world'), 8);
  return b;
}

export { Keypair };
