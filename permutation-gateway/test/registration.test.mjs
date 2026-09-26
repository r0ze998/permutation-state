// The registration window run by the crank: the AI members funded and
// registered at their planned times, strictly in roster order, like anyone
// (kind 2, votes nobody, the default deposit, their roster tag), the season
// started when the window closes once every AI member is registered; and
// what /season, /operator/roster and /tick tell about it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Keypair, PublicKey } from '@solana/web3.js';
import { NOBODY, roleMask } from '../client/src/codec.mjs';
import { ata } from '../client/src/player.mjs';
import { parseTransaction } from '../client/src/solana-tx.mjs';
import { Crank } from '../src/crank.mjs';
import { FundsGuard } from '../src/guards.mjs';
import { decodeRegister } from '../src/routes/x402.mjs';
import { planAi } from '../src/season.mjs';
import { call, fakeConnection, gateway, keyring, memberData, programId, seasonData } from './gateway-fixtures.mjs';
import { ChainClient } from '../client/src/chain.mjs';

const S = 600;

/** A crank over a fake base layer, season 42 Registering, with a plan of `ai` × `nations` AI members; the clock is `clock.t`. */
function setup({ ai = 1, nations = 3, seconds = S, deposit = '2500000', memberCount = 0, waitExternal = 0 } = {}) {
  const keys = keyring();
  const clock = { t: 1_000_000_000 };
  const openedAt = clock.t;
  const chain = new ChainClient(programId, 42n);
  const mint = keys('usdc-mint').publicKey;
  const base = fakeConnection();
  let season = { seasonId: 42n, status: 'Registering', aiCount: ai * nations, memberCount, crank: keys('crank').publicKey, usdcMint: mint, entryFee: 10_000_000n };
  const setSeason = over => { season = { ...season, ...over }; base.accounts.set(chain.season.toBase58(), { data: seasonData(season) }); };
  setSeason({});
  let index = memberCount;
  const registered = [];
  base.sendRawTransaction = async raw => {
    base.sent.push(new Uint8Array(raw));
    const p = parseTransaction(new Uint8Array(raw));
    const reg = p.instructions.length === 1 && p.instructions[0].programId === programId ? decodeRegister(p.instructions[0].data) : null;
    if (reg) {
      const wallet = p.instructions[0].keys[0].pubkey;
      base.accounts.set(chain.member(new PublicKey(wallet)).toBase58(), { data: memberData({ seasonId: 42n, index, civ: reg.civ, wallet, session: reg.session, kind: reg.kind, name: reg.name }) });
      registered.push({ index: index++, wallet, reg, tx: p });
      setSeason({ memberCount: index });
    }
    return 'y'.repeat(64);
  };
  const plan = planAi({ seasonId: 42n, ai, nations, seconds, openedAt, keyFor: keys });
  const state = { seasonId: '42', programId, mint: mint.toBase58(), members: [], lineage: [], aiPlan: plan, faucet: {},
    registration: { seconds, openedAt, closesAt: seconds ? openedAt + seconds * 1000 : null, waitExternal, entryFee: '10000000', deposit } };
  const store = { state, saves: 0, save() { this.saves++; } };
  const logs = [];
  const crank = new Crank({ base, er: fakeConnection(), cfg: { programId, entryFee: 10_000_000n }, store, keys, now: () => clock.t, log: m => logs.push(m),
    ticksDir: mkdtempSync(path.join(os.tmpdir(), 'ticks-')) });
  const started = [];
  crank.start = async () => { started.push(clock.t); crank.phase = 'playing'; };
  crank.recheck = { attempts: 1, delayMs: 0 };
  /** Step the crank until time `until` in `stepMs` steps (a failed step is logged, as `step` does). */
  const runUntil = async (until, stepMs = 1000) => {
    while (clock.t < until && crank.phase === 'registering') {
      await crank.registering().catch(e => logs.push(`crank: ${e.message}`));
      clock.t += stepMs;
    }
  };
  return { crank, clock, base, store, plan, registered, started, logs, keys, chain, mint, setSeason, openedAt, runUntil, bump: () => { index++; setSeason({ memberCount: index }); } };
}

test('the crank funds each AI member at its own time, then registers them at theirs, strictly in roster order, like a person', async () => {
  const x = setup({ ai: 2, nations: 3 });
  await x.runUntil(x.openedAt + S * 1000 - 1);
  assert.equal(x.registered.length, 6, x.logs.join('\n'));
  // Strictly in pos order, each not before it was due, each funded before.
  const byWallet = new Map(x.plan.map(e => [e.wallet, e]));
  assert.deepEqual(x.registered.map(r => byWallet.get(r.wallet).pos), [0, 1, 2, 3, 4, 5]);
  const sends = x.base.sent.map(raw => parseTransaction(raw));
  for (const r of x.registered) {
    const e = byWallet.get(r.wallet);
    const fundedAt = sends.findIndex(t => t.instructions.length === 2 && t.instructions[0].keys[2].pubkey === e.wallet);
    const regAt = sends.findIndex(t => t.signers[1] === e.wallet);
    assert.ok(fundedAt >= 0 && fundedAt < regAt, 'funded (by the faucet function) before it registers');
    // Register: kind 2, votes nobody, the default deposit, its roster tag, its offices, from its associated token account; the crank pays.
    assert.deepEqual([r.reg.kind, r.reg.votes, r.reg.deposit, r.reg.stand, r.reg.name, r.reg.civ], [2, [NOBODY, NOBODY, NOBODY, NOBODY], 2_500_000n, roleMask(e.stand), e.name, e.civ]);
    assert.equal(Buffer.from(r.reg.tag).toString('hex'), e.tag);
    assert.equal(new PublicKey(r.reg.session).toBase58(), e.session);
    assert.deepEqual(r.tx.signers, [x.keys('crank').publicKey.toBase58(), e.wallet]);
    assert.equal(r.tx.instructions[0].keys[4].pubkey, ata(e.wallet, x.mint.toBase58()));
    assert.equal(e.registered, r.index);
  }
  assert.deepEqual(x.store.state.members.map(m => [m.index, m.hosted, m.pos]), x.registered.map((r, k) => [r.index, 'ai', k]));
  assert.deepEqual(x.started, [], 'not before the window closes');
  await x.runUntil(x.openedAt + S * 1000 + 2000);
  assert.equal(x.started.length, 1);
  assert.ok(x.started[0] >= x.openedAt + S * 1000, 'at the deadline, however many people joined');
});

test('while the crank is low on SOL the AI members are neither funded nor registered (people\'s joins are paused too); once funded they go on, spread over what is left', async () => {
  const x = setup({ ai: 1, nations: 2 });
  let lamports = 0.1e9;
  x.crank.funds = FundsGuard.forSol({ read: async () => lamports, minSol: 0.3, now: () => x.clock.t, log: m => x.logs.push(m) });
  const last = Math.max(...x.plan.map(e => e.dueAt));
  await x.runUntil(last + 30_000);
  assert.deepEqual([x.base.sent.length, x.registered.length], [0, 0], 'no faucet grant, no Register while low');
  assert.ok(x.logs.some(l => /AI registrations paused/.test(l)), x.logs.join('\n'));
  lamports = 5e9;
  await x.runUntil(x.openedAt + S * 1000 - 1);
  assert.equal(x.registered.length, 2, x.logs.join('\n'));
  assert.deepEqual(x.registered.map(r => r.wallet), [...x.plan].sort((a, b) => a.pos - b.pos).map(e => e.wallet), 'still in roster order');
  assert.ok(x.logs.some(l => /overdue: spread/.test(l)), 'spread again, not registered in a burst');
  assert.ok(x.logs.some(l => /resumed/.test(l)));
});

test('registration is strictly in order: AI k+1 waits for AI k to land, however late', async () => {
  const x = setup({ ai: 1, nations: 2 });
  const [first, second] = x.plan;
  // The first one's registration keeps failing.
  const send = x.base.sendRawTransaction;
  let failing = true;
  x.base.sendRawTransaction = async raw => {
    const p = parseTransaction(new Uint8Array(raw));
    if (failing && p.signers[1] === first.wallet) throw new Error('RPC down');
    return send(raw);
  };
  await x.runUntil(second.dueAt + 15_000);
  assert.equal(x.registered.length, 0, 'the second waits for the first');
  assert.ok(x.logs.some(l => /could not register/.test(l)), 'said loudly');
  failing = false;
  await x.runUntil(x.openedAt + S * 1000 - 1);
  assert.deepEqual(x.registered.map(r => r.wallet), [first.wallet, second.wallet]);
});

test('after a restart the overdue AI members are spread over what is left of the window (not registered in a burst)', async () => {
  const x = setup({ ai: 2, nations: 2 });
  x.clock.t = x.plan[3].dueAt + 30_000; // the gateway was down: every one is overdue
  await x.crank.registering();
  assert.ok(x.logs.some(l => /overdue: spread/.test(l)), x.logs.join('\n'));
  assert.equal(x.registered.length, 0);
  const pending = x.store.state.aiPlan;
  for (let k = 0; k < pending.length; k++) {
    assert.ok(pending[k].dueAt >= x.clock.t + 3000);
    if (k) assert.ok(pending[k].dueAt >= pending[k - 1].dueAt);
  }
  await x.runUntil(x.openedAt + S * 1000 - 1);
  assert.equal(x.registered.length, 4);
  const times = x.registered.map((r, k) => x.store.state.aiPlan[k].dueAt);
  assert.ok(times.every((t, k) => !k || t - times[k - 1] >= 0));
});

test('the season does not start before every AI member is registered, nor while a registration is in flight', async () => {
  const x = setup({ ai: 1, nations: 1 });
  x.clock.t = x.openedAt + S * 1000 + 1; // closed, the AI member not registered yet (it is registered first, then the season starts)
  x.store.state.aiPlan[0].dueAt = x.clock.t - 1000; // due, not overdue enough to spread again
  await x.runUntil(x.clock.t + 10_000);
  assert.equal(x.registered.length, 1);
  assert.equal(x.started.length, 1);
  assert.ok(x.logs.some(l => /registration closed, but 1 AI members are not registered yet/.test(l)));
  const y = setup({ ai: 0, nations: 1 });
  y.clock.t = y.openedAt + S * 1000;
  const release = y.crank.desk.reserve({ wallet: 'w', session: 's' });
  await y.crank.registering();
  assert.equal(y.started.length, 0, 'a person\'s registration is being sent');
  release();
  await y.crank.registering();
  assert.equal(y.started.length, 1);
});

test('dev mode (no window): start once enough members besides the AI members joined', async () => {
  const x = setup({ ai: 0, nations: 1, seconds: 0, waitExternal: 2 });
  await x.crank.registering();
  x.bump();
  await x.crank.registering();
  assert.equal(x.started.length, 0);
  x.bump();
  await x.crank.registering();
  assert.equal(x.started.length, 1);
});

test('an AI member whose account exists already (a lost confirmation) is recorded, not registered twice', async () => {
  const x = setup({ ai: 1, nations: 1 });
  const e = x.plan[0];
  x.base.accounts.set(x.chain.member(new PublicKey(e.wallet)).toBase58(), { data: memberData({ seasonId: 42n, index: 7, civ: e.civ, wallet: e.wallet, session: new PublicKey(e.session) }) });
  e.funded = { account: 'x', signature: 'y' };
  x.clock.t = e.dueAt;
  await x.runUntil(e.dueAt + 60_000);
  assert.equal(x.registered.length, 0, 'nothing sent');
  assert.equal(e.registered, 7);
  assert.deepEqual(x.store.state.members.map(m => [m.index, m.hosted, m.pos]), [[7, 'ai', 0]]);
});

test('/season: the registration window, AI members not shown by kind; unregistered AI members appear nowhere', async () => {
  const closesAt = Date.now() + 60_000;
  const reg = { seconds: 600, openedAt: closesAt - 600_000, closesAt, waitExternal: 0, entryFee: '10000000', deposit: '1000000' };
  const people = [{ index: 0, civ: 1, name: 'Ada K.', kind: 'undeclared', attested: false, wallet: Keypair.generate().publicKey.toBase58(), session: 'x', stand: 1, shares: 0n, claimed: false, tag: '00' }];
  const plan = [{ pos: 0, civ: 0, wallet: Keypair.generate().publicKey.toBase58(), session: 'p', salt: 'aa'.repeat(32) }];
  const g = gateway({ season: { aiCount: 1, memberCount: 1, entryFee: 10_000_000n }, state: { registration: reg, aiPlan: plan }, registryMembers: people });
  const r = await call(g.public, 'GET', '/season');
  const { serverNow, ...rest } = r.json.registration;
  assert.ok(Math.abs(serverNow - Date.now()) < 5000);
  assert.deepEqual(rest, { members: 1, aiCount: 1, openedAt: reg.openedAt, closesAt, entryFee: '10000000', deposit: '1000000', open: true });
  assert.deepEqual(r.json.members, [{ index: 0, civ: 1, name: 'Ada K.', wallet: people[0].wallet, session: 'x', stand: 1, shares: '0', claimed: false, tag: '00' }], 'no kind, no attested');
  assert.equal(r.json.devWallet, false);
  const op = await call(g.operator, 'GET', '/operator/roster', { headers: { authorization: 'Bearer op-token' } });
  assert.deepEqual([op.json.members, op.json.ai], [[], []], 'the planned AI member is not a member yet');
  // Without AI members kinds are shown; the dev wallet only on localnet with --dev-wallet.
  const open = gateway({ cfg: { devWallet: true }, registryMembers: people });
  const o = await call(open.public, 'GET', '/season');
  assert.deepEqual([o.json.members[0].kind, o.json.devWallet, o.json.registration.closesAt], ['undeclared', true, null]);
  const devnet = gateway({ cfg: { devWallet: true, cluster: 'devnet' } });
  assert.equal((await call(devnet.public, 'GET', '/season')).json.devWallet, false);
});

test('/tick: per nation, which offices committed and revealed for the open tick', async () => {
  const h = (openTick, committed, submitted) => ({ openTick, committed, submitted });
  const snap = { at: Date.now(), slot: 9, header: { meta: { finished: false, frozen: false, revealing: true, deadline: 5 } },
    nations: [h(4, [4, 3, 4, 65535], [4, 65535, 65535, 65535]), null, h(4, [4, 4, 4, 4], [4, 4, 3, 4])] };
  const g = gateway({ crank: { snapshot: snap } });
  const r = await call(g.public, 'GET', '/tick');
  assert.deepEqual(r.json, { tick: 4, phase: 'reveal', deadline: 5, committed: 6, revealed: 4, slot: 9, nations: [
    { civ: 0, committed: [true, false, true, false], revealed: [true, false, false, false] },
    { civ: 1, committed: [false, false, false, false], revealed: [false, false, false, false] },
    { civ: 2, committed: [true, true, true, true], revealed: [true, true, false, true] },
  ] });
});
