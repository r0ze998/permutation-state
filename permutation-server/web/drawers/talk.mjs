// Talk and bounties (V5 §18.7, §18.3): members' messages, public and
// anchored on chain once per tick; and the operator's AI members as far as
// they are known — how many, their bounty, and those whose home city fell.
// In chain mode a message is signed in this browser with the session key
// and sent to the gateway (chainplay.mjs); locally the play server keeps it.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { $, html, usdcFixed } from '../util.mjs';
import { S, civN, invalidate, aiRoster } from '../state.mjs';
import { poll } from '../sync.mjs';
import * as chainplay from '../chainplay.mjs';
import { L, Lh } from '../lang.mjs';

const nameOf = id => (S.view.members || []).find(m => m.id === id)?.name ?? L`メンバー${id}`;
const toText = to => (to?.civ != null ? L`${civN(to.civ)}へ` : to?.member != null ? L`${nameOf(to.member)}へ` : L`全員へ`);
/** S.talkError is kept as a function so that a language switch re-renders it; plain text works too. */
const talkError = () => (typeof S.talkError === 'function' ? S.talkError() : S.talkError);

export async function sendTalk() {
  const text = $('#talk-text')?.value?.trim();
  if (!text) return;
  const t = S.talkTo ?? 'all';
  const to = t === 'all' ? null : { civ: Number(t) };
  if (S.view?.chain) {
    const res = await chainplay.sendTalk({ to, text });
    if (res.ok) { $('#talk-text').value = ''; S.talkError = null; invalidate('drawer'); poll(); } else { S.talkError = () => api.translateError(res); invalidate('drawer'); }
    return;
  }
  const res = await api.post('/api/talk', { to, text });
  if (res.ok) { $('#talk-text').value = ''; poll(); } else { S.talkError = () => api.translateError(res.error); invalidate('drawer'); }
}

export function drawerTalk() {
  const v = S.view;
  const r = aiRoster(v); // /api/state aiRoster (`roster` is my nation's member list)
  const msgs = [...(v.talk || [])].reverse();
  // Chain mode: sending needs this member's key in this browser.
  const can = !!v.member && (!v.chain || !!S.session);
  return html`<div class="eyebrow">${L`TALK · 会話と懸賞`}</div><h2>${L`会話`}</h2>
    <p class="drawer-intro">${Lh`メンバーどうしの会話は公開です。ティックごとにまとめてチェーンに刻まれ、誰がいつ何を言ったかは後から確かめられます。約束を縛るのは言葉ではなく<b>勢力の資金の契約</b>です（外交の画面）。`}</p>
    ${can ? html`<div class="row"><select id="talk-to" data-talk-to-select>${[html`<option value="all">${L`全員へ`}</option>`, ...v.civs.filter(c => c.id !== S.myCiv).map(c => html`<option value="${c.id}" ${String(S.talkTo) === String(c.id) ? 'selected' : ''}>${L`${civN(c.id)}へ`}</option>`)]}</select>
      <input id="talk-text" maxlength="280" placeholder="${L`メッセージ（280字まで）`}"><button class="btn primary" type="button" id="talk-send">${L`送る`}</button></div>
      ${S.talkError ? html`<p class="desc" style="color:var(--bad)">${talkError()}</p>` : ''}${v.chain ? html`<p class="meta">${L`メッセージはこのブラウザのゲーム内の鍵で署名して送ります（1ティックに3件まで）。`}</p>` : ''}` : html`<p class="desc">${v.member && v.chain ? String(chainplay.NO_KEY) : L`参加すると話せます。`}</p>`}
    <div class="section-title">${L`メッセージ ${msgs.length}`}</div>
    ${msgs.length ? msgs.slice(0, 60).map(m => html`<div class="proposal"><div>${Lh`<b>${nameOf(m.member)}</b>（${civN((v.members || []).find(x => x.id === m.member)?.civ)}） ${toText(m.to)}`}</div><div>${m.text}</div><div class="when">${L`ティック${m.tick}${m.anchored ? L` · チェーンに記録済み` : ''}`}</div></div>`) : html`<p class="desc">${L`まだありません。`}</p>`}
    <div class="section-title">${L`運営のAIメンバーと懸賞金`}</div>
    <p class="desc">${Lh`このシーズンには運営のAIメンバーが <b>${r.aiCount ?? 0}人</b> 混ざっています。誰かは遊んでいる間は分かりません。それぞれ自分の勢力のどこかの都市に住んでいて（ティック${r.homeTick ?? 45}に決まる）、その都市を最初に落とした勢力に懸賞金 <b>${usdcFixed(r.bountyEach || 0)} USDC</b> が入ります（直前10ティック以内に条約があった相手からは出ません）。AIメンバーの取り分は、同じ勢力の人に配り直されます。`}</p>
    ${r.fallen.length ? r.fallen.map(f => html`<div class="proposal"><div>${Lh`<b>${f.name}</b>（${civN(f.civ)}）${f.home != null ? L`：${T.cityName(f.home)}を${civN(f.captor)}がティック${f.tick}に落とした${f.bounty ? L`（懸賞金あり）` : L`（条約のため懸賞金なし）`}` : L`：シーズン終了で公開`}`}</div><div class="when">${L`salt ${String(f.salt).slice(0, 16)}… · 登録時の tag と照らし合わせて誰でも確かめられます`}</div></div>`) : html`<p class="desc">${L`まだ落ちた住まいはありません。`}</p>`}`;
}
