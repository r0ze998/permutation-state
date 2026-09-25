// The nation plaza (V5 §12): offices, election, proposals, recalls, and the
// governance actions sent from it.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { html, attrJson, toast } from '../util.mjs';
import { S, civN, held, invalidate } from '../state.mjs';
import { describeOrder } from '../orders.mjs';
import { MAX_OFFICES, RECALL_ACTIVE_TICKS, RECALL_TICKS, activityWindows, recallNeeded } from '../rules.mjs';
import { poll } from '../app.mjs';

/** A member's name with AI / attestation badges; nobody = the acting official. */
const who = m => (m ? html`${m.name}${m.kind === 'agent' ? html` <span class="badge agent">AI</span>` : ''}${m.attested ? html` <span class="badge agent" title="登録の証明あり（ERC-8004 など）">証明済みAI</span>` : ''}` : html`<span class="meta">代行（AI）</span>`);
const you = html` <span class="tag positive">あなた</span>`;

export function drawerNation() {
  const v = S.view, g = v.gov; if (!g) return html`<p class="desc">読み込んでいます…</p>`;
  const me = S.memberId, mine = held();
  const roster = v.roster || [];
  const standing = v.member?.standingFor || [];
  const props = [...g.proposals].sort((a, b) => b.supporters - a.supporters);
  const office = o => html`<div class="office-row ${o.holder?.id === me ? 'me' : ''}"><span class="og">${T.ROLE_GLYPH[o.role]}</span><div class="main"><div class="title">${T.ROLE_JA[o.role]} · ${who(o.holder)}${o.holder?.id === me ? you : ''}</div>
      <div class="meta">${o.holder ? html`ティック${o.since}から · ${o.active ? '活動中' : html`<span class="idle">最後の命令 ${o.lastAct ?? 'なし'}</span>`}` : '立候補がないため代行が務めます'}${o.runnerUp ? ` · 次点 ${o.runnerUp.name}` : ''}</div></div>
      ${o.holder && o.holder.id !== me ? html`<button class="btn" type="button" data-gov="${attrJson({ type: 'Recall', role: o.role })}" title="直近${RECALL_ACTIVE_TICKS}ティックに活動した国民の過半数が${RECALL_TICKS}ティック以内に賛成すると解任">リコール</button>` : ''}</div>`;
  const recall = r => html`<div class="proposal"><b>${T.ROLE_JA[r.role]}</b>（${r.holder?.name || ''}）のリコール${r.automatic ? `（${g.idleRecallTicks ?? 30}ティック命令なしのため自動）` : ''}<div class="when">賛成 ${r.yes} / 必要 ${recallNeeded(g.electorate)}（活動中の国民${g.electorate}人の過半数） · ティック${r.closes}まで</div><div class="row" style="margin-top:6px"><button class="btn danger" type="button" data-gov="${attrJson({ type: 'Recall', role: r.role })}">賛成する</button></div></div>`;
  const candidate = (c, x) => html`<span class="cand-pill">${who(x.member)} <small>${x.votes}票</small>${g.voteOpen ? html`<button class="btn ${S.myVotes[c.role] === x.member.id ? 'primary' : ''}" type="button" data-gov="${attrJson({ type: 'Vote', role: c.role, candidate: x.member.id })}">投票</button>` : ''}</span>`;
  const proposal = p => {
    const adopted = (S.adopt[p.role] || []).includes(p.id);
    const supported = (p.supportedBy || []).includes(me);
    return html`<div class="proposal"><div><span class="og">${T.ROLE_GLYPH[p.role]}</span> <b>${T.ROLE_JA[p.role]}へ</b> · ${who(p.proposer)} · 支持 ${p.supporters}</div>
      <ul class="prop-orders">${p.orders.map(o => html`<li>${describeOrder(o).label}</li>`)}</ul>
      <div class="when">ティック${p.tick}に提出 · ティック${p.expires}で失効 · 採用されると功績を献策者と役職者で半分ずつ</div>
      <div class="row" style="margin-top:6px">${p.proposer?.id !== me ? html`<button class="btn ${supported ? 'primary' : ''}" type="button" ${supported ? 'disabled' : html`data-gov="${attrJson({ type: 'Support', proposal: p.id })}"`}>${supported ? '支持済み' : '支持する'}</button>` : html`<span class="meta">あなたの献策</span>`}
      ${mine.includes(p.role) ? html`<button class="btn ${adopted ? 'primary' : ''}" type="button" data-adopt="${p.role}:${p.id}">${adopted ? '採用予定 ✓（確定で送信）' : '採用する'}</button>` : ''}</div></div>`;
  };
  const windows = activityWindows(v);
  return html`<div class="eyebrow">NATION · 国の広場</div><h2>${civN(S.myCiv)}の政府</h2>
    <p class="drawer-intro">国民は${g.members}人。役職者だけが担当の命令を出せます。国民は誰でも献策・支持・投票・リコールができます（すべてチェーンに記録されます）。</p>
    <div class="section-title">役職者</div>${g.offices.map(office)}
    ${g.recalls.length ? html`<div class="section-title">リコール投票中</div>${g.recalls.map(recall)}` : ''}
    <div class="section-title">選挙 · 次の任期はティック${g.nextElection ?? '—'}から${g.voteOpen ? html` · <span class="tag warning">投票受付中</span>` : ` · 投票はティック${g.voteFrom ?? '—'}から`}</div>
    <div class="row">${T.ROLES.map(r => html`<button class="btn ${standing.includes(r) ? 'primary' : ''}" type="button" data-stand="${r}">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}に${standing.includes(r) ? '立候補中' : '立候補'}</button>`)}</div><p class="meta">1人が兼ねられる役職は${MAX_OFFICES}つまで。立候補はいつでも変えられます。</p>
    ${g.candidates.map(c => html`<div class="cand"><span class="og">${T.ROLE_GLYPH[c.role]}</span><b>${T.ROLE_JA[c.role]}</b> ${c.candidates.length ? c.candidates.map(x => candidate(c, x)) : html`<span class="meta">立候補者なし（代行）</span>`}</div>`)}
    <div class="section-title">献策 · 支持の多い順</div>
    ${props.length ? props.map(proposal) : html`<p class="desc">献策はまだありません。地図で担当外の命令を選んで確定すると、その役職への献策になります。</p>`}
    <div class="section-title">国民 ${roster.length}人</div>${roster.map(m => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title">${who(m)}${m.id === me ? you : ''}</div><div class="meta">功績 ${m.merit.toFixed(1)} · 活動 ${m.activeWindows}/${windows}区間${m.active ? ' ✓' : ''}</div></div></div>`)}`;
}

const GOV_VERB = { Vote: '投票', Support: '支持', Stand: '立候補', Recall: 'リコールに賛成', Propose: '献策' };
/** Send one governance action (vote, support, stand, recall). */
export async function govAction(action, button) {
  if (button) button.disabled = true;
  const r = await api.post('/api/gov', { action });
  const what = GOV_VERB[action.type];
  if (r.ok) {
    if (action.type === 'Vote') { S.myVotes[action.role] = action.candidate; invalidate('drawer', 'nextTurn', 'notifs'); }
    toast(`${what}しました。${S.view.chain ? 'チェーンに記録しました' : '次のティックで反映されます'}。`);
  } else toast(`${what}できませんでした：${api.translateError(r.error)}`, 'error');
  poll();
}

/** Stand for one more office, or withdraw from it. */
export function toggleStand(role, button) {
  const cur = S.view.member?.standingFor || [];
  const roles = cur.includes(role) ? cur.filter(r => r !== role) : [...cur, role];
  return govAction({ type: 'Stand', roles }, button);
}
