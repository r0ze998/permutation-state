import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Advisor } from '../src/advisor.mjs';

const draft = { incoming: 'Will you make peace?', aggression: 20, greed: 50, loyalty: 70, openness: 80, price: 2.5 };

test('without a key the game server\'s sentence is used', async () => {
  const a = new Advisor({ key: null });
  assert.equal(a.available, false);
  assert.equal(await a.phrase({ fallback: 'Offer a contract.', draft, nation: 'Aster' }), 'Offer a contract.');
});

test('a model answer is used, within the budget; errors fall back', async () => {
  let calls = 0;
  const ok = async (url, init) => {
    calls++;
    const body = JSON.parse(init.body);
    assert.match(body.system, /Never claim to be a person/);
    return { ok: true, json: async () => ({ content: [{ type: 'text', text: 'A contract of 2.5 USDC and we have peace.' }] }) };
  };
  const a = new Advisor({ key: 'k', budget: 1, fetchImpl: ok });
  assert.equal(await a.phrase({ fallback: 'x', draft, nation: 'Aster' }), 'A contract of 2.5 USDC and we have peace.');
  assert.equal(await a.phrase({ fallback: 'x', draft, nation: 'Aster' }), 'x', 'over the budget');
  assert.equal(calls, 1);
  const failing = new Advisor({ key: 'k', fetchImpl: async () => ({ ok: false }) });
  assert.equal(await failing.phrase({ fallback: 'y', draft, nation: 'Aster' }), 'y');
});

test('without a draft the game server\'s sentence is used and the model is not called', async () => {
  let calls = 0;
  const a = new Advisor({ key: 'k', fetchImpl: async () => { calls++; throw new Error('not called'); } });
  assert.equal(await a.phrase({ fallback: 'Offer a contract.', draft: undefined, nation: 'Aster' }), 'Offer a contract.');
  assert.deepEqual([calls, a.used], [0, 0]);
});

test('a model slower than the timeout is aborted (through the fetch signal) and falls back', async () => {
  let signal;
  const slow = (url, init) => {
    signal = init.signal;
    return new Promise((_, reject) => init.signal.addEventListener('abort', () => reject(init.signal.reason)));
  };
  const a = new Advisor({ key: 'k', timeoutMs: 20, fetchImpl: slow });
  // AbortSignal.timeout's timer does not keep the process alive (as the server does).
  const alive = setTimeout(() => {}, 5000);
  assert.equal(await a.phrase({ fallback: 'z', draft, nation: 'Aster' }), 'z');
  clearTimeout(alive);
  assert.equal(signal.aborted, true);
});
