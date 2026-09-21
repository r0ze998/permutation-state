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
  MAX_WORLD_CITIZENS,
  WORLD_ACCOUNT_SPACE,
  WORLD_INSTRUCTION,
  WORLD_LOCATIONS,
  WORLD_RESOURCE,
  WORLD_SLUICE,
  WORLD_STATE_BYTES,
  buildCommitAndUndelegateWorldInstruction,
  buildCommitWorldInstruction,
  buildDelegateWorldInstruction,
  buildDepositWorldInventoryInstruction,
  buildGatherWorldResourceInstruction,
  buildInitializeWorldInstruction,
  buildJoinWorldInstruction,
  buildMoveWorldActorInstruction,
  buildRepairEastSluiceInstruction,
  computeWorldStateRoot,
  decodeWorldState,
  deriveWorldDelegationAccounts,
  initialWorldChainState,
  sendJoinWorld,
  worldGuardFromState,
} from "./world-magicblock-transport.mjs";

function fixture() {
  const programId = new PublicKey("11111111111111111111111111111112");
  const authority = Keypair.fromSeed(Uint8Array.from({ length: 32 }, () => 41));
  const citizen = Keypair.fromSeed(Uint8Array.from({ length: 32 }, () => 42));
  const initial = initialWorldChainState({
    programId,
    authority: authority.publicKey,
    worldId: Buffer.alloc(32, 24),
    rulesetHash: Buffer.alloc(32, 25),
  });
  return { programId, authority, citizen, ...initial };
}

function u8(value) { return Buffer.from([value]); }
function u16(value) {
  const encoded = Buffer.alloc(2);
  encoded.writeUInt16LE(value);
  return encoded;
}
function u64(value) {
  const encoded = Buffer.alloc(8);
  encoded.writeBigUInt64LE(BigInt(value));
  return encoded;
}

function encodeWorldAccount(state, size = WORLD_ACCOUNT_SPACE) {
  const payload = Buffer.concat([
    u8(state.version),
    u8(state.bump),
    state.authority.toBuffer(),
    state.worldId,
    state.rulesetHash,
    state.stateRoot,
    state.headEventHash,
    u64(state.seq),
    u8(state.citizenCount),
    ...state.citizens.map((citizen) => new PublicKey(citizen).toBuffer()),
    ...state.positions.flatMap((position) => [u16(position.x), u16(position.y)]),
    ...state.inventoryTimber.map(u16),
    ...state.inventoryStone.map(u16),
    u16(state.warehouseTimber),
    u16(state.warehouseStone),
    u16(state.timberRemaining),
    u16(state.stoneRemaining),
    u8(state.repairProgress),
    u8(state.sluiceCompleted ? 1 : 0),
    u16(state.waterRate),
  ]);
  assert.equal(payload.length, WORLD_STATE_BYTES);
  return Buffer.concat([payload, Buffer.alloc(size - payload.length)]);
}

test("world genesis state matches the fixed Rust account shape", () => {
  const f = fixture();
  assert.equal(f.state.citizens.length, MAX_WORLD_CITIZENS);
  assert.equal(f.state.stateRoot.length, 32);
  assert.equal(f.state.headEventHash.length, 32);
  assert.ok(computeWorldStateRoot(f.state).equals(f.state.stateRoot));

  const decoded = decodeWorldState(encodeWorldAccount(f.state));
  assert.equal(decoded.decodedBytes, WORLD_STATE_BYTES);
  assert.equal(decoded.citizenCount, 0);
  assert.equal(decoded.timberRemaining, 18);
  assert.equal(decoded.stoneRemaining, 15);
  assert.equal(decoded.waterRate, 70);
  assert.equal(decoded.stateRootMatches, true);

  const tampered = encodeWorldAccount(f.state);
  tampered[WORLD_STATE_BYTES - 1] ^= 1;
  assert.equal(decodeWorldState(tampered).stateRootMatches, false);
});

test("world PDA and genesis hashes match the Rust cross-language fixture", () => {
  const programId = new PublicKey(Uint8Array.from({ length: 32 }, () => 21));
  const authority = new PublicKey(Uint8Array.from({ length: 32 }, () => 22));
  const { state, worldPda } = initialWorldChainState({
    programId,
    authority,
    worldId: Buffer.alloc(32, 24),
    rulesetHash: Buffer.alloc(32, 25),
  });
  assert.equal(worldPda.toBase58(), "HFqKWrr8LXR5TyKcweKzonjkp5vNvf8NsKASYAHmdEzi");
  assert.equal(state.bump, 255);
  assert.equal(state.stateRoot.toString("hex"), "928dc4d5347bae18068af3d08f0dae2790c824e2dcf019904aece57c85212703");
  assert.equal(state.headEventHash.toString("hex"), "2d6de979e06bc8b0126b747c6b211b31d17d6477c4d157c8e00e3eba0c93b7f7");
});

test("world instruction builders preserve Rust enum tags and Borsh layouts", () => {
  const f = fixture();
  const guard = worldGuardFromState(f.state, Buffer.alloc(32, 55));
  const initialize = buildInitializeWorldInstruction({
    programId: f.programId,
    authority: f.authority.publicKey,
    worldPda: f.worldPda,
    state: f.state,
  });
  assert.equal(initialize.data[0], WORLD_INSTRUCTION.INITIALIZE_WORLD);
  assert.equal(initialize.data.length, 97);
  assert.equal(initialize.keys.length, 3);
  assert.ok(initialize.keys[2].pubkey.equals(SystemProgram.programId));

  const builders = [
    [buildJoinWorldInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, guard }), WORLD_INSTRUCTION.JOIN_WORLD, 105],
    [buildMoveWorldActorInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, x: 120, y: 140, guard }), WORLD_INSTRUCTION.MOVE_WORLD_ACTOR, 109],
    [buildGatherWorldResourceInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, resourceKind: WORLD_RESOURCE.TIMBER, guard }), WORLD_INSTRUCTION.GATHER_WORLD_RESOURCE, 106],
    [buildDepositWorldInventoryInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, guard }), WORLD_INSTRUCTION.DEPOSIT_WORLD_INVENTORY, 105],
    [buildRepairEastSluiceInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, guard }), WORLD_INSTRUCTION.REPAIR_EAST_SLUICE, 105],
  ];
  for (const [instruction, tag, length] of builders) {
    assert.equal(instruction.data[0], tag);
    assert.equal(instruction.data.length, length);
    assert.equal(instruction.keys.length, 2);
    assert.equal(instruction.keys[0].isSigner, true);
    assert.ok(instruction.keys[1].pubkey.equals(f.worldPda));
  }
  assert.throws(
    () => buildMoveWorldActorInstruction({ programId: f.programId, actor: f.citizen.publicKey, worldPda: f.worldPda, x: 1001, y: 0, guard }),
    /outside the published bounds/,
  );
  assert.deepEqual(WORLD_LOCATIONS.timber, { x: 180, y: 200 });
  assert.deepEqual(WORLD_LOCATIONS.stone, { x: 790, y: 170 });
  assert.deepEqual(WORLD_LOCATIONS.warehouse, { x: 460, y: 310 });
  assert.deepEqual(WORLD_LOCATIONS.eastSluice, { x: 720, y: 300 });
  assert.deepEqual(WORLD_SLUICE, { repairSteps: 4, timberPerStep: 2, stonePerStep: 1 });
});

test("world delegation and settlement builders use MagicBlock lifecycle accounts", () => {
  const f = fixture();
  const validator = new PublicKey("MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57");
  const derived = deriveWorldDelegationAccounts({ programId: f.programId, worldPda: f.worldPda });
  const delegate = buildDelegateWorldInstruction({
    programId: f.programId,
    authority: f.authority.publicKey,
    worldPda: f.worldPda,
    validator,
  });
  assert.deepEqual([...delegate.data], [WORLD_INSTRUCTION.DELEGATE_WORLD]);
  assert.equal(delegate.keys.length, 9);
  assert.ok(delegate.keys[4].pubkey.equals(derived.delegationBuffer));
  assert.ok(delegate.keys[5].pubkey.equals(derived.delegationRecord));
  assert.ok(delegate.keys[6].pubkey.equals(derived.delegationMetadata));
  assert.ok(delegate.keys[7].pubkey.equals(DELEGATION_PROGRAM_ID));

  const commit = buildCommitWorldInstruction({
    programId: f.programId,
    payer: f.authority.publicKey,
    worldPda: f.worldPda,
  });
  const undelegate = buildCommitAndUndelegateWorldInstruction({
    programId: f.programId,
    payer: f.authority.publicKey,
    worldPda: f.worldPda,
  });
  for (const instruction of [commit, undelegate]) {
    assert.ok(instruction.keys[2].pubkey.equals(MAGIC_PROGRAM_ID));
    assert.ok(instruction.keys[3].pubkey.equals(MAGIC_CONTEXT_ID));
  }
  assert.deepEqual([...commit.data], [WORLD_INSTRUCTION.COMMIT_WORLD]);
  assert.deepEqual([...undelegate.data], [WORLD_INSTRUCTION.COMMIT_AND_UNDELEGATE_WORLD]);
});

test("world submission wrapper sends the actor-signed instruction", async () => {
  const f = fixture();
  let submitted = null;
  const connection = {
    rpcEndpoint: "http://127.0.0.1:7799",
    async sendAndConfirmTransaction(transaction, signers) {
      submitted = { transaction, signers };
      return "1".repeat(88);
    },
    async getSignatureStatuses() {
      return { value: [{ slot: 77 }] };
    },
  };
  const result = await sendJoinWorld({
    connection,
    programId: f.programId,
    actor: f.citizen.publicKey,
    worldPda: f.worldPda,
    guard: worldGuardFromState(f.state, Buffer.alloc(32, 56)),
    signers: [f.citizen],
  });
  assert.equal(result.operation, "join-world");
  assert.equal(result.slot, 77);
  assert.equal(submitted.transaction.instructions[0].data[0], WORLD_INSTRUCTION.JOIN_WORLD);
  assert.ok(submitted.signers[0].publicKey.equals(f.citizen.publicKey));
});
