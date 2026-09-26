// POST /seal: an officer's sealed batch, signed by its office key over
// sealMessage, kept for the crank's reveal; every refusal the same for an
// office an AI member holds and one a person holds.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair } from '@solana/web3.js';
import { fromHex, randomBytes, toHex } from '../client/src/bytes.mjs';
import { NOBODY, orderCommitment } from '../client/src/codec.mjs';
import { sealMessage } from '../client/src/player.mjs';
import { signTalk } from '../client/src/talk-node.mjs';
import { SealedStore } from '../src/sealed.mjs';
import { call, gateway } from './gateway-fixtures.mjs';

const TICK = 7;
const ai = Keypair.generate(), person = Keypair.generate();

/** Nation 0: the general is member 3 (an AI member), the scientist member 5 (a person); nation 1: nobody. */
function playing({ revealing = false, frozen = false, phase = 'playing' } = {}) {
  const header = (officers, keys) => ({ openTick: TICK, officers, keys, committed: [TICK, 65535, 65535, 65535], submitted: [65535, 65535, 65535, 65535] });
  const zero = new Uint8Array(32);
  const snap = { at: Date.now(), header: { meta: { finished: false, frozen, revealing, deadline: 0 } },
    nations: [header([3, NOBODY, 5, NOBODY], [ai.publicKey.toBytes(), zero, person.publicKey.toBytes(), zero]), header([NOBODY, NOBODY, NOBODY, NOBODY], [zero, zero, zero, zero])] };
  return gateway({
    state: { members: [{ index: 3, civ: 0, hosted: 'ai', key: 'x' }, { index: 5, civ: 0, hosted: 'external' }] },
    crank: { phase, fresh: async () => snap, sealed: new SealedStore() },
  });
}

/** A seal of `role`'s batch by `signer`, as the browser builds it. */
function seal({ civ = 0, role = 'General', tick = TICK, member = 3, orders = [], adopt = [], digest = '09'.repeat(32), signer = ai, salt = toHex(randomBytes(32)) } = {}) {
  const commitment = orderCommitment({ civ, tick, role, member, decisionDigest: fromHex(digest), orders, adopt }, fromHex(salt));
  const signature = toHex(signTalk(sealMessage({ seasonId: 42n, tick, civ, role, commitment }), signer));
  return { body: { civ, role, tick, member, digest, orders, adopt, salt, signature }, commitment: toHex(commitment) };
}

let n = 0;
const post = (g, body) => call(g.public, 'POST', '/seal', { body, ip: `10.1.0.${++n % 250}` });

test('/seal keeps a batch signed by the office key; the crank reveals it with the rest', async () => {
  const g = playing();
  const orders = [{ type: 'SetResearch', techs: ['Writing'] }];
  const s = seal({ role: 'Science', member: 5, signer: person, orders, adopt: [2] });
  const r = await post(g, s.body);
  assert.equal(r.status, 200, JSON.stringify(r.json));
  assert.deepEqual(r.json, { ok: true, commitment: s.commitment });
  const kept = g.crank.sealed.forTick(TICK);
  assert.equal(kept.length, 1);
  assert.deepEqual([kept[0].civ, kept[0].role, kept[0].member, kept[0].commitment, kept[0].adopt], [0, 'Science', 5, s.commitment, [2]]);
  assert.deepEqual(kept[0].orders, orders);
  assert.equal(toHex(kept[0].salt), s.body.salt);
});

test('/seal refuses: another signer, another tick, outside the commit phase, not the officer, a zero digest, another office\'s order, too much', async () => {
  const g = playing();
  const move = { type: 'MoveUnit', unit: 1, path: Array.from({ length: 60 }, (_, i) => [i, -i]) };
  const cases = [
    [seal({ signer: person }).body, 403, 'BadSignature'],
    [{ ...seal().body, salt: '00'.repeat(32) }, 403, 'BadSignature'],
    [seal({ tick: TICK + 1 }).body, 409, 'WrongTick'],
    [seal({ member: 4 }).body, 403, 'NotOfficer'],
    [seal({ civ: 1, member: NOBODY }).body, 403, 'NotOfficer'],
    [seal({ digest: '00'.repeat(32) }).body, 400, 'MissingRationale'],
    [seal({ orders: [{ type: 'SetResearch', techs: ['Writing'] }] }).body, 400, 'WrongOffice'],
    [seal({ orders: [move, move, move, move] }).body, 400, 'BatchTooLarge'],
    [seal({ adopt: Array.from({ length: 25 }, (_, i) => i) }).body, 400, 'BatchTooLarge'],
    [{ ...seal().body, role: 'King' }, 400, 'InvalidSeal'],
    [{ ...seal().body, signature: 'zz' }, 400, 'InvalidSeal'],
    [{ ...seal().body, orders: [{ type: 'Teleport' }] }, 400, 'InvalidSeal'],
  ];
  for (const [body, status, code] of cases) {
    const r = await post(g, body);
    assert.deepEqual([r.status, r.json.code], [status, code], `${code}: ${r.json.error}`);
  }
  for (const [opts, code] of [[{ revealing: true }, 'WrongPhase'], [{ frozen: true }, 'WrongPhase'], [{ phase: 'registering' }, 'WrongPhase']]) {
    const r = await post(playing(opts), seal().body);
    assert.deepEqual([r.status, r.json.code], [409, code]);
  }
  assert.equal(g.crank.sealed.forTick(TICK).length, 0, 'nothing kept');
});

test('/seal: at most 16 seals per office key and tick (429 TooManySeals); only signed seals count', async () => {
  const g = playing();
  for (let k = 0; k < 5; k++) assert.equal((await post(g, seal({ signer: person }).body)).status, 403, 'forged: not counted');
  for (let k = 0; k < 16; k++) assert.equal((await post(g, seal().body)).status, 200);
  const r = await post(g, seal().body);
  assert.deepEqual([r.status, r.json.code], [429, 'TooManySeals']);
  assert.equal((await post(g, seal({ role: 'Science', member: 5, signer: person }).body)).status, 200, 'another office key');
});

test('/seal answers the same for an office an AI member holds and one a person holds', async () => {
  const g = playing();
  // Rationale reveals: any office may give them.
  const reveal = { type: 'RevealRationale', tick: 1, policy: 'officer@2', salt: '00'.repeat(16), text: 'x'.repeat(500) };
  const both = o => [seal({ ...o }), seal({ role: 'Science', member: 5, ...o, signer: person })];
  const variants = [
    { tick: TICK + 1 }, { digest: '00'.repeat(32) }, { orders: [reveal, reveal] }, { adopt: Array.from({ length: 30 }, (_, i) => i) },
  ];
  for (const v of variants) {
    const [a, p] = both(v);
    const ra = await post(g, a.body), rp = await post(g, p.body);
    assert.deepEqual([ra.status, ra.json], [rp.status, rp.json], JSON.stringify(v));
  }
  // A forged signature: the same answer too.
  const fa = await post(g, seal({ signer: person }).body);
  const fp = await post(g, seal({ role: 'Science', member: 5, signer: ai }).body);
  assert.deepEqual([fa.status, fa.json], [fp.status, fp.json]);
  // The cap, for each.
  for (let k = 0; k < 16; k++) {
    await post(g, seal().body);
    await post(g, seal({ role: 'Science', member: 5, signer: person }).body);
  }
  const ca = await post(g, seal().body), cp = await post(g, seal({ role: 'Science', member: 5, signer: person }).body);
  assert.deepEqual([ca.status, ca.json], [cp.status, cp.json]);
});
