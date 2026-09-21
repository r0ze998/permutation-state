import assert from "node:assert/strict";
import test from "node:test";
import { Keypair, PublicKey } from "@solana/web3.js";
import {
  ACTION,
  EVENT,
  buildApplyInstruction,
  computeStateRoot,
  initialSeasonState,
  initialWorksiteState,
  makeExplorerReceipt,
  previewEvent,
} from "./adapter.mjs";

test("JavaScript roots match the fixed Rust Season and Worksite fixture", () => {
  const key = (byte) => new PublicKey(Buffer.alloc(32, byte));
  const programId = key(1);
  const authority = key(2);
  const seasonId = Buffer.alloc(32, 3);
  const rulesetHash = Buffer.alloc(32, 4);
  const season = initialSeasonState({
    programId,
    authority,
    seasonId,
    rulesetHash,
    payoutRulesHash: Buffer.alloc(32, 5),
  });
  assert.equal(season.seasonPurse.toBase58(), "FBgcmTa8gv9wdkCoisSfFA3nNQayk8dPm2f3XCQRY34g");
  assert.equal(season.state.bump, 255);
  assert.equal(season.state.stateRoot.toString("hex"), "7fcca53cdf561370b3f823ec90c05522c451077aa59ae7a4966202ed2a6fb28d");
  assert.equal(season.state.headEventHash.toString("hex"), "405722c1ab97aefd6fd2fe4c105d0d2f6e2ccc2ed053bb88ef5de70bbafc5dc0");

  const worksite = initialWorksiteState({
    programId,
    authority,
    envoy: key(7),
    maker: key(8),
    successor: key(9),
    seasonId,
    worksiteId: Buffer.alloc(32, 6),
    rulesetHash,
  });
  assert.equal(worksite.worksitePda.toBase58(), "BLsdrKQZVwkGjqS6XZH4evwmHg8j2rxiamYA2kB5C9uW");
  assert.equal(worksite.state.bump, 255);
  assert.equal(worksite.state.stateRoot.toString("hex"), "6a4974bca96781232b853abecd7c5448b3507c21cbd663da113a9f4edae3a968");
  assert.equal(worksite.state.headEventHash.toString("hex"), "3050fd23a4fcbc832e7901cf918453fb9eac9dae0c7d77c7d06ef483cb38879c");
});

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
