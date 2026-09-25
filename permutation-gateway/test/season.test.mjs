// startAndDelegate's delegation step with stubbed connections: it skips
// what is already delegated and only reports success once the ER shows every
// account owned by the program.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, PublicKey } from '@solana/web3.js';
import { DELEGATION_PROGRAM_ID } from '@magicblock-labs/ephemeral-rollups-sdk';
import { ChainClient } from '../client/src/chain.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { aiMembers, bountyAndBond, defaultRoster, delegationTargets, drawSeats, firstVotes, lineageOf, startAndDelegate } from '../src/season.mjs';
import { NATIONS, NOBODY, rosterChain, rosterTag } from '../client/src/codec.mjs';
import { fromHex as bytesOf, toHex } from '../client/src/bytes.mjs';
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

// drawSeats with deterministic keys and bytes: a key per name, and a counter for random bytes.
function draw(roster, { names = ['Aoi', 'Ren', 'Mika'] } = {}) {
  const keyring = new Map();
  const keyFor = name => { if (!keyring.has(name)) keyring.set(name, Keypair.fromSeed(new Uint8Array(32).fill(keyring.size + 1))); return keyring.get(name); };
  let n = 0;
  const random = len => new Uint8Array(len).fill(++n);
  return drawSeats(roster, 42n, { names, keyFor, random });
}

test('drawSeats: only AI members get salts; each AI tag is its roster tag; nobody declares a kind', () => {
  const seats = draw(defaultRoster({ humans: 2, ai: 1, nations: 2 }));
  assert.deepEqual(seats.map(s => [s.hosted, !!s.salt]), [['human', false], ['human', false], ['ai', true], ['ai', true]]);
  for (const s of seats) {
    assert.equal(s.kind, 2);
    assert.equal(s.tag.length, 32);
    if (s.salt) assert.deepEqual(s.tag, rosterTag(42n, s.keys.wallet.publicKey.toBytes(), s.salt));
  }
  // Keys are per season and seat.
  assert.equal(new Set(seats.map(s => s.keys.wallet.publicKey.toBase58())).size, seats.length);
});

test('drawSeats: the roster chain committed over the AI tags in seat order is the one the reveal (registration order) rebuilds', () => {
  const seats = draw(defaultRoster({ humans: 1, ai: 2, nations: 3 }));
  const committed = rosterChain(seats.filter(s => s.salt).map(s => s.tag));
  // Registration indices are the seat positions; the state keeps wallet and salt.
  const state = { members: seats.map((s, index) => ({ index, wallet: s.keys.wallet.publicKey.toBase58(), ...(s.salt ? { salt: toHex(s.salt) } : {}) })).reverse() };
  const revealed = aiMembers(state).map(m => rosterTag(42n, new PublicKey(m.wallet).toBytes(), bytesOf(m.salt)));
  assert.deepEqual(rosterChain(revealed), committed);
});

test('drawSeats: names get a numeric suffix beyond the pool size', () => {
  const seats = draw(defaultRoster({ humans: 0, ai: 4, nations: 2 }), { names: ['Aoi', 'Ren', 'Mika'] });
  assert.deepEqual(seats.map(s => s.name), ['Aoi', 'Ren', 'Mika', 'Aoi 2', 'Ren 2', 'Mika 2', 'Aoi 3', 'Ren 3']);
});

test('bountyAndBond: the bond defaults to AI members × entry fee × 2; none without AI members', () => {
  assert.deepEqual(bountyAndBond({ aiCount: 12, bounty: 5_000_000n, bond: null, entryFee: 1_000_000n }), { bountyEach: 5_000_000n, bond: 24_000_000n });
  assert.deepEqual(bountyAndBond({ aiCount: 12, bounty: 5_000_000n, bond: 7n, entryFee: 1_000_000n }), { bountyEach: 5_000_000n, bond: 7n });
  assert.deepEqual(bountyAndBond({ aiCount: 0, bounty: 5_000_000n, bond: 7n, entryFee: 1_000_000n }), { bountyEach: 0n, bond: 0n });
});

test('lineageOf: the previous season\'s lineage plus its record, the last ten', () => {
  assert.deepEqual(lineageOf(null), []);
  const history = { historyRoot: 'ab', record: { finalRoot: 'cd' } };
  assert.deepEqual(lineageOf({ seasonId: '7', nations: ['A'], history }), [{ seasonId: '7', nations: ['A'], historyRoot: 'ab', record: { finalRoot: 'cd' } }]);
  const older = Array.from({ length: 10 }, (_, i) => ({ seasonId: String(i) }));
  const l = lineageOf({ seasonId: '10', nations: [], history, lineage: older });
  assert.deepEqual([l.length, l[0].seasonId, l.at(-1).seasonId], [10, '1', '10']);
  assert.deepEqual(lineageOf({ seasonId: '8', lineage: older.slice(0, 2) }), older.slice(0, 2), 'no record yet: only the lineage');
});
