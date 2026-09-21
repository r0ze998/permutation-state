import assert from "node:assert/strict";
import test from "node:test";
import { Buffer } from "buffer";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  DELEGATION_PROGRAM_ID,
  MAGIC_CONTEXT_ID,
  MAGIC_PROGRAM_ID,
} from "@magicblock-labs/ephemeral-rollups-sdk";
import {
  ACTION,
  EVENT,
  computeSeasonStateRoot,
  initialSeasonState,
  initialWorksiteState,
  previewEvent,
} from "./adapter.mjs";
import {
  TRANSPORT_SCHEMA,
  PURSE_SOURCE,
  SEASON_STATE_BYTES,
  WORKSITE_INSTRUCTION,
  WORKSITE_STATE_BYTES,
  buildApplyWorksiteEventInstruction,
  buildCommitAndUndelegateWorksiteInstruction,
  buildCommitWorksiteInstruction,
  buildDelegateWorksiteInstruction,
  buildInitializeWorksiteInstruction,
  buildCreditSeasonPurseInstruction,
  buildInitializeSeasonInstruction,
  buildFinalizeSeasonInstruction,
  buildClaimSeasonInstruction,
  decodeSeasonState,
  decodeWorksiteState,
  deriveDelegationAccounts,
  makeErTransactionReceipt,
  makeSolanaCheckpointReceipt,
  pollWorksiteState,
  resolveSolanaCheckpoint,
  sendApplyWorksiteEvent,
  sendCommitAndUndelegateWorksite,
  sendCommitWorksite,
  sendDelegateWorksite,
  sendInitializeWorksite,
  sendCreditSeasonPurse,
  sendInitializeSeason,
  submitWorksiteTransaction,
  subscribeWorksiteState,
} from "./magicblock-transport.mjs";

function fixture() {
  const programId = new PublicKey("11111111111111111111111111111112");
  const authoritySigner = Keypair.generate();
  const envoySigner = Keypair.generate();
  const makerSigner = Keypair.generate();
  const successorSigner = Keypair.generate();
  const stateFixture = initialWorksiteState({
    programId,
    authority: authoritySigner.publicKey,
    envoy: envoySigner.publicKey,
    maker: makerSigner.publicKey,
    successor: successorSigner.publicKey,
    seasonId: Buffer.alloc(32, 7),
    worksiteId: Buffer.alloc(32, 31),
    rulesetHash: Buffer.alloc(32, 9),
  });
  return {
    programId,
    authoritySigner,
    envoySigner,
    makerSigner,
    successorSigner,
    ...stateFixture,
  };
}

function seasonFixture(f = fixture()) {
  return {
    ...f,
    ...initialSeasonState({
      programId: f.programId,
      authority: f.authoritySigner.publicKey,
      seasonId: f.state.seasonId,
      rulesetHash: f.state.rulesetHash,
      payoutRulesHash: Buffer.alloc(32, 10),
    }),
  };
}

function u8(value) {
  return Buffer.from([value]);
}

function u16(value) {
  const out = Buffer.alloc(2);
  out.writeUInt16LE(value);
  return out;
}

function i16(value) {
  const out = Buffer.alloc(2);
  out.writeInt16LE(value);
  return out;
}

function u64(value) {
  const out = Buffer.alloc(8);
  out.writeBigUInt64LE(BigInt(value));
  return out;
}

function encodeWorksiteAccount(state, size = 384) {
  const payload = Buffer.concat([
    u8(state.version),
    u8(state.bump),
    state.authority.toBuffer(),
    state.envoy.toBuffer(),
    state.maker.toBuffer(),
    state.successor.toBuffer(),
    state.seasonId,
    state.worksiteId,
    state.rulesetHash,
    state.stateRoot,
    state.headEventHash,
    u64(state.seq),
    u8(state.stage),
    u8(state.seasonStatus),
    u8(state.settlementStatus),
    u8(state.claimAvailable ? 1 : 0),
    u8(state.branch),
    u8(state.resolution),
    i16(state.water),
    i16(state.food),
    i16(state.cohesion),
    i16(state.timber),
    i16(state.prosperity),
    i16(state.foodDebt),
    i16(state.talaTrust),
    u8(state.serviceStair),
    u8(state.riverkeepers),
    u8(state.memoryReceipt),
    u8(state.worksiteStatus),
    ...state.mandateIds.map(u16),
    ...state.mandateStatus.map(u8),
    u16(state.acceptedMandate),
    state.seasonPurse.toBuffer(),
  ]);
  assert.equal(payload.length, WORKSITE_STATE_BYTES);
  return Buffer.concat([payload, Buffer.alloc(size - payload.length)]);
}

function u32(value) {
  const out = Buffer.alloc(4);
  out.writeUInt32LE(value);
  return out;
}

function encodeSeasonAccount(state, size = 384) {
  const payload = Buffer.concat([
    u8(state.version),
    u8(state.bump),
    state.authority.toBuffer(),
    state.seasonId,
    state.rulesetHash,
    state.payoutRulesHash,
    state.outcomeHash,
    state.chronicleRoot,
    state.claimRoot,
    state.stateRoot,
    state.headEventHash,
    u8(state.status),
    u64(state.seq),
    u32(state.activeWorksites),
    u32(state.activeCitizens),
    u64(state.entryGrossUnits),
    u64(state.entryPurseUnits),
    u64(state.marketplaceGrossUnits),
    u64(state.marketplacePurseUnits),
    u64(state.sellerUnits),
    u64(state.opsUnits),
    u64(state.purseTotal),
    u64(state.claimableUnits),
    u64(state.claimedUnits),
    u32(state.claimCount),
  ]);
  assert.equal(payload.length, SEASON_STATE_BYTES);
  return Buffer.concat([payload, Buffer.alloc(size - payload.length)]);
}

function nodeConnectionMock() {
  let sent = 0;
  return {
    rpcEndpoint: "https://devnet-router.magicblock.app",
    async getClosestValidator() {
      return { identity: "MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57", fqdn: "devnet-as.magicblock.app" };
    },
    async sendAndConfirmTransaction(transaction, signers, options) {
      sent += 1;
      assert.ok(transaction.instructions.length > 0);
      assert.ok(signers.length > 0);
      assert.equal(options.commitment, "confirmed");
      return String(sent).repeat(88);
    },
    async getSignatureStatuses() {
      return { value: [{ slot: 1000 + sent }] };
    },
  };
}

test("MagicBlock builders preserve the coordinated Rust variants and account order", () => {
  const f = fixture();
  const initialize = buildInitializeWorksiteInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    worksitePda: f.worksitePda,
    state: f.state,
  });
  assert.equal(initialize.data[0], WORKSITE_INSTRUCTION.INITIALIZE);
  assert.equal(initialize.keys.length, 4);
  assert.ok(initialize.keys[3].pubkey.equals(f.state.seasonPurse));
  const validator = new PublicKey("MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57");
  const delegation = deriveDelegationAccounts({ programId: f.programId, worksitePda: f.worksitePda });
  const delegate = buildDelegateWorksiteInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    worksitePda: f.worksitePda,
    validator,
  });

  assert.deepEqual([...delegate.data], [WORKSITE_INSTRUCTION.DELEGATE]);
  assert.equal(delegate.keys.length, 9);
  assert.ok(delegate.keys[0].pubkey.equals(f.authoritySigner.publicKey));
  assert.equal(delegate.keys[0].isSigner, true);
  assert.ok(delegate.keys[1].pubkey.equals(SystemProgram.programId));
  assert.ok(delegate.keys[2].pubkey.equals(f.worksitePda));
  assert.ok(delegate.keys[3].pubkey.equals(f.programId));
  assert.ok(delegate.keys[4].pubkey.equals(delegation.delegationBuffer));
  assert.ok(delegate.keys[5].pubkey.equals(delegation.delegationRecord));
  assert.ok(delegate.keys[6].pubkey.equals(delegation.delegationMetadata));
  assert.ok(delegate.keys[7].pubkey.equals(DELEGATION_PROGRAM_ID));
  assert.ok(delegate.keys[8].pubkey.equals(validator));

  const commit = buildCommitWorksiteInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    worksitePda: f.worksitePda,
  });
  const undelegate = buildCommitAndUndelegateWorksiteInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    worksitePda: f.worksitePda,
  });
  for (const instruction of [commit, undelegate]) {
    assert.ok(instruction.keys[0].pubkey.equals(f.authoritySigner.publicKey));
    assert.ok(instruction.keys[1].pubkey.equals(f.worksitePda));
    assert.ok(instruction.keys[2].pubkey.equals(MAGIC_PROGRAM_ID));
    assert.ok(instruction.keys[3].pubkey.equals(MAGIC_CONTEXT_ID));
  }
  assert.deepEqual([...commit.data], [WORKSITE_INSTRUCTION.COMMIT]);
  assert.deepEqual([...undelegate.data], [WORKSITE_INSTRUCTION.COMMIT_AND_UNDELEGATE]);
});

test("Season Purse builders preserve tags, account order, and Borsh layouts", async () => {
  const f = seasonFixture();
  const initialize = buildInitializeSeasonInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    seasonPurse: f.seasonPurse,
    state: f.state,
  });
  assert.equal(initialize.data[0], WORKSITE_INSTRUCTION.INITIALIZE_SEASON);
  assert.equal(initialize.data.length, 129);
  assert.equal(initialize.keys.length, 3);
  assert.ok(initialize.keys[1].pubkey.equals(f.seasonPurse));

  const commonArgs = {
    expectedSeq: 0,
    expectedHeadEventHash: Buffer.alloc(32, 12),
    eventId: Buffer.alloc(32, 13),
  };
  const credit = buildCreditSeasonPurseInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    seasonPurse: f.seasonPurse,
    args: { ...commonArgs, sourceKind: PURSE_SOURCE.ENTRY, grossUnits: 10_000_000 },
  });
  assert.equal(credit.data[0], WORKSITE_INSTRUCTION.CREDIT_SEASON_PURSE);
  assert.equal(credit.data.length, 82);
  assert.equal(credit.keys[0].isSigner, true);
  assert.equal(credit.keys[0].isWritable, false);

  const finalize = buildFinalizeSeasonInstruction({
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    seasonPurse: f.seasonPurse,
    worksites: [f.worksitePda],
    args: {
      ...commonArgs,
      outcomeHash: Buffer.alloc(32, 15),
      chronicleRoot: Buffer.alloc(32, 16),
      claimRoot: Buffer.alloc(32, 17),
    },
  });
  assert.equal(finalize.data[0], WORKSITE_INSTRUCTION.FINALIZE_SEASON);
  assert.equal(finalize.data.length, 169);
  assert.equal(finalize.keys.length, 3);
  assert.equal(finalize.keys[2].isWritable, false);

  const claimPda = PublicKey.findProgramAddressSync(
    [Buffer.from("season_claim"), f.state.seasonId, f.successorSigner.publicKey.toBuffer()],
    f.programId,
  )[0];
  const claim = buildClaimSeasonInstruction({
    programId: f.programId,
    claimant: f.successorSigner.publicKey,
    seasonPurse: f.seasonPurse,
    claimPda,
    args: {
      ...commonArgs,
      amount: 1_000_000,
      leafIndex: 0,
      merkleProof: [Buffer.alloc(32, 14)],
    },
  });
  assert.equal(claim.data[0], WORKSITE_INSTRUCTION.CLAIM_SEASON);
  assert.equal(claim.data.length, 121);
  assert.equal(claim.keys.length, 4);

  const connection = nodeConnectionMock();
  const initialized = await sendInitializeSeason({
    connection,
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    seasonPurse: f.seasonPurse,
    state: f.state,
    signers: [f.authoritySigner],
  });
  const credited = await sendCreditSeasonPurse({
    connection,
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    seasonPurse: f.seasonPurse,
    args: { ...commonArgs, sourceKind: PURSE_SOURCE.ENTRY, grossUnits: 10_000_000 },
    signers: [f.authoritySigner],
  });
  assert.equal(initialized.operation, "initialize-season");
  assert.equal(credited.operation, "credit-season-purse");
});

test("Season decoder matches Constitution splits including floor remainders", () => {
  const f = seasonFixture();
  const state = {
    ...f.state,
    seq: 2,
    entryGrossUnits: 10_000_001,
    entryPurseUnits: 7_000_000,
    marketplaceGrossUnits: 100_000_001,
    marketplacePurseUnits: 1_500_000,
    sellerUnits: 97_500_001,
    opsUnits: 4_000_001,
    purseTotal: 8_500_000,
  };
  state.stateRoot = computeSeasonStateRoot(state);
  const decoded = decodeSeasonState(encodeSeasonAccount(state));
  assert.equal(decoded.decodedBytes, SEASON_STATE_BYTES);
  assert.equal(decoded.entryPurseUnits, 7_000_000);
  assert.equal(decoded.marketplacePurseUnits, 1_500_000);
  assert.equal(decoded.sellerUnits, 97_500_001);
  assert.equal(decoded.opsUnits, 4_000_001);
  assert.equal(decoded.purseTotal, 8_500_000);
  assert.equal(decoded.stateRootMatches, true);
});

test("ApplyWorksiteEvent keeps the existing 172-byte wire contract", () => {
  const f = fixture();
  const preview = previewEvent(f.state, {
    actor: f.envoySigner.publicKey,
    eventKind: EVENT.MARA_CHOICE,
    action: ACTION.OATH,
    clientEventHash: Buffer.alloc(32, 40),
  });
  const instruction = buildApplyWorksiteEventInstruction({
    programId: f.programId,
    actor: f.envoySigner.publicKey,
    worksitePda: f.worksitePda,
    args: preview.args,
  });
  assert.equal(instruction.data[0], WORKSITE_INSTRUCTION.APPLY_WORKSITE_EVENT);
  assert.equal(instruction.data.length, 172);
  assert.equal(instruction.keys[0].isSigner, true);
  assert.equal(instruction.keys[1].isWritable, true);
});

test("decoder reconstructs the fixed Worksite account and verifies its root", () => {
  const f = fixture();
  const decoded = decodeWorksiteState(encodeWorksiteAccount(f.state));
  assert.equal(decoded.decodedBytes, WORKSITE_STATE_BYTES);
  assert.equal(decoded.seq, 0);
  assert.equal(decoded.water, 28);
  assert.equal(decoded.claimAvailable, false);
  assert.ok(decoded.authority.equals(f.authoritySigner.publicKey));
  assert.ok(decoded.stateRootMatches);

  const tampered = encodeWorksiteAccount(f.state);
  tampered[290] ^= 0xff;
  assert.equal(decodeWorksiteState(tampered).stateRootMatches, false);
  const unsupported = encodeWorksiteAccount(f.state);
  unsupported[0] = 2;
  assert.throws(() => decodeWorksiteState(unsupported), /Unsupported Worksite state version/);
  assert.throws(() => decodeWorksiteState(Buffer.alloc(100)), /365-384 bytes/);
});

test("Node submission wrappers cover initialize, delegate, apply, commit, and undelegate", async () => {
  const f = fixture();
  const connection = nodeConnectionMock();
  const commonAuthority = {
    connection,
    programId: f.programId,
    authority: f.authoritySigner.publicKey,
    worksitePda: f.worksitePda,
    signers: [f.authoritySigner],
  };
  const initialized = await sendInitializeWorksite({ ...commonAuthority, state: f.state });
  const delegated = await sendDelegateWorksite(commonAuthority);
  const mara = previewEvent(f.state, {
    actor: f.envoySigner.publicKey,
    eventKind: EVENT.MARA_CHOICE,
    action: ACTION.OATH,
    clientEventHash: Buffer.alloc(32, 40),
  });
  const applied = await sendApplyWorksiteEvent({
    connection,
    programId: f.programId,
    actor: f.envoySigner.publicKey,
    worksitePda: f.worksitePda,
    args: mara.args,
    signers: [f.envoySigner],
  });
  const committed = await sendCommitWorksite(commonAuthority);
  const undelegated = await sendCommitAndUndelegateWorksite(commonAuthority);

  assert.deepEqual(
    [initialized.operation, delegated.operation, applied.operation, committed.operation, undelegated.operation],
    ["initialize", "delegate", "apply-worksite-event", "commit", "commit-and-undelegate"],
  );
  assert.equal(delegated.validator, "MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57");
  assert.deepEqual([...delegated.instruction.data], [WORKSITE_INSTRUCTION.DELEGATE]);
  assert.deepEqual([...committed.instruction.data], [WORKSITE_INSTRUCTION.COMMIT]);
  assert.deepEqual([...undelegated.instruction.data], [WORKSITE_INSTRUCTION.COMMIT_AND_UNDELEGATE]);
  assert.equal(undelegated.slot, 1005);
});

test("browser-wallet submission uses router-aware blockhash preparation without exposing keys", async () => {
  const f = fixture();
  const blockhash = PublicKey.default.toBase58();
  const connection = {
    rpcEndpoint: "https://devnet-router.magicblock.app",
    async getLatestBlockhashForTransaction() {
      return { blockhash, lastValidBlockHeight: 42 };
    },
    async sendRawTransaction(raw) {
      assert.ok(raw.length > 0);
      return "w".repeat(88);
    },
    async confirmTransaction(strategy) {
      assert.equal(strategy.lastValidBlockHeight, 42);
      return { context: { slot: 901 }, value: { err: null } };
    },
  };
  const wallet = {
    publicKey: f.authoritySigner.publicKey,
    async signTransaction(transaction) {
      transaction.partialSign(f.authoritySigner);
      return transaction;
    },
  };
  const instruction = buildCommitWorksiteInstruction({
    programId: f.programId,
    authority: wallet.publicKey,
    worksitePda: f.worksitePda,
  });
  const result = await submitWorksiteTransaction({
    connection,
    instructions: [instruction],
    wallet,
  });
  assert.equal(result.signature, "w".repeat(88));
  assert.equal(result.slot, 901);
});

test("subscription and polling decode state instead of forwarding opaque bytes", async () => {
  const f = fixture();
  const encoded = encodeWorksiteAccount(f.state);
  let accountCallback;
  let removed = null;
  const connection = {
    onAccountChange(_address, callback, commitment) {
      assert.equal(commitment, "processed");
      accountCallback = callback;
      return 73;
    },
    async removeAccountChangeListener(id) {
      removed = id;
    },
    async getAccountInfoAndContext() {
      return {
        context: { slot: 501 },
        value: { data: encoded, owner: f.programId, lamports: 1_000_000 },
      };
    },
  };

  let update;
  const unsubscribe = subscribeWorksiteState({
    connection,
    worksitePda: f.worksitePda,
    onState: (value) => { update = value; },
  });
  accountCallback({ data: encoded, owner: f.programId, lamports: 1_000_000 }, { slot: 500 });
  assert.equal(update.slot, 500);
  assert.equal(update.state.water, 28);
  assert.equal(update.state.stateRootMatches, true);
  await unsubscribe();
  assert.equal(removed, 73);

  const polled = await pollWorksiteState({
    connection,
    worksitePda: f.worksitePda,
    predicate: (state) => state.seq === 0,
    intervalMs: 0,
  });
  assert.equal(polled.slot, 501);
});

test("receipts keep ER execution distinct from verified Solana settlement", async () => {
  const f = fixture();
  const encoded = encodeWorksiteAccount(f.state);
  const decoded = decodeWorksiteState(encoded);
  const erSignature = "e".repeat(88);
  const baseSignature = "b".repeat(88);
  const scheduleSignature = "s".repeat(88);

  const erReceipt = makeErTransactionReceipt({
    operation: "commit",
    signature: erSignature,
    slot: 77,
    programId: f.programId,
    worksitePda: f.worksitePda,
    state: decoded,
  });
  assert.equal(erReceipt.schemaVersion, TRANSPORT_SCHEMA);
  assert.equal(erReceipt.layer, "ephemeral-rollup");
  assert.equal(erReceipt.settlement, "not-yet-proven-on-solana");
  assert.match(erReceipt.explorerUrl, /cluster=custom/);

  const directCheckpoint = makeSolanaCheckpointReceipt({
    signature: baseSignature,
    slot: 88,
    readBackSlot: 89,
    confirmationStatus: "confirmed",
    programId: f.programId,
    worksitePda: f.worksitePda,
    expectedStateRoot: f.state.stateRoot,
    expectedHeadEventHash: f.state.headEventHash,
    state: decoded,
    sourceErSignature: erSignature,
  });
  assert.equal(directCheckpoint.layer, "solana");
  assert.equal(directCheckpoint.settlement, "checkpoint-read-back-verified");
  assert.equal(directCheckpoint.sourceErSignature, erSignature);
  assert.throws(() => makeSolanaCheckpointReceipt({
    signature: baseSignature,
    slot: 88,
    readBackSlot: 89,
    programId: f.programId,
    worksitePda: f.worksitePda,
    expectedStateRoot: f.state.stateRoot,
    expectedHeadEventHash: f.state.headEventHash,
    state: decoded,
  }), /must be confirmed or finalized/);

  const badState = { ...decoded, stateRoot: Buffer.alloc(32, 99) };
  assert.throws(() => makeSolanaCheckpointReceipt({
    signature: baseSignature,
    slot: 88,
    readBackSlot: 89,
    confirmationStatus: "confirmed",
    programId: f.programId,
    worksitePda: f.worksitePda,
    expectedStateRoot: f.state.stateRoot,
    expectedHeadEventHash: f.state.headEventHash,
    state: badState,
  }), /does not equal/);
  assert.throws(() => makeSolanaCheckpointReceipt({
    signature: baseSignature,
    slot: 88,
    readBackSlot: 89,
    confirmationStatus: "confirmed",
    programId: f.programId,
    worksitePda: f.worksitePda,
    expectedStateRoot: f.state.stateRoot,
    expectedHeadEventHash: Buffer.alloc(32, 42),
    state: decoded,
  }), /event-chain head does not equal/);

  const ephemeralConnection = {
    async getTransaction(signature) {
      if (signature === erSignature) {
        return { meta: { err: null, logMessages: [`ScheduledCommitSent signature: ${scheduleSignature}`] } };
      }
      assert.equal(signature, scheduleSignature);
      return { meta: { err: null, logMessages: [`ScheduledCommitSent signature[0]: ${baseSignature}`] } };
    },
    async getLatestBlockhash() {
      return { blockhash: PublicKey.default.toBase58(), lastValidBlockHeight: 12 };
    },
    async confirmTransaction() {
      return { value: { err: null } };
    },
  };
  const baseConnection = {
    async getSignatureStatuses(signatures, options) {
      assert.deepEqual(signatures, [baseSignature]);
      assert.equal(options.searchTransactionHistory, true);
      return {
        value: [{ err: null, slot: 998, confirmations: 1, confirmationStatus: "confirmed" }],
      };
    },
    async getAccountInfoAndContext() {
      return {
        context: { slot: 999 },
        value: { data: encoded, owner: DELEGATION_PROGRAM_ID, lamports: 1_000_000 },
      };
    },
  };
  const resolved = await resolveSolanaCheckpoint({
    erSignature,
    ephemeralConnection,
    baseConnection,
    programId: f.programId,
    worksitePda: f.worksitePda,
    expectedStateRoot: f.state.stateRoot,
    expectedHeadEventHash: f.state.headEventHash,
    intervalMs: 0,
  });
  assert.equal(resolved.signature, baseSignature);
  assert.equal(resolved.slot, 998);
  assert.equal(resolved.readBackSlot, 999);
  assert.equal(resolved.confirmationStatus, "confirmed");
  assert.equal(resolved.sourceErSignature, erSignature);
});
