// Adversarial checks of the x402 entry, against a gateway whose season still
// has an open seat (localnet):
//
//   node scripts/x402-check.mjs --gateway http://127.0.0.1:4193
//
// Every tampered payment must be refused without anything reaching the
// chain, and the season must still be joinable afterwards.
import { Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import assert from 'node:assert/strict';
import { ChainClient } from '../client/src/chain.mjs';
import { GameClient } from '../client/src/game.mjs';

const i = process.argv.indexOf('--gateway');
const gateway = i > 0 ? process.argv[i + 1] : 'http://127.0.0.1:4193';
const url = `${gateway}/x402/join`;
const post = async (headers = {}) => {
  const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: '{}' });
  return { status: r.status, body: await r.json() };
};
const pay = (req, tx) => ({ 'X-PAYMENT': Buffer.from(JSON.stringify({ x402Version: 1, scheme: 'exact', network: req.network,
  payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } })).toString('base64') });

const game = new GameClient({ gateway });
const wallet = Keypair.generate(), session = Keypair.generate();
const { usdcAccount } = await game.faucet(wallet.publicKey);
const before = (await game.season()).season;

const first = await post();
assert.equal(first.status, 402, 'no X-PAYMENT → 402');
const req = first.body.accepts[0];
assert.equal(req.scheme, 'exact');
assert.equal(req.maxAmountRequired, String(before.entryFee));
console.log('✓ 402 with PaymentRequirements (exact, amount = entry fee, payTo = vault)');

assert.equal((await post({ 'X-PAYMENT': 'not base64 json' })).status, 400);
console.log('✓ malformed X-PAYMENT → 400');

const x = req.extra;
const chain = new ChainClient(x.programId, BigInt(x.seasonId));
const join = (feePayer = new PublicKey(x.feePayer)) => chain.joinSeason({ player: wallet.publicKey, feePayer, civ: x.civ, playerToken: new PublicKey(usdcAccount),
  mint: new PublicKey(req.asset), name: 'Mallory', kind: 1, session: session.publicKey, payout: wallet.publicKey });
const build = (ixs, feePayer = new PublicKey(x.feePayer)) => { const t = new Transaction().add(...ixs); t.feePayer = feePayer; t.recentBlockhash = x.recentBlockhash; t.partialSign(wallet); return t; };

// A payment that also drains the fee payer: an extra instruction.
const drain = SystemProgram.transfer({ fromPubkey: new PublicKey(x.feePayer), toPubkey: wallet.publicKey, lamports: 1_000_000 });
let r = await post(pay(req, build([...join(), drain])));
assert.equal(r.status, 402); assert.match(r.body.error, /exactly one JoinSeason/);
console.log('✓ extra instruction (fee-payer drain) → refused');

// The payer pays its own fee (facilitator not the fee payer).
r = await post(pay(req, build(join(wallet.publicKey), wallet.publicKey)));
assert.equal(r.status, 402); assert.match(r.body.error, /fee payer/);
console.log('✓ wrong fee payer → refused');

// Paying into another vault.
const other = new ChainClient(x.programId, BigInt(x.seasonId) + 1n);
const redirected = join();
redirected[0].keys[5] = { pubkey: other.vault, isSigner: false, isWritable: true };
r = await post(pay(req, build(redirected)));
assert.equal(r.status, 402); assert.match(r.body.error, /wrong season or vault/);
console.log('✓ payment into another vault → refused');

// Unsigned by the payer.
const unsigned = new Transaction().add(...join()); unsigned.feePayer = new PublicKey(x.feePayer); unsigned.recentBlockhash = x.recentBlockhash;
r = await post(pay(req, unsigned));
assert.equal(r.status, 402); assert.match(r.body.error, /payer signature/);
console.log('✓ missing payer signature → refused');

const mid = (await game.season()).season;
assert.equal(mid.civs.length, before.civs.length, 'no refused payment registered a civ');
assert.equal(String(mid.pool), String(before.pool), 'no refused payment moved USDC');
console.log('✓ nothing reached the chain: entrants and pool unchanged');

// And the honest payment still settles.
const ok = await game.joinViaX402({ wallet, session, name: 'Honest', usdcAccount });
const after = (await game.season()).season;
assert.equal(after.civs.length, before.civs.length + 1);
assert.equal(BigInt(after.pool), BigInt(before.pool) + BigInt(before.entryFee));
assert.equal(ok.paymentResponse.success, true);
console.log(`✓ honest payment settles: civ ${ok.civ}, pool ${before.pool} → ${after.pool}, tx ${ok.signature.slice(0, 16)}…`);
