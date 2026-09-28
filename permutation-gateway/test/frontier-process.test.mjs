// The Frontier relay as a process (W6-C, DECISIONS O11: W5-C's process-level
// smoke of the relay and the JS SDK): `node src/frontier/server.mjs` started
// as a child on 127.0.0.1:0 against a scripted JSON-RPC chain (also on
// 127.0.0.1:0), answering its read routes with what the JS SDK computes from
// the same Season bytes, refusing the operator routes without the token, and
// stopping cleanly on SIGTERM. No other network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Keypair } from '@solana/web3.js';
import { FrontierAddresses } from '../client/src/frontier/addresses.mjs';
import { decodeAccount, encodeAccount } from '../client/src/frontier/codec.mjs';
import { seasonTipMin, tipPresets } from '../client/src/frontier/fees.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SERVER = path.join(HERE, '../src/frontier/server.mjs');
const PROGRAM = Keypair.generate().publicKey.toBase58();
const SEASON_ID = 7n;
const A = new FrontierAddresses({ programId: PROGRAM, seasonId: SEASON_ID });
const SEASON = encodeAccount('Season', {
  SEASON_ID, STATUS: 2, GENESIS_TS: 1_800_000_000n, MARCH_FEE: 10_000n, SEAL_BOND: 20_000n, MIN_REVEAL_PRIORITY_MILLI: 433,
  REVEAL_CU_LIMIT: 26_500, REVEAL_LOADED_LIMIT: 1_146_880, JOIN_GATE: new Uint8Array(32),
});

/** A JSON-RPC server with the Season account and nothing else. */
function scriptedRpc() {
  const calls = [];
  const account = key => key === A.season.toString()
    ? { data: [Buffer.from(SEASON).toString('base64'), 'base64'], executable: false, lamports: 1_000_000_000, owner: PROGRAM, rentEpoch: 0, space: SEASON.length }
    : null;
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', c => { body += c; });
    req.on('end', () => {
      const one = m => {
        calls.push(m.method);
        const ctx = { context: { slot: 42 } };
        switch (m.method) {
          case 'getAccountInfo': return { ...ctx, value: account(m.params[0]) };
          case 'getMultipleAccounts': return { ...ctx, value: m.params[0].map(account) };
          case 'getSignatureStatuses': return { ...ctx, value: m.params[0].map(() => null) };
          case 'getBlockHeight': return 100;
          case 'getSlot': return 42;
          default: return undefined;
        }
      };
      const reply = m => {
        const result = one(m);
        return result === undefined
          ? { jsonrpc: '2.0', id: m.id, error: { code: -32601, message: `not scripted: ${m.method}` } }
          : { jsonrpc: '2.0', id: m.id, result };
      };
      const msg = JSON.parse(body);
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(Array.isArray(msg) ? msg.map(reply) : reply(msg)));
    });
  });
  return new Promise(r => server.listen(0, '127.0.0.1', () => r({ server, calls, url: `http://127.0.0.1:${server.address().port}` })));
}

function startRelay(rpc, dir) {
  const child = spawn(process.execPath, [SERVER,
    '--program', PROGRAM, '--season', SEASON_ID.toString(), '--rpc', rpc,
    '--port', '0', '--public-port', '0', '--dev', '--pool-size', '4',
    '--master-seed-file', path.join(dir, 'seed'), '--invite-secret-file', path.join(dir, 'invite'),
    '--state-file', path.join(dir, 'state.json'), '--herald', 'http://127.0.0.1:41040'],
  { env: { ...process.env, FRONTIER_OPERATOR_TOKEN: 'op-token' }, stdio: ['ignore', 'pipe', 'pipe'] });
  let out = '';
  child.stderr.on('data', c => { out += c; });
  const port = new Promise((resolve, reject) => {
    child.stdout.on('data', c => {
      out += c;
      const m = /operator 127\.0\.0\.1:(\d+)/.exec(out);
      if (m) resolve(Number(m[1]));
    });
    child.on('exit', code => reject(new Error(`relay exited ${code}: ${out}`)));
  });
  const exited = new Promise(r => child.on('exit', (code, signal) => r({ code, signal })));
  return { child, port, exited, log: () => out };
}

test('the relay runs as a process and answers what the JS SDK computes', async t => {
  const dir = mkdtempSync(path.join(tmpdir(), 'frontier-relay-proc-'));
  const rpc = await scriptedRpc();
  const relay = startRelay(rpc.url, dir);
  t.after(() => {
    relay.child.kill('SIGKILL');
    rpc.server.close();
    rmSync(dir, { recursive: true, force: true });
  });
  const port = await relay.port;
  assert.ok(port > 0 && port !== 41030, 'an ephemeral operator port');
  const base = `http://127.0.0.1:${port}`;

  const r = await fetch(`${base}/f/season`);
  assert.equal(r.status, 200);
  const s = await r.json();
  const season = decodeAccount('Season', SEASON);
  const tipMin = seasonTipMin(season);
  assert.equal(s.programId, PROGRAM);
  assert.equal(s.season, A.season.toString(), 'the SDK derives the same Season address');
  assert.equal(s.seasonId, SEASON_ID.toString());
  assert.equal(s.relayPool, 4);
  assert.equal(s.tipMin, tipMin.toString());
  assert.deepEqual(s.tipPresets, tipPresets(tipMin).map(String));
  assert.equal(s.marchFee, '10000');
  assert.equal(s.inviteRequired, false);
  assert.equal(s.heraldUrl, 'http://127.0.0.1:41040');
  assert.ok(rpc.calls.includes('getAccountInfo') || rpc.calls.includes('getMultipleAccounts'), 'read from the chain');

  const sig = Buffer.alloc(64, 7);
  const bs58 = (await import('../client/src/base58.mjs')).encode;
  const tx = await fetch(`${base}/f/tx/${bs58(sig)}`);
  assert.equal(tx.status, 200);
  assert.equal((await tx.json()).state, 'unknown');

  const op = await fetch(`${base}/f/operator/pool`);
  assert.equal(op.status, 403, 'operator routes need the token');
  const bad = await fetch(`${base}/f/quota?citizen=nope`);
  assert.equal(bad.status, 400);

  relay.child.kill('SIGTERM');
  const { code } = await relay.exited;
  assert.equal(code, 0, `a clean stop on SIGTERM: ${relay.log()}`);
});
