// Transaction builders for the permutation-chain program. Every builder
// returns the instructions for one transaction; heavy ones are prefixed with
// a 1.4M CU limit and the 256 KiB heap the program needs to decode the world.
import { ComputeBudgetProgram, PublicKey, SystemProgram, SYSVAR_SLOT_HASHES_PUBKEY, TransactionInstruction } from '@solana/web3.js';
import {
  DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID,
  delegateBufferPdaFromDelegatedAccountAndOwnerProgram, delegationMetadataPdaFromDelegatedAccount, delegationRecordPdaFromDelegatedAccount,
} from '@magicblock-labs/ephemeral-rollups-sdk';
import { IX } from './codec.mjs';
import { seasonPda, worldChunkPda, WORLD_CHUNKS, vaultPda, ordersPda } from './pda.mjs';
import { TOKEN_PROGRAM_ID } from './pda.mjs';

export const HEAP_BYTES = 256 * 1024;
export const ORDERS_TARGET = 1000;
export { WORLD_CHUNKS };
export const heavy = () => [ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }), ComputeBudgetProgram.requestHeapFrame({ bytes: HEAP_BYTES })];
const W = (pubkey, isSigner = false) => ({ pubkey, isSigner, isWritable: true });
const R = (pubkey, isSigner = false) => ({ pubkey, isSigner, isWritable: false });

export class ChainClient {
  constructor(programId, seasonId) {
    this.programId = new PublicKey(programId);
    this.seasonId = BigInt(seasonId);
    this.season = seasonPda(this.programId, this.seasonId);
    // The world is stored in WORLD_CHUNKS 10 KiB accounts (the CPI allocation limit).
    this.worldChunks = Array.from({ length: WORLD_CHUNKS }, (_, k) => worldChunkPda(this.programId, this.seasonId, k));
    this.world = this.worldChunks[0];
    this.vault = vaultPda(this.programId, this.seasonId);
  }
  orders(civ) { return ordersPda(this.programId, this.seasonId, civ); }
  ordersList(civs) { return Array.from({ length: civs }, (_, i) => this.orders(i)); }
  ix(keys, data) { return new TransactionInstruction({ programId: this.programId, keys, data: Buffer.from(data) }); }

  createSeason({ admin, mint, preset = 0, maxCivs = 6, entryFee, exchangeCredit = 0n, tickSeconds = 30, worldSeed, crank }) {
    return [this.ix([W(admin, true), W(this.season), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID), R(SystemProgram.programId)],
      IX.createSeason({ seasonId: this.seasonId, preset, maxCivs, entryFee, exchangeCredit, tickSeconds, worldSeed, crank: crank.toBytes() }))];
  }
  allocWorld({ payer, chunk }) {
    return [this.ix([W(payer, true), R(this.season), W(this.worldChunks[chunk]), R(SystemProgram.programId)], IX.allocWorld(chunk))];
  }
  chunkKeys() { return this.worldChunks.map(k => W(k)); }
  joinSeason({ player, feePayer, civ, playerToken, mint, name, kind, session, payout }) {
    return [this.ix([R(player, true), W(feePayer, true), W(this.season), W(this.orders(civ)), W(playerToken), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID), R(SystemProgram.programId)],
      IX.joinSeason({ name, kind, session: session.toBytes(), payout: payout.toBytes() }))];
  }
  startSeason({ authority }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), R(SYSVAR_SLOT_HASHES_PUBKEY), ...this.chunkKeys()], IX.startSeason())];
  }
  genesisStep({ civs, work = 50 }) {
    return [...heavy(), this.ix([W(this.season), ...this.chunkKeys(), ...this.ordersList(civs).map(k => W(k))], IX.genesisStep(work))];
  }
  /** target 0..WORLD_CHUNKS = that world chunk; ORDERS_TARGET + civ = that civ's orders account. */
  delegate({ authority, target, validator }) {
    const pda = target < WORLD_CHUNKS ? this.worldChunks[target] : this.orders(target - ORDERS_TARGET);
    return [this.ix([
      W(authority, true), R(SystemProgram.programId), R(this.season), W(pda), R(this.programId),
      W(delegateBufferPdaFromDelegatedAccountAndOwnerProgram(pda, this.programId)),
      W(delegationRecordPdaFromDelegatedAccount(pda)), W(delegationMetadataPdaFromDelegatedAccount(pda)),
      R(DELEGATION_PROGRAM_ID), ...(validator ? [R(new PublicKey(validator))] : []),
    ], IX.delegate(target))];
  }
  submitOrders({ signer, civ, tick, decisionDigest, orders }) {
    return [this.ix([R(signer, true), W(this.orders(civ))], IX.submitOrders({ tick, decisionDigest, orders }))];
  }
  resolveTick({ civs, to = 12 }) {
    return [...heavy(), this.ix([...this.chunkKeys(), ...this.ordersList(civs).map(k => W(k))], IX.resolveTick(to))];
  }
  commit({ payer, civs, undelegate = false }) {
    return [this.ix([W(payer, true), R(MAGIC_PROGRAM_ID), W(MAGIC_CONTEXT_ID), ...this.chunkKeys(), ...this.ordersList(civs).map(k => W(k))],
      undelegate ? IX.commitAndUndelegate() : IX.commit())];
  }
  /** Commit and undelegate `targets` (see `delegate`); chunk 0 is always passed and must go in the last group. */
  undelegatePart({ payer, targets }) {
    const keys = targets.filter(t => t !== 0).map(t => W(t < WORLD_CHUNKS ? this.worldChunks[t] : this.orders(t - ORDERS_TARGET)));
    return [this.ix([W(payer, true), R(MAGIC_PROGRAM_ID), W(MAGIC_CONTEXT_ID), W(this.worldChunks[0]), ...keys], IX.undelegatePart(targets))];
  }
  finishSeason() {
    return [...heavy(), this.ix([W(this.season), ...this.worldChunks.map(k => R(k))], IX.finishSeason())];
  }
  claim({ owner, civ, dest, mint }) {
    return [this.ix([R(owner, true), W(this.season), W(this.vault), W(dest), R(mint), R(TOKEN_PROGRAM_ID)], IX.claim(civ))];
  }
}
