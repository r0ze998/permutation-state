// Research: the plan (up to three techs) and the tech tree by era.
import * as T from '../i18n.mjs';
import * as api from '../api.mjs';
import { html, attrJson, fmt } from '../util.mjs';
import { S, invalidate } from '../state.mjs';
import { QUEUE_MAX } from '../rules.mjs';
import { L } from '../lang.mjs';

export async function loadResearch() {
  const r = await api.tryGet('/api/preview/research');
  if (r) { S.research = r; invalidate('drawer'); }
}

const ERAS = [[1, 'I'], [2, 'II'], [3, 'III'], [4, 'IV']];

export function drawerResearch() {
  const r = S.research, e = S.view.economy;
  const draft = S.drafts.find(d => d.dto.type === 'SetResearch');
  const queue = draft ? draft.dto.techs : e.researchQueue;
  const planned = new Set([...e.techs, ...queue]);
  const tech = t => {
    const inQ = queue.includes(t.tech);
    const prereqOk = t.prereqs.every(p => planned.has(p));
    const can = !t.held && !inQ && prereqOk && queue.length < QUEUE_MAX;
    const why = t.held ? L`研究済み` : inQ ? L`予定に入っています` : !prereqOk ? L`先に${t.prereqs.filter(p => !planned.has(p)).map(p => T.TECH[p]).join(L`・`)}が必要` : queue.length >= QUEUE_MAX ? L`予定は${QUEUE_MAX}件までです` : '';
    const dto = { type: 'SetResearch', techs: [...queue, t.tech].slice(0, QUEUE_MAX) };
    return html`<button class="option ${t.held ? 'queued' : ''}" type="button" ${can ? html`data-order="${attrJson(dto)}"` : html`aria-disabled="true"`}><span class="ic">${t.held ? '✓' : '✧'}</span><span><span class="name">${T.TECH[t.tech]}</span><div class="meta">${L`解放：${T.TECH_UNLOCK[t.tech]}`}</div>${why && !t.held ? html`<div class="why">${why}</div>` : ''}</span><span class="cost">${fmt(t.cost)}</span></button>`;
  };
  return html`<div class="eyebrow">${L`RESEARCH · 研究`}</div><h2>${L`研究`}</h2><p class="drawer-intro">${L`科学は毎ティック蓄積され、予定の先頭の技術に使われます。都市が1つ増えるごとに研究コストは10%上がります。予定は最大${QUEUE_MAX}件。`}</p>
    ${e.research ? html`<div class="meter"><span>${L`研究中：${T.TECH[e.research.tech]} ${fmt(e.research.store)} / ${fmt(e.research.cost)}`}</span><div class="bar"><i style="width:${Math.min(100, e.research.store / e.research.cost * 100)}%"></i></div></div>` : ''}
    <div class="section-title">${L`予定 ${draft ? L`· 命令あり` : ''}`}</div>${queue.length ? html`${queue.map((t, i) => html`<div class="option queued"><span class="ic">✧</span><span class="name">${i + 1}. ${T.TECH[t]}</span><span></span></div>`)}<button class="btn" type="button" data-order="${attrJson({ type: 'SetResearch', techs: [] })}">${L`予定をクリア（枠1）`}</button>` : html`<p class="desc">${L`予定はありません。`}</p>`}
    ${r ? ERAS.map(([era, roman]) => html`<div class="tech-era">ERA ${roman}</div>${r.filter(t => t.era === era).map(tech)}`) : html`<p class="desc">${L`計算しています…`}</p>`}`;
}
