// The onboarding card (web design §7.11): the checklist of the guided first
// bells, driven by onboarding.mjs (chain facts first, local "seen" flags
// only). The current step says what to do and, while the chain works, what
// it is waiting for and about when (O-M1-13 times); each step can be
// skipped and the card dismissed (it returns from the More tab). On the map
// tab the card is open; on the other tabs it is one line that opens.
import { html, raw } from '../../util.mjs';
import { L, Lh, fmtNum } from '../../lang.mjs';
import { PIPELINE_TEXT } from '../fi18n.mjs';
import { STEPS, onboardingState, factsOf, reportOffer, overflowProvinces } from '../onboarding.mjs';
import { timeHtml } from './shell.mjs';
import { guideLevel, guideTarget, goText } from '../hud/guide.mjs';

const TITLE = {
  welcome: () => L`ようこそ`,
  join: () => L`勢力と拠点`,
  build: () => L`最初の建設`,
  scout: () => L`最初の斥候`,
  practice: () => L`練習の衝突`,
  march: () => L`最初の封をした進軍`,
  report: () => L`最初の報告`,
  done: () => L`完了`,
};
const DO = {
  welcome: () => L`五つの言葉だけ覚えましょう：拠点（あなたの土地）、軍勢（動かす兵）、鐘（10分ごとの区切り）、進軍（封をした移動）、探索（斥候で周りを調べる）。`,
  // stage-neutral (a returning or refugee player too), following the automatic ticket (review finding 9)
  join: FS => {
    const stage = FS.land?.stage ?? 'none', st = FS.autoTicket?.state;
    if (stage === 'none') return L`地図のタブで六つの勢力から一つを選び、ゲーム内の鍵を作って参加します。選ぶのは勢力だけです。`;
    if (stage === 'ticket' || st === 'sent') return L`入植希望を自動で出しました。次の鐘（約11〜21分後）に拠点が決まります。待つあいだに練習で戦ってみましょう。`;
    if (st === 'nofree') return L`空いた区画が見つかりません。鐘ごとに自動で探し直します。`;
    if (st === 'room') return L`この鐘の入植希望の枠がいっぱいです。次の鐘に自動で出します。`;
    if (st === 'failed') return L`入植希望を出せませんでした。次の鐘に自動でもう一度出します（地図のタブからすぐ出し直せます）。`;
    return L`空いた区画に、拠点の入植希望を自動で出しています。`;
  },
  build: () => L`拠点のタブで農場と木材所を建てます。`,
  scout: () => L`斥候を訓練して軍勢に編成し、隣の2マスを探索します。`,
  practice: () => L`練習モードで蛮族の野営地を襲ってみます。チェーンには何も送りません。`,
  march: () => L`届く範囲の蛮族の野営地へ、封をした進軍を送ります。行き先と構えは到着の鐘まであなたにしか見えません。チップはキーパーへの報酬で、このタブを閉じてもキーパーが開封します。`,
  report: () => L`衝突の報告を読みます。「このブラウザで確かめる」で結果を自分で計算し直せます。`,
  done: () => L`最初の鐘の案内は終わりです。ガイドは「その他」からいつでも開けます。`,
};
const GO = {
  join: () => html`<button type="button" class="btn" data-act="tab" data-tab="map">${L`地図を開く`}</button>`,
  build: () => html`<button type="button" class="btn" data-act="tab" data-tab="holding">${L`拠点を開く`}</button>`,
  scout: () => html`<button type="button" class="btn" data-act="tab" data-tab="hosts">${L`軍勢を開く`}</button>`,
  practice: () => html`<button type="button" class="btn" data-act="practice-open">${L`練習を開く`}</button>`,
  march: () => html`<button type="button" class="btn" data-act="tab" data-tab="marches">${L`進軍を開く`}</button>`,
};

/** The wait line of a step: expected time, or the pipeline state of the march being waited for. */
export function waitText(w) {
  if (!w) return '';
  const range = L`約${w.minutes[0]}〜${w.minutes[1]}分`;
  if (w.kind === 'ticket') return w.at ? Lh`結果は ${timeHtml(w.at)} ごろ（出してから${range}）。待つあいだに練習できます。` : L`結果は出してから${range}で決まります。`;
  if (w.kind === 'explore') return L`探索の結果は送ってから${range}で出ます。`;
  if (w.kind === 'report') return html`${L`報告は出発から${range}でできます。いまは：`}${PIPELINE_TEXT[w.pipeline] ?? ''}`;
  return '';
}

function stepItem(s) {
  const mark = s.status === 'done' ? L`済み` : s.status === 'skipped' ? L`飛ばした` : s.status === 'current' ? L`いま` : '';
  return html`<li class="${s.status === 'done' ? 'done' : s.status === 'current' ? 'now' : ''}" ${raw(s.status === 'current' ? 'aria-current="step"' : '')}>${TITLE[s.id]()}${mark ? html` <span class="visually-hidden">${L`（${mark}）`}</span>` : ''}</li>`;
}

/**
 * The card for the store: `open` shows the current step in full (the map
 * tab), else one summary line that expands. Nothing when dismissed.
 */
export function render(FS, { open = true } = {}) {
  if (guideLevel(FS) !== 'all') return '';
  const st = onboardingState(factsOf(FS), FS.ui?.dismissed ?? [], FS.clock ?? null);
  if (st.dismissed) return '';
  const target = guideTarget(FS);
  const cur = st.steps.find(s => s.id === st.current);
  const n = st.steps.filter(s => s.status === 'done' || s.status === 'skipped').length;
  const total = STEPS.length - 1;
  const body = html`<p>${cur.id === 'join' ? DO.join(FS) : DO[cur.id]()}</p>
    ${cur.wait ? html`<p class="muted">${waitText(cur.wait)}</p>` : ''}
    <p>${cur.id === 'welcome' ? html`<button type="button" class="btn primary" data-act="ob-seen" data-flag="welcome">${L`わかりました`}</button>` : ''}
      ${target && target.step === cur.id ? html`<button type="button" class="btn primary" data-act="ob-go">${goText(target)}</button>` : ''}
      ${cur.id === 'report' ? renderReportGo(FS) : GO[cur.id]?.() ?? ''}
      ${cur.id !== 'done' ? html`<button type="button" class="btn small" data-act="ob-skip" data-step="${cur.id}">${L`この手順を飛ばす`}</button>` : ''}
      <button type="button" class="btn small" data-act="ob-dismiss">${L`ガイドを閉じる`}</button></p>`;
  const head = L`ガイド ${n}/${total}：${TITLE[cur.id]()}`;
  if (!open) return html`<details class="callout onboarding"><summary>${head}</summary><ol class="steps">${st.steps.map(stepItem)}</ol>${body}</details>`;
  return html`<section class="callout onboarding" aria-labelledby="ob-title"><h3 id="ob-title">${head}</h3>
    <ol class="steps">${st.steps.map(stepItem)}</ol>${body}</section>`;
}

function renderReportGo(FS) {
  const r = reportOffer(FS.marches);
  return r ? html`<button type="button" class="btn primary" data-act="report-open" data-p="${r.p}" data-q="${r.q}" data-bell="${r.bell}">${L`報告を開く`}</button>` : '';
}

/** The "show the guide again" control for the More tab (only while the card is dismissed or steps are skipped). */
export function renderRestore(FS) {
  const flags = FS.ui?.dismissed ?? [];
  if (!flags.some(x => x === 'onboarding' || x.startsWith('ob:skip:'))) return '';
  return html`<button type="button" class="btn" data-act="ob-restore">${L`ガイドをもう一度表示する`}</button>`;
}
