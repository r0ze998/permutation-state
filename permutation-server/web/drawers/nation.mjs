// The nation plaza (V5 §12): offices, election, proposals, recalls, and the
// governance actions sent from it. In chain mode each action is a SubmitGov
// signed in this browser (chainplay.mjs), sent only while the tick takes
// commitments; after the deadline it waits for the next tick.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { html, attrJson, toast } from '../util.mjs';
import { S, civN, held, invalidate, nationRoster } from '../state.mjs';
import { describeOrder, GOV_VERB, reportGov } from '../orders.mjs';
import * as chainplay from '../chainplay.mjs';
import { MAX_OFFICES, RECALL_ACTIVE_TICKS, RECALL_TICKS, activityWindows, recallNeeded } from '../rules.mjs';
import { poll } from '../sync.mjs';
import { L, Lh } from '../lang.mjs';

/** A member's name with AI / attestation badges; nobody = the acting official. */
const who = m => (m ? html`${m.name}${m.kind === 'agent' ? html` <span class="badge agent">AI</span>` : ''}${m.attested ? html` <span class="badge agent" title="${L`登録の証明あり（ERC-8004 など）`}">${L`証明済みAI`}</span>` : ''}` : html`<span class="meta">${L`代行（AI）`}</span>`);
const you = () => html` <span class="tag positive">${L`あなた`}</span>`;

export function drawerNation() {
  const v = S.view, g = v.gov; if (!g) return html`<p class="desc">${L`読み込んでいます…`}</p>`;
  const me = S.memberId, mine = held();
  const roster = nationRoster(v); // /api/state roster: my nation's members (the AI roster is `aiRoster`)
  const standing = v.member?.standingFor || [];
  const props = [...g.proposals].sort((a, b) => b.supporters - a.supporters);
  const office = o => html`<div class="office-row ${o.holder?.id === me ? 'me' : ''}"><span class="og">${T.ROLE_GLYPH[o.role]}</span><div class="main"><div class="title">${T.ROLE_JA[o.role]} · ${who(o.holder)}${o.holder?.id === me ? you() : ''}</div>
      <div class="meta">${o.holder ? Lh`ティック${o.since}から · ${o.active ? L`活動中` : html`<span class="idle">${L`最後の命令 ${o.lastAct ?? L`なし`}`}</span>`}` : L`立候補がないため代行が務めます`}${o.runnerUp ? L` · 次点 ${o.runnerUp.name}` : ''}</div></div>
      ${o.holder && o.holder.id !== me ? html`<button class="btn" type="button" data-gov="${attrJson({ type: 'Recall', role: o.role })}" title="${L`直近${RECALL_ACTIVE_TICKS}ティックに活動したメンバーの過半数が${RECALL_TICKS}ティック以内に賛成すると解任`}">${L`リコール`}</button>` : ''}</div>`;
  const recall = r => html`<div class="proposal">${Lh`<b>${T.ROLE_JA[r.role]}</b>（${r.holder?.name || ''}）のリコール${r.automatic ? L`（${g.idleRecallTicks ?? 30}ティック命令なしのため自動）` : ''}`}<div class="when">${L`賛成 ${r.yes} / 必要 ${recallNeeded(g.electorate)}（活動中のメンバー${g.electorate}人の過半数） · ティック${r.closes}まで`}</div><div class="row" style="margin-top:6px"><button class="btn danger" type="button" data-gov="${attrJson({ type: 'Recall', role: r.role })}">${L`賛成する`}</button></div></div>`;
  const candidate = (c, x) => html`<span class="cand-pill">${who(x.member)} <small>${L`${x.votes}票`}</small>${g.voteOpen ? html`<button class="btn ${S.myVotes[c.role] === x.member.id ? 'primary' : ''}" type="button" data-gov="${attrJson({ type: 'Vote', role: c.role, candidate: x.member.id })}">${L`投票`}</button>` : ''}</span>`;
  const proposal = p => {
    const adopted = (S.adopt[p.role] || []).includes(p.id);
    const supported = (p.supportedBy || []).includes(me);
    return html`<div class="proposal"><div><span class="og">${T.ROLE_GLYPH[p.role]}</span> <b>${L`${T.ROLE_JA[p.role]}へ`}</b> · ${who(p.proposer)} · ${L`支持 ${p.supporters}`}</div>
      <ul class="prop-orders">${p.orders.map(o => html`<li>${describeOrder(o).label}</li>`)}</ul>
      <div class="when">${L`ティック${p.tick}に提出 · ティック${p.expires}で失効 · 採用されると功績を献策者と役職者で半分ずつ`}</div>
      <div class="row" style="margin-top:6px">${p.proposer?.id !== me ? html`<button class="btn ${supported ? 'primary' : ''}" type="button" ${supported ? 'disabled' : html`data-gov="${attrJson({ type: 'Support', proposal: p.id })}"`}>${supported ? L`支持済み` : L`支持する`}</button>` : html`<span class="meta">${L`あなたの献策`}</span>`}
      ${mine.includes(p.role) ? html`<button class="btn ${adopted ? 'primary' : ''}" type="button" data-adopt="${p.role}:${p.id}">${adopted ? L`採用予定 ✓（確定で送信）` : L`採用する`}</button>` : ''}</div></div>`;
  };
  const windows = activityWindows(v);
  return html`<div class="eyebrow">${L`FACTION · 勢力の広場`}</div><h2>${L`${civN(S.myCiv)}の政府`}</h2>
    <p class="drawer-intro">${L`メンバーは${g.members}人。役職者だけが担当の命令を出せます。メンバーは誰でも献策・支持・投票・リコールができます（すべてチェーンに記録されます）。`}</p>
    <div class="section-title">${L`役職者`}</div>${g.offices.map(office)}
    ${g.recalls.length ? html`<div class="section-title">${L`リコール投票中`}</div>${g.recalls.map(recall)}` : ''}
    <div class="section-title">${Lh`選挙 · 次の任期はティック${g.nextElection ?? '—'}から${g.voteOpen ? html` · <span class="tag warning">${L`投票受付中`}</span>` : L` · 投票はティック${g.voteFrom ?? '—'}から`}`}</div>
    <div class="row">${T.ROLES.map(r => html`<button class="btn ${standing.includes(r) ? 'primary' : ''}" type="button" data-stand="${r}">${T.ROLE_GLYPH[r]} ${standing.includes(r) ? L`${T.ROLE_JA[r]}に立候補中` : L`${T.ROLE_JA[r]}に立候補`}</button>`)}</div><p class="meta">${L`1人が兼ねられる役職は${MAX_OFFICES}つまで。立候補はいつでも変えられます。`}</p>
    ${g.candidates.map(c => html`<div class="cand"><span class="og">${T.ROLE_GLYPH[c.role]}</span><b>${T.ROLE_JA[c.role]}</b> ${c.candidates.length ? c.candidates.map(x => candidate(c, x)) : html`<span class="meta">${L`立候補者なし（代行）`}</span>`}</div>`)}
    <div class="section-title">${L`献策 · 支持の多い順`}</div>
    ${props.length ? props.map(proposal) : html`<p class="desc">${L`献策はまだありません。地図で担当外の命令を選んで確定すると、その役職への献策になります。`}</p>`}
    <div class="section-title">${L`メンバー ${roster.length}人`}</div>${roster.map(m => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title">${who(m)}${m.id === me ? you() : ''}</div><div class="meta">${L`功績 ${m.merit.toFixed(1)} · 活動 ${m.activeWindows}/${windows}区間${m.active ? ' ✓' : ''}`}</div></div></div>`)}`;
}

/** Send one governance action (vote, support, stand, recall). */
export async function govAction(action, button) {
  if (S.view?.chain) return govChain(action, button);
  if (button) button.disabled = true;
  const r = await api.post('/api/gov', { action });
  const what = GOV_VERB[action.type];
  if (r.ok) {
    if (action.type === 'Vote') { S.myVotes[action.role] = action.candidate; invalidate('drawer', 'nextTurn', 'notifs'); }
    toast(S.view.chain ? L`${what}しました。チェーンに記録しました。` : L`${what}しました。次のティックで反映されます。`);
  } else toast(L`${what}できませんでした：${api.translateError(r.error)}`, 'error');
  poll();
}

/** Stand for one more office, or withdraw from it. */
export function toggleStand(role, button) {
  const cur = S.view.member?.standingFor || [];
  const roles = cur.includes(role) ? cur.filter(r => r !== role) : [...cur, role];
  return govAction({ type: 'Stand', roles }, button);
}

/**
 * Chain mode: sign and send the action now (with any that waited), or keep
 * it for the next tick when the commitments are closed.
 */
async function govChain(action, button) {
  if (!S.session) { toast(String(chainplay.NO_KEY), 'error'); return; }
  if (chainplay.chainPhase() !== 'commit') {
    chainplay.queueGov(action);
    if (action.type === 'Vote') S.myVotes[action.role] = action.candidate;
    toast(L`締切のあと（公開・解決中）なので、${GOV_VERB[action.type]}は次のティックの受付が始まったら送ります。`);
    invalidate('drawer', 'nextTurn', 'notifs');
    return;
  }
  if (button) button.disabled = true;
  reportGov(await chainplay.submitGov([action]));
  poll();
}
