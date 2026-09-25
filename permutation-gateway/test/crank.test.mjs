import { test } from 'node:test';
import assert from 'node:assert/strict';
import { countSubmitted, commitDue, DEADLINE_GRACE_S, INTENT_GROUP, intentGroups, isDue, MAX_INTENT_TARGETS, SPONSORED_COMMITS } from '../src/crank.mjs';
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

test('countSubmitted counts only batches for each nation\'s open tick', () => {
  assert.equal(countSubmitted([{ openTick: 4, submitted: [4, 3, 4, 65535] }, null, { openTick: 4, submitted: [4, 4, 4, 4] }]), 6);
});

test('isDue: frozen, everyone in, or the deadline plus grace; tick 0 waits one tick after delegation', () => {
  const meta = { frozen: false, deadline: 1000 };
  const base = { meta, openTick: 3, submitted: 5, offices: 24, delegatedAt: 0, tickSeconds: 30 };
  assert.equal(isDue({ ...base, now: 1000_000 }), false, 'at the deadline, within the grace');
  assert.equal(isDue({ ...base, now: (1000 + DEADLINE_GRACE_S) * 1000 }), true);
  assert.equal(isDue({ ...base, now: 0, meta: { ...meta, frozen: true } }), true);
  assert.equal(isDue({ ...base, now: 0, submitted: 24 }), true);
  const delegatedAt = 2000_000;
  assert.equal(isDue({ ...base, openTick: 0, delegatedAt, now: delegatedAt + 29_000 }), false, 'tick 0: deadline passed, but delegation was just now');
  assert.equal(isDue({ ...base, openTick: 0, delegatedAt, now: delegatedAt + 30_000 }), true);
  assert.equal(isDue({ ...base, openTick: 0, delegatedAt, now: delegatedAt, submitted: 24 }), true, 'everyone in: no need to wait');
});

test('commitDue: every commitEvery ticks, keeping one sponsored commit for undelegation', () => {
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: 0 }), true);
  assert.equal(commitDue({ open: 20, commitEvery: 20, commits: 0 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 0 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: SPONSORED_COMMITS - 1 }), false);
  assert.equal(commitDue({ open: 19, commitEvery: 20, commits: SPONSORED_COMMITS - 2 }), true);
});
