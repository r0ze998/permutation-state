// Markets: the in-game gold market (iron, horses ⇄ gold) and the USDC
// exchange between national treasuries.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { $, html, attrJson, fmt, usdc, toast } from '../util.mjs';
import { S, held, myNation, invalidate } from '../state.mjs';
import { addDraft } from '../orders.mjs';
import { AMM_FEE_TEXT, EXCHANGE_FEE, marketFreeze, poolSharePct } from '../rules.mjs';
import { L, Lh } from '../lang.mjs';

// ================================================================== gold market
export async function quoteAmm() {
  const good = $('#amm-good').value, side = $('#amm-side').value, amount = Math.max(1, +$('#amm-amount').value || 1);
  const qt = await api.tryGet(`/api/preview/amm?good=${good}&side=${side}&amount=${amount}`);
  if (!qt) return;
  S.marketQuote = qt.filled ? { ...qt, good, side, amount } : null;
  if (!qt.filled) toast(L`その数量は市場の在庫を超えています。`, 'error');
  invalidate('drawer');
}

function goldMarket(v, hasCurrency) {
  const q = S.marketQuote;
  let quote = '';
  if (q) {
    const impactPct = q.impact * 100;
    const impactColor = Math.abs(impactPct) > 5 ? 'var(--bad)' : Math.abs(impactPct) > 1 ? 'var(--warn)' : 'var(--good)';
    const dto = { type: 'MarketTrade', good: { kind: q.good }, side: q.side, amount: q.amount, limitGold: q.side === 'Buy' ? Math.ceil(Math.abs(q.gold) * 1.02) : Math.floor(Math.abs(q.gold) * .98) };
    quote = html`<div class="quote"><span class="big">${L`${q.side === 'Buy' ? '−' : '+'}${Math.abs(q.gold).toFixed(1)}金 → ${q.side === 'Buy' ? '+' : '−'}${q.amount} ${T.RESOURCE[q.good]}`}</span>
      <span>${Lh`単価 ${q.price.toFixed(2)}金（現在値 ${q.spot.toFixed(2)}）· 価格への影響 <b style="color:${impactColor}">${impactPct.toFixed(1)}%</b>`}</span>
      <span>${L`手数料 ${q.fee.toFixed(1)}金（${AMM_FEE_TEXT}）`}</span>
      <span class="meta">${L`同じティックの他の注文とまとめて約定するため、実際の価格は変わります。許容：見積もり±2%`}</span></div>
      <button class="btn primary" type="button" ${hasCurrency ? '' : 'disabled'} data-order="${attrJson(dto)}">${L`命令に追加（枠1）`}</button>`;
  }
  return html`<div class="section-title">${L`金の市場（鉄・馬 ⇄ 金）`}</div>
    ${hasCurrency ? '' : html`<div class="explanation">${L`「${T.TECH.Currency}」の研究が必要です。`}</div>`}
    ${v.pools.map(p => html`<div class="meter"><span>${L`${T.RESOURCE[p.good]}：在庫 ${fmt(p.goods)} · 金 ${fmt(p.gold)} · 現在値 1あたり ${p.spot.toFixed(2)}金`}</span></div>`)}
    <div class="row"><label class="field">${L`品目`}<select id="amm-good"><option value="Iron">${T.RESOURCE.Iron}</option><option value="Horses">${T.RESOURCE.Horses}</option></select></label><label class="field">${L`売買`}<select id="amm-side"><option value="Buy">${L`買う`}</option><option value="Sell">${L`売る`}</option></select></label><label class="field">${L`数量`}<input id="amm-amount" type="number" min="1" max="50" value="5" style="width:64px"></label></div>
    <button class="btn" type="button" id="amm-quote">${L`見積もる`}</button>${quote}`;
}

// ================================================================== USDC exchange
/** Step 1 → 2: read the form and show what the order would cost. */
export function reviewExchange() {
  S.exDraft = { amount: +$('#ex-amount').value, price: +$('#ex-price').value, side: $('#ex-side').value, g: $('#ex-good').value };
  S.exchangeStep = 1;
  invalidate('drawer');
}
export function cancelExchange() { S.exchangeStep = 0; invalidate('drawer'); }
export function confirmExchange() {
  const { amount, price, side, g } = S.exDraft;
  const good = g === 'Food' || g === 'Production' ? { kind: g, city: myNation().capital } : { kind: g };
  if (addDraft({ type: 'ExchangeOrder', good, side, amount, price: Math.round(price * 1e6) })) toast(L`USDCの注文を命令に追加しました（枠は使いません）。`);
  S.exchangeStep = 0; invalidate('drawer');
}

function exchangeConfirmText(v) {
  const { amount, price, side, g } = S.exDraft;
  const total = amount * price, fee = side === 'Buy' ? total * EXCHANGE_FEE : 0;
  return Lh`<b>${side === 'Buy' ? L`買い` : L`売り`}</b>：${T.goodName({ kind: g })} ${amount} × ${price.toFixed(2)} USDC = ${total.toFixed(2)} USDC${side === 'Buy' ? L`＋手数料 ${fee.toFixed(2)}（うち賞金プールへ ${(fee * poolSharePct(v) / 100).toFixed(2)}）` : ''}。<br>このティックの締切で、価格が合う相手がいれば約定します。テスト用のUSDCです。`;
}

function exchange(v, e) {
  const se = v.season || {};
  const deliveries = e.deliveries || [];
  const freeze = marketFreeze(v);
  const consentUsdc = se.spendConsentUsdc;
  const form = html`<div class="row"><label class="field">${L`品目`}<select id="ex-good">${['Gold', 'Iron', 'Horses'].map(k => html`<option value="${k}" ${S.exDraft?.g === k ? 'selected' : ''}>${T.RESOURCE[k]}</option>`)}${['Food', 'Production'].map(k => html`<option value="${k}" ${S.exDraft?.g === k ? 'selected' : ''}>${L`${T.RESOURCE[k]}（首都へ）`}</option>`)}</select></label>
    <label class="field">${L`売買`}<select id="ex-side"><option value="Buy">${L`買う`}</option><option value="Sell" ${S.exDraft?.side === 'Sell' ? 'selected' : ''}>${L`売る`}</option></select></label></div>
    <div class="row"><label class="field">${L`数量`}<input id="ex-amount" type="number" min="1" value="${S.exDraft?.amount ?? 1}" style="width:64px"></label><label class="field">${L`単価 USDC`}<input id="ex-price" type="number" min="0.01" step="0.01" value="${S.exDraft ? S.exDraft.price.toFixed(2) : '0.10'}" style="width:80px"></label></div>
    ${S.exchangeStep === 1 ? html`<div class="explanation" id="ex-confirm-text">${exchangeConfirmText(v)}</div><div class="row"><button class="btn" type="button" id="ex-cancel">${L`戻る`}</button><button class="btn usdc" type="button" id="ex-confirm">${L`USDCの注文を命令に追加`}</button></div>` : html`<button class="btn usdc" type="button" id="ex-review">${L`内容を確認する`}</button>`}
    ${held().some(r => r !== 'Diplomat') && consentUsdc ? html`<div class="row" style="margin-top:6px"><button class="btn" type="button" data-order="${attrJson({ type: 'ConsentSpend', usdc: consentUsdc })}">${L`⚖ 外交官の支出に同意（${usdc(consentUsdc)} USDCまで）`}</button></div>` : ''}`;
  return html`<div class="usdc-box"><div class="usdc-label">${L`USDC 市場 · 国庫どうしの一括競売 · テスト資金`}</div>
    <p class="desc" style="margin:4px 0">${Lh`原材料だけを他の国と取引できます（ティックごとに1つの価格でまとめて約定）。買い手は手数料${EXCHANGE_FEE * 100}%と<b>関税</b>を払い、どちらも${poolSharePct(v)}%が賞金プール・${100 - poolSharePct(v)}%が運営に入ります。関税は国の累計支出とともに上がります。買った物資は${se.deliveryTicks ?? 3}ティック後に届き、富・交易量・功績には数えません。スターゲートを建てている都市には届きません。自国・交戦中の国とは取引できません。${usdc(consentUsdc)} USDCを超える支出には、外交官とは別の役職者の同意が必要です。`}</p>
    <div class="stats"><div class="stat"><div class="k">${L`国庫`}</div><div class="v">${usdc(e.treasury)}<small> USDC</small></div></div><div class="stat"><div class="k">${L`累計支出`}</div><div class="v">${usdc(e.marketSpent)}</div></div><div class="stat"><div class="k">${L`いまの関税`}</div><div class="v">${(e.tariffBps / 100).toFixed(1)}<small>%</small></div></div></div>
    ${deliveries.length ? html`<div class="section-title">${L`輸送中`}</div>${deliveries.map(d => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title">${T.goodName(d.good)} ${d.qty}</div></div><span class="meta">${L`ティック${d.due}に到着`}</span></div>`)}` : ''}
    ${!se.market ? html`<div class="explanation">${L`このシーズンは市場がオフです。`}</div>` : v.tick >= freeze ? html`<div class="explanation">${L`終盤（ティック${freeze}以降）のため凍結中です。`}</div>` : form}
    <p class="meta" style="margin:6px 0 0">${L`取引所の注文は命令の枠を使いません。賞金プール（テスト）：${usdc(v.projection?.pool)} USDC`}</p></div>`;
}

export function drawerMarket() {
  const v = S.view, e = v.economy;
  return html`<div class="eyebrow">${L`MARKETS · 市場`}</div><h2>${L`市場`}</h2><p class="drawer-intro">${L`ゲーム内の金の市場と、プレイヤー同士のUSDC取引所があります。どちらもティックごとに一度だけ、同じ価格でまとめて約定するので、注文の早さは関係ありません。`}</p>
    ${goldMarket(v, e.techs.includes('Currency'))}${exchange(v, e)}`;
}
