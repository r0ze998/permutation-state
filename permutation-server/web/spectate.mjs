// PERMUTATION STATE — spectator view.
// Reads the omniscient view (no member token, no ?civ), or one nation's
// fogged view when a row is selected. Revealed decisions are re-verified in
// the browser (verify.mjs), so the feed does not rest on the server's word.
import { WorldMap } from './map.mjs';
import * as T from './i18n.mjs';
import * as V from './verify.mjs';
import { $, $$, html, setHtml, fmtOr, short, usdcFixed, singleFlight } from './util.mjs';

const get = p => fetch(p, { cache: 'no-store' }).then(r => { if (!r.ok) throw new Error(r.status); return r.json(); });
const hash = h => short(h, 8, 6, '—');

const S = { view: null, watch: null, tab: 'decisions', decisions: [], verified: new Map(), lastTick: null, focused: false };
const map = new WorldMap($('#world-map'), {});
map.setLens('political');

const civName = id => T.civName(S.view?.civs?.[id]?.name ?? `#${id}`);
const color = id => T.CIV_COLORS[id % T.CIV_COLORS.length];

async function boot() {
  try {
    map.setMap(await get('/api/map'));
    await pollOnce();
    $('#loading').hidden = true;
  } catch {
    $('#loading-text').textContent = 'ゲームサーバーに接続できません。play サーバーを起動してから再読み込みしてください。';
  }
  setInterval(poll, 800);
  setInterval(loadDecisions, 3000);
  loadDecisions();
}

async function pollOnce() {
  const watch = S.watch;
  const v = await get(`/api/state${watch === null ? '' : `?civ=${watch}`}`);
  if (watch !== S.watch) return; // the viewer switched nations meanwhile
  const newTick = S.lastTick !== null && v.tick !== S.lastTick;
  S.view = v; S.lastTick = v.tick;
  map.setView(v, v.me);
  if (!S.focused) { S.focused = true; map.focusTile(S.watch === null ? '0,0' : capitalKey(v), true); }
  if (newTick) loadDecisions();
  render();
}
/** One request in flight; a server that is down just skips the beat. */
const poll = singleFlight(() => pollOnce().catch(() => {}));

function capitalKey(v) {
  const cap = v.cities.find(c => c.id === v.civs[v.me]?.capital);
  return cap ? `${cap.q},${cap.r}` : '0,0';
}

async function loadDecisions() {
  const d = await get('/api/decisions?limit=240').catch(() => null);
  if (!d) return;
  S.decisions = d.records.filter(r => r.reveal).sort((a, b) => b.tick - a.tick || a.civ - b.civ).slice(0, 60);
  // Up to four records per nation and tick (one per office): key by digest.
  await Promise.all(S.decisions.map(async r => {
    if (!S.verified.has(r.digest)) S.verified.set(r.digest, await V.verifyRecord(r));
  }));
  renderFeed();
}

function render() {
  renderTop(); renderBoard(); renderChain(); renderFeed();
}

function renderTop() {
  const v = S.view;
  const [, ph, phEn] = T.phaseOf(v.tick, v.season?.phases);
  setHtml($('#clock'), html`<b>ティック ${v.tick}</b>/ ${v.ticks} · ${ph} <small>${phEn}</small> · ${v.over ? 'シーズン終了' : v.paused ? '停止中' : `次の解決まで ${Math.ceil(v.secondsLeft)}秒`}`);
  setHtml($('#watch-chip'), S.watch === null
    ? html`<span class="badge">観戦</span><b>全体表示</b><span>全ての国</span>`
    : html`<span class="badge fog">国</span><b>${civName(S.watch)}</b><span>の立場から見ています（情報は全員に同じです）</span>`);
}

function renderBoard() {
  const v = S.view;
  const ms = v.members || [];
  const ach = v.achievements?.nations || [];
  const pr = v.projection;
  const rows = [...v.civs].sort((a, b) => (ach[b.id]?.points ?? 0) - (ach[a.id]?.points ?? 0) || a.id - b.id).map(c => {
    const members = ms.filter(m => m.civ === c.id);
    const a = ach[c.id] || { tiers: [0, 0, 0, 0], era: 0, points: 0 };
    return html`<tr class="civ-row ${S.watch === c.id ? 'watching' : ''}" data-civ="${c.id}">
      <td><span class="name"><span class="swatch-s" style="background:${color(c.id)}"></span><b>${T.civName(c.name)}</b>
        <span class="sub"><span class="kind">${members.length ? `国民${members.length}` : '代行のみ'}</span></span></span></td>
      <td>${fmtOr(c.cities)}</td><td>${fmtOr(c.pop)}</td><td>${fmtOr(c.troops ?? c.troopsSeen)}</td>
      <td>第${a.era}時代</td><td title="覇権/繁栄/科学/協調">${a.tiers.join('/')}</td><td>${fmtOr(a.points)}</td><td>${pr ? usdcFixed(pr.nationShare[c.id]) : '—'}</td></tr>`;
  });
  setHtml($('#board'), html`<tr><th>国</th><th>都市</th><th>人口</th><th>兵</th><th>時代</th><th>節目</th><th>点</th><th>取り分 USDC</th></tr>${rows}`);
}

function renderChain() {
  const c = S.view.chain;
  if (!c) {
    setHtml($('#chain'), html`<p class="none">ローカルモード（エンジンをこのプロセスで実行中）。<code>--chain</code> 付きで起動すると、MagicBlock ER 上のシーズンを表示します。</p>`);
    return;
  }
  const t = c.lastTick;
  const er = /^https?:\/\//.test(String(c.endpoints?.er ?? '')) ? c.endpoints.er : null;
  const explorer = sig => (er ? `https://explorer.solana.com/tx/${encodeURIComponent(sig)}?cluster=custom&customUrl=${encodeURIComponent(er)}` : null);
  const verify = c.gateway ? `cargo run --release --bin verify -- \\\n  --gateway ${c.gateway} \\\n  --base ${c.endpoints?.base} --er ${c.endpoints?.er}` : '';
  const tx = t && explorer(t.signature);
  setHtml($('#chain'), html`<dl>
    <dt>シーズン</dt><dd>${c.seasonId}</dd>
    <dt>いまの層</dt><dd><span class="layer">${c.layer === 'er' ? 'MagicBlock ER' : 'Solana base'}</span> slot ${fmtOr(c.slot)}</dd>
    <dt>プログラム</dt><dd>${hash(c.programId)}</dd>
    <dt>直近の解決</dt><dd>${t ? `ティック ${t.tick} · ${fmtOr(t.cu)} CU` : html`<span class="none">まだありません</span>`}</dd>
    ${t ? html`<dt>取引</dt><dd>${tx ? html`<a href="${tx}" target="_blank" rel="noopener">${hash(t.signature)}</a>` : hash(t.signature)}</dd>
    <dt>前の根</dt><dd>${hash(t.preRoot)}</dd><dt>新しい根</dt><dd>${hash(t.root)}</dd>` : ''}
  </dl>${verify ? html`<pre title="このシーズンを手元で再計算して、すべての根を照合します">${verify}</pre>` : ''}`);
}

function renderFeed() {
  const el = $('#feed');
  if (S.tab === 'chronicle') {
    const lines = (S.view?.chronicle || []).slice(0, 80).map(({ tick, text }) => {
      const [kind, t] = T.chronicleText(text);
      return html`<li><span class="t">${tick}</span><span>${T.KIND_GLYPH[kind] || '·'} ${t}</span></li>`;
    });
    setHtml(el, lines.length ? lines : html`<li><span></span><span class="empty">まだ出来事はありません。</span></li>`);
    return;
  }
  const lines = S.decisions.map(r => {
    const ok = S.verified.get(r.digest);
    const badge = ok === true ? html`<span class="ok">✓ ブラウザで検証済み</span>` : ok === false ? html`<span class="bad">✕ 約束と一致しません</span>` : '';
    return html`<li><span class="t">${r.tick}</span><span><span class="who" style="color:${color(r.civ)}">${civName(r.civ)}</span>
      <span class="policy">${r.policy}${r.external ? ' · 外部エージェント' : ''}</span><br>${r.reveal.text || '（理由なし）'}<br>${badge}</span></li>`;
  });
  setHtml(el, lines.length ? lines : html`<li><span></span><span class="empty">判断は、解決後の次のティックで公開されます。</span></li>`);
}

document.addEventListener('click', e => {
  const row = e.target.closest('tr.civ-row');
  if (row) { const id = +row.dataset.civ; S.watch = S.watch === id ? null : id; S.focused = false; poll(); return; }
  const tab = e.target.closest('[data-tab]');
  if (tab) { S.tab = tab.dataset.tab; for (const b of $$('[data-tab]')) b.setAttribute('aria-pressed', String(b === tab)); renderFeed(); return; }
  const lens = e.target.closest('[data-lens]');
  if (lens) { map.setLens(lens.dataset.lens); for (const b of $$('[data-lens]')) b.setAttribute('aria-pressed', String(b === lens)); return; }
  if (e.target.id === 'zoom-in') map.zoomBy(1.2);
  if (e.target.id === 'zoom-out') map.zoomBy(1 / 1.2);
});

boot();
