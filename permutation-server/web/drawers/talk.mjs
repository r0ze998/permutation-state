// Talk and bounties (V5 §18.7, §18.3): members' messages, public and
// anchored on chain once per tick; and the operator's AI members as far as
// they are known — how many, their bounty, and those whose home city fell.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { $, html, usdcFixed } from '../util.mjs';
import { S, civN, invalidate } from '../state.mjs';
import { poll } from '../sync.mjs';

const nameOf = id => (S.view.members || []).find(m => m.id === id)?.name ?? `国民${id}`;
const toText = to => (to?.civ != null ? `${civN(to.civ)}へ` : to?.member != null ? `${nameOf(to.member)}へ` : '全員へ');

export async function sendTalk() {
  const text = $('#talk-text')?.value?.trim();
  if (!text) return;
  const t = S.talkTo ?? 'all';
  const to = t === 'all' ? null : { civ: Number(t) };
  const res = await api.post('/api/talk', { to, text });
  if (res.ok) { $('#talk-text').value = ''; poll(); } else { S.talkError = api.translateError(res.error); invalidate('drawer'); }
}

export function drawerTalk() {
  const v = S.view;
  const r = v.roster || { aiCount: 0, fallen: [] };
  const msgs = [...(v.talk || [])].reverse();
  const can = !!v.member;
  return html`<div class="eyebrow">TALK · 会話と懸賞</div><h2>会話</h2>
    <p class="drawer-intro">国民どうしの会話は公開です。ティックごとにまとめてチェーンに刻まれ、誰がいつ何を言ったかは後から確かめられます。約束を縛るのは言葉ではなく<b>国庫の契約</b>です（外交の画面）。</p>
    ${can ? html`<div class="row"><select id="talk-to" data-talk-to-select>${[html`<option value="all">全員へ</option>`, ...v.civs.filter(c => c.id !== S.myCiv).map(c => html`<option value="${c.id}" ${String(S.talkTo) === String(c.id) ? 'selected' : ''}>${civN(c.id)}へ</option>`)]}</select>
      <input id="talk-text" maxlength="280" placeholder="メッセージ（280字まで）"><button class="btn primary" type="button" id="talk-send">送る</button></div>
      ${S.talkError ? html`<p class="desc" style="color:var(--bad)">${S.talkError}</p>` : ''}` : html`<p class="desc">参加すると話せます。</p>`}
    <div class="section-title">メッセージ ${msgs.length}</div>
    ${msgs.length ? msgs.slice(0, 60).map(m => html`<div class="proposal"><div><b>${nameOf(m.member)}</b>（${civN((v.members || []).find(x => x.id === m.member)?.civ)}） ${toText(m.to)}</div><div>${m.text}</div><div class="when">ティック${m.tick}${m.anchored ? ' · チェーンに記録済み' : ''}</div></div>`) : html`<p class="desc">まだありません。</p>`}
    <div class="section-title">運営のAI国民と懸賞金</div>
    <p class="desc">このシーズンには運営のAI国民が <b>${r.aiCount}人</b> 混ざっています。誰かは遊んでいる間は分かりません。それぞれ自国のどこかの都市に住んでいて（ティック${r.homeTick ?? 45}に決まる）、その都市を最初に落とした国に懸賞金 <b>${usdcFixed(r.bountyEach || 0)} USDC</b> が入ります（直前10ティック以内に条約があった相手からは出ません）。AI国民の取り分は、同じ国の人に配り直されます。</p>
    ${(r.fallen || []).length ? r.fallen.map(f => html`<div class="proposal"><div><b>${f.name}</b>（${civN(f.civ)}）${f.home != null ? `：${T.cityName(f.home)}を${civN(f.captor)}がティック${f.tick}に落とした${f.bounty ? '（懸賞金あり）' : '（条約のため懸賞金なし）'}` : '：シーズン終了で公開'}</div><div class="when">salt ${String(f.salt).slice(0, 16)}… · 登録時の tag と照らし合わせて誰でも確かめられます</div></div>`) : html`<p class="desc">まだ落ちた住まいはありません。</p>`}`;
}
