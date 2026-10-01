// The host panel (web design §7.5; contract §5.10–§5.11, I-44): each host
// of the viewer's holding with its unit, troops, stamina (cap 120, +1 per
// bell), cooldown and pending change; a host whose march is not settled is
// locked (HostInTransit) with the reason in words; March, Explore (Scouts)
// and Dissolve start from here.
import { activeHolding } from '../fstate.mjs';
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { UNITS, errorText } from '../fi18n.mjs';
import { hostsIn, UNIT_ORDER, SCOUT, actionBlocks } from '../fland.mjs';
import { RULES, DEPART_STAMINA } from '../fmarch.mjs';

/** Every host of the viewer's first holding in the provinces the page holds (home and destinations). */
export function hostRows(FS) {
  const h = activeHolding(FS);
  if (!h) return [];
  const out = [];
  for (const env of FS.provinces?.values() ?? []) {
    for (const row of hostsIn(env.province, h, FS.nowBell ?? 0)) out.push({ ...row, p: env.province.p, q: env.province.q, province: env.province });
  }
  return out;
}

const STATE_TEXT = { 1: () => L`顔ぶれにいる`, 2: () => L`編成中（次の鐘から）`, 3: () => L`出発した` };
const PENDING_TEXT = { 0: () => '', 1: () => L`出発の支払い待ち`, 2: () => L`分割待ち`, 3: () => L`合流待ち`, 4: () => L`合流される`, 5: () => L`解散待ち（決着のあと控えに戻る）`, 6: () => L`不正な封で失われる` };

/** The locked host's line (I-44). */
export const lockedText = () => L`この軍勢は進軍の精算が済むまで出発・解散・探索できません（キーパーが精算します）`;

export function render(FS) {
  const rows = hostRows(FS);
  if (!rows.length) return html`<p>${L`軍勢はいません。拠点で兵を訓練し、軍勢を編成してください。`}</p>`;
  return html`<section aria-labelledby="hosts-title"><h3 id="hosts-title">${L`軍勢`}</h3><ul class="list hosts">${rows.map(r => {
    const blocks = actionBlocks('Depart', { holding: activeHolding(FS), province: r.province, nowBell: FS.nowBell ?? 0, now: FS.chain?.now() ?? 0, host: r });
    const cool = (FS.nowBell ?? 0) < r.readyBell;
    return html`<li class="host${r.inTransit ? ' locked' : ''}" data-unit="${r.unit}">
      <strong>${UNITS[UNIT_ORDER[r.unit]]} ${fmtNum(r.troops)}</strong>
      <span>${L`州 ${r.p},${r.q} · ${STATE_TEXT[r.state]?.() ?? ''}`}</span>
      <span>${L`体力 ${r.stamina}/${RULES.STAMINA_CAP}`}${cool ? L`（第${fmtNum(r.readyBell)}鐘まで休息）` : ''}</span>
      ${r.pending ? html`<span class="muted">${PENDING_TEXT[r.pending]?.() ?? ''}</span>` : ''}
      ${r.inTransit ? html`<p class="locked-note" role="note">${lockedText()}</p>` : ''}
      <div class="actions">
        <button type="button" class="btn" data-act="compose" data-host="${String(r.id)}" ${raw(blocks.length || r.stamina < DEPART_STAMINA ? 'disabled' : '')}>${L`進軍させる`}</button>
        ${r.unit === SCOUT ? html`<button type="button" class="btn" data-act="explore-open" data-host="${String(r.id)}" ${raw(actionBlocks('Explore', { holding: activeHolding(FS), province: r.province, nowBell: FS.nowBell ?? 0, now: FS.chain?.now() ?? 0, host: r }).length ? 'disabled' : '')}>${L`探索`}</button>` : ''}
        <button type="button" class="btn" data-act="dissolve" data-host="${String(r.id)}" ${raw(actionBlocks('Dissolve', { holding: activeHolding(FS), province: r.province, nowBell: FS.nowBell ?? 0, now: FS.chain?.now() ?? 0, host: r }).length ? 'disabled' : '')}>${L`解散`}</button>
      </div>
      ${blocks.length ? html`<p class="blocked">${blocks.map(errorText).join(' / ')}</p>` : ''}
    </li>`;
  })}</ul>
  <p class="muted">${L`出発には行き先にかかわらず体力 ${DEPART_STAMINA} を使います（道のりは封の中にあるため）。`}</p></section>`;
}
