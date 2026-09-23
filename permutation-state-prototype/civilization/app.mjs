import { CivilizationMap } from './map.mjs';
import { chronicleText } from './copy.mjs';
import * as Strategy from './core.mjs';
import { stockDetail, journeyMarkup, tileValueMarkup, managementMarkup, projectOfferMarkup, projectsMarkup, tileProjectMarkup } from './strategy-ui.mjs';
import { BUILDINGS, TECHNOLOGIES, getBuildPreview, getRoadPreview, getResearchPreview, getCitizenTasks, hexDistance, tileById } from './core.mjs';

const $ = (id) => document.getElementById(id);
const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (c) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const params = new URLSearchParams(location.search);
// Legacy storage key, not the display name. Keep this stable so saved worlds and citizen tokens survive.
const session = params.get('session') || 'aster';
const identityKey = `permutation.citizen.v2:${session}`;
let identity;
try { identity = JSON.parse(sessionStorage.getItem(identityKey)); } catch { /* A new local identity will be created. */ }
if (!identity?.actorId) identity = { actorId: `citizen-${crypto.randomUUID().slice(0, 12)}`, name: params.get('name')?.slice(0, 24) || 'Mara' };
let world = null;
let selection = null;
let drawer = null;
let lens = 'normal';
let tasks = [];
let taskIndex = 0;
let pending = false;
let joined = false;
let reconnecting = false;
let pollTimer;
let lastEventId = null;
let confirmation = null;
let lastMiniDraw = 0;
const resourceKeys = ['food', 'wood', 'stone', 'ore', 'tools', 'knowledge'];
const icons = { food:'❧', wood:'♧', stone:'◆', ore:'⬡', tools:'⚒', knowledge:'✧' };
const resourceNames = { food:'食料', wood:'木材', stone:'石材', ore:'鉱石', tools:'道具', knowledge:'知識' };
const resourceColors = { food:'#829852', wood:'#9b7950', stone:'#7e8c8a', ore:'#a27151', tools:'#5b8278', knowledge:'#7b7698' };
const buildingNames = Object.fromEntries(Object.entries(BUILDINGS).map(([id, definition]) => [id, definition.name]));
const buildingIcons = { townhall:'♜', farm:'❧', lumbermill:'♧', quarry:'◆', mine:'⬡', workshop:'⚒', watchtower:'♖', archive:'✧', warehouse:'▤' };
const buildingEffects = {
  farm:'食料を育て、街と生産を支えます。', lumbermill:'森から木材を生産。建築の選択肢を広げます。',
  quarry:'石材を切り出し、工房や街の発展を支えます。', mine:'鉱石を採掘。道具を作る産業の起点です。',
  workshop:'木材と鉱石を道具に。生産を次の段階へ。', watchtower:'見渡せる範囲を広げ、未知の土地を発見します。',
  archive:'資源を知識に変え、新しい技術へつなぎます。', townhall:'みんなの物資が集まる、文明の出発点。', warehouse:'備蓄できる容量を増やし、満杯で待つ物流を受け入れます。'
};
const terrains = {
  grass:{name:'草原',en:'GRASSLAND',description:'穏やかな平原。食料を育てるか、産業の拠点を置くか。街の未来を選べる土地です。'},
  forest:{name:'森',en:'WOODLAND',description:'木々が生い茂る資源地。手で木材を運ぶことも、製材所で継続的に生産することもできます。'},
  hill:{name:'丘陵',en:'HIGHLANDS',description:'石材を産する高地。歩くには時間がかかりますが、建築を支える大切な場所です。'},
  mountain:{name:'山岳',en:'MOUNTAINS',description:'険しい岩肌に鉱脈が眠っています。移動には平原の2.5倍の時間が必要。隣から鉱石を採集し、冶金の研究後は鉱山を建てられます。'},
  water:{name:'水域',en:'WATERS',description:'水面が道を隔てています。陸路を探して回り込みましょう。'}
};
const techs = TECHNOLOGIES;
const lensDescriptions = {
  normal:'地形を選択して、文明の次の一手を。', food:'食料を生む土地と、農場の稼働状況。',
  industry:'資源の分布と、生産施設のつながり。', logistics:'道路・街との接続・運搬中の物資。', danger:'水域・山岳と、移動を制約する地形。'
};

function number(value, digits = 0) { return Number(value || 0).toLocaleString('ja-JP', { maximumFractionDigits:digits }); }
function seconds(ms) { return `${Math.max(1, Math.ceil((ms || 0) / 1000))}秒`; }
function setHtml(id, html) { const el = $(id); if (el.innerHTML !== html) el.innerHTML = html; }
function tileName(tile) { if (!tile?.explored) return '未踏の地'; const b = world?.buildings?.[tile.buildingId]; return b ? buildingNames[b.type] || b.type : terrains[tile.terrain]?.name || '土地'; }
function citizen() { return world?.players?.[identity.actorId]; }
function playerTile() { const p = citizen(); return p ? world.tiles.find((t) => t.q === p.q && t.r === p.r) : null; }
function nearTile(tile) { const p = citizen(); return p && hexDistance(p, tile) <= 1; }
function isBusy() { const p = citizen(); return Boolean(p?.job || p?.cargo || p?.path?.length); }
function costMarkup(cost = {}) { return Object.entries(cost).filter(([,v]) => v > 0).map(([key,value]) => `<span class="cost ${(world?.stock?.[key] || 0) < value ? 'negative' : ''}" title="${resourceNames[key] || key}"><span style="color:${resourceColors[key] || 'inherit'}">${icons[key] || '◇'}</span> ${number(value)}</span>`).join(''); }
function notify(message, error = false) {
  const item = document.createElement('div'); item.className = `toast${error ? ' error' : ''}`; item.textContent = message;
  $('notifications').append(item);
  while ($('notifications').children.length > 3) $('notifications').firstElementChild.remove();
  setTimeout(() => item.remove(), error ? 6500 : 4500);
}
function setConnection(online) {
  $('connection').textContent = online ? '世界と同期中' : '再接続中';
  $('connection').classList.toggle('offline', !online);
}
async function api(path, body) {
  const response = await fetch(path, { method: body ? 'POST' : 'GET', headers: body ? { 'Content-Type':'application/json' } : {}, body:body ? JSON.stringify(body) : undefined, signal:AbortSignal.timeout(8000), cache:'no-store' });
  let result; try { result = await response.json(); } catch { throw new Error('サーバーからゲームの状態を取得できませんでした。'); }
  if (!response.ok) throw Object.assign(new Error(result.error || '操作を受け付けられませんでした。'), {status:response.status});
  return result;
}
function receive(snapshot) {
  const next = snapshot.world;
  if (!next) throw new Error('世界データがありません。');
  if (world && (next.revision < world.revision || (next.revision === world.revision && next.timeMs < world.timeMs))) return;
  world = next;
  map.setState(world, identity.actorId);
  setConnection(true);
  const events = world.events || [];
  if (lastEventId !== null) {
    const lastIndex = events.findIndex((event) => event.id === lastEventId);
    if (lastIndex >= 0) {
      const important = events.slice(lastIndex + 1).filter((event) => /complete|discover|research|built|finished/i.test(event.type));
      important.slice(-1).forEach((event) => notify(chronicleText(event)));
    }
  }
  lastEventId = events.at(-1)?.id ?? lastEventId;
  render();
}
async function join() {
  if (reconnecting) return;
  reconnecting = true;
  clearTimeout(pollTimer);
  $('retry').hidden = true;
  $('loading-message').textContent = '文明の現在を読み込んでいます…';
  try {
    const result = await api('/api/civilization/join', { session, actorId:identity.actorId, name:identity.name, ...(identity.token ? {token:identity.token} : {}) });
    identity.token = result.token; sessionStorage.setItem(identityKey, JSON.stringify(identity));
    receive(result); joined = true;
    $('loading').hidden = true;
    if (!sessionStorage.getItem('permutation.civilization.welcome.v2')) {
      notify('ようこそ、私たちの文明へ。土地を選ぶと、できることが見えます。');
      sessionStorage.setItem('permutation.civilization.welcome.v2', '1');
    }
    schedulePoll();
  } catch (error) {
    setConnection(false);
    $('loading-message').textContent = `接続できませんでした。${error.message} 保存された世界を上書きせず、再接続を待っています。`;
    $('retry').hidden = false;
  } finally { reconnecting = false; }
}
function schedulePoll() { clearTimeout(pollTimer); pollTimer = setTimeout(poll, document.hidden ? 1800 : 650); }
async function poll() {
  try { receive(await api(`/api/civilization/state?session=${encodeURIComponent(session)}`)); }
  catch { setConnection(false); }
  finally { if (joined) schedulePoll(); }
}
async function act(action) {
  if (pending || !joined) return;
  pending = true; renderInspector(); renderDrawer();
  try {
    receive(await api('/api/civilization/action', { session, actorId:identity.actorId, token:identity.token, action }));
    const messages = { MOVE:'移動を始めました。', EXPLORE:'探索を始めました。', BUILD:`${buildingNames[action.buildingType] || '施設'}の建設を始めました。`, ROAD:'道路の整備を始めました。', GATHER:'資源の採集と運搬を始めました。', RESEARCH:'研究を始めました。', CREATE_PROJECT:'共同計画を掲示し、材料を予約しました。', CLAIM_PROJECT:'この計画を担当します。現地へ向かいましょう。', RELEASE_PROJECT:'担当を空けました。他の市民が引き継げます。', CANCEL_PROJECT:'計画を取り消し、予約した資源を戻しました。', START_PROJECT:'予約した材料で共同建設を始めました。', SET_PRODUCTION:'生産の方針を変更しました。' };
    notify(messages[action.type] || '世界に反映されました。');
  } catch (error) { notify(error.message, true); }
  finally { pending = false; renderInspector(); renderDrawer(); }
}

const map = new CivilizationMap($('world-map'), {
  onSelect: selectTile,
  onHover(id) {
    const tile = world && tileById(world, id);
    $('hover-label').hidden = !tile;
    if (tile) $('hover-label').textContent = `${tileName(tile)}  ·  ${tile.q}, ${tile.r}`;
  },
  onMove(id) { if (world && tileById(world, id)?.explored) act({type:'MOVE',tileId:id}); }
});
function selectTile(id, focus = false) {
  if (!world || !tileById(world, id)) return;
  selection = id; map.setSelection(id); renderInspector();
  if (focus) map.focusTile(id);
  if (window.innerWidth < 650) { drawer = null; renderDrawer(); }
}
function render() {
  renderResources(); renderAmbitions(); renderInspector(); renderDrawer(); renderCitizen(); renderTasks();
  const elapsedHours = 6 + (world.timeMs || 0) / 300000 * 24;
  const day = Math.floor(elapsedHours / 24) + 1;
  const hour = elapsedHours % 24;
  $('world-time').textContent = `開拓の時代 · DAY ${String(day).padStart(2,'0')} · ${String(Math.floor(hour % 24)).padStart(2,'0')}:${String(Math.floor(hour % 1 * 60)).padStart(2,'0')}`;
  if (performance.now() - lastMiniDraw > 1000) { renderMinimap(); lastMiniDraw = performance.now(); }
}
function renderResources() {
  setHtml('resources', resourceKeys.map((key) => {
    const rate = Number(world.rates?.[key] || 0);
    return `<button class="resource" data-open-economy="${key}" style="--resource-color:${resourceColors[key]}" aria-label="${resourceNames[key]} ${number(world.stock[key])}、毎分${number(rate,1)}"><span class="resource-icon">${icons[key]}</span><span class="resource-amount">${number(world.stock[key])}<span class="resource-name">${resourceNames[key]}</span></span><span class="resource-flow ${rate < 0 ? 'negative' : ''}">${rate >= 0 ? '+' : ''}${number(rate,1)} / 分</span><span class="resource-tip">${resourceNames[key]} · 今使える共有備蓄<br>${esc(stockDetail(world,key))}<br>運搬完了後に備蓄へ届きます。<br>増減は現在の稼働条件での見込みです。</span></button>`;
  }).join(''));
}
function renderAmbitions() {
  const objectives = world.season?.objectives || [];
  $('ambition-count').textContent = `${objectives.filter((o) => o.complete || o.current >= o.target).length} / ${objectives.length}`;
  setHtml('ambitions', `<div class="mini-label" style="margin-bottom:12px">${esc(world.season?.name || '開拓の季節')} · 達成後も世界は続きます</div>${objectives.map((o) => `<div class="objective"><div class="objective-label"><span>${esc(o.label)}</span><span>${number(o.current)} / ${number(o.target)}</span></div><div class="progress-track"><i style="width:${Math.min(100,Math.max(0,o.current / o.target * 100))}%"></i></div></div>`).join('')}`);
}
function renderCitizen() {
  const p = citizen(); if (!p) return;
  $('citizen-name').textContent = `${p.name} · 文明の市民`;
  document.querySelector('.citizen-avatar').textContent = p.name.slice(0,1);
  const cargo = p.cargo ? `${resourceNames[p.cargo.resource] || p.cargo.resource} ${number(p.cargo.amount)}` : '';
  const status = p.job ? ({build:'建設中',gather:'採集中',road:'道路を整備中',explore:'探索中'}[p.job.type] || '作業中') : p.path?.length ? '移動中' : '自由に行動できます';
  const remaining = p.job ? Math.max(0,p.job.durationMs-p.job.elapsedMs) : null;
  $('citizen-status').textContent = cargo ? `${cargo} を運搬中${p.path?.length ? '' : ' · 倉庫の受入待ち'}` : status + (remaining ? ` · 残り${seconds(remaining)}` : '');
  $('citizen-count').textContent = `${Object.keys(world.players).length} 人の市民`;
}
function preview(tileId, type) {
  try { return getBuildPreview(world, identity.actorId, tileId, type); }
  catch (error) { return {allowed:false,reason:error.message,cost:BUILDINGS[type]?.cost || {},durationMs:0}; }
}
function renderInspector() {
  const el = $('inspector');
  const tile = world && tileById(world, selection);
  el.hidden = !tile;
  if (!tile) return;
  const b = world.buildings[tile.buildingId];
  const terrain = terrains[tile.terrain] || terrains.grass;
  const nearby = nearTile(tile);
  const busy = isBusy();
  const isUnknown = !tile.explored;
  const head = `<div class="panel-header" style="margin-bottom:5px"><div class="selection-eyebrow">${isUnknown ? 'BEYOND THE KNOWN WORLD' : b ? 'CIVILIZATION · BUILDING' : terrain.en} · ${tile.q}, ${tile.r}</div><button class="panel-close" data-close-selection aria-label="選択を閉じる">×</button></div><h2>${esc(tileName(tile))}</h2>`;
  if (isUnknown) {
    const adjacentKnown = world.tiles.some((t) => t.explored && hexDistance(t, tile) === 1);
    setHtml('inspector', `${head}<p class="selection-description">まだ誰も足を踏み入れていない土地。探索して地形と資源を明らかにすると、文明全体の可能性が広がります。</p><div class="explanation">探索は市民が境界まで移動して行います。世界の端から遠隔で地図を開くことはできません。</div><button class="primary-button wide" data-action="EXPLORE" data-tile="${esc(tile.id)}" ${pending || busy || !nearby || !adjacentKnown ? 'disabled' : ''}>この土地を探索 <span>↗</span></button>${!nearby ? '<p class="selection-description">まず隣の既知の土地を選び、現地へ移動してください。</p>' : ''}${busy ? '<p class="selection-description">現在の作業・移動が終わると探索できます。</p>' : ''}`);
    return;
  }
  let body = `${head}<p class="selection-description">${esc(b ? buildingEffects[b.type] || '文明を支える施設です。' : terrain.description)}</p><div class="selection-meta"><span class="tag">${terrain.name}</span>${tile.road ? '<span class="tag positive">道路あり</span>' : ''}${tile.resource ? `<span class="tag">${icons[tile.resource] || ''} ${resourceNames[tile.resource] || tile.resource}</span>` : ''}<span class="tag ${nearby ? 'positive' : ''}">${nearby ? '活動できる距離' : '移動が必要'}</span></div>`;
  body += tileValueMarkup(tile) + journeyMarkup(world, identity.actorId, tile.id) + tileProjectMarkup(world, identity.actorId, tile.id, pending);
  if (b) {
    const status = b.status === 'building' ? '建設中' : b.status === 'blocked' ? '生産待ち' : '稼働中';
    body += `<div class="production-detail"><div class="stat-box"><small>施設の状態</small><strong>${status}</strong></div><div class="stat-box"><small>街への物流</small><strong>${b.connected ? '接続済み' : '未接続'}</strong></div></div>`;
    if (b.status === 'building') body += `<div class="objective-label"><span>建設の進捗 · 残り${seconds((1-(b.progress||0))*(BUILDINGS[b.type]?.durationMs||0))}</span><span>${Math.floor((b.progress || 0)*100)}%</span></div><div class="progress-track"><i style="width:${(b.progress || 0)*100}%"></i></div>`;
    if (b.reason) body += `<div class="explanation">${esc(b.reason)}</div>`;
    const goods = Object.entries(b.localStock || {}).filter(([,v]) => v > 0);
    if (goods.length) body += `<div class="section-label">現地の出荷待ち</div><div class="costs" style="margin:0">${costMarkup(Object.fromEntries(goods))}</div><p class="selection-description">物資は運び手が広場に届けると、共有備蓄に加わります。</p>`;
    if (b.ownerId) body += `<p class="mini-label">建設者：${esc(world.players[b.ownerId]?.name || '文明の市民')}</p>`;
    const moving = world.caravans.filter((c) => c.fromTileId === tile.id);
    if (moving.length) body += `<div class="section-label">運搬中</div>${moving.map((c) => `<p class="selection-description">${icons[c.resource] || '◇'} ${resourceNames[c.resource] || c.resource} ${number(c.amount)} → 広場 <span>${Math.floor(c.progress*100)}%</span></p>`).join('')}`;
    body += managementMarkup(world, identity.actorId, b, pending);
  }
  const canWalk = tile.terrain !== 'water';
  body += `<div class="action-row">${canWalk ? `<button class="secondary-button" data-action="MOVE" data-tile="${esc(tile.id)}" ${pending || busy || (playerTile()?.id === tile.id) ? 'disabled' : ''}>⌖ ここへ移動</button>` : ''}${(tile.resource || tile.terrain === 'grass') && !b ? `<button class="secondary-button" data-action="GATHER" data-tile="${esc(tile.id)}" ${pending || busy || !nearby ? 'disabled' : ''}>${icons[tile.resource || 'food']} 採集して運ぶ</button>` : ''}</div>`;
  if (canWalk && world.tiles.some((candidate) => !candidate.explored && hexDistance(tile, candidate) === 1)) {
    body += `<button class="primary-button wide" style="margin:10px 0" data-action="EXPLORE" data-tile="${esc(tile.id)}" ${pending || busy || !nearby ? 'disabled' : ''}>周辺の未知の土地を探索 <span>5秒 ↗</span></button>`;
  }
  if (!b && canWalk) {
    if (!tile.road) {
      const road = getRoadPreview(world, identity.actorId, tile.id);
      body += `<button class="secondary-button" style="width:100%" data-review-road="${esc(tile.id)}" ${pending || !road.allowed ? 'disabled' : ''}>⇄ 道路を整備する · ${seconds(road.durationMs)}</button><div class="costs" style="margin:8px 0">${costMarkup(road.cost)}</div>${!road.allowed ? `<p class="mini-label">${esc(road.reason)}</p>` : ''}`;
    }
    body += '<div class="section-label">この場所に建てる</div>';
    const types = Object.keys(buildingNames).filter((type) => type !== 'townhall' && BUILDINGS[type]);
    const relevant = types.filter((type) => {
      if (type === 'farm') return tile.terrain === 'grass';
      if (type === 'lumbermill') return tile.terrain === 'forest';
      if (type === 'quarry') return tile.terrain === 'hill';
      if (type === 'mine') return tile.terrain === 'mountain';
      return ['grass','hill'].includes(tile.terrain);
    });
    body += relevant.map((type) => {
      const info = preview(tile.id, type);
      return `<div class="investment-choice"><button class="build-option" data-review-build="${type}" data-tile="${esc(tile.id)}" ${!info.allowed || pending ? 'disabled' : ''}><span class="build-option-heading"><span class="building-icon">${buildingIcons[type] || '⌂'}</span><strong>${buildingNames[type]}</strong><time>${seconds(info.durationMs)}</time></span><p>${esc(info.effect || buildingEffects[type])}</p><span class="costs">${costMarkup(info.cost)}</span>${info.allowed ? '' : `<span class="locked-reason">${esc(info.reason || '今は建設できません')}</span>`}</button>${projectOfferMarkup(world,identity.actorId,tile.id,type,pending)}</div>`;
    }).join('');
    if (!relevant.length) body += '<p class="empty-copy">この地形に建設できる施設はありません。</p>';
  } else if (!b && tile.terrain === 'mountain' && tile.resource === 'ore') {
    const info = preview(tile.id, 'mine');
    body += `<button class="build-option" data-review-build="mine" data-tile="${esc(tile.id)}" ${!info.allowed || pending ? 'disabled' : ''}><span class="build-option-heading"><span>⬡</span><strong>鉱山</strong><time>${seconds(info.durationMs)}</time></span><p>${buildingEffects.mine}</p><span class="costs">${costMarkup(info.cost)}</span>${info.allowed ? '' : `<span class="locked-reason">${esc(info.reason)}</span>`}</button>`;
  }
  if (b?.type === 'townhall') body += `<button class="secondary-button" style="width:100%" data-open-drawer="economy">文明の共有備蓄を見る →</button><p class="selection-description">全市民の活動がここにつながっています。研究と建設のための資源も、文明全体で共有します。</p>`;
  setHtml('inspector', body);
}
function renderTasks() {
  const previousId = tasks[taskIndex]?.id;
  try { tasks = getCitizenTasks(world, identity.actorId) || []; } catch { tasks = []; }
  const retainedIndex = tasks.findIndex((task) => task.id === previousId);
  taskIndex = retainedIndex >= 0 ? retainedIndex : tasks.length ? taskIndex % tasks.length : 0;
  const task = tasks[taskIndex];
  $('next-title').textContent = task?.title || '街の外へ目を向ける';
  $('next-detail').textContent = task?.detail || '資源と地形から、次に育てるものを選ぶ。';
  $('skip-task').disabled = tasks.length < 2;
  $('next-button').title = task ? `${taskIndex + 1} / ${tasks.length} 件の提案。クリックで場所を確認します。` : '自分の場所へ';
}
function focusTask() { const task = tasks[taskIndex]; if (task?.tileId) selectTile(task.tileId, true); else map.focusPlayer(); }
function renderDrawer() {
  $('drawer').hidden = !drawer || !world;
  document.querySelectorAll('[data-drawer]').forEach((b) => { b.classList.toggle('active', b.dataset.drawer === drawer); b.setAttribute('aria-expanded', String(b.dataset.drawer === drawer)); });
  if (!drawer || !world) return;
  const names = { settlement:'街と生産', economy:'共有経済', research:'知識と技術', chronicle:'私たちの文明史', projects:'共同計画' };
  let html = `<div class="panel-header"><h2>${names[drawer]}</h2><button class="panel-close" data-close-drawer aria-label="閉じる">×</button></div>`;
  if (drawer === 'settlement') {
    html += '<p class="drawer-intro">施設を選ぶと現地へ。材料と物流の状態が、生産を決めます。</p>';
    html += Object.values(world.buildings).map((b) => `<button class="building-row" data-focus-tile="${esc(b.tileId)}"><span>${buildingIcons[b.type] || '⌂'}</span><span><strong>${esc(buildingNames[b.type] || b.type)}</strong><small>${b.status === 'building' ? `建設 ${Math.floor(b.progress*100)}%` : b.reason || '稼働中'} · ${esc(b.tileId)}</small></span><span class="status-dot ${b.status === 'blocked' ? 'blocked' : ''}"></span></button>`).join('');
    html += '<div class="section-label">街で働く人々</div>';
    html += (world.npcs || []).map((npc) => `<div class="building-row"><span>♙</span><span><strong>${esc(npc.name)}</strong><small>${esc(npc.status || npc.role)}</small></span></div>`).join('');
  } else if (drawer === 'economy') {
    html += '<p class="drawer-intro">すべての市民で共有する備蓄です。各施設の出荷待ち・運搬中の物資は、到着するまで含まれません。</p><div class="economy-row mini-label"><span>資源</span><span>備蓄</span><span>見込み / 分</span></div>';
    html += resourceKeys.map((key) => `<div class="economy-row"><span style="color:${resourceColors[key]}">${icons[key]} ${resourceNames[key]}</span><strong>${number(world.stock[key],1)}</strong><span class="${world.rates[key] < 0 ? 'negative' : ''}">${world.rates[key] >= 0 ? '+' : ''}${number(world.rates[key],1)}</span></div><p class="capacity-note">${esc(stockDetail(world,key))}</p>`).join('');
    html += `<div class="explanation">${world.caravans.length} 件の運搬が進行中。食料は街と働く人々に消費されます。生産施設を増やすと、その材料も必要になります。</div><button class="secondary-button" data-select-lens="logistics" style="width:100%">物流を地図で見る →</button>`;
    if (world.economy) html += '<p class="drawer-intro" style="margin-top:14px">容量がいっぱいなら、施設を休止する、共有資源を投資する、倉庫を増築する、という選択があります。予約材料も容量に含まれ、旧版の超過備蓄は消えません。</p>';
  } else if (drawer === 'research') {
    html += '<p class="drawer-intro">学術院が生む知識を、新しい生産や移動の可能性へ。研究には学術院の近くへ移動します。成果は文明全体で共有します。</p>';
    html += Object.entries(techs).map(([id,tech]) => {
      const unlocked = world.research.unlocked.includes(id); const active = world.research.active === id;
      return `<div class="tech-card"><h3>✧ ${tech.name}</h3><p>${tech.effect}</p><div class="costs" style="margin:0 0 10px">${costMarkup(tech.cost)}<span>${seconds(tech.durationMs)}</span></div>${active ? `<div class="objective-label"><span>研究中</span><span>${Math.floor(world.research.progress*100)}%</span></div><div class="progress-track"><i style="width:${world.research.progress*100}%"></i></div>` : `<button data-review-research="${id}" ${unlocked || world.research.active || pending ? 'disabled' : ''}>${unlocked ? '✓ 文明で共有済み' : '研究条件を確認 →'}</button>`}</div>`;
    }).join('');
  } else if (drawer === 'projects') {
    html += projectsMarkup(world,identity.actorId,pending);
  } else if (drawer === 'chronicle') {
    html += '<p class="drawer-intro">台本ではなく、市民の行動と世界の変化の記録。</p>';
    html += [...world.events].reverse().slice(0,35).map((event) => `<div class="event"><small>DAY ${Math.floor(event.timeMs / 300000)+1} · ${Math.floor(event.timeMs / 1000)}s</small><p>${esc(chronicleText(event))}</p>${event.tileId ? `<button data-focus-tile="${esc(event.tileId)}">現地を見る ↗</button>` : ''}</div>`).join('');
  }
  setHtml('drawer', html);
}
function renderMinimap() {
  const canvas = $('minimap'); const ctx = canvas.getContext('2d');
  ctx.clearRect(0,0,170,104); ctx.fillStyle = '#dfe3d2'; ctx.fillRect(0,0,170,104);
  const colors = {grass:'#b4c48b',forest:'#668368',hill:'#b3a586',mountain:'#838c84',water:'#96b9b4'};
  const size = 4.3;
  for (const t of world.tiles) {
    const x = 85 + (t.q + t.r / 2)*size*1.7, y = 52 + t.r*size*1.38;
    ctx.fillStyle = t.explored ? colors[t.terrain] || colors.grass : '#c4ccbb';
    ctx.beginPath(); for(let i=0;i<6;i++){const a=(i*60+30)*Math.PI/180;ctx.lineTo(x+Math.cos(a)*size,y+Math.sin(a)*size*.83);}ctx.closePath();ctx.fill();
    if(t.explored && t.buildingId){ctx.fillStyle='#b0744e';ctx.fillRect(x-1.5,y-1.5,3,3);}
  }
  for(const p of Object.values(world.players)){ctx.fillStyle=p.id===identity.actorId?'#faf3d9':'#3a7466';ctx.beginPath();ctx.arc(85+(p.q+p.r/2)*size*1.7,52+p.r*size*1.38,2.4,0,Math.PI*2);ctx.fill();}
  $('map-extent').textContent = `${world.tiles.filter(t=>t.explored).length} / ${world.tiles.length} 地形を発見`;
}
function setLens(value) {
  lens = value; map.setLens(value);
  document.querySelectorAll('[data-lens]').forEach((button) => { button.classList.toggle('active',button.dataset.lens===value); button.setAttribute('aria-pressed',String(button.dataset.lens===value)); });
  $('lens-description').textContent = lensDescriptions[value];
}
function openDrawer(value) { drawer = drawer === value ? null : value; renderDrawer(); if (innerWidth < 650 && drawer) { selection=null; map.setSelection(null); renderInspector(); } }
function confirmBuild(type, tileId) {
  const info = preview(tileId,type); if(!info.allowed){notify(info.reason,true);return;}
  confirmation = {type:'BUILD',tileId,buildingType:type};
  setHtml('confirm-content', `<div class="eyebrow">A SHARED INVESTMENT</div><h2>${buildingNames[type]}を建てる</h2><p>${esc(info.effect || buildingEffects[type])}</p><div class="costs">${costMarkup(info.cost)}</div><p>建設時間：${seconds(info.durationMs)}<br>場所：${esc(tileId)} · ${tileName(tileById(world,tileId))}</p><div class="confirm-disclosure">文明の共有備蓄から材料を使います。この施設は、他の市民も利用できる文明の資産になります。</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action>建設を始める</button></div>`);
  $('confirm-dialog').showModal();
}
function confirmRoad(tileId) {
  confirmation = {type:'ROAD',tileId};
  const info = getRoadPreview(world, identity.actorId, tileId);
  setHtml('confirm-content', `<div class="eyebrow">CONNECT THE CIVILIZATION</div><h2>道をつなぐ</h2><p>${esc(info.effect)} · ${seconds(info.durationMs)}</p><div class="costs">${costMarkup(info.cost)}</div>${!info.allowed ? `<div class="explanation">${esc(info.reason)}</div>` : ''}<div class="confirm-disclosure">文明の共有資源を使います。道路は共同倉庫につながる道の隣から延ばします。</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action ${!info.allowed ? 'disabled' : ''}>道路を整備</button></div>`);
  $('confirm-dialog').showModal();
}
function confirmResearch(techId) {
  confirmation = {type:'RESEARCH',techId};
  const info = getResearchPreview(world, identity.actorId, techId);
  setHtml('confirm-content', `<div class="eyebrow">CIVILIZATION RESEARCH</div><h2>${techs[techId].name}</h2><p>${techs[techId].effect} · ${seconds(info.durationMs)}</p><div class="costs">${costMarkup(info.cost)}</div>${!info.allowed ? `<div class="explanation">${esc(info.reason)}</div>` : ''}<div class="confirm-disclosure">研究は学術院の近くで開始します。文明の共有資源を使い、成果は全市民で共有します。</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action ${!info.allowed ? 'disabled' : ''}>研究を開始</button></div>`);
  $('confirm-dialog').showModal();
}
function confirmProject(buildingType,tileId) {
  const info = Strategy.getProjectPreview(world,identity.actorId,tileId,buildingType);
  confirmation = {type:'CREATE_PROJECT',tileId,buildingType};
  setHtml('confirm-content', `<div class="eyebrow">A PLAN FOR EVERYONE</div><h2>${esc(buildingNames[buildingType])}の共同計画</h2><p>${esc(info.effect || buildingEffects[buildingType])}</p><div class="costs">${costMarkup(info.cost)}</div><p>場所：${esc(tileId)} · 建設${seconds(info.durationMs)}</p>${!info.allowed ? `<div class="explanation">${esc(info.reason)}</div>` : ''}<div class="confirm-disclosure">材料を共有備蓄から予約し、他の用途では使えなくします。提案しただけでは建設しません。担当する市民が現地で着工します。着工前なら取り消して材料を戻せます。</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action ${info.allowed ? '' : 'disabled'}>提案・資源を予約</button></div>`);
  $('confirm-dialog').showModal();
}
function confirmMode(buildingId,mode) {
  const info = Strategy.getBuildingManagementPreview(world,identity.actorId,buildingId,mode);
  confirmation = {type:'SET_PRODUCTION',buildingId,mode};
  const label = {normal:'通常生産へ戻す',paused:'生産を休止する',boost:'道具を使って増産する'}[mode];
  setHtml('confirm-content', `<div class="eyebrow">PRODUCTION POLICY</div><h2>${label}</h2><p>${esc(info.effect || (mode === 'boost' ? '次の生産から道具を1つ使い、1回あたりの収量を2倍にします。' : mode === 'paused' ? '生産を一時停止します。すでに投入した材料と生産の進捗は保持します。' : '新しい生産は道具を追加消費せず、通常の収量に戻ります。'))}</p>${!info.allowed ? `<div class="explanation">${esc(info.reason)}</div>` : ''}<div class="confirm-disclosure">全員が使う施設の方針を変更します。道具の配送待ちや倉庫満杯で、生産が待機することがあります。</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action ${info.allowed ? '' : 'disabled'}>方針を変更</button></div>`);
  $('confirm-dialog').showModal();
}
function projectAction(type,projectId) {
  const project = world.projects?.[projectId]; if(!project)return;
  if(type === 'FOCUS') { selectTile(project.tileId,true); return; }
  if(type !== 'CANCEL_PROJECT') { act({type,projectId}); return; }
  confirmation = {type,projectId};
  setHtml('confirm-content', `<div class="eyebrow">RETURN RESERVED MATERIALS</div><h2>共同計画を取り消す</h2><p>${esc(buildingNames[project.buildingType])} · ${esc(project.tileId)}<br>着工前の予約材料を共有備蓄に戻します。</p><div class="costs">${costMarkup(project.reserved || {})}</div><div class="action-row"><button class="secondary-button" data-close-dialog="confirm-dialog">戻る</button><button class="primary-button" data-confirm-action>取り消して材料を戻す</button></div>`);
  $('confirm-dialog').showModal();
}
document.addEventListener('click', (event) => {
  const button = event.target.closest('button'); if(!button || button.disabled) return;
  if(button.dataset.closeDialog) $(button.dataset.closeDialog).close();
  else if(button.hasAttribute('data-confirm-action')) { const action=confirmation; $('confirm-dialog').close(); confirmation=null; if(action)act(action); }
  else if(button.dataset.drawer) openDrawer(button.dataset.drawer);
  else if(button.dataset.openDrawer) { drawer=button.dataset.openDrawer; renderDrawer(); }
  else if(button.hasAttribute('data-close-drawer')) {drawer=null;renderDrawer();}
  else if(button.hasAttribute('data-close-selection')) {selection=null;map.setSelection(null);renderInspector();}
  else if(button.dataset.lens) setLens(button.dataset.lens);
  else if(button.dataset.selectLens) setLens(button.dataset.selectLens);
  else if(button.dataset.openEconomy) {drawer='economy';renderDrawer();}
  else if(button.dataset.focusTile) selectTile(button.dataset.focusTile,true);
  else if(button.dataset.action) act({type:button.dataset.action,tileId:button.dataset.tile});
  else if(button.dataset.reviewBuild) confirmBuild(button.dataset.reviewBuild,button.dataset.tile);
  else if(button.dataset.reviewRoad) confirmRoad(button.dataset.reviewRoad);
  else if(button.dataset.reviewResearch) confirmResearch(button.dataset.reviewResearch);
  else if(button.dataset.reviewProject) confirmProject(button.dataset.reviewProject,button.dataset.tile);
  else if(button.dataset.reviewMode) confirmMode(button.dataset.building,button.dataset.reviewMode);
  else if(button.dataset.projectAction) projectAction(button.dataset.projectAction,button.dataset.projectId);
});
$('ambition-toggle').addEventListener('click', () => { $('ambitions').hidden=!$('ambitions').hidden; $('ambition-toggle').setAttribute('aria-expanded',String(!$('ambitions').hidden)); });
$('next-button').addEventListener('click',focusTask);
$('skip-task').addEventListener('click',()=>{taskIndex++;renderTasks();});
$('home-button').addEventListener('click',()=>map.focusPlayer());
$('citizen-card').addEventListener('click',()=>map.focusPlayer());
$('zoom-in').addEventListener('click',()=>map.zoomBy(1.2));
$('zoom-out').addEventListener('click',()=>map.zoomBy(1/1.2));
$('help-button').addEventListener('click',()=>$('help-dialog').showModal());
$('retry').addEventListener('click',join);
$('minimap').addEventListener('click',(event)=>{
  if(!world)return;
  const rect=$('minimap').getBoundingClientRect();
  const x=(event.clientX-rect.left)/rect.width*170,y=(event.clientY-rect.top)/rect.height*104;
  let nearest=null,dist=Infinity;
  for(const t of world.tiles){const tx=85+(t.q+t.r/2)*4.3*1.7,ty=52+t.r*4.3*1.38;const d=Math.hypot(tx-x,ty-y);if(d<dist){dist=d;nearest=t;}}
  if(nearest)selectTile(nearest.id,true);
});
document.addEventListener('keydown',(event)=>{
  if(event.target.matches('input,textarea,select') || $('help-dialog').open || $('confirm-dialog').open)return;
  if(event.key.toLowerCase()==='f'){event.preventDefault();map.focusPlayer();}
  if(event.key.toLowerCase()==='n'){event.preventDefault();focusTask();}
  if(event.key==='Escape'){selection=null;drawer=null;map.setSelection(null);renderInspector();renderDrawer();}
});
document.addEventListener('visibilitychange',()=>{if(!document.hidden&&joined){clearTimeout(pollTimer);poll();}});
window.addEventListener('pagehide',(event)=>{clearTimeout(pollTimer);if(!event.persisted)map.destroy();});
window.addEventListener('pageshow',(event)=>{if(event.persisted&&joined)poll();});
join();
