// Inspector: a plain tile (terrain, yields, owner, units on it).
import * as T from '../i18n.mjs';
import { html, troops } from '../util.mjs';
import { S, civN } from '../state.mjs';
import { ownerOf } from '../world.mjs';
import { tileYield, ownerColor } from '../map.mjs';
import { head } from './head.mjs';
import { L } from '../lang.mjs';

// Perfect information: owner and units are always current (sight is display only).
export function tilePanel(t, units) {
  const o = ownerOf(t.id);
  const y = tileYield(t);
  const hub = S.map.hubs.some(([q, r]) => q === t.q && r === t.r);
  const rough = t.terrain === 'Forest' || t.terrain === 'Hills';
  const owner = o !== null ? L`${civN(o)}の領土です。` : L`どの勢力の領土でもありません。`;
  return html`${head(`${T.TERRAIN_EN[t.terrain]} · ${t.q}, ${t.r}`, T.TERRAIN[t.terrain], owner)}
    <div class="tags">${t.resource ? html`<span class="tag positive">${T.RESOURCE[t.resource]}</span>` : ''}${t.river ? html`<span class="tag">${L`川 · 金+1`}</span>` : ''}${hub ? html`<span class="tag warning">${L`交易拠点 · 金の市場の手数料1%を得る`}</span>` : ''}${t.terrain === 'Mountain' || t.terrain === 'Water' ? html`<span class="tag bad">${L`通行不可`}</span>` : html`<span class="tag">${L`移動コスト ${rough ? 2 : 1}`}</span>`}${rough ? html`<span class="tag">${L`守備側の被害 −20%`}</span>` : ''}</div>
    <div class="stats"><div class="stat"><div class="k">${L`食料`}</div><div class="v">${y[0]}</div></div><div class="stat"><div class="k">${L`生産`}</div><div class="v">${y[1]}</div></div><div class="stat"><div class="k">${L`金`}</div><div class="v">${y[2]}</div></div></div>
    ${units.length ? html`<div class="section-title">${L`この土地の部隊`}</div>${units.map(u => html`<div class="list-row"><div class="main"><div class="title"><span class="swatch-s" style="background:${ownerColor(u.owner)}"></span>${u.owner === 'barbarian' ? L`蛮族の${T.UNIT[u.type]}` : L`${civN(u.owner)}の${T.UNIT[u.type]}`}</div><div class="meta">${u.civilian ? L`非戦闘` : L`兵 ${troops(u.troops)}`}</div></div></div>`)}` : ''}
    <p class="desc">${L`土地は都市の領土に入ると、その都市の人口に応じて自動で使われます（産出の高い順）。`}</p>`;
}
