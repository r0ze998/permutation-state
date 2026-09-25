// The account lists of the commit instructions, as the program reads them
// (permutation-chain `instruction.rs`): CommitPart carries a nation account
// after world chunk 0, from which the program reads the crank's key.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { NATION_TARGET } from '../client/src/codec.mjs';
import { DEFAULTS } from '../src/config.mjs';

const chain = new ChainClient(DEFAULTS.programId, 1_790_000_000_123n);
const crank = Keypair.generate().publicKey;

test('commitPart passes nation 0 (read-only) after chunk 0, then the targets', () => {
  const [ix] = chain.commitPart({ payer: crank, targets: [NATION_TARGET + 1, 3] });
  const keys = ix.keys.map(k => [k.pubkey.toBase58(), k.isWritable, k.isSigner]);
  assert.deepEqual(keys[0], [crank.toBase58(), true, true]);
  assert.deepEqual(keys[3], [chain.worldChunks[0].toBase58(), true, false]);
  assert.deepEqual(keys[4], [chain.nation(0).toBase58(), false, false]);
  assert.deepEqual(keys.slice(5).map(k => k[0]), [chain.nation(1).toBase58(), chain.worldChunks[3].toBase58()]);
});

test('undelegatePart stays open to any payer and carries no authority account', () => {
  const [ix] = chain.undelegatePart({ payer: crank, targets: [NATION_TARGET + 1, 3] });
  assert.equal(ix.keys.length, 4 + 2);
  assert.deepEqual(ix.keys.slice(4).map(k => k.pubkey.toBase58()), [chain.nation(1).toBase58(), chain.worldChunks[3].toBase58()]);
});
