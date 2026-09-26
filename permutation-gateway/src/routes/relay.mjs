// Signing and relaying for members.
//
//   POST /submit       (operator only) {civ, role, member, tick, digest, orders, adopt}: an office's batch,
//                      sealed (commit–reveal): the gateway draws a salt, sends the commitment
//                      signed with the AI member's session key, and keeps the batch; the
//                      crank reveals it once the tick's commitments close. A vacant office
//                      takes no batch from anyone (the rules' caretaker fills it)
//   POST /gov          (operator only) {member, action} or {member, actions: [...]}: an AI member's
//                      governance actions (at most MAX_GOV_PER_SIGNER), in as few
//                      transactions as fit one packet each
//   GET  /relay        {feePayer, blockhash, lastValidBlockHeight, programId, endpoint?}: for members that
//                      sign their own transactions (an ER blockhash). `endpoint` (the ER RPC) on the
//                      operator listener; on the public one only `--public-er-rpc`, if given
//   POST /relay        {tx, lastValidBlockHeight?}: a member-signed transaction on the ER, exactly
//                      [requestHeapFrame(131072), CommitOrders] | [heap, SubmitGov × 1..8] |
//                      [heap, RevealOrders], every program instruction signed by the same key (not
//                      the gateway's) for a nation of this season; the gateway only adds the fee payer
//                      → {ok, signature}. At most 8 CommitOrders per office, tick and signer
//   GET  /claim-relay  ?season=<id> (default: this season; else one of its lineage)
//                      {feePayer, blockhash, lastValidBlockHeight, programId, mint, status, season}
//   POST /claim-relay  {tx, lastValidBlockHeight?}: a wallet-signed Claim on the base layer for this
//                      season or one in its lineage, optionally after creating the wallet's associated
//                      token account (payer: the gateway) it pays into → {ok, signature}
//
// Every co-signed transaction is parsed exactly, its member signature
// verified, and the exact bytes simulated with signatures checked before
// the gateway signs anything it sends (cosign.mjs). A signer's rate limit
// (SIGNER_LIMITS) is charged only for a transaction that signer really
// signed and that was not charged before (a replay of it, e.g. copied from
// the chain, is 409 Duplicate), so nobody can use up someone else's limit.
// Refusals: 400 InvalidTransaction | RelayRejected | BadSignature, 409
// Duplicate, 429 TooManyCommits | RateLimited, 404 NoSuchMember |
// UnknownSeason, and what the simulation says (the program error by name,
// BlockhashExpired, InsufficientFunds…).
import { PublicKey, Transaction } from '@solana/web3.js';
import { decodeMember, IX_TAG, MAX_GOV_PER_SIGNER, NATIONS, NOBODY } from '../../client/src/codec.mjs';
import { encode as base58 } from '../../client/src/base58.mjs';
import { equal, fromHex, toHex } from '../../client/src/bytes.mjs';
import { allowedOffices } from '../../client/src/offices.mjs';
import { ASSOCIATED_TOKEN_PROGRAM, ata, SUBMIT_HEAP_BYTES, SYSTEM_PROGRAM, TOKEN_PROGRAM } from '../../client/src/player.mjs';
import { COMPUTE_BUDGET_PROGRAM, computeBudgetHeapFrame } from '../../client/src/solana-tx.mjs';
import { coSign, parseWire, signedBy, simulateOrRefuse } from '../cosign.mjs';
import { SIGNER_LIMITS } from '../guards.mjs';
import { seal } from '../sealed.mjs';
import { send, sendWire } from '../send.mjs';
import { RouteError, routeSeason } from './errors.mjs';
import { requireOperator } from './roster.mjs';
import { endpointsFor } from './season.mjs';

/** The one compute-budget instruction a relayed transaction carries (chain.mjs / player.mjs put it first). */
const HEAP_FRAME = computeBudgetHeapFrame(SUBMIT_HEAP_BYTES);
const isHeapFrame = ix => ix.programId === COMPUTE_BUDGET_PROGRAM && ix.keys.length === 0 && equal(ix.data, HEAP_FRAME.data);

/**
 * What POST /relay accepts, for `tx` parsed (solana-tx.mjs; keys base58):
 * exactly two signers, the gateway (`crank`, fee payer) first and the
 * member's key; `[heap frame, CommitOrders]`, `[heap frame, SubmitGov × 1..8]`
 * or `[heap frame, RevealOrders]`, each program instruction taking exactly
 * that key (read-only signer) and a nation account of this season
 * (`nations`: base58 → civ). Returns `{kind, signer, commit?: {civ, role,
 * tick}}` or `{problem}`.
 */
export function relayShape(tx, { programId, crank, nations }) {
  if (tx.signers.length !== 2 || tx.signers[0] !== crank) return { problem: 'two signers: the gateway (fee payer, first) and the member\'s key' };
  const signer = tx.signers[1];
  const [first, ...rest] = tx.instructions;
  if (!first || !isHeapFrame(first)) return { problem: `the first instruction must be requestHeapFrame(${SUBMIT_HEAP_BYTES})` };
  if (!rest.length || rest.some(ix => ix.programId !== programId)) return { problem: 'after the heap frame only program instructions' };
  const tags = rest.map(ix => ix.data[0]);
  const kind = tags.every(t => t === IX_TAG.submitGov) && rest.length <= MAX_GOV_PER_SIGNER ? 'gov'
    : rest.length === 1 && tags[0] === IX_TAG.commitOrders ? 'commit'
      : rest.length === 1 && tags[0] === IX_TAG.revealOrders ? 'reveal' : null;
  if (!kind) return { problem: `one CommitOrders, one RevealOrders or 1–${MAX_GOV_PER_SIGNER} SubmitGov` };
  for (const ix of rest) {
    const [who, nation] = ix.keys;
    if (ix.keys.length !== 2 || who.pubkey !== signer || !who.isSigner || who.isWritable || !nations.has(nation.pubkey) || !nation.isWritable) {
      return { problem: 'each instruction takes the signing key, then a nation account of this season' };
    }
  }
  if (kind !== 'commit') return { kind, signer };
  const d = rest[0].data;
  if (d.length !== 36) return { problem: 'malformed CommitOrders' };
  return { kind, signer, commit: { civ: nations.get(rest[0].keys[1].pubkey), role: d[1], tick: d[2] | (d[3] << 8) } };
}

/** CommitOrders relayed per (office, tick, signer): at most `max`; ticks older than the newest two are forgotten. */
export class CommitCounter {
  constructor(max = 8) {
    this.max = max;
    this.byTick = new Map(); // tick → Map(key → n)
  }

  key({ civ, role, tick }, signer) { return `${civ}:${role}:${signer}`; }
  full(c, signer) { return (this.byTick.get(c.tick)?.get(this.key(c, signer)) ?? 0) >= this.max; }
  add(c, signer) {
    if (!this.byTick.has(c.tick)) this.byTick.set(c.tick, new Map());
    const m = this.byTick.get(c.tick);
    m.set(this.key(c, signer), (m.get(this.key(c, signer)) ?? 0) + 1);
    const newest = Math.max(...this.byTick.keys());
    for (const t of this.byTick.keys()) if (t < newest - 1) this.byTick.delete(t);
  }
}

/**
 * What POST /claim-relay accepts, `tx` parsed: exactly two signers, the
 * gateway (fee payer) and the wallet; one Claim, optionally after creating
 * the wallet's associated token account for the season's mint with the
 * gateway paying, in which case the Claim pays into that account. `seasons`:
 * season address (base58) → ChainClient (this season and its lineage).
 * Returns `{wallet, target (ChainClient), claim (instruction), ata?}` or `{problem}`.
 */
export function claimShape(tx, { programId, crank, seasons }) {
  if (tx.signers.length !== 2 || tx.signers[0] !== crank) return { problem: 'two signers: the gateway (fee payer, first) and the wallet' };
  const wallet = tx.signers[1];
  const ixs = tx.instructions;
  const claim = ixs.at(-1);
  if (!(ixs.length === 1 || ixs.length === 2) || claim.programId !== programId || claim.data.length !== 1 || claim.data[0] !== IX_TAG.claim) {
    return { problem: 'exactly one Claim, optionally after creating the wallet\'s associated token account' };
  }
  const k = claim.keys.map(x => x.pubkey);
  const target = seasons.get(k[1]);
  if (!target) return { problem: 'a Claim for this season or one it follows' };
  if (claim.keys.length !== 7 || k[0] !== wallet || k[2] !== target.member(new PublicKey(wallet)).toBase58() || k[3] !== target.vault.toBase58() || k[6] !== TOKEN_PROGRAM) {
    return { problem: 'Claim takes the wallet, the season, its member account, the vault, the destination, the mint and the token program' };
  }
  if (ixs.length === 1) return { wallet, target, claim };
  const create = ixs[0];
  const mint = k[5];
  const want = [crank, ata(wallet, mint), wallet, mint, SYSTEM_PROGRAM, TOKEN_PROGRAM];
  if (create.programId !== ASSOCIATED_TOKEN_PROGRAM || create.data.length !== 1 || create.data[0] !== 1 || create.keys.length !== 6
    || create.keys.some((x, i) => x.pubkey !== want[i]) || k[4] !== want[1]) {
    return { problem: 'the account created must be the wallet\'s associated token account of the season\'s mint, paid by the gateway, and the Claim must pay into it' };
  }
  return { wallet, target, claim, ata: want[1] };
}

/** A Solana transaction's size limit (one packet). */
export const PACKET_BYTES = 1232;

/** Serialized size of a transaction of `ixs`, paid by `feePayer`, with `signers` signatures. */
export function txSize(ixs, feePayer, signers) {
  const tx = new Transaction({ feePayer, recentBlockhash: PublicKey.default.toBase58() }).add(...ixs);
  return tx.serializeMessage().length + 1 + 64 * signers;
}

/**
 * Split `groups` (instruction lists that must stay together) into as few
 * transactions as fit one packet each, in order. `prefix` (compute budget)
 * goes in front of every transaction.
 */
export function packTransactions(groups, { prefix = [], feePayer, signers }) {
  const out = [];
  let cur = [];
  for (const g of groups) {
    if (cur.length && txSize([...prefix, ...cur, ...g], feePayer, signers) > PACKET_BYTES) {
      out.push([...prefix, ...cur]);
      cur = [];
    }
    cur.push(...g);
  }
  if (cur.length) out.push([...prefix, ...cur]);
  return out;
}

const NOT_HOSTED = 'this member is not one of this gateway\'s AI members; sign it yourself and use /relay';

/**
 * Charge `signer`'s bucket for `tx` (its signature already verified), once
 * per member signature: a transaction charged before (a retry of the same
 * bytes, or a replay copied from the chain) is 409 Duplicate and costs
 * nothing.
 */
function chargeSigner({ limiter, relayed }, tx, signer, bucket, limit, what) {
  const key = `${bucket}:${base58(tx.signatures[tx.signers.indexOf(signer)])}`;
  relayed.refuseRepeat(key);
  limiter.check(`${bucket}:${signer}`, limit, what);
  relayed.add(key);
}

export const relayRoutes = {
  // Only the operator's game server may act for the operator's AI members
  // (it runs them; their keys are this gateway's).
  'POST /submit': async (ctx, req) => {
    requireOperator(ctx, req);
    const { er, chain, crank, hostedKey } = ctx;
    const b = await req.json();
    const member = b.member ?? NOBODY;
    // A vacant office is filled by the rules' caretaker; nobody signs for it.
    if (member === NOBODY) throw new RouteError(403, 'a vacant office takes no batch (the rules fill it)', 'VacantOffice');
    const signer = hostedKey(member);
    if (!signer) throw new RouteError(403, NOT_HOSTED, 'NotHosted');
    // What the program checks only at the reveal is checked now, while the
    // batch can still be fixed: a sealed rationale and orders of this office.
    const digest = fromHex(b.digest || '');
    if (digest.length !== 32 || digest.every(x => x === 0)) throw new RouteError(400, 'digest: 32 bytes, not all zero (officers seal a rationale)', 'MissingRationale');
    const orders = b.orders || [];
    const wrong = orders.find(o => !allowedOffices(o).includes(b.role));
    if (wrong) throw new RouteError(400, `${wrong.type} is not an order of the ${b.role}`, 'WrongOffice');
    const batch = { civ: b.civ, tick: b.tick, role: b.role, member, decisionDigest: digest, orders, adopt: b.adopt || [] };
    const { salt, commitment } = seal(batch);
    // Kept before it is sent (write-ahead): the crank reveals the kept batch
    // whose commitment is the one that landed on chain.
    crank.sealed.put(batch, salt, commitment);
    const ixs = chain.commitOrders({ signer: signer.publicKey, civ: b.civ, role: b.role, tick: b.tick, commitment });
    const r = await send(er, ixs, [crank.crank, signer], `commit ${NATIONS[b.civ]} ${b.role}`);
    return { body: { ok: true, signature: r.signature, commitment: toHex(commitment) } };
  },

  'POST /gov': async (ctx, req) => {
    requireOperator(ctx, req);
    const { er, chain, crank, hostedKey, registry } = ctx;
    const b = await req.json();
    const signer = hostedKey(b.member);
    if (!signer) throw new RouteError(403, NOT_HOSTED, 'NotHosted');
    const civ = (await registry.list()).find(m => m.index === b.member)?.civ;
    if (civ === undefined) throw new RouteError(404, 'no such member', 'NoSuchMember');
    const actions = Array.isArray(b.actions) ? b.actions : [b.action];
    if (!actions.length || actions.some(a => !a?.type)) throw new RouteError(400, 'action (or actions) required', 'InvalidAction');
    if (actions.length > MAX_GOV_PER_SIGNER) throw new RouteError(400, `at most ${MAX_GOV_PER_SIGNER} actions per member and tick`, 'TooManyActions');
    // One instruction per action; as many per transaction as fit, so a
    // member's votes of a vote window go out in one transaction.
    const all = chain.submitGovMany({ signer: signer.publicKey, civ, member: b.member, actions });
    const prefix = all.slice(0, all.length - actions.length); // compute budget
    const txs = packTransactions(all.slice(prefix.length).map(ix => [ix]), { prefix, feePayer: crank.crank.publicKey, signers: 2 });
    const signatures = [];
    for (const tx of txs) {
      const r = await send(er, tx, [crank.crank, signer], `gov ${actions.map(a => a.type).join('+')} by member ${b.member}`);
      signatures.push(r.signature);
    }
    return { body: { ok: true, signature: signatures[0], signatures } };
  },

  'GET /relay': async ({ cfg, crank, blockhashes }, req) => {
    const { blockhash, lastValidBlockHeight } = await blockhashes.er.latest();
    // The operator's own RPC URL may carry an API key: the public listener
    // names only the one it was told to publish (season.mjs endpointsFor).
    const endpoint = endpointsFor(cfg, req?.surface)?.er;
    return { body: { feePayer: crank.crank.publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: cfg.programId, ...(endpoint ? { endpoint } : {}) } };
  },

  'POST /relay': async (ctx, req) => {
    const { er, chain, crank, blockhashes, commits } = ctx;
    const b = await req.json();
    const { tx } = parseWire(b.tx);
    const programId = chain.programId.toBase58();
    const shape = relayShape(tx, { programId, crank: crank.crank.publicKey.toBase58(), nations: await ctx.nationKeys() });
    if (shape.problem) throw new RouteError(400, `relay refused: ${shape.problem}`, 'RelayRejected');
    // Signature first: a forged transaction naming someone's key costs that key nothing.
    if (!signedBy(tx, shape.signer)) throw new RouteError(400, 'the member\'s signature is missing or invalid', 'BadSignature');
    chargeSigner(ctx, tx, shape.signer, 'relay', SIGNER_LIMITS.relay, 'relays for this key');
    if (shape.commit && commits.full(shape.commit, shape.signer)) throw new RouteError(429, `at most ${commits.max} CommitOrders per office and tick`, 'TooManyCommits');
    const signed = coSign(tx, crank.crank);
    await simulateOrRefuse(er, signed, programId);
    if (shape.commit) commits.add(shape.commit, shape.signer);
    const r = await sendWire(er, signed, 'relay', { lastValidBlockHeight: await blockhashes.er.expiryOf(tx.recentBlockhash, b.lastValidBlockHeight), fetch: false });
    return { body: { ok: true, signature: r.signature } };
  },

  // Claims on the base layer, for members without SOL: the member's wallet
  // signs `Claim`, the gateway only pays the fee (and, if asked, the rent of
  // the wallet's associated token account). The program pays only into a
  // token account the wallet owns, so the fee payer cannot redirect it.
  'GET /claim-relay': async (ctx, req) => {
    const { base, cfg, crank, blockhashes } = ctx;
    const which = req.url.searchParams.get('season');
    const target = which ? ctx.claimSeasonById(which) : ctx.chain;
    if (!target) throw new RouteError(404, `season ${which} is neither this season nor one it follows`, 'UnknownSeason');
    const { blockhash, lastValidBlockHeight } = await blockhashes.base.latest();
    const s = await routeSeason({ base, chain: target });
    return { body: { feePayer: crank.crank.publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: cfg.programId, mint: new PublicKey(s.usdcMint).toBase58(),
      status: s.status, season: s.seasonId.toString() } };
  },

  // Checked in this order: the shape, the mint, the wallet's signature,
  // then the wallet's limit (a forged or replayed claim costs the wallet
  // nothing), then its member account (an RPC read), then the simulation.
  'POST /claim-relay': async (ctx, req) => {
    const { base, chain, crank, blockhashes } = ctx;
    const b = await req.json();
    const { tx } = parseWire(b.tx);
    const programId = chain.programId.toBase58();
    const shape = claimShape(tx, { programId, crank: crank.crank.publicKey.toBase58(), seasons: ctx.claimSeasons() });
    if (shape.problem) throw new RouteError(400, `claim relay refused: ${shape.problem}`, 'RelayRejected');
    const s = await routeSeason({ base, chain: shape.target });
    if (shape.claim.keys[5].pubkey !== new PublicKey(s.usdcMint).toBase58()) throw new RouteError(400, 'claim relay refused: not the season\'s mint', 'RelayRejected');
    if (!signedBy(tx, shape.wallet)) throw new RouteError(400, 'the wallet\'s signature is missing or invalid', 'BadSignature');
    chargeSigner(ctx, tx, shape.wallet, 'claim', SIGNER_LIMITS.claim, 'claims from this wallet');
    // Only the member's own wallet claims (read from its member account).
    const account = await base.getAccountInfo(shape.target.member(new PublicKey(shape.wallet)), 'confirmed');
    if (!account) throw new RouteError(404, 'this wallet is not a member of that season', 'NoSuchMember');
    if (new PublicKey(decodeMember(account.data).wallet).toBase58() !== shape.wallet) throw new RouteError(403, 'only the member\'s wallet can claim', 'Unauthorized');
    const signed = coSign(tx, crank.crank);
    await simulateOrRefuse(base, signed, programId);
    const r = await sendWire(base, signed, 'claim relay', { lastValidBlockHeight: await blockhashes.base.expiryOf(tx.recentBlockhash, b.lastValidBlockHeight), fetch: false });
    ctx.claims?.delete(shape.wallet); // GET /claims shows it claimed at once
    return { body: { ok: true, signature: r.signature } };
  },
};
