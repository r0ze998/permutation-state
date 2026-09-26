// The SDK and the reference agents register like everyone else (kind 2 in a
// season with AI members, the public deposit, a generated name, 1–2 offices,
// no pre-season votes) and claim without the faucet, against the gateway's
// own public handler on a random local port.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { Keypair, PublicKey } from '@solana/web3.js';
import { MEMBER_NAMES, NOBODY, ROLES, roleMask } from '../client/src/codec.mjs';
import { GameClient } from '../client/src/game.mjs';
import { ASSOCIATED_TOKEN_PROGRAM, ata } from '../client/src/player.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { joinViaX402, randomOffices } from '../client/src/x402-client.mjs';
import { claimPrize, joinSeason } from '../agents/runner.mjs';
import { decodeRegister } from '../src/routes/x402.mjs';
import { gateway, memberData, programId } from './gateway-fixtures.mjs';

const REG = () => ({ seconds: 600, openedAt: Date.now() - 1000, closesAt: Date.now() + 600_000, waitExternal: 0, entryFee: '10000000', deposit: '2500000' });

/** The gateway's public handler (or `listener: 'operator'`, no per-address limits) on a random port; `requests` lists what was asked. Registrations settle (the member account appears). */
async function serve({ listener = 'public', ...o } = {}) {
  const g = gateway({ season: { aiCount: 4, entryFee: 10_000_000n }, state: { registration: REG() }, ...o });
  let index = 7;
  const send = g.base.sendRawTransaction;
  g.base.sendRawTransaction = async raw => {
    const p = parseTransaction(new Uint8Array(raw));
    const ix = p.instructions.at(-1);
    const reg = ix.programId === programId ? decodeRegister(ix.data) : null;
    if (reg) g.base.accounts.set(g.chain.member(new PublicKey(ix.keys[0].pubkey)).toBase58(), { data: memberData({ seasonId: 42n, index: index++, civ: reg.civ, wallet: ix.keys[0].pubkey, session: reg.session, kind: reg.kind, name: reg.name }) });
    return send(raw);
  };
  const requests = [];
  const server = await new Promise(resolve => { const s = http.createServer((req, res) => { requests.push(`${req.method} ${req.url.split('?')[0]}`); g[listener](req, res); }).listen(0, '127.0.0.1', () => resolve(s)); });
  const url = `http://127.0.0.1:${server.address().port}`;
  const registers = () => g.base.sent.map(raw => parseTransaction(raw)).filter(t => t.instructions.length === 1 && t.instructions[0].programId === programId).map(t => decodeRegister(t.instructions[0].data));
  return { g, url, requests, registers, close: () => new Promise(r => server.close(r)) };
}

const offices = mask => ROLES.filter((_, i) => mask & (1 << i));
const generated = name => MEMBER_NAMES.includes(name.replace(/ [A-Z]\.$/, ''));

test('agents/runner joinSeason: the faucet, then x402 with the SDK\'s defaults: kind 2, the public deposit, 1–2 offices, no votes (never kind 1)', async () => {
  const s = await serve();
  try {
    const game = new GameClient({ gateway: s.url });
    const [wallet, session] = [Keypair.generate(), Keypair.generate()];
    const j = await joinSeason({ game, wallet, session, name: 'Gaia', stand: ['Science', 'Diplomat'] });
    assert.equal(j.member, 7);
    assert.equal(j.usdcAccount, ata(wallet.publicKey.toBase58(), s.g.mint.toBase58()));
    const [reg] = s.registers();
    assert.deepEqual([reg.kind, reg.deposit, reg.stand, reg.votes, reg.name], [2, 2_500_000n, roleMask(['Science', 'Diplomat']), [NOBODY, NOBODY, NOBODY, NOBODY], 'Gaia']);
    assert.deepEqual(s.requests, ['POST /faucet', 'POST /x402/join', 'POST /x402/join']);
  } finally {
    await s.close();
  }
});

test('joinViaX402 defaults (nothing passed but the keys and account): a generated name and 1–2 random offices; an explicit value is kept', async () => {
  const s = await serve({ listener: 'operator' }); // several faucet grants from one address
  try {
    const game = new GameClient({ gateway: s.url });
    for (let i = 0; i < 3; i++) {
      const [wallet, session] = [Keypair.generate(), Keypair.generate()];
      const f = await game.faucet(wallet.publicKey);
      await joinViaX402(s.url, { wallet, session, usdcAccount: f.usdcAccount, stand: i === 2 ? [] : undefined });
    }
    for (const reg of s.registers()) {
      assert.ok(generated(reg.name), reg.name);
      assert.ok([1, 2].includes(offices(reg.stand).length), `stand ${reg.stand}`);
      assert.deepEqual([reg.kind, reg.deposit, reg.votes], [2, 2_500_000n, [NOBODY, NOBODY, NOBODY, NOBODY]]);
    }
    // Asked for the old way (kind 1, no deposit, no office): the gateway refuses, before anything is paid.
    const [wallet, session] = [Keypair.generate(), Keypair.generate()];
    const f = await game.faucet(wallet.publicKey);
    await assert.rejects(joinViaX402(s.url, { wallet, session, usdcAccount: f.usdcAccount, kind: 1 }), e => e.status === 409 && e.code === 'KindHidden');
    await assert.rejects(joinViaX402(s.url, { wallet, session, usdcAccount: f.usdcAccount, deposit: 0n }), e => e.status === 409 && e.code === 'UniformRegistration');
    assert.equal(s.registers().length, 3);
  } finally {
    await s.close();
  }
  for (let i = 0; i < 200; i++) {
    const o = randomOffices();
    assert.ok(o.length >= 1 && o.length <= 2 && o.every(r => ROLES.includes(r)) && new Set(o).size === o.length);
  }
});

test('agents/runner claimPrize: never the faucet (closed after registration); into the wallet\'s associated token account, created in the same transaction', async () => {
  const s = await serve({ season: { status: 'Finalized', aiCount: 4 } });
  try {
    const wallet = Keypair.generate();
    s.g.base.accounts.set(s.g.chain.member(wallet.publicKey).toBase58(), { data: memberData({ seasonId: 42n, index: 3, civ: 1, wallet: wallet.publicKey }) });
    const game = new GameClient({ gateway: s.url });
    const logs = [];
    // A member recovered from the chain: member.json has no USDC account.
    const r = await claimPrize({ game, wallet, seatFile: path.join(os.tmpdir(), `no-such-${Date.now()}.json`), log: m => logs.push(m), retryOpts: { attempts: 1 } });
    assert.equal(r?.ok, true, logs.join('\n'));
    assert.ok(!s.requests.includes('POST /faucet'), s.requests.join(', '));
    assert.ok(s.requests.includes('GET /usdc'));
    const sent = parseTransaction(s.g.base.sent.at(-1));
    const dest = ata(wallet.publicKey.toBase58(), s.g.mint.toBase58());
    assert.equal(sent.instructions[0].programId, ASSOCIATED_TOKEN_PROGRAM);
    assert.equal(sent.instructions[1].keys[4].pubkey, dest, 'the Claim pays into it');
  } finally {
    await s.close();
  }
});
