// Solana wallets in the browser, through the Wallet Standard (Phantom,
// Solflare, Backpack, …): discovery, connect, and the only two things a
// wallet ever signs here — the free message that makes the session key
// (session.mjs) and the Register / Claim transactions (chainio.mjs; the
// gateway pays every fee, so the wallet needs no SOL).
//
// Discovery follows the standard's two events exactly: listen for
// `wallet-standard:register-wallet` (a wallet that loads later calls
// `event.detail(api)`), then announce `wallet-standard:app-ready` with
// `detail: api` (a wallet that loaded first calls `api.register(wallet)`).
//
// The dev wallet ("Dev Wallet (localnet)") is a Wallet Standard wallet
// backed by a random key in localStorage. It exists only when the gateway
// says `devWallet: true` (`--dev-wallet`, localnet only), and is listed first.
import { fromHex, randomBytes, toHex } from './sdk/bytes.mjs';
import { parseTransaction, wireTransaction } from './sdk/solana-tx.mjs';
import { codeError, keyFromSeed } from './session.mjs';

/** Wallet Standard chain id of a cluster. */
export const chainOf = cluster => ({ localnet: 'solana:localnet', devnet: 'solana:devnet', testnet: 'solana:testnet', mainnet: 'solana:mainnet', 'mainnet-beta': 'solana:mainnet' })[cluster] ?? `solana:${cluster}`;

/** Where to get a wallet when none is installed. */
export const INSTALL = Object.freeze([
  { name: 'Phantom', url: 'https://phantom.com/download' },
  { name: 'Solflare', url: 'https://solflare.com/download' },
  { name: 'Backpack', url: 'https://backpack.app/downloads' },
]);
/** How to point each wallet at devnet (balances and simulation otherwise run on mainnet). */
export const DEVNET_HINTS = Object.freeze({
  Phantom: 'Phantom：設定 → 開発者設定 →「テストネットモード」をオン（Solana Devnet）',
  Solflare: 'Solflare：設定 → ネットワーク →「Devnet」に切り替える',
  Backpack: 'Backpack：設定 → Solana → RPC 接続 →「Devnet」に切り替える',
});
export const devnetHint = name => DEVNET_HINTS[name] ?? `${name}：ウォレットのネットワークをテストネット/devnet に切り替える`;

export const DEV_WALLET_NAME = 'Dev Wallet (localnet)';
const DEV_KEY = 'ps-dev-wallet';
const LAST_KEY = 'ps-wallet';
const DEV_ICON = `data:image/svg+xml;base64,${globalThis.btoa?.('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="#213f34"/><text x="16" y="22" font-size="16" text-anchor="middle" fill="#f3d27a" font-family="monospace">DEV</text></svg>') ?? ''}`;

const local = {
  get(k) { try { return globalThis.localStorage?.getItem(k) ?? null; } catch { return null; } },
  set(k, v) { try { if (v == null) globalThis.localStorage?.removeItem(k); else globalThis.localStorage?.setItem(k, v); } catch { /* unavailable */ } },
};

// ------------------------------------------------------------------ registry
const wallets = new Set();
const listeners = new Set();
let dev = null;
let started = false;

function notify() {
  for (const f of [...listeners]) {
    try { f(); } catch (e) { console.error('wallet listener:', e); }
  }
}
/** Call `fn` whenever the wallets or the connected account change; returns an unsubscribe function. */
export function onChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }

/** What the page hands to wallets: `register(...wallets)` returns an unregister function. */
const api = Object.freeze({
  register(...list) {
    const added = list.filter(w => w && typeof w === 'object' && !wallets.has(w));
    for (const w of added) wallets.add(w);
    if (added.length) notify();
    return () => {
      let removed = false;
      for (const w of added) removed = wallets.delete(w) || removed;
      if (current && !wallets.has(current.wallet)) setCurrent(null);
      if (removed) notify();
    };
  },
});

/** Start listening for wallets (idempotent). */
export function discover(target = globalThis.window) {
  if (started || !target) return;
  started = true;
  try {
    target.addEventListener('wallet-standard:register-wallet', e => {
      try { e.detail(api); } catch (err) { console.warn('wallet registration failed:', err); }
    });
  } catch { /* no events here */ }
  try { target.dispatchEvent(new CustomEvent('wallet-standard:app-ready', { detail: api })); } catch { /* no events here */ }
}

/** Why a wallet cannot be used on `chain` (Japanese), or null. */
export function unsupported(w, chain) {
  const f = w?.features || {};
  if (typeof f['standard:connect']?.connect !== 'function') return '接続に対応していません';
  const st = f['solana:signTransaction'];
  if (typeof st?.signTransaction !== 'function') return '取引の署名に対応していません';
  if (!(st.supportedTransactionVersions || []).includes('legacy')) return '従来形式（legacy）の取引に対応していません';
  if (typeof f['solana:signMessage']?.signMessage !== 'function') return 'メッセージの署名に対応していません';
  if (!(w.chains || []).includes(chain)) return `このネットワーク（${chain}）に対応していません`;
  return null;
}

/**
 * Every wallet found, the dev wallet first: `[{wallet, name, icon, dev, why}]`
 * (`why` = null when it can be used on `chain`, else the reason).
 */
export function list(chain) {
  const all = [...(dev ? [dev] : []), ...[...wallets].filter(w => w !== dev)];
  return all.map(w => ({ wallet: w, name: String(w.name ?? '?'), icon: /^data:image\//.test(String(w.icon ?? '')) ? w.icon : '', dev: w === dev, why: unsupported(w, chain) }));
}

/** The name of the wallet connected last time (for a silent reconnect), or null. */
export const lastWalletName = () => local.get(LAST_KEY);

// ------------------------------------------------------------------ connection
let current = null; // { wallet, account, chain, off }

const accountOk = (a, chain) => a && typeof a.address === 'string'
  && (!a.chains?.length || a.chains.includes(chain))
  && (!a.features?.length || (a.features.includes('solana:signTransaction') && a.features.includes('solana:signMessage')));

function setCurrent(next) {
  if (current?.off && current.wallet !== next?.wallet) {
    try { current.off(); } catch { /* ignore */ }
  }
  current = next;
}

/** Whether an error is the user saying no in the wallet. */
export const isRejection = e => e?.code === 4001 || /reject|denied|cancel|closed/i.test(String(e?.message ?? e ?? ''));

/** A wallet's error as `{code, message}` (Japanese): WalletRejected or WalletError. */
function walletError(e) {
  if (e?.code && /^Wallet|^NoAccount/.test(e.code)) return e;
  const err = isRejection(e) ? codeError('WalletRejected', 'ウォレットで取り消されました')
    : codeError('WalletError', `ウォレットのエラー：${String(e?.message ?? e).slice(0, 200)}`);
  err.cause = e;
  return err;
}

/**
 * Connect a wallet (an entry of `list`, or the wallet itself) for `chain`.
 * `silent`: only if the wallet already trusts this site (no popup); returns
 * null instead of throwing when it does not. Returns the connected handle.
 */
export async function connect(entry, { chain, silent = false } = {}) {
  const w = entry?.wallet ?? entry;
  const why = unsupported(w, chain);
  if (why) { if (silent) return null; throw codeError('WalletUnsupported', `${w?.name ?? 'このウォレット'}は使えません：${why}`); }
  let accounts;
  try {
    accounts = (await w.features['standard:connect'].connect(silent ? { silent: true } : undefined))?.accounts;
  } catch (e) {
    if (silent) return null;
    throw walletError(e);
  }
  const account = [...(accounts?.length ? accounts : w.accounts || [])].find(a => accountOk(a, chain));
  if (!account) {
    if (silent) return null;
    throw codeError('NoAccount', `${w.name}に、このネットワーク（${chain}）で使えるアカウントがありません`);
  }
  setCurrent({ wallet: w, account, chain, off: current?.wallet === w ? current.off : null });
  if (!current.off) {
    try {
      current.off = w.features['standard:events']?.on?.('change', props => {
        if (current?.wallet !== w || !props?.accounts) return;
        // The user switched (or removed) the account in the wallet: follow it.
        const next = (w.accounts || []).find(a => accountOk(a, chain));
        if (!next) setCurrent(null);
        else if (next.address !== current.account.address) current.account = next;
        notify();
      }) ?? null;
    } catch { /* no events */ }
  }
  local.set(LAST_KEY, w.name);
  notify();
  return connected();
}

/**
 * The connected wallet: `{name, icon, address, dev, signMessage(bytes),
 * signTransaction(wireBytes)}`, or null.
 */
export function connected() {
  if (!current) return null;
  const { wallet: w, account } = current;
  return Object.freeze({
    name: String(w.name), icon: /^data:image\//.test(String(w.icon ?? '')) ? w.icon : '', address: account.address, dev: w === dev,
    signMessage, signTransaction,
  });
}

/** Forget the connection (and the remembered wallet). */
export async function disconnect() {
  const c = current;
  setCurrent(null);
  local.set(LAST_KEY, null);
  try { await c?.wallet.features['standard:disconnect']?.disconnect?.(); } catch { /* ignore */ }
  notify();
}

function need() {
  if (!current) throw codeError('NoWallet', 'ウォレットが接続されていません');
  return current;
}

/**
 * Sign a message with the connected account: `{signedMessage, signature}`
 * (Uint8Arrays; `signedMessage` is what the wallet actually signed).
 */
export async function signMessage(bytes) {
  const c = need();
  let out;
  try {
    [out] = await c.wallet.features['solana:signMessage'].signMessage({ account: c.account, message: bytes });
  } catch (e) { throw walletError(e); }
  if (!out?.signature) throw codeError('WalletError', 'ウォレットが署名を返しませんでした');
  return { signedMessage: Uint8Array.from(out.signedMessage ?? bytes), signature: Uint8Array.from(out.signature) };
}

/**
 * Have the connected account sign a legacy wire transaction (other
 * signatures may be empty); returns the wire bytes the wallet gives back.
 * The caller checks the message came back unchanged (chainio.mjs).
 */
export async function signTransaction(wire) {
  const c = need();
  let out;
  try {
    [out] = await c.wallet.features['solana:signTransaction'].signTransaction({ account: c.account, transaction: wire, chain: c.chain });
  } catch (e) { throw walletError(e); }
  if (!out?.signedTransaction) throw codeError('WalletError', 'ウォレットが署名済みの取引を返しませんでした');
  return Uint8Array.from(out.signedTransaction);
}

// ------------------------------------------------------------------ dev wallet
/** A Wallet Standard wallet that signs with `key` (keyFromSeed) and never asks. */
export function devWallet(key) {
  const account = Object.freeze({ address: key.publicKey, publicKey: key.publicKeyBytes, chains: ['solana:localnet'], features: ['solana:signTransaction', 'solana:signMessage'] });
  return {
    version: '1.0.0', name: DEV_WALLET_NAME, icon: DEV_ICON, chains: ['solana:localnet'], accounts: [account],
    features: {
      'standard:connect': { version: '1.0.0', connect: async () => ({ accounts: [account] }) },
      'standard:disconnect': { version: '1.0.0', disconnect: async () => {} },
      'standard:events': { version: '1.0.0', on: () => () => {} },
      'solana:signTransaction': {
        version: '1.0.0', supportedTransactionVersions: ['legacy'],
        signTransaction: (...inputs) => Promise.all(inputs.map(async ({ transaction }) => {
          const tx = parseTransaction(transaction);
          const i = tx.signers.indexOf(key.publicKey);
          if (i < 0) throw new Error('the dev wallet is not a signer of this transaction');
          const sigs = tx.signatures.slice();
          sigs[i] = await key.sign(tx.message);
          return { signedTransaction: wireTransaction(tx.message, sigs) };
        })),
      },
      'solana:signMessage': {
        version: '1.0.0',
        signMessage: (...inputs) => Promise.all(inputs.map(async ({ message }) => ({ signedMessage: Uint8Array.from(message), signature: await key.sign(message) }))),
      },
    },
  };
}

/**
 * Offer the dev wallet (idempotent). Only on localnet and only when the
 * gateway allows it (`/season` `devWallet: true`); the key is random and
 * kept in this browser.
 */
export async function enableDevWallet({ cluster, allowed }) {
  if (dev || !allowed || cluster !== 'localnet') return !!dev;
  let hex = local.get(DEV_KEY);
  if (!/^[0-9a-f]{64}$/.test(hex ?? '')) { hex = toHex(randomBytes(32)); local.set(DEV_KEY, hex); }
  dev = devWallet(await keyFromSeed(fromHex(hex)));
  api.register(dev);
  return true;
}
