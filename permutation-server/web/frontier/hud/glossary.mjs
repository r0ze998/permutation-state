// Terms explained where they appear (UI plan E3; Civ's nested tooltips and
// Civilopedia, kept to one level as Old World teaches): a small "?" next to
// a term opens a short definition with a link to the glossary in More. A
// definition never holds another term button — one level, no rabbit hole.
import { html } from '../../util.mjs';
import { L } from '../../lang.mjs';

export const TERMS = Object.freeze({
  bell: { name: () => L`鐘`, text: () => L`10分ごとの区切り。鐘の始まりの顔ぶれで、その州の衝突がまとめて決まります。` },
  stance: { name: () => L`構え`, text: () => L`進軍の戦い方。突撃は側撃に、側撃は迎撃に、迎撃は突撃に強い。待機の構えは相性なし。` },
  retreat: { name: () => L`撤退の比率`, text: () => L`相手の兵がこの倍率を超えていたら、戦わずに引き返します（損失なし）。` },
  seal: { name: () => L`封`, text: () => L`進軍の行き先と構えは封をして送ります。着く鐘までは、あなたにしか見えません。他の人に見えるのは出発と到着の鐘だけです。` },
  dormant: { name: () => L`休眠`, text: () => L`領主が長く何もしないと拠点は休眠し、さらに放っておくと手放されます。何か操作すれば延びます。` },
  ring: { name: () => L`輪`, text: () => L`大協約を中心にした州の輪。シーズンが進むと外側の輪が開きます。` },
  garrison: { name: () => L`守備隊`, text: () => L`拠点に残る守り手。鐘の始まりに数えられ、攻め手に反撃します。城壁があると強くなります。` },
  vigil: { name: () => L`夜番`, text: () => L`毎日決まった時間、あなたが眠っている間も守りを固める時間帯です。` },
  shield: { name: () => L`保護`, text: () => L`拠点ができてしばらくは攻められません。保護が切れるまでの時間です。` },
  tier: { name: () => L`段階`, text: () => L`村・町・都市・城塞。上がるほど生産と守りが増えます。` },
});

/** The "?" button after a term (`key` in TERMS). */
export const termButton = key => (TERMS[key] ? html`<button type="button" class="term-q" data-act="term" data-term="${key}" aria-label="${L`${TERMS[key].name()}とは`}" aria-haspopup="dialog">?</button>` : '');

/** The popover's body for one term. */
export function renderTerm(key) {
  const t = TERMS[key];
  if (!t) return '';
  return html`<div class="res-pop-head"><strong>${t.name()}</strong><span></span><button type="button" class="btn small" data-act="term-close" aria-label="${L`閉じる`}">×</button></div>
    <p>${t.text()}</p><p><button type="button" class="btn small" data-act="glossary-open">${L`用語集を開く`}</button></p>`;
}

/** The glossary (More): every term and its definition. */
export function renderGlossary() {
  return html`<section aria-labelledby="glossary-title" id="glossary"><h3 id="glossary-title">${L`用語集`}</h3>
    <dl class="glossary">${Object.values(TERMS).map(t => html`<div><dt>${t.name()}</dt><dd>${t.text()}</dd></div>`)}</dl></section>`;
}
