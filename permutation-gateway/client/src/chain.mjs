// Transaction builders for the permutation-chain program (Game Design V5).
// Every builder returns the instructions for one transaction, account for
// account as `instruction.rs` lists them; heavy ones (anything that decodes
// the world) are prefixed with a 1.4M CU limit and a 256 KiB heap.
// Instructions that must be alone in their transaction (CloseCommits,
// LogTickInput, ResolveTick, UndelegatePart) end with the Instructions
// sysvar and may only share it with compute-budget instructions.
// test/tx-size.test.mjs checks the largest ones still fit a packet.
import {
  ComputeBudgetProgram, PublicKey, SystemProgram, SYSVAR_INSTRUCTIONS_PUBKEY, SYSVAR_SLOT_HASHES_PUBKEY, TransactionInstruction,
} from '@solana/web3.js';
import {
  DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID, commitRecordPdaFromDelegatedAccount, commitStatePdaFromDelegatedAccount,
  delegateBufferPdaFromDelegatedAccountAndOwnerProgram, delegationMetadataPdaFromDelegatedAccount, delegationRecordPdaFromDelegatedAccount,
} from '@magicblock-labs/ephemeral-rollups-sdk';
import { decodeSeason, IX, NATIONS, NATION_TARGET, VRF_PROGRAM_ID, VRF_QUEUE_BASE, VRF_QUEUE_ER, WORLD_CHUNKS } from './codec.mjs';
import { memberPda, nationPda, rosterPda, seasonPda, TOKEN_PROGRAM_ID, vaultPda, worldChunkPda } from './pda.mjs';

export const HEAP_BYTES = 256 * 1024;
/** A member's registration tag when it is not an operator AI: 32 random bytes. */
export const randomTag = () => globalThis.crypto.getRandomValues(new Uint8Array(32));
export { NATION_TARGET, WORLD_CHUNKS };
/** Targets one `CloseSeasonAccounts` transaction carries at most (a packet). */
export const CLOSE_BATCH = 13;
/**
 * The VRF oracle queue of a season's play mode (the chain refuses the
 * other): the ER's for a delegated season, the base layer's for one played
 * on base (and for the season seed).
 */
export const vrfQueue = (delegated = true) => new PublicKey(delegated ? VRF_QUEUE_ER : VRF_QUEUE_BASE);
export const heavy = () => [ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }), ComputeBudgetProgram.requestHeapFrame({ bytes: HEAP_BYTES })];
/**
 * Submissions decode the nation account (batches and the governance inbox),
 * which outgrows the default 32 KiB heap once they fill up.
 */
const medium = () => [ComputeBudgetProgram.requestHeapFrame({ bytes: 128 * 1024 })];
/**
 * The season account of `chain` (a ChainClient), decoded, as `connection`
 * sees it at 'confirmed'. Throws when it is not there (a wrong program or
 * season id, or a season not created yet) rather than failing on `.data`.
 */
export async function readSeason(connection, chain) {
  const info = await connection.getAccountInfo(chain.season, 'confirmed');
  if (!info) throw Object.assign(new Error('season account not found'), { code: 'SeasonNotFound' });
  return decodeSeason(info.data);
}

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
    this.roster = rosterPda(this.programId, this.seasonId);
  }
  nation(civ) { return nationPda(this.programId, this.seasonId, civ); }
  nations(n) { return Array.from({ length: n }, (_, i) => this.nation(i)); }
  member(wallet) { return memberPda(this.programId, this.seasonId, wallet); }
  target(t) { return t < WORLD_CHUNKS ? this.worldChunks[t] : this.nation(t - NATION_TARGET); }
  ix(keys, data) { return new TransactionInstruction({ programId: this.programId, keys, data: Buffer.from(data) }); }
  chunkKeys() { return this.worldChunks.map(k => W(k)); }
  /** Accounts of an instruction that plays the open tick: every world chunk, every nation in civ order, the Instructions sysvar. */
  tickKeys(nations) { return [...this.chunkKeys(), ...this.nations(nations).map(k => W(k)), R(SYSVAR_INSTRUCTIONS_PUBKEY)]; }
  /** The program's identity PDA `["identity"]`: it signs the program's VRF requests. */
  get identity() { return (this._identity ??= PublicKey.findProgramAddressSync([Buffer.from('identity')], this.programId)[0]); }
  /** Accounts 2–6 of a VRF request (FreezeTick, RetryTickRandomness, StartSeason, RetrySeasonSeed). */
  vrfKeys(queue) { return [R(this.identity), W(queue), R(new PublicKey(VRF_PROGRAM_ID)), R(SystemProgram.programId), R(SYSVAR_SLOT_HASHES_PUBKEY)]; }

  /**
   * Create this season. With `prevSeasonId` it follows that finalized (or
   * aborted) season of the same admin in the history layer: its history root
   * is taken over. With `aiCount` operator AI members (V5 §18) the admin
   * escrows their bounties and the bond from `adminToken`, and commits
   * `rosterCommit` (`rosterCommit(blind, rosterChain(tags))`). `deposit` is
   * every member's treasury deposit (0: none; market seasons only),
   * `startBy` (unix seconds) when StartSeason is due, `validator` the ER
   * validator every account will be delegated to.
   */
  createSeason({ admin, mint, preset = 0, nations = NATIONS.length, entryFee, tickSeconds = 30, worldSeed, crank, market = true, prevSeasonId = null,
    aiCount = 0, rosterCommit, bountyEach = 0n, bond = 0n, adminToken, deposit = 0n, startBy, validator }) {
    if (startBy === undefined || !validator) throw new Error('createSeason: startBy and validator are required');
    const prev = prevSeasonId ? [R(new ChainClient(this.programId, BigInt(prevSeasonId)).season)] : [];
    const ai = aiCount > 0 ? [W(adminToken), W(this.roster)] : [];
    return [this.ix([W(admin, true), W(this.season), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID), R(SystemProgram.programId), ...prev, ...ai],
      IX.createSeason({ seasonId: this.seasonId, preset, nations, entryFee, tickSeconds, worldSeed, crank: crank.toBytes(), market, prevSeasonId: BigInt(prevSeasonId ?? 0),
        aiCount, rosterCommit, bountyEach: BigInt(bountyEach), bond: BigInt(bond), deposit: BigInt(deposit), startBy: BigInt(startBy),
        validator: new PublicKey(validator).toBytes() }))];
  }
  allocWorld({ payer, chunk }) {
    return [this.ix([W(payer, true), R(this.season), W(this.worldChunks[chunk]), R(SystemProgram.programId)], IX.allocWorld(chunk))];
  }
  allocNation({ payer, civ }) {
    return [this.ix([W(payer, true), R(this.season), W(this.nation(civ)), R(SystemProgram.programId)], IX.allocNation(civ))];
  }
  /**
   * Become a member of nation `civ`: the wallet and the session key sign
   * (the session key must differ from the wallet when it pays the fees); the
   * entry fee (and any deposit) moves from `walletToken`, a token account
   * the wallet owns. `tag` is 32 random bytes unless the member is an
   * operator AI (`rosterTag`, V5 §18.2).
   */
  register({ wallet, feePayer, civ, walletToken, mint, name, kind = 0, session, attestation, stand = 0, votes, deposit = 0n, tag = randomTag() }) {
    const key = new PublicKey(session);
    return [this.ix([R(wallet, true), W(feePayer, true), W(this.season), W(this.member(wallet)), W(walletToken), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID),
      R(SystemProgram.programId), R(key, true)],
    IX.register({ civ, name, kind, session: key.toBytes(), attestation, stand, votes, deposit: BigInt(deposit), tag }))];
  }
  updateMember({ signer, wallet, stand, votes }) {
    return [this.ix([R(signer, true), R(this.season), W(this.member(wallet))], IX.updateMember({ stand, votes }))];
  }
  /**
   * Add to the operator's bond from `source` (the authority's USDC account)
   * while registering, so it reaches `bondFloor` before StartSeason. Admin or crank.
   */
  postBond({ authority, source, mint, amount }) {
    return [this.ix([R(authority, true), W(this.season), W(source), W(this.vault), R(mint), R(TOKEN_PROGRAM_ID)], IX.postBond(BigInt(amount)))];
  }
  /**
   * Close registration (status Seeding). The season seed is requested from
   * the VRF (base queue) by RetrySeasonSeed: with `request` it goes in the
   * same transaction (not with a `dev-randomness` program, whose StartSeason
   * goes straight to Genesis). Admin or crank.
   */
  startSeason({ authority, request = false }) {
    return [...medium(), this.ix([W(authority, true), W(this.season), ...this.vrfKeys(vrfQueue(false))], IX.startSeason()),
      ...(request ? this.retrySeasonSeed({ payer: authority }) : [])];
  }
  /** Status Seeding: request the season seed (the first time, and again SEED_RETRY_SECONDS after the last request). Anyone. */
  retrySeasonSeed({ payer }) {
    return [this.ix([W(payer, true), W(this.season), ...this.vrfKeys(vrfQueue(false))], IX.retrySeasonSeed())];
  }
  genesisStep({ work = 50 }) {
    return [...heavy(), this.ix([W(this.season), ...this.chunkKeys()], IX.genesisStep(work))];
  }
  /** Seat members (base-layer Member PDAs, in registration order) into the world. */
  seatMembers({ authority, members }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), ...this.chunkKeys(), ...members.map(m => R(m))], IX.seatMembers())];
  }
  /** Hold the first election and open tick 0 (the operator, or anyone TAKEOVER_SECONDS after seating began). */
  openGovernment({ authority, nations }) {
    return [...heavy(), this.ix([R(authority, true), W(this.season), ...this.chunkKeys(), ...this.nations(nations).map(k => W(k))], IX.openGovernment())];
  }
  /**
   * Delegate one world chunk or nation account to `validator`, the season's
   * pinned ER validator (`Season.validator`): each target once, world chunk
   * 0 last. Admin or crank.
   */
  delegate({ authority, target, validator }) {
    if (!validator) throw new Error('delegate: the validator (Season.validator) is required');
    const pda = this.target(target);
    return [this.ix([
      W(authority, true), R(SystemProgram.programId), W(this.season), W(pda), R(this.programId),
      W(delegateBufferPdaFromDelegatedAccountAndOwnerProgram(pda, this.programId)),
      W(delegationRecordPdaFromDelegatedAccount(pda)), W(delegationMetadataPdaFromDelegatedAccount(pda)),
      R(DELEGATION_PROGRAM_ID), R(new PublicKey(validator)),
    ], IX.delegate(target))];
  }
  /**
   * Start tick 0's clock (`now + tickSeconds`, if earlier than
   * OpenGovernment's grace) once every account is on the layer that plays
   * it: the ER, or base in base play. The crank only.
   */
  startClock({ crank, nations }) {
    return [...heavy(), this.ix([R(crank, true), ...this.chunkKeys(), ...this.nations(nations).map(k => W(k))], IX.startClock())];
  }
  /**
   * Seal one office's orders for the open tick (commit–reveal): only
   * `commitment` (`orderCommitment(batch, salt)`) goes on chain now. Signed
   * by the office holder's session key; a vacant office takes none.
   */
  commitOrders({ signer, civ, role, tick, commitment }) {
    return [...medium(), this.ix([R(signer, true), W(this.nation(civ))], IX.commitOrders({ role, tick, commitment }))];
  }
  /** Reveal sealed orders in the reveal window (anyone holding the plaintext and salt may send it). */
  revealOrders({ signer, civ, role, tick, decisionDigest, orders, adopt = [], salt }) {
    return [...medium(), this.ix([R(signer, true), W(this.nation(civ))], IX.revealOrders({ role, tick, decisionDigest, orders, adopt, salt }))];
  }
  /** Close the open tick's commitments after its deadline and open the reveal window. Permissionless; alone in its transaction. */
  closeCommits({ nations }) {
    return [...heavy(), this.ix(this.tickKeys(nations), IX.closeCommits())];
  }
  /**
   * Close the reveal window and freeze the open tick's input (once every
   * commitment was revealed or the window's deadline passed); the tick
   * randomness is then pending. The program makes no VRF request here:
   * with `request` the RetryTickRandomness that does goes in the same
   * transaction (FreezeTick alone otherwise, and `retryTickRandomness`
   * next). `delegated`: the season's play mode, which picks the VRF queue.
   * Permissionless.
   */
  freezeTick({ payer, nations, delegated = true, request = false }) {
    return [...heavy(), this.ix([W(payer, true), W(this.worldChunks[0]), ...this.vrfKeys(vrfQueue(delegated)), ...this.nations(nations).map(k => W(k))],
      IX.freezeTick()), ...(request ? this.retryTickRandomness({ payer, delegated }) : [])];
  }
  /**
   * Request the frozen tick's randomness from the VRF: the first request,
   * and again VRF_RETRY_SECONDS after the last; VRF_GIVEUP_SECONDS after the
   * freeze it draws the logged fallback instead. Permissionless.
   */
  retryTickRandomness({ payer, delegated = true }) {
    return [this.ix([W(payer, true), W(this.worldChunks[0]), ...this.vrfKeys(vrfQueue(delegated))], IX.retryTickRandomness())];
  }
  /** A governance action queued for the open tick, signed by the member's session key. */
  submitGov({ signer, civ, member, action }) {
    return this.submitGovMany({ signer, civ, member, actions: [action] });
  }
  /** Several governance actions of one member in one transaction, one instruction each. */
  submitGovMany({ signer, civ, member, actions }) {
    return [...medium(), ...actions.map(action => this.ix([R(signer, true), W(this.nation(civ))], IX.submitGov({ member, action })))];
  }
  /**
   * Publish chunk `chunk` of the open tick's input (INPUT_CHUNK bytes per
   * call; chunk 0 also logs the revealed salts), once the tick is frozen
   * and its randomness drawn. Permissionless; alone in its transaction.
   */
  logTickInput({ nations, chunk }) {
    return [...heavy(), this.ix(this.tickKeys(nations), IX.logTickInput(chunk))];
  }
  /** Resolve the open tick up to phase `to` (12: all), or one degraded phase (`DEGRADED`). Permissionless; alone in its transaction. */
  resolveTick({ nations, to = 12 }) {
    return [...heavy(), this.ix(this.tickKeys(nations), IX.resolveTick(to))];
  }
  /**
   * Accounts of a `CommitPart` / `UndelegatePart` intent: chunk 0 (whose
   * header the program reads) always, then the other targets; targets in
   * `gone` (already off the ER) are read-only.
   */
  intentKeys(payer, targets, extra = [], gone = []) {
    const keys = targets.filter(t => t !== 0).map(t => (gone.includes(t) ? R : W)(this.target(t)));
    return [W(payer, true), R(MAGIC_PROGRAM_ID), W(MAGIC_CONTEXT_ID), W(this.worldChunks[0]), ...extra, ...keys];
  }
  /**
   * Commit and undelegate `targets` in one small intent, strictly in
   * `undelegation_order` (world chunks 1–19 one each, the nations 1–3 per
   * intent, chunk 0 last); anyone may send it, alone in its transaction.
   * `gone`: targets already back on base (the program skips them).
   */
  undelegatePart({ payer, targets, gone = [] }) {
    return [this.ix([...this.intentKeys(payer, targets, [], gone), R(SYSVAR_INSTRUCTIONS_PUBKEY)], IX.undelegatePart(targets))];
  }
  /**
   * Commit `targets` to the base layer during play, in one small intent.
   * Only the season's crank may: nation 0's account, which records the
   * crank's key, goes after world chunk 0.
   */
  commitPart({ payer, targets }) {
    return [this.ix(this.intentKeys(payer, targets, [R(this.nation(0))]), IX.commitPart(targets))];
  }
  /**
   * Compute the payouts once the final world is back on base; the vault
   * must hold what they owe. With a revealed operator AI roster the roster
   * account goes last. Permissionless.
   */
  finishSeason({ roster = false } = {}) {
    return [...heavy(), this.ix([W(this.season), ...this.worldChunks.map(k => R(k)), R(this.vault), ...(roster ? [R(this.roster)] : [])], IX.finishSeason())];
  }
  /**
   * Reveal the next operator AI members, in roster order from position
   * `from` (0 restarts): their wallets' member accounts and salts (V5
   * §18.2). The batch that completes the roster carries the `blind`. Admin
   * or crank, after the last tick.
   */
  revealRoster({ authority, from, members, salts, blind }) {
    return [this.ix([R(authority, true), W(this.season), W(this.roster), R(this.worldChunks[0]), ...members.map(m => R(this.member(m)))],
      IX.revealRoster({ from, salts, blind }))];
  }
  /** Anchor a tick's relayed messages (the crank only, V5 §18.7). */
  anchorTalk({ crank, tick, count, root }) {
    return [this.ix([R(crank, true), R(this.nation(0))], IX.anchorTalk({ tick, count, root }))];
  }
  /** A member's prize plus its treasury refund (Finalized), or its refund (Aborted), to the wallet's own token account. */
  claim({ wallet, dest, mint }) {
    return [this.ix([R(wallet, true), W(this.season), W(this.member(wallet)), W(this.vault), W(dest), R(mint), R(TOKEN_PROGRAM_ID)], IX.claim())];
  }
  /** The operations share (Finalized) or `opsAfterAbort` (Aborted), to the admin. */
  withdrawOps({ admin, dest, mint }) {
    return [this.ix([R(admin, true), W(this.season), W(this.vault), W(dest), R(mint), R(TOKEN_PROGRAM_ID)], IX.withdrawOps())];
  }
  /** Abort a season that cannot move on (`checkAbort`): the operator while registering, anyone once a stage is overdue. */
  abort({ caller }) {
    return [this.ix([R(caller, true), W(this.season), ...this.worldChunks.map(k => R(k))], IX.abort())];
  }
  /**
   * Ask the delegation program to give `target` back without the validator
   * (after the season, or once a Running season is past its running
   * deadline). The operator that paid the delegation's rent signs.
   */
  requestUndelegation({ operator, target }) {
    const pda = this.target(target);
    return [this.ix([W(operator, true), R(this.season), R(pda), R(this.programId), W(undelegationRequestPda(pda)),
      R(delegationRecordPdaFromDelegatedAccount(pda)), W(delegationMetadataPdaFromDelegatedAccount(pda)), R(SystemProgram.programId),
      R(DELEGATION_PROGRAM_ID)], IX.requestUndelegation(target))];
  }
  /**
   * Once the request expired, take `target` back with its last committed
   * data. `rentPayer`: the delegation metadata's rent payer;
   * `reimbursement`: the pending commit's identity if there is one, else
   * any writable account (the rent payer). Permissionless.
   */
  rollbackUndelegation({ target, rentPayer, reimbursement = rentPayer }) {
    const pda = this.target(target);
    return [this.ix([W(this.season), W(pda), R(this.programId), W(undelegationRequestPda(pda)), W(delegationRecordPdaFromDelegatedAccount(pda)),
      W(delegationMetadataPdaFromDelegatedAccount(pda)), W(rentPayer), W(commitStatePdaFromDelegatedAccount(pda)),
      W(commitRecordPdaFromDelegatedAccount(pda)), W(reimbursement), R(DELEGATION_PROGRAM_ID)], IX.rollbackUndelegation(target))];
  }
  /** After the season (Finalized or Aborted): close world chunks and nation accounts (at most CLOSE_BATCH), their rent to the operator. */
  closeSeasonAccounts({ operator, targets }) {
    if (!targets.length || targets.length > CLOSE_BATCH) throw new Error(`closeSeasonAccounts: 1–${CLOSE_BATCH} targets per transaction`);
    return [this.ix([W(operator, true), R(this.season), ...targets.map(t => W(this.target(t)))], IX.closeSeasonAccounts(targets))];
  }
}

/** The delegation program's undelegation request of a delegated account (`["undelegation-request", pda]`). */
export const undelegationRequestPda = pda => PublicKey.findProgramAddressSync([Buffer.from('undelegation-request'), new PublicKey(pda).toBuffer()], DELEGATION_PROGRAM_ID)[0];
