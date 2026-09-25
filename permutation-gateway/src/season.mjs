// Season lifecycle on a local (or devnet) stack, Game Design V5:
// test USDC, a season with one account per nation in NATIONS, members
// registering (hosted ones here, outside agents through x402), on-chain
// genesis, seating the members in the world, the first election, and
// delegation of the world chunks and the nation accounts to the ER.
import { randomBytes, randomInt } from 'node:crypto';
import { LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js';
import { DELEGATION_PROGRAM_ID } from '@magicblock-labs/ephemeral-rollups-sdk';
import { toHex, u64le } from '../client/src/bytes.mjs';
import { ChainClient, readSeason } from '../client/src/chain.mjs';
import { decodeMember, MAGIC, NATION_TARGET, NATIONS, NOBODY, ROLES, roleMask, rosterChain, rosterTag } from '../client/src/codec.mjs';
import { poll } from '../client/src/retry.mjs';
import { createMintIxs, createTokenAccountIxs, mintToIx, tokenBalance } from './spl.mjs';
import { namedKey } from './config.mjs';
import { send } from './send.mjs';

export { NATIONS };
const USDC = 1_000_000n;
/** Members per SeatMembers: the transaction also carries every world chunk (test/tx-size.test.mjs). */
export const SEAT_BATCH = 6;

async function airdrop(conn, key, sol = 20) {
  const bal = await conn.getBalance(key, 'confirmed');
  if (bal >= sol * LAMPORTS_PER_SOL / 2) return;
  await conn.confirmTransaction(await conn.requestAirdrop(key, sol * LAMPORTS_PER_SOL), 'confirmed');
}

/**
 * The members this gateway registers and hosts, as `{civ, hosted, stand}`:
 * `humans` claimable human members (nations 0, 1, …) who stand for general
 * and steward, and `ai` AI members per nation (run by the game server's
 * reference AI) who stand for two offices each, rotating, and vote for a
 * human candidate where there is one. Names and keys are drawn per season
 * (`drawSeats`); no seat declares a kind.
 */
export function defaultRoster({ humans = 1, ai = 2, nations = NATIONS.length } = {}) {
  const roster = [];
  for (let h = 0; h < humans; h++) roster.push({ civ: h % nations, hosted: 'human', stand: ['General', 'Steward'] });
  for (let c = 0; c < nations; c++) {
    for (let j = 0; j < ai; j++) {
      roster.push({ civ: c, hosted: 'ai', stand: [ROLES[(2 * j) % ROLES.length], ROLES[(2 * j + 1) % ROLES.length]] });
    }
  }
  return roster;
}

/**
 * Names for the members this gateway hosts, people and AI alike: while the
 * season is played nobody should tell them apart by name (V5 §18.2).
 */
export const SEAT_NAMES = Object.freeze(['Aoi', 'Ren', 'Mika', 'Sora', 'Yuki', 'Haru', 'Kai', 'Nao', 'Rin', 'Toma', 'Lina', 'Mateo',
  'Ines', 'Otto', 'Freya', 'Iris', 'Lucas', 'Nora', 'Theo', 'Zara', 'Emil', 'Selin', 'Kofi', 'Ada', 'Hugo', 'Maya', 'Omar', 'Lea',
  'Ivan', 'Noor', 'Sami', 'Tess', 'Arlo', 'Wren', 'Jin', 'Suri', 'Pablo', 'Elif', 'Rui', 'Kaia', 'Leon', 'Mira', 'Tomo', 'Yara']);

/** `list` in a random order (crypto randomness: the order must not reveal who is an AI). */
export function shuffled(list) {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = randomInt(i + 1);
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/**
 * Votes of the first election for member `me` of `roster`. Member indices
 * are the roster positions: `bootstrap` registers the roster into a season
 * it has just created, before the gateway accepts outside entrants.
 */
export function firstVotes(roster, me) {
  const m = roster[me];
  return ROLES.map(role => {
    const human = roster.findIndex(x => x.civ === m.civ && x.hosted === 'human' && x.stand.includes(role));
    if (human >= 0) return human;
    return m.stand.includes(role) ? me : NOBODY;
  });
}

/**
 * The hosted seats of season `seasonId`, people and operator AI members
 * alike (V5 §18.2), in the order of `roster` (bootstrap shuffles it): fresh
 * keys every season (`keyFor(name)`), a name from `names` (with a numeric
 * suffix once the pool is used up) and no self-declared kind (2), so
 * nothing but play tells them apart. Each AI gets a secret salt and
 * registers with its roster tag; everyone else with random bytes. Pure given
 * `keyFor` and `random(n)` (n random bytes).
 */
export function drawSeats(roster, seasonId, { names = shuffled(SEAT_NAMES), keyFor = namedKey, random = n => new Uint8Array(randomBytes(n)) } = {}) {
  return roster.map((m, i) => {
    const key = part => keyFor(`s${seasonId}-m${i}-${part}`);
    const keys = { wallet: key('wallet'), session: key('session'), token: key('usdc') };
    const salt = m.hosted === 'ai' ? random(32) : null;
    const tag = salt ? rosterTag(seasonId, keys.wallet.publicKey.toBytes(), salt) : random(32);
    const round = Math.floor(i / names.length);
    return { ...m, name: names[i % names.length] + (round ? ` ${round + 1}` : ''), kind: 2, keys, salt, tag };
  });
}

/** The bounty per operator AI member and the operator's bond (default: AI members × entry fee × 2); none without AI members. */
export function bountyAndBond({ aiCount, bounty, bond = null, entryFee }) {
  if (!aiCount) return { bountyEach: 0n, bond: 0n };
  return { bountyEach: bounty, bond: bond ?? BigInt(aiCount) * entryFee * 2n };
}

/** The gateway's test USDC mint, created on first use. */
async function ensureMint({ base, cfg, admin, mint, log }) {
  if (await base.getAccountInfo(mint.publicKey)) return;
  await send(base, await createMintIxs(base, { payer: admin.publicKey, mint: mint.publicKey, authority: admin.publicKey }), [admin, mint], 'create test USDC mint');
  log(`test USDC mint ${mint.publicKey.toBase58()} on ${cfg.cluster} (created by this gateway; no value, not real USDC)`);
}

/** The admin's USDC account, holding at least `need` (the bounties and the bond, escrowed at creation). Returns its address. */
async function escrowBountyAndBond({ base, admin, mint, need }) {
  const t = namedKey('admin-usdc');
  if (!(await base.getAccountInfo(t.publicKey))) {
    await send(base, await createTokenAccountIxs(base, { payer: admin.publicKey, account: t.publicKey, mint: mint.publicKey, owner: admin.publicKey }), [admin, t], 'create admin USDC account');
  }
  const have = await tokenBalance(base, t.publicKey);
  if (have < need) await send(base, [mintToIx({ mint: mint.publicKey, dest: t.publicKey, authority: admin.publicKey, amount: need - have })], [admin], 'mint bounty and bond');
  return t.publicKey;
}

/**
 * The history layer, readable: the records of the seasons a season follows
 * (the last ten), each checkable against its season's history root — the
 * previous season's own lineage plus its record. `prev` is that season's
 * state (null: none).
 */
export function lineageOf(prev) {
  if (!prev) return [];
  const own = prev.history ? [{ seasonId: prev.seasonId, nations: prev.nations, historyRoot: prev.history.historyRoot, record: prev.history.record }] : [];
  return [...(prev.lineage ?? []), ...own].slice(-10);
}

/**
 * Create a season and register the hosted members; the state is saved to
 * `store`. Registration stays open (for x402 entrants) until the crank
 * starts the season.
 */
export async function bootstrap({ base, cfg, store, roster = defaultRoster(), log = console.log }) {
  const admin = namedKey('admin');
  const crank = namedKey('crank');
  const mint = namedKey('usdc-mint');
  if (cfg.cluster === 'localnet') {
    await airdrop(base, admin.publicKey, 50);
    await airdrop(base, crank.publicKey, 50);
  }
  await ensureMint({ base, cfg, admin, mint, log });
  const seasonId = BigInt(Date.now());
  const chain = new ChainClient(cfg.programId, seasonId);
  // The history layer: follow --prev-season, else the finalized season this
  // state file held before (the same world's previous season).
  const prevSeasonId = cfg.prevSeason ? BigInt(cfg.prevSeason) : (store.state?.finalized && store.state?.programId === cfg.programId ? BigInt(store.state.seasonId) : null);
  // The hosted seats in a random order (see drawSeats). The chain of the
  // AI members' tags is committed now and revealed after the season.
  const seats = drawSeats(shuffled(roster), seasonId);
  const ais = seats.filter(s => s.salt);
  const aiCount = ais.length;
  const { bountyEach, bond } = bountyAndBond({ aiCount, bounty: cfg.bounty, bond: cfg.bond, entryFee: cfg.entryFee });
  // The operator escrows the bounties and its bond (test USDC here).
  const adminToken = aiCount ? await escrowBountyAndBond({ base, admin, mint, need: bountyEach * BigInt(aiCount) + bond }) : undefined;
  await send(base, chain.createSeason({ admin: admin.publicKey, mint: mint.publicKey, nations: NATIONS.length, entryFee: cfg.entryFee,
    tickSeconds: cfg.tickSeconds, worldSeed: worldSeedFor(seasonId), crank: crank.publicKey, market: cfg.market, prevSeasonId,
    aiCount, rosterChain: rosterChain(ais.map(s => s.tag)), bountyEach, bond, adminToken }), [admin], 'createSeason');
  if (aiCount) log(`season ${seasonId}: ${aiCount} operator AI members committed (bounty ${bountyEach} each, bond ${bond}); revealed after the season`);
  if (prevSeasonId) log(`season ${seasonId} follows season ${prevSeasonId} (history layer)`);
  for (let k = 0; k < chain.worldChunks.length; k++) await send(base, chain.allocWorld({ payer: crank.publicKey, chunk: k }), [crank], `allocWorld ${k}`);
  for (let c = 0; c < NATIONS.length; c++) await send(base, chain.allocNation({ payer: crank.publicKey, civ: c }), [crank], `allocNation ${c}`);
  log(`season ${seasonId} created: world and ${NATIONS.length} nation accounts allocated`);

  // The lineage is known when the previous season is the one this state file held.
  const lineage = lineageOf(prevSeasonId && store.state?.seasonId === prevSeasonId.toString() ? store.state : null);
  const state = store.save({ seasonId: seasonId.toString(), programId: cfg.programId, mint: mint.publicKey.toBase58(), cluster: cfg.cluster,
    nations: [...NATIONS], members: [], ticks: [], seating: [], lineage,
    roster: { aiCount, bountyEach: bountyEach.toString(), bond: bond.toString(), revealed: false } });
  for (const [i, m] of seats.entries()) {
    const { wallet, session, token } = m.keys;
    const index = await registerWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ: m.civ, name: m.name, kind: m.kind,
      stand: roleMask(m.stand), votes: firstVotes(seats, i), deposit: m.deposit ?? 0n, tag: m.tag });
    // `hosted` and the salt stay in this (private) state file.
    state.members.push({ index, civ: m.civ, name: m.name, kind: m.kind, hosted: m.hosted, key: `s${seasonId}-m${i}`, wallet: wallet.publicKey.toBase58(),
      session: session.publicKey.toBase58(), usdc: token.publicKey.toBase58(), ...(m.salt ? { salt: toHex(m.salt) } : {}) });
    store.save();
    log(`member ${index} ${m.name} joined ${NATIONS[m.civ]}`);
  }
  return state;
}

/**
 * The key file of a hosted member's `part` ('wallet', 'session', 'usdc').
 * Seasons before program version 7 numbered them `member<i>-…`; now each season has its
 * own (`s<season>-m<i>-…`).
 */
export const memberKeyName = (m, part) => (typeof m.key === 'number' ? `member${m.key}-${part}` : `${m.key}-${part}`);

/** The operator AI members of this gateway's season, in roster (registration) order, with their salts. */
export const aiMembers = state => (state.members ?? []).filter(m => m.salt).sort((a, b) => a.index - b.index);

/** Fund a local member with test USDC and register it. The crank pays SOL fees. Returns the member index. */
export async function registerWithUsdc({ base, chain, cfg, admin, crank, mint, wallet, session, token, civ, name, kind, stand = 0, votes, deposit = 0n, tag }) {
  if (!(await base.getAccountInfo(token.publicKey))) {
    await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: token.publicKey, mint: mint.publicKey, owner: wallet.publicKey }), [crank, token], 'create USDC account');
  }
  if ((await tokenBalance(base, token.publicKey)) < cfg.entryFee + BigInt(deposit)) {
    await send(base, [mintToIx({ mint: mint.publicKey, dest: token.publicKey, authority: admin.publicKey, amount: 100n * USDC })], [crank, admin], 'mint test USDC');
  }
  await send(base, chain.register({ wallet: wallet.publicKey, feePayer: crank.publicKey, civ, walletToken: token.publicKey, mint: mint.publicKey,
    name, kind, session: session.publicKey, stand, votes, deposit, tag }), [crank, wallet], `register ${name}`);
  return decodeMember((await base.getAccountInfo(chain.member(wallet.publicKey), 'confirmed')).data).index;
}

/** Every Member PDA of the season, in registration order (hosted or not). */
export async function seasonMembers(base, chain) {
  const id = Buffer.from(u64le(chain.seasonId));
  const accounts = await base.getProgramAccounts(chain.programId, {
    commitment: 'confirmed',
    filters: [{ memcmp: { offset: 0, bytes: Buffer.from(MAGIC.member).toString('base64'), encoding: 'base64' } },
      { memcmp: { offset: 8, bytes: id.toString('base64'), encoding: 'base64' } }],
  });
  return accounts.map(a => ({ pubkey: a.pubkey, ...decodeMember(a.account.data) })).sort((a, b) => a.index - b.index);
}

/** Every delegation target of a season: the world chunks, then the nation accounts. */
export const delegationTargets = (chain, nations) => [...chain.worldChunks.map((_, k) => k), ...Array.from({ length: nations }, (_, c) => NATION_TARGET + c)];

/**
 * Close registration: genesis, seat every member, hold the first election,
 * delegate (skipped with `delegate: false`, to play on the base layer).
 * Every step is resumable, so a failed attempt can simply be run again.
 * Progress is saved to `store`; returns the (live) state.
 */
export async function startAndDelegate({ base, er, cfg, store, log = console.log, delegate = true, delegationTimeoutMs = 30_000 }) {
  const state = store.state;
  const crank = namedKey('crank');
  const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
  const season = () => readSeason(base, chain);
  if ((await season()).status === 'Registering') await send(base, chain.startSeason({ authority: crank.publicKey }), [crank], 'startSeason');
  let steps = 0;
  while ((await season()).status === 'Genesis') {
    const r = await send(base, chain.genesisStep({ work: 50 }), [crank], 'genesisStep');
    steps++;
    for (const rec of r.records) if (rec.tag === 'PS_GENESIS') state.genesis = { root: toHex(rec.root), seasonSeed: toHex(rec.seasonSeed), signature: r.signature };
  }
  if (steps) {
    store.save();
    log(`genesis complete on chain in ${steps} steps`);
  }
  const s = await season();
  if (s.status === 'Seating') {
    const members = await seasonMembers(base, chain);
    for (let i = s.seated; i < members.length; i += SEAT_BATCH) {
      const group = members.slice(i, i + SEAT_BATCH);
      const r = await send(base, chain.seatMembers({ authority: crank.publicKey, members: group.map(m => m.pubkey) }), [crank], `seat members ${i}..${i + group.length - 1}`);
      for (const rec of r.records.filter(x => x.tag === 'PS_SEAT')) state.seating.push({ signature: r.signature, root: toHex(rec.root), members: toHex(rec.members) });
      store.save();
    }
    const r = await send(base, chain.openGovernment({ authority: crank.publicKey, nations: s.nations }), [crank], 'openGovernment');
    for (const rec of r.records.filter(x => x.tag === 'PS_OPEN')) state.open = { signature: r.signature, root: toHex(rec.root) };
    store.save();
    log(`${members.length} members seated; first election held on chain`);
  }
  if (!delegate) return state;

  const program = new PublicKey(cfg.programId);
  const targets = delegationTargets(chain, s.nations);
  const keys = targets.map(t => chain.target(t));
  // Skip what an earlier attempt already delegated (owned by the delegation program on base).
  const onBase = await base.getMultipleAccountsInfo(keys, 'confirmed');
  for (const [i, target] of targets.entries()) {
    if (onBase[i]?.owner.equals(DELEGATION_PROGRAM_ID)) continue;
    await send(base, chain.delegate({ authority: crank.publicKey, target, validator: cfg.erValidator }), [crank], `delegate ${target}`);
  }
  // The ER must have taken every account over (it clones them with the program as owner).
  const missing = async () => {
    const onEr = await er.getMultipleAccountsInfo(keys, 'confirmed');
    return targets.filter((_, i) => !onEr[i]?.owner.equals(program));
  };
  const ready = await poll(async () => ((await missing()).length ? null : true), { attempts: Math.max(1, Math.ceil(delegationTimeoutMs / 500)), delayMs: 500 });
  if (!ready) throw new Error(`delegation not visible on the ER after ${delegationTimeoutMs / 1000} s for targets ${(await missing()).join(',')}; the crank retries`);
  state.delegated = true;
  state.delegatedAt = Date.now();
  store.save();
  log(`world and ${s.nations} nation accounts delegated to the ER`);
  return state;
}

/** World seed of a season: public and fixed at creation (terrain, §2.4). */
export function worldSeedFor(seasonId) {
  const b = new Uint8Array(32);
  b.set(u64le(seasonId), 0);
  b.set(new TextEncoder().encode('PS-world'), 8);
  return b;
}
