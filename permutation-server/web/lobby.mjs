// Registration (V5 §4): choose a nation, a name and the offices to stand for;
// in chain mode, take over a member the gateway registered for you. Also the
// texts that describe who is in this season, and starting the season.
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import { $, html, setHtml, toast, usdc } from './util.mjs';
import { S } from './state.mjs';
import { MAX_OFFICES, adoptRules, opsSharePct, poolSharePct } from './rules.mjs';
import { poll } from './sync.mjs';

let pending = null; // { info, resolve, refresh } while the dialog is open

/** Show the member dialog; resolves with the new member token. */
export function pickMember(info) {
  const dlg = $('#seats');
  pending = { info, resolve: null, refresh: null };
  adoptRules(info);
  renderLobby();
  pending.refresh = setInterval(async () => {
    if (!dlg.open || !pending) return;
    const info = await api.tryGet('/api/lobby', pending.info);
    if (!pending) return;
    pending.info = info;
    adoptRules(info);
    renderLobby();
  }, 3000);
  dlg.oncancel = e => e.preventDefault(); // membership (or spectating) is required
  dlg.showModal();
  return new Promise(resolve => { pending.resolve = resolve; });
}

function renderLobby(error = '') {
  const info = pending.info;
  $('#seat-mode').textContent = info.mode === 'chain' ? 'オンチェーンのシーズン（MagicBlock ER）' : 'ローカルのシーズン（テスト用USDC）';
  $('#seat-nations').textContent = String(info.nations.length);
  const err = error ? html`<li class="seat-error">${error}</li>` : '';
  // On chain, and locally once the season has started, nobody new can join:
  // take over a person's member that nobody is playing, or watch.
  if (info.mode === 'chain' || info.phase !== 'lobby') {
    const chain = info.mode === 'chain';
    // Seats are offered by nation, never by member: which members are seats
    // for people must not show who the operator's AI members are (V5 §18.2).
    const open = info.nations.filter(n => n.seats > 0);
    const origin = chain ? '（ゲートウェイが登録済みの席）' : '（いま誰も操作していない席）';
    const none = chain
      ? 'いま空いている席はありません。AI エージェントは x402 で参加できます（llms.txt）。'
      : 'シーズンは始まっていて、新しく参加することはできません。空いている席もありません。';
    setHtml($('#seat-list'), html`${open.length ? open.map(n => html`<li class="seat human"><span class="swatch" style="background:${T.CIV_COLORS[n.civ]}"></span><span class="who"><b>${T.civName(n.name)}</b><small>空いている席 ${n.seats}${origin}</small></span><button class="btn primary" type="button" data-claim-civ="${n.civ}">この国で遊ぶ</button></li>`)
      : html`<li class="seat-none">${none}<a class="btn primary" href="spectate.html">観戦する →</a></li>`}${err}`);
    return;
  }
  const pick = S.lobbyPick;
  const picked = pick.civ === null ? null : info.nations[pick.civ];
  setHtml($('#seat-list'), html`${info.nations.map(n => html`<li class="seat nation ${pick.civ === n.civ ? 'picked' : ''}" data-pick-civ="${n.civ}">
        <span class="swatch" style="background:${T.CIV_COLORS[n.civ]}"></span>
        <span class="who"><b>${T.civName(n.name)}</b><small>国民 ${n.members}人${n.perMember ? ` · 今の1人あたりの見込み ${usdc(n.perMember)} USDC` : ''}</small></span>
        <span class="meta">${pick.civ === n.civ ? '選択中 ✓' : 'えらぶ'}</span></li>`)}<li class="seat-form"><label class="field">名前<input id="join-name" maxlength="24" placeholder="あなたの名前" value="${S.joinName}"></label>
         <div class="field">立候補する役職（${MAX_OFFICES}つまで）<div class="row">${T.ROLES.map(r => html`<button type="button" class="btn ${pick.stand.includes(r) ? 'primary' : ''}" data-pick-stand="${r}">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}</button>`)}</div></div>
         <p class="desc">参加費は全員同じ ${usdc(info.entryFee)} USDC（${poolSharePct(null)}%が賞金プール、${opsSharePct(null)}%が運営）。国民がいない役職はAIの代行が務めます。</p>
         <button class="btn primary wide" type="button" id="join-btn" ${picked ? '' : 'disabled'}>${picked ? `${T.civName(picked.name)}の国民になる` : '国を選んでください'}</button></li>${err}`);
}

const keepName = () => { S.joinName = $('#join-name')?.value ?? S.joinName; };
export function pickCiv(civ) { keepName(); S.lobbyPick.civ = civ; renderLobby(); }
export function pickStand(role) {
  keepName();
  const st = S.lobbyPick.stand;
  S.lobbyPick.stand = st.includes(role) ? st.filter(x => x !== role) : [...st, role].slice(-MAX_OFFICES);
  renderLobby();
}
export const join = () => finish(api.post('/api/join', { civ: S.lobbyPick.civ, name: $('#join-name').value, kind: 'human', stand: S.lobbyPick.stand }));
export const claim = civ => finish(api.post('/api/claim', { civ }));

async function finish(request) {
  if (!pending) return;
  keepName();
  const res = await request;
  if (!pending) return;
  if (res.ok) {
    clearInterval(pending.refresh);
    api.tokenStore.set(res.token);
    $('#seats').close();
    const { resolve } = pending;
    pending = null;
    resolve(res.token);
    return;
  }
  const info = await api.tryGet('/api/lobby', pending.info);
  if (!pending) return;
  pending.info = info;
  renderLobby(api.translateError(res.error));
}

// ================================================================== who is in this season
/** Who else is in this season, from the member list. */
export function othersText(v) {
  const ms = v?.members || [];
  const ai = v?.roster?.aiCount ?? 0;
  return `この季節の国民は${ms.length}人。${ai ? `うち${ai}人は運営のAI国民です（誰かは、住む都市が落ちたときとシーズンの終わりに公開）。` : ''}国民のいない役職はルールの代行が務めます。`;
}

/** Help texts and titles that match this season (nations, members, clock, terms). */
export function describeSeason(v) {
  const nations = v.civs.length;
  document.title = `PERMUTATION STATE — ${nations}つの国、ひとつの世界`;
  $('#help-title').textContent = `${nations}つの国のひとつの国民として、国を動かす。`;
  $('#help-desc').textContent = `${nations}つの国が同じ地図を共有しています。${othersText(v)}${v.tickSeconds}秒ごとの「ティック」で、全ての国の命令が同時に解決されます。`;
  for (const el of document.querySelectorAll('[data-season]')) {
    const x = v.season?.[el.dataset.season];
    if (x !== undefined && x !== null) el.textContent = String(x);
  }
  if (v.chain) $('#help-mode').textContent = 'このシーズンはオンチェーンです（MagicBlock ER）。時計は止まりません。「手番を終える」で担当の命令に署名して送ります。';
}

/** Start the season (lobby) or resume the clock. */
export async function startSeason() {
  if (S.view?.phase === 'lobby') {
    const r = await api.post('/api/start', {});
    if (!r.ok) toast(r.error || '開幕できませんでした', 'error');
  } else await api.post('/api/control', { paused: false });
  poll();
}
