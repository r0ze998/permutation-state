// The march tracker (web design §7.6, §8.4; contract §5.11, §9.1, I-24,
// I-44): per march — departed, arrival bell, beacon anchored, revealed (by
// this browser or a keeper), clash resolved, settled — with the secret
// destination shown only to this browser ("only you can see this"), the
// lock on the host until the transit is settled (HostInTransit), the rout
// warning when the window closed without a reveal, and the settle button
// once settlement is due (keepers usually settle first).
import { html } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { STANCES as STANCE_TEXT, TRANSIT_OUTCOME_TEXT, failureText } from '../fi18n.mjs';
import { STANCES, unpack } from '../seal.mjs';
import { materialBytes } from '../marchbook.mjs';
import { tracker } from '../fmarch.mjs';
import { lockedText } from './host.mjs';

const STEP_TEXT = {
  departed: () => L`出発`,
  arrivalBell: () => L`到着の鐘`,
  anchored: () => L`ビーコンが記録された（誰でも封を開けられる）`,
  revealed: () => L`開封された`,
  resolved: () => L`決着`,
  settled: () => L`精算`,
};
const HINT_TEXT = {
  keepersWillReveal: () => L`この端末には封の記録がありません。キーパーがビーコンの後で開封します。`,
  sealMismatch: () => L`チェーン上の封がこの端末で作った封と違います。この封は開封しません。`,
  sealed: () => L`封をしました（まだ送っていません）`,
  sent: () => L`送信しました。記録を待っています`,
  landed: () => L`出発しました`,
  revealing: () => L`この端末が開封を送りました`,
  revealed: () => L`開封されました`,
  settled: () => L`精算されました`,
  failed: () => L`送れませんでした`,
};

/** The private line of a march this browser sealed: destination and orders (never sent before the arrival bell). */
export function privateLine(entry) {
  if (!entry) return null;
  const p = unpack(materialBytes(entry).plain);
  return L`州 ${p.destP},${p.destQ} のマス ${p.destTile + 1} · ${STANCE_TEXT[STANCES[p.stance]] ?? ''}`;
}

/** One march card: `m = {entry, transit, facts: {nowBell, pipeline, slotPresent, settled, settleReady}}`. */
export function renderMarch(m) {
  const t = tracker({ entry: m.entry, transit: m.transit, ...m.facts });
  const secret = privateLine(m.entry);
  const host = String(m.entry?.host ?? m.transit?.hostId ?? '');
  return html`<li class="march">
    <p><strong>${L`第${fmtNum(t.arriveBell ?? 0)}鐘に到着`}</strong>${secret ? html` · <span class="secret">${secret}</span> <span class="muted">${L`（あなたにだけ見えます）`}</span>` : ''}</p>
    <ol class="steps">${t.steps.map(s => html`<li class="${s.state}">${STEP_TEXT[s.id]()}</li>`)}</ol>
    <p class="muted">${t.route === 'self' ? L`到着の鐘の始まりにこの端末が開封を送ります（署名なし）。` : HINT_TEXT[t.hint]?.() ?? ''}</p>
    ${t.closedUnrevealed ? html`<p class="warn">${L`開封の受付が開封なしで閉じました。精算で敗走（兵・体力・チップの半分）になるか、到着枠に入れず損失なしで戻ります。`}</p>` : ''}
    ${t.outcome ? html`<p>${TRANSIT_OUTCOME_TEXT[t.outcome] ?? t.outcome}</p>` : ''}
    ${t.locked ? html`<p class="locked-note" role="note">${lockedText()}</p>` : ''}
    ${m.entry?.failure ? html`<p class="notice error">${failureText({ code: m.entry.failure })}</p>` : ''}
    ${m.facts?.settleReady ? html`<button type="button" class="btn" data-act="settle-transit" data-host="${host}" data-slot="${m.entry?.transitSlot ?? m.transit?.slot ?? ''}">${L`精算する`}</button>` : ''}
  </li>`;
}

export function render(FS) {
  const list = FS.marches ?? [];
  if (!list.length) return html`<p class="muted">${L`進軍中の軍勢はいません`}</p>`;
  return html`<section aria-labelledby="tracker-title"><h3 id="tracker-title">${L`進軍`}</h3><ul class="list marches">${list.map(renderMarch)}</ul></section>`;
}

/** Whether the viewer's march list changed state (for the bell chip's dot and the polite announcement). */
export const stateKey = list => (list ?? []).map(m => `${m.entry?.host ?? m.transit?.hostId}:${m.entry?.state}:${m.transit?.state}:${m.facts?.pipeline}`).join('|');
