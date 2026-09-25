// Decision log (§4.3, §7.5): every office commits to what it saw and why
// before a tick resolves and reveals it after. The browser re-verifies the
// digests and observation proofs itself (verify.mjs).
import * as T from '../i18n.mjs';
import * as V from '../verify.mjs';
import * as api from '../api.mjs';
import { html } from '../util.mjs';
import { S, civN, invalidate } from '../state.mjs';
import { map } from '../world.mjs';

const verifiedKey = r => `${r.tick}:${r.civ}:${r.digest}`;

export async function loadDecisions() {
  const d = await api.tryGet('/api/decisions?limit=240');
  if (!d) return;
  S.decisions = d;
  invalidate('drawer');
  for (const r of d.records) {
    const k = verifiedKey(r);
    if (r.reveal && S.verified[k] === undefined) S.verified[k] = await V.verifyRecord(r);
  }
  invalidate('drawer');
}

/** Prove what a nation saw of the selected tile at a past tick (Merkle proof to its observation root). */
export async function proveTile() {
  const t = S.tile && map.tiles.get(S.tile);
  if (!t) { S.proof = { error: '先に地図でマスを選んでください。' }; invalidate('drawer'); return; }
  const civId = +(S.proofCiv ?? S.myCiv), tick = +(S.proofTick ?? Math.max(0, S.view.tick - 1));
  const p = await api.tryGet(`/api/decisions/proof?tick=${tick}&civ=${civId}&kind=tile&id=${t.index}`);
  if (!p) { S.proof = { error: api.translateError('network') }; invalidate('drawer'); return; }
  if (!p.ok) { S.proof = { error: p.error === 'not available' ? 'まだ解決していないティックは証明できません。' : '古い観測は根（ルート）しか残っていません。' }; invalidate('drawer'); return; }
  const leafOk = await V.verifyProof(p);
  const rec = (S.decisions?.records || []).find(r => r.tick === tick && r.civ === civId);
  const digestOk = rec?.reveal ? await V.verifyRecord(rec) : null;
  S.proof = { tick, civ: civId, tile: V.decodeTile(p.body), steps: p.proof.length, root: p.root, leafOk, rootMatches: rec ? rec.obsRoot === p.root : null, digestOk, text: rec?.reveal?.text };
  invalidate('drawer');
}
export function setProofCiv(civ) { S.proofCiv = civ; S.proof = null; invalidate('drawer'); }
export function setProofTick(tick) { S.proofTick = Math.max(0, Math.min(S.view.tick - 1, tick || 0)); S.proof = null; invalidate('drawer'); }

const FOG_NAME = ['未踏（見えない）', '霧の中（記憶のみ）', '視界の中'];
const check = ok => (ok === true ? html`<span class="vchk ok">✓</span>` : ok === false ? html`<span class="vchk bad">✗</span>` : html`<span class="vchk">…</span>`);

function proofResult(pr) {
  if (!pr) return '';
  if (pr.error) return html`<div class="explanation">${pr.error}</div>`;
  return html`<div class="proof-result">
      <div class="pr-head">ティック${pr.tick}の<b>${civN(pr.civ)}</b>から見たマス (${pr.tile.q}, ${pr.tile.r})：<b>${FOG_NAME[pr.tile.fog]}</b>${pr.tile.ownerCity !== null ? ` · ${T.cityName(pr.tile.ownerCity)}の領土と認識` : ''}</div>
      <div class="pr-row">${check(pr.leafOk)} このマスの葉 → 観測ルート（経路${pr.steps}段）</div>
      <div class="pr-row">${check(pr.rootMatches)} 観測ルートが約束に使われたものと一致 <code>${String(pr.root).slice(0, 10)}…</code></div>
      <div class="pr-row">${check(pr.digestOk)} ${pr.digestOk === null ? '理由はまだ非公開です（次のティックで公開され、約束と照合できます）' : `公開された理由が約束（digest）と一致${pr.text ? `：「${pr.text}」` : '（理由の記入なし）'}`}</div>
      ${pr.tile.fog < 2 ? html`<p class="desc">このマスは判断のとき見えていませんでした。ここにいた部隊について、この文明は知り得なかったことになります。</p>` : ''}
    </div>`;
}

/**
 * Who decided: an outside agent (external), or by the policy id once it is
 * known (revealed, or my own nation's): 'human' or an agent policy.
 */
const decider = r => (r.external ? 'AI' : r.policy === null || r.policy === undefined ? null : r.policy === 'human' ? 'HUMAN' : 'AI');

/** One committed batch: who (nation, office, AI or human), the commitment, and its reveal. */
function record(r) {
  const ok = S.verified[verifiedKey(r)];
  const digest = String(r.digest ?? '');
  const status = !r.reveal ? html`<span class="meta">解決後に公開</span>`
    : html`<span>公開 t${r.reveal.at}</span><span class="arrow">→</span>${ok === true ? html`<span class="vchk ok">✓ 一致</span>` : ok === false ? html`<span class="vchk bad">✗ 不一致</span>` : html`<span class="vchk">検証中</span>`}`;
  return html`<div class="dec-row"><div class="dec-who"><span class="swatch-s" style="background:${T.CIV_COLORS[r.civ]}"></span><b>${civN(r.civ)}</b>${r.role ? html`<span class="meta">${T.ROLE_GLYPH[r.role] || ''} ${T.ROLE_JA[r.role] || r.role}</span>` : ''}${decider(r) ? html`<span class="badge ${decider(r) === 'AI' ? 'agent' : 'human'}">${decider(r)}</span>` : ''}${r.policy ? html`<code>${r.policy}</code>` : ''}</div>
      <div class="dec-steps"><span title="decision_digest ${digest}">約束 <code>${digest.slice(0, 8)}</code></span><span class="arrow">→</span>${status}</div>
      ${r.reveal ? html`<div class="dec-text">${r.reveal.text ? r.reveal.text : html`<span class="meta">（理由の記入なし）</span>`}</div>` : ''}</div>`;
}

export function drawerDecisions() {
  const d = S.decisions, civs = S.view.civs;
  const who = S.decFilter;
  const pt = S.tile && map.tiles.get(S.tile);
  const lastTick = Math.max(0, S.view.tick - 1);
  const head = html`<div class="eyebrow">DECISION LOG · 判断ログ</div><h2>判断の証拠</h2>
    <p class="drawer-intro">全文明が毎ティック、解決<b>前</b>に「見えていた世界（観測ルート）・方針・理由」を1つのハッシュで約束し、解決<b>後</b>に理由を公開します。下の ✓ はサーバーではなく、このブラウザが SHA-256 で再計算した結果です。</p>
    <div class="section-title">観測の証明 · そのとき何が見えていたか</div>
    <div class="proof-tool">
      <label>文明 <select id="proof-civ">${civs.map(c => html`<option value="${c.id}" ${+(S.proofCiv ?? S.myCiv) === c.id ? 'selected' : ''}>${civN(c.id)}</option>`)}</select></label>
      <label>ティック <input id="proof-tick" type="number" min="0" max="${lastTick}" value="${S.proofTick ?? lastTick}"></label>
      <span class="meta">マス：${pt ? `${pt.q}, ${pt.r}` : '地図で選択'}</span>
      <button class="btn primary" type="button" id="prove-tile" ${pt ? '' : 'disabled'}>証明する</button>
    </div>
    ${proofResult(S.proof)}
    <div class="section-title">タイムライン</div>
    <div class="row dec-filter"><button class="btn ${who === 'all' ? 'primary' : ''}" type="button" data-dec="all">全員</button>${civs.map(c => html`<button class="btn ${who !== 'all' && +who === c.id ? 'primary' : ''}" type="button" data-dec="${c.id}"><span class="swatch-s" style="background:${T.CIV_COLORS[c.id]}"></span>${civN(c.id)}</button>`)}</div>`;
  if (!d) return html`${head}<p class="desc">読み込んでいます…</p>`;
  const rows = d.records.filter(r => who === 'all' || r.civ === +who).slice(0, 120);
  if (!rows.length) return html`${head}<div class="dec-list"><p class="desc">まだ記録はありません。ティックが進むと並びます。</p></div>`;
  const items = [];
  let last = null;
  for (const r of rows) {
    if (r.tick !== last) { items.push(html`<div class="dec-tick">ティック ${r.tick}${r.tick >= d.open ? ' · 解決待ち' : ''}</div>`); last = r.tick; }
    items.push(record(r));
  }
  return html`${head}<div class="dec-list">${items}</div>`;
}
