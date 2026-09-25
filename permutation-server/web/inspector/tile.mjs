// Inspector: a plain tile (terrain, yields, owner, units on it).
import * as T from '../i18n.mjs';
import { html, troops } from '../util.mjs';
import { S, civN } from '../state.mjs';
import { ownerOf, fogOf } from '../world.mjs';
import { tileYield, ownerColor } from '../map.mjs';
import { head } from './head.mjs';

const FOG_NOTE = {
  0: html`<div class="explanation fog">未踏の地です。地形は公開の世界シードから分かりますが、部隊・都市・国境は見えません。偵察で視界に入れてください。</div>`,
  1: html`<div class="explanation fog">霧の中です。国境と都市は最後に見たときのまま表示しています。部隊は視界の中でしか見えません。</div>`,
};

export function tilePanel(t, units) {
  const o = ownerOf(t.id), fog = fogOf(t.id);
  const y = tileYield(t);
  const hub = S.map.hubs.some(([q, r]) => q === t.q && r === t.r);
  const rough = t.terrain === 'Forest' || t.terrain === 'Hills';
  const owner = fog === '0' ? '誰の領土かはまだ分かりません。' : o !== null ? `${civN(o)}の領土${fog === '1' ? '（最後に見たとき）' : ''}です。` : 'どの文明の領土でもありません。';
  return html`${head(`${T.TERRAIN_EN[t.terrain]} · ${t.q}, ${t.r}`, T.TERRAIN[t.terrain], owner)}
    ${FOG_NOTE[fog] || ''}
    <div class="tags">${t.resource ? html`<span class="tag positive">${T.RESOURCE[t.resource]}</span>` : ''}${t.river ? html`<span class="tag">川 · 金+1</span>` : ''}${hub ? html`<span class="tag warning">交易拠点 · 金の市場の手数料1%を得る</span>` : ''}${t.terrain === 'Mountain' || t.terrain === 'Water' ? html`<span class="tag bad">通行不可</span>` : html`<span class="tag">移動コスト ${rough ? 2 : 1}</span>`}${rough ? html`<span class="tag">守備側の被害 −20%</span>` : ''}</div>
    <div class="stats"><div class="stat"><div class="k">食料</div><div class="v">${y[0]}</div></div><div class="stat"><div class="k">生産</div><div class="v">${y[1]}</div></div><div class="stat"><div class="k">金</div><div class="v">${y[2]}</div></div></div>
    ${units.length ? html`<div class="section-title">この土地の部隊</div>${units.map(u => html`<div class="list-row"><div class="main"><div class="title"><span class="swatch-s" style="background:${ownerColor(u.owner)}"></span>${u.owner === 'barbarian' ? '蛮族' : civN(u.owner)}の${T.UNIT[u.type]}</div><div class="meta">${u.civilian ? '非戦闘' : `兵 ${troops(u.troops)}`}</div></div></div>`)}` : ''}
    <p class="desc">土地は都市の領土に入ると、その都市の人口に応じて自動で使われます（産出の高い順）。</p>`;
}
