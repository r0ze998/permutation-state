// startAndDelegate's delegation step with stubbed connections: it skips
// what is already delegated and only reports success once the ER shows every
// account owned by the program.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PublicKey } from '@solana/web3.js';
import { DELEGATION_PROGRAM_ID } from '@magicblock-labs/ephemeral-rollups-sdk';
import { ChainClient } from '../client/src/chain.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { defaultRoster, delegationTargets, firstVotes, startAndDelegate } from '../src/season.mjs';
import { NATIONS, NOBODY } from '../client/src/codec.mjs';
import { fromHex, vectors } from './vectors.mjs';

const running = Buffer.from(fromHex(vectors.accounts.season.hex));
running[130] = 3; // SeasonStatus::Running: genesis and seating are done
const program = new PublicKey(DEFAULTS.programId);
const cfg = { programId: DEFAULTS.programId, erValidator: DEFAULTS.erValidator };

function setup(erOwner) {
  const saves = [];
  const store = { state: { seasonId: '42', members: [], seating: [] }, save() { saves.push(1); } };
  const base = {
    getAccountInfo: async () => ({ data: running }),
    getMultipleAccountsInfo: async keys => keys.map(() => ({ owner: DELEGATION_PROGRAM_ID })), // all delegated by an earlier attempt
  };
  const er = { getMultipleAccountsInfo: async keys => keys.map((k, i) => ({ owner: erOwner(i) })) };
  return { store, saves, base, er };
}

test('delegation is only marked done once every account is on the ER', async () => {
  const ok = setup(() => program);
  const state = await startAndDelegate({ ...ok, cfg, log: () => {} });
  assert.equal(state.delegated, true);
  assert.ok(state.delegatedAt > 0);
  assert.equal(ok.saves.length, 1);

  const chain = new ChainClient(DEFAULTS.programId, 42n);
  const targets = delegationTargets(chain, NATIONS.length);
  const missing = setup(i => (i === targets.length - 1 ? DELEGATION_PROGRAM_ID : program));
  await assert.rejects(startAndDelegate({ ...missing, cfg, log: () => {}, delegationTimeoutMs: 600 }), new RegExp(`targets ${targets.at(-1)};`));
  assert.equal(missing.store.state.delegated, undefined);
  assert.equal(missing.saves.length, 0);
});

test('the first election: AI members vote for a human candidate of their nation, else themselves where they stand', () => {
  const roster = defaultRoster({ humans: 1, ai: 2, nations: 2 });
  assert.equal(roster.length, 1 + 2 * 2);
  // Member 1 is Aster's first AI (General, Steward): the human (0) stands for both.
  assert.deepEqual(firstVotes(roster, 1), [0, 0, NOBODY, NOBODY]);
  // Member 2 is Aster's second AI (Science, Diplomat).
  assert.deepEqual(firstVotes(roster, 2), [0, 0, 2, 2]);
  // Member 3, Borealis: no human there.
  assert.deepEqual(firstVotes(roster, 3), [3, 3, NOBODY, NOBODY]);
});
