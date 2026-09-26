// scripts/x402-check.mjs against the public listener (in this process, on a
// random local port): its ~14 POST /x402/join exceed the per-address limit,
// and it waits the limit out instead of failing; every check passes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PublicKey } from '@solana/web3.js';
import { decodeSeason } from '../client/src/codec.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { decodeRegister } from '../src/routes/x402.mjs';
import { gateway, memberData, programId } from './gateway-fixtures.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('x402-check.mjs passes against the public listener, waiting out its per-address x402 limit', { timeout: 120_000 }, async () => {
  const REG = { seconds: 600, openedAt: Date.now() - 1000, closesAt: Date.now() + 600_000, waitExternal: 0, entryFee: '10000000', deposit: '0' };
  const g = gateway({ season: { aiCount: 4 }, state: { registration: REG } });
  let index = 3;
  const send = g.base.sendRawTransaction;
  g.base.sendRawTransaction = async raw => {
    const p = parseTransaction(new Uint8Array(raw));
    const ix = p.instructions.at(-1);
    const reg = ix.programId === programId ? decodeRegister(ix.data) : null;
    if (reg) {
      // Settles: the member account, and the fee in the pool (80%) and operations (20%).
      const data = memberData({ seasonId: 42n, index, civ: reg.civ, wallet: ix.keys[0].pubkey, session: reg.session, kind: reg.kind, name: reg.name });
      g.base.accounts.set(g.chain.member(new PublicKey(ix.keys[0].pubkey)).toBase58(), { data });
      g.base.members.push(data);
      const s = decodeSeason(g.base.accounts.get(g.chain.season.toBase58()).data);
      index++;
      g.setSeason({ memberCount: s.memberCount + 1, pool: s.pool + (s.entryFee * 4n) / 5n, ops: s.ops + s.entryFee / 5n, aiCount: 4 });
    }
    return send(raw);
  };
  const server = await new Promise(resolve => { const s = http.createServer(g.public).listen(0, '127.0.0.1', () => resolve(s)); });
  try {
    const out = await new Promise((resolve, reject) => {
      const child = spawn(process.execPath, ['scripts/x402-check.mjs', '--gateway', `http://127.0.0.1:${server.address().port}`], { cwd: ROOT });
      let text = '';
      child.stdout.on('data', d => { text += d; });
      child.stderr.on('data', d => { text += d; });
      child.on('error', reject);
      child.on('close', code => resolve({ code, text }));
    });
    assert.equal(out.code, 0, out.text);
    assert.match(out.text, /rate limited by the public listener/);
    assert.match(out.text, /409 KindHidden/);
    assert.match(out.text, /409 UniformRegistration/);
    assert.match(out.text, /honest payment settles/);
    assert.match(out.text, /409 SessionInUse/);
  } finally {
    await new Promise(r => server.close(r));
  }
});
