// The order dock: office budgets, drafted orders (chips), the seal state
// and the commit / end-turn button. In chain mode the seal state is each
// office's own: sent from this browser, seen on chain (the gateway's /tick
// flags), revealed after the deadline.
import * as T from '../i18n.mjs';
import { $, html, setHtml, listSep } from '../util.mjs';
import { S, held, officeInfo, spendable, isWatching } from '../state.mjs';
import { used, isDirty, turnEnded } from '../orders.mjs';
import { chainPhase, officeStates, NO_KEY } from '../chainplay.mjs';
import { BANK_TICKS, BUDGET_TEXT } from '../rules.mjs';
import { L, Lh } from '../lang.mjs';

/** One office's seal in chain mode: [its glyph and mark, what it means]. */
function sealMark(s) {
  const g = T.ROLE_GLYPH[s.role];
  if (s.revealed) return [L`${g}公開`, L`公開済み（解決を待っています）`];
  if (s.onChain) return [`${g}✓`, L`チェーンで封印を確認`];
  if (s.committed) return [L`${g}送信済`, L`送信済み（チェーンでの確認待ち）`];
  if (s.sending) return [`${g}…`, L`送信中`];
  if (s.failed) return [`${g}✕`, L`送れませんでした：${T.errorText(s.failed)}`];
  return [`${g}—`, L`まだ確定していません`];
}

/** Chain mode: the seal marks of the offices held, and their explanation. */
function chainSeal(states, dirty) {
  if (!states.length) return { markup: '', title: '' };
  const marks = states.map(s => [s, sealMark(s)]);
  const title = marks.map(([s, [, what]]) => `${L`${T.ROLE_JA[s.role]}：${what}`}${s.digest ? `\ndecision_digest ${s.digest}` : ''}`).join('\n');
  const dg = states.find(s => s.digest)?.digest;
  if (dirty) return { markup: html`<span class="seal">${L`確定で封印`}</span>`, title };
  if (!states.some(s => s.committed || s.sending || s.failed)) return { markup: '', title };
  return { markup: html`<span class="seal">${states.some(s => s.committed) ? '🔒 ' : ''}${dg ? `${L`封印 ${dg.slice(0, 6)}…`} ` : ''}${marks.map(([, [m]]) => m).join(' ')}</span>`, title };
}

/** Pips per office held; the bank is a reservoir counter, not more pips. */
function officePips(role) {
  const o = officeInfo(role), u = used(role);
  const pips = [];
  for (let i = 0; i < o.budget; i++) pips.push(html`<span class="pip ${i < Math.min(u, o.budget) ? 'used' : ''}"></span>`);
  const bankUsed = Math.max(0, u - o.budget);
  const bank = o.bank ? html`<span class="bank-well ${bankUsed ? 'drawing' : ''}" title="${L`繰越の枠`}"><i style="height:${Math.round(100 * (o.bank - bankUsed) / Math.max(1, BANK_TICKS * o.budget))}%"></i><b>+${o.bank - bankUsed}</b></span>` : '';
  return html`<span class="office-pips" title="${L`${T.ROLE_JA[role]}：このティック ${o.budget}・繰越 ${o.bank}`}"><span class="og">${T.ROLE_GLYPH[role]}</span>${pips.length ? pips : html`<span class="meta">${L`枠0`}</span>`}${bank}</span>`;
}

export function renderDock() {
  const v = S.view;
  if (!v.economy) return;
  const offices = held();
  const props = S.drafts.filter(d => d.proposal).length;
  setHtml($('#dock-budget'), offices.length
    ? html`<div class="pips" aria-label="${L`命令の枠`}">${offices.map(officePips)}</div><div class="budget-text num">${offices.map(r => `${T.ROLE_JA[r]} ${used(r)}/${spendable(r)}`).join(' · ')}${props ? ` · ${L`献策 ${props}件`}` : ''}</div>
      <span class="tip panel"><div class="tip-row"><span>${L`国の枠`}</span><span>${BUDGET_TEXT()}</span></div><div class="tip-row"><span>${L`役職ごとの割り当て`}</span><span>${T.ROLES.map(r => T.ROLE_JA[r]).join(listSep())}</span></div><div class="tip-row"><span>${L`繰越`}</span><span>${L`役職ごとに最大${BANK_TICKS}ティック分`}</span></div><div class="tip-row"><span>${L`取引所・同意`}</span><span>${L`枠を使わない`}</span></div></span>`
    : html`<div class="budget-text">${props ? Lh`役職がないので、命令はすべて<b>献策</b>になります（${props}件）` : Lh`役職がないので、命令はすべて<b>献策</b>になります`}</div>`);
  setHtml($('#chips'), S.drafts.length
    ? S.drafts.map((d, i) => html`<span class="chip ${d.usdc ? 'usdc' : ''} ${d.proposal ? 'proposal' : ''}" data-chip="${i}" title="${d.label}${d.proposal ? L`（${T.ROLE_JA[d.office]}への献策）` : ''}"><span>${d.proposal ? '✎' : d.glyph}</span><span class="t">${d.label}</span>${d.proposal ? html`<span class="pt">${L`献策→${T.ROLE_JA[d.office]}`}</span>` : ''}<button class="x" type="button" data-remove="${i}" aria-label="${L`取り消す`}">×</button></span>`)
    : html`<span class="empty">${v.chain && !S.session ? NO_KEY() : offices.length ? L`命令はまだありません。地図で部隊や都市を選んでください（Space＝次の判断）。担当外の命令は献策になります。` : L`あなたは役職についていません。地図で選んだ命令は、担当の役職者への献策になります。国の広場で投票・支持・リコールもできます。`}</span>`);
  if (S.pulseChip !== null) { // highlight a chip just added
    const chip = $(`#chips [data-chip="${S.pulseChip}"]`);
    S.pulseChip = null;
    if (chip) { chip.classList.add('pulse'); setTimeout(() => chip.classList.remove('pulse'), 900); }
  }
  const dirty = isDirty();
  if (v.chain) { renderChainCommit(v, offices, dirty); return; }
  const resolving = !v.paused && v.secondsLeft < .6;
  // Nothing drafted and this tick's turn not ended yet: the button ends the turn (an empty sealed batch per office).
  const idle = !dirty && offices.length && !v.member?.ready;
  const state = v.phase === 'lobby' ? ['draft', L`開幕前`] : resolving ? ['resolving', L`解決中`] : dirty ? ['draft', L`下書き · 未確定`] : idle ? ['draft', L`命令なし · 未確定`] : S.drafts.length ? ['committed', L`確定済み ✓`] : ['committed', L`手番を終えました`];
  const dg = (v.member?.committed || []).find(c => c.digest)?.digest;
  // Commit-reveal as a sealed dispatch: sealed now, opened (and checked) after the tick.
  setHtml($('#digest'), dg && !dirty ? html`<span class="seal">🔒 ${L`封印 ${dg.slice(0, 6)}…`}</span>` : dirty && offices.length ? html`<span class="seal">${L`確定で封印`}</span>` : '');
  $('#digest').title = dg ? `decision_digest ${dg}\nobs_root ${v.decision?.obsRoot}` : '';
  const disabled = S.committing || v.over || v.phase === 'lobby' || isWatching() || (!dirty && !idle);
  setHtml($('#commit'), html`<span class="commit-state ${state[0]}">${state[1]}</span><button class="btn ${dirty ? 'primary' : ''}" type="button" id="commit-btn" ${disabled ? 'disabled' : ''}>${dirty ? (offices.length ? L`確定する` : L`献策する`) : idle ? L`命令なしで手番を終える` : L`確定済み`}<span class="key" style="margin-left:4px">⌃↵</span></button>`);
}

/** Chain mode: the seal state per office and the button (closed after the deadline, and without the key). */
function renderChainCommit(v, offices, dirty) {
  const states = officeStates();
  const closed = chainPhase(v) !== 'commit';
  const sending = S.committing || states.some(s => s.sending);
  const failed = states.some(s => s.failed && !s.committed);
  const idle = !dirty && offices.length && !turnEnded();
  const state = closed ? ['resolving', L`解決中`] : sending ? ['draft', L`送信中…`] : dirty ? ['draft', L`下書き · 未確定`]
    : failed ? ['draft', L`送れていない役職があります`] : idle ? ['draft', L`命令なし · 未確定`] : S.drafts.length ? ['committed', L`確定済み ✓`] : ['committed', L`手番を終えました`];
  const seal = chainSeal(states, dirty && offices.length);
  setHtml($('#digest'), seal.markup);
  $('#digest').title = seal.title ? `${seal.title}${v.decision?.obsRoot ? `\nobs_root ${v.decision.obsRoot}` : ''}` : '';
  const keyless = !S.session;
  const disabled = sending || v.over || closed || keyless || isWatching() || (!dirty && !idle);
  const label = keyless ? L`鍵がないため見るだけ` : closed ? L`締切後（解決中）` : dirty ? (offices.length ? L`確定する` : L`献策する`) : idle ? L`命令なしで手番を終える` : L`確定済み`;
  setHtml($('#commit'), html`<span class="commit-state ${state[0]}">${state[1]}</span><button class="btn ${dirty && !closed && !keyless ? 'primary' : ''}" type="button" id="commit-btn" ${disabled ? 'disabled' : ''}>${label}<span class="key" style="margin-left:4px">⌃↵</span></button>`);
}
