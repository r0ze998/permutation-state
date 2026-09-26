// GET /claims?wallet=: what a wallet can claim in this season and in every
// season of the gateway's lineage (so the web never reads the base RPC
// itself), on both listeners, limited per address and cached 5 s per wallet.
// And POST /faucet refreshes the owner's cached GET /usdc.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { claimParts, decodeMember, decodeSeason } from '../client/src/codec.mjs';
import { PUBLIC_ROUTES } from '../src/app.mjs';
import { IP_LIMITS } from '../src/guards.mjs';
import { call, gateway, memberData, programId, seasonData, tokenAccountData, tx, b64 } from './gateway-fixtures.mjs';

/** Season 42 (Registering) follows 41 (Finalized, prize and treasury share for member 2) and 40 (the wallet was not a member). */
function seasons() {
  const g = gateway({ state: { lineage: [{ seasonId: '40' }, { seasonId: '41' }] } });
  const wallet = Keypair.generate();
  const s41 = new ChainClient(programId, 41n), s40 = new ChainClient(programId, 40n);
  const payouts = [0n, 0n, 7_000_000n, 0n];
  g.base.accounts.set(s41.season.toBase58(), { data: seasonData({ seasonId: 41n, status: 'Finalized', usdcMint: g.mint, crank: g.crankKey.publicKey, payouts, treasury: [4_000_000n, 0n], treasuryFinal: [2_000_000n, 0n] }) });
  g.base.accounts.set(s40.season.toBase58(), { data: seasonData({ seasonId: 40n, status: 'Finalized', usdcMint: g.mint, crank: g.crankKey.publicKey }) });
  g.base.accounts.set(s41.member(wallet.publicKey).toBase58(), { data: memberData({ seasonId: 41n, index: 2, civ: 0, wallet: wallet.publicKey, name: 'Ada K.', shares: 1_000_000n, claimed: false }) });
  g.base.accounts.set(g.chain.member(wallet.publicKey).toBase58(), { data: memberData({ seasonId: 42n, index: 5, civ: 3, wallet: wallet.publicKey, name: 'Noor', claimed: false }) });
  let reads = 0;
  const many = g.base.getMultipleAccountsInfo;
  g.base.getMultipleAccountsInfo = async (...a) => { reads++; return many(...a); };
  return { g, wallet, s41, reads: () => reads };
}

test('GET /claims: the wallet\'s members of this season and its lineage, newest first, with the program\'s amount (claimParts) and status; public', async () => {
  const { g, wallet, s41 } = seasons();
  assert.ok(PUBLIC_ROUTES.includes('GET /claims') && IP_LIMITS['GET /claims']);
  const r = await call(g.public, 'GET', `/claims?wallet=${wallet.publicKey.toBase58()}`);
  assert.equal(r.status, 200, JSON.stringify(r.json));
  const season41 = decodeSeason(g.base.accounts.get(s41.season.toBase58()).data);
  const member41 = decodeMember(g.base.accounts.get(s41.member(wallet.publicKey).toBase58()).data);
  const amount = claimParts(season41, member41).total;
  assert.equal(amount, 7_000_000n + 500_000n, 'prize plus a quarter of what is left of the treasury');
  assert.deepEqual(r.json.claims, [
    { seasonId: '42', member: 5, civ: 3, name: 'Noor', amount: '0', claimed: false, status: 'Registering' },
    { seasonId: '41', member: 2, civ: 0, name: 'Ada K.', amount: amount.toString(), claimed: false, status: 'Finalized' },
  ]);
  const none = await call(g.operator, 'GET', `/claims?wallet=${Keypair.generate().publicKey.toBase58()}`);
  assert.deepEqual([none.status, none.json.claims], [200, []]);
});

test('GET /claims: 400 InvalidOwner for a wallet that is not a public key; limited per address', async () => {
  const { g } = seasons();
  for (const q of ['', '?wallet=', '?wallet=not-a-key', '?owner=11111111111111111111111111111111']) {
    const r = await call(g.public, 'GET', `/claims${q}`, { ip: '10.9.0.1' });
    assert.deepEqual([r.status, r.json.code], [400, 'InvalidOwner'], q);
  }
  const { burst } = IP_LIMITS['GET /claims'];
  let last;
  for (let k = 0; k < burst; k++) last = await call(g.public, 'GET', '/claims?wallet=x', { ip: '10.9.0.2' });
  assert.equal(last.status, 400);
  const r = await call(g.public, 'GET', '/claims?wallet=x', { ip: '10.9.0.2' });
  assert.deepEqual([r.status, r.json.code], [429, 'RateLimited']);
});

test('GET /claims: cached 5 s per wallet; a claim relayed through the gateway refreshes it', async () => {
  let now = 1_000_000;
  const { g, wallet, s41, reads } = seasons();
  g.ctx.claims.now = () => now;
  const q = `/claims?wallet=${wallet.publicKey.toBase58()}`;
  await call(g.public, 'GET', q, { ip: '10.9.1.1' });
  await call(g.public, 'GET', q, { ip: '10.9.1.2' });
  assert.equal(reads(), 1, 'one read for both');
  now += 5000;
  await call(g.public, 'GET', q, { ip: '10.9.1.3' });
  assert.equal(reads(), 2);
  // The wallet claims season 41 through /claim-relay: the next GET /claims reads again.
  const dest = Keypair.generate().publicKey;
  const r = await call(g.public, 'POST', '/claim-relay', { body: { tx: b64(tx(s41.claim({ wallet: wallet.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [wallet])) }, ip: '10.9.1.4' });
  assert.equal(r.status, 200, JSON.stringify(r.json));
  g.base.accounts.set(s41.member(wallet.publicKey).toBase58(), { data: memberData({ seasonId: 41n, index: 2, civ: 0, wallet: wallet.publicKey, name: 'Ada K.', shares: 1_000_000n, claimed: true }) });
  const after = await call(g.public, 'GET', q, { ip: '10.9.1.5' });
  assert.equal(reads(), 3);
  assert.equal(after.json.claims.find(c => c.seasonId === '41').claimed, true);
});

test('POST /faucet refreshes the owner\'s GET /usdc (cached 5 s), so the balance shows at once', async () => {
  const REG = { seconds: 600, openedAt: Date.now() - 1000, closesAt: Date.now() + 600_000, waitExternal: 0, entryFee: '10000000', deposit: '0' };
  const tokenAccounts = [];
  const g = gateway({ state: { registration: REG }, base: { tokenAccounts } });
  const owner = Keypair.generate().publicKey;
  const usdc = ip => call(g.public, 'GET', `/usdc?owner=${owner.toBase58()}`, { ip });
  assert.deepEqual((await usdc('10.9.2.1')).json.accounts, [], 'connected: nothing yet (now cached)');
  const f = await call(g.public, 'POST', '/faucet', { body: { owner: owner.toBase58() }, ip: '10.9.2.2' });
  assert.equal(f.status, 200, JSON.stringify(f.json));
  tokenAccounts.push({ owner: owner.toBase58(), address: f.json.usdcAccount, data: tokenAccountData({ mint: g.mint, owner, amount: 10_000_000n }) });
  const after = await usdc('10.9.2.3');
  assert.deepEqual(after.json.accounts, [{ address: f.json.usdcAccount, amount: '10000000' }], 'within 5 s of the first read');
  // Asking again ("already funded") refreshes it too.
  tokenAccounts[0].data = tokenAccountData({ mint: g.mint, owner, amount: 0n });
  await call(g.public, 'POST', '/faucet', { body: { owner: owner.toBase58() }, ip: '10.9.2.4' });
  assert.deepEqual((await usdc('10.9.2.5')).json.accounts, [{ address: f.json.usdcAccount, amount: '0' }]);
});
