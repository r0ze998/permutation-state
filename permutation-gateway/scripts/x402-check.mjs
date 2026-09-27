// Adversarial checks of the x402 registration, against a gateway whose
// season is still registering (localnet):
//
//   node scripts/x402-check.mjs --gateway http://127.0.0.1:4191   (the operator listener: no limits)
//   node scripts/x402-check.mjs --gateway http://127.0.0.1:4194   (the public listener, or its /gw)
//
// It sends about 14 POST /x402/join from one address. The public listener
// allows each address a burst of 10, then one every 5 s (guards.mjs
// IP_LIMITS), so there a request answered 429 RateLimited is sent again once
// the limit allows it (about a minute in all); nothing else changes.
//
// Every tampered payment must be refused without anything reaching the
// chain (a compute-budget instruction, which would make the gateway pay a
// priority fee, included), and the season must still be joinable afterwards;
// then a second registration with the member's session key is refused.
import { ComputeBudgetProgram, Keypair, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import assert from 'node:assert/strict';
import { ChainClient } from '../client/src/chain.mjs';
import { roleMask } from '../client/src/codec.mjs';
import { GameClient } from '../client/src/game.mjs';
import { sleep } from '../client/src/retry.mjs';
import { DEFAULTS, parseArgs } from '../src/config.mjs';
import { IP_LIMITS } from '../src/guards.mjs';

const { gateway } = parseArgs(process.argv.slice(2), { gateway: DEFAULTS.gatewayUrl });
const url = `${gateway}/x402/join`;
/**
 * Send `fn` (which makes `requests` requests) again after the public
 * listener's per-address limit allows that many more (one every
 * 1/perSecond s), for as long as it answers 429 RateLimited.
 */
const WAIT_MS = Math.ceil(1000 / IP_LIMITS['POST /x402/join'].perSecond) + 250;
const limited = e => e?.status === 429 && (e.body?.code ?? e.code) === 'RateLimited';
let told = false;
async function paced(fn, requests = 1) {
  for (let i = 0; ; i++) {
    const r = await fn().catch(e => { if (limited(e) && i < 20) return e; throw e; });
    if (!limited(r) || i >= 20) return r;
    if (!told) console.log(`· rate limited by the public listener: waiting ${WAIT_MS / 1000} s per request`);
    told = true;
    await sleep(WAIT_MS * requests);
  }
}
const post = (headers = {}) => paced(async () => {
  const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: '{}' });
  return { status: r.status, body: await r.json() };
});
const pay = (req, tx) => ({ 'X-PAYMENT': Buffer.from(JSON.stringify({ x402Version: 1, scheme: 'exact', network: req.network,
  payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } })).toString('base64') });

const game = new GameClient({ gateway });
const wallet = Keypair.generate(), session = Keypair.generate();
const { usdcAccount } = await paced(() => game.faucet(wallet.publicKey));
const before = (await game.season()).season;

const first = await post();
assert.equal(first.status, 402, 'no X-PAYMENT → 402');
const req = first.body.accepts[0];
assert.equal(req.scheme, 'exact');
assert.equal(req.maxAmountRequired, String(BigInt(before.entryFee) + BigInt(req.extra.deposit ?? 0)));
console.log('✓ 402 with PaymentRequirements (exact, amount = entry fee + the default deposit, payTo = vault)');

assert.equal((await post({ 'X-PAYMENT': 'not base64 json' })).status, 400);
assert.equal((await post({ 'X-PAYMENT': Buffer.from(JSON.stringify({ scheme: 'exact', payload: { transaction: 'AAAA' } })).toString('base64') })).status, 400);
console.log('✓ malformed X-PAYMENT (not JSON, or not a transaction) → 400');

const x = req.extra;
const kind = x.aiCount > 0 ? 2 : 1;
const chain = new ChainClient(x.programId, BigInt(x.seasonId));
// Registered like everyone (with AI members the gateway refuses anything else: kind 2, the default deposit, 1–2 offices).
const join = (feePayer = new PublicKey(x.feePayer), o = {}) => chain.register({ wallet: wallet.publicKey, feePayer, civ: 5, walletToken: new PublicKey(usdcAccount),
  mint: new PublicKey(req.asset), name: 'Mallory', kind, session: session.publicKey, stand: roleMask(['Steward']), deposit: BigInt(x.deposit ?? 0), ...o });
const build = (ixs, feePayer = new PublicKey(x.feePayer)) => { const t = new Transaction().add(...ixs); t.feePayer = feePayer; t.recentBlockhash = x.recentBlockhash; t.partialSign(wallet, session); return t; };

// A payment that also drains the fee payer: an extra instruction.
const drain = SystemProgram.transfer({ fromPubkey: new PublicKey(x.feePayer), toPubkey: wallet.publicKey, lamports: 1_000_000 });
let r = await post(pay(req, build([...join(), drain])));
assert.equal(r.status, 402); assert.match(r.body.error, /exactly one Register/);
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

// A priority fee the gateway would pay: no compute-budget instruction at all.
r = await post(pay(req, build([ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1_000_000_000 }), ...join()])));
assert.equal(r.status, 402); assert.match(r.body.error, /no compute-budget/);
console.log('✓ compute-budget instruction (priority fee on the gateway) → refused, never sent');

// Kind shown in a season with operator AI members.
if (x.aiCount > 0) {
  r = await post(pay(req, build(join(undefined, { kind: 0 }))));
  assert.deepEqual([r.status, r.body.code], [409, 'KindHidden']);
  console.log('✓ kind 0 in a season with AI members → 409 KindHidden');
  r = await post(pay(req, build(join(undefined, { stand: 0 }))));
  assert.deepEqual([r.status, r.body.code], [409, 'UniformRegistration']);
  console.log('✓ standing for no office in a season with AI members → 409 UniformRegistration');
}

// Unsigned by the payer.
const unsigned = new Transaction().add(...join()); unsigned.feePayer = new PublicKey(x.feePayer); unsigned.recentBlockhash = x.recentBlockhash;
r = await post(pay(req, unsigned));
assert.equal(r.status, 402); assert.match(r.body.error, /payer signature/);
console.log('✓ missing payer signature → refused');

const mid = (await game.season()).season;
assert.equal(mid.memberCount, before.memberCount, 'no refused payment registered a member');
assert.equal(String(mid.pool), String(before.pool), 'no refused payment moved USDC');
console.log('✓ nothing reached the chain: members and pool unchanged');

// And the honest payment still settles.
const ok = await paced(() => game.joinViaX402({ wallet, session, civ: 5, name: 'Honest', usdcAccount }), 2); // the 402, then the payment
const after = (await game.season()).season;
assert.equal(after.memberCount, before.memberCount + 1);
// 80% of the fee to the prize pool, 20% to operations (V5 D10).
assert.equal(BigInt(after.pool) + BigInt(after.ops), BigInt(before.pool) + BigInt(before.ops) + BigInt(before.entryFee));
assert.equal(BigInt(after.ops) - BigInt(before.ops), BigInt(before.entryFee) / 5n);
assert.equal(ok.paymentResponse.success, true);
console.log(`✓ honest payment settles: member ${ok.member} of ${ok.nation}, pool ${before.pool} → ${after.pool} (+80%), operations +20%, tx ${ok.signature.slice(0, 16)}…`);

// Another wallet with the member's session key: refused before anything is sent.
const twin = Keypair.generate();
const twinAccount = (await paced(() => game.faucet(twin.publicKey))).usdcAccount;
const again = await post();
const t = new Transaction().add(...chain.register({ wallet: twin.publicKey, feePayer: new PublicKey(x.feePayer), civ: 5, walletToken: new PublicKey(twinAccount),
  mint: new PublicKey(req.asset), name: 'Twin', kind, session: session.publicKey, stand: roleMask(['General', 'Science']), deposit: BigInt(x.deposit ?? 0) }));
t.feePayer = new PublicKey(x.feePayer); t.recentBlockhash = again.body.accepts[0].extra.recentBlockhash; t.partialSign(twin, session);
r = await post(pay(req, t));
assert.deepEqual([r.status, r.body.code], [409, 'SessionInUse']);
console.log('✓ a second member with the same session key → 409 SessionInUse');
