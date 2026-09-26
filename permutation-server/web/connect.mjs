// The wallet picker shared by the registration lobby and the claim card:
// the wallets found (the dev wallet first; ones that cannot be used here
// greyed out with the reason), install links when there is none, and how to
// point each wallet at devnet. Keeps S.wallet in step with the wallet.
import * as wallet from './wallet.mjs';
import { html, short } from './util.mjs';
import { S } from './state.mjs';
import { codeError } from './session.mjs';
import { L, Lh } from './lang.mjs';

// The user may switch or remove the account inside the wallet.
wallet.onChange(() => { S.wallet = wallet.connected(); });

/** Markup of the picker for `cluster` (buttons carry `data-wallet-connect="<name>"`). */
export function walletPicker(cluster) {
  const chain = wallet.chainOf(cluster);
  const all = wallet.list(chain);
  const usable = all.filter(e => !e.why);
  const icon = e => (e.icon ? html`<img class="wallet-icon" src="${e.icon}" alt="">` : html`<span class="wallet-icon"></span>`);
  const buttons = all.map(e => (e.why
    ? html`<button class="btn wallet-btn" type="button" disabled title="${e.why}">${icon(e)}${e.name}<small>${e.why}</small></button>`
    : html`<button class="btn wallet-btn ${e.dev ? 'dev' : ''}" type="button" data-wallet-connect="${e.name}">${icon(e)}${e.name}</button>`));
  const links = wallet.INSTALL.map((w, i) => html`${i ? L`・` : ''}<a href="${w.url}" target="_blank" rel="noopener">${w.name}</a>`);
  const install = html`<p class="desc">${all.length
    ? Lh`Solana のウォレットがほかに必要です：${links}。インストール後にページを再読み込みしてください（拡張機能はページを開いたときに読み込まれます）。`
    : Lh`Solana のウォレットが必要です：${links}。インストール後にページを再読み込みしてください（拡張機能はページを開いたときに読み込まれます）。`}</p>`;
  let hints = '';
  if (cluster === 'devnet') {
    const names = usable.filter(e => !e.dev).map(e => e.name);
    const list = (names.length ? names : ['Phantom', 'Solflare', 'Backpack']).map(wallet.devnetHint);
    hints = html`<div class="explanation">${L`このシーズンは devnet（テストネット）です。接続する前に、ウォレットをテストネット/devnet に切り替えてください。`}${list.map(h => html`<br>${h}`)}</div>`;
  } else if (cluster === 'localnet' && !all.some(e => e.dev)) {
    hints = html`<div class="explanation">${Lh`ローカルネットです。ふつうのウォレットはこのチェーンに接続できません（ゲートウェイを <code>--dev-wallet</code> 付きで起動すると Dev Wallet が使えます）。`}</div>`;
  }
  return html`${buttons.length ? html`<div class="wallet-list">${buttons}</div>` : ''}${usable.length ? '' : install}${hints}`;
}

/** The connected wallet, one line. */
export const walletLine = w => html`<span class="wallet-line">${w.icon ? html`<img class="wallet-icon" src="${w.icon}" alt="">` : ''}<b>${w.name}</b> <code>${short(w.address, 4, 4)}</code></span>`;

/** Connect the wallet named `name` for `cluster` (asks the user). Sets and returns S.wallet. */
export async function connectWallet(name, cluster) {
  const chain = wallet.chainOf(cluster);
  const e = wallet.list(chain).find(x => x.name === name && !x.why);
  if (!e) throw codeError('NoWallet', L`そのウォレットは見つかりません`);
  S.wallet = await wallet.connect(e, { chain });
  return S.wallet;
}

/** Reconnect the wallet used last time if it already trusts this site (no popup). Sets S.wallet; returns it or null. */
export async function reconnectSilently(cluster) {
  const name = wallet.lastWalletName();
  if (!name || S.wallet) return S.wallet;
  const chain = wallet.chainOf(cluster);
  const e = wallet.list(chain).find(x => x.name === name && !x.why);
  if (!e) return null;
  try { S.wallet = await wallet.connect(e, { chain, silent: true }); } catch { S.wallet = null; }
  return S.wallet;
}

export async function disconnectWallet() {
  await wallet.disconnect();
  S.wallet = null;
}
