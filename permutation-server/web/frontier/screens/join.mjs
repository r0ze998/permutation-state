// Join (web design §7.2; contract §5.9, I-40, I-51; owner decision V2: joining is
// choosing one of the six nations, nothing else): wallet → nation → the in-game key
// (the wallet signs the session text) → Join (relay-paid) → this browser picks up to
// three free sites of the home wedge itself and files the ticket (controller.mjs
// autoTicket) → the first village appears at the next bell (about 11–21 minutes;
// final once the cohort closes). A displaced or ended ticket is filed again
// automatically. There is no site picker.
import { activeHolding } from '../fstate.mjs';
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { factionName, DOCTRINE_NAMES, HOLDING_STATES, failureText } from '../fi18n.mjs';
import { freeByWedge, ticketTimes, homeWedge } from '../fland.mjs';
import { lamports, swatch, timeHtml } from './shell.mjs';
import { leaderCard } from '../people/ui.mjs';

const FACTIONS = [0, 1, 2, 3, 4, 5];
const siteText = s => L`州 ${s.p},${s.q} の区画 ${s.site + 1}`;

function renderWallets(FS) {
  const list = FS.walletList ?? [];
  const items = list.map((w, i) => html`<li><button type="button" class="btn" data-act="connect" data-i="${i}" ${raw(w.why ? 'disabled' : '')}>${w.name}</button>${w.why ? html` <span class="muted">${w.why}</span>` : ''}</li>`);
  return html`<section aria-labelledby="join-wallet"><h3 id="join-wallet">${L`ウォレットをつなぐ`}</h3>
    <p>${L`参加の署名にだけウォレットを使います。遊ぶ操作はこの端末のゲーム内の鍵が署名し、手数料は中継が払います（テスト用の SOL、価値はありません）。`}</p>
    ${items.length ? html`<ul class="list">${items}</ul>` : html`<p class="muted">${L`ウォレットが見つかりません。Solana のウォレットを入れてから読み込み直してください。`}</p>`}</section>`;
}

/** The faction cards: name, colour, doctrine, free land of the home wedge (an upper bound from the overviews). */
export function factionCards(FS) {
  const free = freeByWedge(FS.overviews ?? new Map());
  return FACTIONS.map(f => ({ faction: f, name: factionName(f), doctrine: DOCTRINE_NAMES[f], free: free[homeWedge(f)], chosen: FS.joinDraft?.faction === f }));
}

function renderFactions(FS) {
  const cards = factionCards(FS).map(c => html`<li><button type="button" class="card" data-act="pick-faction" data-f="${c.faction}" aria-pressed="${c.chosen ? 'true' : 'false'}">
    ${leaderCard(c.faction, { size: 88 })}<span class="muted">${L`本拠の扇区の空き区画 約 ${fmtNum(c.free)}`}</span></button></li>`);
  // The season's join gate (I-51), or a relay that answered InviteRequired.
  const gated = FS.inviteRequired || !!FS.season?.joinGate?.some?.(x => x !== 0);
  const ready = Number.isInteger(FS.joinDraft?.faction);
  return html`<section aria-labelledby="join-faction"><h3 id="join-faction">${L`勢力を選ぶ`}</h3>
    <p>${L`選ぶのは六つの勢力のどれか一つだけです。最初の拠点の入植希望は、本拠の扇区の空いた区画にこのページが自動で出し、拠点は次の鐘（約11〜21分後）に決まります。`}</p>
    <p class="muted">${L`M1 では賞金はありません。勢力は本拠の扇区と教義を決めます。`}</p>
    <ul class="cards">${cards}</ul>
    ${gated ? html`<label class="field">${L`招待コード`}<input name="invite" autocomplete="off" data-bind="invite" value="${FS.joinDraft?.invite ?? ''}"></label>` : ''}
    <button type="button" class="btn primary" data-act="join" ${raw(ready ? '' : 'disabled')}>${L`ゲーム内の鍵を作って参加する`}</button>
    <p class="muted">${L`ウォレットが2回たずねます：ゲーム内の鍵のための文面への署名と、参加の取引への署名です。`}</p></section>`;
}

function renderSessionFix(FS) {
  return html`<section aria-labelledby="join-key"><h3 id="join-key">${L`ゲーム内の鍵`}</h3>
    <p>${L`この端末にはこのシーズンのゲーム内の鍵がありません。ウォレットで同じ文面に署名すると、同じ鍵を作り直せます。`}</p>
    <button type="button" class="btn primary" data-act="session">${L`鍵を作り直す`}</button></section>`;
}

/** Why the last holding or ticket ended (stored with its bell, review finding 7), as a line. */
function endLine(FS) {
  const e = FS.ui?.landEnd;
  if (!e) return '';
  const t = { displaced: () => L`第${fmtNum(e.bell)}鐘：仮の拠点は、同じ鐘のより高い順位の入植希望に押し出されました。`,
    lost: () => L`第${fmtNum(e.bell)}鐘：拠点を失いました。`,
    ended: () => L`第${fmtNum(e.bell)}鐘：前の入植希望は区画を得られずに終わりました。` }[e.why];
  return t ? html`<p class="callout">${t()} ${L`空いた区画に、入植希望を自動でもう一度出します。`}</p>` : '';
}

/** While the first holding is being placed (no ticket yet): what this browser is doing about it (review finding 12). */
function renderPlacing(FS) {
  const a = FS.autoTicket;
  const again = !!FS.ui?.landEnd || FS.land?.stage === 'refugee';
  const retry = html`<button type="button" class="btn" data-act="auto-ticket">${L`いますぐもう一度出す`}</button>`;
  const line = {
    searching: () => html`<p role="status">${L`本拠の扇区で空いた区画を探して、入植希望を出しています…`}</p>`,
    sent: () => html`<p role="status">${L`入植希望を出しました。チェーンの記録に現れるのを待っています。`}</p>`,
    waiting: () => html`<p role="status">${L`この鐘の入植希望はもう試しました。次の鐘に自動でもう一度出します。`}</p>${retry}`,
    nofree: () => html`<p class="warn" role="status">${L`本拠の扇区にも隣の扇区にも空いた区画が見つかりません。鐘ごとに自動で探し直し、新しい輪がひらけばそこを探します。`}</p>${retry}`,
    failed: () => html`<p class="notice error" role="alert">${L`入植希望を出せませんでした：${failureText({ code: a.code })}`} ${L`次の鐘に自動でもう一度出します。`}</p>${retry}`,
  }[a?.state] ?? (() => html`<p role="status">${L`まもなく入植希望を自動で出します。`}</p>`);
  return html`<section aria-labelledby="join-sites"><h3 id="join-sites">${again ? L`新しい拠点の場所を探しています` : L`最初の拠点を置いています`}</h3>
    ${endLine(FS)}
    ${line()}
    <p class="muted">${L`場所は選びません。同じ鐘の入植希望はその鐘の乱数でまとめて公平に決まり、拠点は次の鐘（約11〜21分後）に決まります。`}</p>
    <p class="muted">${Lh`預け金：<strong>${lamports(FS.land?.escrowNeeded ?? 0n)}</strong>（拠点の口座の賃料。拠点ができればそこへ移り、できなければ払った人に戻ります）`}</p>
    <p><button type="button" class="btn" data-act="practice-open">${L`待つあいだに練習で戦ってみる`}</button></p></section>`;
}

function renderTicket(FS) {
  const t = FS.land.ticket;
  const times = FS.clock ? ticketTimes(FS.clock, t.bell) : null;
  return html`<section aria-labelledby="join-ticket"><h3 id="join-ticket">${L`入植希望（第${fmtNum(t.bell)}鐘）`}</h3>
    <ol class="chosen">${t.sites.map((s, i) => html`<li ${raw(i < t.next ? 'class="done"' : '')}>${siteText(s)}${i < t.next ? html` <span class="muted">${L`（ふさがっていた）`}</span>` : ''}</li>`)}</ol>
    ${times ? html`<p>${Lh`結果は ${timeHtml(times.resultAbout)} ごろ（出してから約11〜21分）に、この鐘の乱数で決まります。`}</p>` : ''}
    ${endLine(FS)}
    <p class="muted">${FS.ui?.ticketSource === 'overflow' ? L`本拠の扇区に空きがなかったため、区画は隣の扇区のいちばん外の輪から自動で選びました。区画を得られなければ、自動でもう一度出します。` : L`区画は本拠の扇区から自動で選びました。区画を得られなければ、自動でもう一度出します。`}</p>
    <p><button type="button" class="btn" data-act="practice-open">${L`待つあいだに練習で戦ってみる`}</button></p></section>`;
}

function renderProvisional(FS) {
  const h = activeHolding(FS);
  const times = FS.clock && h ? ticketTimes(FS.clock, h.ticketBell) : null;
  return html`<section aria-labelledby="join-prov"><h3 id="join-prov">${HOLDING_STATES.provisional}</h3>
    ${h ? html`<p>${L`州 ${h.p},${h.q} の区画 ${h.site + 1} を得ました。`}</p>` : ''}
    ${times ? html`<p>${Lh`同じ鐘の入植希望がすべて決まると確定します（遅くとも ${timeHtml(times.cohortEndsBy)}）。それまでは、より高い順位の希望に押し出されることがあります。`}</p>` : ''}
    <p class="muted">${L`仮の拠点でも収穫・建設・訓練はできます（押し出されると失われます）。軍勢の編成・出発・探索は確定してからです。`}</p></section>`;
}

/** The join screen for the viewer's stage. */
export function render(FS) {
  if (!FS.wallet) return renderWallets(FS);
  const stage = FS.land?.stage ?? 'none';
  if (stage === 'none') return renderFactions(FS);
  if (!FS.session || FS.sessionProblem) return renderSessionFix(FS);
  if (stage === 'ticket') return renderTicket(FS);
  if (stage === 'provisional') return renderProvisional(FS);
  if (stage === 'final') return html`<p>${L`拠点が確定しました。「拠点」から始めましょう。`}</p>`;
  return renderPlacing(FS);
}
