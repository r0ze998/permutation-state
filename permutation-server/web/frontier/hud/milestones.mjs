// Milestones and the season timeline (UI plan F1, F2; Civ's era quotes and
// Rise and Fall's timeline): the first holding, its confirmation, the first
// march, the first battle held, a holding's tier rising, a ring opening.
// When one is reached the viewer's leader says a line in a small banner (not
// a modal, gone on its own); every milestone keeps its bell, and More shows
// them as the season's timeline. All from state the page already has.
import { html, raw } from '../../util.mjs';
import { L, fmtNum, lang } from '../../lang.mjs';
import { factionName } from '../fi18n.mjs';
import { LEADERS, leaderSvg } from '../people/leaders.mjs';
import { holdingName } from '../people/ui.mjs';

export const MILESTONE_KEY = 'ps-fmile:';
export const BANNER_MS = 12_000;

/** A holding's name in the page's language now (the record keeps coordinates and tier, never a name). */
const nameOf = x => holdingName({ p: x.p, q: x.q, site: x.site }, x.tier ?? 0);

const TEXT = {
  'first-holding': { title: x => L`最初の拠点：${nameOf(x)}`, line: () => L`旗は立った。ここが我らの始まりの地だ。` },
  confirmed: { title: x => L`拠点が確定：${nameOf(x)}`, line: () => L`もう誰にも奪わせはしない。この地を守り抜け。` },
  'first-march': { title: () => L`最初の出陣`, line: () => L`封は閉じた。行き先を知るのは我らだけだ。` },
  'first-win': { title: x => L`最初の勝利：州 ${x.p},${x.q}`, line: () => L`見事だ。辺境は勇む者の手に渡る。` },
  'tier-up': { title: x => L`${nameOf(x)}になった`, line: () => L`民が増え、壁が伸びる。次の鐘も怠るな。` },
  ring: { title: x => L`第${x.ring}輪がひらいた`, line: () => L`霧が晴れ、新しい地が見えた。誰より先に向かえ。` },
};

/** Milestones reached in the state: `[{id, kind, p?, q?, ring?, name?}]` (ids stable across loads). */
export function reachedMilestones(FS) {
  const out = [];
  const hs = FS.holdings ?? [];
  if (hs.length) { const h = hs[0]; out.push({ id: 'first-holding', kind: 'first-holding', p: h.p, q: h.q, site: h.site, tier: 0 }); }
  const fin = hs.find(h => h.state === 2);
  if (fin) out.push({ id: 'confirmed', kind: 'confirmed', p: fin.p, q: fin.q, site: fin.site, tier: Number(fin.tier ?? 0) });
  for (const h of hs) for (let t = 1; t <= Number(h.tier ?? 0); t++) out.push({ id: `tier:${h.p},${h.q},${h.site}:${t}`, kind: 'tier-up', p: h.p, q: h.q, site: h.site, tier: t });
  const ms = FS.marches ?? [];
  if (ms.length || hs.some(h => (h.transit ?? []).some(t => t.state >= 1))) out.push({ id: 'first-march', kind: 'first-march' });
  const won = ms.find(m => (m.facts?.settled?.outcome ?? m.transit?.outcome) === 'Stays' && m.dest);
  if (won) out.push({ id: 'first-win', kind: 'first-win', p: won.dest.p, q: won.dest.q });
  const rings = FS.record?.rings?.length ?? 0;
  for (let r = 2; r < rings; r++) out.push({ id: `ring:${r}`, kind: 'ring', ring: r });
  return out;
}

/** Load the seen record `{v:1, seen: {id: {bell, kind, ...}}}` (a damaged one reads as none). */
export function loadSeen(storage, key) {
  try { const v = JSON.parse(storage.get(key) ?? 'null'); if (v && v.v === 1 && v.seen && typeof v.seen === 'object') return v; } catch { /* damaged */ }
  return null;
}

/**
 * The new milestones and the record to store: the first load of a season
 * records what is already reached quietly (no banners for old news).
 */
export function newMilestones(record, reached, bell) {
  const first = !record;
  const seen = { ...(record?.seen ?? {}) };
  const fresh = [];
  for (const m of reached) {
    if (seen[m.id]) continue;
    seen[m.id] = { bell: bell ?? 0, kind: m.kind, p: m.p, q: m.q, site: m.site, tier: m.tier, ring: m.ring };
    if (!first) fresh.push(m);
  }
  return { fresh, record: { v: 1, seen } };
}

const leaderName = f => (lang() === 'en' ? LEADERS[f]?.name.en : LEADERS[f]?.name.ja) ?? '';

/** The banner of one milestone, spoken by the viewer's leader. */
export function renderBanner(m, faction) {
  const t = TEXT[m.kind];
  if (!t) return '';
  const f = Number.isInteger(faction) ? faction : 0;
  return html`<div class="mile-inner">${raw(leaderSvg(f, { size: 56 }))}<div class="mile-text">
    <strong class="mile-title">${t.title(m)}</strong>
    <q class="mile-line">${t.line()}</q> <span class="muted">— <span data-name>${leaderName(f)}</span> · ${factionName(f)}</span></div>
    <button type="button" class="btn small" data-act="mile-close" aria-label="${L`閉じる`}">×</button></div>`;
}

/** The season's timeline (More): the viewer's milestones by bell. */
export function renderTimeline(record) {
  const list = Object.entries(record?.seen ?? {}).map(([id, x]) => ({ id, ...x })).sort((a, b) => a.bell - b.bell);
  if (!list.length) return '';
  return html`<section aria-labelledby="timeline-title"><h3 id="timeline-title">${L`シーズンの年表`}</h3>
    <ol class="timeline">${list.map(x => html`<li class="tl-${x.kind}"><span class="tl-bell">${L`第${fmtNum(x.bell)}鐘`}</span> <span>${TEXT[x.kind]?.title(x) ?? x.id}</span>
      ${Number.isInteger(x.p) ? html` <button type="button" class="btn small" data-act="goto" data-p="${x.p}" data-q="${x.q}">${L`地図で見る`}</button>` : ''}</li>`)}</ol></section>`;
}

