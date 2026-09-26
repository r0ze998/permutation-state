// A member's transactions without web3.js (client/src/player.mjs) against
// chain.mjs + web3.js: every program address, and every builder compiled to
// the same message and signed to the same wire bytes; the seal receipt's
// layout.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { ComputeBudgetProgram, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { IX, MAX_GOV_PER_SIGNER, NOBODY, ROLES } from '../client/src/codec.mjs';
import { memberPda, nationPda, seasonPda, TOKEN_PROGRAM_ID, vaultPda } from '../client/src/pda.mjs';
import {
  ASSOCIATED_TOKEN_PROGRAM, ata, claimIx, commitOrdersIxs, COMPUTE_BUDGET_PROGRAM, createAtaIdempotentIx, pda, registerIx, revealOrdersIxs,
  sealMessage, submitGovIxs, SYSTEM_PROGRAM, TOKEN_PROGRAM,
} from '../client/src/player.mjs';
import { compileMessage, PACKET_BYTES, parseMessage, wireTransaction } from '../client/src/solana-tx.mjs';
import { signTalk as ed25519 } from '../client/src/talk-node.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { vectors } from './vectors.mjs';

const key = () => Keypair.generate().publicKey;
const programId = DEFAULTS.programId;
const seasonId = 1_790_000_000_123n;
const chain = new ChainClient(programId, seasonId);

/**
 * Both builds of one transaction compile to the same message bytes, and
 * signed (first by `member`, then by the fee payer, as the relays do) to the
 * same wire bytes. `feePayer` and `member` are Keypairs.
 */
function same(web3Ixs, ours, feePayer, member = null) {
  const recentBlockhash = key().toBase58();
  const tx = new Transaction().add(...web3Ixs);
  tx.feePayer = feePayer.publicKey;
  tx.recentBlockhash = recentBlockhash;
  const want = tx.compileMessage().serialize();
  const got = compileMessage({ feePayer: feePayer.publicKey.toBase58(), recentBlockhash, instructions: ours });
  assert.deepEqual(Buffer.from(got), want);
  const sigs = {};
  if (member) {
    tx.partialSign(member);
    sigs[member.publicKey.toBase58()] = ed25519(got, member);
    assert.deepEqual(Buffer.from(wireTransaction(got, sigs)), tx.serialize({ requireAllSignatures: false, verifySignatures: false }));
  }
  tx.partialSign(feePayer);
  sigs[feePayer.publicKey.toBase58()] = ed25519(got, feePayer);
  assert.deepEqual(Buffer.from(wireTransaction(got, sigs)), tx.serialize());
  // And instruction for instruction.
  assert.equal(ours.length, web3Ixs.length);
  ours.forEach((ix, i) => {
    assert.equal(ix.programId, web3Ixs[i].programId.toBase58());
    assert.deepEqual(ix.keys, web3Ixs[i].keys.map(k => ({ pubkey: k.pubkey.toBase58(), isSigner: k.isSigner, isWritable: k.isWritable })));
    assert.deepEqual(new Uint8Array(ix.data), new Uint8Array(web3Ixs[i].data));
  });
  return got;
}

test('program constants', () => {
  assert.equal(TOKEN_PROGRAM, TOKEN_PROGRAM_ID.toBase58());
  assert.equal(SYSTEM_PROGRAM, SystemProgram.programId.toBase58());
  assert.equal(COMPUTE_BUDGET_PROGRAM, ComputeBudgetProgram.programId.toBase58());
  assert.equal(ASSOCIATED_TOKEN_PROGRAM, 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
});

test('program addresses equal pda.mjs (web3.js) for any program, season, civ and wallet', () => {
  for (let i = 0; i < 25; i++) {
    const program = i === 0 ? new PublicKey(programId) : key();
    const id = i === 0 ? seasonId : BigInt(Math.floor(Math.random() * 2 ** 48));
    for (const form of [id, id.toString(), Number(id)]) {
      assert.equal(pda.season(program.toBase58(), form), seasonPda(program, id).toBase58());
      assert.equal(pda.vault(program, form), vaultPda(program, id).toBase58());
    }
    for (let civ = 0; civ < 8; civ++) assert.equal(pda.nation(program, id, civ), nationPda(program, id, civ).toBase58());
    const wallet = key();
    assert.equal(pda.member(program, id, wallet.toBase58()), memberPda(program, id, wallet).toBase58());
    assert.equal(pda.member(program, id, wallet.toBytes()), memberPda(program, id, wallet).toBase58());
  }
  assert.equal(pda.season(programId, seasonId), chain.season.toBase58());
  assert.throws(() => pda.nation(programId, seasonId, 70_000), RangeError);
});

test('ata is the associated token address', () => {
  for (let i = 0; i < 30; i++) {
    const owner = key(), mint = key();
    const [want] = PublicKey.findProgramAddressSync([owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()], new PublicKey(ASSOCIATED_TOKEN_PROGRAM));
    assert.equal(ata(owner.toBase58(), mint), want.toBase58());
  }
});

test('registerIx is chain.register: one instruction, fee payer first, the wallet a read-only signer', () => {
  for (let i = 0; i < 20; i++) {
    const crank = Keypair.generate(), walletKey = Keypair.generate(), wallet = walletKey.publicKey, session = key(), walletToken = key(), mint = key();
    const tag = new Uint8Array(randomBytes(32));
    const attestation = i % 2 ? new Uint8Array(randomBytes(32)) : undefined;
    const votes = i % 3 ? [1, NOBODY, 7, 2] : undefined;
    const stand = ROLES.filter(() => Math.random() < 0.5);
    const o = { civ: i % 6, walletToken, mint, name: i % 2 ? 'Ada K.' : 'アステル', session, attestation, votes, deposit: BigInt(i * 1000), tag };
    const want = chain.register({ ...o, wallet, feePayer: crank.publicKey, kind: 2, stand: stand.reduce((m, r) => m | (1 << ROLES.indexOf(r)), 0) });
    const ours = [registerIx({ ...o, programId, seasonId, wallet: wallet.toBase58(), feePayer: crank.publicKey, walletToken: walletToken.toBase58(), mint: mint.toBase58(),
      session: session.toBase58(), stand })];
    const message = same(want, ours, crank, walletKey);
    const p = parseMessage(message);
    assert.equal(p.instructions.length, 1, 'no compute-budget instruction');
    assert.deepEqual(p.signers, [crank.publicKey.toBase58(), wallet.toBase58()]);
    assert.equal(p.header.numReadonlySignedAccounts, 1);
  }
  // Defaults: kind 2 (undeclared), votes nobody, no deposit, a fresh random tag.
  const session = key();
  const a = registerIx({ programId, seasonId, wallet: key(), feePayer: key(), civ: 1, walletToken: key(), mint: key(), name: 'Ren', session });
  const b = registerIx({ programId, seasonId, wallet: key(), feePayer: key(), civ: 1, walletToken: key(), mint: key(), name: 'Ren', session });
  const tag = a.data.subarray(-32);
  assert.notDeepEqual(tag, b.data.subarray(-32));
  assert.deepEqual(a.data, IX.register({ civ: 1, name: 'Ren', kind: 2, session: session.toBytes(), stand: 0, votes: [NOBODY, NOBODY, NOBODY, NOBODY], deposit: 0n, tag }));
});

test('claimIx is chain.claim; createAtaIdempotentIx is the associated-token program\'s CreateIdempotent', () => {
  for (let i = 0; i < 10; i++) {
    const crankKey = Keypair.generate(), crank = crankKey.publicKey, walletKey = Keypair.generate(), wallet = walletKey.publicKey, mint = key();
    const dest = ata(wallet, mint);
    same(chain.claim({ wallet, dest: new PublicKey(dest), mint }), [claimIx({ programId, seasonId, wallet: wallet.toBase58(), dest, mint: mint.toBase58() })], crankKey, walletKey);
    // The claim relay's optional prefix: create the wallet's ATA (the crank pays), then claim into it.
    const create = new TransactionInstruction({ programId: new PublicKey(ASSOCIATED_TOKEN_PROGRAM), data: Buffer.from([1]), keys: [
      { pubkey: crank, isSigner: true, isWritable: true }, { pubkey: new PublicKey(dest), isSigner: false, isWritable: true },
      { pubkey: wallet, isSigner: false, isWritable: false }, { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false }] });
    same([create, ...chain.claim({ wallet, dest: new PublicKey(dest), mint })],
      [createAtaIdempotentIx({ payer: crank, owner: wallet, mint }), claimIx({ programId, seasonId, wallet, dest, mint })], crankKey, walletKey);
  }
});

test('commitOrdersIxs / revealOrdersIxs / submitGovIxs are chain.mjs: a 128 KiB heap frame, then the program instructions', () => {
  const orders = vectors.orders.map(v => v.dto);
  const actions = vectors.gov.map(v => v.dto);
  for (let i = 0; i < 12; i++) {
    const crank = Keypair.generate(), session = Keypair.generate(), signer = session.publicKey;
    const civ = i % 6, role = ROLES[i % 4], tick = 17 + i * 1000;
    const commitment = new Uint8Array(randomBytes(32));
    same(chain.commitOrders({ signer, civ, role, tick, commitment }), commitOrdersIxs({ programId, seasonId, signer: signer.toBase58(), civ, role, tick, commitment }), crank, session);
    const batch = { decisionDigest: new Uint8Array(randomBytes(32)), orders: orders.slice(0, i * 2), adopt: i % 2 ? [3, 9] : [], salt: new Uint8Array(randomBytes(32)) };
    same(chain.revealOrders({ signer, civ, role, tick, ...batch }), revealOrdersIxs({ programId, seasonId, signer, civ, role, tick, ...batch }), crank, session);
    const some = actions.slice(0, 1 + (i % actions.length));
    same(chain.submitGovMany({ signer, civ, member: 40 + i, actions: some }), submitGovIxs({ programId, seasonId, signer, civ, member: 40 + i, actions: some }), crank, session);
  }
  const eight = Array.from({ length: MAX_GOV_PER_SIGNER }, (_, i) => actions[i % actions.length]);
  const session = Keypair.generate(), signer = session.publicKey, crank = Keypair.generate();
  same(chain.submitGovMany({ signer, civ: 2, member: 5, actions: eight }), submitGovIxs({ programId, seasonId, signer, civ: 2, member: 5, actions: eight }), crank, session);
  assert.throws(() => submitGovIxs({ programId, seasonId, signer, civ: 2, member: 5, actions: [] }), /1–8/);
  assert.throws(() => submitGovIxs({ programId, seasonId, signer, civ: 2, member: 5, actions: [...eight, actions[0]] }), /1–8/);
  // Hex is accepted for the 32-byte values; other lengths are refused.
  const hex = 'ab'.repeat(32);
  assert.deepEqual(commitOrdersIxs({ programId, seasonId, signer, civ: 0, role: 'General', tick: 1, commitment: hex })[1].data,
    commitOrdersIxs({ programId, seasonId, signer, civ: 0, role: 'General', tick: 1, commitment: new Uint8Array(32).fill(0xab) })[1].data);
  assert.throws(() => commitOrdersIxs({ programId, seasonId, signer, civ: 0, role: 'General', tick: 1, commitment: new Uint8Array(31) }), /32 bytes/);
});

test('a full RevealOrders batch fits one packet with the session key and the fee payer', () => {
  const order = { type: 'SendEnvoy', cityState: 1, influence: 5 };
  const orders = Array.from({ length: Math.floor(800 / 7) }, () => order);
  const message = compileMessage({ feePayer: key(), recentBlockhash: key().toBase58(), instructions: revealOrdersIxs({ programId, seasonId, signer: key(), civ: 5,
    role: 'Diplomat', tick: 179, decisionDigest: new Uint8Array(32).fill(9), orders, adopt: [1, 2, 3], salt: new Uint8Array(32).fill(4) }) });
  assert.ok(wireTransaction(message).length <= PACKET_BYTES);
});

test('sealMessage: "PS/seal/v1" ‖ season u64 ‖ tick u16 ‖ civ u16 ‖ role u8 ‖ commitment', () => {
  const commitment = new Uint8Array(32).map((_, i) => i);
  const m = sealMessage({ seasonId: 0x0102030405060708n, tick: 0x1234, civ: 5, role: 'Science', commitment });
  assert.equal(Buffer.from(m).toString('hex'),
    `${Buffer.from('PS/seal/v1').toString('hex')}${'0807060504030201'}${'3412'}${'0500'}${'02'}${Buffer.from(commitment).toString('hex')}`);
  assert.equal(m.length, 10 + 8 + 2 + 2 + 1 + 32);
  assert.deepEqual(sealMessage({ seasonId: '72623859790382856', tick: 0x1234, civ: 5, role: 2, commitment: Buffer.from(commitment).toString('hex') }), m);
  assert.throws(() => sealMessage({ seasonId: 1n, tick: 1, civ: 0, role: 'King', commitment }), /office/);
  assert.throws(() => sealMessage({ seasonId: 1n, tick: 1, civ: 0, role: 4, commitment }), /office/);
  assert.throws(() => sealMessage({ seasonId: 1n, tick: 1, civ: 0, role: 0, commitment: commitment.slice(1) }), /32 bytes/);
});
