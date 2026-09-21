import assert from "node:assert/strict";
import test from "node:test";
import { PublicKey } from "@solana/web3.js";
import {
  LOCAL_PROOF_BUILD,
  LOCAL_PROOF_SCHEMA,
  eventToWire,
  newProofStore,
  sanitizeSession,
  sessionCommitments,
  stateToJson,
} from "./game-contract.mjs";
import { assertSafeNetworkConfiguration } from "./devnet-gateway.mjs";

const PUBLIC_GENESIS = Object.freeze({
  mainnet: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
  devnet: "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
});

const localConfig = Object.freeze({
  cluster: "localnet",
  baseHttp: "http://127.0.0.1:8899",
  baseWs: "ws://127.0.0.1:8900",
  routerHttp: "http://localhost:7799",
  routerWs: "ws://localhost:7800",
  erHttp: "http://[::1]:7799",
  erWs: "ws://[::1]:7800",
});

const devnetConfig = Object.freeze({
  cluster: "devnet",
  baseHttp: "https://api.devnet.solana.com",
  baseWs: "wss://api.devnet.solana.com",
  routerHttp: "https://devnet-router.magicblock.app",
  routerWs: "wss://devnet-router.magicblock.app",
  erHttp: "https://devnet-as.magicblock.app",
  erWs: "wss://devnet-as.magicblock.app",
});

test("session ids are safe, bounded, and never empty", () => {
  assert.equal(sanitizeSession("  Aster / Judge #1  "), "aster-judge-1");
  assert.equal(sanitizeSession("***"), "aster-demo");
  assert.equal(sanitizeSession("x".repeat(80)).length, 40);
});

test("session commitments keep the season and ruleset stable but isolate Worksites", () => {
  const first = sessionCommitments("alpha");
  const second = sessionCommitments("beta");
  assert.equal(first.seasonId.length, 32);
  assert.equal(first.worksiteId.length, 32);
  assert.equal(first.rulesetHash.length, 32);
  assert.deepEqual(first.seasonId, second.seasonId);
  assert.deepEqual(first.rulesetHash, second.rulesetHash);
  assert.notDeepEqual(first.worksiteId, second.worksiteId);
});

test("a new gateway store remains compatible with the browser proof verifier", () => {
  const store = newProofStore("Aster / Shared");
  assert.equal(store.schemaVersion, LOCAL_PROOF_SCHEMA);
  assert.equal(store.buildId, LOCAL_PROOF_BUILD);
  assert.equal(store.sessionId, "aster-shared");
  assert.deepEqual(store.events, []);
  assert.match(store.localProofDisclosure, /NO REAL FUNDS/);
});

test("client proof commands map to the fixed on-chain command vocabulary", () => {
  const hash = "ab".repeat(32);
  const oath = eventToWire({
    type: "MARA_CHOICE",
    actorRole: "mara",
    seq: 0,
    eventHash: hash,
    payload: { choice: "oath" },
  });
  assert.equal(oath.eventKind, 0);
  assert.equal(oath.action, 1);
  assert.deepEqual(oath.clientEventHash, Buffer.from(hash, "hex"));

  const mandate = eventToWire({
    type: "MANDATE_ACCEPTED",
    actorRole: "successor",
    seq: 2,
    eventHash: hash,
    payload: { mandateId: "M-032" },
  });
  assert.equal(mandate.eventKind, 2);
  assert.equal(mandate.action, 1032);
});

test("the gateway rejects event-role mismatches and unsupported actions before signing", () => {
  const base = { seq: 0, eventHash: "cd".repeat(32) };
  assert.throws(
    () => eventToWire({ ...base, type: "MARA_CHOICE", actorRole: "ivo", payload: { choice: "oath" } }),
    /must use actorRole mara/,
  );
  assert.throws(
    () => eventToWire({ ...base, type: "MARA_CHOICE", actorRole: "mara", payload: { choice: "invented" } }),
    /unsupported choice/,
  );
});

test("network identity guard refuses public clusters behind a localnet label", () => {
  assert.doesNotThrow(() => assertSafeNetworkConfiguration({
    config: localConfig,
    baseGenesisHash: "local-only-genesis",
    erGenesisHash: "11111111111111111111111111111111",
  }));
  assert.throws(() => assertSafeNetworkConfiguration({
    config: localConfig,
    baseGenesisHash: PUBLIC_GENESIS.mainnet,
    erGenesisHash: "11111111111111111111111111111111",
  }), /public Solana cluster/);
  assert.throws(() => assertSafeNetworkConfiguration({
    config: { ...localConfig, baseHttp: "https://api.mainnet-beta.solana.com" },
    baseGenesisHash: "local-only-genesis",
    erGenesisHash: "11111111111111111111111111111111",
  }), /loopback host/);
});

test("devnet signing requires the devnet genesis and official TLS MagicBlock endpoints", () => {
  assert.doesNotThrow(() => assertSafeNetworkConfiguration({
    config: devnetConfig,
    baseGenesisHash: PUBLIC_GENESIS.devnet,
    erGenesisHash: "11111111111111111111111111111111",
  }));
  assert.throws(() => assertSafeNetworkConfiguration({
    config: devnetConfig,
    baseGenesisHash: PUBLIC_GENESIS.mainnet,
    erGenesisHash: "11111111111111111111111111111111",
  }), /devnet genesis/);
  assert.throws(() => assertSafeNetworkConfiguration({
    config: { ...devnetConfig, routerHttp: "https://mainnet-router.magicblock.app" },
    baseGenesisHash: PUBLIC_GENESIS.devnet,
    erGenesisHash: "11111111111111111111111111111111",
  }), /official MagicBlock devnet router/);
});

test("state serialization makes root verification and identities public without secrets", () => {
  const key = new PublicKey("11111111111111111111111111111111");
  const bytes = Buffer.alloc(32, 7);
  const state = stateToJson({
    version: 1,
    bump: 255,
    authority: key,
    envoy: key,
    maker: key,
    successor: key,
    seasonId: bytes,
    worksiteId: bytes,
    rulesetHash: bytes,
    stateRoot: bytes,
    computedStateRoot: bytes,
    stateRootMatches: true,
    headEventHash: bytes,
    seq: 2,
    stage: 2,
    seasonStatus: 1,
    settlementStatus: 0,
    claimAvailable: false,
    branch: 1,
    resolution: 1,
    water: 60,
    food: 46,
    cohesion: 64,
    timber: 12,
    prosperity: 43,
    foodDebt: 12,
    talaTrust: 28,
    serviceStair: 1,
    riverkeepers: 1,
    memoryReceipt: 1,
    worksiteStatus: 2,
    mandateIds: [1032, 2019, 3044],
    mandateStatus: [0, 0, 0],
    acceptedMandate: 0,
  });
  assert.equal(state.authority, key.toBase58());
  assert.equal(state.stateRoot, "07".repeat(32));
  assert.equal(state.stateRootMatches, true);
  assert.equal(Object.hasOwn(state, "secretKey"), false);
});
