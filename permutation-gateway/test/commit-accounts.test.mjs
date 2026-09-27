// The account lists of the commit and undelegate intents, as the program
// reads them (permutation-chain `instruction.rs`): CommitPart carries a
// nation account after world chunk 0, from which the program reads the
// crank's key; UndelegatePart carries no authority account and ends with the
// Instructions sysvar (it must be alone in its transaction).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, SYSVAR_INSTRUCTIONS_PUBKEY } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { NATION_TARGET } from '../client/src/codec.mjs';
import { DEFAULTS } from '../src/config.mjs';

const chain = new ChainClient(DEFAULTS.programId, 1_790_000_000_123n);
const crank = Keypair.generate().publicKey;
const keysOf = ix => ix.keys.map(k => [k.pubkey.toBase58(), k.isWritable, k.isSigner]);

test('commitPart passes nation 0 (read-only) after chunk 0, then the targets', () => {
  const [ix] = chain.commitPart({ payer: crank, targets: [NATION_TARGET + 1, 3] });
  const keys = keysOf(ix);
  assert.deepEqual(keys[0], [crank.toBase58(), true, true]);
  assert.deepEqual(keys[3], [chain.worldChunks[0].toBase58(), true, false]);
  assert.deepEqual(keys[4], [chain.nation(0).toBase58(), false, false]);
  assert.deepEqual(keys.slice(5).map(k => k[0]), [chain.nation(1).toBase58(), chain.worldChunks[3].toBase58()]);
});

test('undelegatePart stays open to any payer, carries no authority account, keeps chunk 0 writable and ends with the read-only Instructions sysvar', () => {
  const [ix] = chain.undelegatePart({ payer: crank, targets: [NATION_TARGET + 1, NATION_TARGET + 2] });
  const keys = keysOf(ix);
  assert.equal(keys.length, 4 + 2 + 1);
  assert.deepEqual(keys[3], [chain.worldChunks[0].toBase58(), true, false], 'chunk 0 counts the steps');
  assert.deepEqual(keys.slice(4, 6).map(k => k[0]), [chain.nation(1).toBase58(), chain.nation(2).toBase58()]);
  assert.deepEqual(keys.at(-1), [SYSVAR_INSTRUCTIONS_PUBKEY.toBase58(), false, false]);
  assert.equal(keys.filter(k => k[2]).length, 1, 'the payer is the only signer');
  // Chunk 0 alone: no other target account.
  assert.equal(chain.undelegatePart({ payer: crank, targets: [0] })[0].keys.length, 5);
});

test('undelegatePart: targets already back on base (`gone`) are passed read-only; chunk 0 stays writable', () => {
  const [ix] = chain.undelegatePart({ payer: crank, targets: [NATION_TARGET, NATION_TARGET + 1, NATION_TARGET + 2], gone: [NATION_TARGET + 1] });
  assert.deepEqual(keysOf(ix).slice(4, 7).map(k => k.slice(0, 2)),
    [[chain.nation(0).toBase58(), true], [chain.nation(1).toBase58(), false], [chain.nation(2).toBase58(), true]]);
  const [c0] = chain.undelegatePart({ payer: crank, targets: [0], gone: [0] });
  assert.equal(c0.keys[3].isWritable, true, 'chunk 0 is never skipped');
});
