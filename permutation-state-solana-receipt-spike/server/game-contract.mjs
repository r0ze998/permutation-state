import { createHash } from "node:crypto";
import { ACTION, EVENT } from "../client/adapter.mjs";

export const LOCAL_PROOF_SCHEMA = "permutation-state.local-proof.v1";
export const LOCAL_PROOF_BUILD = "handoff-proof-2026-09-21";
export const NETWORK_SESSION_SCHEMA = "permutation-state.magicblock-session.v1";

const ACTION_BY_CHOICE = Object.freeze({
  oath: ACTION.OATH,
  charter: ACTION.CHARTER,
  service: ACTION.SERVICE,
  millrace: ACTION.MILLRACE,
  cut: ACTION.CUT,
  reconcile: ACTION.RECONCILE,
  "M-032": ACTION.M_032,
  "M-033": ACTION.M_033,
  "E-019": ACTION.E_019,
  "E-020": ACTION.E_020,
  "E-021": ACTION.E_021,
  "E-022": ACTION.E_022,
  "S-044": ACTION.S_044,
  "S-045": ACTION.S_045,
  "S-046": ACTION.S_046,
  "S-047": ACTION.S_047,
  "W-014": ACTION.W_014,
});

const COMMAND_SHAPE = Object.freeze({
  MARA_CHOICE: { actorRole: "mara", eventKind: EVENT.MARA_CHOICE, payloadKey: "choice" },
  IVO_CHOICE: { actorRole: "ivo", eventKind: EVENT.IVO_CHOICE, payloadKey: "choice" },
  MANDATE_ACCEPTED: {
    actorRole: "successor",
    eventKind: EVENT.MANDATE_ACCEPTED,
    payloadKey: "mandateId",
  },
});

export function sanitizeSession(value) {
  const cleaned = String(value || "")
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return (cleaned || "aster-demo").slice(0, 40);
}

export function sha256Bytes(...parts) {
  const digest = createHash("sha256");
  for (const part of parts) digest.update(Buffer.from(String(part)));
  return digest.digest();
}

export function sessionCommitments(sessionIdInput) {
  const sessionId = sanitizeSession(sessionIdInput);
  return {
    sessionId,
    seasonId: sha256Bytes("PERMSTATE/SEASON/V1", "season-zero"),
    worksiteId: sha256Bytes("PERMSTATE/WORKSITE/V1", "east-sluice-031", sessionId),
    rulesetHash: sha256Bytes("PERMSTATE/RULESET/V1", "water-debt-ruleset-v0.3"),
  };
}

export function proofSeed(sessionIdInput) {
  const sessionId = sanitizeSession(sessionIdInput);
  return {
    scenarioSeed: `ASTER:${sessionId}:EAST-SLUICE-031`,
    worldId: "aster",
    seasonId: "season-zero",
    epoch: 3,
    epochCount: 4,
    worksiteId: "east-sluice-031",
    publishedRuleset: "water-debt-ruleset-v0.3",
  };
}

export function newProofStore(sessionIdInput, now = new Date().toISOString()) {
  const sessionId = sanitizeSession(sessionIdInput);
  return {
    schemaVersion: LOCAL_PROOF_SCHEMA,
    buildId: LOCAL_PROOF_BUILD,
    sessionId,
    localProofDisclosure: "MAGICBLOCK LOCAL CLUSTER · DISPOSABLE DEMO SIGNERS · NO REAL FUNDS",
    seed: proofSeed(sessionId),
    createdAt: now,
    events: [],
  };
}

export function eventToWire(event) {
  if (!event || typeof event !== "object") throw new Error("event is required");
  const shape = COMMAND_SHAPE[event.type];
  if (!shape) throw new Error(`unsupported event type: ${event.type || "(missing)"}`);
  if (event.actorRole !== shape.actorRole) {
    throw new Error(`${event.type} must use actorRole ${shape.actorRole}`);
  }
  if (!Number.isInteger(event.seq) || event.seq < 0) throw new Error("event.seq must be non-negative");
  if (typeof event.eventHash !== "string" || !/^[0-9a-f]{64}$/.test(event.eventHash)) {
    throw new Error("event.eventHash must be 32-byte lowercase hex");
  }
  const value = event.payload?.[shape.payloadKey];
  const action = ACTION_BY_CHOICE[value];
  if (!action) throw new Error(`unsupported ${shape.payloadKey}: ${value || "(missing)"}`);
  return {
    actorRole: shape.actorRole,
    eventKind: shape.eventKind,
    action,
    clientEventHash: Buffer.from(event.eventHash, "hex"),
  };
}

function publicKey(value) {
  return value?.toBase58 ? value.toBase58() : String(value);
}

function bytesHex(value) {
  return value ? Buffer.from(value).toString("hex") : null;
}

export function stateToJson(state) {
  if (!state) return null;
  return {
    version: state.version,
    bump: state.bump,
    authority: publicKey(state.authority),
    envoy: publicKey(state.envoy),
    maker: publicKey(state.maker),
    successor: publicKey(state.successor),
    seasonId: bytesHex(state.seasonId),
    worksiteId: bytesHex(state.worksiteId),
    rulesetHash: bytesHex(state.rulesetHash),
    stateRoot: bytesHex(state.stateRoot),
    computedStateRoot: bytesHex(state.computedStateRoot),
    stateRootMatches: state.stateRootMatches,
    headEventHash: bytesHex(state.headEventHash),
    seq: state.seq,
    stage: state.stage,
    seasonStatus: state.seasonStatus,
    settlementStatus: state.settlementStatus,
    claimAvailable: state.claimAvailable,
    branch: state.branch,
    resolution: state.resolution,
    water: state.water,
    food: state.food,
    cohesion: state.cohesion,
    timber: state.timber,
    prosperity: state.prosperity,
    foodDebt: state.foodDebt,
    talaTrust: state.talaTrust,
    serviceStair: state.serviceStair,
    riverkeepers: state.riverkeepers,
    memoryReceipt: state.memoryReceipt,
    worksiteStatus: state.worksiteStatus,
    mandateIds: [...state.mandateIds],
    mandateStatus: [...state.mandateStatus],
    acceptedMandate: state.acceptedMandate,
  };
}

export function sessionRecord({ sessionId, descriptor, store, network, now = new Date().toISOString() }) {
  return {
    schemaVersion: NETWORK_SESSION_SCHEMA,
    sessionId: sanitizeSession(sessionId),
    descriptor,
    store,
    network,
    updatedAt: now,
  };
}
