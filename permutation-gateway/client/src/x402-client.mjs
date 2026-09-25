// The client side of x402 registration: pay the entry fee over HTTP 402 and
// become a member of a nation.
//   1. POST /x402/join → 402 with PaymentRequirements (scheme "exact").
//   2. Sign the program's Register (it moves exactly the entry fee, plus any
//      treasury `deposit`, from `usdcAccount` into the season vault) with
//      `wallet`; the facilitator is the fee payer and adds its signature.
//   3. POST again with X-PAYMENT → 200 + X-PAYMENT-RESPONSE.
import { PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from './chain.mjs';
import { NOBODY, ROLES, roleMask } from './codec.mjs';
import { GameError, HttpError, request } from './http.mjs';

const b64json = v => Buffer.from(JSON.stringify(v)).toString('base64');
const fromB64json = s => JSON.parse(Buffer.from(s, 'base64').toString('utf8'));

/**
 * @param {string} gateway  gateway base URL
 * @param {object} o  wallet, session (Keypairs); civ (default: the smallest nation); name; kind (0 human, 1 agent);
 *                    usdcAccount; stand (office names); votes (member id per office, in ROLES order); deposit; attestation
 * @returns {Promise<{member, civ, nation, name, signature, pool, members, requirements, paymentResponse}>}
 */
export async function joinViaX402(gateway, { wallet, session, civ, name, kind = 1, usdcAccount, stand = [], votes = [], deposit = 0n, attestation }) {
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
  const tx = new Transaction().add(...chain.register({ wallet: wallet.publicKey, feePayer, civ: pick, walletToken: new PublicKey(usdcAccount),
    mint: new PublicKey(req.asset), name, kind, session: session.publicKey, attestation, stand: roleMask(stand),
    votes: ROLES.map((_, i) => votes[i] ?? NOBODY), deposit: BigInt(deposit) }));
  tx.feePayer = feePayer;
  tx.recentBlockhash = x.recentBlockhash;
  tx.partialSign(wallet);
  const payment = { x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } };
  const second = await request(url, { method: 'POST', body: { civ: pick, name }, headers: { 'X-PAYMENT': b64json(payment) } });
  if (second.status !== 200) throw new HttpError(second.status, second.body, url);
  const header = second.headers.get('x-payment-response');
  return { ...second.body, requirements: req, paymentResponse: header ? fromB64json(header) : null };
}
