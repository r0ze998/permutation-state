// The first impression (design session: "no impact, no world when the game
// opens"): a title card over the living map the first time a device opens
// the Frontier (and on demand: More → "show the title", ?intro=1 for the
// demo). The camera drifts in from the mist toward the Concord while the
// emblem, the title, the six leaders and three lines of the world appear;
// the live line says the season is real ("Season 7 · bell 1,034 · 1,055
// holdings"). One button enters (the game page) or starts watching (the
// spectator page); Escape, Enter or the button close it. Nothing behind it
// waits for it: the page loads underneath.
//
// Also here: the bell toll — when a bell turns, its number rings across the
// top of the map for a moment (no sound, no block), so the game's clock is
// felt, not only read.
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { factionName } from '../fi18n.mjs';
import { FACTION_FILL, FACTION_DARK, sigilPath } from '../people/avatar.mjs';
import { leaderSvg, LEADERS } from '../people/leaders.mjs';
import { lang } from '../../lang.mjs';

export const INTRO_KEY = 'ps-fintro:v1';
/** The leaders appear one by one (ms apart) after the title. */
export const LEADER_STAGGER_MS = 220;

/** Whether the title should open: never seen on this device, or asked for. */
export function shouldOpen({ storage, search = '' } = {}) {
  const q = new URLSearchParams(search);
  if (q.get('intro') === '1') return true;
  if (q.get('intro') === '0') return false;
  try { return !storage?.getItem(INTRO_KEY); } catch { return false; }
}
export function markSeen(storage) { try { storage?.setItem(INTRO_KEY, '1'); } catch { /* private mode */ } }

/**
 * The Frontier's emblem: the six sigils on a ring around the bell (SVG).
 * `size` px; decorative unless `title` is given.
 */
export function emblemSvg({ size = 96, title = null } = {}) {
  const a11y = title ? `role="img" aria-label="${String(title).replace(/[<>&"]/g, '')}"` : 'aria-hidden="true" focusable="false"';
  const ring = Array.from({ length: 6 }, (_, f) => {
    const a = -Math.PI / 2 + (f * Math.PI) / 3, x = 50 + Math.cos(a) * 34, y = 50 + Math.sin(a) * 34;
    return `<g transform="translate(${x.toFixed(2)} ${y.toFixed(2)})"><circle r="9.5" fill="#fffaf0" stroke="${FACTION_DARK[f]}" stroke-width="1.8"/><path d="${sigilPath(f, 5.4)}" fill="${FACTION_FILL[f]}"/></g>`;
  }).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="${size}" height="${size}" class="emblem" ${a11y}>
    <circle cx="50" cy="50" r="46" fill="none" stroke="#d9b44a" stroke-width="1.4" opacity=".85"/>
    <circle cx="50" cy="50" r="34" fill="none" stroke="#d9b44a" stroke-width="0.8" stroke-dasharray="2 3" opacity=".7"/>
    <path d="M50 30 C41 30 38 38 38 46 L37 56 L33 61 L67 61 L63 56 L62 46 C62 38 59 30 50 30 Z" fill="#d9b44a" stroke="#7a5a14" stroke-width="1.4"/>
    <path d="M44 62 C44 66 56 66 56 62 Z" fill="#7a5a14"/><circle cx="50" cy="27.5" r="2.6" fill="#7a5a14"/>
    ${ring}
  </svg>`;
}

/** The live line: season, bell, holdings and lords (from what the page already has). */
export function liveLine({ seasonId = null, bell = null, holdings = 0, factions = 6 } = {}) {
  const parts = [];
  if (seasonId !== null && seasonId !== undefined) parts.push(L`第${fmtNum(Number(seasonId))}季`);
  if (Number.isInteger(bell)) parts.push(L`第${fmtNum(bell)}鐘`);
  if (holdings > 0) parts.push(L`${fmtNum(holdings)} の拠点`);
  parts.push(L`${fmtNum(factions)} つの勢力`);
  return parts.join(' · ');
}

/** The title card's markup. `mode` 'play' | 'spectate' | 'practice'. */
export function render({ mode = 'play', live = '' } = {}) {
  const cta = mode === 'spectate' ? L`観戦をはじめる` : mode === 'practice' ? L`練習をはじめる` : L`辺境へ入る`;
  const leaders = LEADERS.map((l, i) => html`<li class="intro-leader intro-leader-${i}">${raw(leaderSvg(l.faction, { size: 64 }))}<span>${factionName(l.faction)}</span></li>`);
  return html`<div class="intro-veil" aria-hidden="true"></div>
    <div class="intro-card">
      <div class="intro-emblem">${raw(emblemSvg({ size: 108 }))}</div>
      <h2 class="intro-title" id="intro-title" data-name>Wylls</h2>
      <p class="intro-tagline">${L`六つの勢力。十分ごとの鐘。封じられた進軍。`}</p>
      <ol class="intro-leaders">${leaders}</ol>
      <div class="intro-lore">
        <p>${L`大協約の地を中心に、六つの勢力が霧の辺境へ広がっていく。`}</p>
        <p>${L`十分ごとに鐘が鳴る。そのあいだに出された進軍はすべて封をされ、行き先は誰にもわからない。`}</p>
        <p>${L`鐘が鳴ると、同じ州に着いた軍勢がいっせいにぶつかる。人が増えれば、霧の向こうに新しい輪がひらく。`}</p>
      </div>
      ${live ? html`<p class="intro-live"><span class="intro-dot" aria-hidden="true"></span>${live}</p>` : ''}
      <button type="button" class="btn primary intro-go" data-act="intro-close">${cta}</button>
    </div>`;
}

/** The camera's drift during the title: from far over the Concord to the fitted view (`k` 0..1, eased). */
export function dolly(target, k) {
  const e = 1 - Math.pow(1 - Math.max(0, Math.min(1, k)), 3);
  const from = { x: 0, y: 0, zoom: Math.max(0.02, target.zoom * 0.35) };
  return { x: from.x + (target.x - from.x) * e, y: from.y + (target.y - from.y) * e, zoom: from.zoom * Math.pow(target.zoom / from.zoom, e) };
}

/** The bell toll's text. */
export const tollText = bell => L`第${fmtNum(bell)}鐘`;
