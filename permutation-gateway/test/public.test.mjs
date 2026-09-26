// The public listener: only PUBLIC_ROUTES (operator routes are 404 there,
// token or not), CORS with the x402 receipt exposed, limits per client
// address (behind a proxy on this machine: its X-Forwarded-For), the circuit
// breaker on the crank's SOL; and the operator token compared in constant time.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { COSIGN_ROUTES, FUNDED_ROUTES, PUBLIC_ROUTES, ROUTES } from '../src/app.mjs';
import { addressBucket, clientIp, FundsGuard, IP_LIMITS, RateLimiter, ReplayCache, TtlCache } from '../src/guards.mjs';
import { requireOperator } from '../src/routes/roster.mjs';
import { call, gateway, keyring } from './gateway-fixtures.mjs';

const OP = { authorization: 'Bearer op-token' };

test('the public listener serves exactly PUBLIC_ROUTES; every operator route is 404 there, with the token too', async () => {
  const g = gateway();
  const operatorOnly = Object.keys(ROUTES).filter(k => !PUBLIC_ROUTES.includes(k));
  assert.deepEqual(operatorOnly.sort(), ['GET /health', 'GET /operator/roster', 'POST /gov', 'POST /roster/announce', 'POST /submit'].sort());
  for (const key of operatorOnly) {
    const [method, path] = key.split(' ');
    const r = await call(g.public, method, path, { body: {}, headers: OP });
    assert.deepEqual([r.status, r.json.code], [404, 'NotFound'], key);
  }
  for (const key of PUBLIC_ROUTES) {
    assert.ok(ROUTES[key], key);
    const [method, path] = key.split(' ');
    const r = await call(g.public, method, path, { body: {}, ip: `10.3.0.${PUBLIC_ROUTES.indexOf(key)}` });
    assert.notEqual(r.json?.code, 'NotFound', key);
  }
  // The operator listener still has everything.
  assert.equal((await call(g.operator, 'GET', '/operator/roster', { headers: OP })).status, 200);
  assert.equal((await call(g.operator, 'GET', '/health')).status, 200);
  assert.equal((await call(g.public, 'GET', '/nope')).status, 404);
});

test('public CORS: preflight only for public paths; the x402 receipt header is readable by pages', async () => {
  const g = gateway();
  const pre = await call(g.public, 'OPTIONS', '/x402/join');
  assert.equal(pre.status, 204);
  assert.equal(pre.headers['Access-Control-Allow-Origin'], '*');
  assert.equal(pre.headers['Access-Control-Expose-Headers'], 'X-PAYMENT-RESPONSE');
  assert.equal((await call(g.public, 'OPTIONS', '/operator/roster')).status, 404);
  const r = await call(g.public, 'GET', '/season');
  assert.equal(r.headers['Access-Control-Expose-Headers'], 'X-PAYMENT-RESPONSE');
  assert.equal(r.headers['X-Content-Type-Options'], 'nosniff');
});

test('public /talk takes signed messages only (unsigned is 400, even with the operator token); the operator listener signs for its AI members', async () => {
  const keys = keyring();
  const g = gateway({ keys, registryMembers: [{ index: 7, civ: 2, session: keys('k7-session').publicKey.toBase58() }], state: { members: [{ index: 7, civ: 2, hosted: 'ai', key: 'k7' }] } });
  const pub = await call(g.public, 'POST', '/talk', { body: { member: 7, text: 'hi' }, headers: OP });
  assert.deepEqual([pub.status, pub.json.code], [400, 'SignatureRequired']);
  assert.equal((await call(g.operator, 'POST', '/talk', { body: { member: 7, text: 'hi' } })).status, 403, 'unsigned needs the token');
  assert.equal((await call(g.operator, 'POST', '/talk', { body: { member: 7, text: 'hi' }, headers: OP })).status, 200, 'the gateway signs for its AI member');
});

test('requireOperator: the token, compared in constant time, and only on the operator listener', () => {
  const ctx = { cfg: { operatorToken: 'op-token' } };
  assert.doesNotThrow(() => requireOperator(ctx, { headers: OP }));
  for (const auth of ['Bearer op-toke', 'Bearer op-token ', 'op-token', '', 'Bearer ' + 'x'.repeat(500)]) {
    assert.throws(() => requireOperator(ctx, { headers: { authorization: auth } }), e => e.status === 403 && e.code === 'OperatorOnly');
  }
  assert.throws(() => requireOperator(ctx, { headers: OP, surface: 'public' }), e => e.code === 'OperatorOnly');
  assert.throws(() => requireOperator({ cfg: {} }, { headers: { authorization: 'Bearer undefined' } }), e => e.code === 'OperatorOnly');
});

test('RateLimiter: a burst, then the steady rate; buckets full again are forgotten', () => {
  let now = 0;
  const l = new RateLimiter({ now: () => now, maxBuckets: 2 });
  const limit = { burst: 3, perSecond: 1 };
  assert.deepEqual([1, 2, 3, 4].map(() => l.take('a', limit)), [true, true, true, false]);
  now = 1000;
  assert.deepEqual([l.take('a', limit), l.take('a', limit)], [true, false]);
  l.take('b', limit);
  now = 10_000;
  l.take('c', limit); // over maxBuckets: prunes the full ones
  assert.ok(!l.buckets.has('a') && !l.buckets.has('b'));
  assert.throws(() => { for (;;) l.check('d', limit); }, e => e.status === 429 && e.code === 'RateLimited');
});

test('public limits per client address (GET /tick, POST /relay…); the operator listener is not limited', async () => {
  const g = gateway();
  const { burst } = IP_LIMITS['GET /tick'];
  for (let k = 0; k < burst; k++) assert.notEqual((await call(g.public, 'GET', '/tick', { ip: '10.4.0.1' })).status, 429);
  const r = await call(g.public, 'GET', '/tick', { ip: '10.4.0.1' });
  assert.deepEqual([r.status, r.json.code], [429, 'RateLimited']);
  assert.notEqual((await call(g.public, 'GET', '/tick', { ip: '10.4.0.2' })).status, 429, 'another address');
  for (let k = 0; k < burst + 5; k++) assert.notEqual((await call(g.operator, 'GET', '/tick', { ip: '127.0.0.1' })).status, 429);
  for (const key of ['POST /x402/join', 'POST /relay', 'POST /claim-relay', 'POST /seal', 'POST /talk', 'POST /faucet', 'GET /usdc', 'GET /relay', 'GET /claim-relay']) assert.ok(IP_LIMITS[key], key);
});

test('clientIp: X-Forwarded-For only with --trust-proxy and a loopback peer, its last address', () => {
  const req = (peer, xff) => ({ socket: { remoteAddress: peer }, headers: xff ? { 'x-forwarded-for': xff } : {} });
  assert.equal(clientIp(req('127.0.0.1', '1.2.3.4')), '127.0.0.1', 'not trusted by default');
  assert.equal(clientIp(req('127.0.0.1', '1.2.3.4'), { trustProxy: true }), '1.2.3.4');
  assert.equal(clientIp(req('::1', '9.9.9.9, 1.2.3.4'), { trustProxy: true }), '1.2.3.4', 'the proxy\'s own entry, not the client\'s claim');
  assert.equal(clientIp(req('::ffff:127.0.0.1', '2001:db8::1'), { trustProxy: true }), '2001:db8::1');
  assert.equal(clientIp(req('203.0.113.5', '1.2.3.4'), { trustProxy: true }), '203.0.113.5', 'a remote peer cannot claim an address');
  assert.equal(clientIp(req('127.0.0.1', 'garbage'), { trustProxy: true }), '127.0.0.1');
});

test('addressBucket: an IPv4 address itself; an IPv6 address by its /64, however it is written', async () => {
  assert.equal(addressBucket('203.0.113.5'), '203.0.113.5');
  assert.equal(addressBucket('::ffff:203.0.113.5'), '203.0.113.5');
  for (const a of ['2001:db8:1:2::1', '2001:DB8:1:2:ffff:ffff:ffff:ffff', '2001:0db8:0001:0002:0:0:0:9', '2001:db8:1:2::192.0.2.1']) assert.equal(addressBucket(a), '2001:db8:1:2::/64', a);
  assert.equal(addressBucket('::1'), '0:0:0:0::/64');
  assert.equal(addressBucket('2001:db8::'), '2001:db8:0:0::/64');
  assert.notEqual(addressBucket('2001:db8:1:3::1'), addressBucket('2001:db8:1:2::1'));
  assert.equal(addressBucket('unknown'), 'unknown');
  // One /64 is one budget on the public listener.
  const g = gateway();
  const { burst } = IP_LIMITS['GET /tick'];
  for (let k = 0; k < burst; k++) await call(g.public, 'GET', '/tick', { ip: `2001:db8:5:6::${(k + 1).toString(16)}` });
  assert.equal((await call(g.public, 'GET', '/tick', { ip: '2001:db8:5:6:abcd::1' })).status, 429);
  assert.notEqual((await call(g.public, 'GET', '/tick', { ip: '2001:db8:5:7::1' })).status, 429);
});

test('behind the play server\'s proxy (--trust-proxy) the limits are per browser', async () => {
  const g = gateway({ cfg: { trustProxy: true } });
  const { burst } = IP_LIMITS['GET /tick'];
  for (let k = 0; k < burst; k++) await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.1' } });
  assert.equal((await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.1' } })).status, 429);
  assert.notEqual((await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.2' } })).status, 429);
});

test('circuit breaker: below --min-crank-sol whatever makes the crank pay for a member answers 503 OperatorLowFunds on both listeners, the AI members\' /submit and /gov too (balance read at most every 30 s); the rest goes on', async () => {
  let now = 0;
  const g = gateway({ base: { balance: 0.1e9 }, now: () => now });
  assert.deepEqual([...FUNDED_ROUTES].sort(), [...COSIGN_ROUTES, 'POST /submit', 'POST /gov'].sort());
  for (const key of COSIGN_ROUTES) {
    const [method, path] = key.split(' ');
    const r = await call(g.public, method, path, { body: {}, ip: `10.5.0.${COSIGN_ROUTES.indexOf(key)}` });
    assert.deepEqual([r.status, r.json.code], [503, 'OperatorLowFunds'], key);
  }
  assert.equal(g.base.calls.getBalance, 1, 'one read for all of them');
  // Gated by what is done, not where: the operator listener pauses the same
  // actions, and the game server's /submit and /gov for the AI members.
  for (const key of FUNDED_ROUTES) {
    const [method, path] = key.split(' ');
    const r = await call(g.operator, method, path, { body: { member: 7, civ: 0, role: 'General', tick: 0, digest: '11'.repeat(32), tx: 'AAAA' }, headers: OP });
    assert.deepEqual([r.status, r.json.code], [503, 'OperatorLowFunds'], `operator ${key}`);
  }
  assert.notEqual((await call(g.public, 'GET', '/relay')).status, 503, 'reads go on');
  assert.notEqual((await call(g.public, 'POST', '/seal', { body: {} })).status, 503, 'a deposit costs no fee');
  assert.notEqual((await call(g.operator, 'POST', '/roster/announce', { body: { member: 1 }, headers: OP })).status, 503, 'operator upkeep goes on');
  assert.ok(g.logs.some(l => /co-signing, the faucet and AI registrations paused/.test(l)));
  // Funded again: seen within 30 s.
  g.base.getBalance = async () => { g.base.count('getBalance'); return 5e9; };
  now = 29_000;
  assert.equal((await call(g.public, 'POST', '/relay', { body: { tx: 'AAAA' }, ip: '10.5.1.1' })).json.code, 'OperatorLowFunds');
  now = 30_000;
  assert.equal((await call(g.public, 'POST', '/relay', { body: { tx: 'AAAA' }, ip: '10.5.1.2' })).json.code, 'InvalidTransaction');
  assert.equal(g.base.calls.getBalance, 2);
});

test('the routes and the crank share one FundsGuard when it is passed in (server.mjs builds one for both)', async () => {
  const funds = new FundsGuard({ read: async () => 0, minLamports: 1 });
  const g = gateway({ funds });
  assert.equal(g.ctx.funds, funds);
});

test('ReplayCache: a charged signature is refused (409 Duplicate) for its window, then forgotten; at most `max` kept, oldest first', () => {
  let now = 0;
  const c = new ReplayCache({ ttlMs: 1000, max: 3, now: () => now });
  c.add('a');
  assert.throws(() => c.refuseRepeat('a'), e => e.status === 409 && e.code === 'Duplicate');
  assert.doesNotThrow(() => c.refuseRepeat('b'));
  now = 1000;
  assert.equal(c.has('a'), false, 'expired');
  for (const k of ['b', 'c', 'd', 'e']) c.add(k);
  assert.deepEqual([...c.seen.keys()], ['c', 'd', 'e']);
});

test('TtlCache.delete: the next get loads again', async () => {
  let loads = 0;
  const cache = new TtlCache({ ttlMs: 5000 });
  const load = async () => ++loads;
  assert.equal(await cache.get('k', load), 1);
  assert.equal(await cache.get('k', load), 1);
  cache.delete('k');
  assert.equal(await cache.get('k', load), 2);
});

test('by default a request from this machine (the play server\'s /gw) is limited by the browser its X-Forwarded-For names; --no-trust-proxy limits them together and says so once', async () => {
  const g = gateway();
  const { burst } = IP_LIMITS['GET /tick'];
  for (let k = 0; k < burst; k++) await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.7' } });
  assert.equal((await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.7' } })).status, 429);
  assert.notEqual((await call(g.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.8' } })).status, 429, 'another browser behind /gw');
  assert.notEqual((await call(g.public, 'GET', '/tick', { ip: '203.0.113.9', headers: { 'x-forwarded-for': '198.51.100.7' } })).status, 429, 'a remote peer cannot pick a bucket');
  const off = gateway({ cfg: { trustProxy: false } });
  for (let k = 0; k < burst; k++) await call(off.public, 'GET', '/tick', { headers: { 'x-forwarded-for': `198.51.100.${k}` } });
  assert.equal((await call(off.public, 'GET', '/tick', { headers: { 'x-forwarded-for': '198.51.100.99' } })).status, 429, 'one budget for every client');
  assert.equal(off.logs.filter(l => /--no-trust-proxy is set: every client shares one rate-limit budget/.test(l)).length, 1);
});

test('the public listener publishes no RPC URL unless told which (--public-base-rpc / --public-er-rpc); the operator listener shows its own', async () => {
  const g = gateway({ cfg: { baseRpc: 'https://rpc.example/?api-key=SECRET', erRpc: 'https://er.example/?api-key=SECRET' } });
  const pub = await call(g.public, 'GET', '/season');
  assert.equal(pub.status, 200);
  assert.equal('endpoints' in pub.json, false);
  const relay = await call(g.public, 'GET', '/relay');
  assert.equal('endpoint' in relay.json, false);
  assert.ok(!JSON.stringify([pub.json, relay.json]).includes('SECRET'));
  const op = await call(g.operator, 'GET', '/season');
  assert.deepEqual(op.json.endpoints, { base: 'https://rpc.example/?api-key=SECRET', er: 'https://er.example/?api-key=SECRET' });
  assert.equal((await call(g.operator, 'GET', '/relay')).json.endpoint, 'https://er.example/?api-key=SECRET');
  const shown = gateway({ cfg: { baseRpc: 'x', erRpc: 'y', publicBaseRpc: 'https://api.devnet.solana.com', publicErRpc: 'https://devnet.magicblock.app' } });
  assert.deepEqual((await call(shown.public, 'GET', '/season')).json.endpoints, { base: 'https://api.devnet.solana.com', er: 'https://devnet.magicblock.app' });
  assert.equal((await call(shown.public, 'GET', '/relay')).json.endpoint, 'https://devnet.magicblock.app');
  const baseOnly = gateway({ cfg: { publicBaseRpc: 'https://api.devnet.solana.com' } });
  assert.deepEqual((await call(baseOnly.public, 'GET', '/season')).json.endpoints, { base: 'https://api.devnet.solana.com' });
  assert.equal('endpoint' in (await call(baseOnly.public, 'GET', '/relay')).json, false);
});

test('FundsGuard: a failed read keeps the last balance (none yet: allowed); TtlCache shares one load per key', async () => {
  let now = 0;
  const reads = [];
  const guard = new FundsGuard({ read: async () => { reads.push(now); throw new Error('rpc'); }, minLamports: 10, intervalMs: 1000, now: () => now });
  await guard.check();
  guard.balance = 5;
  now = 2000;
  await assert.rejects(guard.check(), e => e.code === 'OperatorLowFunds');
  assert.equal(reads.length, 2);
  let loads = 0;
  const cache = new TtlCache({ ttlMs: 5000, now: () => now });
  const load = async () => { loads++; return loads; };
  assert.deepEqual(await Promise.all([cache.get('k', load), cache.get('k', load)]), [1, 1]);
  now = 7001;
  assert.equal(await cache.get('k', load), 2);
});

test('both listeners over real sockets: the public one hides operator routes; the operator one keeps them', async () => {
  const g = gateway();
  const listen = h => new Promise(resolve => { const s = http.createServer(h).listen(0, '127.0.0.1', () => resolve(s)); });
  const [op, pub] = await Promise.all([listen(g.operator), listen(g.public)]);
  try {
    const url = (s, p) => `http://127.0.0.1:${s.address().port}${p}`;
    let r = await fetch(url(pub, '/operator/roster'), { headers: OP });
    assert.equal(r.status, 404);
    r = await fetch(url(op, '/operator/roster'), { headers: OP });
    assert.equal(r.status, 200);
    r = await fetch(url(pub, '/season'));
    assert.equal(r.status, 200);
    assert.equal(r.headers.get('access-control-expose-headers'), 'X-PAYMENT-RESPONSE');
    r = await fetch(url(pub, '/seal'), { method: 'POST', headers: { 'content-type': 'application/json' }, body: 'x'.repeat(70_000) });
    assert.equal(r.status, 413, 'public bodies are small');
  } finally {
    op.close();
    pub.close();
  }
});
