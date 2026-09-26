// The chain layer: prize pool chip, the chain beat in the top bar (one pulse
// per sealed tick) and the chain lens drawer (` key).
import * as T from './i18n.mjs';
import { $, html, setHtml, fmt, usdc, short, logOnce, verifyCommand, verifyNote } from './util.mjs';
import { S, invalidate } from './state.mjs';
import { poolSharePct } from './rules.mjs';
import { L } from './lang.mjs';

export const poolUsdc = () => (S.view?.projection ? Number(S.view.projection.pool) / 1e6 : 0);

/** Update step for a new view: a newly sealed tick pulses the beat and refreshes the gateway data. */
export function onChainView(v) {
  const t = v.chain?.lastTick;
  if (t && S.lastSealed !== t.tick) { S.lastSealed = t.tick; refreshChain(); }
}

async function loadChain() {
  const g = S.view?.chain?.gateway; if (!g) return;
  // Not api.tryGet: that sends X-Member-Token, which must not go to the
  // (other-origin) gateway, and would add a CORS preflight.
  const json = url => fetch(url, { cache: 'no-store' }).then(r => (r.ok ? r.json() : null)).catch(() => null);
  // Refetched with every sealed tick: members registered later, finalization.
  S.season = (await json(`${g}/season`)) ?? S.season;
  const from = Math.max(0, S.view.tick - 14);
  const t = await json(`${g}/ticks?from=${from}`);
  if (t?.records) S.ticks = t.records.slice(-14).reverse();
  invalidate('pool', 'ribbon', 'chainDrawer');
}
/** loadChain is fire-and-forget (every poll that sees a new sealed tick): log a failure once. */
const chainFailed = logOnce('chain: gateway load failed');
const refreshChain = () => loadChain().catch(chainFailed);

export function toggleChain() {
  S.chainOpen = !S.chainOpen;
  $('#chain-drawer').hidden = !S.chainOpen;
  if (S.chainOpen) { invalidate('chainDrawer'); refreshChain(); }
}

export function renderPool() {
  setHtml($('#pool-chip'), html`<span class="k">${L`賞金プール`}</span><span class="v">${fmt(poolUsdc())}<small>USDC</small></span>`);
  const pct = poolSharePct(S.view);
  $('#pool-chip').title = `${L`参加費 ${usdc(S.view?.season?.entryFee)} USDC × メンバー${(S.view?.members || []).length}人の${pct}%、と市場の手数料・関税の${pct}%`}${S.season ? `\nVault ${S.season.accounts?.vault}` : ''}`;
}

export function renderChainBeat() {
  const c = S.view.chain, el = $('#chain-beat');
  if (!c) { el.className = 'chain-beat local'; setHtml(el, html`<span class="dot"></span><span class="l1">LOCAL</span><span class="l2">${L`チェーンなし`}</span>`); return; }
  const t = c.lastTick;
  // Restart the seal animation once per sealed tick.
  if (t && el.dataset.sealed !== String(t.tick)) { el.dataset.sealed = t.tick; el.classList.remove('sealed'); void el.offsetWidth; el.classList.add('sealed'); }
  el.className = `chain-beat ${el.classList.contains('sealed') ? 'sealed' : ''}`;
  setHtml(el, html`<span class="dot"></span><span class="l1">${t ? L`T${t.tick} 封印 ✓` : L`封印待ち`}</span><span class="l2">${c.layer === 'er' ? 'MagicBlock ER' : 'Solana'} · slot ${fmt(c.slot)}</span>`);
}

/** Only http(s) links from the server become hrefs. */
const safeUrl = u => (/^https?:\/\//.test(String(u ?? '')) ? u : null);

export function renderChainDrawer() {
  if (!S.chainOpen) return;
  const v = S.view, c = v.chain, el = $('#chain-drawer');
  if (!c) { setHtml(el, html`<header><b>⛓ CHAIN</b><button class="x" type="button" data-close-chain>✕</button></header><div class="sec">${L`ローカルモードです。エンジンはこのプロセスで動いています。`}<br><span class="k">${L`--chain で起動すると MagicBlock ER 上のシーズンを表示します`}</span></div>`); return; }
  const er = safeUrl(c.endpoints?.er);
  const explorer = sig => (er ? `https://explorer.solana.com/tx/${encodeURIComponent(sig)}?cluster=custom&customUrl=${encodeURIComponent(er)}` : '#');
  const reg = (S.season?.members || []).find(m => m.index === S.memberId);
  const ticks = (S.ticks || []).map((r, i) => html`<div class="tk ${i === 0 ? 'new' : ''}"><span class="t">T${r.tick}</span><span><span class="m">${fmt(r.cu)} CU · ${r.submitted ?? '?'}/${T.ROLES.length * v.civs.length} offices · root ${String(r.root ?? '').slice(0, 8)}…</span><br><a href="${explorer(r.signature)}" target="_blank" rel="noopener">${short(r.signature)}</a></span><span class="ok">✓</span></div>`);
  const gov = (v.chronicle || []).filter(e => /^(gov|recall|era)\|/.test(e.text)).slice(0, 6).map(e => html`<div class="tk"><span class="t">T${e.tick}</span><span class="m">${T.chronicleText(e.text)[1]}</span></div>`);
  const verify = verifyCommand(c);
  setHtml(el, html`<header><b>⛓ CHAIN LENS</b><button class="x" type="button" data-close-chain>✕</button></header>
    <div class="sec"><span class="k">SEASON</span>${c.seasonId} · ${c.cluster || 'localnet'}<span class="k">LAYER</span>${c.layer === 'er' ? 'MagicBlock Ephemeral Rollup' : 'Solana base'} · slot ${fmt(c.slot)}</div>
    <div class="sec"><span class="k">PRIZE VAULT</span>${fmt(poolUsdc())} USDC · ${short(c.accounts?.vault)}<span class="k">YOUR SESSION KEY</span>${reg ? `${short(reg.session)} · ${S.session?.publicKey === reg.session ? 'held by this browser' : 'not in this browser (view only)'} · signs orders, votes, talk and, as an officer, treasury spending · cannot claim the prize or move wallet tokens` : '—'}</div>
    <div class="sec"><span class="k">GOVERNANCE ON CHAIN</span></div><div class="ticker">${gov.length ? gov : html`<span class="k" style="padding:6px">${L`選挙・解任・時代の記録はまだありません`}</span>`}</div>
    <div class="sec"><span class="k">SEALED TICKS</span></div><div class="ticker">${ticks.length ? ticks : html`<span class="k" style="padding:6px">${L`読み込み中…`}</span>`}</div>
    ${verify ? html`<div class="sec"><span class="k">VERIFY IT YOURSELF</span><pre>${verify.text}</pre><span class="k">${verifyNote(verify)}</span></div>` : ''}`);
}
