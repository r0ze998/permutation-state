import * as Rules from './core.mjs';

export const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmt = n => Number(n || 0).toLocaleString('ja-JP', {maximumFractionDigits:1});
const duration = ms => `${Math.max(1, Math.ceil((ms || 0) / 1000))}秒`;
const activeProjects = world => Object.values(world.projects || {}).filter(p => ['open','claimed','building'].includes(p.status));
const modeLabels = {normal:'通常',paused:'休止',boost:'道具で増産'};
const statusLabels = {open:'担当募集中',claimed:'担当決定',building:'建設中',complete:'完成',cancelled:'取り消し済み'};
const disabled = (value) => value ? ' disabled' : '';

export function stockDetail(world, key) {
  if (!world.economy) return '';
  const reserved = world.economy.reserved?.[key] || 0;
  const capacity = world.economy.capacity?.[key] || 0;
  const used = (world.stock[key] || 0) + reserved;
  return `容量 ${fmt(used)} / ${fmt(capacity)} · 計画用 ${fmt(reserved)}${used > capacity ? '（旧備蓄を保持・新規受入待ち）' : ''}`;
}

export function journeyMarkup(world, actorId, tileId) {
  if (!Rules.getMoveEta) return '';
  const result = Rules.getMoveEta(world, actorId, tileId);
  if (!result.allowed || !result.durationMs) return '';
  return `<div class="journey-note">⌖ 現地まで 約${duration(result.durationMs)} <span>地形と道路を考慮</span></div>`;
}

export function tileValueMarkup(tile) {
  if (!tile?.explored || !tile.siteYield || tile.terrain === 'water') return '';
  return `<div class="site-value"><span>${escapeHtml(tile.siteLabel || '標準の産地')}</span><strong>産地収量 ×${fmt(tile.siteYield)}</strong><small>遠い産地ほど、道路と運搬時間も見て選びましょう。</small></div>`;
}

export function managementMarkup(world, actorId, building, pending = false) {
  if (!world.economy || !Rules.getBuildingManagementPreview || building.status === 'building' || !Rules.BUILDINGS[building.type]?.cycleMs) return '';
  return `<div class="section-label">生産の方針</div><div class="production-modes">${Object.entries(modeLabels).map(([mode,label]) => {
    const info = Rules.getBuildingManagementPreview(world, actorId, building.id, mode);
    return `<button class="mode-button ${building.mode === mode ? 'active' : ''}" data-review-mode="${mode}" data-building="${escapeHtml(building.id)}" aria-pressed="${(building.mode || 'normal') === mode}" title="${escapeHtml(info.reason || '')}"${disabled(pending || !info.allowed || (building.mode || 'normal') === mode)}>${label}</button>`;
  }).join('')}</div><p class="mini-label">増産は1回の生産に道具1を運び込み、収量を2倍に。進行中の材料は変更しても失われません。切替は現地で行います。</p>`;
}

export function projectOfferMarkup(world, actorId, tileId, buildingType, pending = false) {
  if (!world.economy || !Rules.getProjectPreview) return '';
  const info = Rules.getProjectPreview(world, actorId, tileId, buildingType);
  return `<button class="project-offer" data-review-project="${escapeHtml(buildingType)}" data-tile="${escapeHtml(tileId)}" title="${escapeHtml(info.reason || '')}"${disabled(pending || !info.allowed)}>＋ 共同計画として提案${info.allowed ? '・資源を予約' : ''}</button>`;
}

export function projectCard(world, actorId, project, pending = false) {
  const p = project;
  const name = Rules.BUILDINGS[p.buildingType]?.name || p.buildingType;
  const claimant = p.assigneeId ? world.players[p.assigneeId]?.name || '市民' : null;
  const mine = p.assigneeId === actorId;
  const prebuild = ['open','claimed'].includes(p.status);
  const player = world.players[actorId];
  const tile = Rules.tileById(world,p.tileId);
  const near = Boolean(player && tile && Rules.hexDistance(player,tile)<=1);
  const busy = Boolean(player?.job || player?.cargo || player?.path?.length);
  const button = (action,label,unavailable=false) => `<button class="secondary-button" data-project-action="${action}" data-project-id="${escapeHtml(p.id)}"${disabled(pending || unavailable)}>${label}</button>`;
  const lease = mine && p.status === 'claimed' && p.expiresMs ? `<small>担当期限 残り${duration(Math.max(0,p.expiresMs-world.timeMs))} · 着工前なら引継ぎ可</small>` : '';
  const material = Object.entries(p.reserved || {}).filter(([,n])=>n>0).map(([key,n])=>`${Rules.RESOURCE_META[key]?.name || key} ${fmt(n)}`).join(' / ');
  let actions = button('FOCUS','現地を見る');
  if (p.status === 'open') actions += button('CLAIM_PROJECT','担当する');
  if (mine && p.status === 'claimed') actions += button('START_PROJECT','着工する',!near || busy) + button('RELEASE_PROJECT','担当を引き継ぐ');
  if (prebuild && (p.creatorId === actorId || mine)) actions += button('CANCEL_PROJECT','計画を取り消す');
  return `<article class="project-card"><div class="project-status">${statusLabels[p.status] || escapeHtml(p.status)}<span>${escapeHtml(p.tileId)}</span></div><h3>${escapeHtml(name)}の共同建設</h3><p>提案：${escapeHtml(world.players[p.creatorId]?.name || '市民')}<br>担当：${escapeHtml(claimant || '募集中')}</p>${lease}${material ? `<p class="reserved-material">予約済み：${escapeHtml(material)}</p>` : ''}${mine && prebuild && !near ? '<p class="mini-label">現地か隣接地へ移動すると着工できます。</p>' : ''}<div class="project-actions">${actions}</div></article>`;
}

export function projectsMarkup(world, actorId, pending = false) {
  const active = activeProjects(world);
  const recent = Object.values(world.projects || {}).filter(p=>!['open','claimed','building'].includes(p.status)).slice(-5).reverse();
  return `<p class="drawer-intro">土地で「共同計画として提案」を選ぶと、材料を一度だけ予約します。担当を決めて現地で着工。他の市民へ引き継げます。</p>${active.length ? active.map(p=>projectCard(world,actorId,p,pending)).join('') : '<div class="empty-copy">共同計画はまだありません。建設したい土地を選んで提案しましょう。</div>'}${recent.length ? '<div class="section-label">最近の計画</div>'+recent.map(p=>projectCard(world,actorId,p,pending)).join('') : ''}`;
}

export function tileProjectMarkup(world, actorId, tileId, pending = false) {
  const project = activeProjects(world).find(p=>p.tileId===tileId);
  return project ? projectCard(world,actorId,project,pending) : '';
}
