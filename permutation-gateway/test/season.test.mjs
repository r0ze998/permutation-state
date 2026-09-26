// startAndDelegate's delegation step with stubbed connections: it skips
// what is already delegated and only reports success once the ER shows every
// account owned by the program.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, PublicKey } from '@solana/web3.js';
import { DELEGATION_PROGRAM_ID } from '@magicblock-labs/ephemeral-rollups-sdk';
import { ChainClient } from '../client/src/chain.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { aiMembers, bountyAndBond, delegationTargets, duplicateSessions, gatewayHeldMembers, lineageOf, pendingAi, planAi, planChain, randomStand, registrationOf, registrationOpen, respread, startAndDelegate } from '../src/season.mjs';
import { MEMBER_NAMES, NATIONS, ROLES, rosterChain, rosterTag } from '../client/src/codec.mjs';
import { fromHex as bytesOf } from '../client/src/bytes.mjs';
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

// planAi with deterministic keys, bytes and draws.
function plan({ ai = 2, nations = 3, seconds = 600, openedAt = 1_000_000, draws } = {}) {
  const keyring = new Map();
  const keyFor = name => { if (!keyring.has(name)) keyring.set(name, Keypair.generate()); return keyring.get(name); };
  let n = 0;
  const random = len => new Uint8Array(len).fill(++n % 256);
  let i = 0;
  const rand = draws ? () => draws[i++ % draws.length] : Math.random;
  return { entries: planAi({ seasonId: 42n, ai, nations, seconds, openedAt, keyFor, rand, random }), keyFor };
}

test('planAi: every nation gets its AI members, sorted by registration time; pos is that order; times within the window', () => {
  const S = 600, openedAt = 1_000_000;
  const { entries } = plan({ ai: 2, nations: 3, seconds: S, openedAt });
  assert.equal(entries.length, 6);
  assert.deepEqual(entries.map(e => e.pos), [0, 1, 2, 3, 4, 5]);
  assert.deepEqual([0, 1, 2].map(c => entries.filter(e => e.civ === c).length), [2, 2, 2]);
  for (let k = 1; k < entries.length; k++) assert.ok(entries[k].dueAt >= entries[k - 1].dueAt, 'sorted by dueAt');
  for (const e of entries) {
    assert.ok(e.dueAt >= openedAt + 0.05 * S * 1000 && e.dueAt <= openedAt + 0.85 * S * 1000, 'dueAt in [5 %, 85 %] of the window');
    assert.ok(e.fundAt >= Math.max(openedAt, e.dueAt - 180_000) && e.fundAt <= Math.max(openedAt, e.dueAt - 10_000), 'funded 10–180 s before');
    assert.ok(e.stand.length === 1 || e.stand.length === 2, 'stands for 1–2 offices');
    assert.ok(e.stand.every(r => ROLES.includes(r)));
    assert.ok(MEMBER_NAMES.includes(e.name.split(' ')[0]), `a generated name: ${e.name}`);
    assert.equal(e.key, `s42-ai${e.pos}`);
    assert.deepEqual(bytesOf(e.tag), rosterTag(42n, new PublicKey(e.wallet).toBytes(), bytesOf(e.salt)));
  }
  // Dev mode: everyone at once, in a random order (not by nation).
  const dev = plan({ seconds: 0, openedAt }).entries;
  assert.ok(dev.every(e => e.dueAt === openedAt && e.fundAt === openedAt));
});

test('planAi: stands are 1 or 2 offices, both counts drawn', () => {
  const counts = new Set();
  for (let k = 0; k < 40; k++) for (const e of plan({ ai: 1, nations: 6 }).entries) counts.add(e.stand.length);
  assert.deepEqual([...counts].sort(), [1, 2]);
  assert.equal(randomStand(() => 0.1).length, 1);
  assert.equal(randomStand(() => 0.9).length, 2);
});

test('the roster chain is committed in pos order, and aiMembers reveals in pos order whatever the member indices', () => {
  const { entries } = plan({ ai: 2, nations: 3 });
  const committed = planChain(entries);
  assert.deepEqual(committed, rosterChain(entries.map(e => bytesOf(e.tag))));
  // People registered in between: member indices do not follow pos.
  const indices = [7, 2, 11, 0, 5, 9];
  const state = { members: [...entries.map((e, k) => ({ index: indices[k], hosted: 'ai', wallet: e.wallet, salt: e.salt, pos: e.pos })), { index: 1, hosted: 'external' }].reverse() };
  const revealed = aiMembers(state).map(m => rosterTag(42n, new PublicKey(m.wallet).toBytes(), bytesOf(m.salt)));
  assert.deepEqual(rosterChain(revealed), committed);
  // A state from before the plan (no pos): registration order.
  assert.deepEqual(aiMembers({ members: [{ index: 3, hosted: 'ai', salt: 'aa' }, { index: 1, hosted: 'ai', salt: 'bb' }] }).map(m => m.index), [1, 3]);
});

test('respread: overdue AI members get new times in what is left of the window, still in pos order; funding before each', () => {
  const S = 600, openedAt = 1_000_000, closesAt = openedAt + S * 1000;
  const { entries } = plan({ ai: 2, nations: 3, seconds: S, openedAt });
  entries[0].registered = 4; // registered already: untouched
  const kept = entries[0].dueAt;
  const now = entries[3].dueAt + 1; // entries 1..3 are overdue (a restart)
  const n = respread(entries, { now, openedAt, seconds: S, closesAt, rand: Math.random });
  assert.equal(n, 3);
  assert.equal(entries[0].dueAt, kept);
  const pending = entries.slice(1);
  for (let k = 0; k < pending.length; k++) {
    assert.ok(pending[k].dueAt >= now, 'none overdue any more');
    assert.ok(pending[k].dueAt <= Math.max(openedAt + 0.85 * S * 1000, closesAt - 10_000), 'within the window');
    if (k) assert.ok(pending[k].dueAt >= pending[k - 1].dueAt, 'strictly in pos order');
    assert.ok(pending[k].fundAt >= now && pending[k].fundAt <= Math.max(now, pending[k].dueAt - 10_000));
  }
  // Past the window's 85 % point: up to 10 s before it closes.
  const late = plan({ ai: 1, nations: 2, seconds: S, openedAt }).entries;
  const t = openedAt + 0.9 * S * 1000;
  respread(late, { now: t, openedAt, seconds: S, closesAt, rand: () => 0.999 });
  assert.ok(late.every(e => e.dueAt >= t + 3000 && e.dueAt <= closesAt - 10_000));
  assert.equal(respread(late, { now: t, openedAt, seconds: S, closesAt }), 0, 'nothing overdue now');
  assert.equal(respread(plan({ seconds: 0 }).entries, { now: openedAt + 1, openedAt, seconds: 0, closesAt: null }), 0, 'dev mode: no window to spread over');
});

test('registrationOf / registrationOpen: the window from the state; a state from before windows is dev mode', () => {
  const state = { registration: { seconds: 60, openedAt: 1000, closesAt: 61_000, waitExternal: 0, entryFee: '10', deposit: '5' } };
  assert.deepEqual(registrationOf(state), { seconds: 60, openedAt: 1000, closesAt: 61_000, waitExternal: 0, entryFee: 10n, deposit: 5n });
  const season = { status: 'Registering' };
  assert.equal(registrationOpen({ state, season, phase: 'registering', now: 60_999 }), true);
  assert.equal(registrationOpen({ state, season, phase: 'registering', now: 61_000 }), false, 'closed at closesAt');
  assert.equal(registrationOpen({ state, season: { status: 'Genesis' }, phase: 'registering', now: 2000 }), false);
  assert.equal(registrationOpen({ state, season, phase: 'playing', now: 2000 }), false);
  assert.equal(registrationOpen({ state: { ...state, creating: {} }, season, phase: 'registering', now: 2000 }), false, 'not before the season exists');
  const legacy = registrationOf({}, { fallbackOpenedAt: 5 });
  assert.deepEqual([legacy.seconds, legacy.openedAt, legacy.closesAt, legacy.deposit], [0, 5, null, 0n]);
  assert.equal(registrationOpen({ state: {}, season, phase: 'registering', now: 1e15 }), true, 'dev mode: open until the crank starts');
  assert.equal(pendingAi({ aiPlan: [{ registered: 3 }, {}, {}] }), 2);
});

test('duplicateSessions finds members that registered one session key', () => {
  const k = Keypair.generate().publicKey.toBytes(), j = Keypair.generate().publicKey.toBytes();
  assert.deepEqual(duplicateSessions([{ index: 0, session: k }, { index: 1, session: j }, { index: 4, session: k }]), [{ session: new PublicKey(k).toBase58(), members: [0, 4] }]);
  assert.deepEqual(duplicateSessions([{ index: 0, session: k }]), []);
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

test('claim-hosted claims every member whose wallet key the gateway holds: the AI members and, in older state files, the gateway-held human seats; never people\'s own wallets', () => {
  const state = { members: [
    { index: 0, hosted: 'human', key: 0, usdc: 'u0' }, // a seat from before wallets (e.g. .local/devnet-3.json)
    { index: 1, hosted: 'ai', key: 1, usdc: 'u1' },
    { index: 2, hosted: 'external', wallet: 'w2' },
    { index: 3, hosted: 'ai', key: 's42-ai0', usdc: 'u3' },
    { index: 4, hosted: 'external', key: 'k4' }, // joined with its own wallet: never the gateway's to claim
  ] };
  assert.deepEqual(gatewayHeldMembers(state).map(m => m.index), [0, 1, 3]);
  assert.deepEqual(gatewayHeldMembers({ members: [] }), []);
});
