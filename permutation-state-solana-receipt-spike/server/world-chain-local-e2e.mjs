import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { GetCommitmentSignature } from "@magicblock-labs/ephemeral-rollups-sdk";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import {
  WORLD_LOCATIONS,
  WORLD_RESOURCE,
  WORLD_SLUICE,
  initialWorldChainState,
  pollWorldChainState,
  sendCommitWorld,
  sendDelegateWorld,
  sendDepositWorldInventory,
  sendGatherWorldResource,
  sendInitializeWorld,
  sendJoinWorld,
  sendMoveWorldActor,
  sendRepairEastSluice,
  worldGuardFromState,
} from "../client/world-magicblock-transport.mjs";
import {
  createBaseLayerConnection,
  createEphemeralConnection,
  createMagicRouterConnection,
} from "../client/magicblock-transport.mjs";

const MODULE_DIR = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_DIR = path.resolve(MODULE_DIR, "..");
const OUTPUTS_DIR = path.resolve(PROJECT_DIR, "..");
const configPath = path.resolve(process.env.PERMSTATE_NETWORK_CONFIG || path.join(PROJECT_DIR, "deployments/local.magicblock.json"));
const keyPath = path.resolve(process.env.PERMSTATE_WORLD_E2E_KEY || path.join(OUTPUTS_DIR, "../work/devnet/mara-keypair.json"));
const config = JSON.parse(await readFile(configPath, "utf8"));

if (config.cluster !== "localnet") {
  throw new Error("The World end-to-end test is locked to localnet");
}

const secret = JSON.parse(await readFile(keyPath, "utf8"));
if (!Array.isArray(secret) || secret.length !== 64) throw new Error("The local test keypair is invalid");
const authority = Keypair.fromSecretKey(Uint8Array.from(secret));
const programId = new PublicKey(config.programId);
const base = createBaseLayerConnection({ httpEndpoint: config.baseHttp, wsEndpoint: config.baseWs });
const router = createMagicRouterConnection({ httpEndpoint: config.routerHttp, wsEndpoint: config.routerWs });
const ephemeral = createEphemeralConnection({ httpEndpoint: config.erHttp, wsEndpoint: config.erWs });

function hash32(...parts) {
  const hash = createHash("sha256");
  parts.forEach((part) => hash.update(String(part)));
  return hash.digest();
}

async function ensureFunding() {
  const minimum = 8 * LAMPORTS_PER_SOL;
  const balance = await base.getBalance(authority.publicKey, "confirmed");
  if (balance < minimum) await base.requestAirdrop(authority.publicKey, minimum - balance);
  const startedAt = Date.now();
  while (await base.getBalance(authority.publicKey, "confirmed") < minimum) {
    if (Date.now() - startedAt > 15_000) throw new Error("Local airdrop did not confirm");
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

async function waitForSignature(connection, signature, timeoutMs = 30_000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt <= timeoutMs) {
    const result = await connection.getSignatureStatuses([signature], { searchTransactionHistory: true });
    const status = result?.value?.[0] || null;
    if (status?.err) throw new Error(`Checkpoint failed: ${JSON.stringify(status.err)}`);
    if (status && (status.confirmationStatus === "confirmed" || status.confirmationStatus === "finalized" || status.confirmations === null)) {
      return status;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for checkpoint ${signature}`);
}

await Promise.all([base.getGenesisHash(), ephemeral.getGenesisHash()]);
await ensureFunding();

const runId = `${Date.now().toString(36)}-${process.pid}`;
const worldId = hash32("PERMSTATE/WORLD/E2E", runId);
const rulesetHash = hash32("PERMSTATE/WORLD/RULESET", "aster-alpha-v1");
const { state: genesis, worldPda } = initialWorldChainState({
  programId,
  authority: authority.publicKey,
  worldId,
  rulesetHash,
});

const initialize = await sendInitializeWorld({
  connection: base,
  programId,
  authority: authority.publicKey,
  worldPda,
  state: genesis,
  signers: [authority],
});
const baseGenesis = await pollWorldChainState({
  connection: base,
  worldPda,
  predicate: (state) => state.seq === 0 && state.stateRootMatches,
});
assert.ok(baseGenesis.owner.equals(programId));

const delegate = await sendDelegateWorld({
  connection: base,
  programId,
  authority: authority.publicKey,
  worldPda,
  validator: config.validatorIdentity,
  signers: [authority],
});
let current = await pollWorldChainState({
  connection: router,
  worldPda,
  predicate: (state) => state.seq === 0 && state.stateRootMatches,
  timeoutMs: 30_000,
});

let eventSerial = 0;
async function apply(label, send, params = {}) {
  eventSerial += 1;
  const before = current.state;
  const submission = await send({
    connection: router,
    programId,
    actor: authority.publicKey,
    worldPda,
    guard: worldGuardFromState(before, hash32("PERMSTATE/WORLD/E2E/EVENT", runId, eventSerial, label)),
    signers: [authority],
    ...params,
  });
  current = await pollWorldChainState({
    connection: router,
    worldPda,
    predicate: (state) => state.seq === before.seq + 1 && state.stateRootMatches,
  });
  return submission;
}

await apply("join", sendJoinWorld);
await apply("move-timber", sendMoveWorldActor, WORLD_LOCATIONS.timber);
await apply("gather-timber-1", sendGatherWorldResource, { resourceKind: WORLD_RESOURCE.TIMBER });
await apply("gather-timber-2", sendGatherWorldResource, { resourceKind: WORLD_RESOURCE.TIMBER });
await apply("move-warehouse-timber", sendMoveWorldActor, WORLD_LOCATIONS.warehouse);
await apply("deposit-timber", sendDepositWorldInventory);
await apply("move-stone", sendMoveWorldActor, WORLD_LOCATIONS.stone);
await apply("gather-stone", sendGatherWorldResource, { resourceKind: WORLD_RESOURCE.STONE });
await apply("move-warehouse-stone", sendMoveWorldActor, WORLD_LOCATIONS.warehouse);
await apply("deposit-stone", sendDepositWorldInventory);

assert.equal(current.state.warehouseTimber, 8);
assert.equal(current.state.warehouseStone, 4);
await apply("move-sluice", sendMoveWorldActor, WORLD_LOCATIONS.eastSluice);
for (let step = 1; step <= WORLD_SLUICE.repairSteps; step += 1) {
  await apply(`repair-${step}`, sendRepairEastSluice);
  assert.equal(current.state.repairProgress, step);
}

assert.equal(current.state.seq, 15);
assert.equal(current.state.sluiceCompleted, true);
assert.equal(current.state.waterRate, 480);
assert.equal(current.state.warehouseTimber, 0);
assert.equal(current.state.warehouseStone, 0);
assert.equal(current.state.stateRootMatches, true);

const expectedRoot = Buffer.from(current.state.stateRoot);
const expectedHead = Buffer.from(current.state.headEventHash);
const commit = await sendCommitWorld({
  connection: router,
  programId,
  payer: authority.publicKey,
  worldPda,
  signers: [authority],
});
const checkpointSignature = await GetCommitmentSignature(commit.signature, ephemeral);
const checkpointStatus = await waitForSignature(base, checkpointSignature);
const checkpoint = await pollWorldChainState({
  connection: base,
  worldPda,
  predicate: (state) => (
    state.seq === current.state.seq
    && state.stateRootMatches
    && Buffer.from(state.stateRoot).equals(expectedRoot)
    && Buffer.from(state.headEventHash).equals(expectedHead)
  ),
  timeoutMs: 30_000,
});

console.log(JSON.stringify({
  ok: true,
  runId,
  programId: programId.toBase58(),
  worldPda: worldPda.toBase58(),
  authority: authority.publicKey.toBase58(),
  initializeSignature: initialize.signature,
  delegateSignature: delegate.signature,
  erCommitSignature: commit.signature,
  checkpointSignature,
  checkpointSlot: checkpointStatus.slot ?? checkpoint.slot,
  sequence: checkpoint.state.seq,
  stateRoot: checkpoint.state.stateRoot.toString("hex"),
  repairProgress: checkpoint.state.repairProgress,
  sluiceCompleted: checkpoint.state.sluiceCompleted,
  waterRate: checkpoint.state.waterRate,
}, null, 2));
