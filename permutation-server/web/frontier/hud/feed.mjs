// What happened (UI plan A2–A4; Civ's notification stack, Old World's turn
// summary, Civ VII's "pan to combat"): the page compares what it knew at
// the last poll with what it knows now and turns each change into one
// notification — a march revealed, resolved or settled, a battle in a
// province of the viewer's, an attack that may come, a building finished,
// a holding made final. When the bell turns, the changes of the bell that
// ended are folded into one summary ("bell 1,034: 2 marches resolved…").
//
// Notifications sit at the map's top right (newest first, at most
// TOAST_MAX, each with "go there" and, for battles, "watch" and "report"),
// a × dismisses one; the More tab keeps the last FEED_MAX with filters.
// Nothing here blocks the game: no modal, no sound.
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { BUILDINGS, TIERS } from '../fi18n.mjs';
import { BUILD_ITEMS } from '../fland.mjs';

export const FEED_MAX = 60;
export const TOAST_MAX = 3;
/** A notification stays a toast for this long (ms) unless dismissed; it stays in the list after. */
export const TOAST_MS = 45_000;

const marchKey = m => String(m.entry?.host ?? m.transit?.hostId ?? '');

/** What the page knows now, reduced to what notifications compare. */
export function snapshot(FS, now = FS.chain?.now?.() ?? 0) {
  const marches = new Map();
  for (const m of FS.marches ?? []) {
    const k = marchKey(m);
    if (k) marches.set(k, { pipeline: m.facts?.pipeline ?? null, revealed: !!m.facts?.slotPresent, settle: !!m.facts?.settleReady,
      dest: m.dest ?? null, bell: m.entry?.arriveBell ?? m.transit?.arriveBell ?? null, outcome: m.transit?.outcome ?? null });
  }
  const builds = new Set(), holdings = new Map();
  for (const h of FS.holdings ?? []) {
    holdings.set(`${h.p},${h.q},${h.site}`, { state: h.state, tier: h.tier, p: h.p, q: h.q });
    for (const q of h.queue ?? []) {
      const done = Number(q.doneAt ?? 0);
      if (done > 0 && done <= now) builds.add(`${h.p},${h.q},${h.site},${q.kind},${done}`);
    }
  }
  const own = new Set((FS.holdings ?? []).map(h => `${h.p},${h.q}`));
  const clashes = new Set();
  for (const x of FS.chronicle ?? []) {
    const r = x.record ?? x;
    if (r.name === 'CLASH' && own.has(`${Number(r.p)},${Number(r.q)}`)) clashes.add(`${Number(r.p)},${Number(r.q)},${Number(r.bell)}`);
  }
  return { bell: FS.nowBell ?? null, marches, builds, holdings, clashes, incoming: new Set((FS.incoming ?? []).map(w => `${w.bell},${w.holding.p},${w.holding.q}`)) };
}

/**
 * The notifications between two snapshots: `[{id, kind, bell, text, p, q,
 * battle?: {p, q, bell}}]`, kinds 'battle' | 'march' | 'incoming' |
 * 'build' | 'holding'. A first snapshot (prev null) is not news.
 */
export function diffFeed(prev, next) {
  if (!prev) return [];
  const out = [];
  const bell = next.bell;
  for (const [k, m] of next.marches) {
    const was = prev.marches.get(k);
    const d = m.dest;
    if (!was) continue;
    if (!was.revealed && m.revealed && d) out.push({ id: `rv:${k}`, kind: 'march', bell, p: d.p, q: d.q, text: L`州 ${d.p},${d.q} への進軍が開封されました` });
    if (was.pipeline !== 'resolved' && m.pipeline === 'resolved' && d) out.push({ id: `rs:${k}`, kind: 'battle', bell, p: d.p, q: d.q, battle: { p: d.p, q: d.q, bell: m.bell }, text: L`州 ${d.p},${d.q} の衝突が決着しました（第${fmtNum(m.bell)}鐘）` });
    if (!was.settle && m.settle && d) out.push({ id: `st:${k}`, kind: 'march', bell, p: d.p, q: d.q, text: L`州 ${d.p},${d.q} の進軍を精算できます` });
  }
  for (const c of next.clashes) {
    if (prev.clashes.has(c)) continue;
    const [p, q, b] = c.split(',').map(Number);
    if (out.some(x => x.battle && x.battle.p === p && x.battle.q === q && x.battle.bell === b)) continue;
    out.push({ id: `cl:${c}`, kind: 'battle', bell, p, q, battle: { p, q, bell: b }, text: L`あなたの拠点の州 ${p},${q} で戦いがありました（第${fmtNum(b)}鐘）` });
  }
  for (const w of next.incoming) {
    if (prev.incoming.has(w)) continue;
    const [b, p, q] = w.split(',').map(Number);
    out.push({ id: `in:${w}`, kind: 'incoming', bell, p, q, text: L`第${fmtNum(b)}鐘に州 ${p},${q} の拠点へ敵が来るかもしれません` });
  }
  for (const b of next.builds) {
    if (prev.builds.has(b)) continue;
    const [p, q, , kind] = b.split(',').map(Number);
    out.push({ id: `bd:${b}`, kind: 'build', bell, p, q, text: L`${BUILDINGS[BUILD_ITEMS[kind]?.resource] ?? L`建物`}が完成しました（州 ${p},${q}）` });
  }
  for (const [k, h] of next.holdings) {
    const was = prev.holdings.get(k);
    if (!was) out.push({ id: `hn:${k}:${h.state}`, kind: 'holding', bell, p: h.p, q: h.q, text: L`州 ${h.p},${h.q} に${TIERS[h.tier] ?? ''}を得ました` });
    else if (was.state !== 2 && h.state === 2) out.push({ id: `hf:${k}`, kind: 'holding', bell, p: h.p, q: h.q, text: L`州 ${h.p},${h.q} の拠点が確定しました` });
    else if (was.tier !== h.tier) out.push({ id: `ht:${k}:${h.tier}`, kind: 'holding', bell, p: h.p, q: h.q, text: L`州 ${h.p},${h.q} の拠点が${TIERS[h.tier] ?? ''}になりました` });
  }
  // a holding that is gone: displaced while provisional, or lost (a new site ticket is filed automatically)
  for (const [k, was] of prev.holdings) {
    if (next.holdings.has(k)) continue;
    out.push({ id: `hg:${k}:${bell}`, kind: 'holding', bell, p: was.p, q: was.q, text: was.state === 2 ? L`州 ${was.p},${was.q} の拠点を失いました。入植希望を自動でもう一度出します` : L`州 ${was.p},${was.q} の仮の拠点は押し出されました。入植希望を自動でもう一度出します` });
  }
  return out;
}

/** The summary of a bell that ended: one line per kind, or null when nothing happened. */
export function bellSummary(feed, bell) {
  const of = feed.filter(x => x.bell === bell && x.kind !== 'summary');
  if (!of.length) return null;
  const n = k => of.filter(x => x.kind === k).length;
  const parts = [
    n('battle') && L`戦い ${fmtNum(n('battle'))}`, n('march') && L`進軍の知らせ ${fmtNum(n('march'))}`, n('incoming') && L`来襲の恐れ ${fmtNum(n('incoming'))}`,
    n('build') && L`完成 ${fmtNum(n('build'))}`, n('holding') && L`拠点 ${fmtNum(n('holding'))}`,
  ].filter(Boolean);
  return { id: `sum:${bell}`, kind: 'summary', bell: bell + 1, text: L`第${fmtNum(bell)}鐘のまとめ：${parts.join(' · ')}`, items: of.map(x => x.id) };
}

/** Add notifications to a feed (newest first, no duplicates, at most FEED_MAX), stamping when they arrived. */
export function pushFeed(feed, items, at = Date.now()) {
  const seen = new Set(feed.map(x => x.id));
  const fresh = items.filter(x => !seen.has(x.id)).map(x => ({ ...x, at }));
  return [...fresh.reverse(), ...feed].slice(0, FEED_MAX);
}

const KIND_ICON = { battle: '⚔', march: '➚', incoming: '!', build: '⚒', holding: '⌂', summary: '≡' };
const KIND_TEXT = { battle: () => L`戦闘`, march: () => L`進軍`, incoming: () => L`来襲`, build: () => L`建設`, holding: () => L`拠点`, summary: () => L`まとめ` };

function itemActions(x) {
  const acts = [];
  if (Number.isInteger(x.p)) acts.push(html`<button type="button" class="btn small" data-act="feed-go" data-id="${x.id}">${L`そこへ移動`}</button>`);
  if (x.battle) {
    acts.push(html`<button type="button" class="btn small" data-act="battle-play" data-p="${x.battle.p}" data-q="${x.battle.q}" data-bell="${x.battle.bell}">${L`戦いを見る`}</button>`);
    acts.push(html`<button type="button" class="btn small" data-act="report-open" data-p="${x.battle.p}" data-q="${x.battle.q}" data-bell="${x.battle.bell}">${L`報告`}</button>`);
  }
  return acts;
}

/** The toasts: the newest live, undismissed notifications. */
export function renderToasts(feed, { dismissed = new Set(), now = Date.now() } = {}) {
  const live = feed.filter(x => !dismissed.has(x.id) && now - (x.at ?? 0) < TOAST_MS).slice(0, TOAST_MAX);
  return live.map(x => html`<div class="toast toast-${x.kind}" role="status">
    <span class="toast-icon" aria-hidden="true">${KIND_ICON[x.kind] ?? '•'}</span>
    <span class="toast-text"><span class="toast-kind">${KIND_TEXT[x.kind]?.() ?? ''}</span>${x.text}</span>
    <span class="toast-acts">${itemActions(x)}<button type="button" class="btn small toast-x" data-act="feed-dismiss" data-id="${x.id}" aria-label="${L`閉じる`}">×</button></span>
  </div>`);
}

export const FEED_FILTERS = Object.freeze(['all', 'battle', 'march', 'holding']);
const FILTER_TEXT = { all: () => L`すべて`, battle: () => L`戦闘`, march: () => L`進軍と来襲`, holding: () => L`拠点と建設` };
const inFilter = (x, f) => f === 'all' || (f === 'battle' && x.kind === 'battle') || (f === 'march' && (x.kind === 'march' || x.kind === 'incoming')) || (f === 'holding' && (x.kind === 'holding' || x.kind === 'build'));

/** The notification list (the More tab). */
export function renderCentre(feed, filter = 'all') {
  const list = feed.filter(x => inFilter(x, filter));
  return html`<section aria-labelledby="feed-title"><h3 id="feed-title">${L`お知らせ`}</h3>
    <div class="feed-filters" role="group" aria-label="${L`絞り込み`}">${FEED_FILTERS.map(f => html`<button type="button" class="btn small" data-act="feed-filter" data-f="${f}" aria-pressed="${f === filter ? 'true' : 'false'}">${FILTER_TEXT[f]()}</button>`)}</div>
    ${list.length ? html`<ol class="list feed-list">${list.map(x => html`<li class="feed-${x.kind}"><span class="toast-icon" aria-hidden="true">${KIND_ICON[x.kind] ?? ''}</span><span>${x.text}${Number.isInteger(x.bell) ? html` <span class="muted">${L`第${fmtNum(x.bell)}鐘`}</span>` : ''}</span><span class="toast-acts">${itemActions(x)}</span></li>`)}</ol>`
      : html`<p class="muted">${L`まだお知らせはありません`}</p>`}</section>`;
}

export { raw };
