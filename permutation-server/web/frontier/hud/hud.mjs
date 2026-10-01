// The game HUD around the map (UI plan, benchmarks: Eternum's header and
// left command column, Civ's yield bar and "what needs you" list, Travian's
// incoming-attack mark):
//   - the next-bell pill: the bell number, the time left and a bar of the
//     ten minutes; calm, then amber from 120 s and red from 30 s before the
//     bell (the screen edge follows only while a march is being composed);
//   - the resource strip: the active holding's stores, amount / cap, the
//     hourly rate and the time until full (full and near-full marked);
//   - the attention pill: what needs the player before the next bell
//     (possible arrivals at a holding, settlements due, full stores, a march
//     left unsent), one press moves to the next item;
//   - the left rail (desktop): the holdings with their warning mark, the
//     action tiles that open the panel's tabs, and what is on the way.
// Models are pure (they read FS and return numbers and text the tests
// check); the render functions return markup; app.mjs puts it in the page.
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { RESOURCES, RESOURCE_ORDER, TIERS, factionName } from '../fi18n.mjs';
import { storesAt, holdingFacts, SETTLER } from '../fland.mjs';
import { hostRows } from '../screens/host.mjs';
import { DEPART_STAMINA } from '../fmarch.mjs';
import { bellChip, countdown, BELL_SECS } from '../clock.mjs';
import { swatch } from '../screens/shell.mjs';
import { personChip, ownTag, highlights, renderHighlights } from '../people/ui.mjs';
import { leaderSvg } from '../people/leaders.mjs';
import { identityOf } from '../people/identity.mjs';

/** Seconds before the bell at which the pill turns amber, then red. */
export const URGENCY = Object.freeze({ warn: 120, crit: 30 });
/** A store this close to its cap (seconds of production) reads as "near full". */
export const NEAR_FULL_SECS = 3600;

export { activeHolding } from '../fstate.mjs';
import { activeHolding } from '../fstate.mjs';

// ------------------------------------------------------------------ the bell pill
/** `{bell, secondsLeft, frac, urgency, beforeGenesis, ended}` at chain time `now`. */
export function bellModel(clock, now) {
  const c = clock ? bellChip(clock, now) : { bell: null, secondsLeft: null };
  const left = c.secondsLeft;
  const inBell = !c.beforeGenesis && !c.ended && Number.isFinite(left);
  const urgency = !inBell ? 'calm' : left <= URGENCY.crit ? 'crit' : left <= URGENCY.warn ? 'warn' : 'calm';
  const frac = inBell ? Math.min(1, Math.max(0, 1 - left / BELL_SECS)) : 0;
  return { ...c, frac, urgency };
}

// ------------------------------------------------------------------ resources
/**
 * The strip's tokens for a holding at `now`: `[{resource, name, value, cap,
 * perHour, fullIn, state}]`, only resources the holding produces or holds;
 * `fullIn` is seconds to the cap (0 when full, null without production),
 * `state` 'full' | 'near' | 'ok'.
 */
export function resourceModel(holding, now) {
  if (!holding) return [];
  return storesAt(holding, now, RESOURCE_ORDER)
    .filter(s => s.value > 0 || s.perHour > 0)
    .map(s => {
      const fullIn = s.cap > 0 && s.value >= s.cap ? 0 : s.perHour > 0 && s.cap > 0 ? Math.ceil(((s.cap - s.value) / s.perHour) * 3600) : null;
      const state = fullIn === 0 ? 'full' : fullIn !== null && fullIn <= NEAR_FULL_SECS ? 'near' : 'ok';
      return { resource: s.resource, name: RESOURCES[s.resource], value: s.value, cap: s.cap, perHour: s.perHour, fullIn, state };
    });
}

/** A span for the strip: "12:30" under an hour, "3時間20分" under a day, else "5日". */
export function span(secs) {
  const s = Math.max(0, Math.ceil(secs));
  if (s < 3600) return countdown(s);
  if (s < 86_400) return L`${Math.floor(s / 3600)}時間${Math.floor((s % 3600) / 60)}分`;
  return L`${Math.floor(s / 86_400)}日`;
}

/** One token's hover text: "食料 1,200 / 5,000 · 毎時 +120 · 満杯まで 31:40". */
export function resourceTitle(r) {
  const full = r.fullIn === 0 ? L`満杯です（これ以上は貯まりません）` : r.fullIn === null ? '' : L`満杯まで ${span(r.fullIn)}`;
  return [`${r.name} ${fmtNum(r.value)} / ${fmtNum(r.cap)}`, L`毎時 +${fmtNum(r.perHour)}`, full].filter(Boolean).join(' · ');
}

// ------------------------------------------------------------------ attention
/**
 * What needs the player, most urgent first: `[{kind, text, p, q, tab}]`.
 *   incoming  another faction's host may arrive at one of the holdings
 *   settle    a march of the viewer's can be settled
 *   draft     a march is being composed and not yet sent
 *   full      a store of the active holding is full
 */
export function attentionItems(FS) {
  const out = [];
  for (const w of FS.incoming ?? []) {
    out.push({ kind: 'incoming', p: w.holding.p, q: w.holding.q, tab: 'marches', bell: w.bell, short: L`来襲 第${fmtNum(w.bell)}鐘`, text: L`第${fmtNum(w.bell)}鐘に州 ${w.holding.p},${w.holding.q} へ敵が来るかもしれません` });
  }
  for (const m of FS.marches ?? []) {
    if (m.facts?.settleReady && m.dest) out.push({ kind: 'settle', p: m.dest.p, q: m.dest.q, tab: 'marches', short: L`精算できる進軍`, text: L`州 ${m.dest.p},${m.dest.q} の進軍を精算できます` });
  }
  if (FS.compose && !FS.compose.sending) {
    const o = FS.compose.origin;
    out.push({ kind: 'draft', p: o.p, q: o.q, tab: 'marches', short: L`進軍が未送信`, text: L`編成中の進軍がまだ送られていません` });
  }
  if (FS.exploreSeedReady) {
    const h0 = activeHolding(FS);
    if (h0) out.push({ kind: 'explore', p: h0.p, q: h0.q, tab: 'hosts', short: L`探索の結果`, text: L`探索の結果を確定できます` });
  }
  const now = FS.chain?.now() ?? 0, bell = FS.nowBell ?? 0;
  // hosts ready to march: in the roster, rested, not on the road, nothing pending
  const idle = FS.holdings?.length ? hostRows(FS).filter(r => r.state === 1 && !r.inTransit && !r.pending && r.stamina >= DEPART_STAMINA && !(r.readyBell > bell)) : [];
  if (idle.length) out.push({ kind: 'idle', p: idle[0].p, q: idle[0].q, tile: idle[0].tile, host: String(idle[0].id), tab: 'hosts', n: idle.length, short: L`出陣できる軍勢 ${fmtNum(idle.length)}`, text: L`出陣できる軍勢が ${fmtNum(idle.length)} あります` });
  for (const h of FS.holdings ?? []) {
    const full = resourceModel(h, now).filter(r => r.state === 'full');
    const facts = FS.season ? holdingFacts(h, FS.season, now) : null;
    if (!facts) { if (full.length) out.push({ kind: 'full', p: h.p, q: h.q, tab: 'holding', holding: h, short: L`満杯 ${fmtNum(full.length)}`, text: L`${full.map(r => r.name).join(' / ')}が満杯です。収穫するか使いましょう` }); continue; }
    if (facts.final && !facts.queue.some(q => q.doneIn > 0)) out.push({ kind: 'build', p: h.p, q: h.q, tab: 'holding', holding: h, short: L`建設の列が空`, text: L`州 ${h.p},${h.q} の建設の列が空いています` });
    const musterable = facts.reserve.filter(r => r.unit !== SETTLER).reduce((a, r) => a + r.troops, 0);
    if (facts.final && musterable >= 100) out.push({ kind: 'muster', p: h.p, q: h.q, tab: 'holding', holding: h, short: L`編成できる兵 ${fmtNum(musterable)}`, text: L`控えの兵 ${fmtNum(musterable)} を軍勢に編成できます` });
    if (facts.dormantIn > 0 && facts.dormantIn < DORMANT_WARN_SECS) out.push({ kind: 'dormant', p: h.p, q: h.q, tab: 'holding', holding: h, short: L`休眠まで ${span(facts.dormantIn)}`, text: L`州 ${h.p},${h.q} の拠点はあと ${span(facts.dormantIn)} で休眠します。何か操作しましょう` });
    if (full.length) out.push({ kind: 'full', p: h.p, q: h.q, tab: 'holding', holding: h, short: L`満杯 ${fmtNum(full.length)}`, text: L`${full.map(r => r.name).join(' / ')}が満杯です。収穫するか使いましょう` });
  }
  return out;
}

/** A holding this close to dormancy (seconds) is an attention item. */
export const DORMANT_WARN_SECS = 6 * 3600;

/**
 * The pill's text (Civ's end-turn button names the next blocker): the first
 * item's short label, then how many more — "来襲 第1,040鐘 · ほか 2";
 * null when nothing needs the player.
 */
export function attentionText(items) {
  if (!items.length) return null;
  const first = items[0].short ?? items[0].text;
  return items.length > 1 ? L`${first} · ほか ${fmtNum(items.length - 1)}` : first;
}

// ------------------------------------------------------------------ markup
export function renderStrip(tokens) {
  return tokens.map(r => html`<span class="res res-${r.state}" title="${resourceTitle(r)}">
    <span class="res-name">${r.name}</span>
    <span class="res-val">${fmtNum(r.value)}</span>
    ${r.state === 'full' ? html`<span class="res-flag">${L`満杯`}</span>` : r.state === 'near' ? html`<span class="res-flag">${span(r.fullIn)}</span>` : ''}
  </span>`);
}

/**
 * A holding's clocks for the rail (Civ's city banner timers): the next
 * building done, the first store full, dormancy when it is near.
 */
export function holdingCountdowns(FS, h) {
  const now = FS.chain?.now?.() ?? 0;
  const out = [];
  const facts = FS.season ? holdingFacts(h, FS.season, now) : null;
  const next = facts?.queue.filter(q => q.doneIn > 0).sort((a, b) => a.doneIn - b.doneIn)[0];
  if (next) out.push(L`建設 あと ${span(next.doneIn)}`);
  const full = resourceModel(h, now).filter(r => r.fullIn > 0).sort((a, b) => a.fullIn - b.fullIn)[0];
  if (full && full.fullIn < 24 * 3600) out.push(L`${full.name} 満杯まで ${span(full.fullIn)}`);
  if (facts && facts.dormantIn > 0 && facts.dormantIn < DORMANT_WARN_SECS) out.push(L`休眠まで ${span(facts.dormantIn)}`);
  return out;
}

/** The action tiles: each opens a tab of the panel (Eternum's Build · Military · Transfer row). */
const TILES = [
  { tab: 'holding', glyph: '⌂', text: () => L`拠点` },
  { tab: 'hosts', glyph: '⚔', text: () => L`軍勢` },
  { tab: 'marches', glyph: '➚', text: () => L`進軍` },
  { tab: 'more', glyph: '☷', text: () => L`その他` },
];

/**
 * The standings of the six factions from the overviews (the spectator's rail):
 * `[{faction, holdings, provinces}]`, most holdings first.
 */
export function standings(overviews) {
  const rows = Array.from({ length: 6 }, (_, f) => ({ faction: f, holdings: 0, provinces: 0 }));
  for (const o of overviews?.values?.() ?? []) for (const pr of o.provinces ?? []) {
    const seen = new Set();
    pr.owners.forEach((f, j) => { if (pr.sites[j] === 1 && f < 6) { rows[f].holdings++; seen.add(f); } });
    for (const f of seen) rows[f].provinces++;
  }
  return rows.sort((a, b) => b.holdings - a.holdings || b.provinces - a.provinces || a.faction - b.faction);
}

function renderStandings(FS) {
  const rows = standings(FS.overviews);
  return html`<h2 class="rail-h">${L`勢力の順位`}</h2>
    <ol class="rail-standings">${rows.map((r, i) => html`<li><span class="rank">${i + 1}</span>${raw(leaderSvg(r.faction, { size: 34 }))}<strong>${factionName(r.faction)}</strong>
      <span class="muted">${L`拠点 ${fmtNum(r.holdings)} · ${fmtNum(r.provinces)} 州`}</span></li>`)}</ol>
    <p class="muted">${L`拠点の数は最新の概観（鐘ごと）から数えています。`}</p>
    <h2 class="rail-h">${L`見どころ`}</h2>${renderHighlights(highlights(FS.chronicle, FS.overviews, FS.roster))}`;
}

/** The left rail: holdings, action tiles, what needs the player (the spectator: the standings). */
export function renderRail(FS) {
  if (FS.mode === 'spectate') return html`<p class="rail-faction muted">${L`観戦中`}</p>${renderStandings(FS)}`;
  const hs = FS.holdings ?? [];
  const active = activeHolding(FS);
  const warned = new Set((FS.incoming ?? []).map(w => `${w.holding.p},${w.holding.q}`));
  const items = attentionItems(FS);
  const faction = FS.citizen?.faction;
  const tag = Number.isInteger(faction) ? ownTag(FS) : null;
  const head = Number.isInteger(faction)
    ? html`<div class="rail-me">${tag !== null ? personChip(identityOf(tag), faction, { size: 44, full: true, note: factionName(faction) }) : html`<p class="rail-faction">${swatch(faction)}<strong>${factionName(faction)}</strong></p>`}</div>`
    : html`<p class="rail-faction muted">${FS.mode === 'spectate' ? L`観戦中` : L`まだ陣営に加わっていません`}</p>`;
  const list = hs.length
    ? html`<ul class="rail-list">${hs.map((h, i) => html`<li><button type="button" class="rail-holding" data-act="holding-pick" data-i="${i}" ${raw(h === active ? 'aria-current="true"' : '')}>
        <span class="rail-tier">${TIERS[h.tier] ?? h.tier}</span>
        <span class="rail-where">${L`州 ${h.p},${h.q} 区画 ${h.site + 1}`}</span>
        ${warned.has(`${h.p},${h.q}`) ? html`<span class="rail-warn">${L`来襲の恐れ`}</span>` : ''}
        ${(() => { const c = holdingCountdowns(FS, h); return c.length ? html`<span class="rail-clock">${c.join(' · ')}</span>` : ''; })()}
      </button></li>`)}</ul>`
    : html`<p class="muted">${FS.mode === 'play' ? L`拠点はまだありません。地図の「参加」から始めます。` : L`拠点はありません`}</p>`;
  const tiles = FS.mode === 'play' && hs.length
    ? html`<div class="rail-tiles">${TILES.map(t => html`<button type="button" class="rail-tile" data-act="tab" data-tab="${t.tab}" ${raw((FS.tab ?? 'map') === t.tab ? 'aria-pressed="true"' : 'aria-pressed="false"')}><span class="rail-glyph" aria-hidden="true">${t.glyph}</span><span>${t.text()}</span></button>`)}</div>`
    : '';
  const todo = items.length
    ? html`<ol class="rail-todo">${items.map((x, i) => html`<li class="todo-${x.kind}"><button type="button" class="rail-todo-btn" data-act="attn-go" data-i="${i}">${x.text}</button></li>`)}</ol>`
    : html`<p class="muted">${L`次の鐘までにやることはありません`}</p>`;
  return html`${head}
    <h2 class="rail-h">${L`拠点`}</h2>${list}
    ${tiles}
    <h2 class="rail-h">${L`次の鐘までに`}</h2>${todo}`;
}
