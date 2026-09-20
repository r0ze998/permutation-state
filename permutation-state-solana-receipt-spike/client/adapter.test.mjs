import assert from "node:assert/strict";
import test from "node:test";
import { Keypair, PublicKey } from "@solana/web3.js";
import {
  ACTION,
  EVENT,
  buildApplyInstruction,
  computeStateRoot,
  initialWorksiteState,
  makeExplorerReceipt,
  previewEvent,
} from "./adapter.mjs";

function fixture() {
  const programId = new PublicKey("11111111111111111111111111111112");
  const authority = Keypair.generate().publicKey;
  const envoy = Keypair.generate().publicKey;
  const maker = Keypair.generate().publicKey;
  const successor = Keypair.generate().publicKey;
  const seasonId = Buffer.alloc(32, 7);
  const worksiteId = Buffer.alloc(32, 31);
  const rulesetHash = Buffer.alloc(32, 9);
  return {
    programId,
    authority,
    envoy,
    maker,
    successor,
    ...initialWorksiteState({ programId, authority, envoy, maker, successor, seasonId, worksiteId, rulesetHash }),
  };
}

test("three-client path produces deterministic commitments and no claim", () => {
  const f = fixture();
  const mara = previewEvent(f.state, {
    actor: f.envoy,
    eventKind: EVENT.MARA_CHOICE,
    action: ACTION.OATH,
    clientEventHash: Buffer.alloc(32, 40),
  });
  const ivo = previewEvent(mara.next, {
    actor: f.maker,
    eventKind: EVENT.IVO_CHOICE,
    action: ACTION.SERVICE,
    clientEventHash: Buffer.alloc(32, 41),
  });
  const successor = previewEvent(ivo.next, {
    actor: f.successor,
    eventKind: EVENT.MANDATE_ACCEPTED,
    action: ACTION.M_032,
    clientEventHash: Buffer.alloc(32, 42),
  });

  assert.equal(successor.next.seq, 3);
  assert.equal(successor.next.water, 60);
  assert.equal(successor.next.acceptedMandate, ACTION.M_032);
  assert.equal(successor.next.seasonStatus, 1);
  assert.equal(successor.next.settlementStatus, 0);
  assert.equal(successor.next.claimAvailable, false);
  assert.deepEqual(successor.next.stateRoot, computeStateRoot(successor.next));

  const ix = buildApplyInstruction({
    programId: f.programId,
    actor: f.successor,
    worksitePda: f.worksitePda,
    args: successor.args,
  });
  assert.equal(ix.keys[0].isSigner, true);
  assert.equal(ix.keys[1].isWritable, true);
  assert.equal(ix.data.length, 172);
});

test("wrong branch and wrong actor fail before a transaction is built", () => {
  const f = fixture();
  const mara = previewEvent(f.state, {
    actor: f.envoy,
    eventKind: EVENT.MARA_CHOICE,
    action: ACTION.OATH,
    clientEventHash: Buffer.alloc(32, 40),
  });
  assert.throws(() => previewEvent(mara.next, {
    actor: f.maker,
    eventKind: EVENT.IVO_CHOICE,
    action: ACTION.CUT,
    clientEventHash: Buffer.alloc(32, 41),
  }), /not unlocked/);
  assert.throws(() => previewEvent(mara.next, {
    actor: f.envoy,
    eventKind: EVENT.IVO_CHOICE,
    action: ACTION.SERVICE,
    clientEventHash: Buffer.alloc(32, 41),
  }), /unauthorized/);
});

test("receipt is explicit devnet evidence, not a generic success flag", () => {
  const f = fixture();
  const mara = previewEvent(f.state, {
    actor: f.envoy,
    eventKind: EVENT.MARA_CHOICE,
    action: ACTION.OATH,
    clientEventHash: Buffer.alloc(32, 40),
  });
  const signature = "4".repeat(88);
  const receipt = makeExplorerReceipt({
    signature,
    slot: 123456,
    programId: f.programId,
    worksitePda: f.worksitePda,
    actor: f.envoy,
    args: mara.args,
    eventHash: mara.next.headEventHash,
  });
  assert.equal(receipt.network, "devnet");
  assert.equal(receipt.signature, signature);
  assert.match(receipt.explorerUrl, /cluster=devnet$/);
  assert.equal(receipt.rulesetHash.length, 64);
  assert.equal(receipt.priorStateRoot.length, 64);
  assert.equal(receipt.newStateRoot.length, 64);
  assert.deepEqual(receipt.invariants, {
    seasonActive: true,
    settlementNotStarted: true,
    claimUnavailable: true,
  });
});
