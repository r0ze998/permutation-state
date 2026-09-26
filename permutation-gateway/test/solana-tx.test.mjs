// Solana transactions without web3.js (client/src/solana-tx.mjs) against
// web3.js 1.99 itself: program addresses and the curve check, the key order
// and bytes of compiled messages, the wire format, and parsing back.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { ComputeBudgetProgram, Keypair, PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js';
import {
  compareKeys, compileMessage, computeBudgetHeapFrame, COMPUTE_BUDGET_PROGRAM, createProgramAddress, findProgramAddress, isOnCurve, messageOf,
  parseMessage, parseTransaction, pubkeyBytes, pubkeyString, wireTransaction,
} from '../client/src/solana-tx.mjs';
import { signTalk } from '../client/src/talk-node.mjs';

const pick = list => list[Math.floor(Math.random() * list.length)];
const rand = n => Math.floor(Math.random() * n);

test('pubkeyBytes / pubkeyString take base58, bytes and PublicKeys; refuse anything else', () => {
  const k = Keypair.generate().publicKey;
  for (const form of [k, k.toBase58(), k.toBytes(), [...k.toBytes()], Buffer.from(k.toBytes())]) {
    assert.deepEqual(pubkeyBytes(form), new Uint8Array(k.toBytes()));
    assert.equal(pubkeyString(form), k.toBase58());
  }
  assert.throws(() => pubkeyBytes(new Uint8Array(31)), /32/);
  assert.throws(() => pubkeyBytes('abc'), /32/);
  assert.throws(() => pubkeyBytes('0OIl'), /base58/);
  assert.throws(() => pubkeyBytes(undefined), TypeError);
});

test('findProgramAddress equals PublicKey.findProgramAddressSync (300 random seed sets)', () => {
  for (let i = 0; i < 300; i++) {
    const program = Keypair.generate().publicKey;
    const seeds = Array.from({ length: rand(5) }, () => randomBytes(pick([0, 1, 2, 8, 16, 31, 32, rand(33)])));
    const [want, wantBump] = PublicKey.findProgramAddressSync(seeds, program);
    const [got, bump] = findProgramAddress(seeds.map(s => new Uint8Array(s)), program.toBase58());
    assert.equal(got, want.toBase58(), `seeds ${seeds.map(s => s.toString('hex'))}`);
    assert.equal(bump, wantBump);
  }
  // String seeds are UTF-8, as Buffer.from(string).
  const program = Keypair.generate().publicKey;
  assert.equal(findProgramAddress(['season', Uint8Array.of(1, 2)], program)[0],
    PublicKey.findProgramAddressSync([Buffer.from('season'), Buffer.from([1, 2])], program)[0].toBase58());
  assert.throws(() => findProgramAddress([new Uint8Array(33)], program), TypeError);
  assert.throws(() => PublicKey.findProgramAddressSync([Buffer.alloc(33)], program), TypeError);
});

test('createProgramAddress refuses exactly the on-curve hashes web3.js refuses', () => {
  let refused = 0;
  for (let i = 0; i < 300; i++) {
    const program = Keypair.generate().publicKey;
    const seeds = [randomBytes(rand(33)), randomBytes(rand(33))];
    let want = null;
    try { want = PublicKey.createProgramAddressSync(seeds, program).toBase58(); } catch { want = null; }
    let got = null;
    try { got = createProgramAddress(seeds, program); } catch { got = null; }
    assert.equal(got, want);
    refused += want === null;
  }
  assert.ok(refused > 50, `about half the hashes are on the curve (${refused}/300 were)`);
});

test('isOnCurve equals PublicKey.isOnCurve: random bytes, wallet keys, and the edge encodings', () => {
  const P = 2n ** 255n - 19n;
  const le = (n, sign = false) => { const b = new Uint8Array(32); let x = n; for (let i = 0; i < 32; i++) { b[i] = Number(x & 0xffn); x >>= 8n; } if (sign) b[31] |= 0x80; return b; };
  const cases = [];
  for (let i = 0; i < 1500; i++) cases.push(new Uint8Array(randomBytes(32)));
  for (let i = 0; i < 100; i++) cases.push(Keypair.generate().publicKey.toBytes());
  for (const y of [0n, 1n, 2n, P - 1n, P - 2n, P, P + 1n, P + 18n, 2n ** 255n - 1n]) cases.push(le(y), le(y, true));
  cases.push(new Uint8Array(32).fill(0xff), new Uint8Array(32).fill(0x7f));
  let on = 0;
  for (const b of cases) {
    const want = PublicKey.isOnCurve(b);
    assert.equal(isOnCurve(b), want, Buffer.from(b).toString('hex'));
    on += want;
  }
  assert.ok(on > 300 && on < cases.length - 300, 'both outcomes are exercised');
  // y = 1 is the identity (x = 0): on the curve, but not with the sign bit set; y = p is not canonical.
  assert.equal(isOnCurve(le(1n)), true);
  assert.equal(isOnCurve(le(1n, true)), false);
  assert.equal(isOnCurve(le(P)), false);
  for (let i = 0; i < 20; i++) assert.equal(isOnCurve(Keypair.generate().publicKey.toBase58()), true);
});

test('compareKeys orders base58 exactly as web3.js does (localeCompare en, caseFirst lower)', () => {
  const opts = { localeMatcher: 'best fit', usage: 'sort', sensitivity: 'variant', ignorePunctuation: false, numeric: false, caseFirst: 'lower' };
  const al = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  const flip = c => (c === c.toLowerCase() ? c.toUpperCase() : c.toLowerCase());
  const word = n => Array.from({ length: n }, () => pick(al)).join('');
  for (let i = 0; i < 50_000; i++) {
    const a = word(1 + rand(6));
    const r = Math.random();
    // unrelated, a case variant, a prefix / extension, or a real key pair
    const b = r < 0.3 ? word(1 + rand(6)) : r < 0.6 ? [...a].map(c => (Math.random() < 0.4 ? flip(c) : c)).join('') : r < 0.8 ? a.slice(0, rand(a.length + 1)) + word(rand(2)) : Keypair.generate().publicKey.toBase58();
    const x = i % 50 === 0 ? Keypair.generate().publicKey.toBase58() : a;
    assert.equal(Math.sign(compareKeys(x, b)), Math.sign(x.localeCompare(b, 'en', opts)), `${x} vs ${b}`);
  }
});

test('computeBudgetHeapFrame is RequestHeapFrame', () => {
  for (const bytes of [32 * 1024, 128 * 1024, 256 * 1024]) {
    const want = ComputeBudgetProgram.requestHeapFrame({ bytes });
    const got = computeBudgetHeapFrame(bytes);
    assert.equal(got.programId, want.programId.toBase58());
    assert.equal(got.programId, COMPUTE_BUDGET_PROGRAM);
    assert.deepEqual(got.keys, []);
    assert.deepEqual(got.data, new Uint8Array(want.data));
  }
});

/** A random instruction set over a small pool of keys (duplicates, merged flags, the payer inside or not). */
function randomTransaction() {
  const pool = Array.from({ length: 2 + rand(9) }, () => Keypair.generate().publicKey);
  const programs = Array.from({ length: 1 + rand(3) }, () => (Math.random() < 0.2 ? pick(pool) : Keypair.generate().publicKey));
  // (A fee payer that is also a program is refused by the runtime and by parseMessage.)
  const payers = pool.filter(k => !programs.includes(k));
  const feePayer = Math.random() < 0.5 && payers.length ? pick(payers) : Keypair.generate().publicKey;
  const ixs = Array.from({ length: 1 + rand(5) }, () => new TransactionInstruction({
    programId: pick(programs),
    keys: Array.from({ length: rand(7) }, () => ({ pubkey: pick(pool), isSigner: Math.random() < 0.3, isWritable: Math.random() < 0.5 })),
    data: randomBytes(rand(60)),
  }));
  return { feePayer, ixs, recentBlockhash: Keypair.generate().publicKey.toBase58() };
}
const ours = ixs => ixs.map(ix => ({ programId: ix.programId.toBase58(), keys: ix.keys.map(k => ({ ...k, pubkey: k.pubkey.toBase58() })), data: new Uint8Array(ix.data) }));
function web3Tx({ feePayer, ixs, recentBlockhash }) {
  const tx = new Transaction().add(...ixs);
  tx.feePayer = feePayer;
  tx.recentBlockhash = recentBlockhash;
  return tx;
}

test('compileMessage is byte-identical to web3.js Transaction.compileMessage().serialize() (600 random transactions)', () => {
  for (let i = 0; i < 600; i++) {
    const t = randomTransaction();
    const want = web3Tx(t).compileMessage().serialize();
    const got = compileMessage({ feePayer: t.feePayer.toBase58(), recentBlockhash: t.recentBlockhash, instructions: ours(t.ixs) });
    assert.deepEqual(Buffer.from(got), want);
    // PublicKeys and web3 instructions are accepted as they are.
    assert.deepEqual(Buffer.from(compileMessage({ feePayer: t.feePayer, recentBlockhash: t.recentBlockhash, instructions: t.ixs })), want);
  }
  assert.throws(() => compileMessage({ feePayer: Keypair.generate().publicKey, instructions: [] }), /recentBlockhash/);
});

test('wireTransaction equals web3.js serialize (signed, and partially signed), and messageOf gives the signed bytes', () => {
  for (let i = 0; i < 100; i++) {
    const payer = Keypair.generate();
    const member = Keypair.generate();
    const program = Keypair.generate().publicKey;
    const ixs = [computeBudgetHeapFrame(128 * 1024), { programId: program.toBase58(), keys: [
      { pubkey: member.publicKey.toBase58(), isSigner: true, isWritable: false }, { pubkey: Keypair.generate().publicKey.toBase58(), isSigner: false, isWritable: true }], data: randomBytes(1 + rand(40)) }];
    const recentBlockhash = Keypair.generate().publicKey.toBase58();
    const message = compileMessage({ feePayer: payer.publicKey, recentBlockhash, instructions: ixs });
    const tx = web3Tx({ feePayer: payer.publicKey, recentBlockhash, ixs: ixs.map(ix => new TransactionInstruction({ programId: new PublicKey(ix.programId),
      keys: ix.keys.map(k => ({ ...k, pubkey: new PublicKey(k.pubkey) })), data: Buffer.from(ix.data) })) });
    tx.partialSign(member);
    // The member signs first; the fee payer's slot stays zero until the gateway signs.
    const memberSig = signTalk(message, member);
    const partial = wireTransaction(message, { [member.publicKey.toBase58()]: memberSig });
    assert.deepEqual(Buffer.from(partial), tx.serialize({ requireAllSignatures: false, verifySignatures: false }));
    tx.partialSign(payer);
    const full = wireTransaction(message, [signTalk(message, payer), memberSig]);
    assert.deepEqual(Buffer.from(full), tx.serialize());
    assert.deepEqual(Buffer.from(messageOf(full)), tx.serializeMessage());
    assert.deepEqual(messageOf(partial), message);
  }
});

test('parseTransaction round-trips and agrees with web3.js Transaction.from', () => {
  for (let i = 0; i < 300; i++) {
    const t = randomTransaction();
    const message = compileMessage({ feePayer: t.feePayer, recentBlockhash: t.recentBlockhash, instructions: t.ixs });
    const n = message[0];
    const sigs = Array.from({ length: n }, () => (Math.random() < 0.3 ? null : new Uint8Array(randomBytes(64))));
    const wire = wireTransaction(message, sigs);
    const p = parseTransaction(wire);
    assert.deepEqual(p.message, message);
    assert.deepEqual(p.signatures, sigs.map(s => s ?? new Uint8Array(64)));
    assert.deepEqual(wireTransaction(p.message, p.signatures), wire);
    assert.equal(p.recentBlockhash, t.recentBlockhash);
    assert.equal(p.accountKeys[0], t.feePayer.toBase58());
    assert.deepEqual(p.signers, p.accountKeys.slice(0, n));
    const w = Transaction.from(Buffer.from(wire));
    assert.equal(p.instructions.length, w.instructions.length);
    p.instructions.forEach((ix, k) => {
      assert.equal(ix.programId, w.instructions[k].programId.toBase58());
      assert.deepEqual(ix.data, new Uint8Array(w.instructions[k].data));
      assert.deepEqual(ix.keys, w.instructions[k].keys.map(m => ({ pubkey: m.pubkey.toBase58(), isSigner: m.isSigner, isWritable: m.isWritable })));
      // The original instruction, with each key's merged flags.
      assert.deepEqual(ix.keys.map(m => m.pubkey), t.ixs[k].keys.map(m => m.pubkey.toBase58()));
      assert.deepEqual(ix.data, new Uint8Array(t.ixs[k].data));
    });
    assert.deepEqual(parseMessage(message), (({ signatures, message: _, ...rest }) => rest)(p));
  }
});

test('parsing refuses what the runtime would not sanitize', () => {
  const payer = Keypair.generate().publicKey;
  const program = Keypair.generate().publicKey;
  const other = Keypair.generate().publicKey;
  const message = compileMessage({ feePayer: payer, recentBlockhash: Keypair.generate().publicKey.toBase58(),
    instructions: [{ programId: program, keys: [{ pubkey: other, isSigner: false, isWritable: true }], data: Uint8Array.of(7) }] });
  const wire = wireTransaction(message, []);
  const keysAt = 4; // header (3) + key count (1)
  const n = message[3];
  const ixAt = keysAt + 32 * n + 32 + 1; // after the keys, the blockhash and the instruction count
  const edit = (f) => { const m = Uint8Array.from(message); f(m); return m; };
  const bad = {
    'trailing bytes': Uint8Array.from([...message, 0]),
    truncated: message.slice(0, message.length - 1),
    versioned: Uint8Array.from([0x80, ...message]),
    'program at index 0': edit(m => { m[ixAt] = 0; }),
    'program index out of range': edit(m => { m[ixAt] = n; }),
    'account index out of range': edit(m => { m[ixAt + 2] = n; }),
    'duplicate keys': edit(m => { m.set(m.subarray(keysAt, keysAt + 32), keysAt + 32); }),
    'no signer': edit(m => { m[0] = 0; }),
    'read-only fee payer': edit(m => { m[1] = 1; }),
  };
  assert.doesNotThrow(() => parseMessage(message));
  for (const [what, m] of Object.entries(bad)) assert.throws(() => parseMessage(m), undefined, what);
  // Signatures: exactly one per required signer; canonical length prefixes only.
  assert.throws(() => parseTransaction(Uint8Array.from([2, ...new Uint8Array(128), ...message])), /2 signatures for 1/);
  assert.throws(() => parseTransaction(Uint8Array.from([0x81, 0x00, ...wire.slice(1)])), /non-canonical/);
  assert.throws(() => wireTransaction(message, [new Uint8Array(64), new Uint8Array(64)]), /2 signatures for 1/);
  assert.throws(() => wireTransaction(message, { [other.toBase58()]: new Uint8Array(64) }), /not a signer/);
  assert.throws(() => wireTransaction(message, [new Uint8Array(63)]), /64 bytes/);
  assert.equal(parseTransaction(wire).signatures.length, 1);
});
