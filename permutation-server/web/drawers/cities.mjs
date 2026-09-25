// Cities & armies: everything I own, with idle ones marked.
import * as T from '../i18n.mjs';
import { html, troops } from '../util.mjs';
import { myCities, myUnits } from '../state.mjs';
import { keyOf } from '../world.mjs';
import { draftedUnits } from '../orders.mjs';

export function drawerCities() {
  const cities = myCities(), units = myUnits();
  const drafted = draftedUnits();
  const idle = html`<span class="idle">待機</span>`;
  const unitState = u => (u.path?.length ? `· 移動中（残り${u.path.length}）` : drafted.has(u.id) ? '· 命令あり' : u.standing ? `· ${T.STANDING_GLYPH[u.standing.kind]} ${T.standingText(u.standing)}` : html`· ${idle}`);
  return html`<div class="eyebrow">CITIES & ARMIES</div><h2>都市と軍</h2><p class="drawer-intro">待機中のものには印がつきます。クリックで地図がその場所へ移動します。</p>
    <div class="section-title">都市 ${cities.length}</div>
    ${cities.map(c => html`<div class="list-row" data-focus="${keyOf(c)}"><div class="main"><div class="title">${c.capital ? '★ ' : ''}${T.cityName(c.id)} <span class="meta">人口${c.pop}</span></div><div class="meta">${(c.queue || []).length ? `生産：${T.itemName(c.queue[0])}` : html`<span class="idle">生産予定なし</span>`}</div></div><span class="meta">防御 ${(c.defense / 10).toFixed(0)}/${(c.defenseMax / 10).toFixed(0)}</span></div>`)}
    <div class="section-title">部隊 ${units.length}</div>
    ${units.length ? units.map(u => html`<div class="list-row" data-select-unit="${u.id}"><div class="main"><div class="title">${T.UNIT_GLYPH[u.type]} ${T.UNIT[u.type]} ${u.civilian ? '' : html`<span class="meta">兵${troops(u.troops)}</span>`}</div><div class="meta">${u.q}, ${u.r} ${unitState(u)}</div></div></div>`) : html`<p class="desc">部隊はいません。</p>`}`;
}
