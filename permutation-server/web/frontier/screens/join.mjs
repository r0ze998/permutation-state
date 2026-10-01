// Join, faction and site (web design §7.2; contract §5.9, I-40, I-47, I-51):
// wallet → faction → the in-game key (the wallet signs the Frontier session
// text) → Join (the only wallet-signed transaction besides SetSession,
// relay-paid) → the site picker (up to three free sites of the home wedge,
// in order) → FileTicket with the refundable Holding-rent escrow shown →
// the ticket card (result about 11–21 minutes later; final once the cohort
// closes) → a one-tap refile after a displacement or an ended ticket.
import { activeHolding } from '../fstate.mjs';
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { factionName, DOCTRINE_NAMES, HOLDING_STATES } from '../fi18n.mjs';
import { freeByWedge, candidateProvinces, freeSitesOf, ticketTimes, refileOffer, homeWedge } from '../fland.mjs';
import { overflowProvinces } from '../onboarding.mjs';
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

/**
 * The site picker's rows: candidate provinces, the free sites of the opened
 * one, the chosen sites in order. When the home wedge shows no free site,
 * the candidates are the adjacent wedges' provinces in the outermost open
 * ring (§5.9; W4-E D8 folded in here), and `overflow` says so
 * (`{ring}`; null otherwise). The program decides on its own counters.
 */
export function sitePicker(FS) {
  const faction = FS.citizen?.faction;
  const overviews = FS.overviews ?? new Map();
  let provinces = candidateProvinces(overviews, faction);
  let overflow = null;
  if (!provinces.length && Number.isInteger(faction)) {
    const o = overflowProvinces(overviews, faction, FS.record?.rings?.length ?? 1);
    if (o.full) { overflow = { ring: o.ring }; provinces = o.provinces.map(({ p, q, ring, free }) => ({ p, q, ring, free })); }
  }
  const env = FS.joinDraft?.envelope;
  const free = env ? freeSitesOf(env.province) : [];
  const chosen = FS.joinDraft?.sites ?? [];
  return { provinces, overflow, free, chosen, escrow: FS.land?.escrowNeeded ?? 0n };
}

function renderSites(FS) {
  const v = sitePicker(FS);
  const offer = refileOffer(FS.land ?? { stage: 'none' }, FS.ui?.lastTicket, { hadHolding: FS.hadHolding, seen: FS.ui?.ticketSeen !== false, filedBell: FS.ui?.lastTicketBell ?? null, nowBell: FS.nowBell ?? null });
  const isChosen = s => v.chosen.findIndex(c => c.p === s.p && c.q === s.q && c.site === s.site);
  return html`<section aria-labelledby="join-sites"><h3 id="join-sites">${L`入植地を選ぶ`}</h3>
    ${offer ? html`<div class="callout">${offer.why === 'displaced' ? L`仮の拠点は同じ鐘のより高い順位の希望に押し出され、入植希望は終わりました。` : L`入植希望は区画を得られずに終わりました。`}
      <button type="button" class="btn primary" data-act="refile">${L`同じ区画でもう一度出す`}</button></div>` : ''}
    <p>${L`本拠の扇区の第2輪より外で、空いている区画を3つまで順に選びます。同じ鐘に出された希望は、その鐘の乱数でまとめて決まります（早い者勝ちではありません）。`}</p>
    ${v.overflow ? html`<p class="warn">${v.provinces.length ? L`本拠の扇区に空き区画がありません。この場合は隣の扇区のいちばん外の輪（第${v.overflow.ring}輪）の区画に入植希望を出せます（最後はチェーンが判断します）。` : L`本拠の扇区に空き区画がなく、隣の扇区のいちばん外の輪にも空きが見つかりません。新しい輪がひらくのを待ってください。`}</p>` : ''}
    <ul class="list">${v.provinces.slice(0, 24).map(pr => html`<li><button type="button" class="btn" data-act="pick-province" data-p="${pr.p}" data-q="${pr.q}">${L`州 ${pr.p},${pr.q}（第${pr.ring}輪、空き 約 ${fmtNum(pr.free)}）`}</button></li>`)}</ul>
    ${FS.joinDraft?.envelope ? html`<h4>${L`州 ${FS.joinDraft.envelope.province.p},${FS.joinDraft.envelope.province.q} の空き区画`}</h4>
      <ul class="list">${v.free.map(s => { const i = isChosen(s); return html`<li><button type="button" class="btn" data-act="toggle-site" data-p="${s.p}" data-q="${s.q}" data-site="${s.site}" aria-pressed="${i >= 0 ? 'true' : 'false'}">${siteText(s)}${i >= 0 ? html` <strong>${L`第${i + 1}希望`}</strong>` : ''}</button></li>`; })}</ul>` : ''}
    ${v.chosen.length ? html`<ol class="chosen">${v.chosen.map(s => html`<li>${siteText(s)}</li>`)}</ol>` : ''}
    <p>${Lh`預け金：<strong>${lamports(v.escrow)}</strong>（拠点の口座の賃料。拠点ができればそこへ移り、できなければ次の希望に使われ、最後は払った人に戻ります）`}</p>
    <button type="button" class="btn primary" data-act="file-ticket" ${raw(v.chosen.length ? '' : 'disabled')}>${L`入植希望を出す`}</button></section>`;
}

function renderTicket(FS) {
  const t = FS.land.ticket;
  const times = FS.clock ? ticketTimes(FS.clock, t.bell) : null;
  return html`<section aria-labelledby="join-ticket"><h3 id="join-ticket">${L`入植希望（第${fmtNum(t.bell)}鐘）`}</h3>
    <ol class="chosen">${t.sites.map((s, i) => html`<li ${raw(i < t.next ? 'class="done"' : '')}>${siteText(s)}${i < t.next ? html` <span class="muted">${L`（ふさがっていた）`}</span>` : ''}</li>`)}</ol>
    ${times ? html`<p>${Lh`結果は ${timeHtml(times.resultAbout)} ごろ（出してから約11〜21分）に、この鐘の乱数で決まります。`}</p>` : ''}
    <p class="muted">${L`待つあいだに練習モードで戦い方を試せます。`}</p></section>`;
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
  return renderSites(FS);
}
