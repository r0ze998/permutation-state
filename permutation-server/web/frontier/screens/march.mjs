// The march composer (web design §7.6; contract §5.11, §8.3, §9.4, I-08,
// I-27, I-32): destination (sealed: only this browser knows it), the path
// from the WASM planner, the arrival bell, the stance, the retreat ratio
// from a list whose first choice is "never" (0), exactly the three tip
// presets (no zero tip), the costs, and the four-step send indicator
// (sealing, saved on this device, sent, on chain).
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { STANCES as STANCE_TEXT, UNITS, clientText, failureText } from '../fi18n.mjs';
import { STANCES } from '../seal.mjs';
import { RETREAT_CHOICES, tipOptions, marchCosts, arrivalWindow, checkMarch, SEND_STEPS, DEPART_STAMINA } from '../fmarch.mjs';
import { UNIT_ORDER } from '../fland.mjs';
import { lamports } from './shell.mjs';

const RETREAT_TEXT = {
  never: () => L`撤退しない`,
  x2: () => L`守り手が自軍の2倍を超えたら撤退`,
  'x1.5': () => L`守り手が自軍の1.5倍を超えたら撤退`,
  x1: () => L`守り手が自軍を超えたら撤退`,
  'x0.5': () => L`守り手が自軍の半分を超えたら撤退`,
  custom: () => L`倍率を指定する`,
};
const TIP_TEXT = { standard: () => L`標準`, decisive: () => L`確実`, maximum: () => L`最大` };
const STEP_TEXT = { sealing: () => L`封をしています`, saved: () => L`この端末に保存`, sent: () => L`送信`, onChain: () => L`チェーンに記録` };
const STANCE_NOTE = { Hold: () => L`相性の効果なし`, Assault: () => L`側撃に強い`, Flank: () => L`迎撃に強い`, Brace: () => L`突撃に強い` };

/** The composer's choices as the markup offers them (the tests read these). */

/** The route line with singular and plural forms (wave-5 review: "1 provinces"). */
export function routeLine(route) {
  const n = route.hexes;
  const p = route.provinces.length;
  const m = Math.ceil(route.secs / 60);
  if (n === 1 && p === 1) return L`1 マス · 1 州 · 約 ${m} 分`;
  if (p === 1) return L`${n} マス · 1 州 · 約 ${m} 分`;
  return L`${n} マス · ${p} 州 · 約 ${m} 分`;
}

export function composerChoices(FS) {
  const season = FS.season;
  return {
    retreat: RETREAT_CHOICES.map(c => ({ id: c.id, bps: c.bps, text: RETREAT_TEXT[c.id]() })),
    tips: season ? tipOptions(season).map(o => ({ ...o, text: TIP_TEXT[o.id]() })) : [],
    stances: STANCES.map((s, i) => ({ value: i, id: s, text: STANCE_TEXT[s] })),
  };
}

/** The march under composition as checkMarch reads it. */
export function composerMarch(FS) {
  const c = FS.compose ?? {};
  const h = FS.holdings?.[0];
  const env = h ? FS.provinces?.get(`${h.p},${h.q}`) : null;
  const host = c.host ?? null;
  return {
    season: FS.season, nowBell: FS.nowBell ?? 0, resolvedNext: env?.province.resolvedNext ?? null, province: env?.province ?? null, now: FS.chain?.now() ?? 0, holding: h ?? null, host,
    dest: c.dest ?? null, route: c.route ?? null, earliest: c.earliest ?? null, arriveBell: c.arriveBell ?? null,
    stance: c.stance ?? 0, retreat: { choice: c.retreat ?? 'never', ratio: c.ratio ?? null }, tip: c.tip ?? null,
  };
}

export function render(FS) {
  const c = FS.compose;
  if (!c?.host) return html`<p class="muted">${L`「軍勢」で軍勢の「進軍」を押すと、ここで行き先と命令を決めます。`}</p>`;
  const m = composerMarch(FS);
  const ch = composerChoices(FS);
  const w = arrivalWindow(FS.season, m.nowBell, m.earliest);
  const bells = [];
  for (let b = w.min; b <= w.max; b++) bells.push(b);
  const problems = checkMarch(m);
  const costs = m.tip ? marchCosts(FS.season, m.tip) : null;
  const unit = UNITS[UNIT_ORDER[m.host.unit]];
  const busy = !!c.sending;
  return html`<section aria-labelledby="march-title"><h3 id="march-title">${L`進軍：${unit} ${fmtNum(m.host.troops)}`}</h3>
    <fieldset><legend>${L`1. 行き先`}</legend>
      <p class="sealed"><span class="lock" aria-hidden="true"></span>${L`行き先は封の中にあり、ほかの人には到着の鐘しか見えません（到着の鐘が始まるまで）。`}</p>
      ${c.dest ? html`<p><strong>${L`州 ${c.dest.p},${c.dest.q} のマス ${c.dest.tile}`}</strong></p>` : ''}
      <button type="button" class="btn" data-act="dest-from-map" ${raw(FS.selected?.idx !== undefined ? '' : 'disabled')}>${L`地図で選んだマスにする`}</button>
      ${(c.quick ?? []).length ? html`<ul class="list">${c.quick.map(q => html`<li><button type="button" class="btn" data-act="dest-quick" data-p="${q.p}" data-q="${q.q}" data-tile="${q.tile}">${q.kind === 'camp' ? L`蛮族の野営地 州 ${q.p},${q.q}` : L`州 ${q.p},${q.q} のマス ${q.tile}`}</button></li>`)}</ul>` : ''}
      <form class="inline" data-form="dest"><label>P<input name="p" type="number" inputmode="numeric" value="${c.dest?.p ?? ''}"></label><label>Q<input name="q" type="number" inputmode="numeric" value="${c.dest?.q ?? ''}"></label>
        <label>${L`マス`}<input name="tile" type="number" min="0" max="60" inputmode="numeric" value="${c.dest?.tile ?? ''}"></label><button type="submit" class="btn">${L`決める`}</button></form>
    </fieldset>
    <fieldset><legend>${L`2. 道のりと到着`}</legend>
      ${c.route ? html`<p>${routeLine(c.route)}</p>` : html`<p class="muted">${c.routeError ? clientText(c.routeError) : L`行き先を決めると道のりを探します`}</p>`}
      <label>${L`到着の鐘`}<select data-bind="arriveBell">${bells.map(b => html`<option value="${b}" ${raw(b === m.arriveBell ? 'selected' : '')}>${L`第${fmtNum(b)}鐘`}</option>`)}</select></label>
      <p class="muted">${L`出発から早くても2鐘あと、遅くとも72鐘あと。出発には体力 ${DEPART_STAMINA} を使います。`}</p>
    </fieldset>
    <fieldset><legend>${L`3. 命令`}</legend>
      <div role="radiogroup" aria-label="${L`構え`}">${ch.stances.map(s => html`<label class="choice"><input type="radio" name="stance" data-bind="stance" value="${s.value}" ${raw(s.value === m.stance ? 'checked' : '')}>${s.text} <span class="muted">${STANCE_NOTE[s.id]()}</span></label>`)}</div>
      <p class="muted">${L`突撃は側撃に、側撃は迎撃に、迎撃は突撃に強く、強い側は与える損害が 20% 増えます。M1 の守り手はいつも待機です。`}</p>
      <label>${L`撤退比`}<select data-bind="retreat">${ch.retreat.map(r => html`<option value="${r.id}" ${raw(r.id === m.retreat.choice ? 'selected' : '')}>${r.text}</option>`)}</select></label>
      ${m.retreat.choice === 'custom' ? html`<label>${L`倍率（守り手 ÷ 自軍、0.0001〜6）`}<input data-bind="ratio" type="number" step="0.05" min="0.0001" max="6" value="${m.retreat.ratio ?? ''}"></label>` : ''}
      <p class="muted">${L`到着の鐘が始まった時点で固まった守り手と比べます。`}</p>
    </fieldset>
    <fieldset><legend>${L`4. チップと費用`}</legend>
      <div role="radiogroup" aria-label="${L`チップ`}">${ch.tips.map(t => html`<label class="choice"><input type="radio" name="tip" data-bind="tip" value="${String(t.lamports)}" ${raw(String(t.lamports) === String(m.tip) ? 'checked' : '')}>${t.text} · ${lamports(t.lamports)} <span class="muted">${L`優先度 ${(Number(t.priorityMilli) / 1000).toFixed(3)}`}</span></label>`)}</div>
      <p class="muted">${L`チップは封を開けたキーパーへ支払われます。タブを閉じても、ビーコンが記録されればキーパーが開封できます。`}</p>
      ${costs ? html`<dl class="facts"><div class="row"><dt>${L`チップ`}</dt><dd>${lamports(costs.tip)}</dd></div><div class="row"><dt>${L`進軍の手数料（決着させた人へ）`}</dt><dd>${lamports(costs.marchFee)}</dd></div>
        <div class="row"><dt>${L`封の保証金（精算で戻る）`}</dt><dd>${lamports(costs.sealBond)}</dd></div><div class="row"><dt>${L`合計`}</dt><dd>${lamports(costs.total)}</dd></div></dl>
        <p class="muted">${L`M1 ではテスト用の SOL を中継が立て替えます。価値はありません。`}</p>` : ''}
    </fieldset>
    ${problems.length ? html`<ul class="problems">${problems.map(p => html`<li>${failureText({ code: p })}</li>`)}</ul>` : ''}
    <ol class="steps" aria-label="${L`送信の段階`}">${SEND_STEPS.map(s => html`<li class="${c.step === s ? 'now' : SEND_STEPS.indexOf(s) < SEND_STEPS.indexOf(c.step) ? 'done' : ''}">${STEP_TEXT[s]()}</li>`)}</ol>
    <button type="button" class="btn primary" data-act="march-send" ${raw(problems.length || busy ? 'disabled' : '')}>${L`封をして出発する`}</button>
    <button type="button" class="btn" data-act="compose-close">${L`やめる`}</button>
  </section>`;
}
