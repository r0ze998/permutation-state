// The browser seal (permutation-server/web/frontier/seal.mjs and
// seal-worker.mjs, over the vendored noble tree) against the vectors the
// Rust side seals and opens (permutation-rules/vectors/seal-vectors-v1.json:
// tlock =0.0.10 with a fixed σ, reopened by the stock tlock opener):
//  - plaintext pack/unpack/validate, salt, commitment, body, ct_hash and
//    seal root byte for byte, and Plain::validate's reason for every case;
//  - the IBE block: with the vector's k and σ the browser produces the same
//    165-byte seal; opening with the recorded quicknet signature gives k and
//    the plaintext back, and each tampered case fails as the program would;
//  - the self-audit catches a flipped bit in U, V, W and the body, and a
//    plaintext that Plain::validate refuses;
//  - the beacon pin: quicknet only, the test key only on localnet, the
//    Season's quicknet_pk_hash must match; T(b) rounds up;
//  - the local test key (I-53): a seal to a test-key round opens with the
//    test key's recorded signature.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as seal from '../../permutation-server/web/frontier/seal.mjs';
import * as worker from '../../permutation-server/web/frontier/seal-worker.mjs';
import { QUICKNET, TEST_BEACON } from '../../permutation-server/web/frontier/abi.mjs';
import { tlockRound } from '../../permutation-server/web/frontier/clock.mjs';

const read = rel => JSON.parse(readFileSync(new URL(rel, import.meta.url), 'utf8'));
const V = read('../../permutation-rules/vectors/seal-vectors-v1.json');
const F = read('./frontier-vectors.json');
const hex = h => Uint8Array.from(Buffer.from(h, 'hex'));
const toHex = b => Buffer.from(b).toString('hex');
const valid = V.cases.filter(c => c.expect === 'valid');
/** The season clock of a vector case (quicknet, its genesis_ts): T(arrive_bell) = c.round. */
const clockOf = c => ({ genesisTs: c.genesis_ts, drand: { genesis: QUICKNET.genesis, period: QUICKNET.period } });
/** A season clock under `drand` whose T(arriveBell) is `round`. */
const clockAt = (drand, round, arriveBell) => ({ genesisTs: drand.genesis + (round - 1) * drand.period - 600 * (arriveBell + 1), drand: { genesis: drand.genesis, period: drand.period } });

test('the vector file is the one the kernel checks, over quicknet', () => {
  assert.equal(V.network, 'quicknet');
  assert.equal(V.public_key, QUICKNET.publicKey);
  assert.ok(V.cases.length >= 24);
  assert.ok(valid.length >= 5);
});

test('plaintext, salt, commitment, body, ct_hash and seal root match every case', () => {
  for (const c of V.cases) {
    const plain = hex(c.plain);
    const f = seal.unpack(plain);
    assert.deepEqual(seal.pack(f), plain, `${c.name}: pack(unpack)`);
    assert.equal(toHex(seal.saltOf(hex(c.k))), c.salt, `${c.name}: salt`);
    if (c.expect !== 'commit_mismatch') assert.equal(toHex(seal.commit(plain, hex(c.salt))), c.commit, `${c.name}: commit`);
    assert.equal(toHex(seal.ctHash(hex(c.seal))), c.ct_hash, `${c.name}: ct_hash`);
    assert.equal(toHex(seal.sealRoot(hex(c.commit), hex(c.ct_hash))), c.seal_root, `${c.name}: seal_root`);
    if (c.expect === 'valid' || c.expect === 'bad_plaintext') assert.equal(toHex(seal.bodyXor(hex(c.k), plain)), c.seal.slice(256), `${c.name}: body`);
    const why = seal.validate(f, BigInt(c.host_id), c.arrive_bell);
    assert.equal(why ?? 'ok', c.validate, `${c.name}: validate`);
  }
});

test('path encoding is the kernel\'s 3-bit little-endian packing', () => {
  const { pathLen, path } = seal.encodePath([0, 1, 2, 3, 4, 5, 0, 5]);
  assert.equal(pathLen, 8);
  assert.deepEqual([0, 1, 2, 3, 4, 5, 6, 7].map(i => seal.pathStep(path, i)), [0, 1, 2, 3, 4, 5, 0, 5]);
  assert.equal(seal.encodePath(new Array(33).fill(0)), null);
  assert.equal(seal.encodePath([6]), null);
  const full = seal.encodePath(Array.from({ length: 32 }, (_, i) => i % 6));
  assert.equal(seal.validate({ ...seal.unpack(new Uint8Array(37)), version: 1, ...full, destTile: 3 }, 0n, 0), null);
});

test('with the vector\'s k and σ the browser seals the same 165 bytes (tlock 0.0.10 interop)', () => {
  let n = 0;
  for (const c of V.cases.filter(x => (x.expect === 'valid' || x.expect === 'bad_plaintext') && x.sigma)) {
    const out = worker.sealWith({ plain: hex(c.plain), round: c.round, publicKey: V.public_key, k: hex(c.k), sigma: hex(c.sigma) });
    if (c.name === 'recorded_q4_tlockjs') continue; // tlock-js's own random σ: checked by opening below
    assert.equal(toHex(out.seal), c.seal, `${c.name}: seal`);
    assert.equal(toHex(out.sealRoot), c.seal_root, `${c.name}: seal root`);
    n++;
  }
  assert.ok(n >= 15, `${n} seals compared`);
});

test('opening with the recorded quicknet signature: valid cases open, tampered ones fail like the program', () => {
  for (const c of V.cases) {
    const sig = hex(c.open_sig);
    if (c.stock_tlock_opens) {
      const o = worker.openSeal(hex(c.seal), sig);
      assert.equal(toHex(o.k), c.stock_k, `${c.name}: k`);
      const commitOk = toHex(o.commit) === c.commit;
      assert.equal(commitOk, c.expect !== 'commit_mismatch', `${c.name}: commitment`);
      if (commitOk) assert.equal(seal.validate(seal.unpack(o.plain), BigInt(c.host_id), c.arrive_bell) ?? 'ok', c.validate, c.name);
    } else {
      assert.throws(() => worker.openSeal(hex(c.seal), sig), e => e.code === 'FoCheck' || e.code === 'BadPoint', c.name);
    }
  }
});

test('the self-audit accepts a good seal and catches a flipped bit in U, V, W or the body', () => {
  const c = valid[0];
  const k = hex(c.k), sigma = hex(c.sigma), plain = hex(c.plain);
  const out = worker.sealWith({ plain, round: c.round, publicKey: V.public_key, k, sigma });
  const base = { ...out, plain, round: c.round, publicKey: V.public_key, k, sigma, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell, clock: clockOf(c) };
  assert.equal(worker.audit(base), true);
  for (const [what, at] of [['U', 40], ['V', 100], ['W', 120], ['body', 140]]) {
    const bad = Uint8Array.from(out.seal);
    bad[at] ^= 0x04;
    assert.throws(() => worker.audit({ ...base, seal: bad, sealRoot: seal.sealRoot(out.commit, seal.ctHash(bad)) }), e => e.code === 'SealAuditFailed', what);
  }
  assert.throws(() => worker.audit({ ...base, hostId: BigInt(c.host_id) + 1n }), /plaintext \(HostMismatch\)/);
  assert.throws(() => worker.audit({ ...base, commit: new Uint8Array(32) }), /commitment/);
  assert.throws(() => worker.audit({ ...base, round: c.round + 1 }), /round .* is not T/);
  // A seal genuinely made for another round (V consistent with it) is
  // refused too: the round must be T(arrive_bell) (integ-W2 review of W2-E).
  const off = worker.sealWith({ plain, round: c.round + 1, publicKey: V.public_key, k, sigma });
  assert.throws(() => worker.audit({ ...base, ...off, round: c.round + 1 }), /round .* is not T/);
  assert.equal(worker.audit({ ...base, ...off, round: c.round + 1, clock: clockAt(QUICKNET, c.round + 1, c.arrive_bell) }), true, 'the same seal audits under the clock whose T it is');
});

test('sealRequest draws k and σ, audits, answers the material and zeroes the key', () => {
  const c = valid[1];
  const drawn = [];
  const draw = () => { const b = globalThis.crypto.getRandomValues(new Uint8Array(16)); drawn.push(b); return b; };
  const r = worker.sealRequest({ plain: hex(c.plain), round: c.round, publicKey: V.public_key, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell, clock: clockOf(c) }, draw);
  assert.equal(r.ok, true, r.error);
  assert.equal(r.seal.length, 165);
  assert.ok(!('k' in r) && !('sigma' in r), 'k and σ never leave the worker');
  assert.ok(drawn.every(b => b.every(x => x === 0)), 'k and σ zeroed');
  // The answer opens with the round's signature and commits to the plaintext.
  const o = worker.openSeal(r.seal, hex(c.open_sig));
  assert.deepEqual(o.plain, hex(c.plain));
  assert.deepEqual(o.salt, r.salt);
  assert.deepEqual(seal.sealRoot(r.commit, seal.ctHash(r.seal)), r.sealRoot);
  // A plaintext for another transit is refused by the audit, nothing returned.
  const bad = worker.sealRequest({ plain: hex(c.plain), round: c.round, publicKey: V.public_key, hostId: 1n, arriveBell: c.arrive_bell, clock: clockOf(c) });
  assert.equal(bad.ok, false);
  assert.equal(bad.code, 'SealAuditFailed');
  assert.ok(!('seal' in bad));
});

test('sealMarch validates first and seals on this thread when there is no Worker', async () => {
  const c = valid[2];
  // The round comes from the clock: T(arrive_bell).
  const r = await seal.sealMarch({ plain: hex(c.plain), clock: clockOf(c), publicKey: V.public_key, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell }, { inline: true });
  assert.equal(r.ok, true, r.error);
  assert.equal(r.round, c.round);
  assert.equal(seal.sealRound(clockOf(c), c.arrive_bell), tlockRound(clockOf(c).drand, c.genesis_ts, c.arrive_bell), 'sealRound = clock.tlockRound');
  // A round passed in that is not T(arrive_bell) (an off-by-one bell) is refused, nothing sealed.
  for (const round of [c.round + 1, c.round - 1, seal.sealRound(clockOf(c), c.arrive_bell + 1)]) {
    const w = await seal.sealMarch({ plain: hex(c.plain), clock: clockOf(c), round, publicKey: V.public_key, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell }, { inline: true });
    assert.deepEqual([w.ok, w.code, 'seal' in w], [false, 'WrongRound', false], `round ${round}`);
  }
  assert.equal((await seal.sealMarch({ plain: hex(c.plain), publicKey: V.public_key, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell }, { inline: true })).code, 'NoClock');
  const refused = await seal.sealMarch({ plain: hex(V.cases.find(x => x.validate === 'Retreat').plain), clock: clockOf(c), publicKey: V.public_key, hostId: 0n, arriveBell: 0 });
  assert.deepEqual([refused.ok, refused.code], [false, 'BadPlaintext']);
});

test('the beacon pin: quicknet, the test key only on localnet, the Season\'s pk hash', () => {
  const q = { publicKey: QUICKNET.publicKey, chainHash: QUICKNET.chainHash, period: 3, genesis: QUICKNET.genesis };
  assert.equal(QUICKNET.pkHash, toHex(seal.ctHash(hex(QUICKNET.publicKey))), 'pk hash = sha256(pk)');
  assert.deepEqual(seal.checkBeacon({ drand: q, seasonPkHash: QUICKNET.pkHash, cluster: 'devnet' }), { ok: true, kind: 'quicknet', publicKey: QUICKNET.publicKey });
  assert.equal(seal.checkBeacon({ drand: q, seasonPkHash: hex(QUICKNET.pkHash), cluster: 'devnet' }).ok, true);
  assert.equal(seal.checkBeacon({ drand: q, seasonPkHash: TEST_BEACON.pkHash, cluster: 'devnet' }).code, 'PkHashMismatch');
  assert.equal(seal.checkBeacon({ drand: { ...q, chainHash: TEST_BEACON.chainHash }, seasonPkHash: QUICKNET.pkHash }).code, 'ChainHashMismatch');
  assert.equal(seal.checkBeacon({ drand: { ...q, period: 30 }, seasonPkHash: QUICKNET.pkHash }).code, 'BeaconClockMismatch');
  assert.equal(seal.checkBeacon({ drand: { ...q, publicKey: '00'.repeat(96) }, seasonPkHash: QUICKNET.pkHash }).code, 'NotQuicknet');
  const t = { publicKey: TEST_BEACON.publicKey, chainHash: TEST_BEACON.chainHash, period: TEST_BEACON.period, genesis: TEST_BEACON.genesis };
  assert.equal(seal.checkBeacon({ drand: t, seasonPkHash: TEST_BEACON.pkHash, cluster: 'devnet' }).code, 'TestBeaconOffLocalnet');
  assert.deepEqual(seal.checkBeacon({ drand: t, seasonPkHash: TEST_BEACON.pkHash, cluster: 'localnet' }).kind, 'test');
  // T(b) is the first round at or after the bell's end (never before it).
  const c = valid[0];
  assert.equal(tlockRound({ genesis: QUICKNET.genesis, period: QUICKNET.period }, c.genesis_ts, c.arrive_bell), c.round);
});

test('the local test key (I-53): a seal to a test-key round opens with its recorded signature', () => {
  const tk = F.beacon.test_key;
  assert.equal(tk.info.public_key, TEST_BEACON.publicKey);
  const c = valid[0];
  for (const { round, sig } of tk.rounds) {
    const plain = hex(c.plain);
    const r = worker.sealRequest({ plain, round, publicKey: TEST_BEACON.publicKey, hostId: BigInt(c.host_id), arriveBell: c.arrive_bell, clock: clockAt(TEST_BEACON, round, c.arrive_bell) });
    assert.equal(r.ok, true, r.error);
    const o = worker.openSeal(r.seal, hex(sig));
    assert.deepEqual(o.plain, plain, `round ${round}`);
    assert.deepEqual(o.commit, r.commit);
    if (round > 1) assert.throws(() => worker.openSeal(r.seal, hex(tk.rounds[0].sig)), e => e.code === 'FoCheck', 'another round\'s signature fails the FO check');
  }
});
