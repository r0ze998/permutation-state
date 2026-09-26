// Season lifecycle on a local (or devnet) stack, Game Design V5:
// test USDC, a season with one account per nation in NATIONS, the
// registration window (people with their own wallets through x402; the
// operator's AI members at planned times, indistinguishable from them),
// on-chain genesis, seating the members in the world, the first election,
// and delegation of the world chunks and the nation accounts to the ER.
import { randomBytes, randomInt } from 'node:crypto';
import { LAMPORTS_PER_SOL, PublicKey, Transaction } from '@solana/web3.js';
import { DELEGATION_PROGRAM_ID } from '@magicblock-labs/ephemeral-rollups-sdk';
import { fromHex, toHex, u64le } from '../client/src/bytes.mjs';
import { ChainClient, readSeason } from '../client/src/chain.mjs';
import { decodeMember, MAGIC, memberName, NATION_TARGET, NATIONS, NOBODY, ROLES, roleMask, rosterChain, rosterTag } from '../client/src/codec.mjs';
import { poll } from '../client/src/retry.mjs';
import { associatedTokenAccount, createMintIxs, createTokenAccountIxs, mintToIx, tokenBalance } from './spl.mjs';
import { namedKey } from './config.mjs';
import { fundOwner, registrationCost } from './faucet.mjs';
import { send, sendWire } from './send.mjs';

export { NATIONS };
/** Members per SeatMembers: the transaction also carries every world chunk (test/tx-size.test.mjs). */
export const SEAT_BATCH = 6;

/**
 * When the AI members register, as fractions of the registration window,
 * and how long before that each is funded (ms): early enough to look like a
 * person who asked the faucet first, never right before.
 */
export const AI_WINDOW = Object.freeze({ from: 0.05, to: 0.85 });
export const FUND_LEAD_MS = Object.freeze({ max: 180_000, min: 10_000 });

async function airdrop(conn, key, sol = 20) {
  const bal = await conn.getBalance(key, 'confirmed');
  if (bal >= sol * LAMPORTS_PER_SOL / 2) return;
  await conn.confirmTransaction(await conn.requestAirdrop(key, sol * LAMPORTS_PER_SOL), 'confirmed');
}

/** A uniform number in [0, 1) from crypto randomness (who registers when must not be guessable). */
export const unit = () => randomInt(0, 2 ** 47) / 2 ** 47;

/** `list` in a random order (crypto randomness). */
export function shuffled(list, rand = unit) {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** 1 or 2 offices at random (in ROLES order): what an AI member stands for, like a person choosing 1–2. */
export function randomStand(rand = unit) {
  const n = rand() < 0.5 ? 1 : 2;
  const picked = new Set(shuffled(ROLES, rand).slice(0, n));
  return ROLES.filter(r => picked.has(r));
}

/**
 * The operator AI members of a new season (V5 §18.2), drawn before the
 * season is created: `ai` per nation, each with a registration time `dueAt`
 * uniform in [5 %, 85 %] of the `seconds`-long window from `openedAt` (dev
 * mode, `seconds` 0: all at `openedAt`), a funding time `fundAt` uniform in
 * [dueAt − 180 s, dueAt − 10 s] (not before `openedAt`), 1–2 offices, a
 * name from `memberName`, fresh keys, and a secret salt whose tag it
 * registers with. Sorted by `dueAt`: `pos` is the position, the order the
 * roster chain commits and the order they register and are revealed in.
 * Times are ms since the epoch. Returns the entries as the state file keeps
 * them (salt and tag hex, keys base58, key-file stem `key`).
 */
export function planAi({ seasonId, ai, nations = NATIONS.length, seconds, openedAt, keyFor = namedKey, rand = unit, random = n => new Uint8Array(randomBytes(n)) }) {
  const draws = [];
  for (let c = 0; c < nations; c++) {
    for (let j = 0; j < ai; j++) {
      const dueAt = seconds ? openedAt + Math.round((AI_WINDOW.from + (AI_WINDOW.to - AI_WINDOW.from) * rand()) * seconds * 1000) : openedAt;
      draws.push({ civ: c, dueAt, fundAt: fundTime({ dueAt, from: openedAt, rand }) });
    }
  }
  // Ties (dev mode: all at once) in a random order, not by nation.
  const order = shuffled(draws, rand).sort((a, b) => a.dueAt - b.dueAt);
  return order.map((d, pos) => {
    const key = `s${seasonId}-ai${pos}`;
    const wallet = keyFor(`${key}-wallet`), session = keyFor(`${key}-session`);
    const salt = random(32);
    return {
      pos, civ: d.civ, name: memberName(random(32)), stand: randomStand(rand), key,
      wallet: wallet.publicKey.toBase58(), session: session.publicKey.toBase58(),
      salt: toHex(salt), tag: toHex(rosterTag(seasonId, wallet.publicKey.toBytes(), salt)), dueAt: d.dueAt, fundAt: d.fundAt,
    };
  });
}

/** A funding time for an AI member due at `dueAt`: in [dueAt − 180 s, dueAt − 10 s], not before `from`. */
function fundTime({ dueAt, from, rand }) {
  const lo = Math.max(from, dueAt - FUND_LEAD_MS.max);
  const hi = Math.max(lo, dueAt - FUND_LEAD_MS.min);
  return Math.round(lo + (hi - lo) * rand());
}

/** The roster chain the season commits: the AI members' tags in `pos` order. */
export const planChain = plan => rosterChain([...plan].sort((a, b) => a.pos - b.pos).map(e => fromHex(e.tag)));

/** An AI member registers this late (ms) before the rest of the overdue ones are spread out again. */
export const RESPREAD_LATE_MS = 20_000;

/**
 * Spread the plan's overdue, unregistered AI members (`dueAt` before `now`)
 * over what is left of the window (a restart, an outage): new times from
 * `now` + 3 s to the window's 85 % point (or, past it, to 10 s before it
 * closes), then every unregistered member's time re-sorted in `pos` order so
 * they still register strictly in that order, and new funding times for
 * those not funded yet. Mutates `plan`; returns how many were overdue.
 */
export function respread(plan, { now, openedAt, seconds, closesAt, rand = unit }) {
  const pending = plan.filter(e => e.registered === undefined).sort((a, b) => a.pos - b.pos);
  const overdue = pending.filter(e => e.dueAt < now);
  if (!overdue.length || !seconds) return 0;
  const lo = now + 3000;
  let hi = openedAt + AI_WINDOW.to * seconds * 1000;
  if (hi < lo + 10_000) hi = Math.max(lo, (closesAt ?? lo) - 10_000);
  for (const e of overdue) e.dueAt = Math.round(lo + (hi - lo) * rand());
  const times = pending.map(e => e.dueAt).sort((a, b) => a - b);
  pending.forEach((e, i) => { e.dueAt = times[i]; });
  for (const e of pending) if (!e.funded) e.fundAt = fundTime({ dueAt: e.dueAt, from: now, rand });
  return overdue.length;
}

/**
 * The registration window of a season state: `{seconds, openedAt, closesAt,
 * waitExternal, entryFee, deposit}` (times in ms since the epoch; `closesAt`
 * null in dev mode; fee and deposit bigints). A state written before
 * registration windows (no `registration`) behaves as dev mode from
 * `fallbackOpenedAt`.
 */
export function registrationOf(state, { fallbackOpenedAt = Date.now(), entryFee = null } = {}) {
  const r = state.registration;
  if (!r) return { seconds: 0, openedAt: fallbackOpenedAt, closesAt: null, waitExternal: 0, entryFee: entryFee === null ? null : BigInt(entryFee), deposit: 0n };
  return { seconds: r.seconds, openedAt: r.openedAt, closesAt: r.closesAt ?? null, waitExternal: r.waitExternal ?? 0,
    entryFee: BigInt(r.entryFee ?? entryFee ?? 0), deposit: BigInt(r.deposit ?? 0) };
}

/** AI members planned but not registered yet (never in `members`, the operator roster or /season). */
export const pendingAi = state => (state.aiPlan ?? []).filter(e => e.registered === undefined).length;

/**
 * Whether people may register now: the season account is Registering, the
 * gateway has finished creating it and not started it, and the window has
 * not closed.
 */
export function registrationOpen({ state, season, phase, now }) {
  const { closesAt } = registrationOf(state, { fallbackOpenedAt: now });
  return season.status === 'Registering' && !state.creating && phase === 'registering' && (closesAt === null || now < closesAt);
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
    await send(base, await createTokenAccountIxs(base, { payer: admin.publicKey, account: t.publicKey, mint, owner: admin.publicKey }), [admin, t], 'create admin USDC account');
  }
  const have = await tokenBalance(base, t.publicKey);
  if (have < need) await send(base, [mintToIx({ mint, dest: t.publicKey, authority: admin.publicKey, amount: need - have })], [admin], 'mint bounty and bond');
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
 * Create a season and open its registration; the state is saved to `store`.
 * The AI members' plan (keys, salts, times) and the window are saved before
 * the season exists (its roster chain is committed at creation, so the salts
 * must survive a crash); creating is resumable (`createSeasonAccounts`).
 * With a window (`cfg.registrationSeconds` > 0) the crank registers the AI
 * members at their planned times; in dev mode (0) they register here, at
 * once, in plan order.
 */
export async function bootstrap({ base, cfg, store, log = console.log, now = Date.now }) {
  const admin = namedKey('admin');
  const crank = namedKey('crank');
  const mint = namedKey('usdc-mint');
  if (cfg.cluster === 'localnet') {
    await airdrop(base, admin.publicKey, 50);
    await airdrop(base, crank.publicKey, 50);
  }
  await ensureMint({ base, cfg, admin, mint, log });
  const seasonId = BigInt(now());
  // The history layer: follow --prev-season, else the finalized season this
  // state file held before (the same world's previous season).
  const prevSeasonId = cfg.prevSeason ? BigInt(cfg.prevSeason) : (store.state?.finalized && store.state?.programId === cfg.programId ? BigInt(store.state.seasonId) : null);
  // The lineage is known when the previous season is the one this state file held.
  const lineage = lineageOf(prevSeasonId && store.state?.seasonId === prevSeasonId.toString() ? store.state : null);
  const seconds = cfg.registrationSeconds;
  const openedAt = now();
  const aiPlan = planAi({ seasonId, ai: cfg.ai, nations: NATIONS.length, seconds, openedAt });
  const { bountyEach, bond } = bountyAndBond({ aiCount: aiPlan.length, bounty: cfg.bounty, bond: cfg.bond, entryFee: cfg.entryFee });
  store.save({ seasonId: seasonId.toString(), programId: cfg.programId, mint: mint.publicKey.toBase58(), cluster: cfg.cluster,
    nations: [...NATIONS], members: [], ticks: [], seating: [], lineage,
    roster: { aiCount: aiPlan.length, bountyEach: bountyEach.toString(), bond: bond.toString(), revealed: false },
    registration: { seconds, openedAt, closesAt: seconds ? openedAt + seconds * 1000 : null, waitExternal: cfg.waitExternal,
      entryFee: cfg.entryFee.toString(), deposit: cfg.deposit.toString() },
    aiPlan, faucet: {}, creating: { prevSeasonId: prevSeasonId?.toString() ?? null, tickSeconds: cfg.tickSeconds, market: cfg.market } });
  await createSeasonAccounts({ base, cfg, store, log, now });
  if (!seconds) {
    for (const entry of aiPlan) await registerPlannedNow({ base, cfg, store, entry, log });
  }
  return store.state;
}

/**
 * The on-chain side of creating the season in `store.state.creating`
 * (resumable: what exists is skipped): CreateSeason with the committed
 * roster chain, the world chunks and the nation accounts. Then the window
 * and the plan's times are moved to start now, so people get the whole
 * window once the season exists.
 */
export async function createSeasonAccounts({ base, cfg, store, log = console.log, now = Date.now }) {
  const state = store.state;
  if (!state?.creating) return state;
  const admin = namedKey('admin');
  const crank = namedKey('crank');
  const mint = new PublicKey(state.mint);
  const seasonId = BigInt(state.seasonId);
  const chain = new ChainClient(cfg.programId, seasonId);
  const aiCount = state.aiPlan?.length ?? 0;
  const bountyEach = BigInt(state.roster?.bountyEach ?? 0), bond = BigInt(state.roster?.bond ?? 0);
  const prevSeasonId = state.creating.prevSeasonId ? BigInt(state.creating.prevSeasonId) : null;
  if (!(await base.getAccountInfo(chain.season, 'confirmed'))) {
    // The operator escrows the bounties and its bond (test USDC here).
    const adminToken = aiCount ? await escrowBountyAndBond({ base, admin, mint, need: bountyEach * BigInt(aiCount) + bond }) : undefined;
    await send(base, chain.createSeason({ admin: admin.publicKey, mint, nations: state.nations.length, entryFee: BigInt(state.registration.entryFee),
      tickSeconds: state.creating.tickSeconds ?? cfg.tickSeconds, worldSeed: worldSeedFor(seasonId), crank: crank.publicKey,
      market: state.creating.market ?? cfg.market, prevSeasonId,
      aiCount, rosterChain: planChain(state.aiPlan ?? []), bountyEach, bond, adminToken }), [admin], 'createSeason');
    if (aiCount) log(`season ${seasonId}: ${aiCount} operator AI members committed (bounty ${bountyEach} each, bond ${bond}); revealed after the season`);
    if (prevSeasonId) log(`season ${seasonId} follows season ${prevSeasonId} (history layer)`);
  }
  const nations = state.nations.length;
  const have = await base.getMultipleAccountsInfo([...chain.worldChunks, ...chain.nations(nations)], 'confirmed');
  for (let k = 0; k < chain.worldChunks.length; k++) {
    if (!have[k]) await send(base, chain.allocWorld({ payer: crank.publicKey, chunk: k }), [crank], `allocWorld ${k}`);
  }
  for (let c = 0; c < nations; c++) {
    if (!have[chain.worldChunks.length + c]) await send(base, chain.allocNation({ payer: crank.publicKey, civ: c }), [crank], `allocNation ${c}`);
  }
  // The window starts once the season can be joined.
  const shift = Math.max(0, now() - state.registration.openedAt);
  state.registration.openedAt += shift;
  if (state.registration.closesAt !== null) state.registration.closesAt += shift;
  for (const e of state.aiPlan ?? []) { e.dueAt += shift; e.fundAt += shift; }
  delete state.creating;
  store.save();
  const r = state.registration;
  log(`season ${seasonId} created: world and ${nations} nation accounts allocated; registration ${r.closesAt === null ? 'open until the dev-mode start' : `open until ${new Date(r.closesAt).toISOString()}`}`);
  return state;
}

/**
 * The key file of a gateway-held member's `part` ('wallet', 'session', 'usdc').
 * Seasons before program version 7 numbered them `member<i>-…`; then each
 * season had its own (`s<season>-m<i>-…`), and AI members now `s<season>-ai<pos>-…`.
 */
export const memberKeyName = (m, part) => (typeof m.key === 'number' ? `member${m.key}-${part}` : `${m.key}-${part}`);

/**
 * The operator AI members of this gateway's season that registered, in
 * roster order (`pos`, the order the roster chain committed; seasons before
 * the plan kept none and registered in that order), with their salts.
 */
export const aiMembers = state => (state.members ?? []).filter(m => m.hosted === 'ai' && m.salt)
  .sort((a, b) => (a.pos ?? a.index) - (b.pos ?? b.index));

/**
 * The members whose wallet key this gateway holds, so only it can claim for
 * them: the operator's AI members, and in state files from before people
 * joined with their own wallets, the gateway-held human seats (`hosted:
 * 'human'`). Members who joined with their own wallets ('external') claim
 * themselves.
 */
export const gatewayHeldMembers = state => (state.members ?? []).filter(m => m.hosted !== 'external' && m.key !== undefined && m.key !== null);

/** The funding an AI member plan entry gets: the faucet's, like anyone's. Marks the entry funded. */
export async function fundPlanned({ base, store, entry, amount, keys = namedKey }) {
  const wallet = keys(`${entry.key}-wallet`);
  const r = await fundOwner({ base, store, owner: wallet.publicKey, amount, keys, ai: true });
  entry.funded = { account: r.account.toBase58(), signature: r.signature };
  store.save();
  return entry.funded;
}

/**
 * Register one planned AI member with a blockhash fetched earlier
 * (`blockhash` = {blockhash, lastValidBlockHeight}): kind 2, votes nobody,
 * the public default deposit, its roster tag, paid from its associated
 * token account; the crank pays the fee and the rent, as for anyone joining
 * through x402. Idempotent: a member account that already exists (a send
 * whose confirmation was lost, a restart) is recorded instead. On success
 * the member joins `state.members` (with its `pos`) and the entry records
 * its index. Returns the member index.
 */
export async function registerPlanned({ base, cfg, store, entry, blockhash, keys = namedKey, log = console.log, recheck = { attempts: 4, delayMs: 1000 } }) {
  const state = store.state;
  const crank = keys('crank');
  const wallet = keys(`${entry.key}-wallet`), session = keys(`${entry.key}-session`);
  const chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
  const mint = new PublicKey(state.mint);
  const walletToken = associatedTokenAccount(wallet.publicKey, mint);
  const pda = chain.member(wallet.publicKey);
  let info = await base.getAccountInfo(pda, 'confirmed');
  if (!info) {
    const { deposit } = registrationOf(state);
    const tx = new Transaction().add(...chain.register({ wallet: wallet.publicKey, feePayer: crank.publicKey, civ: entry.civ, walletToken, mint,
      name: entry.name, kind: 2, session: session.publicKey, stand: roleMask(entry.stand), votes: [NOBODY, NOBODY, NOBODY, NOBODY], deposit, tag: fromHex(entry.tag) }));
    tx.feePayer = crank.publicKey;
    tx.recentBlockhash = blockhash.blockhash;
    tx.sign(crank, wallet);
    try {
      await sendWire(base, tx.serialize(), `register ${entry.name}`, { lastValidBlockHeight: blockhash.lastValidBlockHeight, fetch: false });
    } catch (e) {
      info = await poll(() => base.getAccountInfo(pda, 'confirmed'), recheck);
      if (!info) throw e;
      log(`AI member ${entry.pos}: the confirmation failed (${e.message.slice(0, 80)}), but its member account exists`);
    }
    info ??= await poll(() => base.getAccountInfo(pda, 'confirmed'), { attempts: 6, delayMs: 1000 });
    if (!info) throw new Error(`AI member ${entry.pos} registered but its member account cannot be read`);
  }
  const m = decodeMember(info.data);
  entry.registered = m.index;
  // `hosted`, the salt and `pos` stay in this (private) state file.
  if (!state.members.some(x => x.index === m.index)) {
    state.members.push({ index: m.index, civ: entry.civ, name: entry.name, kind: 2, hosted: 'ai', key: entry.key, wallet: entry.wallet, session: entry.session,
      usdc: walletToken.toBase58(), salt: entry.salt, pos: entry.pos });
  }
  store.save();
  return m.index;
}

/** Fund (if needed) and register a planned AI member right away with a fresh blockhash (dev mode, scripts). */
export async function registerPlannedNow({ base, cfg, store, entry, log = console.log }) {
  if (entry.registered !== undefined) return entry.registered;
  const { entryFee, deposit } = registrationOf(store.state, { entryFee: cfg.entryFee });
  if (!entry.funded) await fundPlanned({ base, store, entry, amount: registrationCost(entryFee, deposit) });
  const index = await registerPlanned({ base, cfg, store, entry, blockhash: await base.getLatestBlockhash('confirmed'), log });
  log(`member ${index} ${entry.name} joined ${NATIONS[entry.civ]}`);
  return index;
}

/** Every Member PDA of the season, in registration order (anyone's). */
export async function seasonMembers(base, chain) {
  const id = Buffer.from(u64le(chain.seasonId));
  const accounts = await base.getProgramAccounts(chain.programId, {
    commitment: 'confirmed',
    filters: [{ memcmp: { offset: 0, bytes: Buffer.from(MAGIC.member).toString('base64'), encoding: 'base64' } },
      { memcmp: { offset: 8, bytes: id.toString('base64'), encoding: 'base64' } }],
  });
  return accounts.map(a => ({ pubkey: a.pubkey, ...decodeMember(a.account.data) })).sort((a, b) => a.index - b.index);
}

/** Members that registered one session key between them: [{session (base58), members: [index…]}]. */
export function duplicateSessions(members) {
  const by = new Map();
  for (const m of members) {
    const k = new PublicKey(m.session).toBase58();
    by.set(k, [...(by.get(k) ?? []), m.index]);
  }
  return [...by].filter(([, list]) => list.length > 1).map(([session, list]) => ({ session, members: list }));
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
  if ((await season()).status === 'Registering') {
    // Two members with one session key: the program seats the later ones
    // with a substitute key (they cannot act this season). Say so loudly.
    for (const d of duplicateSessions(await seasonMembers(base, chain))) {
      log(`WARNING: members ${d.members.join(', ')} registered the same session key ${d.session}; only member ${d.members[0]} can use it (the others are seated with an unusable key)`);
    }
    await send(base, chain.startSeason({ authority: crank.publicKey }), [crank], 'startSeason');
  }
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
