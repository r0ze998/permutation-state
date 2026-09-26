// The web client's Wallet Standard glue (permutation-server/web/wallet.mjs):
// the exact discovery handshake (both orders), which wallets are usable,
// connect (silent and not), the sign calls' shapes, rejections, account
// changes, and the dev wallet (localnet only, signs what node verifies).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verify as nodeVerify, createPublicKey } from 'node:crypto';
import bs58 from 'bs58';
import * as wallet from '../../permutation-server/web/wallet.mjs';
import { compileMessage, parseTransaction, wireTransaction } from '../client/src/solana-tx.mjs';

const CHAIN = 'solana:devnet';
const SPKI = Buffer.from('302a300506032b6570032100', 'hex');
const nodeOk = (address, msg, sig) => nodeVerify(null, Buffer.from(msg), createPublicKey({ key: Buffer.concat([SPKI, bs58.decode(address)]), format: 'der', type: 'spki' }), Buffer.from(sig));

/** A Wallet Standard wallet that records its calls; `opts` override features. */
function fakeWallet(name, opts = {}) {
  const calls = [];
  const account = { address: bs58.encode(Buffer.alloc(32, name.length)), publicKey: new Uint8Array(32), chains: opts.accountChains ?? [CHAIN], features: ['solana:signTransaction', 'solana:signMessage'] };
  const handlers = [];
  const w = {
    version: '1.0.0', name, icon: 'data:image/svg+xml;base64,AAAA', chains: opts.chains ?? [CHAIN], accounts: [],
    features: {
      'standard:connect': { version: '1.0.0', connect: async input => { calls.push(['connect', input]); if (opts.refuse) throw Object.assign(new Error('User rejected the request.'), { code: 4001 }); if (input?.silent && !opts.trusted) return { accounts: [] }; w.accounts = [account]; return { accounts: [account] }; } },
      'standard:events': { version: '1.0.0', on: (ev, cb) => { handlers.push(cb); return () => handlers.splice(handlers.indexOf(cb), 1); } },
      'solana:signTransaction': { version: '1.0.0', supportedTransactionVersions: opts.versions ?? ['legacy', 0], signTransaction: async (...inputs) => { calls.push(['signTransaction', inputs]); return inputs.map(i => ({ signedTransaction: i.transaction })); } },
      'solana:signMessage': { version: '1.0.0', signMessage: async (...inputs) => { calls.push(['signMessage', inputs]); return inputs.map(i => ({ signedMessage: i.message, signature: new Uint8Array(64).fill(9) })); } },
      ...(opts.features ?? {}),
    },
  };
  return { w, calls, account, emit: props => handlers.forEach(h => h(props)) };
}

test('discovery: a wallet that loaded first registers on app-ready; a later one via register-wallet', () => {
  const target = new EventTarget();
  const early = fakeWallet('Early');
  // The wallet's side of the handshake: register when the app says it is ready.
  target.addEventListener('wallet-standard:app-ready', e => { assert.equal(typeof e.detail.register, 'function'); e.detail.register(early.w); });
  let changes = 0;
  const off = wallet.onChange(() => changes++);
  wallet.discover(target);
  wallet.discover(target); // idempotent
  assert.deepEqual(wallet.list(CHAIN).map(x => x.name), ['Early']);
  // A wallet that loads after the app dispatches register-wallet with a callback.
  const late = fakeWallet('Late');
  let unregister;
  target.dispatchEvent(new CustomEvent('wallet-standard:register-wallet', { detail: api => { unregister = api.register(late.w); } }));
  assert.deepEqual(wallet.list(CHAIN).map(x => x.name), ['Early', 'Late']);
  assert.ok(changes >= 2);
  unregister();
  assert.deepEqual(wallet.list(CHAIN).map(x => x.name), ['Early']);
  off();
});

test('unsupported: connect, legacy transactions, signMessage and the season chain are required', () => {
  assert.equal(wallet.unsupported(fakeWallet('A').w, CHAIN), null);
  assert.match(wallet.unsupported(fakeWallet('B', { versions: [0] }).w, CHAIN), /legacy/);
  assert.match(wallet.unsupported(fakeWallet('C', { chains: ['solana:mainnet'] }).w, CHAIN), /solana:devnet/);
  assert.match(wallet.unsupported(fakeWallet('D', { features: { 'solana:signMessage': undefined } }).w, CHAIN), /メッセージ/);
  assert.equal(wallet.chainOf('devnet'), 'solana:devnet');
  assert.equal(wallet.chainOf('localnet'), 'solana:localnet');
  assert.equal(wallet.chainOf('mainnet'), 'solana:mainnet');
  assert.match(wallet.devnetHint('Phantom'), /テストネットモード/);
  assert.match(wallet.devnetHint('Other'), /devnet/);
});

test('connect: silent needs trust; the sign calls pass the exact account and chain; rejections are WalletRejected', async () => {
  const f = fakeWallet('Signer');
  assert.equal(await wallet.connect(f.w, { chain: CHAIN, silent: true }), null);
  assert.deepEqual(f.calls.at(-1), ['connect', { silent: true }]);
  const h = await wallet.connect(f.w, { chain: CHAIN });
  assert.equal(h.address, f.account.address);
  assert.equal(wallet.connected().address, f.account.address);
  const msg = new Uint8Array([1, 2, 3]);
  const signed = await wallet.signMessage(msg);
  assert.deepEqual(f.calls.at(-1), ['signMessage', [{ account: f.account, message: msg }]]);
  assert.equal(signed.signature.length, 64);
  const wire = new Uint8Array([0, 1, 2]);
  assert.deepEqual(await wallet.signTransaction(wire), wire);
  assert.deepEqual(f.calls.at(-1), ['signTransaction', [{ account: f.account, transaction: wire, chain: CHAIN }]]);

  // The user switches accounts in the wallet: the connection follows.
  const other = { ...f.account, address: bs58.encode(Buffer.alloc(32, 77)) };
  f.w.accounts = [other];
  f.emit({ accounts: [other] });
  assert.equal(wallet.connected().address, other.address);
  f.w.accounts = [];
  f.emit({ accounts: [] });
  assert.equal(wallet.connected(), null);
  await assert.rejects(wallet.signMessage(msg), e => e.code === 'NoWallet');

  const no = fakeWallet('Refuses', { refuse: true });
  await assert.rejects(wallet.connect(no.w, { chain: CHAIN }), e => e.code === 'WalletRejected');
  assert.equal(await wallet.connect(no.w, { chain: CHAIN, silent: true }), null);
  const wrongChain = fakeWallet('Mainnet', { accountChains: ['solana:mainnet'] });
  await assert.rejects(wallet.connect(wrongChain.w, { chain: CHAIN }), e => e.code === 'NoAccount');
  assert.equal(wallet.isRejection(new Error('Transaction cancelled')), true);
  assert.equal(wallet.isRejection({ code: 4001 }), true);
  assert.equal(wallet.isRejection(new Error('insufficient funds')), false);
  await wallet.disconnect();
});

test('dev wallet: only on localnet when allowed, listed first, signs what node verifies', async () => {
  assert.equal(await wallet.enableDevWallet({ cluster: 'devnet', allowed: true }), false);
  assert.equal(await wallet.enableDevWallet({ cluster: 'localnet', allowed: false }), false);
  assert.equal(wallet.list('solana:localnet').some(x => x.dev), false);
  assert.equal(await wallet.enableDevWallet({ cluster: 'localnet', allowed: true }), true);
  const [first] = wallet.list('solana:localnet');
  assert.equal(first.dev, true);
  assert.equal(first.name, wallet.DEV_WALLET_NAME);
  assert.equal(first.why, null);
  assert.match(first.icon, /^data:image\/svg\+xml;base64,/);
  const h = await wallet.connect(first, { chain: 'solana:localnet' });
  assert.equal(h.dev, true);
  // signMessage
  const msg = new TextEncoder().encode('hello');
  const { signedMessage, signature } = await h.signMessage(msg);
  assert.deepEqual(signedMessage, msg);
  assert.ok(nodeOk(h.address, msg, signature));
  // signTransaction: its own slot only, message untouched.
  const feePayer = bs58.encode(Buffer.alloc(32, 3));
  const message = compileMessage({ feePayer, recentBlockhash: bs58.encode(Buffer.alloc(32, 4)), instructions: [
    { programId: bs58.encode(Buffer.alloc(32, 5)), keys: [{ pubkey: h.address, isSigner: true, isWritable: false }], data: new Uint8Array([1]) }] });
  const tx = parseTransaction(await h.signTransaction(wireTransaction(message)));
  assert.deepEqual(tx.message, message);
  assert.deepEqual(tx.signatures[0], new Uint8Array(64));
  assert.ok(nodeOk(h.address, message, tx.signatures[1]));
  await wallet.disconnect();
});
