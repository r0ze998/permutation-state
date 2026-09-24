import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { commit, decisionDigest, clip } from '../client/src/decision.mjs';

// Records the Rust server verified with permutation-rules::decision::verify_reveal
// (a bot's and outside agents'), captured from a live season.
const vectors = JSON.parse(readFileSync(new URL('./decision-vectors.json', import.meta.url), 'utf8'));

test('JS digest equals the engine digest', () => {
  assert.ok(vectors.length > 0);
  for (const v of vectors) assert.equal(decisionDigest(v), v.digest, `tick ${v.tick} ${v.policy}`);
});

test('commit is reproducible from its reveal and binds every part', () => {
  const c = commit({ tick: 7, obsRoot: 'ab'.repeat(32), policy: 'p/v1', text: '理由' });
  assert.equal(decisionDigest(c), c.digest);
  for (const k of ['tick', 'policy', 'text']) assert.notEqual(decisionDigest({ ...c, [k]: k === 'tick' ? 8 : `${c[k]}x` }), c.digest);
  assert.notEqual(decisionDigest({ ...c, salt: '00'.repeat(16) }), c.digest);
});

test('clip keeps UTF-8 characters whole', () => {
  assert.equal(new TextEncoder().encode(clip('あいう', 7)).length, 6);
  assert.equal(clip('abc', 10), 'abc');
});
