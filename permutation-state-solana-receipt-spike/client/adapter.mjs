import { sha256 } from "@noble/hashes/sha256";
import { Buffer } from "buffer";
import {
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";

export const PROGRAM_SCHEMA = "permutation-state.solana-receipt.v1";
export const ACCOUNT_SPACE = 384;
export const DOMAINS = Object.freeze({
  state: Buffer.from("PERMSTATE/WORKSITE_STATE/V1"),
  event: Buffer.from("PERMSTATE/WORKSITE_EVENT/V1"),
  genesis: Buffer.from("PERMSTATE/WORKSITE_GENESIS/V1"),
  seasonState: Buffer.from("PERMSTATE/SEASON_STATE/V1"),
  seasonGenesis: Buffer.from("PERMSTATE/SEASON_GENESIS/V1"),
});
export const EVENT = Object.freeze({ MARA_CHOICE: 0, IVO_CHOICE: 1, MANDATE_ACCEPTED: 2 });
export const ACTION = Object.freeze({
  OATH: 1,
  CHARTER: 2,
  SERVICE: 11,
  MILLRACE: 12,
  CUT: 13,
  RECONCILE: 14,
  M_032: 1032,
  M_033: 1033,
  E_019: 2019,
  E_020: 2020,
  E_021: 2021,
  E_022: 2022,
  S_044: 3044,
  S_045: 3045,
  S_046: 3046,
  S_047: 3047,
  W_014: 4014,
});

function hashv(...buffers) {
  const hash = sha256.create();
  buffers.forEach((buffer) => hash.update(buffer));
  return Buffer.from(hash.digest());
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

function bool(value) {
  return u8(value ? 1 : 0);
}

function bytes32(value, label = "bytes32") {
  const out = Buffer.from(value);
  if (out.length !== 32) throw new Error(`${label} must be exactly 32 bytes`);
  return out;
}

function pubkeyBytes(value) {
  return new PublicKey(value).toBuffer();
}

function clone(state) {
  return {
    ...state,
    authority: new PublicKey(state.authority),
    envoy: new PublicKey(state.envoy),
    maker: new PublicKey(state.maker),
    successor: new PublicKey(state.successor),
    seasonPurse: new PublicKey(state.seasonPurse),
    seasonId: Buffer.from(state.seasonId),
    worksiteId: Buffer.from(state.worksiteId),
    rulesetHash: Buffer.from(state.rulesetHash),
    stateRoot: Buffer.from(state.stateRoot),
    headEventHash: Buffer.from(state.headEventHash),
    mandateIds: [...state.mandateIds],
    mandateStatus: [...state.mandateStatus],
  };
}
export function deriveWorksitePda(programId, seasonId, worksiteId) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("worksite"), bytes32(seasonId, "seasonId"), bytes32(worksiteId, "worksiteId")],
    new PublicKey(programId),
  );
}

export function deriveSeasonPursePda(programId, seasonId) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("season"), bytes32(seasonId, "seasonId")],
    new PublicKey(programId),
  );
}

export function initialWorksiteState({
  programId,
  authority,
  envoy,
  maker,
  successor,
  seasonId,
  worksiteId,
  rulesetHash,
  seasonPurse = null,
}) {
  const [worksitePda, bump] = deriveWorksitePda(programId, seasonId, worksiteId);
  const [derivedSeasonPurse] = deriveSeasonPursePda(programId, seasonId);
  const state = {
    version: 1,
    bump,
    authority: new PublicKey(authority),
    envoy: new PublicKey(envoy),
    maker: new PublicKey(maker),
    successor: new PublicKey(successor),
    seasonId: bytes32(seasonId, "seasonId"),
    worksiteId: bytes32(worksiteId, "worksiteId"),
    rulesetHash: bytes32(rulesetHash, "rulesetHash"),
    stateRoot: Buffer.alloc(32),
    headEventHash: Buffer.alloc(32),
    seq: 0,
    stage: 0,
    seasonStatus: 1,
    settlementStatus: 0,
    claimAvailable: false,
    branch: 0,
    resolution: 0,
    water: 28,
    food: 46,
    cohesion: 54,
    timber: 18,
    prosperity: 38,
    foodDebt: 0,
    talaTrust: 0,
    serviceStair: 0,
    riverkeepers: 0,
    memoryReceipt: 0,
    worksiteStatus: 0,
    mandateIds: [0, 0, 0],
    mandateStatus: [0, 0, 0],
    acceptedMandate: 0,
    seasonPurse: new PublicKey(seasonPurse || derivedSeasonPurse),
  };
  state.stateRoot = computeStateRoot(state);
  state.headEventHash = hashv(
    DOMAINS.genesis,
    new PublicKey(programId).toBuffer(),
    worksitePda.toBuffer(),
    state.rulesetHash,
    state.stateRoot,
  );
  return { state, worksitePda };
}

export function encodeStateRootView(state) {
  return Buffer.concat([
    pubkeyBytes(state.authority),
    pubkeyBytes(state.envoy),
    pubkeyBytes(state.maker),
    pubkeyBytes(state.successor),
    bytes32(state.seasonId, "seasonId"),
    bytes32(state.worksiteId, "worksiteId"),
    bytes32(state.rulesetHash, "rulesetHash"),
    u64(state.seq),
    u8(state.stage),
    u8(state.seasonStatus),
    u8(state.settlementStatus),
    bool(state.claimAvailable),
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
    pubkeyBytes(state.seasonPurse),
  ]);
}

export function initialSeasonState({
  programId,
  authority,
  seasonId,
  rulesetHash,
  payoutRulesHash,
}) {
  const [seasonPurse, bump] = deriveSeasonPursePda(programId, seasonId);
  const state = {
    version: 1,
    bump,
    authority: new PublicKey(authority),
    seasonId: bytes32(seasonId, "seasonId"),
    rulesetHash: bytes32(rulesetHash, "rulesetHash"),
    payoutRulesHash: bytes32(payoutRulesHash, "payoutRulesHash"),
    outcomeHash: Buffer.alloc(32),
    chronicleRoot: Buffer.alloc(32),
    claimRoot: Buffer.alloc(32),
    stateRoot: Buffer.alloc(32),
    headEventHash: Buffer.alloc(32),
    status: 1,
    seq: 0,
    activeWorksites: 0,
    activeCitizens: 0,
    entryGrossUnits: 0,
    entryPurseUnits: 0,
    marketplaceGrossUnits: 0,
    marketplacePurseUnits: 0,
    sellerUnits: 0,
    opsUnits: 0,
    purseTotal: 0,
    claimableUnits: 0,
    claimedUnits: 0,
    claimCount: 0,
  };
  state.stateRoot = computeSeasonStateRoot(state);
  state.headEventHash = hashv(
    DOMAINS.seasonGenesis,
    new PublicKey(programId).toBuffer(),
    seasonPurse.toBuffer(),
    state.seasonId,
    state.rulesetHash,
    state.payoutRulesHash,
    state.stateRoot,
  );
  return { state, seasonPurse };
}

export function encodeSeasonStateRootView(state) {
  const u32 = (value) => {
    const out = Buffer.alloc(4);
    out.writeUInt32LE(value);
    return out;
  };
  return Buffer.concat([
    u8(state.version),
    u8(state.bump),
    pubkeyBytes(state.authority),
    bytes32(state.seasonId, "seasonId"),
    bytes32(state.rulesetHash, "rulesetHash"),
    bytes32(state.payoutRulesHash, "payoutRulesHash"),
    bytes32(state.outcomeHash, "outcomeHash"),
    bytes32(state.chronicleRoot, "chronicleRoot"),
    bytes32(state.claimRoot, "claimRoot"),
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
}

export function computeSeasonStateRoot(state) {
  return hashv(DOMAINS.seasonState, encodeSeasonStateRootView(state));
}

export function computeStateRoot(state) {
  return hashv(DOMAINS.state, encodeStateRootView(state));
}

function assertInvariant(state) {
  if (state.seasonStatus !== 1 || state.settlementStatus !== 0 || state.claimAvailable !== false) {
    throw new Error("season-active/no-settlement/no-claim invariant failed");
  }
}

export function previewEvent(current, { actor, eventKind, action, clientEventHash }) {
  const actorKey = new PublicKey(actor);
  const expectedActor = [current.envoy, current.maker, current.successor][current.stage];
  if (!expectedActor || !actorKey.equals(expectedActor)) throw new Error("unauthorized actor for stage");
  assertInvariant(current);

  const next = clone(current);
  if (next.stage === 0 && eventKind === EVENT.MARA_CHOICE) {
    if (action === ACTION.OATH) {
      Object.assign(next, { branch: 1, water: 36, cohesion: 60, foodDebt: 12, talaTrust: 20, serviceStair: 1, riverkeepers: 1, memoryReceipt: 1 });
    } else if (action === ACTION.CHARTER) {
      Object.assign(next, { branch: 2, water: 33, cohesion: 46, foodDebt: 0, talaTrust: -20, serviceStair: 2, riverkeepers: 2, memoryReceipt: 2 });
    } else throw new Error("illegal Mara action");
    next.stage = 1;
    next.worksiteStatus = 1;
  } else if (next.stage === 1 && eventKind === EVENT.IVO_CHOICE) {
    if (next.branch === 1 && action === ACTION.SERVICE) {
      Object.assign(next, { resolution: 1, water: next.water + 24, cohesion: next.cohesion + 4, timber: next.timber - 6, prosperity: next.prosperity + 5, talaTrust: 28, mandateIds: [ACTION.M_032, ACTION.E_019, ACTION.S_044] });
    } else if (next.branch === 1 && action === ACTION.MILLRACE) {
      Object.assign(next, { resolution: 2, water: next.water + 18, cohesion: next.cohesion - 4, timber: next.timber - 3, prosperity: next.prosperity + 3, talaTrust: 8, mandateIds: [ACTION.E_020, ACTION.M_032, ACTION.S_045] });
    } else if (next.branch === 2 && action === ACTION.CUT) {
      Object.assign(next, { resolution: 3, water: next.water + 22, cohesion: next.cohesion - 5, timber: next.timber - 10, prosperity: next.prosperity + 1, talaTrust: -35, mandateIds: [ACTION.E_021, ACTION.W_014, ACTION.S_046] });
    } else if (next.branch === 2 && action === ACTION.RECONCILE) {
      Object.assign(next, { resolution: 4, water: next.water + 18, cohesion: next.cohesion + 8, timber: next.timber - 6, prosperity: next.prosperity + 4, talaTrust: 5, foodDebt: 8, serviceStair: 1, riverkeepers: 3, mandateIds: [ACTION.M_033, ACTION.E_022, ACTION.S_047] });
    } else throw new Error("Ivo action is not unlocked by the inherited branch");
    next.stage = 2;
    next.worksiteStatus = action === ACTION.CUT ? 3 : 2;
    next.mandateStatus = [0, 0, 0];
  } else if (next.stage === 2 && eventKind === EVENT.MANDATE_ACCEPTED) {
    const index = next.mandateIds.indexOf(action);
    if (index < 0 || next.mandateStatus[index] !== 0 || next.acceptedMandate !== 0) throw new Error("mandate is not queued");
    next.mandateStatus[index] = 1;
    next.acceptedMandate = action;
    next.stage = 3;
  } else throw new Error("event is illegal at current stage");

  next.seq += 1;
  assertInvariant(next);
  const newStateRoot = computeStateRoot(next);
  const clientHash = bytes32(clientEventHash, "clientEventHash");
  const eventHash = hashv(
    DOMAINS.event,
    current.rulesetHash,
    current.headEventHash,
    current.stateRoot,
    newStateRoot,
    actorKey.toBuffer(),
    u64(current.seq),
    u8(eventKind),
    u16(action),
    clientHash,
  );
  next.stateRoot = newStateRoot;
  next.headEventHash = eventHash;
  return {
    next,
    args: {
      eventKind,
      action,
      expectedSeq: current.seq,
      rulesetHash: current.rulesetHash,
      priorStateRoot: current.stateRoot,
      expectedNewStateRoot: newStateRoot,
      expectedPrevEventHash: current.headEventHash,
      clientEventHash: clientHash,
    },
  };
}

export function encodeInitializeInstruction({ state }) {
  return Buffer.concat([
    u8(0),
    state.seasonId,
    state.worksiteId,
    state.rulesetHash,
    state.envoy.toBuffer(),
    state.maker.toBuffer(),
    state.successor.toBuffer(),
    state.stateRoot,
  ]);
}

export function encodeApplyInstruction(args) {
  return Buffer.concat([
    u8(1),
    u8(args.eventKind),
    u16(args.action),
    u64(args.expectedSeq),
    bytes32(args.rulesetHash, "rulesetHash"),
    bytes32(args.priorStateRoot, "priorStateRoot"),
    bytes32(args.expectedNewStateRoot, "expectedNewStateRoot"),
    bytes32(args.expectedPrevEventHash, "expectedPrevEventHash"),
    bytes32(args.clientEventHash, "clientEventHash"),
  ]);
}

export function buildInitializeInstruction({ programId, authority, worksitePda, state }) {
  return new TransactionInstruction({
    programId: new PublicKey(programId),
    keys: [
      { pubkey: new PublicKey(authority), isSigner: true, isWritable: true },
      { pubkey: new PublicKey(worksitePda), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: new PublicKey(state.seasonPurse), isSigner: false, isWritable: true },
    ],
    data: encodeInitializeInstruction({ state }),
  });
}

export function buildApplyInstruction({ programId, actor, worksitePda, args }) {
  return new TransactionInstruction({
    programId: new PublicKey(programId),
    keys: [
      { pubkey: new PublicKey(actor), isSigner: true, isWritable: false },
      { pubkey: new PublicKey(worksitePda), isSigner: false, isWritable: true },
    ],
    data: encodeApplyInstruction(args),
  });
}

export function makeExplorerReceipt({
  signature,
  slot,
  programId,
  worksitePda,
  actor,
  args,
  eventHash,
}) {
  if (!signature) throw new Error("confirmed transaction signature is required");
  return {
    schemaVersion: PROGRAM_SCHEMA,
    network: "devnet",
    signature,
    slot,
    explorerUrl: `https://explorer.solana.com/tx/${signature}?cluster=devnet`,
    programId: new PublicKey(programId).toBase58(),
    worksitePda: new PublicKey(worksitePda).toBase58(),
    actor: new PublicKey(actor).toBase58(),
    sequence: args.expectedSeq,
    eventKind: args.eventKind,
    action: args.action,
    rulesetHash: Buffer.from(args.rulesetHash).toString("hex"),
    priorStateRoot: Buffer.from(args.priorStateRoot).toString("hex"),
    newStateRoot: Buffer.from(args.expectedNewStateRoot).toString("hex"),
    previousEventHash: Buffer.from(args.expectedPrevEventHash).toString("hex"),
    eventHash: Buffer.from(eventHash).toString("hex"),
    clientEventHash: Buffer.from(args.clientEventHash).toString("hex"),
    invariants: {
      seasonActive: true,
      settlementNotStarted: true,
      claimUnavailable: true,
    },
  };
}
