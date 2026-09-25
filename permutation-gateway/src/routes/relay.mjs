// Signing and relaying for members.
//
//   POST /submit       {civ, role, member|null, tick, digest, orders, adopt}: an office's batch,
//                      signed with a hosted member's session key (or the crank for a vacant
//                      office, the acting official)
//   POST /gov          {member, action}: a hosted member's governance action
//   GET  /relay        {feePayer, blockhash, lastValidBlockHeight}: for members that sign their
//                      own transactions
//   POST /relay        {tx, lastValidBlockHeight?}: a member-signed SubmitOrders or SubmitGov; the
//                      gateway only adds the fee payer
//   GET  /claim-relay  {feePayer, blockhash, lastValidBlockHeight, mint, status}: for claims by
//                      members without SOL
//   POST /claim-relay  {tx, lastValidBlockHeight?}: a wallet-signed Claim on the base layer; the
//                      gateway only adds the fee payer
import { ComputeBudgetProgram, PublicKey, Transaction } from '@solana/web3.js';
import { decodeSeason, IX_TAG, NATIONS, NOBODY } from '../../client/src/codec.mjs';
import { send, sendSigned } from '../send.mjs';
import { RouteError } from './errors.mjs';

const fromHex = h => Uint8Array.from(Buffer.from(h || '', 'hex'));

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

const NOT_HOSTED = 'this member is not hosted by this gateway; sign it yourself and use /relay';

export const relayRoutes = {
  'POST /submit': async ({ er, chain, crank, hostedKey }, req) => {
    const b = await req.json();
    const member = b.member ?? NOBODY;
    // A vacant office is run by the acting official, which the crank signs for.
    const signer = member === NOBODY ? crank.crank : hostedKey(member);
    if (!signer) throw new RouteError(403, NOT_HOSTED, 'NotHosted');
    const digest = fromHex(b.digest);
    const ixs = chain.submitOrders({ signer: signer.publicKey, civ: b.civ, role: b.role, tick: b.tick, decisionDigest: digest.length === 32 ? digest : new Uint8Array(32), orders: b.orders || [], adopt: b.adopt || [] });
    const r = await send(er, ixs, signer === crank.crank ? [crank.crank] : [crank.crank, signer], `submit ${NATIONS[b.civ]} ${b.role}`);
    return { body: { ok: true, signature: r.signature } };
  },

  'POST /gov': async ({ er, chain, crank, hostedKey, registry }, req) => {
    const b = await req.json();
    const signer = hostedKey(b.member);
    if (!signer) throw new RouteError(403, NOT_HOSTED, 'NotHosted');
    const civ = (await registry.list()).find(m => m.index === b.member)?.civ;
    if (civ === undefined) throw new RouteError(404, 'no such member', 'NoSuchMember');
    const r = await send(er, chain.submitGov({ signer: signer.publicKey, civ, member: b.member, action: b.action }), [crank.crank, signer], `gov ${b.action?.type} by member ${b.member}`);
    return { body: { ok: true, signature: r.signature } };
  },

  'GET /relay': async ({ cfg, crank, blockhashes }) => {
    const { blockhash, lastValidBlockHeight } = await blockhashes.er.latest();
    return { body: { feePayer: crank.crank.publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: cfg.programId, endpoint: cfg.erRpc } };
  },

  'POST /relay': async ({ er, chain, crank, blockhashes }, req) => {
    const b = await req.json();
    const tx = parseTransaction(b.tx);
    const ix = onlyProgramInstruction(tx, { programId: chain.programId, tags: [IX_TAG.submitOrders, IX_TAG.submitGov], feePayer: crank.crank.publicKey });
    if (!ix) throw new RouteError(400, 'relay accepts exactly one SubmitOrders or SubmitGov instruction with the gateway as fee payer', 'RelayRejected');
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
    const s = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
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
