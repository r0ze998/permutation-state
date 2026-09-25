// Signing and relaying for members.
//
//   POST /submit       (operator only) {civ, role, member, tick, digest, orders, adopt}: an office's batch,
//                      sealed (commit–reveal): the gateway draws a salt, sends the commitment
//                      signed with the hosted member's session key, and keeps the batch; the
//                      crank reveals it once the tick's commitments close. A vacant office
//                      takes no batch from anyone (the rules' caretaker fills it)
//   POST /gov          (operator only) {member, action} or {member, actions: [...]}: a hosted member's
//                      governance actions (at most MAX_GOV_PER_SIGNER), in as few
//                      transactions as fit one packet each
//   GET  /relay        {feePayer, blockhash, lastValidBlockHeight}: for members that sign their
//                      own transactions
//   POST /relay        {tx, lastValidBlockHeight?}: a member-signed CommitOrders, RevealOrders or SubmitGov; the
//                      gateway only adds the fee payer
//   GET  /claim-relay  {feePayer, blockhash, lastValidBlockHeight, mint, status}: for claims by
//                      members without SOL
//   POST /claim-relay  {tx, lastValidBlockHeight?}: a wallet-signed Claim on the base layer; the
//                      gateway only adds the fee payer
import { ComputeBudgetProgram, PublicKey, Transaction } from '@solana/web3.js';
import { IX_TAG, MAX_GOV_PER_SIGNER, NATIONS, NOBODY } from '../../client/src/codec.mjs';
import { send, sendSigned } from '../send.mjs';
import { RouteError, routeSeason } from './errors.mjs';
import { seal } from '../sealed.mjs';
import { allowedOffices } from '../../client/src/offices.mjs';
import { requireOperator } from './roster.mjs';
import { fromHex, toHex } from '../../client/src/bytes.mjs';

/** A base64 wire transaction, or a 400. */
export function parseTransaction(b64) {
  try {
    if (typeof b64 !== 'string' || !b64) throw new Error('missing');
    return Transaction.from(Buffer.from(b64, 'base64'));
  } catch {
    throw new RouteError(400, 'tx must be a base64 serialized transaction', 'InvalidTransaction');
  }
}

/**
 * The relay's filter: exactly one instruction to `programId` whose tag is in
 * `tags`, besides compute-budget instructions, with `feePayer` paying.
 * Returns that instruction, or null.
 */
export function onlyProgramInstruction(tx, { programId, tags, feePayer, allowBudget = true }) {
  const program = tx.instructions.filter(i => i.programId.equals(programId));
  const budget = allowBudget ? tx.instructions.filter(i => i.programId.equals(ComputeBudgetProgram.programId)) : [];
  const only = program.length === 1 && program.length + budget.length === tx.instructions.length && tags.includes(program[0].data[0]);
  return only && tx.feePayer?.equals(feePayer) ? program[0] : null;
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

const NOT_HOSTED = 'this member is not hosted by this gateway; sign it yourself and use /relay';

export const relayRoutes = {
  // Only the operator's game server may act for hosted members (it runs
  // their AI and relays the people who claimed a seat).
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

  'GET /relay': async ({ cfg, crank, blockhashes }) => {
    const { blockhash, lastValidBlockHeight } = await blockhashes.er.latest();
    return { body: { feePayer: crank.crank.publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: cfg.programId, endpoint: cfg.erRpc } };
  },

  'POST /relay': async ({ er, chain, crank, blockhashes }, req) => {
    const b = await req.json();
    const tx = parseTransaction(b.tx);
    const ix = onlyProgramInstruction(tx, { programId: chain.programId, tags: [IX_TAG.commitOrders, IX_TAG.revealOrders, IX_TAG.submitGov], feePayer: crank.crank.publicKey });
    if (!ix) throw new RouteError(400, 'relay accepts exactly one CommitOrders, RevealOrders or SubmitGov instruction with the gateway as fee payer', 'RelayRejected');
    tx.partialSign(crank.crank);
    const r = await sendSigned(er, tx, 'relay', { lastValidBlockHeight: await blockhashes.er.expiryOf(tx.recentBlockhash, b.lastValidBlockHeight) });
    return { body: { ok: true, signature: r.signature } };
  },

  // Claims on the base layer, for members without SOL (agents): the member's
  // wallet signs `Claim`, the gateway only pays the fee. The program pays
  // only into a token account the wallet owns, so the fee payer cannot
  // redirect it.
  'GET /claim-relay': async ({ base, cfg, chain, crank, blockhashes }) => {
    const { blockhash, lastValidBlockHeight } = await blockhashes.base.latest();
    const s = await routeSeason({ base, chain });
    return { body: { feePayer: crank.crank.publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: cfg.programId, mint: new PublicKey(s.usdcMint).toBase58(), status: s.status } };
  },

  'POST /claim-relay': async ({ base, chain, crank, blockhashes }, req) => {
    const b = await req.json();
    const tx = parseTransaction(b.tx);
    const ix = onlyProgramInstruction(tx, { programId: chain.programId, tags: [IX_TAG.claim], feePayer: crank.crank.publicKey, allowBudget: false });
    if (!ix || !ix.keys[1]?.pubkey.equals(chain.season)) throw new RouteError(400, 'claim relay accepts exactly one Claim for this season with the gateway as fee payer', 'RelayRejected');
    tx.partialSign(crank.crank);
    const r = await sendSigned(base, tx, 'claim relay', { lastValidBlockHeight: await blockhashes.base.expiryOf(tx.recentBlockhash, b.lastValidBlockHeight) });
    return { body: { ok: true, signature: r.signature } };
  },
};
