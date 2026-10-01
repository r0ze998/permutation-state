// x402 registration (V5 D14): an agent — or a person's browser, which is an
// x402 client with a wallet — pays the entry fee over HTTP 402 and becomes a
// member of a nation, with the program's Register instruction.
//
//   POST /x402/join {civ, name}
//     no X-PAYMENT  → 402 + PaymentRequirements (scheme "exact")
//     X-PAYMENT     → the gateway (acting as facilitator) checks the signed
//                     Register transaction, co-signs as fee payer, simulates
//                     the exact bytes, submits them and answers 200 +
//                     X-PAYMENT-RESPONSE.
//
// The payment is program-mediated: instead of a bare SPL transfer, the
// signed transaction is the program's Register, which moves exactly the
// season's entry fee (80% prize pool, 20% operations) plus any treasury
// deposit from the payer's USDC account into the season vault, and records
// the member in the same instruction. The facilitator can only add its
// fee-payer signature; it cannot change the amount or the payee.
//
// What it co-signs (a public route: the crank pays the fee and the member
// account's rent): exactly one instruction, Register (no compute-budget
// instruction), signed by exactly three keys — the facilitator (fee payer,
// first), the wallet and the session key (never the facilitator or the
// wallet), whose signatures are checked here — for this season,
// vault and mint; a wallet that is not a member yet; a session key no member
// has (a second one could not act, V5 §18); kind 2 (undeclared) in a season
// with operator AI members (nobody's kind is shown there, V5 §18.2), and
// there also registered like everyone else, AI or not: the public default
// deposit, no pre-season votes, no attestation, standing for 1–2 offices
// (`registrationTells`; the SDK's defaults, the web lobby and the AI
// members all register so).
//
// Refusals: 409 RegistrationClosed, SeasonFull, KindHidden, UniformRegistration, SessionInUse,
// AlreadyInitialized (the wallet is a member), RegistrationInFlight; 402
// (with `accepts`) for a payment that is not such a Register (`code`
// InvalidPayment) or that the simulation or the cluster refused (`code`: the
// program error, InsufficientFunds, BlockhashExpired, SettlementFailed…);
// 400 InvalidPayment for an X-PAYMENT that is not base64 JSON with a transaction.
import { PublicKey } from '@solana/web3.js';
import { Reader } from '../../client/src/borsh.mjs';
import { encode as base58 } from '../../client/src/base58.mjs';
import { fromBase64 } from '../../client/src/bytes.mjs';
import { decodeMember, IX_TAG, MAX_MEMBERS, NATIONS, NOBODY, ROLES } from '../../client/src/codec.mjs';
import { parseTransaction } from '../../client/src/solana-tx.mjs';
import { poll } from '../../client/src/retry.mjs';
import { coSign, refusal, signatureOf, signedBy } from '../cosign.mjs';
import { SIGNER_LIMITS } from '../guards.mjs';
import { pendingAi, registrationOf, registrationOpen, seasonMembers } from '../season.mjs';
import { SimulationError, sendWire, simulateWire } from '../send.mjs';
import { RouteError, routeSeason } from './errors.mjs';

const b64json = v => Buffer.from(JSON.stringify(v)).toString('base64');
export const x402Network = cluster => (cluster === 'devnet' ? 'solana-devnet' : cluster === 'mainnet' ? 'solana' : 'solana-localnet');
/** Register's accounts (chain.mjs `register`), by position. */
export const REGISTER_KEYS = Object.freeze({ wallet: 0, feePayer: 1, season: 2, member: 3, walletToken: 4, vault: 5, mint: 6, tokenProgram: 7, system: 8, session: 9 });

/** The member account, read through transient RPC errors (null if it is not there). */
const memberAccount = (base, pda) => poll(() => base.getAccountInfo(pda, 'confirmed'), { attempts: 6, delayMs: 1000 });

/**
 * Register's instruction data (codec.mjs IX.register), decoded: `{civ,
 * name, kind, session, attestation, stand, votes, deposit, tag}` (keys as
 * bytes), or null unless it is exactly that. The session key sits at offset
 * 1 + 2 + 4 + len(name) + 1.
 */
export function decodeRegister(data) {
  try {
    const r = new Reader(data);
    if (r.u8() !== IX_TAG.register) return null;
    const out = { civ: r.u16(), name: new TextDecoder('utf-8', { fatal: true }).decode(r.bytes()), kind: r.u8(), session: r.fixed(32), attestation: r.fixed(32), stand: r.u8() };
    out.votes = [r.u32(), r.u32(), r.u32(), r.u32()];
    out.deposit = r.u64();
    out.tag = r.fixed(32);
    return r.o === r.b.length ? out : null;
  } catch {
    return null;
  }
}

/**
 * How a Register (decoded) would stand out from the operator's AI members'
 * in a season with AI members, as a list of what it must do instead (empty:
 * it registers like everyone): the public default `deposit`, votes for
 * nobody, no attestation, standing for 1–2 offices (V5 §18.2).
 */
export function registrationTells(register, { deposit }) {
  const tells = [];
  if (register.deposit !== BigInt(deposit)) tells.push(`deposit ${deposit} (the public default)`);
  if (register.votes.some(v => v !== NOBODY)) tells.push('no pre-season votes (every vote NOBODY)');
  if (register.attestation.some(b => b !== 0)) tells.push('no attestation');
  const offices = ROLES.filter((_, i) => register.stand & (1 << i)).length;
  if (register.stand >> ROLES.length || offices < 1 || offices > 2) tells.push('stand for 1–2 offices');
  return tells;
}

/** Decode the X-PAYMENT header: base64 JSON whose payload holds a base64 transaction. 400 if it is not that. */
export function parsePayment(header) {
  let payment;
  try {
    payment = JSON.parse(new TextDecoder().decode(fromBase64(String(header))));
  } catch {
    throw new RouteError(400, 'X-PAYMENT is not base64 JSON', 'InvalidPayment', { x402Version: 1 });
  }
  try {
    const wire = fromBase64(String(payment?.payload?.transaction ?? ''));
    return { payment, wire, tx: parseTransaction(wire) };
  } catch {
    throw new RouteError(400, 'X-PAYMENT payload.transaction is not a base64 serialized legacy transaction', 'InvalidPayment', { x402Version: 1 });
  }
}

/**
 * What is wrong with a payment (empty if nothing), `tx` parsed
 * (solana-tx.mjs): exactly one instruction, this program's Register, for
 * this season, vault and mint and the requested nation; exactly three
 * signers, the facilitator (fee payer, first; also Register's fee payer),
 * the wallet (never the facilitator) and the session key named in the data
 * (account 9; a key of its own), whose signatures are valid. Keys base58.
 */
export function paymentProblems({ payment, tx, network, programId, season, vault, mint = null, facilitator, civ = null }) {
  const problems = [];
  if (payment.scheme !== 'exact' || payment.network !== network) problems.push('scheme/network');
  const ix = tx.instructions.length === 1 ? tx.instructions[0] : null;
  if (!ix || ix.programId !== programId || ix.data[0] !== IX_TAG.register) problems.push('must contain exactly one Register and nothing else (no compute-budget instructions)');
  const register = ix && ix.programId === programId ? decodeRegister(ix.data) : null;
  if (ix && ix.programId === programId && ix.data[0] === IX_TAG.register && !register) problems.push('malformed Register data');
  if (tx.signers.length !== 3 || tx.signers[0] !== facilitator) problems.push('three signers: the facilitator (fee payer, first), the wallet and the session key');
  const keys = ix?.keys ?? [];
  if (register && keys.length !== 10) problems.push('Register takes 10 accounts (the last is the session key, a signer)');
  if (register && keys[REGISTER_KEYS.feePayer]?.pubkey !== facilitator) problems.push('Register\'s fee payer must be the facilitator');
  if (register && (keys[REGISTER_KEYS.season]?.pubkey !== season || keys[REGISTER_KEYS.vault]?.pubkey !== vault)) problems.push('wrong season or vault');
  if (register && mint && keys[REGISTER_KEYS.mint]?.pubkey !== mint) problems.push('wrong mint');
  const payer = register ? keys[REGISTER_KEYS.wallet]?.pubkey ?? null : null;
  // The facilitator only ever pays the fee: it must never be the one paying the entry.
  if (payer && payer === facilitator) problems.push('payer must not be the facilitator');
  else if (payer && !signedBy(tx, payer)) problems.push('payer signature missing or invalid');
  const session = register ? base58(register.session) : null;
  if (register && keys[REGISTER_KEYS.session]?.pubkey !== session) problems.push('account 9 must be the session key named in the data');
  else if (session && (session === facilitator || session === payer)) problems.push('the session key must be a key of its own (not the facilitator, not the wallet)');
  else if (session && !signedBy(tx, session)) problems.push('session key signature missing or invalid');
  if (register && civ !== null && register.civ !== civ) problems.push('the transaction registers for another nation than requested');
  return { problems, payer, register };
}

export const x402Routes = {
  'POST /x402/join': async (ctx, req) => {
    const { base, cfg, chain, crank, store, registry, blockhashes, log, desk, limiter, now } = ctx;
    const body = await req.json().catch(() => ({}));
    const season = await routeSeason({ base, chain });
    const state = store.state;
    const t = now();
    if (!registrationOpen({ state, season, phase: crank.phase, now: t })) throw new RouteError(409, 'registration is closed for this season', 'RegistrationClosed');
    // Seats the operator's AI members will still take are not for sale.
    if (season.memberCount + pendingAi(state) + desk.size >= MAX_MEMBERS) throw new RouteError(409, 'the season is full', 'SeasonFull');
    const reg = registrationOf(state, { fallbackOpenedAt: crank.startedAt ?? t, entryFee: season.entryFee });
    const facilitator = crank.crank;
    const civ = Number.isInteger(body.civ) ? body.civ : null;
    const mint = new PublicKey(season.usdcMint).toBase58();
    const requirements = {
      scheme: 'exact',
      network: x402Network(cfg.cluster),
      maxAmountRequired: (season.entryFee + reg.deposit).toString(),
      resource: req.surface === 'public' ? `http://${cfg.publicHost}:${cfg.publicPort}/x402/join` : `http://127.0.0.1:${cfg.port}/x402/join`,
      description: `Membership of a nation in Wylls season ${season.seasonId} (${season.memberCount} members so far)`,
      mimeType: 'application/json',
      payTo: chain.vault.toBase58(),
      maxTimeoutSeconds: 120,
      asset: mint,
      extra: {
        feePayer: facilitator.publicKey.toBase58(),
        crank: facilitator.publicKey.toBase58(),
        programId: cfg.programId,
        seasonId: season.seasonId.toString(),
        instruction: 'Register',
        nations: NATIONS.slice(0, season.nations).map((name, i) => ({ civ: i, name, members: season.nationMembers[i] })),
        market: season.market,
        accounts: { season: chain.season.toBase58(), vault: chain.vault.toBase58() },
        usdcMint: mint,
        // Everyone registers with this deposit and, with AI members, kind 2.
        deposit: reg.deposit.toString(),
        aiCount: season.aiCount,
        closesAt: reg.closesAt,
        recentBlockhash: (await blockhashes.base.latest()).blockhash,
      },
    };
    const header = req.headers['x-payment'];
    if (!header) return { status: 402, body: { x402Version: 1, error: 'X-PAYMENT header is required', accepts: [requirements] } };
    const { payment, tx } = parsePayment(header);
    const { problems, payer, register } = paymentProblems({ payment, tx, network: requirements.network, programId: chain.programId.toBase58(), season: chain.season.toBase58(),
      vault: chain.vault.toBase58(), mint, facilitator: facilitator.publicKey.toBase58(), civ });
    const invalid = (error, code, extra = {}) => ({ status: 402, body: { x402Version: 1, error, code, ...extra, accepts: [requirements] } });
    if (problems.length) return invalid(`invalid payment: ${problems.join(', ')}`, 'InvalidPayment');
    limiter.check(`join:${payer}`, SIGNER_LIMITS.join, 'registrations from this wallet');
    if (season.aiCount > 0 && register.kind !== 2) throw new RouteError(409, 'this season has operator AI members: every member registers kind 2 (undeclared)', 'KindHidden');
    const tells = season.aiCount > 0 ? registrationTells(register, { deposit: reg.deposit }) : [];
    if (tells.length) throw new RouteError(409, `this season has operator AI members: every member registers the same way: ${tells.join(', ')}`, 'UniformRegistration');
    const payerKey = new PublicKey(payer);
    if (await base.getAccountInfo(chain.member(payerKey), 'confirmed')) throw new RouteError(409, 'this wallet is a member of this season already', 'AlreadyInitialized');
    const session = base58(register.session);
    const release = desk.reserve({ wallet: payer, session });
    if (!release) throw new RouteError(409, 'a registration with this wallet or session key is being sent', 'RegistrationInFlight');
    let signature;
    let account = null;
    try {
      // A session key can be one member's only (checked fresh: the cached
      // registry could be seconds old).
      const planned = (state.aiPlan ?? []).some(e => e.session === session);
      if (planned || (await seasonMembers(base, chain)).some(m => base58(m.session) === session)) {
        throw new RouteError(409, 'this session key is a member\'s already; make a new one', 'SessionInUse');
      }
      const signed = coSign(tx, facilitator);
      try {
        await simulateWire(base, signed);
      } catch (e) {
        if (!(e instanceof SimulationError)) throw e;
        const r = refusal(e, chain.programId.toBase58());
        return invalid(`payment refused: ${r.message}`, r.code, r.extra);
      }
      signature = signatureOf(signed);
      try {
        await sendWire(base, signed, 'x402 register', { lastValidBlockHeight: await blockhashes.base.expiryOf(tx.recentBlockhash), fetch: false });
      } catch (e) {
        // The payment may have settled even though confirming it failed (a
        // flaky RPC): the Member PDA is the proof. Only report failure if it
        // is not there.
        account = await memberAccount(base, chain.member(payerKey));
        if (!account) return invalid(`settlement failed: ${e.message}`, /blockhash expired/i.test(e.message) ? 'BlockhashExpired' : 'SettlementFailed', { logs: e.logs?.slice(-4) });
        log(`x402: confirmation failed (${e.message.slice(0, 80)}), but the member account exists: settled`);
      }
      account ??= await memberAccount(base, chain.member(payerKey));
    } finally {
      release();
    }
    if (!account) throw new Error(`x402: ${signature} settled but the member account cannot be read`);
    const m = decodeMember(account.data);
    const after = await routeSeason({ base, chain });
    // Remember the member at once: the file is the gateway's record of who is external.
    if (!state.members.some(x => x.index === m.index)) {
      state.members.push({ index: m.index, civ: m.civ, name: m.name, kind: m.kind, hosted: 'external', wallet: payer, session: new PublicKey(m.session).toBase58() });
    }
    store.save();
    registry.invalidate();
    log(`x402: ${m.name} joined ${NATIONS[m.civ]} as member ${m.index}, paid ${season.entryFee + register.deposit} into the vault (${signature.slice(0, 12)}…)`);
    return {
      headers: { 'X-PAYMENT-RESPONSE': b64json({ success: true, transaction: signature, network: requirements.network, payer }) },
      body: { ok: true, member: m.index, civ: m.civ, nation: NATIONS[m.civ], name: m.name, signature, pool: after.pool.toString(), members: after.memberCount },
    };
  },
};
