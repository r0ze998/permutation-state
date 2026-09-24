// Season lifecycle on a local (or devnet) stack, Game Design V5:
// test USDC, a season with six nations, members registering (hosted ones
// here, outside agents through x402), on-chain genesis, seating the members
// in the world, the first election, and delegation of the world and the
// nation accounts to the ER.
import { LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js';
import { ChainClient, NATION_TARGET } from '../client/src/chain.mjs';
import { decodeMember, decodeSeason, NOBODY, ROLES, roleMask } from '../client/src/codec.mjs';
import { createMintIxs, createTokenAccountIxs, mintToIx, tokenBalance } from './spl.mjs';
import { namedKey, writeState } from './config.mjs';
import { send } from './send.mjs';

export const NATIONS = ['Aster', 'Borealis', 'Cinder', 'Dunmar', 'Ember', 'Fjordal'];
const USDC = 1_000_000n;
const SEAT_BATCH = 10;

async function airdrop(conn, key, sol = 20) {
  const bal = await conn.getBalance(key, 'confirmed');
  if (bal >= sol * LAMPORTS_PER_SOL / 2) return;
  await conn.confirmTransaction(await conn.requestAirdrop(key, sol * LAMPORTS_PER_SOL), 'confirmed');
}

/**
 * The members this gateway registers and hosts: `humans` claimable human
 * members (nations 0, 1, …) who stand for general and steward, and `ai`
 * AI members per nation (run by the game server's reference AI) who stand
 * for two offices each, rotating, and vote for a human candidate where
 * there is one.
 */
export function defaultRoster({ humans = 1, ai = 2, nations = NATIONS.length } = {}) {
  const roster = [];
  for (let h = 0; h < humans; h++) roster.push({ civ: h % nations, hosted: 'human', kind: 0, name: `Player ${h + 1}`, stand: ['General', 'Steward'] });
  for (let c = 0; c < nations; c++) {
    for (let j = 0; j < ai; j++) {
      roster.push({ civ: c, hosted: 'ai', kind: 1, name: `${NATIONS[c]}-AI${j + 1}`, stand: [ROLES[(2 * j) % 4], ROLES[(2 * j + 1) % 4]] });
    }
  }
  return roster;
}

/** Votes of the first election for member `me` of `roster` (indices = registration order). */
function firstVotes(roster, me) {
  const m = roster[me];
  return ROLES.map(role => {
    const human = roster.findIndex(x => x.civ === m.civ && x.hosted === 'human' && x.stand.includes(role));
    if (human >= 0) return human;
    return m.stand.includes(role) ? me : NOBODY;
  });
}

/**
 * Create a season and register the hosted members. Registration stays open
 * (for x402 entrants) until the crank starts the season.
 */
export async function bootstrap({ base, cfg, roster = defaultRoster(), log = console.log }) {
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
  await send(base, chain.createSeason({ admin: admin.publicKey, mint: mint.publicKey, nations: NATIONS.length, entryFee: cfg.entryFee,
    tickSeconds: cfg.tickSeconds, worldSeed: worldSeedFor(seasonId), crank: crank.publicKey, market: cfg.market }), [admin], 'createSeason');
  for (let k = 0; k < chain.worldChunks.length; k++) await send(base, chain.allocWorld({ payer: crank.publicKey, chunk: k }), [crank], `allocWorld ${k}`);
  for (let c = 0; c < NATIONS.length; c++) await send(base, chain.allocNation({ payer: crank.publicKey, civ: c }), [crank], `allocNation ${c}`);
  log(`season ${seasonId} created: world and ${NATIONS.length} nation accounts allocated`);

  const state = { seasonId: seasonId.toString(), programId: cfg.programId, mint: mint.publicKey.toBase58(), cluster: cfg.cluster, nations: NATIONS, members: [], ticks: [], seating: [] };
  for (const [i, m] of roster.entries()) {
    const wallet = namedKey(`member${i}-wallet`), session = namedKey(`member${i}-session`), token = namedKey(`member${i}-usdc`);
    const index = await registerWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ: m.civ, name: m.name, kind: m.kind,
      stand: roleMask(m.stand), votes: firstVotes(roster, i), deposit: m.deposit ?? 0n });
    state.members.push({ index, civ: m.civ, name: m.name, kind: m.kind, hosted: m.hosted, key: i, wallet: wallet.publicKey.toBase58(), session: session.publicKey.toBase58(), usdc: token.publicKey.toBase58() });
    log(`member ${index} ${m.name} joined ${NATIONS[m.civ]} (${m.hosted})`);
  }
  writeState(state);
  return state;
}

/** Fund a local member with test USDC and register it. The crank pays SOL fees. Returns the member index. */
export async function registerWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ, name, kind, stand = 0, votes, deposit = 0n }) {
  if (!(await base.getAccountInfo(token.publicKey))) {
    await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: token.publicKey, mint: mint.publicKey, owner: wallet.publicKey }), [crank, token], 'create USDC account');
  }
  if ((await tokenBalance(base, token.publicKey)) < cfg.entryFee + BigInt(deposit)) {
    await send(base, [mintToIx({ mint: mint.publicKey, dest: token.publicKey, authority: admin.publicKey, amount: 100n * USDC })], [crank, admin], 'mint test USDC');
  }
  await send(base, chain.register({ wallet: wallet.publicKey, feePayer: crank.publicKey, civ, walletToken: token.publicKey, mint: mint.publicKey,
    name, kind, session: session.publicKey, stand, votes, deposit }), [crank, wallet], `register ${name}`);
  return decodeMember((await base.getAccountInfo(chain.member(wallet.publicKey), 'confirmed')).data).index;
}

/** Every Member PDA of the season, in registration order (hosted or not). */
export async function seasonMembers(base, chain) {
  const id = Buffer.alloc(8);
  id.writeBigUInt64LE(chain.seasonId);
  const accounts = await base.getProgramAccounts(chain.programId, {
    commitment: 'confirmed',
    filters: [{ memcmp: { offset: 0, bytes: Buffer.from('PSMEMBR5').toString('base64'), encoding: 'base64' } },
      { memcmp: { offset: 8, bytes: id.toString('base64'), encoding: 'base64' } }],
  });
  return accounts.map(a => ({ pubkey: a.pubkey, ...decodeMember(a.account.data) })).sort((a, b) => a.index - b.index);
}

/**
 * Close registration: genesis, seat every member, hold the first election,
 * delegate (skipped with `delegate: false`, to play on the base layer).
 */
export async function startAndDelegate({ base, er, cfg, state, log = console.log, delegate = true }) {
  const crank = namedKey('crank');
  const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
  const season = () => base.getAccountInfo(chain.season, 'confirmed').then(a => decodeSeason(a.data));
  if ((await season()).status === 'Registering') await send(base, chain.startSeason({ authority: crank.publicKey }), [crank], 'startSeason');
  let steps = 0;
  while ((await season()).status === 'Genesis') {
    const r = await send(base, chain.genesisStep({ work: 50 }), [crank], 'genesisStep');
    steps++;
    for (const rec of r.records) if (rec.tag === 'PS_GENESIS') state.genesis = { root: Buffer.from(rec.root).toString('hex'), seasonSeed: Buffer.from(rec.seasonSeed).toString('hex'), signature: r.signature };
  }
  log(`genesis complete on chain in ${steps} steps`);
  const s = await season();
  if (s.status === 'Seating') {
    const members = await seasonMembers(base, chain);
    for (let i = s.seated; i < members.length; i += SEAT_BATCH) {
      const group = members.slice(i, i + SEAT_BATCH);
      const r = await send(base, chain.seatMembers({ authority: crank.publicKey, members: group.map(m => m.pubkey) }), [crank], `seat members ${i}..${i + group.length - 1}`);
      for (const rec of r.records.filter(x => x.tag === 'PS_SEAT')) state.seating.push({ signature: r.signature, root: Buffer.from(rec.root).toString('hex'), members: Buffer.from(rec.members).toString('hex') });
      writeState(state);
    }
    const r = await send(base, chain.openGovernment({ authority: crank.publicKey, nations: s.nations }), [crank], 'openGovernment');
    for (const rec of r.records.filter(x => x.tag === 'PS_OPEN')) state.open = { signature: r.signature, root: Buffer.from(rec.root).toString('hex') };
    log(`${members.length} members seated; first election held on chain`);
  }
  if (!delegate) return state;
  const targets = [...chain.worldChunks.map((_, k) => k), ...Array.from({ length: s.nations }, (_, c) => NATION_TARGET + c)];
  for (const target of targets) {
    await send(base, chain.delegate({ authority: crank.publicKey, target, validator: cfg.erValidator }), [crank], `delegate ${target}`);
  }
  for (let i = 0; i < 60; i++) {
    const w = await er.getAccountInfo(chain.world, 'confirmed').catch(() => null);
    if (w && w.owner.equals(new PublicKey(cfg.programId))) break;
    await new Promise(r => setTimeout(r, 500));
  }
  state.delegated = true;
  state.delegatedAt = Date.now();
  writeState(state);
  log(`world and ${s.nations} nation accounts delegated to the ER`);
  return state;
}

/** World seed of a season: public and fixed at creation (terrain, §2.4). */
export function worldSeedFor(seasonId) {
  const b = new Uint8Array(32);
  new DataView(b.buffer).setBigUint64(0, BigInt(seasonId), true);
  b.set(new TextEncoder().encode('PS-world'), 8);
  return b;
}
