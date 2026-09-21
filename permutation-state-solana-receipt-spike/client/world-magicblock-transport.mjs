import { sha256 } from "@noble/hashes/sha256";
import { Buffer } from "buffer";
import {
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  DELEGATION_PROGRAM_ID,
  MAGIC_CONTEXT_ID,
  MAGIC_PROGRAM_ID,
  delegateBufferPdaFromDelegatedAccountAndOwnerProgram,
  delegationMetadataPdaFromDelegatedAccount,
  delegationRecordPdaFromDelegatedAccount,
} from "@magicblock-labs/ephemeral-rollups-sdk";
import {
  getClosestValidator,
  submitWorksiteTransaction,
} from "./magicblock-transport.mjs";

export const WORLD_TRANSPORT_SCHEMA = "permutation-state.magicblock-world-transport.v1";
export const WORLD_ACCOUNT_SPACE = 512;
export const WORLD_STATE_BYTES = 503;
export const MAX_WORLD_CITIZENS = 8;

export const WORLD_INSTRUCTION = Object.freeze({
  INITIALIZE_WORLD: 9,
  JOIN_WORLD: 10,
  MOVE_WORLD_ACTOR: 11,
  GATHER_WORLD_RESOURCE: 12,
  DEPOSIT_WORLD_INVENTORY: 13,
  REPAIR_EAST_SLUICE: 14,
  DELEGATE_WORLD: 15,
  COMMIT_WORLD: 16,
  COMMIT_AND_UNDELEGATE_WORLD: 17,
});

export const WORLD_RESOURCE = Object.freeze({ TIMBER: 1, STONE: 2 });
export const WORLD_LIMITS = Object.freeze({ maxX: 1_000, maxY: 600, inventory: 12 });
export const WORLD_LOCATIONS = Object.freeze({
  spawn: Object.freeze({ x: 500, y: 300 }),
  timber: Object.freeze({ x: 180, y: 200 }),
  stone: Object.freeze({ x: 790, y: 170 }),
  warehouse: Object.freeze({ x: 460, y: 310 }),
  eastSluice: Object.freeze({ x: 720, y: 300 }),
});
export const WORLD_SLUICE = Object.freeze({ repairSteps: 4, timberPerStep: 2, stonePerStep: 1 });

const DOMAIN = Object.freeze({
  state: Buffer.from("PERMSTATE/WORLD_STATE/V1"),
  genesis: Buffer.from("PERMSTATE/WORLD_GENESIS/V1"),
});

function hashv(...parts) {
  const hash = sha256.create();
  parts.forEach((part) => hash.update(Buffer.from(part)));
  return Buffer.from(hash.digest());
}

function publicKey(value, label) {
  try {
    return value instanceof PublicKey ? value : new PublicKey(value);
  } catch {
    throw new Error(`${label} must be a valid public key`);
  }
}

function bytes32(value, label) {
  const encoded = typeof value === "string" && /^[0-9a-fA-F]{64}$/.test(value)
    ? Buffer.from(value, "hex")
    : Buffer.from(value);
  if (encoded.length !== 32) throw new Error(`${label} must be exactly 32 bytes`);
  return encoded;
}

function u8(value, label) {
  if (!Number.isInteger(value) || value < 0 || value > 0xff) {
    throw new Error(`${label} must be an unsigned 8-bit integer`);
  }
  return Buffer.from([value]);
}

function u16(value, label) {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new Error(`${label} must be an unsigned 16-bit integer`);
  }
  const encoded = Buffer.alloc(2);
  encoded.writeUInt16LE(value);
  return encoded;
}

function u64(value, label) {
  let parsed;
  try {
    parsed = BigInt(value);
  } catch {
    throw new Error(`${label} must be an unsigned 64-bit integer`);
  }
  if (parsed < 0n || parsed > 0xffff_ffff_ffff_ffffn) {
    throw new Error(`${label} must be an unsigned 64-bit integer`);
  }
  const encoded = Buffer.alloc(8);
  encoded.writeBigUInt64LE(parsed);
  return encoded;
}

function bool(value) {
  return Buffer.from([value ? 1 : 0]);
}

function position(value, label) {
  if (!value || typeof value !== "object") throw new Error(`${label} is required`);
  return Buffer.concat([u16(value.x, `${label}.x`), u16(value.y, `${label}.y`)]);
}

function emptyCitizen() {
  return PublicKey.default;
}

export function deriveWorldPda(programId, worldId) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("world"), bytes32(worldId, "worldId")],
    publicKey(programId, "programId"),
  );
}

export function encodeWorldStateRootView(state) {
  if (!Array.isArray(state.citizens) || state.citizens.length !== MAX_WORLD_CITIZENS) {
    throw new Error(`citizens must contain ${MAX_WORLD_CITIZENS} entries`);
  }
  if (!Array.isArray(state.positions) || state.positions.length !== MAX_WORLD_CITIZENS) {
    throw new Error(`positions must contain ${MAX_WORLD_CITIZENS} entries`);
  }
  if (!Array.isArray(state.inventoryTimber) || state.inventoryTimber.length !== MAX_WORLD_CITIZENS) {
    throw new Error(`inventoryTimber must contain ${MAX_WORLD_CITIZENS} entries`);
  }
  if (!Array.isArray(state.inventoryStone) || state.inventoryStone.length !== MAX_WORLD_CITIZENS) {
    throw new Error(`inventoryStone must contain ${MAX_WORLD_CITIZENS} entries`);
  }
  return Buffer.concat([
    u8(state.version, "version"),
    u8(state.bump, "bump"),
    publicKey(state.authority, "authority").toBuffer(),
    bytes32(state.worldId, "worldId"),
    bytes32(state.rulesetHash, "rulesetHash"),
    u64(state.seq, "seq"),
    u8(state.citizenCount, "citizenCount"),
    ...state.citizens.map((citizen, index) => publicKey(citizen, `citizens[${index}]`).toBuffer()),
    ...state.positions.map((entry, index) => position(entry, `positions[${index}]`)),
    ...state.inventoryTimber.map((amount, index) => u16(amount, `inventoryTimber[${index}]`)),
    ...state.inventoryStone.map((amount, index) => u16(amount, `inventoryStone[${index}]`)),
    u16(state.warehouseTimber, "warehouseTimber"),
    u16(state.warehouseStone, "warehouseStone"),
    u16(state.timberRemaining, "timberRemaining"),
    u16(state.stoneRemaining, "stoneRemaining"),
    u8(state.repairProgress, "repairProgress"),
    bool(state.sluiceCompleted),
    u16(state.waterRate, "waterRate"),
  ]);
}

export function computeWorldStateRoot(state) {
  return hashv(DOMAIN.state, encodeWorldStateRootView(state));
}

export function initialWorldChainState({
  programId,
  authority,
  worldId,
  rulesetHash,
}) {
  const normalizedWorldId = bytes32(worldId, "worldId");
  const [worldPda, bump] = deriveWorldPda(programId, normalizedWorldId);
  const state = {
    version: 1,
    bump,
    authority: publicKey(authority, "authority"),
    worldId: normalizedWorldId,
    rulesetHash: bytes32(rulesetHash, "rulesetHash"),
    stateRoot: Buffer.alloc(32),
    headEventHash: Buffer.alloc(32),
    seq: 0,
    citizenCount: 0,
    citizens: Array.from({ length: MAX_WORLD_CITIZENS }, emptyCitizen),
    positions: Array.from({ length: MAX_WORLD_CITIZENS }, () => ({ x: 0, y: 0 })),
    inventoryTimber: Array(MAX_WORLD_CITIZENS).fill(0),
    inventoryStone: Array(MAX_WORLD_CITIZENS).fill(0),
    warehouseTimber: 0,
    warehouseStone: 0,
    timberRemaining: 18,
    stoneRemaining: 15,
    repairProgress: 0,
    sluiceCompleted: false,
    waterRate: 70,
  };
  state.stateRoot = computeWorldStateRoot(state);
  state.headEventHash = hashv(
    DOMAIN.genesis,
    publicKey(programId, "programId").toBuffer(),
    worldPda.toBuffer(),
    state.worldId,
    state.rulesetHash,
    state.stateRoot,
  );
  return { state, worldPda };
}

export function worldGuardFromState(state, eventId) {
  return {
    expectedSeq: state.seq,
    priorStateRoot: bytes32(state.stateRoot, "stateRoot"),
    expectedHeadEventHash: bytes32(state.headEventHash, "headEventHash"),
    eventId: bytes32(eventId, "eventId"),
  };
}

function encodeGuard(guard) {
  if (!guard) throw new Error("guard is required");
  return Buffer.concat([
    u64(guard.expectedSeq, "expectedSeq"),
    bytes32(guard.priorStateRoot, "priorStateRoot"),
    bytes32(guard.expectedHeadEventHash, "expectedHeadEventHash"),
    bytes32(guard.eventId, "eventId"),
  ]);
}

function worldActionInstruction({ programId, actor, worldPda, variant, payload = [] }) {
  return new TransactionInstruction({
    programId: publicKey(programId, "programId"),
    keys: [
      { pubkey: publicKey(actor, "actor"), isSigner: true, isWritable: false },
      { pubkey: publicKey(worldPda, "worldPda"), isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.from([variant]), ...payload]),
  });
}

export function buildInitializeWorldInstruction({ programId, authority, worldPda, state }) {
  return new TransactionInstruction({
    programId: publicKey(programId, "programId"),
    keys: [
      { pubkey: publicKey(authority, "authority"), isSigner: true, isWritable: true },
      { pubkey: publicKey(worldPda, "worldPda"), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([
      Buffer.from([WORLD_INSTRUCTION.INITIALIZE_WORLD]),
      bytes32(state.worldId, "worldId"),
      bytes32(state.rulesetHash, "rulesetHash"),
      bytes32(state.stateRoot, "expectedGenesisStateRoot"),
    ]),
  });
}

export function buildJoinWorldInstruction(params) {
  return worldActionInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.JOIN_WORLD,
    payload: [encodeGuard(params.guard)],
  });
}

export function buildMoveWorldActorInstruction(params) {
  if (params.x > WORLD_LIMITS.maxX || params.y > WORLD_LIMITS.maxY) {
    throw new Error("world coordinates are outside the published bounds");
  }
  return worldActionInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.MOVE_WORLD_ACTOR,
    payload: [u16(params.x, "x"), u16(params.y, "y"), encodeGuard(params.guard)],
  });
}

export function buildGatherWorldResourceInstruction(params) {
  if (![WORLD_RESOURCE.TIMBER, WORLD_RESOURCE.STONE].includes(params.resourceKind)) {
    throw new Error("resourceKind must be TIMBER or STONE");
  }
  return worldActionInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.GATHER_WORLD_RESOURCE,
    payload: [u8(params.resourceKind, "resourceKind"), encodeGuard(params.guard)],
  });
}

export function buildDepositWorldInventoryInstruction(params) {
  return worldActionInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.DEPOSIT_WORLD_INVENTORY,
    payload: [encodeGuard(params.guard)],
  });
}

export function buildRepairEastSluiceInstruction(params) {
  return worldActionInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.REPAIR_EAST_SLUICE,
    payload: [encodeGuard(params.guard)],
  });
}

export function deriveWorldDelegationAccounts({ programId, worldPda }) {
  const ownerProgram = publicKey(programId, "programId");
  const delegatedAccount = publicKey(worldPda, "worldPda");
  return {
    ownerProgram,
    delegatedAccount,
    delegationBuffer: delegateBufferPdaFromDelegatedAccountAndOwnerProgram(delegatedAccount, ownerProgram),
    delegationRecord: delegationRecordPdaFromDelegatedAccount(delegatedAccount),
    delegationMetadata: delegationMetadataPdaFromDelegatedAccount(delegatedAccount),
    delegationProgram: DELEGATION_PROGRAM_ID,
  };
}

export function buildDelegateWorldInstruction({ programId, authority, worldPda, validator }) {
  const accounts = deriveWorldDelegationAccounts({ programId, worldPda });
  const validatorKey = validator == null ? null : publicKey(validator, "validator");
  return new TransactionInstruction({
    programId: accounts.ownerProgram,
    keys: [
      { pubkey: publicKey(authority, "authority"), isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: accounts.delegatedAccount, isSigner: false, isWritable: true },
      { pubkey: accounts.ownerProgram, isSigner: false, isWritable: false },
      { pubkey: accounts.delegationBuffer, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationRecord, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationMetadata, isSigner: false, isWritable: true },
      { pubkey: accounts.delegationProgram, isSigner: false, isWritable: false },
      ...(validatorKey ? [{ pubkey: validatorKey, isSigner: false, isWritable: false }] : []),
    ],
    data: Buffer.from([WORLD_INSTRUCTION.DELEGATE_WORLD]),
  });
}

function buildWorldLifecycleInstruction({ programId, payer, worldPda, variant }) {
  return new TransactionInstruction({
    programId: publicKey(programId, "programId"),
    keys: [
      { pubkey: publicKey(payer, "payer"), isSigner: true, isWritable: true },
      { pubkey: publicKey(worldPda, "worldPda"), isSigner: false, isWritable: true },
      { pubkey: MAGIC_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: MAGIC_CONTEXT_ID, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([variant]),
  });
}

export function buildCommitWorldInstruction(params) {
  return buildWorldLifecycleInstruction({ ...params, variant: WORLD_INSTRUCTION.COMMIT_WORLD });
}

export function buildCommitAndUndelegateWorldInstruction(params) {
  return buildWorldLifecycleInstruction({
    ...params,
    variant: WORLD_INSTRUCTION.COMMIT_AND_UNDELEGATE_WORLD,
  });
}

function submit(params, instruction) {
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

export async function sendInitializeWorld(params) {
  const instruction = buildInitializeWorldInstruction(params);
  return { operation: "initialize-world", instruction, ...(await submit(params, instruction)) };
}

export async function sendJoinWorld(params) {
  const instruction = buildJoinWorldInstruction(params);
  return { operation: "join-world", instruction, ...(await submit(params, instruction)) };
}

export async function sendMoveWorldActor(params) {
  const instruction = buildMoveWorldActorInstruction(params);
  return { operation: "move-world-actor", instruction, ...(await submit(params, instruction)) };
}

export async function sendGatherWorldResource(params) {
  const instruction = buildGatherWorldResourceInstruction(params);
  return { operation: "gather-world-resource", instruction, ...(await submit(params, instruction)) };
}

export async function sendDepositWorldInventory(params) {
  const instruction = buildDepositWorldInventoryInstruction(params);
  return { operation: "deposit-world-inventory", instruction, ...(await submit(params, instruction)) };
}

export async function sendRepairEastSluice(params) {
  const instruction = buildRepairEastSluiceInstruction(params);
  return { operation: "repair-east-sluice", instruction, ...(await submit(params, instruction)) };
}

export async function sendDelegateWorld(params) {
  const validator = params.validator == null
    ? (await getClosestValidator(params.connection)).publicKey
    : publicKey(params.validator, "validator");
  const instruction = buildDelegateWorldInstruction({ ...params, validator });
  return {
    operation: "delegate-world",
    validator: validator.toBase58(),
    instruction,
    ...(await submit(params, instruction)),
  };
}

export async function sendCommitWorld(params) {
  const instruction = buildCommitWorldInstruction(params);
  return { operation: "commit-world", instruction, ...(await submit(params, instruction)) };
}

export async function sendCommitAndUndelegateWorld(params) {
  const instruction = buildCommitAndUndelegateWorldInstruction(params);
  return { operation: "commit-and-undelegate-world", instruction, ...(await submit(params, instruction)) };
}

class Cursor {
  constructor(data) {
    this.bytes = Buffer.from(data);
    this.offset = 0;
  }

  take(length, label) {
    if (this.offset + length > this.bytes.length) throw new Error(`World account ended while reading ${label}`);
    const value = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }

  u8(label) { return this.take(1, label)[0]; }
  u16(label) { return this.take(2, label).readUInt16LE(0); }
  u64(label) {
    const value = this.take(8, label).readBigUInt64LE(0);
    if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error(`${label} exceeds JavaScript's safe range`);
    return Number(value);
  }
  bool(label) {
    const value = this.u8(label);
    if (value !== 0 && value !== 1) throw new Error(`${label} is not a Borsh bool`);
    return value === 1;
  }
  pubkey(label) { return new PublicKey(this.take(32, label)); }
  bytes32(label) { return Buffer.from(this.take(32, label)); }
  position(label) { return { x: this.u16(`${label}.x`), y: this.u16(`${label}.y`) }; }
}

export function decodeWorldState(data) {
  const bytes = Buffer.from(data);
  if (bytes.length < WORLD_STATE_BYTES || bytes.length > WORLD_ACCOUNT_SPACE) {
    throw new Error(`World account data must be ${WORLD_STATE_BYTES}-${WORLD_ACCOUNT_SPACE} bytes; received ${bytes.length}`);
  }
  const cursor = new Cursor(bytes);
  const state = {
    version: cursor.u8("version"),
    bump: cursor.u8("bump"),
    authority: cursor.pubkey("authority"),
    worldId: cursor.bytes32("worldId"),
    rulesetHash: cursor.bytes32("rulesetHash"),
    stateRoot: cursor.bytes32("stateRoot"),
    headEventHash: cursor.bytes32("headEventHash"),
    seq: cursor.u64("seq"),
    citizenCount: cursor.u8("citizenCount"),
    citizens: Array.from({ length: MAX_WORLD_CITIZENS }, (_, index) => cursor.pubkey(`citizens[${index}]`)),
    positions: Array.from({ length: MAX_WORLD_CITIZENS }, (_, index) => cursor.position(`positions[${index}]`)),
    inventoryTimber: Array.from({ length: MAX_WORLD_CITIZENS }, (_, index) => cursor.u16(`inventoryTimber[${index}]`)),
    inventoryStone: Array.from({ length: MAX_WORLD_CITIZENS }, (_, index) => cursor.u16(`inventoryStone[${index}]`)),
    warehouseTimber: cursor.u16("warehouseTimber"),
    warehouseStone: cursor.u16("warehouseStone"),
    timberRemaining: cursor.u16("timberRemaining"),
    stoneRemaining: cursor.u16("stoneRemaining"),
    repairProgress: cursor.u8("repairProgress"),
    sluiceCompleted: cursor.bool("sluiceCompleted"),
    waterRate: cursor.u16("waterRate"),
  };
  if (state.version !== 1) throw new Error(`Unsupported World state version ${state.version}`);
  const computedStateRoot = computeWorldStateRoot(state);
  return {
    ...state,
    computedStateRoot,
    stateRootMatches: Buffer.from(state.stateRoot).equals(computedStateRoot),
    decodedBytes: cursor.offset,
  };
}

async function accountWithContext(connection, address, commitment) {
  if (typeof connection.getAccountInfoAndContext === "function") {
    return connection.getAccountInfoAndContext(address, commitment);
  }
  return { context: { slot: null }, value: await connection.getAccountInfo(address, commitment) };
}

export async function readWorldChainState(connection, worldPda, commitment = "confirmed") {
  const address = publicKey(worldPda, "worldPda");
  const result = await accountWithContext(connection, address, commitment);
  if (!result.value) throw new Error(`World PDA ${address.toBase58()} was not found`);
  return {
    address,
    slot: result.context?.slot ?? null,
    owner: result.value.owner,
    lamports: result.value.lamports,
    state: decodeWorldState(result.value.data),
  };
}

export async function pollWorldChainState({
  connection,
  worldPda,
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
    latest = await readWorldChainState(connection, worldPda, commitment);
    if (predicate(latest.state, latest)) return latest;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Timed out waiting for World PDA after ${timeoutMs}ms${latest ? ` (last seq ${latest.state.seq})` : ""}`);
}
