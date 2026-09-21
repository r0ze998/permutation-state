import { createReadStream, existsSync } from "node:fs";
import { mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import http from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import {
  initialSeasonState,
  initialWorksiteState,
  previewEvent,
} from "../client/adapter.mjs";
import {
  PURSE_SOURCE,
  createBaseLayerConnection,
  createEphemeralConnection,
  createMagicRouterConnection,
  getWorksiteDelegationStatus,
  makeErTransactionReceipt,
  pollSeasonState,
  pollWorksiteState,
  readSeasonState,
  readWorksiteState,
  resolveSolanaCheckpoint,
  sendApplyWorksiteEvent,
  sendCommitWorksite,
  sendCreditSeasonPurse,
  sendDelegateWorksite,
  sendInitializeSeason,
  sendInitializeWorksite,
} from "../client/magicblock-transport.mjs";
import {
  eventToWire,
  newProofStore,
  sanitizeSession,
  sha256Bytes,
  sessionCommitments,
  sessionRecord,
  seasonStateToJson,
  stateToJson,
} from "./game-contract.mjs";
import { createCivilizationService, routeCivilizationRequest } from "./civilization-service.mjs";
import { createWorldService, routeWorldRequest } from "./world-service.mjs";

const MODULE_DIR = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const ProofCore = require("../../permutation-state-prototype/proof/core.js");
const PROJECT_DIR = path.resolve(MODULE_DIR, "..");
const DEFAULT_WORK_DIR = path.resolve(PROJECT_DIR, "../../work/devnet");
const DEFAULT_STATIC_DIR = path.resolve(PROJECT_DIR, "../permutation-state-prototype");
const DEFAULT_CONFIG = path.resolve(PROJECT_DIR, "deployments/local.magicblock.json");
const JSON_LIMIT = 1_000_000;
const MOCK_USDC_MICRO_UNITS = 1_000_000;
const DEMO_ENTRY_GROSS_UNITS = 10 * MOCK_USDC_MICRO_UNITS;
const DEMO_MARKETPLACE_GROSS_UNITS = 100 * MOCK_USDC_MICRO_UNITS;
const locks = new Map();
const seasonLocks = new Map();
const PUBLIC_CLUSTER_GENESIS = Object.freeze({
  mainnet: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
  devnet: "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
  testnet: "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY",
});
const MAGICBLOCK_ER_GENESIS = "11111111111111111111111111111111";
const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "[::1]", "::1"]);

const CONTENT_TYPES = Object.freeze({
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
});

function envPath(name, fallback) {
  return path.resolve(process.env[name] || fallback);
}

function endpoint(value, label) {
  try {
    const parsed = new URL(value);
    if (parsed.username || parsed.password) throw new Error("credentials are not allowed");
    return parsed;
  } catch (error) {
    throw new Error(`${label} is not a safe URL: ${error.message}`);
  }
}

export function assertSafeNetworkConfiguration({ config, baseGenesisHash, erGenesisHash }) {
  if (!config || !["localnet", "devnet"].includes(config.cluster)) {
    throw new Error("Gateway is locked to localnet or devnet");
  }
  if (!baseGenesisHash) throw new Error("Base-layer genesis hash is required before signing");

  if (config.cluster === "localnet") {
    for (const field of ["baseHttp", "baseWs", "routerHttp", "routerWs", "erHttp", "erWs"]) {
      const parsed = endpoint(config[field], field);
      if (!LOOPBACK_HOSTS.has(parsed.hostname)) {
        throw new Error(`Localnet ${field} must use a loopback host`);
      }
    }
    if (Object.values(PUBLIC_CLUSTER_GENESIS).includes(baseGenesisHash)) {
      throw new Error("Localnet configuration resolved to a public Solana cluster; signing refused");
    }
    return;
  }

  if (baseGenesisHash !== PUBLIC_CLUSTER_GENESIS.devnet) {
    throw new Error("Devnet configuration did not resolve to the Solana devnet genesis; signing refused");
  }
  const baseHttp = endpoint(config.baseHttp, "baseHttp");
  const baseWs = endpoint(config.baseWs, "baseWs");
  const routerHttp = endpoint(config.routerHttp, "routerHttp");
  const routerWs = endpoint(config.routerWs, "routerWs");
  const erHttp = endpoint(config.erHttp, "erHttp");
  const erWs = endpoint(config.erWs, "erWs");
  if (baseHttp.protocol !== "https:" || baseWs.protocol !== "wss:") {
    throw new Error("Devnet base-layer endpoints must use TLS");
  }
  if (
    routerHttp.protocol !== "https:"
    || routerWs.protocol !== "wss:"
    || routerHttp.hostname !== "devnet-router.magicblock.app"
    || routerWs.hostname !== "devnet-router.magicblock.app"
  ) {
    throw new Error("Devnet router must use the official MagicBlock devnet router over TLS");
  }
  if (
    erHttp.protocol !== "https:"
    || erWs.protocol !== "wss:"
    || erHttp.hostname !== "devnet-as.magicblock.app"
    || erWs.hostname !== "devnet-as.magicblock.app"
  ) {
    throw new Error("Devnet ER must use the official MagicBlock devnet validator over TLS");
  }
  if (erGenesisHash !== MAGICBLOCK_ER_GENESIS) {
    throw new Error("Devnet ER identity did not match the expected MagicBlock execution network");
  }
}

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

async function writeJsonAtomic(filePath, value) {
  await mkdir(path.dirname(filePath), { recursive: true });
  const temporary = `${filePath}.${process.pid}.tmp`;
  await writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
  await rename(temporary, filePath);
}

async function loadKeypair(filePath, { createDisposable = false } = {}) {
  let secret;
  try {
    secret = JSON.parse(await readFile(filePath, "utf8"));
  } catch (error) {
    if (!createDisposable || error.code !== "ENOENT") throw error;
    const generated = Keypair.generate();
    secret = Array.from(generated.secretKey);
    await writeJsonAtomic(filePath, secret);
  }
  if (!Array.isArray(secret) || secret.length !== 64) {
    throw new Error(`Invalid disposable keypair: ${path.basename(filePath)}`);
  }
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

async function ensureLocalFunding(connection, roles) {
  const targets = [
    [roles.mara.publicKey, 20 * LAMPORTS_PER_SOL],
    [roles.ivo.publicKey, 5 * LAMPORTS_PER_SOL],
    [roles.successor.publicKey, 5 * LAMPORTS_PER_SOL],
  ];
  for (const [publicKey, minimum] of targets) {
    const balance = await connection.getBalance(publicKey, "confirmed");
    if (balance >= minimum) continue;
    await connection.requestAirdrop(publicKey, minimum - balance);
    const startedAt = Date.now();
    while (await connection.getBalance(publicKey, "confirmed") < minimum) {
      if (Date.now() - startedAt > 10_000) {
        throw new Error(`Local airdrop did not confirm for ${publicKey.toBase58()}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }
}

function publicReceipt({ layer, operation, submission, cluster, programId, worksitePda }) {
  const isPublicDevnet = cluster === "devnet";
  return {
    layer,
    operation,
    signature: submission.signature,
    slot: submission.slot ?? null,
    cluster,
    programId: String(programId),
    worksitePda: String(worksitePda),
    explorerUrl: isPublicDevnet
      ? `https://explorer.solana.com/tx/${submission.signature}?cluster=devnet`
      : null,
  };
}

function cleanNetwork(network) {
  return JSON.parse(JSON.stringify(network || {}));
}

async function createRuntime() {
  const configPath = envPath("PERMSTATE_NETWORK_CONFIG", DEFAULT_CONFIG);
  const workDir = envPath("PERMSTATE_WORK_DIR", DEFAULT_WORK_DIR);
  const staticDir = envPath("PERMSTATE_STATIC_DIR", DEFAULT_STATIC_DIR);
  const config = await readJson(configPath);
  const programId = new PublicKey(config.programId);
  const router = createMagicRouterConnection({
    httpEndpoint: config.routerHttp,
    wsEndpoint: config.routerWs,
  });
  const base = createBaseLayerConnection({
    httpEndpoint: config.baseHttp,
    wsEndpoint: config.baseWs,
  });
  const ephemeral = createEphemeralConnection({
    httpEndpoint: config.erHttp,
    wsEndpoint: config.erWs,
  });
  const [baseGenesisHash, erGenesisHash] = await Promise.all([
    base.getGenesisHash(),
    ephemeral.getGenesisHash(),
  ]);
  assertSafeNetworkConfiguration({ config, baseGenesisHash, erGenesisHash });

  const keyDir = envPath("PERMSTATE_KEYS_DIR", workDir);
  const createDisposable = config.cluster === "localnet";
  const [mara, ivo, successor] = await Promise.all([
    loadKeypair(path.join(keyDir, "mara-keypair.json"), { createDisposable }),
    loadKeypair(path.join(keyDir, "ivo-keypair.json"), { createDisposable }),
    loadKeypair(path.join(keyDir, "nia-keypair.json"), { createDisposable }),
  ]);
  const roles = { mara, ivo, successor };
  if (config.cluster === "localnet") await ensureLocalFunding(base, roles);
  const civilizationService = createCivilizationService({ workDir });
  const worldService = createWorldService({ workDir });
  return {
    config,
    configPath,
    workDir,
    sessionsDir: path.join(workDir, "sessions"),
    staticDir,
    programId,
    router,
    base,
    ephemeral,
    roles,
    authority: mara,
    civilizationService,
    worldService,
  };
}

function descriptorFor(runtime, sessionIdInput) {
  const ids = sessionCommitments(sessionIdInput);
  const season = initialSeasonState({
    programId: runtime.programId,
    authority: runtime.authority.publicKey,
    seasonId: ids.seasonId,
    rulesetHash: ids.rulesetHash,
    payoutRulesHash: ids.payoutRulesHash,
  });
  const initial = initialWorksiteState({
    programId: runtime.programId,
    authority: runtime.authority.publicKey,
    envoy: runtime.roles.mara.publicKey,
    maker: runtime.roles.ivo.publicKey,
    successor: runtime.roles.successor.publicKey,
    seasonId: ids.seasonId,
    worksiteId: ids.worksiteId,
    rulesetHash: ids.rulesetHash,
    seasonPurse: season.seasonPurse,
  });
  return {
    sessionId: ids.sessionId,
    programId: runtime.programId.toBase58(),
    worksitePda: initial.worksitePda.toBase58(),
    seasonId: ids.seasonId.toString("hex"),
    worksiteId: ids.worksiteId.toString("hex"),
    rulesetHash: ids.rulesetHash.toString("hex"),
    payoutRulesHash: ids.payoutRulesHash.toString("hex"),
    seasonPurse: season.seasonPurse.toBase58(),
    roles: {
      mara: runtime.roles.mara.publicKey.toBase58(),
      ivo: runtime.roles.ivo.publicKey.toBase58(),
      successor: runtime.roles.successor.publicKey.toBase58(),
    },
    initialState: initial.state,
    initialSeasonState: season.state,
    worksiteAddress: initial.worksitePda,
    seasonAddress: season.seasonPurse,
  };
}

function publicDescriptor(descriptor) {
  const {
    initialState: _initialState,
    initialSeasonState: _initialSeasonState,
    worksiteAddress: _worksiteAddress,
    seasonAddress: _seasonAddress,
    ...value
  } = descriptor;
  return value;
}

function sessionPath(runtime, sessionId) {
  return path.join(runtime.sessionsDir, `${sanitizeSession(sessionId)}.json`);
}

function seasonLedgerPath(runtime, descriptor) {
  return path.join(runtime.sessionsDir, `_season-ledger-${descriptor.seasonPurse}.json`);
}

async function loadSeasonLedger(runtime, descriptor) {
  const filePath = seasonLedgerPath(runtime, descriptor);
  if (!existsSync(filePath)) return null;
  return readJson(filePath);
}

async function saveSeasonLedger(runtime, descriptor, patch) {
  const existing = await loadSeasonLedger(runtime, descriptor);
  const next = {
    schemaVersion: "permutation-state.season-ledger-index.v1",
    seasonPurse: descriptor.seasonPurse,
    cluster: runtime.config.cluster,
    initialize: existing?.initialize || null,
    economyReceipts: existing?.economyReceipts || [],
    ...patch,
    updatedAt: new Date().toISOString(),
  };
  await writeJsonAtomic(seasonLedgerPath(runtime, descriptor), next);
  return next;
}

async function loadSessionRecord(runtime, sessionId) {
  const filePath = sessionPath(runtime, sessionId);
  if (!existsSync(filePath)) return null;
  return readJson(filePath);
}

async function saveSessionRecord(runtime, record) {
  await writeJsonAtomic(sessionPath(runtime, record.sessionId), record);
}

function pendingCheckpointIndex(record) {
  return (record?.network?.receipts || []).findLastIndex(
    (receipt) => receipt.checkpointRequired && !receipt.checkpoint,
  );
}

async function saveReceiptPatch(runtime, record, index, patch, networkPatch = {}) {
  const receipts = [...(record.network?.receipts || [])];
  if (!receipts[index]) throw new Error("Checkpoint receipt index no longer exists");
  receipts[index] = { ...receipts[index], ...patch };
  const next = sessionRecord({
    sessionId: record.sessionId,
    descriptor: record.descriptor,
    store: record.store,
    network: {
      ...record.network,
      ...networkPatch,
      receipts,
      lastReceipt: receipts.at(-1) || null,
    },
  });
  await saveSessionRecord(runtime, next);
  return next;
}

async function withKeyLock(lockMap, key, operation) {
  const previous = lockMap.get(key) || Promise.resolve();
  let release;
  const current = new Promise((resolve) => { release = resolve; });
  const queued = previous.then(() => current);
  lockMap.set(key, queued);
  await previous;
  try {
    return await operation();
  } finally {
    release();
    if (lockMap.get(key) === queued) lockMap.delete(key);
  }
}

async function withSessionLock(sessionIdInput, operation) {
  const sessionId = sanitizeSession(sessionIdInput);
  return withKeyLock(locks, sessionId, () => operation(sessionId));
}

async function withSeasonLock(seasonPurse, operation) {
  return withKeyLock(seasonLocks, String(seasonPurse), operation);
}

async function tryRead(connection, address) {
  try {
    return await readWorksiteState(connection, address, "confirmed");
  } catch (error) {
    if (/was not found/.test(error.message)) return null;
    throw error;
  }
}

async function tryReadSeason(connection, address) {
  try {
    return await readSeasonState(connection, address, "confirmed");
  } catch (error) {
    if (/was not found/.test(error.message)) return null;
    throw error;
  }
}

async function delegationStatus(runtime, address) {
  try {
    return (await getWorksiteDelegationStatus(runtime.router, address)).isDelegated;
  } catch {
    return null;
  }
}

function sameBytes(left, right) {
  return Buffer.from(left).equals(Buffer.from(right));
}

function assertWorksiteIdentity(state, descriptor) {
  const initial = descriptor.initialState;
  const publicKeysMatch = ["authority", "envoy", "maker", "successor"]
    .every((field) => state[field].equals(initial[field]));
  const commitmentsMatch = ["seasonId", "worksiteId", "rulesetHash"]
    .every((field) => sameBytes(state[field], initial[field]));
  if (
    state.version !== initial.version
    || !publicKeysMatch
    || !commitmentsMatch
    || !state.seasonPurse.equals(initial.seasonPurse)
  ) {
    throw Object.assign(
      new Error("Existing Worksite identity does not match this session descriptor"),
      { statusCode: 409 },
    );
  }
  if (state.seq === 0 && (
    !sameBytes(state.stateRoot, initial.stateRoot)
    || !sameBytes(state.headEventHash, initial.headEventHash)
    || state.stage !== initial.stage
  )) {
    throw Object.assign(
      new Error("Existing genesis Worksite does not match the published initial state"),
      { statusCode: 409 },
    );
  }
}

function assertSeasonIdentity(state, descriptor) {
  const initial = descriptor.initialSeasonState;
  if (
    state.version !== initial.version
    || !state.authority.equals(initial.authority)
    || !sameBytes(state.seasonId, initial.seasonId)
    || !sameBytes(state.rulesetHash, initial.rulesetHash)
    || !sameBytes(state.payoutRulesHash, initial.payoutRulesHash)
  ) {
    throw Object.assign(
      new Error("Existing Season identity does not match the published Season Zero commitments"),
      { statusCode: 409 },
    );
  }
  if (!state.stateRootMatches) {
    throw Object.assign(new Error("Existing Season PDA failed its state-root check"), { statusCode: 409 });
  }
  if (state.status === 1 && [state.outcomeHash, state.chronicleRoot, state.claimRoot].some((hash) => (
    Buffer.from(hash).some((byte) => byte !== 0)
  ))) {
    throw Object.assign(new Error("Active Season exposes finalized outcome or claim roots"), { statusCode: 409 });
  }
}

export function allocatePurseUnits(sourceKind, grossUnits) {
  const gross = BigInt(grossUnits);
  if (sourceKind === PURSE_SOURCE.ENTRY) {
    const purse = (gross * 7_000n) / 10_000n;
    return { purse, ops: gross - purse, seller: 0n };
  }
  if (sourceKind === PURSE_SOURCE.MARKETPLACE) {
    const purse = (gross * 150n) / 10_000n;
    const ops = (gross * 100n) / 10_000n;
    return { purse, ops, seller: gross - purse - ops };
  }
  throw new Error("Unsupported Season Purse source");
}

function seasonLedgerReceipt({ operation, sourceKind = null, grossUnits = 0, submission, before, after, runtime, descriptor }) {
  const allocation = sourceKind == null ? null : allocatePurseUnits(sourceKind, grossUnits);
  return {
    layer: "solana-base",
    operation,
    signature: submission.signature,
    slot: submission.slot ?? null,
    cluster: runtime.config.cluster,
    programId: runtime.programId.toBase58(),
    seasonPurse: descriptor.seasonPurse,
    explorerUrl: runtime.config.cluster === "devnet"
      ? `https://explorer.solana.com/tx/${submission.signature}?cluster=devnet`
      : null,
    sourceKind: sourceKind === PURSE_SOURCE.ENTRY
      ? "entry"
      : sourceKind === PURSE_SOURCE.MARKETPLACE
        ? "marketplace"
        : null,
    grossUnits,
    purseUnits: allocation ? Number(allocation.purse) : 0,
    opsUnits: allocation ? Number(allocation.ops) : 0,
    sellerUnits: allocation ? Number(allocation.seller) : 0,
    sequenceBefore: before?.seq ?? null,
    sequenceAfter: after?.seq ?? null,
    stateRoot: after?.stateRoot ? Buffer.from(after.stateRoot).toString("hex") : null,
    headEventHash: after?.headEventHash ? Buffer.from(after.headEventHash).toString("hex") : null,
    purseTotal: after?.purseTotal ?? null,
  };
}

async function ensureSeasonInitialized(runtime, descriptor) {
  let existing = await tryReadSeason(runtime.base, descriptor.seasonAddress);
  if (existing) {
    assertSeasonIdentity(existing.state, descriptor);
    return { read: existing, initializeReceipt: null };
  }
  const submission = await sendInitializeSeason({
    connection: runtime.base,
    programId: runtime.programId,
    authority: runtime.authority.publicKey,
    seasonPurse: descriptor.seasonAddress,
    state: descriptor.initialSeasonState,
    signers: [runtime.authority],
  });
  existing = await pollSeasonState({
    connection: runtime.base,
    seasonPurse: descriptor.seasonAddress,
    predicate: (state) => state.seq === 0 && state.stateRootMatches,
    timeoutMs: 20_000,
  });
  assertSeasonIdentity(existing.state, descriptor);
  const initializeReceipt = seasonLedgerReceipt({
    operation: "initialize-season",
    submission,
    before: null,
    after: existing.state,
    runtime,
    descriptor,
  });
  await saveSeasonLedger(runtime, descriptor, { initialize: initializeReceipt });
  return {
    read: existing,
    initializeReceipt,
  };
}

async function creditSeasonPurse(runtime, descriptor, before, sourceKind, grossUnits, eventLabel) {
  const eventId = sha256Bytes(
    "PERMSTATE/SEASON_LEDGER_EVENT/V1",
    descriptor.seasonId,
    eventLabel,
  );
  const submission = await sendCreditSeasonPurse({
    connection: runtime.base,
    programId: runtime.programId,
    authority: runtime.authority.publicKey,
    seasonPurse: descriptor.seasonAddress,
    args: {
      sourceKind,
      grossUnits,
      expectedSeq: before.state.seq,
      expectedHeadEventHash: before.state.headEventHash,
      eventId,
    },
    signers: [runtime.authority],
  });
  const allocation = allocatePurseUnits(sourceKind, grossUnits);
  const expectedPurse = before.state.purseTotal + Number(allocation.purse);
  const after = await pollSeasonState({
    connection: runtime.base,
    seasonPurse: descriptor.seasonAddress,
    predicate: (state) => (
      state.seq === before.state.seq + 1
      && state.purseTotal === expectedPurse
      && state.stateRootMatches
    ),
    timeoutMs: 20_000,
  });
  const receipt = seasonLedgerReceipt({
    operation: "credit-season-purse",
    sourceKind,
    grossUnits,
    submission,
    before: before.state,
    after: after.state,
    runtime,
    descriptor,
  });
  const ledger = await loadSeasonLedger(runtime, descriptor);
  await saveSeasonLedger(runtime, descriptor, {
    economyReceipts: [...(ledger?.economyReceipts || []), receipt],
  });
  return {
    read: after,
    receipt,
  };
}

async function ensureDemoPurseLedger(runtime, descriptor, initialRead) {
  let current = initialRead;
  const receipts = [];
  const state = current.state;
  const isEmpty = state.entryGrossUnits === 0
    && state.marketplaceGrossUnits === 0
    && state.purseTotal === 0
    && state.opsUnits === 0
    && state.sellerUnits === 0;
  if (isEmpty) {
    const credited = await creditSeasonPurse(
      runtime,
      descriptor,
      current,
      PURSE_SOURCE.ENTRY,
      DEMO_ENTRY_GROSS_UNITS,
      "season-zero-demo-entry-10",
    );
    current = credited.read;
    receipts.push(credited.receipt);
  }
  const afterEntry = current.state;
  const entryOnly = afterEntry.entryGrossUnits === DEMO_ENTRY_GROSS_UNITS
    && afterEntry.entryPurseUnits === 7 * MOCK_USDC_MICRO_UNITS
    && afterEntry.marketplaceGrossUnits === 0;
  if (entryOnly) {
    const credited = await creditSeasonPurse(
      runtime,
      descriptor,
      current,
      PURSE_SOURCE.MARKETPLACE,
      DEMO_MARKETPLACE_GROSS_UNITS,
      "season-zero-demo-marketplace-100",
    );
    current = credited.read;
    receipts.push(credited.receipt);
  }
  return { read: current, receipts };
}

async function isSessionDelegated(runtime, address, erRead = null) {
  const status = await delegationStatus(runtime, address);
  if (status !== null) return status;
  // The standalone stack's direct ER endpoint is itself authoritative evidence
  // that the account was cloned after delegation. Hosted environments must
  // report router delegation status explicitly.
  return runtime.config.cluster === "localnet" && Boolean(erRead);
}

async function attemptPendingCheckpoint(runtime, descriptor, record) {
  const index = pendingCheckpointIndex(record);
  if (index < 0) return record;
  let currentRecord = record;
  let receipt = currentRecord.network.receipts[index];
  try {
    const expectedRoot = Buffer.from(receipt.er.stateRoot, "hex");
    const erRead = await pollWorksiteState({
      connection: runtime.router,
      worksitePda: descriptor.worksiteAddress,
      predicate: (state) => (
        state.seq === receipt.er.sequence
        && sameBytes(state.stateRoot, expectedRoot)
        && state.stateRootMatches
      ),
      timeoutMs: 8_000,
    });

    // A user-triggered retry after a recorded failure schedules a fresh
    // same-root commit instead of resolving one dead scheduling transaction
    // forever. Gameplay is blocked while pending, so the ER root is stable.
    let commitReceipt = receipt.checkpointError ? null : receipt.commit;
    if (!commitReceipt) {
      const commit = await sendCommitWorksite({
        connection: runtime.router,
        programId: runtime.programId,
        authority: runtime.roles.ivo.publicKey,
        worksitePda: descriptor.worksiteAddress,
        signers: [runtime.roles.ivo],
      });
      commitReceipt = makeErTransactionReceipt({
        operation: "commit",
        signature: commit.signature,
        slot: commit.slot,
        rpcEndpoint: runtime.config.erHttp,
        programId: runtime.programId,
        worksitePda: descriptor.worksiteAddress,
        state: erRead.state,
      });
      if (runtime.config.cluster === "localnet") commitReceipt.explorerUrl = null;
      currentRecord = await saveReceiptPatch(runtime, currentRecord, index, {
        commit: commitReceipt,
        commitAttempts: [...(receipt.commitAttempts || []), commitReceipt],
        checkpointError: null,
      });
      receipt = currentRecord.network.receipts[index];
    }

    const checkpoint = await resolveSolanaCheckpoint({
      erSignature: commitReceipt.signature,
      ephemeralConnection: runtime.ephemeral,
      baseConnection: runtime.base,
      programId: runtime.programId,
      worksitePda: descriptor.worksiteAddress,
      expectedStateRoot: expectedRoot,
      expectedHeadEventHash: Buffer.from(receipt.er.headEventHash, "hex"),
      timeoutMs: 30_000,
    });
    checkpoint.cluster = runtime.config.cluster;
    if (runtime.config.cluster === "localnet") checkpoint.explorerUrl = null;
    return saveReceiptPatch(runtime, currentRecord, index, {
      checkpoint,
      checkpointError: null,
    }, {
      lastLayer: "solana-checkpoint",
    });
  } catch (error) {
    return saveReceiptPatch(runtime, currentRecord, index, {
      checkpointError: error.message,
    }, {
      lastLayer: "ephemeral-rollup",
    });
  }
}

async function currentSession(runtime, sessionIdInput) {
  const descriptor = descriptorFor(runtime, sessionIdInput);
  const record = await loadSessionRecord(runtime, descriptor.sessionId);
  const erRead = await tryRead(runtime.router, descriptor.worksiteAddress);
  const baseRead = await tryRead(runtime.base, descriptor.worksiteAddress);
  const seasonRead = await tryReadSeason(runtime.base, descriptor.seasonAddress);
  const seasonNetwork = await loadSeasonLedger(runtime, descriptor);
  const chainRead = erRead || baseRead;
  if (chainRead) {
    if (!chainRead.owner.equals(runtime.programId)) {
      throw Object.assign(new Error("Worksite PDA is not owned by the configured program"), { statusCode: 409 });
    }
    assertWorksiteIdentity(chainRead.state, descriptor);
  }
  if (seasonRead) {
    if (!seasonRead.owner.equals(runtime.programId)) {
      throw Object.assign(new Error("Season PDA is not owned by the configured program"), { statusCode: 409 });
    }
    assertSeasonIdentity(seasonRead.state, descriptor);
  }
  const delegated = chainRead
    ? await isSessionDelegated(runtime, descriptor.worksiteAddress, erRead)
    : false;
  // A local validator reset invalidates its generated receipt index. Never
  // replay a stale JSON record when no corresponding base/ER account exists.
  const activeRecord = chainRead ? record : null;
  return {
    schemaVersion: "permutation-state.magicblock-gateway.v1",
    initialized: Boolean(chainRead),
    recoverable: !chainRead || Boolean(record),
    cluster: runtime.config.cluster,
    disclosure: runtime.config.disclosure,
    descriptor: publicDescriptor(descriptor),
    state: stateToJson(chainRead?.state),
    stateSlot: chainRead?.slot ?? null,
    stateLayer: erRead ? "ephemeral-rollup" : baseRead ? "solana-base" : null,
    season: seasonStateToJson(seasonRead?.state),
    seasonSlot: seasonRead?.slot ?? null,
    seasonNetwork: cleanNetwork(seasonNetwork),
    delegated,
    store: activeRecord?.store || newProofStore(descriptor.sessionId),
    network: cleanNetwork(activeRecord?.network),
    endpoints: {
      base: runtime.config.baseHttp,
      router: runtime.config.routerHttp,
      er: runtime.config.erHttp,
    },
  };
}

async function bootstrapSession(runtime, sessionIdInput) {
  return withSessionLock(sessionIdInput, async (sessionId) => {
    const descriptor = descriptorFor(runtime, sessionId);
    const { seasonSetup, purseSetup } = await withSeasonLock(
      descriptor.seasonPurse,
      async () => {
        const initialized = await ensureSeasonInitialized(runtime, descriptor);
        const seeded = await ensureDemoPurseLedger(runtime, descriptor, initialized.read);
        return { seasonSetup: initialized, purseSetup: seeded };
      },
    );
    const erExisting = await tryRead(runtime.router, descriptor.worksiteAddress);
    const baseExisting = await tryRead(runtime.base, descriptor.worksiteAddress);
    const existing = erExisting || baseExisting;
    let record = await loadSessionRecord(runtime, sessionId);
    if (existing) {
      assertWorksiteIdentity(existing.state, descriptor);
      if (record && record.store.events.length !== existing.state.seq) {
        throw Object.assign(
          new Error("The Worksite sequence does not match its local receipt index; use a new session id"),
          { statusCode: 409 },
        );
      }
      if (!record && existing.state.seq !== 0) {
        throw Object.assign(
          new Error("The Worksite exists but its local receipt index is missing; use a new session id"),
          { statusCode: 409 },
        );
      }
      if (await isSessionDelegated(runtime, descriptor.worksiteAddress, erExisting)) {
        if (!record) {
          record = sessionRecord({
            sessionId,
            descriptor: publicDescriptor(descriptor),
            store: newProofStore(sessionId),
            network: { recoveredAt: new Date().toISOString(), receipts: [] },
          });
          await saveSessionRecord(runtime, record);
        }
        return currentSession(runtime, sessionId);
      }
      if (existing.state.seq !== 0 || (record?.store.events.length || 0) !== 0) {
        throw Object.assign(
          new Error("The existing Worksite is not delegated; continuing would mislabel base-layer execution as ER play"),
          { statusCode: 409 },
        );
      }

      const lifecycleConnection = runtime.config.cluster === "localnet"
        ? runtime.base
        : runtime.router;
      const delegate = await sendDelegateWorksite({
        connection: lifecycleConnection,
        programId: runtime.programId,
        authority: runtime.authority.publicKey,
        worksitePda: descriptor.worksiteAddress,
        validator: runtime.config.validatorIdentity,
        signers: [runtime.authority],
      });
      const delegated = await pollWorksiteState({
        connection: runtime.router,
        worksitePda: descriptor.worksiteAddress,
        predicate: (state) => state.seq === 0 && state.stateRootMatches,
        timeoutMs: 30_000,
      });
      record = sessionRecord({
        sessionId,
        descriptor: publicDescriptor(descriptor),
        store: record?.store || newProofStore(sessionId),
        network: {
          ...(record?.network || {}),
          seasonInitialize: record?.network?.seasonInitialize || seasonSetup.initializeReceipt,
          economyReceipts: [
            ...(record?.network?.economyReceipts || []),
            ...purseSetup.receipts,
          ],
          recoveredAt: new Date().toISOString(),
          lastLayer: "ephemeral-rollup",
          receipts: record?.network?.receipts || [],
          delegate: publicReceipt({
            layer: "solana-base",
            operation: "delegate",
            submission: delegate,
            cluster: runtime.config.cluster,
            programId: descriptor.programId,
            worksitePda: descriptor.worksitePda,
          }),
          stateRoot: Buffer.from(delegated.state.stateRoot).toString("hex"),
          genesisSlot: existing.slot,
        },
      });
      await saveSessionRecord(runtime, record);
      return currentSession(runtime, sessionId);
    }

    // The hosted Magic Router selects L1/ER automatically. The standalone
    // stack intentionally exposes those surfaces separately, so bootstrap on
    // L1 and switch to the ER only after delegation is confirmed.
    const lifecycleConnection = runtime.config.cluster === "localnet"
      ? runtime.base
      : runtime.router;
    const initialize = await sendInitializeWorksite({
      connection: lifecycleConnection,
      programId: runtime.programId,
      authority: runtime.authority.publicKey,
      worksitePda: descriptor.worksiteAddress,
      state: descriptor.initialState,
      signers: [runtime.authority],
    });
    const initialized = await pollWorksiteState({
      connection: lifecycleConnection,
      worksitePda: descriptor.worksiteAddress,
      predicate: (state) => state.seq === 0 && state.stateRootMatches,
      timeoutMs: 20_000,
    });
    const delegate = await sendDelegateWorksite({
      connection: lifecycleConnection,
      programId: runtime.programId,
      authority: runtime.authority.publicKey,
      worksitePda: descriptor.worksiteAddress,
      validator: runtime.config.validatorIdentity,
      signers: [runtime.authority],
    });
    const delegated = await pollWorksiteState({
      connection: runtime.router,
      worksitePda: descriptor.worksiteAddress,
      predicate: (state) => state.seq === 0 && state.stateRootMatches,
      timeoutMs: 30_000,
    });
    const network = {
      initializedAt: new Date().toISOString(),
      lastLayer: "ephemeral-rollup",
      receipts: [],
      seasonInitialize: seasonSetup.initializeReceipt,
      economyReceipts: purseSetup.receipts,
      initialize: publicReceipt({
        layer: "solana-base",
        operation: "initialize",
        submission: initialize,
        cluster: runtime.config.cluster,
        programId: descriptor.programId,
        worksitePda: descriptor.worksitePda,
      }),
      delegate: publicReceipt({
        layer: "solana-base",
        operation: "delegate",
        submission: delegate,
        cluster: runtime.config.cluster,
        programId: descriptor.programId,
        worksitePda: descriptor.worksitePda,
      }),
      stateRoot: Buffer.from(delegated.state.stateRoot).toString("hex"),
      genesisSlot: initialized.slot,
    };
    record = sessionRecord({
      sessionId,
      descriptor: publicDescriptor(descriptor),
      store: newProofStore(sessionId),
      network,
    });
    await saveSessionRecord(runtime, record);
    return currentSession(runtime, sessionId);
  });
}

async function applySessionEvent(runtime, sessionIdInput, clientEvent) {
  return withSessionLock(sessionIdInput, async (sessionId) => {
    const descriptor = descriptorFor(runtime, sessionId);
    const record = await loadSessionRecord(runtime, sessionId);
    if (!record) throw Object.assign(new Error("Initialize this MagicBlock session first"), { statusCode: 409 });
    if (pendingCheckpointIndex(record) >= 0) {
      throw Object.assign(
        new Error("The Ivo result is accepted on the ER, but its Solana checkpoint must be retried before play continues"),
        { statusCode: 409 },
      );
    }
    if (clientEvent.sessionId !== sessionId) throw new Error("Event session does not match URL session");
    if (record.store.events.length !== clientEvent.seq) {
      throw Object.assign(new Error("Stale event sequence; refresh and try again"), { statusCode: 409 });
    }
    const erRead = await tryRead(runtime.router, descriptor.worksiteAddress);
    if (!await isSessionDelegated(runtime, descriptor.worksiteAddress, erRead)) {
      throw Object.assign(
        new Error("Worksite is not delegated to the Ephemeral Rollup; action refused"),
        { statusCode: 409 },
      );
    }
    const proof = await ProofCore.verifyLog({
      sessionId,
      seed: record.store.seed,
      events: [...record.store.events, clientEvent],
    });
    if (!proof.valid || proof.validatedEventCount !== record.store.events.length + 1) {
      throw Object.assign(
        new Error(`Client proof rejected: ${proof.errors.join("; ") || "event did not extend the verified head"}`),
        { statusCode: 400 },
      );
    }
    const wire = eventToWire(clientEvent);
    const actor = runtime.roles[wire.actorRole];
    const before = await pollWorksiteState({
      connection: runtime.router,
      worksitePda: descriptor.worksiteAddress,
      predicate: (state) => state.seq === clientEvent.seq && state.stateRootMatches,
      timeoutMs: 8_000,
    });
    const preview = previewEvent(before.state, {
      actor: actor.publicKey,
      eventKind: wire.eventKind,
      action: wire.action,
      clientEventHash: wire.clientEventHash,
    });
    const applied = await sendApplyWorksiteEvent({
      connection: runtime.router,
      programId: runtime.programId,
      actor: actor.publicKey,
      worksitePda: descriptor.worksiteAddress,
      args: preview.args,
      signers: [actor],
    });
    const after = await pollWorksiteState({
      connection: runtime.router,
      worksitePda: descriptor.worksiteAddress,
      predicate: (state) => (
        state.seq === preview.next.seq
        && Buffer.from(state.stateRoot).equals(preview.next.stateRoot)
        && state.stateRootMatches
      ),
      timeoutMs: 15_000,
    });
    const erReceipt = makeErTransactionReceipt({
      operation: "apply-worksite-event",
      signature: applied.signature,
      slot: after.slot ?? applied.slot,
      rpcEndpoint: runtime.config.erHttp,
      programId: runtime.programId,
      worksitePda: descriptor.worksiteAddress,
      state: after.state,
    });
    if (runtime.config.cluster === "localnet") erReceipt.explorerUrl = null;

    const receipt = {
      sequence: clientEvent.seq,
      acceptedAt: new Date().toISOString(),
      er: erReceipt,
      checkpointRequired: clientEvent.type === "IVO_CHOICE",
      commit: null,
      commitAttempts: [],
      checkpoint: null,
      checkpointError: null,
    };
    const store = {
      ...record.store,
      events: [...record.store.events, clientEvent],
      updatedAt: new Date().toISOString(),
    };
    const network = {
      ...record.network,
      lastLayer: "ephemeral-rollup",
      stateRoot: Buffer.from(after.state.stateRoot).toString("hex"),
      receipts: [...(record.network.receipts || []), receipt],
      lastReceipt: receipt,
    };
    let acceptedRecord = sessionRecord({
      sessionId,
      descriptor: publicDescriptor(descriptor),
      store,
      network,
    });
    // Persist the already-confirmed ER action before attempting the separate
    // base-layer checkpoint. A checkpoint transport failure must never erase
    // the accepted event or make the JSON index lag the delegated account.
    await saveSessionRecord(runtime, acceptedRecord);
    if (receipt.checkpointRequired) {
      acceptedRecord = await attemptPendingCheckpoint(runtime, descriptor, acceptedRecord);
    }
    return currentSession(runtime, sessionId);
  });
}

async function retrySessionCheckpoint(runtime, sessionIdInput) {
  return withSessionLock(sessionIdInput, async (sessionId) => {
    const descriptor = descriptorFor(runtime, sessionId);
    const record = await loadSessionRecord(runtime, sessionId);
    if (!record) throw Object.assign(new Error("Initialize this MagicBlock session first"), { statusCode: 409 });
    const updated = await attemptPendingCheckpoint(runtime, descriptor, record);
    if (pendingCheckpointIndex(updated) >= 0 && !updated.network.receipts.at(-1)?.checkpointError) {
      throw Object.assign(new Error("Checkpoint is still pending"), { statusCode: 409 });
    }
    return currentSession(runtime, sessionId);
  });
}

async function parseBody(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > JSON_LIMIT) throw Object.assign(new Error("Request body is too large"), { statusCode: 413 });
    chunks.push(chunk);
  }
  if (!chunks.length) return {};
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw Object.assign(new Error("Request body must be valid JSON"), { statusCode: 400 });
  }
}

function sendJson(response, statusCode, value) {
  const body = `${JSON.stringify(value)}\n`;
  response.writeHead(statusCode, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
  });
  response.end(body);
}

async function serveStatic(runtime, requestUrl, response, method = "GET") {
  let pathname = decodeURIComponent(requestUrl.pathname);
  if (pathname.endsWith("/")) pathname += "index.html";
  const filePath = path.resolve(runtime.staticDir, `.${pathname}`);
  if (filePath !== runtime.staticDir && !filePath.startsWith(`${runtime.staticDir}${path.sep}`)) {
    sendJson(response, 403, { error: "Forbidden" });
    return;
  }
  try {
    const info = await stat(filePath);
    if (!info.isFile()) throw new Error("Not a file");
    response.writeHead(200, {
      "Content-Type": CONTENT_TYPES[path.extname(filePath)] || "application/octet-stream",
      "Content-Length": info.size,
      "Cache-Control": "no-cache",
    });
    if (method === "HEAD") {
      response.end();
      return;
    }
    createReadStream(filePath).pipe(response);
  } catch {
    sendJson(response, 404, { error: "Not found" });
  }
}

async function route(runtime, request, response) {
  const requestUrl = new URL(request.url, "http://127.0.0.1");
  if (await routeCivilizationRequest(runtime.civilizationService, request, response, requestUrl)) return;
  if (await routeWorldRequest(runtime.worldService, request, response, requestUrl)) return;
  if (request.method === "GET" && requestUrl.pathname === "/api/magicblock/health") {
    const [baseSlot, erSlot] = await Promise.all([
      runtime.base.getSlot("confirmed"),
      runtime.ephemeral.getSlot("confirmed"),
    ]);
    sendJson(response, 200, {
      ok: true,
      cluster: runtime.config.cluster,
      programId: runtime.programId.toBase58(),
      baseSlot,
      erSlot,
      roles: Object.fromEntries(Object.entries(runtime.roles).map(([name, keypair]) => [name, keypair.publicKey.toBase58()])),
    });
    return;
  }
  if (request.method === "GET" && requestUrl.pathname === "/api/magicblock/session") {
    sendJson(response, 200, await currentSession(runtime, requestUrl.searchParams.get("session")));
    return;
  }
  if (request.method === "POST" && requestUrl.pathname === "/api/magicblock/bootstrap") {
    const body = await parseBody(request);
    sendJson(response, 200, await bootstrapSession(runtime, body.session));
    return;
  }
  if (request.method === "POST" && requestUrl.pathname === "/api/magicblock/action") {
    const body = await parseBody(request);
    sendJson(response, 200, await applySessionEvent(runtime, body.session, body.event));
    return;
  }
  if (request.method === "POST" && requestUrl.pathname === "/api/magicblock/checkpoint") {
    const body = await parseBody(request);
    sendJson(response, 200, await retrySessionCheckpoint(runtime, body.session));
    return;
  }
  if (
    (request.method === "GET" || request.method === "HEAD")
    && (requestUrl.pathname === "/" || requestUrl.pathname === "/civilization")
  ) {
    response.writeHead(302, {
      Location: "/civilization/",
      "Cache-Control": "no-store",
    });
    response.end();
    return;
  }
  if (request.method !== "GET" && request.method !== "HEAD") {
    sendJson(response, 405, { error: "Method not allowed" });
    return;
  }
  await serveStatic(runtime, requestUrl, response, request.method);
}

export async function startGateway({ port = Number(process.env.PORT || 4173) } = {}) {
  const runtime = await createRuntime();
  await mkdir(runtime.sessionsDir, { recursive: true });
  const server = http.createServer((request, response) => {
    route(runtime, request, response).catch((error) => {
      console.error(`[gateway] ${request.method} ${request.url}: ${error.message}`);
      if (!response.headersSent) {
        sendJson(response, error.statusCode || 500, { error: error.message });
      } else {
        response.destroy(error);
      }
    });
  });
  server.once("close", () => {
    Promise.all([
      runtime.civilizationService.close(),
      runtime.worldService.close(),
    ]).catch((error) => console.error(`[gateway] close: ${error.message}`));
  });
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", resolve);
    });
  } catch (error) {
    await Promise.all([
      runtime.civilizationService.close(),
      runtime.worldService.close(),
    ]);
    throw error;
  }
  console.log(`PERMUTATION STATE gateway ready at http://127.0.0.1:${port}`);
  console.log(`Network: ${runtime.config.cluster} · Program: ${runtime.programId.toBase58()}`);
  return { server, runtime };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  startGateway().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
