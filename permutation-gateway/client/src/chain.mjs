// Transaction builders for the permutation-chain program (Game Design V5).
// Every builder returns the instructions for one transaction; heavy ones
// (anything that decodes the world) are prefixed with a 1.4M CU limit and a
// 256 KiB heap. test/tx-size.test.mjs checks the largest ones still fit a
// packet.
import { ComputeBudgetProgram, PublicKey, SystemProgram, SYSVAR_SLOT_HASHES_PUBKEY, TransactionInstruction } from '@solana/web3.js';
import {
  DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID,
  delegateBufferPdaFromDelegatedAccountAndOwnerProgram, delegationMetadataPdaFromDelegatedAccount, delegationRecordPdaFromDelegatedAccount,
} from '@magicblock-labs/ephemeral-rollups-sdk';
import { IX, NATIONS, NATION_TARGET, WORLD_CHUNKS } from './codec.mjs';
import { memberPda, nationPda, seasonPda, TOKEN_PROGRAM_ID, vaultPda, worldChunkPda } from './pda.mjs';

export const HEAP_BYTES = 256 * 1024;
export { NATION_TARGET, WORLD_CHUNKS };
export const heavy = () => [ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }), ComputeBudgetProgram.requestHeapFrame({ bytes: HEAP_BYTES })];
/**
 * Submissions decode the nation account (batches and the governance inbox),
 * which outgrows the default 32 KiB heap once they fill up.
 */
const medium = () => [ComputeBudgetProgram.requestHeapFrame({ bytes: 128 * 1024 })];
const W = (pubkey, isSigner = false) => ({ pubkey, isSigner, isWritable: true });
const R = (pubkey, isSigner = false) => ({ pubkey, isSigner, isWritable: false });

export class ChainClient {
  constructor(programId, seasonId) {
    this.programId = new PublicKey(programId);
    this.seasonId = BigInt(seasonId);
    this.season = seasonPda(this.programId, this.seasonId);
    // The world is stored in WORLD_CHUNKS accounts of CHUNK bytes, small
    // enough for the ER committor to finalize one densely written chunk.
    this.worldChunks = Array.from({ length: WORLD_CHUNKS }, (_, k) => worldChunkPda(this.programId, this.seasonId, k));
    this.world = this.worldChunks[0];
    this.vault = vaultPda(this.programId, this.seasonId);
  }
  nation(civ) { return nationPda(this.programId, this.seasonId, civ); }
  nations(n) { return Array.from({ length: n }, (_, i) => this.nation(i)); }
  member(wallet) { return memberPda(this.programId, this.seasonId, wallet); }
  target(t) { return t < WORLD_CHUNKS ? this.worldChunks[t] : this.nation(t - NATION_TARGET); }
  ix(keys, data) { return new TransactionInstruction({ programId: this.programId, keys, data: Buffer.from(data) }); }
  chunkKeys() { return this.worldChunks.map(k => W(k)); }

  createSeason({ admin, mint, preset = 0, nations = NATIONS.length, entryFee, tickSeconds = 30, worldSeed, crank, market = true }) {
    return [this.ix([W(admin, true), W(this.season), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID), R(SystemProgram.programId)],
      IX.createSeason({ seasonId: this.seasonId, preset, nations, entryFee, tickSeconds, worldSeed, crank: crank.toBytes(), market }))];
  }
  allocWorld({ payer, chunk }) {
    return [this.ix([W(payer, true), R(this.season), W(this.worldChunks[chunk]), R(SystemProgram.programId)], IX.allocWorld(chunk))];
  }
  allocNation({ payer, civ }) {
    return [this.ix([W(payer, true), R(this.season), W(this.nation(civ)), R(SystemProgram.programId)], IX.allocNation(civ))];
  }
  /** Become a member of nation `civ`: the wallet signs; the entry fee (and any deposit) moves from `walletToken`. */
  register({ wallet, feePayer, civ, walletToken, mint, name, kind = 0, session, attestation, stand = 0, votes, deposit = 0n }) {
    return [this.ix([R(wallet, true), W(feePayer, true), W(this.season), W(this.member(wallet)), W(walletToken), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID), R(SystemProgram.programId)],
      IX.register({ civ, name, kind, session: session.toBytes(), attestation, stand, votes, deposit: BigInt(deposit) }))];
  }
  updateMember({ signer, wallet, stand, votes }) {
    return [this.ix([R(signer, true), R(this.season), W(this.member(wallet))], IX.updateMember({ stand, votes }))];
  }
  startSeason({ authority }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), R(SYSVAR_SLOT_HASHES_PUBKEY), ...this.chunkKeys()], IX.startSeason())];
  }
  genesisStep({ work = 50 }) {
    return [...heavy(), this.ix([W(this.season), ...this.chunkKeys()], IX.genesisStep(work))];
  }
  /** Seat members (base-layer Member PDAs, in registration order) into the world. */
  seatMembers({ authority, members }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), ...this.chunkKeys(), ...members.map(m => R(m))], IX.seatMembers())];
  }
  openGovernment({ authority, nations }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), ...this.chunkKeys(), ...this.nations(nations).map(k => W(k))], IX.openGovernment())];
  }
  delegate({ authority, target, validator }) {
    const pda = this.target(target);
    return [this.ix([
      W(authority, true), R(SystemProgram.programId), R(this.season), W(pda), R(this.programId),
      W(delegateBufferPdaFromDelegatedAccountAndOwnerProgram(pda, this.programId)),
      W(delegationRecordPdaFromDelegatedAccount(pda)), W(delegationMetadataPdaFromDelegatedAccount(pda)),
      R(DELEGATION_PROGRAM_ID), ...(validator ? [R(new PublicKey(validator))] : []),
    ], IX.delegate(target))];
  }
  /** One office's batch for the open tick, signed by the office holder's session key (or the crank for a vacant office). */
  submitOrders({ signer, civ, role, tick, decisionDigest, orders, adopt = [] }) {
    return [...medium(), this.ix([R(signer, true), W(this.nation(civ))], IX.submitOrders({ role, tick, decisionDigest, orders, adopt }))];
  }
  /** A governance action queued for the open tick, signed by the member's session key. */
  submitGov({ signer, civ, member, action }) {
    return [...medium(), this.ix([R(signer, true), W(this.nation(civ))], IX.submitGov({ member, action }))];
  }
  /** Publish chunk `chunk` of the open tick's input (chunk 0 freezes it). */
  logTickInput({ nations, chunk }) {
    return [...heavy(), this.ix([...this.chunkKeys(), ...this.nations(nations).map(k => W(k))], IX.logTickInput(chunk))];
  }
  resolveTick({ nations, to = 12 }) {
    return [...heavy(), this.ix([...this.chunkKeys(), ...this.nations(nations).map(k => W(k))], IX.resolveTick(to))];
  }
  /** Accounts of a `CommitPart` / `UndelegatePart` intent: chunk 0 (whose header the program reads) always, then the other targets. */
  intentKeys(payer, targets) {
    const keys = targets.filter(t => t !== 0).map(t => W(this.target(t)));
    return [W(payer, true), R(MAGIC_PROGRAM_ID), W(MAGIC_CONTEXT_ID), W(this.worldChunks[0]), ...keys];
  }
  /** Commit and undelegate `targets` (see `delegate`) in one small intent; chunk 0 must go in the last group. */
  undelegatePart({ payer, targets }) {
    return [this.ix(this.intentKeys(payer, targets), IX.undelegatePart(targets))];
  }
  /** Commit `targets` to the base layer during play, in one small intent. */
  commitPart({ payer, targets }) {
    return [this.ix(this.intentKeys(payer, targets), IX.commitPart(targets))];
  }
  finishSeason() {
    return [...heavy(), this.ix([W(this.season), ...this.worldChunks.map(k => R(k))], IX.finishSeason())];
  }
  /** A member's prize plus its treasury refund, to the wallet's own token account. */
  claim({ wallet, dest, mint }) {
    return [this.ix([R(wallet, true), W(this.season), W(this.member(wallet)), W(this.vault), W(dest), R(mint), R(TOKEN_PROGRAM_ID)], IX.claim())];
  }
  withdrawOps({ admin, dest, mint }) {
    return [this.ix([R(admin, true), W(this.season), W(this.vault), W(dest), R(mint), R(TOKEN_PROGRAM_ID)], IX.withdrawOps())];
  }
}
