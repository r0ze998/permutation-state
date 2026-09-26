// Inspector: my city, a foreign city, a city-state.
import * as T from '../i18n.mjs';
import { html, attrJson, hexDist, troops } from '../util.mjs';
import { S, civ, civN } from '../state.mjs';
import { keyOf, unitsAt } from '../world.mjs';
import { AUTO_PURCHASE_STEPS, ENVOY_AMOUNTS, PURCHASE_GOLD, QUEUE_MAX, SUZERAIN_MIN, SUZERAIN_REVIEW } from '../rules.mjs';
import { head } from './head.mjs';
import { L } from '../lang.mjs';

const defense = c => html`${(c.defense / 10).toFixed(1)}<small>/${(c.defenseMax / 10).toFixed(0)}</small>`;

export function myCityPanel(c) {
  const p = S.cityPreview?.id === c.id ? S.cityPreview : null;
  const o = p?.outlook;
  const qDraft = S.drafts.find(d => d.dto.type === 'SetQueue' && d.dto.city === c.id);
  const queue = qDraft ? qDraft.dto.items : (c.queue || []);
  const fDraft = S.drafts.find(d => d.dto.type === 'SetFocus' && d.dto.city === c.id);
  const focus = fDraft ? fDraft.dto.focus : c.focus;
  const here = unitsAt(keyOf(c)).filter(u => u.owner === S.myCiv);
  const top = html`${head(`${c.capital ? L`CAPITAL · 首都` : L`CITY · 都市`} · ${c.q}, ${c.r}`, T.cityName(c.id), '')}<div class="stats">
    <div class="stat"><div class="k">${L`人口`}</div><div class="v">${c.pop}</div></div>
    <div class="stat"><div class="k">${L`食料`}</div><div class="v">${o ? (o.foodSurplus >= 0 ? '+' : '') + o.foodSurplus : '…'}<small>/t</small></div></div>
    <div class="stat"><div class="k">${L`生産`}</div><div class="v">${o ? o.production : '…'}<small>/t</small></div></div>
    <div class="stat"><div class="k">${L`防御`}</div><div class="v">${defense(c)}</div></div>
    <div class="stat"><div class="k">${L`忠誠`}</div><div class="v">${c.loyalty}</div></div>
    <div class="stat"><div class="k">${L`快適度`}</div><div class="v">${o ? o.amenities : '…'}</div></div></div>
    ${o ? html`<div class="meter"><span>${L`成長：${c.food} / ${o.growthThreshold}${o.ticksToGrow ? L`（あと約${o.ticksToGrow}ティック）` : L`（食料が足りず停滞）`}`}</span><div class="bar"><i style="width:${Math.min(100, c.food / o.growthThreshold * 100)}%"></i></div></div>` : ''}
    <div class="tags">${(c.buildings || []).length ? c.buildings.map(b => html`<span class="tag">${T.BUILDING_GLYPH[b]} ${T.BUILDING[b]}</span>`) : html`<span class="tag">${L`建物なし`}</span>`}</div>
    ${here.length ? html`<div class="section-title">${L`この都市の部隊（もう一度クリックでも選べます）`}</div><div class="row">${here.map(u => html`<button class="btn" type="button" data-select-unit="${u.id}">${T.UNIT_GLYPH[u.type]} ${T.UNIT[u.type]}${u.civilian ? '' : ` ${L`兵${troops(u.troops)}`}`}</button>`)}</div>` : ''}
    <div class="section-title">${L`方針（土地の割り当て）`}${fDraft ? L` · 命令あり` : ''}</div><div class="row">${Object.entries(T.FOCUS).map(([k, n]) => html`<button class="btn ${k === focus ? 'primary' : ''}" type="button" data-order="${attrJson({ type: 'SetFocus', city: c.id, focus: k })}" ${k === focus ? 'disabled' : ''}>${n}</button>`)}</div>
    ${citySection(c)}
    <div class="section-title">${L`生産予定`} ${qDraft ? L`· 命令あり（確定で置き換わります）` : ''}</div>
    ${queue.length ? html`${queue.map((it, i) => html`<div class="option queued"><span class="ic">${T.itemGlyph(it)}</span><span><span class="name">${i + 1}. ${T.itemName(it)}</span>${i === 0 ? html`<div class="meta">${L`蓄積 ${c.prod}${o ? L` · 毎ティック +${o.production}` : ''}`}</div>` : ''}</span><span></span></div>`)}<div class="row"><button class="btn" type="button" data-queue-clear="${c.id}">${L`予定をクリア（枠1）`}</button><button class="btn" type="button" data-order="${attrJson({ type: 'Purchase', city: c.id, gold: PURCHASE_GOLD })}">◆ ${L`${PURCHASE_GOLD}金で生産を購入`}</button></div>`
      : html`<div class="explanation">${L`生産予定がありません。生産は都市に蓄積されるので無駄にはなりませんが、下から選んでください。`}</div>`}
    <div class="section-title">${L`追加できるもの（最大${QUEUE_MAX}件・枠1）`}</div>`;
  if (!p) return html`${top}<p class="desc">${L`選択肢を計算しています…`}</p>`;
  const full = queue.length >= QUEUE_MAX;
  return html`${top}${p.options.map(opt => {
    const it = opt.item, blocked = opt.blocked;
    const effect = it.kind === 'Building' ? T.BUILDING_EFFECT[it.building] : it.kind === 'Troops' ? L`${T.UNIT[it.unit]}の部隊（兵5）` : it.kind === 'Settler' ? L`新しい都市を建てる（人口−1）` : L`偵察用`;
    const dto = { type: 'SetQueue', city: c.id, items: [...queue, it].slice(0, QUEUE_MAX) };
    return html`<button class="option" type="button" ${blocked || full ? html`aria-disabled="true"` : html`data-order="${attrJson(dto)}"`}><span class="ic">${T.itemGlyph(it)}</span><span><span class="name">${T.itemName(it)}</span><div class="meta">${effect}${opt.ticks ? L` · 約${opt.ticks}ティック` : ''}</div>${blocked ? html`<div class="why">${T.blockedText(blocked)}</div>` : full ? html`<div class="why">${L`予定は${QUEUE_MAX}件までです`}</div>` : ''}</span><span class="cost">${opt.cost}<small style="font-size:9px;color:var(--muted)"> ${L`生産`}</small></span></button>`;
  })}`;
}

/** Standing rules of my city (§13): repeat the queue, auto-purchase. */
function citySection(c) {
  const st = c.standing || { repeatQueue: true, autoPurchase: 0 };
  const order = rule => attrJson({ type: 'SetStanding', target: { kind: 'City', id: c.id }, rule });
  const pending = kind => S.drafts.find(d => d.dto.type === 'SetStanding' && d.dto.target.kind === 'City' && d.dto.target.id === c.id && d.dto.rule.kind === kind);
  const rp = pending('QueueRepeat'), ap = pending('AutoPurchase');
  const repeat = rp ? rp.dto.rule.on : st.repeatQueue, buy = ap ? ap.dto.rule.maxGold : st.autoPurchase;
  return html`<div class="section-title">${L`継続命令 · 毎ティック自動`}${rp || ap ? L` · 下書きあり` : ''}</div>
    <div class="standing-row"><span class="sl">${T.STANDING_GLYPH.QueueRepeat} ${L`生産の繰り返し`}</span>${[[true, 'ON'], [false, 'OFF']].map(([on, t]) => html`<button class="btn ${repeat === on ? 'primary' : ''}" type="button" data-order="${order({ kind: 'QueueRepeat', on })}" ${repeat === on ? 'disabled' : ''}>${t}</button>`)}</div>
    <div class="standing-row"><span class="sl">${T.STANDING_GLYPH.AutoPurchase} ${L`自動購入`}</span>${AUTO_PURCHASE_STEPS.map(g => html`<button class="btn ${buy === g ? 'primary' : ''}" type="button" data-order="${order({ kind: 'AutoPurchase', maxGold: g })}" ${buy === g ? 'disabled' : ''}>${g ? L`${g}金` : L`なし`}</button>`)}</div>
    <p class="desc">${L`繰り返し：予定が空になったら最後の部隊をもう一度作ります。自動購入：毎ティック指定額まで金で生産を進めます（手動の購入をしたティックは除く）。`}</p>`;
}

export function foreignCityPanel(c) {
  const o = c.owner; const rel = o === null ? null : civ(o)?.relation;
  const mine = S.view.units.filter(u => u.owner === S.myCiv && !u.civilian && hexDist(u, c) <= 2);
  return html`${head(`${o === null ? L`FREE CITY · 自由都市` : L`CITY · 都市`} · ${c.q}, ${c.r}`, T.cityName(c.id), o === null ? L`どの勢力にも属さない自由都市です。攻撃は侵略扱いになります。` : c.capital ? L`${civN(o)}の首都です。関係：${T.RELATION[rel]}` : L`${civN(o)}の都市です。関係：${T.RELATION[rel]}`)}
    <div class="stats"><div class="stat"><div class="k">${L`人口`}</div><div class="v">${c.pop}</div></div><div class="stat"><div class="k">${L`防御`}</div><div class="v">${defense(c)}</div></div><div class="stat"><div class="k">${L`城壁`}</div><div class="v">${c.walls ? L`あり` : L`なし`}</div></div></div>
    ${c.stages ? html`<div class="explanation">${L`スターゲート ${c.stages}/3 段階。この都市を占領すると、完成した段階はすべて失われます。`}</div>` : ''}
    ${mine.length ? html`<div class="section-title">${L`近くのあなたの部隊`}</div>${mine.map(u => html`<button class="option" type="button" data-select-unit="${u.id}"><span class="ic">${T.UNIT_GLYPH[u.type]}</span><span><span class="name">${T.UNIT[u.type]} ${L`兵${troops(u.troops)}`}</span><div class="meta">${L`選択して攻撃の予測を見る`}</div></span><span></span></button>`)}` : ''}
    ${rel && rel !== 'war' && o !== null ? html`<button class="btn wide" type="button" data-drawer="diplomacy">✉ ${L`${civN(o)}との外交を開く`}</button>` : ''}`;
}

export function cityStatePanel(cs) {
  const me = S.view.economy.influence;
  return html`${head(L`CITY-STATE · 都市国家 · ${cs.q}, ${cs.r}`, L`都市国家 ${cs.id + 1}`, L`${T.SPECIALTY[cs.specialty]}の都市国家。${T.SPECIALTY_BONUS[cs.specialty]}。`)}
    <div class="stats"><div class="stat"><div class="k">${L`宗主`}</div><div class="v" style="font-size:14px">${cs.suzerain === null ? L`なし` : civN(cs.suzerain)}</div></div><div class="stat"><div class="k">${L`あなたの影響力`}</div><div class="v">${cs.myInfluence}</div></div><div class="stat"><div class="k">${L`最多`}</div><div class="v">${cs.topInfluence}</div></div></div>
    <div class="meter"><span>${L`宗主になる条件：影響力${SUZERAIN_MIN}以上で最多（${SUZERAIN_REVIEW}ティックごとに見直し、全員の影響力が半分に）`}</span><div class="bar gold"><i style="width:${Math.min(100, cs.myInfluence / SUZERAIN_MIN * 100)}%"></i></div></div>
    <div class="section-title">${L`使節を送る（枠1・所持影響力 ${me}）`}</div><div class="row">${ENVOY_AMOUNTS.map(n => html`<button class="btn" type="button" ${me < n ? html`disabled title="${L`影響力が足りません`}"` : ''} data-order="${attrJson({ type: 'SendEnvoy', cityState: cs.id, influence: n })}">${L`影響力 ${n}`}</button>`)}</div>
    <div class="explanation">${L`都市国家への攻撃は侵略扱いです：すべての都市国家への影響力を失います（協調の道の節目に響きます）。`}</div>`;
}
