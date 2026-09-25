import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isHeavyError, isTransientRpcError, poll, retry } from '../client/src/retry.mjs';

test('retry: until success, at most `attempts`, only while retryIf holds', async () => {
  let n = 0;
  assert.equal(await retry(async i => { n++; if (i < 2) throw new Error('flaky'); return 'ok'; }, { attempts: 5, delayMs: 0 }), 'ok');
  assert.equal(n, 3);
  n = 0;
  await assert.rejects(retry(async () => { n++; throw new Error('down'); }, { attempts: 3, delayMs: 0 }), /down/);
  assert.equal(n, 3);
  n = 0;
  await assert.rejects(retry(async () => { n++; throw new Error('fatal'); }, { attempts: 5, delayMs: 0, retryIf: e => e.message !== 'fatal' }), /fatal/);
  assert.equal(n, 1);
});

test('retry: backoff grows and is capped; onRetry sees each wait', async () => {
  const waits = [];
  await retry(async i => { if (i < 4) throw new Error('x'); }, { attempts: 5, delayMs: 1, backoff: 2, maxDelayMs: 4, onRetry: (_, __, w) => waits.push(w) });
  assert.deepEqual(waits, [1, 2, 4, 4]);
});

test('poll: first non-null value, errors count as not yet, null when it never comes', async () => {
  let n = 0;
  assert.equal(await poll(async () => (++n < 3 ? null : n), { attempts: 5, delayMs: 0 }), 3);
  assert.equal(await poll(async () => { throw new Error('rpc'); }, { attempts: 3, delayMs: 0 }), null);
  assert.equal(await poll(async () => undefined, { attempts: 2, delayMs: 0 }), null);
});

test('error classes', () => {
  for (const m of ['429 Too Many Requests', 'fetch failed', 'read ECONNRESET', 'request timed out', 'Service Unavailable']) assert.ok(isTransientRpcError(new Error(m)), m);
  for (const m of ['{"InstructionError":[0,{"Custom":27}]}', 'blockhash expired', 'ECONNREFUSED']) assert.ok(!isTransientRpcError(new Error(m)), m);
  assert.ok(isHeavyError({ message: 'resolve: {"InstructionError":[2,"ProgramFailedToComplete"]}' }));
  assert.ok(isHeavyError({ message: 'resolve failed', logs: ['Program x consumed 1400000 of 1400000 compute units', 'exceeded CUs meter at BPF instruction'] }));
  assert.ok(isHeavyError({ message: 'x', logs: ['Error: memory allocation failed, out of memory'] }));
  assert.ok(isHeavyError({ message: '{"InstructionError":[2,"ComputationalBudgetExceeded"]}' }));
  assert.ok(!isHeavyError({ message: '{"Custom":16}' }));
});
