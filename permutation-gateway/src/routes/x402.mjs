// x402 registration (V5 D14): an agent pays the entry fee over HTTP 402 and
// becomes a member of a nation, with the same Register instruction a
// person's wallet signs.
//
//   POST /x402/join {civ, name}
//     no X-PAYMENT  → 402 + PaymentRequirements (scheme "exact")
//     X-PAYMENT     → the gateway (acting as facilitator) verifies the signed
//                     Register transaction, co-signs as fee payer, submits it
//                     and answers 200 + X-PAYMENT-RESPONSE.
//
// The payment is program-mediated: instead of a bare SPL transfer, the
// signed transaction is the program's Register, which moves exactly the
// season's entry fee (80% prize pool, 20% operations) plus any treasury
// deposit from the payer's USDC account into the season vault, and records
// the member in the same instruction. The facilitator can only add its
// fee-payer signature; it cannot change the amount or the payee.
import { ComputeBudgetProgram, PublicKey, Transaction } from '@solana/web3.js';
import bs58 from 'bs58';
import { decodeMember, IX_TAG, NATIONS } from '../../client/src/codec.mjs';
import { poll } from '../../client/src/retry.mjs';
import { sendSigned } from '../send.mjs';
import { RouteError, routeSeason } from './errors.mjs';

const b64json = v => Buffer.from(JSON.stringify(v)).toString('base64');
export const x402Network = cluster => (cluster === 'devnet' ? 'solana-devnet' : cluster === 'mainnet' ? 'solana' : 'solana-localnet');
/** Register's accounts (chain.mjs `register`): payer wallet, fee payer, season, member, wallet token, vault, … */
const REGISTER_KEYS = { wallet: 0, season: 2, vault: 5 };

/** The member account, read through transient RPC errors (null if it is not there). */
const memberAccount = (base, pda) => poll(() => base.getAccountInfo(pda, 'confirmed'), { attempts: 6, delayMs: 1000 });

/** Decode the X-PAYMENT header: base64 JSON whose payload holds a base64 transaction. 400 if it is not that. */
export function parsePayment(header) {
  let payment;
  try {
    payment = JSON.parse(Buffer.from(String(header), 'base64').toString('utf8'));
  } catch {
    throw new RouteError(400, 'X-PAYMENT is not base64 JSON', 'InvalidPayment', { x402Version: 1 });
  }
  let tx;
  try {
    tx = Transaction.from(Buffer.from(String(payment?.payload?.transaction ?? ''), 'base64'));
  } catch {
    throw new RouteError(400, 'X-PAYMENT payload.transaction is not a base64 serialized transaction', 'InvalidPayment', { x402Version: 1 });
  }
  return { payment, tx };
}

/**
 * What is wrong with a payment (empty if nothing): exactly one Register for
 * this season, paying into this vault, for the requested nation, with the
 * facilitator as fee payer (and never as payer) and the payer's signature.
 */
export function paymentProblems({ payment, tx, network, programId, season, vault, facilitator, civ }) {
  const ix = tx.instructions.find(i => i.programId.equals(programId));
  const others = tx.instructions.filter(i => i !== ix && !i.programId.equals(ComputeBudgetProgram.programId));
  const problems = [];
  if (payment.scheme !== 'exact' || payment.network !== network) problems.push('scheme/network');
  if (!ix || ix.data[0] !== IX_TAG.register || others.length) problems.push('must contain exactly one Register');
  if (ix && (!ix.keys[REGISTER_KEYS.season]?.pubkey.equals(season) || !ix.keys[REGISTER_KEYS.vault]?.pubkey.equals(vault))) problems.push('wrong season or vault');
  if (!tx.feePayer?.equals(facilitator)) problems.push('fee payer must be the facilitator');
  const payer = ix?.keys[REGISTER_KEYS.wallet]?.pubkey;
  if (!payer || !tx.signatures.some(s => s.publicKey.equals(payer) && s.signature)) problems.push('payer signature missing');
  // The facilitator only ever pays the fee: it must never be the one paying the entry.
  if (payer?.equals(facilitator)) problems.push('payer must not be the facilitator');
  if (ix && civ !== null && (ix.data.length < 3 || ix.data.readUInt16LE(1) !== civ)) problems.push('the transaction registers for another nation than requested');
  return { problems, payer };
}

export const x402Routes = {
  'POST /x402/join': async ({ base, cfg, chain, crank, store, registry, blockhashes, log }, req) => {
    const body = await req.json().catch(() => ({}));
    const season = await routeSeason({ base, chain });
    const facilitator = crank.crank;
    const civ = Number.isInteger(body.civ) ? body.civ : null;
    const requirements = {
      scheme: 'exact',
      network: x402Network(cfg.cluster),
      maxAmountRequired: season.entryFee.toString(),
      resource: `http://127.0.0.1:${cfg.port}/x402/join`,
      description: `Membership of a nation in PERMUTATION STATE season ${season.seasonId} (${season.memberCount} members so far)`,
      mimeType: 'application/json',
      payTo: chain.vault.toBase58(),
      maxTimeoutSeconds: 120,
      asset: new PublicKey(season.usdcMint).toBase58(),
      extra: {
        feePayer: facilitator.publicKey.toBase58(),
        programId: cfg.programId,
        seasonId: season.seasonId.toString(),
        instruction: 'Register',
        nations: NATIONS.slice(0, season.nations).map((name, i) => ({ civ: i, name, members: season.nationMembers[i] })),
        market: season.market,
        accounts: { season: chain.season.toBase58(), vault: chain.vault.toBase58() },
        recentBlockhash: (await blockhashes.base.latest()).blockhash,
      },
    };
    const header = req.headers['x-payment'];
    if (!header) {
      if (season.status !== 'Registering') throw new RouteError(409, 'registration is closed for this season', 'RegistrationClosed');
      return { status: 402, body: { x402Version: 1, error: 'X-PAYMENT header is required', accepts: [requirements] } };
    }
    const { payment, tx } = parsePayment(header);
    const { problems, payer } = paymentProblems({ payment, tx, network: requirements.network, programId: chain.programId, season: chain.season,
      vault: chain.vault, facilitator: facilitator.publicKey, civ });
    if (problems.length) return { status: 402, body: { x402Version: 1, error: `invalid payment: ${problems.join(', ')}`, code: 'InvalidPayment', accepts: [requirements] } };

    tx.partialSign(facilitator);
    let signature;
    let account = null;
    try {
      signature = (await sendSigned(base, tx, 'x402 register', { lastValidBlockHeight: await blockhashes.base.expiryOf(tx.recentBlockhash) })).signature;
    } catch (e) {
      // The payment may have settled even though confirming it failed (a
      // flaky RPC): the Member PDA is the proof. Only report failure if it
      // is not there.
      account = await memberAccount(base, chain.member(payer));
      if (!account) return { status: 402, body: { x402Version: 1, error: `settlement failed: ${e.message}`, logs: e.logs?.slice(-4), accepts: [requirements] } };
      signature = bs58.encode(tx.signature);
      log(`x402: confirmation failed (${e.message.slice(0, 80)}), but the member account exists: settled`);
    }
    account ??= await memberAccount(base, chain.member(payer));
    if (!account) throw new Error(`x402: ${signature} settled but the member account cannot be read`);
    const m = decodeMember(account.data);
    const after = await routeSeason({ base, chain });
    // Remember the member at once: the file is the gateway's record of who is external.
    store.state.members.push({ index: m.index, civ: m.civ, name: m.name, kind: m.kind, hosted: 'external', wallet: payer.toBase58(), session: new PublicKey(m.session).toBase58() });
    store.save();
    registry.invalidate();
    log(`x402: ${m.name} joined ${NATIONS[m.civ]} as member ${m.index}, paid ${season.entryFee} into the vault (${signature.slice(0, 12)}…)`);
    return {
      headers: { 'X-PAYMENT-RESPONSE': b64json({ success: true, transaction: signature, network: requirements.network, payer: payer.toBase58() }) },
      body: { ok: true, member: m.index, civ: m.civ, nation: NATIONS[m.civ], name: m.name, signature, pool: after.pool.toString(), members: after.memberCount },
    };
  },
};
