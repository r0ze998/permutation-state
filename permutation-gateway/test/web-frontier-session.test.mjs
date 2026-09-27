// The Frontier's in-game key (permutation-server/web/frontier/fsession.mjs):
// the text a wallet signs pinned byte for byte (contract §9.1: never
// translated, printable ASCII, names site, cluster, program and season),
// the seed domain, keys distinct from v9's (other text, domain and storage
// prefix; web/session.mjs untouched), deriving the same key from the same
// signature, refusing a bad or altered signature, keeping keys per wallet
// and season, the Citizen check, and importing a backup.
import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign as nodeSign } from 'node:crypto';
import { readFileSync } from 'node:fs';
import bs58 from 'bs58';
import * as fs from '../../permutation-server/web/frontier/fsession.mjs';
import * as v9 from '../../permutation-server/web/session.mjs';
import { sha256 } from '../client/src/sha256.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';

afterEach(() => setLang('ja'));

function fakeWallet(mangle = x => x) {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const address = bs58.encode(publicKey.export({ format: 'der', type: 'spki' }).subarray(-32));
  return { address, signMessage: async m => mangle({ signedMessage: m, signature: new Uint8Array(nodeSign(null, Buffer.from(m), privateKey)) }) };
}
const scope = { cluster: 'localnet', programId: 'J5yoiT3bVuC7pLFPJeS2yqLTouT5BHxBivvpqkG2jvAz', seasonId: '1' };
const where = { origin: 'http://127.0.0.1:41040', host: '127.0.0.1:41040' };

test('the session text is pinned byte for byte, printable ASCII, never translated', () => {
  const want = 'Permutation State: Sixfold Frontier wants you to create an in-game key.\n'
    + 'Site: http://127.0.0.1:41040\n'
    + 'Cluster: localnet\n'
    + 'Program: J5yoiT3bVuC7pLFPJeS2yqLTouT5BHxBivvpqkG2jvAz\n'
    + 'Season: 1\n'
    + 'Anyone holding this key can act as you in this season (build, train, march, explore). It cannot move tokens from your wallet.\n'
    + 'Sign only on 127.0.0.1:41040.';
  assert.equal(fs.sessionText({ ...where, ...scope }), want);
  setLang('en');
  assert.equal(fs.sessionText({ ...where, ...scope }), want, 'the language does not change it');
  assert.equal(Buffer.from(sha256(Buffer.from(want))).toString('hex'), 'af2129ba1bd6d6a9649f672eaf33edf6e1c9d527f205a0c5651e07f71c4e79bb', 'the pinned text\'s sha256');
  assert.throws(() => fs.sessionText({ ...where, ...scope, programId: 'Prög' }), e => e.code === 'BadSessionText');
  assert.throws(() => fs.sessionText({ ...where, ...scope, cluster: '本番' }), /printable ASCII/);
  // The source keeps the text in one place and is never passed through L`…`.
  const src = readFileSync(new URL('../../permutation-server/web/frontier/fsession.mjs', import.meta.url), 'utf8');
  assert.equal((src.match(/Sixfold Frontier wants you/g) || []).length, 1);
  assert.doesNotMatch(src, /L`Permutation State/);
});

test('distinct from v9: another text, seed domain and storage prefix; v9 session.mjs untouched', () => {
  assert.equal(fs.SEED_DOMAIN, 'PS/frontier-session/v1');
  assert.notEqual(fs.sessionText({ ...where, ...scope }), v9.sessionText({ host: where.host, ...scope }));
  const sig = new Uint8Array(64).fill(3);
  assert.notDeepEqual(Buffer.from(fs.seedOf(sig)), Buffer.from(v9.seedOf(sig)));
  assert.equal(fs.storeKey(scope, 'W'), 'ps-fsession:localnet:J5yoiT3bVuC7pLFPJeS2yqLTouT5BHxBivvpqkG2jvAz:1:W');
  assert.notEqual(fs.storeKey(scope, 'W'), v9.storeKey(scope, 'W'));
  assert.deepEqual(Buffer.from(fs.seedOf(sig)), Buffer.from(sha256(Buffer.concat([Buffer.from('PS/frontier-session/v1'), Buffer.from(sig)]))));
});

test('derive: the same signature makes the same key; a bad or altered signature is refused', async () => {
  const w = fakeWallet();
  const a = await fs.derive({ wallet: w, ...scope, ...where });
  const b = await fs.derive({ wallet: w, ...scope, ...where });
  assert.equal(a.publicKey, b.publicKey, 'Ed25519 signatures are deterministic');
  assert.equal(a.wallet, w.address);
  const msg = new TextEncoder().encode('Harvest');
  assert.equal(await v9.verify(a.publicKey, msg, await a.sign(msg)), true);
  await assert.rejects(fs.derive({ wallet: fakeWallet(o => ({ ...o, signature: o.signature.map((x, i) => (i ? x : x ^ 1)) })), ...scope, ...where }), e => e.code === 'WalletBadSignature');
  await assert.rejects(fs.derive({ wallet: fakeWallet(o => ({ ...o, signedMessage: new TextEncoder().encode('other') })), ...scope, ...where }), e => e.code === 'WalletAlteredMessage');
  const other = await fs.derive({ wallet: w, ...scope, ...where, seasonId: '2' });
  assert.notEqual(other.publicKey, a.publicKey, 'another season, another key');
});

test('keep, restore and forget per wallet and season; the Citizen check; backup import', async () => {
  const w = fakeWallet();
  const s = await fs.derive({ wallet: w, ...scope, ...where });
  fs.remember(s);
  const back = await fs.restore(scope, w.address);
  assert.equal(back.publicKey, s.publicKey);
  assert.equal(await fs.restore({ ...scope, seasonId: '9' }, w.address), null);
  const citizen = { session: bs58.decode(s.publicKey), sessionExpiry: 5000n };
  assert.deepEqual(fs.matchesCitizen(s, citizen, 4999), { ok: true });
  assert.deepEqual(fs.matchesCitizen(s, citizen, 5000), { ok: false, code: 'SessionExpired' });
  assert.deepEqual(fs.matchesCitizen(s, { session: new Uint8Array(32) }), { ok: false, code: 'NoSession' });
  assert.deepEqual(fs.matchesCitizen(s, { session: new Uint8Array(32).fill(1) }), { ok: false, code: 'SessionMismatch' });
  const text = s.backupText();
  assert.match(text, /Sixfold Frontier key/);
  fs.forget(s);
  assert.equal(await fs.restore(scope, w.address), null);
  const { session, citizen: c } = await fs.importBackup(text, scope, [{ wallet: 'x', session: 'y' }, { wallet: w.address, session: s.publicKey }]);
  assert.equal(session.publicKey, s.publicKey);
  assert.equal(c.wallet, w.address);
  assert.equal((await fs.restore(scope, w.address)).publicKey, s.publicKey);
  await assert.rejects(fs.importBackup(text, scope, []), e => e.code === 'SessionMismatch');
  await assert.rejects(fs.importBackup('nothing here', scope, []), e => e.code === 'BadBackup');
});
