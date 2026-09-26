// POST /relay and /claim-relay: exactly the shapes members send, their
// signature checked, the exact bytes simulated before the gateway pays;
// claims for this season or one in its lineage, optionally creating the
// wallet's associated token account first.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ComputeBudgetProgram, Keypair, PublicKey, TransactionInstruction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { createAtaIdempotentIx, associatedTokenAccount } from '../src/spl.mjs';
import { b64, call, gateway, memberData, programId, seasonData, tx } from './gateway-fixtures.mjs';

// Each request from its own address (the per-address limits are tested in public.test.mjs).
let n = 0;
const ip = () => `10.0.${(++n >> 8) & 255}.${n & 255}`;
const relay = (g, t) => call(g.public, 'POST', '/relay', { body: { tx: b64(t) }, ip: ip() });
const claim = (g, t) => call(g.public, 'POST', '/claim-relay', { body: { tx: b64(t) }, ip: ip() });
const votes = n => Array.from({ length: n }, (_, i) => ({ type: 'Vote', role: ['General', 'Steward', 'Science', 'Diplomat'][i % 4], candidate: i }));

test('/relay: 1..8 SubmitGov of one signer, one CommitOrders or one RevealOrders, after the heap frame; simulated exactly, then sent', async () => {
  const g = gateway();
  const session = Keypair.generate();
  const ok = [
    g.chain.submitGovMany({ signer: session.publicKey, civ: 2, member: 3, actions: votes(1) }),
    g.chain.submitGovMany({ signer: session.publicKey, civ: 2, member: 3, actions: votes(8) }),
    g.chain.commitOrders({ signer: session.publicKey, civ: 0, role: 'General', tick: 4, commitment: new Uint8Array(32).fill(1) }),
    g.chain.revealOrders({ signer: session.publicKey, civ: 0, role: 'General', tick: 4, decisionDigest: new Uint8Array(32).fill(2), orders: [], salt: new Uint8Array(32).fill(3) }),
  ];
  for (const [i, ixs] of ok.entries()) {
    const r = await relay(g, tx(ixs, g.crankKey.publicKey, [session]));
    assert.equal(r.status, 200, `${i}: ${JSON.stringify(r.json)}`);
  }
  assert.equal(g.er.simulated.length, ok.length);
  assert.ok(g.er.simulated.every(s => s.config.sigVerify === true && s.config.replaceRecentBlockhash === false));
  assert.deepEqual(g.er.sent, g.er.simulated.map(s => s.wire));
  assert.deepEqual(parseTransaction(g.er.sent[0]).signers, [g.crankKey.publicKey.toBase58(), session.publicKey.toBase58()]);
});

test('/relay refuses other shapes before simulating: mixed signers, 9 actions, no or another heap frame, a priority fee, another season\'s nation, the gateway as signer', async () => {
  const g = gateway();
  const a = Keypair.generate(), b = Keypair.generate();
  const other = new ChainClient(programId, 43n);
  const gov = (signer, civ = 1, chain = g.chain, n = 1) => chain.submitGovMany({ signer: signer.publicKey, civ, member: 1, actions: votes(n) });
  const bare = gov(a).slice(1);
  const cases = {
    'mixed signers': tx([...gov(a), ...gov(b).slice(1)], g.crankKey.publicKey, [a, b]),
    'nine actions': tx(gov(a, 1, g.chain, 9), g.crankKey.publicKey, [a]),
    'no heap frame': tx(bare, g.crankKey.publicKey, [a]),
    'another heap frame': tx([ComputeBudgetProgram.requestHeapFrame({ bytes: 256 * 1024 }), ...bare], g.crankKey.publicKey, [a]),
    'a priority fee': tx([...gov(a), ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1e9 })], g.crankKey.publicKey, [a]),
    'another season\'s nation': tx(gov(a, 1, other), g.crankKey.publicKey, [a]),
    'the gateway signs the instruction': tx(gov(g.crankKey), g.crankKey.publicKey, [g.crankKey]),
    'someone else pays': tx(gov(a), a.publicKey, [a]),
    'commit and gov together': tx([...gov(a), ...g.chain.commitOrders({ signer: a.publicKey, civ: 1, role: 'General', tick: 1, commitment: new Uint8Array(32).fill(1) }).slice(1)], g.crankKey.publicKey, [a]),
    'a writable signer': tx([gov(a)[0], new TransactionInstruction({ programId: new PublicKey(programId), keys: [{ pubkey: a.publicKey, isSigner: true, isWritable: true }, { pubkey: g.chain.nation(1), isSigner: false, isWritable: true }], data: Buffer.from(bare[0].data) })], g.crankKey.publicKey, [a]),
  };
  for (const [name, t] of Object.entries(cases)) {
    const r = await relay(g, t);
    assert.deepEqual([r.status, r.json.code], [400, 'RelayRejected'], name);
  }
  const forged = tx(gov(a), g.crankKey.publicKey, [a]);
  forged.signatures[1].signature = Buffer.alloc(64, 1);
  const r = await relay(g, forged);
  assert.deepEqual([r.status, r.json.code], [400, 'BadSignature']);
  assert.deepEqual([g.er.simulated.length, g.er.sent.length], [0, 0]);
});

test('/relay: at most 8 CommitOrders per office, tick and signer (429 TooManyCommits)', async () => {
  const g = gateway();
  const s = Keypair.generate();
  const commit = (tick, role = 'General', fill = 1) => tx(g.chain.commitOrders({ signer: s.publicKey, civ: 0, role, tick, commitment: new Uint8Array(32).fill(fill) }), g.crankKey.publicKey, [s]);
  for (let k = 0; k < 8; k++) assert.equal((await relay(g, commit(5, 'General', k + 1))).status, 200);
  const r = await relay(g, commit(5, 'General', 9));
  assert.deepEqual([r.status, r.json.code], [429, 'TooManyCommits']);
  assert.equal((await relay(g, commit(5, 'Steward'))).status, 200, 'another office');
  assert.equal((await relay(g, commit(6))).status, 200, 'the next tick');
});

test('/relay: a transaction the ER simulation refuses is not sent; the program error is the answer', async () => {
  const g = gateway({ er: { simulate: () => ({ err: { InstructionError: [1, { Custom: 14 }] }, logs: [`Program ${programId} failed: custom program error: 0xe`] }) } });
  const s = Keypair.generate();
  const r = await relay(g, tx(g.chain.commitOrders({ signer: s.publicKey, civ: 0, role: 'General', tick: 1, commitment: new Uint8Array(32).fill(1) }), g.crankKey.publicKey, [s]));
  assert.deepEqual([r.status, r.json.code], [409, 'WrongTick']);
  assert.deepEqual([g.er.simulated.length, g.er.sent.length], [1, 0]);
});

/** A gateway whose season 42 follows season 41 (finalized, in the lineage); `wallet` is a member of 41. */
function lineage() {
  const g = gateway({ state: { lineage: [{ seasonId: '41', historyRoot: 'ab' }] } });
  const prev = new ChainClient(programId, 41n);
  g.base.accounts.set(prev.season.toBase58(), { data: seasonData({ seasonId: 41n, status: 'Finalized', usdcMint: g.mint, crank: g.crankKey.publicKey }) });
  const wallet = Keypair.generate();
  g.base.accounts.set(prev.member(wallet.publicKey).toBase58(), { data: memberData({ seasonId: 41n, index: 2, wallet: wallet.publicKey }) });
  return { g, prev, wallet };
}

test('/claim-relay: a claim for a season in the lineage, optionally creating the wallet\'s associated token account (the gateway pays) it pays into', async () => {
  const { g, prev, wallet } = lineage();
  const dest = Keypair.generate().publicKey;
  let r = await claim(g, tx(prev.claim({ wallet: wallet.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [wallet]));
  assert.equal(r.status, 200, JSON.stringify(r.json));
  const ata = associatedTokenAccount(wallet.publicKey, g.mint);
  const create = createAtaIdempotentIx({ payer: g.crankKey.publicKey, owner: wallet.publicKey, mint: g.mint });
  r = await claim(g, tx([create, ...prev.claim({ wallet: wallet.publicKey, dest: ata, mint: g.mint })], g.crankKey.publicKey, [wallet]));
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [2, 2]);
  assert.ok(g.base.simulated.every(s => s.config.sigVerify));
  const status = await call(g.public, 'GET', '/claim-relay?season=41');
  assert.deepEqual([status.json.season, status.json.status, status.json.mint], ['41', 'Finalized', g.mint.toBase58()]);
  assert.equal((await call(g.public, 'GET', '/claim-relay')).json.season, '42');
  const unknown = await call(g.public, 'GET', '/claim-relay?season=40');
  assert.deepEqual([unknown.status, unknown.json.code], [404, 'UnknownSeason']);
});

test('/claim-relay refuses: a season outside the lineage, an account created for someone else or not paid into, another mint, a non-member', async () => {
  const { g, prev, wallet } = lineage();
  const ata = associatedTokenAccount(wallet.publicKey, g.mint);
  const stranger = Keypair.generate().publicKey;
  const create = (o = {}) => createAtaIdempotentIx({ payer: g.crankKey.publicKey, owner: wallet.publicKey, mint: g.mint, ...o });
  const claimIxs = (chain = prev, dest = ata, mint = g.mint) => chain.claim({ wallet: wallet.publicKey, dest, mint });
  const cases = {
    'another season': tx(claimIxs(new ChainClient(programId, 40n)), g.crankKey.publicKey, [wallet]),
    'someone else\'s account': tx([create({ owner: stranger }), ...claimIxs(prev, associatedTokenAccount(stranger, g.mint))], g.crankKey.publicKey, [wallet]),
    'created but paid elsewhere': tx([create(), ...claimIxs(prev, Keypair.generate().publicKey)], g.crankKey.publicKey, [wallet]),
    'the wallet pays the rent': tx([create({ payer: wallet.publicKey }), ...claimIxs()], g.crankKey.publicKey, [wallet]),
    'two claims': tx([...claimIxs(), ...claimIxs()], g.crankKey.publicKey, [wallet]),
    'a priority fee': tx([ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 5 }), ...claimIxs()], g.crankKey.publicKey, [wallet]),
    'another mint': tx(claimIxs(prev, ata, Keypair.generate().publicKey), g.crankKey.publicKey, [wallet]),
  };
  for (const [name, t] of Object.entries(cases)) {
    const r = await claim(g, t);
    assert.deepEqual([r.status, r.json.code], [400, 'RelayRejected'], name);
  }
  const outsider = Keypair.generate();
  const r = await claim(g, tx(prev.claim({ wallet: outsider.publicKey, dest: ata, mint: g.mint }), g.crankKey.publicKey, [outsider]));
  assert.deepEqual([r.status, r.json.code], [404, 'NoSuchMember']);
  assert.deepEqual([g.base.simulated.length, g.base.sent.length], [0, 0]);
});

// A signer's rate limit is charged only for transactions that signer really
// signed, and once per signature: forged or replayed floods naming a victim's
// key or wallet leave the victim's own requests untouched.
const commitBy = (g, key, fill, tick = 3) => tx(g.chain.commitOrders({ signer: key.publicKey, civ: 0, role: 'General', tick, commitment: new Uint8Array(32).fill(fill) }), g.crankKey.publicKey, [key]);
const govBy = (g, key, n) => tx(g.chain.submitGovMany({ signer: key.publicKey, civ: 1, member: 3, actions: votes(n) }), g.crankKey.publicKey, [key]);
/** `t` with the member's signature replaced by `sig` (zeros: unsigned). */
const forge = (t, sig = Buffer.alloc(64)) => { t.signatures[1].signature = sig; return t; };

test('/relay: 30 forged relays naming key K (from many addresses) cost K nothing; K\'s own relay still goes through', async () => {
  const g = gateway();
  const k = Keypair.generate();
  const other = Keypair.generate();
  for (let i = 0; i < 30; i++) {
    // Unsigned, garbage, or someone else's signature: never K's.
    const t = i % 3 === 0 ? forge(commitBy(g, k, i + 1))
      : i % 3 === 1 ? forge(govBy(g, k, 1 + (i % 8)), Buffer.alloc(64, i))
        : forge(commitBy(g, k, i + 1), Buffer.from(govBy(g, other, 1).signatures[1].signature));
    const r = await relay(g, t);
    assert.deepEqual([r.status, r.json.code], [400, 'BadSignature'], `forged ${i}`);
  }
  assert.equal(g.ctx.limiter.buckets.has(`relay:${k.publicKey.toBase58()}`), false, 'K\'s bucket untouched');
  const r = await relay(g, commitBy(g, k, 99));
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.equal(g.er.simulated.length, 1, 'no forged one was simulated');
});

test('/relay: replays of K\'s relayed transaction (copied from the chain) are 409 Duplicate and cost K nothing; K goes on relaying', async () => {
  const g = gateway();
  const k = Keypair.generate();
  const mine = commitBy(g, k, 1);
  assert.equal((await relay(g, mine)).status, 200);
  const onChain = Buffer.from(g.er.sent[0]).toString('base64'); // as anyone reads it back from the ER
  for (let i = 0; i < 30; i++) {
    const r = await call(g.public, 'POST', '/relay', { body: { tx: onChain }, ip: ip() });
    assert.deepEqual([r.status, r.json.code], [409, 'Duplicate'], `replay ${i}`);
  }
  for (let i = 0; i < 10; i++) {
    const r = await relay(g, mine);
    assert.deepEqual([r.status, r.json.code], [409, 'Duplicate'], `resent ${i}`);
  }
  assert.equal(g.er.simulated.length, 1, 'no replay was simulated');
  // Burst 24: had the 40 replays been charged, K would be at 429 by now.
  for (let n = 1; n <= 8; n++) assert.equal((await relay(g, govBy(g, k, n))).status, 200, `fresh gov ${n}`);
  for (let fill = 2; fill <= 8; fill++) assert.equal((await relay(g, commitBy(g, k, fill))).status, 200, `fresh commit ${fill}`);
  assert.equal(g.er.sent.length, 16);
});

test('/claim-relay: the order is shape, mint, signature, the wallet\'s limit, then its member account: forged claims naming W cost W nothing (nor a read of its account)', async () => {
  const { g, prev, wallet } = lineage();
  const dest = Keypair.generate().publicKey;
  const memberReads = () => g.base.memberReads;
  const get = g.base.getAccountInfo;
  g.base.memberReads = 0;
  const memberKey = prev.member(wallet.publicKey).toBase58();
  g.base.getAccountInfo = async key => { if (new PublicKey(key).toBase58() === memberKey) g.base.memberReads++; return get(key); };
  for (let i = 0; i < 30; i++) {
    const r = await claim(g, forge(tx(prev.claim({ wallet: wallet.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [wallet]), Buffer.alloc(64, i + 1)));
    assert.deepEqual([r.status, r.json.code], [400, 'BadSignature'], `forged ${i}`);
  }
  assert.equal(memberReads(), 0, 'W\'s member account was never read for them');
  assert.equal(g.ctx.limiter.buckets.has(`claim:${wallet.publicKey.toBase58()}`), false);
  // A stranger's forged claim is BadSignature too, not NoSuchMember (the member lookup comes after).
  const stranger = Keypair.generate();
  const s = forge(tx(prev.claim({ wallet: stranger.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [stranger]));
  assert.equal((await claim(g, s)).json.code, 'BadSignature');
  const r = await claim(g, tx(prev.claim({ wallet: wallet.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [wallet]));
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.equal(memberReads(), 1);
});

test('/claim-relay: replays of W\'s claim are 409 Duplicate and cost W nothing; W\'s next claim goes through', async () => {
  const { g, prev, wallet } = lineage();
  const dest = Keypair.generate().publicKey;
  const mine = tx(prev.claim({ wallet: wallet.publicKey, dest, mint: g.mint }), g.crankKey.publicKey, [wallet]);
  assert.equal((await claim(g, mine)).status, 200);
  for (let i = 0; i < 10; i++) {
    const r = await call(g.public, 'POST', '/claim-relay', { body: { tx: Buffer.from(g.base.sent[0]).toString('base64') }, ip: ip() });
    assert.deepEqual([r.status, r.json.code], [409, 'Duplicate'], `replay ${i}`);
  }
  // Burst 4: had the replays been charged, this would be 429.
  const ata = associatedTokenAccount(wallet.publicKey, g.mint);
  const create = createAtaIdempotentIx({ payer: g.crankKey.publicKey, owner: wallet.publicKey, mint: g.mint });
  const r = await claim(g, tx([create, ...prev.claim({ wallet: wallet.publicKey, dest: ata, mint: g.mint })], g.crankKey.publicKey, [wallet]));
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.equal(g.base.sent.length, 2);
});
