// The web client's session keys (permutation-server/web/session.mjs), run
// in Node's WebCrypto: the pure-JS Ed25519 public key against node:crypto,
// the text a wallet signs, deriving the same key from the same signature,
// refusing a signature that does not verify, keeping keys per wallet and
// season, and importing a backup.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign as nodeSign } from 'node:crypto';
import bs58 from 'bs58';
import * as session from '../../permutation-server/web/session.mjs';
import { sha256 } from '../client/src/sha256.mjs';

const PKCS8 = Buffer.from('302e020100300506032b657004220420', 'hex');
const nodePub = seed => createPublicKey(createPrivateKey({ key: Buffer.concat([PKCS8, seed]), format: 'der', type: 'pkcs8' }))
  .export({ format: 'der', type: 'spki' }).subarray(-32);

/** A fake connected wallet (wallet.mjs shape) backed by a node key; `mangle` changes what it returns. */
function fakeWallet(mangle = x => x) {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const address = bs58.encode(publicKey.export({ format: 'der', type: 'spki' }).subarray(-32));
  return {
    address,
    signMessage: async message => mangle({ signedMessage: message, signature: new Uint8Array(nodeSign(null, Buffer.from(message), privateKey)) }),
  };
}
const scope = { cluster: 'localnet', programId: '11111111111111111111111111111111', seasonId: '7' };

test('publicKeyOf matches node:crypto on random seeds and edge seeds', async () => {
  const seeds = [Buffer.alloc(32), Buffer.alloc(32, 0xff), ...Array.from({ length: 40 }, () => randomBytes(32))];
  for (const seed of seeds) assert.deepEqual(Buffer.from(await session.publicKeyOf(seed)), nodePub(seed), seed.toString('hex'));
});

test('keyFromSeed signs what node verifies; verify accepts it and refuses tampering', async () => {
  const seed = randomBytes(32);
  const k = await session.keyFromSeed(seed);
  assert.equal(k.publicKey, bs58.encode(nodePub(seed)));
  const msg = new TextEncoder().encode('hello');
  const sig = await k.sign(msg);
  assert.equal(sig.length, 64);
  assert.equal(await session.verify(k.publicKey, msg, sig), true);
  assert.equal(await session.verify(k.publicKeyBytes, msg, sig), true);
  const bad = Uint8Array.from(sig); bad[5] ^= 1;
  assert.equal(await session.verify(k.publicKey, msg, bad), false);
  assert.equal(await session.verify(k.publicKey, new TextEncoder().encode('hellp'), sig), false);
  assert.equal(await session.verify('not a key', msg, sig), false);
  await assert.rejects(session.keyFromSeed(new Uint8Array(31)), /32 bytes/);
});

test('preflight: needs a secure context; passes with WebCrypto Ed25519', async () => {
  assert.deepEqual(await session.preflight({ secure: true }), { ok: true });
  const r = await session.preflight({ secure: false });
  assert.equal(r.ok, false);
  assert.equal(r.code, 'InsecureContext');
  assert.match(r.error, /https か 127\.0\.0\.1 で開いてください/);
});

test('sessionText is the fixed printable text naming site, cluster, program and season', () => {
  const t = session.sessionText({ host: 'play.example:8443', cluster: 'devnet', programId: 'Prog111', seasonId: '42' });
  assert.equal(t, 'Permutation State wants you to create an in-game key.\nSite: play.example:8443\nCluster: devnet\nProgram: Prog111\nSeason: 42\n'
    + "Anyone holding this key can act as you in this season (orders, votes, and your nation's treasury if you hold an office). "
    + 'It cannot claim your prize or move tokens from your wallet.\nSign only on play.example:8443.');
  assert.match(t, /^[\x20-\x7e\n]+$/);
  assert.throws(() => session.sessionText({ host: 'ドメイン', cluster: 'devnet', programId: 'P', seasonId: 1 }), /printable ASCII/);
});

test('derive: the seed is sha256("PS/session/v1" ‖ signature); the same wallet makes the same key', async () => {
  const w = fakeWallet();
  const a = await session.derive({ wallet: w, ...scope, host: '127.0.0.1:4185' });
  const b = await session.derive({ wallet: w, ...scope, host: '127.0.0.1:4185' });
  assert.equal(a.publicKey, b.publicKey);
  assert.equal(a.wallet, w.address);
  // The key is the documented function of the wallet's signature.
  const { signature } = await w.signMessage(new TextEncoder().encode(session.sessionText({ host: '127.0.0.1:4185', ...scope })));
  const seed = sha256(new TextEncoder().encode('PS/session/v1'), signature);
  assert.equal(a.seedHex(), Buffer.from(seed).toString('hex'));
  assert.equal(a.publicKey, bs58.encode(nodePub(Buffer.from(seed))));
  // Another site, cluster or season gives another key.
  assert.notEqual((await session.derive({ wallet: w, ...scope, host: 'evil.example' })).publicKey, a.publicKey);
  assert.notEqual((await session.derive({ wallet: w, ...scope, seasonId: '8', host: '127.0.0.1:4185' })).publicKey, a.publicKey);
});

test('derive refuses a signature that does not verify against the wallet', async () => {
  const flip = fakeWallet(o => { const s = Uint8Array.from(o.signature); s[0] ^= 1; return { ...o, signature: s }; });
  await assert.rejects(session.derive({ wallet: flip, ...scope, host: 'h' }), e => e.code === 'WalletBadSignature');
  const other = fakeWallet();
  const liar = { address: other.address, signMessage: fakeWallet().signMessage };
  await assert.rejects(session.derive({ wallet: liar, ...scope, host: 'h' }), e => e.code === 'WalletBadSignature');
  // A wallet that signs a prefixed message is checked over what it signed.
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const prefixed = {
    address: bs58.encode(publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)),
    signMessage: async m => { const signedMessage = new Uint8Array([0xff, ...m]); return { signedMessage, signature: new Uint8Array(nodeSign(null, Buffer.from(signedMessage), privateKey)) }; },
  };
  assert.ok((await session.derive({ wallet: prefixed, ...scope, host: 'h' })).publicKey);
});

test('remember / restore / cached / forget: per wallet and season (memory when there is no localStorage)', async () => {
  const w1 = fakeWallet(), w2 = fakeWallet();
  const s1 = await session.derive({ wallet: w1, ...scope, host: 'h' });
  const s2 = await session.derive({ wallet: w2, ...scope, host: 'h' });
  assert.equal(typeof session.remember(s1), 'boolean'); // false where there is no localStorage (memory only)
  session.remember(s2);
  assert.equal(session.storeKey(scope, w1.address), `ps-session:localnet:${scope.programId}:7:${w1.address}`);
  assert.equal((await session.restore(scope, w1.address)).publicKey, s1.publicKey);
  assert.equal(await session.restore({ ...scope, seasonId: '8' }, w1.address), null);
  const all = (await session.cached(scope)).map(s => s.publicKey).sort();
  assert.deepEqual(all, [s1.publicKey, s2.publicKey].sort());
  const r = await session.restore(scope, w2.address);
  const msg = new Uint8Array([1, 2, 3]);
  assert.equal(await session.verify(s2.publicKey, msg, await r.sign(msg)), true);
  session.forget(s1);
  assert.equal(await session.restore(scope, w1.address), null);
  session.forget(s2);
});

test('backups: parse the backup text or bare hex; import only a member\'s key', async () => {
  const w = fakeWallet();
  const s = await session.derive({ wallet: w, ...scope, host: 'h' });
  const text = s.backupText();
  assert.match(text, new RegExp(`Key: ${s.seedHex()}`));
  assert.equal(Buffer.from(session.parseBackup(text)).toString('hex'), s.seedHex());
  assert.equal(Buffer.from(session.parseBackup(`  ${s.seedHex().toUpperCase()}\n`)).toString('hex'), s.seedHex());
  assert.equal(session.parseBackup('abc'), null);
  const members = [{ index: 0, wallet: 'x', session: 'y' }, { index: 3, wallet: w.address, session: s.publicKey }];
  const { session: imported, member } = await session.importBackup(text, scope, members);
  assert.equal(member.index, 3);
  assert.equal(imported.publicKey, s.publicKey);
  assert.equal((await session.restore(scope, w.address)).publicKey, s.publicKey);
  await assert.rejects(session.importBackup(text, scope, members.slice(0, 1)), e => e.code === 'SessionMismatch');
  await assert.rejects(session.importBackup('nothing here', scope, members), e => e.code === 'BadBackup');
  session.forget(imported);
});
