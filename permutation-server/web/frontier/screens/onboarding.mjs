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

const TITLE = {
  welcome: () => L`ようこそ`,
  join: () => L`勢力と入植地`,
  build: () => L`最初の建設`,
  scout: () => L`最初の斥候`,
  practice: () => L`練習の衝突`,
  march: () => L`最初の封をした進軍`,
  report: () => L`最初の報告`,
  done: () => L`完了`,
};
const DO = {
  welcome: () => L`五つの言葉だけ覚えましょう：拠点（あなたの土地）、軍勢（動かす兵）、鐘（10分ごとの区切り）、進軍（封をした移動）、探索（斥候で周りを調べる）。`,
  join: stage => ({
    none: () => L`地図のタブで勢力を選び、ゲーム内の鍵を作って参加します。`,
    joined: () => L`空いている区画を3つまで選んで入植希望を出します。`,
    ticket: () => L`入植希望を出しました。この鐘の乱数で区画が決まります。`,
    refugee: () => L`拠点を失いました。もう一度入植希望を出せます。`,
  }[stage]?.() ?? L`空いている区画を3つまで選んで入植希望を出します。`),
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
  const st = onboardingState(factsOf(FS), FS.ui?.dismissed ?? [], FS.clock ?? null);
  if (st.dismissed) return '';
  const cur = st.steps.find(s => s.id === st.current);
  const n = st.steps.filter(s => s.status === 'done' || s.status === 'skipped').length;
  const total = STEPS.length - 1;
  const body = html`<p>${cur.id === 'join' ? DO.join(FS.land?.stage ?? 'none') : DO[cur.id]()}</p>
    ${cur.wait ? html`<p class="muted">${waitText(cur.wait)}</p>` : ''}
    ${cur.id === 'join' && (FS.land?.stage === 'joined' || FS.land?.stage === 'refugee') ? renderOverflow(FS) : ''}
    <p>${cur.id === 'welcome' ? html`<button type="button" class="btn primary" data-act="ob-seen" data-flag="welcome">${L`わかりました`}</button>` : ''}
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

/** The adjacent-wedge offer when the home wedge shows no free site (§5.9). */
export function renderOverflow(FS) {
  const faction = FS.citizen?.faction;
  if (!Number.isInteger(faction)) return '';
  const o = overflowProvinces(FS.overviews ?? new Map(), faction, FS.record?.rings?.length ?? 1);
  if (!o.full) return '';
  if (!o.provinces.length) return html`<p class="warn">${L`本拠の扇区に空き区画がなく、隣の扇区のいちばん外の輪にも空きが見つかりません。新しい輪がひらくのを待ってください。`}</p>`;
  return html`<div class="warn"><p>${L`本拠の扇区に空き区画がありません。この場合は隣の扇区のいちばん外の輪（第${o.ring}輪）の区画に入植希望を出せます（最後はチェーンが判断します）。`}</p>
    <ul class="list">${o.provinces.slice(0, 12).map(pr => html`<li><button type="button" class="btn" data-act="pick-province" data-p="${pr.p}" data-q="${pr.q}">${L`州 ${pr.p},${pr.q}（第${pr.ring}輪、空き 約 ${fmtNum(pr.free)}）`}</button></li>`)}</ul></div>`;
}

/** The "show the guide again" control for the More tab (only while the card is dismissed or steps are skipped). */
export function renderRestore(FS) {
  const flags = FS.ui?.dismissed ?? [];
  if (!flags.some(x => x === 'onboarding' || x.startsWith('ob:skip:'))) return '';
  return html`<button type="button" class="btn" data-act="ob-restore">${L`ガイドをもう一度表示する`}</button>`;
}
