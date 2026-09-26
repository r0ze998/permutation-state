// POST /faucet: while registration is open, exactly the entry fee plus the
// default deposit into the owner's associated token account, once per owner
// per season; the AI members funded by the same function (same transaction
// shape). GET /usdc: an owner's token accounts of the season's mint.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair } from '@solana/web3.js';
import { ASSOCIATED_TOKEN_PROGRAM, ata, SYSTEM_PROGRAM, TOKEN_PROGRAM } from '../client/src/player.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { fundPlanned } from '../src/season.mjs';
import { call, gateway, tokenAccountData } from './gateway-fixtures.mjs';

const REG = { seconds: 600, openedAt: Date.now() - 1000, closesAt: Date.now() + 600_000, waitExternal: 0, entryFee: '10000000', deposit: '2500000' };
let n = 0;
const ask = (g, owner, ip = `10.2.0.${++n % 250}`) => call(g.public, 'POST', '/faucet', { body: { owner: owner.toBase58() }, ip });

/** The shape of a sent funding transaction: signers and, per instruction, program and accounts. */
const shape = raw => {
  const p = parseTransaction(raw);
  return { signers: p.signers, instructions: p.instructions.map(ix => ({ programId: ix.programId, keys: ix.keys.map(k => k.pubkey), data: Array.from(ix.data) })) };
};

test('/faucet: the entry fee plus the default deposit into the owner\'s associated token account (created if missing, the crank pays), once per season', async () => {
  const g = gateway({ season: { entryFee: 10_000_000n }, state: { registration: REG } });
  const owner = Keypair.generate().publicKey;
  const r = await ask(g, owner);
  assert.equal(r.status, 200, JSON.stringify(r.json));
  const account = ata(owner.toBase58(), g.mint.toBase58());
  assert.deepEqual([r.json.usdcAccount, r.json.mint, r.json.amount], [account, g.mint.toBase58(), '12500000']);
  assert.equal(g.base.sent.length, 1);
  const s = shape(g.base.sent[0]);
  const crank = g.crankKey.publicKey.toBase58(), admin = g.keys('admin').publicKey.toBase58();
  assert.deepEqual(s.signers, [crank, admin]);
  assert.deepEqual(s.instructions[0], { programId: ASSOCIATED_TOKEN_PROGRAM, keys: [crank, account, owner.toBase58(), g.mint.toBase58(), SYSTEM_PROGRAM, TOKEN_PROGRAM], data: [1] });
  const amount = Buffer.alloc(8); amount.writeBigUInt64LE(12_500_000n);
  assert.deepEqual(s.instructions[1], { programId: TOKEN_PROGRAM, keys: [g.mint.toBase58(), account, admin], data: [7, ...amount] });
  assert.deepEqual(Object.keys(g.store.state.faucet), [owner.toBase58()], 'kept in the state file');
  assert.ok(g.store.saves >= 1);
  // Again: the same account, nothing minted.
  const again = await ask(g, owner);
  assert.deepEqual([again.status, again.json.usdcAccount, again.json.amount, again.json.note], [200, account, '0', 'already funded']);
  assert.equal(g.base.sent.length, 1);
});

test('/faucet: only while registration is open; never more waiting owners than seats; mainnet has none; a limit per address', async () => {
  const closed = gateway({ season: { status: 'Genesis' }, state: { registration: REG } });
  let r = await ask(closed, Keypair.generate().publicKey);
  assert.deepEqual([r.status, r.json.code], [409, 'RegistrationClosed']);
  const late = gateway({ state: { registration: { ...REG, closesAt: Date.now() - 1 } } });
  assert.equal((await ask(late, Keypair.generate().publicKey)).json.code, 'RegistrationClosed');
  // 250 members and 6 AI members to come: no seat left for another person.
  const full = gateway({ season: { memberCount: 250 }, state: { registration: REG, aiPlan: Array.from({ length: 6 }, (_, pos) => ({ pos })) } });
  r = await ask(full, Keypair.generate().publicKey);
  assert.deepEqual([r.status, r.json.code], [409, 'SeasonFull']);
  // One seat left: one owner may wait for it (an earlier grant to a member does not count).
  const one = gateway({ season: { memberCount: 249 }, state: { registration: REG, aiPlan: Array.from({ length: 6 }, (_, pos) => ({ pos })) } });
  assert.equal((await ask(one, Keypair.generate().publicKey)).status, 200);
  assert.deepEqual((await ask(one, Keypair.generate().publicKey)).json.code, 'SeasonFull');
  const main = gateway({ cfg: { cluster: 'mainnet' }, state: { registration: REG } });
  assert.deepEqual((await ask(main, Keypair.generate().publicKey)).json.code, 'FaucetDisabled');
  const g = gateway({ state: { registration: REG } });
  for (let k = 0; k < 3; k++) assert.equal((await ask(g, Keypair.generate().publicKey, '10.9.9.9')).status, 200);
  r = await ask(g, Keypair.generate().publicKey, '10.9.9.9');
  assert.deepEqual([r.status, r.json.code], [429, 'RateLimited']);
  assert.equal((await ask(g, Keypair.generate().publicKey, '10.9.9.10')).status, 200, 'another address');
  assert.deepEqual((await call(g.public, 'POST', '/faucet', { body: { owner: 'nope' } })).json.code, 'InvalidOwner');
  assert.deepEqual((await call(g.public, 'POST', '/faucet', { body: {} })).json.code, 'InvalidOwner');
});

test('an AI member is funded by the faucet\'s own function: the same transaction shape as a person\'s', async () => {
  const g = gateway({ state: { registration: REG } });
  const person = Keypair.generate().publicKey;
  await ask(g, person);
  const entry = { pos: 0, key: 's42-ai0' };
  const funded = await fundPlanned({ base: g.base, store: g.store, entry, amount: 12_500_000n, keys: g.keys });
  const aiWallet = g.keys('s42-ai0-wallet').publicKey.toBase58();
  assert.equal(funded.account, ata(aiWallet, g.mint.toBase58()));
  const [p, a] = g.base.sent.map(shape);
  const anon = (s, owner) => JSON.stringify(s).replaceAll(owner, 'OWNER').replaceAll(ata(owner, g.mint.toBase58()), 'ACCOUNT');
  assert.equal(anon(a, aiWallet), anon(p, person.toBase58()));
  assert.equal(g.store.state.faucet[aiWallet].ai, true, 'marked in the private state file only');
  assert.equal(await fundPlanned({ base: g.base, store: g.store, entry, amount: 12_500_000n, keys: g.keys }).then(() => g.base.sent.length), 2, 'once per season');
});

test('/usdc: the owner\'s token accounts of the season\'s mint, largest first, cached for 5 s', async () => {
  const owner = Keypair.generate().publicKey.toBase58();
  const a1 = Keypair.generate().publicKey.toBase58(), a2 = Keypair.generate().publicKey.toBase58();
  const g = gateway({ base: { tokenAccounts: [] } });
  g.base.getTokenAccountsByOwner = async (key, filter) => {
    g.base.count('getTokenAccountsByOwner');
    assert.equal(filter.mint.toBase58(), g.mint.toBase58());
    return { value: key.toBase58() !== owner ? [] : [a1, a2].map((address, i) => ({ pubkey: address, account: { data: tokenAccountData({ mint: g.mint, owner, amount: [5, 20][i] }) } })) };
  };
  const r = await call(g.public, 'GET', `/usdc?owner=${owner}`);
  assert.deepEqual(r.json, { mint: g.mint.toBase58(), decimals: 6, accounts: [{ address: a2, amount: '20' }, { address: a1, amount: '5' }] });
  await call(g.public, 'GET', `/usdc?owner=${owner}`);
  assert.equal(g.base.calls.getTokenAccountsByOwner, 1, 'cached');
  assert.deepEqual((await call(g.public, 'GET', `/usdc?owner=${Keypair.generate().publicKey.toBase58()}`)).json.accounts, []);
  assert.deepEqual((await call(g.public, 'GET', '/usdc?owner=zz')).json.code, 'InvalidOwner');
  assert.deepEqual((await call(g.public, 'GET', '/usdc')).json.code, 'InvalidOwner');
});
