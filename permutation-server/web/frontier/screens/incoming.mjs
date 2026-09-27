// Incoming-arrival warnings (web design §7.7): departures are public
// (origin and arrival bell, never the destination), so for each hostile
// departure that could reach one of the viewer's holdings by its arrival
// bell the panel says "up to N hosts (M troops) may arrive at bell b;
// destinations unknown". The reach test is the kernel's `reachable`
// (optimistic: a "no" is certain, a "yes" is only a warning); without the
// rules module the panel says the warnings are unavailable.
import { html } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';

/** One warning line. */
export const warningText = w => L`第${fmtNum(w.bell)}鐘に、州 ${w.holding.p},${w.holding.q} の拠点へ最大 ${w.hosts} の軍勢（${fmtNum(w.troops)} 兵）が来るかもしれません。行き先はわかりません。`;

export function render(FS) {
  const w = FS.incoming;
  if (w === null || w === undefined) return html`<p class="muted">${L`来襲の見込みを出すにはルールのモジュールが必要です。`}</p>`;
  if (!w.length) return html`<p class="muted">${L`いまのところ、あなたの拠点に届きうる他勢力の出発はありません。`}</p>`;
  return html`<section aria-labelledby="incoming-title"><h3 id="incoming-title">${L`来襲の見込み`}</h3>
    <ul class="list incoming">${w.map(x => html`<li class="warn">${warningText(x)}</li>`)}</ul></section>`;
}
