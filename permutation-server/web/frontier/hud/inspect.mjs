// The selection inspector (UI plan stage 3; Eternum's tile details, Civ's
// select → details): what is on the chosen province or tile, from what the
// page already has — the overview (owners and site states of every open
// province), the province envelope (site mirror, hosts, camp, relations)
// and the rules module's terrain — and the actions that make sense there:
// open the holding, choose the tile as the march destination, add the site
// to the settlement ticket, read the last clash report. Every action is an
// existing `data-act` of the play screens; the inspector only offers it.
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { TIERS, UNITS, factionName } from '../fi18n.mjs';
import { UNIT_ORDER } from '../fland.mjs';
import { ringOf } from '../fgeo.mjs';
import { troopsOf } from '../fmarch.mjs';
import { swatch } from '../screens/shell.mjs';
import { personChip, hostOwner } from '../people/ui.mjs';
import { identityOf } from '../people/identity.mjs';
import { activityText } from '../people/activity.mjs';

const TERRAIN_TEXT = { Grassland: () => L`草原`, Plains: () => L`平原`, Forest: () => L`森`, Hills: () => L`丘`, Mountain: () => L`山`, Water: () => L`水` };
/** Site states as the overview carries them (herald SITE_STATE) and the mirror (3 = released: a Free City). */
const SITE_TEXT = { free: () => L`空き区画`, holding: () => L`拠点`, camp: () => L`蛮族の野営地`, reserved: () => L`予約済みの区画`, freeCity: () => L`自由都市` };

const overviewRec = (FS, p, q) => {
  for (const o of FS.overviews?.values?.() ?? []) { const r = o.provinces?.find(x => x.p === p && x.q === q); if (r) return r; }
  return null;
};

/** Whether factions a and b are not hostile in a province (RELATIONS bit a*8+b). */
export const friendly = (relations, a, b) => a === b || (relations !== undefined && relations !== null && ((BigInt(relations) >> BigInt(a * 8 + b)) & 1n) === 1n);

/**
 * The inspector's view of the selection: `{p, q, ring, opened, owners, tile?}`
 * with `tile = {idx, terrain, site?, hosts, camp?}`; null without a selection.
 */
export function inspectModel(FS, terrainOf) {
  const s = FS.selected;
  if (!s || !Number.isInteger(s.p)) return null;
  const rec = overviewRec(FS, s.p, s.q);
  const prov = FS.provinces?.get(`${s.p},${s.q}`)?.province ?? null;
  const viewer = FS.citizen?.faction;
  const owners = rec ? [...new Set(rec.owners.filter((f, j) => rec.sites[j] === 1 && f < 6))] : [];
  const out = { p: s.p, q: s.q, ring: ringOf(s.p, s.q), opened: !!rec, owners, clash: !!rec?.clash, loaded: !!prov,
    relation: Number.isInteger(viewer) && prov ? owners.filter(f => f !== viewer).map(f => ({ faction: f, friendly: friendly(prov.relations, viewer, f) })) : [],
    reportBell: prov?.resolveSummary?.bell || null, tile: null };
  if (!Number.isInteger(s.idx)) return out;
  const t = terrainOf?.(s.p, s.q) ?? null;
  const j = t ? t.sites.indexOf(s.idx) : prov ? Array.from(prov.sites ?? []).indexOf(s.idx) : -1;
  const tile = { idx: s.idx, terrain: t ? t.names[t.terrain[s.idx]] : null, site: null, hosts: [], camp: null };
  if (j >= 0) {
    const m = prov?.siteMirror?.[j];
    const state = m ? (m.state === 3 ? 'freeCity' : ['free', 'holding', 'camp', 'free', 'reserved'][m.state] ?? 'free') : ['free', 'holding', 'camp', 'reserved'][rec?.sites?.[j] ?? 0];
    const faction = m ? m.faction : rec?.owners?.[j];
    const mine = (FS.holdings ?? []).find(h => h.p === s.p && h.q === s.q && h.site === j) ?? null;
    const holder = state === 'holding' ? FS.roster?.ownerOf(s.p, s.q, j) ?? null : null;
    tile.site = { index: j, state, faction: state === 'holding' ? faction : null, owner: holder ? identityOf(holder.tag) : null, tier: m?.tier ?? null, garrison: m ? troopsOf(m.garrison) : null,
      shield: m ? m.shieldUntilBell > (FS.nowBell ?? 0) : false, mine: !!mine, holdingIndex: mine ? FS.holdings.indexOf(mine) : -1 };
  }
  for (const e of prov?.entries ?? []) {
    if (e.tile !== s.idx || (e.state !== 1 && e.state !== 2)) continue;
    tile.hosts.push({ id: String(e.id), faction: e.faction, owner: hostOwner(FS.roster, e.id), unit: UNIT_ORDER[e.unit] ?? null, troops: troopsOf(e.troops), pending: e.state === 2 });
  }
  if (prov?.camp?.state === 1 && prov.camp.tile === s.idx) tile.camp = { troops: troopsOf(prov.camp.troops) };
  out.tile = tile;
  return out;
}

/** The actions the selection offers: `[{act, data, text, primary?}]` (existing play actions only). */
export function inspectActions(FS, m) {
  if (!m || FS.mode !== 'play') return [];
  const out = [];
  const t = m.tile;
  if (t?.site?.mine) out.push({ act: 'holding-pick', data: { i: t.site.holdingIndex }, text: L`この拠点を開く`, primary: true });
  if (FS.compose && t && !FS.compose.sending) out.push({ act: 'dest-from-map', data: {}, text: L`ここを進軍の行き先にする`, primary: true });
  if (FS.land?.stage === 'joined' && t?.site?.state === 'free') {
    const draft = FS.joinDraft?.envelope?.province;
    if (draft && draft.p === m.p && draft.q === m.q) {
      const chosen = (FS.joinDraft.sites ?? []).some(x => x.p === m.p && x.q === m.q && x.site === t.site.index);
      out.push({ act: 'toggle-site', data: { p: m.p, q: m.q, site: t.site.index }, text: chosen ? L`入植希望から外す` : L`この区画を入植希望に加える`, primary: !chosen });
    } else out.push({ act: 'pick-province', data: { p: m.p, q: m.q }, text: L`この州の空き区画を見る` });
  }
  if (m.reportBell) out.push({ act: 'report-open', data: { p: m.p, q: m.q, bell: m.reportBell }, text: L`第${fmtNum(m.reportBell)}鐘の衝突の報告` });
  return out;
}

const dataAttrs = d => raw(Object.entries(d).map(([k, v]) => `data-${k}="${String(v).replace(/[^\w.-]/g, '')}"`).join(' '));

export function render(FS, terrainOf, activities = null) {
  const m = inspectModel(FS, terrainOf);
  if (!m) return html`<section class="inspect" aria-labelledby="inspect-title"><h3 id="inspect-title">${L`選択`}</h3><p class="muted">${L`地図のマスを選ぶと、ここに中身が出ます。`}</p></section>`;
  const t = m.tile;
  const title = t ? L`州 ${m.p},${m.q} · マス ${t.idx + 1}` : L`州 ${m.p},${m.q}`;
  const facts = [];
  facts.push(html`<div class="row"><dt>${L`輪`}</dt><dd>${m.opened ? L`第${m.ring}輪` : L`第${m.ring}輪（まだひらいていません）`}</dd></div>`);
  if (t?.terrain) facts.push(html`<div class="row"><dt>${L`地形`}</dt><dd>${TERRAIN_TEXT[t.terrain]?.() ?? t.terrain}</dd></div>`);
  if (!t && m.owners.length) facts.push(html`<div class="row"><dt>${L`拠点を持つ陣営`}</dt><dd>${m.owners.map(f => html`<span class="nowrap">${swatch(f)}${factionName(f)}</span> `)}</dd></div>`);
  if (m.relation.length) facts.push(html`<div class="row"><dt>${L`あなたとの関係`}</dt><dd>${m.relation.map(r => html`<span class="nowrap rel-${r.friendly ? 'ally' : 'war'}">${swatch(r.faction)}${factionName(r.faction)} ${r.friendly ? L`友好` : L`敵対`}</span> `)}</dd></div>`);
  if (m.clash) facts.push(html`<div class="row"><dt>${L`この鐘`}</dt><dd>${L`衝突あり`}</dd></div>`);
  const site = t?.site;
  const siteBlock = site ? html`<h4>${site.state === 'holding' ? html`${swatch(site.faction)}${L`${TIERS[site.tier] ?? ''}（${factionName(site.faction)}）`}` : SITE_TEXT[site.state]()}${site.mine ? html` <span class="tag">${L`あなたの拠点`}</span>` : ''}</h4>
    ${site.owner ? html`<p class="inspect-owner">${personChip(site.owner, site.faction, { size: 40, full: true, note: L`この拠点の領主` })}</p>` : ''}
    <dl class="facts">
      <div class="row"><dt>${L`区画`}</dt><dd>${fmtNum(site.index + 1)}</dd></div>
      ${site.garrison !== null && site.state === 'holding' ? html`<div class="row"><dt>${L`守備隊`}</dt><dd>${fmtNum(site.garrison)}</dd></div>` : ''}
      ${site.shield ? html`<div class="row"><dt>${L`保護`}</dt><dd>${L`保護中（攻撃されません）`}</dd></div>` : ''}
    </dl>` : '';
  const hosts = t?.hosts?.length ? html`<h4>${L`このマスの軍勢`}</h4><ul class="list">${t.hosts.map(h => html`<li class="host-row">${h.owner ? personChip(h.owner, h.faction, { size: 24 }) : swatch(h.faction)}<span>${factionName(h.faction)} · ${h.unit ? UNITS[h.unit] : ''} ${fmtNum(h.troops)}</span>${h.pending ? html` <span class="muted">${L`（次の鐘から）`}</span>` : ''}</li>`)}</ul>` : '';
  const now = t ? (activities?.get(`${m.p},${m.q},${t.idx}`) ?? []).filter((a, i, all) => all.findIndex(b => b.kind === a.kind) === i) : [];
  const doing = now.length ? html`<h4>${L`いまの様子`}</h4><ul class="list doing">${now.map(a => html`<li class="doing-${a.kind}">${activityText(a)}</li>`)}</ul>` : '';
  const camp = t?.camp ? html`<p class="warn">${L`蛮族の野営地（${fmtNum(t.camp.troops)} 兵）`}</p>` : '';
  const acts = inspectActions(FS, m);
  return html`<section class="inspect" aria-labelledby="inspect-title"><h3 id="inspect-title">${title}</h3>
    <dl class="facts">${facts}</dl>${siteBlock}${doing}${hosts}${camp}
    ${!m.loaded && m.opened ? html`<p class="muted">${L`州の詳しい中身を読み込んでいます…`}</p>` : ''}
    ${acts.length ? html`<div class="actions">${acts.map(a => html`<button type="button" class="btn${a.primary ? ' primary' : ''}" data-act="${a.act}" ${dataAttrs(a.data)}>${a.text}</button>`)}</div>` : ''}
  </section>`;
}
