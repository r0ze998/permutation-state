// POST /x402/join hardened for the public: what it co-signs, what it
// refuses before anything is simulated or sent, the exact bytes it
// simulates (signatures checked) and sends, and the 402's requirements.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ComputeBudgetProgram, Keypair, PublicKey } from '@solana/web3.js';
import { fromBase64 } from '../client/src/bytes.mjs';
import { NOBODY } from '../client/src/codec.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { verifyTalk } from '../client/src/talk-node.mjs';
import { decodeRegister } from '../src/routes/x402.mjs';
import { call, gateway, memberData, payment, tx } from './gateway-fixtures.mjs';

/** A person's Register (the wallet and the session key sign; the gateway pays), as the browser and the SDK build it. */
function register(g, { wallet = Keypair.generate(), session = Keypair.generate(), kind = 2, civ = 1, feePayer = g.crankKey.publicKey, extra = [], deposit = 0n } = {}) {
  const ixs = g.chain.register({ wallet: wallet.publicKey, feePayer, civ, walletToken: Keypair.generate().publicKey, mint: g.mint, name: 'Ada K.', kind,
    session: session.publicKey, stand: 3, deposit });
  return { wallet, session, t: tx([...extra, ...ixs], feePayer, [wallet, session]) };
}

/** Settles: the member account appears once the transaction is sent. */
function settling(g, { index = 5, civ = 1 } = {}) {
  g.base.sendRawTransaction = async raw => {
    g.base.sent.push(new Uint8Array(raw));
    const p = parseTransaction(new Uint8Array(raw));
    const wallet = p.instructions[0].keys[0].pubkey;
    const reg = decodeRegister(p.instructions[0].data);
    g.base.accounts.set(g.chain.member(new PublicKey(wallet)).toBase58(), { data: memberData({ index, civ, wallet, session: reg.session, kind: reg.kind, name: reg.name }) });
    return 'x'.repeat(64);
  };
}

const join = (g, t, body = { civ: 1 }) => call(g.public, 'POST', '/x402/join', { body, headers: payment(t) });

test('/x402/join: the 402 names the season, its mint, crank, deposit, AI members and deadline; the amount is fee + deposit', async () => {
  const closesAt = Date.now() + 60_000;
  const g = gateway({ season: { aiCount: 12, entryFee: 10_000_000n }, state: { registration: { seconds: 600, openedAt: closesAt - 600_000, closesAt, waitExternal: 0, entryFee: '10000000', deposit: '5000000' } } });
  const r = await call(g.public, 'POST', '/x402/join', { body: {} });
  assert.equal(r.status, 402);
  const req = r.json.accepts[0];
  assert.equal(req.maxAmountRequired, '15000000');
  assert.equal(req.asset, g.mint.toBase58());
  assert.deepEqual([req.extra.aiCount, req.extra.deposit, req.extra.closesAt, req.extra.usdcMint, req.extra.crank, req.extra.feePayer],
    [12, '5000000', closesAt, g.mint.toBase58(), g.crankKey.publicKey.toBase58(), g.crankKey.publicKey.toBase58()]);
  assert.equal(req.extra.accounts.season, g.chain.season.toBase58());
  assert.equal(r.headers['Access-Control-Expose-Headers'], 'X-PAYMENT-RESPONSE');
});

test('/x402/join: a valid payment is simulated exactly as signed (signatures checked), then those bytes are sent and the member recorded', async () => {
  const g = gateway({ season: { aiCount: 2 } });
  settling(g);
  const { t, wallet, session } = register(g);
  const r = await join(g, t);
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.deepEqual([r.json.ok, r.json.member, r.json.civ], [true, 5, 1]);
  assert.equal(g.base.simulated.length, 1);
  assert.deepEqual(g.base.simulated[0].config, { sigVerify: true, replaceRecentBlockhash: false, commitment: 'confirmed' });
  assert.equal(g.base.sent.length, 1);
  assert.deepEqual(g.base.sent[0], g.base.simulated[0].wire, 'the simulated bytes are the ones sent');
  const sent = parseTransaction(g.base.sent[0]);
  assert.equal(sent.signers[0], g.crankKey.publicKey.toBase58());
  assert.deepEqual(sent.signers.slice(1).sort(), [wallet.publicKey.toBase58(), session.publicKey.toBase58()].sort(), 'the wallet and the session key signed');
  for (const [i, k] of sent.signers.entries()) assert.ok(verifyTalk(sent.message, sent.signatures[i], new PublicKey(k).toBytes()), `signature ${i} valid`);
  assert.deepEqual(g.store.state.members, [{ index: 5, civ: 1, name: 'Ada K.', kind: 2, hosted: 'external', wallet: wallet.publicKey.toBase58(), session: session.publicKey.toBase58() }]);
  const receipt = JSON.parse(Buffer.from(r.headers['X-PAYMENT-RESPONSE'], 'base64').toString());
  assert.deepEqual([receipt.success, receipt.payer], [true, wallet.publicKey.toBase58()]);
});

test('/x402/join: a compute-budget instruction, another fee payer or a bad wallet signature is refused before anything is simulated or sent', async () => {
  const g = gateway();
  const budget = register(g, { extra: [ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 10_000_000 })] });
  let r = await join(g, budget.t);
  assert.deepEqual([r.status, r.json.code], [402, 'InvalidPayment']);
  assert.match(r.json.error, /no compute-budget/);
  const limit = register(g, { extra: [ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 })] });
  assert.equal((await join(g, limit.t)).status, 402);
  const forged = register(g);
  forged.t.signatures.find(s => s.publicKey.equals(forged.wallet.publicKey)).signature = Buffer.alloc(64, 7);
  r = await join(g, forged.t);
  assert.equal(r.status, 402);
  assert.match(r.json.error, /payer signature/);
  // The wallet pays its own fee: the gateway is not the fee payer.
  const own = register(g, { feePayer: Keypair.generate().publicKey });
  own.t.feePayer = own.wallet.publicKey;
  own.t.signatures = [];
  own.t.partialSign(own.wallet);
  r = await join(g, own.t);
  assert.equal(r.status, 402);
  assert.match(r.json.error, /fee payer/);
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [0, 0]);
});

test('/x402/join: kind must be 2 in a season with AI members (409 KindHidden); any kind without', async () => {
  const g = gateway({ season: { aiCount: 3 } });
  for (const kind of [0, 1]) {
    const r = await join(g, register(g, { kind }).t);
    assert.deepEqual([r.status, r.json.code], [409, 'KindHidden']);
  }
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [0, 0]);
  const open = gateway({ season: { aiCount: 0 } });
  settling(open);
  assert.equal((await join(open, register(open, { kind: 0 }).t)).status, 200);
});

test('/x402/join: a session key a member (or a planned AI member) has is refused, read fresh (409 SessionInUse); so is a member\'s wallet', async () => {
  const g = gateway();
  const session = Keypair.generate();
  g.base.members.push(memberData({ index: 0, session: session.publicKey }));
  let r = await join(g, register(g, { session }).t);
  assert.deepEqual([r.status, r.json.code], [409, 'SessionInUse']);
  assert.ok(g.base.calls.getProgramAccounts >= 1, 'read from the chain, not the cached registry');
  const planned = Keypair.generate();
  g.store.state.aiPlan = [{ pos: 0, session: planned.publicKey.toBase58() }];
  r = await join(g, register(g, { session: planned }).t);
  assert.deepEqual([r.status, r.json.code], [409, 'SessionInUse']);
  const member = register(g);
  g.base.accounts.set(g.chain.member(member.wallet.publicKey).toBase58(), { data: memberData() });
  r = await join(g, member.t);
  assert.deepEqual([r.status, r.json.code], [409, 'AlreadyInitialized']);
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [0, 0]);
});

test('/x402/join: two registrations with one session key in flight at once: the second is refused', async () => {
  const g = gateway();
  let release;
  const gate = new Promise(r => { release = r; });
  settling(g);
  const send = g.base.sendRawTransaction;
  g.base.sendRawTransaction = async raw => { await gate; return send(raw); };
  const session = Keypair.generate();
  const first = join(g, register(g, { session }).t);
  await new Promise(r => setTimeout(r, 20));
  const second = await join(g, register(g, { session }).t);
  assert.deepEqual([second.status, second.json.code], [409, 'RegistrationInFlight']);
  release();
  assert.equal((await first).status, 200);
});

test('/x402/join: closed after closesAt (409 RegistrationClosed, the 402 too); full with the AI members still to come (409 SeasonFull)', async () => {
  const past = Date.now() - 1;
  const g = gateway({ state: { registration: { seconds: 60, openedAt: past - 60_000, closesAt: past, waitExternal: 0, entryFee: '10000000', deposit: '0' } } });
  let r = await call(g.public, 'POST', '/x402/join', { body: {} });
  assert.deepEqual([r.status, r.json.code], [409, 'RegistrationClosed']);
  r = await join(g, register(g).t);
  assert.deepEqual([r.status, r.json.code], [409, 'RegistrationClosed']);
  const full = gateway({ season: { memberCount: 250 }, state: { aiPlan: Array.from({ length: 6 }, (_, pos) => ({ pos })) } });
  r = await call(full.public, 'POST', '/x402/join', { body: {} });
  assert.deepEqual([r.status, r.json.code], [409, 'SeasonFull']);
  const room = gateway({ season: { memberCount: 249 }, state: { aiPlan: Array.from({ length: 6 }, (_, pos) => ({ pos })) } });
  assert.equal((await call(room.public, 'POST', '/x402/join', { body: {} })).status, 402);
});

test('/x402/join: a payment the simulation refuses is never sent; the 402 says why (program error, insufficient funds, expired blockhash)', async () => {
  const cases = [
    [{ err: { InstructionError: [0, { Custom: 6 }] }, logs: ['Program J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n failed: custom program error: 0x6'] }, 'WrongStatus'],
    [{ err: { InstructionError: [0, { Custom: 1 }] }, logs: ['Program log: Error: insufficient funds', 'Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: custom program error: 0x1'] }, 'InsufficientFunds'],
    [{ err: 'BlockhashNotFound', logs: [] }, 'BlockhashExpired'],
  ];
  for (const [result, code] of cases) {
    const g = gateway({ base: { simulate: () => result } });
    const r = await join(g, register(g).t);
    assert.deepEqual([r.status, r.json.code], [402, code], code);
    assert.ok(r.json.accepts, 'with the requirements to try again');
    assert.deepEqual([g.base.simulated.length, g.base.sent.length], [1, 0]);
  }
});

test('decodeRegister reads exactly the Register data (the session key after the name)', () => {
  const g = gateway();
  const session = Keypair.generate().publicKey;
  const ix = g.chain.register({ wallet: Keypair.generate().publicKey, feePayer: g.crankKey.publicKey, civ: 3, walletToken: session, mint: g.mint, name: 'Zoë', kind: 2, session, stand: 5, deposit: 7n })[0];
  const d = decodeRegister(new Uint8Array(ix.data));
  assert.deepEqual([d.civ, d.name, d.kind, d.stand, d.votes, d.deposit], [3, 'Zoë', 2, 5, [NOBODY, NOBODY, NOBODY, NOBODY], 7n]);
  assert.deepEqual(d.session, session.toBytes());
  assert.equal(decodeRegister(new Uint8Array([...ix.data, 0])), null, 'trailing bytes');
  assert.equal(decodeRegister(new Uint8Array(ix.data).slice(0, 40)), null, 'truncated');
  assert.equal(fromBase64('AA==').length, 1);
});

test('/x402/join in a season with AI members: everyone registers the same way (the public deposit, no votes, no attestation, 1–2 offices), else 409 UniformRegistration before anything is simulated', async () => {
  const closesAt = Date.now() + 60_000;
  const reg = { seconds: 600, openedAt: closesAt - 600_000, closesAt, waitExternal: 0, entryFee: '10000000', deposit: '2500000' };
  const person = (g, o = {}) => {
    const wallet = Keypair.generate(), session = Keypair.generate();
    const ixs = g.chain.register({ wallet: wallet.publicKey, feePayer: g.crankKey.publicKey, civ: 1, walletToken: Keypair.generate().publicKey, mint: g.mint, name: 'Ada K.', kind: 2,
      session: session.publicKey, stand: 3, deposit: 2_500_000n, ...o });
    return tx(ixs, g.crankKey.publicKey, [wallet, session]);
  };
  let n = 0;
  const join = (g, t) => call(g.public, 'POST', '/x402/join', { body: { civ: 1 }, headers: payment(t), ip: `10.8.0.${++n}` });
  const g = gateway({ season: { aiCount: 4 }, state: { registration: reg } });
  settling(g);
  const cases = {
    'no deposit': { deposit: 0n }, 'another deposit': { deposit: 9n }, 'a pre-season vote': { votes: [3, NOBODY, NOBODY, NOBODY] },
    'an attestation': { attestation: new Uint8Array(32).fill(1) }, 'no office': { stand: 0 }, 'three offices': { stand: 7 }, 'an unknown office bit': { stand: 1 | 16 },
  };
  for (const [name, o] of Object.entries(cases)) {
    const r = await join(g, person(g, o));
    assert.deepEqual([r.status, r.json.code], [409, 'UniformRegistration'], name);
  }
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [0, 0]);
  for (const stand of [1, 2, 4, 8, 3, 12]) assert.equal((await join(g, person(g, { stand }))).status, 200, `stand ${stand}`);
  // Without AI members nobody hides among them: any candidacy, vote or deposit goes.
  const plain = gateway({ state: { registration: reg } });
  settling(plain);
  assert.equal((await join(plain, person(plain, { deposit: 0n, stand: 0, votes: [3, NOBODY, NOBODY, NOBODY] }))).status, 200);
});
