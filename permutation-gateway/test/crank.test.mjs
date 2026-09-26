import { test } from 'node:test';
import assert from 'node:assert/strict';
import { countSeals, commitDue, DEADLINE_GRACE_S, historyEntry, INTENT_GROUP, intentGroups, MAX_INTENT_TARGETS, nextStep, SPONSORED_COMMITS } from '../src/crank.mjs';
import { MAX_NATIONS, NATION_TARGET, WORLD_CHUNKS } from '../client/src/codec.mjs';

test('intentGroups: every target once, chunk 0 last, each group small and within the program limit', () => {
  for (let nations = 1; nations <= MAX_NATIONS; nations++) {
    const groups = intentGroups({ nations });
    const flat = groups.flat();
    const expected = [...Array.from({ length: WORLD_CHUNKS }, (_, k) => k), ...Array.from({ length: nations }, (_, c) => NATION_TARGET + c)];
    assert.deepEqual([...flat].sort((a, b) => a - b), expected, `nations ${nations}`);
    assert.equal(new Set(flat).size, flat.length, 'no target twice');
    assert.deepEqual(groups.at(-1), [0], 'chunk 0 alone and last');
    for (const g of groups) {
      assert.ok(g.length >= 1 && g.length <= INTENT_GROUP && g.length <= MAX_INTENT_TARGETS);
      assert.ok(g.every(t => t >= NATION_TARGET) || g.length === 1, 'world chunks go alone');
    }
  }
  assert.deepEqual(intentGroups({ nations: 2, chunks: 3, groupSize: 5 }), [[1000, 1001], [1], [2], [0]]);
});

test('countSeals counts commitments and reveals for each nation\'s open tick only', () => {
  const x = (openTick, committed, submitted) => ({ openTick, committed, submitted });
  assert.deepEqual(countSeals([x(4, [4, 3, 4, 65535], [4, 65535, 65535, 65535]), null, x(4, [4, 4, 4, 4], [4, 4, 3, 4])]), { committed: 6, revealed: 4 });
});

test('nextStep: close after the deadline plus grace (never early); publish once every commitment is revealed or the reveal window ended', () => {
  const commit = { frozen: false, revealing: false, deadline: 1000 };
  const base = { meta: commit, openTick: 3, committed: 5, revealed: 0, delegatedAt: 0, tickSeconds: 30 };
  assert.equal(nextStep({ ...base, now: 1000_000 }), null, 'at the deadline, within the grace');
  assert.equal(nextStep({ ...base, now: (1000 + DEADLINE_GRACE_S) * 1000 }), 'close');
  assert.equal(nextStep({ ...base, committed: 24, now: 0 }), null, 'every office committed: members without office still have the tick');
  const reveal = { ...commit, revealing: true, deadline: 2000 };
  assert.equal(nextStep({ ...base, meta: reveal, committed: 5, revealed: 3, now: 1500_000 }), null, 'reveals outstanding');
  assert.equal(nextStep({ ...base, meta: reveal, committed: 5, revealed: 5, now: 1500_000 }), 'publish', 'everything revealed');
  assert.equal(nextStep({ ...base, meta: reveal, committed: 5, revealed: 3, now: (2000 + DEADLINE_GRACE_S) * 1000 }), 'publish', 'window over');
  assert.equal(nextStep({ ...base, meta: { ...commit, frozen: true }, now: 0 }), 'publish');
  const delegatedAt = 2000_000;
  assert.equal(nextStep({ ...base, openTick: 0, delegatedAt, now: delegatedAt + 29_000 }), null, 'tick 0: deadline passed, but delegation was just now');
  assert.equal(nextStep({ ...base, openTick: 0, delegatedAt, now: delegatedAt + 30_000 }), 'close');
});

test('commitDue: every commitEvery ticks, keeping one sponsored commit for undelegation', () => {
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: 0 }), true);
  assert.equal(commitDue({ open: 20, commitEvery: 20, commits: 0 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 0 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: SPONSORED_COMMITS - 1 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: SPONSORED_COMMITS - 2 }), true);
});

test('a PS_HISTORY record is kept as JSON (u64s and bytes survive the state file)', () => {
  const h = {
    prevHistoryRoot: new Uint8Array(32), historyRoot: new Uint8Array(32).fill(0xab),
    record: { finalRoot: new Uint8Array(32).fill(1), nations: [{ points: 2n ** 60n, era: 3, tiers: [1, 2, 3, 4], share: 5n, cities: 2, members: 1 }], cities: [], ruins: [] },
  };
  const e = historyEntry(h, 'sig');
  assert.deepEqual(JSON.parse(JSON.stringify(e)), e);
  assert.equal(e.historyRoot, 'ab'.repeat(32));
  assert.equal(e.record.nations[0].points, (2n ** 60n).toString());
  assert.equal(e.record.finalRoot, '01'.repeat(32));
  assert.equal(e.signature, 'sig');
});

test('Crank.refresh: concurrent callers share one read of the world', async () => {
  const { mkdtempSync } = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { Crank } = await import('../src/crank.mjs');
  const { fakeConnection, keyring, programId, seasonData } = await import('./gateway-fixtures.mjs');
  const { ChainClient } = await import('../client/src/chain.mjs');
  const chain = new ChainClient(programId, 42n);
  const base = fakeConnection();
  base.accounts.set(chain.season.toBase58(), { data: seasonData({ seasonId: 42n }) });
  let reads = 0;
  base.getMultipleAccountsInfoAndContext = async keys => { reads++; await new Promise(r => setTimeout(r, 5)); return { context: { slot: 1 }, value: keys.map(() => null) }; };
  const crank = new Crank({ base, er: fakeConnection(), cfg: { programId }, store: { state: { seasonId: '42', members: [] }, save() {} }, keys: keyring(),
    ticksDir: mkdtempSync(path.join(os.tmpdir(), 'ticks-')) });
  const snaps = await Promise.all([crank.refresh(), crank.refresh(), crank.refresh()]);
  assert.deepEqual([reads, snaps], [1, [null, null, null]]);
  await crank.refresh();
  assert.equal(reads, 2, 'a later call reads again');
});
