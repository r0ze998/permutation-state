// The web client's commit path in play (permutation-server/web/orders.mjs,
// with the HUD modules that read the view), run in node against a stub DOM,
// a fake play server (/api/validate echoes the parsed DTOs the way serde
// writes them) and a fake gateway whose /seal is the gateway's own route:
//  - an open Capture contract offer commits although the echo adds "to": null;
//  - chain auto-commit waits for the drafts to settle, fires once per draft
//    state (typing a rationale does not commit again), retries a failure that
//    may pass after a pause, never retries a refusal, reports a retry that ran
//    out of time, and never goes past an office's commit cap;
//  - the chain lens prints an absolute gateway URL for verify, and the top
//    label counts the AI members from `aiRoster`.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { sealRoutes, SealCounter } from '../src/routes/seal.mjs';
import { RouteError } from '../src/routes/errors.mjs';
import { encode as base58 } from '../client/src/base58.mjs';
import { fromBase64 } from '../client/src/bytes.mjs';
import { IX_TAG, ROLES } from '../client/src/codec.mjs';
import { parseTransaction as decodeWire, pubkeyBytes } from '../client/src/solana-tx.mjs';

// ------------------------------------------------------------------ a stub DOM (before the web modules load)
const toasts = [];
class El {
  constructor(tag = 'div') {
    Object.assign(this, { tag, children: [], className: '', textContent: '', value: '', innerHTML: '', title: '', hidden: false, style: {}, dataset: {}, offsetWidth: 0 });
    const cls = new Set();
    this.classList = { add: c => cls.add(c), remove: c => cls.delete(c), contains: c => cls.has(c), toggle: (c, on = !cls.has(c)) => (on ? cls.add(c) : cls.delete(c), on) };
  }
  appendChild(c) { this.children.push(c); return c; }
  prepend(c) { this.children.unshift(c); }
  remove() {}
  setAttribute() {}
  addEventListener() {}
  closest() { return null; }
  querySelector() { return null; }
}
// The map's canvas: anything goes.
const anything = new Proxy(function () {}, {
  get: (t, k) => (k === Symbol.toPrimitive ? () => 0 : k === 'then' ? undefined : anything),
  apply: () => anything, construct: () => anything, set: () => true,
});
const els = new Map();
const notifications = { children: [], prepend: el => toasts.push({ text: el.children[0]?.textContent ?? '', kind: el.className.replace(/^toast\s*/, '') }) };
globalThis.document = {
  querySelector: s => (s === '#world-map' ? anything : s === '#notifications' ? notifications : (els.get(s) ?? els.set(s, new El()).get(s))),
  querySelectorAll: () => [],
  createElement: tag => (tag === 'canvas' ? anything : new El(tag)),
  addEventListener() {},
};
globalThis.window = globalThis;
globalThis.addEventListener ??= () => {};
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
globalThis.requestAnimationFrame = () => 0;
globalThis.location = { href: 'https://play.example.org/index.html?member=3', host: 'play.example.org' };
// toast() removes its element after 4.5–8 s: do not keep the test process alive for that.
const realSetTimeout = globalThis.setTimeout;
globalThis.setTimeout = (fn, ms, ...args) => { const t = realSetTimeout(fn, ms, ...args); t?.unref?.(); return t; };

const orders = await import('../../permutation-server/web/orders.mjs');
const chainplay = await import('../../permutation-server/web/chainplay.mjs');
const chainio = await import('../../permutation-server/web/chainio.mjs');
const book = await import('../../permutation-server/web/sealbook.mjs');
const { S } = await import('../../permutation-server/web/state.mjs');
const { keyFromSeed } = await import('../../permutation-server/web/session.mjs');
const { renderChainDrawer } = await import('../../permutation-server/web/chain.mjs');
const { renderTop } = await import('../../permutation-server/web/hud/top.mjs');

// ------------------------------------------------------------------ the season
const key = () => base58(randomBytes(32));
const PROGRAM = key(), CRANK = key(), BLOCKHASH = key();
const SEASON = '7', CIV = 2, ME = 3, TICK = 5;
const OBS = Buffer.from(randomBytes(32)).toString('hex');
let session, now;
Date.now = () => now; // the auto-commit clock

const research = { type: 'SetResearch', techs: ['Writing'] };
const research2 = { type: 'SetResearch', techs: ['Mathematics'] };
const capture = { type: 'OfferContract', term: { kind: 'Capture', city: 11 }, usdc: 3000000, deadline: 90 }; // as drawers/diplomacy.mjs builds it: no "to"
const OFFICE = { SetResearch: 'Science', OfferContract: 'Diplomat', SendEnvoy: 'Diplomat' };

/** What serde writes for an order the play server parsed: an absent Option as null, keys in its own order. */
const serdeEcho = o => {
  const x = o.type === 'OfferContract' ? { type: o.type, to: o.to ?? null, term: o.term, usdc: o.usdc, deadline: o.deadline } : { ...o };
  return JSON.parse(JSON.stringify(Object.fromEntries(Object.entries(x).reverse())));
};

/**
 * The fake play server and gateway behind one stubbed fetch. `relay(n)`
 * answers the n-th CommitOrders ({ok} or {status, code}); `validate(n)` may
 * throw (no answer) for the n-th /api/validate.
 */
function servers({ mine = ['Science'], relay = () => ({ ok: true }), validate = () => {} } = {}) {
  const log = { validates: [], seals: [], sealed: [], commits: [] };
  const keys = ROLES.map(r => (mine.includes(r) ? pubkeyBytes(session.publicKey) : randomBytes(32)));
  const snap = { header: { meta: { finished: false, frozen: false, revealing: false } }, nations: [] };
  snap.nations[CIV] = { openTick: TICK, officers: ROLES.map(r => (mine.includes(r) ? ME : 99)), keys };
  const ctx = {
    crank: { phase: 'playing', fresh: async () => snap, sealed: { put: (batch, salt, commitment) => log.sealed.push({ batch, salt, commitment }) } },
    store: { state: { seasonId: SEASON } }, limiter: { check() {} }, seals: new SealCounter(),
  };
  const json = (status, body) => ({ ok: status < 300, status, json: async () => body });
  globalThis.fetch = async (url, init = {}) => {
    const method = init.method ?? 'GET';
    const u = new URL(String(url), 'http://play');
    const body = init.body ? JSON.parse(init.body) : null;
    const where = `${method} ${u.host === 'gw' ? 'gw' : 'play'}${u.pathname}`;
    if (where === 'POST play/api/validate') {
      log.validates.push(body);
      validate(log.validates.length); // may throw: no answer
      const offices = mine.map(role => ({ role, orders: body.orders.filter(o => OFFICE[o.type] === role).map(serdeEcho), adopt: body.adopt?.[role] ?? [], cost: 1, spendable: 3, error: null }));
      const refused = body.orders.filter(o => !mine.includes(OFFICE[o.type])).map(o => ({ order: serdeEcho(o), error: 'propose it instead' }));
      return json(200, { ok: true, tick: TICK, offices, refused, warnings: [] });
    }
    if (where.startsWith('GET play/api/decisions')) return json(200, { open: TICK, records: [] });
    if (where === 'GET gw/season') return json(200, { programId: PROGRAM, season: { seasonId: SEASON, crank: CRANK }, members: [] });
    if (where === 'GET gw/tick') return json(200, { tick: TICK, phase: 'commit', nations: [{ civ: CIV, committed: [false, false, false, false], revealed: [false, false, false, false] }] });
    if (where === 'GET gw/relay') return json(200, { feePayer: CRANK, blockhash: BLOCKHASH, lastValidBlockHeight: 100 });
    if (where === 'POST gw/seal') {
      log.seals.push(body);
      try {
        return json(200, (await sealRoutes['POST /seal'](ctx, { json: async () => body })).body);
      } catch (e) {
        if (!(e instanceof RouteError)) throw e;
        return json(e.status, { error: e.message, code: e.code });
      }
    }
    if (where === 'POST gw/relay') {
      const tx = decodeWire(fromBase64(body.tx));
      const ix = tx.instructions.find(i => i.data[0] === IX_TAG.commitOrders);
      if (!ix) return json(400, { error: 'not a commit', code: 'RelayRejected' });
      log.commits.push(tx);
      const r = relay(log.commits.length);
      return r.ok ? json(200, { ok: true, signature: `sig${log.commits.length}` }) : json(r.status ?? 409, { error: r.code, code: r.code });
    }
    return json(404, { error: `no route ${where}`, code: 'NotFound' });
  };
  return log;
}

/** /api/state?member=ME at TICK (tick of 30 s: auto-commit from 10 s before the deadline). */
function view(mine = ['Science'], { secondsLeft = 9, phase = 'commit' } = {}) {
  return {
    tick: TICK, ticks: 120, tickSeconds: 30, secondsLeft, chainPhase: phase, paused: false,
    chain: { seasonId: SEASON, gateway: '/gw', cluster: 'devnet', endpoints: {} }, decision: { tick: TICK, obsRoot: OBS },
    civs: [0, 1, 2].map(id => ({ id, name: `N${id}` })), members: [{ id: ME, name: 'Me', civ: CIV }, { id: 8, name: 'Other', civ: 0 }],
    member: { id: ME, name: 'Me' }, season: { preset: 'Blitz' },
    gov: { offices: ROLES.map(role => ({ role, holder: mine.includes(role) ? { id: ME } : { id: 99 } })) },
    economy: { offices: ROLES.map(role => ({ role, budget: 3, bank: 0, spendable: 3 })) },
    roster: [{ id: ME, name: 'Me', kind: 'undeclared', merit: 0, active: true, activeWindows: 1 }],
    aiRoster: { aiCount: 5, bountyEach: '1000000', homeTick: 45, revealed: false, fallen: [] },
  };
}

/** Let everything in flight finish (fake fetches, WebCrypto signatures). */
async function settle() {
  for (let i = 0; i < 400; i++) {
    await new Promise(r => setImmediate(r));
    if (!S.committing && !S.autoCommit?.inFlight) {
      for (let j = 0; j < 20; j++) await new Promise(r => setImmediate(r));
      if (!S.committing && !S.autoCommit?.inFlight) return;
    }
  }
  throw new Error('still committing');
}

/** An applied view at time `t` (ms), as the poll loop applies it. */
async function applyView(t, v = S.view) {
  now = t;
  S.view = v;
  orders.afterView(v);
  await settle();
}

const errors = () => toasts.filter(t => t.kind === 'error').map(t => t.text);
const setRationale = text => { document.querySelector('#rationale').value = text; };

beforeEach(async () => {
  const k = await keyFromSeed(randomBytes(32));
  session = { ...k, wallet: key() };
  now = 1_000_000;
  toasts.length = 0;
  chainio.setGateway('http://gw');
  chainio.setPin({ programId: PROGRAM, cluster: 'devnet', seasonId: SEASON, entryFee: 10n });
  Object.assign(S, { view: view(), session, memberId: ME, myCiv: CIV, watch: null, chainSeals: null, govQueue: [], committing: false, autoCommit: null, hintedDock: true, hintedPropose: true });
  const b = chainplay.playBook();
  b.sealed = {};
  b.decisions = [];
  orders.resetTick();
});

test('an open Capture offer commits: the echo\'s "to": null matches the draft, and the draft is what gets sealed', async () => {
  const log = servers({ mine: ['Science', 'Diplomat'] });
  S.view = view(['Science', 'Diplomat'], { secondsLeft: 25 });
  await chainio.season();
  assert.equal(orders.addDraft(capture), true);
  assert.equal(orders.addDraft(research), true);
  const r = await orders.commit();
  assert.deepEqual(errors(), []);
  assert.equal(r.ok, true);
  assert.equal('to' in log.validates[0].orders.find(o => o.type === 'OfferContract'), false, 'sent without "to"');
  // Both offices sealed with the gateway's own /seal route and committed on chain.
  assert.deepEqual(log.sealed.map(s => s.batch.role).sort(), ['Diplomat', 'Science']);
  assert.deepEqual(log.sealed.find(s => s.batch.role === 'Diplomat').batch.orders, [capture]);
  assert.equal(log.commits.length, 2);
  assert.equal(orders.isDirty(), false);
});

test('auto-commit waits for the drafts to settle and does not commit again while the rationale is typed', async () => {
  const log = servers();
  await chainio.season();
  const t0 = now;
  orders.addDraft(research);
  await applyView(t0);
  await applyView(t0 + 1000);
  assert.equal(log.commits.length, 0, 'the drafts changed less than 2 s ago');
  await applyView(t0 + book.AUTO_QUIET_MS);
  assert.equal(log.commits.length, 1);
  assert.equal(book.commitCount(chainplay.playBook(), TICK, 'Science'), 1);
  assert.ok(toasts.some(t => /締切が近いため/.test(t.text)));
  // A memo typed during the window, with pauses longer than the settling
  // time: dirty again, but the same draft state.
  let t = t0 + book.AUTO_QUIET_MS;
  for (const text of ['W', 'Wr', 'Wri', 'Writ', 'Writing first', 'Writing first, then Mathematics']) {
    setRationale(text);
    for (const step of [750, 750, 750, 750]) await applyView(t += step);
  }
  assert.equal(orders.isDirty(), true);
  assert.equal(log.commits.length, 1, 'rationale keystrokes do not seal again');
  assert.equal(book.commitCount(chainplay.playBook(), TICK, 'Science'), 1);
  // A changed draft is a new state: it goes once it has settled, with the memo as it is then.
  orders.addDraft(research2);
  await applyView(t + 500);
  assert.equal(log.commits.length, 1);
  await applyView(t + 500 + book.AUTO_QUIET_MS);
  assert.equal(log.commits.length, 2);
  assert.deepEqual(log.sealed.at(-1).batch.orders, [research2]);
  assert.deepEqual(errors(), []);
});

test('auto-commit retries a failure that may pass (RateLimited) after a pause, without an error toast', async () => {
  const log = servers({ relay: n => (n === 1 ? { status: 429, code: 'RateLimited' } : { ok: true }) });
  await chainio.season();
  const t0 = now;
  orders.addDraft(research);
  await applyView(t0);
  await applyView(t0 + 2000);
  assert.equal(log.commits.length, 1);
  assert.deepEqual(errors(), [], 'a failure that will be retried is not shown as an error');
  assert.ok(toasts.some(t => /もう一度送ります/.test(t.text)));
  await applyView(t0 + 3000);
  assert.equal(log.commits.length, 1, 'backing off');
  await applyView(t0 + 2000 + book.AUTO_RETRY_MS + 100);
  assert.equal(log.commits.length, 2);
  assert.equal(orders.isDirty(), false);
  assert.deepEqual(book.candidates(chainplay.playBook(), TICK, 'Science').map(c => c.state), ['committed', 'failed']);
  assert.deepEqual(errors(), []);
});

test('auto-commit retries when the play server did not answer the check', async () => {
  const log = servers({ validate: n => { if (n === 1) throw new Error('connection reset'); } });
  await chainio.season();
  const t0 = now;
  orders.addDraft(research);
  await applyView(t0);
  await applyView(t0 + 2000);
  assert.equal(log.validates.length, 1);
  assert.equal(log.commits.length, 0);
  await applyView(t0 + 2000 + book.AUTO_RETRY_MS);
  assert.equal(log.commits.length, 1);
  assert.equal(orders.isDirty(), false);
  assert.deepEqual(errors(), []);
});

test('auto-commit sends a refused draft once: the refusal is shown and not retried', async () => {
  const log = servers({ relay: () => ({ status: 409, code: 'WrongOffice' }) });
  await chainio.season();
  const t0 = now;
  orders.addDraft(research);
  for (let t = t0; t <= t0 + 8000; t += 750) await applyView(t);
  assert.equal(log.commits.length, 1);
  assert.equal(log.seals.length, 1);
  assert.equal(errors().length, 1);
  assert.match(errors()[0], /確定できませんでした/);
});

test('a retry that runs out of time reports the failure it held back', async () => {
  const log = servers({ relay: () => ({ status: 429, code: 'RateLimited' }) });
  await chainio.season();
  const t0 = now;
  orders.addDraft(research);
  S.view = view(['Science'], { secondsLeft: 3 }); // too close to the deadline to wait for the drafts to settle
  await applyView(t0);
  assert.equal(log.commits.length, 1);
  assert.deepEqual(errors(), []);
  await applyView(t0 + book.AUTO_RETRY_MS + 100, view(['Science'], { secondsLeft: 2 }));
  assert.equal(log.commits.length, 1, 'no retry with 2 s left');
  assert.equal(errors().length, 1);
  assert.match(errors()[0], /確定できませんでした/);
  await applyView(t0 + 2 * book.AUTO_RETRY_MS, view(['Science'], { secondsLeft: 1 }));
  assert.equal(errors().length, 1, 'reported once');
});

test('auto-commit never goes past an office\'s commit cap', async () => {
  const log = servers();
  await chainio.season();
  const b = chainplay.playBook();
  for (let i = 0; i < book.MAX_COMMITS; i++) {
    book.addCandidate(b, { tick: TICK, role: 'Science', commitment: `c${i}`, drafts: [], adopt: [] }, { tick: TICK, role: 'Science', digest: `d${i}` });
    book.markCandidate(b, TICK, 'Science', `c${i}`, { state: 'failed', code: 'network' });
  }
  const t0 = now;
  orders.addDraft(research);
  for (let t = t0; t <= t0 + 6000; t += 750) await applyView(t);
  assert.equal(log.validates.length, 0);
  assert.equal(log.commits.length, 0);
});

test('the chain lens prints an absolute gateway for verify; the top label counts the AI members from aiRoster', () => {
  S.chainOpen = true;
  S.view = view();
  renderChainDrawer();
  const lens = document.querySelector('#chain-drawer').innerHTML;
  assert.match(lens, /--gateway https:\/\/play\.example\.org\/gw /);
  assert.doesNotMatch(lens, /--gateway \/gw|undefined/);
  assert.match(lens, /&lt;base RPC&gt;/);
  S.view = { ...view(), economy: undefined }; // (the resource bar is not what this is about)
  renderTop();
  assert.match(document.querySelector('.prototype-label').textContent, /国民2人（うち運営のAI 5人）/);
});
