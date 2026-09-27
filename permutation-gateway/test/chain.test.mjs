// Account lists of the v9 builders, account for account as permutation-chain
// `instruction.rs` documents them (the program checks them in that order):
// the VRF requests and their queue per play mode, the tick instructions'
// Instructions sysvar, the season's writable account and pinned validator
// in Delegate, the vault in FinishSeason, the session signer in Register,
// and the escape hatches.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, PublicKey, SystemProgram, SYSVAR_INSTRUCTIONS_PUBKEY, SYSVAR_SLOT_HASHES_PUBKEY } from '@solana/web3.js';
import {
  DELEGATION_PROGRAM_ID, commitRecordPdaFromDelegatedAccount, commitStatePdaFromDelegatedAccount, delegationMetadataPdaFromDelegatedAccount,
  delegationRecordPdaFromDelegatedAccount,
} from '@magicblock-labs/ephemeral-rollups-sdk';
import { ChainClient, CLOSE_BATCH, undelegationRequestPda, vrfQueue } from '../client/src/chain.mjs';
import { IX_TAG, NATION_TARGET, VRF_PROGRAM_ID, VRF_QUEUE_BASE, VRF_QUEUE_ER } from '../client/src/codec.mjs';
import { TOKEN_PROGRAM_ID } from '../client/src/pda.mjs';
import { vectors } from './vectors.mjs';

const programId = vectors.vrf.programId;
const chain = new ChainClient(programId, 1_790_000_000_123n);
const payer = Keypair.generate().publicKey;
const b58 = k => new PublicKey(k).toBase58();
/** The program's instruction of a builder's list (after any compute-budget ones). */
const programIx = ixs => ixs.filter(ix => ix.programId.equals(chain.programId));
const keysOf = ix => ix.keys.map(k => [k.pubkey.toBase58(), k.isWritable, k.isSigner]);
const W = (k, s = false) => [b58(k), true, s];
const R = (k, s = false) => [b58(k), false, s];

test('the program identity PDA and the VRF constants match the Rust vectors', () => {
  assert.equal(chain.identity.toBase58(), vectors.vrf.programIdentity);
  assert.equal(VRF_PROGRAM_ID, vectors.constants.VRF_PROGRAM_ID);
  assert.equal(vrfQueue(true).toBase58(), VRF_QUEUE_ER);
  assert.equal(vrfQueue(false).toBase58(), VRF_QUEUE_BASE);
});

const vrf = queue => [R(chain.identity), W(queue), R(VRF_PROGRAM_ID), R(SystemProgram.programId), R(SYSVAR_SLOT_HASHES_PUBKEY)];

test('freezeTick and retryTickRandomness: the queue of the play mode at account 3 (the ER\'s when delegated, the base\'s in base play)', () => {
  for (const delegated of [true, false]) {
    const queue = delegated ? VRF_QUEUE_ER : VRF_QUEUE_BASE;
    const [freeze] = programIx(chain.freezeTick({ payer, nations: 6, delegated }));
    assert.equal(freeze.data[0], IX_TAG.freezeTick);
    assert.deepEqual(keysOf(freeze), [W(payer, true), W(chain.worldChunks[0]), ...vrf(queue), ...chain.nations(6).map(k => W(k))]);
    const [retry] = chain.retryTickRandomness({ payer, delegated });
    assert.equal(retry.data[0], IX_TAG.retryTickRandomness);
    assert.deepEqual(keysOf(retry), [W(payer, true), W(chain.worldChunks[0]), ...vrf(queue)]);
  }
  // Default: a delegated season. FreezeTick makes no request; `request` adds RetryTickRandomness to the same transaction.
  assert.equal(keysOf(programIx(chain.freezeTick({ payer, nations: 2 }))[0])[3][0], VRF_QUEUE_ER);
  const both = programIx(chain.freezeTick({ payer, nations: 2, request: true }));
  assert.deepEqual(both.map(ix => ix.data[0]), [IX_TAG.freezeTick, IX_TAG.retryTickRandomness]);
  assert.equal(programIx(chain.freezeTick({ payer, nations: 2 })).length, 1);
});

test('startSeason and retrySeasonSeed: the base queue at account 3, no world chunks', () => {
  const [start] = programIx(chain.startSeason({ authority: payer }));
  assert.deepEqual(keysOf(start), [W(payer, true), W(chain.season), ...vrf(VRF_QUEUE_BASE)]);
  const [retry] = chain.retrySeasonSeed({ payer });
  assert.deepEqual(keysOf(retry), [W(payer, true), W(chain.season), ...vrf(VRF_QUEUE_BASE)]);
  assert.deepEqual(programIx(chain.startSeason({ authority: payer, request: true })).map(ix => ix.data[0]), [IX_TAG.startSeason, IX_TAG.retrySeasonSeed]);
});

test('the tick instructions (CloseCommits, LogTickInput, ResolveTick) end with the Instructions sysvar; StartClock carries none', () => {
  const tick = [...chain.worldChunks.map(k => W(k)), ...chain.nations(4).map(k => W(k)), R(SYSVAR_INSTRUCTIONS_PUBKEY)];
  assert.deepEqual(keysOf(programIx(chain.closeCommits({ nations: 4 }))[0]), tick);
  assert.deepEqual(keysOf(programIx(chain.logTickInput({ nations: 4, chunk: 1 }))[0]), tick);
  assert.deepEqual(keysOf(programIx(chain.resolveTick({ nations: 4, to: 0x83 }))[0]), tick);
  assert.deepEqual([...programIx(chain.resolveTick({ nations: 4, to: 0x83 }))[0].data], [IX_TAG.resolveTick, 0x83]);
  const [clock] = programIx(chain.startClock({ crank: payer, nations: 4 }));
  assert.equal(clock.data[0], IX_TAG.startClock);
  assert.deepEqual(keysOf(clock), [R(payer, true), ...chain.worldChunks.map(k => W(k)), ...chain.nations(4).map(k => W(k))]);
});

test('delegate: the season writable, the pinned validator last (required)', () => {
  const validator = Keypair.generate().publicKey;
  const [ix] = chain.delegate({ authority: payer, target: NATION_TARGET + 2, validator });
  const keys = keysOf(ix);
  assert.equal(keys.length, 10);
  assert.deepEqual(keys[2], W(chain.season));
  assert.deepEqual(keys[3], W(chain.nation(2)));
  assert.deepEqual(keys[9], R(validator));
  assert.throws(() => chain.delegate({ authority: payer, target: 0 }), /validator/);
});

test('register: ten accounts, the session key last, a read-only signer', () => {
  const wallet = Keypair.generate().publicKey, session = Keypair.generate().publicKey, walletToken = Keypair.generate().publicKey, mint = Keypair.generate().publicKey;
  const [ix] = chain.register({ wallet, feePayer: payer, civ: 1, walletToken, mint, name: 'Ada', session, tag: new Uint8Array(32) });
  assert.deepEqual(keysOf(ix), [R(wallet, true), W(payer, true), W(chain.season), W(chain.member(wallet)), W(walletToken), W(chain.vault), R(mint), R(TOKEN_PROGRAM_ID),
    R(SystemProgram.programId), R(session, true)]);
});

test('finishSeason: the vault after the world chunks, the roster last', () => {
  const base = [W(chain.season), ...chain.worldChunks.map(k => R(k)), R(chain.vault)];
  assert.deepEqual(keysOf(programIx(chain.finishSeason())[0]), base);
  assert.deepEqual(keysOf(programIx(chain.finishSeason({ roster: true }))[0]), [...base, R(chain.roster)]);
});

test('revealRoster and postBond: the operator signs; chunk 0 shows the season is over', () => {
  const authority = Keypair.generate().publicKey, wallets = [Keypair.generate().publicKey, Keypair.generate().publicKey];
  const salts = [new Uint8Array(32).fill(1), new Uint8Array(32).fill(2)];
  const [reveal] = chain.revealRoster({ authority, from: 3, members: wallets, salts, blind: new Uint8Array(32).fill(9) });
  assert.deepEqual(keysOf(reveal), [R(authority, true), W(chain.season), W(chain.roster), R(chain.worldChunks[0]), ...wallets.map(w => R(chain.member(w)))]);
  assert.deepEqual([...reveal.data.subarray(0, 3)], [IX_TAG.revealRoster, 3, 0]);
  assert.deepEqual([...reveal.data.subarray(-32)], Array(32).fill(9), 'the blind closes the data');
  const source = Keypair.generate().publicKey, mint = Keypair.generate().publicKey;
  const [bond] = chain.postBond({ authority, source, mint, amount: 7n });
  assert.deepEqual(keysOf(bond), [R(authority, true), W(chain.season), W(source), W(chain.vault), R(mint), R(TOKEN_PROGRAM_ID)]);
  assert.deepEqual([...bond.data], [IX_TAG.postBond, 7, 0, 0, 0, 0, 0, 0, 0]);
});

test('createSeason: deposit, startBy and the validator are in the data; both are required', () => {
  const admin = Keypair.generate().publicKey, mint = Keypair.generate().publicKey, crank = Keypair.generate().publicKey, validator = Keypair.generate().publicKey;
  const [ix] = chain.createSeason({ admin, mint, entryFee: 10n, worldSeed: new Uint8Array(32), crank, deposit: 5n, startBy: 1_790_000_000, validator });
  assert.deepEqual([...ix.data.subarray(-32)], [...validator.toBytes()]);
  assert.equal(Buffer.from(ix.data).readBigInt64LE(ix.data.length - 40), 1_790_000_000n);
  assert.equal(Buffer.from(ix.data).readBigUInt64LE(ix.data.length - 48), 5n);
  assert.throws(() => chain.createSeason({ admin, mint, entryFee: 10n, worldSeed: new Uint8Array(32), crank, validator }), /startBy/);
});

test('the escape hatches: abort, requestUndelegation, rollbackUndelegation, closeSeasonAccounts (at most 13 targets)', () => {
  const [abort] = chain.abort({ caller: payer });
  assert.deepEqual(keysOf(abort), [R(payer, true), W(chain.season), ...chain.worldChunks.map(k => R(k))]);

  const pda = chain.nation(1);
  const [req] = chain.requestUndelegation({ operator: payer, target: NATION_TARGET + 1 });
  assert.deepEqual(keysOf(req), [W(payer, true), R(chain.season), R(pda), R(chain.programId), W(undelegationRequestPda(pda)),
    R(delegationRecordPdaFromDelegatedAccount(pda)), W(delegationMetadataPdaFromDelegatedAccount(pda)), R(SystemProgram.programId), R(DELEGATION_PROGRAM_ID)]);
  assert.equal(undelegationRequestPda(pda).toBase58(), PublicKey.findProgramAddressSync([Buffer.from('undelegation-request'), pda.toBuffer()], DELEGATION_PROGRAM_ID)[0].toBase58());
  assert.deepEqual([...req.data], [IX_TAG.requestUndelegation, 0xe9, 0x03]);

  const rentPayer = Keypair.generate().publicKey, chunk = chain.worldChunks[7];
  const [back] = chain.rollbackUndelegation({ target: 7, rentPayer });
  assert.deepEqual(keysOf(back), [W(chain.season), W(chunk), R(chain.programId), W(undelegationRequestPda(chunk)), W(delegationRecordPdaFromDelegatedAccount(chunk)),
    W(delegationMetadataPdaFromDelegatedAccount(chunk)), W(rentPayer), W(commitStatePdaFromDelegatedAccount(chunk)), W(commitRecordPdaFromDelegatedAccount(chunk)),
    W(rentPayer), R(DELEGATION_PROGRAM_ID)]);
  const other = Keypair.generate().publicKey;
  assert.deepEqual(keysOf(chain.rollbackUndelegation({ target: 7, rentPayer, reimbursement: other })[0])[9], W(other));

  const targets = [...Array.from({ length: 10 }, (_, k) => k), NATION_TARGET, NATION_TARGET + 1, NATION_TARGET + 2];
  const [close] = chain.closeSeasonAccounts({ operator: payer, targets });
  assert.equal(targets.length, CLOSE_BATCH);
  assert.deepEqual(keysOf(close), [W(payer, true), R(chain.season), ...targets.map(t => W(chain.target(t)))]);
  assert.throws(() => chain.closeSeasonAccounts({ operator: payer, targets: [...targets, 11] }), /1–13/);
  assert.throws(() => chain.closeSeasonAccounts({ operator: payer, targets: [] }), /1–13/);
});
