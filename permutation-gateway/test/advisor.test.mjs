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
