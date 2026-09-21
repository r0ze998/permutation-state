/**
 * MagicBlock transport for the Permutation State Worksite vertical slice.
 *
 * This module intentionally owns transport only. The deterministic transition
 * preview and fixed-width state-root contract remain in adapter.mjs.
 *
 * Wire contract coordinated with the pending Native Rust ER program:
 *   0 Initialize(InitializeArgs)
 *   1 ApplyWorksiteEvent(ApplyEventArgs)
 *   2 Delegate
 *   3 Commit
 *   4 CommitAndUndelegate
 *
 * Variants 2-4 have no payload and therefore serialize to one Borsh enum byte.
 * The validator-only undelegation callback is not client-callable. Its fixed
 * discriminator remains [196, 28, 41, 206, 48, 37, 51, 167] in Rust.
 */

import { Buffer } from "buffer";
import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  ConnectionMagicRouter,
  DELEGATION_PROGRAM_ID,
  GetCommitmentSignature,
  MAGIC_CONTEXT_ID,
  MAGIC_PROGRAM_ID,
  delegateBufferPdaFromDelegatedAccountAndOwnerProgram,
  delegationMetadataPdaFromDelegatedAccount,
  delegationRecordPdaFromDelegatedAccount,
} from "@magicblock-labs/ephemeral-rollups-sdk";
import {
  ACCOUNT_SPACE,
  buildApplyInstruction,
  buildInitializeInstruction,
  computeSeasonStateRoot,
  computeStateRoot,
  deriveSeasonPursePda,
  deriveWorksitePda,
} from "./adapter.mjs";

export { deriveSeasonPursePda, deriveWorksitePda };

export const MAGICBLOCK_SDK_VERSION = "0.17.2";
export const TRANSPORT_SCHEMA = "permutation-state.magicblock-transport.v1";
export const WORKSITE_STATE_BYTES = 365;
export const SEASON_STATE_BYTES = 383;
export const SEASON_ACCOUNT_SPACE = 384;
export const CLAIM_ACCOUNT_SPACE = 160;
export const WORKSITE_INSTRUCTION = Object.freeze({
  INITIALIZE: 0,
  APPLY_WORKSITE_EVENT: 1,
  DELEGATE: 2,
  COMMIT: 3,
  COMMIT_AND_UNDELEGATE: 4,
  INITIALIZE_SEASON: 5,
  CREDIT_SEASON_PURSE: 6,
  FINALIZE_SEASON: 7,
  CLAIM_SEASON: 8,
});
export const PURSE_SOURCE = Object.freeze({ ENTRY: 1, MARKETPLACE: 2 });
export const DEFAULT_ENDPOINTS = Object.freeze({
  routerHttp: "https://devnet-router.magicblock.app",
  routerWs: "wss://devnet-router.magicblock.app",
  erHttp: "https://devnet-as.magicblock.app",
  erWs: "wss://devnet-as.magicblock.app",
  solanaHttp: "https://api.devnet.solana.com",
  solanaWs: "wss://api.devnet.solana.com",
  cluster: "devnet",
});

function asPublicKey(value, label) {
  try {
    return value instanceof PublicKey ? value : new PublicKey(value);
  } catch {
    throw new Error(`${label} must be a valid public key`);
  }
}

function asBytes32(value, label) {
  const bytes = typeof value === "string" && /^[0-9a-fA-F]{64}$/.test(value)
    ? Buffer.from(value, "hex")
    : Buffer.from(value);
  if (bytes.length !== 32) throw new Error(`${label} must be exactly 32 bytes`);
  return bytes;
}

function sameBytes(left, right) {
  return Buffer.from(left).equals(Buffer.from(right));
}

function sleep(milliseconds, signal) {
  if (signal?.aborted) return Promise.reject(signal.reason || new Error("Operation aborted"));
  return new Promise((resolve, reject) => {
    const onAbort = () => {
      clearTimeout(timer);
      reject(signal.reason || new Error("Operation aborted"));
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, milliseconds);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

export function createMagicRouterConnection({
  httpEndpoint = DEFAULT_ENDPOINTS.routerHttp,
  wsEndpoint = DEFAULT_ENDPOINTS.routerWs,
  commitment = "confirmed",
} = {}) {
  const connection = new ConnectionMagicRouter(httpEndpoint, { wsEndpoint, commitment });
  // The standalone MagicBlock stack returns the Solana RPC envelope
  // (`{ context, value }`) for getBlockhashForAccounts, while the hosted
  // router returns the value directly. Normalize both shapes here so the
  // SDK's transaction sender always receives `blockhash` at the top level.
  const getRouterBlockhash = connection.getLatestBlockhashForTransaction.bind(connection);
  connection.getLatestBlockhashForTransaction = async (...args) => {
    const result = await getRouterBlockhash(...args);
    return result?.value?.blockhash ? result.value : result;
  };
  return connection;
}

export function createBaseLayerConnection({
  httpEndpoint = DEFAULT_ENDPOINTS.solanaHttp,
  wsEndpoint = DEFAULT_ENDPOINTS.solanaWs,
  commitment = "confirmed",
} = {}) {
  return new Connection(httpEndpoint, { wsEndpoint, commitment });
}

export function createEphemeralConnection({
  httpEndpoint = DEFAULT_ENDPOINTS.erHttp,
  wsEndpoint = DEFAULT_ENDPOINTS.erWs,
  commitment = "confirmed",
} = {}) {
  return new Connection(httpEndpoint, { wsEndpoint, commitment });
}

export async function getClosestValidator(connection) {
  if (!connection || typeof connection.getClosestValidator !== "function") {
    throw new Error("A ConnectionMagicRouter instance is required to discover a validator");
  }
  const result = await connection.getClosestValidator();
  return {
    publicKey: asPublicKey(result.identity, "validator identity"),
    fqdn: result.fqdn || null,
  };
}

export async function getWorksiteDelegationStatus(connection, worksitePda) {
  if (!connection || typeof connection.getDelegationStatus !== "function") {
    throw new Error("A ConnectionMagicRouter instance is required to read delegation status");
  }
  const address = asPublicKey(worksitePda, "worksitePda");
  const status = await connection.getDelegationStatus(address);
  return {
    worksitePda: address,
    isDelegated: Boolean(status?.isDelegated),
    raw: status,
  };
}

export function deriveDelegationAccounts({ programId, worksitePda }) {
  const ownerProgram = asPublicKey(programId, "programId");
  const delegatedAccount = asPublicKey(worksitePda, "worksitePda");
  return {
    ownerProgram,
    delegatedAccount,
    delegationBuffer: delegateBufferPdaFromDelegatedAccountAndOwnerProgram(
      delegatedAccount,
      ownerProgram,
    ),
    delegationRecord: delegationRecordPdaFromDelegatedAccount(delegatedAccount),
    delegationMetadata: delegationMetadataPdaFromDelegatedAccount(delegatedAccount),
    delegationProgram: DELEGATION_PROGRAM_ID,
  };
}

export function buildDelegateWorksiteInstruction({
  programId,
  authority,
  worksitePda,
  validator,
}) {
  const ownerProgram = asPublicKey(programId, "programId");
  const payer = asPublicKey(authority, "authority");
  const accounts = deriveDelegationAccounts({ programId: ownerProgram, worksitePda });
  const validatorKey = validator == null ? null : asPublicKey(validator, "validator");
  return new TransactionInstruction({
    programId: ownerProgram,
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: accounts.delegatedAccount, isSigner: false, isWritable: true },
      { pubkey: accounts.ownerProgram, isSigner: false, isWritable: false },
      { pubkey: accounts.delegationBuffer, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationRecord, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationMetadata, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationProgram, isSigner: false, isWritable: false },
      ...(validatorKey
        ? [{ pubkey: validatorKey, isSigner: false, isWritable: false }]
        : []),
    ],
    data: Buffer.from([WORKSITE_INSTRUCTION.DELEGATE]),
  });
}

export function buildApplyWorksiteEventInstruction({
  programId,
  actor,
  worksitePda,
  args,
}) {
  return buildApplyInstruction({ programId, actor, worksitePda, args });
}

export function buildInitializeWorksiteInstruction(params) {
  return buildInitializeInstruction(params);
}

function encodeU32(value, label) {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${label} must be an unsigned 32-bit integer`);
  }
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value);
  return bytes;
}

function encodeU64(value, label) {
  let amount;
  try {
    amount = BigInt(value);
  } catch {
    throw new Error(`${label} must be an unsigned 64-bit integer`);
  }
  if (amount < 0n || amount > 0xffff_ffff_ffff_ffffn) {
    throw new Error(`${label} must be an unsigned 64-bit integer`);
  }
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(amount);
  return bytes;
}

export function deriveSeasonClaimPda(programId, seasonId, claimant) {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("season_claim"),
      asBytes32(seasonId, "seasonId"),
      asPublicKey(claimant, "claimant").toBuffer(),
    ],
    asPublicKey(programId, "programId"),
  );
}

export function buildInitializeSeasonInstruction({
  programId,
  authority,
  seasonPurse,
  state,
}) {
  const data = Buffer.concat([
    Buffer.from([WORKSITE_INSTRUCTION.INITIALIZE_SEASON]),
    asBytes32(state.seasonId, "seasonId"),
    asBytes32(state.rulesetHash, "rulesetHash"),
    asBytes32(state.payoutRulesHash, "payoutRulesHash"),
    asBytes32(state.stateRoot, "expectedGenesisStateRoot"),
  ]);
  return new TransactionInstruction({
    programId: asPublicKey(programId, "programId"),
    keys: [
      { pubkey: asPublicKey(authority, "authority"), isSigner: true, isWritable: true },
      { pubkey: asPublicKey(seasonPurse, "seasonPurse"), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

export function buildCreditSeasonPurseInstruction({
  programId,
  authority,
  seasonPurse,
  args,
}) {
  if (![PURSE_SOURCE.ENTRY, PURSE_SOURCE.MARKETPLACE].includes(args.sourceKind)) {
    throw new Error("sourceKind must be ENTRY or MARKETPLACE");
  }
  return new TransactionInstruction({
    programId: asPublicKey(programId, "programId"),
    keys: [
      { pubkey: asPublicKey(authority, "authority"), isSigner: true, isWritable: false },
      { pubkey: asPublicKey(seasonPurse, "seasonPurse"), isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([
      Buffer.from([WORKSITE_INSTRUCTION.CREDIT_SEASON_PURSE, args.sourceKind]),
      encodeU64(args.grossUnits, "grossUnits"),
      encodeU64(args.expectedSeq, "expectedSeq"),
      asBytes32(args.expectedHeadEventHash, "expectedHeadEventHash"),
      asBytes32(args.eventId, "eventId"),
    ]),
  });
}

export function buildFinalizeSeasonInstruction({
  programId,
  authority,
  seasonPurse,
  worksites = [],
  args,
}) {
  return new TransactionInstruction({
    programId: asPublicKey(programId, "programId"),
    keys: [
      { pubkey: asPublicKey(authority, "authority"), isSigner: true, isWritable: false },
      { pubkey: asPublicKey(seasonPurse, "seasonPurse"), isSigner: false, isWritable: true },
      ...worksites.map((worksite) => ({
        pubkey: asPublicKey(worksite, "worksite"),
        isSigner: false,
        isWritable: false,
      })),
    ],
    data: Buffer.concat([
      Buffer.from([WORKSITE_INSTRUCTION.FINALIZE_SEASON]),
      encodeU64(args.expectedSeq, "expectedSeq"),
      asBytes32(args.expectedHeadEventHash, "expectedHeadEventHash"),
      asBytes32(args.eventId, "eventId"),
      asBytes32(args.outcomeHash, "outcomeHash"),
      asBytes32(args.chronicleRoot, "chronicleRoot"),
      asBytes32(args.claimRoot, "claimRoot"),
    ]),
  });
}

export function buildClaimSeasonInstruction({
  programId,
  claimant,
  seasonPurse,
  claimPda,
  args,
}) {
  const proof = Array.isArray(args.merkleProof) ? args.merkleProof : [];
  if (proof.length > 20) throw new Error("merkleProof cannot contain more than 20 nodes");
  return new TransactionInstruction({
    programId: asPublicKey(programId, "programId"),
    keys: [
      { pubkey: asPublicKey(claimant, "claimant"), isSigner: true, isWritable: true },
      { pubkey: asPublicKey(seasonPurse, "seasonPurse"), isSigner: false, isWritable: true },
      { pubkey: asPublicKey(claimPda, "claimPda"), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([
      Buffer.from([WORKSITE_INSTRUCTION.CLAIM_SEASON]),
      encodeU64(args.amount, "amount"),
      encodeU32(args.leafIndex, "leafIndex"),
      encodeU32(proof.length, "merkleProof.length"),
      ...proof.map((node) => asBytes32(node, "merkleProof node")),
      encodeU64(args.expectedSeq, "expectedSeq"),
      asBytes32(args.expectedHeadEventHash, "expectedHeadEventHash"),
      asBytes32(args.eventId, "eventId"),
    ]),
  });
}

function buildSettlementInstruction({
  programId,
  authority,
  payer,
  worksitePda,
  variant,
}) {
  const lifecyclePayer = payer ?? authority;
  return new TransactionInstruction({
    programId: asPublicKey(programId, "programId"),
    keys: [
      { pubkey: asPublicKey(lifecyclePayer, "payer"), isSigner: true, isWritable: true },
      { pubkey: asPublicKey(worksitePda, "worksitePda"), isSigner: false, isWritable: true },
      { pubkey: MAGIC_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: MAGIC_CONTEXT_ID, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([variant]),
  });
}

export function buildCommitWorksiteInstruction(params) {
  return buildSettlementInstruction({ ...params, variant: WORKSITE_INSTRUCTION.COMMIT });
}

export function buildCommitAndUndelegateWorksiteInstruction(params) {
  return buildSettlementInstruction({
    ...params,
    variant: WORKSITE_INSTRUCTION.COMMIT_AND_UNDELEGATE,
  });
}

function resolveFeePayer({ feePayer, wallet, signers }) {
  if (feePayer) return asPublicKey(feePayer, "feePayer");
  if (wallet?.publicKey) return asPublicKey(wallet.publicKey, "wallet.publicKey");
  if (signers?.[0]?.publicKey) return asPublicKey(signers[0].publicKey, "signers[0].publicKey");
  throw new Error("feePayer, wallet.publicKey, or signers[0].publicKey is required");
}

async function signatureSlot(connection, signature) {
  if (typeof connection.getSignatureStatuses !== "function") return null;
  const result = await connection.getSignatureStatuses(
    [signature],
    { searchTransactionHistory: true },
  );
  return result?.value?.[0]?.slot ?? null;
}

/**
 * Submit with either a browser wallet (`wallet.signTransaction`) or Node signers.
 * The function never creates, persists, logs, or exports key material.
 */
export async function submitWorksiteTransaction({
  connection,
  instructions,
  feePayer,
  wallet = null,
  signers = [],
  confirmOptions = {},
  sendOptions = {},
}) {
  if (!connection) throw new Error("connection is required");
  if (!Array.isArray(instructions) || instructions.length === 0) {
    throw new Error("at least one instruction is required");
  }
  const payer = resolveFeePayer({ feePayer, wallet, signers });
  const transaction = new Transaction();
  transaction.feePayer = payer;
  transaction.add(...instructions);
  const options = {
    commitment: "confirmed",
    skipPreflight: true,
    ...confirmOptions,
  };

  let signature;
  let slot = null;
  if (wallet) {
    if (typeof wallet.signTransaction !== "function") {
      throw new Error("wallet.signTransaction is required for browser-wallet submission");
    }
    const latest = typeof connection.getLatestBlockhashForTransaction === "function"
      ? await connection.getLatestBlockhashForTransaction(transaction, options)
      : await connection.getLatestBlockhash(options.commitment);
    transaction.recentBlockhash = latest.blockhash;
    transaction.lastValidBlockHeight = latest.lastValidBlockHeight;
    if (signers.length) transaction.partialSign(...signers);
    const signed = await wallet.signTransaction(transaction);
    signature = await connection.sendRawTransaction(signed.serialize(), {
      skipPreflight: options.skipPreflight,
      ...sendOptions,
    });
    const confirmation = await connection.confirmTransaction({
      signature,
      blockhash: latest.blockhash,
      lastValidBlockHeight: latest.lastValidBlockHeight,
    }, options.commitment);
    if (confirmation.value.err) {
      throw new Error(`transaction ${signature} failed: ${JSON.stringify(confirmation.value.err)}`);
    }
    slot = confirmation.context?.slot ?? null;
  } else {
    if (!signers.length) throw new Error("at least one signer is required for Node submission");
    signature = typeof connection.sendAndConfirmTransaction === "function"
      ? await connection.sendAndConfirmTransaction(transaction, signers, options)
      : await sendAndConfirmTransaction(connection, transaction, signers, options);
    slot = await signatureSlot(connection, signature);
  }

  return {
    signature,
    slot,
    commitment: options.commitment,
    rpcEndpoint: connection.rpcEndpoint || null,
  };
}

function transactionSubmission(params, instruction) {
  return submitWorksiteTransaction({
    connection: params.connection,
    instructions: [instruction],
    feePayer: params.feePayer,
    wallet: params.wallet,
    signers: params.signers,
    confirmOptions: params.confirmOptions,
    sendOptions: params.sendOptions,
  });
}

export async function sendDelegateWorksite(params) {
  const validator = params.validator == null
    ? (await getClosestValidator(params.connection)).publicKey
    : asPublicKey(params.validator, "validator");
  const instruction = buildDelegateWorksiteInstruction({ ...params, validator });
  return {
    operation: "delegate",
    validator: validator.toBase58(),
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendInitializeWorksite(params) {
  const instruction = buildInitializeWorksiteInstruction(params);
  return {
    operation: "initialize",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendInitializeSeason(params) {
  const instruction = buildInitializeSeasonInstruction(params);
  return {
    operation: "initialize-season",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendCreditSeasonPurse(params) {
  const instruction = buildCreditSeasonPurseInstruction(params);
  return {
    operation: "credit-season-purse",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendFinalizeSeason(params) {
  const instruction = buildFinalizeSeasonInstruction(params);
  return {
    operation: "finalize-season",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendClaimSeason(params) {
  const instruction = buildClaimSeasonInstruction(params);
  return {
    operation: "claim-season",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendApplyWorksiteEvent(params) {
  const instruction = buildApplyWorksiteEventInstruction(params);
  return {
    operation: "apply-worksite-event",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendCommitWorksite(params) {
  const instruction = buildCommitWorksiteInstruction(params);
  return {
    operation: "commit",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

export async function sendCommitAndUndelegateWorksite(params) {
  const instruction = buildCommitAndUndelegateWorksiteInstruction(params);
  return {
    operation: "commit-and-undelegate",
    instruction,
    ...(await transactionSubmission(params, instruction)),
  };
}

class StateCursor {
  constructor(data) {
    this.bytes = Buffer.from(data);
    this.offset = 0;
  }

  take(length, label) {
    if (this.offset + length > this.bytes.length) {
      throw new Error(`Worksite account ended while reading ${label}`);
    }
    const value = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }

  u8(label) {
    return this.take(1, label)[0];
  }

  bool(label) {
    const value = this.u8(label);
    if (value !== 0 && value !== 1) throw new Error(`${label} is not a Borsh bool`);
    return value === 1;
  }

  u16(label) {
    return this.take(2, label).readUInt16LE(0);
  }

  u32(label) {
    return this.take(4, label).readUInt32LE(0);
  }

  i16(label) {
    return this.take(2, label).readInt16LE(0);
  }

  u64(label) {
    const value = this.take(8, label).readBigUInt64LE(0);
    if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error(`${label} exceeds JavaScript's safe integer range`);
    }
    return Number(value);
  }

  pubkey(label) {
    return new PublicKey(this.take(32, label));
  }

  bytes32(label) {
    return Buffer.from(this.take(32, label));
  }
}

export function decodeWorksiteState(data) {
  const bytes = Buffer.from(data);
  if (bytes.length < WORKSITE_STATE_BYTES || bytes.length > ACCOUNT_SPACE) {
    throw new Error(
      `Worksite account data must be ${WORKSITE_STATE_BYTES}-${ACCOUNT_SPACE} bytes; received ${bytes.length}`,
    );
  }
  const cursor = new StateCursor(bytes);
  const state = {
    version: cursor.u8("version"),
    bump: cursor.u8("bump"),
    authority: cursor.pubkey("authority"),
    envoy: cursor.pubkey("envoy"),
    maker: cursor.pubkey("maker"),
    successor: cursor.pubkey("successor"),
    seasonId: cursor.bytes32("seasonId"),
    worksiteId: cursor.bytes32("worksiteId"),
    rulesetHash: cursor.bytes32("rulesetHash"),
    stateRoot: cursor.bytes32("stateRoot"),
    headEventHash: cursor.bytes32("headEventHash"),
    seq: cursor.u64("seq"),
    stage: cursor.u8("stage"),
    seasonStatus: cursor.u8("seasonStatus"),
    settlementStatus: cursor.u8("settlementStatus"),
    claimAvailable: cursor.bool("claimAvailable"),
    branch: cursor.u8("branch"),
    resolution: cursor.u8("resolution"),
    water: cursor.i16("water"),
    food: cursor.i16("food"),
    cohesion: cursor.i16("cohesion"),
    timber: cursor.i16("timber"),
    prosperity: cursor.i16("prosperity"),
    foodDebt: cursor.i16("foodDebt"),
    talaTrust: cursor.i16("talaTrust"),
    serviceStair: cursor.u8("serviceStair"),
    riverkeepers: cursor.u8("riverkeepers"),
    memoryReceipt: cursor.u8("memoryReceipt"),
    worksiteStatus: cursor.u8("worksiteStatus"),
    mandateIds: [
      cursor.u16("mandateIds[0]"),
      cursor.u16("mandateIds[1]"),
      cursor.u16("mandateIds[2]"),
    ],
    mandateStatus: [
      cursor.u8("mandateStatus[0]"),
      cursor.u8("mandateStatus[1]"),
      cursor.u8("mandateStatus[2]"),
    ],
    acceptedMandate: cursor.u16("acceptedMandate"),
    seasonPurse: cursor.pubkey("seasonPurse"),
  };
  if (state.version !== 1) throw new Error(`Unsupported Worksite state version ${state.version}`);
  const computedStateRoot = computeStateRoot(state);
  return {
    ...state,
    computedStateRoot,
    stateRootMatches: sameBytes(state.stateRoot, computedStateRoot),
    decodedBytes: cursor.offset,
  };
}

export function decodeSeasonState(data) {
  const bytes = Buffer.from(data);
  if (bytes.length < SEASON_STATE_BYTES || bytes.length > SEASON_ACCOUNT_SPACE) {
    throw new Error(
      `Season account data must be ${SEASON_STATE_BYTES}-${SEASON_ACCOUNT_SPACE} bytes; received ${bytes.length}`,
    );
  }
  const cursor = new StateCursor(bytes);
  const state = {
    version: cursor.u8("version"),
    bump: cursor.u8("bump"),
    authority: cursor.pubkey("authority"),
    seasonId: cursor.bytes32("seasonId"),
    rulesetHash: cursor.bytes32("rulesetHash"),
    payoutRulesHash: cursor.bytes32("payoutRulesHash"),
    outcomeHash: cursor.bytes32("outcomeHash"),
    chronicleRoot: cursor.bytes32("chronicleRoot"),
    claimRoot: cursor.bytes32("claimRoot"),
    stateRoot: cursor.bytes32("stateRoot"),
    headEventHash: cursor.bytes32("headEventHash"),
    status: cursor.u8("status"),
    seq: cursor.u64("seq"),
    activeWorksites: cursor.u32("activeWorksites"),
    activeCitizens: cursor.u32("activeCitizens"),
    entryGrossUnits: cursor.u64("entryGrossUnits"),
    entryPurseUnits: cursor.u64("entryPurseUnits"),
    marketplaceGrossUnits: cursor.u64("marketplaceGrossUnits"),
    marketplacePurseUnits: cursor.u64("marketplacePurseUnits"),
    sellerUnits: cursor.u64("sellerUnits"),
    opsUnits: cursor.u64("opsUnits"),
    purseTotal: cursor.u64("purseTotal"),
    claimableUnits: cursor.u64("claimableUnits"),
    claimedUnits: cursor.u64("claimedUnits"),
    claimCount: cursor.u32("claimCount"),
  };
  if (state.version !== 1) throw new Error(`Unsupported Season state version ${state.version}`);
  const computedStateRoot = computeSeasonStateRoot(state);
  return {
    ...state,
    computedStateRoot,
    stateRootMatches: sameBytes(state.stateRoot, computedStateRoot),
    decodedBytes: cursor.offset,
  };
}

async function accountInfoWithContext(connection, address, commitment) {
  if (typeof connection.getAccountInfoAndContext === "function") {
    return connection.getAccountInfoAndContext(address, commitment);
  }
  return {
    context: { slot: null },
    value: await connection.getAccountInfo(address, commitment),
  };
}

export async function readWorksiteState(connection, worksitePda, commitment = "confirmed") {
  const address = asPublicKey(worksitePda, "worksitePda");
  const result = await accountInfoWithContext(connection, address, commitment);
  if (!result.value) throw new Error(`Worksite PDA ${address.toBase58()} was not found`);
  return {
    address,
    slot: result.context?.slot ?? null,
    owner: result.value.owner,
    lamports: result.value.lamports,
    state: decodeWorksiteState(result.value.data),
  };
}

export async function readSeasonState(connection, seasonPurse, commitment = "confirmed") {
  const address = asPublicKey(seasonPurse, "seasonPurse");
  const result = await accountInfoWithContext(connection, address, commitment);
  if (!result.value) throw new Error(`Season PDA ${address.toBase58()} was not found`);
  return {
    address,
    slot: result.context?.slot ?? null,
    owner: result.value.owner,
    lamports: result.value.lamports,
    state: decodeSeasonState(result.value.data),
  };
}

export function subscribeWorksiteState({
  connection,
  worksitePda,
  onState,
  onError = () => {},
  commitment = "processed",
}) {
  if (typeof onState !== "function") throw new Error("onState callback is required");
  const address = asPublicKey(worksitePda, "worksitePda");
  const subscriptionId = connection.onAccountChange(address, (accountInfo, context) => {
    try {
      onState({
        address,
        slot: context.slot,
        owner: accountInfo.owner,
        lamports: accountInfo.lamports,
        state: decodeWorksiteState(accountInfo.data),
      });
    } catch (error) {
      onError(error);
    }
  }, commitment);
  return async () => connection.removeAccountChangeListener(await subscriptionId);
}

export async function pollWorksiteState({
  connection,
  worksitePda,
  predicate = () => true,
  commitment = "confirmed",
  intervalMs = 250,
  timeoutMs = 20_000,
  signal,
}) {
  const startedAt = Date.now();
  let latest = null;
  while (Date.now() - startedAt <= timeoutMs) {
    if (signal?.aborted) throw signal.reason || new Error("Operation aborted");
    latest = await readWorksiteState(connection, worksitePda, commitment);
    if (await predicate(latest.state, latest)) return latest;
    await sleep(intervalMs, signal);
  }
  const lastRoot = latest?.state?.stateRoot
    ? Buffer.from(latest.state.stateRoot).toString("hex")
    : "unavailable";
  throw new Error(`Timed out waiting for Worksite state; last root ${lastRoot}`);
}

export async function pollSeasonState({
  connection,
  seasonPurse,
  predicate = () => true,
  commitment = "confirmed",
  intervalMs = 250,
  timeoutMs = 20_000,
  signal,
}) {
  const startedAt = Date.now();
  let latest = null;
  while (Date.now() - startedAt <= timeoutMs) {
    if (signal?.aborted) throw signal.reason || new Error("Operation aborted");
    latest = await readSeasonState(connection, seasonPurse, commitment);
    if (await predicate(latest.state, latest)) return latest;
    await sleep(intervalMs, signal);
  }
  const lastRoot = latest?.state?.stateRoot
    ? Buffer.from(latest.state.stateRoot).toString("hex")
    : "unavailable";
  throw new Error(`Timed out waiting for Season state; last root ${lastRoot}`);
}

export function erExplorerUrl(signature, rpcEndpoint = DEFAULT_ENDPOINTS.erHttp) {
  return `https://solscan.io/tx/${signature}?cluster=custom&customUrl=${encodeURIComponent(rpcEndpoint)}`;
}

export function solanaExplorerUrl(signature, cluster = DEFAULT_ENDPOINTS.cluster) {
  return `https://explorer.solana.com/tx/${signature}?cluster=${encodeURIComponent(cluster)}`;
}

export function makeErTransactionReceipt({
  operation,
  signature,
  slot = null,
  programId,
  worksitePda,
  rpcEndpoint = DEFAULT_ENDPOINTS.erHttp,
  state = null,
}) {
  if (!signature) throw new Error("ER transaction signature is required");
  return {
    schemaVersion: TRANSPORT_SCHEMA,
    layer: "ephemeral-rollup",
    settlement: "not-yet-proven-on-solana",
    operation,
    signature,
    slot,
    rpcEndpoint,
    explorerUrl: erExplorerUrl(signature, rpcEndpoint),
    programId: asPublicKey(programId, "programId").toBase58(),
    worksitePda: asPublicKey(worksitePda, "worksitePda").toBase58(),
    sequence: state?.seq ?? null,
    stateRoot: state?.stateRoot ? Buffer.from(state.stateRoot).toString("hex") : null,
    headEventHash: state?.headEventHash ? Buffer.from(state.headEventHash).toString("hex") : null,
  };
}

export function makeSolanaCheckpointReceipt({
  signature,
  slot = null,
  readBackSlot = null,
  confirmationStatus = null,
  programId,
  worksitePda,
  expectedStateRoot,
  expectedHeadEventHash,
  state,
  cluster = DEFAULT_ENDPOINTS.cluster,
  sourceErSignature = null,
}) {
  if (!signature) throw new Error("Solana checkpoint signature is required");
  if (!["confirmed", "finalized"].includes(confirmationStatus)) {
    throw new Error("Solana checkpoint signature must be confirmed or finalized");
  }
  if (!Number.isInteger(slot) || !Number.isInteger(readBackSlot) || readBackSlot < slot) {
    throw new Error("Solana checkpoint read-back must occur at or after the confirmed transaction slot");
  }
  if (!state) throw new Error("read-back Worksite state is required");
  const expected = asBytes32(expectedStateRoot, "expectedStateRoot");
  const expectedHead = asBytes32(expectedHeadEventHash, "expectedHeadEventHash");
  if (!sameBytes(expected, state.stateRoot)) {
    throw new Error("Solana checkpoint state root does not equal the expected ER root");
  }
  if (!sameBytes(expectedHead, state.headEventHash)) {
    throw new Error("Solana checkpoint event-chain head does not equal the expected ER head");
  }
  if (!state.stateRootMatches) {
    throw new Error("Solana checkpoint stores a state root that does not match its decoded state");
  }
  const invariants = {
    seasonActive: state.seasonStatus === 1,
    settlementNotStarted: state.settlementStatus === 0,
    claimUnavailable: state.claimAvailable === false,
  };
  if (!Object.values(invariants).every(Boolean)) {
    throw new Error("Solana checkpoint violates the active-season/no-settlement/no-claim invariants");
  }
  return {
    schemaVersion: TRANSPORT_SCHEMA,
    layer: "solana",
    settlement: "checkpoint-read-back-verified",
    signature,
    sourceErSignature,
    slot,
    readBackSlot,
    confirmationStatus,
    cluster,
    explorerUrl: solanaExplorerUrl(signature, cluster),
    programId: asPublicKey(programId, "programId").toBase58(),
    worksitePda: asPublicKey(worksitePda, "worksitePda").toBase58(),
    sequence: state.seq,
    stateRoot: Buffer.from(state.stateRoot).toString("hex"),
    headEventHash: Buffer.from(state.headEventHash).toString("hex"),
    invariants,
  };
}

function satisfiesCommitment(status, commitment) {
  if (!status) return false;
  const level = status.confirmationStatus;
  if (commitment === "processed") return true;
  if (commitment === "finalized") return level === "finalized";
  return level === "confirmed" || level === "finalized" || status.confirmations === null;
}

async function pollConfirmedSignature({
  connection,
  signature,
  commitment,
  intervalMs,
  timeoutMs,
  signal,
}) {
  if (!connection || typeof connection.getSignatureStatuses !== "function") {
    throw new Error("baseConnection.getSignatureStatuses is required to verify the checkpoint signature");
  }
  const startedAt = Date.now();
  while (Date.now() - startedAt <= timeoutMs) {
    if (signal?.aborted) throw signal.reason || new Error("Operation aborted");
    const result = await connection.getSignatureStatuses(
      [signature],
      { searchTransactionHistory: true },
    );
    const status = result?.value?.[0] || null;
    if (status?.err) {
      throw new Error(`Solana checkpoint transaction failed: ${JSON.stringify(status.err)}`);
    }
    if (satisfiesCommitment(status, commitment)) return status;
    await sleep(intervalMs, signal);
  }
  throw new Error(`Timed out waiting for Solana checkpoint signature ${signature}`);
}

/**
 * Resolve the official MagicBlock ER->Solana commitment signature, then refuse
 * to issue a checkpoint receipt until the base-layer PDA reads back the same
 * state root. An ER signature by itself is never labeled as Solana settlement.
 */
export async function resolveSolanaCheckpoint({
  erSignature,
  ephemeralConnection,
  baseConnection,
  programId,
  worksitePda,
  expectedStateRoot,
  expectedHeadEventHash,
  commitment = "confirmed",
  intervalMs = 250,
  timeoutMs = 30_000,
  signal,
}) {
  if (!erSignature) throw new Error("erSignature is required");
  if (!ephemeralConnection) throw new Error("ephemeralConnection is required");
  const expected = asBytes32(expectedStateRoot, "expectedStateRoot");
  const expectedHead = asBytes32(expectedHeadEventHash, "expectedHeadEventHash");
  const startedAt = Date.now();
  const signature = await GetCommitmentSignature(erSignature, ephemeralConnection);
  const signatureStatus = await pollConfirmedSignature({
    connection: baseConnection,
    signature,
    commitment,
    intervalMs,
    timeoutMs,
    signal,
  });
  const remainingMs = Math.max(1, timeoutMs - (Date.now() - startedAt));
  const readBack = await pollWorksiteState({
    connection: baseConnection,
    worksitePda,
    commitment,
    intervalMs,
    timeoutMs: remainingMs,
    signal,
    predicate: (state, value) => (
      sameBytes(state.stateRoot, expected)
      && sameBytes(state.headEventHash, expectedHead)
      && state.stateRootMatches
      && (signatureStatus.slot == null
        || (Number.isInteger(value.slot) && value.slot >= signatureStatus.slot))
    ),
  });
  return makeSolanaCheckpointReceipt({
    signature,
    slot: signatureStatus.slot ?? null,
    readBackSlot: readBack.slot,
    confirmationStatus: signatureStatus.confirmationStatus || commitment,
    programId,
    worksitePda,
    expectedStateRoot: expected,
    expectedHeadEventHash: expectedHead,
    state: readBack.state,
    sourceErSignature: erSignature,
  });
}
