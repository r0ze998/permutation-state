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
import { storesAt } from '../fland.mjs';
import { bellChip, countdown, BELL_SECS } from '../clock.mjs';
import { swatch } from '../screens/shell.mjs';

/** Seconds before the bell at which the pill turns amber, then red. */
export const URGENCY = Object.freeze({ warn: 120, crit: 30 });
/** A store this close to its cap (seconds of production) reads as "near full". */
export const NEAR_FULL_SECS = 3600;

/** The holding the HUD shows: the chosen one (FS.activeHolding), else the first. */
export function activeHolding(FS) {
  const hs = FS.holdings ?? [];
  const i = Number.isInteger(FS.activeHolding) && FS.activeHolding < hs.length ? FS.activeHolding : 0;
  return hs[i] ?? null;
}

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
    out.push({ kind: 'incoming', p: w.holding.p, q: w.holding.q, tab: 'marches', bell: w.bell, text: L`第${fmtNum(w.bell)}鐘に州 ${w.holding.p},${w.holding.q} へ敵が来るかもしれません` });
  }
  for (const m of FS.marches ?? []) {
    if (m.facts?.settleReady && m.dest) out.push({ kind: 'settle', p: m.dest.p, q: m.dest.q, tab: 'marches', text: L`州 ${m.dest.p},${m.dest.q} の進軍を精算できます` });
  }
  if (FS.compose && !FS.compose.sending) {
    const o = FS.compose.origin;
    out.push({ kind: 'draft', p: o.p, q: o.q, tab: 'marches', text: L`編成中の進軍がまだ送られていません` });
  }
  const h = activeHolding(FS);
  if (h) {
    const full = resourceModel(h, FS.chain?.now() ?? 0).filter(r => r.state === 'full');
    if (full.length) out.push({ kind: 'full', p: h.p, q: h.q, tab: 'holding', text: L`${full.map(r => r.name).join(' / ')}が満杯です。収穫するか使いましょう` });
  }
  return out;
}

/** The pill's text: "要対応 3" / "Needs you 3"; null when nothing needs the player. */
export const attentionText = items => (items.length ? L`要対応 ${fmtNum(items.length)}` : null);

// ------------------------------------------------------------------ markup
export function renderStrip(tokens) {
  return tokens.map(r => html`<span class="res res-${r.state}" title="${resourceTitle(r)}">
    <span class="res-name">${r.name}</span>
    <span class="res-val">${fmtNum(r.value)}</span>
    ${r.state === 'full' ? html`<span class="res-flag">${L`満杯`}</span>` : r.state === 'near' ? html`<span class="res-flag">${span(r.fullIn)}</span>` : ''}
  </span>`);
}

/** The action tiles: each opens a tab of the panel (Eternum's Build · Military · Transfer row). */
const TILES = [
  { tab: 'holding', glyph: '⌂', text: () => L`拠点` },
  { tab: 'hosts', glyph: '⚔', text: () => L`軍勢` },
  { tab: 'marches', glyph: '➚', text: () => L`進軍` },
  { tab: 'more', glyph: '☷', text: () => L`記録` },
];

/** The left rail: holdings, action tiles, what needs the player. */
export function renderRail(FS) {
  const hs = FS.holdings ?? [];
  const active = activeHolding(FS);
  const warned = new Set((FS.incoming ?? []).map(w => `${w.holding.p},${w.holding.q}`));
  const items = attentionItems(FS);
  const faction = FS.citizen?.faction;
  const head = Number.isInteger(faction)
    ? html`<p class="rail-faction">${swatch(faction)}<strong>${factionName(faction)}</strong></p>`
    : html`<p class="rail-faction muted">${FS.mode === 'spectate' ? L`観戦中` : L`まだ陣営に加わっていません`}</p>`;
  const list = hs.length
    ? html`<ul class="rail-list">${hs.map((h, i) => html`<li><button type="button" class="rail-holding" data-act="holding-pick" data-i="${i}" ${raw(h === active ? 'aria-current="true"' : '')}>
        <span class="rail-tier">${TIERS[h.tier] ?? h.tier}</span>
        <span class="rail-where">${L`州 ${h.p},${h.q} 区画 ${h.site + 1}`}</span>
        ${warned.has(`${h.p},${h.q}`) ? html`<span class="rail-warn">${L`来襲の恐れ`}</span>` : ''}
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
