// x402 entry (V4 §5.5): an agent pays the entry fee over HTTP 402.
//
//   POST /x402/join {name, kind, session, payout}
//     no X-PAYMENT  → 402 + PaymentRequirements (scheme "exact")
//     X-PAYMENT     → the gateway (acting as facilitator) verifies the signed
//                     JoinSeason transaction, co-signs as fee payer, submits it
//                     and answers 200 + X-PAYMENT-RESPONSE.
//
// The payment is program-mediated: instead of a bare SPL transfer, the
// signed transaction is the program's JoinSeason, which moves exactly the
// season's entry fee from the payer's USDC account into the season vault and
// registers the civilization in the same instruction. The facilitator can
// only add its fee-payer signature; it cannot change the amount or the payee.
//
//   POST /faucet {owner}  (localnet only) → a USDC account with 100 test USDC
import { PublicKey, Transaction } from '@solana/web3.js';
import { decodeSeason } from '../client/src/codec.mjs';
import { namedKey } from './config.mjs';
import { createTokenAccountIxs, mintToIx } from './spl.mjs';
import { send, sendSigned } from './send.mjs';

const b64json = v => Buffer.from(JSON.stringify(v)).toString('base64');
const network = cfg => (cfg.cluster === 'devnet' ? 'solana-devnet' : cfg.cluster === 'mainnet' ? 'solana' : 'solana-localnet');

export async function mountX402({ req, res, url, cfg, base, chain, state, json, readBody, log }) {
  if (url.pathname === '/faucet' && req.method === 'POST') {
    if (cfg.cluster !== 'localnet') return json(res, 403, { error: 'faucet is localnet only' }), true;
    const { owner } = await readBody(req);
    const ownerKey = new PublicKey(owner);
    const admin = namedKey('admin'), crank = namedKey('crank');
    const account = namedKey(`faucet-${ownerKey.toBase58().slice(0, 16)}`);
    const mint = new PublicKey(state.mint);
    if (!(await base.getAccountInfo(account.publicKey))) {
      await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: account.publicKey, mint, owner: ownerKey }), [crank, account], 'faucet account');
    }
    await send(base, [mintToIx({ mint, dest: account.publicKey, authority: admin.publicKey, amount: 100_000_000n })], [crank, admin], 'faucet mint');
    json(res, 200, { usdcAccount: account.publicKey.toBase58(), mint: state.mint, amount: '100000000', note: 'localnet test USDC, no value' });
    return true;
  }
  if (url.pathname !== '/x402/join' || req.method !== 'POST') return false;

  const season = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
  const facilitator = namedKey('crank');
  const requirements = {
    scheme: 'exact',
    network: network(cfg),
    maxAmountRequired: season.entryFee.toString(),
    resource: `http://127.0.0.1:${cfg.port}/x402/join`,
    description: `Entry to PERMUTATION STATE season ${season.seasonId} (${season.civs.length}/${season.maxCivs} civilizations)`,
    mimeType: 'application/json',
    payTo: chain.vault.toBase58(),
    maxTimeoutSeconds: 120,
    asset: new PublicKey(season.usdcMint).toBase58(),
    extra: {
      feePayer: facilitator.publicKey.toBase58(),
      programId: cfg.programId,
      seasonId: season.seasonId.toString(),
      instruction: 'JoinSeason',
      civ: season.civs.length,
      accounts: { season: chain.season.toBase58(), vault: chain.vault.toBase58(), orders: chain.orders(season.civs.length).toBase58() },
      recentBlockhash: (await base.getLatestBlockhash('confirmed')).blockhash,
    },
  };
  const header = req.headers['x-payment'];
  if (!header) {
    const closed = season.status !== 'Registering' || season.civs.length >= season.maxCivs;
    json(res, closed ? 409 : 402, closed ? { error: 'entry is closed for this season' } : { x402Version: 1, error: 'X-PAYMENT header is required', accepts: [requirements] });
    return true;
  }
  let payment;
  try {
    payment = JSON.parse(Buffer.from(String(header), 'base64').toString('utf8'));
  } catch {
    json(res, 400, { x402Version: 1, error: 'X-PAYMENT is not base64 JSON' });
    return true;
  }
  const tx = Transaction.from(Buffer.from(payment?.payload?.transaction || '', 'base64'));
  // Verify: exactly one JoinSeason for this season, paying into this vault,
  // with the facilitator as fee payer and the payer's signature present.
  const ix = tx.instructions.find(i => i.programId.toBase58() === cfg.programId);
  const others = tx.instructions.filter(i => i !== ix && i.programId.toBase58() !== 'ComputeBudget111111111111111111111111111111');
  const problems = [];
  if (payment.scheme !== 'exact' || payment.network !== requirements.network) problems.push('scheme/network');
  if (!ix || ix.data[0] !== 2 || others.length) problems.push('must contain exactly one JoinSeason');
  if (ix && (!ix.keys[2]?.pubkey.equals(chain.season) || !ix.keys[5]?.pubkey.equals(chain.vault))) problems.push('wrong season or vault');
  if (!tx.feePayer?.equals(facilitator.publicKey)) problems.push('fee payer must be the facilitator');
  const payer = ix?.keys[0]?.pubkey;
  if (!payer || !tx.signatures.some(s => s.publicKey.equals(payer) && s.signature)) problems.push('payer signature missing');
  // The facilitator only ever pays the fee: it must never be the one paying the entry.
  if (payer?.equals(facilitator.publicKey)) problems.push('payer must not be the facilitator');
  if (problems.length) {
    json(res, 402, { x402Version: 1, error: `invalid payment: ${problems.join(', ')}`, accepts: [requirements] });
    return true;
  }
  tx.partialSign(facilitator);
  try {
    const r = await sendSigned(base, tx, 'x402 join');
    const after = decodeSeason((await base.getAccountInfo(chain.season, 'confirmed')).data);
    const civ = after.civs.findIndex(c => new PublicKey(c.player).equals(payer));
    log(`x402: ${after.civs[civ]?.name} joined as civ ${civ}, paid ${season.entryFee} into the vault (${r.signature.slice(0, 12)}…)`);
    res.setHeader('X-PAYMENT-RESPONSE', b64json({ success: true, transaction: r.signature, network: requirements.network, payer: payer.toBase58() }));
    json(res, 200, { ok: true, civ, name: after.civs[civ]?.name, signature: r.signature, pool: after.pool.toString(), civs: after.civs.length, maxCivs: after.maxCivs });
  } catch (e) {
    json(res, 402, { x402Version: 1, error: `settlement failed: ${e.message}`, logs: e.logs?.slice(-4), accepts: [requirements] });
  }
  return true;
}
