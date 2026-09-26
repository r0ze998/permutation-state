// The gateway's listeners: both ports are checked free before anything
// happens on chain (a gateway that cannot serve must not create a season),
// and a port in use is a ConfigError that says which flag to change.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdtempSync, existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ConfigError } from '../src/config.mjs';
import { main, probePorts } from '../src/server.mjs';

const hold = () => new Promise(resolve => { const s = http.createServer().listen(0, '127.0.0.1', () => resolve(s)); });
const freePort = async () => { const s = await hold(); const { port } = s.address(); await new Promise(r => s.close(r)); return port; };

test('probePorts: a busy operator or public port is a ConfigError naming the flag; free ones pass and stay free', async () => {
  const busy = await hold();
  const taken = busy.address().port;
  try {
    const free = await freePort();
    await assert.rejects(probePorts({ port: taken, publicPort: 0 }), e => e instanceof ConfigError && /operator port 127\.0\.0\.1:\d+ is in use/.test(e.message) && /--port/.test(e.message));
    await assert.rejects(probePorts({ port: free, publicPort: taken, publicHost: '127.0.0.1' }),
      e => e instanceof ConfigError && /public port 127\.0\.0\.1:\d+ is in use/.test(e.message) && /--public-port/.test(e.message));
    await probePorts({ port: free, publicPort: 0 });
    // Released again: the real listener can take it.
    const again = http.createServer();
    await new Promise((resolve, reject) => { again.once('error', reject); again.listen(free, '127.0.0.1', resolve); });
    await new Promise(r => again.close(r));
  } finally {
    busy.close();
  }
});

test('main: a public port in use stops the gateway before it creates a season (nothing on chain, no state file)', async () => {
  const busy = await hold();
  const dir = mkdtempSync(path.join(os.tmpdir(), 'gw-'));
  const state = path.join(dir, 'season.json');
  const saved = process.env.PS_OPERATOR_TOKEN;
  process.env.PS_OPERATOR_TOKEN = 'test-token';
  try {
    const port = await freePort();
    // An unreachable RPC: had it got that far, bootstrap would fail differently (and later).
    await assert.rejects(main(['--new-season', '--port', String(port), '--public-port', String(busy.address().port), '--state', state, '--base', 'http://127.0.0.1:9', '--er', 'http://127.0.0.1:9']),
      e => e instanceof ConfigError && /public port/.test(e.message));
    assert.equal(existsSync(state), false, 'no season was created');
  } finally {
    if (saved === undefined) delete process.env.PS_OPERATOR_TOKEN; else process.env.PS_OPERATOR_TOKEN = saved;
    busy.close();
  }
});
