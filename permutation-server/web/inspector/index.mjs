// The inspector (right column): explains what is selected and offers what
// can be done with it; blocked options carry the engine's own reason.
import * as T from '../i18n.mjs';
import { $, html, setHtml } from '../util.mjs';
import { S, held, myNation, unitById } from '../state.mjs';
import { keyOf, unitsAt, map } from '../world.mjs';
import { othersText } from '../lobby.mjs';
import { tilePanel } from './tile.mjs';
import { unitPanel, targetPanel, patrolPanel } from './unit.mjs';
import { myCityPanel, foreignCityPanel, cityStatePanel } from './city.mjs';
import { head } from './head.mjs';

export function renderInspector() {
  const el = $('#inspector');
  el.classList.toggle('is-idle', S.unit === null && !S.tile && !S.patrol); // phones hide the idle intro to keep the map visible
  setHtml(el, panel());
}

function panel() {
  if (S.patrol) return patrolPanel();
  if (S.unit !== null) {
    const u = unitById(S.unit);
    if (u && S.tile && S.tile !== keyOf(u)) return targetPanel(u, S.tile);
    if (u) return unitPanel(u);
  }
  if (!S.tile) return introPanel();
  const c = S.view.cities.find(c => keyOf(c) === S.tile);
  if (c) return c.owner === S.myCiv ? myCityPanel(c) : foreignCityPanel(c);
  const cs = S.view.cityStates.find(c => keyOf(c) === S.tile && c.capturedBy === null);
  if (cs) return cityStatePanel(cs);
  return tilePanel(map.tiles.get(S.tile), unitsAt(S.tile));
}

function introPanel() {
  const cap = S.view.cities.find(c => c.id === myNation().capital);
  const mine = held();
  return html`${head('YOUR TURN · このティック', mine.length ? `あなたは${mine.map(r => T.ROLE_JA[r]).join('・')}` : 'あなたは国民（役職なし）', '都市・部隊・土地をクリックすると、ここに詳細と「できること」が出ます。できないことには理由が表示されます。')}
    <div class="explanation">${othersText(S.view)}${mine.length ? '担当の役職の命令は封印して送り、担当外の命令は献策になります。' : 'あなたの命令は担当の役職者への献策になります。採用されると功績を半分ずつ分けます。'}</div>
    <button class="btn wide" type="button" data-drawer="nation">⚖ 国の広場を開く（選挙・献策・リコール）</button>
    ${cap ? html`<button class="btn primary wide" type="button" data-focus="${keyOf(cap)}">⌖ 首都 ${T.cityName(cap.id)} を見る</button>` : ''}`;
}
