// Every constant, magic and name the JS side shares with the Rust crates,
// against the values the Rust code exports (vectors.constants).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as codec from '../client/src/codec.mjs';
import { NATION_TARGET as CHAIN_NATION_TARGET, WORLD_CHUNKS as CHAIN_WORLD_CHUNKS } from '../client/src/chain.mjs';
import { MAX_POLICY, MAX_RATIONALE } from '../client/src/decision.mjs';
import { WORLD_CHUNKS as PDA_WORLD_CHUNKS } from '../client/src/pda.mjs';
import { NATIONS as SEASON_NATIONS } from '../src/season.mjs';
import { vectors } from './vectors.mjs';

const c = vectors.constants;

test('layout sizes and limits match permutation-chain and permutation-server', () => {
  for (const name of ['WORLD_CHUNKS', 'CHUNK', 'WORLD_HEADER', 'NATION_TARGET', 'INPUT_CHUNK', 'MAX_NATIONS', 'MAX_NAME', 'MAX_MEMBERS', 'MAX_GOV_PER_SIGNER', 'BATCH_BYTES']) {
    assert.equal(codec[name], c[name], name);
  }
  assert.equal(MAX_POLICY, c.MAX_POLICY);
  assert.equal(MAX_RATIONALE, c.MAX_RATIONALE);
  // Re-exports are the same values, not copies.
  assert.equal(CHAIN_NATION_TARGET, c.NATION_TARGET);
  assert.equal(CHAIN_WORLD_CHUNKS, c.WORLD_CHUNKS);
  assert.equal(PDA_WORLD_CHUNKS, c.WORLD_CHUNKS);
});

test('PDA seeds and account magics match state.rs', () => {
  assert.deepEqual({ ...codec.SEEDS }, c.SEEDS);
  assert.deepEqual({ ...codec.MAGIC }, c.MAGIC);
});

test('names: nations, offices, season statuses', () => {
  assert.deepEqual([...codec.NATIONS], c.NATIONS);
  assert.deepEqual([...codec.ROLES], c.ROLES);
  assert.deepEqual([...codec.SEASON_STATUS], c.SEASON_STATUS);
  assert.equal(SEASON_NATIONS, codec.NATIONS, 'season.mjs uses the codec list');
  assert.equal(codec.NOBODY, 2 ** 32 - 1);
  assert.equal(codec.MEMBER_KINDS.length, 3);
});
