// PERMUTATION STATE — spectator view.
// Reads the omniscient view (no seat token, no ?civ), or one civilization's
// fogged view when a row is selected. Revealed decisions are re-verified in
// the browser (verify.mjs), so the feed does not rest on the server's word.
import { WorldMap } from './map.mjs';
import * as T from './i18n.mjs';
import * as V from './verify.mjs';

const $ = s => document.querySelector(s);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const fmt = n => (n === null || n === undefined ? '—' : Number(n).toLocaleString('ja-JP'));
const short = h => (h ? `${h.slice(0, 8)}…${h.slice(-6)}` : '—');
const get = p => fetch(p, { cache: 'no-store' }).then(r => { if (!r.ok) throw new Error(r.status); return r.json(); });

const S = { view: null, watch: null, tab: 'decisions', decisions: [], verified: new Map(), lastTick: null, focused: false };
const map = new WorldMap($('#world-map'), {});
map.setLens('political');

const civName = id => T.civName(S.view?.civs?.[id]?.name ?? `#${id}`);
const color = id => T.CIV_COLORS[id % T.CIV_COLORS.length];
function setHtml(el, html) { if (el.__html !== html) { el.innerHTML = html; el.__html = html; } }

async function boot() {
  try {
    map.setMap(await get('/api/map'));
    await poll();
    $('#loading').hidden = true;
  } catch {
    $('#loading-text').textContent = 'ゲームサーバーに接続できません。play サーバーを起動してから再読み込みしてください。';
  }
  setInterval(poll, 800);
  setInterval(loadDecisions, 3000);
  loadDecisions();
}

async function poll() {
  const v = await get(`/api/state${S.watch === null ? '' : `?civ=${S.watch}`}`);
  const newTick = S.lastTick !== null && v.tick !== S.lastTick;
  S.view = v; S.lastTick = v.tick;
  map.setView(v, v.me);
  if (!S.focused) { S.focused = true; map.focusTile(S.watch === null ? '0,0' : capitalKey(v), true); }
  if (newTick) loadDecisions();
  render();
}

function capitalKey(v) {
  const cap = v.cities.find(c => c.id === v.civs[v.me]?.capital);
  return cap ? `${cap.q},${cap.r}` : '0,0';
}

async function loadDecisions() {
  const d = await get('/api/decisions?limit=240').catch(() => null);
  if (!d) return;
  S.decisions = d.records.filter(r => r.reveal).sort((a, b) => b.tick - a.tick || a.civ - b.civ).slice(0, 60);
  await Promise.all(S.decisions.map(async r => {
    const k = `${r.tick}:${r.civ}`;
    if (!S.verified.has(k)) S.verified.set(k, await V.verifyRecord(r));
  }));
  renderFeed();
}

function render() {
  renderTop(); renderBoard(); renderChain(); renderFeed();
}

function renderTop() {
  const v = S.view;
  const [, ph, phEn] = T.phaseOf(v.tick);
  setHtml($('#clock'), `<b>ティック ${v.tick}</b>/ ${v.ticks} · ${ph} <small>${phEn}</small> · ${v.over ? 'シーズン終了' : v.paused ? '停止中' : `次の解決まで ${Math.ceil(v.secondsLeft)}秒`}`);
  setHtml($('#watch-chip'), S.watch === null
    ? '<span class="badge">観戦</span><b>全体表示</b><span>霧なし・全文明</span>'
    : `<span class="badge fog">視界</span><b>${esc(civName(S.watch))}</b><span>の霧の中から見ています</span>`);
}

function renderBoard() {
  const v = S.view;
  const seats = new Map((v.seats || []).map(s => [s.civ, s]));
  const kindLabel = { human: '人間', bot: 'ボット', external: 'エージェント' };
  const rows = [...v.civs].sort((a, b) => b.dominion - a.dominion || a.id - b.id).map(c => {
    const s = seats.get(c.id) || {};
    return `<tr class="civ-row ${S.watch === c.id ? 'watching' : ''}" data-civ="${c.id}">
      <td><span class="name"><span class="swatch-s" style="background:${color(c.id)}"></span><b>${esc(T.civName(c.name))}</b>
        <span class="sub"><span class="kind ${s.kind || ''}">${kindLabel[s.kind] || ''}</span>${s.ready ? '<span class="ready" title="このティックの手番を終えた">✓ 手番終了</span>' : ''}</span></span></td>
      <td>${fmt(c.cities)}</td><td>${fmt(c.pop)}</td><td>${fmt(c.troops ?? c.troopsSeen)}</td>
      <td>${fmt(c.dominion)}</td><td>${fmt(c.concord)}</td><td>${c.stages}/3</td></tr>`;
  }).join('');
  setHtml($('#board'), `<tr><th>文明</th><th>都市</th><th>人口</th><th>兵</th><th>覇権</th><th>協調</th><th>科学</th></tr>${rows}`);
}

function renderChain() {
  const c = S.view.chain;
  if (!c) {
    setHtml($('#chain'), '<p class="none">ローカルモード（エンジンをこのプロセスで実行中）。<code>--chain</code> 付きで起動すると、MagicBlock ER 上のシーズンを表示します。</p>');
    return;
  }
  const t = c.lastTick;
  const er = c.endpoints?.er;
  const explorer = sig => (er ? `https://explorer.solana.com/tx/${sig}?cluster=custom&customUrl=${encodeURIComponent(er)}` : null);
  const verify = c.gateway ? `cargo run --release --bin verify -- \\\n  --gateway ${c.gateway} \\\n  --base ${c.endpoints?.base} --er ${er}` : '';
  setHtml($('#chain'), `<dl>
    <dt>シーズン</dt><dd>${esc(c.seasonId)}</dd>
    <dt>いまの層</dt><dd><span class="layer">${c.layer === 'er' ? 'MagicBlock ER' : 'Solana base'}</span> slot ${fmt(c.slot)}</dd>
    <dt>プログラム</dt><dd>${short(c.programId)}</dd>
    <dt>直近の解決</dt><dd>${t ? `ティック ${t.tick} · ${fmt(t.cu)} CU` : '<span class="none">まだありません</span>'}</dd>
    ${t ? `<dt>取引</dt><dd>${explorer(t.signature) ? `<a href="${explorer(t.signature)}" target="_blank" rel="noopener">${short(t.signature)}</a>` : short(t.signature)}</dd>
    <dt>前の根</dt><dd>${short(t.preRoot)}</dd><dt>新しい根</dt><dd>${short(t.root)}</dd>` : ''}
  </dl>${verify ? `<pre title="このシーズンを手元で再計算して、すべての根を照合します">${esc(verify)}</pre>` : ''}`);
}

function renderFeed() {
  const el = $('#feed');
  if (S.tab === 'chronicle') {
    const lines = (S.view?.chronicle || []).slice(0, 80).map(({ tick, text }) => {
      const [kind, t] = T.chronicleText(text);
      return `<li><span class="t">${tick}</span><span>${T.KIND_GLYPH[kind] || '·'} ${esc(t)}</span></li>`;
    });
    setHtml(el, lines.join('') || '<li><span></span><span class="empty">まだ出来事はありません。</span></li>');
    return;
  }
  const lines = S.decisions.map(r => {
    const ok = S.verified.get(`${r.tick}:${r.civ}`);
    const badge = ok === true ? '<span class="ok">✓ ブラウザで検証済み</span>' : ok === false ? '<span class="bad">✕ 約束と一致しません</span>' : '';
    return `<li><span class="t">${r.tick}</span><span><span class="who" style="color:${color(r.civ)}">${esc(civName(r.civ))}</span>
      <span class="policy">${esc(r.policy)}${r.external ? ' · 外部エージェント' : ''}</span><br>${esc(r.reveal.text || '（理由なし）')}<br>${badge}</span></li>`;
  });
  setHtml(el, lines.join('') || '<li><span></span><span class="empty">判断は、解決後の次のティックで公開されます。</span></li>');
}

document.addEventListener('click', e => {
  const row = e.target.closest("tr.civ-row");
  if (row) { const id = +row.dataset.civ; S.watch = S.watch === id ? null : id; S.focused = false; poll(); return; }
  const tab = e.target.closest('[data-tab]');
  if (tab) { S.tab = tab.dataset.tab; document.querySelectorAll('[data-tab]').forEach(b => b.setAttribute('aria-pressed', String(b === tab))); renderFeed(); return; }
  const lens = e.target.closest('[data-lens]');
  if (lens) { map.setLens(lens.dataset.lens); document.querySelectorAll('[data-lens]').forEach(b => b.setAttribute('aria-pressed', String(b === lens))); return; }
  if (e.target.id === 'zoom-in') map.zoomBy(1.2);
  if (e.target.id === 'zoom-out') map.zoomBy(1 / 1.2);
});

boot();
