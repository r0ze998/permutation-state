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
  for (const name of ['WORLD_CHUNKS', 'CHUNK', 'WORLD_HEADER', 'NATION_TARGET', 'INPUT_CHUNK', 'MAX_NATIONS', 'MAX_NAME', 'MAX_MEMBERS', 'MAX_GOV_PER_SIGNER', 'BATCH_BYTES', 'MAX_AI',
    'ROSTER_GRACE_SECONDS', 'WORLD_META_SPACE', 'WORLD_BODY_MAX', 'REVEAL_ROOM', 'MAX_REVEAL_BYTES', 'NATION_HEAD_LEN', 'NATION_BASE_LEN', 'MAX_NATIONS_PER_INTENT',
    // Governance slots and member caps (WP03, WP07).
    'MAX_GOV_ACTION_BYTES', 'GOV_SLOT_BYTES', 'GOV_SLOTS_PER_TICK', 'GOV_QUOTA_MIN', 'GOV_QUOTA_MAX', 'SEASON_MEMBER_CAP', 'NATION_MEMBER_CAP',
    // Batch caps and value bounds (WP04, WP08).
    'MAX_BATCH_ORDERS', 'MAX_FREE_ORDERS', 'MAX_TRADE_AMOUNT', 'MAX_ORDER_COORD', 'DEGRADED',
    // Presets and genesis (WP13).
    'PRESET_BLITZ', 'MIN_NATIONS', 'MAX_GENESIS_WORK', 'TICKS_PER_SEASON',
    // USDC and deadlines (WP12, WP14, WP01, WP11).
    'USDC_DECIMALS', 'ABORT_GRACE_SECONDS', 'FINISH_GRACE_SECONDS', 'MAX_REGISTRATION_SECONDS', 'MAX_TICK_SECONDS', 'TICK_OVERHEAD_SECONDS',
    'TAKEOVER_SECONDS', 'TICK0_GRACE_SECONDS', 'VRF_RETRY_SECONDS', 'VRF_GIVEUP_SECONDS', 'SEED_RETRY_SECONDS',
    // Randomness (WP11) and the program id.
    'VRF_PROGRAM_ID', 'VRF_QUEUE_BASE', 'VRF_QUEUE_ER', 'CANONICAL_PROGRAM_ID']) {
    assert.equal(codec[name], c[name], name);
  }
  // USDC amounts are u64 (bigints here, decimal strings in the vectors).
  for (const name of ['MAX_ENTRY_FEE', 'MAX_DEPOSIT', 'MAX_BOUNTY', 'MAX_BOND', 'EXCHANGE_MAX_PRICE']) {
    assert.equal(typeof codec[name], 'bigint', name);
    assert.equal(codec[name].toString(), c[name], name);
  }
  assert.ok(codec.MAX_BOND >= 2n * BigInt(codec.MAX_AI) * codec.MAX_ENTRY_FEE, 'the default bond (2 × fee per AI) stays under MAX_BOND');
  assert.deepEqual({ ...codec.RAND }, c.RAND);
  assert.deepEqual({ ...codec.SEED }, c.SEED);
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
