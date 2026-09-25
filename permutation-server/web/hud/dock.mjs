// The order dock: office budgets, drafted orders (chips), the seal state
// and the commit / end-turn button.
import * as T from '../i18n.mjs';
import { $, html, setHtml } from '../util.mjs';
import { S, held, officeInfo, spendable, isWatching } from '../state.mjs';
import { used, isDirty } from '../orders.mjs';
import { BANK_TICKS, BUDGET_TEXT } from '../rules.mjs';

/** Pips per office held; the bank is a reservoir counter, not more pips. */
function officePips(role) {
  const o = officeInfo(role), u = used(role);
  const pips = [];
  for (let i = 0; i < o.budget; i++) pips.push(html`<span class="pip ${i < Math.min(u, o.budget) ? 'used' : ''}"></span>`);
  const bankUsed = Math.max(0, u - o.budget);
  const bank = o.bank ? html`<span class="bank-well ${bankUsed ? 'drawing' : ''}" title="繰越の枠"><i style="height:${Math.round(100 * (o.bank - bankUsed) / Math.max(1, BANK_TICKS * o.budget))}%"></i><b>+${o.bank - bankUsed}</b></span>` : '';
  return html`<span class="office-pips" title="${T.ROLE_JA[role]}：このティック ${o.budget}・繰越 ${o.bank}"><span class="og">${T.ROLE_GLYPH[role]}</span>${pips.length ? pips : html`<span class="meta">枠0</span>`}${bank}</span>`;
}

export function renderDock() {
  const v = S.view;
  if (!v.economy) return;
  const offices = held();
  const props = S.drafts.filter(d => d.proposal).length;
  setHtml($('#dock-budget'), offices.length
    ? html`<div class="pips" aria-label="命令の枠">${offices.map(officePips)}</div><div class="budget-text num">${offices.map(r => `${T.ROLE_JA[r]} ${used(r)}/${spendable(r)}`).join(' · ')}${props ? ` · 献策 ${props}件` : ''}</div>
      <span class="tip panel"><div class="tip-row"><span>国の枠</span><span>${BUDGET_TEXT}</span></div><div class="tip-row"><span>役職ごとの割り当て</span><span>${T.ROLES.map(r => T.ROLE_JA[r]).join('・')}</span></div><div class="tip-row"><span>繰越</span><span>役職ごとに最大${BANK_TICKS}ティック分</span></div><div class="tip-row"><span>取引所・同意</span><span>枠を使わない</span></div></span>`
    : html`<div class="budget-text">役職がないので、命令はすべて<b>献策</b>になります${props ? `（${props}件）` : ''}</div>`);
  setHtml($('#chips'), S.drafts.length
    ? S.drafts.map((d, i) => html`<span class="chip ${d.usdc ? 'usdc' : ''} ${d.proposal ? 'proposal' : ''}" data-chip="${i}" title="${d.label}${d.proposal ? `（${T.ROLE_JA[d.office]}への献策）` : ''}"><span>${d.proposal ? '✎' : d.glyph}</span><span class="t">${d.label}</span>${d.proposal ? html`<span class="pt">献策→${T.ROLE_JA[d.office]}</span>` : ''}<button class="x" type="button" data-remove="${i}" aria-label="取り消す">×</button></span>`)
    : html`<span class="empty">${offices.length ? '命令はまだありません。地図で部隊や都市を選んでください（Space＝次の判断）。担当外の命令は献策になります。' : 'あなたは役職についていません。地図で選んだ命令は、担当の役職者への献策になります。国の広場で投票・支持・リコールもできます。'}</span>`);
  if (S.pulseChip !== null) { // highlight a chip just added
    const chip = $(`#chips [data-chip="${S.pulseChip}"]`);
    S.pulseChip = null;
    if (chip) { chip.classList.add('pulse'); setTimeout(() => chip.classList.remove('pulse'), 900); }
  }
  const dirty = isDirty();
  const resolving = !v.paused && v.secondsLeft < .6;
  // Nothing drafted and this tick's turn not ended yet: the button ends the turn (an empty sealed batch per office).
  const idle = !dirty && offices.length && !v.member?.ready;
  const state = v.phase === 'lobby' ? ['draft', '開幕前'] : resolving ? ['resolving', '解決中'] : dirty ? ['draft', '下書き · 未確定'] : idle ? ['draft', '命令なし · 未確定'] : S.drafts.length ? ['committed', '確定済み ✓'] : ['committed', '手番を終えました'];
  const dg = (v.member?.committed || []).find(c => c.digest)?.digest;
  // Commit-reveal as a sealed dispatch: sealed now, opened (and checked) after the tick.
  setHtml($('#digest'), dg && !dirty ? html`<span class="seal">🔒 封印 ${dg.slice(0, 6)}…</span>` : dirty && offices.length ? html`<span class="seal">確定で封印</span>` : '');
  $('#digest').title = dg ? `decision_digest ${dg}\nobs_root ${v.decision?.obsRoot}` : '';
  const disabled = S.committing || v.over || v.phase === 'lobby' || isWatching() || (!dirty && !idle);
  setHtml($('#commit'), html`<span class="commit-state ${state[0]}">${state[1]}</span><button class="btn ${dirty ? 'primary' : ''}" type="button" id="commit-btn" ${disabled ? 'disabled' : ''}>${dirty ? (offices.length ? '確定する' : '献策する') : idle ? '命令なしで手番を終える' : '確定済み'}<span class="key" style="margin-left:4px">⌃↵</span></button>`);
}
