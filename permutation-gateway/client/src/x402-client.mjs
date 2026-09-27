// The client side of x402 registration: pay the entry fee over HTTP 402 and
// become a member of a nation.
//   1. POST /x402/join → 402 with PaymentRequirements (scheme "exact").
//   2. Sign the program's Register (it moves exactly the entry fee, plus any
//      treasury `deposit`, from `usdcAccount` into the season vault) with
//      `wallet` and `session` (the program takes only a session key that
//      signed); the facilitator is the fee payer and adds its signature.
//   3. POST again with X-PAYMENT → 200 + X-PAYMENT-RESPONSE.
//
// By default a member registers like everyone else, the operator's AI
// members included (V5 §18.2): kind 2 (undeclared) when the season has AI
// members, the season's public default deposit, a name from the one
// generator (`memberName`), standing for 1–2 offices drawn at random, no
// pre-season votes. In a season with AI members the gateway refuses anything
// else (409 KindHidden, UniformRegistration).
import { randomBytes, randomInt } from 'node:crypto';
import { PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from './chain.mjs';
import { memberName, NOBODY, ROLES, roleMask } from './codec.mjs';
import { GameError, HttpError, request } from './http.mjs';

const b64json = v => Buffer.from(JSON.stringify(v)).toString('base64');
const fromB64json = s => JSON.parse(Buffer.from(s, 'base64').toString('utf8'));

/** 1 or 2 offices at random (in ROLES order), as the AI members and people stand. */
export function randomOffices(rand = n => randomInt(n)) {
  const pool = [...ROLES];
  const picked = new Set();
  for (let n = rand(2) + 1; n > 0; n--) picked.add(pool.splice(rand(pool.length), 1)[0]);
  return ROLES.filter(r => picked.has(r));
}

/**
 * @param {string} gateway  gateway base URL
 * @param {object} o  wallet, session (Keypairs: both sign Register); civ (default: the smallest nation); name (default: `memberName`
 *                    of fresh random bytes); kind (0 human, 1 agent, 2 undeclared; default: 2 when the season has
 *                    operator AI members, where it is the only kind the gateway accepts, else 1); usdcAccount;
 *                    stand (office names; default, or empty: 1–2 at random); votes (member id per office, in
 *                    ROLES order; default nobody); deposit (default: the season's public default, the 402's
 *                    `extra.deposit`); attestation
 * @returns {Promise<{member, civ, nation, name, signature, pool, members, requirements, paymentResponse}>}
 */
export async function joinViaX402(gateway, { wallet, session, civ, name, kind, usdcAccount, stand, votes = [], deposit, attestation }) {
  const url = `${gateway}/x402/join`;
  const first = await request(url, { method: 'POST', body: { civ, name } });
  if (first.status !== 402) throw new HttpError(first.status, first.body, url);
  const req = first.body.accepts?.[0];
  if (!req || req.scheme !== 'exact') throw new GameError('X402Scheme', 'gateway did not offer the "exact" scheme');
  const x = req.extra;
  const chain = new ChainClient(x.programId, BigInt(x.seasonId));
  // Never sign for a different season or vault than the requirements name.
  if (chain.season.toBase58() !== x.accounts.season || chain.vault.toBase58() !== req.payTo) throw new GameError('X402Mismatch', 'payment requirements do not match the season PDAs');
  const pick = civ ?? x.nations.reduce((a, b) => (b.members < a.members ? b : a)).civ;
  const feePayer = new PublicKey(x.feePayer);
  // With operator AI members nobody declares a kind (V5 §18.2).
  const declared = kind ?? (x.aiCount > 0 ? 2 : 1);
  const offices = stand?.length ? stand : randomOffices();
  const drawn = name ?? memberName(new Uint8Array(randomBytes(32)));
  const tx = new Transaction().add(...chain.register({ wallet: wallet.publicKey, feePayer, civ: pick, walletToken: new PublicKey(usdcAccount),
    mint: new PublicKey(req.asset), name: drawn, kind: declared, session: session.publicKey, attestation, stand: roleMask(offices),
    votes: ROLES.map((_, i) => votes[i] ?? NOBODY), deposit: BigInt(deposit ?? x.deposit ?? 0) }));
  tx.feePayer = feePayer;
  tx.recentBlockhash = x.recentBlockhash;
  tx.partialSign(wallet, session);
  const payment = { x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } };
  const second = await request(url, { method: 'POST', body: { civ: pick, name: drawn }, headers: { 'X-PAYMENT': b64json(payment) } });
  if (second.status !== 200) throw new HttpError(second.status, second.body, url);
  const header = second.headers.get('x-payment-response');
  return { ...second.body, requirements: req, paymentResponse: header ? fromB64json(header) : null };
}
