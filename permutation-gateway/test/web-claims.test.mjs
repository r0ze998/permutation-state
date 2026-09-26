// The web client's earlier-season prizes (permutation-server/web/claim.mjs
// findPastClaims, chainio.mjs claims) against a fake gateway: the list comes
// from the gateway's GET /claims?wallet= (this page never calls an RPC, so
// the operator's RPC URL never has to be public), finalized, unclaimed and
// non-zero seasons other than this one; a gateway without /claims falls back
// to its /history lineage with unknown amounts; a gateway that cannot answer
// now is asked again later (null).
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import bs58 from 'bs58';
import * as chainio from '../../permutation-server/web/chainio.mjs';
import { findPastClaims, pastClaimsHtml } from '../../permutation-server/web/claim.mjs';

const key = () => bs58.encode(randomBytes(32));
const PROGRAM = key(), WALLET = key();
const CURRENT = '9';

let calls;
/** A fake gateway at http://site/gw: `routes` maps "METHOD /path" to [status, body]. */
function gateway(routes) {
  calls = [];
  globalThis.fetch = async (url, init = {}) => {
    const u = new URL(String(url));
    const method = init.method ?? 'GET';
    calls.push({ method, url: u.href, host: u.host, path: u.pathname, query: u.searchParams, body: init.body ?? null });
    const hit = u.host === 'site' ? routes[`${method} ${u.pathname.replace(/^\/gw/, '')}`] : null;
    const [status, body] = hit ?? [404, { error: 'not found', code: 'NotFound' }];
    return { ok: status >= 200 && status < 300, status, json: async () => body };
  };
}

beforeEach(() => {
  chainio.setGateway('http://site/gw');
  chainio.setPin({ programId: PROGRAM, cluster: 'devnet', seasonId: CURRENT, entryFee: 0 });
});

const claim = (seasonId, extra = {}) => ({ seasonId, member: 3, civ: 1, name: 'n', amount: '2500000', claimed: false, status: 'Finalized', ...extra });

test('claims: GET /claims?wallet= on the gateway, nothing else', async () => {
  gateway({ 'GET /claims': [200, { claims: [claim('8')] }] });
  const r = await chainio.claims(WALLET);
  assert.equal(r.ok, true);
  assert.deepEqual(calls.map(c => [c.method, c.host, c.path, c.query.get('wallet')]), [['GET', 'site', '/gw/claims', WALLET]]);
  assert.equal(typeof chainio.memberOnChain, 'undefined', 'no direct RPC read is left');
});

test('past claims: this wallet\'s finalized, unclaimed, non-zero prizes of earlier seasons, from the gateway alone', async () => {
  gateway({
    'GET /claims': [200, { claims: [
      claim(CURRENT, { amount: '7000000' }),              // this season: the result card handles it
      claim('8'),
      claim('7', { claimed: true }),
      claim('6', { amount: '0' }),
      claim('5', { status: 'Running' }),                  // not finalized yet
      claim('4', { amount: '12345678901234567890' }),     // more than 2^53: kept exact
      claim('4'),                                         // listed twice: once
      claim('3', { amount: 'lots' }),                     // unreadable
    ] }],
    // A /season with endpoints must not tempt the page into calling the RPC.
    'GET /season': [200, { endpoints: { base: 'http://rpc.example/?api-key=secret' } }],
  });
  const list = await findPastClaims(WALLET);
  assert.deepEqual(list, [{ seasonId: '8', total: 2_500_000n }, { seasonId: '4', total: 12345678901234567890n }]);
  assert.deepEqual(calls.map(c => `${c.method} ${c.host}${c.path}`), ['GET site/gw/claims']);
  assert.ok(calls.every(c => c.host === 'site' && c.method === 'GET'), 'no JSON-RPC call anywhere');
  const markup = String(pastClaimsHtml(list, ''));
  assert.match(markup, /シーズン 8/);
  assert.match(markup, /data-claim-season="4"/);
  assert.doesNotMatch(markup, /シーズン 7|シーズン 6|シーズン 5/);
});

test('past claims: a gateway without /claims → its lineage with unknown amounts; one that cannot answer now → null (asked again later)', async () => {
  gateway({ 'GET /history': [200, { lineage: [{ seasonId: '8' }, { seasonId: CURRENT }, { seasonId: 7 }, { seasonId: '8' }] }] });
  assert.deepEqual(await findPastClaims(WALLET), [{ seasonId: '8', total: null }, { seasonId: '7', total: null }]);
  assert.match(String(pastClaimsHtml([{ seasonId: '8', total: null }], '')), /ゲートウェイから読めませんでした/);
  for (const [status, body] of [[429, { error: 'slow down', code: 'RateLimited' }], [503, {}], [500, { error: 'rpc down', code: 'RpcError' }]]) {
    gateway({ 'GET /claims': [status, body], 'GET /history': [200, { lineage: [{ seasonId: '8' }] }] });
    assert.equal(await findPastClaims(WALLET), null, `HTTP ${status}`);
    assert.deepEqual(calls.map(c => c.path), ['/gw/claims'], 'no fallback listing on a passing failure');
  }
  globalThis.fetch = async () => { throw new Error('offline'); };
  assert.equal(await findPastClaims(WALLET), null);
  gateway({});
  assert.equal(await findPastClaims(WALLET), null, 'no /claims and no /history either');
});

test('result card: a fresh page fetches /season on its first render (no 20 s wait)', async () => {
  const { S } = await import('../../permutation-server/web/state.mjs');
  const { claimCard } = await import('../../permutation-server/web/claim.mjs');
  gateway({ 'GET /season': [200, { season: { status: 'Finalized', payouts: ['0', '0', '0', '5000000'], treasury: [], treasuryFinal: [] }, members: [{ index: 3, civ: 1, wallet: WALLET, shares: '0', claimed: false }] }] });
  S.chainMember = { index: 3, wallet: WALLET };
  S.claim = null;
  claimCard({ chain: {} });
  await new Promise(r => setTimeout(r, 0));
  assert.deepEqual(calls.map(c => c.path), ['/gw/season'], 'loaded at once, not 20 s after page load');
  assert.ok(S.claim.season, 'the season is kept for the next render');
});
