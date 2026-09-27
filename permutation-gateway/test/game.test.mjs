// GameClient against a fake game server and gateway (fetch is stubbed).
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, Transaction } from '@solana/web3.js';
import { GameClient, HttpError } from '../client/src/game.mjs';
import { GameError, errorCode } from '../client/src/http.mjs';
import { DEFAULTS } from '../src/config.mjs';

const realFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = realFetch; });

const json = (status, body) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const feePayer = Keypair.generate().publicKey.toBase58();
const blockhash = Keypair.generate().publicKey.toBase58();

/** A fake server + gateway; `relay(i, tx)` answers the i-th POST /relay. Returns the request log. */
function fake({ relay }) {
  const calls = [];
  let relays = 0;
  globalThis.fetch = async (url, init = {}) => {
    const { pathname } = new URL(url);
    const method = init.method ?? 'GET';
    calls.push(`${method} ${pathname}`);
    const body = init.body ? JSON.parse(init.body) : null;
    if (pathname === '/api/validate') return json(200, { ok: true, offices: [], warnings: [] });
    if (pathname === '/season') return json(200, { programId: DEFAULTS.programId, season: { seasonId: '42' } });
    if (pathname === '/relay' && method === 'GET') return json(200, { feePayer, blockhash, lastValidBlockHeight: 99 });
    if (pathname === '/relay') return relay(relays++, Transaction.from(Buffer.from(body.tx, 'base64')), body);
    return json(404, { error: 'not found' });
  };
  return calls;
}

const view = { tick: 5, decision: { obsRoot: 'ab'.repeat(32) }, gov: { offices: [{ role: 'Science', holder: { id: 3 } }, { role: 'Diplomat', holder: { id: 3 } }] } };
const client = () => new GameClient({ member: 3, civ: 1, session: Keypair.generate() });

test('submit sends one batch per office held; a later office failing does not hide the earlier ones', async () => {
  const calls = fake({ relay: (i, tx, body) => {
    assert.equal(body.lastValidBlockHeight, 99, 'the blockhash expiry is passed along');
    assert.equal(tx.feePayer.toBase58(), feePayer);
    return i === 0 ? json(200, { ok: true, signature: 'sig0' }) : json(409, { error: 'TickFrozen: …', code: 'TickFrozen' });
  } });
  const g = client();
  const r = await g.submit({ orders: [{ type: 'SetResearch', techs: ['Writing'] }], policy: 'test/v1', view });
  assert.equal(r.ok, false);
  assert.equal(r.code, 'TickFrozen');
  assert.equal(r.status, 409);
  assert.deepEqual(r.offices.map(o => [o.role, o.signature ?? o.code]), [['Science', 'sig0'], ['Diplomat', 'TickFrozen']]);
  assert.deepEqual([...g.decisions.keys()], ['5:Science'], 'only the relayed office is remembered for its reveal');
  assert.equal(calls.filter(c => c === 'POST /relay').length, 2);
});

test('submit packs every batch first: an oversized one sends nothing', async () => {
  const calls = fake({ relay: () => json(200, { ok: true, signature: 'never' }) });
  const g = client();
  const many = Array.from({ length: 150 }, () => ({ type: 'SendEnvoy', cityState: 1, influence: 5 })); // 7 bytes each
  const r = await g.submit({ orders: [{ type: 'SetResearch', techs: ['Writing'] }, ...many], policy: 'test/v1', view });
  assert.equal(r.ok, false);
  assert.equal(r.code, 'BatchTooLarge');
  assert.match(r.error, /^Diplomat: batch too large/);
  assert.ok(!calls.some(c => c.includes('/relay')), 'no office was relayed');
  assert.equal(g.decisions.size, 0);
});

test('errors carry the gateway code', async () => {
  fake({ relay: () => json(403, { error: 'Unauthorized: …', code: 'Unauthorized' }) });
  const g = client();
  const e = await g.gov({ type: 'Support', proposal: 1 }).catch(x => x);
  assert.ok(e instanceof HttpError);
  assert.equal(e.status, 403);
  assert.equal(e.code, 'Unauthorized');
  assert.equal(errorCode(e), 'Unauthorized');
  const notMember = await new GameClient().submit({ orders: [], policy: 'p', view }).catch(x => x);
  assert.ok(notMember instanceof GameError);
  assert.equal(notMember.code, 'NotAMember');
});

test('propose refuses treasury orders (a rejection) before anything is sent; other proposals go out', async () => {
  const calls = fake({ relay: () => json(200, { ok: true, signature: 'sigP' }) });
  const g = client();
  for (const order of [{ type: 'ExchangeOrder', good: { kind: 'Iron' }, side: 'Buy', amount: 1, price: 1 }, { type: 'ConsentSpend', usdc: 5 },
    { type: 'OfferContract', to: 2, term: { kind: 'Peace' }, usdc: 1, deadline: 9 }]) {
    await assert.rejects(g.propose('Diplomat', [{ type: 'SendEnvoy', cityState: 1, influence: 5 }, order]), /cannot be proposed/, order.type);
  }
  assert.equal(calls.length, 0, 'nothing was requested');
  const r = await g.propose('Diplomat', [{ type: 'SendEnvoy', cityState: 1, influence: 5 }]);
  assert.equal(r.signature, 'sigP');
});

test('submit adopts proposals from the view: their orders count towards the caps; an unknown id sends nothing', async () => {
  const sent = [];
  fake({ relay: (i, tx) => { sent.push(tx); return json(200, { ok: true, signature: `sig${i}` }); } });
  const research = { type: 'SetResearch', techs: ['Writing'] };
  const costed = n => Array.from({ length: n }, (_, i) => ({ type: 'Purchase', city: i, gold: 1 }));
  const withProposals = { ...view, gov: { ...view.gov, proposals: [{ id: 4, role: 'Science', orders: [research] }, { id: 9, role: 'Diplomat', orders: costed(5) }] } };
  const unknown = await client().submit({ orders: [research], policy: 'p/v1', adopt: { Science: [5] }, view: withProposals });
  assert.deepEqual([unknown.ok, unknown.code], [false, 'UnknownProposal']);
  const otherOffice = await client().submit({ orders: [research], policy: 'p/v1', adopt: { Science: [9] }, view: withProposals });
  assert.equal(otherOffice.code, 'UnknownProposal', 'a proposal to another office is not adoptable');
  const tooMany = await client().submit({ orders: Array.from({ length: 20 }, () => ({ type: 'SendEnvoy', cityState: 1, influence: 1 })), policy: 'p/v1',
    adopt: { Diplomat: [9] }, view: withProposals });
  assert.deepEqual([tooMany.ok, tooMany.code], [false, 'BatchTooLarge']);
  assert.match(tooMany.error, /^Diplomat: more than 24 orders, counting the adopted/);
  assert.equal(sent.length, 0);
  const g = client();
  const ok = await g.submit({ orders: [research], policy: 'p/v1', adopt: { Science: [4] }, view: withProposals });
  assert.equal(ok.ok, true, JSON.stringify(ok));
  assert.deepEqual(g.sealed.get('5:Science')[0].batch.adopt, [4]);
});

test('claim proceeds once the season is Finalized or Aborted (a refund), and not before', async () => {
  const wallet = Keypair.generate();
  const mint = Keypair.generate().publicKey.toBase58();
  const dest = Keypair.generate().publicKey.toBase58();
  let status = 'Aborted';
  const posted = [];
  globalThis.fetch = async (url, init = {}) => {
    const { pathname } = new URL(url);
    const method = init.method ?? 'GET';
    if (pathname === '/season') return json(200, { programId: DEFAULTS.programId, season: { seasonId: '42' } });
    if (pathname === '/claim-relay' && method === 'GET') return json(200, { status, feePayer, blockhash, lastValidBlockHeight: 99, mint });
    if (pathname === '/claim-relay') { posted.push(Transaction.from(Buffer.from(JSON.parse(init.body).tx, 'base64'))); return json(200, { ok: true, signature: 'sigC' }); }
    return json(404, { error: 'not found' });
  };
  const g = client();
  assert.equal((await g.claim({ wallet, usdcAccount: dest })).signature, 'sigC');
  status = 'Finalized';
  assert.equal((await g.claim({ wallet, usdcAccount: dest })).signature, 'sigC');
  assert.equal(posted.length, 2);
  status = 'Running';
  const e = await g.claim({ wallet, usdcAccount: dest }).catch(x => x);
  assert.equal(e.code, 'NotFinalized');
  assert.match(e.message, /Finalized or Aborted/);
});
