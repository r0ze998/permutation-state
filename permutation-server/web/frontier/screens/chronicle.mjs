// The chronicle (web design §7.10): the season's public record as the
// herald's event pages carry it — ring and province openings, settled
// sites, departures (origin and arrival bell only), reveals, clashes,
// settlements, explorations — decoded by this page from the raw PS2
// bodies (flog.mjs); the viewer's own lines are marked.
import { html } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { TRANSIT_OUTCOME_TEXT } from '../fi18n.mjs';
import { TRANSIT_OUTCOMES, SETTLE_OUTCOMES } from '../flog.mjs';

/** A record's line, or null for a kind the chronicle does not list. */
export function lineOf(r) {
  switch (r.name) {
    case 'RING_OPEN': return L`第${r.d}輪がひらかれた`;
    case 'PROVINCE_OPEN': return L`州 ${r.p},${r.q} がひらかれた（区画 ${r.site_count}）`;
    case 'JOIN': return L`新しい市民が加わった`;
    case 'SETTLE': return { fresh: () => L`州 ${r.p},${r.q} の区画 ${r.site + 1} に入植した`, displace: () => L`州 ${r.p},${r.q} の区画 ${r.site + 1} で仮の拠点が押し出された`,
      taken: () => null, expired: () => L`入植希望が期限切れになった` }[SETTLE_OUTCOMES[r.outcome]]?.() ?? null;
    case 'HOLDING_FINAL': return L`州 ${r.p},${r.q} の区画 ${r.site + 1} の拠点が確定した`;
    case 'DEPART': return L`州 ${r.origin_p},${r.origin_q} から軍勢が出発した（第${fmtNum(r.arrive_bell)}鐘に到着）`;
    case 'REVEAL': return L`州 ${r.p},${r.q} の第${fmtNum(r.arrive)}鐘の到着が開封された`;
    case 'CLASH': return L`州 ${r.p},${r.q} で第${fmtNum(r.bell)}鐘の衝突が決着した`;
    case 'TRANSIT_SETTLED': return L`進軍が精算された：${TRANSIT_OUTCOME_TEXT[TRANSIT_OUTCOMES[r.outcome]] ?? r.outcome}`;
    case 'EXPLORE_RESULT': return L`探索の結果：功績 ${r.works}`;
    case 'CAMP': return L`州 ${r.p},${r.q} に蛮族の野営地が現れた`;
    default: return null;
  }
}

/** Chronicle rows, newest first: `[{seq, bell, text, mine}]`; `mine(record)` marks the viewer's own. */
export function chronicleRows(records, mine = () => false, limit = 60) {
  const out = [];
  for (let i = records.length - 1; i >= 0 && out.length < limit; i--) {
    const { seq, record } = records[i];
    const text = lineOf(record);
    if (text) out.push({ seq, bell: record.bell, text, mine: !!mine(record) });
  }
  return out;
}

export function render(FS) {
  const rows = chronicleRows(FS.chronicle ?? [], FS.isMine ?? (() => false));
  if (!rows.length) return html`<p class="muted">${L`まだ記録はありません`}</p>`;
  return html`<section aria-labelledby="chronicle-title"><h3 id="chronicle-title">${L`年代記`}</h3>
    <ol class="list chronicle">${rows.map(r => html`<li class="${r.mine ? 'mine' : ''}"><span class="muted">${L`第${fmtNum(r.bell)}鐘`}</span> ${r.text}</li>`)}</ol></section>`;
}
