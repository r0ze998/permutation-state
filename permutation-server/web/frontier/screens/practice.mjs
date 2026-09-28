// Practice mode (web design §7.12; contract §1 "practice mode", §9.5): the
// rules-v10 clash kernel (frontier.wasm `resolve_clash`) run in this
// browser against a small rule bot. Nothing is signed, nothing is sent,
// nothing is earned, and nothing is stored but this browser's UI
// preferences. Available before any wallet (practice.html) and from a clash
// report ("what if": the same inputs and seed with the viewer's own stance
// or retreat ratio changed).
//
// Scenarios: raid a barbarian camp; defend a holding (garrison and walls);
// the stance triangle; the retreat ratio; hex fair share by side (allies
// cannot pool slots: in M1 every faction is its own side); the arrival
// quota (the four largest same-faction arrivals take the slots, the rest
// bounce home with no loss); an unrevealed arrival (not in the clash; routed
// at settlement). Seeds are drawn locally and marked "practice"; "re-roll"
// shows the variance.
//
// The rule bot is deterministic for a seed: it predicts the stance the
// viewer used most in this session and counter-picks it, with a seeded mix
// (web-frontier-practice.test.mjs pins its triangle against the kernel).
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { sha256 } from '../../sdk/sha256.mjs';
import { toHex } from '../../sdk/bytes.mjs';
import { hostId } from '../faddr.mjs';
import { RETREAT_CHOICES, retreatBps } from '../fmarch.mjs';
import { STANCES as STANCE_TEXT, UNITS as UNIT_TEXT, factionName } from '../fi18n.mjs';
import { STANCES } from '../seal.mjs';
import { UNIT_ORDER } from '../fland.mjs';
import { resolveArgs, outcomeRows, renderRows, codeText, campId, NEUTRAL, SITES, STORAGE } from './report.mjs';

// ------------------------------------------------------------------ the practice province
/** Practice coordinates (host ids need a real province; nothing here touches it). */
export const PRACTICE_PROVINCE = Object.freeze({ p: 2, q: 0 });
/** A plain province: every tile open grassland, twelve sites, a camp on a non-site tile. */
export const PRACTICE_TERRAIN = Object.freeze({
  terrain: Object.freeze(Array(61).fill(0)),
  resource: Object.freeze(Array(61).fill(null)),
  sites: Object.freeze([2, 5, 9, 14, 20, 24, 30, 36, 41, 47, 52, 58]),
  siteCount: SITES,
});
export const CAMP_TILE = 33;
export const HOLDING_SITE = 3;
export const HOLDING_TILE = PRACTICE_TERRAIN.sites[HOLDING_SITE];
export const PRACTICE_BELL = 100;
/** The share of troops an unrevealed arrival loses at settlement (host.rs ROUT_LOSS_BPS; pinned by the test). */
export const ROUT_LOSS_BPS = 5_000;
const MILLI = 1000;
const BPS_ONE = 10_000;
const unitIx = name => UNIT_ORDER.indexOf(name);
const id = (site, seq) => hostId({ ...PRACTICE_PROVINCE, site, gen: 0, seq });

// ------------------------------------------------------------------ the rule bot
/** Assault > Flank > Brace > Assault (stance.rs `beats`); Hold is neutral. */
export const BEATS = Object.freeze({ 1: 2, 2: 3, 3: 1 });
/** The stance that beats `s` (Hold has none: null). */
export const counterTo = s => Number(Object.keys(BEATS).find(k => BEATS[k] === s) ?? NaN) || null;

/** A byte stream from a practice seed (sha256 counter mode). */
function stream(seed, label) {
  let block = null, i = 32, n = 0;
  return () => {
    if (i >= 32) { block = sha256('PS-FRONTIER-PRACTICE-BOT', label, seed, Uint8Array.of(n & 0xff, (n >> 8) & 0xff)); n++; i = 0; }
    return block[i++];
  };
}

/**
 * The bot's stance for one of its hosts: counter-pick of the viewer's most
 * used stance so far (Hold when none), else — or with a 30% seeded chance —
 * a stance drawn from Assault, Flank and Brace. Deterministic for (seed,
 * history, k).
 */
export function botStance(seed, history = [], k = 0) {
  const next = stream(seed, `stance${k}`);
  const counts = [0, 0, 0, 0];
  for (const s of history) if (s >= 0 && s <= 3) counts[s]++;
  const top = counts.indexOf(Math.max(...counts));
  const predicted = history.length ? top : 0;
  const counter = counterTo(predicted);
  if (counter !== null && next() >= 77) return counter; // ≈ 70% counter-pick
  return 1 + (next() % 3);
}

// ------------------------------------------------------------------ scenarios
const fighter = (o) => ({ faction: 0, unit: 0, troops: 0, stamina: 100, tile: CAMP_TILE, posture: 0, retreatBps: null, dealtBps: BPS_ONE, ...o });

/** The practice scenarios: id → {controls, build(opts) → {args, notes, displaced?, routed?}}. */
export const SCENARIOS = Object.freeze({
  camp: {
    controls: ['stance', 'retreat', 'troops'], troops: 400,
    build: o => ({
      garrisons: [{ id: campId(0), faction: NEUTRAL, tile: CAMP_TILE, troops: 250 * MILLI, walls: false, posture: 0 }],
      residents: [],
      arrivals: [fighter({ id: id(0, 1), faction: o.you, troops: o.troops * MILLI, posture: o.stance, retreatBps: o.retreat })],
    }),
  },
  defend: {
    controls: ['troops'], troops: 300,
    build: o => ({
      garrisons: [{ id: id(HOLDING_SITE, 0), faction: o.you, tile: HOLDING_TILE, troops: o.troops * MILLI, walls: true, posture: 0 }],
      residents: [],
      arrivals: [
        fighter({ id: id(7, 1), faction: o.bot, unit: unitIx('Horseman'), troops: 350 * MILLI, tile: HOLDING_TILE, posture: o.botStance(0) }),
        fighter({ id: id(7, 2), faction: o.bot, troops: 300 * MILLI, tile: HOLDING_TILE, posture: o.botStance(1) }),
      ],
    }),
  },
  stance: {
    controls: ['stance'], troops: 500,
    build: o => ({
      garrisons: [], residents: [],
      arrivals: [
        fighter({ id: id(0, 1), faction: o.you, troops: 500 * MILLI, posture: o.stance }),
        fighter({ id: id(7, 1), faction: o.bot, troops: 500 * MILLI, posture: o.botStance(0) }),
      ],
    }),
  },
  retreat: {
    controls: ['retreat', 'troops'], troops: 500,
    build: o => ({
      garrisons: [],
      residents: [fighter({ id: id(7, 1), faction: o.bot, unit: unitIx('Pikeman'), troops: 1_200 * MILLI })],
      arrivals: [fighter({ id: id(0, 1), faction: o.you, troops: o.troops * MILLI, posture: o.stance, retreatBps: o.retreat })],
    }),
  },
  fairshare: {
    controls: ['stance'], troops: 300,
    build: o => ({
      garrisons: [], residents: [],
      arrivals: [
        ...[1, 2].map(k => fighter({ id: id(0, k), faction: o.you, troops: 300 * MILLI, posture: o.stance })),
        ...[1, 2, 3, 4, 5].map(k => fighter({ id: id(7, k), faction: o.bot, troops: 300 * MILLI, posture: o.botStance(k) })),
        ...[1, 2].map(k => fighter({ id: id(9, k), faction: o.third, troops: 300 * MILLI, posture: o.botStance(10 + k) })),
      ],
    }),
  },
  quota: {
    controls: ['stance'], troops: 0,
    build: o => {
      // Six arrivals of the viewer's faction: the four largest (by troops at departure) take the four slots.
      const all = [520, 480, 450, 400, 350, 300].map((t, k) => fighter({ id: id(0, k + 1), faction: o.you, troops: t * MILLI, posture: o.stance }));
      const ranked = [...all].sort((a, b) => b.troops - a.troops || (a.id < b.id ? -1 : 1));
      return {
        garrisons: [{ id: campId(0), faction: NEUTRAL, tile: CAMP_TILE, troops: 900 * MILLI, walls: false, posture: 0 }],
        residents: [], arrivals: ranked.slice(0, 4), displaced: ranked.slice(4),
      };
    },
  },
  rout: {
    controls: ['stance'], troops: 300,
    build: o => {
      const unrevealed = fighter({ id: id(0, 2), faction: o.you, troops: 400 * MILLI, posture: o.stance });
      return {
        garrisons: [{ id: campId(0), faction: NEUTRAL, tile: CAMP_TILE, troops: 250 * MILLI, walls: false, posture: 0 }],
        residents: [], arrivals: [fighter({ id: id(0, 1), faction: o.you, troops: 300 * MILLI, posture: o.stance })],
        routed: [{ ...unrevealed, after: Math.floor(unrevealed.troops * (BPS_ONE - ROUT_LOSS_BPS) / BPS_ONE) }],
      };
    },
  },
});
export const SCENARIO_IDS = Object.freeze(Object.keys(SCENARIOS));

/** A practice seed: 32 bytes from `entropy` (bytes) or the browser's random source. */
export function practiceSeed(entropy = null) {
  const e = entropy ?? globalThis.crypto.getRandomValues(new Uint8Array(32));
  return sha256('PS-FRONTIER-PRACTICE', e);
}

/** The default practice state (the viewer's faction when known). */
export function practiceState({ faction = 0, scenario = 'camp' } = {}) {
  const you = Number.isInteger(faction) && faction >= 0 && faction < 6 ? faction : 0;
  return { scenario, you, stance: 1, retreat: 'never', ratio: null, troops: SCENARIOS[scenario].troops, seed: null, history: [], result: null, error: null, whatif: null };
}

/** The kernel input of the state's scenario (and its side notes). */
export function scenarioArgs(st, seed) {
  const sc = SCENARIOS[st.scenario];
  if (!sc) throw new Error(`no scenario ${st.scenario}`);
  const retreat = st.retreat === 'never' ? null : retreatBps(st.retreat, st.ratio);
  const troops = Math.max(100, Math.min(30_000, Math.trunc(Number(st.troops) || sc.troops || 100)));
  const o = { you: st.you, bot: (st.you + 1) % 6, third: (st.you + 2) % 6, stance: st.stance, retreat, troops, botStance: k => botStance(seed, st.history, k) };
  const built = sc.build(o);
  const args = {
    province: { ...PRACTICE_PROVINCE }, bell: PRACTICE_BELL, seed: Uint8Array.from(seed),
    terrain: { terrain: [...PRACTICE_TERRAIN.terrain], resource: [...PRACTICE_TERRAIN.resource], sites: [...PRACTICE_TERRAIN.sites], siteCount: PRACTICE_TERRAIN.siteCount },
    residents: built.residents, garrisons: built.garrisons, arrivals: built.arrivals,
    relations: 0n, occupancy: { pending: [0, 0, 0, 0, 0, 0, 0, 0], storageFree: STORAGE },
  };
  return { args, displaced: built.displaced ?? [], routed: built.routed ?? [] };
}

/**
 * Run the state's scenario in the kernel with `seed` (32 bytes): `{ok: true,
 * args, outcome, digest, displaced, routed, seedHex}` or `{ok: false, why}`.
 * The viewer's stance joins the bot's history (it adapts next time).
 */
export function runScenario(kernel, st, seed) {
  const { args, displaced, routed } = scenarioArgs(st, seed);
  const r = resolveArgs(kernel, args);
  if (!r.ok) return r;
  return { ok: true, args, outcome: r.outcome, digest: r.digest, displaced, routed, seedHex: toHex(seed) };
}

/**
 * "What if" on a verified report: the report's kernel input with the
 * viewer's own arrivals (`mine(id)`) given `stance` and `retreat` (bps or
 * null), resolved with the report's seed. `{ok, base: {outcome}, alt: {args,
 * outcome, digest}, changed: [ids whose fate or troops differ]}`.
 */
export function whatIf(kernel, { args, outcome }, mine, { stance, retreat }) {
  const alt = { ...args, arrivals: args.arrivals.map(f => (mine(f.id) ? { ...f, posture: stance, retreatBps: retreat } : f)) };
  const r = resolveArgs(kernel, alt);
  if (!r.ok) return r;
  const key = f => `${f.arrival ? 'a' : 'r'}${f.id}`;
  const before = new Map(outcome.fighters.map(f => [key(f), f]));
  const changed = r.outcome.fighters.filter(f => { const b = before.get(key(f)); return !b || b.fate.kind !== f.fate.kind || b.troops !== f.troops; }).map(f => String(f.id));
  return { ok: true, alt: { args: alt, outcome: r.outcome, digest: r.digest }, changed };
}

// ------------------------------------------------------------------ rendering
const SCENARIO_TEXT = {
  camp: () => L`蛮族の野営地を襲う`,
  defend: () => L`拠点を守る（守備隊と城壁）`,
  stance: () => L`構えの三すくみ`,
  retreat: () => L`撤退比`,
  fairshare: () => L`マスの公平な割り当て`,
  quota: () => L`到着枠（同じ勢力の大きい4軍）`,
  rout: () => L`開封されない到着は敗走する`,
};
const SCENARIO_NOTE = {
  camp: () => L`野営地の守り手は中立です。構えと撤退比を選んで襲います。`,
  defend: () => L`あなたの拠点に敵の2軍が到着します。守備隊は待機の構えで戦い、城壁とともに反撃します。守備隊の兵数を選べます。`,
  stance: () => L`同じ兵数の2軍が同じマスに到着します。突撃は側撃に、側撃は迎撃に、迎撃は突撃に強く、勝つ側は与える損害が2割増えます。`,
  retreat: () => L`大軍が守るマスへ到着します。撤退比を超える守り手がいれば、戦わずに損失なしで引き返します。守り手は鐘の始まりの顔ぶれで数えます。`,
  fairshare: () => L`3つの勢力の9軍が1つのマスに来ます。1マスには6軍までで、どの勢力にも公平に枠が割り当てられます（同盟でも枠は合わせられません）。`,
  quota: () => L`同じ勢力の6軍が同じ州と鐘に到着します。出発時の兵が多い4軍が到着枠に入り、残りは損失なしで押し戻されます。`,
  rout: () => L`開封されなかった到着は衝突に加わらず、精算で敗走します（兵と体力とチップの半分を失います）。チップはキーパーに開封してもらうためのものです。`,
};
const RETREAT_TEXT = {
  never: () => L`撤退しない`,
  x2: () => L`守り手が自軍の2倍を超えたら撤退`,
  'x1.5': () => L`守り手が自軍の1.5倍を超えたら撤退`,
  x1: () => L`守り手が自軍を超えたら撤退`,
  'x0.5': () => L`守り手が自軍の半分を超えたら撤退`,
};

/** The practice panel: `st` = the practice state; `whatif` shows the report's variant; `closable` on the game page. */
export function render(st, { kernelError = null, closable = false } = {}) {
  if (!st) return '';
  if (st.whatif) return renderWhatIf(st, kernelError);
  const sc = SCENARIOS[st.scenario];
  const controls = new Set(sc.controls);
  const r = st.result;
  return html`<section aria-labelledby="practice-title"><h3 id="practice-title">${L`練習モード`}</h3>
    ${closable ? html`<button type="button" class="btn small" data-act="practice-close">${L`閉じる`}</button>` : ''}
    <p class="muted">${L`練習はこのブラウザの中だけで動き、何も送らず、何も得ません。結果はこのブラウザの外には保存されません。`}</p>
    <div role="group" aria-label="${L`練習の場面`}"><ul class="cards">${SCENARIO_IDS.map(s => html`<li><button type="button" class="card" data-act="practice-scenario" data-scenario="${s}" aria-pressed="${s === st.scenario ? 'true' : 'false'}">${SCENARIO_TEXT[s]()}</button></li>`)}</ul></div>
    <p>${SCENARIO_NOTE[st.scenario]()}</p>
    <form data-form="practice-run">
      ${controls.has('stance') ? html`<label class="field">${L`あなたの構え`}<select name="stance" data-bind="pr-stance">${STANCES.map((s, i) => html`<option value="${i}" ${raw(i === st.stance ? 'selected' : '')}>${STANCE_TEXT[s]}</option>`)}</select></label>` : ''}
      ${controls.has('retreat') ? html`<label class="field">${L`撤退比`}<select name="retreat" data-bind="pr-retreat">${RETREAT_CHOICES.filter(c => c.id !== 'custom').map(c => html`<option value="${c.id}" ${raw(c.id === st.retreat ? 'selected' : '')}>${RETREAT_TEXT[c.id]()}</option>`)}</select></label>` : ''}
      ${controls.has('troops') ? html`<label class="field">${st.scenario === 'defend' ? L`守備隊の兵数` : L`あなたの兵数`}<input name="troops" type="number" inputmode="numeric" min="100" max="30000" step="1" value="${st.troops}" data-bind="pr-troops"></label>` : ''}
      <p><button type="submit" class="btn primary">${L`衝突を試す`}</button> <button type="button" class="btn" data-act="practice-reroll" ${raw(r?.ok ? '' : 'disabled')}>${L`乱数を引き直す`}</button></p>
    </form>
    ${kernelError ? html`<p class="notice error" role="alert">${L`ルールのモジュール（frontier.wasm）を読み込めないため練習できません（${kernelError}）`}</p>` : ''}
    ${r ? renderResult(st, r) : ''}
  </section>`;
}

function renderResult(st, r) {
  if (!r.ok) return html`<p class="notice error" role="alert">${L`ルールがこの場面を受け付けませんでした（${codeText(r.why)}）`}</p>`;
  const mine = x => r.args.arrivals.concat(r.args.residents).some(f => f.id === BigInt(x) && f.faction === st.you) || r.args.garrisons.some(g => g.id === BigInt(x) && g.faction === st.you);
  const bot = r.args.arrivals.filter(f => f.faction !== st.you && f.faction !== NEUTRAL).map(f => STANCE_TEXT[STANCES[f.posture]]);
  return html`<div role="status" aria-live="polite"><h4>${L`練習の結果`}</h4>
    ${renderRows(outcomeRows(r.args, r.outcome, mine), L`練習の結果`)}
    ${bot.length ? html`<p class="muted">${L`相手のボットの構え：${bot.join(' / ')}`}</p>` : ''}
    ${r.displaced.length ? html`<p>${L`到着枠に入れず押し戻された軍勢（損失なし）：${r.displaced.map(f => fmtNum(Math.floor(f.troops / MILLI))).join(' / ')}`}</p>` : ''}
    ${r.routed.map(f => html`<p class="warn">${L`開封されなかった到着：兵 ${fmtNum(Math.floor(f.troops / MILLI))} → ${fmtNum(Math.floor(f.after / MILLI))}、体力 0、チップは戻りません`}</p>`)}
    <p class="muted">${L`交戦 ${fmtNum(r.outcome.engagements)} · 練習用の乱数 ${r.seedHex.slice(0, 12)}…（本番の結果ではありません）`}</p></div>`;
}

function renderWhatIf(st, kernelError) {
  const w = st.whatif;
  const title = L`もしも：州 ${w.p},${w.q} · 第${fmtNum(w.bell)}鐘`;
  return html`<section aria-labelledby="practice-title"><h3 id="practice-title">${title}</h3>
    <p class="muted">${L`実際の衝突と同じ入力と乱数で、あなたの到着軍勢の構えと撤退比だけを変えて計算します。チェーンには何も送りません。`}</p>
    <form data-form="practice-whatif">
      <label class="field">${L`あなたの構え`}<select name="stance" data-bind="pr-stance">${STANCES.map((s, i) => html`<option value="${i}" ${raw(i === st.stance ? 'selected' : '')}>${STANCE_TEXT[s]}</option>`)}</select></label>
      <label class="field">${L`撤退比`}<select name="retreat" data-bind="pr-retreat">${RETREAT_CHOICES.filter(c => c.id !== 'custom').map(c => html`<option value="${c.id}" ${raw(c.id === st.retreat ? 'selected' : '')}>${RETREAT_TEXT[c.id]()}</option>`)}</select></label>
      <p><button type="submit" class="btn primary">${L`もしもを計算する`}</button> <button type="button" class="btn" data-act="practice-close">${L`報告に戻る`}</button></p>
    </form>
    ${kernelError ? html`<p class="notice error" role="alert">${L`ルールのモジュール（frontier.wasm）を読み込めないため練習できません（${kernelError}）`}</p>` : ''}
    ${w.mineCount === 0 ? html`<p class="muted">${L`この衝突にあなたの到着軍勢はいません。`}</p>` : ''}
    ${w.result?.ok ? html`<div role="status" aria-live="polite"><h4>${L`実際`}</h4>${renderRows(outcomeRows(w.args, w.outcome, w.mine), L`実際`)}
      <h4>${L`もしも`}</h4>${renderRows(outcomeRows(w.result.alt.args, w.result.alt.outcome, w.mine), L`もしも`)}
      <p>${w.result.changed.length ? L`結末か兵数が変わった軍勢：${w.result.changed.length}` : L`この変更では結果は変わりません`}</p></div>` : ''}
    ${w.result && !w.result.ok ? html`<p class="notice error" role="alert">${L`ルールがこの場面を受け付けませんでした（${codeText(w.result.why)}）`}</p>` : ''}
  </section>`;
}

/** Faction label of the viewer in practice (for the page's intro line). */
export const youText = st => L`あなたの勢力：${factionName(st.you)} · ${UNIT_TEXT.Spearman}`;
